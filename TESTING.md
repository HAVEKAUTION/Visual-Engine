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

## M1 part 2 – Rhythm and structure

Use a song with a steady kick (house, techno, pop). The rhythm row is at the top of the
meter panel; events are listed under **EVENTS** in the side panel.

**Onset lights**
- [ ] **KICK** flashes on every kick drum, **SNARE** on snares/claps, **HAT** on hi-hats,
      **ONSET** on any hit. The numbers under them count up.
- [ ] The lights stay dark during silence or when paused.

**Tempo**
- [ ] Within ~5 s of the music starting, the BPM turns white and settles on the song's
      tempo (check against the BPM your DJ software or a website reports, ±1).
- [ ] It doesn't jump to double or half (e.g. 64 or 256 for a 128 song).
- [ ] Pause: the BPM stays where it was. Play again: it carries on.
- [ ] The "confidence" number is high (0.6+) for steady dance music, lower for ambient music.

**Beat clock**
- [ ] The circle sweeps round once per beat and the white dot passes the top as each kick
      hits. The circle lights up on every beat.
- [ ] The four boxes step 1-2-3-4. Bar counter ("bar 3/8") and phrase bar move along.
- [ ] **R**: press it on the "1" of a bar (the big downbeat). From then on beat 1 of the
      boxes lines up with the start of each bar.
- [ ] **T**: tap T along with the beat 4+ times. The BPM follows your tapping and shows
      "TAPPED (manual)". "Auto tempo" (side panel) goes back to automatic.

**Structure** (EDM / dance tracks with a build-up and drop work best)
- [ ] During a build-up (riser, rolls getting faster, energy rising) the **build** bar fills
      and says BUILDING; a "BUILD" line appears in EVENTS.
- [ ] When the drop hits, a "DROP" line appears with the song time of the drop, and the
      bar counter restarts at "bar 1/8".
- [ ] A big change in sound (breakdown, new section) adds "SECTION CHANGE" lines. A steady
      groove should not keep producing them.

**Analyse file offline**
- [ ] "📊 Analyse file offline…" → pick a song. A spinner shows, then "Saved <song>.analysis.csv"
      and a summary (BPM, kick/snare/hat counts, builds, drops).
- [ ] A 4-minute song takes a few seconds at most.
- [ ] "Show in folder" opens the folder with the CSV selected.
- [ ] Open the CSV in Excel / Numbers / Google Sheets. One row per ~10.7 ms. `kick`, `snare`,
      `hat` columns hold the time of each hit; `events` holds e.g. `drop@93.751`. Jump to that
      time in the song and check it's right.
- [ ] Command line: `cargo run --release -- --analyse "path/to/song.mp3"` does the same.

**Known quirks**
- Very fast drum & bass (170+ BPM) reads as half time (e.g. 87). Tap T to fix it.
- Snares with a lot of low body can also flash KICK (the kick band is 20–250 Hz).
- Bar 1 is a guess until you press R or a drop is detected.

## M2 – Renderer and first scenes

Start the app with a song playing (house/techno with a clear kick is best). The picture
is in the middle; the **SCENE** panel is on the right.

**Basics**
- [ ] The picture moves smoothly. The top-left corner shows about **60 fps** (or your
      screen's refresh rate). **F** hides/shows the counter.
- [ ] The scene list (top of the right panel) has **Kaleido Bloom**, **Radial Spectrum**
      and **CRT Glitch**. Switching is instant.
- [ ] Moving a slider changes the picture right away. **Reset** puts them back. Switch to
      another scene and back: your slider settings are remembered.
- [ ] **F11** (or double-click the picture) = fullscreen picture only. **F11** or **Esc**
      comes back. **M** shows/hides the signal meters.

**Kaleido Bloom** (psychedelic)
- [ ] A mirrored, flowing fractal with trails. Kicks give a zoom "punch".
- [ ] It flows faster when the music is louder and spins faster with more bass, but
      never jumps. Paused or quiet: it keeps drifting slowly.
- [ ] Colour shifts with the sound: bassy parts warmer (red/orange), bright parts cooler.
- [ ] "Trails" at 0 = no trails; near max = long smeary trails.

**Radial Spectrum** (minimal)
- [ ] Bars around a ring: bass at the top, highs at the bottom, mirrored left/right.
- [ ] The ring and the centre dot pulse on every kick (dot turns the kick colour), leaving
      fading echo rings.
- [ ] Silence: bars shrink to nothing, only the thin ring stays.

**CRT Glitch** (retro/glitch)
- [ ] Synthwave sun, a grid floor moving towards you (faster with more bass) and a
      skyline made of the spectrum.
- [ ] Every snare/clap: the picture tears into shifted blocks with colour fringes.
- [ ] At a drop: a short negative flash and a big glitch.
- [ ] Scanline / Screen curve / Noise sliders change the CRT look.

**Hot reload** (try it: no programming needed, just change a number)
- [ ] Click **Open scenes folder**, open `radial_spectrum.wgsl` in Notepad (Windows) or
      TextEdit (Mac; Format → Make Plain Text first). Change `"default": 72` (Bars) to
      `"default": 20`, save. Within a second the scene reloads ("Reloaded ..." in green);
      press **Reset** to see 20 bars.
- [ ] Break it on purpose: type `xyz` on a line inside `fs_main` and save. A red error
      appears with the line number; the scene keeps running. Remove `xyz`, save: the error
      goes away.
- [ ] Undo your changes afterwards (or keep them; they are your scenes now).

**Known limits**
- Resizing the window or panels restarts the trails (feedback clears).
- The picture renders at the preview size; fullscreen on a 4K screen renders 4K.
- There is no separate output window yet (2nd screen comes in M5); fullscreen is the
  main window.

## GATE – Analysis on your own songs

Goal: check that the meters agree with what you hear before we build more on top.
Turn the meters on (**M**). Pick the **Radial Spectrum** scene: it shows the analysis most
directly (the dot = kicks).

**Songs to try** (one or two of each; 1–2 minutes per song is enough)
1. **Four-on-the-floor house / techno** (120–130 BPM): the easy case. Everything should
   be right here.
2. **Hip-hop / boom-bap** (85–95 BPM, swung drums).
3. **Drum & bass / jungle** (170–175 BPM, busy breakbeats).
4. **Slow acoustic or ballad** (soft, no drums or brushes only).
5. **A track with a big build-up and drop** (EDM, dubstep, big-room).
6. **Rock / metal** (dense guitars, fast double-kick).
7. Optional: **live input**, music played out loud into your microphone.

**What each meter should do**
| Meter | Should | Problem if |
|---|---|---|
| KICK light | flash on every kick drum | misses kicks, or flashes on bass notes / snares |
| SNARE light | flash on snares and claps | silent on obvious snares |
| HAT light | flicker with hi-hats / shakers | flickers constantly on noise, or nothing |
| BPM | settle within ~5 s on the song's tempo (±1) | wrong, or double / half (e.g. 64 for 128) |
| Beat circle | dot passes the top on each kick | drifts off the beat over time |
| Bands (sub … air) | bass bars jump on kicks/bass, air on cymbals | a band is stuck at 0 or always maxed |
| BUILD / DROP | BUILD during a build-up, DROP when it hits | missing, or firing on every chorus |
| Acoustic song | lights quiet, BPM may stay "searching…" | that is OK, it's not a failure |

**Known and expected:** drum & bass may show half tempo (e.g. 87 for 174). Press **T** in
time to fix. Bar 1 is a guess until you press **R** on a downbeat.

**How to report a problem to me** (copy this, one block per song):
```
Song: Artist – Title (genre, real BPM if you know it)
Source: file / live mic
What was wrong: e.g. "KICK missed about half the kicks in the chorus"
When: e.g. "from 1:20 to 1:45"
Meter values: e.g. "BPM showed 64, should be 128"
```
Best of all: click **Analyse file offline…** for that song and tell me where the
`.analysis.csv` file is saved. It lets me see every value the analysis produced.
