//! Icons: the Phosphor icon font (MIT, `assets/fonts/LICENSE-Phosphor.txt`),
//! bundled into the program. It is added behind the normal text font, so an
//! icon can sit inside any label: `format!("{} Open song…", icons::FOLDER_OPEN)`.
//!
//! In string literals the same icons are written as escapes, e.g.
//! `"\u{E256} Open song…"`; the table below says which is which. Use these
//! instead of emoji: emoji render as coloured blobs or empty boxes.

// A reference table: most icons are written as escapes in string literals,
// so not every name here is used in code.
#![allow(dead_code)]

pub const ARROW_CLOCKWISE: &str = "\u{E036}"; // refresh / rescan / again
pub const ARROW_DOWN: &str = "\u{E03E}";
pub const ARROW_LEFT: &str = "\u{E058}"; // "fed from"
pub const ARROW_U_UP_LEFT: &str = "\u{E08A}"; // undo
pub const ARROW_U_UP_RIGHT: &str = "\u{E08C}"; // redo
pub const ARROW_UP: &str = "\u{E08E}";
pub const CAMERA: &str = "\u{E10E}";
pub const CARET_DOWN: &str = "\u{E136}"; // open / move down
pub const CARET_LEFT: &str = "\u{E138}";
pub const CARET_RIGHT: &str = "\u{E13A}"; // folded
pub const CARET_UP: &str = "\u{E13C}"; // move up
pub const CHECK: &str = "\u{E182}";
pub const CIRCLE_HALF: &str = "\u{E18C}"; // mask
pub const DIAMOND: &str = "\u{E1EC}"; // macro knob
pub const EYE: &str = "\u{E220}"; // shown in the viewer
pub const FILM_STRIP: &str = "\u{E792}"; // templates / sets
pub const FLOPPY_DISK: &str = "\u{E248}"; // save
pub const FOLDER: &str = "\u{E24A}";
pub const FOLDER_OPEN: &str = "\u{E256}";
pub const IMAGE: &str = "\u{E2CA}";
pub const MAGNIFYING_GLASS: &str = "\u{E30C}";
pub const MICROPHONE: &str = "\u{E326}";
pub const PALETTE: &str = "\u{E6C8}"; // themes
pub const PAUSE: &str = "\u{E39E}";
pub const PLAY: &str = "\u{E3D0}";
pub const PLUS: &str = "\u{E3D4}";
pub const SIDEBAR: &str = "\u{EAB6}"; // panels
pub const SPARKLE: &str = "\u{E6A2}";
pub const SPEAKER_HIGH: &str = "\u{E44A}"; // computer audio / sound
pub const SQUARES_FOUR: &str = "\u{E464}"; // layouts
pub const STOP: &str = "\u{E46C}";
pub const TOGGLE_LEFT: &str = "\u{E674}"; // bypassed
pub const TOGGLE_RIGHT: &str = "\u{E676}"; // on
pub const TRASH: &str = "\u{E4A6}";
pub const WARNING: &str = "\u{E4E0}";
pub const X: &str = "\u{E4F6}"; // remove / unplug

const FONT: &[u8] = include_bytes!("../assets/fonts/Phosphor.ttf");

/// Add the icon font behind the text fonts (call once at start-up).
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("phosphor".into(), egui::FontData::from_static(FONT).into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(keys) = fonts.families.get_mut(&family) {
            // Right after the main font, before the emoji fallbacks.
            let at = keys.len().min(1);
            keys.insert(at, "phosphor".into());
        }
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    /// Every icon above is really in the bundled font (a wrong code point
    /// would show an empty box).
    #[test]
    fn the_font_has_every_icon() {
        let ctx = egui::Context::default();
        super::install(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let font = egui::FontId::proportional(14.0);
        for icon in [
            super::ARROW_CLOCKWISE,
            super::ARROW_DOWN,
            super::ARROW_LEFT,
            super::ARROW_U_UP_LEFT,
            super::ARROW_U_UP_RIGHT,
            super::ARROW_UP,
            super::CAMERA,
            super::CARET_DOWN,
            super::CARET_LEFT,
            super::CARET_RIGHT,
            super::CARET_UP,
            super::CHECK,
            super::CIRCLE_HALF,
            super::DIAMOND,
            super::EYE,
            super::FILM_STRIP,
            super::FLOPPY_DISK,
            super::FOLDER,
            super::FOLDER_OPEN,
            super::IMAGE,
            super::MAGNIFYING_GLASS,
            super::MICROPHONE,
            super::PALETTE,
            super::PAUSE,
            super::PLAY,
            super::PLUS,
            super::SIDEBAR,
            super::SPARKLE,
            super::SPEAKER_HIGH,
            super::SQUARES_FOUR,
            super::STOP,
            super::TOGGLE_LEFT,
            super::TOGGLE_RIGHT,
            super::TRASH,
            super::WARNING,
            super::X,
        ] {
            let ch = icon.chars().next().unwrap();
            assert!(
                ctx.fonts_mut(|f| f.has_glyph(&font, ch)),
                "U+{:04X} is missing",
                ch as u32
            );
        }
    }
}
