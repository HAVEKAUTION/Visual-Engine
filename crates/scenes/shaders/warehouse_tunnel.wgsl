/*{
  "name": "Warehouse Tunnel",
  "author": "Visual Engine",
  "style": "industrial / monochrome",
  "genres": ["Techno", "Minimal", "Industrial"],
  "description": "An endless square corridor of light frames racing towards you. The bass sets the speed, every kick fires the frames, hats flicker along the corners and a few frames per bar burn in the accent colour. Optional strobe on the beat when the energy is high (off by default).",
  "inputs": [
    { "name": "speed",      "label": "Speed",            "type": "float", "min": 0,   "max": 3,   "default": 1.0 },
    { "name": "spacing",    "label": "Frame spacing",    "type": "float", "min": 0.3, "max": 3,   "default": 1.0 },
    { "name": "line_width", "label": "Line width",       "type": "float", "min": 0.2, "max": 2,   "default": 0.8 },
    { "name": "roll_speed", "label": "Roll speed",       "type": "float", "min": -0.5,"max": 0.5, "default": 0.04 },
    { "name": "accent_amt", "label": "Accent frames",    "type": "float", "min": 0,   "max": 0.5, "default": 0.12 },
    { "name": "strobe",     "label": "Strobe (careful)", "type": "float", "min": 0,   "max": 1,   "default": 0.0 },
    { "name": "accent",     "label": "Accent colour",    "type": "color", "default": [1.0, 0.12, 0.15, 1.0] }
  ],
  "clocks": [
    { "name": "travel", "speed": "speed",      "signal": "bass_slow",     "amount": 1.0 },
    { "name": "roll",   "speed": "roll_speed", "signal": "loudness_slow", "amount": 1.0 }
  ]
}*/

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
    let w = 0.03 * params.line_width;
    let frame = 1.0 - smoothstep(w, w + aa, min(f, 1.0 - f));
    let fog = exp(-0.22 * z);

    // A few frames per bar glow in the accent colour.
    let lit = step(hash11(id * 1.37 + floor(audio.bar_count) * 9.1), params.accent_amt);
    let base = mix(vec3f(0.85), params.accent.rgb * 1.3, lit);
    var col = base * frame * fog * (0.35 + 1.1 * kick);

    // Corner seams flicker with the hats.
    let seam = 1.0 - smoothstep(0.0, 0.006 * z + 0.002, abs(abs(q.x) - abs(q.y)));
    col += vec3f(0.6, 0.7, 0.8) * seam * fog * (0.05 + 0.5 * env(audio.hat_age, 0.05));

    // Faint concrete texture on the walls.
    let wall_uv = vec2f(select(q.y, q.x, abs(q.y) > abs(q.x)) * z, t);
    col += vec3f(0.035) * noise2(wall_uv * vec2f(6.0, 3.0)) * fog;

    // Strobe on the beat, only while the track is pushing.
    let hot = smoothstep(1.0, 1.3, audio.loudness_slow);
    col += vec3f(1.0) * env(audio.beat_age, 0.035) * params.strobe * hot;

    // Drop: everything flares in the accent colour.
    col += params.accent.rgb * env(audio.drop_age, 0.6) * 0.5 * fog;

    return vec4f(col, 1.0);
}
