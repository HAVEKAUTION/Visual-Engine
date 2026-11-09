/*{
  "name": "Canopy Breaks",
  "author": "Visual Engine",
  "style": "organic / chopped",
  "genres": ["Jungle", "Drum & Bass", "Breakbeat"],
  "description": "Layers of jungle foliage rushing sideways under shafts of light. Hi-hats and breaks drive the speed and light the leaf edges, every snare chops the picture into shifted slices like a cut-up break, and the sub-bass bends the whole canopy.",
  "inputs": [
    { "name": "scroll_speed", "label": "Scroll speed",     "type": "float", "min": 0,   "max": 2,    "default": 0.5 },
    { "name": "density",      "label": "Foliage density",  "type": "float", "min": 0.2, "max": 1,    "default": 0.6 },
    { "name": "chop",         "label": "Snare chop",       "type": "float", "min": 0,   "max": 1.5,  "default": 0.8 },
    { "name": "sub_warp",     "label": "Sub-bass bend",    "type": "float", "min": 0,   "max": 1.5,  "default": 0.6 },
    { "name": "glow",         "label": "Light shafts",     "type": "float", "min": 0,   "max": 1.5,  "default": 0.7 },
    { "name": "trails",       "label": "Trails",           "type": "float", "min": 0,   "max": 0.95, "default": 0.5 },
    { "name": "leaf",         "label": "Leaf colour",      "type": "color", "default": [0.16, 0.42, 0.14, 1.0] },
    { "name": "edge",         "label": "Edge light colour","type": "color", "default": [0.75, 1.0, 0.3, 1.0] }
  ],
  "clocks": [
    { "name": "scroll", "speed": "scroll_speed", "signal": "high_mid_slow", "amount": 1.5 },
    { "name": "sway",   "speed": 0.4,            "signal": "sub_slow",      "amount": 1.0 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // --- Breakbeat chop: horizontal slices jump sideways on each snare ---
    let snare = env(audio.snare_age, 0.12) * params.chop;
    let seed = audio.snare_count;
    var uv = in.uv;
    let slices = 3.0 + floor(hash11(seed) * 7.0);
    let slice = floor(uv.y * slices);
    uv.x += (hash21(vec2f(slice, seed)) - 0.5) * 0.18 * snare;

    // --- Sub-bass bends space ----------------------------------------------
    let sub = clamp(audio.sub_fast, 0.0, 3.0) * params.sub_warp;
    var xy = (uv - 0.5) * vec2f(audio.aspect, -1.0) * 2.0;
    xy += vec2f(sin(xy.y * 2.5 + params.sway * 2.0), cos(xy.x * 2.0 + params.sway * 1.6)) * 0.035 * sub;

    // --- Sky and light shafts ------------------------------------------------
    let sky = mix(vec3f(0.1, 0.22, 0.14), vec3f(0.45, 0.62, 0.32), smoothstep(-0.8, 0.9, xy.y));
    var col = sky;
    let shaft = pow(max(0.0, sin(xy.x * 2.2 + xy.y * 0.9 + params.sway * 0.4)), 10.0)
        * smoothstep(-0.6, 1.0, xy.y);
    col += vec3f(0.65, 0.85, 0.35) * shaft * 0.3 * params.glow * (0.5 + 0.5 * clamp(audio.loudness_slow, 0.0, 2.0));

    // --- Four layers of foliage, back to front --------------------------------
    let hat = env(audio.hat_age, 0.07);
    let threshold = 0.62 - 0.25 * params.density;
    for (var i = 0; i < 4; i++) {
        let fi = f32(i);
        let depth = 1.0 + 0.7 * (3.0 - fi);
        let p = vec2f(xy.x * depth + params.scroll * (0.4 + 0.6 * fi) + fi * 13.7, xy.y * depth + fi * 3.1);
        // Thick at the top (canopy) and bottom (undergrowth), open in the middle.
        let n = fbm2(p * 1.4) + (abs(xy.y) - 0.55) * 0.55;
        let m = smoothstep(threshold - 0.01, threshold + 0.01, n);
        let shade = mix(0.25, 1.0, fi / 3.0);
        // Far layers fade into the sky (atmosphere), near ones are dark silhouettes.
        let layer = mix(sky * 0.8, params.leaf.rgb * mix(0.9, 0.12, fi / 3.0), 0.35 + 0.65 * shade);
        col = mix(col, layer, m);
        // Edges catch the light on hats.
        let edge = smoothstep(0.035, 0.0, abs(n - threshold));
        col += params.edge.rgb * edge * (0.08 + 0.5 * hat) * (1.0 - 0.2 * fi);
    }

    // --- Fireflies: drift, flicker with the breaks ---------------------------
    let g = xy * 6.0 + vec2f(params.scroll * 2.0, 0.0);
    let cell = floor(g);
    let h = hash21(cell);
    let fpos = vec2f(hash21(cell + 1.3), hash21(cell + 7.1)) * 0.8 + 0.1;
    let fd = length(fract(g) - fpos);
    let fly = step(0.85, h) * exp(-fd * 30.0) * (0.3 + 0.7 * env(audio.onset_age, 0.1));
    col += vec3f(1.0, 0.9, 0.4) * fly;

    // Kick: a short exposure pump.
    col *= 1.0 + 0.3 * env(audio.kick_age, 0.12);

    let steps = audio.delta * 60.0;
    let old = prev(in.uv).rgb * pow(params.trails, steps);
    col = max(col, old);
    return vec4f(col, 1.0);
}
