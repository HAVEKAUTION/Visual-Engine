//! Tempo estimation by weighted autocorrelation (spec item 6), plus tap tempo.
//!
//! The onset signal of the last ~6 s is autocorrelated. Each lag τ is weighted
//! with a log-Gaussian centred on 120 BPM,
//! `W(τ) = exp(−(log2(τ/τ120))² / (2σ²))`, which settles the classic
//! half/double-tempo ambiguity in favour of the usual dance-music range. The
//! best lag (refined to a fraction of a hop) gives the BPM. Small changes are
//! smoothed, big jumps must persist before they are accepted, and in silence
//! the last good BPM is held.

use crate::config::TempoConfig;
use crate::smoothing::alpha;

pub struct TempoTracker {
    cfg: TempoConfig,
    /// Hops per second.
    frame_rate: f32,
    /// Ring of the onset signal.
    history: Vec<f32>,
    pos: usize,
    filled: usize,
    /// Scratch: the history in time order, mean removed.
    linear: Vec<f32>,
    /// Scratch: weighted autocorrelation per lag.
    scores: Vec<f32>,
    lag_min: usize,
    lag_max: usize,
    hops_since_update: usize,

    bpm: f32,
    confidence: f32,
    locked: bool,
    manual: bool,
    /// A tempo jump waiting to be confirmed: (candidate bpm, seconds stable).
    pending: Option<(f32, f32)>,

    /// Tap tempo: times of recent taps (ring) and how many are valid.
    taps: [f64; 8],
    tap_count: usize,
}

impl TempoTracker {
    pub fn new(cfg: &TempoConfig, frame_rate: f32) -> Self {
        let len = (cfg.window_secs * frame_rate).round() as usize;
        let lag_min = ((60.0 * frame_rate / cfg.max_bpm).floor() as usize).max(2);
        let lag_max = ((60.0 * frame_rate / cfg.min_bpm).ceil() as usize).min(len / 2);
        Self {
            cfg: cfg.clone(),
            frame_rate,
            history: vec![0.0; len],
            pos: 0,
            filled: 0,
            linear: vec![0.0; len],
            scores: vec![0.0; lag_max + 2],
            lag_min,
            lag_max,
            hops_since_update: 0,
            bpm: cfg.prior_bpm,
            confidence: 0.0,
            locked: false,
            manual: false,
            pending: None,
            taps: [0.0; 8],
            tap_count: 0,
        }
    }

    pub fn bpm(&self) -> f32 {
        self.bpm
    }
    pub fn confidence(&self) -> f32 {
        self.confidence
    }
    pub fn locked(&self) -> bool {
        self.locked
    }
    pub fn manual(&self) -> bool {
        self.manual
    }

    /// Go back to fully automatic tempo tracking.
    pub fn clear_manual(&mut self) {
        self.manual = false;
        self.tap_count = 0;
    }

    /// Register a tap at audio time `now`. Returns the new BPM once there are
    /// at least two taps in a row.
    pub fn tap(&mut self, now: f64) -> Option<f32> {
        if self.tap_count > 0 {
            let last = self.taps[(self.tap_count - 1) % self.taps.len()];
            if now - last > self.cfg.tap_timeout_secs as f64 || now <= last {
                self.tap_count = 0;
            }
        }
        self.taps[self.tap_count % self.taps.len()] = now;
        self.tap_count += 1;
        let n = self.tap_count.min(self.taps.len());
        if n < 2 {
            return None;
        }
        // Average interval over the taps we still remember.
        let newest = self.taps[(self.tap_count - 1) % self.taps.len()];
        let oldest = self.taps[(self.tap_count - n) % self.taps.len()];
        let interval = (newest - oldest) / (n - 1) as f64;
        let bpm = (60.0 / interval) as f32;
        if !(self.cfg.min_bpm * 0.5..=self.cfg.max_bpm * 2.0).contains(&bpm) {
            return None;
        }
        self.bpm = bpm;
        self.locked = true;
        self.manual = true;
        self.pending = None;
        Some(bpm)
    }

    /// Feed one hop of the onset signal. `seconds_since_onset` is the time since
    /// the most recent onset (for holding the tempo through silence).
    pub fn process(&mut self, onset_signal: f32, seconds_since_onset: f64) {
        self.history[self.pos] = onset_signal;
        self.pos = (self.pos + 1) % self.history.len();
        self.filled = (self.filled + 1).min(self.history.len());

        self.hops_since_update += 1;
        if self.hops_since_update < self.cfg.update_hops {
            return;
        }
        let dt = self.hops_since_update as f32 / self.frame_rate;
        self.hops_since_update = 0;

        let enough = self.filled as f32 >= self.cfg.min_history_secs * self.frame_rate;
        let silent = seconds_since_onset > self.cfg.hold_after_silence_secs as f64;
        if !enough || silent {
            self.pending = None;
            return; // hold the last good BPM
        }
        if let Some((candidate, confidence)) = self.estimate() {
            self.confidence = confidence;
            self.accept(candidate, dt);
        }
    }

    /// Weighted autocorrelation → (bpm, confidence).
    fn estimate(&mut self) -> Option<(f32, f32)> {
        let n = self.filled;
        let len = self.history.len();
        let start = if n == len { self.pos } else { 0 };
        let mut mean = 0.0;
        for i in 0..n {
            let v = self.history[(start + i) % len];
            self.linear[i] = v;
            mean += v;
        }
        mean /= n as f32;
        let x = &mut self.linear[..n];
        for v in x.iter_mut() {
            *v -= mean;
        }
        let r0: f32 = x.iter().map(|v| v * v).sum::<f32>() / n as f32;
        if r0 <= 1e-12 {
            return None;
        }

        let lag_max = self.lag_max.min(n / 2);
        if lag_max <= self.lag_min + 2 {
            return None;
        }
        let tau120 = 60.0 * self.frame_rate / self.cfg.prior_bpm;
        let two_sigma_sq = 2.0 * self.cfg.prior_sigma * self.cfg.prior_sigma;
        let mut best = self.lag_min;
        for lag in self.lag_min - 1..=lag_max + 1 {
            let mut r = 0.0;
            for i in lag..n {
                r += x[i] * x[i - lag];
            }
            r /= (n - lag) as f32;
            let w = (-(lag as f32 / tau120).log2().powi(2) / two_sigma_sq).exp();
            self.scores[lag] = w * r;
            if (self.lag_min..=lag_max).contains(&lag) && self.scores[lag] > self.scores[best] {
                best = lag;
            }
        }
        let peak = self.scores[best];
        if peak <= 0.0 {
            return None;
        }
        // Parabolic interpolation around the peak for sub-hop precision.
        let (a, b, c) = (
            self.scores[best - 1],
            self.scores[best],
            self.scores[best + 1],
        );
        let denom = a - 2.0 * b + c;
        let shift = if denom.abs() > 1e-12 {
            (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        let lag = best as f32 + shift;
        let bpm = 60.0 * self.frame_rate / lag;
        let confidence = {
            let w = (-(best as f32 / tau120).log2().powi(2) / two_sigma_sq).exp();
            (peak / w / r0).clamp(0.0, 1.0)
        };
        Some((bpm, confidence))
    }

    fn accept(&mut self, mut candidate: f32, dt: f32) {
        if self.manual {
            // A tapped tempo is only fine-tuned: fold the estimate to the octave
            // nearest the tapped value and ignore anything far from it.
            while candidate > self.bpm * 1.5 {
                candidate *= 0.5;
            }
            while candidate < self.bpm / 1.5 {
                candidate *= 2.0;
            }
            if (candidate / self.bpm - 1.0).abs() < self.cfg.jump_fraction {
                self.bpm += alpha(dt, self.cfg.smoothing_secs) * (candidate - self.bpm);
            }
            return;
        }
        if !self.locked {
            self.bpm = candidate;
            self.locked = true;
            return;
        }
        if (candidate / self.bpm - 1.0).abs() < self.cfg.jump_fraction {
            self.pending = None;
            self.bpm += alpha(dt, self.cfg.smoothing_secs) * (candidate - self.bpm);
            return;
        }
        // A big change: accept only once it has been stable for a while.
        match &mut self.pending {
            Some((p, t)) if (candidate / *p - 1.0).abs() < self.cfg.jump_fraction => {
                *p = candidate;
                *t += dt;
                if *t >= self.cfg.jump_confirm_secs {
                    self.bpm = candidate;
                    self.pending = None;
                }
            }
            _ => self.pending = Some((candidate, 0.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FR: f32 = 48_000.0 / 512.0;

    fn cfg() -> TempoConfig {
        crate::AnalysisConfig::default().tempo
    }

    /// Impulses at `bpm` for `secs`, fed hop by hop.
    fn run(t: &mut TempoTracker, bpm: f32, secs: f32) {
        let period = 60.0 / bpm;
        let hops = (secs * FR) as usize;
        let mut next = 0.0f32;
        for i in 0..hops {
            let time = i as f32 / FR;
            let x = if time >= next {
                next += period;
                1.0
            } else {
                0.0
            };
            t.process(x, 0.0);
        }
    }

    #[test]
    fn finds_tempo_from_an_impulse_train() {
        for bpm in [80.0, 90.0, 100.0, 120.0, 128.0, 140.0, 160.0] {
            let mut t = TempoTracker::new(&cfg(), FR);
            run(&mut t, bpm, 12.0);
            assert!(t.locked());
            assert!((t.bpm() - bpm).abs() < 1.0, "{bpm}: got {}", t.bpm());
        }
    }

    #[test]
    fn very_fast_tempos_read_as_half_time() {
        // Known limitation of the 120-BPM-centred weighting (spec item 6):
        // 174 BPM drum & bass reads as 87. Tap tempo fixes it by hand.
        let mut t = TempoTracker::new(&cfg(), FR);
        run(&mut t, 174.0, 12.0);
        assert!((t.bpm() - 87.0).abs() < 1.0, "{}", t.bpm());
    }

    #[test]
    fn holds_tempo_in_silence() {
        let mut t = TempoTracker::new(&cfg(), FR);
        run(&mut t, 128.0, 10.0);
        let before = t.bpm();
        for _ in 0..(10.0 * FR) as usize {
            t.process(0.0, 5.0);
        }
        assert_eq!(t.bpm(), before);
    }

    #[test]
    fn tap_tempo_sets_bpm_and_manual_mode() {
        let mut t = TempoTracker::new(&cfg(), FR);
        assert_eq!(t.tap(10.0), None);
        for i in 1..6 {
            t.tap(10.0 + i as f64 * 0.6);
        }
        assert!((t.bpm() - 100.0).abs() < 0.01);
        assert!(t.manual() && t.locked());
        // A long pause starts a new tap sequence.
        assert_eq!(t.tap(30.0), None);
    }

    #[test]
    fn manual_tempo_is_only_fine_tuned() {
        let mut t = TempoTracker::new(&cfg(), FR);
        t.tap(0.0);
        t.tap(60.0 / 64.0); // tapped at half speed: 64 BPM
        run(&mut t, 128.0, 10.0);
        // Folded to the tapped octave and refined, never jumping to 128.
        assert!((t.bpm() - 64.0).abs() < 1.0, "{}", t.bpm());
    }
}
