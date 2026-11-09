//! The mapping rule (CLAUDE.md, maths item 10), used by every parameter:
//!
//! `p = clamp(p0 + Σ dᵢ·Sᵢ(mᵢ(t)), pmin, pmax)`
//!
//! where `m` is a [`Source`] (a signal, a beat-synced LFO, a manual control or a
//! random walk) and `S` is a [`Shape`]. Depths are fractions of the parameter's
//! range, so a depth of 0.5 can move a slider across half its travel.
//!
//! Everything here is a pure function of the current signals, beat position
//! and time: there is no per-frame state, so results never depend on the frame
//! rate.

use serde::{Deserialize, Serialize};

/// LFO wave forms. All are unipolar (0..1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Wave {
    Sine,
    Triangle,
    Saw,
    Square,
}

impl Wave {
    pub const ALL: [Wave; 4] = [Wave::Sine, Wave::Triangle, Wave::Saw, Wave::Square];

    pub fn name(self) -> &'static str {
        match self {
            Wave::Sine => "sine",
            Wave::Triangle => "triangle",
            Wave::Saw => "saw",
            Wave::Square => "square",
        }
    }

    /// Value at phase `x` (cycles), 0..1, starting at 0 on the beat.
    pub fn at(self, x: f64) -> f32 {
        let p = x.rem_euclid(1.0) as f32;
        match self {
            Wave::Sine => 0.5 - 0.5 * (std::f32::consts::TAU * p).cos(),
            Wave::Triangle => 1.0 - (2.0 * p - 1.0).abs(),
            Wave::Saw => p,
            Wave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// Where a modulation's raw value comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    /// A named signal from the scene signal block, e.g. `bass_fast` (1.0 =
    /// typical) or `kick_age` (seconds since the last kick).
    Signal(String),
    /// LFO locked to the beat clock: one cycle every `beats` beats (4 = a bar).
    Lfo { wave: Wave, beats: f32 },
    /// A manual control, 0..1 (macro slider now, MIDI knob later).
    Control(usize),
    /// Smooth random wander, 0..1, about `rate` changes per second.
    Random { rate: f32, seed: u32 },
}

/// How the raw value is shaped before it is scaled by the depth.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    /// As is.
    Linear,
    /// `x^g` (x clamped at 0). g > 1 = only peaks, g < 1 = more sensitive.
    Curve { gamma: f32 },
    /// Beat envelope `exp(−x/decay)`: x is a time since a hit (an `*_age`
    /// signal), so this is 1 on the hit, fading over `decay` seconds.
    Envelope { decay: f32 },
    /// Phase pulse `(1−φ)^k`: x is a phase 0..1 (beat_phase, bar_phase).
    Pulse { k: f32 },
    /// Synced LFO `sin(2π·rate·x)`: x is a phase (usually bar_phase). −1..1.
    Lfo { rate: f32 },
    /// Steps `floor(n·x)/n`.
    Step { n: f32 },
}

impl Shape {
    pub fn name(&self) -> &'static str {
        match self {
            Shape::Linear => "linear",
            Shape::Curve { .. } => "curve",
            Shape::Envelope { .. } => "hit envelope",
            Shape::Pulse { .. } => "phase pulse",
            Shape::Lfo { .. } => "sine of phase",
            Shape::Step { .. } => "steps",
        }
    }

    pub fn apply(&self, x: f32) -> f32 {
        match *self {
            Shape::Linear => x,
            Shape::Curve { gamma } => x.max(0.0).powf(gamma.max(0.01)),
            Shape::Envelope { decay } => (-x.max(0.0) / decay.max(1e-3)).exp(),
            Shape::Pulse { k } => (1.0 - x.rem_euclid(1.0)).powf(k.max(0.01)),
            Shape::Lfo { rate } => (std::f32::consts::TAU * rate * x).sin(),
            Shape::Step { n } => {
                let n = n.max(1.0).round();
                (n * x).floor() / n
            }
        }
    }
}

/// What a modulation can read this frame.
pub struct ModContext<'a> {
    /// Signal value by name (`None` = unknown name, counts as 0).
    pub signal: &'a dyn Fn(&str) -> Option<f32>,
    /// Continuous beat position (beats since start, fractional).
    pub beats: f64,
    /// Seconds.
    pub time: f64,
    /// Manual controls, 0..1.
    pub controls: &'a [f32],
}

/// One `dᵢ·Sᵢ(mᵢ)` term.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModSlot {
    pub source: Source,
    pub shape: Shape,
    /// Fraction of the parameter's range (−1..1 in the UI).
    pub depth: f32,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

fn hash(n: u64) -> f32 {
    // SplitMix64 finaliser → 0..1.
    let mut z = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth 1-D value noise, 0..1, at position `x`.
pub fn random_walk(x: f64, seed: u32) -> f32 {
    let i = x.floor();
    let f = (x - i) as f32;
    let s = (seed as u64) << 32;
    let a = hash(s ^ (i as i64 as u64));
    let b = hash(s ^ ((i + 1.0) as i64 as u64));
    let u = f * f * (3.0 - 2.0 * f);
    a + (b - a) * u
}

impl ModSlot {
    pub fn new(source: Source, shape: Shape, depth: f32) -> Self {
        Self {
            source,
            shape,
            depth,
            enabled: true,
        }
    }

    /// The raw source value `m(t)`.
    pub fn source_value(&self, ctx: &ModContext) -> f32 {
        match &self.source {
            Source::Signal(name) => (ctx.signal)(name).unwrap_or(0.0),
            Source::Lfo { wave, beats } => wave.at(ctx.beats / beats.max(1e-3) as f64),
            Source::Control(i) => ctx.controls.get(*i).copied().unwrap_or(0.0),
            Source::Random { rate, seed } => random_walk(ctx.time * rate.max(0.0) as f64, *seed),
        }
    }

    /// `S(m(t))`, before depth.
    pub fn shaped(&self, ctx: &ModContext) -> f32 {
        let v = self.shape.apply(self.source_value(ctx));
        if v.is_finite() { v } else { 0.0 }
    }
}

/// A parameter under the mapping rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Modulator<'a> {
    pub base: f32,
    pub min: f32,
    pub max: f32,
    pub slots: &'a [ModSlot],
}

impl Modulator<'_> {
    /// `clamp(p0 + Σ dᵢ·Sᵢ(mᵢ(t))·(max−min)·depth_scale, min, max)`.
    pub fn value(&self, ctx: &ModContext, depth_scale: f32) -> f32 {
        let range = self.max - self.min;
        let sum: f32 = self
            .slots
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.depth * depth_scale * range * s.shaped(ctx))
            .sum();
        (self.base + sum).clamp(self.min.min(self.max), self.max.max(self.min))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with<'a>(
        f: &'a dyn Fn(&str) -> Option<f32>,
        beats: f64,
        time: f64,
        controls: &'a [f32],
    ) -> ModContext<'a> {
        ModContext {
            signal: f,
            beats,
            time,
            controls,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn shapes_follow_the_spec() {
        assert!(close(Shape::Linear.apply(0.3), 0.3));
        assert!(close(Shape::Curve { gamma: 2.0 }.apply(0.5), 0.25));
        assert!(close(Shape::Curve { gamma: 2.0 }.apply(-1.0), 0.0));
        assert!(close(Shape::Envelope { decay: 0.2 }.apply(0.0), 1.0));
        assert!(close(
            Shape::Envelope { decay: 0.2 }.apply(0.2),
            (-1.0f32).exp()
        ));
        assert!(close(Shape::Pulse { k: 2.0 }.apply(0.0), 1.0));
        assert!(close(Shape::Pulse { k: 2.0 }.apply(0.5), 0.25));
        assert!(close(Shape::Lfo { rate: 1.0 }.apply(0.25), 1.0));
        assert!(close(Shape::Lfo { rate: 2.0 }.apply(0.25), 0.0));
        assert!(close(Shape::Step { n: 4.0 }.apply(0.6), 0.5));
        assert!(close(Shape::Step { n: 4.0 }.apply(0.99), 0.75));
    }

    #[test]
    fn lfo_source_is_locked_to_beats() {
        let none = |_: &str| None;
        let slot = ModSlot::new(
            Source::Lfo {
                wave: Wave::Saw,
                beats: 4.0,
            },
            Shape::Linear,
            1.0,
        );
        assert!(close(slot.shaped(&ctx_with(&none, 2.0, 0.0, &[])), 0.5));
        assert!(close(slot.shaped(&ctx_with(&none, 6.0, 9.0, &[])), 0.5));
        assert!(close(Wave::Sine.at(0.0), 0.0));
        assert!(close(Wave::Sine.at(0.5), 1.0));
        assert!(close(Wave::Square.at(0.25), 1.0));
        assert!(close(Wave::Triangle.at(0.5), 1.0));
    }

    #[test]
    fn mapping_rule_adds_scaled_terms_and_clamps() {
        let sig = |n: &str| match n {
            "bass_fast" => Some(2.0),
            "kick_age" => Some(0.0),
            _ => None,
        };
        let ctx = ctx_with(&sig, 0.0, 0.0, &[0.5]);
        let slots = vec![
            ModSlot::new(Source::Signal("bass_fast".into()), Shape::Linear, 0.1),
            ModSlot::new(Source::Control(0), Shape::Linear, 0.2),
        ];
        let m = Modulator {
            base: 1.0,
            min: 0.0,
            max: 10.0,
            slots: &slots,
        };
        // 1 + 0.1·10·2 + 0.2·10·0.5 = 1 + 2 + 1 = 4
        assert!(close(m.value(&ctx, 1.0), 4.0));
        // depth scale (global intensity) halves the modulation.
        assert!(close(m.value(&ctx, 0.5), 2.5));

        // Clamping at the top...
        let big = vec![ModSlot::new(
            Source::Signal("kick_age".into()),
            Shape::Envelope { decay: 0.1 },
            5.0,
        )];
        let m = Modulator {
            base: 1.0,
            min: 0.0,
            max: 10.0,
            slots: &big,
        };
        assert!(close(m.value(&ctx, 1.0), 10.0));
        // ...and the bottom; disabled slots do nothing.
        let mut neg = big.clone();
        neg[0].depth = -5.0;
        let m2 = Modulator {
            slots: &neg,
            ..m.clone()
        };
        assert!(close(m2.value(&ctx, 1.0), 0.0));
        neg[0].enabled = false;
        let m3 = Modulator {
            slots: &neg,
            ..m.clone()
        };
        assert!(close(m3.value(&ctx, 1.0), 1.0));
        // Unknown signals count as zero.
        let odd = vec![ModSlot::new(
            Source::Signal("nope".into()),
            Shape::Linear,
            1.0,
        )];
        let m4 = Modulator { slots: &odd, ..m };
        assert!(close(m4.value(&ctx, 1.0), 1.0));
    }

    #[test]
    fn independent_of_frame_rate() {
        // Every source is a pure function of time / beats, so sampling the same
        // moment after 10 big steps or 1000 small ones gives the same value.
        let none = |_: &str| None;
        let slot = ModSlot::new(
            Source::Random { rate: 3.0, seed: 7 },
            Shape::Curve { gamma: 1.5 },
            1.0,
        );
        let mut t_coarse = 0.0;
        for _ in 0..10 {
            t_coarse += 0.1;
        }
        let mut t_fine = 0.0;
        for _ in 0..1000 {
            t_fine += 0.001;
        }
        let a = slot.shaped(&ctx_with(&none, 0.0, t_coarse, &[]));
        let b = slot.shaped(&ctx_with(&none, 0.0, t_fine, &[]));
        assert!((a - b).abs() < 1e-3, "{a} vs {b}");
    }

    #[test]
    fn random_walk_is_smooth_and_bounded() {
        let mut prev = random_walk(0.0, 1);
        for i in 1..2000 {
            let v = random_walk(i as f64 * 0.01, 1);
            assert!((0.0..=1.0).contains(&v));
            assert!((v - prev).abs() < 0.05, "jump at {i}");
            prev = v;
        }
        assert_ne!(random_walk(0.5, 1), random_walk(0.5, 2));
    }
}
