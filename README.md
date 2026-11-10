# Visual Engine (working title)

**Live visuals that listen to the music.** A free, open-source desktop app for
Windows. It analyses sound in real time (kicks, snares, hats, tempo, builds, drops)
and lets that drive anything you see. Your own photos, videos, webcams and shaders are
all wired together in one node graph, in the style of DaVinci Resolve.

Made for VJs, DJs, streamers and musicians who want visuals that move *with* a track,
not just alongside it.

> **Status: early, in active development.** It runs and is fun to play with, but it
> isn't a finished release yet. There are no ready-made downloads: you build it from
> the source code (steps below). Settings and saved files may change between versions.

![Nine of the built-in scenes](docs/images/scenes.jpg)

*Some of the built-in scenes. Every one reacts to the music.*

---

## What it does

- **Hears the music properly.** Spectrum, 32 bands, plus sub / bass / mids / highs.
  Kick, snare and hat hits. Tempo, and where you are in the beat, bar and phrase.
  It spots builds, drops and section changes. Levels auto-adjust to each song, so
  "1.0" always means "typical for this track".
- **Any sound source.** A song file (MP3, WAV, FLAC, OGG), a microphone or line-in,
  or whatever your computer is playing (Spotify, a DJ app, a browser).
- **One node graph.** Sources (shader scenes, pictures, videos, cameras) flow through
  effects into the output. Any setting can follow the music: wire a signal into it, or
  use the modulation editor (curves, beat envelopes, LFOs locked to the bar, steps).
- **Compositing.** All 21 Photoshop-style blend modes (Screen, Multiply, Overlay,
  Hard Light, Colour, Luminosity…). A Layer Mixer with solo, mute and reorder. A mask
  plug on every node, with Shape and Gradient masks that pulse to the beat, Luma and
  Chroma (green screen) keys, a Motion Mask that follows movement, and a Key Mixer.
- **Media.** Pictures, video files (loop, ping-pong, beat-locked, scrub, stutter) and
  live webcams. Movement, brightness and colour seen in a video or camera become
  signals too.
- **Built for playing live.** Undo everything, autosave, groups with macro knobs, a
  live preview on each node, GPU cost per node, and a sound-to-picture sync test.
  A broken shader never stops the show: it keeps its last good picture.

Coming next is listed in the [Roadmap](#roadmap).

## Install (build from source)

It runs on **Windows 10 and 11** (macOS isn't supported). You need two things:
**Rust** (the programming language the app is written in) and, for video files,
**ffmpeg**. Building takes 5–10 minutes the first time and
seconds after that.

### Windows 10 / 11

1. Install the Microsoft C++ build tools. Download "Build Tools for Visual Studio"
   from <https://visualstudio.microsoft.com/visual-cpp-build-tools/>, run it, tick
   **Desktop development with C++**, and press Install.
2. Install Rust: download and run `rustup-init.exe` from <https://rustup.rs>. Accept
   the defaults.
3. Optional, for video files: open **Terminal** and run
   `winget install Gyan.FFmpeg`
4. Download the code. On this page, click the green **Code** button, then
   **Download ZIP**, and unzip it. Or, if you have git:
   `git clone https://github.com/HAVEKAUTION/Visual-Engine.git`
5. Open **Terminal** in that folder (in File Explorer, right-click the folder and choose
   *Open in Terminal*), then run:

   ```
   cargo run --release
   ```

### Handy extras

- Open a song straight away: `cargo run --release -- "C:/Music/song.mp3"`
- Analyse songs without opening the window. This writes `<song>.analysis.csv` next
  to each one:
  `cargo run --release -- --analyse "a.mp3" "b.flac"`
- Keep a test session's settings and files separate from your real ones: set the
  environment variable `VISUAL_ENGINE_DATA` to a folder before starting.

## Check the code yourself

You're running software from the internet, so here is how to see what it is and what
it does.

- **It's all here.** Everything the app does is in this repository, and you build it
  yourself from that code. Nothing is downloaded at run time, and there are no
  pre-built binaries to trust.
- **No internet, no tracking.** The app makes no network connections: no telemetry,
  no accounts, no update checks. You can confirm this. Search the code for network
  libraries (`reqwest`, `ureq`, `hyper`, `TcpStream`): there are none. The full list
  of libraries it uses is in `Cargo.lock`.
- **What it touches on your computer:**
  - songs, pictures and videos you open (read only);
  - the microphone, line-in, camera or computer audio, only when you pick one;
  - its own settings and autosave folder;
  - `.analysis.csv` files, only if you run `--analyse`.

  It runs `ffmpeg` as a separate program to read video files.
- **Run the tests.** About 160 automated checks cover the audio maths, the graph, the
  blend modes (compared pixel by pixel on your graphics card) and more:

  ```
  cargo test --workspace
  ```

- **Check the libraries for known security problems** (optional):

  ```
  cargo install cargo-audit
  cargo audit
  ```

- **Match a version.** Each change is a git commit. `git log` lists them, and
  `git checkout <commit>` builds exactly that version.

## Finding your way around

```
┌──────────────┬──────────────────────────────────┬──────────────┐
│ MEDIA        │             VIEWER               │  INSPECTOR   │
│ pictures,    │  what the audience sees, or the  │  settings of │
│ videos,      │  selected node, wipe, PiP        │  the node    │
│ cameras;     │                                  │  you clicked │
│ song & input │                                  │              │
├──────────────┴──────────────────────────────────┴──────────────┤
│ NODE GRAPH: right-click to add nodes, drag from a plug to wire │
├────────────────────────────────────────────────────────────────┤
│ SIGNAL STRIP: bass, mids, highs · kick / snare / hat · BPM     │
└────────────────────────────────────────────────────────────────┘
```

- The buttons at the top left (**Media, Inspector, Graph, Signals**) show and hide
  each panel. If one is missing, click its button.
- **Sound source:** in the Media panel on the left, scroll down past the pictures to
  **Open song…**, the live inputs, and **Computer audio**.
- **Settings for a node:** click the node in the graph. Its settings appear in the
  **Inspector** on the right, with a short "How to use" note and buttons to add an
  effect, a layer or a mask. With nothing selected, the Inspector shows getting-started
  steps.
- **Right-click a node** for everything you can do with it.
- Press **?** for every keyboard shortcut. **Tab** hides or shows the graph, and
  **F11** goes fullscreen.

A guided tutorial inside the app is planned (see the roadmap).

## Roadmap

Done:
- the analysis engine;
- the node graph and workspace;
- media (pictures, video, cameras);
- blend modes, the Layer Mixer, masks and keys.

Next, roughly in order:

1. **Compositing, last part.** Transform and Corner Pin handles you drag in the
   viewer, and a demo project.
2. **UI pass.** Dockable panels you can drag, float and tab (like Adobe apps), saved
   layouts, user themes, a clearer sources panel, and bolder Resolve-style looks.
3. **Looks and the full node library.** Signature looks you can mutate and breed,
   ready-made node layouts in the right-click menu, more original effects (datamosh,
   liquify, a symmetry engine), text, depth, and a free pack of starter pictures.
4. **Perform mode.** Macros, MIDI learn and controller support, an automatic
   "Director", a photosensitivity safety guard, and a panic key.
5. **Outputs.** Second screen and projectors (with warping), Spout / Syphon / NDI for
   OBS, and a live desktop wallpaper mode.
6. **Video intelligence.** Motion tracking, and keying people out without a green screen.
7. **Export.** 4K, vertical and square video and loops, with the song or the live
   audio, plus Ableton Link.
8. **Tutorial** inside the app.

Ideas for later: stems (separate drums / bass / vocals as their own signals) and
sampling, lyric tracks, DMX lighting, OSC, a phone remote, and sharing Looks.

The detailed plan is in [`DIRECTION_UPDATE_v2.md`](DIRECTION_UPDATE_v2.md), and what
changed when is in [`CHANGELOG.md`](CHANGELOG.md).

## For developers

Rust (2024 edition), [wgpu](https://wgpu.rs) with WGSL shaders, egui / eframe for
the UI, cpal for audio, symphonia for decoding, realfft, nokhwa for cameras, ffmpeg
for video.

```
crates/
  app/        the program: window, workspace, node editor, inspector
  audio/      song decoding, playback, live and computer-audio input, click track
  analysis/   pure DSP: spectrum, bands, onsets, tempo, beat phase, structure
  graph/      node graph data model: typed plugs, wires, groups, undo, save / load
  nodes/      node types and the graph evaluator (runs the graph on the GPU)
  media/      pictures, video (ffmpeg), cameras, video signals
  signals/    the signal bus and the modulation rule
  render/     wgpu renderer, texture pool, feedback buffers
  scenes/     shader scenes, the WGSL prelude, blend maths, hot reload
```

How audio flows:

```
song file ──decode+resample (loader thread)──► Song (stereo @ device rate + mono @ 48 kHz)
                                                 │
               output callback: plays stereo, pushes matching 48 kHz mono
                                                 ▼
live input ──input callback: mono mix → 48 kHz──► rtrb ring buffer ──► analysis thread
                                                                        │ triple buffer
                                                                        ▼
                                                                    UI / renderer
```

Rules the code keeps:
- Audio callbacks never allocate, lock, log or touch files.
- Analysis is pure and deterministic, so it runs the same live and offline.
- The output never waits for the UI, file loading or a bad shader.

Writing your own scenes: [`docs/SCENES.md`](docs/SCENES.md). The brief and working
rules: [`CLAUDE.md`](CLAUDE.md). Manual test checklists: [`TESTING.md`](TESTING.md).

Before sending changes:

```
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your
option. This is the usual choice for Rust projects.

## Credits

Designed and play-tested by HAVEKAUTION. The code is written with
[Claude Code](https://claude.com/claude-code).
