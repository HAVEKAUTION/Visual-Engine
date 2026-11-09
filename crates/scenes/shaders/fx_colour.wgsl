/*{
  "name": "Colour Grade",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Brightness, contrast, saturation and hue. 'Palette map' recolours the whole picture through this node's palette (dark parts take the first colours, bright parts the last), so any scene can wear any palette.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "palette": ["#0d0221", "#541388", "#d90368", "#f4a261", "#fff3b0"],
  "inputs": [
    { "name": "brightness",  "label": "Brightness",  "type": "float", "min": 0, "max": 3, "default": 1.0 },
    { "name": "contrast",    "label": "Contrast",    "type": "float", "min": 0, "max": 3, "default": 1.0 },
    { "name": "saturation",  "label": "Saturation",  "type": "float", "min": 0, "max": 2, "default": 1.0 },
    { "name": "hue",         "label": "Hue shift",   "type": "float", "min": 0, "max": 1, "default": 0.0 },
    { "name": "palette_map", "label": "Palette map", "type": "float", "min": 0, "max": 1, "default": 0.0 },
    { "name": "invert",      "label": "Invert",      "type": "float", "min": 0, "max": 1, "default": 0.0 }
  ]
}*/

fn rgb2hsv(c: vec3<f32>) -> vec3<f32> {
    let k = vec4f(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4f(c.bg, k.wz), vec4f(c.gb, k.xy), step(c.b, c.g));
    let q = mix(vec4f(p.xyw, c.r), vec4f(c.r, p.yzx), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    return vec3f(abs(q.z + (q.w - q.y) / (6.0 * d + 1e-6)), d / (q.x + 1e-6), q.x);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var c = max(img_input(in.uv).rgb, vec3f(0.0));
    c = max((c - 0.5) * params.contrast + 0.5, vec3f(0.0)) * params.brightness;
    let l = luma(c);
    c = max(mix(vec3f(l), c, params.saturation), vec3f(0.0));
    if params.hue > 0.0 {
        let peak = max(1.0, max(c.r, max(c.g, c.b)));
        var hsv = rgb2hsv(c / peak);
        hsv.x = fract(hsv.x + params.hue);
        c = hsv2rgb(hsv) * peak;
    }
    // Palette map: brightness picks a colour along the palette (no wrap-around).
    c = mix(c, pal(clamp(l, 0.0, 1.0) * 0.79) * (0.3 + 0.9 * l), params.palette_map);
    c = mix(c, vec3f(1.0) - clamp(c, vec3f(0.0), vec3f(1.0)), params.invert);
    return vec4f(c, 1.0);
}
