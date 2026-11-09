# Visual Engine (working title)

A native desktop app (Windows + macOS) that listens to music in real time and drives
live, performable visuals. See `CLAUDE.md` for the full brief and milestone plan.

## Status

- **M0 – app shell**: done. Window, song playback / live input, live waveform.
- **M1 part 1 – analysis core**: done. Spectrum, 32 mel + 6 named bands, auto-gain,
  fast/slow smoothing, centroid, loudness, with a live meter panel.
- **M1 part 2 – rhythm and structure**: done. Kick/snare/hat onsets, BPM, beat/bar/
  phrase clock, build/drop/section change, tap tempo, offline CSV export.
- **GATE** (meters match kicks, snares and tempo on your own songs): passed.
- **M2 – renderer and first scenes**: done. WGSL scenes with feedback, hot reload, three
  built-in scenes, scene picker, fullscreen, FPS counter. Writing scenes: `docs/SCENES.md`.
- **Genre scene pack**: nine scenes in total, each tagged with the music it suits.
- **M3 – modulation, presets, hotkeys**: done. Link any slider to the music, colour
  palettes, presets on keys 1–9 with crossfades, blackout / flash / intensity keys.
- **Computer audio**: visuals can follow whatever the computer plays (loopback).
- **Direction v2** (2026-11-10): one node graph at the core, Looks instead of presets.
  See `DIRECTION_UPDATE_v2.md`; the parts above that v2 replaces are listed in `CLAUDE.md`.
- **v2 M2 – graph engine**: in progress. Part 1: the graph data model
  (`crates/graph`). Part 2: GPU evaluation (`crates/nodes`) with a temporary test graph
  in the app.
- **v2 M3 – workspace + node editor**: in progress. Part 1: the node editor
  (right-click add, wiring, Inspector with modulation, copy/paste, undo, autosave).
  Part 2: the one-screen workspace (toolbar, signal strip, theme), viewer
  Program/Selected/Wipe/PiP, live thumbnails, ten signal/trigger nodes, frames, snap,
  colour tags, S/P/L insert. Next: groups with macro knobs, GPU cost per node.

## Running it

You need [Rust](https://rustup.rs) (stable). Then, from this folder:

```
cargo run --release
```

To open a song straight away: `cargo run --release -- "C:/Music/song.mp3"` (any path works).

To analyse songs without opening the window (writes `<song>.analysis.csv` next to each):
`cargo run --release -- --analyse "C:/Music/a.mp3" "C:/Music/b.flac"`.

The first build downloads and compiles dependencies and takes a few minutes; later
builds are quick.

## Layout

```
crates/
  app/        # the program: window, UI panels, wiring
  audio/      # song decoding, playback, live input, resampling, analysis feed
  analysis/   # (M1) spectrum, bands, onsets, tempo, beat phase, structure
  graph/      # (v2 M2) node graph data model: typed plugs, wires, groups, undo, save/load
  nodes/      # (v2 M2) node types + graph evaluator (runs the graph on the GPU)
  signals/    # (M1/M3) Signal Bus and modulation rule
  render/     # (M2) wgpu renderer, layers, feedback buffers, outputs
  scenes/     # (M2) built-in scene shaders
```

## How audio flows

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

Audio callbacks never allocate, lock, log or touch files. A new ring buffer is made
each time the source changes and its reading end is handed to the analysis thread.

## Developer checks

```
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```
