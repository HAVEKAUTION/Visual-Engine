# Audio-Reactive Visual Engine (working title)

A native desktop app (Windows + macOS) that analyses music in real time and drives
live, performable visuals: shaders, 3D, minimal and glitch styles. Built for live VJ
sets (keyboard, MIDI, fully automatic), stream output and 4K video export.

## Who's who

- **The user is the play-tester, not a programmer.** Claude Code writes all the code.
- After every task, tell the user in plain English:
  1. what changed,
  2. the exact command to run it (copy-pasteable),
  3. what to look for to confirm it works,
  4. anything that's known not to work yet.
- Keep `TESTING.md` up to date with a checklist per milestone.
- Keep `CHANGELOG.md` with one short entry per task.
- Before handing over: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` must pass.
- Commit after each working step with a clear message. Never leave the project in a
  state that doesn't build.
- If something needs the user to install or click something, give step-by-step
  instructions for both Windows and macOS.

## Tech stack

- Language: **Rust** (stable, latest), Cargo workspace.
- GPU: **wgpu**, shaders in **WGSL** (naga for importing GLSL later).
- Window + UI: **eframe/egui** with the wgpu backend (winit underneath).
- Audio I/O: **cpal**. Decoding: **symphonia** (MP3, WAV, FLAC, OGG).
- FFT: **realfft**. Lock-free audio → analysis handoff: **rtrb** ring buffer.
- Later: **midir** (MIDI), **rosc** (OSC), Spout (Windows) / Syphon (macOS) / NDI
  output, **ffmpeg** (spawned process) for video export, Ableton Link.
- Presets/config: **serde** + RON or TOML. Errors: **anyhow**/**thiserror**. Logs: **log** + **env_logger**.
- Use current versions of crates; check they compile together before committing.

## Architecture (crates in the workspace)

```
crates/
  app/        # binary: window, UI panels, wiring
  audio/      # input sources (file, mic/line-in, system loopback), playback
  analysis/   # pure DSP: spectrum, bands, onsets, tempo, beat phase, structure
  signals/    # the Signal Bus: named, normalised signals + modulation rule
  render/     # wgpu renderer, scene graph, layers, feedback buffers, outputs
  scenes/     # built-in scene shaders (.wgsl + metadata header)
```

- `analysis` must be **pure and deterministic**: same samples in → same signals out.
  It must run both live and offline (for tests and for video export).
- The audio callback thread is real-time: **no allocation, no locks, no I/O, no
  logging** inside it. Push samples into an rtrb ring buffer and return.
- Analysis runs on its own thread; it publishes a `SignalFrame` snapshot that the
  render thread reads (triple buffer or atomic swap, never blocking the renderer).

## The maths spec (implement exactly; tune constants via config)

All signals are updated every hop (~94 times/sec) and **normalised so 1.0 = typical
for the current song**. All smoothing uses elapsed time, never frame counts, so the feel
is identical at any frame rate.

1. **Spectrum.** 48 kHz, window N = 2048, hop H = 512, Hann window, realfft.
   Log-compressed magnitude: `Y[k] = ln(1 + γ·|X[k]|)`, γ ≈ 100.
2. **Bands.** 32 mel-spaced bands, `mel(f) = 2595·log10(1 + f/700)`, plus six named
   bands: sub 20–60 Hz, bass 60–250, low_mid 250–500, mid 500–2k, high_mid 2k–6k,
   air 6k–16k. Band energy = mean power of its bins.
3. **Auto-gain.** Per band slow running mean `Ē += α_slow·(E − Ē)` (time constant
   ~8 s), level `ℓ = E / (Ē + ε)`. So 1.0 = typical. Clamp to a sane max (e.g. 8).
4. **Attack/release smoothing.** `s += α·(x − s)`, `α = 1 − exp(−Δt/τ)`, τ = attack if
   rising else release. Each band exposes:
   - `fast`: attack 5 ms, release 120 ms
   - `slow`: attack 300 ms, release 1.5 s
5. **Onsets.** Spectral flux on Y: `SF = Σ max(0, Y_n[k] − Y_{n−1}[k])`. Adaptive
   threshold `δ = λ·median(last M values) + κ`; onset if local peak above δ and
   ≥ 50 ms since last. Per-band flux gives `kick` (sub+bass), `snare` (low_mid+mid),
   `hat` (air) triggers.
6. **Tempo.** Autocorrelate the onset signal over the last ~6 s; weight lags with a
   log-Gaussian centred on 120 BPM: `W(τ) = exp(−(log2(τ/τ120))² / (2σ²))`;
   BPM from the best lag. Smooth BPM changes; hold last good BPM in silence.
7. **Beat phase.** Phase φ ∈ [0,1) advances by `BPM/60·Δt`; on each onset nudge it
   `φ += k·wrap(φ_onset − φ)`, k ≈ 0.1. Derive `bar_phase` (4 beats) and
   `phrase_phase` (8/16/32 bars). Ableton Link, when connected, overrides tempo/phase.
8. **Structure.**
   - `build`: positive slope (linear regression) of slow loudness over last 8 bars,
     with rising hat/onset rate.
   - `drop`: at a phrase boundary right after a build, slow bass jumps ≥ ~2×.
   - `section_change`: cosine distance between mean spectra of last 4 bars vs the
     4 before exceeds a threshold.
9. **Extra features.** Spectral centroid `C = Σ f·|X| / Σ |X|`, mapped to hue as
   `log2(C/Cmin) / log2(Cmax/Cmin)`; overall loudness; onset rate.
10. **Mapping rule (used by EVERY parameter everywhere):**
    `p = clamp(p0 + Σ dᵢ·Sᵢ(mᵢ(t)), pmin, pmax)` where m is any signal, an LFO
    synced to beat/bar phase, a MIDI/keyboard control or a random walk, and S is a
    shape: curve `x^g`, beat envelope `exp(−(t−t_hit)/decay)`, phase pulse
    `(1−φ)^k`, synced LFO `sin(2π·rate·bar_phase)`, step `floor(n·x)/n`.
11. **Design rule: drive speed, not position.** Motion parameters integrate an
    audio-modulated speed (`θ += v0·(1 + a·ℓ_slow)·Δt`) instead of jumping.

## Scenes and the control model

- Presets, Layers and Nodes are **three views of one scene graph**, not three systems.
  A preset = a saved scene graph. A layer = a node chain shown as a stack.
- A scene/effect = a WGSL shader + a metadata header (ISF-style JSON: name, inputs,
  ranges, defaults, passes, persistent buffers). Every declared input is
  automatically modulatable with the mapping rule.
- Every scene receives one uniform block with all signals (bands, fast/slow, onsets,
  bpm, phases, structure flags, centroid, time, resolution).
- Feedback (previous frame) buffers are a first-class feature.

## Milestones

- M0 App shell: window, load song / pick mic, playback.
- M1 Analysis + live meters (debug panel showing every signal).
- GATE: meters match kicks, snares, tempo on the user's own songs.
- M2 First scenes (one per style) with feedback, shader hot-reload.
- M3 Modulation + presets + keyboard hotkeys.
- GATE: a whole song looks great hands-off.
- M4 Layers, blend modes, MIDI learn.
- M5 Auto-pilot (builds/drops/phrases) + outputs (2nd screen fullscreen, Spout/Syphon/NDI).
- GATE: a full live set without a crash.
- M6 Node editor, ISF/Shadertoy import, 3D scenes.
- M7 Record a set → re-render as 4K video (ffmpeg), Ableton Link.

Do one milestone (or one part of a milestone) per task. Don't start the next
milestone's features early.
