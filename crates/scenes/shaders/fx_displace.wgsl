/*{
  "name": "Displace",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Bends one picture by the shapes in another: your photo flows along the lines of a scene, a video ripples through a kaleidoscope. Plug the picture to bend into Picture and anything moving into Shape (a scene works best). The bend swells on the kick, so the two move together with the music.",
  "images": [
    { "name": "input", "label": "Picture" },
    { "name": "shape", "label": "Shape (bends it)" }
  ],
  "inputs": [
    { "name": "amount",     "label": "Amount",               "type": "float",  "min": 0, "max": 0.3, "default": 0.04 },
    { "name": "kick_boost", "label": "More on the kick",     "type": "float",  "min": 0, "max": 3,   "default": 1.0 },
    { "name": "bass_boost", "label": "More with the bass",   "type": "float",  "min": 0, "max": 2,   "default": 0.5 },
    { "name": "style",      "label": "Style",                "type": "choice", "options": ["Liquid (follows the shape's slopes)", "Push out (bright parts push outwards)", "Sideways (bright parts slide)"], "default": 0 },
    { "name": "tint",       "label": "Take the shape's colour", "type": "float", "min": 0, "max": 1, "default": 0.0 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let push = params.amount
        * (1.0 + params.kick_boost * env(audio.kick_age, 0.25))
        * (1.0 + params.bass_boost * clamp(audio.bass_slow - 1.0, -0.5, 2.0));
    var offset = vec2f(0.0);
    switch i32(params.style + 0.5) {
        case 0: {
            // The slope of the shape's brightness: pictures pour downhill.
            let e = 2.0 / max(audio.res_y, 1.0) * 3.0;
            let dx = luma(img_shape(uv + vec2f(e, 0.0)).rgb) - luma(img_shape(uv - vec2f(e, 0.0)).rgb);
            let dy = luma(img_shape(uv + vec2f(0.0, e)).rgb) - luma(img_shape(uv - vec2f(0.0, e)).rgb);
            offset = -vec2f(dx, dy) * 6.0 * push;
        }
        case 1: {
            let l = luma(img_shape(uv).rgb);
            let c = (uv - 0.5) * vec2f(audio.aspect, 1.0);
            offset = c / max(length(c), 0.001) / vec2f(audio.aspect, 1.0) * (l - 0.3) * push;
        }
        default: {
            let l = luma(img_shape(uv).rgb);
            offset = vec2f(l - 0.3, 0.0) * push;
        }
    }
    let src = img_input(uv + offset);
    let tinted = src.rgb * mix(vec3f(1.0), img_shape(uv).rgb * 1.6 + 0.2, params.tint);
    return vec4f(tinted, src.a);
}
