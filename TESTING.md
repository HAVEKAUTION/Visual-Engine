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
      screen's refresh rate). **I** hides/shows the counter (was F before M3).
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

## M3 – Modulation, presets, palettes, performance keys

Start the app and play a song with a clear beat. Everything below is in the **SCENE**
panel on the right, or on the keyboard (click the picture first so keys go to the app).

**Modulation ("~" buttons)**
- [ ] Every slider has a **~** button. Click it, then **＋ Add modulation** → *Kick hit*.
      The slider gets an orange marker that jumps on every kick and the button shows **~1**.
- [ ] Change **amount**: bigger = bigger jumps; negative = moves the other way.
- [ ] Add *LFO synced to the beat*: the value swings smoothly, in time with the music.
- [ ] Add *Random wander*: slow wandering, never jumps.
- [ ] Untick the small box at the start of a modulation row: it switches off without
      being deleted. **✕** removes it.
- [ ] Even at full amount, the value never leaves the slider's range.

**Colours**
- [ ] Open **COLOURS**: pick *Fire*, *Ice*, *Acid*… the whole scene recolours instantly.
- [ ] *Custom*: change one of the five colour boxes and watch it change live.
- [ ] *Scene colours* puts the original back.

**Presets**
- [ ] Open **PRESETS**: each scene lists 2–3 examples. Clicking one loads it.
- [ ] Change some sliders, type a name, **Save**. It appears in the list (and as a
      `.ron` file in `crates/scenes/presets/<scene>/`). Restart the app: still there.
- [ ] The small number box next to a preset puts it on a key (1–9).
- [ ] The bin icon deletes a preset.

**Performance keys**
- [ ] **1–9**: smooth crossfade to that preset (default 4 beats, set under PERFORMANCE).
      Default keys: 1 Concrete, 2 Acid Corridor, 3 Deep Jungle, 4 Clean Ring,
      5 Night Drive, 6 Rainy Night, 7 Northern Sky, 8 Forge, 9 Saturday Night.
- [ ] **Space**: blackout; Space again brings it back.
- [ ] **F**: one white flash, exactly on the next beat.
- [ ] **↑ / ↓**: modulations get stronger / weaker (shown at the bottom of the picture).
- [ ] **← / →**: everything moves slower / faster. **0** resets both to 100%.
- [ ] **H**: hotkey cheat sheet; **H** again hides it.
- [ ] While typing a preset name, the keys type letters instead of triggering.
- [ ] **MACROS**: eight knobs. Link a slider to *Macro 1* with **~**, then turn the
      macro: the slider follows (MIDI will drive these in M4).

**Known limits**
- Colour pickers inside a scene can't be modulated yet, only number sliders.
- Crossfades restart the trails of the incoming scene.

## Rebuilt scenes – Iron Furnace and Velvet Mirrorball

**Iron Furnace** (metal; key 8). Best with a metal / hardcore track.
- [ ] A glowing molten core with 4 rings of saw-toothed iron around it; neighbouring
      rings turn in opposite directions.
- [ ] Heavier guitars = bigger, hotter core and faster blades. Quiet intro = dimmer.
- [ ] Every kick: the view punches in and a burst of sparks shoots outwards. Double-kick
      passages = constant spark storm.
- [ ] Every snare: white lightning cracks across the rings.
- [ ] Drop / breakdown: a white-hot blast.
- [ ] Try *Blade rings*, *Teeth*, *Tooth depth* and the presets *Blast Furnace* and
      *Cold Steel* (blue fire).

**Velvet Mirrorball** (house / disco; key 9). Best with a four-to-the-floor track.
- [ ] Velvet curtains at the back, a tiled dance floor at the front, a mirrorball at
      the top with a wire.
- [ ] Every kick: the floor lights a new pattern of coloured tiles and the light spots
      flash.
- [ ] The light spots circle around the ball; more bass = faster.
- [ ] Coloured beams swing through the haze from the ball.
- [ ] Hi-hats: tiles on the ball sparkle white.
- [ ] Every 4 bars the light colours rotate (*Colour rotation* at 0 stops that).
- [ ] Try the presets *Golden Era* and *Afterhours*.

## Computer audio (system loopback)

- [ ] Play music in any app (Spotify, YouTube in the browser…). In Visual Engine click
      **🔊 Computer audio** under LIVE INPUT. Within a second the scene reacts to it and
      the meters (M) move. The device box shows "Computer audio: <your speakers>".
- [ ] Pause the music: the visuals calm down. Play again: they come back.
- [ ] Changing the volume barely matters (auto-gain), but muting the speakers may stop
      the capture on some systems.
- [ ] **⏹ Stop** stops listening.
- [ ] Headphones / other outputs: pick "Computer audio: <that device>" in the list and
      press **🎤 Start input**.
- macOS only: the first time, macOS asks to allow **System Audio Recording** for the
  app (or for Terminal, if started from Terminal). Click Allow. If you said no: System
  Settings → Privacy & Security → Screen & System Audio Recording → turn it on, then
  restart the app. Needs macOS 14.2 or newer.

## v2 M5 part 1 – Blend modes, masks, Layer Mixer

Run: `cargo run --release -p app` (close any running copy first)

**Layer Mixer**
- [ ] Select a picture node (e.g. a scene) and press **L**, pick a picture or video:
      a **Layers** node appears with the old one as Layer 1 and the new one on top.
      Select the new node, press **L** again: a third layer joins the same mixer.
- [ ] The Layer Mixer node only shows the layers in use plus one free plug.
- [ ] Inspector: layers listed top first. Change a layer's blend to **Hard Light**,
      **Soft Light**, **Multiply**, **Colour**, **Luminosity**: each looks clearly
      different. Opacity fades it.
- [ ] **S** (solo) shows only soloed layers; **M** (mute) hides one; the name gets a
      line through it.
- [ ] **▲ / ▼** move a layer: the picture order changes. Ctrl+Z puts it back.
- [ ] Wire an LFO into a layer's opacity, then move that layer: the wire moves too.

**Masks on any node**
- [ ] Drop a black-and-white picture (white shape on black) into the graph and wire
      it into any node's green **Mask** plug: the node only shows inside the white.
      On an effect (e.g. Glitch) the effect only happens there; outside you see its
      input untouched.
- [ ] On a source (scene, picture, video) the outside becomes see-through, so in a
      Layer Mixer the layers below show through.

**Merge**
- [ ] An old saved Merge still has its blend mode; the list now has 21 modes.
      "Blend in linear light" gives softer, more physical mixes (try Add, Screen).

Known gaps (M5 part 2): keys (Luma, Chroma), Shape / Gradient / Motion masks and
the Key Mixer; on-viewer handles for Transform and Corner Pin; the compositing demo.

## v2 M4 part 2 – Cameras, video signals, sync

Run: `cargo run --release -p app` (close any running copy first)

**Camera**
- [ ] Left panel → PICTURES & VIDEOS → **Cameras** lists your webcam. Drag it into
      the graph (or double-click it). A Camera node appears, already showing you.
- [ ] Wire it into Program Out. Mirror is on, so raising your right hand raises the
      hand on the right of the picture.
- [ ] Inspector → **Size** and the fps box: switch to 640 × 480. The picture
      changes size within a second or two. "Automatic" goes back.
- [ ] The grey line under it shows the camera, the real fps and "delay ≈ … ms".
      In a dark room the fps drops (the camera exposes longer); add light and it rises.
- [ ] Unplug the camera: after a moment the node shows ⚠ and goes black; the app
      keeps running. Plug it back in, press ⟳ next to the camera name: it comes back.

**Video signals: wave at your webcam**
- [ ] Select the Camera node: under VIDEO SIGNALS, **Motion** jumps when you wave
      and falls back when you keep still. "Where across / up" follow your hand.
      Hold something colourful up: the Colour swatch turns that colour.
- [ ] Add an **Audio Signal** node, pick **motion**, wire it into Transform's Zoom
      (or any slider): waving now drives it.
- [ ] A video file node shows the same meters (cuts and fast action = motion).
- [ ] With two media nodes, only the first "feeds the video signals"; the other
      says so. Switch the first one's "Feeds the video signals" off: the second takes over.

**Stutter**
- [ ] Play a song. Audio Trigger (Snare) → Video **Stutter**: on each snare the
      clip repeats a quarter-beat slice 4 times, then carries on. Try a slice of 1
      beat and 2 repeats.

**Sound & picture sync**
- [ ] Media tab → **🔊 Sound & picture sync…**. Pause the music, press **Start the
      sync test**.
- [ ] Step 1: a dot flashes in the middle of the viewer; press Space with it 8
      times. Step 2: look away; press Space on each click 8 times.
- [ ] It tells you the result and sets the offset. In the check, dot and click
      should feel like one; nudge the slider if not. With Bluetooth headphones the
      offset should come out clearly positive.
- [ ] Set −100 ms with a song playing: the visuals hit slightly before the sound.
      Set +100 ms: slightly after. The offset is remembered after a restart.

Known gaps: the camera delay is an estimate (one frame + decoding + waiting); the
sync test measures your whole chain. macOS camera support is written but untested
(Windows tested with a Logitech C270). Camera sound is ignored.

## v2 M4 part 1 – Pictures and videos

Needs ffmpeg for video (you have it). On another PC: Windows `winget install Gyan.FFmpeg`,
macOS `brew install ffmpeg`, then restart the app. Pictures work without it.

Run: `cargo run --release -p app`

**Bringing media in**
- [ ] Left panel → **PICTURES & VIDEOS** → **Import…**, pick a picture and a video.
      They appear with small thumbnails.
- [ ] Drag a file from Explorer straight onto the graph: it joins the pool *and* a node
      appears where you let go. Drop a song file anywhere: it opens.
- [ ] Drag a pool item into the graph (or double-click it): an Image / Video node,
      named after the file, already showing its picture.
- [ ] Wire it into Program Out (or into Transform → Program Out). It shows in the viewer.

**Make it move with the music**
- [ ] Video → Inspector → Playback **Beat-locked**, Length 1 bar: the clip restarts in
      time with every bar (play a song first).
- [ ] Playback **Scrub**, then drag a blue wire from an **Audio Signal** (Bass) onto
      **Position**: the bass moves the playhead.
- [ ] **Audio Trigger** (Kick) → the Video's **Jump** plug: it jumps to a new spot on
      every kick. **Restart** instead: back to the start on every kick.
- [ ] **Hold** node (Start = Drop, Stop = Hat) → the Video's **Hold (freeze)**: the
      video freezes on the drop until the next hat.
- [ ] Wire Speed: Audio Signal (Loudness) → Speed. Louder = faster.
- [ ] Image → Transform (Kick punch up) → Program Out: the picture punches on kicks.

**Safety**
- [ ] Rename or move a used file, restart the app: the node shows ⚠ and black, the
      Inspector says what's missing. **Look in a folder…** finds it again.
- [ ] A broken / non-video file shows an error on the node; the app keeps running.

Known gaps (M4 part 2): cameras, video signals (motion, brightness, hue for the bus),
latency calibration. Video sound is ignored (the music comes from the song / input).
Masks and blend modes come with M5.

## Editor polish – node size, per-node pictures, Hold

- [ ] Hold **Ctrl** and scroll over the graph: nodes and their text grow / shrink (a
      message shows the size). Plain scroll still zooms. Restart: the size is kept.
- [ ] Right-click a picture node → **Hide live picture**: just that node's preview goes.
      Right-click again → **Show live picture**.
- [ ] *On the drop, bend until the next hat:* add **Audio Trigger** (Drop) → **Hold**
      Start, another **Audio Trigger** (Hat) → **Hold** Stop, then drop Hold's blue wire
      on a Transform → Zoom (or any slider). It jumps on the drop and lets go on the
      next hat. Set **Length** to 4 (beats) and unplug Stop: it holds for one bar.
      Try Fade out 1 s for a softer release.

## v2 M3 part 3 – Groups, macro knobs, GPU cost

Run: `cargo run --release -p app`. Tip: right-click empty graph space → **Add starter
graph** if your restored graph is small.

**Groups**
- [ ] Select Warehouse Tunnel and Transform (Shift+click), press **Ctrl+G**. A lilac
      backdrop titled "Group 1" wraps them. Drag the title: both move.
- [ ] Double-click the title, type a name, Enter: renamed.
- [ ] Click **▾** at the left of the title: the two nodes fold into one box. Its left and
      right plugs are the wires going in and out; the wires still work and the picture
      doesn't change. Double-click the box: it opens again.
- [ ] Ctrl+Z undoes fold, rename and grouping, step by step.
- [ ] Group the group together with Aurora (select the box + Aurora, Ctrl+G): a group
      inside a group. Ctrl+Shift+G on the outer one leaves the inner one intact.
- [ ] Ctrl+C / Ctrl+V a whole group: the copy is grouped too.

**Macro knobs**
- [ ] Click a node inside a group. In the Inspector, right-click a setting's *name*
      (e.g. Zoom) → **Publish to group**. The name gets a ◆.
- [ ] Click the group's title (or its box): the Inspector shows the group with its
      knobs. Rename a knob, move it with ↑ ↓, remove it with ✕.
- [ ] Fold the group: the knobs show as bars on the box. Drag a bar sideways: the
      setting changes (Shift = fine). The blue tick shows the live value when music
      modulates it.
- [ ] Drag a blue wire from Audio Signal onto the folded box: it asks which knob to drive.

**GPU cost**
- [ ] Every picture node shows "x.xx ms" on its thumbnail (green = cheap, yellow, red =
      heavy). Open groups show their total on the title; folded boxes in the header.
- [ ] Toolbar, top right: "GPU x.xx ms" is the total. **Cost** toggles the labels.
- [ ] Set a node to Half or Quarter size in the Inspector: its number drops.

Known gaps: masks (green plugs) still do nothing (M5). A folded box sits where its top-left
node was, so it can overlap nearby nodes; drag it clear.

## v2 M3 part 2 – Workspace, signal nodes, tidy graphs

Run: `cargo run --release -p app`, then open a song or press **🔊 Computer audio**.

Note: your graph from last time is restored, so it won't have the new starter graph.
To see it: right-click empty graph space → **Add starter graph** (or **Clear graph**
first; Ctrl+Z undoes that).

**Workspace**
- [ ] Top toolbar: Media / Inspector / Graph / Signals buttons hide and show the panels.
      Close and reopen the app: they stay as you left them (also panel sizes).
- [ ] The strip along the bottom shows six band bars, K / S / H lights flashing on
      hits, the BPM, four beat dots (one per beat of the bar) and bar/phrase progress.
- [ ] Left panel has **Media** and **Looks** tabs (Looks is a placeholder until M6).

**Viewer**
- [ ] The big viewer shows the final output. Click a node: a small picture of *that*
      node appears top-right (PiP). Click the PiP: the viewer shows that node. Press
      **V** to go back to the Program.
- [ ] Press **W** (or Viewer → Wipe): the left part shows the selected node, the right
      the Program. Drag the line. W again to leave.
- [ ] Every picture node shows a live thumbnail. Toolbar → Thumbnails turns them off.

**Depth: try these chains** (right-click → Signals / Triggers)
1. [ ] *Kick punch:* **Audio Trigger** (Kick) → **Envelope** → drop its blue wire on a
       Transform → Zoom. The picture punches on every kick and eases back.
2. [ ] *Change every bar:* **Audio Trigger** (Every bar) → **Counter** (Steps 4) →
       any slider. It steps through four values, one per bar.
3. [ ] *Tame a signal:* **Audio Signal** (Bass) → **Smooth** (fall 1 s) → **Remap**
       (from 0–3 to 0–1) → a slider. Smooth and big instead of jittery.
4. [ ] *Threshold:* **Audio Signal** (Loudness) → **Gate** (threshold 1.5) →
       **Envelope** → a slider. Only the loud moments fire.
5. [ ] *Random every 2 beats:* **Random** (Changes per beat 0.5, Smooth off) → a slider.
- [ ] Trigger nodes show an orange light and their wires flash when they fire.

**Tidy graphs**
- [ ] Drag nodes: they snap to the grid. **G** turns snapping off/on.
- [ ] Select a few nodes, right-click empty space → **Frame the selected nodes**. Drag
      the frame's title: everything inside moves. Drag the bottom-right corner to resize.
      Double-click the title to rename. Right-click the title for colour / remove.
- [ ] Right-click a node → pick a colour: its header takes that colour.

**Quicker building**
- [ ] Select Warehouse Tunnel, press **S**, pick Mirror: Mirror is inserted between the
      tunnel and the Transform. Ctrl+Z removes it in one go.
- [ ] Select a picture node, press **L**, pick a Source: a new layer and a Merge appear,
      already wired. **P** does the same as a 50/50 parallel mix.
- [ ] Drag a wire from a plug and let go on empty space: the search opens with only
      nodes that fit; pick one and it's already wired.
- [ ] Hover a node or a plug: a short explanation. **?**: all the shortcuts.

Known gaps (next part of M3): groups (with published macro knobs) and per-node GPU cost.
The green Mask plugs do nothing yet (masks come with compositing in M5).

## v2 M3 part 1 – The node editor

Run: `cargo run --release -p app`

The node graph fills the bottom of the window. Open a song (or press **🔊 Computer
audio**) so things move.

**Try this tour (5 minutes):**
1. [ ] Click **02 Transform**. It gets an orange outline, the viewer shows *its* picture
       ("Viewing 02 Transform" bottom-left) and the Inspector (right) shows its sliders.
       Click empty graph space: the viewer goes back to the full output.
2. [ ] In the Inspector, drag **Spin speed**: the tunnel turns faster. Press **Ctrl+Z**:
       it goes back.
3. [ ] Right-click empty graph space → **Effects → Mirror** (or Glow). A new node 08
       appears where you clicked.
4. [ ] Rewire: drag the wire end off Merge's **Foreground** plug and drop it on nothing
       (it unplugs). Now drag from Transform's output plug (right side) to the new
       node's **Picture** plug, then from the new node's output to Merge's
       **Foreground**. The effect is now in the chain.
5. [ ] Make the music drive it: right-click → **Signals → Audio Signal**. In the
       Inspector pick **Kick hit**. Drag from its blue plug onto the new effect node
       and choose a slider (or drop it straight onto a slider in the Inspector). The
       slider now jumps with each kick. Try the wire's **depth** number and the
       **~** button (modulation editor) under that slider.
6. [ ] Select a node and press **Ctrl+D**: bypassed (dimmed, "off"). Again: back on.
7. [ ] Drag a box around a few nodes, **Ctrl+C**, move the mouse, **Ctrl+V**: copies
       with new numbers, wires kept. **Del** removes them. **Ctrl+Z** brings them back.
8. [ ] Scroll to zoom, middle-drag (or hold Space and drag) to pan, **Home** to fit all.
9. [ ] **Shift+Space**: a search box. Type "glow", Enter: a Glow node appears.
10. [ ] **Tab** hides the graph (bigger viewer); Tab again shows it.
11. [ ] Close the app and start it again: "Welcome back…", your graph is as you left it.
12. [ ] Right-click empty space → **Clear graph**: empty canvas message. Ctrl+Z restores
        it. **Add starter graph** adds the example again.
13. [ ] Top of the right panel → **Classic scenes**: the old view with presets (keys 1–9)
        still works. **Node graph** goes back.

Known gaps (next parts of M3): no S/P/L quick-add keys, no groups yet, no split-wipe
compare, no per-node GPU cost, no new theme or signal strip yet. Images, video and
cameras as nodes come in M4.

## v2 M2 part 2 – First visible node graph (replaced by the editor above)

Run: `cargo run --release -p app`

- [ ] At the top of the right-hand scene panel there is a **🧪 Node graph test** button.
      Click it: the picture changes to the Warehouse Tunnel blended (Screen) over Aurora
      Drift, and the panel shows NODES (01–07) and WIRES.
- [ ] Play music. The tunnel picture pulses bigger on the bass (that's the Audio Signal
      node wired into Transform's zoom) and slowly turns.
- [ ] Every 8 beats the tunnel fades a bit and comes back (the LFO wired into Merge's
      opacity). The bars next to "05 Audio Signal" and "06 LFO" move.
- [ ] Untick **02 Transform**: the tunnel stops turning/pulsing (bypassed, picture passes
      straight through). Tick it again: back.
- [ ] Untick **04 Merge**: only the Aurora (background) shows. Untick **03 Aurora
      Drift** instead: only the tunnel shows.
- [ ] Untick **01 Warehouse Tunnel**: only the Aurora shows, no crash.
- [ ] Intensity / blackout / flash keys still work while the graph shows.
- [ ] Click the button again: the normal scene and presets come back as before.
- [ ] Optional: open `crates/scenes/shaders/fx_transform.wgsl`, break it (type `xx`
      somewhere) and save. The picture keeps going and 02 shows a ⚠ message. Undo your
      change and save: the warning goes away.
- [ ] `cargo test -p nodes` ends with **3 passed** (GPU tests) and **1 passed**.

Not yet: you can't add, move or wire nodes yourself. That is the node editor (next
milestone, M3). The test graph isn't saved.

## v2 M2 part 1 – Graph data model (no visible change)

This step only adds the graph's "brain" (what can connect to what, undo, saving).
There is nothing new to see in the app yet. To check it:

- [ ] Run `cargo test -p graph`. It should end with **13 passed; 0 failed**.
- [ ] Run the app as usual (`cargo run --release -p app`): everything works exactly as
      before (scenes, presets, keys, computer audio).

What the tests prove, in plain English:
- A wire only goes where its type fits (a Signal can't go into a picture input, a
  picture can't drive a slider); a picture *can* be used as a mask.
- Wires can't make loops.
- Plugging into an input that's already used replaces the old wire.
- Every edit can be undone and redone, and a whole slider drag undoes in one go.
- Deleting a node removes its wires and group spot; undo brings it all back.
- Groups can sit inside groups and publish controls from anywhere inside them.
- Saving and loading gives back exactly the same graph; broken files are refused.
