/*{
  "name": "Trails",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Feedback: every frame keeps a fading copy of the last one, so moving things leave trails. Zoom and twist make the trails stream outwards or spiral.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "length",       "label": "Trail length",           "type": "float", "min": 0,     "max": 0.98, "default": 0.85 },
    { "name": "zoom",         "label": "Zoom",                   "type": "float", "min": -0.04, "max": 0.04, "default": 0.01 },
    { "name": "twist",        "label": "Twist",                  "type": "float", "min": -0.05, "max": 0.05, "default": 0.0 },
    { "name": "drift_x",      "label": "Drift sideways",         "type": "float", "min": -0.02, "max": 0.02, "default": 0.0 },
    { "name": "drift_y",      "label": "Drift up/down",          "type": "float", "min": -0.02, "max": 0.02, "default": 0.0 },
    { "name": "mode",         "label": "Mix",                    "type": "choice", "options": ["Brightest", "Add"], "default": 0 },
    { "name": "colour_shift", "label": "Trails through palette", "type": "float", "min": 0, "max": 1, "default": 0.0 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let now = img_input(in.uv).rgb;
    // Where last frame's pixel came from (zoom / twist around the centre),
    // scaled by the frame time so it looks the same at any frame rate.
    let k = audio.delta * 60.0;
    var p = (in.uv - 0.5) * vec2f(audio.aspect, 1.0);
    p = rot2(-params.twist * k) * p / (1.0 + params.zoom * k);
    let src = p / vec2f(audio.aspect, 1.0) + 0.5 - vec2f(params.drift_x, -params.drift_y) * k;
    var old = prev(src).rgb * pow(params.length, k);
    // Fade outside the frame instead of smearing the edge.
    let inside = step(0.0, src.x) * step(src.x, 1.0) * step(0.0, src.y) * step(src.y, 1.0);
    old *= inside;
    old = mix(old, pal(luma(old) + audio.time * 0.05) * luma(old), clamp(params.colour_shift * 0.1 * k, 0.0, 1.0));
    let out = select(max(now, old), now + old * 0.5, params.mode > 0.5);
    return vec4f(out, 1.0);
}
