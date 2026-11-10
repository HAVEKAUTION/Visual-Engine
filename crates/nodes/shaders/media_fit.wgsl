/*{
  "name": "Media",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Places a picture or video frame in the output: fit, fill or stretch, optionally mirrored. Used inside the Image and Video nodes.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "fit",    "label": "Fit",    "type": "choice", "options": ["Fit (show all)", "Fill (crop)", "Stretch"], "default": 1 },
    { "name": "mirror", "label": "Mirror", "type": "bool", "default": false },
    { "name": "scale",  "label": "Scale",  "type": "float", "min": 0.1, "max": 4, "default": 1.0 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2f(textureDimensions(image_in_a));
    let src = dims.x / max(dims.y, 1.0);
    let out = audio.aspect;
    // Half-size of the picture in output units (x: -aspect..aspect, y: -1..1).
    var half = vec2f(out, 1.0);
    let mode = i32(params.fit + 0.5);
    if mode == 0 {
        half = select(vec2f(src, 1.0), vec2f(out, out / src), src > out);
    } else if mode == 1 {
        half = select(vec2f(out, out / src), vec2f(src, 1.0), src > out);
    }
    half = half * params.scale;
    var p = in.xy;
    if params.mirror > 0.5 {
        p.x = -p.x;
    }
    let uv = vec2f(p.x / (2.0 * half.x) + 0.5, 0.5 - p.y / (2.0 * half.y));
    let inside = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
    let c = textureSampleLevel(image_in_a, smp, clamp(uv, vec2f(0.0), vec2f(1.0)), 0.0);
    return vec4f(c.rgb * c.a * inside, c.a * inside);
}
