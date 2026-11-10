/*{
  "name": "Fractal Bloom",
  "author": "Visual Engine",
  "style": "psychedelic",
  "genres": ["Psytrance", "Techno", "Trance", "Ambient", "Psychedelic"],
  "palette": ["#ff3d9a", "#7cff3a", "#ff8a00", "#2ad4ff", "#ffe14a"],
  "description": "Round bubbling bulbs with sharp spiky edges: the Mandelbrot set, slowly diving in and out with the loudness, while the bulbs bubble with the bass. Switch Shape to Flowers for its Julia cousins: fractal blossoms that keep changing form. The insides fan out in colour slices, the spiky edges flash on every kick and sparkle on the hi-hats.",
  "inputs": [
    { "name": "shape",        "label": "Shape",                  "type": "choice", "options": ["Bulbs (Mandelbrot)", "Flowers (Julia)"], "default": 0 },
    { "name": "bubbling",     "label": "Bubbling",               "type": "float",  "min": 0,    "max": 1,    "default": 0.35 },
    { "name": "wander_speed", "label": "Bubble speed",           "type": "float",  "min": 0,    "max": 0.6,  "default": 0.06 },
    { "name": "depth",        "label": "Dive depth",             "type": "float",  "min": 0,    "max": 12,   "default": 5.0 },
    { "name": "dive_speed",   "label": "Dive speed",             "type": "float",  "min": 0,    "max": 0.5,  "default": 0.05 },
    { "name": "dive_point",   "label": "Dive into",              "type": "choice", "options": ["Seahorse valley", "Elephant valley", "Spiral", "Mini copy"], "default": 0 },
    { "name": "spin_speed",   "label": "Spin speed",             "type": "float",  "min": -0.5, "max": 0.5,  "default": 0.04 },
    { "name": "slices",       "label": "Colour slices inside",   "type": "float",  "min": 0,    "max": 12,   "default": 3.0 },
    { "name": "edge_flash",   "label": "Edges flash on the kick", "type": "float", "min": 0,    "max": 2,    "default": 0.8 },
    { "name": "sparkle",      "label": "Hi-hat sparkle",         "type": "float",  "min": 0,    "max": 1,    "default": 0.4 },
    { "name": "darkness",     "label": "Dark background",        "type": "float",  "min": 0,    "max": 1,    "default": 0.75 },
    { "name": "hue_shift",    "label": "Colour shift",           "type": "float",  "min": 0,    "max": 1,    "default": 0.0 },
    { "name": "detail",       "label": "Detail (steps)",         "type": "float",  "min": 40,   "max": 300,  "default": 140 },
    { "name": "smooth_edges", "label": "Smooth edges",           "type": "choice", "options": ["Off (faster)", "On (4 samples)"], "default": 1 }
  ],
  "clocks": [
    { "name": "wander", "speed": "wander_speed", "signal": "bass_slow",     "amount": 1.5 },
    { "name": "dive",   "speed": "dive_speed",   "signal": "loudness_slow", "amount": 1.0 },
    { "name": "spin",   "speed": "spin_speed",   "signal": "mid_slow",      "amount": 1.0 }
  ]
}*/

// Motion is all clocks (the music sets the speed, never the position), so the
// bulbs bubble and the dive breathes without jumping. The dive goes in and
// out (a cosine), which also keeps it inside what 32-bit floats can show.

fn dive_target(i: i32) -> vec2<f32> {
    switch i {
        case 1: { return vec2f(0.2850, 0.0110); }
        case 2: { return vec2f(-0.761574, -0.0847596); }
        case 3: { return vec2f(-1.7685, 0.0010); }
        default: { return vec2f(-0.743644, 0.131825); }
    }
}

// A point on the edge of the big heart shape: Julia sets made from these
// are round, bubbly and spiky at once.
fn julia_c(t: f32) -> vec2<f32> {
    let a = t * TAU;
    let e1 = vec2f(cos(a), sin(a));
    let e2 = vec2f(cos(2.0 * a), sin(2.0 * a));
    // 0.5·e^{iθ} − 0.25·e^{2iθ}, nudged just outside for more filigree.
    return (e1 * 0.5 - e2 * 0.25) * 1.02 + vec2f(0.01 * sin(a * 3.0), 0.0);
}

fn fractal(p: vec2<f32>) -> vec3<f32> {
    let julia = i32(params.shape + 0.5) == 1;
    let kick = env(audio.kick_age, 0.2);
    // Where the view is: dive into a point (bulbs) or stay round 0 (flowers).
    let dive = 0.5 - 0.5 * cos(params.dive);
    let level = select(params.depth, params.depth * 0.2, julia) * dive;
    let scale = 1.35 / exp2(level) / (1.0 + 0.06 * kick * params.edge_flash);
    let centre = select(dive_target(i32(params.dive_point + 0.5)), vec2f(0.0), julia);
    let w = centre + rot2(params.spin) * p * scale;

    var z: vec2<f32>;
    var c: vec2<f32>;
    if julia {
        z = w;
        // Bubbling widens the path the flower's seed travels.
        c = julia_c(params.wander * 0.25) * (1.0 + 0.04 * params.bubbling * sin(params.wander * 1.7));
    } else {
        // Starting a little away from zero makes the bulbs swell and wobble
        // like bubbles, while keeping the familiar shape.
        let a = params.wander * TAU * 0.5;
        z = vec2f(cos(a), sin(a * 1.31)) * params.bubbling * 0.3;
        c = w;
    }
    let steps = i32(clamp(params.detail, 8.0, 400.0));
    var n = 0;
    var trap = 1e9;
    for (var i = 0; i < steps; i++) {
        z = vec2f(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + c;
        trap = min(trap, abs(length(z) - 0.5));
        if dot(z, z) > 256.0 {
            break;
        }
        n++;
    }

    let hue = params.hue_shift + audio.centroid_hue_slow * 0.35;
    if n >= steps {
        // Inside: colour slices fanning round, like the cut of a fruit.
        let a = atan2(z.y, z.x) / TAU + 0.5;
        let slice = fract(a * max(floor(params.slices), 1.0) + params.wander * 0.1);
        let shade = 0.45 + 0.55 * smoothstep(0.0, 1.0, slice);
        let ring = 0.6 + 0.4 * smoothstep(0.0, 0.25, trap);
        return pal(a + hue) * shade * ring * (0.8 + 0.25 * clamp(audio.bass_slow, 0.0, 2.0));
    }

    // Outside: smooth escape count, bright near the edge, dark far away.
    let sn = f32(n) + 1.0 - log2(max(log2(dot(z, z)) * 0.5, 1e-6));
    let t = clamp(sn / f32(steps), 0.0, 1.0);
    let near = pow(t, mix(0.35, 0.9, params.darkness));
    var col = pal(sn * 0.025 + hue) * near;
    // The spiky edge: thin, bright, flashing on the kick.
    let edge = smoothstep(0.55, 1.0, t);
    col += pal(hue + 0.5) * edge * (0.35 + params.edge_flash * kick);
    // Hi-hat sparkle along the filaments.
    let glint = step(0.985, hash11(floor(sn * 3.0) + floor(audio.time * 12.0)));
    col += vec3f(1.0) * glint * edge * params.sparkle * env(audio.hat_age, 0.1);
    // Orbit-trap rings give the bulbs their round, bubbly rims.
    col += pal(hue + 0.25) * exp(-trap * 30.0) * 0.25 * (1.0 - params.darkness * 0.5);
    return col;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var col = vec3f(0.0);
    if i32(params.smooth_edges + 0.5) == 1 {
        // Four samples inside the pixel: no shimmering on the spiky edges.
        let px = 1.0 / max(audio.res_y, 1.0);
        col += fractal(in.xy + vec2f(-0.25, -0.25) * px * 2.0);
        col += fractal(in.xy + vec2f(0.25, -0.25) * px * 2.0);
        col += fractal(in.xy + vec2f(-0.25, 0.25) * px * 2.0);
        col += fractal(in.xy + vec2f(0.25, 0.25) * px * 2.0);
        col *= 0.25;
    } else {
        col = fractal(in.xy);
    }
    return vec4f(col, 1.0);
}
