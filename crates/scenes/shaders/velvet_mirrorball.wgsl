/*{
  "name": "Velvet Mirrorball",
  "author": "Visual Engine",
  "style": "warm / club",
  "genres": ["House", "Disco", "Funk"],
  "palette": ["#2a0618", "#ff3d8b", "#ffb347", "#8a5cff", "#3de0ff"],
  "description": "A disco room: velvet curtains, a lit dance floor and a mirrorball throwing light. The bass turns the ball and the spots sweep around the room with it; coloured beams swing through the haze; every kick lights a new pattern on the floor; hi-hats make the ball sparkle; the colours rotate every few bars and a drop lights up everything.",
  "inputs": [
    { "name": "spin_speed",   "label": "Ball spin",           "type": "float", "min": 0,    "max": 1,    "default": 0.25 },
    { "name": "spots",        "label": "Spot density",        "type": "float", "min": 6,    "max": 40,   "default": 18 },
    { "name": "spot_size",    "label": "Spot size",           "type": "float", "min": 0.1,  "max": 0.45, "default": 0.22 },
    { "name": "spot_bright",  "label": "Spot brightness",     "type": "float", "min": 0,    "max": 1.5,  "default": 0.8 },
    { "name": "beams",        "label": "Beams",               "type": "float", "min": 0,    "max": 8,    "default": 4 },
    { "name": "beam_speed",   "label": "Beam sweep speed",    "type": "float", "min": 0,    "max": 1.5,  "default": 0.4 },
    { "name": "haze",         "label": "Haze",                "type": "float", "min": 0,    "max": 1.5,  "default": 0.7 },
    { "name": "floor_bright", "label": "Dance floor",         "type": "float", "min": 0,    "max": 1.5,  "default": 0.9 },
    { "name": "floor_tiles",  "label": "Floor tiles",         "type": "float", "min": 2,    "max": 12,   "default": 6 },
    { "name": "curtain",      "label": "Curtain brightness",  "type": "float", "min": 0,    "max": 1.5,  "default": 0.8 },
    { "name": "sparkle",      "label": "Ball sparkle",        "type": "float", "min": 0,    "max": 1.5,  "default": 1.0 },
    { "name": "colour_turn",  "label": "Colour rotation",     "type": "float", "min": 0,    "max": 1,    "default": 1.0 }
  ],
  "clocks": [
    { "name": "spin",  "speed": "spin_speed", "signal": "bass_slow",     "amount": 0.8 },
    { "name": "sweep", "speed": "beam_speed", "signal": "mid_slow",      "amount": 1.0 },
    { "name": "sway",  "speed": 0.15,         "signal": "loudness_slow", "amount": 0.5 }
  ]
}*/

// Palette: 0 = velvet, 1-4 = the four light colours.

const BALL_C: vec2<f32> = vec2<f32>(0.0, 0.6);
const BALL_R: f32 = 0.15;
const HORIZON: f32 = -0.3;

// Light colour 0..3, rotating through the palette every 4 bars.
fn light(i: f32) -> vec3<f32> {
    let turn = floor(audio.bar_count / 4.0) * step(0.5, params.colour_turn);
    return pal_color(1 + i32((i + turn) % 4.0));
}

fn wrap_a(a: f32) -> f32 {
    return a - TAU * floor(a / TAU + 0.5);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let xy = in.xy;
    let kick = env(audio.kick_age, 0.14);
    let drop = env(audio.drop_age, 0.8);
    let loud = clamp(audio.loudness_fast, 0.0, 2.0);
    var col = vec3f(0.0);

    // --- Back wall: velvet curtain folds swaying gently -------------------
    if xy.y > HORIZON {
        let fold_x = xy.x * 11.0 + 0.6 * sin(xy.y * 1.7 + params.sway) * (1.0 - 0.5 * xy.y);
        let fold = 0.5 + 0.5 * sin(fold_x);
        let sheen = pow(fold, 6.0);
        col = pal_color(0) * (0.35 + 0.9 * fold * fold) + mix(pal_color(0), vec3f(1.0), 0.3) * sheen * 0.25;
        col *= params.curtain * 1.6 * (0.55 + 0.45 * smoothstep(HORIZON, 0.6, xy.y));
        // Darker near the floor.
        col *= 0.6 + 0.4 * smoothstep(HORIZON, HORIZON + 0.3, xy.y);
    } else {
        // --- Dance floor: perspective tiles lighting up with the kicks ----
        let depth = 0.35 / (HORIZON - xy.y);
        let fp = vec2f(xy.x * depth, depth) * params.floor_tiles * 0.5;
        let tile = floor(fp);
        let g = fract(fp);
        let grout = smoothstep(0.0, 0.06, min(min(g.x, 1.0 - g.x), min(g.y, 1.0 - g.y)));
        let beat = floor(audio.kick_count);
        let h = hash21(tile + beat * 1.37);
        let lit = step(0.55, h) + drop;
        let c = light(floor(hash21(tile + beat * 0.71 + 3.0) * 4.0));
        let fog = exp(-depth * 0.35);
        var floor_col = pal_color(0) * 0.15;
        floor_col += c * lit * (0.45 + 0.75 * kick + 0.2 * loud) * params.floor_bright;
        // Soft glow inside each tile.
        floor_col *= grout * (0.7 + 0.3 * (1.0 - length(g - 0.5)));
        col = floor_col * fog;
        // The curtain's reflection in the glossy floor.
        col += pal_color(0) * 0.12 * params.curtain * exp(-(HORIZON - xy.y) * 6.0);
    }

    // --- Spots thrown by the ball, sweeping around it ---------------------
    let bp = xy - BALL_C;
    let rr = length(bp);
    let a = atan2(bp.y, bp.x);
    let sp = 2.4 / params.spots;
    let k = floor(rr / sp);
    let n = floor(TAU * (k + 0.5)) + 1.0;
    let u = (a / TAU + params.spin * 0.08) * n;
    let cell = vec2f(k, floor(u) - n * floor(floor(u) / n));
    let hs = hash21(cell * vec2f(1.0, 1.3) + 7.0);
    let jit = vec2f(hash21(cell + 2.1), hash21(cell + 5.7)) - 0.5;
    let dv = vec2f(fract(u) - 0.5 + jit.x * 0.4, fract(rr / sp) - 0.5 + jit.y * 0.4);
    let spot = smoothstep(params.spot_size, params.spot_size * 0.55, length(dv)) * step(0.3, hs);
    // Most spots are warm white; some carry a colour.
    let tint = mix(vec3f(1.0, 0.95, 0.85), light(floor(hs * 13.0) % 4.0), step(0.65, hs));
    let twinkle = 0.6 + 0.4 * sin(audio.time * (2.0 + 5.0 * hs) + hs * 40.0);
    let spot_amt = spot * twinkle * smoothstep(BALL_R, BALL_R + 0.2, rr) * exp(-rr * 0.35)
        * (0.3 + 0.8 * kick + 0.2 * loud + drop) * params.spot_bright;
    col += tint * spot_amt;

    // --- Beams swinging through the haze ----------------------------------
    let nb = i32(floor(params.beams));
    let smoke = 0.55 + 0.45 * fbm2(xy * 1.8 + vec2f(params.sway * 0.7, audio.time * 0.05));
    for (var b = 0; b < 8; b++) {
        if b >= nb {
            break;
        }
        let fb = f32(b);
        let ang = -PI * 0.5 + sin(params.sweep * (0.6 + 0.17 * fb) + fb * 2.1) * 1.15;
        let da = abs(wrap_a(a - ang));
        let width = 0.035 + 0.02 * hash11(fb);
        let beam = exp(-pow(da / width, 2.0)) * exp(-rr * 0.55) * smoothstep(BALL_R, BALL_R + 0.1, rr);
        col += light(fb) * beam * smoke * params.haze * (0.35 + 0.9 * kick + drop) * 0.8;
    }
    // General haze glow around the ball.
    col += mix(light(0.0), vec3f(1.0), 0.6) * exp(-rr * 3.0) * params.haze * 0.25 * (1.0 + kick);

    // --- The mirrorball ---------------------------------------------------
    let q = bp / BALL_R;
    let qr = length(q);
    if qr < 1.0 {
        let z = sqrt(1.0 - qr * qr);
        let lon = atan2(q.x, z) + params.spin * 0.5;
        let lat = asin(clamp(q.y, -1.0, 1.0));
        let tiles = vec2f(lon * 16.0 / PI, lat * 16.0 / PI);
        let tid = floor(tiles);
        let tf = fract(tiles);
        // Each tile is a flat mirror: shade it with its centre's normal.
        let clon = (tid.x + 0.5) * PI / 16.0 - params.spin * 0.5;
        let clat = (tid.y + 0.5) * PI / 16.0;
        let nf = vec3f(sin(clon) * cos(clat), sin(clat), cos(clon) * cos(clat));
        let ldir = normalize(vec3f(-0.5, 0.7, 0.6));
        let refl = reflect(-ldir, nf);
        let spec = pow(max(refl.z, 0.0), 40.0);
        let diff = max(dot(nf, ldir), 0.0);
        let grout = smoothstep(0.0, 0.12, min(min(tf.x, 1.0 - tf.x), min(tf.y, 1.0 - tf.y)));
        let ht = hash21(tid);
        var ball = vec3f(0.18 + 0.4 * diff) * (0.7 + 0.3 * ht);
        ball += light(floor(ht * 4.0)) * 0.25 * step(0.7, ht);
        ball += vec3f(1.0) * spec * 1.6;
        // Hi-hat sparkles.
        let sparkle = step(0.9, hash21(tid + floor(audio.hat_count) * 3.3)) * env(audio.hat_age, 0.09);
        ball += vec3f(1.6) * sparkle * params.sparkle;
        ball *= grout * (0.6 + 0.4 * z);
        col = mix(col, ball, smoothstep(1.0, 0.96, qr));
    }
    // Hanging wire.
    col += vec3f(0.25) * smoothstep(0.004, 0.0, abs(xy.x)) * step(BALL_C.y + BALL_R, xy.y);

    col *= 1.0 - 0.4 * pow(length(in.uv - 0.5) * 1.3, 2.0);
    return vec4f(col, 1.0);
}
