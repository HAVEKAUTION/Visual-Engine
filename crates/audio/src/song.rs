//! A loaded song, pre-converted for playback and for analysis.

use std::path::Path;

use anyhow::Result;

use crate::ANALYSIS_SAMPLE_RATE;
use crate::decode::{DecodedAudio, decode_file};
use crate::resample::resample_offline;

/// A song held in memory in two forms:
/// - stereo at the output device's rate (what you hear);
/// - mono at 48 kHz (what the analysis sees).
///
/// Both are prepared once on a loader thread, so the audio callback only ever
/// copies samples.
pub struct Song {
    pub name: String,
    /// Sample rate of the original file.
    pub source_rate: u32,
    pub source_channels: usize,
    /// Rate of [`Self::playback`].
    pub output_rate: u32,
    /// Interleaved stereo at `output_rate`.
    pub playback: Vec<f32>,
    /// Mono at [`ANALYSIS_SAMPLE_RATE`].
    pub analysis: Vec<f32>,
}

impl Song {
    /// Decode a file and prepare it for an output device running at `output_rate`.
    pub fn load(path: &Path, output_rate: u32) -> Result<Self> {
        let decoded = decode_file(path)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "song".into());
        Ok(Self::from_decoded(name, &decoded, output_rate))
    }

    pub fn from_decoded(name: String, decoded: &DecodedAudio, output_rate: u32) -> Self {
        let ch = decoded.channels.max(1);
        let frames = decoded.frames();

        // Split into left/right (mono files play on both sides; for >2
        // channels we use the front pair) plus a mono mix of every channel.
        let mut left = Vec::with_capacity(frames);
        let mut right = Vec::with_capacity(frames);
        let mut mono = Vec::with_capacity(frames);
        for frame in decoded.samples.chunks_exact(ch) {
            left.push(frame[0]);
            right.push(if ch > 1 { frame[1] } else { frame[0] });
            mono.push(mix_to_mono(frame));
        }

        let rate = decoded.sample_rate;
        let (left, right, analysis) = std::thread::scope(|s| {
            let l = s.spawn(|| resample_offline(&left, rate, output_rate));
            let r = s.spawn(|| resample_offline(&right, rate, output_rate));
            let m = s.spawn(|| resample_offline(&mono, rate, ANALYSIS_SAMPLE_RATE));
            (
                l.join().expect("resampler thread panicked"),
                r.join().expect("resampler thread panicked"),
                m.join().expect("resampler thread panicked"),
            )
        });

        let mut playback = Vec::with_capacity(left.len() * 2);
        for (l, r) in left.iter().zip(&right) {
            playback.push(*l);
            playback.push(*r);
        }

        Self {
            name,
            source_rate: rate,
            source_channels: ch,
            output_rate,
            playback,
            analysis,
        }
    }

    /// Length in output-device frames.
    pub fn frames(&self) -> u64 {
        (self.playback.len() / 2) as u64
    }

    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / self.output_rate as f64
    }
}

/// The exact signal the analysis sees for a decoded file: every channel mixed
/// to mono, then resampled to 48 kHz. Live playback and offline analysis both
/// use this, so they give identical results.
pub fn analysis_signal(decoded: &DecodedAudio) -> Vec<f32> {
    let mono: Vec<f32> = decoded
        .samples
        .chunks_exact(decoded.channels.max(1))
        .map(mix_to_mono)
        .collect();
    resample_offline(&mono, decoded.sample_rate, ANALYSIS_SAMPLE_RATE)
}

/// Decode a file straight to the analysis signal (for offline analysis).
pub fn load_analysis_signal(path: &Path) -> Result<Vec<f32>> {
    Ok(analysis_signal(&decode_file(path)?))
}

/// Average all channels of one frame.
#[inline]
pub fn mix_to_mono(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    frame.iter().sum::<f32>() / frame.len() as f32
}

/// Map an output-device frame index to the matching 48 kHz analysis sample index.
/// Exact integer maths, so playback and analysis never drift apart.
#[inline]
pub fn analysis_index(frame: u64, output_rate: u32) -> u64 {
    if output_rate == ANALYSIS_SAMPLE_RATE {
        frame
    } else {
        ((frame as u128 * ANALYSIS_SAMPLE_RATE as u128) / output_rate as u128) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_mix_averages_channels() {
        assert_eq!(mix_to_mono(&[1.0, 0.0]), 0.5);
        assert_eq!(mix_to_mono(&[0.3]), 0.3);
        assert_eq!(mix_to_mono(&[]), 0.0);
    }

    #[test]
    fn analysis_index_is_exact() {
        assert_eq!(analysis_index(12_345, 48_000), 12_345);
        assert_eq!(analysis_index(44_100, 44_100), 48_000);
        assert_eq!(analysis_index(96_000, 96_000), 48_000);
        // One hour at 44.1 kHz lands exactly on one hour at 48 kHz.
        assert_eq!(analysis_index(44_100 * 3600, 44_100), 48_000 * 3600);
    }

    #[test]
    fn song_is_prepared_for_both_paths() {
        let decoded = DecodedAudio {
            sample_rate: 44_100,
            channels: 1,
            samples: vec![0.25; 44_100],
        };
        let song = Song::from_decoded("t".into(), &decoded, 48_000);
        assert_eq!(song.frames(), 48_000);
        assert_eq!(song.playback.len(), 96_000);
        assert_eq!(song.analysis.len(), 48_000);
        assert!((song.duration_secs() - 1.0).abs() < 1e-9);
        // Mono file plays equally on both sides.
        assert!((song.playback[2000] - song.playback[2001]).abs() < 1e-6);
    }

    #[test]
    fn offline_signal_matches_playback_analysis_signal() {
        let decoded = DecodedAudio {
            sample_rate: 44_100,
            channels: 2,
            samples: (0..20_000)
                .map(|i| ((i * 7919) % 200) as f32 / 100.0 - 1.0)
                .collect(),
        };
        let song = Song::from_decoded("t".into(), &decoded, 44_100);
        assert_eq!(analysis_signal(&decoded), song.analysis);
    }

    #[test]
    fn loads_from_disk() {
        let path = crate::decode::tests::temp_path("song.wav");
        crate::decode::tests::write_wav(&path, 48_000, 2, &vec![0.1; 9600]);
        let song = Song::load(&path, 48_000).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(song.frames(), 4800);
        assert_eq!(song.analysis.len(), 4800);
        assert!(song.name.ends_with("song.wav"));
    }
}
