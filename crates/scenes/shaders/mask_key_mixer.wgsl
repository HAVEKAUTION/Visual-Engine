/*{
  "name": "Key Mixer",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Combines two masks into one: both together (add), A with B cut out (subtract), only where they overlap (intersect), or where just one of them is (difference). Masks are read as brightness: white = on.",
  "images": [
    { "name": "a", "label": "Mask A" },
    { "name": "b", "label": "Mask B" }
  ],
  "inputs": [
    { "name": "op",     "label": "Combine",  "type": "choice", "options": ["Add (both)", "Subtract (A minus B)", "Intersect (overlap)", "Difference (one or the other)"], "default": 0 },
    { "name": "amount", "label": "B amount", "type": "float", "min": 0, "max": 1, "default": 1.0 },
    { "name": "invert", "label": "Invert result", "type": "bool", "default": false }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let a = clamp(luma(img_a(in.uv).rgb), 0.0, 1.0);
    let b = clamp(luma(img_b(in.uv).rgb), 0.0, 1.0) * params.amount;
    let op = i32(params.op + 0.5);
    var m: f32;
    if op == 1 {
        m = max(a - b, 0.0);
    } else if op == 2 {
        m = min(a, b);
    } else if op == 3 {
        m = abs(a - b);
    } else {
        m = min(a + b, 1.0);
    }
    if params.invert > 0.5 {
        m = 1.0 - m;
    }
    return vec4f(vec3f(m), 1.0);
}
