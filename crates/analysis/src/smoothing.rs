//! Time-based smoothing (spec items 3 and 4). Everything takes the elapsed time
//! Δt, never a frame count, so the feel is identical at any update rate.

use crate::config::AttackRelease;

/// One-pole coefficient for time constant `tau` over `dt`: `α = 1 − exp(−Δt/τ)`.
#[inline]
pub fn alpha(dt: f32, tau: f32) -> f32 {
    if tau <= 0.0 {
        1.0
    } else {
        1.0 - (-dt / tau).exp()
    }
}

/// Attack/release smoother: `s += α·(x − s)`, τ = attack if rising else release.
#[derive(Clone, Copy, Debug)]
pub struct Smoother {
    pub value: f32,
    times: AttackRelease,
}

impl Smoother {
    pub fn new(times: AttackRelease) -> Self {
        Self { value: 0.0, times }
    }

    #[inline]
    pub fn update(&mut self, x: f32, dt: f32) -> f32 {
        let tau = if x > self.value {
            self.times.attack
        } else {
            self.times.release
        };
        self.value += alpha(dt, tau) * (x - self.value);
        self.value
    }

    pub fn reset(&mut self) {
        self.value = 0.0;
    }
}

/// Auto-gain (spec item 3): slow running mean `Ē += α_slow·(E − Ē)` and level
/// `ℓ = E / (Ē + ε)`, clamped to `[0, max]`.
///
/// Warm-up: until the mean has seen `τ/Δt` non-silent frames it uses a plain
/// running average (α = 1/n), so 1.0 means "typical" within the first second
/// instead of after ~8 s of overshoot. After warm-up it is exactly the spec.
#[derive(Clone, Copy, Debug)]
pub struct AutoGain {
    pub mean: f32,
    active_frames: u32,
    tau: f32,
    epsilon: f32,
    max: f32,
}

impl AutoGain {
    pub fn new(tau: f32, epsilon: f32, max: f32) -> Self {
        Self {
            mean: 0.0,
            active_frames: 0,
            tau,
            epsilon,
            max,
        }
    }

    #[inline]
    pub fn update(&mut self, energy: f32, dt: f32) -> f32 {
        let mut a = alpha(dt, self.tau);
        if energy > self.epsilon {
            self.active_frames = self.active_frames.saturating_add(1);
            a = a.max(1.0 / self.active_frames as f32);
        }
        self.mean += a * (energy - self.mean);
        (energy / (self.mean + self.epsilon)).clamp(0.0, self.max)
    }

    pub fn reset(&mut self) {
        self.mean = 0.0;
        self.active_frames = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AR: AttackRelease = AttackRelease {
        attack: 0.05,
        release: 0.2,
    };

    /// Run a smoother over `secs` of input `x(t)` with step `dt`.
    fn run(dt: f32, secs: f32, x: impl Fn(f32) -> f32) -> f32 {
        let mut s = Smoother::new(AR);
        let steps = (secs / dt).round() as usize;
        for i in 0..steps {
            // Sample the input mid-step so float rounding can't move edges.
            s.update(x((i as f32 + 0.5) * dt), dt);
        }
        s.value
    }

    #[test]
    fn smoothing_is_frame_rate_independent() {
        // Step up for 0.1 s: same result at 1 ms, ~10.7 ms (one hop) and 20 ms steps.
        let a = run(0.001, 0.1, |_| 1.0);
        let b = run(0.1 / 10.0, 0.1, |_| 1.0);
        let c = run(0.02, 0.1, |_| 1.0);
        let exact = 1.0 - (-0.1f32 / AR.attack).exp();
        for v in [a, b, c] {
            assert!((v - exact).abs() < 1e-4, "{v} vs {exact}");
        }
        // Rise then release: piecewise-constant input gives matching results too.
        let x = |t: f32| if t < 0.2 { 1.0 } else { 0.0 };
        let fine = run(0.001, 0.6, x);
        let coarse = run(0.01, 0.6, x);
        assert!((fine - coarse).abs() < 2e-3, "{fine} vs {coarse}");
    }

    #[test]
    fn attack_is_faster_than_release() {
        let mut s = Smoother::new(AR);
        s.update(1.0, 0.05);
        let after_attack = s.value; // one attack time constant: ~63%
        assert!((after_attack - 0.632).abs() < 0.01);
        s.value = 1.0;
        s.update(0.0, 0.05);
        assert!(s.value > 0.7, "release should be slower: {}", s.value);
    }

    #[test]
    fn auto_gain_makes_steady_input_one() {
        for energy in [1e-4f32, 1e-2, 1.0] {
            let mut g = AutoGain::new(8.0, 1e-8, 8.0);
            let mut l = 0.0;
            for _ in 0..200 {
                l = g.update(energy, 0.0107);
            }
            assert!((l - 1.0).abs() < 0.01, "energy {energy}: level {l}");
        }
    }

    #[test]
    fn auto_gain_treats_silence_as_zero_and_clamps() {
        let mut g = AutoGain::new(8.0, 1e-8, 8.0);
        assert_eq!(g.update(0.0, 0.01), 0.0);
        for _ in 0..500 {
            g.update(1e-4, 0.01);
        }
        assert_eq!(g.update(1.0, 0.01), 8.0); // a huge spike is clamped
    }
}
