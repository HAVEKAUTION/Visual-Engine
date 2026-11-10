//! Running a graph each frame: signal nodes on the CPU, picture nodes on the
//! GPU, in the order [`graph::Graph::plan`] gives.
//!
//! - Every number parameter follows the mapping rule: its own modulations
//!   plus every Signal wire plugged into it (same maths, same data).
//! - A node whose shader breaks keeps showing its last good frame (or passes
//!   its input through) and reports the error; the output never stops.
//! - Bypassed nodes pass their first picture input straight through.
//! - Given the same graph, signals and time steps, the output is the same
//!   (video frames arrive from a background decoder, so live playback shows
//!   the nearest decoded frame; export will wait for exact frames).
//! - Image / Video nodes read files through [`media::MediaLibrary`] and draw
//!   them with a small built-in shader (fit / fill / stretch, mirror, scale).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use graph::{Graph, Node, NodeId, Param, ParamValue};
use media::{MediaLibrary, Quality, Status};
use render::{MEDIA_KEY, Renderer, ScenePipeline, Show};
use scenes::{CompiledScene, ParamState, SceneLibrary, SignalState};
use signals::modulation::{ModContext, Modulator, Wave, random_walk};

use crate::registry::{self, *};

/// Is this a CPU node (signal or trigger) rather than a picture node?
pub fn is_signal_kind(kind: &str) -> bool {
    matches!(
        kind,
        AUDIO_SIGNAL
            | LFO
            | AUDIO_TRIGGER
            | BEAT_TRIGGER
            | GATE
            | MATH
            | REMAP
            | SMOOTH
            | ENVELOPE
            | COUNTER
            | SAMPLE_HOLD
            | RANDOM
            | HOLD
    )
}

/// Memory a signal node keeps between frames.
#[derive(Clone, Debug, Default)]
struct SigState {
    started: bool,
    prev: f32,
    level: f32,
    rising: bool,
    count: u32,
    since: f32,
    /// Hold: beat position when it started.
    mark: f64,
}

/// What a signal node can read this frame.
struct SigIn<'a> {
    dt: f32,
    beats: f64,
    signals: &'a SignalState,
    num: &'a dyn Fn(&str, f32) -> f32,
    flag: &'a dyn Fn(&str, bool) -> bool,
    choice: &'a dyn Fn(&str) -> u32,
    /// Value plugged into an input (`None` = unplugged).
    input: &'a dyn Fn(&str) -> Option<f32>,
}

/// One frame of a signal / trigger node. Triggers output 1 on the frame they
/// fire and 0 otherwise.
fn signal_node(kind: &str, st: &mut SigState, i: &SigIn) -> f32 {
    let dt = i.dt.clamp(0.0, 0.25);
    let fired = |x: Option<f32>| x.unwrap_or(0.0) > 0.5;
    let first = !st.started;
    st.started = true;
    match kind {
        AUDIO_SIGNAL => scenes::uniforms::SIGNAL_NAMES
            .get((i.choice)("signal") as usize)
            .and_then(|n| i.signals.get(n))
            .unwrap_or(0.0),
        LFO => {
            let per = (i.num)("beats", 4.0).max(1e-3) as f64;
            let wave = Wave::ALL
                .get((i.choice)("wave") as usize)
                .copied()
                .unwrap_or(Wave::Sine);
            wave.at(i.beats / per + (i.num)("offset", 0.0) as f64)
        }
        AUDIO_TRIGGER => {
            let event = TRIGGER_EVENTS
                .get((i.choice)("event") as usize)
                .map(|e| e.0)
                .unwrap_or("kick");
            let (x, hit) = match event {
                "beat" => {
                    let x = i.signals.get("beat_count").unwrap_or(0.0);
                    (x, x > st.prev)
                }
                "bar" => {
                    let x = i.signals.get("bar_phase").unwrap_or(0.0);
                    (x, x < st.prev - 0.5)
                }
                e => {
                    let x = i.signals.get(&format!("{e}_age")).unwrap_or(1e9);
                    (x, x + 1e-4 < st.prev)
                }
            };
            st.prev = x;
            if hit && !first { 1.0 } else { 0.0 }
        }
        BEAT_TRIGGER => {
            let every = (i.num)("every", 1.0).max(0.01) as f64;
            let x = ((i.beats - (i.num)("offset", 0.0) as f64) / every).floor() as f32;
            let hit = x > st.prev;
            st.prev = x;
            if hit && !first { 1.0 } else { 0.0 }
        }
        GATE => {
            let x = (i.input)("in").unwrap_or(0.0);
            let thr = (i.num)("threshold", 1.5);
            st.since += dt;
            let hit = !first && x >= thr && st.prev < thr && st.since >= (i.num)("gap", 0.1);
            st.prev = x;
            if hit {
                st.since = 0.0;
                1.0
            } else {
                0.0
            }
        }
        MATH => {
            let a = (i.input)("a").unwrap_or_else(|| (i.num)("a_value", 0.0));
            let b = (i.input)("b").unwrap_or_else(|| (i.num)("b_value", 1.0));
            match MATH_OPS
                .get((i.choice)("op") as usize)
                .copied()
                .unwrap_or("Add")
            {
                "Subtract" => a - b,
                "Multiply" => a * b,
                "Min" => a.min(b),
                "Max" => a.max(b),
                "Mix" => a + (b - a) * (i.num)("mix", 0.5),
                "Difference" => (a - b).abs(),
                _ => a + b,
            }
        }
        REMAP => {
            let x = (i.input)("in").unwrap_or(0.0);
            let (lo, hi) = ((i.num)("in_min", 0.0), (i.num)("in_max", 1.0));
            let mut t = if (hi - lo).abs() < 1e-6 {
                0.0
            } else {
                (x - lo) / (hi - lo)
            };
            if (i.flag)("clamp", true) {
                t = t.clamp(0.0, 1.0);
            }
            let t = t.signum() * t.abs().powf((i.num)("curve", 1.0).max(0.01));
            let (a, b) = ((i.num)("out_min", 0.0), (i.num)("out_max", 1.0));
            a + t * (b - a)
        }
        SMOOTH => {
            let x = (i.input)("in").unwrap_or(0.0);
            if first {
                st.level = x;
            }
            let tau = if x > st.level {
                (i.num)("attack", 0.05)
            } else {
                (i.num)("release", 0.6)
            };
            let alpha = if tau <= 0.0 {
                1.0
            } else {
                1.0 - (-dt / tau).exp()
            };
            st.level += alpha * (x - st.level);
            st.level
        }
        ENVELOPE => {
            let attack = (i.num)("attack", 0.01);
            let hit = fired((i.input)("trigger"));
            if hit && attack <= 0.0 {
                // Instant punch: full on the frame it fires.
                st.level = 1.0;
                st.rising = false;
                return st.level;
            }
            if hit {
                st.rising = true;
            }
            if st.rising {
                st.level += dt / attack.max(1e-4);
                if st.level >= 1.0 {
                    st.level = 1.0;
                    st.rising = false;
                }
            } else {
                st.level *= (-dt / (i.num)("decay", 0.35).max(1e-3)).exp();
            }
            st.level
        }
        COUNTER => {
            if fired((i.input)("reset")) {
                st.count = 0;
            }
            if fired((i.input)("count")) {
                st.count = st.count.wrapping_add(1);
            }
            let n = (i.num)("steps", 4.0).round().max(2.0) as u32;
            (st.count % n) as f32 / (n - 1) as f32
        }
        HOLD => {
            let start = fired((i.input)("start"));
            let stop = fired((i.input)("stop"));
            let on = &mut st.rising;
            if start && *on && (i.flag)("toggle", false) {
                *on = false;
            } else if start {
                *on = true;
                st.since = 0.0;
                st.mark = i.beats;
            } else if stop {
                *on = false;
            }
            if *on {
                st.since += dt;
                let length = (i.num)("length", 0.0);
                if length > 0.0 {
                    let gone = match HOLD_UNITS.get((i.choice)("unit") as usize) {
                        Some(&"seconds") => st.since as f64,
                        Some(&"bars") => (i.beats - st.mark) / 4.0,
                        _ => i.beats - st.mark,
                    };
                    if gone >= length as f64 {
                        *on = false;
                    }
                }
            }
            let (target, time) = if st.rising {
                (1.0, (i.num)("fade_in", 0.0))
            } else {
                (0.0, (i.num)("fade_out", 0.25))
            };
            if time <= 0.0 {
                st.level = target;
            } else {
                let step = dt / time;
                st.level += (target - st.level).clamp(-step, step);
            }
            st.level
        }
        SAMPLE_HOLD => {
            if first || fired((i.input)("trigger")) {
                st.level = (i.input)("in").unwrap_or(0.0);
            }
            st.level
        }
        RANDOM => {
            let x = i.beats * (i.num)("rate", 0.5).max(0.0) as f64;
            let seed = (i.num)("seed", 1.0).round() as u32;
            if (i.flag)("smooth", true) {
                random_walk(x, seed)
            } else {
                random_walk(x.floor(), seed)
            }
        }
        _ => 0.0,
    }
}

/// Where a Video node's playhead is.
#[derive(Clone, Debug, Default)]
struct Play {
    /// 0..1 through the clip (0..2 for ping-pong: there and back).
    pos: f64,
    /// Beat position the beat-locked clip started at.
    mark: f64,
    jumps: u64,
    started: bool,
}

/// A Video node's settings this frame (after modulation).
struct VideoKnobs {
    mode: usize,
    speed: f64,
    bars: f64,
    position: f64,
    start: f64,
    end: f64,
    quality: Quality,
    restart: bool,
    jump: bool,
    hold: bool,
}

fn hash01(n: u64) -> f64 {
    let mut z = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}

impl Play {
    /// Move the playhead by one frame and return where it is, 0..1 within
    /// the trimmed part of the clip.
    fn step(&mut self, k: &VideoKnobs, dt: f64, beats: f64, clip_secs: f64, seed: u64) -> f64 {
        let lap = (k.bars * 4.0).max(0.25);
        if !self.started || k.restart {
            self.started = true;
            self.pos = 0.0;
            self.mark = beats;
        }
        if k.jump {
            self.jumps += 1;
            let r = hash01(seed.wrapping_mul(1_000_003) ^ self.jumps);
            self.pos = r;
            self.mark = beats - r * lap;
        }
        let rate = k.speed / clip_secs.max(1e-3);
        if !k.hold {
            match VIDEO_MODES.get(k.mode).copied().unwrap_or("Loop") {
                "Ping-pong" => self.pos = (self.pos + dt * rate).rem_euclid(2.0),
                "Play once" => self.pos = (self.pos + dt * rate).clamp(0.0, 1.0),
                "Beat-locked" => {
                    self.pos = ((beats - self.mark) / lap * k.speed).rem_euclid(1.0);
                }
                "Scrub" => self.pos = k.position.clamp(0.0, 1.0),
                _ => self.pos = (self.pos + dt * rate).rem_euclid(1.0),
            }
        }
        if self.pos > 1.0 {
            2.0 - self.pos // ping-pong on the way back
        } else {
            self.pos
        }
    }
}

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
    /// Memory of signal nodes (envelopes, counters...).
    sig: HashMap<NodeId, SigState>,
    /// Problems by node, shown as badges.
    pub errors: BTreeMap<NodeId, String>,
    /// Image and video files in use.
    media: MediaLibrary,
    play: HashMap<NodeId, Play>,
    /// What each media node is showing ("1920×1080 · 30 fps · frame 12 / 300").
    media_info: HashMap<NodeId, String>,
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
            sig: HashMap::new(),
            errors: BTreeMap::new(),
            media: MediaLibrary::new(),
            play: HashMap::new(),
            media_info: HashMap::new(),
        }
    }

    /// A line about what an Image / Video node shows (size, rate, frame).
    pub fn media_info(&self, node: NodeId) -> Option<&str> {
        self.media_info.get(&node).map(|s| s.as_str())
    }

    /// Read a media file again (after it was edited or relinked).
    pub fn reload_media(&mut self, path: &std::path::Path) {
        self.media.reload(path);
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
                None if file == MEDIA_SHADER => scenes::compile(MEDIA_SHADER_SRC, file),
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
    /// node being looked at) and return the picture of the first one.
    /// `previews` are extra nodes (e.g. ones not wired to anything yet) run
    /// at a quarter of the size, for their thumbnails. Call
    /// [`Renderer::begin_frame`] before and [`Renderer::end_frame`] after
    /// (once for all evaluators).
    pub fn run(
        &mut self,
        graph: &Graph,
        library: &SceneLibrary,
        renderer: &mut Renderer,
        input: FrameInput,
        targets: &[NodeId],
        previews: &[NodeId],
    ) -> Show {
        self.errors.clear();
        self.values.clear();
        self.live.clear();
        // Forget nodes that were deleted.
        self.rt.retain(|id, _| graph.node(*id).is_some());
        self.sig.retain(|id, _| graph.node(*id).is_some());
        self.play.retain(|id, _| graph.node(*id).is_some());
        self.media_info.clear();
        let main: BTreeSet<NodeId> = graph.plan(targets).into_iter().collect();
        let all: Vec<NodeId> = targets.iter().chain(previews).copied().collect();

        let beats = input.signals.get("beats").unwrap_or(0.0) as f64;
        let time = input.signals.get("time").unwrap_or(0.0) as f64;
        // Which picture each node puts out this frame (`None` = black).
        let mut image: HashMap<NodeId, Option<u64>> = HashMap::new();

        for id in graph.plan(&all) {
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
                k if is_signal_kind(k) => {
                    let values = &self.values;
                    let input_of = |port: &str| {
                        graph
                            .input_link(id, port)
                            .and_then(|l| values.get(&l.from.node).copied())
                    };
                    let v = if node.bypass {
                        // Processors pass their first input through; sources go quiet.
                        node.inputs
                            .iter()
                            .find_map(|p| input_of(&p.name))
                            .filter(|_| {
                                node.outputs
                                    .first()
                                    .is_some_and(|o| o.ty == graph::PortType::Signal)
                            })
                            .unwrap_or(0.0)
                    } else {
                        let num = |name: &str, d: f32| node.param(name).map(eff).unwrap_or(d);
                        let flag = |name: &str, d: bool| match node.param(name).map(|p| &p.value) {
                            Some(ParamValue::Bool(b)) => *b,
                            _ => d,
                        };
                        let choice = |name: &str| match node.param(name).map(|p| &p.value) {
                            Some(ParamValue::Choice(c)) => *c,
                            _ => 0,
                        };
                        for p in node.params.iter().filter(|p| p.def.is_modulatable()) {
                            self.live.insert((id, p.def.name.clone()), eff(p));
                        }
                        let st = self.sig.entry(id).or_default();
                        signal_node(
                            k,
                            st,
                            &SigIn {
                                dt: input.dt,
                                beats,
                                signals: sig,
                                num: &num,
                                flag: &flag,
                                choice: &choice,
                                input: &input_of,
                            },
                        )
                    };
                    let v = if v.is_finite() { v } else { 0.0 };
                    self.values.insert(id, v);
                }
                PROGRAM_OUT => {
                    let src = graph
                        .input_link(id, "image")
                        .and_then(|l| image.get(&l.from.node).copied().flatten());
                    image.insert(id, src);
                }
                kind => {
                    let is_media = registry::is_media_kind(kind);
                    let file = if is_media {
                        Some(MEDIA_SHADER)
                    } else {
                        registry::shader_file(kind)
                    };
                    let Some(file) = file else {
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
                    // Media nodes: their file's picture goes in as the input.
                    let mut media_key = None;
                    if is_media {
                        match self.media_frame(
                            graph,
                            node,
                            renderer,
                            &ctx,
                            input.dt,
                            beats,
                            input.intensity,
                        ) {
                            Ok(Some(k)) => media_key = Some(k),
                            Ok(None) => {
                                image.insert(id, None);
                                continue;
                            }
                            Err(e) => {
                                self.errors.insert(id, e);
                                image.insert(id, None);
                                continue;
                            }
                        }
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
                    let mut inputs: [Option<u64>; 2] = std::array::from_fn(|i| {
                        meta.images
                            .get(i)
                            .and_then(|img| graph.input_link(id, &img.name))
                            .and_then(|l| image.get(&l.from.node).copied().flatten())
                    });
                    if let Some(k) = media_key {
                        inputs = [Some(k), None];
                    }
                    // Nodes only shown as thumbnails render at a quarter size.
                    let scale = if main.contains(&id) { 1.0 } else { 0.25 };
                    let size =
                        graph::node_size(node.resolution, input.output_size, input.proxy * scale);
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
        self.media.end_frame();
        targets
            .first()
            .map(|t| self.picture(*t))
            .unwrap_or(Show::Black)
    }

    /// Get the picture an Image / Video node shows this frame into the
    /// renderer. `Ok(None)` = nothing to show yet (no file, still loading).
    #[allow(clippy::too_many_arguments)]
    fn media_frame(
        &mut self,
        graph: &Graph,
        node: &Node,
        renderer: &mut Renderer,
        ctx: &ModContext,
        dt: f32,
        beats: f64,
        intensity: f32,
    ) -> Result<Option<u64>, String> {
        let id = node.id;
        let key = MEDIA_KEY | self.key(id);
        let path = match node.param(FILE_PARAM).map(|p| &p.value) {
            Some(ParamValue::Text(t)) if !t.trim().is_empty() => std::path::PathBuf::from(t.trim()),
            _ => {
                self.media_info.insert(
                    id,
                    "No file yet: pick one in the Inspector, or drag one from the Media pool."
                        .into(),
                );
                return Ok(None);
            }
        };
        if !path.exists() {
            return Err(format!(
                "File not found: {}. Use \"Find file…\" in the Inspector to relink it.",
                path.display()
            ));
        }
        // Keep showing the last picture while the next one loads.
        let keep = |renderer: &mut Renderer| {
            if renderer.media_picture(key).is_some() {
                renderer.keep_media(key);
                Some(key)
            } else {
                None
            }
        };
        let show = |renderer: &mut Renderer, pic: &media::Picture| {
            if renderer.media_picture(key) == Some(pic.id) {
                renderer.keep_media(key);
            } else {
                renderer.upload_media(key, (pic.width, pic.height), pic.id, &pic.rgba);
            }
            Some(key)
        };
        if node.kind == IMAGE {
            return match self.media.image(&path) {
                Status::Ready(pic) => {
                    self.media_info
                        .insert(id, format!("{} × {}", pic.width, pic.height));
                    Ok(show(renderer, &pic))
                }
                Status::Loading => {
                    self.media_info.insert(id, "Loading…".into());
                    Ok(keep(renderer))
                }
                Status::Failed(e) => Err(e),
            };
        }

        // Video: settings after modulation, plus its trigger / gate inputs.
        let num = |name: &str, d: f32| {
            node.param(name)
                .map(|p| effective_value(graph, id, p, ctx, &self.values, intensity))
                .unwrap_or(d) as f64
        };
        let choice = |name: &str| match node.param(name).map(|p| &p.value) {
            Some(ParamValue::Choice(c)) => *c,
            _ => 0,
        };
        let input = |port: &str| {
            graph
                .input_link(id, port)
                .and_then(|l| self.values.get(&l.from.node).copied())
                .unwrap_or(0.0)
                > 0.5
        };
        let knobs = VideoKnobs {
            mode: choice("mode") as usize,
            speed: num("speed", 1.0),
            bars: num("bars", 4.0),
            position: num("position", 0.0),
            start: num("start", 0.0),
            end: num("end", 1.0),
            quality: Quality::from_index(choice("quality")),
            restart: input("restart"),
            jump: input("jump"),
            hold: input("hold"),
        };
        for p in node.params.iter().filter(|p| p.def.is_modulatable()) {
            let v = effective_value(graph, id, p, ctx, &self.values, intensity);
            self.live.insert((id, p.def.name.clone()), v);
        }
        let stream = self.media.video(&path, knobs.quality);
        let info = match stream.info() {
            Status::Ready(i) => i,
            Status::Loading => {
                self.media_info.insert(id, "Opening…".into());
                return Ok(keep(renderer));
            }
            Status::Failed(e) => return Err(e),
        };
        let (a, b) = (
            knobs.start.min(knobs.end).clamp(0.0, 1.0),
            knobs.start.max(knobs.end).clamp(0.0, 1.0),
        );
        let span = (b - a).max(1e-4);
        let t = self.play.entry(id).or_default().step(
            &knobs,
            dt as f64,
            beats,
            info.seconds * span,
            id.0 as u64,
        );
        let last = (info.frames - 1).max(0);
        let frame = (((a + t * span) * last as f64).round() as i64).clamp(0, last);
        stream.request(frame);
        let pic = stream.frame_near(frame);
        let (dw, dh) = info.decode_size(knobs.quality);
        self.media_info.insert(
            id,
            format!(
                "{} × {} ({dw} × {dh}) · {:.2} fps · {:.1} s · {} · frame {} / {}",
                info.width,
                info.height,
                info.fps,
                info.seconds,
                info.codec,
                frame + 1,
                info.frames
            ),
        );
        Ok(match pic {
            Some(p) => show(renderer, &p),
            None => keep(renderer),
        })
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

#[cfg(test)]
mod signal_tests {
    use super::*;

    /// Run `kind` for `frames` frames of `dt`, feeding `inputs(frame)`.
    fn run(
        kind: &str,
        params: &[(&str, f32)],
        frames: usize,
        dt: f32,
        inputs: &dyn Fn(usize, &str) -> Option<f32>,
    ) -> Vec<f32> {
        let sig = SignalState::new();
        let mut st = SigState::default();
        (0..frames)
            .map(|f| {
                let num = |n: &str, d: f32| {
                    params
                        .iter()
                        .find(|(k, _)| *k == n)
                        .map(|(_, v)| *v)
                        .unwrap_or(d)
                };
                let input = |p: &str| inputs(f, p);
                signal_node(
                    kind,
                    &mut st,
                    &SigIn {
                        dt,
                        beats: f as f64 * 0.25,
                        signals: &sig,
                        num: &num,
                        flag: &|_, d| d,
                        choice: &|_| 0,
                        input: &input,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn envelope_punches_and_fades_by_time() {
        let trig = |f: usize, p: &str| (p == "trigger").then_some(if f == 1 { 1.0 } else { 0.0 });
        let v = run(
            ENVELOPE,
            &[("attack", 0.0), ("decay", 0.5)],
            40,
            0.01,
            &trig,
        );
        assert_eq!(v[0], 0.0);
        assert_eq!(v[1], 1.0);
        // After 0.5 s it is down to 1/e, whatever the frame rate.
        assert!((v[39] - (-0.38f32 / 0.5).exp()).abs() < 1e-3);
        let fast = run(
            ENVELOPE,
            &[("attack", 0.0), ("decay", 0.5)],
            21,
            0.02,
            &trig,
        );
        assert!((fast[20] - v[39]).abs() < 0.03);
    }

    #[test]
    fn counter_steps_and_resets() {
        let inputs = |f: usize, p: &str| match p {
            "count" => Some(1.0),
            "reset" => Some(if f == 6 { 1.0 } else { 0.0 }),
            _ => None,
        };
        let v = run(COUNTER, &[("steps", 3.0)], 8, 0.01, &inputs);
        assert_eq!(&v[..6], &[0.5, 1.0, 0.0, 0.5, 1.0, 0.0]);
        assert_eq!(v[6], 0.5); // reset, then counted this frame's trigger
    }

    #[test]
    fn gate_fires_once_per_crossing() {
        let wave = |f: usize, _: &str| Some(if (f / 5) % 2 == 1 { 3.0 } else { 0.0 });
        let v = run(GATE, &[("threshold", 1.5), ("gap", 0.0)], 20, 0.01, &wave);
        let fires: Vec<usize> = (0..20).filter(|i| v[*i] > 0.5).collect();
        assert_eq!(fires, vec![5, 15]);
    }

    #[test]
    fn remap_math_smooth_beat_trigger() {
        let x = |_: usize, _: &str| Some(1.5);
        let v = run(
            REMAP,
            &[
                ("in_min", 1.0),
                ("in_max", 2.0),
                ("out_min", 10.0),
                ("out_max", 20.0),
            ],
            1,
            0.01,
            &x,
        );
        assert!((v[0] - 15.0).abs() < 1e-4);
        let v = run(MATH, &[], 1, 0.01, &x);
        assert_eq!(v[0], 3.0); // Add, both plugged with 1.5
        // Smooth: starts at the input, then follows with the rise time.
        let step = |f: usize, _: &str| Some(if f == 0 { 0.0 } else { 1.0 });
        let v = run(SMOOTH, &[("attack", 0.1)], 11, 0.01, &step);
        assert!((v[10] - (1.0 - (-1.0f32).exp())).abs() < 0.01);
        // Beat trigger every beat: beats advance 0.25 per frame here.
        let none = |_: usize, _: &str| None;
        let v = run(BEAT_TRIGGER, &[("every", 1.0)], 9, 0.01, &none);
        let fires: Vec<usize> = (0..9).filter(|i| v[*i] > 0.5).collect();
        assert_eq!(fires, vec![4, 8]);
    }

    #[test]
    fn audio_trigger_fires_on_kicks() {
        let mut sig = SignalState::new();
        let mut f = signals::SignalFrame::default();
        let mut st = SigState::default();
        let mut fires = vec![];
        for frame in 0..30 {
            f.hop_index += 1;
            f.time = frame as f64 * 0.02;
            if frame % 10 == 5 {
                f.rhythm.kick.fire(f.time, 1.0);
            }
            sig.update(&f, 0.02, [64.0, 64.0]);
            let v = signal_node(
                AUDIO_TRIGGER,
                &mut st,
                &SigIn {
                    dt: 0.02,
                    beats: 0.0,
                    signals: &sig,
                    num: &|_, d| d,
                    flag: &|_, d| d,
                    choice: &|_| 0, // kick
                    input: &|_| None,
                },
            );
            if v > 0.5 {
                fires.push(frame);
            }
        }
        assert_eq!(fires, vec![5, 15, 25]);
    }

    #[test]
    fn hold_stays_on_from_start_until_stop_or_length() {
        let fire_at = |at: usize| move |f: usize| (f == at).then_some(1.0);
        // Start at frame 2, Stop at frame 6, instant fades.
        let out = run(HOLD, &[("fade_out", 0.0)], 10, 0.1, &|f, p| match p {
            "start" => fire_at(2)(f),
            "stop" => fire_at(6)(f),
            _ => None,
        });
        assert_eq!(out, vec![0., 0., 1., 1., 1., 1., 0., 0., 0., 0.]);
        // No Stop, length 1 beat (a frame = 0.25 beats here): 4 frames on.
        let out = run(
            HOLD,
            &[("length", 1.0), ("fade_out", 0.0)],
            10,
            0.1,
            &|f, p| (p == "start").then(|| fire_at(1)(f)).flatten(),
        );
        assert_eq!(out, vec![0., 1., 1., 1., 1., 0., 0., 0., 0., 0.]);
        // Fade out over 0.4 s at 0.1 s frames: 1 → 0.75 → 0.5 → 0.25 → 0.
        let out = run(HOLD, &[("fade_out", 0.4)], 8, 0.1, &|f, p| match p {
            "start" => fire_at(0)(f),
            "stop" => fire_at(2)(f),
            _ => None,
        });
        for (got, want) in out.iter().zip([1.0, 1.0, 0.75, 0.5, 0.25, 0.0, 0.0, 0.0]) {
            assert!((got - want).abs() < 1e-5, "{out:?}");
        }
    }

    fn knobs(mode: &str) -> VideoKnobs {
        VideoKnobs {
            mode: VIDEO_MODES.iter().position(|m| *m == mode).unwrap(),
            speed: 1.0,
            bars: 1.0,
            position: 0.3,
            start: 0.0,
            end: 1.0,
            quality: Quality::Low,
            restart: false,
            jump: false,
            hold: false,
        }
    }

    #[test]
    fn video_playheads_follow_their_mode() {
        // A 2 s clip, 0.5 s steps.
        let run = |k: &VideoKnobs, steps: usize| {
            let mut p = Play::default();
            (0..steps)
                .map(|i| p.step(k, 0.5, i as f64, 2.0, 1))
                .collect::<Vec<f64>>()
        };
        // First frame starts at 0 and moves; loop wraps.
        assert_eq!(
            run(&knobs("Loop"), 6),
            vec![0.25, 0.5, 0.75, 0.0, 0.25, 0.5]
        );
        // Ping-pong goes there and back.
        assert_eq!(
            run(&knobs("Ping-pong"), 8),
            vec![0.25, 0.5, 0.75, 1.0, 0.75, 0.5, 0.25, 0.0]
        );
        // Play once stops at the end.
        assert_eq!(
            run(&knobs("Play once"), 6),
            vec![0.25, 0.5, 0.75, 1.0, 1.0, 1.0]
        );
        // Beat-locked: 1 bar = 4 beats per lap, whatever the clip length.
        assert_eq!(
            run(&knobs("Beat-locked"), 6),
            vec![0.0, 0.25, 0.5, 0.75, 0.0, 0.25]
        );
        // Scrub follows the position knob.
        assert_eq!(run(&knobs("Scrub"), 2), vec![0.3, 0.3]);
        // Hold freezes; jump lands somewhere repeatable.
        let mut p = Play::default();
        let mut k = knobs("Loop");
        p.step(&k, 0.5, 0.0, 2.0, 1);
        k.hold = true;
        assert_eq!(p.step(&k, 0.5, 1.0, 2.0, 1), 0.25);
        k.hold = false;
        k.jump = true;
        let a = p.step(&k, 0.0, 2.0, 2.0, 1);
        let mut q = Play::default();
        q.step(&knobs("Loop"), 0.5, 0.0, 2.0, 1);
        assert_eq!(q.step(&k, 0.0, 2.0, 2.0, 1), a);
        assert!((0.0..1.0).contains(&a));
    }
}
