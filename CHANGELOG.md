# Changelog

One short entry per task, newest first.

## 2026-11-10 — v2 M2 part 1: graph data model (`graph` crate)
- New `graph` crate, data only (no GPU, no UI): numbered and labelled nodes, typed plugs
  (Image, Mask, Signal, Trigger), wires checked for type and loops, Signal wires into
  parameters (carrying the mapping rule's shape and depth), groups inside groups with
  published controls, per-node resolution and bypass.
- Every image node gets an optional Mask input automatically.
- Every edit is a reversible command: undo/redo, with slider drags merged into one step.
- Save / load as RON; broken files are reported instead of loaded.
- 13 tests. Nothing changes in the app yet.

## 2026-11-10 — Node graph groundwork (paused, not visible in the app yet)
- Scene files can declare effects (`"kind": "effect"`, `"images"`) and dropdown inputs
  (`"type": "choice"`). Eight built-in effects: Merge, Glow, Trails, Mirror, Glitch,
  Colour Grade, Transform, Pixelate. Hidden from the scene list until layers exist.
- Still to do for step 1: the graph model, per-node rendering, the Layers panel and
  tests/docs for them.

## 2026-11-09 — Computer audio (system loopback)
- New **🔊 Computer audio** button under LIVE INPUT: the visuals follow whatever the
  computer is playing (Spotify, YouTube, a DJ app…), like OBS's desktop audio. Every
  output device also appears as "Computer audio: …" in the device list.
- Windows: WASAPI loopback, nothing to install. macOS: needs 14.2+ and the System Audio
  Recording permission.
- Harmless audio dropouts no longer stop a live input.

## 2026-11-09 — Rebuilt Iron Furnace (metal) and Velvet Mirrorball (house / disco)
- Iron Furnace: a furnace seen through rings of spinning sawblades. Guitars feed the
  core and spin the blades, kicks punch and throw bursts of sparks, snares crack it with
  lightning, drops go white-hot. 11 controls, palette-driven, 3 presets.
- Velvet Mirrorball: velvet curtains, a lit dance floor that changes pattern on every
  kick, a faceted mirrorball with hi-hat sparkles, sweeping beams in the haze and light
  spots circling the room. Colours rotate every 4 bars. 12 controls, 3 presets.
- Keys 8 and 9 now load Forge and Saturday Night.

## 2026-11-09 — M3: modulation, presets, colour palettes, performance keys
- Every slider has a **~** button: link it to the music (any signal), a beat-synced
  LFO, a macro knob or a slow random wander, shaped as curve / beat envelope / phase
  pulse / LFO / steps. Uses the mapping rule from the brief; the orange marker shows the
  live value.
- Colour palettes: every scene has 5 colours. Pick the scene's own, one of 12 built-in
  palettes, or your own colours, live.
- Presets: save / load / delete, stored as files in `crates/scenes/presets`, 2–3 examples
  for seven scenes. Keys 1–9 crossfade to the preset on that key (length in beats).
- Performance keys: Space blackout, F flash on next beat, arrows intensity / speed,
  0 reset, H cheat sheet. FPS counter moved to I. Eight macro knobs.
- Warehouse Tunnel reworked: wall waves, colour cycle, accent frames, core glow, strobe.

## 2026-11-09 — M2 extra: genre scene pack + gate test plan
- Six new scenes, each made for a style of music: Canopy Breaks (jungle / drum & bass),
  Warehouse Tunnel (techno), Iron Furnace (metal), Midnight Window (lo-fi),
  Velvet Mirrorball (house / disco), Aurora Drift (ambient / acoustic).
- Scene headers can list `"genres"`; the picker shows them under each scene name.
  The original three are tagged too (psytrance, any/pop, synthwave).
- TESTING.md: practical test plan for checking the analysis on your own songs.

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
