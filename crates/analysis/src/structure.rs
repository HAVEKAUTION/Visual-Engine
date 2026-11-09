//! Song-structure detection (spec item 8), driven by the beat clock.
//!
//! - **build**: over the last `build_bars` bars, the linear-regression slope of
//!   slow loudness (in dB) is positive and large enough, and the onset rate is
//!   rising too.
//! - **drop**: right after a build, slow bass power jumps ≥ `drop_ratio`× on a
//!   beat (compared one beat later against the beat before). The spec says "at
//!   a phrase boundary"; we test every beat because our bar/phrase grid may not
//!   be aligned with the song yet, and a detected drop then *defines* the
//!   phrase boundary (the analyzer re-aligns the grid to it).
//! - **section_change**: cosine distance between the mean (log-mel) spectra of
//!   the last 4 bars and the 4 before exceeds a threshold.

use signals::MEL_BANDS;

use crate::config::{AttackRelease, StructureConfig};
use crate::smoothing::Smoother;

/// Events found at one beat.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StructureEvents {
    pub build_started: bool,
    /// Drop: (audio time of the downbeat it landed on, bass jump ratio).
    pub drop: Option<(f64, f32)>,
    /// Section change: cosine distance.
    pub section_change: Option<f32>,
}

/// Slope of the least-squares line through `y` (x = 0, 1, 2, ...).
pub fn regression_slope(y: impl ExactSizeIterator<Item = f32> + Clone) -> f32 {
    let n = y.len() as f32;
    if n < 2.0 {
        return 0.0;
    }
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = y.clone().sum::<f32>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (i, v) in y.enumerate() {
        let dx = i as f32 - mean_x;
        num += dx * (v - mean_y);
        den += dx * dx;
    }
    num / den
}

fn cosine_distance(a: &[f32; MEL_BANDS], b: &[f32; MEL_BANDS]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na < 1e-9 || nb < 1e-9 {
        return 0.0;
    }
    1.0 - dot / (na * nb)
}

pub struct StructureDetector {
    cfg: StructureConfig,
    log_gamma: f32,
    loud_db: Smoother,
    bass: Smoother,

    // Per-beat history for the build regression: (slow loudness dB, onsets).
    beat_loud: Vec<f32>,
    beat_onsets: Vec<f32>,
    beat_pos: usize,
    beats_filled: usize,
    onsets_this_beat: u32,
    beat_counter: i64,

    // Per-bar mean spectra (last 8 bars).
    bar_sum: [f32; MEL_BANDS],
    bar_hops: u32,
    bar_spectra: [[f32; MEL_BANDS]; 8],
    bars_filled: usize,
    bar_pos: usize,
    bar_counter: i64,

    pub building: bool,
    pub build_amount: f32,
    last_build_beat: Option<i64>,
    /// Slow bass and time at the last three beats (oldest first).
    bass_hist: [f32; 3],
    beat_times: [f64; 3],
    beats_seen: u32,
    last_drop_beat: Option<i64>,
    last_section_bar: Option<i64>,
}

impl StructureDetector {
    pub fn new(cfg: &StructureConfig, slow: AttackRelease, log_gamma: f32) -> Self {
        let beats = (cfg.build_bars * 4).max(4) as usize;
        Self {
            cfg: cfg.clone(),
            log_gamma,
            loud_db: Smoother::new(slow),
            bass: Smoother::new(slow),
            beat_loud: vec![0.0; beats],
            beat_onsets: vec![0.0; beats],
            beat_pos: 0,
            beats_filled: 0,
            onsets_this_beat: 0,
            beat_counter: 0,
            bar_sum: [0.0; MEL_BANDS],
            bar_hops: 0,
            bar_spectra: [[0.0; MEL_BANDS]; 8],
            bars_filled: 0,
            bar_pos: 0,
            bar_counter: 0,
            building: false,
            build_amount: 0.0,
            last_build_beat: None,
            bass_hist: [0.0; 3],
            beat_times: [0.0; 3],
            beats_seen: 0,
            last_drop_beat: None,
            last_section_bar: None,
        }
    }

    /// Every hop: accumulate loudness, bass and spectrum.
    pub fn on_hop(
        &mut self,
        loud_db: f32,
        bass_power: f32,
        mel_power: &[f32; MEL_BANDS],
        onset: bool,
        dt: f32,
    ) {
        // Smooth dB from a sane floor so silence doesn't dominate the slope.
        self.loud_db.update(loud_db.max(-90.0), dt);
        self.bass.update(bass_power, dt);
        for (s, &p) in self.bar_sum.iter_mut().zip(mel_power) {
            *s += (1.0 + self.log_gamma * p.sqrt()).ln();
        }
        self.bar_hops += 1;
        if onset {
            self.onsets_this_beat += 1;
        }
    }

    /// At every beat of the clock. `beat_in_bar == 0` is a downbeat.
    pub fn on_beat(&mut self, beat_in_bar: u32, now: f64) -> StructureEvents {
        let mut ev = StructureEvents::default();
        self.beat_counter += 1;

        // --- build ---
        let n = self.beat_loud.len();
        self.beat_loud[self.beat_pos] = self.loud_db.value;
        self.beat_onsets[self.beat_pos] = self.onsets_this_beat as f32;
        self.beat_pos = (self.beat_pos + 1) % n;
        self.beats_filled = (self.beats_filled + 1).min(n);
        self.onsets_this_beat = 0;
        if self.beats_filled == n {
            let order = (0..n).map(|i| (self.beat_pos + i) % n);
            let loud_slope = regression_slope(order.clone().map(|i| self.beat_loud[i]));
            let rate_slope = regression_slope(order.map(|i| self.beat_onsets[i]));
            let rise_db = loud_slope * n as f32;
            let was = self.building;
            // Right after a drop the window still contains the drop's jump in
            // loudness, which is not a build.
            let after_drop = self
                .last_drop_beat
                .is_some_and(|b| self.beat_counter - b < n as i64);
            self.building =
                !after_drop && rise_db >= self.cfg.build_min_rise_db && rate_slope > 0.0;
            self.build_amount = if self.building {
                (rise_db / (2.0 * self.cfg.build_min_rise_db)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if self.building {
                self.last_build_beat = Some(self.beat_counter);
                ev.build_started = !was;
            }
        }

        // --- drop: did the bass jump on the previous beat? Compare now (one
        // beat after it) with the beat before it.
        self.bass_hist = [self.bass_hist[1], self.bass_hist[2], self.bass.value];
        self.beat_times = [self.beat_times[1], self.beat_times[2], now];
        self.beats_seen = self.beats_seen.saturating_add(1);
        if self.beats_seen >= 3 {
            let recent_build = self.last_build_beat.is_some_and(|b| {
                self.beat_counter - b <= (self.cfg.drop_window_bars * 4) as i64 + 2
            });
            let ratio = self.bass_hist[2] / self.bass_hist[0].max(1e-12);
            let cooled = self
                .last_drop_beat
                .is_none_or(|b| self.beat_counter - b >= (self.cfg.cooldown_bars * 4) as i64);
            if recent_build && ratio >= self.cfg.drop_ratio && cooled {
                ev.drop = Some((self.beat_times[1], ratio));
                self.last_drop_beat = Some(self.beat_counter);
                self.last_build_beat = None;
                self.building = false;
                self.build_amount = 0.0;
            }
        }

        // --- bar boundary: spectra and section change ---
        if beat_in_bar == 0 {
            self.bar_counter += 1;

            if self.bar_hops > 0 {
                let mut mean = [0.0; MEL_BANDS];
                for (m, s) in mean.iter_mut().zip(&self.bar_sum) {
                    *m = s / self.bar_hops as f32;
                }
                self.bar_spectra[self.bar_pos] = mean;
                self.bar_pos = (self.bar_pos + 1) % 8;
                self.bars_filled = (self.bars_filled + 1).min(8);
            }
            self.bar_sum = [0.0; MEL_BANDS];
            self.bar_hops = 0;

            if self.bars_filled == 8 {
                let mut recent = [0.0; MEL_BANDS];
                let mut before = [0.0; MEL_BANDS];
                for i in 0..4 {
                    let r = &self.bar_spectra[(self.bar_pos + 4 + i) % 8];
                    let b = &self.bar_spectra[(self.bar_pos + i) % 8];
                    for k in 0..MEL_BANDS {
                        recent[k] += r[k] / 4.0;
                        before[k] += b[k] / 4.0;
                    }
                }
                let d = cosine_distance(&recent, &before);
                let cooled = self
                    .last_section_bar
                    .is_none_or(|b| self.bar_counter - b >= self.cfg.cooldown_bars as i64);
                if d > self.cfg.section_threshold && cooled {
                    ev.section_change = Some(d);
                    self.last_section_bar = Some(self.bar_counter);
                }
            }
        }

        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slope_of_a_line() {
        let y = [1.0f32, 3.0, 5.0, 7.0];
        assert!((regression_slope(y.iter().copied()) - 2.0).abs() < 1e-6);
        assert_eq!(regression_slope([4.0f32].iter().copied()), 0.0);
    }

    #[test]
    fn cosine_distance_basics() {
        let a = [1.0; MEL_BANDS];
        let mut b = [0.0; MEL_BANDS];
        b[0] = 1.0;
        assert!(cosine_distance(&a, &a).abs() < 1e-6);
        assert!(cosine_distance(&a, &b) > 0.5);
    }
}
