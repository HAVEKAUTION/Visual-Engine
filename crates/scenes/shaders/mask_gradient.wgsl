/*{
  "name": "Gradient Mask",
  "kind": "scene",
  "author": "Visual Engine",
  "description": "A black-to-white ramp: straight, round, around a point or diamond, once or repeating in bands. Plug it into a Mask plug to fade a node in across the screen. It can scroll with the music.",
  "inputs": [
    { "name": "ramp",         "label": "Type",            "type": "choice", "options": ["Straight", "Round", "Around the centre", "Diamond"], "default": 0 },
    { "name": "angle",        "label": "Angle",           "type": "float", "min": 0,    "max": 1,  "default": 0.0 },
    { "name": "centre_x",     "label": "Centre sideways", "type": "float", "min": -1,   "max": 1,  "default": 0.0 },
    { "name": "centre_y",     "label": "Centre up/down",  "type": "float", "min": -1,   "max": 1,  "default": 0.0 },
    { "name": "length",       "label": "Length",          "type": "float", "min": 0.05, "max": 4,  "default": 1.0 },
    { "name": "offset",       "label": "Shift",           "type": "float", "min": -1,   "max": 1,  "default": 0.0 },
    { "name": "repeat",       "label": "Repeat",          "type": "choice", "options": ["Once", "Repeat", "Mirror"], "default": 0 },
    { "name": "bands",        "label": "Bands",           "type": "float", "min": 1,    "max": 32, "default": 1 },
    { "name": "scroll_speed", "label": "Scroll speed",    "type": "float", "min": -2,   "max": 2,  "default": 0.0 },
    { "name": "curve",        "label": "Curve",           "type": "float", "min": 0.2,  "max": 5,  "default": 1.0 },
    { "name": "invert",       "label": "Invert",          "type": "bool",  "default": false }
  ],
  "clocks": [ { "name": "scroll", "speed": "scroll_speed", "signal": "bass_slow", "amount": 1.0 } ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let p = in.xy - vec2f(params.centre_x * audio.aspect, params.centre_y);
    let q = rot2(params.angle * TAU) * p;
    let ty = i32(params.ramp + 0.5);
    var t: f32;
    if ty == 1 {
        t = length(q) / params.length;
    } else if ty == 2 {
        t = atan2(q.y, q.x) / TAU + 0.5;
    } else if ty == 3 {
        t = (abs(q.x) + abs(q.y)) / params.length;
    } else {
        t = q.x / (params.length * 2.0) + 0.5;
    }
    t = t * floor(params.bands + 0.5) - params.offset - params.scroll * 0.25;
    let rep = i32(params.repeat + 0.5);
    if rep == 1 {
        t = fract(t);
    } else if rep == 2 {
        t = 1.0 - abs(1.0 - 2.0 * fract(t * 0.5));
    } else {
        t = clamp(t, 0.0, 1.0);
    }
    var m = pow(t, params.curve);
    if params.invert > 0.5 {
        m = 1.0 - m;
    }
    return vec4f(vec3f(m), 1.0);
}
