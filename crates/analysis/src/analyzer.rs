//! The analysis core: samples in, [`SignalFrame`]s out.
//!
//! Pure and deterministic: the output depends only on the samples fed in (and
//! the config), never on wall-clock time or how the samples were chunked. It
//! runs the same way live (analysis thread) and offline (tests, video export).

use std::ops::Range;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use signals::{Band, BandSignal, MEL_BANDS, SignalFrame};

use crate::bands::{bins_for, mel_edges};
use crate::config::AnalysisConfig;
use crate::smoothing::{AutoGain, Smoother};

/// Auto-gain plus fast/slow smoothing for one signal.
#[derive(Clone, Copy, Debug)]
struct Channel {
    gain: AutoGain,
    fast: Smoother,
    slow: Smoother,
}

impl Channel {
    fn new(cfg: &AnalysisConfig) -> Self {
        Self {
            gain: AutoGain::new(cfg.auto_gain_tau, cfg.auto_gain_epsilon, cfg.level_max),
            fast: Smoother::new(cfg.fast),
            slow: Smoother::new(cfg.slow),
        }
    }

    fn update(&mut self, energy: f32, dt: f32) -> BandSignal {
        let level = self.gain.update(energy, dt);
        BandSignal {
            level,
            fast: self.fast.update(level, dt),
            slow: self.slow.update(level, dt),
            energy_db: power_db(energy),
        }
    }
}

fn power_db(p: f32) -> f32 {
    10.0 * (p + 1e-12).log10()
}

pub struct Analyzer {
    cfg: AnalysisConfig,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    /// Magnitude scale so a full-scale sine peaks at |X| ~ 1: 2 / sum(w).
    mag_scale: f32,

    /// Circular buffer of the last N input samples.
    ring: Vec<f32>,
    ring_pos: usize,
    /// Samples received since the last hop.
    since_hop: usize,

    fft_in: Vec<f32>,
    fft_out: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    /// |X[k]| (scaled), k = 0..=N/2.
    magnitude: Vec<f32>,
    /// Y[k] = ln(1 + gamma |X[k]|).
    log_spectrum: Vec<f32>,

    mel_bins: Vec<Range<usize>>,
    named_bins: Vec<Range<usize>>,
    mel: Vec<Channel>,
    named: Vec<Channel>,
    loudness: Channel,
    /// Raw band energies of the last hop (for tests/debugging).
    named_energy: [f32; Band::COUNT],

    frame: SignalFrame,
}

impl Analyzer {
    pub fn new(cfg: AnalysisConfig) -> Self {
        assert!(cfg.fft_size >= 64 && cfg.fft_size.is_power_of_two());
        assert!(cfg.hop_size > 0 && cfg.hop_size <= cfg.fft_size);
        let n = cfg.fft_size;
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);

        // Periodic Hann window.
        let window: Vec<f32> = (0..n)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
            .collect();
        let mag_scale = 2.0 / window.iter().sum::<f32>();

        let edges = mel_edges(cfg.mel_min_hz, cfg.mel_max_hz, MEL_BANDS);
        let mel_bins = edges
            .windows(2)
            .map(|w| bins_for(&cfg, w[0], w[1]))
            .collect();
        let named_bins = cfg
            .named_bands
            .iter()
            .map(|&(lo, hi)| bins_for(&cfg, lo, hi))
            .collect();

        let fft_in = fft.make_input_vec();
        let fft_out = fft.make_output_vec();
        let scratch = fft.make_scratch_vec();
        let bins = n / 2 + 1;

        Self {
            fft,
            window,
            mag_scale,
            ring: vec![0.0; n],
            ring_pos: 0,
            since_hop: 0,
            fft_in,
            fft_out,
            scratch,
            magnitude: vec![0.0; bins],
            log_spectrum: vec![0.0; bins],
            mel_bins,
            named_bins,
            mel: vec![Channel::new(&cfg); MEL_BANDS],
            named: vec![Channel::new(&cfg); Band::COUNT],
            loudness: Channel::new(&cfg),
            named_energy: [0.0; Band::COUNT],
            frame: SignalFrame {
                dt: cfg.hop_seconds(),
                ..Default::default()
            },
            cfg,
        }
    }

    pub fn config(&self) -> &AnalysisConfig {
        &self.cfg
    }

    /// Forget everything (call when the audio source changes).
    pub fn reset(&mut self) {
        let cfg = self.cfg.clone();
        *self = Self::new(cfg);
    }

    /// The latest frame.
    pub fn frame(&self) -> &SignalFrame {
        &self.frame
    }

    /// Log-compressed spectrum `Y[k]` of the latest hop.
    pub fn log_spectrum(&self) -> &[f32] {
        &self.log_spectrum
    }

    /// Raw (un-normalised) named-band energies of the latest hop.
    pub fn named_band_energy(&self) -> &[f32; Band::COUNT] {
        &self.named_energy
    }

    /// Feed samples (mono, at `config.sample_rate`). `on_frame` is called once
    /// per completed hop. Does not allocate.
    pub fn push(&mut self, samples: &[f32], mut on_frame: impl FnMut(&SignalFrame)) {
        let n = self.ring.len();
        for &s in samples {
            self.ring[self.ring_pos] = s;
            self.ring_pos = (self.ring_pos + 1) % n;
            self.since_hop += 1;
            if self.since_hop == self.cfg.hop_size {
                self.since_hop = 0;
                self.process_hop();
                on_frame(&self.frame);
            }
        }
    }

    fn process_hop(&mut self) {
        let n = self.ring.len();
        let dt = self.cfg.hop_seconds();

        // 1. Spectrum: window the last N samples (oldest first) and FFT.
        let mut sum_sq = 0.0f32;
        for i in 0..n {
            let s = self.ring[(self.ring_pos + i) % n];
            sum_sq += s * s;
            self.fft_in[i] = s * self.window[i];
        }
        // Buffers come from the planner, so this cannot fail.
        let _ =
            self.fft
                .process_with_scratch(&mut self.fft_in, &mut self.fft_out, &mut self.scratch);

        let gamma = self.cfg.log_gamma;
        let mut weighted = 0.0f32;
        let mut total = 0.0f32;
        for (k, c) in self.fft_out.iter().enumerate() {
            let m = c.norm() * self.mag_scale;
            self.magnitude[k] = m;
            self.log_spectrum[k] = (1.0 + gamma * m).ln();
            if k > 0 {
                weighted += self.cfg.bin_hz(k) * m;
                total += m;
            }
        }

        // 2-4. Bands -> auto-gain -> fast/slow.
        for (b, bins) in self.mel_bins.iter().enumerate() {
            let e = mean_power(&self.magnitude[bins.clone()]);
            self.frame.mel[b] = self.mel[b].update(e, dt).fast;
        }
        for (b, bins) in self.named_bins.iter().enumerate() {
            let e = mean_power(&self.magnitude[bins.clone()]);
            self.named_energy[b] = e;
            self.frame.bands[b] = self.named[b].update(e, dt);
        }

        // 9. Loudness (frame mean-square, shown in dBFS) and centroid.
        let mean_sq = sum_sq / n as f32;
        let mut loud = self.loudness.update(mean_sq, dt);
        loud.energy_db = power_db(mean_sq);
        self.frame.loudness = loud;

        // Hold the last centroid through silence.
        if total > 1e-6 {
            let c = weighted / total;
            self.frame.centroid_hz = c;
            let (lo, hi) = (self.cfg.centroid_min_hz, self.cfg.centroid_max_hz);
            self.frame.centroid_hue = ((c / lo).log2() / (hi / lo).log2()).clamp(0.0, 1.0);
        }

        self.frame.hop_index += 1;
        self.frame.time =
            self.frame.hop_index as f64 * self.cfg.hop_size as f64 / self.cfg.sample_rate as f64;
        self.frame.dt = dt;
    }
}

/// Mean of |X|^2 over a set of bins.
fn mean_power(mags: &[f32]) -> f32 {
    if mags.is_empty() {
        return 0.0;
    }
    mags.iter().map(|m| m * m).sum::<f32>() / mags.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const FS: f32 = 48_000.0;

    fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
        // f64 phase: an f32 phase drifts after a few seconds and adds fake noise.
        (0..(secs * FS) as usize)
            .map(|i| {
                let ph = std::f64::consts::TAU * freq as f64 * i as f64 / FS as f64;
                amp * ph.sin() as f32
            })
            .collect()
    }

    /// Run a signal through a fresh analyzer; return it and the last frame.
    fn analyse(x: &[f32]) -> (Analyzer, SignalFrame) {
        let mut a = Analyzer::new(AnalysisConfig::default());
        let mut last = SignalFrame::default();
        a.push(x, |f| last = f.clone());
        (a, last)
    }

    fn loudest_band(a: &Analyzer) -> Band {
        let e = a.named_band_energy();
        Band::ALL
            .into_iter()
            .max_by(|x, y| e[x.index()].total_cmp(&e[y.index()]))
            .unwrap()
    }

    // Note: auto-gain normalises each band to its *own* typical level, so for a
    // steady test tone "which band dominates" is judged on raw energy.

    #[test]
    fn hop_rate_and_time() {
        let (_, f) = analyse(&vec![0.0; 48_000]);
        // 48000 / 512 = 93.75 hops per second.
        assert_eq!(f.hop_index, 93);
        assert!((f.dt - 512.0 / 48_000.0).abs() < 1e-9);
        assert!((f.time - 93.0 * 512.0 / 48_000.0).abs() < 1e-9);
    }

    #[test]
    fn sixty_hz_is_sub_and_bass() {
        let (a, f) = analyse(&sine(60.0, 0.5, 2.0));
        let e = a.named_band_energy();
        let low = e[Band::Sub.index()].max(e[Band::Bass.index()]);
        for b in [Band::LowMid, Band::Mid, Band::HighMid, Band::Air] {
            assert!(low > 1000.0 * e[b.index()], "{b:?} too strong");
        }
        assert!(f.band(Band::Sub).level > 0.5 || f.band(Band::Bass).level > 0.5);
        // Far-away bands only see numerical leakage: below epsilon, so zero.
        assert!(f.band(Band::Air).level < 0.01);
    }

    #[test]
    fn one_khz_is_mid() {
        let (a, f) = analyse(&sine(1000.0, 0.5, 2.0));
        assert_eq!(loudest_band(&a), Band::Mid);
        assert!((f.band(Band::Mid).level - 1.0).abs() < 0.1);
        assert!(f.band(Band::Sub).level < 0.01);
    }

    #[test]
    fn ten_khz_is_air() {
        let (a, f) = analyse(&sine(10_000.0, 0.5, 2.0));
        assert_eq!(loudest_band(&a), Band::Air);
        assert!((f.band(Band::Air).level - 1.0).abs() < 0.1);
        assert!(f.band(Band::Sub).level < 0.01);
    }

    #[test]
    fn mel_bands_follow_frequency() {
        let peak = |freq| {
            let (a, _) = analyse(&sine(freq, 0.5, 1.0));
            // The mel band with the most raw energy.
            a.mel_bins
                .iter()
                .enumerate()
                .max_by(|(_, x), (_, y)| {
                    mean_power(&a.magnitude[(*x).clone()])
                        .total_cmp(&mean_power(&a.magnitude[(*y).clone()]))
                })
                .unwrap()
                .0
        };
        let (lo, mid, hi) = (peak(100.0), peak(1000.0), peak(8000.0));
        assert!(lo < mid && mid < hi, "{lo} {mid} {hi}");
    }

    /// A drum-ish test signal: noise bursts plus a pulsing 80 Hz tone at 120 BPM.
    fn music_like(amp: f32, secs: f32) -> Vec<f32> {
        let mut seed: u32 = 12345;
        (0..(secs * FS) as usize)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                let t = i as f32 / FS;
                let beat = (t * 2.0).fract();
                let env = (-beat * 12.0).exp();
                amp * (0.6 * env * (TAU * 80.0 * t).sin() + 0.2 * noise * (0.3 + env))
            })
            .collect()
    }

    #[test]
    fn auto_gain_makes_quiet_and_loud_alike() {
        // Same music at 0 dB and -26 dB.
        let (_, loud) = analyse(&music_like(1.0, 20.0));
        let (_, quiet) = analyse(&music_like(0.05, 20.0));
        for b in Band::ALL {
            let (l, q) = (loud.band(b).slow, quiet.band(b).slow);
            assert!(
                (l - q).abs() < 0.1 * l.max(0.2),
                "{b:?}: loud {l} vs quiet {q}"
            );
        }
        let (l, q) = (loud.loudness.slow, quiet.loudness.slow);
        assert!((l - q).abs() < 0.1, "loudness {l} vs {q}");
        // ...while the raw dB values are ~26 dB apart.
        let d = loud.loudness.energy_db - quiet.loudness.energy_db;
        assert!((d - 26.0).abs() < 0.5, "dB difference {d}");
    }

    #[test]
    fn levels_average_one_for_steady_music() {
        // Auto-gain's promise: over time, every band's level averages ~1.
        let x = music_like(0.5, 30.0);
        let mut a = Analyzer::new(AnalysisConfig::default());
        let mut sums = [0.0f64; Band::COUNT];
        let mut count = 0;
        a.push(&x, |f| {
            if f.time > 10.0 {
                for b in Band::ALL {
                    sums[b.index()] += f.band(b).level as f64;
                }
                count += 1;
            }
        });
        // (The test signal has no sub content, so sub is skipped.) The bass is
        // a sharp kick whose peaks exceed the level clamp of 8, so its average
        // sits a little under 1; the steadier bands land right on it.
        for b in [
            Band::Bass,
            Band::LowMid,
            Band::Mid,
            Band::HighMid,
            Band::Air,
        ] {
            let mean = sums[b.index()] / count as f64;
            let tolerance = if b == Band::Bass { 0.25 } else { 0.15 };
            assert!((mean - 1.0).abs() < tolerance, "{b:?} mean level {mean}");
        }
    }

    #[test]
    fn silence_gives_zero_levels() {
        let (_, f) = analyse(&vec![0.0; 48_000]);
        assert!(f.bands.iter().all(|b| b.level == 0.0 && b.fast == 0.0));
        assert_eq!(f.loudness.level, 0.0);
    }

    #[test]
    fn centroid_tracks_brightness() {
        let (_, f) = analyse(&sine(1000.0, 0.5, 1.0));
        assert!((f.centroid_hz - 1000.0).abs() < 60.0, "{}", f.centroid_hz);
        let (_, dark) = analyse(&sine(150.0, 0.5, 1.0));
        let (_, bright) = analyse(&sine(6000.0, 0.5, 1.0));
        assert!(dark.centroid_hue < f.centroid_hue && f.centroid_hue < bright.centroid_hue);
        assert!((0.0..=1.0).contains(&bright.centroid_hue));
    }

    #[test]
    fn deterministic_regardless_of_chunking() {
        let x = music_like(0.5, 3.0);
        let (_, whole) = analyse(&x);
        let mut a = Analyzer::new(AnalysisConfig::default());
        let mut last = SignalFrame::default();
        for chunk in x.chunks(441) {
            a.push(chunk, |f| last = f.clone());
        }
        assert_eq!(whole, last);
    }

    #[test]
    fn reset_starts_over() {
        let mut a = Analyzer::new(AnalysisConfig::default());
        a.push(&sine(1000.0, 0.5, 1.0), |_| {});
        a.reset();
        assert_eq!(a.frame().hop_index, 0);
        assert_eq!(a.frame().band(Band::Mid).slow, 0.0);
    }
}
