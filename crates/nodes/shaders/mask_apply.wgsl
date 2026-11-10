/*{
  "name": "Mask",
  "kind": "effect",
  "description": "Built in: limits a node to where its mask is white. Where the mask is black the node's input shows unchanged (or nothing, for a source).",
  "images": [
    { "name": "orig" },
    { "name": "result" },
    { "name": "mask" }
  ],
  "inputs": [
    { "name": "has_orig", "type": "bool", "default": true },
    { "name": "invert", "type": "bool", "default": false }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var m = clamp(luma(img_mask(in.uv).rgb), 0.0, 1.0);
    if params.invert > 0.5 {
        m = 1.0 - m;
    }
    let orig = select(vec4f(0.0), img_orig(in.uv), params.has_orig > 0.5);
    return mix(orig, img_result(in.uv), m);
}
