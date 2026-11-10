/*{
  "name": "Datamosh",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "The broken-video look: the old picture refuses to leave and gets dragged around, in blocks, by the movement of the new one, so cuts melt into each other. It kicks in on snares, section changes and drops (or hold Mosh up), then clears block by block. Best right after a Switch that cuts between clips or scenes.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "mosh",       "label": "Mosh (always)",          "type": "float", "min": 0, "max": 1,   "default": 0.0 },
    { "name": "on_snare",   "label": "Mosh on snares",         "type": "float", "min": 0, "max": 1,   "default": 0.35 },
    { "name": "on_section", "label": "Mosh on section changes", "type": "float", "min": 0, "max": 1,  "default": 0.8 },
    { "name": "on_drop",    "label": "Mosh on the drop",       "type": "float", "min": 0, "max": 1,   "default": 1.0 },
    { "name": "blocks",     "label": "Block size",             "type": "float", "min": 6, "max": 64,  "default": 22 },
    { "name": "flow",       "label": "Drag strength",          "type": "float", "min": 0, "max": 4,   "default": 1.6 },
    { "name": "melt",       "label": "Melt downwards",         "type": "float", "min": 0, "max": 1,   "default": 0.3 },
    { "name": "heal",       "label": "Heal speed",             "type": "float", "min": 0, "max": 0.3, "default": 0.03 }
  ],
  "buffers": [ { "name": "A", "persistent": true }, { "name": "B", "persistent": true } ],
  "passes": [
    { "entry": "fs_mosh", "target": "A" },
    { "entry": "fs_keep", "target": "B" },
    { "entry": "fs_main" }
  ]
}*/

// A = the moshed picture (kept between frames). B = last frame's input, to
// see how the picture moved. Real datamosh drops the video's key frames so
// motion from the new shot drags the old shot's pixels: here the movement is
// estimated per block (brightness change ÷ slope) and A is pushed along it.

fn amount() -> f32 {
    return clamp(
        params.mosh
            + params.on_snare * env(audio.snare_age, 0.35)
            + params.on_section * env(audio.section_age, 1.5)
            + params.on_drop * env(audio.drop_age, 2.5),
        0.0, 1.0);
}

@fragment
fn fs_mosh(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let src = img_input(uv);
    let grid = vec2f(params.blocks * audio.aspect, params.blocks);
    let cell = floor(uv * grid);
    let centre = (cell + 0.5) / grid;
    // Movement of this block between last frame and now.
    let e = 0.5 / grid;
    let now = luma(img_input(centre).rgb);
    let before = luma(sample_b(centre).rgb);
    let gx = luma(img_input(centre + vec2f(e.x, 0.0)).rgb) - luma(img_input(centre - vec2f(e.x, 0.0)).rgb);
    let gy = luma(img_input(centre + vec2f(0.0, e.y)).rgb) - luma(img_input(centre - vec2f(0.0, e.y)).rgb);
    let slope = vec2f(gx, gy) / (2.0 * e);
    var flow = -(now - before) * slope / (dot(slope, slope) + 4.0);
    let len = length(flow);
    flow = flow / max(len, 1e-5) * min(len, 0.06) * params.flow;
    flow += vec2f(0.0, -0.004) * params.melt;

    let m = amount();
    let old = sample_a(uv - flow * m);
    if old.a < 0.01 {
        return src; // nothing kept yet
    }
    // Blocks heal back to the real picture at random (a new key frame).
    let tick = floor(audio.time * 15.0);
    let heals = step(hash21(cell + vec2f(tick, tick * 0.37)), params.heal + (1.0 - m) * 0.25);
    return mix(src, old, m * (1.0 - heals));
}

@fragment
fn fs_keep(in: VsOut) -> @location(0) vec4<f32> {
    return img_input(in.uv);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return sample_a(in.uv);
}
