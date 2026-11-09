//! Phase-locked beat clock (spec item 7).
//!
//! The clock keeps a continuous beat position `pos` (whole part = beat number,
//! fraction = phase φ). Every hop it advances by `BPM/60·Δt`; on each onset it
//! is nudged towards the nearest beat: `φ += k·wrap(φ_onset − φ)`. Bars and
//! phrases are counted from a downbeat offset that the user (resync key) or a
//! detected drop can set.

use crate::config::BeatConfig;

pub struct BeatClock {
    cfg: BeatConfig,
    /// Continuous beat position.
    pos: f64,
    /// Highest whole beat already announced (beats fire only going forward).
    last_beat: i64,
    /// Beat number that counts as beat 1 of a bar / bar 1 of a phrase.
    downbeat: i64,
    /// False until the first tempo lock, when the phase is snapped to the music.
    aligned: bool,
}

/// Wrap to `[-0.5, 0.5)`.
fn wrap(x: f64) -> f64 {
    x - (x + 0.5).floor()
}

impl BeatClock {
    pub fn new(cfg: &BeatConfig) -> Self {
        Self {
            cfg: cfg.clone(),
            pos: 0.0,
            last_beat: 0,
            downbeat: 0,
            aligned: false,
        }
    }

    pub fn aligned(&self) -> bool {
        self.aligned
    }

    /// Advance by `dt` seconds at `bpm`. Returns true if a new beat started.
    pub fn advance(&mut self, bpm: f32, dt: f32) -> bool {
        self.pos += bpm as f64 / 60.0 * dt as f64;
        let beat = self.pos.floor() as i64;
        if beat > self.last_beat {
            self.last_beat = beat;
            true
        } else {
            false
        }
    }

    /// Phase the clock had `ago` seconds in the past (at constant `bpm`).
    fn pos_at(&self, ago: f64, bpm: f32) -> f64 {
        self.pos - ago * bpm as f64 / 60.0
    }

    /// An onset happened `ago` seconds before now: pull towards its beat.
    pub fn nudge(&mut self, ago: f64, bpm: f32) {
        let p = self.pos_at(ago, bpm);
        let error = wrap(p.round() - p);
        if self.aligned {
            self.pos += self.cfg.nudge as f64 * error;
        } else {
            // First lock: jump straight onto the beat grid.
            self.pos += error;
            self.aligned = true;
        }
    }

    /// Make the moment `ago` seconds before now exactly a beat (tap tempo).
    pub fn set_beat(&mut self, ago: f64, bpm: f32) {
        let p = self.pos_at(ago, bpm);
        self.pos += wrap(p.round() - p);
        self.aligned = true;
    }

    /// Make the beat nearest to now beat 1 of bar 1 of a phrase.
    pub fn resync_downbeat(&mut self) {
        self.set_downbeat(self.pos.round() as i64);
    }

    /// Make the beat nearest to `ago` seconds ago the start of a phrase.
    pub fn set_phrase_start(&mut self, ago: f64, bpm: f32) {
        self.set_downbeat(self.pos_at(ago, bpm).round() as i64);
    }

    fn set_downbeat(&mut self, beat: i64) {
        self.downbeat = beat;
    }

    fn beats_since_downbeat(&self) -> f64 {
        self.pos - self.downbeat as f64
    }

    pub fn beat_phase(&self) -> f32 {
        self.pos.rem_euclid(1.0) as f32
    }

    pub fn beat_in_bar(&self) -> u32 {
        (self.beats_since_downbeat().floor() as i64).rem_euclid(self.cfg.beats_per_bar as i64)
            as u32
    }

    pub fn bar_phase(&self) -> f32 {
        let bpb = self.cfg.beats_per_bar as f64;
        (self.beats_since_downbeat().rem_euclid(bpb) / bpb) as f32
    }

    /// Bars since the downbeat reference (can be negative before it).
    pub fn bar_index(&self) -> i64 {
        (self.beats_since_downbeat() / self.cfg.beats_per_bar as f64).floor() as i64
    }

    /// Whole bars since the clock started (never reset by resyncs).
    pub fn bars_elapsed(&self) -> u64 {
        (self.pos / self.cfg.beats_per_bar as f64).floor().max(0.0) as u64
    }

    pub fn bar_in_phrase(&self) -> u32 {
        self.bar_index().rem_euclid(self.cfg.phrase_bars as i64) as u32
    }

    pub fn phrase_phase(&self) -> f32 {
        let beats = (self.cfg.beats_per_bar * self.cfg.phrase_bars) as f64;
        (self.beats_since_downbeat().rem_euclid(beats) / beats) as f32
    }

    pub fn phrase_bars(&self) -> u32 {
        self.cfg.phrase_bars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> BeatClock {
        BeatClock::new(&crate::AnalysisConfig::default().beat)
    }

    #[test]
    fn advances_and_counts_beats_and_bars() {
        let mut c = clock();
        let mut beats = 0;
        // 120 BPM for 4.1 s = 8.2 beats = 2 bars and a bit.
        for _ in 0..410 {
            if c.advance(120.0, 0.01) {
                beats += 1;
            }
        }
        assert_eq!(beats, 8);
        assert!((c.beat_phase() - 0.2).abs() < 1e-6);
        assert_eq!(c.bar_index(), 2);
        assert_eq!(c.beat_in_bar(), 0);
        assert!((c.phrase_phase() - 8.2 / 32.0).abs() < 1e-6);
    }

    #[test]
    fn onsets_pull_phase_onto_the_beat() {
        let mut c = clock();
        c.advance(120.0, 0.1); // phase 0.2, first onset aligns instantly
        c.nudge(0.0, 120.0);
        assert!(c.beat_phase() < 1e-6 || c.beat_phase() > 1.0 - 1e-6);
        // Now off by +0.1 beat: each nudge removes 10% of the error.
        c.advance(120.0, 0.05);
        let before = wrap(c.pos.round() - c.pos).abs();
        c.nudge(0.0, 120.0);
        let after = wrap(c.pos.round() - c.pos).abs();
        assert!((after - 0.9 * before).abs() < 1e-9);
    }

    #[test]
    fn resync_makes_nearest_beat_the_downbeat() {
        let mut c = clock();
        for _ in 0..(5.4 / 0.01) as usize {
            c.advance(120.0, 0.01); // 10.8 beats
        }
        c.resync_downbeat(); // nearest beat is 11
        assert_eq!(c.beat_in_bar(), 3); // 10.8 is just before beat 1 of the new bar
        assert_eq!(c.bar_in_phrase(), 7);
        c.advance(120.0, 0.2); // past beat 11
        assert_eq!(c.beat_in_bar(), 0);
        assert_eq!(c.bar_in_phrase(), 0);
    }

    #[test]
    fn wrap_is_symmetric() {
        assert!((wrap(0.7) + 0.3).abs() < 1e-12);
        assert!((wrap(-0.7) - 0.3).abs() < 1e-12);
        assert_eq!(wrap(0.0), 0.0);
    }
}
