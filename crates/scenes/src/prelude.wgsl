
// ---------------------------------------------------------------------------
// Visual Engine prelude: appended to every scene automatically (after your
// code, so error line numbers match your file).
// ---------------------------------------------------------------------------

@group(0) @binding(0) var<uniform> audio: Signals;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var smp: sampler;
@group(0) @binding(3) var prev_frame: texture_2d<f32>;
@group(0) @binding(4) var buf_a: texture_2d<f32>;
@group(0) @binding(5) var buf_b: texture_2d<f32>;
// Picture inputs of an effect (black for scenes / when nothing is plugged in).
@group(0) @binding(6) var image_in_a: texture_2d<f32>;
@group(0) @binding(7) var image_in_b: texture_2d<f32>;

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318531;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    // 0..1, (0,0) = top-left. Use this to sample textures.
    @location(0) uv: vec2<f32>,
    // Centred, y up, aspect-correct: x in -aspect..aspect, y in -1..1.
    @location(1) xy: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32) -> VsOut {
    let c = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    let pos = c * 2.0 - 1.0;
    var o: VsOut;
    o.pos = vec4<f32>(pos, 0.0, 1.0);
    o.uv = vec2<f32>(c.x, 1.0 - c.y);
    o.xy = pos * vec2<f32>(audio.aspect, 1.0);
    return o;
}

// Mel band 0..31 (fast-smoothed, 1.0 = typical), clamped index.
fn mel_band(i: i32) -> f32 {
    let j = clamp(i, 0, 31);
    return audio.mel[j >> 2u][j & 3];
}

// Mel spectrum at x in 0..1, interpolated.
fn mel_at(x: f32) -> f32 {
    let f = clamp(x, 0.0, 1.0) * 31.0;
    let i = i32(floor(f));
    return mix(mel_band(i), mel_band(i + 1), fract(f));
}

// Palette colour 0..4 (the scene's colours, or the palette picked in the UI).
fn pal_color(i: i32) -> vec3<f32> {
    return audio.palette[clamp(i, 0, 4)].rgb;
}

// Smooth gradient through the five palette colours; t wraps around (0..1).
fn pal(t: f32) -> vec3<f32> {
    let x = fract(t) * 5.0;
    let i = i32(floor(x));
    let f = smoothstep(0.0, 1.0, fract(x));
    return mix(pal_color(i), pal_color((i + 1) % 5), f);
}

// Decaying envelope: 1 at the hit, falling with time constant `decay` (s).
fn env(age: f32, decay: f32) -> f32 {
    return exp(-max(age, 0.0) / decay);
}

// Phase pulse: 1 at phase 0, falling to 0 at phase 1 with sharpness k.
fn pulse(phase: f32, k: f32) -> f32 {
    return pow(1.0 - fract(phase), k);
}

fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let k = vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0);
    let p = abs(fract(vec3<f32>(c.x) + k) * 6.0 - 3.0);
    return c.z * mix(vec3<f32>(1.0), clamp(p - 1.0, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}

fn rot2(a: f32) -> mat2x2<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat2x2<f32>(c, s, -s, c);
}

fn hash11(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Smooth value noise, roughly 0..1.
fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Fractal noise, 5 octaves, roughly 0..1.
fn fbm2(p: vec2<f32>) -> f32 {
    var q = p;
    var sum = 0.0;
    var amp = 0.5;
    for (var o = 0; o < 5; o++) {
        sum += amp * noise2(q);
        q = rot2(0.5) * q * 2.03 + vec2<f32>(1.7, 9.2);
        amp *= 0.5;
    }
    return sum / 0.97;
}

// Previous frame of this scene's output (feedback), sampled at uv.
fn prev(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(prev_frame, smp, uv, 0.0);
}

fn sample_a(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(buf_a, smp, uv, 0.0);
}

fn sample_b(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(buf_b, smp, uv, 0.0);
}

// An effect's first / second picture input at uv (see "images" in the header;
// `img_<name>(uv)` does the same by name).
fn image_a(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(image_in_a, smp, uv, 0.0);
}

fn image_b(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(image_in_b, smp, uv, 0.0);
}

// Rec. 709 brightness of a colour.
fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}
