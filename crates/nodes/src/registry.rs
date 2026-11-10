//! The node types and what they look like to the graph (plugs and
//! parameters).
//!
//! - **Shader nodes** (`shader:<file>.wgsl`): every scene and effect file in
//!   the library. A scene is a Shader Generator (no picture inputs); an effect
//!   takes the pictures its header declares in `"images"`. Header inputs
//!   become parameters.
//! - **Audio Signal**: any signal from the bus (bass, kick age, beat phase...).
//! - **LFO**: a wave locked to the beat.
//! - **Program Out**: the main output.

use graph::{NodeSpec, ParamDef, ParamValue, PortDef, PortType};
use scenes::meta::{InputKind, SceneMeta};
use scenes::uniforms::SIGNAL_NAMES;
use signals::modulation::Wave;

pub const SHADER_PREFIX: &str = "shader:";
pub const AUDIO_SIGNAL: &str = "audio_signal";
pub const LFO: &str = "lfo";
pub const PROGRAM_OUT: &str = "program_out";
pub const AUDIO_TRIGGER: &str = "audio_trigger";
pub const BEAT_TRIGGER: &str = "beat_trigger";
pub const GATE: &str = "gate";
pub const MATH: &str = "math";
pub const REMAP: &str = "remap";
pub const SMOOTH: &str = "smooth";
pub const ENVELOPE: &str = "envelope";
pub const COUNTER: &str = "counter";
pub const SAMPLE_HOLD: &str = "sample_hold";
pub const RANDOM: &str = "random";
pub const HOLD: &str = "hold";
pub const IMAGE: &str = "image";
pub const VIDEO: &str = "video";
pub const CAMERA: &str = "camera";
/// The Camera node's parameters: which camera, and its size / rate
/// ("1280x720@30", empty = automatic).
pub const CAMERA_PARAM: &str = "camera";
pub const CAMERA_MODE_PARAM: &str = "camera_mode";
/// Media nodes' "feed the video signals" switch.
pub const SIGNALS_PARAM: &str = "signals";
/// Video playback modes.
pub const VIDEO_MODES: &[&str] = &["Loop", "Ping-pong", "Play once", "Beat-locked", "Scrub"];
/// The media nodes' parameter holding the file path.
pub const FILE_PARAM: &str = "file";
/// The shader inside the Image / Video nodes (not in the scenes library).
pub const MEDIA_SHADER: &str = "media_fit.wgsl";
pub const MEDIA_SHADER_SRC: &str = include_str!("../shaders/media_fit.wgsl");
/// The pass that limits any node to its mask.
pub const MASK_SHADER: &str = "mask_apply.wgsl";
/// One layer of a Layer Mixer.
pub const LAYER_SHADER: &str = "layer_blend.wgsl";

/// Shaders built into the app (not files in the scenes folder).
pub fn builtin_shader(file: &str) -> Option<&'static str> {
    match file {
        MEDIA_SHADER => Some(MEDIA_SHADER_SRC),
        MASK_SHADER => Some(include_str!("../shaders/mask_apply.wgsl")),
        LAYER_SHADER => Some(include_str!("../shaders/layer_blend.wgsl")),
        _ => None,
    }
}

pub const LAYER_MIXER: &str = "layer_mixer";
/// How many layers a Layer Mixer takes.
pub const LAYERS: usize = 8;

/// A Layer Mixer's plug and settings for layer `i` (1 = bottom).
pub fn layer_port(i: usize) -> String {
    format!("layer{i}")
}
pub fn layer_param(what: &str, i: usize) -> String {
    format!("{what}{i}")
}

/// Which layer a Layer Mixer plug is (`layer3` → 3).
pub fn layer_index(port: &str) -> Option<usize> {
    port.strip_prefix("layer")?.parse().ok()
}

/// Is this an Image, Video or Camera node?
pub fn is_media_kind(kind: &str) -> bool {
    kind == IMAGE || kind == VIDEO || kind == CAMERA
}
/// Units for Hold's length.
pub const HOLD_UNITS: &[&str] = &["beats", "bars", "seconds"];

/// Events the Audio Trigger node can fire on: (key, label).
pub const TRIGGER_EVENTS: &[(&str, &str)] = &[
    ("kick", "Kick"),
    ("snare", "Snare"),
    ("hat", "Hi-hat"),
    ("onset", "Any hit"),
    ("beat", "Every beat"),
    ("bar", "Every bar"),
    ("build", "Build-up starts"),
    ("drop", "Drop"),
    ("section", "Section change"),
];

pub const MATH_OPS: &[&str] = &[
    "Add",
    "Subtract",
    "Multiply",
    "Min",
    "Max",
    "Mix",
    "Difference",
];

/// Name of the picture output of shader nodes and of the value output of
/// signal nodes.
pub const IMAGE_OUT: &str = "out";
pub const VALUE_OUT: &str = "value";
pub const TRIGGER_OUT: &str = "fire";

pub fn shader_kind(file: &str) -> String {
    format!("{SHADER_PREFIX}{file}")
}

/// The shader file of a `shader:` node kind.
pub fn shader_file(kind: &str) -> Option<&str> {
    kind.strip_prefix(SHADER_PREFIX)
}

/// The node spec for a scene or effect file.
pub fn shader_spec(file: &str, meta: &SceneMeta) -> NodeSpec {
    let params = meta
        .inputs
        .iter()
        .map(|i| {
            let (min, max) = i.range();
            let d = i.default_value();
            let default = match i.kind {
                InputKind::Float => ParamValue::Float(d[0]),
                InputKind::Bool => ParamValue::Bool(d[0] > 0.5),
                InputKind::Choice => ParamValue::Choice(d[0].max(0.0) as u32),
                InputKind::Color => ParamValue::Colour([d[0], d[1], d[2], d[3]]),
            };
            ParamDef {
                name: i.name.clone(),
                label: i.label().to_string(),
                default,
                min: if i.kind == InputKind::Float { min } else { 0.0 },
                max: if i.kind == InputKind::Float { max } else { 0.0 },
                options: i.options.clone(),
            }
        })
        .collect();
    NodeSpec {
        kind: shader_kind(file),
        label: meta.name.clone(),
        inputs: meta
            .images
            .iter()
            .map(|img| PortDef {
                name: img.name.clone(),
                label: img.label().to_string(),
                ty: PortType::Image,
                optional: true,
            })
            .collect(),
        outputs: vec![PortDef::new(IMAGE_OUT, PortType::Image)],
        params,
    }
}

/// Specs of the built-in (non-shader) nodes.
pub fn builtin_spec(kind: &str) -> Option<NodeSpec> {
    let value_out = || vec![PortDef::new(VALUE_OUT, PortType::Signal)];
    let fire_out = || vec![PortDef::new(TRIGGER_OUT, PortType::Trigger)];
    let sig_in = |name: &str, label: &str| PortDef {
        label: label.into(),
        ..PortDef::new(name, PortType::Signal).optional()
    };
    let trig_in = |name: &str, label: &str| PortDef {
        label: label.into(),
        ..PortDef::new(name, PortType::Trigger).optional()
    };
    let num = |name: &str, label: &str, min: f32, max: f32, default: f32| ParamDef {
        label: label.into(),
        ..ParamDef::float(name, min, max, default)
    };
    let pick = |name: &str, label: &str, options: &[&str], default: u32| ParamDef {
        label: label.into(),
        ..ParamDef::choice(name, options, default)
    };
    let toggle = |name: &str, label: &str, on: bool| ParamDef {
        label: label.into(),
        default: ParamValue::Bool(on),
        min: 0.0,
        max: 0.0,
        ..ParamDef::float(name, 0.0, 0.0, 0.0)
    };
    let spec = |label: &str, inputs: Vec<PortDef>, outputs: Vec<PortDef>, params| NodeSpec {
        kind: kind.into(),
        label: label.into(),
        inputs,
        outputs,
        params,
    };
    Some(match kind {
        AUDIO_TRIGGER => spec(
            "Audio Trigger",
            vec![],
            fire_out(),
            vec![pick(
                "event",
                "Fires on",
                &TRIGGER_EVENTS.iter().map(|e| e.0).collect::<Vec<_>>(),
                0,
            )],
        ),
        BEAT_TRIGGER => spec(
            "Beat Trigger",
            vec![],
            fire_out(),
            vec![
                num("every", "Every (beats)", 0.25, 64.0, 1.0),
                num("offset", "Offset (beats)", 0.0, 16.0, 0.0),
            ],
        ),
        GATE => spec(
            "Gate",
            vec![sig_in("in", "In")],
            fire_out(),
            vec![
                num("threshold", "Threshold", -4.0, 8.0, 1.5),
                num("gap", "Min gap (s)", 0.0, 4.0, 0.1),
            ],
        ),
        MATH => spec(
            "Math",
            vec![sig_in("a", "A"), sig_in("b", "B")],
            value_out(),
            vec![
                pick("op", "Operation", MATH_OPS, 0),
                num("a_value", "A (if unplugged)", -10.0, 10.0, 0.0),
                num("b_value", "B (if unplugged)", -10.0, 10.0, 1.0),
                num("mix", "Mix amount", 0.0, 1.0, 0.5),
            ],
        ),
        REMAP => spec(
            "Remap",
            vec![sig_in("in", "In")],
            value_out(),
            vec![
                num("in_min", "From min", -10.0, 10.0, 0.0),
                num("in_max", "From max", -10.0, 10.0, 2.0),
                num("out_min", "To min", -10.0, 10.0, 0.0),
                num("out_max", "To max", -10.0, 10.0, 1.0),
                num("curve", "Curve", 0.2, 5.0, 1.0),
                toggle("clamp", "Clamp", true),
            ],
        ),
        SMOOTH => spec(
            "Smooth",
            vec![sig_in("in", "In")],
            value_out(),
            vec![
                num("attack", "Rise time (s)", 0.0, 5.0, 0.05),
                num("release", "Fall time (s)", 0.0, 10.0, 0.6),
            ],
        ),
        ENVELOPE => spec(
            "Envelope",
            vec![trig_in("trigger", "Trigger")],
            value_out(),
            vec![
                num("attack", "Attack (s)", 0.0, 2.0, 0.01),
                num("decay", "Decay (s)", 0.02, 8.0, 0.35),
            ],
        ),
        COUNTER => spec(
            "Counter",
            vec![trig_in("count", "Count"), trig_in("reset", "Reset")],
            value_out(),
            vec![num("steps", "Steps", 2.0, 64.0, 4.0)],
        ),
        IMAGE | VIDEO | CAMERA => {
            let text = |name: &str, label: &str| ParamDef {
                label: label.into(),
                default: ParamValue::Text(String::new()),
                min: 0.0,
                max: 0.0,
                ..ParamDef::float(name, 0.0, 0.0, 0.0)
            };
            let file = text(FILE_PARAM, "File");
            let feeds = toggle(SIGNALS_PARAM, "Feeds the video signals", true);
            let place = [
                pick(
                    "fit",
                    "Fit",
                    &["Fit (show all)", "Fill (crop)", "Stretch"],
                    1,
                ),
                toggle("mirror", "Mirror", false),
                num("scale", "Scale", 0.1, 4.0, 1.0),
            ];
            if kind == CAMERA {
                let mut place = place;
                place[1] = toggle("mirror", "Mirror (like a mirror)", true);
                return Some(spec(
                    "Camera",
                    vec![sig_in("hold", "Hold (freeze)")],
                    vec![PortDef::new(IMAGE_OUT, PortType::Image)],
                    [
                        text(CAMERA_PARAM, "Camera"),
                        text(CAMERA_MODE_PARAM, "Size and rate"),
                    ]
                    .into_iter()
                    .chain(place)
                    .chain([feeds])
                    .collect(),
                ));
            }
            if kind == IMAGE {
                return Some(spec(
                    "Image",
                    vec![],
                    vec![PortDef::new(IMAGE_OUT, PortType::Image)],
                    std::iter::once(file).chain(place).collect(),
                ));
            }
            spec(
                "Video",
                vec![
                    trig_in("restart", "Restart"),
                    trig_in("jump", "Jump (random)"),
                    trig_in("stutter", "Stutter"),
                    sig_in("hold", "Hold (freeze)"),
                ],
                vec![PortDef::new(IMAGE_OUT, PortType::Image)],
                std::iter::once(file)
                    .chain([
                        pick("mode", "Playback", VIDEO_MODES, 0),
                        num("speed", "Speed", -4.0, 4.0, 1.0),
                        num("bars", "Length in bars (beat-locked)", 0.25, 64.0, 4.0),
                        num("position", "Position (scrub)", 0.0, 1.0, 0.0),
                        num("start", "Start at", 0.0, 1.0, 0.0),
                        num("end", "End at", 0.0, 1.0, 1.0),
                        num("stutter_len", "Stutter slice (beats)", 0.0625, 4.0, 0.25),
                        num("stutter_repeats", "Stutter repeats", 1.0, 32.0, 4.0),
                    ])
                    .chain(place)
                    .chain([pick(
                        "quality",
                        "Decode size",
                        &["Low (640)", "Medium (960)", "High (1280)", "Full (1920)"],
                        2,
                    )])
                    .chain([feeds])
                    .collect(),
            )
        }
        LAYER_MIXER => {
            let modes = scenes::blend::BLEND_MODES;
            let mut params = Vec::new();
            for i in 1..=LAYERS {
                params.push(pick(
                    &layer_param("mode", i),
                    &format!("Layer {i} blend"),
                    modes,
                    0,
                ));
                params.push(num(
                    &layer_param("opacity", i),
                    &format!("Layer {i} opacity"),
                    0.0,
                    1.0,
                    1.0,
                ));
                params.push(toggle(
                    &layer_param("solo", i),
                    &format!("Layer {i} solo"),
                    false,
                ));
                params.push(toggle(
                    &layer_param("mute", i),
                    &format!("Layer {i} mute"),
                    false,
                ));
            }
            params.push(toggle("linear", "Blend in linear light", false));
            spec(
                "Layer Mixer",
                (1..=LAYERS)
                    .map(|i| PortDef {
                        label: format!("Layer {i}"),
                        ..PortDef::new(&layer_port(i), PortType::Image).optional()
                    })
                    .collect(),
                vec![PortDef::new(IMAGE_OUT, PortType::Image)],
                params,
            )
        }
        HOLD => spec(
            "Hold",
            vec![trig_in("start", "Start"), trig_in("stop", "Stop")],
            value_out(),
            vec![
                num("length", "Length (0 = until Stop)", 0.0, 64.0, 0.0),
                pick("unit", "Length in", HOLD_UNITS, 0),
                num("fade_in", "Fade in (s)", 0.0, 4.0, 0.0),
                num("fade_out", "Fade out (s)", 0.0, 8.0, 0.25),
                toggle("toggle", "Start again = stop", false),
            ],
        ),
        SAMPLE_HOLD => spec(
            "Sample & Hold",
            vec![sig_in("in", "In"), trig_in("trigger", "Trigger")],
            value_out(),
            vec![],
        ),
        RANDOM => spec(
            "Random",
            vec![],
            value_out(),
            vec![
                num("rate", "Changes per beat", 0.05, 16.0, 0.5),
                toggle("smooth", "Smooth", true),
                num("seed", "Seed", 0.0, 100.0, 1.0),
            ],
        ),
        AUDIO_SIGNAL => NodeSpec {
            kind: AUDIO_SIGNAL.into(),
            label: "Audio Signal".into(),
            inputs: vec![],
            outputs: value_out(),
            params: vec![ParamDef {
                label: "Signal".into(),
                ..ParamDef::choice("signal", SIGNAL_NAMES, signal_choice("bass_fast"))
            }],
        },
        LFO => NodeSpec {
            kind: LFO.into(),
            label: "LFO".into(),
            inputs: vec![],
            outputs: value_out(),
            params: vec![
                ParamDef {
                    label: "Beats per cycle".into(),
                    ..ParamDef::float("beats", 0.25, 64.0, 4.0)
                },
                ParamDef {
                    label: "Wave".into(),
                    ..ParamDef::choice("wave", &Wave::ALL.map(|w| w.name()), 0)
                },
                ParamDef {
                    label: "Phase offset".into(),
                    ..ParamDef::float("offset", 0.0, 1.0, 0.0)
                },
            ],
        },
        PROGRAM_OUT => NodeSpec {
            kind: PROGRAM_OUT.into(),
            label: "Program Out".into(),
            inputs: vec![PortDef::new("image", PortType::Image)],
            outputs: vec![],
            params: vec![],
        },
        _ => return None,
    })
}

/// Index of a signal name in the Audio Signal node's list.
pub fn signal_choice(name: &str) -> u32 {
    SIGNAL_NAMES.iter().position(|n| *n == name).unwrap_or(0) as u32
}

/// The spec for any node kind, looking shader files up in `library`.
pub fn spec_for(kind: &str, library: &scenes::SceneLibrary) -> Result<NodeSpec, String> {
    if let Some(spec) = builtin_spec(kind) {
        return Ok(spec);
    }
    let file = shader_file(kind).ok_or_else(|| format!("unknown node type \"{kind}\""))?;
    let entry = library
        .get(file)
        .ok_or_else(|| format!("{file} is not in the scenes folder"))?;
    let meta = SceneMeta::parse(&entry.source, file.trim_end_matches(".wgsl"))?;
    Ok(shader_spec(file, &meta))
}

/// Where a node type sits in the add menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    /// Makes a picture from nothing (scenes = Shader Generators).
    Source,
    /// Changes or combines pictures.
    Effect,
    /// Makes a number that can drive any slider.
    Signal,
    /// Fires events (hits, beats, thresholds).
    Trigger,
    Output,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::Source,
        Category::Effect,
        Category::Signal,
        Category::Trigger,
        Category::Output,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Source => "Sources",
            Category::Effect => "Effects",
            Category::Signal => "Signals",
            Category::Trigger => "Triggers",
            Category::Output => "Output",
        }
    }
}

/// One entry in the add menu.
#[derive(Clone, Debug)]
pub struct CatalogueEntry {
    pub kind: String,
    pub label: String,
    pub category: Category,
    pub description: String,
    /// Types of its inputs (not counting the automatic mask).
    pub inputs: Vec<PortType>,
    /// Has number settings a Signal wire can drive.
    pub modulatable: bool,
}

/// Every node type that can be added, sorted by category then name.
pub fn catalogue(library: &scenes::SceneLibrary) -> Vec<CatalogueEntry> {
    let mut out: Vec<CatalogueEntry> = library
        .entries()
        .iter()
        .filter_map(|e| {
            let meta = SceneMeta::parse(&e.source, e.file_name.trim_end_matches(".wgsl")).ok()?;
            Some(CatalogueEntry {
                kind: shader_kind(&e.file_name),
                label: meta.name.clone(),
                category: if meta.is_effect() {
                    Category::Effect
                } else {
                    Category::Source
                },
                description: meta.description.clone(),
                inputs: meta.images.iter().map(|_| PortType::Image).collect(),
                modulatable: meta.inputs.iter().any(|i| i.kind == InputKind::Float),
            })
        })
        .collect();
    let builtin = [
        (
            AUDIO_SIGNAL,
            Category::Signal,
            "Any signal: bass, kick hits, loudness, beat phase, and video motion / brightness /              colour. Wire it into a slider.",
        ),
        (
            LFO,
            Category::Signal,
            "A wave locked to the beat. Wire it into a slider for steady, musical motion.",
        ),
        (
            MATH,
            Category::Signal,
            "Combine two signals: add, multiply, min/max, mix and more.",
        ),
        (
            REMAP,
            Category::Signal,
            "Rescale a signal into a new range, with a curve. Bass 0 to 3 becomes zoom 1 to 1.5.",
        ),
        (
            SMOOTH,
            Category::Signal,
            "Takes the jitter out of a signal, with separate rise and fall times.",
        ),
        (
            ENVELOPE,
            Category::Signal,
            "Turns each trigger into a punch that fades out. Kick, Envelope, anything.",
        ),
        (
            COUNTER,
            Category::Signal,
            "Counts triggers and steps through values: change colour every bar, every 4 kicks...",
        ),
        (
            HOLD,
            Category::Signal,
            "When Start fires, goes to 1 and stays there until Stop fires or the length runs              out. \"On the drop, bend until the next hi-hat.\"",
        ),
        (
            SAMPLE_HOLD,
            Category::Signal,
            "Grabs a signal on each trigger and holds it until the next.",
        ),
        (
            RANDOM,
            Category::Signal,
            "A random value locked to the beat, smooth or stepped.",
        ),
        (
            AUDIO_TRIGGER,
            Category::Trigger,
            "Fires on kicks, snares, hats, beats, bars, build-ups, drops or section changes.",
        ),
        (
            BEAT_TRIGGER,
            Category::Trigger,
            "Fires every N beats, with an offset: a metronome for visuals.",
        ),
        (
            GATE,
            Category::Trigger,
            "Fires when a signal rises above a threshold.",
        ),
        (
            IMAGE,
            Category::Source,
            "A picture file (PNG, JPG, WebP...). Wire it through effects and let the music bend it.",
        ),
        (
            VIDEO,
            Category::Source,
            "A video file: loop, ping-pong, lock it to the beat, scrub it with a signal,              jump, stutter or freeze it on triggers.",
        ),
        (
            CAMERA,
            Category::Source,
            "A live camera (webcam, capture card). Its movement becomes signals too: wave              at it and watch Motion.",
        ),
        (
            LAYER_MIXER,
            Category::Effect,
            "Stacks pictures like layers: each one on top of the ones before, with its own \
             blend mode and opacity. Solo, mute and reorder them in the Inspector.",
        ),
        (PROGRAM_OUT, Category::Output, "The final picture."),
    ];
    for (kind, category, description) in builtin {
        if let Some(spec) = builtin_spec(kind) {
            out.push(CatalogueEntry {
                kind: kind.into(),
                label: spec.label.clone(),
                category,
                description: description.into(),
                inputs: spec.inputs.iter().map(|p| p.ty).collect(),
                modulatable: spec.params.iter().any(|p| p.is_modulatable()),
            });
        }
    }
    out.sort_by(|a, b| (a.category, &a.label).cmp(&(b.category, &b.label)));
    out
}

/// The category of a node kind (for its colour in the editor).
pub fn category_of(kind: &str, outputs_image: bool, has_image_inputs: bool) -> Category {
    match kind {
        AUDIO_TRIGGER | BEAT_TRIGGER | GATE => Category::Trigger,
        AUDIO_SIGNAL | LFO | MATH | REMAP | SMOOTH | ENVELOPE | COUNTER | SAMPLE_HOLD | RANDOM
        | HOLD => Category::Signal,
        PROGRAM_OUT => Category::Output,
        _ if outputs_image && !has_image_inputs => Category::Source,
        _ => Category::Effect,
    }
}
