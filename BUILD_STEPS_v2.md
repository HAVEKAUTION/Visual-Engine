# Build Steps v2: milestone-by-milestone instructions

Give these to Claude Code **one at a time, in order**, after it has read
`DIRECTION_UPDATE_v2.md` and you've agreed a migration plan (see the session
prompts). After each step, run the app, check it against TESTING.md, then move on.

M0 (app shell) and M1 (audio analysis) are unchanged from the original plan, so
the steps start at M2.

---

## Step 1: Graph engine (M2)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 2: the graph engine (no editor UI yet).

1. `graph` crate: node graph data model with typed ports (Image, Mask, Signal,
   Trigger), numbered nodes with labels, connections with type checking, groups
   (nested graphs) and published macro parameters. Serialise to RON.
2. Undo/redo for every graph edit (command pattern). Autosave every 30 s and crash
   recovery on next launch.
3. Evaluation: topological order, only nodes reachable from active outputs,
   texture pool reuse, per-node resolution (inherit/half/quarter), proxy mode.
   A node that errors outputs its last good frame and reports the error.
   Evaluation must be deterministic for a given time and inputs.
4. Modulation: any parameter can be driven by a Signal connection or by the
   mapping rule from the modulation editor, using the same underlying data.
5. Implement just enough nodes to prove it: Shader Generator, Transform, Merge
   (Normal/Add/Multiply/Screen only for now), Audio Signal, LFO, Program Out.
6. Temporary: build a small hard-coded test graph in code and render it in the
   window, so I can see a modulated shader through a transform and merge.
7. Tests: graph cycles rejected, type mismatches rejected, undo/redo round-trips,
   save → load gives an identical graph, deterministic render of a test graph.
8. fmt, clippy, tests, commit, test instructions.
```

---

## Step 2: Workspace + node editor (M3)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 3: the one-screen workspace and node editor.

1. Build the workspace layout from the UI section of DIRECTION_UPDATE_v2.md: Media/Looks
   (left, tabs), Viewer (centre), Inspector (right), Node Graph (bottom), Signal
   Strip (very bottom). Panels resize/collapse and the layout is remembered. Tab
   toggles Perform mode (for now, just collapse the graph and enlarge the viewer).
2. Custom egui theme: deep charcoal, one warm accent, clean compact type, thin
   dividers, no heavy borders or gradients. Show me screenshots-worthy polish; take
   inspiration from DaVinci Resolve's clarity but keep it simpler and calmer.
3. Node editor: pan/zoom, drag to connect with colour-coded typed wires (Image grey,
   Mask green, Signal blue, Trigger orange), node numbers + labels, the mask input
   on every image node, multi-select, box select, copy/paste, delete, groups.
   Shortcuts: S serial, P parallel (auto Mixer), L layer (auto Layer Mixer),
   Shift+Space search-to-add, Ctrl/Cmd+D bypass node, Shift+D bypass graph,
   ? shortcut overlay.
4. Click a node → the viewer shows its output. Split-wipe compare between the
   selected node and the final output. Per-node GPU cost badges and total frame time.
5. Inspector: parameters for the selected node, with the modulation editor (add
   source, shape, depth) and a live indicator on modulated params. Dragging a blue
   Signal wire onto a parameter also modulates it.
6. Signal strip: compact meters for bands, kick/snare/hat lights, BPM, beat/bar/phrase
   phase and fps.
7. Human, brief UI copy and characterful empty states (see DIRECTION_UPDATE_v2.md).
8. fmt, clippy, tests, commit, test instructions with a short "try this" tour.
```

---

## Step 3: Media: images, video, cameras (M4)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 4: media sources.

1. `media` crate. Images (PNG/JPG/WebP with alpha). Video files via ffmpeg:
   choose between ffmpeg-next and a piped child process and explain your choice;
   include H.264/H.265/ProRes/VP9, and HAP if feasible. Cameras: pick the best
   current cross-platform crate (check nokhwa and alternatives), with a device,
   resolution and fps picker plus a mirror toggle.
2. Decode/capture on background threads, upload as GPU textures, latest frame
   wins, never block the renderer. Show camera latency in the UI.
3. Media Pool panel: import files (and drag-and-drop from the OS), list cameras,
   thumbnails, and drag into the graph to create Image / Video File / Camera nodes.
   A relink dialog for missing media.
4. Video playback modes: normal, loop, ping-pong, beat-locked (clip stretched to
   N bars), scrub by signal, stutter on triggers, speed × signal.
5. Video signals on the bus: motion, motion_x, motion_y, brightness, video_hue
   (normalised like the audio signals).
6. Include the audio/video latency calibration from DIRECTION_UPDATE_v2.md (offset slider +
   tap-along flash/click test).
7. fmt, clippy, tests, commit, test instructions (e.g. "wave at your webcam and
   watch the motion meter").
```

---

## Step 4: Compositing, blend modes, keys (M5)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 5: layering and compositing.

1. Merge and Layer Mixer with ALL blend modes listed in DIRECTION_UPDATE_v2.md, implemented
   exactly with the given formulas (including Hard Light, Soft Light, Colour
   Dodge/Burn and the non-separable Hue/Saturation/Color/Luminosity), opacity,
   alpha and mask inputs. sRGB-encoded blending by default, linear toggle per node.
   Unit-test each blend mode against known values.
2. Layer Mixer: N inputs, each with blend mode, opacity, solo/mute, reorder in the
   Inspector (lower input on top, like Resolve).
3. Masks and keys: Luma Key, Chroma Key (with spill suppression), Shape Mask
   (circle/rect/polygon, feather), Gradient Mask, Motion Mask, and Key Mixer
   (add/subtract/intersect/difference). Every image node's mask input works.
4. Transform and Corner Pin nodes with on-viewer handles.
5. Build me a demo project: my song playing, a video clip in Hard Light over a
   shader, a photo masked by a beat-pulsing shape, and the webcam keyed in.
6. fmt, clippy, tests, commit, test instructions.
```

**GATE:** load your own song, a video and a photo, and layer them. Does it look and feel great? Tell Claude Code exactly what doesn't.

---

## Step 5: Node library + signature Looks + Look DNA (M6)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 6: the full node library and personality.

1. Implement the full v1 node library from DIRECTION_UPDATE_v2.md (sources, combine, keys,
   colour, space/shape, texture/time, signal nodes). Each node: sensible ranges,
   short human descriptions in the Inspector, and good defaults that already look
   good with music.
2. Signature Looks: design 6–8 hand-built Looks, each showing off a DIFFERENT idea
   (e.g. one built on feedback, one on the webcam, one on typography, one on
   geometry, one on glitch/datamosh, one on photo/video collage). Each has a name,
   a one-line character description and a temperament (what it listens to and how
   it reacts). No generic or filler Looks. Pitch me the list (names, idea,
   temperament) BEFORE building them, and wait for my feedback.
3. Look DNA: mark parameters as genes with ranges; Mutate (strength slider, new
   seed), Breed (gene mixing for compatible graphs; splice via Merge with a random
   blend mode for incompatible ones), and saved lineage with a family-tree view
   in the Looks panel. Look thumbnails render live.
4. Tests: mutate stays within gene ranges, breeding is reproducible from seeds,
   lineage survives save/load.
5. fmt, clippy, tests, commit, test instructions.
```

---

## Step 6: Perform mode, MIDI, Director, safety (M7)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 7: performing live.

1. Perform mode (Tab): the graph is replaced by the macro deck: published macros as
   knobs/faders, trigger pads, the Look switcher (with crossfade length in beats),
   node/group Versions A/B/C, and Director controls. The viewer grows. Same state.
2. Hotkeys: number keys switch Looks, F = flash on next beat, Space = blackout,
   arrows = global intensity/speed, M = mutate current Look, Esc held = panic fade.
3. MIDI (midir) and OSC (rosc) with MIDI learn on any control/macro/pad; mappings
   saved per project. MIDI/OSC also appear as Signal and Trigger nodes.
4. The Director with temperaments (Minimalist, Maximalist, Storyteller) as in
   DIRECTION_UPDATE_v2.md: changes Looks and mutates genes only on phrase boundaries, reacts to
   builds/drops/section changes, avoids repetition, and has a mood slider.
5. Photosensitivity guard exactly as specified (≤ 3 large or red flashes per second,
   on by default, clear warning when disabled). Frame-budget auto-proxy.
6. Stress test: run a 30-minute song loop with the Director on and log frame
   times, memory use and any errors. Fix leaks or stutters you find.
7. fmt, clippy, tests, commit, and a live-set rehearsal checklist in TESTING.md.
```

**GATE:** do a full set (rehearsal) with the Director and some hands-on. No crashes, nothing that looks generic.

---

## Step 7: Outputs (M8)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 8: outputs.

1. Output window on any monitor/projector, fullscreen, independent of the
   workspace; multiple outputs, each fed by any Output node.
2. Spout (Windows) and Syphon (macOS) senders so OBS and VJ apps receive the
   visuals. Research the best Rust option for each and explain it.
3. NDI: research the SDK and licence situation and tell me before adding it.
4. Projection warp per output: corner pin plus a simple mesh warp, with edge
   blending for two projectors.
5. fmt, clippy, tests, commit, test instructions (including OBS setup steps).
```

---

## Step 8: Video intelligence (M9)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 9: making video smarter.

1. GPU optical flow node (pick a real-time method and explain it) that outputs a
   flow texture and drives Displace, particles and Echo nodes, so movement in the
   camera pushes the visuals around.
2. Person Mask node: person segmentation via ONNX Runtime (ort) with a small,
   permissively licensed model. Check the licence and tell me before adding it.
   It must run in real time on my machine or fall back to a lower rate.
3. Build one new signature Look that uses both (e.g. visuals flowing around the
   performer). Pitch it first.
4. fmt, clippy, tests, commit, test instructions.
```

---

## Step 9: Record and export, Ableton Link (M10)

```
Read CLAUDE.md and DIRECTION_UPDATE_v2.md. Task: Milestone 10: recording, export and sync.

1. Record a performance: every action (Look changes, macro/MIDI moves, hotkeys,
   Director decisions) timestamped alongside the song and media.
2. Export: re-render offline via ffmpeg at any resolution up to 4K and in
   16:9, 9:16 (vertical), 1:1 and seamless-loop formats, with the analysis rerun
   offline so it matches exactly, audio included, progress shown, cancellable.
   Video sources must also be read deterministically (frame-accurate) for export.
3. Ableton Link: check the licence and the state of the Rust options, and tell me
   before adding it.
4. fmt, clippy, tests, commit, test instructions.
```

---

## Handy messages for any time

**Bug report**
```
Bug: [what you did] → [what happened] → [what you expected].
Media/song: [what]. Happens [always / sometimes].
Find the cause, fix it, add a test so it can't return, tell me how to verify.
```

**It crashed / won't build**
```
It crashed / won't build. Full error:
[paste]
Fix it, explain in one or two sentences what went wrong, re-run the checks.
```

**It feels wrong**
```
This feels [twitchy / laggy / off-beat / generic / cluttered]. Suggest 2–3
changes with reasons, and add tuning controls so I can adjust it while music plays.
```

**Make it less generic**
```
[Look / node / panel] feels generic. Give it more character: propose three
directions that would make it feel hand-made and specific, and wait for my pick.
```

**Polish pass**
```
Do a polish pass on [panel/feature]: spacing, alignment, type sizes, hover and
focus states, empty states, copy and keyboard shortcuts. Keep it clean and calm.
List what you changed.
```

**Check-in**
```
Summarise where the project is: done, left in this milestone, known bugs or
shortcuts taken, and the next prompt I should give you.
```
