# Changelog

One short entry per task, newest first.

## 2026-11-09 — M1 part 1: analysis core
- `analysis` crate: STFT spectrum (N 2048, H 512, Hann, `ln(1+100|X|)`), 32 mel bands +
  six named bands, per-band auto-gain (8 s running mean, 1.0 = typical, clamp 8),
  fast (5/120 ms) and slow (300 ms/1.5 s) attack/release smoothing, spectral centroid
  (+ hue), loudness. Pure and deterministic; all constants in `AnalysisConfig`.
- `signals` crate: `SignalFrame` + lock-free Signal Bus (triple buffer).
- Analysis thread runs every hop (~94/s) and publishes frames; UI never waits.
- Debug panel: 32 mel meters, six bands fast|slow, loudness and centroid.
- `visual-engine <song>` opens a song at startup.

## 2026-11-09 — M0: app shell
- Cargo workspace with `app`, `audio`, `analysis`, `signals`, `render`, `scenes`.
- Dark eframe/egui window on the wgpu backend (prefers the discrete GPU): side panel +
  large preview area.
- Open song (MP3/WAV/FLAC/OGG via symphonia) with play/pause, seek bar and volume.
- Live input picker (mic/line-in) via cpal, with rescan.
- All sources are converted to 48 kHz mono (windowed-sinc resampler) and pushed into a
  lock-free rtrb ring buffer, read by a stand-in analysis thread.
- Live waveform of the last 2 s of the analysis feed in the preview.
