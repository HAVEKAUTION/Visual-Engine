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
    pub onsets: OnsetConfig,
    pub tempo: TempoConfig,
    pub beat: BeatConfig,
    pub structure: StructureConfig,
}

/// Spectral-flux onset detection (spec item 5).
#[derive(Clone, Debug, PartialEq)]
pub struct OnsetConfig {
    /// λ: threshold multiplier on the running median.
    pub lambda: f32,
    /// κ: threshold offset (in flux units: summed `Y` increases). Guards
    /// against near-silence; the median term handles background noise.
    pub kappa: f32,
    /// Length of the median window (s): M = this / hop time.
    pub median_secs: f32,
    /// Minimum time between two onsets of the same kind (s).
    pub min_interval: f32,
    /// How far before the end of the analysis window an onset really happened
    /// (s). Compensates the window/peak-picking delay so onset times line up
    /// with the audio.
    pub time_offset: f32,
    /// Window over which `onset_rate` is counted (s).
    pub rate_window: f32,
    /// Compare each bin with the max of its ±this-many neighbours in the
    /// previous frame ("SuperFlux"). Stops a sliding pitch (e.g. a kick drum's
    /// falling sweep) from re-triggering. 0 = the plain spec formula.
    pub max_filter_bins: usize,
}

/// Tempo estimation (spec item 6).
#[derive(Clone, Debug, PartialEq)]
pub struct TempoConfig {
    /// Autocorrelation window (s).
    pub window_secs: f32,
    /// Seconds of audio needed before the first estimate.
    pub min_history_secs: f32,
    /// Re-estimate every this many hops.
    pub update_hops: usize,
    pub min_bpm: f32,
    pub max_bpm: f32,
    /// Centre of the log-Gaussian lag weighting (BPM).
    pub prior_bpm: f32,
    /// σ of the weighting, in octaves.
    pub prior_sigma: f32,
    /// Smoothing time constant for small tempo changes (s).
    pub smoothing_secs: f32,
    /// Changes bigger than this fraction count as a tempo jump...
    pub jump_fraction: f32,
    /// ...which is only accepted after it has been stable this long (s).
    pub jump_confirm_secs: f32,
    /// No estimate is made if the last onset is older than this (s): hold in silence.
    pub hold_after_silence_secs: f32,
    /// Taps further apart than this start a new tap sequence (s).
    pub tap_timeout_secs: f32,
}

/// Phase-locked beat clock (spec item 7).
#[derive(Clone, Debug, PartialEq)]
pub struct BeatConfig {
    /// k: how strongly each onset pulls the phase.
    pub nudge: f32,
    pub beats_per_bar: u32,
    /// Bars per phrase (8, 16 or 32).
    pub phrase_bars: u32,
    /// Use full-spectrum onsets for phase when no kick has been seen for this long (s).
    pub kick_fallback_secs: f32,
}

/// Build / drop / section change (spec item 8).
#[derive(Clone, Debug, PartialEq)]
pub struct StructureConfig {
    /// Bars used for the build regression.
    pub build_bars: u32,
    /// Minimum loudness rise over `build_bars` to call it a build (dB).
    pub build_min_rise_db: f32,
    /// A drop must follow a build within this many bars.
    pub drop_window_bars: u32,
    /// Slow bass must jump by at least this power ratio.
    pub drop_ratio: f32,
    /// Section change when the cosine distance between the mean spectra of the
    /// last 4 bars and the 4 before exceeds this.
    pub section_threshold: f32,
    /// Bars to wait after an event before another of the same kind.
    pub cooldown_bars: u32,
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
            onsets: OnsetConfig {
                lambda: 1.5,
                kappa: 0.1,
                median_secs: 0.5,
                min_interval: 0.05,
                time_offset: 0.012,
                rate_window: 4.0,
                max_filter_bins: 1,
            },
            tempo: TempoConfig {
                window_secs: 6.0,
                min_history_secs: 3.0,
                update_hops: 8,
                min_bpm: 60.0,
                max_bpm: 200.0,
                prior_bpm: 120.0,
                prior_sigma: 1.0,
                smoothing_secs: 1.5,
                jump_fraction: 0.04,
                jump_confirm_secs: 3.0,
                hold_after_silence_secs: 2.0,
                tap_timeout_secs: 2.0,
            },
            beat: BeatConfig {
                nudge: 0.1,
                beats_per_bar: 4,
                phrase_bars: 8,
                kick_fallback_secs: 4.0,
            },
            structure: StructureConfig {
                build_bars: 8,
                build_min_rise_db: 2.0,
                drop_window_bars: 2,
                drop_ratio: 2.0,
                section_threshold: 0.05,
                cooldown_bars: 4,
            },
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
