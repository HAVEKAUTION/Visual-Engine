//! Live input (microphone / line-in, or the computer's own sound) via cpal.
//!
//! "Computer audio" entries record an *output* device in loopback: WASAPI
//! loopback on Windows, a Core Audio tap on macOS 14.2+.
//!
//! The input callback mixes each frame to mono, converts it to 48 kHz with a
//! pre-allocated [`StreamResampler`] and pushes it into the analysis feed. It
//! never allocates, locks, logs or does I/O.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::ANALYSIS_SAMPLE_RATE;
use crate::feed::{FeedProducer, push_one};
use crate::resample::StreamResampler;

/// An input device that can be opened with [`LiveInput::start`].
#[derive(Clone)]
pub struct InputDevice {
    pub name: String,
    pub is_default: bool,
    /// Records what this output device is playing (system audio).
    pub loopback: bool,
    device: cpal::Device,
}

impl std::fmt::Debug for InputDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputDevice")
            .field("name", &self.name)
            .field("is_default", &self.is_default)
            .field("loopback", &self.loopback)
            .finish()
    }
}

fn device_name(d: &cpal::Device) -> String {
    d.description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "unknown device".into())
}

/// Name prefix of the loopback ("computer audio") entries.
pub const LOOPBACK_PREFIX: &str = "Computer audio: ";

/// All capture devices: first "computer audio" (loopback of every output
/// device, the default output first), then mics and line-ins.
pub fn list_input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let mut out = Vec::new();

    if cfg!(any(target_os = "windows", target_os = "macos")) {
        let default_out = host.default_output_device().and_then(|d| d.id().ok());
        let mut loop_devs: Vec<InputDevice> = host
            .output_devices()
            .map(|it| {
                it.map(|device| InputDevice {
                    name: format!("{LOOPBACK_PREFIX}{}", device_name(&device)),
                    is_default: default_out.is_some() && device.id().ok() == default_out,
                    loopback: true,
                    device,
                })
                .collect()
            })
            .unwrap_or_default();
        loop_devs.sort_by_key(|d| !d.is_default);
        // Only the mic is the "default" pick; keep the flag for sorting only.
        for d in &mut loop_devs {
            d.is_default = false;
        }
        out.extend(loop_devs);
    }

    let default_id = host.default_input_device().and_then(|d| d.id().ok());
    if let Ok(devices) = host.input_devices() {
        out.extend(devices.map(|device| {
            let is_default = default_id.is_some() && device.id().ok() == default_id;
            InputDevice {
                name: device_name(&device),
                is_default,
                loopback: false,
                device,
            }
        }));
    }
    out
}

/// A running capture stream. Dropping it stops capture.
pub struct LiveInput {
    _stream: Stream,
    failed: Arc<AtomicBool>,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl LiveInput {
    pub fn start(input: &InputDevice, producer: FeedProducer) -> Result<Self> {
        // Loopback records in the output device's own format.
        let supported = if input.loopback {
            input.device.default_output_config()
        } else {
            input.device.default_input_config()
        }
        .with_context(|| format!("could not query '{}'", input.name))?;
        let config: StreamConfig = supported.config();
        let failed = Arc::new(AtomicBool::new(false));

        macro_rules! build {
            ($t:ty) => {
                build_input::<$t>(&input.device, &config, failed.clone(), producer)
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
            other => Err(anyhow!("unsupported input sample format {other:?}")),
        }?;
        stream
            .play()
            .with_context(|| format!("could not start '{}'", input.name))?;

        log::info!(
            "live input '{}' at {} Hz, {} ch",
            input.name,
            config.sample_rate,
            config.channels
        );
        Ok(Self {
            _stream: stream,
            failed,
            device_name: input.name.clone(),
            sample_rate: config.sample_rate,
            channels: config.channels,
        })
    }

    /// True if the device reported an error (e.g. it was unplugged).
    pub fn has_failed(&self) -> bool {
        self.failed.load(Relaxed)
    }
}

fn build_input<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    failed: Arc<AtomicBool>,
    mut producer: FeedProducer,
) -> Result<Stream>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = (config.channels as usize).max(1);
    // Allocated here, before the stream starts; the callback only reuses it.
    let mut resampler = StreamResampler::new(config.sample_rate, ANALYSIS_SAMPLE_RATE);
    let inv_ch = 1.0 / channels as f32;

    let data_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
        for frame in data.chunks_exact(channels) {
            let mut sum = 0.0f32;
            for &s in frame {
                sum += f32::from_sample(s);
            }
            resampler.process(sum * inv_ch, |y| push_one(&mut producer, y));
        }
    };
    let err_cb = move |err: cpal::Error| {
        // Dropouts and re-routes are harmless (loopback reports a dropout when
        // it starts); anything else means the device is gone.
        use cpal::ErrorKind::*;
        if !matches!(err.kind(), Xrun | DeviceChanged | RealtimeDenied) {
            failed.store(true, Relaxed);
        }
    };
    device
        .build_input_stream::<T, _, _>(*config, data_cb, err_cb, None)
        .context("could not open the input stream")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opens "computer audio" on the default output and checks that sound
    /// arrives. Needs real hardware and something playing, so it only runs
    /// when `VE_LOOPBACK_TEST=1`.
    #[test]
    fn loopback_captures_system_audio() {
        if std::env::var("VE_LOOPBACK_TEST").is_err() {
            return;
        }
        let dev = list_input_devices()
            .into_iter()
            .find(|d| d.loopback)
            .expect("no computer-audio device");
        let (tx, mut rx) = crate::feed::new_feed();
        let live = LiveInput::start(&dev, tx).expect("loopback start");
        std::thread::sleep(std::time::Duration::from_secs(3));
        let mut n = 0usize;
        let mut peak = 0.0f32;
        while let Ok(x) = rx.pop() {
            n += 1;
            peak = peak.max(x.abs());
        }
        println!("{}: {n} samples, peak {peak}", live.device_name);
        assert!(!live.has_failed());
        assert!(n > 48_000, "too few samples: {n}");
        assert!(peak > 0.001, "silent");
    }
}
