//! The main window: side panel with audio sources, central preview area.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use audio::{AudioEngine, InputDevice, Song, Source};
use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::analysis_thread::{self, Command, WaveformSnapshot};
use crate::debug_panel;
use crate::offline::{self, OfflineReport};
use crate::rhythm_panel::RhythmView;

const PREVIEW_BG: Color32 = Color32::from_rgb(8, 8, 12);
const PANEL_BG: Color32 = Color32::from_rgb(20, 20, 26);
const WAVE_COLOR: Color32 = Color32::from_rgb(90, 200, 255);
const ERROR_COLOR: Color32 = Color32::from_rgb(255, 110, 110);

pub struct VisualEngineApp {
    engine: AudioEngine,
    waveform: triple_buffer::Output<WaveformSnapshot>,
    signals: signals::SignalReader,
    commands: std::sync::mpsc::Sender<Command>,
    rhythm_view: RhythmView,
    /// Offline analysis in progress: (file name, result channel).
    offline_job: Option<(String, Receiver<anyhow::Result<OfflineReport>>)>,
    offline_result: Option<Result<OfflineReport, String>>,
    show_meters: bool,
    /// For the hops-per-second readout: (hop index, when) of the last sample.
    hop_rate_mark: (u64, Instant),
    hops_per_sec: f32,
    loading: Option<(String, Receiver<anyhow::Result<Song>>)>,
    input_devices: Vec<InputDevice>,
    selected_input: Option<usize>,
    volume: f32,
    /// Seek bar value while the user is dragging it.
    seek_drag: Option<f64>,
    error: Option<String>,
    gpu_info: String,
}

impl VisualEngineApp {
    pub fn new(cc: &eframe::CreationContext<'_>, startup_song: Option<std::path::PathBuf>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = PANEL_BG;
        visuals.window_fill = PANEL_BG;
        cc.egui_ctx.set_visuals(visuals);

        let gpu_info = match &cc.wgpu_render_state {
            Some(rs) => {
                let info = rs.adapter.get_info();
                format!("{} ({:?})", info.name, info.backend)
            }
            None => "unknown".into(),
        };
        log::info!("GPU: {gpu_info}");

        let (engine, feeds) = AudioEngine::new();
        let (input, output) = triple_buffer::triple_buffer(&WaveformSnapshot::default());
        // The thread stops by itself when the engine (and its sender) is dropped.
        let (publisher, reader) = signals::bus();
        let (commands, command_rx) = std::sync::mpsc::channel();
        let _ = analysis_thread::spawn(feeds, input, publisher, command_rx);

        let mut app = Self {
            engine,
            waveform: output,
            signals: reader,
            commands,
            rhythm_view: RhythmView::default(),
            offline_job: None,
            offline_result: None,
            show_meters: true,
            hop_rate_mark: (0, Instant::now()),
            hops_per_sec: 0.0,
            loading: None,
            input_devices: Vec::new(),
            selected_input: None,
            volume: 0.8,
            seek_drag: None,
            error: None,
            gpu_info,
        };
        app.rescan_inputs();
        if let Some(path) = startup_song {
            app.load_song(path);
        }
        app
    }

    fn rescan_inputs(&mut self) {
        let previous = self
            .selected_input
            .and_then(|i| self.input_devices.get(i))
            .map(|d| d.name.clone());
        self.input_devices = audio::list_input_devices();
        self.selected_input = previous
            .and_then(|name| self.input_devices.iter().position(|d| d.name == name))
            .or_else(|| self.input_devices.iter().position(|d| d.is_default))
            .or(if self.input_devices.is_empty() {
                None
            } else {
                Some(0)
            });
        log::info!("found {} input device(s)", self.input_devices.len());
    }

    fn open_song_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Open song")
            .add_filter("Audio", audio::decode::SUPPORTED_EXTENSIONS)
            .pick_file();
        if let Some(path) = picked {
            self.load_song(path);
        }
    }

    /// Start loading a song in the background; it plays when ready.
    pub fn load_song(&mut self, path: std::path::PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        log::info!("loading {}", path.display());
        self.error = None;
        self.loading = Some((name, audio::load_song_in_background(path)));
    }

    fn poll_loading(&mut self) {
        let Some((name, rx)) = &self.loading else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("the loader stopped unexpectedly"))
            }
        };
        let name = name.clone();
        self.loading = None;
        match result.and_then(|song| self.engine.start_song(song, self.volume)) {
            Ok(()) => {
                self.seek_drag = None;
                if let Source::File(p) = self.engine.source() {
                    p.play();
                }
            }
            Err(e) => {
                log::error!("could not load {name}: {e:#}");
                self.error = Some(format!("Could not load {name}: {e:#}"));
            }
        }
    }

    fn check_stream_errors(&mut self) {
        let failed = match self.engine.source() {
            Source::File(p) => p.has_failed(),
            Source::Live(l) => l.has_failed(),
            Source::None => false,
        };
        if failed {
            log::error!("audio stream error; stopping the source");
            self.error = Some(
                "The audio device reported an error (was it unplugged?). Source stopped.".into(),
            );
            self.engine.stop();
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("Visual Engine");
        ui.label(RichText::new("Milestone 1 · analysis + rhythm").weak());
        ui.add_space(12.0);

        // ---------------- Song file ----------------
        ui.label(RichText::new("SONG FILE").strong());
        ui.separator();
        ui.horizontal(|ui| {
            let busy = self.loading.is_some();
            if ui
                .add_enabled(!busy, egui::Button::new("📂 Open song…"))
                .clicked()
            {
                self.open_song_dialog();
            }
            if let Some((name, _)) = &self.loading {
                ui.spinner();
                ui.label(format!("Loading {name}…"));
            }
        });

        let mut seek_to = None;
        if let Source::File(player) = self.engine.source() {
            ui.add_space(6.0);
            ui.label(RichText::new(&player.song().name).color(Color32::WHITE));
            ui.label(
                RichText::new(format!(
                    "{:.1} kHz, {} ch → {} ({:.1} kHz)",
                    player.song().source_rate as f32 / 1000.0,
                    player.song().source_channels,
                    player.device_name(),
                    player.song().output_rate as f32 / 1000.0,
                ))
                .small()
                .weak(),
            );

            ui.horizontal(|ui| {
                let label = if player.is_playing() {
                    "⏸ Pause"
                } else {
                    "▶ Play"
                };
                if ui.button(label).clicked() {
                    player.toggle();
                }
                let pos = self.seek_drag.unwrap_or_else(|| player.position_secs());
                ui.label(format!(
                    "{} / {}",
                    fmt_time(pos),
                    fmt_time(player.duration_secs())
                ));
            });

            // Seek bar: follow playback, but while the user holds the handle
            // keep their value and only seek when they let go.
            let mut pos = self.seek_drag.unwrap_or_else(|| player.position_secs());
            ui.spacing_mut().slider_width = ui.available_width() - 8.0;
            let resp = ui.add(
                egui::Slider::new(&mut pos, 0.0..=player.duration_secs().max(0.001))
                    .show_value(false),
            );
            if resp.changed() {
                self.seek_drag = Some(pos);
            }
            if self.seek_drag.is_some() && !resp.is_pointer_button_down_on() {
                seek_to = self.seek_drag.take();
            }

            ui.spacing_mut().slider_width = ui.available_width() - 80.0;
            ui.horizontal(|ui| {
                ui.label("Volume");
                let mut v = self.volume;
                if ui
                    .add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    self.volume = v;
                    player.set_volume(v);
                }
                ui.label(format!("{:.0}%", self.volume * 100.0));
            });
        }
        if let (Some(secs), Source::File(player)) = (seek_to, self.engine.source()) {
            player.seek_secs(secs);
        }

        ui.add_space(18.0);

        // ---------------- Live input ----------------
        ui.label(RichText::new("LIVE INPUT").strong());
        ui.separator();
        let selected_text = self
            .selected_input
            .and_then(|i| self.input_devices.get(i))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "No input devices found".into());
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("input-device")
                .width(ui.available_width() - 40.0)
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (i, d) in self.input_devices.iter().enumerate() {
                        let label = if d.is_default {
                            format!("{} (default)", d.name)
                        } else {
                            d.name.clone()
                        };
                        ui.selectable_value(&mut self.selected_input, Some(i), label);
                    }
                });
            if ui.button("⟳").on_hover_text("Rescan devices").clicked() {
                self.rescan_inputs();
            }
        });

        let live_active = matches!(self.engine.source(), Source::Live(_));
        ui.horizontal(|ui| {
            let can_start = self.selected_input.is_some();
            if ui
                .add_enabled(can_start, egui::Button::new("🎤 Start input"))
                .clicked()
                && let Some(device) = self
                    .selected_input
                    .and_then(|i| self.input_devices.get(i))
                    .cloned()
            {
                self.error = None;
                if let Err(e) = self.engine.start_live(&device) {
                    log::error!("could not start {}: {e:#}", device.name);
                    self.error = Some(format!("Could not start {}: {e:#}", device.name));
                }
            }
            if ui
                .add_enabled(live_active, egui::Button::new("⏹ Stop"))
                .clicked()
            {
                self.engine.stop();
            }
        });
        if let Source::Live(live) = self.engine.source() {
            ui.label(
                RichText::new(format!(
                    "Listening: {} ({:.1} kHz, {} ch → 48 kHz mono)",
                    live.device_name,
                    live.sample_rate as f32 / 1000.0,
                    live.channels
                ))
                .color(WAVE_COLOR),
            );
        }

        ui.add_space(18.0);
        ui.label(RichText::new("ANALYSIS").strong());
        ui.separator();
        ui.checkbox(&mut self.show_meters, "Show signal meters");
        ui.horizontal(|ui| {
            if ui
                .button("Tap (T)")
                .on_hover_text("Tap along with the beat")
                .clicked()
            {
                self.send(Command::Tap);
            }
            if ui
                .button("Downbeat (R)")
                .on_hover_text("Press on beat 1 of a bar: makes it bar 1, beat 1")
                .clicked()
            {
                self.send(Command::ResyncDownbeat);
            }
            let manual = self.signals.latest().rhythm.tempo_manual;
            if ui
                .add_enabled(manual, egui::Button::new("Auto tempo"))
                .on_hover_text("Forget the tapped tempo")
                .clicked()
            {
                self.send(Command::AutoTempo);
            }
        });

        ui.add_space(6.0);
        let busy = self.offline_job.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new("\u{1F4CA} Analyse file offline\u{2026}"),
                )
                .on_hover_text("Analyse a whole song as fast as possible and save a CSV next to it")
                .clicked()
            {
                self.start_offline_analysis();
            }
            if let Some((name, _)) = &self.offline_job {
                ui.spinner();
                ui.label(format!("Analysing {name}\u{2026}"));
            }
        });
        let mut reveal = None;
        match &self.offline_result {
            Some(Ok(report)) => {
                ui.label(
                    RichText::new(format!(
                        "Saved {}",
                        report
                            .csv_path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ))
                    .color(WAVE_COLOR),
                );
                ui.label(RichText::new(report.summary()).small());
                if ui.small_button("Show in folder").clicked() {
                    reveal = Some(report.csv_path.clone());
                }
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(ERROR_COLOR));
            }
            None => {}
        }
        if let Some(path) = reveal {
            offline::reveal_in_file_manager(&path);
        }

        ui.add_space(14.0);
        ui.label(RichText::new("EVENTS").strong());
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(170.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if self.rhythm_view.log.is_empty() {
                    ui.label(
                        RichText::new("Builds, drops and section changes appear here.")
                            .small()
                            .weak(),
                    );
                }
                for e in &self.rhythm_view.log {
                    ui.label(RichText::new(&e.text).monospace().small().color(e.color));
                }
            });

        if let Some(err) = &self.error {
            ui.add_space(12.0);
            ui.label(RichText::new(err).color(ERROR_COLOR));
        }

        // ---------------- Footer ----------------
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("GPU: {}", self.gpu_info))
                    .small()
                    .weak(),
            );
            let snap = self.waveform.read();
            ui.label(
                RichText::new(format!(
                    "Analysis feed: 48 kHz mono, {:.1} s received",
                    snap.samples_received as f64 / audio::ANALYSIS_SAMPLE_RATE as f64
                ))
                .small()
                .weak(),
            );
        });
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, PREVIEW_BG);

        let snap = self.waveform.read();
        let has_source = !matches!(self.engine.source(), Source::None);

        // Waveform band across the middle of the preview.
        let band = Rect::from_center_size(
            rect.center(),
            Vec2::new(rect.width() - 48.0, (rect.height() * 0.5).min(420.0)),
        );
        let mid = band.center().y;
        painter.line_segment(
            [Pos2::new(band.left(), mid), Pos2::new(band.right(), mid)],
            Stroke::new(1.0, Color32::from_gray(40)),
        );

        let n = snap.columns.len();
        if n > 0 {
            let half = band.height() * 0.5;
            let stroke = Stroke::new(1.5, WAVE_COLOR);
            for (i, [lo, hi]) in snap.columns.iter().enumerate() {
                let x = band.left() + band.width() * (i as f32 + 0.5) / n as f32;
                let y_top = mid - hi.clamp(-1.0, 1.0) * half;
                let y_bot = mid - lo.clamp(-1.0, 1.0) * half;
                // Always draw at least a 1 px line so silence is visible.
                let (y0, y1) = if (y_bot - y_top) < 1.0 {
                    (mid - 0.5, mid + 0.5)
                } else {
                    (y_top, y_bot)
                };
                painter.line_segment([Pos2::new(x, y0), Pos2::new(x, y1)], stroke);
            }
        }

        let label_color = Color32::from_gray(120);
        painter.text(
            rect.left_top() + Vec2::new(16.0, 14.0),
            egui::Align2::LEFT_TOP,
            format!(
                "Waveform · last {:.0} s · 48 kHz mono feed · peak {:.2}",
                analysis_thread::HISTORY_SECS,
                snap.peak
            ),
            egui::FontId::proportional(13.0),
            label_color,
        );
        if !has_source {
            painter.text(
                band.center_bottom() + Vec2::new(0.0, 28.0),
                egui::Align2::CENTER_TOP,
                "No audio source. Open a song or start a live input on the left.",
                egui::FontId::proportional(15.0),
                label_color,
            );
        }
    }
}

impl VisualEngineApp {
    fn send(&self, cmd: Command) {
        let _ = self.commands.send(cmd);
    }

    fn start_offline_analysis(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Analyse file offline")
            .add_filter("Audio", audio::decode::SUPPORTED_EXTENSIONS)
            .pick_file();
        if let Some(path) = picked {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            log::info!("offline analysis of {}", path.display());
            self.offline_result = None;
            self.offline_job = Some((name, offline::analyse_in_background(path)));
        }
    }

    fn poll_offline(&mut self) {
        let Some((name, rx)) = &self.offline_job else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("the analysis stopped unexpectedly"))
            }
        };
        let name = name.clone();
        self.offline_job = None;
        self.offline_result = Some(match result {
            Ok(report) => {
                log::info!("offline analysis of {name}: {}", report.summary());
                Ok(report)
            }
            Err(e) => {
                log::error!("offline analysis of {name} failed: {e:#}");
                Err(format!("Could not analyse {name}: {e:#}"))
            }
        });
    }

    /// Measure how many analysis hops arrive per second (should be ~93.75).
    fn update_hop_rate(&mut self, hop_index: u64) {
        let (last_hop, since) = self.hop_rate_mark;
        if hop_index < last_hop {
            // Source changed and the count restarted.
            self.hop_rate_mark = (hop_index, Instant::now());
            return;
        }
        let elapsed = since.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            self.hops_per_sec = (hop_index - last_hop) as f32 / elapsed;
            self.hop_rate_mark = (hop_index, Instant::now());
        }
    }
}

impl eframe::App for VisualEngineApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_loading();
        self.poll_offline();
        self.check_stream_errors();
        // The preview is live, so keep redrawing.
        ctx.request_repaint();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("sources")
            .resizable(true)
            .default_size(320.0)
            .size_range(260.0..=520.0)
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(PANEL_BG)
                    .inner_margin(12.0),
            )
            .show(ui, |ui| self.side_panel(ui));

        // Manual rhythm controls (keyboard).
        let (tap, resync) =
            ui.input(|i| (i.key_pressed(egui::Key::T), i.key_pressed(egui::Key::R)));
        if tap {
            self.send(Command::Tap);
        }
        if resync {
            self.send(Command::ResyncDownbeat);
        }

        let frame = self.signals.latest().clone();
        let song_pos = match self.engine.source() {
            Source::File(p) => Some(p.position_secs()),
            _ => None,
        };
        self.rhythm_view.update(&frame, song_pos);
        if self.show_meters {
            self.update_hop_rate(frame.hop_index);
            let rate = self.hops_per_sec;
            let view = &self.rhythm_view;
            egui::Panel::bottom("signals")
                .resizable(true)
                .default_size(440.0)
                .size_range(340.0..=700.0)
                .frame(egui::Frame::NONE.fill(PANEL_BG))
                .show(ui, |ui| debug_panel::show(ui, &frame, rate, view));
        }

        egui::CentralPanel::no_frame().show(ui, |ui| self.preview(ui));
    }
}

/// `m:ss`
fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::fmt_time;

    #[test]
    fn formats_times() {
        assert_eq!(fmt_time(0.0), "0:00");
        assert_eq!(fmt_time(65.9), "1:05");
        assert_eq!(fmt_time(-3.0), "0:00");
    }
}
