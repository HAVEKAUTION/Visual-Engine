//! Graph templates: ready-made node layouts that look like a VJ set behind a
//! DJ, inserted from the graph's right-click menu (Templates). Each one is an
//! ordinary graph, so every part can be opened up and changed, and each has a
//! note saying what it does and what to try.
//!
//! What makes them feel like a set rather than a loop:
//! - pictures **shape each other** (a scene bends a photo through Displace, a
//!   kaleidoscope pours an aurora along its lines) instead of just taking
//!   turns;
//! - changes come from the **music's structure** (phrases, section changes,
//!   drops), in a **random order** with a **different transition** each time;
//! - your pictures are **recognised**: busy ones become backdrops, ones with a
//!   plain or see-through background become subjects that are cut out and
//!   placed on top (see `media::picture_role`);
//! - a shared finish (Glow + a gentle Colour Grade) keeps one tone.

use graph::{Command, Endpoint, Frame, Graph, Link, NodeId, OutputRef, ParamValue, WireMod};
use media::PictureRole;
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
        id: "woven",
        name: "Your media, woven together",
        note: "Your pictures and videos sorted by what they are: busy ones become the \
               backdrop, flowing along the lines of a music-driven kaleidoscope; ones with \
               a plain or see-through background are cut out and float on top, rippling on \
               the kick. Light from the music bleeds through. Changes come on phrases and \
               section changes, in a random order.",
        uses: "up to 2 videos and 4 pictures",
    },
    Template {
        id: "peak_time",
        name: "Peak time",
        note: "Tunnel, furnace, fractal and jungle, in a random order with a different \
               transition each time. They change on phrases, section changes and drops, not \
               on a fixed clock, and the drop blooms with light.",
        uses: "",
    },
    Template {
        id: "warm_up",
        name: "Warm-up",
        note: "One evolving picture instead of scenes taking turns: an aurora poured along \
               the shapes of a slow kaleidoscope, trailing and swaying with the bass. For \
               the start of the night.",
        uses: "",
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
               echoes. A picture with a plain background is cut out automatically.",
        uses: "your first picture",
    },
    Template {
        id: "logo_stamp",
        name: "Logo on the kick",
        note: "Your logo, cut out from its background automatically, glowing in over a \
               light tunnel on every kick and rippling like liquid.",
        uses: "your first picture with a plain background",
    },
    Template {
        id: "glitch_drop",
        name: "Glitch on the drop",
        note: "Tunnel and jungle change on every section change; the drop sends a burst \
               of glitch and colour split through everything.",
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

/// A built template.
pub struct Built {
    pub graph: Graph,
    pub out: NodeId,
    /// What it found in your media, for the message after adding it.
    pub summary: String,
}

/// Build a template into a fresh graph (pasted into the user's graph by the
/// editor).
pub fn build(id: &str, library: &SceneLibrary, media: &DemoMedia) -> Result<Built, String> {
    let t = find(id).ok_or_else(|| format!("No template called {id}"))?;
    let mut b = Builder::new(library);
    let summary = match id {
        "woven" => woven(&mut b, media)?,
        "peak_time" => peak_time(&mut b)?,
        "warm_up" => warm_up(&mut b)?,
        "kaleido_clip" => kaleido_clip(&mut b, media)?,
        "photo_pulse" => photo_pulse(&mut b, media)?,
        "logo_stamp" => logo_stamp(&mut b, media)?,
        "glitch_drop" => glitch_drop(&mut b)?,
        "webcam_mirror" => webcam_mirror(&mut b, media)?,
        _ => return Err(format!("No template called {id}")),
    };
    b.frame_all(t.name, [240, 160, 80])?;
    let out = b.out.ok_or("template has no output")?;
    Ok(Built {
        graph: b.g,
        out,
        summary,
    })
}

/// Your pictures sorted into backdrops and subjects (unreadable files count
/// as backdrops).
pub fn sort_pictures(photos: &[String]) -> (Vec<String>, Vec<String>) {
    let mut backdrops = Vec::new();
    let mut subjects = Vec::new();
    for p in photos {
        match media::picture_role(std::path::Path::new(p)) {
            Ok(PictureRole::Subject) => subjects.push(p.clone()),
            _ => backdrops.push(p.clone()),
        }
    }
    (backdrops, subjects)
}

// ---------------------------------------------------------------- builder

const COL: f32 = 250.0;
const ROW: f32 = 240.0;

struct Builder<'a> {
    g: Graph,
    lib: &'a SceneLibrary,
    out: Option<NodeId>,
}

/// When a Switch moves on.
struct Changes {
    /// Every this many beats (0 = not on a clock).
    every: f32,
    /// Also on these audio events (indexes into `registry::TRIGGER_EVENTS`).
    events: &'static [(u32, &'static str)],
    random: bool,
    /// Index into the Switch's transitions (6 = a different one each time).
    transition: u32,
    beats: f32,
}

const SECTION: (u32, &str) = (8, "Section change");
const DROP: (u32, &str) = (7, "Drop");
const MIX_IT_UP: u32 = 6;

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

    fn event(&mut self, index: u32, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let n = self.node(AUDIO_TRIGGER, label, col, row)?;
        self.choice(n, "event", index)?;
        Ok(n)
    }

    /// A Switch over `sources` (2 to 4), moving on as `when` says.
    fn switch(
        &mut self,
        sources: &[NodeId],
        when: &Changes,
        col: f32,
        row: f32,
    ) -> Result<NodeId, String> {
        let n = sources.len().clamp(2, 4);
        let sw = self.shader("fx_switch.wgsl", "", col, row)?;
        for (i, s) in sources.iter().take(4).enumerate() {
            self.wire(*s, sw, ["a", "b", "c", "d"][i])?;
        }
        self.choice(sw, "count", n as u32 - 2)?;
        self.choice(sw, "order", u32::from(when.random))?;
        self.choice(sw, "transition", when.transition)?;
        self.num(sw, &[("beats", when.beats)])?;
        let count = self.node(COUNTER, "Next picture", col, row - 1.0)?;
        self.num(count, &[("steps", n as f32)])?;
        if when.every > 0.0 {
            let label = format!("Every {} bars", when.every / 4.0);
            let beat = self.node(BEAT_TRIGGER, &label, col - 1.0, row - 1.5)?;
            self.num(beat, &[("every", when.every)])?;
            self.trigger(beat, count, "count")?;
        }
        for (k, (index, label)) in when.events.iter().enumerate() {
            let e = self.event(*index, label, col - 2.0, row - 1.5 + k as f32 * 0.6)?;
            self.trigger(e, count, "count")?;
        }
        self.drive(count, sw, "show", 1.0)?;
        Ok(sw)
    }

    /// The shared ending: Glow → Colour Grade → Program Out. Returns the Glow.
    fn finish(&mut self, from: NodeId, col: f32, row: f32, glow: f32) -> Result<NodeId, String> {
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
        Ok(gl)
    }

    /// The drop blooms: a burst of extra glow for a couple of seconds.
    fn drop_bloom(&mut self, glow: NodeId, col: f32, row: f32) -> Result<(), String> {
        let drop = self.event(DROP.0, "Drop", col, row)?;
        let burst = self.node(ENVELOPE, "Bloom (2 s)", col + 1.0, row)?;
        self.num(burst, &[("attack", 0.02), ("decay", 2.0)])?;
        self.trigger(drop, burst, "trigger")?;
        self.drive(burst, glow, "amount", 0.5)
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

    /// Picture → Auto Cut-out → Liquid: a subject that floats and ripples.
    fn subject(&mut self, file: &str, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let pic = self.image(file, label, col, row)?;
        self.choice(pic, "fit", 0)?; // show all of it
        let cut = self.shader("mask_auto_cutout.wgsl", "", col + 1.0, row)?;
        let liquid = self.shader("fx_liquid.wgsl", "", col + 2.0, row)?;
        self.wire(pic, cut, "input")?;
        self.wire(cut, liquid, "input")?;
        Ok(liquid)
    }

    fn mixer_layer(
        &mut self,
        mix: NodeId,
        from: NodeId,
        i: usize,
        mode: &str,
        opacity: f32,
    ) -> Result<(), String> {
        self.wire(from, mix, &registry::layer_port(i))?;
        let m = scenes::blend::mode_index(mode).ok_or("no such blend mode")? as u32;
        self.set(
            mix,
            &registry::layer_param("mode", i),
            ParamValue::Choice(m),
        )?;
        self.num(mix, &[(&registry::layer_param("opacity", i), opacity)])
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

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// ---------------------------------------------------------------- the templates

fn woven(b: &mut Builder, media: &DemoMedia) -> Result<String, String> {
    let (backdrop_pics, subject_pics) = sort_pictures(&media.photos);

    // Backdrops: videos first, then busy pictures (up to 4), changing on
    // phrases and section changes in a random order.
    let mut backdrops = Vec::new();
    let mut row = 1.0;
    let videos = media.videos.len().min(2);
    for (i, v) in media.videos.iter().take(2).enumerate() {
        backdrops.push(b.video(v, &format!("Backdrop video {}", i + 1), 0.0, row)?);
        row += 1.0;
    }
    for (i, p) in backdrop_pics.iter().take(4 - videos).enumerate() {
        backdrops.push(b.image(p, &format!("Backdrop {}", i + 1), 0.0, row)?);
        row += 1.0;
    }
    let found_backdrops = backdrops.len();
    if backdrops.is_empty() {
        let label = "Stand-in backdrop (import pictures)";
        backdrops.push(b.shader("aurora_drift.wgsl", label, 0.0, row)?);
        row += 1.0;
    }
    let backdrop = if backdrops.len() >= 2 {
        let when = Changes {
            every: 32.0,
            events: &[SECTION],
            random: true,
            transition: 1,
            beats: 4.0,
        };
        b.switch(&backdrops, &when, 2.0, 2.0)?
    } else {
        backdrops[0]
    };
    // The music shapes the backdrop: it pours along a kaleidoscope's lines.
    let shape = b.shader("kaleido_bloom.wgsl", "Shape from the music", 2.0, row)?;
    b.num(
        shape,
        &[
            ("flow_speed", 0.15),
            ("saturation", 0.5),
            ("kick_punch", 0.4),
        ],
    )?;
    let bend = b.shader("fx_displace.wgsl", "Backdrop follows the music", 3.0, 2.0)?;
    b.num(
        bend,
        &[("amount", 0.05), ("kick_boost", 1.2), ("tint", 0.25)],
    )?;
    b.wire(backdrop, bend, "input")?;
    b.wire(shape, bend, "shape")?;

    // Light from the music bleeding through.
    let light = b.shader("warehouse_tunnel.wgsl", "Light", 2.0, row + 1.0)?;
    b.num(light, &[("frame_bright", 0.7), ("colour_cycle", 0.3)])?;

    let mix = b.node(LAYER_MIXER, "Layers", 5.0, 2.5)?;
    b.mixer_layer(mix, bend, 1, "Normal", 1.0)?;
    b.mixer_layer(mix, light, 2, "Screen", 0.35)?;

    // Subjects: cut out, floating and rippling on top, changing every 4 bars.
    let mut subjects = Vec::new();
    let mut srow = row + 2.0;
    for (i, p) in subject_pics.iter().take(4).enumerate() {
        subjects.push(b.subject(p, &format!("Subject {}", i + 1), 0.0, srow)?);
        srow += 1.0;
    }
    if !subjects.is_empty() {
        let subject = if subjects.len() >= 2 {
            let when = Changes {
                every: 16.0,
                events: &[],
                random: true,
                transition: MIX_IT_UP,
                beats: 1.0,
            };
            b.switch(&subjects, &when, 3.5, srow)?
        } else {
            subjects[0]
        };
        let place = b.shader("fx_transform.wgsl", "Size + kick punch", 4.0, srow + 1.0)?;
        b.num(place, &[("zoom", 0.7), ("kick_punch", 0.18)])?;
        b.choice(place, "edges", 0)?; // nothing around it, not mirrored copies
        b.wire(subject, place, "input")?;
        b.mixer_layer(mix, place, 3, "Normal", 1.0)?;
    }
    let glow = b.finish(mix, 6.0, 2.5, 0.6)?;
    b.drop_bloom(glow, 6.0, 3.5)?;

    let mut found = Vec::new();
    if videos > 0 {
        found.push(plural(videos, "video", "videos"));
    }
    if found_backdrops > videos {
        let n = found_backdrops - videos;
        found.push(plural(n, "backdrop picture", "backdrop pictures"));
    }
    if !subjects.is_empty() {
        let n = subjects.len();
        found.push(plural(n, "subject to cut out", "subjects to cut out"));
    }
    Ok(if found.is_empty() {
        "No pictures or videos yet: import some in the Media panel and add it again.".into()
    } else {
        format!("Found {}.", found.join(", "))
    })
}

fn peak_time(b: &mut Builder) -> Result<String, String> {
    let tunnel = b.shader("warehouse_tunnel.wgsl", "", 0.0, 1.0)?;
    let furnace = b.shader("iron_furnace.wgsl", "", 0.0, 2.0)?;
    let bloom = b.shader("fractal_bloom.wgsl", "", 0.0, 3.0)?;
    let canopy = b.shader("canopy_breaks.wgsl", "", 0.0, 4.0)?;
    b.num(bloom, &[("edge_flash", 1.2), ("dive_speed", 0.08)])?;
    b.num(tunnel, &[("colour_cycle", 0.4), ("accent_amt", 0.3)])?;
    let when = Changes {
        every: 64.0,
        events: &[SECTION, DROP],
        random: true,
        transition: MIX_IT_UP,
        beats: 1.0,
    };
    let sw = b.switch(&[tunnel, furnace, bloom, canopy], &when, 2.0, 2.5)?;
    let liquid = b.shader("fx_liquid.wgsl", "Kick ripple", 3.0, 2.5)?;
    b.num(liquid, &[("wobble", 0.004), ("ripple", 0.02)])?;
    b.wire(sw, liquid, "input")?;
    let glow = b.finish(liquid, 4.0, 2.5, 0.8)?;
    b.drop_bloom(glow, 4.0, 3.5)?;
    Ok(String::new())
}

fn warm_up(b: &mut Builder) -> Result<String, String> {
    let aurora = b.shader("aurora_drift.wgsl", "", 0.0, 1.0)?;
    let bloom = b.shader(
        "kaleido_bloom.wgsl",
        "Slow kaleidoscope (the shape)",
        0.0,
        2.0,
    )?;
    b.num(
        bloom,
        &[
            ("flow_speed", 0.06),
            ("spin_speed", 0.02),
            ("kick_punch", 0.1),
            ("saturation", 0.4),
        ],
    )?;
    let pour = b.shader("fx_displace.wgsl", "Aurora poured along it", 1.0, 1.5)?;
    b.num(
        pour,
        &[
            ("amount", 0.09),
            ("kick_boost", 0.3),
            ("bass_boost", 0.8),
            ("tint", 0.35),
        ],
    )?;
    b.wire(aurora, pour, "input")?;
    b.wire(bloom, pour, "shape")?;
    let trails = b.shader("fx_trails.wgsl", "", 2.0, 1.5)?;
    b.num(
        trails,
        &[("length", 0.86), ("zoom", 0.004), ("twist", 0.002)],
    )?;
    let sway = b.shader("fx_liquid.wgsl", "Sway with the bass", 3.0, 1.5)?;
    b.num(
        sway,
        &[("wobble", 0.01), ("ripple", 0.0), ("churn_speed", 0.2)],
    )?;
    b.wire(pour, trails, "input")?;
    b.wire(trails, sway, "input")?;
    b.finish(sway, 4.0, 1.5, 0.5)?;
    Ok(String::new())
}

fn kaleido_clip(b: &mut Builder, media: &DemoMedia) -> Result<String, String> {
    let src = if let Some(v) = media.videos.first() {
        b.video(v, "Clip", 0.0, 1.0)?
    } else if let Some(p) = media.photos.first() {
        b.image(p, "Picture", 0.0, 1.0)?
    } else {
        b.shader("canopy_breaks.wgsl", "Stand-in (import a clip)", 0.0, 1.0)?
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
    b.finish(trails, 3.0, 1.0, 0.9)?;
    Ok(String::new())
}

fn photo_pulse(b: &mut Builder, media: &DemoMedia) -> Result<String, String> {
    let ring = b.shader("radial_spectrum.wgsl", "", 0.0, 1.0)?;
    b.num(
        ring,
        &[
            ("radius", 0.42),
            ("colour_spread", 0.6),
            ("colour_drift", 0.15),
        ],
    )?;
    let (_, subjects) = sort_pictures(&media.photos);
    let (pic, note) = if let Some(p) = subjects.first() {
        let pic = b.subject(p, "Picture", 0.0, 2.0)?;
        (pic, "Cut out its plain background.")
    } else if let Some(p) = media.photos.first() {
        let pic = b.image(p, "Picture", 0.0, 2.0)?;
        b.choice(pic, "fit", 0)?;
        let round = b.shader("mask_shape.wgsl", "Round window", 0.0, 3.0)?;
        b.num(round, &[("size", 0.62), ("kick_pulse", 0.25)])?;
        b.wire(round, pic, graph::MASK_PORT)?;
        (pic, "A busy picture: shown in a round window.")
    } else {
        let label = "Stand-in (import a picture)";
        (b.shader("kaleido_bloom.wgsl", label, 0.0, 2.0)?, "")
    };
    let punch = b.shader("fx_transform.wgsl", "Kick punch", 3.0, 2.0)?;
    b.num(punch, &[("zoom", 0.62), ("kick_punch", 0.35)])?;
    b.choice(punch, "edges", 0)?; // nothing around it, not mirrored copies
    b.wire(pic, punch, "input")?;
    let mix = b.node(LAYER_MIXER, "Layers", 4.0, 1.5)?;
    b.mixer_layer(mix, ring, 1, "Normal", 1.0)?;
    b.mixer_layer(mix, punch, 2, "Normal", 1.0)?;
    let trails = b.shader("fx_trails.wgsl", "Echoes", 5.0, 1.5)?;
    b.num(trails, &[("length", 0.7), ("zoom", 0.015)])?;
    b.wire(mix, trails, "input")?;
    b.finish(trails, 6.0, 1.5, 0.7)?;
    Ok(note.into())
}

fn logo_stamp(b: &mut Builder, media: &DemoMedia) -> Result<String, String> {
    let tunnel = b.shader("warehouse_tunnel.wgsl", "", 0.0, 1.0)?;
    b.num(tunnel, &[("frame_bright", 0.8), ("colour_cycle", 0.3)])?;
    let (_, subjects) = sort_pictures(&media.photos);
    let file = subjects.first().or(media.photos.first());
    let logo = match file {
        Some(p) => b.subject(p, "Logo", 0.0, 2.0)?,
        None => b.shader("radial_spectrum.wgsl", "Stand-in (import a logo)", 0.0, 2.0)?,
    };
    let size = b.shader("fx_transform.wgsl", "Size + punch", 3.0, 2.0)?;
    b.num(size, &[("zoom", 0.55), ("kick_punch", 0.2)])?;
    b.choice(size, "edges", 0)?; // nothing around it, not mirrored copies
    b.wire(logo, size, "input")?;
    let mix = b.node(LAYER_MIXER, "Layers", 4.0, 1.5)?;
    b.mixer_layer(mix, tunnel, 1, "Normal", 1.0)?;
    b.mixer_layer(mix, size, 2, "Normal", 0.35)?;
    let kick = b.event(0, "Kick", 2.0, 3.0)?;
    let env = b.node(ENVELOPE, "Glow in, fade out", 3.0, 3.0)?;
    b.num(env, &[("attack", 0.02), ("decay", 0.6)])?;
    b.trigger(kick, env, "trigger")?;
    b.drive(env, mix, &registry::layer_param("opacity", 2), 0.65)?;
    b.finish(mix, 5.0, 1.5, 0.7)?;
    Ok(if file.is_some() && subjects.is_empty() {
        "That picture has a busy background: a logo on a plain or see-through background \
         works best."
            .into()
    } else {
        String::new()
    })
}

fn glitch_drop(b: &mut Builder) -> Result<String, String> {
    let tunnel = b.shader("warehouse_tunnel.wgsl", "", 0.0, 1.0)?;
    let canopy = b.shader("canopy_breaks.wgsl", "", 0.0, 2.0)?;
    let when = Changes {
        every: 0.0,
        events: &[SECTION],
        random: false,
        transition: 2,
        beats: 2.0,
    };
    let sw = b.switch(&[tunnel, canopy], &when, 2.0, 1.5)?;
    let glitch = b.shader("fx_glitch.wgsl", "", 3.0, 1.5)?;
    b.num(
        glitch,
        &[("split", 0.02), ("split_kick", 0.4), ("tear", 0.0)],
    )?;
    let drop = b.event(DROP.0, "Drop", 2.0, 2.5)?;
    let burst = b.node(ENVELOPE, "Burst (2 s)", 3.0, 2.5)?;
    b.num(burst, &[("attack", 0.01), ("decay", 2.0)])?;
    b.trigger(drop, burst, "trigger")?;
    b.drive(burst, glitch, "tear", 0.8)?;
    b.drive(burst, glitch, "split", 0.5)?;
    b.wire(sw, glitch, "input")?;
    b.finish(glitch, 4.0, 1.5, 0.8)?;
    Ok(String::new())
}

fn webcam_mirror(b: &mut Builder, media: &DemoMedia) -> Result<String, String> {
    let bloom = b.shader("kaleido_bloom.wgsl", "", 0.0, 1.0)?;
    b.num(bloom, &[("saturation", 0.7), ("flow_speed", 0.15)])?;
    let cam = match &media.camera {
        Some(name) => {
            let c = b.node(registry::CAMERA, "Webcam", 0.0, 2.0)?;
            b.set(c, registry::CAMERA_PARAM, ParamValue::Text(name.clone()))?;
            c
        }
        None => b.shader("iron_furnace.wgsl", "Stand-in (plug in a camera)", 0.0, 2.0)?,
    };
    let mirror = b.shader("fx_mirror.wgsl", "", 1.0, 2.0)?;
    b.choice(mirror, "mode", 3)?; // four ways
    let trails = b.shader("fx_trails.wgsl", "", 2.0, 2.0)?;
    b.num(trails, &[("length", 0.88), ("zoom", 0.006)])?;
    b.wire(cam, mirror, "input")?;
    b.wire(mirror, trails, "input")?;
    let mix = b.node(LAYER_MIXER, "Layers", 3.0, 1.5)?;
    b.mixer_layer(mix, bloom, 1, "Normal", 1.0)?;
    b.mixer_layer(mix, trails, 2, "Screen", 0.85)?;
    b.finish(mix, 4.0, 1.5, 0.7)?;
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(n_video: usize, photos: Vec<String>, cam: bool) -> DemoMedia {
        DemoMedia {
            videos: (0..n_video).map(|i| format!("v{i}.mp4")).collect(),
            photos,
            camera: cam.then(|| "Cam".to_string()),
        }
    }

    fn fake(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("p{i}.png")).collect()
    }

    /// Every template builds with no media, lots of media, and some, has
    /// exactly one output that something feeds, and a frame with its name.
    #[test]
    fn every_template_builds_with_or_without_media() {
        let lib = SceneLibrary::new(None);
        for m in [
            media(0, vec![], false),
            media(2, fake(4), true),
            media(1, vec![], false),
        ] {
            for t in ALL {
                let built = build(t.id, &lib, &m).unwrap_or_else(|e| panic!("{}: {e}", t.id));
                let g = &built.graph;
                let outs = g.nodes().iter().filter(|n| n.kind == PROGRAM_OUT).count();
                assert_eq!(outs, 1, "{}", t.id);
                assert!(g.input_link(built.out, "image").is_some(), "{}", t.id);
                assert!(g.frames().iter().any(|f| f.label.contains(t.name)));
                assert!(!t.note.is_empty());
                // The CRT scene is out of the templates (the user's call).
                let crt = g.nodes().iter().any(|n| n.kind.contains("crt_glitch"));
                assert!(!crt, "{}", t.id);
            }
        }
    }

    /// Real pictures are recognised: a logo on white becomes a cut-out
    /// subject, a busy picture a backdrop.
    #[test]
    fn woven_sorts_backdrops_and_subjects() {
        let dir = std::env::temp_dir().join(format!("ve_woven_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let logo = dir.join("logo.png");
        image::RgbaImage::from_fn(64, 64, |x, y| {
            if (20..44).contains(&x) && (20..44).contains(&y) {
                image::Rgba([200, 20, 40, 255])
            } else {
                image::Rgba([250, 250, 250, 255])
            }
        })
        .save(&logo)
        .unwrap();
        let busy = dir.join("busy.png");
        image::RgbaImage::from_fn(64, 64, |x, y| {
            let r = (x * 37 % 256) as u8;
            let g = (y * 53 % 256) as u8;
            let b = ((x * y) % 256) as u8;
            image::Rgba([r, g, b, 255])
        })
        .save(&busy)
        .unwrap();
        let photos = vec![
            logo.to_string_lossy().into_owned(),
            busy.to_string_lossy().into_owned(),
        ];
        let (backdrops, subjects) = sort_pictures(&photos);
        assert_eq!(subjects, vec![photos[0].clone()]);
        assert_eq!(backdrops, vec![photos[1].clone()]);

        let lib = SceneLibrary::new(None);
        let built = build("woven", &lib, &media(0, photos, false)).unwrap();
        assert!(built.summary.contains("1 subject"), "{}", built.summary);
        let cut = built
            .graph
            .nodes()
            .iter()
            .any(|n| n.kind.contains("mask_auto_cutout"));
        assert!(cut, "the subject is cut out");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = ALL.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ALL.len());
    }
}
