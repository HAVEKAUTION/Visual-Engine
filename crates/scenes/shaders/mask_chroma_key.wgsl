/*{
  "name": "Chroma Key",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Green (or blue, or any colour) screen: makes one colour see-through and takes the coloured glow off the edges (spill). Put the camera through it and onto a layer above a scene.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "key",       "label": "Screen colour",       "type": "color", "default": [0.1, 0.75, 0.25, 1] },
    { "name": "tolerance", "label": "Tolerance",           "type": "float", "min": 0, "max": 1, "default": 0.3 },
    { "name": "softness",  "label": "Softness",            "type": "float", "min": 0, "max": 1, "default": 0.2 },
    { "name": "spill",     "label": "Remove colour spill", "type": "float", "min": 0, "max": 1, "default": 0.7 },
    { "name": "output",    "label": "Output",              "type": "choice", "options": ["Cut-out picture", "Black & white (for masks)"], "default": 0 }
  ]
}*/

// BT.709 chroma (Cb, Cr) of a display-referred colour.
fn chroma_of(c: vec3f) -> vec2f {
    let y = luma(c);
    return vec2f((c.b - y) / 1.8556, (c.r - y) / 1.5748);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let s = img_input(in.uv);
    var rgb = s.rgb / max(s.a, 1e-4);
    let k = chroma_of(params.key.rgb);
    let c = chroma_of(rgb);
    let tol = params.tolerance * 0.4;
    let soft = params.softness * 0.4 + 1e-4;
    let a = smoothstep(tol, tol + soft, distance(c, k)) * s.a;
    // Spill: take the screen colour's direction out of the chroma.
    let kl = length(k);
    if kl > 1e-4 {
        let dir = k / kl;
        let along = dot(c, dir);
        if along > 0.0 {
            let c2 = c - dir * along * params.spill;
            let y = luma(rgb);
            let r = y + 1.5748 * c2.y;
            let b = y + 1.8556 * c2.x;
            let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
            rgb = clamp(vec3f(r, g, b), vec3f(0.0), vec3f(1.0));
        }
    }
    if params.output > 0.5 {
        return vec4f(vec3f(a), 1.0);
    }
    return vec4f(rgb * a, a);
}
