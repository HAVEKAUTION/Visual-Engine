/*{
  "name": "Glitch",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "RGB split, block tearing and scan noise. The colour split jumps on the kick and the picture tears on the snare.",
  "images": [ { "name": "input", "label": "Picture" } ],
  "inputs": [
    { "name": "split",      "label": "RGB split",     "type": "float", "min": 0, "max": 1,   "default": 0.2 },
    { "name": "split_kick", "label": "Split on kick", "type": "float", "min": 0, "max": 2,   "default": 0.8 },
    { "name": "angle",      "label": "Split angle",   "type": "float", "min": 0, "max": 1,   "default": 0.0 },
    { "name": "tear",       "label": "Tear on snare", "type": "float", "min": 0, "max": 1.5, "default": 0.6 },
    { "name": "blocks",     "label": "Block rows",    "type": "float", "min": 4, "max": 60,  "default": 18 },
    { "name": "noise",      "label": "Scan noise",    "type": "float", "min": 0, "max": 1,   "default": 0.15 }
  ]
}*/

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var uv = in.uv;
    // Tear: some rows shift sideways on the snare.
    let snare = env(audio.snare_age, 0.12) * params.tear;
    let row = floor(uv.y * params.blocks);
    let seed = row + floor(audio.snare_count) * 13.7;
    let shift = (hash11(seed) - 0.5) * step(0.6, hash11(seed + 3.1)) * 0.25 * snare;
    uv.x = fract(uv.x + shift);
    // RGB split, bigger on the kick.
    let amt = (params.split + params.split_kick * env(audio.kick_age, 0.1)) * 0.02;
    let dir = vec2f(cos(params.angle * TAU), sin(params.angle * TAU)) * amt;
    var col = vec3f(img_input(uv + dir).r, img_input(uv).g, img_input(uv - dir).b);
    // Scan noise.
    let line = hash21(vec2f(floor(in.uv.y * audio.res_y * 0.5), floor(audio.time * 30.0)));
    col += (line - 0.5) * 0.15 * params.noise;
    col *= 1.0 - 0.15 * params.noise * (0.5 + 0.5 * sin(in.uv.y * audio.res_y * 1.5));
    return vec4f(max(col, vec3f(0.0)), 1.0);
}
