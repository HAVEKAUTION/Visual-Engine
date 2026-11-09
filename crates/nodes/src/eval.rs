//! Running a graph each frame: signal nodes on the CPU, picture nodes on the
//! GPU, in the order [`graph::Graph::plan`] gives.
//!
//! - Every number parameter follows the mapping rule: its own modulations
//!   plus every Signal wire plugged into it (same maths, same data).
//! - A node whose shader breaks keeps showing its last good frame (or passes
//!   its input through) and reports the error; the output never stops.
//! - Bypassed nodes pass their first picture input straight through.
//! - Given the same graph, signals and time steps, the output is the same.

use std::collections::{BTreeMap, HashMap};

use graph::{Graph, Node, NodeId, Param, ParamValue};
use render::{Renderer, ScenePipeline, Show};
use scenes::{CompiledScene, ParamState, SceneLibrary, SignalState};
use signals::modulation::{ModContext, Modulator, Wave};

use crate::registry::{self, AUDIO_SIGNAL, LFO, PROGRAM_OUT};

/// What the evaluator needs from the outside each frame.
pub struct FrameInput<'a> {
    /// This frame's signals (already updated by the caller).
    pub signals: &'a mut SignalState,
    /// Seconds since the last frame (already scaled by any global speed).
    pub dt: f32,
    /// Manual controls (macros), 0..1.
    pub controls: &'a [f32],
    /// Global modulation depth (1 = as set).
    pub intensity: f32,
    /// Size of the final picture, in pixels.
    pub output_size: (u32, u32),
    /// Preview scale (1 = full size).
    pub proxy: f32,
}

/// The value of a number parameter this frame: its base value, plus its
/// modulations, plus every Signal wire into it (`signal_values` = the
/// outputs of the signal nodes this frame), clamped to its range.
pub fn effective_value(
    graph: &Graph,
    node: NodeId,
    param: &Param,
    ctx: &ModContext,
    signal_values: &HashMap<NodeId, f32>,
    intensity: f32,
) -> f32 {
    let ParamValue::Float(base) = param.value else {
        return 0.0;
    };
    let ranged = param.def.min < param.def.max;
    let (min, max) = if ranged {
        (param.def.min, param.def.max)
    } else {
        (f32::MIN, f32::MAX)
    };
    let range = if ranged { max - min } else { 1.0 };
    // Modulations are a fraction of the range, so only ranged values take them.
    let modulated = if ranged {
        Modulator {
            base,
            min,
            max,
            slots: &param.mods,
        }
        .value(ctx, intensity)
    } else {
        base
    };
    let wires: f32 = graph
        .param_links(node, &param.def.name)
        .filter_map(|l| {
            let m = l.modulation.filter(|m| m.enabled)?;
            let x = signal_values.get(&l.from.node).copied().unwrap_or(0.0);
            let v = m.shape.apply(x);
            Some(if v.is_finite() {
                m.depth * intensity * range * v
            } else {
                0.0
            })
        })
        .sum();
    (modulated + wires).clamp(min, max)
}

struct NodeRt {
    file: String,
    pipeline: Option<ScenePipeline>,
    params: ParamState,
    /// The shader file changed on disk: rebuild before the next frame.
    dirty: bool,
}

/// Runs one graph. Keep one per graph that is on screen (e.g. two during a
/// crossfade), each with its own `key_base` so their pictures don't mix.
pub struct Evaluator {
    key_base: u64,
    compiled: HashMap<String, Result<CompiledScene, String>>,
    rt: HashMap<NodeId, NodeRt>,
    values: HashMap<NodeId, f32>,
    /// The picture each evaluated node put out last frame (`None` = black).
    pictures: HashMap<NodeId, Option<u64>>,
    /// Number parameters after modulation, last frame.
    live: HashMap<(NodeId, String), f32>,
    /// Problems by node, shown as badges.
    pub errors: BTreeMap<NodeId, String>,
}

impl Evaluator {
    pub fn new(key_base: u64) -> Self {
        Self {
            key_base,
            compiled: HashMap::new(),
            rt: HashMap::new(),
            values: HashMap::new(),
            pictures: HashMap::new(),
            live: HashMap::new(),
            errors: BTreeMap::new(),
        }
    }

    /// The renderer key of a node's picture.
    pub fn key(&self, node: NodeId) -> u64 {
        self.key_base + node.0 as u64
    }

    /// A signal node's value this frame.
    pub fn signal_value(&self, node: NodeId) -> Option<f32> {
        self.values.get(&node).copied()
    }

    /// The shader file changed on disk: nodes using it rebuild next frame
    /// (keeping the old version running if the new one is broken).
    pub fn invalidate(&mut self, file: &str) {
        self.compiled.remove(file);
        for rt in self.rt.values_mut().filter(|r| r.file == file) {
            rt.dirty = true;
        }
    }

    fn compiled(&mut self, file: &str, library: &SceneLibrary) -> Result<CompiledScene, String> {
        self.compiled
            .entry(file.to_string())
            .or_insert_with(|| match library.get(file) {
                Some(e) => scenes::compile(&e.source, file),
                None => Err(format!("{file} is not in the scenes folder")),
            })
            .clone()
    }

    /// Make sure a shader node has a pipeline (building or rebuilding it).
    fn prepare(
        &mut self,
        node: &Node,
        file: &str,
        library: &SceneLibrary,
        renderer: &Renderer,
    ) -> Result<(), String> {
        let needs_build = match self.rt.get(&node.id) {
            Some(rt) => rt.file != file || rt.dirty,
            None => true,
        };
        if !needs_build {
            return Ok(());
        }
        let built = self
            .compiled(file, library)
            .and_then(|c| renderer.create_scene(&c).map(|p| (p, c)));
        match built {
            Ok((pipeline, compiled)) => {
                let params = match self.rt.get(&node.id) {
                    Some(old) if old.file == file => old.params.remap(&compiled.meta),
                    _ => ParamState::new(&compiled.meta),
                };
                self.rt.insert(
                    node.id,
                    NodeRt {
                        file: file.to_string(),
                        pipeline: Some(pipeline),
                        params,
                        dirty: false,
                    },
                );
                Ok(())
            }
            Err(e) => {
                if let Some(rt) = self.rt.get_mut(&node.id) {
                    rt.dirty = false; // keep the last good version
                }
                Err(e)
            }
        }
    }

    /// What a node put out last frame, ready to show (a Program Out node shows
    /// what is plugged into it).
    pub fn picture(&self, node: NodeId) -> Show {
        match self.pictures.get(&node).copied().flatten() {
            Some(k) => Show::Node(k),
            None => Show::Black,
        }
    }

    /// A number parameter's value after modulation, last frame.
    pub fn live_value(&self, node: NodeId, param: &str) -> Option<f32> {
        self.live.get(&(node, param.to_string())).copied()
    }

    /// Evaluate everything the `targets` need (the Program Out node, plus any
    /// node being looked at) and return the picture of the first one. Call
    /// [`Renderer::begin_frame`] before and [`Renderer::end_frame`] after
    /// (once for all evaluators).
    pub fn run(
        &mut self,
        graph: &Graph,
        library: &SceneLibrary,
        renderer: &mut Renderer,
        input: FrameInput,
        targets: &[NodeId],
    ) -> Show {
        self.errors.clear();
        self.values.clear();
        self.live.clear();
        // Forget nodes that were deleted.
        self.rt.retain(|id, _| graph.node(*id).is_some());

        let beats = input.signals.get("beats").unwrap_or(0.0) as f64;
        let time = input.signals.get("time").unwrap_or(0.0) as f64;
        // Which picture each node puts out this frame (`None` = black).
        let mut image: HashMap<NodeId, Option<u64>> = HashMap::new();

        for id in graph.plan(targets) {
            let Some(node) = graph.node(id) else { continue };
            let first_input = |image: &HashMap<NodeId, Option<u64>>| {
                node.inputs
                    .iter()
                    .filter(|p| p.ty == graph::PortType::Image)
                    .find_map(|p| graph.input_link(id, &p.name))
                    .and_then(|l| image.get(&l.from.node).copied().flatten())
            };
            let sig = &*input.signals;
            let lookup = |n: &str| sig.get(n);
            let ctx = ModContext {
                signal: &lookup,
                beats,
                time,
                controls: input.controls,
            };
            let eff =
                |p: &Param| effective_value(graph, id, p, &ctx, &self.values, input.intensity);

            match node.kind.as_str() {
                AUDIO_SIGNAL => {
                    let name = match node.param("signal").map(|p| &p.value) {
                        Some(ParamValue::Choice(i)) => {
                            scenes::uniforms::SIGNAL_NAMES.get(*i as usize).copied()
                        }
                        _ => None,
                    };
                    let v = name.and_then(|n| sig.get(n)).unwrap_or(0.0);
                    self.values.insert(id, v);
                }
                LFO => {
                    let per = node.param("beats").map(eff).unwrap_or(4.0).max(1e-3);
                    let wave = match node.param("wave").map(|p| &p.value) {
                        Some(ParamValue::Choice(i)) => {
                            Wave::ALL.get(*i as usize).copied().unwrap_or(Wave::Sine)
                        }
                        _ => Wave::Sine,
                    };
                    let v = if node.bypass {
                        0.0
                    } else {
                        wave.at(beats / per as f64)
                    };
                    self.values.insert(id, v);
                }
                PROGRAM_OUT => {
                    let src = graph
                        .input_link(id, "image")
                        .and_then(|l| image.get(&l.from.node).copied().flatten());
                    image.insert(id, src);
                }
                kind => {
                    let Some(file) = registry::shader_file(kind) else {
                        self.errors
                            .insert(id, format!("unknown node type \"{kind}\""));
                        image.insert(id, first_input(&image));
                        continue;
                    };
                    if node.bypass {
                        image.insert(id, first_input(&image));
                        continue;
                    }
                    // Effective values before borrowing the runtime.
                    let values: Vec<(String, Vec<f32>)> = node
                        .params
                        .iter()
                        .map(|p| {
                            let v = match &p.value {
                                ParamValue::Float(_) => {
                                    let v = eff(p);
                                    self.live.insert((id, p.def.name.clone()), v);
                                    vec![v]
                                }
                                ParamValue::Bool(b) => vec![if *b { 1.0 } else { 0.0 }],
                                ParamValue::Choice(i) => vec![*i as f32],
                                ParamValue::Colour(c) => c.to_vec(),
                                ParamValue::Text(_) => vec![],
                            };
                            (p.def.name.clone(), v)
                        })
                        .collect();
                    if let Err(e) = self.prepare(node, file, library, renderer) {
                        self.errors.insert(id, e);
                    }
                    let key = self.key(id);
                    let Some(rt) = self.rt.get_mut(&id) else {
                        // Never built: pass the input through.
                        image.insert(id, first_input(&image));
                        continue;
                    };
                    let meta = rt.params.meta().clone();
                    for (name, v) in values {
                        if let Some(i) = meta.input_index(&name)
                            && rt.params.effective[i].len() == v.len()
                        {
                            rt.params.effective[i] = v;
                        }
                    }
                    rt.params.advance(input.signals, input.dt);
                    input.signals.set_palette(&rt.params.current_palette());
                    let inputs: [Option<u64>; 2] = std::array::from_fn(|i| {
                        meta.images
                            .get(i)
                            .and_then(|img| graph.input_link(id, &img.name))
                            .and_then(|l| image.get(&l.from.node).copied().flatten())
                    });
                    let size = graph::node_size(node.resolution, input.output_size, input.proxy);
                    match &rt.pipeline {
                        Some(p) => {
                            renderer.render_node(
                                key,
                                size,
                                p,
                                input.signals.as_bytes(),
                                rt.params.block(),
                                inputs,
                            );
                            image.insert(id, Some(key));
                        }
                        None if renderer.has_node(key) => {
                            renderer.keep_node(key);
                            image.insert(id, Some(key));
                        }
                        None => {
                            image.insert(id, first_input(&image));
                        }
                    }
                }
            }
        }
        self.pictures = image;
        targets
            .first()
            .map(|t| self.picture(*t))
            .unwrap_or(Show::Black)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graph::{Command, Endpoint, Link, NodeSpec, OutputRef, ParamDef, PortDef, PortType};
    use signals::modulation::{ModSlot, Shape, Source};

    #[test]
    fn modulations_and_wires_add_up_and_clamp() {
        let mut g = Graph::new();
        let target = g.new_node(&NodeSpec {
            kind: "t".into(),
            label: "t".into(),
            inputs: vec![],
            outputs: vec![],
            params: vec![ParamDef::float("x", 0.0, 10.0, 2.0)],
        });
        let src = g.new_node(&NodeSpec {
            kind: "s".into(),
            label: "s".into(),
            inputs: vec![],
            outputs: vec![PortDef::new("value", PortType::Signal)],
            params: vec![],
        });
        let (t, s) = (target.id, src.id);
        g.apply(Command::AddNode(Box::new(target))).unwrap();
        g.apply(Command::AddNode(Box::new(src))).unwrap();
        g.apply(Command::Connect(Link::new(
            OutputRef::new(s, "value"),
            Endpoint::param(t, "x"),
        )))
        .unwrap();
        g.apply(Command::SetMods {
            node: t,
            param: "x".into(),
            mods: vec![ModSlot::new(Source::Control(0), Shape::Linear, 0.1)],
        })
        .unwrap();
        let none = |_: &str| None;
        let ctx = ModContext {
            signal: &none,
            beats: 0.0,
            time: 0.0,
            controls: &[1.0],
        };
        let p = g.node(t).unwrap().param("x").unwrap().clone();
        let mut vals = HashMap::new();
        // Base 2 + macro 1.0·0.1·10 = 3, + wire 0.4·0.5(default depth)·10 = 5.
        vals.insert(s, 0.4);
        assert!((effective_value(&g, t, &p, &ctx, &vals, 1.0) - 5.0).abs() < 1e-5);
        // Intensity scales both.
        assert!((effective_value(&g, t, &p, &ctx, &vals, 0.5) - 3.5).abs() < 1e-5);
        // Clamped to the range.
        vals.insert(s, 100.0);
        assert_eq!(effective_value(&g, t, &p, &ctx, &vals, 1.0), 10.0);
    }
}
