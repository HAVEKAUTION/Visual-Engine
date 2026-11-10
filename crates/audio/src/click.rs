//! A metronome click on the default output, for the sound/picture sync
//! test. Clicks fall exactly every `interval` seconds of output samples; the
//! UI learns *when* each one was handed to the speakers through a shared
//! clock (no locks: one atomic, written by the callback).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

struct Shared {
    /// Click loudness 0..1 as f32 bits (0 = silent, the clock keeps running).
    gain: AtomicU32,
    /// Seconds after `base` at which output sample 0 was handed over, as
    /// f64 bits (smoothed over callbacks).
    zero_at: AtomicU64,
    /// What the device says its output delay is, in microseconds.
    device_latency_us: AtomicU64,
    failed: AtomicBool,
}

/// Plays clicks until dropped.
pub struct ClickTrack {
    _stream: Stream,
    shared: Arc<Shared>,
    base: Instant,
    interval: f64,
    device_name: String,
}

const CLICK_HZ: f32 = 1800.0;
const CLICK_DECAY: f32 = 0.006;
const CLICK_LEN: f32 = 0.04;

impl ClickTrack {
    /// Start clicking every `interval` seconds (silent until
    /// [`Self::set_audible`]).
    pub fn start(interval: f64) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no audio output device found"))?;
        let device_name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "output device".into());
        let supported = device
            .default_output_config()
            .context("could not query the output device")?;
        let config = StreamConfig {
            channels: supported.channels(),
            sample_rate: supported.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        let shared = Arc::new(Shared {
            gain: AtomicU32::new(0.0f32.to_bits()),
            zero_at: AtomicU64::new(f64::NAN.to_bits()),
            device_latency_us: AtomicU64::new(0),
            failed: AtomicBool::new(false),
        });
        let base = Instant::now();
        macro_rules! build {
            ($t:ty) => {
                build::<$t>(&device, &config, shared.clone(), base, interval)
            };
        }
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build!(f32),
            SampleFormat::F64 => build!(f64),
            SampleFormat::I16 => build!(i16),
            SampleFormat::U16 => build!(u16),
            SampleFormat::I32 => build!(i32),
            SampleFormat::U32 => build!(u32),
            SampleFormat::I8 => build!(i8),
            SampleFormat::U8 => build!(u8),
            other => Err(anyhow!("unsupported output sample format {other:?}")),
        }?;
        stream.play().context("could not start the click")?;
        Ok(Self {
            _stream: stream,
            shared,
            base,
            interval,
            device_name,
        })
    }

    pub fn set_audible(&self, on: bool) {
        let g: f32 = if on { 0.5 } else { 0.0 };
        self.shared.gain.store(g.to_bits(), Relaxed);
    }

    pub fn interval(&self) -> f64 {
        self.interval
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// The output delay the device reports (often less than the real one,
    /// e.g. Bluetooth).
    pub fn device_latency(&self) -> Duration {
        Duration::from_micros(self.shared.device_latency_us.load(Relaxed))
    }

    pub fn has_failed(&self) -> bool {
        self.shared.failed.load(Relaxed)
    }

    /// When click `k` was handed to the speakers (`None` until the first
    /// callback).
    pub fn click_time(&self, k: u64) -> Option<Instant> {
        let zero = f64::from_bits(self.shared.zero_at.load(Relaxed));
        if !zero.is_finite() {
            return None;
        }
        let at = zero + k as f64 * self.interval;
        Some(self.base + Duration::from_secs_f64(at.max(0.0)))
    }

    /// The click nearest to `t` and how far `t` is after it (seconds,
    /// negative = before).
    pub fn nearest_click(&self, t: Instant) -> Option<(u64, f64)> {
        let zero = f64::from_bits(self.shared.zero_at.load(Relaxed));
        if !zero.is_finite() {
            return None;
        }
        let secs = (t - self.base).as_secs_f64() - zero;
        let k = (secs / self.interval).round().max(0.0);
        Some((k as u64, secs - k * self.interval))
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<Shared>,
    base: Instant,
    interval: f64,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = (config.channels as usize).max(1);
    let rate = config.sample_rate as f64;
    let every = (interval * rate).round().max(1.0) as u64;
    let len = (CLICK_LEN as f64 * rate) as u64;
    let mut pos: u64 = 0;
    let mut zero = f64::NAN;
    let err_flag = shared.clone();
    let data_cb = move |out: &mut [T], info: &cpal::OutputCallbackInfo| {
        // Where sample 0 sits in wall-clock time, smoothed (callbacks jitter).
        let now = base.elapsed().as_secs_f64();
        let est = now - pos as f64 / rate;
        zero = if zero.is_finite() {
            zero + 0.05 * (est - zero)
        } else {
            est
        };
        shared.zero_at.store(zero.to_bits(), Relaxed);
        let ts = info.timestamp();
        let d = ts.playback.duration_since(ts.callback);
        shared
            .device_latency_us
            .store(d.as_micros() as u64, Relaxed);
        let gain = f32::from_bits(shared.gain.load(Relaxed));
        for frame in out.chunks_exact_mut(channels) {
            let into = pos % every;
            let s = if gain > 0.0 && into < len {
                let t = into as f32 / rate as f32;
                gain * (std::f32::consts::TAU * CLICK_HZ * t).sin() * (-t / CLICK_DECAY).exp()
            } else {
                0.0
            };
            for c in frame.iter_mut() {
                *c = T::from_sample(s);
            }
            pos += 1;
        }
    };
    let err_cb = move |_err: cpal::Error| {
        err_flag.failed.store(true, Relaxed);
    };
    device
        .build_output_stream::<T, _, _>(*config, data_cb, err_cb, None)
        .context("could not open the output for the click")
}
