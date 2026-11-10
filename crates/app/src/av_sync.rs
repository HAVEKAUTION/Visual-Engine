//! Sound and picture in sync.
//!
//! One global offset, −200…+200 ms:
//! - **+** = pictures later. For speakers that play late (Bluetooth, a long
//!   PA chain): the signals the visuals see wait in a short delay line.
//! - **−** = pictures earlier. For screens / projectors that show late: a
//!   song file is analysed that far *ahead* of what's heard (exact); with a
//!   live input the future isn't known, so only the beat clock is moved
//!   ahead (beat / bar / phrase phase are predictable from the tempo).
//!
//! The sync test finds the offset by tapping along: first to a flashing dot
//! (how late the picture reaches you), then to a click (how late the sound
//! does). Your own reaction time is in both and cancels out.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use audio::ClickTrack;
use egui::{Color32, RichText};
use signals::SignalFrame;

use crate::theme;

pub const OFFSET_RANGE: std::ops::RangeInclusive<f32> = -200.0..=200.0;
/// Seconds between test clicks (80 BPM).
const INTERVAL: f64 = 0.75;
const TAPS: usize = 8;
/// How long the test dot stays lit.
const FLASH_SECS: f64 = 0.09;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Tap on each flash (silent).
    Flash,
    /// Tap on each click (no flash).
    Click,
    /// Both together, with the offset applied: fine-tune by ear and eye.
    Check,
}

struct Test {
    click: ClickTrack,
    step: Step,
    /// Tap errors in seconds (tap time − when the flash / click was sent).
    taps: Vec<f64>,
    flash_late: Option<f64>,
}

pub struct AvSync {
    pub offset_ms: f32,
    pub open: bool,
    delay: VecDeque<(Instant, SignalFrame)>,
    test: Option<Test>,
    error: Option<String>,
    result: Option<String>,
}

impl Default for AvSync {
    fn default() -> Self {
        Self {
            offset_ms: 0.0,
            open: false,
            delay: VecDeque::new(),
            test: None,
            error: None,
            result: None,
        }
    }
}

/// Move the beat clock of `f` ahead by `secs` (it's predictable from the
/// tempo; hits aren't).
pub fn predict_clock(f: &mut SignalFrame, secs: f32) {
    let r = &mut f.rhythm;
    if !r.tempo_locked || secs <= 0.0 {
        return;
    }
    let beats = secs * r.bpm / 60.0;
    let p = r.beat_phase + beats;
    let whole = p.floor();
    r.beat_phase = p - whole;
    if whole >= 1.0 {
        r.beat.count += whole as u32;
        r.beat_in_bar = (r.beat_in_bar + whole as u32) % 4;
    }
    r.bar_phase = (r.bar_phase + beats / 4.0).fract();
    let phrase_beats = 4.0 * r.phrase_bars.max(1) as f32;
    r.phrase_phase = (r.phrase_phase + beats / phrase_beats).fract();
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s[s.len() / 2]
}

impl AvSync {
    /// The analysis frame the visuals should use now.
    pub fn frame(&mut self, latest: &SignalFrame, live_input: bool) -> SignalFrame {
        let now = Instant::now();
        if self
            .delay
            .back()
            .is_none_or(|(_, f)| f.hop_index != latest.hop_index)
        {
            self.delay.push_back((now, latest.clone()));
        }
        let wait = Duration::from_secs_f32(self.offset_ms.max(0.0) / 1000.0);
        // Keep the newest frame that is at least `wait` old (and nothing older).
        while self.delay.len() > 1 && now.duration_since(self.delay[1].0) >= wait {
            self.delay.pop_front();
        }
        let mut f = if self.offset_ms > 0.0 {
            self.delay
                .front()
                .map(|(_, f)| f.clone())
                .unwrap_or_else(|| latest.clone())
        } else {
            latest.clone()
        };
        if self.offset_ms < 0.0 && live_input {
            predict_clock(&mut f, -self.offset_ms / 1000.0);
        }
        f
    }

    /// How far ahead a song file should be analysed (seconds).
    pub fn player_lead(&self) -> f64 {
        (-self.offset_ms).max(0.0) as f64 / 1000.0
    }

    /// With a saved offset (clamped to the allowed range).
    pub fn new(offset_ms: f32) -> Self {
        Self {
            offset_ms: offset_ms.clamp(*OFFSET_RANGE.start(), *OFFSET_RANGE.end()),
            ..Default::default()
        }
    }

    /// Take Space presses as taps while the test listens for them (call
    /// before anything else reads the keyboard).
    pub fn capture_taps(&mut self, ctx: &egui::Context) {
        let Some(t) = self.test.as_mut() else { return };
        if t.step == Step::Check {
            return;
        }
        let tapped = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        if tapped && let Some((_, late)) = t.click.nearest_click(Instant::now()) {
            t.taps.push(late);
        }
        if t.taps.len() >= TAPS {
            let late = median(&t.taps);
            t.taps.clear();
            match t.step {
                Step::Flash => {
                    t.flash_late = Some(late);
                    t.step = Step::Click;
                    t.click.set_audible(true);
                }
                Step::Click => {
                    let flash = t.flash_late.unwrap_or(0.0);
                    let diff_ms = ((late - flash) * 1000.0) as f32;
                    self.offset_ms = diff_ms.clamp(*OFFSET_RANGE.start(), *OFFSET_RANGE.end());
                    self.result = Some(if diff_ms.abs() < 15.0 {
                        "Sound and picture reach you together. No offset needed.".into()
                    } else if diff_ms > 0.0 {
                        format!(
                            "The sound reaches you {diff_ms:.0} ms after the picture, so the \
                             pictures now wait {:.0} ms.",
                            self.offset_ms
                        )
                    } else {
                        format!(
                            "The picture reaches you {:.0} ms after the sound, so the pictures \
                             now come {:.0} ms early.",
                            -diff_ms, -self.offset_ms
                        )
                    });
                    t.step = Step::Check;
                }
                Step::Check => {}
            }
        }
        ctx.request_repaint();
    }

    /// The test dot, over the viewer.
    pub fn draw(&self, painter: &egui::Painter, rect: egui::Rect) {
        let Some(t) = &self.test else { return };
        let shift = match t.step {
            Step::Flash => 0.0,
            Step::Click => return,
            Step::Check => self.offset_ms as f64 / 1000.0,
        };
        let now = Instant::now();
        let at = if shift >= 0.0 {
            now.checked_sub(Duration::from_secs_f64(shift))
        } else {
            now.checked_add(Duration::from_secs_f64(-shift))
        };
        let Some((_, since)) = at.and_then(|a| t.click.nearest_click(a)) else {
            return;
        };
        let r = rect.width().min(rect.height()) * 0.08;
        let c = rect.center();
        painter.circle_stroke(c, r, egui::Stroke::new(1.5, Color32::from_gray(70)));
        if (0.0..FLASH_SECS).contains(&since) {
            painter.circle_filled(c, r, Color32::WHITE);
        }
    }

    /// The sync window (opened from the Media tab).
    pub fn window(&mut self, ctx: &egui::Context) {
        if !self.open {
            if self.test.is_some() {
                self.test = None;
            }
            return;
        }
        let mut open = self.open;
        egui::Window::new("Sound & picture sync")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| self.window_ui(ui));
        self.open = open;
        if self.test.is_some() {
            ctx.request_repaint();
        }
    }

    fn window_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Offset");
            ui.add(
                egui::Slider::new(&mut self.offset_ms, OFFSET_RANGE)
                    .suffix(" ms")
                    .step_by(1.0),
            );
            if ui.small_button("0").on_hover_text("No offset").clicked() {
                self.offset_ms = 0.0;
            }
        });
        ui.label(
            RichText::new(
                "+ = pictures later (speakers that play late, e.g. Bluetooth).\n\
                 − = pictures earlier (a projector or TV that shows late).",
            )
            .small()
            .weak(),
        );
        if self.offset_ms < 0.0 {
            ui.label(
                RichText::new(
                    "Early pictures: exact for songs. With a live input only the beat clock \
                     can run ahead (hits can't be seen before they happen).",
                )
                .small()
                .weak(),
            );
        }
        ui.separator();
        let Some(t) = self.test.as_mut() else {
            ui.label(
                "The test: tap Space along with a flashing dot, then along with a click. \
                 It works out the offset for this room.",
            );
            ui.label(
                RichText::new(
                    "Pause the music, use the speakers and screen you'll play on, and sit \
                     where the audience will be.",
                )
                .small()
                .weak(),
            );
            if let Some(e) = &self.error {
                ui.label(RichText::new(e).small().color(theme::error()));
            }
            if let Some(r) = &self.result {
                ui.label(RichText::new(r).small().color(theme::accent()));
            }
            if ui.button("▶ Start the sync test").clicked() {
                match ClickTrack::start(INTERVAL) {
                    Ok(click) => {
                        self.error = None;
                        self.result = None;
                        self.test = Some(Test {
                            click,
                            step: Step::Flash,
                            taps: Vec::new(),
                            flash_late: None,
                        });
                    }
                    Err(e) => self.error = Some(format!("Can't play the click: {e:#}")),
                }
            }
            return;
        };
        if t.click.has_failed() {
            self.error = Some("The audio output stopped (device unplugged?).".into());
            self.test = None;
            return;
        }
        let progress = |ui: &mut egui::Ui, n: usize| {
            ui.add(
                egui::ProgressBar::new(n as f32 / TAPS as f32)
                    .text(format!("{n} / {TAPS} taps"))
                    .desired_width(300.0),
            );
        };
        let mut stop = false;
        let mut again = false;
        match t.step {
            Step::Flash => {
                ui.label(RichText::new("Step 1 of 2: the picture").strong());
                ui.label(
                    "Watch the dot in the middle of the viewer. Press Space each time it \
                     lights up, in time with it.",
                );
                progress(ui, t.taps.len());
            }
            Step::Click => {
                ui.label(RichText::new("Step 2 of 2: the sound").strong());
                ui.label("Look away from the screen. Press Space on each click.");
                progress(ui, t.taps.len());
            }
            Step::Check => {
                if let Some(r) = &self.result {
                    ui.label(RichText::new(r).color(theme::accent()));
                }
                ui.label(
                    "Check it: the dot and the click should now feel like one. If not, move \
                     the offset slider until they do.",
                );
            }
        }
        ui.horizontal(|ui| {
            if t.step == Step::Check {
                stop = ui.button("✔ Done").clicked();
                again = ui.button("⟳ Test again").clicked();
            } else {
                stop = ui.button("Cancel").clicked();
            }
        });
        let lat = t.click.device_latency().as_secs_f32() * 1000.0;
        ui.label(
            RichText::new(format!(
                "Output: {} (it reports {lat:.0} ms of delay; Bluetooth hides more).",
                t.click.device_name()
            ))
            .small()
            .weak(),
        );
        if again {
            t.step = Step::Flash;
            t.taps.clear();
            t.flash_late = None;
            t.click.set_audible(false);
        }
        if stop {
            self.test = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_offsets_delay_the_signals() {
        let mut s = AvSync {
            offset_ms: 50.0,
            ..Default::default()
        };
        let mut f = SignalFrame {
            hop_index: 1,
            ..Default::default()
        };
        assert_eq!(s.frame(&f, false).hop_index, 1);
        f.hop_index = 2;
        // Too soon: still the old frame.
        assert_eq!(s.frame(&f, false).hop_index, 1);
        std::thread::sleep(Duration::from_millis(70));
        f.hop_index = 3;
        assert_eq!(s.frame(&f, false).hop_index, 2);
        // No offset: always the newest.
        s.offset_ms = 0.0;
        f.hop_index = 4;
        assert_eq!(s.frame(&f, false).hop_index, 4);
        assert_eq!(s.player_lead(), 0.0);
        s.offset_ms = -80.0;
        assert!((s.player_lead() - 0.08).abs() < 1e-9);
    }

    #[test]
    fn the_beat_clock_runs_ahead_for_live_input() {
        let mut f = SignalFrame::default();
        f.rhythm.tempo_locked = true;
        f.rhythm.bpm = 120.0;
        f.rhythm.beat_phase = 0.9;
        f.rhythm.bar_phase = 0.2;
        f.rhythm.beat.count = 10;
        f.rhythm.beat_in_bar = 3;
        f.rhythm.phrase_bars = 8;
        // 100 ms at 120 BPM = 0.2 beats: crosses into the next beat.
        predict_clock(&mut f, 0.1);
        assert!((f.rhythm.beat_phase - 0.1).abs() < 1e-5);
        assert_eq!(f.rhythm.beat.count, 11);
        assert_eq!(f.rhythm.beat_in_bar, 0);
        assert!((f.rhythm.bar_phase - 0.25).abs() < 1e-5);
        // Without a tempo nothing moves.
        let mut g = SignalFrame::default();
        g.rhythm.beat_phase = 0.5;
        predict_clock(&mut g, 0.1);
        assert_eq!(g.rhythm.beat_phase, 0.5);
    }
}
