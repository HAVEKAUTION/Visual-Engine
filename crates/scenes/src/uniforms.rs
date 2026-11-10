//! The signal uniform block every scene receives as `audio`.
//!
//! All values are `f32` scalars (named in [`SIGNAL_NAMES`], in this order),
//! followed by the 32 mel bands packed as `array<vec4<f32>, 8>`. The WGSL
//! struct is generated from the same list, so the two can't drift apart.

use signals::{Band, SignalFrame, Trigger};

/// Every scalar in the signal block, in memory order.
pub const SIGNAL_NAMES: &[&str] = &[
    // Time and output.
    "time",
    "delta",
    "frame",
    "res_x",
    "res_y",
    "aspect",
    // Overall.
    "loudness",
    "loudness_fast",
    "loudness_slow",
    "centroid_hz",
    "centroid_hue",
    "centroid_hue_slow",
    // Named bands: level, fast, slow (same order as `signals::Band`).
    "sub",
    "sub_fast",
    "sub_slow",
    "bass",
    "bass_fast",
    "bass_slow",
    "low_mid",
    "low_mid_fast",
    "low_mid_slow",
    "mid",
    "mid_fast",
    "mid_slow",
    "high_mid",
    "high_mid_fast",
    "high_mid_slow",
    "air",
    "air_fast",
    "air_slow",
    // Onsets: seconds since the last one, how many so far, strength.
    "onset_age",
    "onset_count",
    "onset_strength",
    "kick_age",
    "kick_count",
    "kick_strength",
    "snare_age",
    "snare_count",
    "snare_strength",
    "hat_age",
    "hat_count",
    "hat_strength",
    "onset_rate",
    // Tempo and beat clock.
    "bpm",
    "tempo_confidence",
    "tempo_locked",
    "beat_phase",
    "bar_phase",
    "phrase_phase",
    "beat_in_bar",
    "bar_in_phrase",
    "phrase_bars",
    "bar_count",
    "beat_age",
    "beat_count",
    "beats",
    // Structure.
    "building",
    "build_amount",
    "build_age",
    "drop_age",
    "drop_count",
    "section_age",
    "section_count",
    // Built-in audio clocks (integrated speeds; see the "drive speed" rule).
    "clock_energy",
    "clock_bass",
    "clock_high",
    "clock_kick",
    // Video (from the first Image / Video / Camera node that feeds them;
    // see `media::VideoSignals`). Kept last so saved choices don't move.
    "motion",
    "motion_x",
    "motion_y",
    "brightness",
    "video_hue",
];

/// Number of scalars, rounded up to a multiple of 4 (the mel array that
/// follows must start on a 16-byte boundary).
pub const SCALARS_PADDED: usize = SIGNAL_NAMES.len().div_ceil(4) * 4;
/// Where the palette (5 × vec4) starts.
pub const PALETTE_OFFSET: usize = SCALARS_PADDED + signals::MEL_BANDS;
/// Floats in the whole block.
pub const BLOCK_FLOATS: usize = PALETTE_OFFSET + 4 * crate::palette::PALETTE_SIZE;

/// Age reported for events that have not happened yet (s).
pub const NEVER: f32 = 1.0e4;

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Index of a signal name; a compile-time error when used in `const` and missing.
pub const fn idx(name: &str) -> usize {
    let mut i = 0;
    while i < SIGNAL_NAMES.len() {
        if str_eq(SIGNAL_NAMES[i], name) {
            return i;
        }
        i += 1;
    }
    panic!("unknown signal name");
}

/// Index of a signal name, at run time.
pub fn signal_index(name: &str) -> Option<usize> {
    SIGNAL_NAMES.iter().position(|n| *n == name)
}

/// The WGSL declaration of the `Signals` struct.
pub fn wgsl_struct() -> String {
    let mut s = String::from("struct Signals {\n");
    for n in SIGNAL_NAMES {
        s += &format!("    {n}: f32,\n");
    }
    for i in SIGNAL_NAMES.len()..SCALARS_PADDED {
        s += &format!("    _pad{i}: f32,\n");
    }
    s += &format!("    mel: array<vec4<f32>, {}>,\n", signals::MEL_BANDS / 4);
    s += &format!(
        "    palette: array<vec4<f32>, {}>,\n}}\n",
        crate::palette::PALETTE_SIZE
    );
    s
}

/// Turns analysis frames (which arrive at ~94 Hz) into the per-render-frame
/// uniform block: event ages in render time, smoothed hue and the built-in
/// clocks.
#[derive(Debug, Clone)]
pub struct SignalState {
    pub block: [f32; BLOCK_FLOATS],
    seen: [u32; 8],
    hit_time: [f64; 8],
    last_hop: u64,
    time: f64,
    frame: u64,
}

impl Default for SignalState {
    fn default() -> Self {
        Self::new()
    }
}

fn smooth(current: f32, target: f32, dt: f32, tau: f32) -> f32 {
    current + (1.0 - (-dt / tau).exp()) * (target - current)
}

impl SignalState {
    pub fn new() -> Self {
        Self {
            block: [0.0; BLOCK_FLOATS],
            seen: [0; 8],
            hit_time: [f64::NEG_INFINITY; 8],
            last_hop: 0,
            time: 0.0,
            frame: 0,
        }
    }

    pub fn get(&self, name: &str) -> Option<f32> {
        signal_index(name).map(|i| self.block[i])
    }

    /// Advance by `dt` seconds of render time and copy in `f`.
    pub fn update(&mut self, f: &SignalFrame, dt: f32, resolution: [f32; 2]) {
        let dt = dt.clamp(0.0, 0.25);
        self.time += dt as f64;
        self.frame += 1;
        let b = &mut self.block;
        macro_rules! set {
            ($name:literal, $v:expr) => {
                b[const { idx($name) }] = $v
            };
        }

        set!("time", self.time as f32);
        set!("delta", dt);
        set!("frame", self.frame as f32);
        set!("res_x", resolution[0]);
        set!("res_y", resolution[1]);
        set!("aspect", resolution[0] / resolution[1].max(1.0));

        set!("loudness", f.loudness.level);
        set!("loudness_fast", f.loudness.fast);
        set!("loudness_slow", f.loudness.slow);
        set!("centroid_hz", f.centroid_hz);
        set!("centroid_hue", f.centroid_hue);
        let hue = b[const { idx("centroid_hue_slow") }];
        set!("centroid_hue_slow", smooth(hue, f.centroid_hue, dt, 0.6));

        let base = const { idx("sub") };
        for band in Band::ALL {
            let s = &f.bands[band.index()];
            let i = base + 3 * band.index();
            b[i] = s.level;
            b[i + 1] = s.fast;
            b[i + 2] = s.slow;
        }

        // Event ages are measured in render time from when the UI first saw the
        // count change, so envelopes keep decaying smoothly (and stop when the
        // music stops, instead of freezing mid-flash).
        if f.hop_index < self.last_hop {
            // New source: counts restarted.
            self.seen = [0; 8];
        }
        self.last_hop = f.hop_index;
        let r = &f.rhythm;
        let s = &f.structure;
        let triggers: [&Trigger; 8] = [
            &r.onset,
            &r.kick,
            &r.snare,
            &r.hat,
            &r.beat,
            &s.build,
            &s.drop,
            &s.section_change,
        ];
        let mut ages = [NEVER; 8];
        for (i, t) in triggers.iter().enumerate() {
            if t.count > self.seen[i] {
                self.hit_time[i] = self.time;
            }
            self.seen[i] = t.count;
            if self.hit_time[i].is_finite() {
                ages[i] = ((self.time - self.hit_time[i]) as f32).min(NEVER);
            }
        }
        let base = const { idx("onset_age") };
        for i in 0..4 {
            b[base + 3 * i] = ages[i];
            b[base + 3 * i + 1] = triggers[i].count as f32;
            b[base + 3 * i + 2] = triggers[i].strength;
        }
        set!("onset_rate", r.onset_rate);

        set!("bpm", r.bpm);
        set!("tempo_confidence", r.tempo_confidence);
        set!("tempo_locked", r.tempo_locked as u8 as f32);
        set!("beat_phase", r.beat_phase);
        set!("bar_phase", r.bar_phase);
        set!("phrase_phase", r.phrase_phase);
        set!("beat_in_bar", r.beat_in_bar as f32);
        set!("bar_in_phrase", r.bar_in_phrase as f32);
        set!("phrase_bars", r.phrase_bars as f32);
        set!("bar_count", r.bar_count as f32);
        set!("beat_age", ages[4]);
        set!("beat_count", r.beat.count as f32);
        set!("beats", r.beat.count as f32 + r.beat_phase);

        set!("building", s.building as u8 as f32);
        set!("build_amount", s.build_amount);
        set!("build_age", ages[5]);
        set!("drop_age", ages[6]);
        set!("drop_count", s.drop.count as f32);
        set!("section_age", ages[7]);
        set!("section_count", s.section_change.count as f32);

        let high = 0.5 * (f.bands[Band::HighMid.index()].slow + f.bands[Band::Air.index()].slow);
        let kick_env = (-ages[1] / 0.2).exp();
        b[const { idx("clock_energy") }] += f.loudness.slow * dt;
        b[const { idx("clock_bass") }] += f.bands[Band::Bass.index()].slow * dt;
        b[const { idx("clock_high") }] += high * dt;
        b[const { idx("clock_kick") }] += kick_env * dt;

        b[SCALARS_PADDED..PALETTE_OFFSET].copy_from_slice(&f.mel);
    }

    /// Set the video signals (motion, motion_x, motion_y, brightness,
    /// video_hue).
    pub fn set_video(&mut self, v: [f32; 5]) {
        let at = const { idx("motion") };
        self.block[at..at + 5].copy_from_slice(&v);
    }

    /// Set the five palette colours scenes see as `pal_color(0..4)`.
    pub fn set_palette(&mut self, p: &crate::palette::Palette) {
        for (i, c) in p.iter().enumerate() {
            let at = PALETTE_OFFSET + 4 * i;
            self.block[at..at + 3].copy_from_slice(c);
            self.block[at + 3] = 1.0;
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        // f32 has no padding and any bit pattern is valid.
        unsafe {
            std::slice::from_raw_parts(
                self.block.as_ptr() as *const u8,
                std::mem::size_of_val(&self.block),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_valid() {
        for (i, a) in SIGNAL_NAMES.iter().enumerate() {
            assert!(a.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
            assert!(!SIGNAL_NAMES[i + 1..].contains(a), "{a} twice");
        }
        assert_eq!(SCALARS_PADDED % 4, 0);
    }

    #[test]
    fn copies_signals_by_name() {
        let mut f = SignalFrame::default();
        f.bands[Band::Bass.index()].slow = 2.0;
        f.bands[Band::Air.index()].fast = 0.5;
        f.rhythm.bpm = 128.0;
        f.mel[5] = 0.75;
        let mut s = SignalState::new();
        s.update(&f, 0.25, [640.0, 360.0]);
        assert_eq!(s.get("bass_slow"), Some(2.0));
        assert_eq!(s.get("air_fast"), Some(0.5));
        assert_eq!(s.get("bpm"), Some(128.0));
        assert_eq!(s.get("res_x"), Some(640.0));
        assert_eq!(s.block[SCALARS_PADDED + 5], 0.75);
        // Bass clock integrates bass_slow over time: 2.0 * 0.25 s.
        assert!((s.get("clock_bass").unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(s.get("kick_age"), Some(NEVER));
    }

    #[test]
    fn event_ages_count_up_from_each_hit() {
        let mut f = SignalFrame::default();
        let mut s = SignalState::new();
        s.update(&f, 0.1, [1.0, 1.0]);
        f.hop_index = 10;
        f.rhythm.kick.fire(1.0, 2.0);
        s.update(&f, 0.1, [1.0, 1.0]);
        assert_eq!(s.get("kick_age"), Some(0.0));
        assert_eq!(s.get("kick_count"), Some(1.0));
        f.hop_index = 20;
        s.update(&f, 0.1, [1.0, 1.0]);
        s.update(&f, 0.1, [1.0, 1.0]);
        assert!((s.get("kick_age").unwrap() - 0.2).abs() < 1e-5);
        // A new source restarts the counts without a false hit.
        let fresh = SignalFrame::default();
        s.update(&fresh, 0.1, [1.0, 1.0]);
        assert!((s.get("kick_age").unwrap() - 0.3).abs() < 1e-5);
    }
}
