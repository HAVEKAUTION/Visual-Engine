/*{
  "name": "Midnight Window",
  "author": "Visual Engine",
  "style": "cozy / soft",
  "genres": ["Lo-Fi", "Chillhop", "Jazz"],
  "palette": ["#16112a", "#ffb86b", "#ff7eb3", "#7afcff", "#feff9c"],
  "description": "City lights blurred through a rainy window at night, on slightly worn tape. Each of the soft bokeh lights listens to its own slice of the spectrum, kicks breathe the exposure gently, hats bring the rain.",
  "inputs": [
    { "name": "drift_speed", "label": "Drift speed",   "type": "float", "min": 0,    "max": 1,   "default": 0.15 },
    { "name": "bokeh_size",  "label": "Light size",    "type": "float", "min": 0.04, "max": 0.3, "default": 0.12 },
    { "name": "rain",        "label": "Rain",          "type": "float", "min": 0,    "max": 1,   "default": 0.6 },
    { "name": "warmth",      "label": "Warmth",        "type": "float", "min": 0,    "max": 1,   "default": 0.7 },
    { "name": "wobble",      "label": "Tape wobble",   "type": "float", "min": 0,    "max": 1,   "default": 0.4 },
    { "name": "grain",       "label": "Grain",         "type": "float", "min": 0,    "max": 1,   "default": 0.45 }
  ],
  "clocks": [
    { "name": "drift",    "speed": "drift_speed", "signal": "loudness_slow", "amount": 0.5 },
    { "name": "rainfall", "speed": 0.5,           "signal": "high_mid_slow", "amount": 0.5 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // --- Tape wobble -----------------------------------------------------
    var uv = in.uv;
    uv.x += sin(uv.y * 9.0 + audio.time * 1.3) * 0.0025 * params.wobble;
    uv.x += (hash11(floor(audio.time * 8.0)) - 0.5) * 0.002 * params.wobble;
    let xy = (uv - 0.5) * vec2f(audio.aspect, -1.0) * 2.0;

    // --- Night city background -------------------------------------------
    var col = pal_color(0) * mix(0.6, 1.2, smoothstep(1.0, -1.0, xy.y));

    // --- Bokeh lights, one per mel band ------------------------------------
    for (var i = 0; i < 24; i++) {
        let fi = f32(i);
        let span = 2.0 * audio.aspect + 0.6;
        let x = fract(hash11(fi * 3.17) + params.drift * (0.02 + 0.03 * hash11(fi * 1.9))) * span - 0.5 * span;
        let y = hash11(fi * 7.31) * 1.6 - 0.9;
        let r = params.bokeh_size * (0.6 + 0.8 * hash11(fi * 5.3));
        let d = length(xy - vec2f(x, y));
        let disk = smoothstep(r, r * 0.8, d);
        let rim = smoothstep(r * 0.75, r * 0.95, d) * disk;
        // Lights use palette colours 1–4.
        let lc = mix(pal_color(1 + i32(hash11(fi * 2.7) * 4.0)), vec3f(1.0), 0.15);
        let band = 1.0 - exp(-0.8 * mel_band(i + 4));
        col += lc * (disk * 0.18 + rim * 0.12) * (0.4 + 0.9 * band);
    }

    // --- Rain on the glass: beads plus streaks running down -----------------
    let g = xy * vec2f(9.0, 5.0);
    let cell = floor(g);
    let h = hash21(cell);
    let fall = fract(params.rainfall * (0.3 + h) + h);
    let bead_pos = vec2f(0.5 + 0.3 * (hash21(cell + 3.3) - 0.5), 1.0 - fall);
    let bd = length((fract(g) - bead_pos) * vec2f(1.0, 1.8));
    let bead = smoothstep(0.12, 0.05, bd) * step(1.0 - params.rain * 0.7, h);
    let trail = smoothstep(0.03, 0.0, abs(fract(g.x) - bead_pos.x)) * step(fract(g.y), bead_pos.y)
        * step(1.0 - params.rain * 0.7, h) * 0.25;
    let hat_rain = 0.6 + 0.6 * env(audio.hat_age, 0.2);
    col += mix(pal_color(3), vec3f(0.7), 0.6) * (bead * 0.35 + trail * 0.15) * hat_rain;

    // --- Warm grade, gentle kick breath, soft blur via feedback -------------
    col *= mix(vec3f(1.0), vec3f(1.15, 0.95, 0.78), params.warmth);
    col *= 1.0 + 0.08 * env(audio.kick_age, 0.35);
    col += vec3f(0.035, 0.028, 0.04);
    let steps = audio.delta * 60.0;
    col = mix(col, prev(in.uv).rgb, pow(0.4, steps));

    col += (hash21(in.pos.xy + fract(audio.time * 3.7) * 57.0) - 0.5) * 0.08 * params.grain;
    col *= 1.0 - 0.5 * pow(length(in.uv - 0.5) * 1.25, 2.2);
    return vec4f(max(col, vec3f(0.0)), 1.0);
}
