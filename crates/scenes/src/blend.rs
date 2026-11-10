//! Blend modes, exactly as the W3C Compositing spec (and Photoshop /
//! Procreate) define them. This is the reference the WGSL in the prelude
//! (`blend_rgb`, `blend_over`) follows; tests compare the two.
//!
//! Backdrop B (what's underneath), source S (what goes on top), channels in
//! 0..1, straight (not premultiplied) colour. In display (sRGB-encoded)
//! values by default, like Photoshop; `linear` blends in linear light.

/// Every blend mode, in the order shaders number them. The first eight
/// keep the order the Merge node always had, so saved graphs don't change.
pub const BLEND_MODES: &[&str] = &[
    "Normal",
    "Add",
    "Screen",
    "Multiply",
    "Overlay",
    "Difference",
    "Lighten",
    "Darken",
    "Subtract",
    "Exclusion",
    "Colour Dodge",
    "Colour Burn",
    "Hard Light",
    "Soft Light",
    "Vivid Light",
    "Linear Light",
    "Pin Light",
    "Hue",
    "Saturation",
    "Colour",
    "Luminosity",
];

/// Index of a blend mode by name.
pub fn mode_index(name: &str) -> Option<usize> {
    BLEND_MODES.iter().position(|m| *m == name)
}

fn screen(b: f32, s: f32) -> f32 {
    b + s - b * s
}

fn dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (b / (1.0 - s)).min(1.0)
    }
}

fn burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - b) / s).min(1.0)
    }
}

fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 {
            ((16.0 * b - 12.0) * b + 4.0) * b
        } else {
            b.sqrt()
        };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

/// One channel of a separable mode (modes 1..=16).
fn channel(mode: usize, b: f32, s: f32) -> f32 {
    match mode {
        1 => (b + s).min(1.0),
        2 => screen(b, s),
        3 => b * s,
        4 => hard_light(s, b), // Overlay = Hard Light with the layers swapped
        5 => (b - s).abs(),
        6 => b.max(s),
        7 => b.min(s),
        8 => (b - s).max(0.0),
        9 => b + s - 2.0 * b * s,
        10 => dodge(b, s),
        11 => burn(b, s),
        12 => hard_light(b, s),
        13 => soft_light(b, s),
        14 => {
            if s <= 0.5 {
                burn(b, 2.0 * s)
            } else {
                dodge(b, 2.0 * s - 1.0)
            }
        }
        15 => (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
        16 => {
            if s <= 0.5 {
                b.min(2.0 * s)
            } else {
                b.max(2.0 * s - 1.0)
            }
        }
        _ => s,
    }
}

type Rgb = [f32; 3];

pub fn lum(c: Rgb) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_colour(c: Rgb) -> Rgb {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 {
        out = out.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        out = out.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    out
}

fn set_lum(c: Rgb, l: f32) -> Rgb {
    let d = l - lum(c);
    clip_colour(c.map(|v| v + d))
}

fn sat(c: Rgb) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: Rgb, s: f32) -> Rgb {
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    if x > n {
        c.map(|v| (v - n) * s / (x - n))
    } else {
        [0.0; 3]
    }
}

/// B(Cb, Cs) for one pixel. Inputs are clamped to 0..1 (Normal passes the
/// source through untouched, so bright HDR values survive).
pub fn blend(mode: usize, b: Rgb, s: Rgb) -> Rgb {
    if mode == 0 || mode >= BLEND_MODES.len() {
        return s;
    }
    let b = b.map(|v| v.clamp(0.0, 1.0));
    let s = s.map(|v| v.clamp(0.0, 1.0));
    match mode {
        17 => set_lum(set_sat(s, sat(b)), lum(b)),
        18 => set_lum(set_sat(b, sat(s)), lum(b)),
        19 => set_lum(s, lum(b)),
        20 => set_lum(b, lum(s)),
        m => [0, 1, 2].map(|i| channel(m, b[i], s[i])),
    }
}

/// sRGB transfer function (encoded → linear light).
pub fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light → sRGB-encoded.
pub fn to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.max(0.0).powf(1.0 / 2.4) - 0.055
    }
}

/// Put `front` over `back` (both premultiplied RGBA): the W3C formula
/// `Cs' = (1 − αb)·Cs + αb·B(Cb, Cs)`, then source-over with
/// `opacity × αs`. Returns premultiplied RGBA.
pub fn blend_over(
    back: [f32; 4],
    front: [f32; 4],
    mode: usize,
    opacity: f32,
    linear: bool,
) -> [f32; 4] {
    let ab = back[3].clamp(0.0, 1.0);
    let a_s = front[3].clamp(0.0, 1.0);
    let un = |c: [f32; 4], a: f32| -> Rgb {
        if a > 1e-6 {
            [c[0] / a, c[1] / a, c[2] / a]
        } else {
            [0.0; 3]
        }
    };
    let mut cb = un(back, ab);
    let mut cs = un(front, a_s);
    if linear {
        cb = cb.map(to_linear);
        cs = cs.map(to_linear);
    }
    let mixed = blend(mode, cb, cs);
    let cs2: Rgb = [0, 1, 2].map(|i| (1.0 - ab) * cs[i] + ab * mixed[i]);
    let a = (a_s * opacity).clamp(0.0, 1.0);
    let ao = a + ab * (1.0 - a);
    let mut co: Rgb = [0, 1, 2].map(|i| a * cs2[i] + (1.0 - a) * ab * cb[i]);
    if linear && ao > 1e-6 {
        co = co.map(|v| to_srgb(v / ao) * ao);
    }
    [co[0], co[1], co[2], ao]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Rgb, b: Rgb) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    fn by(name: &str, b: Rgb, s: Rgb) -> Rgb {
        blend(mode_index(name).unwrap(), b, s)
    }

    #[test]
    fn separable_modes_match_known_values() {
        let b = [0.2, 0.5, 0.8];
        let s = [0.6, 0.3, 0.9];
        let cases: &[(&str, Rgb)] = &[
            ("Normal", [0.6, 0.3, 0.9]),
            ("Add", [0.8, 0.8, 1.0]),
            ("Screen", [0.68, 0.65, 0.98]),
            ("Multiply", [0.12, 0.15, 0.72]),
            // Overlay: B ≤ .5 → 2BS, else screen(S, 2B−1).
            ("Overlay", [0.24, 0.3, 0.96]),
            ("Difference", [0.4, 0.2, 0.1]),
            ("Lighten", [0.6, 0.5, 0.9]),
            ("Darken", [0.2, 0.3, 0.8]),
            ("Subtract", [0.0, 0.2, 0.0]),
            ("Exclusion", [0.56, 0.5, 0.26]),
            // Dodge: B / (1 − S), at most 1.
            ("Colour Dodge", [0.5, 0.5 / 0.7, 1.0]),
            // Burn: 1 − (1 − B) / S, at least 0.
            ("Colour Burn", [0.0, 0.0, 1.0 - 0.2 / 0.9]),
            // Hard Light: S ≤ .5 → 2BS, else screen(B, 2S − 1).
            (
                "Hard Light",
                [0.2 + 0.2 - 0.2 * 0.2, 0.3, 0.8 + 0.8 - 0.8 * 0.8],
            ),
            ("Linear Light", [0.4, 0.1, 1.0]),
            ("Pin Light", [0.2, 0.5, 0.8]),
        ];
        for (name, want) in cases {
            let got = by(name, b, s);
            assert!(close(got, *want), "{name}: got {got:?}, want {want:?}");
        }
    }

    #[test]
    fn soft_and_vivid_light_follow_their_formulas() {
        // Soft Light, S ≤ .5: B − (1 − 2S)·B·(1 − B).
        let got = by("Soft Light", [0.5; 3], [0.25; 3]);
        assert!(close(got, [0.5 - 0.5 * 0.5 * 0.5; 3]));
        // S > .5 with B ≤ .25: D(B) = ((16B − 12)B + 4)B.
        let b = 0.2f32;
        let d = ((16.0 * b - 12.0) * b + 4.0) * b;
        let got = by("Soft Light", [b; 3], [0.75; 3]);
        assert!(close(got, [b + 0.5 * (d - b); 3]));
        // S > .5 with B > .25: D(B) = √B.
        let got = by("Soft Light", [0.64; 3], [1.0; 3]);
        assert!(close(got, [0.8; 3]));
        // Vivid Light: burn(B, 2S) below half, dodge(B, 2S − 1) above.
        assert!(close(by("Vivid Light", [0.6; 3], [0.4; 3]), [0.5; 3]));
        assert!(close(by("Vivid Light", [0.3; 3], [0.75; 3]), [0.6; 3]));
        // Dodge / burn edge cases from the spec.
        assert!(close(by("Colour Dodge", [0.0; 3], [1.0; 3]), [0.0; 3]));
        assert!(close(by("Colour Burn", [1.0; 3], [0.0; 3]), [1.0; 3]));
    }

    #[test]
    fn non_separable_modes_swap_hue_saturation_and_brightness() {
        let red = [1.0, 0.0, 0.0];
        let grey = [0.5, 0.5, 0.5];
        // Luminosity of grey onto red keeps red's hue, takes grey's brightness.
        let l = by("Luminosity", red, grey);
        assert!((lum(l) - 0.5).abs() < 1e-4);
        assert!(l[0] > l[1] && (l[1] - l[2]).abs() < 1e-5);
        // Colour of red onto grey: grey's brightness, red's hue and saturation.
        let c = by("Colour", grey, red);
        assert!((lum(c) - 0.5).abs() < 1e-4 && c[0] > c[1]);
        // Saturation of grey (0) onto red: no colour left.
        let s = by("Saturation", red, grey);
        assert!((s[0] - s[1]).abs() < 1e-5 && (s[1] - s[2]).abs() < 1e-5);
        assert!((lum(s) - 0.3).abs() < 1e-4);
        // Hue of blue onto red: blue's hue, red's saturation and brightness.
        let h = by("Hue", red, [0.0, 0.0, 1.0]);
        assert!(h[2] > h[0] && h[2] > h[1]);
        assert!((lum(h) - 0.3).abs() < 1e-4);
    }

    #[test]
    fn compositing_respects_alpha_and_opacity() {
        let back = [0.2, 0.4, 0.6, 1.0];
        let front = [0.8, 0.8, 0.8, 1.0];
        // Normal at half opacity: halfway.
        let o = blend_over(back, front, 0, 0.5, false);
        assert!(close([o[0], o[1], o[2]], [0.5, 0.6, 0.7]) && o[3] == 1.0);
        // Over nothing: the source itself, whatever the mode.
        let o = blend_over([0.0; 4], front, mode_index("Multiply").unwrap(), 1.0, false);
        assert!(close([o[0], o[1], o[2]], [0.8; 3]) && o[3] == 1.0);
        // A see-through source changes nothing.
        let o = blend_over(back, [0.0; 4], 3, 1.0, false);
        assert_eq!(o, back);
        // Half-transparent source (premultiplied) over opaque: halfway.
        let o = blend_over(back, [0.4, 0.4, 0.4, 0.5], 0, 1.0, false);
        assert!(close([o[0], o[1], o[2]], [0.5, 0.6, 0.7]));
        // Linear light: same ends, a different middle.
        let lin = blend_over([0.0, 0.0, 0.0, 1.0], [1.0; 4], 0, 0.5, true);
        assert!((lin[0] - to_srgb(0.5)).abs() < 1e-4);
        assert!((to_srgb(to_linear(0.3)) - 0.3).abs() < 1e-5);
    }

    #[test]
    fn merge_header_lists_every_mode_in_order() {
        let src = include_str!("../shaders/fx_merge.wgsl");
        let meta = crate::meta::SceneMeta::parse(src, "fx_merge").unwrap();
        let mode = meta.inputs.iter().find(|i| i.name == "mode").unwrap();
        assert_eq!(mode.options, BLEND_MODES);
    }
}
