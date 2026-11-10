/*{
  "name": "Liquid",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Makes a picture move like it's under water: a slow wobble that churns with the bass, and on every kick a ring ripples out from the centre. Good on photos and logos so they feel alive rather than pasted on.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "wobble",       "label": "Wobble",               "type": "float", "min": 0,    "max": 0.06, "default": 0.012 },
    { "name": "wobble_size",  "label": "Wobble size",          "type": "float", "min": 0.5,  "max": 8,    "default": 2.5 },
    { "name": "churn_speed",  "label": "Churn speed",          "type": "float", "min": 0,    "max": 2,    "default": 0.35 },
    { "name": "ripple",       "label": "Kick ripple",          "type": "float", "min": 0,    "max": 0.08, "default": 0.025 },
    { "name": "ripple_speed", "label": "Ripple speed",         "type": "float", "min": 0.2,  "max": 3,    "default": 1.2 },
    { "name": "centre_x",     "label": "Ripple centre (x)",    "type": "float", "min": -0.5, "max": 0.5,  "default": 0.0 },
    { "name": "centre_y",     "label": "Ripple centre (y)",    "type": "float", "min": -0.5, "max": 0.5,  "default": 0.0 }
  ],
  "clocks": [ { "name": "churn", "speed": "churn_speed", "signal": "bass_slow", "amount": 1.0 } ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let p = (uv - 0.5) * vec2f(audio.aspect, 1.0);
    // Slow wobble: two noise fields drifting with the churn clock.
    let q = p * params.wobble_size;
    let w = vec2f(
        fbm2(q + vec2f(params.churn, 0.0)) - 0.5,
        fbm2(q + vec2f(17.3, params.churn * 0.8)) - 0.5,
    ) * params.wobble * 2.0;
    // Kick ripple: a ring travelling out from the centre, fading as it goes.
    let c = p - vec2f(params.centre_x * audio.aspect, -params.centre_y);
    let d = length(c);
    let front = audio.kick_age * params.ripple_speed;
    let ring = exp(-pow((d - front) * 9.0, 2.0)) * exp(-audio.kick_age * 2.5);
    let r = c / max(d, 0.001) * ring * params.ripple * sin((d - front) * 40.0);
    let offset = (w + r) / vec2f(audio.aspect, 1.0);
    return img_input(uv + offset);
}
