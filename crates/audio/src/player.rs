//! Song playback through the default output device.
//!
//! The UI thread talks to the audio callback only through atomics in
//! [`PlayerShared`]. The callback copies pre-converted samples out of an
//! immutable [`Song`] and pushes the matching 48 kHz mono samples into the
//! analysis feed. It never allocates, locks, logs or does I/O.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::feed::{FeedProducer, push_silence, push_slice};
use crate::song::{Song, analysis_index};

const NO_SEEK: u64 = u64::MAX;
/// Volume changes glide over this time constant to avoid clicks.
const VOLUME_SMOOTHING_SECS: f32 = 0.01;

#[derive(Debug)]
struct PlayerShared {
    playing: AtomicBool,
    /// Current position in output frames (written by the callback).
    position: AtomicU64,
    /// Pending seek target in output frames, or `NO_SEEK`.
    seek_to: AtomicU64,
    /// Volume 0..1 as `f32` bits.
    volume: AtomicU32,
    /// Set by the stream's error callback.
    failed: AtomicBool,
    /// Analyse this many output frames ahead of what's being played, so the
    /// visuals can come early (for a projector or screen that shows them
    /// late).
    analysis_lead: AtomicU64,
}

/// Plays one [`Song`]. Dropping it stops playback.
pub struct FilePlayer {
    // Field order matters: the stream (and the callback that owns a clone of
    // the song) is dropped before our own reference to the song.
    _stream: Stream,
    song: Arc<Song>,
    shared: Arc<PlayerShared>,
    device_name: String,
}

impl FilePlayer {
    /// Open the default output device and start the (paused) stream.
    pub fn start(song: Song, producer: FeedProducer, volume: f32) -> Result<Self> {
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
            sample_rate: song.output_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        let song = Arc::new(song);
        let shared = Arc::new(PlayerShared {
            playing: AtomicBool::new(false),
            position: AtomicU64::new(0),
            seek_to: AtomicU64::new(NO_SEEK),
            volume: AtomicU32::new(volume.clamp(0.0, 1.0).to_bits()),
            failed: AtomicBool::new(false),
            analysis_lead: AtomicU64::new(0),
        });

        macro_rules! build {
            ($t:ty) => {
                build_output::<$t>(&device, &config, song.clone(), shared.clone(), producer)
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
        stream.play().context("could not start the output stream")?;

        log::info!(
            "playing '{}' ({} Hz, {} ch source) on '{}' at {} Hz, {} ch",
            song.name,
            song.source_rate,
            song.source_channels,
            device_name,
            config.sample_rate,
            config.channels
        );

        Ok(Self {
            _stream: stream,
            song,
            shared,
            device_name,
        })
    }

    pub fn song(&self) -> &Song {
        &self.song
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Relaxed)
    }

    pub fn play(&self) {
        // Pressing play at the very end starts the song again.
        if self.position_frames() >= self.song.frames() {
            self.seek_frames(0);
        }
        self.shared.playing.store(true, Relaxed);
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Relaxed);
    }

    pub fn toggle(&self) {
        if self.is_playing() {
            self.pause()
        } else {
            self.play()
        }
    }

    fn position_frames(&self) -> u64 {
        match self.shared.seek_to.load(Relaxed) {
            NO_SEEK => self.shared.position.load(Relaxed),
            pending => pending,
        }
    }

    fn seek_frames(&self, frame: u64) {
        self.shared
            .seek_to
            .store(frame.min(self.song.frames()), Relaxed);
    }

    pub fn position_secs(&self) -> f64 {
        self.position_frames() as f64 / self.song.output_rate as f64
    }

    pub fn duration_secs(&self) -> f64 {
        self.song.duration_secs()
    }

    pub fn seek_secs(&self, secs: f64) {
        let frame = (secs.max(0.0) * self.song.output_rate as f64) as u64;
        self.seek_frames(frame);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Relaxed))
    }

    pub fn set_volume(&self, v: f32) {
        self.shared
            .volume
            .store(v.clamp(0.0, 1.0).to_bits(), Relaxed);
    }

    /// True if the output device reported an error (e.g. it was unplugged).
    /// Analyse `secs` ahead of what's heard (0 = in step).
    pub fn set_analysis_lead(&self, secs: f64) {
        let frames = (secs.clamp(0.0, 1.0) * self.song.output_rate as f64) as u64;
        self.shared.analysis_lead.store(frames, Relaxed);
    }

    pub fn has_failed(&self) -> bool {
        self.shared.failed.load(Relaxed)
    }
}

/// Perceptual volume taper: slider position → linear gain.
#[inline]
fn volume_to_gain(v: f32) -> f32 {
    v * v
}

fn build_output<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    song: Arc<Song>,
    shared: Arc<PlayerShared>,
    mut producer: FeedProducer,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = config.channels as usize;
    let rate = config.sample_rate;
    let total = song.frames();
    let smoothing = 1.0 - (-1.0 / (VOLUME_SMOOTHING_SECS * rate as f32)).exp();

    // Callback-local state (moved into the closure; nothing shared mutably).
    let mut pos: u64 = 0;
    let mut silence_clock: u64 = 0;
    let mut gain = volume_to_gain(f32::from_bits(shared.volume.load(Relaxed)));
    let err_flag = shared.clone();

    let data_cb = move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
        let seek = shared.seek_to.swap(NO_SEEK, Relaxed);
        if seek != NO_SEEK {
            pos = seek.min(total);
        }
        let playing = shared.playing.load(Relaxed);
        let target = volume_to_gain(f32::from_bits(shared.volume.load(Relaxed)));
        let start = pos;
        let frames = (out.len() / channels.max(1)) as u64;

        for frame in out.chunks_exact_mut(channels.max(1)) {
            gain += (target - gain) * smoothing;
            let (l, r) = if playing && pos < total {
                let i = pos as usize * 2;
                pos += 1;
                (song.playback[i] * gain, song.playback[i + 1] * gain)
            } else {
                (0.0, 0.0)
            };
            if channels == 1 {
                frame[0] = T::from_sample(0.5 * (l + r));
            } else {
                for (c, s) in frame.iter_mut().enumerate() {
                    *s = T::from_sample(match c {
                        0 => l,
                        1 => r,
                        _ => 0.0,
                    });
                }
            }
        }

        // Analysis feed: exactly the stretch of song that was just played
        // (pre-volume; shifted ahead by the lead), then silence for any
        // frames that were silent. Past the end, silence keeps the pace.
        if pos > start {
            let lead = shared.analysis_lead.load(Relaxed);
            let a = analysis_index(start + lead, rate) as usize;
            let b = analysis_index(pos + lead, rate) as usize;
            let len = song.analysis.len();
            let (ca, cb) = (a.min(len), b.min(len));
            if ca < cb {
                push_slice(&mut producer, &song.analysis[ca..cb]);
            }
            let past_end = (b - a) - (cb - ca);
            if past_end > 0 {
                push_silence(&mut producer, past_end);
            }
        }
        let silent = frames - (pos - start);
        if silent > 0 {
            let a = analysis_index(silence_clock, rate);
            silence_clock += silent;
            let b = analysis_index(silence_clock, rate);
            push_silence(&mut producer, (b - a) as usize);
        }

        if playing && pos >= total {
            shared.playing.store(false, Relaxed);
        }
        shared.position.store(pos, Relaxed);
    };

    let err_cb = move |_err: cpal::Error| {
        // No logging on the audio thread: just raise a flag for the UI.
        err_flag.failed.store(true, Relaxed);
    };

    device
        .build_output_stream::<T, _, _>(*config, data_cb, err_cb, None)
        .context("could not open the output stream")
}

/// Sample rate of the default output device (what songs are converted to).
pub fn default_output_rate() -> Result<u32> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow!("no audio output device found"))?;
    Ok(device
        .default_output_config()
        .context("could not query the output device")?
        .sample_rate())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_taper_is_monotonic_with_fixed_ends() {
        assert_eq!(volume_to_gain(0.0), 0.0);
        assert_eq!(volume_to_gain(1.0), 1.0);
        assert!(volume_to_gain(0.5) < 0.5);
    }

    #[test]
    fn sample_conversion_is_available() {
        // Guards the generic bounds used by `build_output`.
        use cpal::Sample;
        let v: i16 = i16::from_sample(1.0f32);
        assert_eq!(v, i16::MAX);
    }
}
