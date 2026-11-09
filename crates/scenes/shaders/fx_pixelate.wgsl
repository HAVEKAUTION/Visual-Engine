/*{
  "name": "Pixelate",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Chunky pixels and fewer colour steps (retro / 8-bit look). Pixels can grow on the kick.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "size",      "label": "Pixel size",   "type": "float", "min": 1, "max": 64, "default": 8 },
    { "name": "kick_grow", "label": "Grow on kick", "type": "float", "min": 0, "max": 3,  "default": 0.0 },
    { "name": "levels",    "label": "Colour steps", "type": "float", "min": 2, "max": 32, "default": 32 },
    { "name": "gaps",      "label": "Grid lines",   "type": "float", "min": 0, "max": 1,  "default": 0.0 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = max(1.0, floor(params.size * (1.0 + params.kick_grow * env(audio.kick_age, 0.15))));
    let cell = vec2f(px) / vec2f(audio.res_x, audio.res_y);
    var c = img_input((floor(in.uv / cell) + 0.5) * cell).rgb;
    let n = floor(params.levels);
    if n < 31.5 {
        c = floor(clamp(c, vec3f(0.0), vec3f(1.0)) * n + 0.5) / n;
    }
    let f = fract(in.uv / cell);
    let edge = step(min(f.x, f.y), 1.0 / px);
    c *= 1.0 - params.gaps * edge * step(3.0, px);
    return vec4f(c, 1.0);
}
