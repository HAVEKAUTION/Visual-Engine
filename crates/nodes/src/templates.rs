//! Graph templates: ready-made node layouts that look like a VJ set behind a
//! DJ, inserted from the graph's right-click menu (Templates). Each one is an
//! ordinary graph, so every part can be opened up and changed, and each has a
//! note saying what it does and what to try.
//!
//! They share a recipe that keeps the tone together: a few strong sources, a
//! **Switch** that changes between them on the beat (a Beat Trigger feeding a
//! Counter), then **Glow** and a gentle **Colour Grade** at the end so
//! everything sits in one look.

use graph::{Command, Endpoint, Frame, Graph, Link, NodeId, OutputRef, ParamValue, WireMod};
use scenes::SceneLibrary;
use signals::modulation::Shape;

use crate::demo::DemoMedia;
use crate::registry::{
    self, AUDIO_TRIGGER, BEAT_TRIGGER, COUNTER, ENVELOPE, IMAGE_OUT, LAYER_MIXER, PROGRAM_OUT,
    TRIGGER_OUT, VALUE_OUT,
};

/// One template in the menu.
#[derive(Clone, Copy, Debug)]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    /// One or two sentences: what it does and what to try.
    pub note: &'static str,
    /// Which of your media it uses ("" = none).
    pub uses: &'static str,
}

pub const ALL: &[Template] = &[
    Template {
        id: "peak_time",
        name: "Peak time",
        note: "Four big scenes (tunnel, furnace, kaleidoscope, CRT), a new one every 2 bars \
               with a zoom-through. Change Every on the Beat Trigger for longer or shorter.",
        uses: "",
    },
    Template {
        id: "warm_up",
        name: "Warm-up",
        note: "Slow and moody: rainy city lights, aurora and a slow kaleidoscope, dissolving \
               into each other every 4 bars. For the start of the night.",
        uses: "",
    },
    Template {
        id: "clip_cuts",
        name: "Your clips, cut to the beat",
        note: "Your videos and pictures, cutting to the next one every bar, each video \
               jumping to a new spot when it comes back. The colour split kicks with the \
               kick drum. Import clips in the Media panel first.",
        uses: "up to 2 videos and 2 pictures",
    },
    Template {
        id: "kaleido_clip",
        name: "Kaleidoscope clip",
        note: "A clip folded into a spinning kaleidoscope with streaming trails. Any video \
               works, even a boring one.",
        uses: "your first video (or picture)",
    },
    Template {
        id: "photo_pulse",
        name: "Photo pulse",
        note: "A picture punching to the kick in the middle of a spectrum ring, with \
               echoes. Try a portrait, an album cover or an artist photo.",
        uses: "your first picture",
    },
    Template {
        id: "logo_stamp",
        name: "Logo on the kick",
        note: "Your logo (white on black works best) stamped over a light tunnel, \
               glowing in on every kick and fading out. Luma Key removes its black \
               background.",
        uses: "your first picture",
    },
    Template {
        id: "glitch_drop",
        name: "Glitch on the drop",
        note: "Disco room and synthwave CRT switch on every section change; the drop \
               sends a burst of glitch and colour split through everything.",
        uses: "",
    },
    Template {
        id: "webcam_mirror",
        name: "Webcam mirror",
        note: "The webcam folded four ways with long trails, glowing over a kaleidoscope. \
               Dance in front of it.",
        uses: "your webcam",
    },
];

pub fn find(id: &str) -> Option<&'static Template> {
    ALL.iter().find(|t| t.id == id)
}

/// Build a template into a fresh graph (pasted into the user's graph by the
/// editor). Returns the graph and its Program Out.
pub fn build(
    id: &str,
    library: &SceneLibrary,
    media: &DemoMedia,
) -> Result<(Graph, NodeId), String> {
    let t = find(id).ok_or_else(|| format!("No template called {id}"))?;
    let mut b = Builder::new(library);
    match id {
        "peak_time" => peak_time(&mut b)?,
        "warm_up" => warm_up(&mut b)?,
        "clip_cuts" => clip_cuts(&mut b, media)?,
        "kaleido_clip" => kaleido_clip(&mut b, media)?,
        "photo_pulse" => photo_pulse(&mut b, media)?,
        "logo_stamp" => logo_stamp(&mut b, media)?,
        "glitch_drop" => glitch_drop(&mut b)?,
        "webcam_mirror" => webcam_mirror(&mut b, media)?,
        _ => return Err(format!("No template called {id}")),
    }
    b.frame_all(t.name, [240, 160, 80])?;
    let out = b.out.ok_or("template has no output")?;
    Ok((b.g, out))
}

// ---------------------------------------------------------------- builder

const COL: f32 = 250.0;
const ROW: f32 = 240.0;

struct Builder<'a> {
    g: Graph,
    lib: &'a SceneLibrary,
    out: Option<NodeId>,
}

impl<'a> Builder<'a> {
    fn new(lib: &'a SceneLibrary) -> Self {
        Self {
            g: Graph::new(),
            lib,
            out: None,
        }
    }

    fn apply(&mut self, c: Command) -> Result<(), String> {
        self.g.apply(c).map(|_| ()).map_err(|e| e.to_string())
    }

    fn node(&mut self, kind: &str, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let spec = registry::spec_for(kind, self.lib)?;
        let mut n = self.g.new_node(&spec);
        n.pos = [col * COL, row * ROW];
        if !label.is_empty() {
            n.label = label.into();
        }
        let id = n.id;
        self.apply(Command::AddNode(Box::new(n)))?;
        Ok(id)
    }

    fn shader(&mut self, file: &str, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        self.node(&registry::shader_kind(file), label, col, row)
    }

    fn set(&mut self, node: NodeId, param: &str, value: ParamValue) -> Result<(), String> {
        // A misspelt setting would silently do nothing: refuse it.
        let n = self.g.node(node).ok_or("no such node")?;
        if n.param(param).is_none() {
            return Err(format!("{} has no setting \"{param}\"", n.kind));
        }
        self.apply(Command::SetParam {
            node,
            param: param.into(),
            value,
        })
    }

    fn num(&mut self, node: NodeId, values: &[(&str, f32)]) -> Result<(), String> {
        for (p, v) in values {
            self.set(node, p, ParamValue::Float(*v))?;
        }
        Ok(())
    }

    fn choice(&mut self, node: NodeId, param: &str, index: u32) -> Result<(), String> {
        self.set(node, param, ParamValue::Choice(index))
    }

    fn wire(&mut self, from: NodeId, to: NodeId, port: &str) -> Result<(), String> {
        self.apply(Command::Connect(Link::new(
            OutputRef::new(from, IMAGE_OUT),
            Endpoint::input(to, port),
        )))
    }

    fn trigger(&mut self, from: NodeId, to: NodeId, port: &str) -> Result<(), String> {
        self.apply(Command::Connect(Link::new(
            OutputRef::new(from, TRIGGER_OUT),
            Endpoint::input(to, port),
        )))
    }

    /// A signal wire into a setting (the modulation rule: base + depth × value).
    fn drive(&mut self, from: NodeId, to: NodeId, param: &str, depth: f32) -> Result<(), String> {
        self.apply(Command::Connect(Link {
            modulation: Some(WireMod {
                shape: Shape::Linear,
                depth,
                enabled: true,
            }),
            ..Link::new(OutputRef::new(from, VALUE_OUT), Endpoint::param(to, param))
        }))
    }

    /// A Switch over `sources`, moving on every `every` beats.
    fn switch(
        &mut self,
        sources: &[NodeId],
        every: f32,
        transition: u32,
        beats: f32,
        col: f32,
        row: f32,
    ) -> Result<NodeId, String> {
        let n = sources.len().clamp(2, 4);
        let sw = self.shader("fx_switch.wgsl", "", col, row)?;
        for (i, s) in sources.iter().take(4).enumerate() {
            self.wire(*s, sw, ["a", "b", "c", "d"][i])?;
        }
        self.choice(sw, "count", n as u32 - 2)?;
        self.choice(sw, "transition", transition)?;
        self.num(sw, &[("beats", beats)])?;
        let beat = self.node(
            BEAT_TRIGGER,
            &format!("Every {every} beats"),
            col - 1.0,
            row - 1.0,
        )?;
        self.num(beat, &[("every", every)])?;
        let count = self.node(COUNTER, "Next picture", col, row - 1.0)?;
        self.num(count, &[("steps", n as f32)])?;
        self.trigger(beat, count, "count")?;
        self.drive(count, sw, "show", 1.0)?;
        Ok(sw)
    }

    /// The shared ending: Glow → Colour Grade → Program Out.
    fn finish(&mut self, from: NodeId, col: f32, row: f32, glow: f32) -> Result<(), String> {
        let gl = self.shader("fx_glow.wgsl", "", col, row)?;
        self.num(
            gl,
            &[
                ("amount", glow),
                ("threshold", 0.55),
                ("radius", 1.6),
                ("kick_boost", 0.5),
            ],
        )?;
        let grade = self.shader("fx_colour.wgsl", "Final look", col + 1.0, row)?;
        self.num(grade, &[("contrast", 1.12), ("saturation", 1.1)])?;
        let out = self.node(PROGRAM_OUT, "", col + 2.0, row)?;
        self.wire(from, gl, "input")?;
        self.wire(gl, grade, "input")?;
        self.wire(grade, out, "image")?;
        self.out = Some(out);
        Ok(())
    }

    fn image(&mut self, file: &str, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let n = self.node(registry::IMAGE, label, col, row)?;
        self.set(n, registry::FILE_PARAM, ParamValue::Text(file.into()))?;
        Ok(n)
    }

    fn video(&mut self, file: &str, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let n = self.node(registry::VIDEO, label, col, row)?;
        self.set(n, registry::FILE_PARAM, ParamValue::Text(file.into()))?;
        Ok(n)
    }

    /// A frame (titled backdrop) around everything.
    fn frame_all(&mut self, title: &str, colour: [u8; 3]) -> Result<(), String> {
        let ps: Vec<[f32; 2]> = self.g.nodes().iter().map(|n| n.pos).collect();
        if ps.is_empty() {
            return Ok(());
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in ps {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let id = self.g.new_frame_id();
        self.apply(Command::PutFrame(Frame {
            id,
            label: format!("Template: {title}"),
            rect: [x0 - 30.0, y0 - 40.0, x1 - x0 + 270.0, y1 - y0 + 260.0],
            colour,
        }))
    }
}

/// A scene to stand in when the user has no media of a kind.
fn stand_in(
    b: &mut Builder,
    file: &str,
    label: &str,
    col: f32,
    row: f32,
) -> Result<NodeId, String> {
    b.shader(file, label, col, row)
}

// ---------------------------------------------------------------- the templates

fn peak_time(b: &mut Builder) -> Result<(), String> {
    let tunnel = b.shader("warehouse_tunnel.wgsl", "", 0.0, 1.0)?;
    let furnace = b.shader("iron_furnace.wgsl", "", 0.0, 2.0)?;
    let bloom = b.shader("kaleido_bloom.wgsl", "", 0.0, 3.0)?;
    let crt = b.shader("crt_glitch.wgsl", "", 0.0, 4.0)?;
    b.num(bloom, &[("kick_punch", 0.8), ("warp", 1.3)])?;
    b.num(tunnel, &[("colour_cycle", 0.4), ("accent_amt", 0.3)])?;
    let sw = b.switch(&[tunnel, furnace, bloom, crt], 8.0, 4, 1.0, 1.5, 2.5)?;
    b.finish(sw, 2.5, 2.5, 0.8)
}

fn warm_up(b: &mut Builder) -> Result<(), String> {
    let window = b.shader("midnight_window.wgsl", "", 0.0, 1.0)?;
    let aurora = b.shader("aurora_drift.wgsl", "", 0.0, 2.0)?;
    let bloom = b.shader("kaleido_bloom.wgsl", "Slow kaleidoscope", 0.0, 3.0)?;
    b.num(
        bloom,
        &[
            ("flow_speed", 0.08),
            ("spin_speed", 0.03),
            ("kick_punch", 0.15),
            ("saturation", 0.6),
        ],
    )?;
    let sw = b.switch(&[window, aurora, bloom], 16.0, 1, 4.0, 1.5, 2.0)?;
    b.finish(sw, 2.5, 2.0, 0.5)
}

fn clip_cuts(b: &mut Builder, media: &DemoMedia) -> Result<(), String> {
    let mut sources = Vec::new();
    let mut row = 1.0;
    let jump = b.node(AUDIO_TRIGGER, "On every bar", -1.0, 0.5)?;
    b.choice(jump, "event", 5)?; // every bar
    for (i, v) in media.videos.iter().take(2).enumerate() {
        let n = b.video(v, &format!("Clip {}", i + 1), 0.0, row)?;
        b.trigger(jump, n, "jump")?;
        sources.push(n);
        row += 1.0;
    }
    for (i, p) in media
        .photos
        .iter()
        .take(4 - sources.len().min(4))
        .enumerate()
    {
        if sources.len() >= 4 {
            break;
        }
        let pic = b.image(p, &format!("Picture {}", i + 1), 0.0, row)?;
        let punch = b.shader("fx_transform.wgsl", "Kick punch", 1.0, row)?;
        b.num(punch, &[("kick_punch", 0.25)])?;
        b.wire(pic, punch, "input")?;
        sources.push(punch);
        row += 1.0;
    }
    // Not enough media: scenes fill the gaps (import clips to replace them).
    let fill = ["crt_glitch.wgsl", "warehouse_tunnel.wgsl"];
    let mut k = 0;
    while sources.len() < 2 {
        sources.push(stand_in(b, fill[k], "Stand-in (import clips)", 0.0, row)?);
        row += 1.0;
        k += 1;
    }
    let sw = b.switch(&sources, 4.0, 0, 0.1, 2.0, 2.0)?;
    let glitch = b.shader("fx_glitch.wgsl", "", 3.0, 2.0)?;
    b.num(
        glitch,
        &[
            ("split", 0.03),
            ("split_kick", 0.7),
            ("tear", 0.15),
            ("noise", 0.05),
        ],
    )?;
    b.wire(sw, glitch, "input")?;
    b.finish(glitch, 4.0, 2.0, 0.5)
}

fn kaleido_clip(b: &mut Builder, media: &DemoMedia) -> Result<(), String> {
    let src = if let Some(v) = media.videos.first() {
        b.video(v, "Clip", 0.0, 1.0)?
    } else if let Some(p) = media.photos.first() {
        b.image(p, "Picture", 0.0, 1.0)?
    } else {
        stand_in(
            b,
            "canopy_breaks.wgsl",
            "Stand-in (import a clip)",
            0.0,
            1.0,
        )?
    };
    let mirror = b.shader("fx_mirror.wgsl", "", 1.0, 1.0)?;
    b.choice(mirror, "mode", 0)?;
    b.num(
        mirror,
        &[("segments", 8.0), ("spin_speed", 0.15), ("zoom", 1.2)],
    )?;
    let trails = b.shader("fx_trails.wgsl", "", 2.0, 1.0)?;
    b.num(
        trails,
        &[("length", 0.82), ("zoom", 0.012), ("twist", 0.004)],
    )?;
    b.wire(src, mirror, "input")?;
    b.wire(mirror, trails, "input")?;
    b.finish(trails, 3.0, 1.0, 0.9)
}

fn photo_pulse(b: &mut Builder, media: &DemoMedia) -> Result<(), String> {
    let ring = b.shader("radial_spectrum.wgsl", "", 0.0, 1.0)?;
    b.num(
        ring,
        &[
            ("radius", 0.42),
            ("colour_spread", 0.6),
            ("colour_drift", 0.15),
        ],
    )?;
    let pic = match media.photos.first() {
        Some(p) => b.image(p, "Picture", 0.0, 2.0)?,
        None => stand_in(
            b,
            "kaleido_bloom.wgsl",
            "Stand-in (import a picture)",
            0.0,
            2.0,
        )?,
    };
    if !media.photos.is_empty() {
        b.set(pic, "fit", ParamValue::Choice(0))?; // show all of it
    }
    let punch = b.shader("fx_transform.wgsl", "Kick punch", 1.0, 2.0)?;
    b.num(punch, &[("zoom", 0.62), ("kick_punch", 0.35)])?;
    let round = b.shader("mask_shape.wgsl", "Round window", 0.0, 3.0)?;
    b.num(round, &[("size", 0.62), ("kick_pulse", 0.25)])?;
    b.wire(pic, punch, "input")?;
    b.wire(round, punch, graph::MASK_PORT)?;
    let mix = b.node(LAYER_MIXER, "Layers", 2.0, 1.5)?;
    b.wire(ring, mix, &registry::layer_port(1))?;
    b.wire(punch, mix, &registry::layer_port(2))?;
    let trails = b.shader("fx_trails.wgsl", "Echoes", 3.0, 1.5)?;
    b.num(trails, &[("length", 0.7), ("zoom", 0.015)])?;
    b.wire(mix, trails, "input")?;
    b.finish(trails, 4.0, 1.5, 0.7)
}

fn logo_stamp(b: &mut Builder, media: &DemoMedia) -> Result<(), String> {
    let tunnel = b.shader("warehouse_tunnel.wgsl", "", 0.0, 1.0)?;
    b.num(tunnel, &[("frame_bright", 0.8), ("colour_cycle", 0.3)])?;
    let logo = match media.photos.first() {
        Some(p) => b.image(p, "Logo", 0.0, 2.0)?,
        None => stand_in(
            b,
            "radial_spectrum.wgsl",
            "Stand-in (import a logo)",
            0.0,
            2.0,
        )?,
    };
    if !media.photos.is_empty() {
        b.set(logo, "fit", ParamValue::Choice(0))?;
    }
    let key = b.shader("mask_luma_key.wgsl", "Drop the black", 1.0, 2.0)?;
    b.num(key, &[("threshold", 0.15), ("softness", 0.2)])?;
    let size = b.shader("fx_transform.wgsl", "Size + punch", 2.0, 2.0)?;
    b.num(size, &[("zoom", 0.55), ("kick_punch", 0.2)])?;
    b.wire(logo, key, "input")?;
    b.wire(key, size, "input")?;
    let mix = b.node(LAYER_MIXER, "Layers", 3.0, 1.5)?;
    b.wire(tunnel, mix, &registry::layer_port(1))?;
    b.wire(size, mix, &registry::layer_port(2))?;
    b.set(
        mix,
        &registry::layer_param("mode", 2),
        ParamValue::Choice(scenes::blend::mode_index("Screen").unwrap_or(0) as u32),
    )?;
    b.num(mix, &[(&registry::layer_param("opacity", 2), 0.35)])?;
    let kick = b.node(AUDIO_TRIGGER, "Kick", 1.0, 3.0)?;
    b.choice(kick, "event", 0)?;
    let env = b.node(ENVELOPE, "Glow in, fade out", 2.0, 3.0)?;
    b.num(env, &[("attack", 0.02), ("decay", 0.6)])?;
    b.trigger(kick, env, "trigger")?;
    b.drive(env, mix, &registry::layer_param("opacity", 2), 0.65)?;
    b.finish(mix, 4.0, 1.5, 0.7)
}

fn glitch_drop(b: &mut Builder) -> Result<(), String> {
    let ball = b.shader("velvet_mirrorball.wgsl", "", 0.0, 1.0)?;
    let crt = b.shader("crt_glitch.wgsl", "", 0.0, 2.0)?;
    let sw = b.switch(&[ball, crt], 32.0, 2, 2.0, 1.5, 1.5)?;
    // Also move on every section change.
    let section = b.node(AUDIO_TRIGGER, "Section change", 0.5, 0.0)?;
    b.choice(section, "event", 8)?;
    let counter =
        b.g.nodes()
            .iter()
            .find(|n| n.kind == COUNTER)
            .map(|n| n.id)
            .ok_or("no counter")?;
    b.trigger(section, counter, "count")?;
    let glitch = b.shader("fx_glitch.wgsl", "", 2.5, 1.5)?;
    b.num(
        glitch,
        &[("split", 0.02), ("split_kick", 0.4), ("tear", 0.0)],
    )?;
    let drop = b.node(AUDIO_TRIGGER, "Drop", 1.5, 2.5)?;
    b.choice(drop, "event", 7)?;
    let burst = b.node(ENVELOPE, "Burst (2 s)", 2.5, 2.5)?;
    b.num(burst, &[("attack", 0.01), ("decay", 2.0)])?;
    b.trigger(drop, burst, "trigger")?;
    b.drive(burst, glitch, "tear", 0.8)?;
    b.drive(burst, glitch, "split", 0.5)?;
    b.wire(sw, glitch, "input")?;
    b.finish(glitch, 3.5, 1.5, 0.8)
}

fn webcam_mirror(b: &mut Builder, media: &DemoMedia) -> Result<(), String> {
    let bloom = b.shader("kaleido_bloom.wgsl", "", 0.0, 1.0)?;
    b.num(bloom, &[("saturation", 0.7), ("flow_speed", 0.15)])?;
    let cam = match &media.camera {
        Some(name) => {
            let c = b.node(registry::CAMERA, "Webcam", 0.0, 2.0)?;
            b.set(c, registry::CAMERA_PARAM, ParamValue::Text(name.clone()))?;
            c
        }
        None => stand_in(
            b,
            "iron_furnace.wgsl",
            "Stand-in (plug in a camera)",
            0.0,
            2.0,
        )?,
    };
    let mirror = b.shader("fx_mirror.wgsl", "", 1.0, 2.0)?;
    b.choice(mirror, "mode", 3)?; // four ways
    let trails = b.shader("fx_trails.wgsl", "", 2.0, 2.0)?;
    b.num(trails, &[("length", 0.88), ("zoom", 0.006)])?;
    b.wire(cam, mirror, "input")?;
    b.wire(mirror, trails, "input")?;
    let mix = b.node(LAYER_MIXER, "Layers", 3.0, 1.5)?;
    b.wire(bloom, mix, &registry::layer_port(1))?;
    b.wire(trails, mix, &registry::layer_port(2))?;
    b.set(
        mix,
        &registry::layer_param("mode", 2),
        ParamValue::Choice(scenes::blend::mode_index("Screen").unwrap_or(0) as u32),
    )?;
    b.num(mix, &[(&registry::layer_param("opacity", 2), 0.85)])?;
    b.finish(mix, 4.0, 1.5, 0.7)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(n_video: usize, n_photo: usize, cam: bool) -> DemoMedia {
        DemoMedia {
            videos: (0..n_video).map(|i| format!("v{i}.mp4")).collect(),
            photos: (0..n_photo).map(|i| format!("p{i}.png")).collect(),
            camera: cam.then(|| "Cam".to_string()),
        }
    }

    /// Every template builds, with and without media, has exactly one output
    /// that something feeds, and has a frame with its name.
    #[test]
    fn every_template_builds_with_or_without_media() {
        let lib = SceneLibrary::new(None);
        for m in [media(0, 0, false), media(2, 3, true), media(1, 0, false)] {
            for t in ALL {
                let (g, out) = build(t.id, &lib, &m).unwrap_or_else(|e| panic!("{}: {e}", t.id));
                assert_eq!(
                    g.nodes().iter().filter(|n| n.kind == PROGRAM_OUT).count(),
                    1,
                    "{}",
                    t.id
                );
                assert!(g.input_link(out, "image").is_some(), "{}", t.id);
                assert!(g.frames().iter().any(|f| f.label.contains(t.name)));
                assert!(!t.note.is_empty());
            }
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = ALL.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ALL.len());
    }
}
