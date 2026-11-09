//! Onset detection with an adaptive threshold (spec item 5).
//!
//! Each hop supplies a spectral-flux value. The threshold is
//! `δ = λ·median(last M flux values) + κ`; a hop is an onset when its flux is a
//! local peak above δ and at least `min_interval` after the previous onset.
//! Deciding "local peak" needs the next hop, so onsets are reported one hop late
//! (with the correct, earlier time).

use crate::config::OnsetConfig;

/// Spectral flux of one hop: sum over the bins in `range` of
/// `max(0, Y_n[k] − max(Y_{n−1}[k−r ..= k+r]))`, with `r = max_filter`.
///
/// With `r = 0` this is exactly the spec's `Σ max(0, Y_n[k] − Y_{n−1}[k])`.
pub fn spectral_flux(
    current: &[f32],
    previous: &[f32],
    range: std::ops::Range<usize>,
    max_filter: usize,
) -> f32 {
    if range.is_empty() {
        return 0.0;
    }
    let last = previous.len().saturating_sub(1);
    let mut sum = 0.0;
    for k in range.clone() {
        let lo = k.saturating_sub(max_filter);
        let hi = (k + max_filter).min(last);
        let reference = previous[lo..=hi]
            .iter()
            .fold(f32::NEG_INFINITY, |m, &v| m.max(v));
        sum += (current[k] - reference).max(0.0);
    }
    sum
}

pub struct OnsetDetector {
    lambda: f32,
    kappa: f32,
    min_interval: f64,
    /// Ring of the last M flux values, and scratch space for the median.
    history: Vec<f32>,
    scratch: Vec<f32>,
    pos: usize,
    filled: usize,
    /// Flux and threshold of the two previous hops.
    prev: f32,
    prev_threshold: f32,
    prev_prev: f32,
    /// Time of the last reported onset.
    last_onset: f64,
    /// `max(0, flux − median)` of the latest hop: a cleaned-up onset signal.
    novelty: f32,
}

/// An onset found by [`OnsetDetector::process`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Onset {
    /// Hop index the onset belongs to (the previous hop).
    pub hops_ago: u32,
    /// Flux divided by the threshold (> 1).
    pub strength: f32,
}

impl OnsetDetector {
    pub fn new(cfg: &OnsetConfig, hop_seconds: f32) -> Self {
        let m = ((cfg.median_secs / hop_seconds).round() as usize).max(3);
        Self {
            lambda: cfg.lambda,
            kappa: cfg.kappa,
            min_interval: cfg.min_interval as f64,
            history: vec![0.0; m],
            scratch: vec![0.0; m],
            pos: 0,
            filled: 0,
            prev: 0.0,
            prev_threshold: f32::INFINITY,
            prev_prev: 0.0,
            last_onset: f64::NEG_INFINITY,
            novelty: 0.0,
        }
    }

    /// The latest `max(0, flux − median)`.
    pub fn novelty(&self) -> f32 {
        self.novelty
    }

    /// Feed the flux of the current hop. `prev_time` is the audio time of the
    /// previous hop; if that hop was an onset it is returned. No allocation.
    pub fn process(&mut self, flux: f32, prev_time: f64) -> Option<Onset> {
        // Running median of the last M values (including this one).
        self.history[self.pos] = flux;
        self.pos = (self.pos + 1) % self.history.len();
        self.filled = (self.filled + 1).min(self.history.len());
        let window = &mut self.scratch[..self.filled];
        window.copy_from_slice(&self.history[..self.filled]);
        let mid = self.filled / 2;
        let (_, median, _) = window.select_nth_unstable_by(mid, f32::total_cmp);
        let median = *median;
        let threshold = self.lambda * median + self.kappa;
        self.novelty = (flux - median).max(0.0);

        // Was the previous hop a peak above its threshold?
        let p = self.prev;
        let is_onset = p > self.prev_prev
            && p >= flux
            && p > self.prev_threshold
            && prev_time - self.last_onset >= self.min_interval;
        let result = if is_onset {
            self.last_onset = prev_time;
            Some(Onset {
                hops_ago: 1,
                strength: p / self.prev_threshold,
            })
        } else {
            None
        };

        self.prev_prev = p;
        self.prev = flux;
        self.prev_threshold = threshold;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OnsetConfig {
        crate::AnalysisConfig::default().onsets
    }

    #[test]
    fn flux_counts_only_increases() {
        let (cur, prev) = ([1.0, 2.0, 3.0], [2.0, 1.0, 3.0]);
        assert_eq!(spectral_flux(&cur, &prev, 0..3, 0), 1.0);
        assert_eq!(spectral_flux(&cur, &prev, 1..1, 0), 0.0);
    }

    #[test]
    fn max_filter_ignores_a_peak_sliding_one_bin() {
        // A peak moves from bin 2 to bin 1: plain flux sees an increase, the
        // max-filtered version does not.
        let prev = [0.0, 0.0, 5.0, 0.0];
        let cur = [0.0, 5.0, 0.0, 0.0];
        assert!(spectral_flux(&cur, &prev, 0..4, 0) > 1.0);
        assert_eq!(spectral_flux(&cur, &prev, 0..4, 1), 0.0);
    }

    #[test]
    fn isolated_peaks_are_onsets_and_flat_input_is_not() {
        let mut d = OnsetDetector::new(&cfg(), 0.01);
        let mut found = Vec::new();
        for i in 0..200 {
            let flux = if i % 50 == 10 { 1.0 } else { 0.001 };
            if let Some(o) = d.process(flux, (i - 1) as f64 * 0.01) {
                found.push(i - 1);
                assert!(o.strength > 1.0);
            }
        }
        assert_eq!(found, vec![10, 60, 110, 160]);
    }

    #[test]
    fn minimum_interval_is_respected() {
        let mut d = OnsetDetector::new(&cfg(), 0.01);
        let mut found = 0;
        // Peaks every 3 hops = 30 ms apart, closer than the 50 ms minimum.
        for i in 0..100 {
            let flux = if i % 3 == 0 { 1.0 } else { 0.0 };
            if d.process(flux, (i - 1) as f64 * 0.01).is_some() {
                found += 1;
            }
        }
        assert!(found <= 100 / 5 + 1, "{found}");
    }
}
