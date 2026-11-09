/*{
  "name": "Radial Spectrum",
  "author": "Visual Engine",
  "style": "minimal",
  "description": "A clean ring of spectrum bars (bass at the top, highs at the bottom, mirrored). The ring and the centre dot pulse on kicks and leave fading echoes.",
  "inputs": [
    { "name": "radius",     "label": "Ring radius",       "type": "float", "min": 0.1,  "max": 0.8,  "default": 0.34 },
    { "name": "bar_length", "label": "Bar length",        "type": "float", "min": 0,    "max": 1.2,  "default": 0.5 },
    { "name": "bars",       "label": "Bars",              "type": "float", "min": 16,   "max": 160,  "default": 72 },
    { "name": "thickness",  "label": "Bar thickness",     "type": "float", "min": 0.1,  "max": 0.9,  "default": 0.45 },
    { "name": "kick_pulse", "label": "Kick pulse",        "type": "float", "min": 0,    "max": 1,    "default": 0.6 },
    { "name": "echo",       "label": "Echo (feedback)",   "type": "float", "min": 0,    "max": 0.95, "default": 0.8 },
    { "name": "turn_speed", "label": "Turn speed",        "type": "float", "min": -0.3, "max": 0.3,  "default": 0.03 },
    { "name": "color",      "label": "Colour",            "type": "color", "default": [0.55, 0.85, 1.0, 1.0] },
    { "name": "accent",     "label": "Kick colour",       "type": "color", "default": [1.0, 0.45, 0.3, 1.0] }
  ],
  "clocks": [
    { "name": "turn", "speed": "turn_speed", "signal": "loudness_slow", "amount": 1.0 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kick = env(audio.kick_age, 0.16) * params.kick_pulse;
    let px = 2.0 / max(audio.res_y, 1.0); // one pixel, in xy units

    let q = rot2(params.turn) * in.xy;
    let r = length(q);
    // Angle: 0 at the top, ±0.5 at the bottom.
    let t = atan2(q.x, q.y) / TAU;

    // Which bar this pixel is in, and how far from its centre line.
    let n = floor(params.bars);
    let cell = (t + 0.5) * n;
    let id = floor(cell);
    let local = fract(cell) - 0.5;
    // Mirror left/right: 0 = top (bass) .. 1 = bottom (highs).
    let m = abs((id + 0.5) / n * 2.0 - 1.0);
    let level = 1.0 - exp(-0.7 * mel_at(m));

    let r0 = params.radius * (1.0 + 0.15 * kick);
    let len = params.bar_length * level + px;
    let half_w = 0.5 * params.thickness * TAU * r0 / n;
    let d_side = abs(local) * TAU * r / n - half_w;
    let d_end = max(r0 - r, r - (r0 + len));
    let bar = 1.0 - smoothstep(0.0, 1.5 * px, max(d_side, d_end));

    // Thin inner ring and centre dot.
    let ring = 1.0 - smoothstep(0.0, 1.5 * px, abs(r - 0.9 * r0) - 0.6 * px);
    let core = 1.0 - smoothstep(0.0, 1.5 * px, r - r0 * (0.06 + 0.12 * kick));

    let base = params.color.rgb;
    let hot = mix(base, params.accent.rgb, kick);
    var col = hot * bar * (0.55 + 0.45 * level);
    col += base * ring * (0.35 + 0.65 * kick);
    col += mix(vec3<f32>(1.0), params.accent.rgb, 0.5) * core;

    // Echo: last frame, scaled up slightly about the centre so copies ripple
    // outward. Time-based decay (same look at any frame rate).
    let steps = audio.delta * 60.0;
    let fuv = (in.uv - 0.5) * pow(0.988, steps) + 0.5;
    let old = prev(fuv).rgb * pow(params.echo, steps) * 0.97;
    col = max(col, old);

    return vec4<f32>(col, 1.0);
}
