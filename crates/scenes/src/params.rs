//! A scene's own uniform block, `params`: its declared inputs and clocks.
//!
//! Layout: colour inputs first (each a `vec4<f32>`, 16-byte aligned), then
//! float/bool inputs and clocks as `f32`, then one padding float. The WGSL
//! struct is generated from the metadata, so it always matches.

use std::collections::BTreeMap;

use signals::modulation::{ModContext, ModSlot, Modulator};

use crate::meta::{InputKind, NumOrInput, SceneMeta};
use crate::palette::{self, Palette, PaletteChoice};
use crate::preset::Preset;
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

/// Current input values, modulations, palette and clock positions for one scene.
#[derive(Debug, Clone)]
pub struct ParamState {
    meta: SceneMeta,
    layout: ParamLayout,
    /// Base value per input (what the slider is set to): 1 float
    /// (float/bool) or 4 (colour).
    pub values: Vec<Vec<f32>>,
    /// Modulations per input (only used for floats).
    pub mods: Vec<Vec<ModSlot>>,
    /// Values after modulation, updated by [`Self::update`].
    pub effective: Vec<Vec<f32>>,
    pub palette: PaletteChoice,
    scene_palette: Palette,
    pub clocks: Vec<f64>,
    clock_signal: Vec<Option<usize>>,
    block: Vec<f32>,
}

impl ParamState {
    pub fn new(meta: &SceneMeta) -> Self {
        let layout = ParamLayout::new(meta);
        let values: Vec<Vec<f32>> = meta.inputs.iter().map(|i| i.default_value()).collect();
        Self {
            effective: values.clone(),
            values,
            mods: vec![Vec::new(); meta.inputs.len()],
            palette: PaletteChoice::Scene,
            scene_palette: palette::from_hex_list(&meta.palette).unwrap_or(palette::BUILTIN[0].1),
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

    /// The scene's own palette (from its header).
    pub fn scene_palette(&self) -> &Palette {
        &self.scene_palette
    }

    /// The palette currently in use.
    pub fn current_palette(&self) -> Palette {
        self.palette.resolve(&self.scene_palette)
    }

    /// Switch to new metadata (after a hot reload), keeping the values,
    /// modulations and clock positions of everything that still exists with
    /// the same type.
    pub fn remap(&self, meta: &SceneMeta) -> Self {
        let mut new = Self::new(meta);
        new.palette = self.palette.clone();
        for (i, input) in meta.inputs.iter().enumerate() {
            if let Some(j) = self.meta.input_index(&input.name)
                && self.meta.inputs[j].kind == input.kind
            {
                let mut v = self.values[j].clone();
                if input.kind == InputKind::Float {
                    let (lo, hi) = input.range();
                    v[0] = v[0].clamp(lo, hi);
                }
                new.values[i] = v;
                new.mods[i] = self.mods[j].clone();
            }
        }
        new.effective = new.values.clone();
        for (i, c) in meta.clocks.iter().enumerate() {
            if let Some(j) = self.meta.clocks.iter().position(|o| o.name == c.name) {
                new.clocks[i] = self.clocks[j];
            }
        }
        new
    }

    /// Back to the scene's defaults: values, no modulation, scene palette.
    pub fn reset_inputs(&mut self) {
        self.values = self.meta.inputs.iter().map(|i| i.default_value()).collect();
        self.effective = self.values.clone();
        self.mods = vec![Vec::new(); self.meta.inputs.len()];
        self.palette = PaletteChoice::Scene;
    }

    fn resolve(&self, v: &NumOrInput) -> f32 {
        match v {
            NumOrInput::Num(x) => *x,
            NumOrInput::Input(name) => self
                .meta
                .input_index(name)
                .map(|i| self.effective[i][0])
                .unwrap_or(0.0),
        }
    }

    /// Apply the mapping rule to every float input. `depth_scale` is the
    /// global intensity (1 = as set).
    pub fn update(&mut self, ctx: &ModContext, depth_scale: f32) {
        for (i, input) in self.meta.inputs.iter().enumerate() {
            if input.kind != InputKind::Float || self.mods[i].is_empty() {
                self.effective[i].clone_from(&self.values[i]);
                continue;
            }
            let (min, max) = input.range();
            let m = Modulator {
                base: self.values[i][0],
                min,
                max,
                slots: &self.mods[i],
            };
            self.effective[i][0] = m.value(ctx, depth_scale);
        }
    }

    /// Advance the clocks: `θ += speed · (1 + amount · signal) · Δt`. Speeds
    /// read the modulated input values, so modulating a speed changes the
    /// rate of motion, never its position.
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
        for (i, v) in self.effective.iter().enumerate() {
            let at = self.layout.input_offsets[i];
            self.block[at..at + v.len()].copy_from_slice(v);
        }
        for (i, c) in self.clocks.iter().enumerate() {
            // Integrated in f64; f32 still resolves ~1 ms of motion after hours.
            self.block[self.layout.clock_offsets[i]] = *c as f32;
        }
        &self.block
    }

    /// Save as a preset.
    pub fn to_preset(&self, name: &str, scene_file: &str) -> Preset {
        let mut values = BTreeMap::new();
        let mut mods = BTreeMap::new();
        for (i, input) in self.meta.inputs.iter().enumerate() {
            values.insert(input.name.clone(), self.values[i].clone());
            if !self.mods[i].is_empty() {
                mods.insert(input.name.clone(), self.mods[i].clone());
            }
        }
        Preset {
            name: name.to_string(),
            scene: scene_file.to_string(),
            values,
            mods,
            palette: self.palette.clone(),
        }
    }

    /// Load a preset's values, modulations and palette (unknown names are
    /// ignored, missing ones get defaults).
    pub fn apply_preset(&mut self, p: &Preset) {
        self.reset_inputs();
        for (i, input) in self.meta.inputs.iter().enumerate() {
            if let Some(v) = p.values.get(&input.name)
                && v.len() == self.values[i].len()
            {
                let mut v = v.clone();
                if input.kind == InputKind::Float {
                    let (lo, hi) = input.range();
                    v[0] = v[0].clamp(lo, hi);
                }
                self.values[i] = v;
            }
            if let Some(m) = p.mods.get(&input.name)
                && input.kind == InputKind::Float
            {
                self.mods[i] = m.clone();
            }
        }
        self.effective = self.values.clone();
        self.palette = p.palette.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<f32> {
        None
    }

    fn ctx<'a>(f: &'a dyn Fn(&str) -> Option<f32>) -> ModContext<'a> {
        ModContext {
            signal: f,
            beats: 0.0,
            time: 0.0,
            controls: &[],
        }
    }

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
        p.update(&ctx(&none), 1.0);
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

    #[test]
    fn modulation_moves_effective_value_and_clock_speed() {
        use signals::modulation::{Shape, Source};
        let m = meta(
            r#"/*{"inputs": [{"name": "spd", "type": "float", "min": 0, "max": 4, "default": 1}],
                "clocks": [{"name": "k", "speed": "spd"}]}*/"#,
        );
        let mut p = ParamState::new(&m);
        // +0.25 of the range (4) while the control is at 1 → 1 + 1 = 2.
        p.mods[0].push(ModSlot::new(Source::Control(0), Shape::Linear, 0.25));
        let sig = |_: &str| None;
        let c = ModContext {
            signal: &sig,
            beats: 0.0,
            time: 0.0,
            controls: &[1.0],
        };
        p.update(&c, 1.0);
        assert_eq!(p.effective[0], vec![2.0]);
        assert_eq!(p.values[0], vec![1.0]); // the slider itself stays put
        let s = SignalState::new();
        p.advance(&s, 0.25);
        assert!((p.clocks[0] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn clocks_do_not_depend_on_frame_rate() {
        let m = meta(
            r#"/*{"clocks": [{"name": "k", "speed": 0.7, "signal": "bass_slow", "amount": 2}]}*/"#,
        );
        let mut s = SignalState::new();
        s.block[uniforms::signal_index("bass_slow").unwrap()] = 1.5;
        let mut a = ParamState::new(&m);
        let mut b = ParamState::new(&m);
        for _ in 0..30 {
            a.advance(&s, 1.0 / 30.0);
        }
        for _ in 0..240 {
            b.advance(&s, 1.0 / 240.0);
        }
        assert!((a.clocks[0] - b.clocks[0]).abs() < 1e-5);
    }

    #[test]
    fn presets_round_trip_through_param_state() {
        use signals::modulation::{Shape, Source};
        let m = meta(
            r#"/*{"inputs": [{"name": "a", "type": "float", "min": 0, "max": 2, "default": 1},
                             {"name": "c", "type": "color"}]}*/"#,
        );
        let mut p = ParamState::new(&m);
        p.values[0][0] = 1.7;
        p.values[1] = vec![0.1, 0.2, 0.3, 1.0];
        p.mods[0].push(ModSlot::new(
            Source::Signal("bass".into()),
            Shape::Linear,
            0.1,
        ));
        p.palette = PaletteChoice::Named("Ice".into());
        let preset = p.to_preset("mine", "x.wgsl");
        let mut q = ParamState::new(&m);
        q.apply_preset(&preset);
        assert_eq!(q.values, p.values);
        assert_eq!(q.mods, p.mods);
        assert_eq!(q.palette, p.palette);
        q.reset_inputs();
        assert_eq!(q.values[0], vec![1.0]);
        assert!(q.mods[0].is_empty());
    }
}
