/*{
  "name": "Transform",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Zoom, rotate and move a picture. The kick can punch the zoom; spin speed follows the music. Edges can fade to black, repeat or mirror.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "zoom",       "label": "Zoom",          "type": "float", "min": 0.2, "max": 4,   "default": 1.0 },
    { "name": "kick_punch", "label": "Kick punch",    "type": "float", "min": 0,   "max": 0.5, "default": 0.0 },
    { "name": "angle",      "label": "Angle",         "type": "float", "min": 0,   "max": 1,   "default": 0.0 },
    { "name": "spin_speed", "label": "Spin speed",    "type": "float", "min": -1,  "max": 1,   "default": 0.0 },
    { "name": "move_x",     "label": "Move sideways", "type": "float", "min": -1,  "max": 1,   "default": 0.0 },
    { "name": "move_y",     "label": "Move up/down",  "type": "float", "min": -1,  "max": 1,   "default": 0.0 },
    { "name": "edges",      "label": "Edges",         "type": "choice", "options": ["Empty (see-through)", "Repeat", "Mirror"], "default": 2 }
  ],
  "clocks": [ { "name": "spin", "speed": "spin_speed", "signal": "loudness_slow", "amount": 1.0 } ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let z = params.zoom * (1.0 + params.kick_punch * env(audio.kick_age, 0.12));
    let p = rot2(-(params.angle * TAU + params.spin)) * in.xy / z - vec2f(params.move_x, params.move_y);
    var uv = vec2f(p.x / audio.aspect, -p.y) * 0.5 + 0.5;
    let mode = i32(params.edges + 0.5);
    var inside = 1.0;
    if mode == 1 {
        uv = fract(uv);
    } else if mode == 2 {
        uv = 1.0 - abs(1.0 - 2.0 * fract(uv * 0.5));
    } else {
        inside = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
    }
    // Keep see-through parts (cut-outs) see-through; outside is empty.
    return img_input(uv) * inside;
}
