/*{
  "name": "Merge",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Puts one picture (foreground) over another (background) with a blend mode, the same maths as Photoshop. Screen and Add suit glowing visuals on black; Hard Light, Soft Light and Overlay mix textures in; Hue / Colour / Luminosity swap colour and brightness. The foreground's see-through parts let the background show; 'Black is see-through' does the same for its dark parts. Plug a mask in to merge only there.",
  "images": [
    { "name": "base", "label": "Background" },
    { "name": "top",  "label": "Foreground" }
  ],
  "inputs": [
    { "name": "mode",    "label": "Blend mode", "type": "choice",
      "options": ["Normal", "Add", "Screen", "Multiply", "Overlay", "Difference", "Lighten", "Darken",
                  "Subtract", "Exclusion", "Colour Dodge", "Colour Burn", "Hard Light", "Soft Light",
                  "Vivid Light", "Linear Light", "Pin Light", "Hue", "Saturation", "Colour", "Luminosity"],
      "default": "Screen" },
    { "name": "opacity", "label": "Opacity",              "type": "float", "min": 0, "max": 1, "default": 1.0 },
    { "name": "key",     "label": "Black is see-through", "type": "float", "min": 0, "max": 1, "default": 0.0 },
    { "name": "linear",  "label": "Blend in linear light", "type": "bool", "default": false }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let b = img_base(in.uv);
    var t = img_top(in.uv);
    // "Black is see-through": dark parts of the foreground fade out.
    let rgb = select(vec3f(0.0), t.rgb / max(t.a, 1e-6), t.a > 1e-6);
    t = t * mix(1.0, smoothstep(0.0, 0.3, luma(max(rgb, vec3f(0.0)))), params.key);
    return blend_over(b, t, i32(params.mode + 0.5), params.opacity, params.linear > 0.5);
}
