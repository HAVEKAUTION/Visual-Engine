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

/// Media for the compositing demo (whatever is missing gets its layer muted).
#[derive(Clone, Debug, Default)]
pub struct DemoMedia {
    pub video: Option<String>,
    pub photo: Option<String>,
    pub camera: Option<String>,
}

/// The compositing demo: a scene with a video over it in Hard Light, a photo
/// inside a star that pulses on the beat, and the webcam showing only what
/// moves (a Chroma Key ready for a green screen), all in one Layer Mixer.
/// Without a camera the webcam part is left out.
///
/// ```text
/// Kaleido Bloom ──────────────────────────────► Layers 1 (Normal)
/// Video ──────────────────────────────────────► Layers 2 (Hard Light)
/// Photo (mask: Pulsing star) ─────────────────► Layers 3 (Normal)
/// Webcam ─► Chroma Key (off) ─► Motion Mask ──► Layers 4 (Screen) ─► Program Out
/// ```
pub fn compositing(library: &SceneLibrary, media: &DemoMedia) -> Result<(Graph, NodeId), String> {
    let mut g = Graph::new();
    let mut cmds = Vec::new();
    let add = |g: &mut Graph, kind: &str, label: &str, pos: [f32; 2]| -> Result<NodeId, String> {
        let spec = registry::spec_for(kind, library)?;
        let mut n = g.new_node(&spec);
        n.pos = pos;
        if !label.is_empty() {
            n.label = label.into();
        }
        let id = n.id;
        g.apply(Command::AddNode(Box::new(n)))
            .map_err(|e| e.to_string())?;
        Ok(id)
    };
    let shader = registry::shader_kind;
    let scene = add(&mut g, &shader("kaleido_bloom.wgsl"), "", [0.0, 0.0])?;
    let video = add(&mut g, registry::VIDEO, "Video", [0.0, 230.0])?;
    let photo = add(&mut g, registry::IMAGE, "Photo", [0.0, 580.0])?;
    let star = add(
        &mut g,
        &shader("mask_shape.wgsl"),
        "Pulsing star",
        [-250.0, 640.0],
    )?;
    let mix = add(&mut g, registry::LAYER_MIXER, "Layers", [760.0, 420.0])?;
    let out = add(&mut g, PROGRAM_OUT, "", [1000.0, 420.0])?;

    let text = |s: &Option<String>| ParamValue::Text(s.clone().unwrap_or_default());
    cmds.extend([
        wire(scene, mix, &registry::layer_port(1)),
        wire(video, mix, &registry::layer_port(2)),
        wire(photo, mix, &registry::layer_port(3)),
        wire(star, photo, graph::MASK_PORT),
        wire(mix, out, "image"),
        set(video, registry::FILE_PARAM, text(&media.video)),
        set(video, "mode", ParamValue::Choice(3)), // beat-locked
        set(photo, registry::FILE_PARAM, text(&media.photo)),
        set(star, "shape", ParamValue::Choice(3)),
        set(star, "size", ParamValue::Float(0.55)),
        set(star, "beat_pulse", ParamValue::Float(0.5)),
        set(star, "kick_pulse", ParamValue::Float(0.3)),
        set(star, "spin_speed", ParamValue::Float(0.1)),
        set(star, "feather", ParamValue::Float(0.03)),
        set(mix, "mode2", ParamValue::Choice(blend("Hard Light"))),
    ]);
    // The webcam only if there is one (an empty name would open the first
    // camera found).
    let mut frames = vec![
        (
            "Video in Hard Light (beat-locked)",
            [-30.0, 196.0, 240.0, 330.0],
            [150, 110, 240],
        ),
        (
            "Photo inside a star that pulses on the beat",
            [-280.0, 546.0, 520.0, 300.0],
            [80, 190, 120],
        ),
    ];
    if let Some(name) = &media.camera {
        let cam = add(&mut g, registry::CAMERA, "Webcam", [0.0, 920.0])?;
        let key = add(&mut g, &shader("mask_chroma_key.wgsl"), "", [240.0, 920.0])?;
        let motion = add(&mut g, &shader("mask_motion.wgsl"), "", [480.0, 920.0])?;
        cmds.extend([
            wire(cam, key, "input"),
            wire(key, motion, "input"),
            wire(motion, mix, &registry::layer_port(4)),
            set(cam, registry::CAMERA_PARAM, ParamValue::Text(name.clone())),
            set(motion, "output", ParamValue::Choice(1)), // cut-out
            set(motion, "trail", ParamValue::Float(0.9)),
            set(mix, "mode4", ParamValue::Choice(blend("Screen"))),
            Command::SetBypass {
                node: key,
                bypass: true,
            },
        ]);
        frames.push((
            "Webcam: only what moves. Green screen? Turn Chroma Key on",
            [-30.0, 886.0, 750.0, 250.0],
            [230, 150, 60],
        ));
    }
    // No media for a layer: mute it (it would only show black).
    for (i, missing) in [(2, media.video.is_none()), (3, media.photo.is_none())] {
        if missing {
            cmds.push(set(
                mix,
                &registry::layer_param("mute", i),
                ParamValue::Bool(true),
            ));
        }
    }
    for (label, rect, colour) in frames {
        let id = g.new_frame_id();
        cmds.push(Command::PutFrame(Frame {
            id,
            label: label.into(),
            rect,
            colour,
        }));
        for c in cmds.drain(..) {
            g.apply(c).map_err(|e| e.to_string())?;
        }
    }
    Ok((g, out))
}

fn blend(name: &str) -> u32 {
    scenes::blend::mode_index(name).unwrap_or(0) as u32
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
