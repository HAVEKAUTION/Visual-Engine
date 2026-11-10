/*{
  "name": "Motion Mask",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "White where the picture moves, black where it's still: wave at the camera and only your hands get the effect. Trail makes the white linger. Plug it into a Mask plug, or set Output to Cut-out to keep only what moves.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "sensitivity", "label": "Sensitivity", "type": "float", "min": 0, "max": 1,    "default": 0.6 },
    { "name": "softness",    "label": "Softness",    "type": "float", "min": 0, "max": 1,    "default": 0.3 },
    { "name": "trail",       "label": "Trail",       "type": "float", "min": 0, "max": 0.98, "default": 0.85 },
    { "name": "spread",      "label": "Spread",      "type": "float", "min": 0, "max": 1,    "default": 0.3 },
    { "name": "output",      "label": "Output",      "type": "choice", "options": ["Black & white (for masks)", "Cut-out picture"], "default": 0 }
  ],
  "buffers": [ { "name": "A", "persistent": true } ],
  "passes": [ { "entry": "fs_store", "target": "A" }, { "entry": "fs_main" } ]
}*/

// Buffer A: r = this frame's brightness, g = last frame's.
@fragment
fn fs_store(in: VsOut) -> @location(0) vec4<f32> {
    let now = luma(img_input(in.uv).rgb);
    let before = sample_a(in.uv).r;
    return vec4f(now, before, 0.0, 1.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Average the change over a small area so noise doesn't sparkle.
    let r = params.spread * 0.01;
    var change = 0.0;
    for (var i = 0; i < 5; i++) {
        let ang = f32(i) * 1.2566;
        let o = select(vec2f(0.0), vec2f(cos(ang), sin(ang)) * r, i > 0);
        let b = sample_a(in.uv + o * vec2f(1.0 / audio.aspect, 1.0));
        change += abs(b.r - b.g);
    }
    change /= 5.0;
    let th = mix(0.25, 0.005, params.sensitivity);
    let soft = th * (0.2 + params.softness * 2.0);
    var m = smoothstep(th, th + soft, change);
    // Trails, the same at any frame rate.
    let keep = pow(params.trail, audio.delta * 60.0);
    // Last frame's mask: its brightness, or its alpha for the cut-out.
    let last = prev(in.uv);
    let cut = params.output > 0.5;
    m = max(m, select(last.g, last.a, cut) * keep);
    if cut {
        let s = img_input(in.uv);
        return vec4f(s.rgb * m, m);
    }
    return vec4f(vec3f(m), 1.0);
}
