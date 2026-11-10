# Audio-Reactive Visual Engine (working title)

## ⚠ Direction v2: read these first

The plan changed on 2026-11-10. Every session must read, in this order:

1. **`DIRECTION_UPDATE_v2.md`**: the current direction (one node graph, Looks instead
   of presets, media inputs, one workspace UI, safety, milestones M0–M10).
   **Where it disagrees with this file or with existing code, it wins**, unless that
   would throw away working, tested code: then flag it and propose a migration first.
2. **`BUILD_STEPS_v2.md`**: the step-by-step prompts per milestone. The user gives
   them one at a time; do only the step asked for.

This file keeps the rules that did not change (how we work, threading, the audio
maths) and records what in the code is now **legacy**.

---

A native desktop app for **Windows** for live, music-driven visuals (macOS dropped on
2026-11-10, see below). Audio, images,
video files and live cameras flow through **one node graph** (DaVinci Resolve-inspired,
simplified). Built for live VJ sets (keyboard, MIDI, fully automatic), stream output
and 4K video export.

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
  instructions for Windows.
- **Platform (decided 2026-11-10): Windows only.** Don't spend effort on macOS (no
  macOS testing, instructions or features such as Syphon). Keep code portable where
  it costs nothing, since a web port may happen one day, but that isn't a goal either.
  The goal is a complete, polished program that runs well and efficiently on the
  user's machine.
- **Everything findable (user, 2026-11-10):** every feature must be reachable from the
  Inspector or by right-clicking (a node, a plug, the graph). Keyboard shortcuts are
  extras, never the only way. New nodes get a "How to use" note (`app/src/tips.rs`).
- If the brief conflicts with existing code, say so and propose a migration before
  rewriting anything large.

## Tech stack

- Language: **Rust** (stable, latest), Cargo workspace.
- GPU: **wgpu**, shaders in **WGSL** (naga for importing GLSL later).
- Window + UI: **eframe/egui** with the wgpu backend, heavily custom-styled.
- Audio I/O: **cpal**. Decoding: **symphonia** (MP3, WAV, FLAC, OGG).
- FFT: **realfft**. Lock-free audio → analysis handoff: **rtrb** ring buffer.
- Media (v2, M4): **ffmpeg** for video, a camera crate (nokhwa or best current),
  **image** for stills. See `DIRECTION_UPDATE_v2.md`.
- Later: **midir** (MIDI), **rosc** (OSC), Spout / Syphon / NDI output, ffmpeg for
  export, Ableton Link, ONNX Runtime for segmentation.
- Project / Look files: **serde** + RON. Errors: **anyhow**/**thiserror**. Logs: **log** + **env_logger**.
- Use current versions of crates; check they compile together before committing.
  (`egui-snarl` does not support our egui version: the node editor is custom.)

## Architecture (crates in the workspace)

Target layout (v2):

```
crates/
  app/        # binary: window, workspace UI, wiring
  audio/      # audio sources (file, mic/line-in, loopback), playback
  analysis/   # pure DSP: spectrum, bands, onsets, tempo, phase, structure
  media/      # video decode, camera capture, image loading -> GPU textures   (new, M4)
  graph/      # node graph data model, evaluation, undo/redo, serialisation  (new, M2)
  nodes/      # built-in node implementations (+ their WGSL)                 (from scenes/)
  signals/    # signal bus + modulation rule
  render/     # wgpu device, texture pool, compositor, outputs, export
  looks/      # signature Looks, Look DNA (mutate/breed), Director           (new, M6)
```

Today the code has `app`, `audio`, `analysis`, `signals`, `render` and `scenes`.
`scenes` (shader header format, compile/validation, prelude, hot reload, clocks,
palettes) is planned to become `nodes`; see the migration below.

Threading rules (non-negotiable):
- `analysis` must be **pure and deterministic**: same samples in → same signals out.
  It must run both live and offline (for tests and for video export).
- The audio callback thread is real-time: **no allocation, no locks, no I/O, no
  logging** inside it. Push samples into an rtrb ring buffer and return.
- Analysis, video decode and camera capture each run on their own threads and hand
  the newest result to the renderer **without blocking it** (latest-frame-wins).
- The output must never stall because of UI work, file loading or a bad shader. A
  failing node outputs its last good frame (or passes its input through) and shows an
  error badge.

## The maths spec (unchanged in v2; implement exactly; tune constants via config)

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
    In v2 a Signal wire into a parameter and the Inspector's modulation editor are
    the same data underneath (this rule).
11. **Design rule: drive speed, not position.** Motion parameters integrate an
    audio-modulated speed (`θ += v0·(1 + a·ℓ_slow)·Δt`) instead of jumping.

v2 adds video signals (`motion`, `motion_x`, `motion_y`, `brightness`, `video_hue`) to
the bus in M4; see `DIRECTION_UPDATE_v2.md`.

## Legacy (v1) parts still in the code

These work and are tested, but are **out of date** with v2. Keep them working until
the v2 milestone that replaces them; don't extend them.

| v1 part (in the code today) | v2 replacement | Replaced in |
|---|---|---|
| Scene picker + side panels (`app/src/scene_view.rs`, `ui.rs`) | One workspace: viewer, inspector, node graph, signal strip; Tab = Perform | M3 |
| Debug meter panel | Signal strip | M3 |
| Two render "decks" with crossfade (`render`) | Texture pool + graph evaluation | M2 |
| `scenes` crate (header format, compile, hot reload, clocks, palettes) | `nodes` crate; scenes become Shader Generator nodes | M2 |
| Effect modules `shaders/fx_*.wgsl` (Merge, Glow, Trails, Mirror, Glitch, Colour Grade, Transform, Pixelate) | v2 nodes (Merge, Glow, Echo/Trails, Mirror/Kaleido, RGB Split, Hue Shift/Gradient Map, Transform, Posterize). Merge must move to the exact W3C blend maths | M2 / M5 |
| Presets (`scenes/presets/*.ron`), key bank 1–9 | **No generic presets**: 6–8 signature Looks + Look DNA | M6 |
| Blackout, flash, intensity, speed, macros (`app/src/perform.rs`) | Perform mode, Director, safety guard, panic key | M7 |

Removed ideas: the "Presets, Layers and Nodes are three views of one scene graph"
model and the planned Perform/Build/Audio pages. In v2 the node graph is the core,
layers are built from nodes (Layer Mixer), and there is one workspace with no pages.

Strobes and flashes have no photosensitivity guard until M7: keep every strobe
default **off**.

## Milestones

**Current plan: v2 milestones M0–M10 in `DIRECTION_UPDATE_v2.md`** (plus the
"v2.1 additions" section with the user's later wishes; research in `RESEARCH.md`),
built with the steps in `BUILD_STEPS_v2.md`. Done: M2, M3, M4, M5. Next up: **the M3b UI pass** (themes, docking, a clear
Sources panel, bolder look), then M6
(BUILD_STEPS_v2 Step 4). Then the M3b UI pass (themes, docking). No Twitch.

Done so far (v1 numbering, kept for history):
- M0 app shell, M1 analysis + meters, GATE (analysis on the user's songs): passed.
  These match v2 M0/M1.
- v1 M2 scenes + hot reload, genre scene pack, v1 M3 modulation + presets + palettes +
  hotkeys, computer-audio loopback, node-graph groundwork (effect modules). Now
  legacy, see above.

Do one milestone (or one part of a milestone) per task. Don't start the next
milestone's features early.
