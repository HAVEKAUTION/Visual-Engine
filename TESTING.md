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
