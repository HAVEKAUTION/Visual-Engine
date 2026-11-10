/*{
  "name": "Auto Cut-out",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Finds a picture's background by itself and removes it, so the subject (a person, a product, a logo) can sit on top of other things. It looks at the colour around the picture's edges and cuts that colour away. Works best on plain backgrounds; pictures that are already see-through (PNG) keep their own cut-out.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "tolerance", "label": "Tolerance",  "type": "float",  "min": 0, "max": 1, "default": 0.22 },
    { "name": "softness",  "label": "Softness",   "type": "float",  "min": 0, "max": 1, "default": 0.25 },
    { "name": "edge_glow", "label": "Edge glow",  "type": "float",  "min": 0, "max": 2, "default": 0.0 },
    { "name": "output",    "label": "Output",     "type": "choice", "options": ["Cut-out picture", "Black & white (for masks)"], "default": 0 }
  ],
  "buffers": [ { "name": "A", "persistent": false } ],
  "passes": [ { "entry": "fs_background", "target": "A" }, { "entry": "fs_main" } ]
}*/

// Pass 1 works out the background colour (the same in every pixel of A):
// the average of points around the border, then averaged again with only
// the points close to that, so a subject touching the edge doesn't spoil it.

fn border_point(i: i32) -> vec2<f32> {
    let k = f32(i % 12) / 12.0 + 0.04;
    let inset = 0.02;
    switch i / 12 {
        case 0: { return vec2f(k, inset); }
        case 1: { return vec2f(k, 1.0 - inset); }
        case 2: { return vec2f(inset, k); }
        default: { return vec2f(1.0 - inset, k); }
    }
}

@fragment
fn fs_background(in: VsOut) -> @location(0) vec4<f32> {
    var mean = vec3f(0.0);
    var seen = 0.0;
    for (var i = 0; i < 48; i++) {
        let c = img_input(border_point(i));
        if c.a > 0.5 {
            mean += c.rgb / c.a;
            seen += 1.0;
        }
    }
    if seen < 1.0 {
        // A see-through border: nothing to remove, the picture keeps its alpha.
        return vec4f(0.0, 0.0, 0.0, 0.0);
    }
    mean /= seen;
    var close = vec3f(0.0);
    var n = 0.0;
    for (var i = 0; i < 48; i++) {
        let c = img_input(border_point(i));
        if c.a > 0.5 && distance(c.rgb / c.a, mean) < 0.2 {
            close += c.rgb / c.a;
            n += 1.0;
        }
    }
    // Most of the border agrees: that's the background. Otherwise (a busy
    // photo) there is no plain background to remove.
    let plain = select(0.0, 1.0, n >= seen * 0.6);
    return vec4f(select(mean, close / max(n, 1.0), n > 0.0), plain);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let src = img_input(in.uv);
    let bg = sample_a(vec2f(0.5));
    var keep = 1.0;
    if bg.a > 0.5 && src.a > 0.0 {
        let d = distance(src.rgb / src.a, bg.rgb);
        let tol = params.tolerance * 0.6;
        keep = smoothstep(tol, tol + 0.02 + params.softness * 0.3, d);
    }
    let out = src * keep;
    // Optional glow along the cut edge (where keep is in between).
    let edge = keep * (1.0 - keep) * 4.0 * params.edge_glow;
    let lit = out.rgb + pal(0.3) * edge * src.a;
    if i32(params.output + 0.5) == 1 {
        let m = keep * src.a;
        return vec4f(m, m, m, 1.0);
    }
    return vec4f(lit, out.a);
}
