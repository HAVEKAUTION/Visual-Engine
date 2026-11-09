/*{
  "name": "Iron Furnace",
  "author": "Visual Engine",
  "style": "aggressive / fire",
  "genres": ["Metal", "Hardcore", "Rock"],
  "description": "A roaring wall of molten fire that rises faster as the guitars thicken (mids and upper mids). Kicks shake the screen (double-kick = rapid shudder), snares split it with lightning, embers fly, and a drop or breakdown blasts it white-hot.",
  "inputs": [
    { "name": "heat",       "label": "Heat",          "type": "float", "min": 0.2, "max": 2,   "default": 1.0 },
    { "name": "rise_speed", "label": "Rise speed",    "type": "float", "min": 0,   "max": 2,   "default": 0.6 },
    { "name": "shake",      "label": "Kick shake",    "type": "float", "min": 0,   "max": 1,   "default": 0.5 },
    { "name": "lightning",  "label": "Snare lightning","type": "float", "min": 0,  "max": 1.5, "default": 1.0 },
    { "name": "embers",     "label": "Embers",        "type": "float", "min": 0,   "max": 1,   "default": 0.6 },
    { "name": "grain",      "label": "Grain",         "type": "float", "min": 0,   "max": 1,   "default": 0.35 }
  ],
  "clocks": [
    { "name": "rise",  "speed": "rise_speed", "signal": "mid_slow",      "amount": 1.5 },
    { "name": "churn", "speed": 0.25,         "signal": "high_mid_slow", "amount": 2.0 }
  ]
}*/

fn fire_ramp(t: f32) -> vec3<f32> {
    return vec3f(
        smoothstep(0.05, 0.45, t),
        smoothstep(0.35, 0.85, t) * 0.85,
        smoothstep(0.75, 1.2, t) * 0.7,
    ) * 1.15;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kick = env(audio.kick_age, 0.07);
    let kc = audio.kick_count;
    let shake = vec2f(hash11(kc) - 0.5, hash11(kc + 7.3) - 0.5) * 0.08 * kick * params.shake;
    let xy = in.xy + shake;

    // --- Fire: warped fractal noise scrolling upward ---------------------
    let p = vec2f(xy.x * 1.6, xy.y * 1.1 - params.rise * 1.6);
    let warp = fbm2(p * 1.3 + vec2f(params.churn, 0.0));
    let n = fbm2(p + vec2f(warp * 1.3, 0.0));
    let drive = clamp(0.5 * (audio.mid_fast + audio.high_mid_fast), 0.0, 2.5);
    let height = 1.25 - (xy.y + 1.0) * 0.45; // hotter at the bottom
    let flame = n * height * params.heat * (0.55 + 0.45 * drive) + 0.25 * kick * height;
    var col = fire_ramp(flame);

    // --- Lightning on snares: a jagged crack, new path every hit ---------
    let sn = env(audio.snare_age, 0.14) * params.lightning;
    let sc = audio.snare_count;
    let x0 = (hash11(sc) - 0.5) * 1.6 * audio.aspect;
    let path = x0 + (fbm2(vec2f(xy.y * 5.0, sc * 3.7)) - 0.5) * 0.7;
    let dist = abs(xy.x - path);
    col += vec3f(0.8, 0.85, 1.0) * (exp(-dist * 140.0) + 0.25 * exp(-dist * 12.0)) * sn;

    // --- Embers flying up ------------------------------------------------
    let g = vec2f(xy.x * 9.0, xy.y * 9.0 - params.rise * 14.0);
    let cell = floor(g);
    let ep = vec2f(hash21(cell + 2.1), hash21(cell + 5.7)) * 0.8 + 0.1;
    let ember = step(0.88, hash21(cell)) * exp(-length(fract(g) - ep) * 22.0);
    col += vec3f(1.0, 0.55, 0.15) * ember * params.embers * (0.6 + 0.8 * drive);

    // --- Drop / breakdown: white-hot blast -------------------------------
    col += vec3f(1.0, 0.8, 0.6) * env(audio.drop_age, 0.4) * 0.8;

    // --- Smoke trail upward (feedback), grit and vignette ---------------
    let steps = audio.delta * 60.0;
    let old = prev(in.uv + vec2f(0.0, 0.004 * steps)).rgb * pow(0.55, steps);
    col = max(col, old);
    col += (hash21(in.pos.xy + fract(audio.time * 13.1) * 91.0) - 0.5) * 0.12 * params.grain;
    col *= 1.0 - 0.45 * pow(length(in.uv - 0.5) * 1.3, 2.5);
    return vec4f(max(col, vec3f(0.0)), 1.0);
}
