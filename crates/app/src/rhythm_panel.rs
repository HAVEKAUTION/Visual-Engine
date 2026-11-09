//! Rhythm and structure part of the debug panel: onset lights, BPM, beat-phase
//! circle, bar/phrase counters and build meter. Also keeps the event log, which
//! the side panel shows.

use std::collections::VecDeque;
use std::time::Instant;

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, Vec2};
use signals::{SignalFrame, Trigger};

const TEXT: Color32 = Color32::from_gray(150);
const TEXT_DIM: Color32 = Color32::from_gray(95);
const RING: Color32 = Color32::from_rgb(45, 45, 58);
const ACCENT: Color32 = Color32::from_rgb(90, 200, 255);
const BUILD_COLOR: Color32 = Color32::from_rgb(255, 170, 60);
const DROP_COLOR: Color32 = Color32::from_rgb(255, 80, 120);
const SECTION_COLOR: Color32 = Color32::from_rgb(170, 140, 255);

/// How long an onset light glows (s).
const FLASH_SECS: f32 = 0.15;
const LOG_LEN: usize = 200;

/// The four onset lights: label, colour.
const LIGHTS: [(&str, Color32); 4] = [
    ("ONSET", Color32::from_rgb(230, 230, 240)),
    ("KICK", Color32::from_rgb(255, 120, 70)),
    ("SNARE", Color32::from_rgb(120, 230, 140)),
    ("HAT", Color32::from_rgb(170, 140, 255)),
];

/// An entry in the event log.
pub struct LogEntry {
    pub text: String,
    pub color: Color32,
}

/// UI-side state: remembers trigger counts to flash lights and log events.
/// (The UI reads frames at its own rate, so it watches `count`, not `fired`.)
#[derive(Default)]
pub struct RhythmView {
    seen: [u32; 5],
    flash: [Option<Instant>; 5],
    seen_events: [u32; 3],
    last_hop: u64,
    pub log: VecDeque<LogEntry>,
}

fn fmt_secs(t: f64) -> String {
    let t = t.max(0.0);
    format!("{}:{:04.1}", (t / 60.0) as u64, t % 60.0)
}

impl RhythmView {
    /// Call once per UI frame with the newest frame. `song_pos` is the song
    /// playback position, if a file is playing (for the log).
    pub fn update(&mut self, f: &SignalFrame, song_pos: Option<f64>) {
        if f.hop_index < self.last_hop {
            // New source: counts restarted.
            self.seen = [0; 5];
            self.seen_events = [0; 3];
            self.push("— new source —".into(), TEXT_DIM);
        }
        self.last_hop = f.hop_index;

        let r = &f.rhythm;
        let triggers: [&Trigger; 5] = [&r.onset, &r.kick, &r.snare, &r.hat, &r.beat];
        for (i, t) in triggers.iter().enumerate() {
            if t.count != self.seen[i] {
                self.seen[i] = t.count;
                self.flash[i] = Some(Instant::now());
            }
        }

        let s = &f.structure;
        let events: [(&Trigger, &str, Color32); 3] = [
            (&s.build, "BUILD", BUILD_COLOR),
            (&s.drop, "DROP", DROP_COLOR),
            (&s.section_change, "SECTION CHANGE", SECTION_COLOR),
        ];
        for (i, (t, name, color)) in events.into_iter().enumerate() {
            if t.count != self.seen_events[i] {
                self.seen_events[i] = t.count;
                let detail = match i {
                    1 => format!(" (bass ×{:.1})", t.strength),
                    2 => format!(" (distance {:.2})", t.strength),
                    _ => String::new(),
                };
                // Song position when the event *happened* (it can be detected
                // a little later, e.g. a drop is confirmed one beat after).
                let song = song_pos
                    .map(|p| format!("  song {}", fmt_secs(p - (f.time - t.last_time))))
                    .unwrap_or_default();
                let text = format!("t {}{song}  {name}{detail}", fmt_secs(t.last_time));
                log::info!("event: {text}");
                self.push(text, color);
            }
        }
    }

    fn push(&mut self, text: String, color: Color32) {
        self.log.push_front(LogEntry { text, color });
        self.log.truncate(LOG_LEN);
    }

    /// 0..1 glow of light `i`.
    fn glow(&self, i: usize) -> f32 {
        self.flash[i]
            .map(|t| (1.0 - t.elapsed().as_secs_f32() / FLASH_SECS).max(0.0))
            .unwrap_or(0.0)
    }
}

fn text(p: &Painter, pos: Pos2, align: Align2, s: impl ToString, size: f32, color: Color32) {
    p.text(pos, align, s.to_string(), FontId::monospace(size), color);
}

/// Draw the rhythm row into `rect`.
pub fn draw(p: &Painter, rect: Rect, f: &SignalFrame, view: &RhythmView) {
    let r = &f.rhythm;
    let mut x = rect.left();
    let cy = rect.center().y;

    // --- onset lights ---
    for (i, (name, color)) in LIGHTS.iter().enumerate() {
        let c = Pos2::new(x + 22.0, cy - 12.0);
        let g = view.glow(i);
        p.circle_filled(c, 15.0, RING);
        if g > 0.0 {
            p.circle_filled(c, 15.0, color.gamma_multiply(0.25 + 0.75 * g));
        }
        p.circle_stroke(c, 15.0, Stroke::new(1.0, color.gamma_multiply(0.5)));
        text(
            p,
            c + Vec2::new(0.0, 22.0),
            Align2::CENTER_TOP,
            name,
            10.0,
            TEXT,
        );
        let count = [r.onset.count, r.kick.count, r.snare.count, r.hat.count][i];
        text(
            p,
            c + Vec2::new(0.0, 35.0),
            Align2::CENTER_TOP,
            count,
            9.5,
            TEXT_DIM,
        );
        x += 50.0;
    }
    x += 16.0;

    // --- BPM ---
    let bpm_color = if r.tempo_locked {
        Color32::WHITE
    } else {
        TEXT_DIM
    };
    text(
        p,
        Pos2::new(x, rect.top() + 4.0),
        Align2::LEFT_TOP,
        format!("{:.1}", r.bpm),
        28.0,
        bpm_color,
    );
    text(
        p,
        Pos2::new(x + 92.0, rect.top() + 16.0),
        Align2::LEFT_TOP,
        "BPM",
        11.0,
        TEXT,
    );
    let status = if r.tempo_manual {
        "TAPPED (manual)".to_string()
    } else if r.tempo_locked {
        format!("auto · confidence {:.2}", r.tempo_confidence)
    } else {
        "searching…".to_string()
    };
    text(
        p,
        Pos2::new(x, rect.top() + 42.0),
        Align2::LEFT_TOP,
        status,
        10.0,
        if r.tempo_manual { BUILD_COLOR } else { TEXT },
    );
    text(
        p,
        Pos2::new(x, rect.top() + 58.0),
        Align2::LEFT_TOP,
        format!("onsets {:.1}/s", r.onset_rate),
        10.0,
        TEXT_DIM,
    );
    text(
        p,
        Pos2::new(x, rect.top() + 80.0),
        Align2::LEFT_TOP,
        "T = tap tempo",
        10.0,
        TEXT_DIM,
    );
    text(
        p,
        Pos2::new(x, rect.top() + 93.0),
        Align2::LEFT_TOP,
        "R = this beat is 1",
        10.0,
        TEXT_DIM,
    );
    x += 170.0;

    // --- beat phase circle ---
    let radius = (rect.height() * 0.5 - 6.0).min(46.0);
    let c = Pos2::new(x + radius, cy);
    let beat_glow = view.glow(4);
    p.circle_filled(c, radius, Color32::from_rgb(22, 22, 30));
    if beat_glow > 0.0 {
        p.circle_filled(c, radius, ACCENT.gamma_multiply(0.35 * beat_glow));
    }
    p.circle_stroke(c, radius, Stroke::new(2.0, RING));
    // Arc showing progress through the beat.
    let steps = 48;
    let done = (r.beat_phase * steps as f32) as usize;
    let pts: Vec<Pos2> = (0..=done)
        .map(|i| {
            let a = i as f32 / steps as f32 * std::f32::consts::TAU;
            c + Vec2::new(a.sin(), -a.cos()) * radius
        })
        .collect();
    if pts.len() > 1 {
        p.add(egui::Shape::line(pts, Stroke::new(3.0, ACCENT)));
    }
    let a = r.beat_phase * std::f32::consts::TAU;
    let tip = c + Vec2::new(a.sin(), -a.cos()) * radius;
    p.line_segment([c, tip], Stroke::new(1.5, ACCENT.gamma_multiply(0.6)));
    p.circle_filled(tip, 4.5, Color32::WHITE);
    text(
        p,
        c,
        Align2::CENTER_CENTER,
        r.beat_in_bar + 1,
        20.0,
        Color32::WHITE,
    );
    x += 2.0 * radius + 22.0;

    // --- bar / phrase / build ---
    let col_w = (rect.right() - x).clamp(120.0, 220.0);
    text(
        p,
        Pos2::new(x, rect.top() + 4.0),
        Align2::LEFT_TOP,
        format!(
            "beat {}/4 · bar {}/{}",
            r.beat_in_bar + 1,
            r.bar_in_phrase + 1,
            r.phrase_bars
        ),
        11.0,
        Color32::WHITE,
    );
    // Four beat boxes.
    for i in 0..4u32 {
        let b = Rect::from_min_size(
            Pos2::new(x + i as f32 * 26.0, rect.top() + 22.0),
            Vec2::new(22.0, 12.0),
        );
        let on = i == r.beat_in_bar;
        p.rect_filled(b, CornerRadius::same(2), if on { ACCENT } else { RING });
    }
    // Phrase progress.
    let bar = Rect::from_min_size(
        Pos2::new(x, rect.top() + 44.0),
        Vec2::new(col_w - 10.0, 8.0),
    );
    p.rect_filled(bar, CornerRadius::same(2), RING);
    p.rect_filled(
        Rect::from_min_size(
            bar.min,
            Vec2::new(bar.width() * r.phrase_phase, bar.height()),
        ),
        CornerRadius::same(2),
        ACCENT.gamma_multiply(0.7),
    );
    text(
        p,
        Pos2::new(x, rect.top() + 56.0),
        Align2::LEFT_TOP,
        format!(
            "phrase {:.0}% · bar #{}",
            r.phrase_phase * 100.0,
            r.bar_count
        ),
        10.0,
        TEXT_DIM,
    );
    // Build meter.
    let s = &f.structure;
    let b = Rect::from_min_size(
        Pos2::new(x, rect.top() + 80.0),
        Vec2::new(col_w - 10.0, 10.0),
    );
    p.rect_filled(b, CornerRadius::same(2), RING);
    p.rect_filled(
        Rect::from_min_size(b.min, Vec2::new(b.width() * s.build_amount, b.height())),
        CornerRadius::same(2),
        BUILD_COLOR,
    );
    text(
        p,
        Pos2::new(x, rect.top() + 93.0),
        Align2::LEFT_TOP,
        if s.building { "BUILDING" } else { "build" },
        10.0,
        if s.building { BUILD_COLOR } else { TEXT_DIM },
    );
}
