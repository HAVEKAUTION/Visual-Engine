/*{
  "name": "Merge",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Puts one picture (foreground) over another (background) with a blend mode. Screen and Add suit glowing visuals on black. 'Black is see-through' lets the background show through the dark parts of the foreground in any mode.",
  "images": [
    { "name": "base", "label": "Background" },
    { "name": "top",  "label": "Foreground" }
  ],
  "inputs": [
    { "name": "mode",    "label": "Blend mode", "type": "choice",
      "options": ["Normal", "Add", "Screen", "Multiply", "Overlay", "Difference", "Lighten", "Darken"],
      "default": "Screen" },
    { "name": "opacity", "label": "Opacity",              "type": "float", "min": 0, "max": 1, "default": 1.0 },
    { "name": "key",     "label": "Black is see-through", "type": "float", "min": 0, "max": 1, "default": 0.0 }
  ]
}*/

fn blend(mode: i32, b: vec3<f32>, t: vec3<f32>) -> vec3<f32> {
    let b1 = clamp(b, vec3f(0.0), vec3f(1.0));
    let t1 = clamp(t, vec3f(0.0), vec3f(1.0));
    switch mode {
        case 1: { return b + t; }
        case 2: { return 1.0 - (1.0 - b1) * (1.0 - t1); }
        case 3: { return b * t; }
        case 4: {
            let lo = 2.0 * b1 * t1;
            let hi = 1.0 - 2.0 * (1.0 - b1) * (1.0 - t1);
            return select(hi, lo, b1 < vec3f(0.5));
        }
        case 5: { return abs(b - t); }
        case 6: { return max(b, t); }
        case 7: { return min(b, t); }
        default: { return t; }
    }
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let b = img_base(in.uv).rgb;
    let t = img_top(in.uv).rgb;
    let see_through = mix(1.0, smoothstep(0.0, 0.3, luma(max(t, vec3f(0.0)))), params.key);
    let a = params.opacity * see_through;
    return vec4f(mix(b, blend(i32(params.mode + 0.5), b, t), a), 1.0);
}
