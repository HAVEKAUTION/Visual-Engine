# Changelog

One short entry per task, newest first.

## 2026-11-09 — M2: renderer and first scenes
- wgpu renderer: full-screen WGSL scenes in the preview, one uniform block with every
  signal (+ time, resolution, built-in audio clocks), ping-pong feedback of the previous
  frame, optional buffers A/B (persistent or not) and multi-pass scenes.
- Scene format: `.wgsl` with an ISF-style JSON header (name, author, inputs, clocks,
  buffers, passes). Inputs become sliders / checkboxes / colour pickers. "Clocks" make
  motion follow the drive-speed-not-position rule. See docs/SCENES.md.
- Hot reload: saving a scene recompiles it; errors are shown and the last working
  version keeps running.
- Three scenes: Kaleido Bloom (psychedelic), Radial Spectrum (minimal), CRT Glitch
  (retro/glitch).
- Scene picker panel, F11 / double-click fullscreen (Esc leaves), FPS counter (F),
  meters toggle (M, now off by default), optional waveform overlay.

## 2026-11-09 — M1 part 2: rhythm and structure
- Onsets: spectral flux with adaptive threshold (λ·median + κ, 50 ms minimum gap),
  overall + kick/snare/hat; one-bin max filter stops kick pitch sweeps double-triggering.
  Onset times calibrated to within a few ms.
- Tempo: weighted autocorrelation (log-Gaussian around 120 BPM), sub-hop precision,
  smoothing, confirmed jumps, held in silence.
- Beat clock: phase locked to kicks, beat/bar/phrase phase and counters.
- Structure: build (loudness slope + rising onset rate), drop (bass jump after a build,
  re-aligns the phrase grid), section change (cosine distance of 4-bar spectra).
- Debug panel: onset lights, BPM, beat-phase circle, bar/phrase counters, build meter;
  event log in the side panel.
- T = tap tempo, R = resync downbeat, "Auto tempo" button.
- "Analyse file offline" (and `--analyse` on the command line) saves a CSV per hop.

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
