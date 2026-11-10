# Changelog

One short entry per task, newest first.

## 2026-11-10 — Docs: README as a project page
- README rewritten for people who find the project on GitHub. It covers:
  - what the app does, with a picture of nine scenes;
  - installing on Windows and macOS step by step;
  - how to check the code yourself (no network, what it touches, tests, cargo audit);
  - a map of the workspace, the roadmap, and notes for developers.
- Licence files added (MIT or Apache 2.0, as `Cargo.toml` already said).
- The roadmap now covers the latest playtest notes: navigation fixes in the UI pass,
  graph templates, a starter picture pack, datamosh, and stems and sampling for later.

## 2026-11-10 — v2 M5 part 1: blend modes, masks, Layer Mixer
- **21 blend modes**, the exact W3C / Photoshop maths (Normal, Add, Screen, Multiply,
  Overlay, Difference, Lighten, Darken, Subtract, Exclusion, Colour Dodge / Burn,
  Hard / Soft / Vivid / Linear / Pin Light, Hue, Saturation, Colour, Luminosity),
  with alpha and opacity, in display values by default or linear light per node.
  A Rust reference with known-value tests; a GPU test checks every mode against it.
  Shaders can use them too: `blend_rgb` and `blend_over` in the prelude.
- **Merge** uses them (old graphs keep their mode).
- **Every node's Mask plug works**: the node shows where the mask is white and its
  input (or nothing, for a source) where it's black.
- **Layer Mixer** node: up to 8 layers, bottom to top, each with blend mode, opacity,
  solo and mute; move layers up/down in the Inspector (one undo step). The node grows
  a free plug as you wire it. **L** now adds a layer through a Layer Mixer (or into
  the next free slot of the one already there).
- Effects can take up to 4 pictures.
- Saved graphs pick up new settings and choices (e.g. Merge's new modes) when loaded.
- `VISUAL_ENGINE_DATA` now also holds the window layout and settings.

## 2026-11-10 — v2 M4 part 2: cameras, video signals, sound/picture sync
- **Camera** node (Sources): any webcam or capture card, via nokhwa (Media
  Foundation / AVFoundation / V4L2). Pick the camera, size and frame rate in the
  Inspector; mirror on by default; Hold (freeze) input. Captured on its own thread,
  newest frame wins; the node shows the real frame rate and an estimated delay.
  Cameras are listed in the Media pool (drag or double-click to add).
- **Video signals** on the signal bus: `motion`, `motion_x`, `motion_y`,
  `brightness`, `video_hue`, normalised like the audio ones (1 = typical for this
  source). They come from the first Image / Video / Camera node with "Feeds the video
  signals" on. The Inspector shows live meters. Pick them in an Audio Signal node or
  a modulation; shaders can read `sig.motion` etc.
- Video node: **Stutter** trigger (repeats a slice of N beats, M times, then carries
  on where it would have been).
- **Sound & picture sync** (Media tab): one offset, −200 to +200 ms. + makes the
  pictures wait (Bluetooth speakers); − makes them early (projectors): songs are
  analysed ahead, live input moves the beat clock ahead. A tap-along test (flash,
  then click) works the offset out.
- Graphs saved by older versions get new plugs and settings when loaded.

## 2026-11-10 — v2 M4 part 1: pictures and videos
- New `media` crate. Pictures (PNG, JPG, WebP, BMP, GIF, TIFF) load in the background.
  Videos are decoded by the ffmpeg program in a background process (H.264, H.265,
  ProRes, VP9, AV1, HAP...) into a frame cache, so loops and scrubbing are smooth;
  the renderer never waits for them.
- New **Image** and **Video** nodes (Sources). Fit / Fill / Stretch, mirror, scale.
  Video playback: Loop, Ping-pong, Play once, Beat-locked (one lap = N bars), Scrub
  (position follows a signal); speed can be driven by any signal; Restart and Jump
  (random) triggers and a Hold (freeze) input; start/end trim; decode size.
- Media pool (left panel): Import, thumbnails, drag into the graph (or double-click).
  Drop files from Explorer / Finder onto the window: pictures and videos join the pool
  (and land in the graph where you drop them), songs open.
- Missing files: the node shows an error, the Inspector lists what's missing and finds
  it again by name in a folder you pick; "Find file…" relinks one by hand.
- Renderer: media textures (uploaded only when the picture changes).

## 2026-11-10 — Editor polish: node size, per-node pictures, Hold node
- Ctrl+scroll makes nodes and their text bigger or smaller (80–160%, remembered);
  plain scroll still zooms the view. Nodes start 15% bigger, wires a bit bolder.
- Right-click a picture node → Hide / Show live picture (per node, saved with the
  graph; the toolbar's Thumbnails still turns them all off).
- New Hold node: Start trigger → 1 until the Stop trigger fires or a length runs out
  (beats, bars or seconds), with fade in / out. "On the drop, bend until the next hat."

## 2026-11-10 — v2 M3 part 3: groups, macro knobs, GPU cost
- Groups: select nodes, Ctrl+G. An open group has a lilac backdrop with a title (drag it
  to move everything, double-click to rename). ▾ folds it into one box that shows the
  wires crossing its edge as plugs, its macro knobs and a picture; double-click the box
  to open it again. Groups can sit inside groups. Ctrl+Shift+G ungroups (nodes stay;
  in a nested group they move up to the parent). Copy/paste and duplicate keep groups.
- Macro knobs: right-click any setting's name in the Inspector → Publish to group.
  Knobs show on the folded box (drag sideways, Shift = fine) and in the group's own
  Inspector (rename, reorder, remove). Dropping a blue wire on a folded box offers
  its knobs.
- GPU cost: each node shows the milliseconds it takes on the graphics card (green /
  yellow / red); groups show their total; the toolbar shows the graph's total.
  Uses GPU timestamps when the card has them (toolbar → Cost to hide).
- Graph files gain the group "folded" flag (older files still load).

## 2026-11-10 — v2 M3 part 2: workspace, depth, tidy graphs
- New one-screen workspace: toolbar (panel toggles, viewer mode, snap, thumbnails,
  undo/redo, fps, ?), Media | Looks tabs on the left, Inspector on the right, the graph
  below and a signal strip along the bottom (bands, K/S/H lights, BPM, beat dots, bar
  and phrase). New dark theme with one warm accent. Layout and toggles are remembered.
- Viewer: shows the Program (final output) by default; the selected node appears as a
  picture-in-picture (click it to view it full size). V toggles Program/Selected, W
  toggles a draggable wipe that compares the selected node with the Program.
- Live thumbnails inside every picture node (nodes not wired to the output render at
  quarter size just for their thumbnail).
- Ten new signal/trigger nodes: Math, Remap, Smooth, Envelope, Counter, Sample & Hold,
  Random, Audio Trigger (kick/snare/hat/hit/beat/bar/build/drop/section), Beat Trigger,
  Gate. LFO gained a phase offset. Orange trigger wires and lights flash when they fire.
- Tidy graphs: snap to grid (G), Frames (coloured backdrops with a title; drag the
  title to move everything inside, drag the corner to resize, double-click to rename),
  colour tags for nodes (right-click → Colour). Colour-coded categories in the menus.
- Quicker building: S inserts an effect after the selected node, P adds a parallel
  branch (auto 50/50 Merge), L adds a layer on top (auto Merge). Drop a wire on empty
  space to add a node already wired to it. Search also matches descriptions. Hover a
  node or plug for help; ? shows every shortcut.
- The starter graph now uses Remap (bass → zoom stays sensible) and a Frame.
- Graph files gain frames and node colours (older files still load).

## 2026-11-10 — v2 M3 part 1: the node editor
- The node graph is now the main view: a Resolve-style editor across the bottom of the
  window (Tab hides/shows it). The old one-scene panel is still there as "Classic
  scenes" at the top of the right panel.
- Right-click to add nodes (Sources, Effects, Signals, Output), or Shift+Space /
  double-click to search. Drag from a plug to wire; drag a wire off an input to unplug.
  Wires are colour-coded (picture grey, mask green, signal blue) and signal wires glow
  with their value.
- Drop a blue Signal wire on a node (pick a slider) or on a slider in the Inspector to
  make the music drive it. Hits (kick, snare…) get a decaying shape automatically.
- Click a node to see its picture in the viewer and edit it in the Inspector: name,
  on/bypass, resolution, every setting, modulation editor, and the wires driving each
  slider (shape, depth, on/off, unplug).
- Box select, Shift/Ctrl+click, Ctrl+A, Del, Ctrl+D bypass, Ctrl+C/X/V (pastes between
  graphs too), duplicate, Ctrl+Z / Ctrl+Shift+Z, Home to fit. Scroll zooms, middle-drag
  (or Space/Alt+drag) pans.
- Autosave every 30 s and on exit; your graph comes back on the next launch.
- Replaces the temporary "Node graph test" switch.

## 2026-11-10 — v2 M2 part 2: the graph runs on the GPU (first visible nodes)
- New `nodes` crate: node types (every scene and effect file becomes a node, plus Audio
  Signal, LFO and Program Out) and the evaluator that runs a graph every frame.
- Signal wires into sliders use the same mapping-rule maths as the modulation editor.
- Bypassed nodes pass their picture through; a broken node shows a warning and passes
  its picture through (or keeps its last good frame). The output never stops.
- The renderer keeps one picture per node and reuses freed textures (texture pool).
- Merge's Add mode is now clamped (`min(1, B + S)`), as v2 specifies.
- Temporary **🧪 Node graph test** switch at the top of the scene panel shows a 7-node
  demo graph, with a bypass tick box per node and live values for signal nodes.
- Tests: wire maths, demo graph draws and is identical every run (and after save/load),
  bypass and broken nodes pass through, empty output is black.

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
