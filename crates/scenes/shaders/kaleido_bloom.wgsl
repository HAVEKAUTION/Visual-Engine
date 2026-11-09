/*{
  "name": "Kaleido Bloom",
  "author": "Visual Engine",
  "style": "psychedelic",
  "genres": ["Psytrance", "Trance", "Psychedelic"],
  "description": "A domain-warped fractal folded into a kaleidoscope, with feedback trails. Colour follows how bright the sound is (spectral centroid); bass drives the spin, loudness the flow, kicks punch the zoom.",
  "inputs": [
    { "name": "segments",   "label": "Mirror segments",   "type": "float", "min": 2,    "max": 12,   "default": 6 },
    { "name": "zoom",       "label": "Zoom",              "type": "float", "min": 0.3,  "max": 3,    "default": 1.0 },
    { "name": "warp",       "label": "Warp amount",       "type": "float", "min": 0,    "max": 2,    "default": 1.0 },
    { "name": "spin_speed", "label": "Spin speed",        "type": "float", "min": -0.6, "max": 0.6,  "default": 0.08 },
    { "name": "flow_speed", "label": "Flow speed",        "type": "float", "min": 0,    "max": 1.5,  "default": 0.25 },
    { "name": "trails",     "label": "Trails (feedback)", "type": "float", "min": 0,    "max": 0.97, "default": 0.88 },
    { "name": "kick_punch", "label": "Kick punch",        "type": "float", "min": 0,    "max": 1,    "default": 0.6 },
    { "name": "saturation", "label": "Saturation",        "type": "float", "min": 0,    "max": 1,    "default": 0.85 },
    { "name": "hue_shift",  "label": "Hue shift",         "type": "float", "min": 0,    "max": 1,    "default": 0.0 }
  ],
  "clocks": [
    { "name": "spin",    "speed": "spin_speed", "signal": "bass_slow",     "amount": 1.5 },
    { "name": "flow",    "speed": "flow_speed", "signal": "loudness_slow", "amount": 1.2 },
    { "name": "shimmer", "speed": 0.15,         "signal": "high_mid_slow", "amount": 2.0 }
  ]
}*/

// Every motion here is a clock (params.spin, params.flow, params.shimmer):
// the music changes how FAST things move, never where they are, so nothing
// jumps. Kicks only add short, decaying pulses on top.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kick = env(audio.kick_age, 0.18) * params.kick_punch;

    // --- Kaleidoscope fold ---------------------------------------------
    var q = rot2(params.spin) * in.xy;
    q = q / (params.zoom * (1.0 + 0.3 * kick));
    let r = length(q);
    let seg = TAU / max(floor(params.segments), 1.0);
    var a = atan2(q.y, q.x);
    a = abs(fract(a / seg + 0.5) - 0.5) * seg;
    q = vec2<f32>(cos(a), sin(a)) * r;

    // --- Domain-warped fractal noise: f(p + w(p + w(p))) ---------------
    let t = params.flow;
    let p0 = q * 1.4 + vec2<f32>(0.3 * t, -0.2 * t);
    let w1 = vec2<f32>(
        fbm2(p0 + vec2<f32>(0.0, 0.4 * t)),
        fbm2(p0 + vec2<f32>(5.2, 1.3) - 0.3 * t),
    );
    let k = params.warp * 3.0;
    let w2 = vec2<f32>(
        fbm2(p0 + k * w1 + vec2<f32>(1.7, 9.2) + 0.2 * params.shimmer),
        fbm2(p0 + k * w1 + vec2<f32>(8.3, 2.8)),
    );
    let f = fbm2(p0 + k * w2);

    // --- Colour ------------------------------------------------------------
    // Hue from the spectral centroid: bassy, dark sound = warm, bright = cool.
    let hue = audio.centroid_hue_slow * 0.75 + params.hue_shift + 0.35 * length(w2) + 0.3 * f;
    let level = clamp(audio.loudness_slow, 0.0, 3.0);
    let v = smoothstep(0.2, 0.9, f) * 1.1 * (0.5 + 0.35 * level) + 0.7 * kick * f * f;
    var col = hsv2rgb(vec3<f32>(hue, params.saturation, 1.0)) * v;
    // Bright filaments where the warp folds hard, sparkling with the hats.
    let fil = smoothstep(0.72, 0.95, w1.x * w2.y * 1.8);
    col += hsv2rgb(vec3<f32>(hue + 0.5, 0.5 * params.saturation, 1.0))
        * fil * (0.25 + 0.6 * env(audio.hat_age, 0.08));
    // Soft fade at the centre so the fold point doesn't look pinched.
    col *= smoothstep(0.0, 0.25, r * params.zoom);

    // --- Feedback trails -------------------------------------------------
    // Sample last frame slightly zoomed in and rotated, so trails drift
    // outward and swirl. Decay uses elapsed time, so it looks the same at any
    // frame rate.
    let steps = audio.delta * 60.0;
    let aspect = vec2<f32>(audio.aspect, 1.0);
    let c = (in.uv - 0.5) * aspect;
    let fuv = rot2((0.004 + 0.012 * kick) * steps) * c * pow(0.985 - 0.02 * kick, steps) / aspect + 0.5;
    let old = prev(fuv).rgb * pow(params.trails, steps);
    col = max(col, old);

    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.5)), 1.0);
}
