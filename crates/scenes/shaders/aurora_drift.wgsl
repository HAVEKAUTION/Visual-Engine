/*{
  "name": "Aurora Drift",
  "author": "Visual Engine",
  "style": "calm / slow",
  "genres": ["Ambient", "Acoustic", "Downtempo", "Classical"],
  "description": "Northern-light curtains rippling over dark mountains under a starry sky. Loudness sets how fast the curtains flow, each part of the curtain glows with its own slice of the spectrum, and the colour shifts with the brightness of the sound. Made for slow and quiet music.",
  "inputs": [
    { "name": "flow_speed", "label": "Flow speed",  "type": "float", "min": 0,   "max": 1,    "default": 0.12 },
    { "name": "brightness", "label": "Brightness",  "type": "float", "min": 0.3, "max": 2,    "default": 1.0 },
    { "name": "height",     "label": "Height",      "type": "float", "min": 0,   "max": 1,    "default": 0.5 },
    { "name": "hue_base",   "label": "Colour",      "type": "float", "min": 0,   "max": 1,    "default": 0.36 },
    { "name": "stars",      "label": "Stars",       "type": "float", "min": 0,   "max": 1,    "default": 0.6 },
    { "name": "trails",     "label": "Silkiness",   "type": "float", "min": 0,   "max": 0.97, "default": 0.85 }
  ],
  "clocks": [
    { "name": "flow",    "speed": "flow_speed", "signal": "loudness_slow", "amount": 1.5 },
    { "name": "shimmer", "speed": 0.1,          "signal": "high_mid_slow", "amount": 2.0 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let xy = in.xy;

    // --- Sky and stars -------------------------------------------------------
    var col = mix(vec3f(0.005, 0.01, 0.03), vec3f(0.02, 0.04, 0.08), smoothstep(1.0, -0.6, xy.y));
    let sg = xy * 40.0;
    let sc = floor(sg);
    let sh = hash21(sc);
    let sd = length(fract(sg) - vec2f(hash21(sc + 3.1), hash21(sc + 8.2)));
    let twinkle = 0.6 + 0.4 * sin(audio.time * (1.0 + 3.0 * sh) + sh * 30.0);
    col += vec3f(0.9, 0.95, 1.0) * step(0.97, sh) * smoothstep(0.08, 0.0, sd) * twinkle * params.stars;

    // --- Three aurora curtains ------------------------------------------------
    for (var i = 0; i < 3; i++) {
        let fi = f32(i);
        let base_y = -0.45 + 0.7 * params.height + 0.14 * fi;
        let x = xy.x * (0.7 + 0.2 * fi) + fi * 4.1;
        let y0 = base_y + 0.4 * (fbm2(vec2f(x * 0.6 + params.flow * 0.3, fi * 2.0)) - 0.5);
        let above = xy.y - y0;
        // Bright lower edge, fading rays upward.
        let curtain = smoothstep(-0.03, 0.01, above) * exp(-max(above, 0.0) * (1.4 + 0.6 * fi));
        let rays = 0.55 + 0.45 * noise2(vec2f(x * 18.0 + params.flow * 0.6 + params.shimmer, fi * 5.0));
        let band = 1.0 - exp(-0.7 * mel_at(fract(abs(xy.x) / max(audio.aspect, 0.01) * 0.9 + fi * 0.15)));
        let hue = params.hue_base + 0.08 * fi + 0.12 * audio.centroid_hue_slow + 0.25 * max(above, 0.0);
        let c = hsv2rgb(vec3f(hue, 0.75, 1.0));
        col += c * curtain * rays * (0.15 + 0.6 * band) * params.brightness * (0.7 - 0.15 * fi);
    }

    // --- Mountains -----------------------------------------------------------
    let ridge = -0.6 + 0.25 * fbm2(vec2f(xy.x * 1.2, 7.0)) + 0.1 * fbm2(vec2f(xy.x * 5.0, 3.0));
    let mtn = smoothstep(ridge + 0.004, ridge - 0.004, xy.y);
    col = mix(col, vec3f(0.004, 0.006, 0.012), mtn);

    // --- Silky persistence (feedback, time-based) ----------------------------
    let steps = audio.delta * 60.0;
    let old = prev(in.uv).rgb * pow(params.trails, steps);
    col = max(col, old);
    return vec4f(col, 1.0);
}
