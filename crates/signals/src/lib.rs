//! The Signal Bus: named, normalised signals produced by the analysis and read
//! by the UI and renderer.
//!
//! Convention: every `level`/`fast`/`slow` value is normalised so that **1.0 is
//! typical for the current song** (0 = silent, clamped to a sane maximum).
//!
//! The analysis thread owns a [`SignalPublisher`]; any number of frames per
//! second can be published. The UI/render thread owns a [`SignalReader`] and
//! always gets the newest complete [`SignalFrame`] without waiting (triple
//! buffer: neither side ever blocks the other).

/// Number of mel-spaced bands.
pub const MEL_BANDS: usize = 32;

/// The six named frequency bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Band {
    Sub,
    Bass,
    LowMid,
    Mid,
    HighMid,
    Air,
}

impl Band {
    pub const COUNT: usize = 6;
    pub const ALL: [Band; Band::COUNT] = [
        Band::Sub,
        Band::Bass,
        Band::LowMid,
        Band::Mid,
        Band::HighMid,
        Band::Air,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Band::Sub => "sub",
            Band::Bass => "bass",
            Band::LowMid => "low_mid",
            Band::Mid => "mid",
            Band::HighMid => "high_mid",
            Band::Air => "air",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// One normalised signal with its smoothed versions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BandSignal {
    /// Auto-gained level: `E / (running mean + ε)`, 1.0 = typical.
    pub level: f32,
    /// `level` with fast attack/release (punchy).
    pub fast: f32,
    /// `level` with slow attack/release (the trend).
    pub slow: f32,
    /// Un-normalised energy in dB (for debugging/meters only).
    pub energy_db: f32,
}

/// A snapshot of every signal at one analysis hop.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SignalFrame {
    /// Hops processed since the source started.
    pub hop_index: u64,
    /// Audio time of this frame since the source started, in seconds.
    pub time: f64,
    /// Seconds between hops.
    pub dt: f32,
    /// Mel bands, low to high: fast-smoothed normalised levels.
    pub mel: [f32; MEL_BANDS],
    /// Named bands, indexed by [`Band::index`].
    pub bands: [BandSignal; Band::COUNT],
    /// Overall loudness (energy_db here is the frame RMS in dBFS).
    pub loudness: BandSignal,
    /// Spectral centroid in Hz.
    pub centroid_hz: f32,
    /// Centroid mapped to `[0, 1]` on a log scale (use as a hue).
    pub centroid_hue: f32,
    /// Onsets, tempo and the beat clock.
    pub rhythm: Rhythm,
    /// Build / drop / section-change detection.
    pub structure: Structure,
}

/// A momentary event (onset, drop, ...).
///
/// The UI reads frames at its own rate and can miss the single hop where
/// `fired` is true, so it should watch `count` instead: any change means the
/// event happened at least once since the last look.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trigger {
    /// True only on the hop where the event was detected.
    pub fired: bool,
    /// How many times it has happened since the source started.
    pub count: u32,
    /// Audio time (s) the most recent one happened at.
    pub last_time: f64,
    /// Detection strength of the most recent one (how far above threshold, ≥ 1).
    pub strength: f32,
}

impl Trigger {
    /// Record an event at `time`.
    pub fn fire(&mut self, time: f64, strength: f32) {
        self.fired = true;
        self.count = self.count.wrapping_add(1);
        self.last_time = time;
        self.strength = strength;
    }
}

/// Rhythm signals (spec items 5–7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rhythm {
    /// Any onset (full-spectrum flux).
    pub onset: Trigger,
    /// Low-end onsets (sub + bass).
    pub kick: Trigger,
    /// Mid onsets (low_mid + mid).
    pub snare: Trigger,
    /// High onsets (air).
    pub hat: Trigger,
    /// Onsets per second over the last few seconds.
    pub onset_rate: f32,

    /// Current tempo. 120 until the first estimate.
    pub bpm: f32,
    /// How periodic the onsets are, 0–1.
    pub tempo_confidence: f32,
    /// A tempo has been found (or tapped); false at start and with no music yet.
    pub tempo_locked: bool,
    /// The tempo comes from tap tempo (auto-tracking only fine-tunes it).
    pub tempo_manual: bool,

    /// Position within the current beat, 0–1 (0 = on the beat).
    pub beat_phase: f32,
    /// Position within the current 4-beat bar, 0–1.
    pub bar_phase: f32,
    /// Position within the current phrase, 0–1.
    pub phrase_phase: f32,
    /// Fires on every beat of the clock.
    pub beat: Trigger,
    /// 0–3.
    pub beat_in_bar: u32,
    /// 0 .. phrase_bars-1.
    pub bar_in_phrase: u32,
    /// Bars per phrase (8 by default).
    pub phrase_bars: u32,
    /// Bars since the source started.
    pub bar_count: u64,
}

/// Song-structure signals (spec item 8).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Structure {
    /// A build-up is in progress.
    pub building: bool,
    /// How strongly it is building, 0–1.
    pub build_amount: f32,
    /// Fires when a build starts.
    pub build: Trigger,
    /// Fires on a drop.
    pub drop: Trigger,
    /// Fires when the sound of the song changes markedly.
    pub section_change: Trigger,
}

impl SignalFrame {
    pub fn band(&self, band: Band) -> &BandSignal {
        &self.bands[band.index()]
    }
}

/// Writing end of the bus (analysis thread).
pub struct SignalPublisher(triple_buffer::Input<SignalFrame>);

/// Reading end of the bus (UI / render thread).
pub struct SignalReader(triple_buffer::Output<SignalFrame>);

/// Create a connected publisher/reader pair.
pub fn bus() -> (SignalPublisher, SignalReader) {
    let (input, output) = triple_buffer::triple_buffer(&SignalFrame::default());
    (SignalPublisher(input), SignalReader(output))
}

impl SignalPublisher {
    /// Publish a frame. Never blocks; copies into a pre-allocated slot.
    pub fn publish(&mut self, frame: &SignalFrame) {
        self.0.input_buffer_mut().clone_from(frame);
        self.0.publish();
    }
}

impl SignalReader {
    /// The newest published frame. Never blocks.
    pub fn latest(&mut self) -> &SignalFrame {
        self.0.read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_sees_latest_frame() {
        let (mut tx, mut rx) = bus();
        assert_eq!(rx.latest().hop_index, 0);
        for i in 1..=3 {
            let f = SignalFrame {
                hop_index: i,
                ..Default::default()
            };
            tx.publish(&f);
        }
        assert_eq!(rx.latest().hop_index, 3);
    }

    #[test]
    fn trigger_counts_and_remembers() {
        let mut t = Trigger::default();
        t.fire(1.5, 2.0);
        t.fire(2.5, 3.0);
        assert!(t.fired);
        assert_eq!(t.count, 2);
        assert_eq!(t.last_time, 2.5);
        assert_eq!(t.strength, 3.0);
    }

    #[test]
    fn band_names_and_indices_line_up() {
        for (i, b) in Band::ALL.iter().enumerate() {
            assert_eq!(b.index(), i);
        }
        assert_eq!(Band::Air.name(), "air");
    }
}
