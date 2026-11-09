//! Keyboard performance mode: preset keys, blackout, flash on the next beat,
//! global intensity and speed, macros, and the on-screen hotkey sheet.

use std::time::Instant;

use egui::{Align2, Color32, CornerRadius, FontId, Key, Modifiers, Pos2, Rect, Vec2};

/// Manual controls (sliders now, MIDI knobs in M4).
pub const MACROS: usize = 8;

/// Something the keyboard asked for that the scene view must do.
pub enum KeyAction {
    /// Keys 1–9 (0-based slot).
    Preset(usize),
}

pub struct Perform {
    /// Multiplies every modulation depth (1 = as set).
    pub intensity: f32,
    /// Multiplies the speed of time for scenes (clocks, envelopes).
    pub speed: f32,
    /// Master brightness.
    pub brightness: f32,
    pub blackout: bool,
    gain: f32,
    flash_armed: bool,
    flash_age: f32,
    last_beat: f32,
    /// Preset crossfade length in beats (0 = cut).
    pub crossfade_beats: f32,
    pub macros: [f32; MACROS],
    pub show_help: bool,
    toast: Option<(String, Instant)>,
}

const FLASH_DECAY: f32 = 0.18;

impl Default for Perform {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            speed: 1.0,
            brightness: 1.0,
            blackout: false,
            gain: 1.0,
            flash_armed: false,
            flash_age: 1.0e4,
            last_beat: 0.0,
            crossfade_beats: 4.0,
            macros: [0.0; MACROS],
            show_help: false,
            toast: None,
        }
    }
}

const NUM_KEYS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

impl Perform {
    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    pub fn toggle_blackout(&mut self) {
        self.blackout = !self.blackout;
        self.toast(if self.blackout {
            "Blackout"
        } else {
            "Blackout off"
        });
    }

    pub fn arm_flash(&mut self) {
        self.flash_armed = true;
        self.toast("Flash on the next beat");
    }

    pub fn flash_armed(&self) -> bool {
        self.flash_armed
    }

    fn set_intensity(&mut self, v: f32) {
        self.intensity = (v * 10.0).round() / 10.0;
        self.intensity = self.intensity.clamp(0.0, 2.0);
        self.toast(format!("Intensity {:.0}%", self.intensity * 100.0));
    }

    fn set_speed(&mut self, v: f32) {
        self.speed = v.clamp(0.25, 4.0);
        self.toast(format!("Speed {:.0}%", self.speed * 100.0));
    }

    /// Read the performance keys. They are consumed, so a focused slider or
    /// button won't also react to them. Ignored while typing in a text field.
    pub fn handle_keys(&mut self, ctx: &egui::Context) -> Vec<KeyAction> {
        let mut actions = Vec::new();
        if ctx.egui_wants_keyboard_input() {
            return actions;
        }
        let take = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        for (n, k) in NUM_KEYS.iter().enumerate() {
            if take(*k) {
                actions.push(KeyAction::Preset(n));
            }
        }
        if take(Key::Space) {
            self.toggle_blackout();
        }
        if take(Key::F) {
            self.arm_flash();
        }
        if take(Key::ArrowUp) {
            self.set_intensity(self.intensity + 0.1);
        }
        if take(Key::ArrowDown) {
            self.set_intensity(self.intensity - 0.1);
        }
        if take(Key::ArrowRight) {
            self.set_speed(self.speed * 1.25);
        }
        if take(Key::ArrowLeft) {
            self.set_speed(self.speed / 1.25);
        }
        if take(Key::Num0) {
            self.intensity = 1.0;
            self.speed = 1.0;
            self.toast("Intensity and speed back to 100%");
        }
        if take(Key::H) {
            self.show_help = !self.show_help;
        }
        actions
    }

    /// Advance by `dt` real seconds; `beat_count` is the analysis beat count.
    pub fn update(&mut self, dt: f32, beat_count: f32) {
        if beat_count != self.last_beat {
            if beat_count > self.last_beat && self.flash_armed {
                self.flash_armed = false;
                self.flash_age = 0.0;
            }
            self.last_beat = beat_count;
        }
        self.flash_age += dt;
        // Blackout fades over ~0.1 s rather than snapping.
        let target = if self.blackout { 0.0 } else { 1.0 };
        let a = 1.0 - (-dt / 0.06).exp();
        self.gain += a * (target - self.gain);
    }

    /// Master brightness after blackout.
    pub fn gain(&self) -> f32 {
        self.gain * self.brightness
    }

    /// White added by a flash, 0..1.
    pub fn flash(&self) -> f32 {
        (-self.flash_age / FLASH_DECAY).exp()
    }

    /// Status badges, the last toast and (H) the cheat sheet.
    pub fn draw_overlay(&self, p: &egui::Painter, rect: Rect, show_status: bool) {
        if show_status {
            let mut badges: Vec<(String, Color32)> = Vec::new();
            if self.blackout {
                badges.push(("BLACKOUT".into(), Color32::from_rgb(255, 90, 90)));
            }
            if self.flash_armed {
                badges.push(("FLASH ARMED".into(), Color32::WHITE));
            }
            if (self.intensity - 1.0).abs() > 0.01 {
                badges.push((
                    format!("INTENSITY {:.0}%", self.intensity * 100.0),
                    Color32::from_rgb(255, 190, 90),
                ));
            }
            if (self.speed - 1.0).abs() > 0.01 {
                badges.push((
                    format!("SPEED {:.0}%", self.speed * 100.0),
                    Color32::from_rgb(120, 200, 255),
                ));
            }
            let mut x = rect.right() - 10.0;
            for (text, color) in badges {
                let g = p.layout_no_wrap(text, FontId::monospace(12.0), color);
                let r = Rect::from_min_size(
                    Pos2::new(x - g.size().x - 8.0, rect.top() + 8.0),
                    g.size() + Vec2::new(8.0, 6.0),
                );
                p.rect_filled(r, CornerRadius::same(3), Color32::from_black_alpha(170));
                p.galley(r.min + Vec2::new(4.0, 3.0), g, color);
                x = r.left() - 6.0;
            }
            if let Some((text, at)) = &self.toast {
                let age = at.elapsed().as_secs_f32();
                if age < 1.6 {
                    let alpha = ((1.6 - age) / 0.4).min(1.0);
                    let color = Color32::WHITE.gamma_multiply(alpha);
                    let g = p.layout_no_wrap(text.clone(), FontId::proportional(20.0), color);
                    let c = Pos2::new(rect.center().x, rect.bottom() - 60.0);
                    let r = Rect::from_center_size(c, g.size() + Vec2::new(24.0, 12.0));
                    p.rect_filled(
                        r,
                        CornerRadius::same(6),
                        Color32::from_black_alpha((170.0 * alpha) as u8),
                    );
                    p.galley(r.min + Vec2::new(12.0, 6.0), g, color);
                }
            }
        }
        if self.show_help {
            draw_help(p, rect);
        }
    }
}

const HELP: &[(&str, &str)] = &[
    ("1 – 9", "switch to the preset on that key (crossfades)"),
    ("Space", "blackout on / off"),
    ("F", "flash on the next beat"),
    ("↑ / ↓", "intensity: how strongly modulations move"),
    ("← / →", "speed of all motion"),
    ("0", "intensity and speed back to 100%"),
    ("T", "tap tempo"),
    ("R", "this beat is beat 1 (downbeat)"),
    ("F11 / double-click", "fullscreen (Esc leaves)"),
    ("M", "signal meters"),
    ("I", "FPS counter"),
    ("H", "show / hide this sheet"),
];

fn draw_help(p: &egui::Painter, rect: Rect) {
    // Shrink the whole sheet to fit small previews instead of clipping it.
    let (w0, h0) = (470.0, 46.0 + HELP.len() as f32 * 22.0);
    let s = ((rect.width() - 16.0) / w0)
        .min((rect.height() - 16.0) / h0)
        .clamp(0.45, 1.0);
    let r = Rect::from_center_size(rect.center(), Vec2::new(w0 * s, h0 * s));
    p.rect_filled(r, CornerRadius::same(8), Color32::from_black_alpha(215));
    p.text(
        r.left_top() + Vec2::new(18.0, 14.0) * s,
        Align2::LEFT_TOP,
        "HOTKEYS",
        FontId::proportional(16.0 * s),
        Color32::WHITE,
    );
    for (i, (key, what)) in HELP.iter().enumerate() {
        let y = r.top() + (44.0 + i as f32 * 22.0) * s;
        p.text(
            Pos2::new(r.left() + 18.0 * s, y),
            Align2::LEFT_TOP,
            key,
            FontId::monospace(13.0 * s),
            Color32::from_rgb(255, 200, 100),
        );
        p.text(
            Pos2::new(r.left() + 170.0 * s, y),
            Align2::LEFT_TOP,
            what,
            FontId::proportional(13.0 * s),
            Color32::from_gray(220),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_waits_for_the_next_beat() {
        let mut p = Perform::default();
        p.update(0.01, 4.0);
        p.arm_flash();
        p.update(0.1, 4.0);
        assert!(p.flash() < 0.01);
        p.update(0.0, 5.0);
        assert!(p.flash() > 0.99);
        p.update(0.5, 5.0);
        assert!(p.flash() < 0.1);
        assert!(!p.flash_armed());
    }

    #[test]
    fn blackout_fades_quickly() {
        let mut p = Perform::default();
        p.toggle_blackout();
        for _ in 0..30 {
            p.update(1.0 / 60.0, 0.0);
        }
        assert!(p.gain() < 0.01);
        p.toggle_blackout();
        for _ in 0..30 {
            p.update(1.0 / 60.0, 0.0);
        }
        assert!(p.gain() > 0.99);
    }
}
