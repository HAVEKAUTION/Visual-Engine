//! The look of the app. Colours, text size, roundness and spacing come from a
//! **theme**: a few are built in, and anyone can make their own in the theme
//! editor (Theme menu → Edit colours…) or by editing a `.ron` text file in the
//! themes folder. A theme file being edited in Notepad updates the app as soon
//! as it is saved. Themes are shareable: send someone the `.ron` file.
//!
//! The UI reads colours through the functions below (`accent()`, `panel()`…),
//! so a theme change shows on the next frame everywhere.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};
use std::time::SystemTime;

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, TextStyle, Vec2};
use serde::{Deserialize, Serialize};

/// Colours are written as `"#RRGGBB"` in theme files.
mod hex {
    use egui::Color32;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(c: &Color32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::to_hex(*c))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Color32, D::Error> {
        let text = String::deserialize(d)?;
        super::from_hex(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("\"{text}\" isn't a colour like \"#F2A648\""))
        })
    }
}

pub fn to_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

pub fn from_hex(text: &str) -> Option<Color32> {
    let h = text.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// One theme. Missing entries in a file take the default theme's value, so
/// a file can change just the accent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    /// Selected and active things, headings: the "you are here" colour.
    #[serde(with = "hex")]
    pub accent: Color32,
    /// Panel backgrounds.
    #[serde(with = "hex")]
    pub panel: Color32,
    /// Behind the panels: tab bars, gaps, text fields.
    #[serde(with = "hex")]
    pub deep: Color32,
    /// Buttons and sliders.
    #[serde(with = "hex")]
    pub raised: Color32,
    /// Buttons under the mouse.
    #[serde(with = "hex")]
    pub hover: Color32,
    /// Thin dividing lines.
    #[serde(with = "hex")]
    pub line: Color32,
    #[serde(with = "hex")]
    pub text: Color32,
    /// Hints and labels.
    #[serde(with = "hex")]
    pub text_weak: Color32,
    #[serde(with = "hex")]
    pub error: Color32,
    /// Node graph background.
    #[serde(with = "hex")]
    pub canvas: Color32,
    /// Node graph grid dots.
    #[serde(with = "hex")]
    pub grid: Color32,
    /// Node bodies.
    #[serde(with = "hex")]
    pub node: Color32,
    /// Node title bars.
    #[serde(with = "hex")]
    pub node_header: Color32,
    /// Around the picture in the viewer.
    #[serde(with = "hex")]
    pub viewer_bg: Color32,
    /// Body text size (points). Headings and small text follow it.
    pub text_size: f32,
    /// Corner roundness of buttons and windows (0 = square).
    pub roundness: f32,
    /// Room between things (1 = normal).
    pub spacing: f32,
}

impl Default for Theme {
    fn default() -> Self {
        ember()
    }
}

fn c(hex: &str) -> Color32 {
    from_hex(hex).unwrap_or(Color32::MAGENTA)
}

/// The default: deep charcoal with one warm accent.
pub fn ember() -> Theme {
    Theme {
        name: "Ember".into(),
        accent: c("#F2A648"),
        panel: c("#1A1A1F"),
        deep: c("#0F0F12"),
        raised: c("#2A2A31"),
        hover: c("#383842"),
        line: c("#2E2E36"),
        text: c("#E2E2E2"),
        text_weak: c("#8E8E96"),
        error: c("#FF6E6E"),
        canvas: c("#111114"),
        grid: c("#26262C"),
        node: c("#25252B"),
        node_header: c("#33333B"),
        viewer_bg: c("#08080A"),
        text_size: 13.5,
        roundness: 4.0,
        spacing: 1.0,
    }
}

/// Neutral greys and an orange-red accent, like a colour-grading suite.
pub fn graphite() -> Theme {
    Theme {
        name: "Graphite".into(),
        accent: c("#EB6A3C"),
        panel: c("#28282B"),
        deep: c("#1B1B1D"),
        raised: c("#3A3A3F"),
        hover: c("#4A4A50"),
        line: c("#3C3C42"),
        text: c("#E8E8E8"),
        text_weak: c("#9C9CA2"),
        error: c("#FF6B6B"),
        canvas: c("#1E1E20"),
        grid: c("#2E2E32"),
        node: c("#34343A"),
        node_header: c("#44444C"),
        viewer_bg: c("#0C0C0D"),
        text_size: 13.5,
        roundness: 3.0,
        spacing: 1.0,
    }
}

/// Deep violet with an electric cyan accent, for dark clubs.
pub fn neon() -> Theme {
    Theme {
        name: "Neon".into(),
        accent: c("#36F2DC"),
        panel: c("#15121F"),
        deep: c("#0B0912"),
        raised: c("#241D36"),
        hover: c("#32294A"),
        line: c("#2A2340"),
        text: c("#ECE8FF"),
        text_weak: c("#9188AE"),
        error: c("#FF5C8A"),
        canvas: c("#0D0B15"),
        grid: c("#221C33"),
        node: c("#1E1830"),
        node_header: c("#2B2244"),
        viewer_bg: c("#050408"),
        text_size: 13.5,
        roundness: 6.0,
        spacing: 1.0,
    }
}

/// Black and white with a yellow accent and bigger text: easiest to read.
pub fn high_contrast() -> Theme {
    Theme {
        name: "High contrast".into(),
        accent: c("#FFD400"),
        panel: c("#0B0B0B"),
        deep: c("#000000"),
        raised: c("#2A2A2A"),
        hover: c("#454545"),
        line: c("#5C5C5C"),
        text: c("#FFFFFF"),
        text_weak: c("#CFCFCF"),
        error: c("#FF5050"),
        canvas: c("#000000"),
        grid: c("#2C2C2C"),
        node: c("#1C1C1C"),
        node_header: c("#333333"),
        viewer_bg: c("#000000"),
        text_size: 15.5,
        roundness: 2.0,
        spacing: 1.15,
    }
}

pub fn built_in() -> Vec<Theme> {
    vec![ember(), graphite(), neon(), high_contrast()]
}

static CURRENT: LazyLock<RwLock<Theme>> = LazyLock::new(|| RwLock::new(ember()));

fn read<T>(f: impl FnOnce(&Theme) -> T) -> T {
    f(&CURRENT.read().unwrap_or_else(|e| e.into_inner()))
}

pub fn current() -> Theme {
    read(Theme::clone)
}

pub fn accent() -> Color32 {
    read(|t| t.accent)
}
pub fn panel() -> Color32 {
    read(|t| t.panel)
}
pub fn deep() -> Color32 {
    read(|t| t.deep)
}
pub fn raised() -> Color32 {
    read(|t| t.raised)
}
pub fn line() -> Color32 {
    read(|t| t.line)
}
pub fn text() -> Color32 {
    read(|t| t.text)
}
pub fn text_weak() -> Color32 {
    read(|t| t.text_weak)
}
pub fn error() -> Color32 {
    read(|t| t.error)
}
pub fn canvas() -> Color32 {
    read(|t| t.canvas)
}
pub fn grid() -> Color32 {
    read(|t| t.grid)
}
/// Every 8th grid dot, a little brighter.
pub fn grid_major() -> Color32 {
    read(|t| t.grid.lerp_to_gamma(t.text_weak, 0.18))
}
pub fn node() -> Color32 {
    read(|t| t.node)
}
pub fn node_header() -> Color32 {
    read(|t| t.node_header)
}
pub fn viewer_bg() -> Color32 {
    read(|t| t.viewer_bg)
}

/// A panel section title: bold, in the accent colour, with a rule under it.
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(text)
            .strong()
            .size(read(|t| t.text_size) - 0.5)
            .color(accent()),
    );
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 2.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(r, 1.0, accent().gamma_multiply(0.35));
    ui.add_space(2.0);
}

/// Make `theme` the current one and restyle the app.
pub fn set(ctx: &egui::Context, theme: Theme) {
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = theme;
    apply(ctx);
}

/// Restyle egui from the current theme.
pub fn apply(ctx: &egui::Context) {
    let t = current();
    let text_size = t.text_size.clamp(9.0, 24.0);
    let sp = t.spacing.clamp(0.6, 1.8);
    let round = t.roundness.clamp(0.0, 12.0) as u8;
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = t.panel;
        v.window_fill = t.panel.lerp_to_gamma(t.raised, 0.35);
        v.extreme_bg_color = t.deep;
        v.faint_bg_color = t.panel.lerp_to_gamma(t.raised, 0.2);
        v.code_bg_color = t.deep;
        v.window_stroke = Stroke::new(1.0, t.line);
        v.window_corner_radius = CornerRadius::same(round + 2);
        v.menu_corner_radius = CornerRadius::same(round + 2);
        v.window_shadow = egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(120),
        };
        v.popup_shadow = v.window_shadow;
        v.selection.bg_fill = t.accent.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, t.accent);
        v.hyperlink_color = t.accent;
        v.warn_fg_color = t.accent;
        v.error_fg_color = t.error;
        v.override_text_color = None;
        let radius = CornerRadius::same(round);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = t.panel;
        w.noninteractive.weak_bg_fill = t.panel;
        w.noninteractive.bg_stroke = Stroke::new(1.0, t.line);
        w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
        w.noninteractive.corner_radius = radius;
        let pressed = t.hover.lerp_to_gamma(t.accent, 0.18);
        for (state, fill) in [
            (&mut w.inactive, t.raised),
            (&mut w.hovered, t.hover),
            (&mut w.active, pressed),
            (&mut w.open, t.hover),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::NONE;
            state.corner_radius = radius;
            state.expansion = 0.0;
        }
        w.inactive.fg_stroke = Stroke::new(1.0, t.text.lerp_to_gamma(t.text_weak, 0.25));
        w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE.lerp_to_gamma(t.text, 0.3));
        w.hovered.bg_stroke = Stroke::new(1.0, t.line.lerp_to_gamma(t.text_weak, 0.4));
        w.active.fg_stroke = Stroke::new(1.0, t.accent);
        w.active.bg_stroke = Stroke::new(1.0, t.accent.gamma_multiply(0.6));
        w.open.fg_stroke = Stroke::new(1.0, Color32::WHITE.lerp_to_gamma(t.text, 0.3));

        let s = &mut style.spacing;
        s.item_spacing = Vec2::new(6.0, 5.0) * sp;
        s.button_padding = Vec2::new(8.0, 3.5) * sp;
        s.interact_size = Vec2::new(36.0, text_size + 8.0);
        s.menu_margin = Margin::same((6.0 * sp) as i8);
        s.window_margin = Margin::same((8.0 * sp) as i8);
        s.slider_rail_height = 4.0;
        s.combo_width = 120.0;

        let p = |size: f32| FontId::new(size, FontFamily::Proportional);
        style.text_styles = [
            (TextStyle::Heading, p(text_size + 4.0)),
            (TextStyle::Body, p(text_size)),
            (TextStyle::Button, p(text_size)),
            (TextStyle::Small, p(text_size - 2.0)),
            (
                TextStyle::Monospace,
                FontId::new(text_size - 1.0, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

// ---------------------------------------------------------------- files

/// The themes folder (inside the app's data folder).
pub fn folder() -> Option<PathBuf> {
    crate::graph_view::data_folder().map(|d| d.join("themes"))
}

pub fn load_file(path: &Path) -> Result<Theme, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut theme: Theme = ron::from_str(&text).map_err(|e| e.to_string())?;
    if theme.name.trim().is_empty() || theme.name == ember().name {
        theme.name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Theme".into());
    }
    Ok(theme)
}

const HEADER: &str = "// A Visual Engine theme. Colours are \"#RRGGBB\". Edit and save: the app\n\
// updates straight away. Share it by sending this file; put it in the themes\n\
// folder (Theme menu -> Open themes folder) or use Theme -> Import.\n";

pub fn to_text(theme: &Theme) -> String {
    let body =
        ron::ser::to_string_pretty(theme, ron::ser::PrettyConfig::default()).unwrap_or_default();
    format!("{HEADER}{body}\n")
}

/// A safe file name for a theme name.
pub fn file_name(name: &str) -> String {
    let mut s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        s = "my_theme".into();
    }
    format!("{s}.ron")
}

pub fn save_to(dir: &Path, theme: &Theme) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(file_name(&theme.name));
    std::fs::write(&path, to_text(theme)).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Every theme file in the folder, by name.
pub fn scan(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ron")))
        .map(|p| {
            let name = load_file(&p).map(|t| t.name).unwrap_or_else(|_| {
                p.file_stem()
                    .map(|s| format!("{} (has a mistake)", s.to_string_lossy()))
                    .unwrap_or_default()
            });
            (p, name)
        })
        .collect();
    out.sort_by_key(|a| a.1.to_lowercase());
    out
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

// ---------------------------------------------------------------- the app side

/// Which theme is chosen (saved in the settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Choice {
    BuiltIn(String),
    File(PathBuf),
}

impl Default for Choice {
    fn default() -> Self {
        Choice::BuiltIn(ember().name)
    }
}

/// The Theme menu, the theme editor window and live reload.
pub struct Themes {
    pub choice: Choice,
    /// Last-seen change time of the chosen theme file.
    stamp: Option<SystemTime>,
    last_check: f64,
    user: Vec<(PathBuf, String)>,
    pub editor_open: bool,
    /// The theme as it was before editing (for "Undo my changes").
    before_edit: Option<Theme>,
    save_name: String,
    pub message: Option<(String, bool)>,
}

impl Themes {
    /// Pick up the saved choice. A missing or broken file falls back to the
    /// default theme (with a message).
    pub fn new(ctx: &egui::Context, choice: Choice) -> Self {
        let mut me = Self {
            choice: Choice::default(),
            stamp: None,
            last_check: 0.0,
            user: Vec::new(),
            editor_open: false,
            before_edit: None,
            save_name: String::new(),
            message: None,
        };
        set(ctx, ember());
        me.rescan();
        me.choose(ctx, choice);
        me
    }

    fn rescan(&mut self) {
        self.user = folder().map(|d| scan(&d)).unwrap_or_default();
    }

    fn say(&mut self, text: impl Into<String>, ok: bool) {
        self.message = Some((text.into(), ok));
    }

    pub fn choose(&mut self, ctx: &egui::Context, choice: Choice) {
        match &choice {
            Choice::BuiltIn(name) => {
                let theme = built_in()
                    .into_iter()
                    .find(|t| &t.name == name)
                    .unwrap_or_else(ember);
                set(ctx, theme);
                self.choice = Choice::BuiltIn(current().name);
                self.stamp = None;
                self.message = None;
            }
            Choice::File(path) => match load_file(path) {
                Ok(theme) => {
                    set(ctx, theme);
                    self.stamp = modified(path);
                    self.choice = choice.clone();
                    self.message = None;
                }
                Err(e) => {
                    // Keep whatever look is showing; if that was this file,
                    // fall back to the default.
                    if self.choice == choice {
                        self.choice = Choice::default();
                        self.stamp = None;
                        set(ctx, ember());
                    }
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    self.say(format!("Couldn't use the theme \"{name}\": {e}"), false);
                }
            },
        }
    }

    /// Live reload: when the chosen theme file changes on disk, use it. A
    /// file with a mistake keeps the last good look and says what's wrong.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.poll_at(ctx, now);
    }

    fn poll_at(&mut self, ctx: &egui::Context, now: f64) {
        if now - self.last_check < 0.5 {
            return;
        }
        self.last_check = now;
        let Choice::File(path) = &self.choice else {
            return;
        };
        let stamp = modified(path);
        if stamp.is_none() || stamp == self.stamp {
            return;
        }
        self.stamp = stamp;
        match load_file(path) {
            Ok(theme) => {
                set(ctx, theme);
                self.say("Theme file changed: updated.", true);
            }
            Err(e) => self.say(
                format!("The theme file has a mistake, so the last good look stays: {e}"),
                false,
            ),
        }
    }

    /// The toolbar's Theme menu.
    pub fn menu(&mut self, ui: &mut egui::Ui) {
        let resp = ui.menu_button(
            RichText::new(format!("{} Theme", crate::icons::PALETTE)).strong(),
            |ui| {
                ui.label(RichText::new("Built in").small().weak());
                for t in built_in() {
                    let on = self.choice == Choice::BuiltIn(t.name.clone());
                    if ui.radio(on, t.name.as_str()).clicked() {
                        self.choose(ui.ctx(), Choice::BuiltIn(t.name));
                    }
                }
                ui.separator();
                ui.label(RichText::new("My themes").small().weak());
                if self.user.is_empty() {
                    ui.label(
                        RichText::new("None yet: use Edit colours… then Save.")
                            .small()
                            .weak(),
                    );
                }
                for (path, name) in self.user.clone() {
                    let on = self.choice == Choice::File(path.clone());
                    if ui
                        .radio(on, name.as_str())
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        self.choose(ui.ctx(), Choice::File(path));
                    }
                }
                ui.separator();
                if ui
                    .button("\u{E6C8} Edit colours…")
                    .on_hover_text("Change any colour, the text size and roundness, and save it")
                    .clicked()
                {
                    self.open_editor();
                    ui.close();
                }
                if ui
                    .button("Import a theme file…")
                    .on_hover_text("Add a .ron theme someone shared with you")
                    .clicked()
                {
                    self.import(ui.ctx());
                    ui.close();
                }
                if ui
                    .button("Open themes folder")
                    .on_hover_text(
                        "Theme files are plain text. Open one in Notepad: the app updates \
                     when you save it.",
                    )
                    .clicked()
                {
                    self.open_folder();
                    ui.close();
                }
                if let Some((m, ok)) = &self.message {
                    ui.separator();
                    ui.label(RichText::new(m).small().color(if *ok {
                        text_weak()
                    } else {
                        error()
                    }));
                }
            },
        );
        if resp.response.clicked() {
            self.rescan();
        }
        resp.response
            .on_hover_text("Colours, text size and roundness of the whole app");
    }

    fn open_editor(&mut self) {
        self.editor_open = true;
        self.before_edit = Some(current());
        let name = current().name;
        self.save_name = if matches!(self.choice, Choice::BuiltIn(_)) {
            format!("My {name}")
        } else {
            name
        };
    }

    fn import(&mut self, ctx: &egui::Context) {
        let Some(src) = rfd::FileDialog::new()
            .add_filter("Theme", &["ron"])
            .pick_file()
        else {
            return;
        };
        let Some(dir) = folder() else { return };
        match load_file(&src) {
            Ok(theme) => match save_to(&dir, &theme) {
                Ok(path) => {
                    self.rescan();
                    self.choose(ctx, Choice::File(path));
                    self.say(format!("Imported \"{}\".", theme.name), true);
                }
                Err(e) => self.say(format!("Couldn't save the theme: {e}"), false),
            },
            Err(e) => self.say(format!("That file isn't a theme: {e}"), false),
        }
    }

    fn open_folder(&mut self) {
        let Some(dir) = folder() else { return };
        if !dir.exists() {
            // Start the folder with an example to copy.
            let mut example = current();
            example.name = format!("{} (example)", example.name);
            let _ = save_to(&dir, &example);
            self.rescan();
        }
        if let Err(e) = std::process::Command::new("explorer").arg(&dir).spawn() {
            self.say(format!("Couldn't open {}: {e}", dir.display()), false);
        }
    }

    /// The theme editor window: every colour, live.
    pub fn editor(&mut self, ctx: &egui::Context) {
        if !self.editor_open {
            return;
        }
        let mut open = true;
        let mut t = current();
        let mut changed = false;
        egui::Window::new("Theme editor")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_pos(ctx.content_rect().center() - Vec2::new(170.0, 260.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Changes show straight away. Save to keep them.")
                        .small()
                        .weak(),
                );
                egui::Grid::new("theme_colours")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        let rows: [(&str, &str, &mut Color32); 14] = [
                            (
                                "Accent",
                                "Selected and active things, headings",
                                &mut t.accent,
                            ),
                            ("Panels", "Panel backgrounds", &mut t.panel),
                            ("Behind panels", "Tab bars, gaps, text boxes", &mut t.deep),
                            ("Buttons", "Buttons and sliders", &mut t.raised),
                            ("Buttons (mouse over)", "", &mut t.hover),
                            ("Lines", "Thin dividers", &mut t.line),
                            ("Text", "", &mut t.text),
                            ("Quiet text", "Hints and labels", &mut t.text_weak),
                            ("Errors", "", &mut t.error),
                            ("Graph background", "", &mut t.canvas),
                            ("Graph dots", "", &mut t.grid),
                            ("Nodes", "Node bodies", &mut t.node),
                            ("Node title bars", "", &mut t.node_header),
                            ("Viewer surround", "Around the picture", &mut t.viewer_bg),
                        ];
                        for (label, hint, colour) in rows {
                            let l = ui.label(label);
                            if !hint.is_empty() {
                                l.on_hover_text(hint);
                            }
                            ui.horizontal(|ui| {
                                changed |= ui.color_edit_button_srgba(colour).changed();
                                ui.label(RichText::new(to_hex(*colour)).small().weak());
                            });
                            ui.end_row();
                        }
                        ui.label("Text size");
                        changed |= ui
                            .add(egui::Slider::new(&mut t.text_size, 11.0..=18.0).step_by(0.5))
                            .changed();
                        ui.end_row();
                        ui.label("Roundness");
                        changed |= ui
                            .add(egui::Slider::new(&mut t.roundness, 0.0..=10.0).step_by(1.0))
                            .changed();
                        ui.end_row();
                        ui.label("Spacing");
                        changed |= ui
                            .add(egui::Slider::new(&mut t.spacing, 0.8..=1.4).step_by(0.05))
                            .changed();
                        ui.end_row();
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(160.0));
                });
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("\u{E248} Save").strong())
                        .on_hover_text("Save as one of My themes (a .ron file you can share)")
                        .clicked()
                    {
                        self.save_current(ctx);
                    }
                    if ui
                        .button("Undo my changes")
                        .on_hover_text("Back to how the theme was when this window opened")
                        .clicked()
                        && let Some(b) = self.before_edit.clone()
                    {
                        set(ctx, b);
                    }
                });
                if let Some((m, ok)) = &self.message {
                    ui.label(RichText::new(m).small().color(if *ok {
                        text_weak()
                    } else {
                        error()
                    }));
                }
            });
        if changed {
            set(ctx, t);
        }
        if !open {
            self.editor_open = false;
        }
    }

    fn save_current(&mut self, ctx: &egui::Context) {
        let Some(dir) = folder() else {
            self.say("There's no folder to save themes in.", false);
            return;
        };
        let mut theme = current();
        let name = self.save_name.trim();
        theme.name = if name.is_empty() {
            "My theme".into()
        } else {
            name.into()
        };
        if built_in().iter().any(|b| b.name == theme.name) {
            theme.name = format!("My {}", theme.name);
            self.save_name = theme.name.clone();
        }
        match save_to(&dir, &theme) {
            Ok(path) => {
                self.rescan();
                self.choose(ctx, Choice::File(path.clone()));
                self.before_edit = Some(current());
                self.say(format!("Saved as {}", path.display()), true);
            }
            Err(e) => self.say(format!("Couldn't save: {e}"), false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let col = Color32::from_rgb(242, 166, 72);
        assert_eq!(to_hex(col), "#F2A648");
        assert_eq!(from_hex("#f2a648"), Some(col));
        assert_eq!(from_hex("f2a648"), Some(col));
        assert_eq!(from_hex("#f2a64"), None);
        assert_eq!(from_hex("#zzzzzz"), None);
    }

    #[test]
    fn theme_files_round_trip_and_fill_gaps() {
        for t in built_in() {
            let back: Theme = ron::from_str(&to_text(&t)).unwrap();
            assert_eq!(back, t);
        }
        // A file with only an accent takes the rest from the default.
        let t: Theme = ron::from_str("(name: \"Red\", accent: \"#FF0000\")").unwrap();
        assert_eq!(t.accent, Color32::from_rgb(255, 0, 0));
        assert_eq!(t.panel, ember().panel);
        // A mistake is reported, not ignored.
        assert!(ron::from_str::<Theme>("(accent: \"red\")").is_err());
    }

    #[test]
    fn save_scan_load() {
        let dir = std::env::temp_dir().join(format!("ve_themes_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut t = neon();
        t.name = "Club / night".into();
        let path = save_to(&dir, &t).unwrap();
        assert_eq!(path.file_name().unwrap(), "Club___night.ron");
        std::fs::write(dir.join("broken.ron"), "(accent: 12)").unwrap();
        let found = scan(&dir);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|(_, n)| n == "Club / night"));
        assert!(found.iter().any(|(_, n)| n.contains("mistake")));
        assert_eq!(load_file(&path).unwrap(), t);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_reload_follows_the_file_and_survives_mistakes() {
        let dir = std::env::temp_dir().join(format!("ve_live_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut t = graphite();
        t.name = "Live".into();
        let path = save_to(&dir, &t).unwrap();
        let ctx = egui::Context::default();
        let mut themes = Themes::new(&ctx, Choice::File(path.clone()));
        assert_eq!(current().name, "Live");

        let touch = |secs: u64| {
            let f = std::fs::File::options().write(true).open(&path).unwrap();
            f.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs))
                .unwrap();
        };
        // Edited in Notepad: the accent changes.
        t.accent = Color32::from_rgb(1, 2, 3);
        std::fs::write(&path, to_text(&t)).unwrap();
        touch(1_000_000);
        themes.poll_at(&ctx, 10.0);
        assert_eq!(accent(), Color32::from_rgb(1, 2, 3));

        // A typo keeps the last good look and says so.
        std::fs::write(&path, "(accent: \"oops\")").unwrap();
        touch(2_000_000);
        themes.poll_at(&ctx, 20.0);
        assert_eq!(accent(), Color32::from_rgb(1, 2, 3));
        assert!(matches!(&themes.message, Some((m, false)) if m.contains("mistake")));

        // Missing file at start-up: falls back to the default.
        let _ = std::fs::remove_dir_all(&dir);
        let themes = Themes::new(&ctx, Choice::File(path));
        assert_eq!(themes.choice, Choice::default());
        assert_eq!(current().name, ember().name);
    }

    #[test]
    fn built_in_names_are_unique() {
        let names: Vec<String> = built_in().into_iter().map(|t| t.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }
}
