# Claude Code prompts: Audio-Reactive Visual Engine

Paste these into Claude Code **one at a time, in order**. Wait until each one is done,
run the app, and check it works before moving on. Paste the whole block, including
the line that starts with `Read CLAUDE.md`.

---

## Before you start (one-time setup)

1. Install **Git**: https://git-scm.com
2. Install **Rust** with rustup: https://rustup.rs (on Windows, accept the prompt to
   install the Visual Studio C++ build tools).
3. Install **Claude Code** and sign in.
4. Make an empty folder, e.g. `visual-engine`, and put **CLAUDE.md** inside it.
5. Open a terminal in that folder and run `claude`.

---

## Prompt 1: Project setup + app shell (M0)

```
Read CLAUDE.md fully. It's the project brief and you must follow it for every task.

Task: Milestone 0 (app shell).
1. Check my OS, Rust toolchain and GPU (tell me what you found). Initialise git and
   create the Cargo workspace with the crate layout from CLAUDE.md
   (app, audio, analysis, signals, render, scenes). Add a .gitignore, README.md,
   CHANGELOG.md and TESTING.md.
2. Build a window with eframe/egui on the wgpu backend: a dark UI, a large central
   preview area (just a dark background for now) and a side panel.
3. Audio sources in the side panel:
   - "Open song" (file dialog; MP3/WAV/FLAC/OGG via symphonia), with play/pause,
     a seek bar and volume.
   - "Live input" with a dropdown of input devices (mic/line-in) via cpal.
   - Research and tell me whether system-audio loopback capture is possible on my
     OS with cpal. Don't build it yet.
4. Resample to 48 kHz mono for analysis and push samples into an rtrb ring buffer
   that the (not yet written) analysis thread will read. Follow the real-time
   audio rules in CLAUDE.md strictly.
5. Show a simple live waveform of the last ~2 s in the preview area so I can see
   the audio is flowing.
6. Run fmt, clippy and tests, commit, then give me the exact run command and a
   short test checklist (also add it to TESTING.md).
```

---

## Prompt 2: Spectrum, bands, auto-gain, smoothing (M1, part 1)

```
Read CLAUDE.md. Task: Milestone 1, part 1: the analysis core.

1. In the `analysis` crate, implement maths spec items 1–4 exactly (spectrum, mel
   bands + six named bands, auto-gain, attack/release fast and slow versions),
   plus item 9 (centroid, loudness). Keep it pure and deterministic, with
   constants in a config struct.
2. Run it on its own thread, reading from the ring buffer, and publish a
   SignalFrame snapshot via the `signals` crate without ever blocking the UI.
3. Unit tests with synthetic audio: a 60 Hz sine must make bass/sub dominant,
   1 kHz → mid, 10 kHz → air; quiet vs loud versions of the same signal should give
   similar normalised levels after auto-gain settles; smoothing must behave the
   same at different Δt.
4. Debug panel in the app: live bar meters for the 32 mel bands, and for the six
   named bands show fast and slow side by side, plus centroid and loudness.
5. fmt, clippy, tests, commit; then tell me how to test it and what "good" looks like.
```

---

## Prompt 3: Onsets, tempo, beat phase, structure (M1, part 2)

```
Read CLAUDE.md. Task: Milestone 1, part 2: rhythm and structure.

1. Implement maths spec items 5–8: spectral-flux onsets with adaptive threshold
   (overall + kick/snare/hat), tempo by weighted autocorrelation, the phase-locked
   beat clock (beat, bar, phrase phase), and build/drop/section_change detection.
2. Tests: generate a synthetic 128 BPM kick pattern. BPM must come out at 128 ± 1
   (not 64 or 256), and onsets must land within ±20 ms of the kicks. Add a test
   for silence (no false onsets, BPM held).
3. Debug panel additions: kick/snare/hat lights that flash on onsets, a BPM readout,
   a circle showing beat phase, bar/phrase counters, and a scrolling event log for
   build / drop / section change with timestamps.
4. Add a "tap tempo" key (T) and a "resync to downbeat" key (R) as manual overrides.
5. Add "Analyse file offline": runs the analyser on a whole song as fast as
   possible and saves a CSV (time, bands, onsets, bpm, phases, events) next to the
   song, so I can check it against the music.
6. fmt, clippy, tests, commit, test instructions.
```

---

## Prompt 4: Gate check: help me verify the analysis

```
Read CLAUDE.md. I'm about to test the analysis on my own songs before we build any
visuals. Give me a short, practical test plan: which kinds of songs to try (e.g.
four-on-the-floor house, hip-hop, a slow acoustic song, a track with a big drop),
what each meter should do, and how to report problems to you. Then wait for my
results.
```

Then play your songs and tell Claude Code what you saw, e.g.
"On [song], BPM shows 87 but it's really 174" or "hats light up on every vocal".

---

## Prompt 5: Renderer + first scenes (M2)

```
Read CLAUDE.md. Task: Milestone 2: renderer and first scenes.

1. In `render`: a wgpu pipeline that draws full-screen WGSL scenes into the preview,
   with one uniform block containing every signal from the SignalFrame
   (see CLAUDE.md), plus time and resolution.
2. Scene file format: a .wgsl file starting with an ISF-style JSON metadata comment
   (name, author, inputs with type/min/max/default, passes, persistent buffers).
   Parse it, and show each declared input as a slider in the UI.
3. Feedback: support a persistent previous-frame buffer (ping-pong) that scenes
   can sample.
4. Hot-reload: when a .wgsl file changes on disk, recompile it live. On a shader
   error, keep the last working version and show the error in the UI.
5. Write three built-in scenes, all clearly audio-reactive using the "drive speed,
   not position" rule:
   - Psychedelic: domain-warped fractal/kaleidoscope with feedback trails, hue from
     centroid.
   - Minimal: a clean radial spectrum or line sculpture that pulses on kicks.
   - Retro/glitch: CRT scanlines, RGB split and block glitches triggered by
     snares and drops.
6. A scene picker and a fullscreen toggle (F11). Show an FPS counter.
7. fmt, clippy, tests, commit, test instructions.
```

---

## Prompt 6: Modulation + presets + hotkeys (M3)

```
Read CLAUDE.md. Task: Milestone 3: modulation, presets, keyboard control.

1. Implement the mapping rule (item 10) in the `signals` crate as one reusable
   Modulator type: base value + list of (source, shape, depth), clamped to range.
   Sources: any signal, synced LFOs (rate in beats/bars), manual controls, random
   walk. Shapes: curve, beat envelope, phase pulse, synced LFO, step.
2. Every scene input slider gets a small "modulate" button that opens a compact
   editor to add sources. Show the live modulated value on the slider.
3. Presets: save/load a scene + all its modulations to a file in a presets folder.
   Include 2–3 example presets per built-in scene.
4. Keyboard performance mode: number keys switch presets with a crossfade
   (crossfade length in beats), Space = blackout, F = flash on next beat,
   arrow keys = global intensity and speed. Show a hotkey cheat-sheet overlay (H).
5. Tests for the modulation maths (each shape, clamping, Δt independence).
6. fmt, clippy, tests, commit, test instructions.
```

---

## Prompt 7: Layers + MIDI (M4)

```
Read CLAUDE.md. Task: Milestone 4: layers and MIDI.

1. Layer stack: multiple scenes and effects rendered and blended (normal, add,
   screen, multiply, difference) with opacity. Add, remove, reorder, solo and mute.
   Effects (blur, mirror/kaleido, colour grade, feedback zoom, glitch) are also
   scene files with metadata, applied to the layer below.
2. Internally, layers must be a node chain in the scene graph (see "three views of
   one scene graph" in CLAUDE.md), even though the UI shows a stack.
3. Presets now save the whole layer stack.
4. MIDI via midir: device picker, and "MIDI learn": click any control, move a
   knob/pad, and it's mapped. Mappings are saved. MIDI controls also work as
   modulation sources. Pads can trigger presets and flashes.
5. Also add an OSC input (rosc) with the same learn system.
6. fmt, clippy, tests, commit, test instructions (tell me how to test MIDI without
   hardware, e.g. a virtual MIDI app).
```

---

## Prompt 8: Auto-pilot + outputs (M5)

```
Read CLAUDE.md. Task: Milestone 5: auto-pilot and outputs.

1. Auto-pilot: an on/off mode that runs a show by itself using the structure
   signals. It changes presets on section changes and phrase boundaries (never
   mid-phrase), goes bigger on drops, calmer in breakdowns, ramps intensity during
   builds, and avoids repeating the same preset too often. A playlist/pool of
   presets with weights, and a "mood" slider (calm ↔ intense).
2. Output window: send the final output to a second monitor/projector, fullscreen,
   separate from the control window. The preview stays in the control window.
3. Spout output on Windows and Syphon on macOS so OBS and VJ apps can receive the
   visuals. Research the best Rust option for each and explain your choice.
4. NDI output (network): research the SDK/licence situation first and tell me
   before adding it.
5. Stability: catch GPU device loss and shader errors without crashing; never let
   the output window go black for more than one frame because of a UI action.
6. fmt, clippy, tests, commit, and a "live set rehearsal" checklist in TESTING.md.
```

---

## Prompt 9: Node editor + imports + 3D (M6)

```
Read CLAUDE.md. Task: Milestone 6: node editor, shader import, 3D.

1. Node editor view of the scene graph (egui node-graph crate or custom): open any
   layer to see its nodes, add/connect nodes, and see changes live. Changes made
   here must show up immediately in the Layers and Presets views, and vice versa.
2. Signal nodes: any signal, math (add, multiply, remap, smooth), LFOs and shapes as
   nodes, so modulation can be wired visually as well as through slider editors.
3. Import: ISF (.fs) files and Shadertoy-style GLSL, translated to WGSL with naga.
   Map common uniforms (iTime, iResolution, iChannel0 as audio/feedback texture)
   automatically. Show clear errors when something can't be translated.
4. 3D scenes: a basic 3D pipeline (camera, depth, instanced meshes, particles) with
   two built-in scenes: an audio-reactive particle field and a tunnel/terrain flythrough.
5. fmt, clippy, tests, commit, test instructions.
```

---

## Prompt 10: Record, export, Ableton Link (M7)

```
Read CLAUDE.md. Task: Milestone 7: recording, export and tempo sync.

1. Record a performance: while playing, save a timeline of every action (preset
   changes, slider/MIDI moves, hotkeys) with timestamps, alongside the song.
2. Export: re-render a recorded set offline with ffmpeg at a chosen resolution
   (up to 4K) and frame rate, running the analysis offline on the song so it
   matches exactly. Include the audio in the file. Show progress and allow
   cancelling. Also allow simply exporting a song + preset without a recording.
3. Ableton Link: tempo and phase sync with DJs/DAWs. Check the licence situation
   and the state of the Rust options, and tell me before adding it.
4. fmt, clippy, tests, commit, test instructions.
```

---

## Handy prompts for any time

**Bug report**
```
Bug: [what you did] → [what happened] → [what you expected].
Song/input: [name or type]. It happens [always / sometimes].
Find the cause, fix it, add a test so it can't come back, and tell me how to verify.
```

**It won't build / it crashed**
```
It crashed / won't build. Here's the full error:
[paste it]
Fix it, explain in one or two sentences what was wrong, and re-run the checks.
```

**Something feels wrong rather than broken**
```
The visuals feel [too twitchy / laggy / not on the beat / samey]. Look at the
relevant analysis and modulation constants, suggest 2–3 changes, and add sliders
for them in a "Tuning" panel so I can adjust them myself while music plays.
```

**Check-in**
```
Summarise where the project is: what's done, what's left in this milestone, any
known bugs or shortcuts you took, and the next prompt I should give you.
```
