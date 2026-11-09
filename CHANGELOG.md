# Changelog

One short entry per task, newest first.

## 2026-11-09 — M0: app shell
- Cargo workspace with `app`, `audio`, `analysis`, `signals`, `render`, `scenes`.
- Dark eframe/egui window on the wgpu backend (prefers the discrete GPU): side panel +
  large preview area.
- Open song (MP3/WAV/FLAC/OGG via symphonia) with play/pause, seek bar and volume.
- Live input picker (mic/line-in) via cpal, with rescan.
- All sources are converted to 48 kHz mono (windowed-sinc resampler) and pushed into a
  lock-free rtrb ring buffer, read by a stand-in analysis thread.
- Live waveform of the last 2 s of the analysis feed in the preview.
