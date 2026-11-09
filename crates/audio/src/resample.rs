//! Band-limited (windowed-sinc) sample-rate conversion.
//!
//! One implementation serves two jobs:
//! - **offline**: converting a whole decoded song on a loader thread
//!   ([`resample_offline`]);
//! - **streaming**: converting live input inside the real-time audio callback
//!   ([`StreamResampler::process`]). All memory is allocated in
//!   [`StreamResampler::new`]; `process` never allocates, locks or blocks.
//!
//! Timing is tracked with exact integer arithmetic (reduced `from:to` ratio), so
//! the output never drifts, no matter how long it runs.

use std::f64::consts::PI;

/// Kernel half-width, in input samples, when up-sampling.
const BASE_TAPS: usize = 32;
/// Number of fractional-delay phases stored in the kernel table.
const PHASES: usize = 256;
/// Cut-off as a fraction of the lower of the two Nyquist frequencies.
const CUTOFF: f64 = 0.95;
/// Kaiser window shape (~80 dB stop-band).
const KAISER_BETA: f64 = 8.6;

/// Streaming windowed-sinc resampler for a single channel.
pub struct StreamResampler {
    passthrough: bool,
    /// Kernel half-width in input samples.
    taps: usize,
    /// `(PHASES + 1)` rows of `2 * taps` weights.
    table: Vec<f32>,
    /// Mirrored history: every sample is written at `pos` and `pos + 2*taps`,
    /// so the last `2*taps` samples are always one contiguous slice.
    hist: Vec<f32>,
    pos: usize,
    /// Number of input samples received so far.
    n_in: u64,
    /// Index of the next output sample.
    n_out: u64,
    /// Reduced ratio: output sample `j` sits at input time `j * from / to`.
    from: u64,
    to: u64,
}

impl StreamResampler {
    /// Create a resampler converting `from_rate` Hz to `to_rate` Hz.
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        assert!(
            from_rate > 0 && to_rate > 0,
            "sample rates must be non-zero"
        );
        let g = gcd(from_rate as u64, to_rate as u64);
        let (from, to) = (from_rate as u64 / g, to_rate as u64 / g);
        let passthrough = from == to;

        let ratio = to_rate as f64 / from_rate as f64;
        // Cut-off frequency relative to the input Nyquist frequency.
        let fc = CUTOFF * ratio.min(1.0);
        // When down-sampling the kernel must widen to keep the same quality.
        let taps = if passthrough {
            0
        } else {
            (BASE_TAPS as f64 / ratio.min(1.0)).ceil() as usize
        };
        let width = 2 * taps;

        let mut table = vec![0.0f32; (PHASES + 1) * width];
        if !passthrough {
            for p in 0..=PHASES {
                let frac = p as f64 / PHASES as f64;
                let row = &mut table[p * width..(p + 1) * width];
                let mut sum = 0.0f64;
                let mut w = vec![0.0f64; width];
                for (m, wm) in w.iter_mut().enumerate() {
                    // Distance from the output time to input sample `i - taps + 1 + m`.
                    let d = frac + taps as f64 - 1.0 - m as f64;
                    *wm = fc * sinc(fc * d) * kaiser(d / taps as f64, KAISER_BETA);
                    sum += *wm;
                }
                // Normalise each phase to unity DC gain.
                for (r, wm) in row.iter_mut().zip(&w) {
                    *r = (wm / sum) as f32;
                }
            }
        }

        Self {
            passthrough,
            taps,
            table,
            hist: vec![0.0; 2 * width.max(1)],
            pos: 0,
            n_in: 0,
            n_out: 0,
            from,
            to,
        }
    }

    /// Delay introduced by the filter, in input samples.
    pub fn latency_input_samples(&self) -> usize {
        self.taps
    }

    /// Feed one input sample; `emit` is called for every output sample that is
    /// now ready (zero, one or several). Real-time safe.
    #[inline]
    pub fn process(&mut self, x: f32, mut emit: impl FnMut(f32)) {
        if self.passthrough {
            emit(x);
            return;
        }
        let width = 2 * self.taps;
        self.hist[self.pos] = x;
        self.hist[self.pos + width] = x;
        self.pos = (self.pos + 1) % width;
        self.n_in += 1;
        // Index of the newest input sample.
        let newest = self.n_in - 1;

        loop {
            let t_num = self.n_out * self.from;
            let i = t_num / self.to;
            if i + self.taps as u64 > newest {
                break;
            }
            // Because we check after every input sample, `newest == i + taps`
            // here, so the history window starts at sample `i - taps + 1`.
            let rem = t_num % self.to;
            let phase = rem as f64 * PHASES as f64 / self.to as f64;
            let p = (phase as usize).min(PHASES - 1);
            let a = (phase - p as f64) as f32;

            let window = &self.hist[self.pos..self.pos + width];
            let row0 = &self.table[p * width..(p + 1) * width];
            let row1 = &self.table[(p + 1) * width..(p + 2) * width];
            let mut y0 = 0.0f32;
            let mut y1 = 0.0f32;
            for ((&s, &w0), &w1) in window.iter().zip(row0).zip(row1) {
                y0 += s * w0;
                y1 += s * w1;
            }
            emit(y0 + a * (y1 - y0));
            self.n_out += 1;
        }
    }
}

/// Number of output samples [`resample_offline`] produces for `len` inputs.
pub fn output_len(len: usize, from_rate: u32, to_rate: u32) -> usize {
    (len as u64 * to_rate as u64).div_ceil(from_rate as u64) as usize
}

/// Resample a whole single-channel signal. Output is time-aligned with the
/// input (no filter delay) and has [`output_len`] samples.
pub fn resample_offline(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }
    let n = output_len(input.len(), from_rate, to_rate);
    let mut r = StreamResampler::new(from_rate, to_rate);
    let mut out = Vec::with_capacity(n + 4);
    for &x in input {
        r.process(x, |y| out.push(y));
    }
    // Flush the filter tail.
    while out.len() < n {
        r.process(0.0, |y| out.push(y));
    }
    out.truncate(n);
    out
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// Kaiser window on `x ∈ [-1, 1]`.
fn kaiser(x: f64, beta: f64) -> f64 {
    if x.abs() > 1.0 {
        return 0.0;
    }
    bessel_i0(beta * (1.0 - x * x).sqrt()) / bessel_i0(beta)
}

/// Zeroth-order modified Bessel function of the first kind (power series).
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let half = x / 2.0;
    for k in 1..64 {
        term *= half / k as f64;
        let t2 = term * term;
        sum += t2;
        if t2 < sum * 1e-17 {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|n| (2.0 * PI * freq * n as f64 / rate as f64).sin() as f32)
            .collect()
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn same_rate_is_identity() {
        let x = sine(440.0, 48_000, 1000);
        assert_eq!(resample_offline(&x, 48_000, 48_000), x);
        let mut r = StreamResampler::new(48_000, 48_000);
        let mut out = Vec::new();
        for &s in &x {
            r.process(s, |y| out.push(y));
        }
        assert_eq!(out, x);
    }

    #[test]
    fn output_length_matches_ratio() {
        let x = vec![0.0; 44_100];
        assert_eq!(resample_offline(&x, 44_100, 48_000).len(), 48_000);
        let x = vec![0.0; 96_001];
        assert_eq!(resample_offline(&x, 96_000, 48_000).len(), 48_001);
    }

    #[test]
    fn upsampled_sine_matches_ideal() {
        let x = sine(1000.0, 44_100, 44_100);
        let y = resample_offline(&x, 44_100, 48_000);
        let ideal = sine(1000.0, 48_000, y.len());
        // Ignore the edges, where the input starts/stops abruptly.
        let max_err = y[200..y.len() - 200]
            .iter()
            .zip(&ideal[200..ideal.len() - 200])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(max_err < 2e-3, "max error {max_err}");
    }

    #[test]
    fn dc_is_preserved() {
        let x = vec![0.5f32; 10_000];
        let y = resample_offline(&x, 44_100, 48_000);
        for &v in &y[200..y.len() - 200] {
            assert!((v - 0.5).abs() < 1e-4, "{v}");
        }
    }

    #[test]
    fn downsampling_removes_content_above_new_nyquist() {
        // 30 kHz cannot exist at 48 kHz; it must be filtered, not aliased to 18 kHz.
        let x = sine(30_000.0, 96_000, 96_000);
        let y = resample_offline(&x, 96_000, 48_000);
        let level = rms(&y[500..y.len() - 500]);
        assert!(level < 1e-3, "alias rms {level}");
        // ...while a 5 kHz tone passes untouched.
        let x = sine(5_000.0, 96_000, 96_000);
        let y = resample_offline(&x, 96_000, 48_000);
        let level = rms(&y[500..y.len() - 500]);
        assert!((level - 0.5f64.sqrt()).abs() < 0.01, "pass rms {level}");
    }

    #[test]
    fn streaming_output_is_offline_output_delayed() {
        let x = sine(700.0, 44_100, 5_000);
        let offline = resample_offline(&x, 44_100, 48_000);
        let mut r = StreamResampler::new(44_100, 48_000);
        let mut streamed = Vec::new();
        for &s in &x {
            r.process(s, |y| streamed.push(y));
        }
        // Streaming only lags; the samples it has produced are identical.
        assert!(streamed.len() < offline.len());
        assert_eq!(&offline[..streamed.len()], &streamed[..]);
    }
}
