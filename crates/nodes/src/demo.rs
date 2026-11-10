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

/// Media for the compositing demo: up to two videos and three pictures (from
/// the Media panel) and a camera. Layers without media are muted.
#[derive(Clone, Debug, Default)]
pub struct DemoMedia {
    pub videos: Vec<String>,
    pub photos: Vec<String>,
    pub camera: Option<String>,
}

/// The compositing demo: one Layer Mixer showing off blend modes, masks and
/// placement with the user's own media.
///
/// ```text
/// 1 Warehouse Tunnel                                  Normal
/// 2 Video 1 (beat-locked)                           Screen
/// 3 Video 2 (ping-pong)                             Screen, fades in and out (LFO)
/// 4 Picture 1, mask: star pulsing on the beat       Normal
/// 5 Picture 2 ─► Corner Pin (a tilted card)         Normal
/// 6 Picture 3, mask: scrolling bands                Screen
///   then Layers ─► Glow ─► Colour Grade ─► Program Out
/// 7 Webcam ─► Chroma Key (off) ─► Motion Mask       Screen (muted to start)
/// ```
/// Without a camera the webcam part is left out (an empty camera name would
/// open the first camera found).
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
    let file =
        |list: &[String], i: usize| ParamValue::Text(list.get(i).cloned().unwrap_or_default());
    const ROW: f32 = 270.0;
    let (src_x, fx_x, mask_x, mix_x) = (0.0, 250.0, -260.0, 560.0);

    let mix = add(&mut g, registry::LAYER_MIXER, "Layers", [mix_x, ROW * 2.5])?;
    // One finish for the whole picture so the layers sit in one look.
    let glow = add(
        &mut g,
        &shader("fx_glow.wgsl"),
        "",
        [mix_x + 250.0, ROW * 2.5],
    )?;
    let grade = add(
        &mut g,
        &shader("fx_colour.wgsl"),
        "Final look",
        [mix_x + 500.0, ROW * 2.5],
    )?;
    let out = add(&mut g, PROGRAM_OUT, "", [mix_x + 750.0, ROW * 2.5])?;
    cmds.extend([
        wire(mix, glow, "input"),
        wire(glow, grade, "input"),
        wire(grade, out, "image"),
        set(glow, "amount", ParamValue::Float(0.6)),
        set(glow, "threshold", ParamValue::Float(0.55)),
        set(grade, "contrast", ParamValue::Float(1.12)),
        set(grade, "saturation", ParamValue::Float(1.1)),
    ]);
    let layer = |cmds: &mut Vec<Command>, from: NodeId, i: usize, mode: &str| {
        cmds.push(wire(from, mix, &registry::layer_port(i)));
        cmds.push(set(
            mix,
            &registry::layer_param("mode", i),
            ParamValue::Choice(blend(mode)),
        ));
    };
    let mut frames = Vec::new();

    // 1. The scene.
    let scene = add(&mut g, &shader("warehouse_tunnel.wgsl"), "", [src_x, 0.0])?;
    cmds.push(set(scene, "colour_cycle", ParamValue::Float(0.3)));
    layer(&mut cmds, scene, 1, "Normal");

    // 2. Video 1, beat-locked, in Screen.
    let v1 = add(&mut g, registry::VIDEO, "Video 1", [src_x, ROW])?;
    cmds.push(set(v1, registry::FILE_PARAM, file(&media.videos, 0)));
    cmds.push(set(v1, "mode", ParamValue::Choice(3)));
    layer(&mut cmds, v1, 2, "Screen");
    cmds.push(set(
        mix,
        &registry::layer_param("opacity", 2),
        ParamValue::Float(0.6),
    ));
    frames.push((
        "Video 1: Screen, beat-locked",
        [src_x - 30.0, ROW - 34.0, 240.0, 250.0],
        [150, 110, 240],
    ));

    // 3. Video 2, ping-pong, in Screen, fading in and out every 8 beats.
    let v2 = add(&mut g, registry::VIDEO, "Video 2", [src_x, ROW * 2.0])?;
    let lfo = add(&mut g, LFO, "Fade (8 beats)", [mask_x, ROW * 2.0 + 40.0])?;
    cmds.push(set(v2, registry::FILE_PARAM, file(&media.videos, 1)));
    cmds.push(set(v2, "mode", ParamValue::Choice(1)));
    cmds.push(set(lfo, "beats", ParamValue::Float(8.0)));
    layer(&mut cmds, v2, 3, "Screen");
    cmds.push(set(
        mix,
        &registry::layer_param("opacity", 3),
        ParamValue::Float(0.2),
    ));
    cmds.push(Command::Connect(Link {
        modulation: Some(WireMod {
            shape: Shape::Linear,
            depth: 0.7,
            enabled: true,
        }),
        ..Link::new(
            OutputRef::new(lfo, VALUE_OUT),
            Endpoint::param(mix, &registry::layer_param("opacity", 3)),
        )
    }));
    frames.push((
        "Video 2: Screen, fading in and out",
        [mask_x - 30.0, ROW * 2.0 - 34.0, 500.0, 250.0],
        [90, 150, 240],
    ));

    // 4. Picture 1 inside a star that pulses on the beat.
    let p1 = add(&mut g, registry::IMAGE, "Picture 1", [src_x, ROW * 3.0])?;
    let star = add(
        &mut g,
        &shader("mask_shape.wgsl"),
        "Pulsing star",
        [mask_x, ROW * 3.0 + 40.0],
    )?;
    cmds.extend([
        set(p1, registry::FILE_PARAM, file(&media.photos, 0)),
        wire(star, p1, graph::MASK_PORT),
        set(star, "shape", ParamValue::Choice(3)),
        set(star, "size", ParamValue::Float(0.5)),
        set(star, "beat_pulse", ParamValue::Float(0.5)),
        set(star, "kick_pulse", ParamValue::Float(0.3)),
        set(star, "spin_speed", ParamValue::Float(0.1)),
        set(star, "feather", ParamValue::Float(0.03)),
    ]);
    layer(&mut cmds, p1, 4, "Normal");
    frames.push((
        "Picture 1 in a star that pulses on the beat",
        [mask_x - 30.0, ROW * 3.0 - 34.0, 500.0, 250.0],
        [80, 190, 120],
    ));

    // 5. Picture 2 pinned like a tilted card, punching on the kick.
    let p2 = add(&mut g, registry::IMAGE, "Picture 2", [src_x, ROW * 4.0])?;
    let pin = add(&mut g, &shader("fx_corner_pin.wgsl"), "", [fx_x, ROW * 4.0])?;
    cmds.extend([
        set(p2, registry::FILE_PARAM, file(&media.photos, 1)),
        wire(p2, pin, "input"),
        set(pin, "kick_punch", ParamValue::Float(0.12)),
    ]);
    for (name, v) in [
        ("tl_x", 0.58),
        ("tl_y", 0.12),
        ("tr_x", 0.92),
        ("tr_y", 0.2),
        ("br_x", 0.9),
        ("br_y", 0.62),
        ("bl_x", 0.6),
        ("bl_y", 0.52),
    ] {
        cmds.push(set(pin, name, ParamValue::Float(v)));
    }
    layer(&mut cmds, pin, 5, "Normal");
    frames.push((
        "Picture 2 on a tilted card (drag its corners in the viewer)",
        [src_x - 30.0, ROW * 4.0 - 34.0, 500.0, 250.0],
        [230, 190, 80],
    ));

    // 6. Picture 3 through scrolling bands, in Screen.
    let p3 = add(&mut g, registry::IMAGE, "Picture 3", [src_x, ROW * 5.0])?;
    let bands = add(
        &mut g,
        &shader("mask_gradient.wgsl"),
        "Scrolling bands",
        [mask_x, ROW * 5.0 + 40.0],
    )?;
    cmds.extend([
        set(p3, registry::FILE_PARAM, file(&media.photos, 2)),
        wire(bands, p3, graph::MASK_PORT),
        set(bands, "repeat", ParamValue::Choice(2)),
        set(bands, "bands", ParamValue::Float(4.0)),
        set(bands, "angle", ParamValue::Float(0.125)),
        set(bands, "scroll_speed", ParamValue::Float(0.3)),
    ]);
    layer(&mut cmds, p3, 6, "Screen");
    cmds.push(set(
        mix,
        &registry::layer_param("opacity", 6),
        ParamValue::Float(0.6),
    ));
    frames.push((
        "Picture 3 through scrolling bands, Screen",
        [mask_x - 30.0, ROW * 5.0 - 34.0, 500.0, 250.0],
        [200, 110, 160],
    ));

    // 7. The webcam: only what moves. Muted to start (unmute it in the Layers node).
    if let Some(name) = &media.camera {
        let cam = add(&mut g, registry::CAMERA, "Webcam", [src_x, ROW * 6.0])?;
        let key = add(
            &mut g,
            &shader("mask_chroma_key.wgsl"),
            "",
            [fx_x, ROW * 6.0],
        )?;
        let motion = add(
            &mut g,
            &shader("mask_motion.wgsl"),
            "",
            [fx_x + 250.0, ROW * 6.0],
        )?;
        cmds.extend([
            wire(cam, key, "input"),
            wire(key, motion, "input"),
            set(cam, registry::CAMERA_PARAM, ParamValue::Text(name.clone())),
            set(motion, "output", ParamValue::Choice(1)), // cut-out
            set(motion, "trail", ParamValue::Float(0.9)),
            set(
                mix,
                &registry::layer_param("mute", 7),
                ParamValue::Bool(true),
            ),
            set(
                mix,
                &registry::layer_param("opacity", 7),
                ParamValue::Float(0.7),
            ),
            Command::SetBypass {
                node: key,
                bypass: true,
            },
        ]);
        layer(&mut cmds, motion, 7, "Screen");
        frames.push((
            "Webcam: only what moves (muted: unmute layer 7). Green screen? Turn Chroma Key on",
            [src_x - 30.0, ROW * 6.0 - 34.0, 750.0, 250.0],
            [230, 150, 60],
        ));
    }

    // No media for a layer: mute it (it would only show black).
    for (i, missing) in [
        (2, media.videos.is_empty()),
        (3, media.videos.len() < 2),
        (4, media.photos.is_empty()),
        (5, media.photos.len() < 2),
        (6, media.photos.len() < 3),
    ] {
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
    for c in cmds.drain(..) {
        g.apply(c).map_err(|e| e.to_string())?;
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
