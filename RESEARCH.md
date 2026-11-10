# Research: what comparable tools do, and what we take from them

Written 2026-11-10. Sources are listed at the end. This is research only. What we
actually build is decided in `DIRECTION_UPDATE_v2.md` (see "v2.1 additions").

## The field, in one line each

| Tool | What it is best at | What we take | What we avoid |
|---|---|---|---|
| **Resolume Arena** | The industry standard for clip-based live VJing: layers, decks, effects, audio reactivity, output mapping, licensed **Pioneer PRO DJ LINK** and **Denon StageLinQ** sync | DJ-gear sync, cue-safe live behaviour, feature voting with users | Clip grids as the core idea (our core is the graph) |
| **TouchDesigner** | Unlimited node-based generative and installation work | Depth of the graph, operator families, "everything is wireable" | Its learning curve: users report needing *courses* to start |
| **VDMX** (Mac) | Modular VJ rig: deterministic cues, step sequencer, audio analysis plug-ins | Step sequencer and data-source plug-ins (→ our Signal/Trigger nodes) | Mac-only, dated UI |
| **Synesthesia** | Audio-reactive shader "scenes", ISF/Shadertoy import, MIDI/OSC, Spout/Syphon/NDI | **ISF shader import** (a huge free shader library), dozens of named audio uniforms | Scenes as closed boxes |
| **Magic Music Visuals** | Beginner-friendly patching for music visuals | Proof that a *gentle* graph sells to musicians | Basic look and feel |
| **MadMapper** | Projection mapping: mesh warp with Bézier points, Bézier masks, soft-edge blending, LED mapping | M8 outputs: warp, masks, soft edge, multiple projectors | Being a mapper first (we are a visual instrument first) |
| **After Effects** (+ Trapcode Form/Particular, Plexus, Boris FX) | Offline motion graphics; audio-to-keyframes, particles, displacement, glows, flares | The *effects vocabulary* people know (displace, particles, glow, echo, chromatic split), done in real time | Offline rendering, keyframe-only thinking |
| **Wallpaper Engine** | Desktop wallpapers reacting to system audio (16/32/64 bands) | "Desktop mode": our output as a live wallpaper | Its per-frame unsmoothed audio values (we already do better) |
| **OBS** | Scenes, sources, filters, docks you can rearrange, stream output | Dockable panels, Spout/NDI out as a "source" for OBS | Being a streaming app |
| **DaVinci Resolve / Fusion** | Clean, calm node UI, numbered nodes, S/P/L quick insert, Inspector | Already our model | Pages (we have one workspace) |
| **FL Studio** | Playful pattern/step workflow, plug-in windows, mixer routing, ZGameEditor visualizer | Step sequencing, "pattern" feel, drag-anything-anywhere | Window clutter |

## What users ask for (forum wishes, recurring)

From the Resolume feature-wish threads, VirtualDJ / DJ.Studio feedback boards and
VJ community posts:

- **Easy links between apps:** Spout/Syphon/NDI in *and* out, OSC both ways. Users
  call MIDI and Syphon setup "clunky".
- **Tap tempo driving effect parameters** directly.
- **Choose where recordings go** (not the program folder).
- **Sync with DJ gear** (CDJ/Prime track position and tempo), not just audio guessing.
- **More layers and many effects per layer** (our graph covers this).
- **Careful about dangerous controls on MIDI/keys** (Resolume's own pushback: things
  like "clear all" must not be one stray button away). Our safety rule.
- **Drag & drop anything anywhere** in the UI.

Beginner reality: Magic Music Visuals is called "the best entry-level software for
reactive music" and TouchDesigner needs courses (12 × 3 h lessons in one account).
**Our gap to fill:** TouchDesigner depth with Magic's first ten minutes.

## Findings that change our plan

1. **DJ gear sync is a real pro feature.** Pioneer licenses PRO DJ LINK data to
   visual software (Resolume uses the free *PRO DJ LINK Bridge*); Denon StageLinQ is
   built into Resolume. Both need a licence or partner programme. Plan: Ableton Link
   + MIDI clock first (open), then apply for Pioneer/Denon programmes before release.
2. **ISF shader import** gives users thousands of existing shaders on day one, and
   we already parse WGSL headers. ISF is GLSL + a JSON header; naga can translate
   GLSL. This fits M6.
3. **Dockable panels:** `egui_tiles` 0.18 (by egui's author) supports our egui 0.37
   (checked on crates.io); `egui_dock` 0.21 is still on egui 0.36. So docking is
   doable without forking anything.
4. **Desktop mode** is a known, loved use (Wallpaper Engine). On Windows a window
   can live behind the desktop icons (the WorkerW trick); on macOS a window at
   desktop level. It's an output like any other (M8).
5. **Twitch:** Twitch's own channel-point rewards already have per-stream and
   per-user limits and cooldowns from 1 minute to 7 days, and tools like Streamer.bot,
   Firebot and Sound Alerts add moderation (timeouts, AutoMod). That is the model if
   we do it: viewers *spend* points to fire a small, pre-approved event, never
   control the show freely.
6. **Crates for the next milestones are current:** `ffmpeg-next` 9.0 (Aug 2026),
   `nokhwa` 0.10.11 (cameras), `image` 0.25.10, `midir` 0.11.1 (Oct 2026).

## Sources

- [Resolume vs TouchDesigner (Interactive & Immersive HQ)](https://interactiveimmersive.io/blog/technology/resolume-vs-touchdesigner/)
- [Which live visuals software should you buy? (alltd.org)](https://alltd.org/?p=61918)
- [Resolume forum: feature voting](https://resolume.com/blog/19816/new-feature-voting-on-the-forum)
- [Resolume forum: wishes threads](https://www.resolume.com/forum/viewtopic.php?p=4257), [Resolume 7 feature requests](https://www.resolume.com/forum/viewtopic.php?p=89622)
- [VirtualDJ forum: Resolume integration](https://virtualdj.com/forums/246964/Wishes_and_new_features/resolume_inegration.html)
- [DJ.Studio feedback: Spout/NDI + MIDI mapping](https://feedback.dj.studio/p/spoutndi-input-and-output-support-custom-shaders-folder-midi-mapping)
- [Synesthesia overview (eliteai.tools)](https://eliteai.tools/tool/synesthesia), [Troikatronix forum on Synesthesia](https://community.troikatronix.com/post/49692)
- [r/vjing: beginner where to start](https://redlib.ssps.io/r/vjing/comments/1c67yvy/beginner_who_doesnt_know_where_to_start/l034h8n/?context=3), [Audio-reactive visuals: my journey with TouchDesigner](https://oliviaema.substack.com/p/navigating-audio-reactive-visuals)
- [Pioneer: PRO DJ LINK licensed to VFX companies](https://www.pioneerdj.com/en/news/2019/pro-dj-link-network-information-licensed-to-vfx-companies/), [PRO DJ LINK Bridge supports Resolume Arena](https://www.leisuretec.co.uk/news/pro-dj-link-bridge-now-officially-supports-resolume-arena)
- [Resolume Arena with Denon StageLinQ (bonedo)](https://www.bonedo.de/artikel/resolume-arena-mit-integriertem-denon-stagelinq-protokoll), [StageLinQ, SoundSwitch and Resolume (djworx)](https://djworx.com/stagelinq-soundswitch-resolume-denon-djs-visual-trinity/)
- [MadMapper features](https://madmapper.com/madmapper/features), [MadMapper mesh warping guide](https://madmapper.com/files/05-Mesh%20Warping.pdf)
- [Wallpaper Engine: audio in SceneScript](https://docs.wallpaperengine.io/en/scene/scenescript/tutorial/audio.html), [Steam discussion on system audio](https://steamcommunity.com/app/431960/discussions/2/1729828401680135391)
- [After Effects audio react templates (Tuts+)](https://photography.tutsplus.com/articles/10-top-audio-react-visualisation-after-effects-templates--cms-35241), [Trapcode Form visualizer (Videohive)](https://videohive.net/item/audio-react-music-visualizer/22057731)
- [Twitch channel points cooldowns (Sound Alerts)](https://support.soundalerts.com/article/channel-points), [Firebot channel rewards](https://docs.firebot.app/v5/core/channel-rewards), [Streamer.bot redemption moderation](https://extensions.streamer.bot/t/channel-point-redemption-moderation/3188)
- [egui_dock on docs.rs](https://docs.rs/crate/egui_dock/latest); crate versions checked on crates.io 2026-11-10.
