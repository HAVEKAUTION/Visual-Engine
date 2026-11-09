/*{
  "name": "Warehouse Tunnel",
  "author": "Visual Engine",
  "style": "industrial",
  "genres": ["Techno", "Minimal", "Industrial"],
  "palette": ["#0a0a0a", "#ff1f3d", "#3b4a5c", "#9ab0c8", "#e6e6e6"],
  "description": "An endless square corridor of light frames racing towards you. The bass sets the speed and every kick fires the frames. Waves of colour roll along the walls, each one lit by its own part of the spectrum, and a glow breathes at the vanishing point. A few frames per bar burn in the accent colour, and the colour cycle turns the frames into a moving gradient. Optional strobe (off by default).",
  "inputs": [
    { "name": "speed",        "label": "Speed",               "type": "float", "min": 0,    "max": 3,   "default": 1.0 },
    { "name": "spacing",      "label": "Frame spacing",       "type": "float", "min": 0.3,  "max": 3,   "default": 1.0 },
    { "name": "line_width",   "label": "Line width",          "type": "float", "min": 0.2,  "max": 2,   "default": 0.8 },
    { "name": "frame_bright", "label": "Frame brightness",    "type": "float", "min": 0,    "max": 1.5, "default": 1.0 },
    { "name": "roll_speed",   "label": "Roll speed",          "type": "float", "min": -0.5, "max": 0.5, "default": 0.04 },
    { "name": "accent_amt",   "label": "Accent frames",       "type": "float", "min": 0,    "max": 0.6, "default": 0.12 },
    { "name": "colour_cycle", "label": "Colour cycle",        "type": "float", "min": 0,    "max": 1,   "default": 0.0 },
    { "name": "waves",        "label": "Wall waves",          "type": "float", "min": 0,    "max": 1.5, "default": 0.6 },
    { "name": "wave_count",   "label": "Wave count",          "type": "float", "min": 1,    "max": 8,   "default": 3 },
    { "name": "wave_speed",   "label": "Wave speed",          "type": "float", "min": 0,    "max": 2,   "default": 0.4 },
    { "name": "core_glow",    "label": "Vanishing-point glow","type": "float", "min": 0,    "max": 1.5, "default": 0.5 },
    { "name": "haze",         "label": "Depth haze",          "type": "float", "min": 0.05, "max": 0.6, "default": 0.22 },
    { "name": "strobe",       "label": "Strobe (careful)",    "type": "float", "min": 0,    "max": 1,   "default": 0.0 }
  ],
  "clocks": [
    { "name": "travel", "speed": "speed",      "signal": "bass_slow",     "amount": 1.0 },
    { "name": "roll",   "speed": "roll_speed", "signal": "loudness_slow", "amount": 1.0 },
    { "name": "flow",   "speed": "wave_speed", "signal": "mid_slow",      "amount": 1.0 }
  ]
}*/

// Palette: 0 = background, 1 = accent, 2 = wave base, 3 = seams / glow,
// 4 = frames.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kick = env(audio.kick_age, 0.11);
    let q = rot2(params.roll) * in.xy;
    // Square tunnel: distance to the nearest wall gives depth.
    let d = max(max(abs(q.x), abs(q.y)), 1e-3);
    let z = 0.5 / d;
    let t = z / params.spacing + params.travel * 2.0;
    let id = floor(t);
    let f = fract(t);
    let aa = fwidth(t) * 1.5;
    let fog = exp(-params.haze * z);

    // Position across the current wall, -1..1, and which wall (0..3).
    let on_side = abs(q.x) > abs(q.y);
    let across = select(q.x, q.y, on_side) / d;
    let wall = select(select(2.0, 0.0, q.y > 0.0), select(3.0, 1.0, q.x > 0.0), on_side);

    // --- Background: palette 0 with waves rolling along the walls -------
    var col = pal_color(0);
    let wn = floor(params.wave_count);
    // Ribbons snake along the walls as they recede.
    let snake = 1.8 * sin(t * params.spacing * 4.0 - params.flow * 2.0 + wall * 1.3)
        * smoothstep(12.0, 3.0, z);
    let phase = across * wn * PI + snake - params.flow * 3.0 + wall * 1.7;
    let wave = 0.5 + 0.5 * sin(phase);
    let ribbon = smoothstep(0.75, 1.0, wave);
    // Each ribbon listens to its own band of the spectrum.
    let band = 1.0 - exp(-0.8 * mel_at(fract(0.5 + 0.5 * across + wall * 0.25)));
    let wave_col = pal(0.35 + 0.15 * sin(params.flow + wall) + 0.25 * band);
    col += wave_col * ribbon * (0.15 + 0.85 * band) * params.waves * fog;
    col += pal_color(2) * wave * 0.08 * params.waves * fog;

    // --- Light frames ------------------------------------------------------
    let w = 0.03 * params.line_width;
    let frame = 1.0 - smoothstep(w, w + aa, min(f, 1.0 - f));
    let lit = step(hash11(id * 1.37 + floor(audio.bar_count) * 9.1), params.accent_amt);
    let cyc = pal(id * 0.13 + params.travel * 0.05);
    var frame_col = mix(pal_color(4), cyc, params.colour_cycle);
    frame_col = mix(frame_col, pal_color(1) * 1.3, lit);
    col += frame_col * frame * fog * (0.35 + 1.1 * kick) * params.frame_bright;

    // Corner seams flicker with the hats.
    let seam = 1.0 - smoothstep(0.0, 0.006 * z + 0.002, abs(abs(q.x) - abs(q.y)));
    col += pal_color(3) * seam * fog * (0.05 + 0.5 * env(audio.hat_age, 0.05));

    // Faint concrete texture on the walls.
    col += vec3f(0.03) * noise2(vec2f(across * 3.0 + wall * 7.0, t) * vec2f(2.0, 3.0)) * fog;

    // Glow at the vanishing point, breathing with the bass.
    let core = exp(-d * 9.0) * params.core_glow * (0.4 + 0.6 * clamp(audio.bass_slow, 0.0, 2.0));
    col += mix(pal_color(3), pal_color(1), kick) * core;

    // Strobe on the beat, only while the track is pushing.
    let hot = smoothstep(1.0, 1.3, audio.loudness_slow);
    col += vec3f(1.0) * env(audio.beat_age, 0.035) * params.strobe * hot;

    // Drop: everything flares in the accent colour.
    col += pal_color(1) * env(audio.drop_age, 0.6) * 0.5 * fog;

    return vec4f(col, 1.0);
}
