/*{
  "name": "Shape Mask",
  "kind": "scene",
  "author": "Visual Engine",
  "description": "A white shape on black: circle, rectangle, polygon, star or ring, with soft edges. Plug it into any node's Mask plug to show that node only inside the shape. It can pulse on the kick or the beat and spin with the music.",
  "inputs": [
    { "name": "shape",      "label": "Shape",            "type": "choice", "options": ["Circle", "Rectangle", "Polygon", "Star", "Ring"], "default": 0 },
    { "name": "size",       "label": "Size",             "type": "float", "min": 0,    "max": 2,   "default": 0.5 },
    { "name": "stretch",    "label": "Width / height",   "type": "float", "min": 0.2,  "max": 5,   "default": 1.0 },
    { "name": "centre_x",   "label": "Centre sideways",  "type": "float", "min": -1,   "max": 1,   "default": 0.0 },
    { "name": "centre_y",   "label": "Centre up/down",   "type": "float", "min": -1,   "max": 1,   "default": 0.0 },
    { "name": "angle",      "label": "Angle",            "type": "float", "min": 0,    "max": 1,   "default": 0.0 },
    { "name": "spin_speed", "label": "Spin speed",       "type": "float", "min": -1,   "max": 1,   "default": 0.0 },
    { "name": "sides",      "label": "Sides / points",   "type": "float", "min": 3,    "max": 12,  "default": 5 },
    { "name": "sharpness",  "label": "Star sharpness",   "type": "float", "min": 0,    "max": 1,   "default": 0.6 },
    { "name": "roundness",  "label": "Corner roundness", "type": "float", "min": 0,    "max": 1,   "default": 0.0 },
    { "name": "thickness",  "label": "Ring thickness",   "type": "float", "min": 0.01, "max": 1,   "default": 0.15 },
    { "name": "feather",    "label": "Soft edge",        "type": "float", "min": 0,    "max": 0.5, "default": 0.02 },
    { "name": "kick_pulse", "label": "Pulse on kick",    "type": "float", "min": 0,    "max": 1,   "default": 0.0 },
    { "name": "beat_pulse", "label": "Pulse on beat",    "type": "float", "min": 0,    "max": 1,   "default": 0.0 },
    { "name": "invert",     "label": "Invert (make a hole)", "type": "bool", "default": false }
  ],
  "clocks": [ { "name": "spin", "speed": "spin_speed", "signal": "loudness_slow", "amount": 1.0 } ]
}*/

fn sd_box(p: vec2f, b: vec2f, r: f32) -> f32 {
    let q = abs(p) - b + r;
    return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Star with n points; m in [2, n] (2 = sharpest, n = a polygon).
fn sd_star(p0: vec2f, r: f32, n: f32, m: f32) -> f32 {
    let an = PI / n;
    let en = PI / m;
    let acs = vec2f(cos(an), sin(an));
    let ecs = vec2f(cos(en), sin(en));
    let a = atan2(p0.x, p0.y);
    let bn = a - 2.0 * an * floor(a / (2.0 * an)) - an;
    var p = length(p0) * vec2f(cos(bn), abs(sin(bn)));
    p -= r * acs;
    p += ecs * clamp(-dot(p, ecs), 0.0, r * acs.y / ecs.y);
    return length(p) * sign(p.x);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let grow = 1.0 + params.kick_pulse * env(audio.kick_age, 0.15)
        + params.beat_pulse * pulse(audio.beat_phase, 4.0) * 0.5;
    let r = params.size * grow;
    var p = in.xy - vec2f(params.centre_x * audio.aspect, params.centre_y);
    p = rot2(params.angle * TAU + params.spin) * p;
    p.x /= params.stretch;
    let n = floor(params.sides + 0.5);
    let shape = i32(params.shape + 0.5);
    var d: f32;
    if shape == 1 {
        d = sd_box(p, vec2f(r, r), params.roundness * r);
    } else if shape == 2 {
        d = sd_star(p, r, n, n);
    } else if shape == 3 {
        d = sd_star(p, r, n, mix(n, 2.0, params.sharpness));
    } else if shape == 4 {
        d = abs(length(p) - r) - params.thickness * r * 0.5;
    } else {
        d = length(p) - r;
    }
    // Distances were squeezed by the stretch; keep the edge even.
    d *= min(params.stretch, 1.0);
    let f = max(params.feather, 0.004);
    var m = 1.0 - smoothstep(-f, f, d);
    if params.invert > 0.5 {
        m = 1.0 - m;
    }
    return vec4f(vec3f(m), 1.0);
}
