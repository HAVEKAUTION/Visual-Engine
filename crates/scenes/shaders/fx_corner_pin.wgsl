/*{
  "name": "Corner Pin",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Pins the picture's four corners anywhere: onto a screen in a photo, a wall, a tilted card. Drag the corners in the viewer. Outside the four corners is see-through, so put it on a layer.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "tl_x", "label": "Top-left sideways",     "type": "float", "min": -1, "max": 2, "default": 0.0 },
    { "name": "tl_y", "label": "Top-left up/down",      "type": "float", "min": -1, "max": 2, "default": 0.0 },
    { "name": "tr_x", "label": "Top-right sideways",    "type": "float", "min": -1, "max": 2, "default": 1.0 },
    { "name": "tr_y", "label": "Top-right up/down",     "type": "float", "min": -1, "max": 2, "default": 0.0 },
    { "name": "br_x", "label": "Bottom-right sideways", "type": "float", "min": -1, "max": 2, "default": 1.0 },
    { "name": "br_y", "label": "Bottom-right up/down",  "type": "float", "min": -1, "max": 2, "default": 1.0 },
    { "name": "bl_x", "label": "Bottom-left sideways",  "type": "float", "min": -1, "max": 2, "default": 0.0 },
    { "name": "bl_y", "label": "Bottom-left up/down",   "type": "float", "min": -1, "max": 2, "default": 1.0 },
    { "name": "kick_punch", "label": "Kick punch",      "type": "float", "min": 0,  "max": 0.5, "default": 0.0 }
  ]
}*/

// Corners are in picture units: (0, 0) top-left, (1, 1) bottom-right.
// The square-to-quad projective map (Heckbert), inverted per pixel.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var q0 = vec2f(params.tl_x, params.tl_y);
    var q1 = vec2f(params.tr_x, params.tr_y);
    var q2 = vec2f(params.br_x, params.br_y);
    var q3 = vec2f(params.bl_x, params.bl_y);
    // The kick pushes the corners out from their middle.
    let mid = (q0 + q1 + q2 + q3) * 0.25;
    let k = 1.0 + params.kick_punch * env(audio.kick_age, 0.12);
    q0 = mid + (q0 - mid) * k;
    q1 = mid + (q1 - mid) * k;
    q2 = mid + (q2 - mid) * k;
    q3 = mid + (q3 - mid) * k;

    let d1 = q1 - q2;
    let d2 = q3 - q2;
    let d3 = q0 - q1 + q2 - q3;
    let den = d1.x * d2.y - d2.x * d1.y;
    var g = 0.0;
    var h = 0.0;
    if abs(den) > 1e-6 {
        g = (d3.x * d2.y - d2.x * d3.y) / den;
        h = (d1.x * d3.y - d3.x * d1.y) / den;
    }
    // Rows map (u, v, 1) to (x w, y w, w).
    let m = mat3x3<f32>(
        vec3f(q1.x - q0.x + g * q1.x, q1.y - q0.y + g * q1.y, g),
        vec3f(q3.x - q0.x + h * q3.x, q3.y - q0.y + h * q3.y, h),
        vec3f(q0.x, q0.y, 1.0),
    );
    // Inverse by the adjugate (the scale doesn't matter: we divide by w).
    let c0 = m[0];
    let c1 = m[1];
    let c2 = m[2];
    let inv = transpose(mat3x3<f32>(cross(c1, c2), cross(c2, c0), cross(c0, c1)));
    let p = inv * vec3f(in.uv, 1.0);
    // Points the quad never reaches (a twisted quad's far side) have w <= 0.
    let det = dot(c0, cross(c1, c2));
    let valid = step(1e-9, p.z * det);
    let uv = p.xy / select(p.z, 1e-6, abs(p.z) < 1e-6);
    let inside = valid * step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
    // Soften the edge by about a pixel.
    let e = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let w = fwidth(e) + 1e-5;
    let a = inside * clamp(e / w + 0.5, 0.0, 1.0);
    return img_input(clamp(uv, vec2f(0.0), vec2f(1.0))) * a;
}
