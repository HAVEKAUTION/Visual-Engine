//! Every tunable constant of the analysis, in one place.

/// Named band edges in Hz, in [`signals::Band::ALL`] order.
pub const NAMED_BAND_EDGES: [(f32, f32); signals::Band::COUNT] = [
    (20.0, 60.0),        // sub
    (60.0, 250.0),       // bass
    (250.0, 500.0),      // low_mid
    (500.0, 2_000.0),    // mid
    (2_000.0, 6_000.0),  // high_mid
    (6_000.0, 16_000.0), // air
];

/// Attack/release time constants in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackRelease {
    pub attack: f32,
    pub release: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisConfig {
    /// Input sample rate (Hz). The audio crate always delivers 48 kHz.
    pub sample_rate: u32,
    /// FFT window length N.
    pub fft_size: usize,
    /// Hop H between analysis frames.
    pub hop_size: usize,
    /// Log-compression strength γ in `Y = ln(1 + γ|X|)`.
    pub log_gamma: f32,
    /// Lowest/highest frequency covered by the mel bands (Hz).
    pub mel_min_hz: f32,
    pub mel_max_hz: f32,
    /// Named band edges (Hz).
    pub named_bands: [(f32, f32); signals::Band::COUNT],
    /// Auto-gain running-mean time constant (s).
    pub auto_gain_tau: f32,
    /// Auto-gain ε: mean *bin* power treated as silence (a full-scale sine's
    /// peak bin is ~1). Must sit well below quiet broadband music (~1e-8).
    pub auto_gain_epsilon: f32,
    /// Normalised levels are clamped to `[0, level_max]`.
    pub level_max: f32,
    pub fast: AttackRelease,
    pub slow: AttackRelease,
    /// Centroid → hue mapping range (Hz).
    pub centroid_min_hz: f32,
    pub centroid_max_hz: f32,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            fft_size: 2048,
            hop_size: 512,
            log_gamma: 100.0,
            mel_min_hz: 20.0,
            mel_max_hz: 16_000.0,
            named_bands: NAMED_BAND_EDGES,
            auto_gain_tau: 8.0,
            auto_gain_epsilon: 1e-10,
            level_max: 8.0,
            fast: AttackRelease {
                attack: 0.005,
                release: 0.120,
            },
            slow: AttackRelease {
                attack: 0.300,
                release: 1.5,
            },
            centroid_min_hz: 100.0,
            centroid_max_hz: 8_000.0,
        }
    }
}

impl AnalysisConfig {
    /// Seconds between hops (H / fs).
    pub fn hop_seconds(&self) -> f32 {
        self.hop_size as f32 / self.sample_rate as f32
    }

    /// Frequency of FFT bin `k` in Hz.
    pub fn bin_hz(&self, k: usize) -> f32 {
        k as f32 * self.sample_rate as f32 / self.fft_size as f32
    }
}
