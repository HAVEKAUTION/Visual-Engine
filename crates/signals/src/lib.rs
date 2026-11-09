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
    fn band_names_and_indices_line_up() {
        for (i, b) in Band::ALL.iter().enumerate() {
            assert_eq!(b.index(), i);
        }
        assert_eq!(Band::Air.name(), "air");
    }
}
