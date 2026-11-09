//! The look of the app: deep charcoal, one warm accent, compact clean type,
//! thin dividers, no heavy borders or gradients (DIRECTION_UPDATE_v2.md, UI).

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Vec2};

/// The one warm accent (selection, active things, the "you are here" colour).
pub const ACCENT: Color32 = Color32::from_rgb(242, 166, 72);
/// Panels.
pub const PANEL: Color32 = Color32::from_rgb(24, 24, 28);
/// Behind everything (viewer surround, node canvas).
pub const DEEP: Color32 = Color32::from_rgb(17, 17, 20);
/// Raised things (buttons, fields).
pub const RAISED: Color32 = Color32::from_rgb(38, 38, 44);
pub const HOVER: Color32 = Color32::from_rgb(50, 50, 58);
/// Thin dividers.
pub const LINE: Color32 = Color32::from_rgb(44, 44, 51);
pub const TEXT: Color32 = Color32::from_gray(222);
pub const TEXT_WEAK: Color32 = Color32::from_gray(138);
pub const ERROR: Color32 = Color32::from_rgb(255, 110, 110);

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = PANEL;
        v.window_fill = Color32::from_rgb(30, 30, 35);
        v.extreme_bg_color = DEEP;
        v.faint_bg_color = Color32::from_rgb(28, 28, 33);
        v.code_bg_color = DEEP;
        v.window_stroke = Stroke::new(1.0, LINE);
        v.window_corner_radius = CornerRadius::same(6);
        v.menu_corner_radius = CornerRadius::same(6);
        v.window_shadow = egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(110),
        };
        v.popup_shadow = v.window_shadow;
        v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.hyperlink_color = ACCENT;
        v.override_text_color = None;
        let radius = CornerRadius::same(4);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = PANEL;
        w.noninteractive.weak_bg_fill = PANEL;
        w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.corner_radius = radius;
        for (state, fill) in [
            (&mut w.inactive, RAISED),
            (&mut w.hovered, HOVER),
            (&mut w.active, Color32::from_rgb(62, 58, 52)),
            (&mut w.open, HOVER),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::NONE;
            state.corner_radius = radius;
            state.expansion = 0.0;
        }
        w.inactive.fg_stroke = Stroke::new(1.0, Color32::from_gray(205));
        w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(70));
        w.active.fg_stroke = Stroke::new(1.0, ACCENT);
        w.active.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
        w.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);

        let s = &mut style.spacing;
        s.item_spacing = Vec2::new(6.0, 5.0);
        s.button_padding = Vec2::new(7.0, 3.0);
        s.interact_size = Vec2::new(36.0, 20.0);
        s.menu_margin = Margin::same(6);
        s.window_margin = Margin::same(8);
        s.slider_rail_height = 4.0;
        s.combo_width = 120.0;

        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(16.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(13.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.0, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}
