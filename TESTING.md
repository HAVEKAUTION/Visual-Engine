# Testing checklist

Run the app with:

```
cargo run --release
```

Tick each box when it behaves as described. If something doesn't, note what you saw.

## M0 – App shell

**Window**
- [ ] A dark window titled "Visual Engine" opens (about 1280×800).
- [ ] Left side panel with "SONG FILE" and "LIVE INPUT" sections; large dark area on the right.
- [ ] Bottom-left shows `GPU: NVIDIA GeForce RTX 3060 (...)` (or your graphics card).
- [ ] The side panel can be resized by dragging its right edge.
- [ ] With nothing loaded the preview says "No audio source…" and shows a flat line.

**Song file**
- [ ] "Open song…" opens a file picker that shows MP3/WAV/FLAC/OGG files.
- [ ] After choosing a song, a spinner shows briefly, then it starts playing by itself.
- [ ] The song name and format line appear (e.g. `44.1 kHz, 2 ch → Speakers (48.0 kHz)`).
- [ ] Pause stops the sound; Play resumes from the same spot.
- [ ] The time readout counts up (`m:ss / m:ss`) and the seek bar moves along.
- [ ] Dragging the seek bar and letting go jumps there; clicking on the bar jumps too.
- [ ] Volume slider changes loudness smoothly with no clicks; 0% is silent.
- [ ] Volume does **not** change the waveform size (the analysis sees the song itself).
- [ ] At the end of the song it stops; pressing Play starts it again from the beginning.
- [ ] Try one file of each type you have: MP3, WAV, FLAC, OGG.
- [ ] Choosing a non-audio file (rename a .txt to .mp3) shows a red error, no crash.

**Live input**
- [ ] The dropdown lists your mics/line-ins; the default one is marked "(default)".
- [ ] "Start input" shows `Listening: <device> (…kHz, … ch → 48 kHz mono)`.
- [ ] Talking/clapping into the mic makes the waveform move.
- [ ] "Stop" ends it; the waveform freezes and the status line disappears.
- [ ] Starting live input while a song plays stops the song (one source at a time).
- [ ] ⟳ rescans devices (plug a USB mic in, press ⟳, it appears).
- [ ] Windows: if the mic stays flat, check Settings → Privacy & security → Microphone →
      "Let desktop apps access your microphone" is On.
- [ ] macOS: on first use, allow microphone access when asked (or System Settings →
      Privacy & Security → Microphone → enable your terminal app).

**Waveform**
- [ ] With a song playing, the waveform scrolls right-to-left continuously and smoothly.
- [ ] Loud parts are taller; quiet intros are small; paused = flat line slides in.
- [ ] The label shows `peak` changing, and the footer's "s received" counts up ~1 per second.

**Robustness**
- [ ] Unplugging the output or input device shows a red error instead of crashing.
- [ ] Opening a second song while one plays switches cleanly.
- [ ] Leave a song playing for 10 minutes: no stutter, no growing lag.

## M1 part 1 – Analysis meters

Open a song you know well (something with a clear kick drum is best). The meter panel
sits under the waveform; untick "Show signal meters" in the side panel to hide it.

**How to read the meters:** 0 at the bottom, 3 at the top, and the thin white line is
**1.0 = "typical for this song"**. A white cap on top means the value went above 3.
Each named band shows two bars: bright = **fast** (punchy), dim = **slow** (the trend).

**Basics**
- [ ] The status line reads about **93.8 hops/s** while audio is flowing.
- [ ] `t` counts up in seconds and restarts at 0 when you open a new song or start input.
- [ ] With nothing playing (or paused), every bar sinks to zero within ~2 seconds.

**Does it match the music?**
- [ ] **Kick drum** → `sub` and `bass` fast bars jump on every kick; the left (red/orange) mel bars pulse.
- [ ] **Snare / claps** → `low_mid` and `mid` jump.
- [ ] **Hi-hats / cymbals** → `air` (and the right-hand violet mel bars) flicker quickly.
- [ ] Fast bars snap up instantly and fall back within a fraction of a second.
- [ ] Slow bars move gently and lag a second or so behind.
- [ ] **Breakdown with no kick** → bass slow bar sinks over a couple of seconds; when the
      kick returns, bass fast jumps well above the line.

**Auto-gain (1.0 = typical)**
- [ ] After ~5–10 s of steady music, the bars hover around the white line rather than
      sitting pinned at the top or bottom. Spiky parts like kicks usually sit at 1–3; that's normal.
- [ ] A quiet song and a loud song give similar-looking meters after a few seconds, while
      the **dBFS** number differs a lot.
- [ ] The **Volume** slider changes what you hear but not the meters.

**Centroid / loudness**
- [ ] **dBFS**: around −20 to −8 for a typical mastered track, lower for quiet ones.
- [ ] **Centroid Hz** drops in bass-heavy, muffled parts and rises in bright, hat-heavy
      parts; the marker on the colour strip and the swatch colour move with it.

**Live input**
- [ ] Start your mic: talking moves `mid`/`high_mid`; claps and hisses move `air`; silence → zero.
