
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
@group(0) @binding(8) var image_in_c: texture_2d<f32>;
@group(0) @binding(9) var image_in_d: texture_2d<f32>;

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

// An effect's picture inputs (first to fourth) at uv (see "images" in the
// header; `img_<name>(uv)` does the same by name). Pictures are premultiplied
// RGBA in display (sRGB-encoded) values.
fn image_a(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(image_in_a, smp, uv, 0.0);
}

fn image_b(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(image_in_b, smp, uv, 0.0);
}

fn image_c(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(image_in_c, smp, uv, 0.0);
}

fn image_d(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(image_in_d, smp, uv, 0.0);
}

// Rec. 709 brightness of a colour.
fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// ---------------------------------------------------------------------------
// Blend modes (W3C Compositing / Photoshop maths; the Rust reference with
// tests is scenes/src/blend.rs). Modes, numbered: 0 Normal, 1 Add, 2 Screen,
// 3 Multiply, 4 Overlay, 5 Difference, 6 Lighten, 7 Darken, 8 Subtract,
// 9 Exclusion, 10 Colour Dodge, 11 Colour Burn, 12 Hard Light, 13 Soft Light,
// 14 Vivid Light, 15 Linear Light, 16 Pin Light, 17 Hue, 18 Saturation,
// 19 Colour, 20 Luminosity.
// ---------------------------------------------------------------------------

fn blend_screen1(b: f32, s: f32) -> f32 {
    return b + s - b * s;
}

fn blend_dodge1(b: f32, s: f32) -> f32 {
    if b <= 0.0 { return 0.0; }
    if s >= 1.0 { return 1.0; }
    return min(b / (1.0 - s), 1.0);
}

fn blend_burn1(b: f32, s: f32) -> f32 {
    if b >= 1.0 { return 1.0; }
    if s <= 0.0 { return 0.0; }
    return 1.0 - min((1.0 - b) / s, 1.0);
}

fn blend_hard1(b: f32, s: f32) -> f32 {
    if s <= 0.5 { return b * 2.0 * s; }
    return blend_screen1(b, 2.0 * s - 1.0);
}

fn blend_soft1(b: f32, s: f32) -> f32 {
    if s <= 0.5 { return b - (1.0 - 2.0 * s) * b * (1.0 - b); }
    var d = sqrt(b);
    if b <= 0.25 { d = ((16.0 * b - 12.0) * b + 4.0) * b; }
    return b + (2.0 * s - 1.0) * (d - b);
}

// One channel of a separable mode (1..16).
fn blend_channel(mode: i32, b: f32, s: f32) -> f32 {
    switch mode {
        case 1: { return min(b + s, 1.0); }
        case 2: { return blend_screen1(b, s); }
        case 3: { return b * s; }
        case 4: { return blend_hard1(s, b); }
        case 5: { return abs(b - s); }
        case 6: { return max(b, s); }
        case 7: { return min(b, s); }
        case 8: { return max(b - s, 0.0); }
        case 9: { return b + s - 2.0 * b * s; }
        case 10: { return blend_dodge1(b, s); }
        case 11: { return blend_burn1(b, s); }
        case 12: { return blend_hard1(b, s); }
        case 13: { return blend_soft1(b, s); }
        case 14: {
            if s <= 0.5 { return blend_burn1(b, 2.0 * s); }
            return blend_dodge1(b, 2.0 * s - 1.0);
        }
        case 15: { return clamp(b + 2.0 * s - 1.0, 0.0, 1.0); }
        case 16: {
            if s <= 0.5 { return min(b, 2.0 * s); }
            return max(b, 2.0 * s - 1.0);
        }
        default: { return s; }
    }
}

fn blend_lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn blend_clip(c: vec3<f32>) -> vec3<f32> {
    let l = blend_lum(c);
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    var o = c;
    if n < 0.0 { o = l + (o - l) * l / (l - n); }
    if x > 1.0 { o = l + (o - l) * (1.0 - l) / (x - l); }
    return o;
}

fn blend_set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return blend_clip(c + (l - blend_lum(c)));
}

fn blend_sat(c: vec3<f32>) -> f32 {
    return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b);
}

fn blend_set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    if x > n { return (c - n) * s / (x - n); }
    return vec3<f32>(0.0);
}

// B(backdrop, source) for straight (not premultiplied) colours.
fn blend_rgb(mode: i32, b_in: vec3<f32>, s_in: vec3<f32>) -> vec3<f32> {
    if mode <= 0 || mode > 20 { return s_in; }
    let b = clamp(b_in, vec3<f32>(0.0), vec3<f32>(1.0));
    let s = clamp(s_in, vec3<f32>(0.0), vec3<f32>(1.0));
    switch mode {
        case 17: { return blend_set_lum(blend_set_sat(s, blend_sat(b)), blend_lum(b)); }
        case 18: { return blend_set_lum(blend_set_sat(b, blend_sat(s)), blend_lum(b)); }
        case 19: { return blend_set_lum(s, blend_lum(b)); }
        case 20: { return blend_set_lum(b, blend_lum(s)); }
        default: {
            return vec3<f32>(
                blend_channel(mode, b.r, s.r),
                blend_channel(mode, b.g, s.g),
                blend_channel(mode, b.b, s.b),
            );
        }
    }
}

fn blend_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn blend_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// Put `front` over `back` (premultiplied RGBA) with a blend mode:
// Cs' = (1 - ab)·Cs + ab·B(Cb, Cs), then source-over with opacity × as.
// `linear` blends in linear light instead of display values.
fn blend_over(back: vec4<f32>, front: vec4<f32>, mode: i32, opacity: f32, linear: bool) -> vec4<f32> {
    let ab = clamp(back.a, 0.0, 1.0);
    let as_ = clamp(front.a, 0.0, 1.0);
    var cb = select(vec3<f32>(0.0), back.rgb / max(ab, 1e-6), ab > 1e-6);
    var cs = select(vec3<f32>(0.0), front.rgb / max(as_, 1e-6), as_ > 1e-6);
    if linear {
        cb = blend_to_linear(cb);
        cs = blend_to_linear(cs);
    }
    let cs2 = (1.0 - ab) * cs + ab * blend_rgb(mode, cb, cs);
    let a = clamp(as_ * opacity, 0.0, 1.0);
    let ao = a + ab * (1.0 - a);
    var co = a * cs2 + (1.0 - a) * ab * cb;
    if linear && ao > 1e-6 {
        co = blend_to_srgb(co / ao) * ao;
    }
    return vec4<f32>(co, ao);
}
