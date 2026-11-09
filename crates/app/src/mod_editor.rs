//! Slider rows with a "modulate" button and a compact modulation editor, and
//! the palette picker.

use egui::{Color32, RichText, Sense, Stroke, Vec2};
use scenes::meta::InputDef;
use scenes::palette::{self, Palette, PaletteChoice};
use signals::modulation::{ModSlot, Shape, Source, Wave};

use crate::perform::MACROS;

pub const MOD_COLOR: Color32 = Color32::from_rgb(255, 170, 60);

/// Friendly names for signals that make good modulation sources.
pub const SIGNAL_SOURCES: &[(&str, &str)] = &[
    ("kick_age", "Kick hit"),
    ("snare_age", "Snare hit"),
    ("hat_age", "Hi-hat hit"),
    ("onset_age", "Any hit"),
    ("beat_age", "Every beat"),
    ("drop_age", "Drop"),
    ("section_age", "Section change"),
    ("sub_fast", "Sub level"),
    ("bass_fast", "Bass level"),
    ("low_mid_fast", "Low-mid level"),
    ("mid_fast", "Mid level"),
    ("high_mid_fast", "High-mid level"),
    ("air_fast", "Air level"),
    ("bass_slow", "Bass (slow trend)"),
    ("loudness_fast", "Loudness"),
    ("loudness_slow", "Loudness (slow trend)"),
    ("centroid_hue_slow", "Brightness of the sound"),
    ("beat_phase", "Beat phase"),
    ("bar_phase", "Bar phase"),
    ("phrase_phase", "Phrase phase"),
    ("build_amount", "Build-up amount"),
    ("onset_rate", "Busyness (hits/s)"),
];

pub fn source_label(s: &Source) -> String {
    match s {
        Source::Signal(n) => SIGNAL_SOURCES
            .iter()
            .find(|(k, _)| k == n)
            .map(|(_, l)| l.to_string())
            .unwrap_or_else(|| n.clone()),
        Source::Lfo { wave, beats } => format!("LFO {} / {}", wave.name(), beats_label(*beats)),
        Source::Control(i) => format!("Macro {}", i + 1),
        Source::Random { .. } => "Random wander".into(),
    }
}

fn beats_label(b: f32) -> String {
    match b {
        x if (x - 0.25).abs() < 1e-3 => "¼ beat".into(),
        x if (x - 0.5).abs() < 1e-3 => "½ beat".into(),
        x if (x - 1.0).abs() < 1e-3 => "1 beat".into(),
        x if (x % 4.0).abs() < 1e-3 => format!("{} bar{}", x / 4.0, if x > 4.0 { "s" } else { "" }),
        x => format!("{x} beats"),
    }
}

/// The natural shape for a source: hits decay, phases pulse, levels pass.
pub fn default_shape(s: &Source) -> Shape {
    match s {
        Source::Signal(n) if n.ends_with("_age") => Shape::Envelope { decay: 0.15 },
        Source::Signal(n) if n.ends_with("_phase") => Shape::Pulse { k: 3.0 },
        _ => Shape::Linear,
    }
}

pub fn new_slot(source: Source) -> ModSlot {
    let depth = match &source {
        Source::Signal(n) if !n.ends_with("_age") && !n.ends_with("_phase") => 0.1,
        _ => 0.25,
    };
    ModSlot::new(source.clone(), default_shape(&source), depth)
}

fn random_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1)
}

/// Menu of sources; returns the chosen one.
pub fn source_menu(ui: &mut egui::Ui) -> Option<Source> {
    let mut pick = None;
    ui.label(RichText::new("Audio").small().weak());
    for (name, label) in SIGNAL_SOURCES {
        if ui.button(*label).clicked() {
            pick = Some(Source::Signal(name.to_string()));
        }
    }
    ui.separator();
    ui.label(RichText::new("Rhythm & motion").small().weak());
    if ui.button("LFO synced to the beat").clicked() {
        pick = Some(Source::Lfo {
            wave: Wave::Sine,
            beats: 4.0,
        });
    }
    if ui.button("Random wander").clicked() {
        pick = Some(Source::Random {
            rate: 0.5,
            seed: random_seed(),
        });
    }
    ui.separator();
    ui.label(RichText::new("Manual").small().weak());
    for i in 0..MACROS {
        if ui.button(format!("Macro {}", i + 1)).clicked() {
            pick = Some(Source::Control(i));
        }
    }
    if pick.is_some() {
        ui.close();
    }
    pick
}

pub const SHAPES: [Shape; 6] = [
    Shape::Linear,
    Shape::Curve { gamma: 2.0 },
    Shape::Envelope { decay: 0.15 },
    Shape::Pulse { k: 3.0 },
    Shape::Lfo { rate: 1.0 },
    Shape::Step { n: 4.0 },
];

/// Edit one modulation. Returns true if it should be removed.
pub fn slot_ui(ui: &mut egui::Ui, slot: &mut ModSlot, id: usize) -> bool {
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.checkbox(&mut slot.enabled, "");
        ui.menu_button(source_label(&slot.source), |ui| {
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    if let Some(s) = source_menu(ui) {
                        slot.shape = default_shape(&s);
                        slot.source = s;
                    }
                });
        })
        .response
        .on_hover_text("Where the movement comes from");
        if ui.small_button("✕").on_hover_text("Remove").clicked() {
            remove = true;
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(22.0);
        match &mut slot.source {
            Source::Lfo { wave, beats } => {
                egui::ComboBox::from_id_salt(("wave", id))
                    .width(80.0)
                    .selected_text(wave.name())
                    .show_ui(ui, |ui| {
                        for w in Wave::ALL {
                            ui.selectable_value(wave, w, w.name());
                        }
                    });
                egui::ComboBox::from_id_salt(("beats", id))
                    .width(80.0)
                    .selected_text(beats_label(*beats))
                    .show_ui(ui, |ui| {
                        for b in [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0] {
                            ui.selectable_value(beats, b, beats_label(b));
                        }
                    });
            }
            Source::Random { rate, .. } => {
                ui.label("changes/s");
                ui.add(egui::DragValue::new(rate).speed(0.02).range(0.01..=20.0));
            }
            _ => {
                egui::ComboBox::from_id_salt(("shape", id))
                    .width(110.0)
                    .selected_text(slot.shape.name())
                    .show_ui(ui, |ui| {
                        for s in SHAPES {
                            let same =
                                std::mem::discriminant(&s) == std::mem::discriminant(&slot.shape);
                            if ui.selectable_label(same, s.name()).clicked() && !same {
                                slot.shape = s;
                            }
                        }
                    });
                match &mut slot.shape {
                    Shape::Linear => {}
                    Shape::Curve { gamma } => {
                        ui.label("power");
                        ui.add(egui::DragValue::new(gamma).speed(0.02).range(0.1..=8.0));
                    }
                    Shape::Envelope { decay } => {
                        ui.label("fade s");
                        ui.add(egui::DragValue::new(decay).speed(0.005).range(0.01..=4.0));
                    }
                    Shape::Pulse { k } => {
                        ui.label("sharp");
                        ui.add(egui::DragValue::new(k).speed(0.05).range(0.1..=20.0));
                    }
                    Shape::Lfo { rate } => {
                        ui.label("cycles");
                        ui.add(egui::DragValue::new(rate).speed(0.05).range(0.0..=16.0));
                    }
                    Shape::Step { n } => {
                        ui.label("steps");
                        ui.add(egui::DragValue::new(n).speed(0.1).range(1.0..=32.0));
                    }
                }
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(22.0);
        ui.label("amount");
        ui.add(
            egui::Slider::new(&mut slot.depth, -1.0..=1.0)
                .fixed_decimals(2)
                .clamping(egui::SliderClamping::Always),
        )
        .on_hover_text(
            "How far it moves the slider (fraction of its range; negative = the other way)",
        );
    });
    remove
}

/// A float input: label, base-value slider with the live value marked on it,
/// and a modulate button that opens the editor below.
pub fn float_row(
    ui: &mut egui::Ui,
    input: &InputDef,
    value: &mut f32,
    effective: f32,
    mods: &mut Vec<ModSlot>,
    open: &mut bool,
    id: usize,
) {
    let (lo, hi) = input.range();
    let active = mods.iter().any(|m| m.enabled);
    ui.horizontal(|ui| {
        ui.label(input.label());
        if active {
            ui.label(
                RichText::new(format!("→ {effective:.2}"))
                    .small()
                    .color(MOD_COLOR),
            );
        }
    });
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 130.0).max(60.0);
        ui.spacing_mut().slider_width = w;
        let resp = ui.add(egui::Slider::new(value, lo..=hi).show_value(false));
        if active {
            // Live (modulated) value as a marker on the rail.
            let r = resp.rect;
            let t = ((effective - lo) / (hi - lo)).clamp(0.0, 1.0);
            let x = r.left() + 6.0 + t * (r.width() - 12.0);
            ui.painter().line_segment(
                [
                    egui::pos2(x, r.top() + 1.0),
                    egui::pos2(x, r.bottom() - 1.0),
                ],
                Stroke::new(3.0, MOD_COLOR),
            );
        }
        ui.add(
            egui::DragValue::new(value)
                .speed((hi - lo) / 300.0)
                .range(lo..=hi)
                .max_decimals(3),
        );
        let label = if mods.is_empty() {
            "~".to_string()
        } else {
            format!("~{}", mods.len())
        };
        let btn = egui::Button::new(RichText::new(label).color(if active {
            MOD_COLOR
        } else {
            ui.visuals().text_color()
        }))
        .selected(*open);
        if ui
            .add(btn)
            .on_hover_text("Modulate: make this follow the music, the beat, an LFO or a macro")
            .clicked()
        {
            *open = !*open;
        }
    });
    if *open {
        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(28, 26, 22))
            .show(ui, |ui| {
                let mut remove = None;
                for (k, slot) in mods.iter_mut().enumerate() {
                    if k > 0 {
                        ui.separator();
                    }
                    if slot_ui(ui, slot, id * 100 + k) {
                        remove = Some(k);
                    }
                }
                if let Some(k) = remove {
                    mods.remove(k);
                }
                ui.menu_button("＋ Add modulation", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            if let Some(s) = source_menu(ui) {
                                mods.push(new_slot(s));
                            }
                        });
                });
            });
    }
}

fn swatch(ui: &mut egui::Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(70.0, 14.0), Sense::hover());
    let w = rect.width() / p.len() as f32;
    for (i, c) in p.iter().enumerate() {
        let r = egui::Rect::from_min_size(
            rect.min + Vec2::new(i as f32 * w, 0.0),
            Vec2::new(w, rect.height()),
        );
        ui.painter().rect_filled(
            r,
            0.0,
            Color32::from_rgb(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            ),
        );
    }
}

/// Palette picker.
pub fn palette_ui(ui: &mut egui::Ui, choice: &mut PaletteChoice, scene_palette: &Palette) {
    let current = choice.resolve(scene_palette);
    ui.horizontal(|ui| {
        swatch(ui, &current);
        egui::ComboBox::from_id_salt("palette")
            .width(ui.available_width())
            .selected_text(choice.label())
            .height(500.0)
            .show_ui(ui, |ui| {
                let row = |ui: &mut egui::Ui,
                           c: PaletteChoice,
                           p: &Palette,
                           choice: &mut PaletteChoice| {
                    ui.horizontal(|ui| {
                        swatch(ui, p);
                        let sel = *choice == c;
                        if ui.selectable_label(sel, c.label()).clicked() {
                            *choice = c;
                        }
                    });
                };
                row(ui, PaletteChoice::Scene, scene_palette, choice);
                for (name, p) in palette::BUILTIN {
                    row(ui, PaletteChoice::Named(name.to_string()), p, choice);
                }
                let custom = match choice {
                    PaletteChoice::Custom(p) => *p,
                    _ => current,
                };
                row(ui, PaletteChoice::Custom(custom), &custom, choice);
            });
    });
    if let PaletteChoice::Custom(p) = choice {
        ui.horizontal(|ui| {
            for c in p.iter_mut() {
                ui.color_edit_button_rgb(c);
            }
        });
    }
}
