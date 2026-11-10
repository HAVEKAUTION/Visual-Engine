//! Signature Looks: a few hand-made sets, each built around one idea, with a
//! character and a temperament (what it listens to, how it reacts). Loading
//! a Look replaces the graph with it; everything in it is ordinary nodes.
//!
//! Each Look marks some of its settings as **genes** (a setting and the
//! range it may move in). Look DNA (mutate, breed) works on those.

use graph::{Graph, NodeId, ParamValue};
use nodes::build::{Builder, Changes, DROP, SECTION};
use nodes::demo::DemoMedia;
use nodes::registry::{BEAT_TRIGGER, ENVELOPE};
use nodes::templates::sort_pictures;
use scenes::SceneLibrary;

/// One Look in the Looks panel.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub id: &'static str,
    pub name: &'static str,
    /// The idea it shows off.
    pub idea: &'static str,
    /// One line: what it looks like.
    pub character: &'static str,
    /// What it listens to and how it reacts.
    pub temperament: &'static str,
    /// What of your media it uses.
    pub uses: &'static str,
}

pub const ALL: &[Look] = &[
    Look {
        id: "afterimage",
        name: "Afterimage",
        idea: "Feedback",
        character: "Whatever moves leaves a glowing ghost in the opposite colour, swirling slowly \
                    outwards.",
        temperament: "Patient: follows the slow energy. Ghosts lurch on the beat and flare on \
                      the kick; the drop wipes the slate clean.",
        uses: "Your videos, or your pictures as spinning cards (stand-in: the spectrum ring).",
    },
    Look {
        id: "datamosh",
        name: "Datamosh",
        idea: "Glitch",
        character: "Broken video: every cut melts into the next, the old picture dragged around \
                    in blocks by the new one's movement.",
        temperament: "Twitchy: lives on snares and section changes. Cuts every 4 bars; the drop \
                      smears everything, then it heals block by block.",
        uses: "Up to 4 of your videos and busy pictures, topped up with scenes.",
    },
    Look {
        id: "fractal",
        name: "Fractal Bloom",
        idea: "Geometry",
        character: "Round bubbling bulbs with sharp glittering edges, diving in and out; fractal \
                    flowers when the song changes section.",
        temperament: "Hypnotic: breathes with the bass, glitters on the hi-hats, edges flash on \
                      the kick and the drop blooms with light.",
        uses: "Nothing: it's all generated.",
    },
];

pub fn find(id: &str) -> Option<&'static Look> {
    ALL.iter().find(|l| l.id == id)
}

/// A setting a Look lets Look DNA change, and the range it may move in.
#[derive(Clone, Debug, PartialEq)]
pub struct Gene {
    pub node: NodeId,
    pub param: String,
    pub min: f32,
    pub max: f32,
}

/// A built Look.
pub struct BuiltLook {
    pub graph: Graph,
    pub out: NodeId,
    pub genes: Vec<Gene>,
    /// What it found in your media ("" = nothing to say).
    pub summary: String,
}

/// Build a Look into a fresh graph.
pub fn build(id: &str, library: &SceneLibrary, media: &DemoMedia) -> Result<BuiltLook, String> {
    let look = find(id).ok_or_else(|| format!("No Look called {id}"))?;
    let mut l = LookBuilder {
        b: Builder::new(library),
        genes: Vec::new(),
    };
    let summary = match id {
        "afterimage" => afterimage(&mut l, media)?,
        "datamosh" => datamosh(&mut l, media)?,
        "fractal" => fractal(&mut l)?,
        _ => return Err(format!("No Look called {id}")),
    };
    l.b.frame_all(&format!("Look: {}", look.name), [150, 110, 230])?;
    let out = l.b.out.ok_or("Look has no output")?;
    Ok(BuiltLook {
        graph: l.b.g,
        out,
        genes: l.genes,
        summary,
    })
}

struct LookBuilder<'a> {
    b: Builder<'a>,
    genes: Vec<Gene>,
}

impl LookBuilder<'_> {
    /// Mark `param` of `node` as a gene moving in `min..=max` (checked
    /// against the setting's own range).
    fn gene(&mut self, node: NodeId, param: &str, min: f32, max: f32) -> Result<(), String> {
        let n = self.b.g.node(node).ok_or("no such node")?;
        let p = n
            .param(param)
            .ok_or_else(|| format!("{} has no setting \"{param}\"", n.kind))?;
        if !matches!(p.value, ParamValue::Float(_)) {
            return Err(format!("gene {param} isn't a number"));
        }
        let (lo, hi) = (p.def.min, p.def.max);
        if lo < hi && (min < lo || max > hi || min >= max) {
            return Err(format!("gene {param}: {min}..{max} is outside {lo}..{hi}"));
        }
        self.genes.push(Gene {
            node,
            param: param.into(),
            min,
            max,
        });
        Ok(())
    }

    fn genes(&mut self, node: NodeId, list: &[(&str, f32, f32)]) -> Result<(), String> {
        for (p, lo, hi) in list {
            self.gene(node, p, *lo, *hi)?;
        }
        Ok(())
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// ---------------------------------------------------------------- the Looks

/// Feedback: a moving source leaves complementary ghosts.
fn afterimage(l: &mut LookBuilder, media: &DemoMedia) -> Result<String, String> {
    let (backdrops, subjects) = sort_pictures(&media.photos);
    let b = &mut l.b;
    let mut sources = Vec::new();
    let mut row = 1.0;
    let summary = if !media.videos.is_empty() {
        for (i, v) in media.videos.iter().take(3).enumerate() {
            sources.push(b.video(v, &format!("Video {}", i + 1), 0.0, row)?);
            row += 1.0;
        }
        format!("Using {}.", plural(sources.len(), "video", "videos"))
    } else if !subjects.is_empty() || !backdrops.is_empty() {
        // Cut-outs first (they leave the best ghosts), then busy pictures.
        for (i, p) in subjects.iter().take(3).enumerate() {
            sources.push(b.subject(p, &format!("Subject {}", i + 1), 0.0, row)?);
            row += 1.0;
        }
        for (i, p) in backdrops.iter().take(3 - sources.len().min(3)).enumerate() {
            let pic = b.image(p, &format!("Picture {}", i + 1), 0.0, row)?;
            b.choice(pic, "fit", 0)?;
            sources.push(pic);
            row += 1.0;
        }
        format!("Using {}.", plural(sources.len(), "picture", "pictures"))
    } else {
        let ring = b.shader("radial_spectrum.wgsl", "Stand-in (import media)", 0.0, row)?;
        sources.push(ring);
        String::new()
    };
    let source = if sources.len() >= 2 {
        let when = Changes {
            every: 32.0,
            events: &[SECTION],
            random: true,
            transition: 1,
            beats: 4.0,
        };
        b.switch(&sources, &when, 2.0, 2.0)?
    } else {
        sources[0]
    };
    // It has to move to leave trails: a slowly turning card that punches.
    let card = b.shader("fx_transform.wgsl", "Turning card", 3.0, 2.0)?;
    b.num(
        card,
        &[("zoom", 0.72), ("spin_speed", 0.07), ("kick_punch", 0.12)],
    )?;
    b.choice(card, "edges", 0)?;
    b.wire(source, card, "input")?;
    let ghost = b.shader("fx_afterimage.wgsl", "", 4.0, 2.0)?;
    b.wire(card, ghost, "input")?;
    let glow = b.finish(ghost, 5.0, 2.0, 0.5)?;
    b.drop_bloom(glow, 5.0, 3.0)?;
    l.genes(
        ghost,
        &[
            ("trail", 0.82, 0.97),
            ("complement", 0.2, 1.0),
            ("drift", -0.008, 0.012),
            ("swirl", -0.3, 0.3),
            ("beat_smear", 0.1, 1.0),
            ("ghost_glow", 0.6, 1.5),
        ],
    )?;
    l.genes(card, &[("zoom", 0.45, 0.95), ("spin_speed", -0.2, 0.2)])?;
    l.genes(glow, &[("amount", 0.2, 0.9)])?;
    Ok(summary)
}

/// Glitch: cuts between clips that melt into each other.
fn datamosh(l: &mut LookBuilder, media: &DemoMedia) -> Result<String, String> {
    let (backdrops, _) = sort_pictures(&media.photos);
    let b = &mut l.b;
    let mut sources = Vec::new();
    let mut row = 1.0;
    for (i, v) in media.videos.iter().take(4).enumerate() {
        sources.push(b.video(v, &format!("Video {}", i + 1), 0.0, row)?);
        row += 1.0;
    }
    let videos = sources.len();
    for (i, p) in backdrops.iter().take(4 - videos).enumerate() {
        sources.push(b.image(p, &format!("Picture {}", i + 1), 0.0, row)?);
        row += 1.0;
    }
    let found = sources.len();
    // Topped up with moving scenes: moshing needs movement.
    let want = if found == 0 { 3 } else { 2 };
    for scene in [
        "warehouse_tunnel.wgsl",
        "fractal_bloom.wgsl",
        "iron_furnace.wgsl",
    ] {
        if sources.len() >= want {
            break;
        }
        sources.push(b.shader(scene, "", 0.0, row)?);
        row += 1.0;
    }
    let every = 16.0;
    let when = Changes {
        every,
        events: &[SECTION, DROP],
        random: true,
        transition: 0, // cuts: the mosh is the transition
        beats: 1.0,
    };
    let sw = b.switch(&sources, &when, 2.0, 2.0)?;
    let mosh = b.shader("fx_datamosh.wgsl", "", 3.0, 2.0)?;
    b.wire(sw, mosh, "input")?;
    // Every cut moshes for a moment (the section / drop ones are built in).
    let tick = b.node(BEAT_TRIGGER, "Every 4 bars", 2.0, 3.2)?;
    b.num(tick, &[("every", every)])?;
    let burst = b.node(ENVELOPE, "Mosh after the cut", 3.0, 3.2)?;
    b.num(burst, &[("attack", 0.01), ("decay", 1.4)])?;
    b.trigger(tick, burst, "trigger")?;
    b.drive(burst, mosh, "mosh", 0.9)?;
    let glow = b.finish(mosh, 4.0, 2.0, 0.35)?;
    l.genes(
        mosh,
        &[
            ("blocks", 10.0, 44.0),
            ("flow", 0.6, 3.0),
            ("melt", 0.0, 0.8),
            ("heal", 0.01, 0.12),
            ("on_snare", 0.0, 0.7),
        ],
    )?;
    l.genes(burst, &[("decay", 0.5, 3.0)])?;
    l.genes(glow, &[("amount", 0.1, 0.6)])?;
    Ok(if found == 0 {
        String::new()
    } else {
        format!(
            "Using {}.",
            plural(
                found,
                "of your clips / pictures",
                "of your clips / pictures"
            )
        )
    })
}

/// Geometry: the Mandelbrot bulbs, and Julia flowers on section changes.
fn fractal(l: &mut LookBuilder) -> Result<String, String> {
    let b = &mut l.b;
    let bulbs = b.shader("fractal_bloom.wgsl", "Bulbs", 0.0, 1.0)?;
    b.num(bulbs, &[("bubbling", 0.4), ("dive_speed", 0.05)])?;
    let flowers = b.shader("fractal_bloom.wgsl", "Flowers", 0.0, 2.0)?;
    b.choice(flowers, "shape", 1)?;
    b.num(flowers, &[("hue_shift", 0.35), ("wander_speed", 0.08)])?;
    let when = Changes {
        every: 128.0,
        events: &[SECTION],
        random: false,
        transition: 1,
        beats: 8.0,
    };
    let sw = b.switch(&[bulbs, flowers], &when, 2.0, 1.5)?;
    let sway = b.shader("fx_liquid.wgsl", "Kick ripple", 3.0, 1.5)?;
    b.num(sway, &[("wobble", 0.004), ("ripple", 0.02)])?;
    b.wire(sw, sway, "input")?;
    let glow = b.finish(sway, 4.0, 1.5, 0.7)?;
    b.drop_bloom(glow, 4.0, 2.5)?;
    l.genes(
        bulbs,
        &[
            ("bubbling", 0.0, 1.0),
            ("dive_speed", 0.01, 0.15),
            ("depth", 2.0, 9.0),
            ("slices", 1.0, 8.0),
            ("hue_shift", 0.0, 1.0),
            ("edge_flash", 0.3, 1.6),
        ],
    )?;
    l.genes(
        flowers,
        &[("wander_speed", 0.02, 0.2), ("hue_shift", 0.0, 1.0)],
    )?;
    l.genes(glow, &[("amount", 0.3, 1.0)])?;
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(dir: &std::path::Path) -> DemoMedia {
        std::fs::create_dir_all(dir).unwrap();
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
            image::Rgba([
                (x * 37 % 256) as u8,
                (y * 53 % 256) as u8,
                (x * y % 256) as u8,
                255,
            ])
        })
        .save(&busy)
        .unwrap();
        DemoMedia {
            videos: vec![],
            photos: vec![
                logo.to_string_lossy().into_owned(),
                busy.to_string_lossy().into_owned(),
            ],
            camera: None,
        }
    }

    #[test]
    fn every_look_builds_with_and_without_media_and_its_genes_are_real() {
        let lib = SceneLibrary::new(None);
        let dir = std::env::temp_dir().join(format!("ve_looks_{}", std::process::id()));
        for m in [DemoMedia::default(), media(&dir)] {
            for look in ALL {
                let built = build(look.id, &lib, &m).unwrap_or_else(|e| panic!("{}: {e}", look.id));
                assert!(built.graph.node(built.out).is_some());
                assert!(built.genes.len() >= 5, "{} has few genes", look.id);
                for g in &built.genes {
                    let p = built
                        .graph
                        .node(g.node)
                        .and_then(|n| n.param(&g.param))
                        .unwrap_or_else(|| panic!("{}: gene {} missing", look.id, g.param));
                    assert!(g.min < g.max);
                    if let ParamValue::Float(v) = p.value {
                        assert!(
                            (g.min..=g.max).contains(&v),
                            "{}: {} starts at {v}, outside its gene range",
                            look.id,
                            g.param
                        );
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn looks_have_different_ideas() {
        let mut ideas: Vec<_> = ALL.iter().map(|l| l.idea).collect();
        ideas.sort();
        ideas.dedup();
        assert_eq!(ideas.len(), ALL.len());
    }
}
