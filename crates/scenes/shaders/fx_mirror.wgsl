/*{
  "name": "Mirror",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Kaleidoscope and mirror folds. Turns any picture into a symmetric pattern that spins with the bass.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "mode",       "label": "Mirror",          "type": "choice",
      "options": ["Kaleidoscope", "Left / right", "Top / bottom", "Four ways"], "default": 0 },
    { "name": "segments",   "label": "Segments",        "type": "float", "min": 2,    "max": 16,  "default": 6 },
    { "name": "spin_speed", "label": "Spin speed",      "type": "float", "min": -1,   "max": 1,   "default": 0.1 },
    { "name": "zoom",       "label": "Zoom",            "type": "float", "min": 0.3,  "max": 3,   "default": 1.0 },
    { "name": "centre_x",   "label": "Centre sideways", "type": "float", "min": -0.5, "max": 0.5, "default": 0.0 },
    { "name": "centre_y",   "label": "Centre up/down",  "type": "float", "min": -0.5, "max": 0.5, "default": 0.0 }
  ],
  "clocks": [ { "name": "spin", "speed": "spin_speed", "signal": "bass_slow", "amount": 1.0 } ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var p = in.xy / params.zoom;
    let mode = i32(params.mode + 0.5);
    if mode == 0 {
        let n = floor(params.segments);
        let seg = TAU / n;
        let q = rot2(params.spin) * p;
        var a = atan2(q.y, q.x);
        a = abs(a - seg * floor(a / seg) - seg * 0.5);
        p = vec2f(cos(a), sin(a)) * length(q);
    } else if mode == 1 {
        p.x = abs(p.x);
    } else if mode == 2 {
        p.y = abs(p.y);
    } else {
        p = abs(p);
    }
    p += vec2f(params.centre_x, params.centre_y);
    // Back to texture coordinates, mirror-repeating at the edges.
    var uv = vec2f(p.x / audio.aspect, -p.y) * 0.5 + 0.5;
    uv = 1.0 - abs(1.0 - 2.0 * fract(uv * 0.5));
    return vec4f(img_input(uv).rgb, 1.0);
}
