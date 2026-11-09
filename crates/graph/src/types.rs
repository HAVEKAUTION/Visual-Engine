//! Port and parameter types: what a node can take in, put out and be set to.

use serde::{Deserialize, Serialize};
use signals::modulation::ModSlot;

/// What travels along a wire. Each type has its own wire colour in the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortType {
    /// A picture with alpha.
    Image,
    /// A single-channel picture (keys, mattes).
    Mask,
    /// One number per frame (audio, video, LFO, MIDI).
    Signal,
    /// Events (onsets, drops, MIDI notes, keys).
    Trigger,
}

impl PortType {
    pub const ALL: [PortType; 4] = [
        PortType::Image,
        PortType::Mask,
        PortType::Signal,
        PortType::Trigger,
    ];

    /// Can an output of type `from` be plugged into an input of this type?
    /// Same type always; an Image also works as a Mask (its brightness is used).
    pub fn accepts(self, from: PortType) -> bool {
        self == from || (self == PortType::Mask && from == PortType::Image)
    }

    pub fn name(self) -> &'static str {
        match self {
            PortType::Image => "Image",
            PortType::Mask => "Mask",
            PortType::Signal => "Signal",
            PortType::Trigger => "Trigger",
        }
    }
}

/// An input or output plug of a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortDef {
    /// Unique among the node's inputs (or outputs), e.g. `base`, `mask`.
    pub name: String,
    #[serde(default)]
    pub label: String,
    pub ty: PortType,
    /// The node still works with nothing plugged in.
    #[serde(default)]
    pub optional: bool,
}

impl PortDef {
    pub fn new(name: &str, ty: PortType) -> Self {
        Self {
            name: name.into(),
            label: String::new(),
            ty,
            optional: false,
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn label(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }
}

/// Name of the mask input every image node gets automatically.
pub const MASK_PORT: &str = "mask";

/// A parameter's value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    Float(f32),
    Bool(bool),
    /// Index into the parameter's `options`.
    Choice(u32),
    /// RGBA, 0..1.
    Colour([f32; 4]),
    Text(String),
}

impl ParamValue {
    /// Same variant (a Float can only be replaced by a Float, ...).
    pub fn same_kind(&self, other: &ParamValue) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// The definition of a parameter: name, range, default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamDef {
    pub name: String,
    #[serde(default)]
    pub label: String,
    pub default: ParamValue,
    /// Range of a Float (ignored if `min >= max`).
    #[serde(default)]
    pub min: f32,
    #[serde(default)]
    pub max: f32,
    /// Names of the options of a Choice.
    #[serde(default)]
    pub options: Vec<String>,
}

impl ParamDef {
    pub fn float(name: &str, min: f32, max: f32, default: f32) -> Self {
        Self {
            name: name.into(),
            label: String::new(),
            default: ParamValue::Float(default),
            min,
            max,
            options: Vec::new(),
        }
    }

    pub fn choice(name: &str, options: &[&str], default: u32) -> Self {
        Self {
            name: name.into(),
            label: String::new(),
            default: ParamValue::Choice(default),
            min: 0.0,
            max: 0.0,
            options: options.iter().map(|o| o.to_string()).collect(),
        }
    }

    pub fn label(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }

    /// Only number parameters can be modulated (by the mapping rule or a
    /// Signal wire).
    pub fn is_modulatable(&self) -> bool {
        matches!(self.default, ParamValue::Float(_))
    }

    /// `value` made valid for this parameter (Floats clamped to the range,
    /// Choices to the options), or `None` if it is the wrong kind.
    pub fn sanitise(&self, value: ParamValue) -> Option<ParamValue> {
        if !value.same_kind(&self.default) {
            return None;
        }
        Some(match value {
            ParamValue::Float(v) if self.min < self.max => {
                ParamValue::Float(v.clamp(self.min, self.max))
            }
            ParamValue::Choice(i) if !self.options.is_empty() => {
                ParamValue::Choice(i.min(self.options.len() as u32 - 1))
            }
            other => other,
        })
    }
}

/// A parameter on a node: its definition, current value and modulations
/// (the mapping rule; Signal wires into it are stored as links).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub def: ParamDef,
    pub value: ParamValue,
    #[serde(default)]
    pub mods: Vec<ModSlot>,
}

impl Param {
    pub fn new(def: ParamDef) -> Self {
        Self {
            value: def.default.clone(),
            def,
            mods: Vec::new(),
        }
    }
}

/// What kind of node to create: its type name, plugs and parameters. Node
/// implementations (the `nodes` crate) provide these; the graph only uses them
/// to know what can connect to what.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeSpec {
    /// Type name, e.g. `merge`, `shader:warehouse_tunnel.wgsl`.
    pub kind: String,
    /// Default label shown on the node.
    pub label: String,
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
    pub params: Vec<ParamDef>,
}
