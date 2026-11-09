/*{
  "name": "Velvet Mirrorball",
  "author": "Visual Engine",
  "style": "warm / club",
  "genres": ["House", "Disco", "Funk"],
  "description": "A mirrorball turning above a warm, hazy room, throwing hundreds of light spots across the walls. The bass sets the spin, spots flash on every kick, the palette changes every two bars and the ball sparkles with the hi-hats.",
  "inputs": [
    { "name": "spin_speed", "label": "Spin speed",  "type": "float", "min": 0,     "max": 1,    "default": 0.12 },
    { "name": "spots",      "label": "Spot count",  "type": "float", "min": 10,    "max": 60,   "default": 28 },
    { "name": "spot_size",  "label": "Spot size",   "type": "float", "min": 0.05,  "max": 0.45, "default": 0.16 },
    { "name": "haze",       "label": "Haze",        "type": "float", "min": 0,     "max": 1,    "default": 0.5 },
    { "name": "kick_flash", "label": "Kick flash",  "type": "float", "min": 0,     "max": 1,    "default": 0.6 },
    { "name": "room",       "label": "Room colour", "type": "color", "default": [0.35, 0.08, 0.18, 1.0] }
  ],
  "clocks": [
    { "name": "spin", "speed": "spin_speed", "signal": "bass_slow", "amount": 0.8 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let xy = in.xy;
    let kick = env(audio.kick_age, 0.12) * params.kick_flash;
    let ball_c = vec2f(0.0, 0.62);
    let ball_r = 0.16;

    // --- Room: warm velvet gradient with haze glow around the ball ------
    var col = params.room.rgb * (0.25 + 0.35 * smoothstep(-1.0, 0.6, -xy.y));
    let glow = exp(-length(xy - ball_c) * 2.2) * params.haze;
    col += vec3f(1.0, 0.85, 0.7) * glow * (0.35 + 0.3 * kick);

    // --- Light spots: project each pixel onto a sphere around the ball and
    //     find the nearest mirror tile reflection on a turning grid. ------
    let dir = normalize(vec3f(xy.x, xy.y - ball_c.y, 1.1));
    let n = floor(params.spots);
    let lon = atan2(dir.x, dir.z) / TAU + params.spin * 0.1;
    let lat = asin(clamp(dir.y, -1.0, 1.0)) / PI + 0.5;
    let g = vec2f(lon * n * 2.0, lat * n);
    let cell = floor(g);
    let h = hash21(cell);
    let off = vec2f(hash21(cell + 1.7), hash21(cell + 4.3)) * 0.5 + 0.25;
    let d = length(fract(g) - off);
    let spot = smoothstep(params.spot_size, params.spot_size * 0.6, d) * step(0.35, h);
    // Palette changes every two bars.
    let pal = floor(audio.bar_count / 2.0);
    let hue = fract(hash11(pal) + 0.15 * hash21(cell + pal));
    let sc = mix(vec3f(1.0, 0.95, 0.85), hsv2rgb(vec3f(hue, 0.6, 1.0)), 0.6);
    let twinkle = 0.55 + 0.45 * sin(audio.time * (2.0 + 4.0 * h) + h * 40.0);
    col += sc * spot * twinkle * (0.35 + 0.9 * kick + 0.25 * clamp(audio.loudness_fast, 0.0, 2.0));

    // --- The ball: tiles, shading, sparkles on hats -----------------------
    let bp = (xy - ball_c) / ball_r;
    let br = length(bp);
    if br < 1.0 {
        let z = sqrt(1.0 - br * br);
        let bl = atan2(bp.x, z) / TAU + params.spin * 0.1;
        let bt = asin(bp.y) / PI;
        let tiles = vec2f(bl * 40.0, bt * 20.0);
        let tid = floor(tiles);
        let grout = min(fract(tiles).x, fract(tiles).y);
        let shade = 0.25 + 0.6 * z;
        var ball = vec3f(0.75) * shade * smoothstep(0.0, 0.08, grout);
        let spark = step(0.93, hash21(tid + floor(audio.time * 6.0))) * env(audio.hat_age, 0.08);
        ball += vec3f(1.0) * spark * 1.5;
        col = mix(col, ball, smoothstep(1.0, 0.97, br));
    }
    // Hanging wire.
    col += vec3f(0.2) * smoothstep(0.004, 0.0, abs(xy.x)) * step(ball_c.y + ball_r, xy.y);

    col *= 1.0 - 0.4 * pow(length(in.uv - 0.5) * 1.3, 2.0);
    return vec4f(col, 1.0);
}
