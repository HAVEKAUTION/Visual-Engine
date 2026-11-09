//! The starter graph: a small example to learn from (shown on first launch,
//! and from the graph's right-click menu).
//!
//! ```text
//! 01 Warehouse Tunnel ─► 02 Transform ─► 04 Merge (top) ─► 07 Program Out
//! 03 Aurora Drift ─────────────────────► 04 Merge (base)
//! 05 Audio Signal (bass) ─► 08 Remap ┄┄► 02 Transform · zoom
//! 06 LFO (8 beats)                   ┄┄► 04 Merge · opacity
//! ```

use graph::{Command, Endpoint, Frame, Graph, Link, NodeId, OutputRef, ParamValue, WireMod};
use scenes::SceneLibrary;
use signals::modulation::Shape;

use crate::registry::{self, AUDIO_SIGNAL, IMAGE_OUT, LFO, PROGRAM_OUT, REMAP, VALUE_OUT};

/// The starter graph and its Program Out node.
pub fn build(library: &SceneLibrary) -> Result<(Graph, NodeId), String> {
    let mut g = Graph::new();
    let add = |g: &mut Graph, kind: &str, pos: [f32; 2]| -> Result<NodeId, String> {
        let spec = registry::spec_for(kind, library)?;
        let mut n = g.new_node(&spec);
        n.pos = pos;
        let id = n.id;
        g.apply(Command::AddNode(Box::new(n)))
            .map_err(|e| e.to_string())?;
        Ok(id)
    };
    let tunnel = add(
        &mut g,
        &registry::shader_kind("warehouse_tunnel.wgsl"),
        [0.0, 0.0],
    )?;
    let transform = add(
        &mut g,
        &registry::shader_kind("fx_transform.wgsl"),
        [240.0, 0.0],
    )?;
    let aurora = add(
        &mut g,
        &registry::shader_kind("aurora_drift.wgsl"),
        [0.0, 224.0],
    )?;
    let merge = add(
        &mut g,
        &registry::shader_kind("fx_merge.wgsl"),
        [480.0, 112.0],
    )?;
    let bass = add(&mut g, AUDIO_SIGNAL, [-240.0, -128.0])?;
    let lfo = add(&mut g, LFO, [240.0, 288.0])?;
    let out = add(&mut g, PROGRAM_OUT, [720.0, 112.0])?;
    let remap = add(&mut g, REMAP, [0.0, -128.0])?;

    let modulate = |from: NodeId, to: NodeId, param: &str, depth: f32| {
        Command::Connect(Link {
            modulation: Some(WireMod {
                shape: Shape::Linear,
                depth,
                enabled: true,
            }),
            ..Link::new(OutputRef::new(from, VALUE_OUT), Endpoint::param(to, param))
        })
    };
    let mut cmds = vec![
        wire(tunnel, transform, "input"),
        wire(transform, merge, "top"),
        wire(aurora, merge, "base"),
        wire(merge, out, "image"),
        Command::Connect(Link::new(
            OutputRef::new(bass, VALUE_OUT),
            Endpoint::input(remap, "in"),
        )),
        // Bass 0..4 (1 = typical) becomes 0..1, gently curved, then pushes
        // the zoom up to +0.3.
        set(remap, "in_max", ParamValue::Float(4.0)),
        set(remap, "curve", ParamValue::Float(1.5)),
        modulate(remap, transform, "zoom", 0.08),
        modulate(lfo, merge, "opacity", -0.6),
        set(transform, "spin_speed", ParamValue::Float(0.05)),
        set(transform, "kick_punch", ParamValue::Float(0.15)),
        set(lfo, "beats", ParamValue::Float(8.0)),
        Command::SetLabel {
            node: remap,
            label: "Bass → 0..1".into(),
        },
    ];
    let id = g.new_frame_id();
    cmds.push(Command::PutFrame(Frame {
        id,
        label: "Sound → motion".into(),
        rect: [-272.0, -176.0, 480.0, 128.0],
        colour: [80, 150, 240],
    }));
    for c in cmds.drain(..) {
        g.apply(c).map_err(|e| e.to_string())?;
    }
    Ok((g, out))
}

fn wire(from: NodeId, to: NodeId, port: &str) -> Command {
    Command::Connect(Link::new(
        OutputRef::new(from, IMAGE_OUT),
        Endpoint::input(to, port),
    ))
}

fn set(node: NodeId, param: &str, value: ParamValue) -> Command {
    Command::SetParam {
        node,
        param: param.into(),
        value,
    }
}

/// A readable list of the graph's nodes and wires, for the test panel.
pub fn describe(g: &Graph) -> Vec<String> {
    let name = |id: NodeId| {
        g.node(id)
            .map(|n| format!("{:02} {}", id.0, n.label))
            .unwrap_or_else(|| format!("{:02} ?", id.0))
    };
    let mut lines: Vec<String> = g.nodes().iter().map(|n| name(n.id)).collect();
    for l in g.links() {
        let to = match &l.to {
            Endpoint::Input { node, port } => format!("{} · {port}", name(*node)),
            Endpoint::Param { node, param } => format!("{} · {param} (modulates)", name(*node)),
        };
        lines.push(format!("{}  →  {to}", name(l.from.node)));
    }
    lines
}
