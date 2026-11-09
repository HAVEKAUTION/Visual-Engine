//! Colour palettes: five colours every scene can use through `pal(t)` /
//! `pal_color(i)`. Each scene declares its own default palette in its header
//! (`"palette": ["#ff2a6d", ...]`); the user can swap in any built-in palette or
//! a custom one, live.

use serde::{Deserialize, Serialize};

/// Five colours, display-space RGB 0..1.
pub type Palette = [[f32; 3]; 5];

pub const PALETTE_SIZE: usize = 5;

const fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

/// The built-in palettes.
pub const BUILTIN: &[(&str, Palette)] = &[
    (
        "Neon",
        [
            rgb(0xff2a6d),
            rgb(0x05d9e8),
            rgb(0xd1f7ff),
            rgb(0x7b2cff),
            rgb(0x01ff89),
        ],
    ),
    (
        "Sunset",
        [
            rgb(0x2d1b4e),
            rgb(0xc4306b),
            rgb(0xff6b35),
            rgb(0xffb627),
            rgb(0xffe8a3),
        ],
    ),
    (
        "Ocean",
        [
            rgb(0x03045e),
            rgb(0x0077b6),
            rgb(0x00b4d8),
            rgb(0x90e0ef),
            rgb(0xe0fbfc),
        ],
    ),
    (
        "Forest",
        [
            rgb(0x0b2414),
            rgb(0x2d6a4f),
            rgb(0x74c69d),
            rgb(0xd8f3dc),
            rgb(0xe9c46a),
        ],
    ),
    (
        "Fire",
        [
            rgb(0x1a0000),
            rgb(0x8b0000),
            rgb(0xff4500),
            rgb(0xffa500),
            rgb(0xfff3b0),
        ],
    ),
    (
        "Ice",
        [
            rgb(0x0b132b),
            rgb(0x3a506b),
            rgb(0x5bc0be),
            rgb(0xc6f1ff),
            rgb(0xffffff),
        ],
    ),
    (
        "Mono",
        [
            rgb(0x0a0a0a),
            rgb(0x404040),
            rgb(0x8c8c8c),
            rgb(0xd0d0d0),
            rgb(0xffffff),
        ],
    ),
    (
        "Vapor",
        [
            rgb(0xff71ce),
            rgb(0x01cdfe),
            rgb(0x05ffa1),
            rgb(0xb967ff),
            rgb(0xfffb96),
        ],
    ),
    (
        "Acid",
        [
            rgb(0x0d0d0d),
            rgb(0x39ff14),
            rgb(0xfaff00),
            rgb(0xff00a0),
            rgb(0x00f0ff),
        ],
    ),
    (
        "Pastel",
        [
            rgb(0xffadad),
            rgb(0xffd6a5),
            rgb(0xfdffb6),
            rgb(0xcaffbf),
            rgb(0xa0c4ff),
        ],
    ),
    (
        "Blood",
        [
            rgb(0x050505),
            rgb(0x3d0000),
            rgb(0x950101),
            rgb(0xff0000),
            rgb(0xe8e8e8),
        ],
    ),
    (
        "Gold",
        [
            rgb(0x1b1300),
            rgb(0x6b4f00),
            rgb(0xc9a227),
            rgb(0xf5d76e),
            rgb(0xfff8e1),
        ],
    ),
];

/// Which palette a scene uses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PaletteChoice {
    /// The scene's own palette from its header.
    #[default]
    Scene,
    /// A built-in palette by name.
    Named(String),
    /// User-picked colours.
    Custom(Palette),
}

impl PaletteChoice {
    pub fn label(&self) -> String {
        match self {
            PaletteChoice::Scene => "Scene colours".into(),
            PaletteChoice::Named(n) => n.clone(),
            PaletteChoice::Custom(_) => "Custom".into(),
        }
    }

    pub fn resolve(&self, scene_default: &Palette) -> Palette {
        match self {
            PaletteChoice::Scene => *scene_default,
            PaletteChoice::Named(n) => named(n).unwrap_or(*scene_default),
            PaletteChoice::Custom(p) => *p,
        }
    }
}

pub fn named(name: &str) -> Option<Palette> {
    BUILTIN
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, p)| *p)
}

/// `#rrggbb` (or `rrggbb`) → RGB 0..1.
pub fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(rgb(v))
}

/// A scene's palette from its header colours; missing entries repeat the
/// given ones, and no colours at all means "Neon".
pub fn from_hex_list(colours: &[String]) -> Result<Palette, String> {
    let parsed: Vec<[f32; 3]> = colours
        .iter()
        .map(|c| parse_hex(c).ok_or_else(|| format!("palette: \"{c}\" is not a #rrggbb colour")))
        .collect::<Result<_, _>>()?;
    if parsed.is_empty() {
        return Ok(BUILTIN[0].1);
    }
    Ok(std::array::from_fn(|i| parsed[i % parsed.len()]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#ff0000"), Some([1.0, 0.0, 0.0]));
        assert_eq!(parse_hex("00ff00"), Some([0.0, 1.0, 0.0]));
        assert_eq!(parse_hex("#fff"), None);
        let p = from_hex_list(&["#000000".into(), "#ffffff".into()]).unwrap();
        assert_eq!(p[2], [0.0, 0.0, 0.0]);
        assert_eq!(p[3], [1.0, 1.0, 1.0]);
        assert!(from_hex_list(&["red".into()]).is_err());
    }

    #[test]
    fn choices_resolve() {
        let own = BUILTIN[3].1;
        assert_eq!(PaletteChoice::Scene.resolve(&own), own);
        assert_eq!(
            PaletteChoice::Named("ice".into()).resolve(&own),
            named("Ice").unwrap()
        );
        assert_eq!(PaletteChoice::Named("??".into()).resolve(&own), own);
    }
}
