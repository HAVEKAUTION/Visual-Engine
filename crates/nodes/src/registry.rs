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

/// Name of the picture output of shader nodes and of the value output of
/// signal nodes.
pub const IMAGE_OUT: &str = "out";
pub const VALUE_OUT: &str = "value";

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
    Some(match kind {
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
    Output,
}

impl Category {
    pub const ALL: [Category; 4] = [
        Category::Source,
        Category::Effect,
        Category::Signal,
        Category::Output,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Source => "Sources",
            Category::Effect => "Effects",
            Category::Signal => "Signals",
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
            })
        })
        .collect();
    let builtin = [
        (
            AUDIO_SIGNAL,
            Category::Signal,
            "Any sound signal: bass, kick hits, loudness, beat phase… Wire it into a slider.",
        ),
        (
            LFO,
            Category::Signal,
            "A wave locked to the beat. Wire it into a slider for steady, musical motion.",
        ),
        (PROGRAM_OUT, Category::Output, "The final picture."),
    ];
    for (kind, category, description) in builtin {
        if let Some(spec) = builtin_spec(kind) {
            out.push(CatalogueEntry {
                kind: kind.into(),
                label: spec.label,
                category,
                description: description.into(),
            });
        }
    }
    out.sort_by(|a, b| (a.category, &a.label).cmp(&(b.category, &b.label)));
    out
}

/// The category of a node kind (for its colour in the editor).
pub fn category_of(kind: &str, outputs_image: bool, has_image_inputs: bool) -> Category {
    match kind {
        AUDIO_SIGNAL | LFO => Category::Signal,
        PROGRAM_OUT => Category::Output,
        _ if outputs_image && !has_image_inputs => Category::Source,
        _ => Category::Effect,
    }
}
