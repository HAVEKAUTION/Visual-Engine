//! The scene metadata header: an ISF-style JSON object in a comment at the very
//! top of the `.wgsl` file.
//!
//! ```text
//! /*{
//!   "name": "Kaleido Bloom",
//!   "author": "Visual Engine",
//!   "description": "...",
//!   "inputs": [
//!     { "name": "zoom", "type": "float", "min": 0.5, "max": 3, "default": 1.2 },
//!     { "name": "mirror", "type": "bool", "default": true },
//!     { "name": "tint", "type": "color", "default": [1, 0.5, 0.2, 1] }
//!   ],
//!   "clocks": [
//!     { "name": "spin", "speed": "spin_speed", "signal": "bass_slow", "amount": 1.5 }
//!   ],
//!   "buffers": [ { "name": "A", "persistent": true } ],
//!   "passes": [ { "entry": "fs_trail", "target": "A" }, { "entry": "fs_main" } ]
//! }*/
//! ```
//!
//! ISF's upper-case keys (`NAME`, `INPUTS`, `DEFAULT`, ...) are accepted too.

use serde::Deserialize;

use crate::uniforms;

/// Parsed and checked scene metadata.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SceneMeta {
    #[serde(default, alias = "NAME")]
    pub name: String,
    #[serde(default, alias = "AUTHOR", alias = "CREDIT")]
    pub author: String,
    #[serde(default, alias = "DESCRIPTION")]
    pub description: String,
    /// Free-form style tag (psychedelic, minimal, glitch, ...).
    #[serde(default, alias = "STYLE", alias = "CATEGORIES")]
    pub style: serde_json::Value,
    /// Music the scene is made for, e.g. `["Techno", "Minimal"]`.
    #[serde(default, alias = "GENRES")]
    pub genres: Vec<String>,
    #[serde(default, alias = "ISFVSN", alias = "VSN")]
    pub version: serde_json::Value,
    #[serde(default, alias = "INPUTS")]
    pub inputs: Vec<InputDef>,
    #[serde(default, alias = "CLOCKS")]
    pub clocks: Vec<ClockDef>,
    #[serde(default, alias = "BUFFERS", alias = "PERSISTENT_BUFFERS")]
    pub buffers: Vec<BufferDef>,
    #[serde(default, alias = "PASSES")]
    pub passes: Vec<PassDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputKind {
    Float,
    Bool,
    Color,
}

/// A user-facing control, shown as a slider / checkbox / colour picker.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputDef {
    #[serde(alias = "NAME")]
    pub name: String,
    #[serde(default, alias = "LABEL")]
    pub label: String,
    #[serde(rename = "type", alias = "TYPE")]
    pub kind: InputKind,
    #[serde(default, alias = "MIN")]
    pub min: Option<f32>,
    #[serde(default, alias = "MAX")]
    pub max: Option<f32>,
    #[serde(default, alias = "DEFAULT")]
    pub default: Option<serde_json::Value>,
}

impl InputDef {
    pub fn label(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }

    /// (min, max) for floats; (0, 1) otherwise.
    pub fn range(&self) -> (f32, f32) {
        match self.kind {
            InputKind::Float => (self.min.unwrap_or(0.0), self.max.unwrap_or(1.0)),
            _ => (0.0, 1.0),
        }
    }

    /// The default as 1 or 4 floats.
    pub fn default_value(&self) -> Vec<f32> {
        let (lo, hi) = self.range();
        match self.kind {
            InputKind::Float => {
                let v = self
                    .default
                    .as_ref()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(lo);
                vec![v.clamp(lo.min(hi), hi.max(lo))]
            }
            InputKind::Bool => {
                let on = match &self.default {
                    Some(serde_json::Value::Bool(b)) => *b,
                    Some(v) => v.as_f64().unwrap_or(0.0) > 0.5,
                    None => false,
                };
                vec![if on { 1.0 } else { 0.0 }]
            }
            InputKind::Color => {
                let mut c = vec![1.0, 1.0, 1.0, 1.0];
                if let Some(serde_json::Value::Array(a)) = &self.default {
                    for (i, v) in a.iter().take(4).enumerate() {
                        c[i] = v.as_f64().unwrap_or(1.0) as f32;
                    }
                }
                c
            }
        }
    }
}

/// A number, or the name of an input whose current value is used.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum NumOrInput {
    Num(f32),
    Input(String),
}

/// An audio-driven clock: `value += speed · (1 + amount · signal) · Δt`.
///
/// This is the "drive speed, not position" rule: scenes animate with clocks
/// instead of mapping loudness straight onto a position, so motion speeds up
/// with the music but never jumps.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockDef {
    #[serde(alias = "NAME")]
    pub name: String,
    #[serde(default = "one", alias = "SPEED")]
    pub speed: NumOrInput,
    /// Any signal name (see [`uniforms::SIGNAL_NAMES`]); none = plain time.
    #[serde(default, alias = "SIGNAL")]
    pub signal: Option<String>,
    #[serde(default = "one", alias = "AMOUNT")]
    pub amount: NumOrInput,
}

fn one() -> NumOrInput {
    NumOrInput::Num(1.0)
}

/// An off-screen buffer a pass can render into and later passes (and, when
/// persistent, the next frame) can sample.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BufferDef {
    /// "A" or "B": sampled as `buf_a` / `buf_b`.
    #[serde(alias = "NAME", alias = "TARGET")]
    pub name: String,
    /// Keep the contents from frame to frame (feedback). Default true.
    #[serde(default = "yes", alias = "PERSISTENT")]
    pub persistent: bool,
}

fn yes() -> bool {
    true
}

/// One full-screen render pass.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassDef {
    /// Fragment entry point. Default `fs_main`.
    #[serde(default = "default_entry", alias = "ENTRY")]
    pub entry: String,
    /// "A" / "B" to render into a buffer; none = the scene output.
    #[serde(default, alias = "TARGET")]
    pub target: Option<String>,
}

fn default_entry() -> String {
    "fs_main".into()
}

/// Which texture a pass renders into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Output,
    Buffer(usize),
}

/// Maximum buffers ("A", "B").
pub const MAX_BUFFERS: usize = 2;

fn buffer_slot(name: &str) -> Option<usize> {
    match name.trim().to_ascii_uppercase().as_str() {
        "A" | "BUF_A" | "BUFFERA" => Some(0),
        "B" | "BUF_B" | "BUFFERB" => Some(1),
        _ => None,
    }
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.starts_with("__")
}

/// The header comment's JSON text, if the file starts with `/*{ ... }*/`.
fn header_json(source: &str) -> Option<&str> {
    let s = source.trim_start_matches('\u{feff}').trim_start();
    let rest = s.strip_prefix("/*")?;
    if !rest.trim_start().starts_with('{') {
        return None;
    }
    let end = rest.find("*/")?;
    Some(&rest[..end])
}

impl SceneMeta {
    /// Parse the header of `source`. A file without a header gets `fallback_name`
    /// and no inputs.
    pub fn parse(source: &str, fallback_name: &str) -> Result<Self, String> {
        let mut meta: SceneMeta = match header_json(source) {
            Some(json) => serde_json::from_str(json)
                .map_err(|e| format!("Metadata header (the JSON comment at the top): {e}"))?,
            None => SceneMeta::default(),
        };
        if meta.name.trim().is_empty() {
            meta.name = fallback_name.to_string();
        }
        if meta.passes.is_empty() {
            meta.passes.push(PassDef {
                entry: default_entry(),
                target: None,
            });
        }
        meta.check()?;
        Ok(meta)
    }

    fn check(&self) -> Result<(), String> {
        let mut names: Vec<&str> = Vec::new();
        for n in self
            .inputs
            .iter()
            .map(|i| i.name.as_str())
            .chain(self.clocks.iter().map(|c| c.name.as_str()))
        {
            if !is_identifier(n) {
                return Err(format!(
                    "\"{n}\" is not a valid name (letters, digits and _ only, not starting with a digit)"
                ));
            }
            if names.contains(&n) {
                return Err(format!("the name \"{n}\" is used twice"));
            }
            names.push(n);
        }
        for i in &self.inputs {
            if let (InputKind::Float, (lo, hi)) = (i.kind, i.range())
                && lo >= hi
            {
                return Err(format!("input \"{}\": min must be below max", i.name));
            }
        }
        for c in &self.clocks {
            if let Some(sig) = &c.signal
                && uniforms::signal_index(sig).is_none()
            {
                return Err(format!(
                    "clock \"{}\": unknown signal \"{sig}\" (try e.g. bass_slow, loudness_slow, kick_age)",
                    c.name
                ));
            }
            for v in [&c.speed, &c.amount] {
                if let NumOrInput::Input(name) = v
                    && !self
                        .inputs
                        .iter()
                        .any(|i| &i.name == name && i.kind == InputKind::Float)
                {
                    return Err(format!(
                        "clock \"{}\": \"{name}\" is not a float input",
                        c.name
                    ));
                }
            }
        }
        if self.buffers.len() > MAX_BUFFERS {
            return Err(format!("at most {MAX_BUFFERS} buffers (A and B)"));
        }
        for b in &self.buffers {
            if buffer_slot(&b.name).is_none() {
                return Err(format!(
                    "buffer \"{}\": name must be \"A\" or \"B\"",
                    b.name
                ));
            }
        }
        for (i, p) in self.passes.iter().enumerate() {
            let last = i + 1 == self.passes.len();
            match (self.pass_target(p)?, last) {
                (Target::Output, false) => {
                    return Err(format!(
                        "pass {} ({}) has no target; only the last pass draws the output",
                        i + 1,
                        p.entry
                    ));
                }
                (Target::Buffer(_), true) => {
                    return Err("the last pass must draw the output (no \"target\")".into());
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Where `pass` renders to.
    pub fn pass_target(&self, pass: &PassDef) -> Result<Target, String> {
        let Some(t) = &pass.target else {
            return Ok(Target::Output);
        };
        let slot = buffer_slot(t).ok_or_else(|| format!("unknown target buffer \"{t}\""))?;
        Ok(Target::Buffer(slot))
    }

    /// Whether buffer `slot` keeps its contents between frames. Buffers that are
    /// rendered into but not declared are persistent.
    pub fn buffer_persistent(&self, slot: usize) -> bool {
        self.buffers
            .iter()
            .find(|b| buffer_slot(&b.name) == Some(slot))
            .is_none_or(|b| b.persistent)
    }

    /// Input index by name.
    pub fn input_index(&self, name: &str) -> Option<usize> {
        self.inputs.iter().position(|i| i.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header() {
        let src = r#"/*{
            "name": "Test",
            "inputs": [
                {"name": "zoom", "type": "float", "min": 0.5, "max": 3, "default": 1.2},
                {"name": "on", "type": "bool", "default": true},
                {"name": "tint", "type": "color", "default": [1, 0.5, 0.25]}
            ],
            "clocks": [{"name": "spin", "speed": "zoom", "signal": "bass_slow"}],
            "buffers": [{"name": "A"}],
            "passes": [{"entry": "fs_a", "target": "A"}, {}]
        }*/
        @fragment fn fs_main() {}"#;
        let m = SceneMeta::parse(src, "file").unwrap();
        assert_eq!(m.name, "Test");
        assert_eq!(m.inputs.len(), 3);
        assert_eq!(m.inputs[0].default_value(), vec![1.2]);
        assert_eq!(m.inputs[1].default_value(), vec![1.0]);
        assert_eq!(m.inputs[2].default_value(), vec![1.0, 0.5, 0.25, 1.0]);
        assert_eq!(m.clocks[0].speed, NumOrInput::Input("zoom".into()));
        assert_eq!(m.clocks[0].amount, NumOrInput::Num(1.0));
        assert_eq!(m.pass_target(&m.passes[0]), Ok(Target::Buffer(0)));
        assert_eq!(m.passes[1].entry, "fs_main");
        assert!(m.buffer_persistent(0));
    }

    #[test]
    fn accepts_isf_upper_case_keys() {
        let src = r#"/*{"NAME": "Isf", "INPUTS": [{"NAME": "amt", "TYPE": "float", "MIN": 0, "MAX": 2, "DEFAULT": 3}]}*/"#;
        let m = SceneMeta::parse(src, "file").unwrap();
        assert_eq!(m.name, "Isf");
        // Default clamped into range.
        assert_eq!(m.inputs[0].default_value(), vec![2.0]);
    }

    #[test]
    fn no_header_is_fine() {
        let m = SceneMeta::parse("@fragment fn fs_main() {}", "plain").unwrap();
        assert_eq!(m.name, "plain");
        assert!(m.inputs.is_empty());
        assert_eq!(m.passes.len(), 1);
    }

    #[test]
    fn reports_bad_headers() {
        let bad = [
            r#"/*{"name": "x", "inputs": [{"name": "a b", "type": "float"}]}*/"#,
            r#"/*{"inputs": [{"name": "a", "type": "float", "min": 2, "max": 1}]}*/"#,
            r#"/*{"clocks": [{"name": "c", "signal": "nope"}]}*/"#,
            r#"/*{"clocks": [{"name": "c", "speed": "missing"}]}*/"#,
            r#"/*{"passes": [{"entry": "fs_main", "target": "A"}]}*/"#,
            r#"/*{"passes": [{"entry": "a"}, {"entry": "b"}]}*/"#,
            r#"/*{"buffers": [{"name": "Z"}]}*/"#,
            r#"/*{"name": 3}*/"#,
            r#"/*{"nmae": "typo"}*/"#,
        ];
        for b in bad {
            assert!(SceneMeta::parse(b, "f").is_err(), "{b}");
        }
    }
}
