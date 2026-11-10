/*{
  "name": "Afterimage",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Everything leaves a glowing ghost behind, like the after-image you see when you look away from a bright light: the trail takes the opposite colour, drifts and swirls slowly, lurches on every beat, and on a drop the slate wipes clean and starts again. Bright things on dark backgrounds work best.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "trail",        "label": "Trail length",             "type": "float", "min": 0,    "max": 0.99, "default": 0.92 },
    { "name": "complement",   "label": "Ghost in opposite colour", "type": "float", "min": 0,    "max": 1,    "default": 0.65 },
    { "name": "drift",        "label": "Drift outwards",           "type": "float", "min": -0.02, "max": 0.02, "default": 0.004 },
    { "name": "swirl",        "label": "Swirl",                    "type": "float", "min": -0.5, "max": 0.5,  "default": 0.06 },
    { "name": "beat_smear",   "label": "Lurch on the beat",        "type": "float", "min": 0,    "max": 1,    "default": 0.5 },
    { "name": "ghost_glow",   "label": "Ghost brightness",         "type": "float", "min": 0,    "max": 2,    "default": 1.0 },
    { "name": "kick_flare",   "label": "Ghosts flare on the kick", "type": "float", "min": 0,    "max": 2,    "default": 0.5 },
    { "name": "snap_on_drop", "label": "Wipe clean on the drop",   "type": "float", "min": 0,    "max": 1,    "default": 1.0 }
  ],
  "buffers": [ { "name": "A", "persistent": true } ],
  "passes": [ { "entry": "fs_ghost", "target": "A" }, { "entry": "fs_main" } ]
}*/

// Buffer A holds the ghost: every frame the old ghost is moved a little
// (drift, swirl, a lurch on the beat), faded by time (not frames, so it feels
// the same at any frame rate), and the picture is laid in on top.

@fragment
fn fs_ghost(in: VsOut) -> @location(0) vec4<f32> {
    let lurch = env(audio.beat_age, 0.15) * params.beat_smear;
    let frames = audio.delta * 60.0;
    let c = (in.uv - 0.5) * vec2f(audio.aspect, 1.0);
    let zoom = 1.0 + (params.drift + lurch * 0.02) * frames;
    let turn = (params.swirl * 0.02 + lurch * 0.01) * frames;
    let q = rot2(turn) * c / zoom;
    let back = q / vec2f(audio.aspect, 1.0) + 0.5;
    var ghost = sample_a(back) * pow(params.trail, frames);
    // The drop wipes the slate clean (for a moment after it lands).
    if audio.drop_age < 0.08 {
        ghost *= 1.0 - params.snap_on_drop;
    }
    let src = img_input(in.uv);
    return max(ghost, src);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let src = img_input(in.uv);
    let g = sample_a(in.uv).rgb;
    // How much of the ghost is trail rather than the thing itself: where the
    // picture is as bright as its ghost, there's no trail.
    let lg = luma(g);
    let trail = clamp((lg - luma(src.rgb)) / max(lg, 1e-3), 0.0, 1.0);
    // The same lightness with the hue turned half way round.
    let hi = max(g.r, max(g.g, g.b));
    let lo = min(g.r, min(g.g, g.b));
    let opposite = vec3f(hi + lo) - g;
    let flare = 1.0 + params.kick_flare * env(audio.kick_age, 0.2);
    let ghost = mix(g, opposite, params.complement) * params.ghost_glow * flare * trail;
    let rgb = src.rgb + ghost;
    return vec4f(rgb, max(src.a, min(max(rgb.r, max(rgb.g, rgb.b)), 1.0)));
}
