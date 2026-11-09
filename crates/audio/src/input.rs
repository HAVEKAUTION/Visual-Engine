//! Live input (microphone / line-in) via cpal.
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
    device: cpal::Device,
}

impl std::fmt::Debug for InputDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputDevice")
            .field("name", &self.name)
            .field("is_default", &self.is_default)
            .finish()
    }
}

fn device_name(d: &cpal::Device) -> String {
    d.description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "unknown device".into())
}

/// All capture devices (mics, line-ins) on the default audio host.
pub fn list_input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default_id = host.default_input_device().and_then(|d| d.id().ok());
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .map(|device| {
            let is_default = default_id.is_some() && device.id().ok() == default_id;
            InputDevice {
                name: device_name(&device),
                is_default,
                device,
            }
        })
        .collect()
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
        let supported = input
            .device
            .default_input_config()
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
    let err_cb = move |_err: cpal::Error| {
        failed.store(true, Relaxed);
    };
    device
        .build_input_stream::<T, _, _>(*config, data_cb, err_cb, None)
        .context("could not open the input stream")
}
