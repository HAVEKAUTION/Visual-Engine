/*{
  "name": "CRT Glitch",
  "author": "Visual Engine",
  "style": "retro / glitch",
  "genres": ["Synthwave", "Electro", "Glitch Hop"],
  "description": "A neon synthwave horizon (grid floor, striped sun, spectrum skyline) shown on a worn CRT. Snares tear the picture into displaced blocks with RGB split; drops flash and smash it.",
  "inputs": [
    { "name": "drive_speed",  "label": "Grid speed",          "type": "float", "min": 0,   "max": 3,   "default": 0.6 },
    { "name": "snare_glitch", "label": "Snare glitch",        "type": "float", "min": 0,   "max": 1.5, "default": 0.8 },
    { "name": "drop_glitch",  "label": "Drop glitch",         "type": "float", "min": 0,   "max": 1.5, "default": 1.0 },
    { "name": "rgb_split",    "label": "RGB split",           "type": "float", "min": 0,   "max": 2,   "default": 0.6 },
    { "name": "scanlines",    "label": "Scanline strength",   "type": "float", "min": 0,   "max": 1,   "default": 0.45 },
    { "name": "curve",        "label": "Screen curve",        "type": "float", "min": 0,   "max": 1,   "default": 0.35 },
    { "name": "persistence",  "label": "Phosphor glow",       "type": "float", "min": 0,   "max": 0.9, "default": 0.5 },
    { "name": "noise",        "label": "Noise",               "type": "float", "min": 0,   "max": 1,   "default": 0.25 }
  ],
  "clocks": [
    { "name": "drive", "speed": "drive_speed", "signal": "bass_slow", "amount": 1.0 },
    { "name": "sun_scroll", "speed": 0.08, "signal": "loudness_slow", "amount": 1.0 }
  ],
  "buffers": [ { "name": "A", "persistent": false } ],
  "passes": [
    { "entry": "fs_scene", "target": "A" },
    { "entry": "fs_main" }
  ]
}*/

// Pass 1 draws the clean picture into buffer A; pass 2 runs it through the
// CRT (curvature, glitches, RGB split, scanlines, phosphor glow).

const HORIZON: f32 = -0.05;

fn grid_line(v: f32, w: f32) -> f32 {
    let d = abs(fract(v) - 0.5);
    return smoothstep(0.5 - w, 0.5, d);
}

@fragment
fn fs_scene(in: VsOut) -> @location(0) vec4<f32> {
    let xy = in.xy;
    let kick = env(audio.kick_age, 0.15);
    let h = xy.y - HORIZON;

    // --- Sky -------------------------------------------------------------
    var sky = mix(vec3<f32>(0.95, 0.35, 0.45), vec3<f32>(0.06, 0.02, 0.16), smoothstep(0.0, 0.9, h));
    sky *= 0.55;

    // Striped sun, breathing with the bass.
    let sun_c = vec2<f32>(0.0, HORIZON + 0.42);
    let sun_r = 0.36 * (1.0 + 0.06 * kick + 0.04 * clamp(audio.bass_slow - 1.0, -1.0, 1.0));
    let ds = length(xy - sun_c) - sun_r;
    let sun_col = mix(vec3<f32>(1.0, 0.25, 0.55), vec3<f32>(1.0, 0.85, 0.3), smoothstep(-0.3, 0.35, xy.y - sun_c.y));
    // Horizontal cuts in the lower half that scroll downward.
    let sy = (sun_c.y - xy.y) / sun_r; // 0 at centre, 1 at bottom
    let cut = step(0.0, sy) * step(fract(sy * 7.0 - params.sun_scroll * 3.0), sy * 0.6);
    let sun = (1.0 - smoothstep(0.0, 0.006, ds)) * (1.0 - cut);
    sky = mix(sky, sun_col * (1.1 + 0.4 * kick), sun);
    sky += sun_col * 0.25 * exp(-max(ds, 0.0) * 6.0); // glow

    // Spectrum skyline: mirrored mountains standing on the horizon.
    let sx = abs(xy.x) / max(audio.aspect, 0.01);
    let mtn = 0.04 + 0.22 * (1.0 - exp(-0.6 * mel_at(sx * 0.8)));
    let in_mtn = 1.0 - smoothstep(0.0, 0.004, h - mtn);
    let edge = 1.0 - smoothstep(0.0, 0.006, abs(h - mtn));
    sky = mix(sky, vec3<f32>(0.03, 0.0, 0.08), in_mtn);
    sky += vec3<f32>(0.2, 0.9, 1.0) * edge * 0.8;

    // --- Grid floor (perspective) -------------------------------------
    let d = max(-h, 1e-3);
    let z = 0.35 / d;
    let gx = xy.x * z;
    let gz = z + params.drive * 2.0;
    let w = clamp(0.06 * z, 0.02, 0.45);
    let lines = max(grid_line(gx, w), grid_line(gz, w));
    let fade = exp(-0.08 * z);
    let neon = mix(vec3<f32>(1.0, 0.2, 0.8), vec3<f32>(0.2, 0.9, 1.0), 0.5 + 0.5 * sin(params.drive * 0.3));
    var floor_col = vec3<f32>(0.04, 0.0, 0.08) + neon * lines * fade * (0.7 + 0.8 * kick);
    floor_col += vec3<f32>(0.6, 0.2, 0.5) * exp(-d * 18.0) * 0.6; // horizon haze

    let col = select(floor_col, sky, h > 0.0);
    return vec4<f32>(col, 1.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let snare = env(audio.snare_age, 0.14) * params.snare_glitch;
    let drop = env(audio.drop_age, 0.9) * params.drop_glitch;
    let g = clamp(snare + 1.6 * drop, 0.0, 2.5);

    // --- Screen curvature ------------------------------------------------
    let cc = in.uv * 2.0 - 1.0;
    let bend = 0.12 * params.curve;
    var uv = cc * (1.0 + bend * dot(cc, cc)) * (1.0 - 0.5 * bend) * 0.5 + 0.5;
    let inside = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);

    // --- Block glitch: a new random pattern on every snare / drop --------
    let seed = audio.snare_count * 1.618 + audio.drop_count * 31.7;
    let blocks = vec2<f32>(12.0, 8.0) * (1.0 + floor(hash11(seed) * 3.0));
    let bid = floor(uv * blocks);
    if hash21(bid + seed) < 0.22 * g {
        uv.x += (hash21(bid + seed + 3.1) - 0.5) * 0.25 * g;
        uv.y += (hash21(bid + seed + 7.7) - 0.5) * 0.04 * g;
    }
    // Horizontal tearing of single rows.
    let row = floor(uv.y * 90.0);
    let tear = step(0.85, hash21(vec2<f32>(row, seed + 1.0)));
    uv.x += (hash21(vec2<f32>(row * 1.3, seed)) - 0.5) * 0.08 * g * tear;

    // --- RGB split ---------------------------------------------------------
    let split = params.rgb_split * (0.0015 + 0.015 * g + 0.002 * clamp(audio.bass_fast, 0.0, 3.0));
    var col = vec3<f32>(
        sample_a(uv + vec2<f32>(split, 0.0)).r,
        sample_a(uv).g,
        sample_a(uv - vec2<f32>(split, 0.0)).b,
    );
    // Colour-channel swap inside glitched blocks.
    if hash21(bid + seed + 11.0) < 0.12 * g {
        col = col.brg;
    }
    // Drop: a short negative flash.
    let flash = step(audio.drop_age, 0.12) * step(0.01, params.drop_glitch);
    col = mix(col, vec3<f32>(1.0) - col, flash);

    // --- Phosphor persistence (feedback) ---------------------------------
    let steps = audio.delta * 60.0;
    let old = prev(in.uv).rgb * pow(params.persistence, steps);
    col = max(col, old);

    // --- CRT surface ---------------------------------------------------------
    let res_y = max(audio.res_y, 1.0);
    let scan = 0.5 + 0.5 * sin(uv.y * res_y * 0.5 * TAU);
    col *= mix(1.0, 0.35 + 0.65 * scan, params.scanlines);
    // Aperture grille: every third pixel column favours R, G or B.
    let m = u32(in.pos.x) % 3u;
    let mask = select(select(vec3<f32>(0.85, 0.85, 1.1), vec3<f32>(0.85, 1.1, 0.85), m == 1u), vec3<f32>(1.1, 0.85, 0.85), m == 0u);
    col *= mix(vec3<f32>(1.0), mask, params.scanlines);
    // Noise (more during glitches) and vignette.
    let grain = hash21(in.pos.xy + fract(audio.time * 7.31) * 100.0) - 0.5;
    col += grain * params.noise * (0.08 + 0.25 * g);
    let vig = 1.0 - 0.6 * pow(length(cc) * 0.72, 3.0);
    col *= vig * inside;

    return vec4<f32>(max(col, vec3<f32>(0.0)), 1.0);
}
