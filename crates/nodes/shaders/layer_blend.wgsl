/*{
  "name": "Layer",
  "kind": "effect",
  "description": "Built in: one layer of a Layer Mixer, blended over the layers below it.",
  "images": [
    { "name": "back" },
    { "name": "front" }
  ],
  "inputs": [
    { "name": "mode", "type": "choice",
      "options": ["Normal", "Add", "Screen", "Multiply", "Overlay", "Difference", "Lighten", "Darken",
                  "Subtract", "Exclusion", "Colour Dodge", "Colour Burn", "Hard Light", "Soft Light",
                  "Vivid Light", "Linear Light", "Pin Light", "Hue", "Saturation", "Colour", "Luminosity"],
      "default": "Normal" },
    { "name": "opacity", "type": "float", "min": 0, "max": 1, "default": 1.0 },
    { "name": "linear", "type": "bool", "default": false },
    { "name": "has_back", "type": "bool", "default": false }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let back = select(vec4f(0.0), img_back(in.uv), params.has_back > 0.5);
    return blend_over(back, img_front(in.uv), i32(params.mode + 0.5), params.opacity, params.linear > 0.5);
}
