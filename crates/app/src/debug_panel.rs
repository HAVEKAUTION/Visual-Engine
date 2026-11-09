//! The signal debug panel: live meters for every analysis signal.
//!
//! Meter scale: 0 at the bottom, [`METER_MAX`] at the top. The thin line marks
//! 1.0, which by design means "typical for this song".

use egui::ecolor::Hsva;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use signals::{Band, BandSignal, MEL_BANDS, SignalFrame};

/// Top of every meter.
pub const METER_MAX: f32 = 3.0;

const METER_BG: Color32 = Color32::from_rgb(28, 28, 36);
const TEXT: Color32 = Color32::from_gray(150);
const TEXT_DIM: Color32 = Color32::from_gray(95);
const REF_LINE: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 70);

/// Colour for a position `t ∈ [0, 1]` across the spectrum (low = red, high = violet).
fn spectrum_color(t: f32, value: f32) -> Color32 {
    Hsva::new(0.02 + 0.75 * t, 0.75, value, 1.0).into()
}

/// One vertical meter. Values above the scale show a bright cap.
fn meter(painter: &egui::Painter, rect: Rect, value: f32, color: Color32) {
    painter.rect_filled(rect, CornerRadius::same(2), METER_BG);
    let frac = (value / METER_MAX).clamp(0.0, 1.0);
    if frac > 0.0 {
        let top = rect.bottom() - rect.height() * frac;
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(rect.left(), top), rect.right_bottom()),
            CornerRadius::same(2),
            color,
        );
    }
    if value > METER_MAX {
        painter.rect_filled(
            Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), 3.0)),
            CornerRadius::ZERO,
            Color32::WHITE,
        );
    }
    let y = rect.bottom() - rect.height() / METER_MAX;
    painter.line_segment(
        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
        Stroke::new(1.0, REF_LINE),
    );
}

fn heading(painter: &egui::Painter, pos: Pos2, text: &str) {
    painter.text(
        pos,
        Align2::LEFT_TOP,
        text,
        FontId::proportional(12.0),
        TEXT,
    );
}

fn small(painter: &egui::Painter, pos: Pos2, align: Align2, text: String, color: Color32) {
    painter.text(pos, align, text, FontId::monospace(10.5), color);
}

/// Draw a fast/slow pair with a label and values underneath.
fn pair(painter: &egui::Painter, rect: Rect, name: &str, s: &BandSignal, color: Color32) {
    let gap = 3.0;
    let w = (rect.width() - gap) / 2.0;
    let meters_h = rect.height() - 44.0;
    let fast = Rect::from_min_size(rect.left_top(), Vec2::new(w, meters_h));
    let slow = Rect::from_min_size(
        rect.left_top() + Vec2::new(w + gap, 0.0),
        Vec2::new(w, meters_h),
    );
    meter(painter, fast, s.fast, color);
    meter(painter, slow, s.slow, color.gamma_multiply(0.45));
    let cx = rect.center().x;
    small(
        painter,
        Pos2::new(cx, fast.bottom() + 3.0),
        Align2::CENTER_TOP,
        name.to_string(),
        TEXT,
    );
    for (row, v) in [s.fast, s.slow].into_iter().enumerate() {
        painter.text(
            Pos2::new(cx, fast.bottom() + 17.0 + 12.0 * row as f32),
            Align2::CENTER_TOP,
            format!("{v:.2}"),
            FontId::monospace(9.5),
            TEXT_DIM,
        );
    }
}

/// Draw the whole panel into the available space.
pub fn show(
    ui: &mut egui::Ui,
    f: &SignalFrame,
    hops_per_sec: f32,
    rhythm: &crate::rhythm_panel::RhythmView,
) {
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    let painter = ui.painter_at(rect);
    let r = rect.shrink2(Vec2::new(16.0, 10.0));

    // Status line.
    small(
        &painter,
        r.left_top(),
        Align2::LEFT_TOP,
        format!(
            "hop {}  ·  t {:.1} s  ·  {:.1} hops/s  ·  line = 1.0 (typical)  ·  scale 0–{METER_MAX:.0}",
            f.hop_index, f.time, hops_per_sec
        ),
        TEXT_DIM,
    );

    // Rhythm row (onset lights, BPM, beat phase, counters, events).
    let rhythm_rect = Rect::from_min_size(
        r.left_top() + Vec2::new(0.0, 22.0),
        Vec2::new(r.width(), 110.0),
    );
    crate::rhythm_panel::draw(&painter, rhythm_rect, f, rhythm);

    let body = Rect::from_min_max(
        r.left_top() + Vec2::new(0.0, 22.0 + 110.0 + 34.0),
        r.right_bottom(),
    );
    let col_gap = 28.0;
    let mel_w = body.width() * 0.38;
    let named_w = body.width() * 0.36;
    let mel_rect = Rect::from_min_size(body.left_top(), Vec2::new(mel_w, body.height()));
    let named_rect = Rect::from_min_size(
        mel_rect.right_top() + Vec2::new(col_gap, 0.0),
        Vec2::new(named_w - col_gap, body.height()),
    );
    let extra_rect = Rect::from_min_max(
        named_rect.right_top() + Vec2::new(col_gap, 0.0),
        body.right_bottom(),
    );

    // --- 32 mel bands ---
    heading(
        &painter,
        mel_rect.left_top() + Vec2::new(0.0, -18.0),
        "MEL BANDS (fast)",
    );
    let bars = Rect::from_min_max(
        mel_rect.left_top(),
        mel_rect.right_bottom() - Vec2::new(0.0, 16.0),
    );
    let gap = 2.0;
    let w = (bars.width() - gap * (MEL_BANDS as f32 - 1.0)) / MEL_BANDS as f32;
    for (i, &v) in f.mel.iter().enumerate() {
        let x = bars.left() + i as f32 * (w + gap);
        let t = i as f32 / (MEL_BANDS as f32 - 1.0);
        meter(
            &painter,
            Rect::from_min_size(Pos2::new(x, bars.top()), Vec2::new(w, bars.height())),
            v,
            spectrum_color(t, 0.95),
        );
    }
    small(
        &painter,
        Pos2::new(bars.left(), bars.bottom() + 3.0),
        Align2::LEFT_TOP,
        "20 Hz".into(),
        TEXT_DIM,
    );
    small(
        &painter,
        Pos2::new(bars.right(), bars.bottom() + 3.0),
        Align2::RIGHT_TOP,
        "16 kHz".into(),
        TEXT_DIM,
    );

    // --- six named bands, fast | slow ---
    heading(
        &painter,
        named_rect.left_top() + Vec2::new(0.0, -18.0),
        "BANDS (fast | slow)",
    );
    let gap = 10.0;
    let w = (named_rect.width() - gap * 5.0) / 6.0;
    for (i, band) in Band::ALL.iter().enumerate() {
        let x = named_rect.left() + i as f32 * (w + gap);
        let t = (i as f32 + 0.5) / 6.0;
        pair(
            &painter,
            Rect::from_min_size(
                Pos2::new(x, named_rect.top()),
                Vec2::new(w, named_rect.height()),
            ),
            band.name(),
            f.band(*band),
            spectrum_color(t, 0.95),
        );
    }

    // --- loudness and centroid ---
    heading(
        &painter,
        extra_rect.left_top() + Vec2::new(0.0, -18.0),
        "LOUDNESS · CENTROID",
    );
    let lw = (extra_rect.width() * 0.32).min(70.0);
    let loud_rect = Rect::from_min_size(extra_rect.left_top(), Vec2::new(lw, extra_rect.height()));
    pair(
        &painter,
        loud_rect,
        "loud",
        &f.loudness,
        Color32::from_rgb(230, 230, 240),
    );

    let c = Rect::from_min_max(
        Pos2::new(loud_rect.right() + 18.0, extra_rect.top()),
        extra_rect.right_bottom(),
    );
    let hue_color: Color32 = Hsva::new(f.centroid_hue * 0.8, 0.8, 0.95, 1.0).into();
    let mut y = c.top();
    let mut line = |text: String, color: Color32, size: f32| {
        painter.text(
            Pos2::new(c.left(), y),
            Align2::LEFT_TOP,
            text,
            FontId::monospace(size),
            color,
        );
        y += size + 6.0;
    };
    line(
        format!("{:.1} dBFS", f.loudness.energy_db),
        Color32::WHITE,
        12.5,
    );
    line("loudness (RMS)".into(), TEXT_DIM, 10.5);
    line(format!("{:.0} Hz", f.centroid_hz), Color32::WHITE, 12.5);
    line("centroid".into(), TEXT_DIM, 10.5);
    line(format!("hue {:.2}", f.centroid_hue), TEXT, 12.0);

    // Hue swatch plus a strip showing where the centroid sits on the scale.
    let strip = Rect::from_min_size(
        Pos2::new(c.left(), y + 4.0),
        Vec2::new(c.width().min(160.0), 10.0),
    );
    let steps = 32;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let seg = Rect::from_min_size(
            Pos2::new(strip.left() + strip.width() * t0, strip.top()),
            Vec2::new(strip.width() / steps as f32 + 0.5, strip.height()),
        );
        painter.rect_filled(
            seg,
            CornerRadius::ZERO,
            Color32::from(Hsva::new(t0 * 0.8, 0.8, 0.8, 1.0)),
        );
    }
    let mx = strip.left() + strip.width() * f.centroid_hue;
    painter.line_segment(
        [
            Pos2::new(mx, strip.top() - 3.0),
            Pos2::new(mx, strip.bottom() + 3.0),
        ],
        Stroke::new(2.0, Color32::WHITE),
    );
    let swatch = Rect::from_min_size(
        Pos2::new(c.left(), strip.bottom() + 10.0),
        Vec2::new(
            strip.width(),
            (c.bottom() - strip.bottom() - 10.0).clamp(0.0, 40.0),
        ),
    );
    painter.rect_filled(swatch, CornerRadius::same(3), hue_color);
}
