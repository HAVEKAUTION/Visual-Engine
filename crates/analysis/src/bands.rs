//! Frequency band layout (spec item 2): which FFT bins belong to which band.

use std::ops::Range;

use crate::config::AnalysisConfig;

/// `mel(f) = 2595·log10(1 + f/700)`
pub fn hz_to_mel(f: f32) -> f32 {
    2595.0 * (1.0 + f / 700.0).log10()
}

pub fn mel_to_hz(m: f32) -> f32 {
    700.0 * (10f32.powf(m / 2595.0) - 1.0)
}

/// `count + 1` band edges in Hz, equally spaced on the mel scale.
pub fn mel_edges(min_hz: f32, max_hz: f32, count: usize) -> Vec<f32> {
    let (lo, hi) = (hz_to_mel(min_hz), hz_to_mel(max_hz));
    (0..=count)
        .map(|i| mel_to_hz(lo + (hi - lo) * i as f32 / count as f32))
        .collect()
}

/// Bins whose centre frequency lies in `[lo_hz, hi_hz)`. A band too narrow to
/// contain any bin gets the single bin nearest its centre, so it is never empty.
pub fn bins_for(cfg: &AnalysisConfig, lo_hz: f32, hi_hz: f32) -> Range<usize> {
    let nyquist_bin = cfg.fft_size / 2;
    let per_bin = cfg.bin_hz(1);
    let start = ((lo_hz / per_bin).ceil() as usize).clamp(1, nyquist_bin);
    let end = ((hi_hz / per_bin).ceil() as usize).clamp(1, nyquist_bin + 1);
    if end > start {
        start..end
    } else {
        let centre = (((lo_hz + hi_hz) * 0.5 / per_bin).round() as usize).clamp(1, nyquist_bin);
        centre..centre + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_round_trips() {
        for f in [20.0, 440.0, 1000.0, 16_000.0] {
            assert!((mel_to_hz(hz_to_mel(f)) - f).abs() < 0.05 * f.max(1.0) / 100.0);
        }
        assert!((hz_to_mel(1000.0) - 1000.0).abs() < 1.0); // classic anchor
    }

    #[test]
    fn mel_edges_are_increasing_and_span_the_range() {
        let e = mel_edges(20.0, 16_000.0, 32);
        assert_eq!(e.len(), 33);
        assert!((e[0] - 20.0).abs() < 0.01);
        assert!((e[32] - 16_000.0).abs() < 1.0);
        assert!(e.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn every_band_has_bins() {
        let cfg = AnalysisConfig::default();
        let e = mel_edges(cfg.mel_min_hz, cfg.mel_max_hz, 32);
        for w in e.windows(2) {
            assert!(!bins_for(&cfg, w[0], w[1]).is_empty());
        }
        // sub 20–60 Hz at 23.4 Hz per bin: bins 1 and 2.
        assert_eq!(bins_for(&cfg, 20.0, 60.0), 1..3);
    }
}
