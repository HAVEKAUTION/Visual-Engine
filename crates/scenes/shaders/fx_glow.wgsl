/*{
  "name": "Glow",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Bright parts bleed light around them (bloom). The glow swells with the kick.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "amount",     "label": "Glow amount",     "type": "float", "min": 0,   "max": 3, "default": 1.0 },
    { "name": "radius",     "label": "Glow size",       "type": "float", "min": 0.2, "max": 4, "default": 1.5 },
    { "name": "threshold",  "label": "Only above",      "type": "float", "min": 0,   "max": 1, "default": 0.35 },
    { "name": "kick_boost", "label": "Kick boost",      "type": "float", "min": 0,   "max": 2, "default": 0.6 },
    { "name": "tint",       "label": "Tint to palette", "type": "float", "min": 0,   "max": 1, "default": 0.0 }
  ],
  "buffers": [ { "name": "A", "persistent": false } ],
  "passes": [ { "entry": "fs_bright_h", "target": "A" }, { "entry": "fs_main" } ]
}*/

const TAPS: i32 = 9;

fn weight(i: i32) -> f32 {
    let x = f32(i) / f32(TAPS);
    return exp(-x * x * 4.0);
}

// Pass 1: keep the bright parts and blur them sideways into buffer A.
@fragment
fn fs_bright_h(in: VsOut) -> @location(0) vec4<f32> {
    let stp = vec2f(params.radius * 2.0 / audio.res_x, 0.0);
    var sum = vec3f(0.0);
    var wsum = 0.0;
    for (var i = -TAPS; i <= TAPS; i++) {
        let c = img_input(in.uv + stp * f32(i)).rgb;
        let w = weight(i);
        sum += max(c - vec3f(params.threshold), vec3f(0.0)) * w;
        wsum += w;
    }
    return vec4f(sum / wsum, 1.0);
}

// Pass 2: blur A up and down, add it on top of the picture.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let stp = vec2f(0.0, params.radius * 2.0 / audio.res_y);
    var sum = vec3f(0.0);
    var wsum = 0.0;
    for (var i = -TAPS; i <= TAPS; i++) {
        let w = weight(i);
        sum += sample_a(in.uv + stp * f32(i)).rgb * w;
        wsum += w;
    }
    var glow = sum / wsum;
    glow = mix(glow, pal(luma(glow) * 0.8) * luma(glow) * 1.5, params.tint);
    let boost = 1.0 + params.kick_boost * env(audio.kick_age, 0.15);
    return vec4f(img_input(in.uv).rgb + glow * params.amount * 2.5 * boost, 1.0);
}
