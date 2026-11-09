//! A scene's own uniform block, `params`: its declared inputs and clocks.
//!
//! Layout: colour inputs first (each a `vec4<f32>`, 16-byte aligned), then
//! float/bool inputs and clocks as `f32`, then one padding float. The WGSL
//! struct is generated from the metadata, so it always matches.

use std::collections::HashMap;

use crate::meta::{InputKind, NumOrInput, SceneMeta};
use crate::uniforms::{self, SignalState};

/// Where each input and clock lives in the params block (in floats).
#[derive(Debug, Clone)]
pub struct ParamLayout {
    pub input_offsets: Vec<usize>,
    pub clock_offsets: Vec<usize>,
    /// Floats in the block, a multiple of 4.
    pub len: usize,
}

impl ParamLayout {
    pub fn new(meta: &SceneMeta) -> Self {
        let mut input_offsets = vec![0; meta.inputs.len()];
        let mut at = 0;
        for (i, input) in meta.inputs.iter().enumerate() {
            if input.kind == InputKind::Color {
                input_offsets[i] = at;
                at += 4;
            }
        }
        for (i, input) in meta.inputs.iter().enumerate() {
            if input.kind != InputKind::Color {
                input_offsets[i] = at;
                at += 1;
            }
        }
        let clock_offsets = (0..meta.clocks.len()).map(|i| at + i).collect();
        at += meta.clocks.len() + 1; // + _pad
        Self {
            input_offsets,
            clock_offsets,
            len: at.div_ceil(4) * 4,
        }
    }

    /// WGSL declaration of the `Params` struct.
    pub fn wgsl_struct(meta: &SceneMeta) -> String {
        let mut s = String::from("struct Params {\n");
        for i in meta.inputs.iter().filter(|i| i.kind == InputKind::Color) {
            s += &format!("    {}: vec4<f32>,\n", i.name);
        }
        for i in meta.inputs.iter().filter(|i| i.kind != InputKind::Color) {
            s += &format!("    {}: f32,\n", i.name);
        }
        for c in &meta.clocks {
            s += &format!("    {}: f32,\n", c.name);
        }
        s += "    _pad: f32,\n}\n";
        s
    }
}

/// Current input values and clock positions for one scene.
#[derive(Debug, Clone)]
pub struct ParamState {
    meta: SceneMeta,
    layout: ParamLayout,
    /// One entry per input: 1 float (float/bool) or 4 (colour).
    pub values: Vec<Vec<f32>>,
    pub clocks: Vec<f64>,
    clock_signal: Vec<Option<usize>>,
    block: Vec<f32>,
}

impl ParamState {
    pub fn new(meta: &SceneMeta) -> Self {
        let layout = ParamLayout::new(meta);
        Self {
            values: meta.inputs.iter().map(|i| i.default_value()).collect(),
            clocks: vec![0.0; meta.clocks.len()],
            clock_signal: meta
                .clocks
                .iter()
                .map(|c| c.signal.as_deref().and_then(uniforms::signal_index))
                .collect(),
            block: vec![0.0; layout.len],
            meta: meta.clone(),
            layout,
        }
    }

    pub fn meta(&self) -> &SceneMeta {
        &self.meta
    }

    /// Switch to new metadata (after a hot reload), keeping the values and
    /// clock positions of everything that still exists with the same type.
    pub fn remap(&self, meta: &SceneMeta) -> Self {
        let mut new = Self::new(meta);
        let old_values: HashMap<&str, (InputKind, &Vec<f32>)> = self
            .meta
            .inputs
            .iter()
            .zip(&self.values)
            .map(|(i, v)| (i.name.as_str(), (i.kind, v)))
            .collect();
        for (i, input) in meta.inputs.iter().enumerate() {
            if let Some((kind, v)) = old_values.get(input.name.as_str())
                && *kind == input.kind
            {
                let mut v = (*v).clone();
                if kind == &InputKind::Float {
                    let (lo, hi) = input.range();
                    v[0] = v[0].clamp(lo, hi);
                }
                new.values[i] = v;
            }
        }
        for (i, c) in meta.clocks.iter().enumerate() {
            if let Some(j) = self.meta.clocks.iter().position(|o| o.name == c.name) {
                new.clocks[i] = self.clocks[j];
            }
        }
        new
    }

    /// Reset every input to its default.
    pub fn reset_inputs(&mut self) {
        self.values = self.meta.inputs.iter().map(|i| i.default_value()).collect();
    }

    fn resolve(&self, v: &NumOrInput) -> f32 {
        match v {
            NumOrInput::Num(x) => *x,
            NumOrInput::Input(name) => self
                .meta
                .input_index(name)
                .map(|i| self.values[i][0])
                .unwrap_or(0.0),
        }
    }

    /// Advance the clocks: `θ += speed · (1 + amount · signal) · Δt`.
    pub fn advance(&mut self, signals: &SignalState, dt: f32) {
        let dt = dt.clamp(0.0, 0.25) as f64;
        for (i, c) in self.meta.clocks.iter().enumerate() {
            let speed = self.resolve(&c.speed) as f64;
            let amount = self.resolve(&c.amount) as f64;
            let signal = self.clock_signal[i]
                .map(|s| signals.block[s] as f64)
                .unwrap_or(0.0);
            self.clocks[i] += speed * (1.0 + amount * signal) * dt;
        }
    }

    /// The packed block, ready to upload.
    pub fn block(&mut self) -> &[f32] {
        for (i, v) in self.values.iter().enumerate() {
            let at = self.layout.input_offsets[i];
            self.block[at..at + v.len()].copy_from_slice(v);
        }
        for (i, c) in self.clocks.iter().enumerate() {
            // Integrated in f64; f32 still resolves ~1 ms of motion after hours.
            self.block[self.layout.clock_offsets[i]] = *c as f32;
        }
        &self.block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(json: &str) -> SceneMeta {
        SceneMeta::parse(json, "t").unwrap()
    }

    #[test]
    fn colours_come_first_and_block_is_padded() {
        let m = meta(
            r#"/*{"inputs": [
                {"name": "a", "type": "float", "default": 0.5},
                {"name": "c", "type": "color", "default": [0.1, 0.2, 0.3, 0.4]},
                {"name": "b", "type": "bool", "default": true}],
                "clocks": [{"name": "k", "speed": 2}]}*/"#,
        );
        let l = ParamLayout::new(&m);
        assert_eq!(l.input_offsets, vec![4, 0, 5]);
        assert_eq!(l.clock_offsets, vec![6]);
        assert_eq!(l.len, 8);
        let mut p = ParamState::new(&m);
        assert_eq!(&p.block()[..6], &[0.1, 0.2, 0.3, 0.4, 0.5, 1.0]);
        let wgsl = ParamLayout::wgsl_struct(&m);
        assert!(wgsl.find("c: vec4").unwrap() < wgsl.find("a: f32").unwrap());
    }

    #[test]
    fn clocks_integrate_speed_times_signal() {
        let m = meta(
            r#"/*{"inputs": [{"name": "spd", "type": "float", "min": 0, "max": 4, "default": 2}],
                "clocks": [{"name": "k", "speed": "spd", "signal": "bass_slow", "amount": 0.5}]}*/"#,
        );
        let mut p = ParamState::new(&m);
        let mut s = SignalState::new();
        let i = uniforms::signal_index("bass_slow").unwrap();
        s.block[i] = 2.0;
        // 2 · (1 + 0.5·2) · 0.1 = 0.4 per step.
        for _ in 0..10 {
            p.advance(&s, 0.1);
        }
        assert!((p.clocks[0] - 4.0).abs() < 1e-5);
        // Changing the speed slider changes the rate, not the position.
        p.values[0][0] = 0.0;
        p.advance(&s, 0.1);
        assert!((p.clocks[0] - 4.0).abs() < 1e-5);
    }

    #[test]
    fn remap_keeps_matching_values() {
        let m1 = meta(
            r#"/*{"inputs": [{"name": "a", "type": "float", "min": 0, "max": 10, "default": 1},
                             {"name": "b", "type": "float", "default": 0.5}],
                "clocks": [{"name": "k"}]}*/"#,
        );
        let m2 = meta(
            r#"/*{"inputs": [{"name": "a", "type": "float", "min": 0, "max": 5, "default": 1},
                             {"name": "b", "type": "bool"}],
                "clocks": [{"name": "k"}]}*/"#,
        );
        let mut p = ParamState::new(&m1);
        p.values[0][0] = 8.0;
        p.clocks[0] = 3.0;
        let q = p.remap(&m2);
        assert_eq!(q.values[0], vec![5.0]); // clamped to the new max
        assert_eq!(q.values[1], vec![0.0]); // type changed: default
        assert_eq!(q.clocks[0], 3.0);
    }
}
