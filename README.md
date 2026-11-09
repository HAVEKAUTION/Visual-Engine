# Visual Engine (working title)

A native desktop app (Windows + macOS) that listens to music in real time and drives
live, performable visuals. See `CLAUDE.md` for the full brief and milestone plan.

## Status

- **M0 – app shell**: done. Window, song playback / live input, live waveform.
- **M1 part 1 – analysis core**: done. Spectrum, 32 mel + 6 named bands, auto-gain,
  fast/slow smoothing, centroid, loudness, with a live meter panel.
- **M1 part 2** (onsets, tempo, beat phase, structure): next.

## Running it

You need [Rust](https://rustup.rs) (stable). Then, from this folder:

```
cargo run --release
```

To open a song straight away: `cargo run --release -- "C:/Music/song.mp3"` (any path works).

The first build downloads and compiles dependencies and takes a few minutes; later
builds are quick.

## Layout

```
crates/
  app/        # the program: window, UI panels, wiring
  audio/      # song decoding, playback, live input, resampling, analysis feed
  analysis/   # (M1) spectrum, bands, onsets, tempo, beat phase, structure
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
