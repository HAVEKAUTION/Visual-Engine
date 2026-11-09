# Writing scenes

A scene is one `.wgsl` file in the scenes folder (`crates/scenes/shaders` when you run
from the source tree; the app's **Open scenes folder** button opens it). Save the file and
it reloads instantly. If it has an error, the last working version keeps running and the
error is shown in the scene panel.

## The header

The file starts with a JSON comment (ISF-style; ISF's upper-case keys also work):

```wgsl
/*{
  "name": "My Scene",
  "author": "me",
  "description": "What it does.",
  "inputs": [
    { "name": "zoom",  "label": "Zoom", "type": "float", "min": 0.5, "max": 3, "default": 1 },
    { "name": "invert", "type": "bool", "default": false },
    { "name": "tint",   "type": "color", "default": [1, 0.5, 0.2, 1] }
  ],
  "clocks": [
    { "name": "spin", "speed": 0.2, "signal": "bass_slow", "amount": 1.5 }
  ],
  "buffers": [ { "name": "A", "persistent": true } ],
  "passes":  [ { "entry": "fs_trail", "target": "A" }, { "entry": "fs_main" } ]
}*/
```

- **inputs** become sliders / checkboxes / colour pickers, read as `params.zoom`,
  `params.invert` (0 or 1), `params.tint` (vec4).
- **clocks** follow the "drive speed, not position" rule:
  `value += speed · (1 + amount · signal) · Δt`. Read as `params.spin`. `speed` and
  `amount` can be numbers or the name of a float input. Use clocks for all motion.
- **buffers** `A` / `B`: off-screen textures, sampled with `sample_a(uv)` / `sample_b(uv)`.
  Persistent buffers keep last frame's contents (feedback); non-persistent start black.
- **passes**: run in order; every pass but the last renders into a buffer, the last draws
  the picture. Default: one pass, `fs_main`.

## The shader

```wgsl
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kick = env(audio.kick_age, 0.15);          // 1 on a kick, fading out
    let r = length(rot2(params.spin) * in.xy);
    let old = prev(in.uv).rgb * pow(0.9, audio.delta * 60.0); // feedback, time-based fade
    return vec4<f32>(max(vec3<f32>(kick * step(r, 0.5)), old), 1.0);
}
```

`in.uv` is 0..1 from the top-left; `in.xy` is centred, y up, aspect-correct.

**Signals** (`audio.*`, all `f32`): `time delta frame res_x res_y aspect`,
`loudness(_fast/_slow) centroid_hz centroid_hue centroid_hue_slow`,
per band `sub bass low_mid mid high_mid air` with `_fast` / `_slow` (1.0 = typical),
`onset/kick/snare/hat` + `_age` (seconds since) `_count` `_strength`, `onset_rate`,
`bpm tempo_confidence tempo_locked beat_phase bar_phase phrase_phase beat_in_bar
bar_in_phrase phrase_bars bar_count beat_age beat_count beats`,
`building build_amount build_age drop_age drop_count section_age section_count`,
built-in clocks `clock_energy clock_bass clock_high clock_kick`.
Mel spectrum: `mel_band(i)` (0..31) or `mel_at(x)` (0..1).

**Helpers**: `PI TAU env(age, decay) pulse(phase, k) hsv2rgb rot2 hash11 hash21 noise2
fbm2 prev(uv) sample_a(uv) sample_b(uv)`, sampler `smp`, textures `prev_frame buf_a buf_b`.

Write colours as they should look on screen (like Shadertoy). Use `textureSampleLevel`
(not `textureSample`) inside `if` blocks. Fade feedback with `pow(x, audio.delta * 60.0)`
so it looks the same at any frame rate.
