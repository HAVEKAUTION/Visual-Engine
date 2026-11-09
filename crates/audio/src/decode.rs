//! Decoding audio files (MP3, WAV, FLAC, OGG/Vorbis) with symphonia.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// File extensions offered in the "Open song" dialog.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "ogg", "oga"];

/// A fully decoded audio file.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: usize,
    /// Interleaved samples in `[-1, 1]`.
    pub samples: Vec<f32>,
}

impl DecodedAudio {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }
}

/// Decode an entire file into memory.
pub fn decode_file(path: &Path) -> Result<DecodedAudio> {
    let file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("unsupported or unrecognised audio format")?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow!("the file has no audio track"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| anyhow!("missing audio codec parameters"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .context("unsupported audio codec")?;

    let mut samples: Vec<f32> = Vec::new();
    let mut scratch: Vec<f32> = Vec::new();
    let mut spec: Option<(u32, usize)> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            // Some files end with a truncated packet; keep what we have.
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e).context("error while reading the file"),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let rate = buf.spec().rate();
                let ch = buf.spec().channels().count();
                match spec {
                    None => spec = Some((rate, ch)),
                    Some(s) if s != (rate, ch) => {
                        bail!("the stream changes sample rate or channel count mid-file")
                    }
                    _ => {}
                }
                scratch.resize(buf.samples_interleaved(), 0.0);
                buf.copy_to_slice_interleaved(&mut scratch[..]);
                samples.extend_from_slice(&scratch);
            }
            // A corrupt packet: skip it, like most players do.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(e).context("error while decoding"),
        }
    }

    let (sample_rate, channels) = spec.ok_or_else(|| anyhow!("the file contains no audio"))?;
    if channels == 0 {
        bail!("the file has zero channels");
    }
    Ok(DecodedAudio {
        sample_rate,
        channels,
        samples,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// Write a 16-bit PCM WAV file (test helper).
    pub(crate) fn write_wav(path: &Path, rate: u32, channels: u16, samples: &[f32]) {
        let data_len = (samples.len() * 2) as u32;
        let mut f = File::create(path).unwrap();
        let block_align = channels * 2;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&(rate * block_align as u32).to_le_bytes())
            .unwrap();
        f.write_all(&block_align.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            f.write_all(&v.to_le_bytes()).unwrap();
        }
    }

    pub(crate) fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("visual-engine-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn decodes_a_stereo_wav() {
        let path = temp_path("decode.wav");
        let input: Vec<f32> = (0..2000)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.25 })
            .collect();
        write_wav(&path, 44_100, 2, &input);

        let decoded = decode_file(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(decoded.sample_rate, 44_100);
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.frames(), 1000);
        for (a, b) in decoded.samples.iter().zip(&input) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }

    #[test]
    fn missing_file_is_an_error_not_a_panic() {
        assert!(decode_file(Path::new("definitely/not/here.mp3")).is_err());
    }
}
