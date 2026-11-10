//! The Switch node's memory, worked out on the CPU: which picture shows, the
//! one before it, how far the transition has got and which transition it is.
//! The shader (`fx_switch.wgsl`) only draws the result. Knowing this here
//! lets the evaluator skip drawing the pictures nobody can see.

/// The Switch's file name.
pub const SWITCH_SHADER: &str = "fx_switch.wgsl";

/// What the Switch's settings say this frame.
#[derive(Clone, Copy, Debug)]
pub struct SwitchKnobs {
    /// The Show setting, 0..1.
    pub show: f32,
    /// Pictures used (2 to 4).
    pub count: usize,
    /// Random order (never the same twice) instead of in turn.
    pub random: bool,
    /// Transition index (0 = Cut … 5 = Iris, 6 = a different one each time).
    pub transition: u32,
    /// Transition length in beats.
    pub beats: f32,
}

const MIX_IT_UP: u32 = 6;

#[derive(Clone, Debug, Default)]
pub struct SwitchState {
    started: bool,
    pub now: usize,
    pub before: usize,
    /// 0..1 (1 = done).
    pub progress: f32,
    /// The transition in use (0 = Cut … 5 = Iris).
    pub mode: u32,
    last_show: f32,
    /// Changes so far: with the node's seed, picks the random choices, so a
    /// set plays back the same way every time (export matches live).
    changes: u64,
}

impl SwitchState {
    /// Advance one frame. `seed` should differ per node; `bpm` 0 = unknown.
    pub fn step(&mut self, k: &SwitchKnobs, dt: f32, bpm: f32, seed: u64) {
        let n = k.count.clamp(2, 4);
        let show = k.show.clamp(0.0, 1.0);
        let in_turn = ((show * (n - 1) as f32).round() as usize).min(n - 1);
        if !self.started {
            self.started = true;
            self.now = in_turn;
            self.before = in_turn;
            self.progress = 1.0;
            self.mode = self.pick_mode(k.transition, seed);
            self.last_show = show;
            return;
        }
        let mut want = self.now.min(n - 1);
        if !k.random {
            want = in_turn;
        } else if (show - self.last_show).abs() > 0.001 {
            // Anything but the picture showing now.
            let r = rand01(seed, self.changes, 1);
            let skip = 1 + (r * (n - 1) as f64 * 0.999) as usize;
            want = (self.now + skip) % n;
        }
        self.last_show = show;
        if want != self.now {
            self.before = self.now;
            self.now = want;
            self.progress = 0.0;
            self.mode = self.pick_mode(k.transition, seed);
            self.changes += 1;
            return;
        }
        let bpm = if bpm > 30.0 { bpm } else { 120.0 };
        let secs = (k.beats * 60.0 / bpm).max(0.001);
        self.progress = (self.progress + dt / secs).min(1.0);
    }

    fn pick_mode(&self, transition: u32, seed: u64) -> u32 {
        if transition == MIX_IT_UP {
            1 + (rand01(seed, self.changes, 2) * 4.999) as u32
        } else {
            transition.min(5)
        }
    }

    /// Which of the four pictures can be seen this frame.
    pub fn visible(&self) -> [bool; 4] {
        let mut v = [false; 4];
        v[self.now.min(3)] = true;
        if self.progress < 1.0 && self.mode != 0 {
            v[self.before.min(3)] = true;
        }
        v
    }

    /// The values the shader reads (`_now`, `_before`, `_progress`, `_mode`).
    pub fn shader_values(&self) -> [(&'static str, f32); 4] {
        [
            ("_now", self.now as f32),
            ("_before", self.before as f32),
            ("_progress", self.progress),
            ("_mode", self.mode as f32),
        ]
    }
}

/// A repeatable random number in 0..1 (SplitMix64).
fn rand01(seed: u64, n: u64, salt: u64) -> f64 {
    let mut z = seed
        .wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(salt.wrapping_mul(0xD1B5_4A32_D192_ED03));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knobs(show: f32, random: bool, transition: u32) -> SwitchKnobs {
        SwitchKnobs {
            show,
            count: 4,
            random,
            transition,
            beats: 1.0,
        }
    }

    #[test]
    fn in_turn_follows_show_and_transitions_take_their_beats() {
        let mut s = SwitchState::default();
        s.step(&knobs(0.0, false, 1), 0.016, 120.0, 7);
        assert_eq!((s.now, s.progress), (0, 1.0));
        assert_eq!(s.visible(), [true, false, false, false]);
        s.step(&knobs(1.0, false, 1), 0.016, 120.0, 7);
        assert_eq!((s.now, s.before, s.progress), (3, 0, 0.0));
        assert_eq!(
            s.visible(),
            [true, false, false, true],
            "both during a dissolve"
        );
        // One beat at 120 BPM = 0.5 s.
        for _ in 0..10 {
            s.step(&knobs(1.0, false, 1), 0.05, 120.0, 7);
        }
        assert_eq!(s.progress, 1.0);
        assert_eq!(
            s.visible(),
            [false, false, false, true],
            "only the new one after"
        );
    }

    #[test]
    fn a_cut_never_needs_the_old_picture() {
        let mut s = SwitchState::default();
        s.step(&knobs(0.0, false, 0), 0.016, 120.0, 1);
        s.step(&knobs(1.0, false, 0), 0.016, 120.0, 1);
        assert_eq!(s.visible(), [false, false, false, true]);
    }

    #[test]
    fn random_never_repeats_and_is_repeatable() {
        let run = |seed| {
            let mut s = SwitchState::default();
            let mut shown = Vec::new();
            for i in 0..40 {
                let show = (i as f32 * 0.37).fract();
                let before = s.now;
                s.step(&knobs(show, true, MIX_IT_UP), 0.016, 128.0, seed);
                if i > 0 {
                    assert_ne!(s.now, before, "change {i} kept the same picture");
                    assert!((1..=5).contains(&s.mode), "mix it up never cuts");
                }
                shown.push(s.now);
            }
            shown
        };
        assert_eq!(run(5), run(5), "same seed, same set");
        assert_ne!(run(5), run(6), "another node, another order");
    }
}
