# Direction Update v2 — Audio-Reactive Visual Engine

**For Claude Code:** this document updates the project's direction. The original plan
(audio analysis → shader scenes → presets → layers) still stands for the audio
analysis and the maths, but several parts have changed. Where this document and
`CLAUDE.md` or existing code disagree, **this document wins**, unless doing so would
throw away working, tested code: in that case, flag it and propose a migration
before changing anything.

**For the user:** drop this file in the project root and point Claude Code at it
using the session prompts.

---

## What changed from the original plan

| Area | Original plan | Now |
|---|---|---|
| Visual structure | Scenes + a layer stack, with nodes as an advanced view | **One node graph is the core** (DaVinci Resolve-inspired, simplified). Layers are built from nodes. |
| Presets | Built-in preset banks per scene | **No generic presets.** 6–8 hand-made **Looks** with character + **Look DNA** (mutate/breed) |
| Auto-pilot | Switches presets on structure | **The Director**, with temperaments (Minimalist, Maximalist, Storyteller) |
| Inputs | Audio only | Audio **plus images, video files and live cameras**, all as graph sources; video motion becomes signals |
| Compositing | Basic blend modes on layers | **Full Photoshop/Procreate blend modes** (Hard Light, Soft Light etc.), masks and keys on every node |
| UI | Side panel + preview | **One workspace, no pages**, Resolve-like clarity; Tab toggles Perform mode |
| Safety | Not covered | Flash limiter (WCAG), A/V latency calibration, frame budget, panic key, autosave |
| Milestones | M0–M7 | M0–M10 (below). M0 and M1 (app shell, audio analysis) are unchanged. |

**Unchanged:** Rust + wgpu + egui stack, real-time audio rules, the audio maths
(spectrum → bands → auto-gain → smoothing → onsets → tempo → beat phase →
structure), the mapping rule, "drive speed, not position", testing/commit workflow.

**Expect more change here:** the node system below is a **first pass**. It will be
refined heavily later. Keep the graph data model, the evaluation and the editor UI
cleanly separated, so the editor can be redesigned without touching evaluation and
vice versa.

---


A native desktop app for Windows (macOS dropped 2026-11-10) for live, music-driven visuals. Audio, images,
video files and live cameras all flow through **one node graph** (inspired by
DaVinci Resolve's Color/Fusion nodes, simplified). Performable live (keyboard, MIDI,
fully automatic), streamable (Spout/Syphon/NDI) and exportable (4K video).

**The north star:** it should feel like an instrument with a personality, not a
preset browser. Nothing generic. Every built-in thing is hand-made and has a reason
to exist.

---

## Who's who and how we work

- **The user is the play-tester, not a programmer.** Claude Code writes all the code.
- After every task, tell the user in plain English: what changed, the exact command
  to run it, what to look for, and what doesn't work yet.
- Keep `TESTING.md` (checklist per milestone) and `CHANGELOG.md` up to date.
- Before handing over: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` pass.
- Commit after each working step. Never leave the project not building.
- Installs or settings the user must do: step-by-step for Windows.
- One milestone (or part of one) per task. Don't build later milestones early.
- If this brief conflicts with existing code, **say so and propose a migration**
  before rewriting anything large.

---

## Tech stack

- **Rust** (stable), Cargo workspace. **wgpu** + **WGSL** (naga for GLSL import).
- UI: **egui** (eframe, wgpu backend), heavily custom-styled (see UI section).
- Audio: **cpal** (I/O), **symphonia** (decode), **realfft**, **rtrb** ring buffers.
- Video files: **ffmpeg** (via `ffmpeg-next` or a child process piping raw frames;
  pick one, explain why). Support H.264/H.265/ProRes/VP9 and **HAP** (GPU-friendly
  VJ codec) if feasible.
- Cameras: **nokhwa** (or the best current cross-platform camera crate; check).
- Images: **image** crate (PNG, JPG, WebP, with alpha).
- Later: **midir**, **rosc**, Spout/Syphon/NDI, **ort** (ONNX Runtime) for
  segmentation, Ableton Link.
- serde + RON for project/look files. anyhow/thiserror. log + env_logger.

## Architecture

```
crates/
  app/        # binary: window, workspace UI, wiring
  audio/      # audio sources (file, mic/line-in, loopback), playback
  analysis/   # pure DSP: spectrum, bands, onsets, tempo, phase, structure
  media/      # video decode, camera capture, image loading -> GPU textures
  graph/      # node graph data model, evaluation, undo/redo, serialisation
  nodes/      # built-in node implementations (+ their WGSL)
  signals/    # signal bus + modulation rule
  render/     # wgpu device, texture pool, compositor, outputs, export
  looks/      # signature Looks, Look DNA (mutate/breed), Director
```

Threading rules (non-negotiable):
- Audio callback: no allocation, locks, I/O or logging. Push to rtrb and return.
- Analysis, video decode and camera capture each run on their own threads and hand
  the newest result to the renderer **without blocking it** (latest-frame-wins).
- The output window must never stall because of UI work, file loading or a bad
  shader. A failing node outputs its last good frame (or passes input through) and
  shows an error badge.

---

## The audio maths (unchanged from v1; implement exactly)

All signals update every hop (~94/s), **normalised so 1.0 = typical for this song**,
and all smoothing uses elapsed time (identical feel at any frame rate).

1. Spectrum: 48 kHz, N = 2048, hop 512, Hann, `Y[k] = ln(1 + γ|X[k]|)`, γ ≈ 100.
2. Bands: 32 mel bands `mel(f) = 2595·log10(1 + f/700)`; named bands sub 20–60,
   bass 60–250, low_mid 250–500, mid 500–2k, high_mid 2k–6k, air 6k–16k Hz.
3. Auto-gain: `Ē += α_slow(E − Ē)` (~8 s), `ℓ = E/(Ē + ε)`, clamp ≤ 8.
4. Attack/release: `s += α(x − s)`, `α = 1 − exp(−Δt/τ)`; `fast` 5 ms/120 ms,
   `slow` 300 ms/1.5 s.
5. Onsets: spectral flux `SF = Σ max(0, Y_n − Y_{n−1})`, threshold
   `δ = λ·median(last M) + κ`, ≥ 50 ms apart; kick / snare / hat per band group.
6. Tempo: autocorrelation of onset signal over ~6 s with log-Gaussian weight
   centred on 120 BPM.
7. Beat phase: `φ += BPM/60·Δt`, nudged on onsets `φ += k·wrap(φ_onset − φ)`,
   k ≈ 0.1; bar and phrase (8/16/32 bar) phase. Link overrides when connected.
8. Structure: build (positive loudness slope over 8 bars + rising hats), drop
   (bass ≥ ~2× at phrase boundary after build), section_change (cosine distance of
   4-bar mean spectra).
9. Extras: spectral centroid → hue `log2(C/Cmin)/log2(Cmax/Cmin)`, loudness,
   onset rate.
10. Mapping rule for EVERY parameter:
    `p = clamp(p0 + Σ dᵢ·Sᵢ(mᵢ(t)), pmin, pmax)`. Shapes: curve `x^g`, beat
    envelope `exp(−(t−t_hit)/decay)`, phase pulse `(1−φ)^k`, synced LFO
    `sin(2π·rate·bar_phase)`, step `floor(n·x)/n`.
11. Drive speed, not position: `θ += v0·(1 + a·ℓ_slow)·Δt`.

**New video signals** (also on the signal bus, same normalisation):
- `motion`: mean absolute frame difference of a camera/video (downsampled, smoothed).
- `motion_x`, `motion_y`: centroid of motion (where movement is happening).
- `brightness`, `video_hue`: mean luma and dominant hue of a source.
- Later: optical flow field (texture), person mask (texture).

---

## The node system (the heart of the app)

### Data types and wires (colour-coded)
| Type | What it carries | Wire colour |
|---|---|---|
| Image | RGBA texture with alpha | white/grey |
| Mask | single-channel texture (keys, mattes) | green |
| Signal | one float per frame (audio, video, LFO, MIDI) | blue |
| Trigger | events (onsets, drops, MIDI notes, keys) | orange |

### Resolve-style behaviour (simplified)
- Nodes are **numbered** in creation order and can be **labelled**.
- **Every image node** has an optional **mask input** (limits where its effect
  applies) and outputs **alpha**. This is how layering and keys work everywhere.
- Quick-add from a selected node: **S** = add serial node after it,
  **P** = add parallel branch (auto-inserts a Mixer), **L** = add layer
  (auto-inserts a Layer Mixer, lower input on top, like Resolve).
- **Shift+Space** = search box to add any node by name (Fusion-style).
- **Ctrl/Cmd+D** = bypass the selected node; **Shift+D** = bypass the whole graph
  (instant before/after).
- **Click a node** to show its output in the viewer; **split viewer** compares the
  selected node with the final output (wipe line).
- **Groups**: select nodes → Group. A group shows as one node; any parameters
  inside can be **published as macros** (these become performance knobs/pads).
- **Versions**: a node or group can hold versions A/B/C to flip between live.
- Each node shows a tiny **GPU cost** badge (timestamp queries); the graph header
  shows total frame time.
- Undo/redo for every edit; copy/paste nodes between projects; autosave every 30 s
  and crash recovery on next launch.

### Modulation inside the graph
- Any parameter can take a **Signal wire** (drag a blue wire onto a parameter in the
  Inspector or onto the node) or be edited in the Inspector's modulation editor.
  Both are the same data underneath (the mapping rule).
- Modulated parameters show a live indicator; wires animate subtly with their signal.

### Built-in node library (v1 set; each hand-made, no filler)
- **Sources:** Image, Video File, Camera, Shader Generator, Text, Solid/Gradient,
  Feedback (previous frame of any node).
- **Combine:** Merge (2 inputs + mask, blend mode, opacity), Layer Mixer
  (N inputs, each with blend mode + opacity), Key Mixer (combine masks:
  add/subtract/intersect/difference).
- **Keys/masks:** Luma Key, Chroma Key (green screen), Shape Mask (circle, rect,
  polygon, feathered), Motion Mask, Gradient Mask; later Person Mask (segmentation).
- **Look/colour:** Colour Wheels (lift/gamma/gain/offset), Curves, Hue Shift,
  Posterize, Gradient Map, Duotone.
- **Space/shape:** Transform (pos/scale/rot/anchor), Corner Pin, Mirror/Kaleido,
  Tile, Polar ↔ Cartesian, Displace (by any image/mask), Liquify/Warp.
- **Texture/time:** Blur (gaussian/zoom/radial), Glow, Grain, Halftone, Pixel Sort,
  RGB Split, Datamosh-style smear, Echo/Trails, Time Slice (slit-scan), Strobe
  (safety-limited), Stutter (frame hold on triggers).
- **Signal nodes:** Audio Signal (pick any bus signal), Video Signal, LFO (beat-synced),
  Math (add/mul/remap/clamp), Smooth, Envelope (trigger → envelope), Random Walk,
  Counter (counts triggers, e.g. bars), MIDI/Key input.
- **Output:** Program Out (main), plus optional extra outputs (second screen, Spout…).

### Graph evaluation
- Topological order; evaluate only nodes reachable from active outputs.
- Texture pool reuse; per-node resolution (inherit / half / quarter / custom).
- **Proxy mode** for the editor preview; full resolution on outputs and export.
- Deterministic given the same inputs and time (needed for export).

---

## Media: images, video and cameras

- Media Pool holds images, video files, camera devices and audio files. Drag any into
  the graph to create a source node.
- **Video playback modes:** normal, loop, ping-pong, **beat-locked** (clip length
  stretched to N bars), **scrub by signal** (e.g. playhead follows bass_slow),
  **stutter** (jump/hold on triggers), speed × signal.
- Cameras: device + resolution + frame-rate picker, mirror toggle, latency shown.
- Decode/capture threads deliver frames as GPU textures; latest frame wins.
- Missing media on load: a relink dialog, never a crash.

### Blend modes (Merge and Layer Mixer) — W3C/Photoshop maths
Backdrop B, source S, per channel in [0,1]:
- Normal `S` · Multiply `B·S` · Screen `B + S − B·S` · Overlay `HardLight(S→B swapped)`
- Darken `min(B,S)` · Lighten `max(B,S)` · Difference `|B − S|` · Exclusion `B + S − 2BS`
- Add (linear dodge) `min(1, B+S)` · Subtract `max(0, B−S)`
- Colour Dodge: `B=0 → 0; S=1 → 1; else min(1, B/(1−S))`
- Colour Burn: `B=1 → 1; S=0 → 0; else 1 − min(1, (1−B)/S)`
- **Hard Light:** `S ≤ 0.5 → B·2S; else Screen(B, 2S − 1)`
- **Soft Light:** `S ≤ 0.5 → B − (1−2S)·B·(1−B); else B + (2S−1)·(D(B) − B)`,
  `D(B) = B ≤ 0.25 ? ((16B − 12)B + 4)B : √B`
- Vivid/Linear/Pin Light, Hue, Saturation, Color, Luminosity (non-separable, W3C).
- Composite: `C = (1 − αb)·S + αb·Blend(B,S)`, then source-over with opacity × αs ×
  mask.
- Blend in **sRGB-encoded space by default** (matches Photoshop/Procreate feel);
  per-node toggle for linear-light blending.

---

## UI: one workspace, no pages

Inspired by DaVinci Resolve's clarity, but **one screen** that reshapes instead of
separate pages.

```
┌───────────────┬───────────────────────────────┬────────────────┐
│ Media | Looks │            VIEWER             │   INSPECTOR    │
│ (tabs)        │  (program / selected node /   │ (selected node │
│               │   split wipe compare)         │  params + mod) │
├───────────────┴───────────────────────────────┴────────────────┤
│                         NODE GRAPH                              │
├─────────────────────────────────────────────────────────────────┤
│ SIGNAL STRIP: bands · kick/snare/hat · BPM · beat/bar/phrase · fps│
└─────────────────────────────────────────────────────────────────┘
```
- **Tab** toggles **Perform**: the node graph slides away and is replaced by the
  macro deck (knobs, pads, look switcher, Director controls); the viewer grows.
  Same screen, same state, instant.
- Panels resize and collapse; layout is remembered.
- Style: deep charcoal, one warm accent colour, compact clean type, thin dividers,
  generous spacing, no heavy borders, no gradients. Motion is subtle and purposeful
  (panel slides, a gentle beat pulse in the signal strip only).
- Keyboard-first, with a shortcut overlay (**?**).
- Copy is human and brief. Empty states have character
  (e.g. "Nothing's playing. Drop a song here, or press M to listen.").

---

## Personality: Looks, not presets

- **No generic preset banks.** The app ships with a small set (6–8) of **signature
  Looks**, each a hand-built node graph with a name, a one-line character
  description and a **temperament** (what it listens to and how it reacts: e.g.
  patient = follows slow energy; twitchy = lives on hats; brooding = bass and dark
  palettes). Each Look must show off a different idea (feedback, video, type,
  geometry, glitch…), not variations of one.
- **Look DNA:** a Look = graph + seed + **genes** (parameters marked as genes, each
  with a range).
  - **Mutate:** perturb genes around current values (strength slider), new seed.
  - **Breed:** two Looks with compatible graphs mix genes (random per gene, or by
    weight). Incompatible graphs → **splice**: keep A, graft one branch of B in
    through a Merge with a random blend mode.
  - Lineage is saved (parents, seed), so you can go back, and every user ends up
    with Looks nobody else has.
- **The Director** (auto-pilot) has temperaments too: *Minimalist* (few changes,
  long holds), *Maximalist* (changes on every phrase, big drops), *Storyteller*
  (follows song structure closely, saves the biggest Look for the biggest drop).
  It changes Looks and mutates genes on phrase boundaries, never mid-phrase.

---

## Safety and pro issues (solve these properly)

- **Photosensitivity guard (on by default):** measure frame-to-frame mean
  luminance change and red saturation on a downsampled output; keep large flashes
  to **≤ 3 per second** (WCAG 2.3.1 general/red flash thresholds) by damping
  strobe/flash amounts. Can be turned off with a clear warning, per project.
- **A/V latency calibration:** global offset (−200…+200 ms) with a tap-along
  calibration (flash + click), for Bluetooth speakers, projectors and cameras.
- **Frame budget:** if frame time exceeds budget, lower proxy resolution of the
  most expensive nodes first; show it in the signal strip.
- **Panic key** (Esc held): instant fade to a safe Look or black.
- **Never lose work:** autosave, crash recovery, missing-media relink.

---

## Milestones (v2)

- M0 App shell · M1 Audio analysis + signal strip → **GATE: analysis feels right**
- M2 Graph engine (data model, evaluation, texture pool, undo/redo, save/load)
- M3 Workspace UI + node editor (Resolve-style) + inspector + viewer compare
- M4 Media: images, video files (playback modes), cameras, video signals
- M5 Compositing: Merge + Layer Mixer with all blend modes, masks/keys, transforms
  → **GATE: a song + a video + a photo layered and reacting looks great**
- M6 Node library (full v1 set) + signature Looks + Look DNA
- M7 Perform mode: macros, groups/versions, hotkeys, MIDI learn, Director,
  safety guard, latency calibration, panic → **GATE: a full live set, no crash**
- M8 Outputs: second screen, Spout/Syphon, NDI, corner-pin/mesh warp
- M9 Video intelligence: motion flow, chroma/person keying via segmentation
- M10 Record + export (4K, vertical 9:16, square, loops), Ableton Link

## v2.1 additions (2026-11-10, from the user's playtest notes)

The user's wishes after M3, placed into the milestones. Research behind them is in
`RESEARCH.md`. ✅ = done.

**Node editor**
- ✅ Bolder nodes; Ctrl+scroll sizes nodes and text; per-node live picture on/off.
- ✅ Hold node ("on the drop, bend until the next hat / for N beats").

**Timing and logic (the "if this, then that, until …" depth)**, mostly M6, pulled
earlier where cheap:
- Trigger logic: **Logic** (AND / OR / NOT / XOR of triggers or gates), **Delay**
  (fire N beats/seconds later), **Chance** (fire with probability p, seeded),
  **Every Nth**, **Switch** (pick one of N signals or pictures by a counter/trigger),
  **Step Sequencer** (16 steps per bar, FL-style pattern grid, outputs signal +
  trigger), **Busyness** (onset rate as a smooth signal: how much is happening).
- Every image node keeps an audio-driven **mask** input (M5), so any sound can
  limit any effect to an area, and colour nodes take signals (M6).

**Media reacting to the music (M4/M5)**
- Image and video sources are ordinary picture nodes, so every effect and every
  signal works on them. M5/M6 add the "contort it" effects: Displace by noise or by
  another picture, Liquify/Warp, Bend, Twist, Ripple from the centre on kicks, Slit
  scan, Datamosh smear, Stutter. Each with an original twist (below).

**Original, not generic (M6)**
- Every built-in effect needs its own idea, not the textbook version. Example:
  **Kaleido** becomes a symmetry engine: n-gon folds (triangle, square, diamond,
  hexagon, star), all 17 wallpaper symmetry groups for tiling, a fold count that can
  be driven by signals (morphing smoothly between shapes), and a "breathing" centre.
- **ISF shader import** (GLSL + JSON header via naga): thousands of existing shaders.

**Text (M6)**: a Text source where each letter is its own instance: per-glyph
stagger, scale/offset/rotation by signals, beat-locked reveal, outline/extrude,
fonts from the system. Lyric tracks stay in the ideas bank.

**Depth: "part of the wall, not a screen" (M6, preview in M8)**
- 2.5D nodes: Depth from brightness (displaced relief with lighting), Parallax
  layers, Volumetric light and fog shafts, Lens/bloom/chromatic edge, LED-wall and
  CRT surface simulation, room reflections.
- **Venue preview** (M8): see the output projected on a wall in a dark virtual club,
  for judging how it reads in a room. Preview only, never in the output.

**UI to release quality** (new **M3b**, after M5)
- Full visual pass (spacing, type scale, icons, empty states, motion).
- **Themes**: a theme file (RON, CSS-like names: colours, radius, spacing, font
  sizes) loaded at start and live-reloaded; users can share theme files.
- **Dockable workspace** (Adobe/Unity/OBS style): drag panels anywhere, tabs, split,
  float, saved layouts ("Build", "Perform", "Stream"). Built on `egui_tiles` (egui
  0.37 compatible).

**Export (M10)**: visual only, or visual + audio. Audio comes from the song file,
or from what we *captured* live (computer audio / input), recorded alongside the
frames so live sets export too.

**DJs and MIDI (M7, pro level)**: MIDI learn on anything (knobs, pads, jog wheels,
notes → triggers, CC → signals, 14-bit CC), controller profiles with LED feedback
(Launchpad, APC40, DDJ…), MIDI clock in, Ableton Link (M10 → pulled into M7).
PRO DJ LINK (Pioneer) and StageLinQ (Denon) need licence programmes: apply before
release.

**Outputs (M8)**: projector mapping (mesh warp with Bézier points, masks, soft-edge
blending for several projectors), **Desktop mode** (live wallpaper behind the desktop
icons on Windows, desktop-level window on macOS, low-power option), Spout/Syphon/NDI.

**Tutorial (after M7)**: an interactive walkthrough inside the app (highlights a
panel, waits for the user to do the step), plus a starter Look per feature.

**Twitch: decided 2026-11-10, skip it.** Streaming is covered by Spout/NDI into OBS.

**Order decided 2026-11-10:** M4 media → M5 compositing → M3b UI pass (themes,
docking, polish) → M6 … Desktop mode ships with the M8 outputs.

**More playtest notes (2026-11-10, after M5 part 1)**
- **UI pass (M3b) must fix navigation first**:
  - Picking the sound source and the camera is buried at the bottom of the Media
    panel. Give **Sources** its own clear panel or toolbar menu: song, mic or line-in,
    computer audio, cameras, and the sync test.
  - Make the Inspector impossible to miss: it opens when a node is selected and has a
    clear header.
  - Panels you can collapse, move and float (Adobe style), with Resolve's calm layout
    but bolder.
- **Graph templates (M6)**: ready-made node layouts ("photo pulsing to the kick",
  "webcam keyed over a scene", "video glitch on the drop" and so on). Insert them from
  the right-click menu, each with a short note that explains it. The tutorial uses the
  same templates.
- **Starter picture pack (M6)**: original pictures generated by us (textures,
  gradients, shapes, masks), so new users have something to play with. They must be
  free to use.
- **More scenes like the user's favourite round, bubbling, sharp-edged scene (M6)**.
  Ask which one before building.
- **Datamosh (M6)**: smear motion across cuts. Fake it on the GPU (keep the previous
  frame, move it by motion vectors from the optical flow) rather than corrupting real
  codec data.
- **Stems and sampling** (ideas bank, after release): split a song into
  drums / bass / vocals / other (an ONNX source-separation model, offline) so each
  stem gets its own signals, and slice samples from it. Too big for now.

**Decided 2026-11-10 (after M5 part 1)**
- **Windows only.** No macOS work. A web version might come one day; not a goal.
- **Everything findable:** all features reachable from the Inspector or a right-click
  menu; shortcuts are extras. Each node has a short "How to use" note, and the
  Inspector shows getting-started steps when nothing is selected.

## Ideas bank (do NOT build until asked)
Lyric/typography tracks synced to the song · Spotify Canvas & TikTok loop export ·
DMX/Art-Net lighting sync · Pioneer/DJ link · OSC signals · phone as a
wireless controller · Look sharing (single file) and a community gallery · stems
input (separate drums/bass/vocals signals) · LED wall pixel mapping · AI generative
source node (later, optional, never required).
