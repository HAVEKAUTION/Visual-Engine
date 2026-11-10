/*{
  "name": "Luma Key",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Cuts out the bright (or dark) parts of a picture. Put it on a layer to drop a black background, or set Output to Black & white to use the result as a mask for another node.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "keep",      "label": "Keep",      "type": "choice", "options": ["Bright parts", "Dark parts"], "default": 0 },
    { "name": "threshold", "label": "Threshold", "type": "float", "min": 0, "max": 1, "default": 0.25 },
    { "name": "softness",  "label": "Softness",  "type": "float", "min": 0, "max": 1, "default": 0.15 },
    { "name": "output",    "label": "Output",    "type": "choice", "options": ["Cut-out picture", "Black & white (for masks)"], "default": 0 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let s = img_input(in.uv);
    let rgb = s.rgb / max(s.a, 1e-4);
    let s2 = params.softness * 0.5;
    var a = smoothstep(params.threshold - s2 - 1e-4, params.threshold + s2 + 1e-4, luma(rgb));
    if params.keep > 0.5 {
        a = 1.0 - a;
    }
    a *= s.a;
    if params.output > 0.5 {
        return vec4f(vec3f(a), 1.0);
    }
    return vec4f(rgb * a, a);
}
