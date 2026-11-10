//! The main window, one workspace (DIRECTION_UPDATE_v2.md, UI): toolbar on
//! top, signal strip at the bottom, and between them dockable panels (see
//! `dock.rs`): Viewer, Node Graph, Inspector, Sources, Media, Looks,
//! Analysis, Meters. Drag a tab to move a panel; the layout is remembered.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use audio::{AudioEngine, InputDevice, Song, Source};
use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::analysis_thread::{self, Command, WaveformSnapshot};
use crate::debug_panel;
use crate::dock::{self, Layout, Pane};
use crate::graph_view::ViewerMode;
use crate::offline::{self, OfflineReport};
use crate::rhythm_panel::RhythmView;
use crate::scene_view::SceneView;
use crate::theme;

const PREVIEW_BG: Color32 = Color32::from_rgb(8, 8, 10);
const PANEL_BG: Color32 = theme::PANEL;

/// Layout and view settings remembered between sessions.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Prefs {
    /// The panel layout (None = the Build layout).
    dock: Option<egui_tiles::Tree<Pane>>,
    layout: Layout,
    show_strip: bool,
    viewer: ViewerMode,
    pip: bool,
    snap: bool,
    thumbnails: bool,
    show_fps: bool,
    show_cost: bool,
    node_scale: f32,
    media_pool: Vec<std::path::PathBuf>,
    /// Sound / picture offset (ms, + = pictures later).
    av_offset_ms: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            dock: None,
            layout: Layout::Build,
            show_strip: true,
            viewer: ViewerMode::Program,
            pip: true,
            snap: true,
            thumbnails: true,
            show_fps: true,
            show_cost: true,
            node_scale: crate::graph_editor::DEFAULT_NODE_SCALE,
            media_pool: Vec::new(),
            av_offset_ms: 0.0,
        }
    }
}

const PREFS_KEY: &str = "workspace";
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
    scene: SceneView,
    fullscreen: bool,
    show_waveform: bool,
    show_fps: bool,
    /// The dockable panels.
    dock: egui_tiles::Tree<Pane>,
    /// The ready-made layout last picked (shown in the toolbar).
    layout: Layout,
    show_strip: bool,
    /// The graph panel is on screen this frame.
    graph_shown: bool,
    sync: crate::av_sync::AvSync,
}

impl VisualEngineApp {
    pub fn new(cc: &eframe::CreationContext<'_>, startup_song: Option<std::path::PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        let prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, PREFS_KEY))
            .unwrap_or_default();

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
            hop_rate_mark: (0, Instant::now()),
            hops_per_sec: 0.0,
            loading: None,
            input_devices: Vec::new(),
            selected_input: None,
            volume: 0.8,
            seek_drag: None,
            error: None,
            gpu_info,
            scene: SceneView::new(cc),
            fullscreen: false,
            show_waveform: false,
            show_fps: prefs.show_fps,
            dock: prefs
                .dock
                .filter(dock::is_complete)
                .unwrap_or_else(|| dock::build(prefs.layout)),
            layout: prefs.layout,
            show_strip: prefs.show_strip,
            graph_shown: true,
            sync: crate::av_sync::AvSync::new(prefs.av_offset_ms),
        };
        app.scene.ws.viewer = prefs.viewer;
        app.scene.ws.pip = prefs.pip;
        app.scene.ws.editor.snap = prefs.snap;
        app.scene.ws.editor.thumbnails = prefs.thumbnails;
        app.scene.ws.editor.show_cost = prefs.show_cost;
        for p in &prefs.media_pool {
            app.scene.ws.pool.add(p);
        }
        app.scene.ws.editor.node_scale = prefs.node_scale.clamp(
            *crate::graph_editor::NODE_SCALE.start(),
            *crate::graph_editor::NODE_SCALE.end(),
        );
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

    /// Is a panel shown?
    fn shown(&self, pane: Pane) -> bool {
        dock::is_shown(&self.dock, pane)
    }

    fn set_shown(&mut self, pane: Pane, on: bool) {
        dock::set_shown(&mut self.dock, pane, on);
    }

    fn media_panel(&mut self, ui: &mut egui::Ui) {
        if !self.scene.graph_mode {
            ui.label(
                RichText::new("Switch the Inspector to Node graph to use pictures and videos.")
                    .weak(),
            );
            return;
        }
        self.scene.ws.pool_ui(ui);
    }

    fn looks_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("No Looks yet.")
                    .size(16.0)
                    .color(Color32::from_gray(200)),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Looks are hand-made signature visuals you can switch, mutate and \
                     breed. They arrive in M6. Until then, build your own in the graph, or \
                     right-click it → Add compositing demo.",
                )
                .weak(),
            );
        });
    }

    fn heading(ui: &mut egui::Ui, text: &str) {
        ui.add_space(4.0);
        ui.label(RichText::new(text).strong().size(12.0).color(theme::ACCENT));
        ui.separator();
    }

    /// Where the sound comes from (song, live input, computer audio), the
    /// cameras, and sound / picture sync.
    fn sources_panel(&mut self, ui: &mut egui::Ui) {
        // ---------------- Song file ----------------
        Self::heading(ui, "SONG");
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
            ui.spacing_mut().slider_width = (ui.available_width() - 12.0).max(60.0);
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

            ui.spacing_mut().slider_width = (ui.available_width() - 120.0).max(60.0);
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
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button("🔊 Sound & picture sync…")
                .on_hover_text(
                    "If the visuals feel early or late (Bluetooth speakers, a projector), set an offset here, or let the tap test find it.",
                )
                .clicked()
            {
                self.sync.open = true;
            }
            if self.sync.offset_ms != 0.0 {
                ui.label(
                    RichText::new(format!("{:+.0} ms", self.sync.offset_ms))
                        .small()
                        .color(crate::theme::ACCENT),
                );
            }
        });

        ui.add_space(14.0);

        // ---------------- Live input ----------------
        Self::heading(ui, "LIVE INPUT");
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
        ui.horizontal_wrapped(|ui| {
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
            let computer = self.input_devices.iter().position(|d| d.loopback);
            if ui
                .add_enabled(computer.is_some(), egui::Button::new("🔊 Computer audio"))
                .on_hover_text(
                    "Listen to whatever this computer is playing (Spotify, YouTube, a DJ app…)",
                )
                .clicked()
                && let Some(i) = computer
            {
                self.selected_input = Some(i);
                let device = self.input_devices[i].clone();
                self.error = None;
                if let Err(e) = self.engine.start_live(&device) {
                    log::error!("could not start {}: {e:#}", device.name);
                    self.error = Some(format!("Could not capture computer audio: {e:#}."));
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

        if let Some(err) = &self.error {
            ui.add_space(8.0);
            ui.label(RichText::new(err).color(ERROR_COLOR));
        }

        // ---------------- Cameras ----------------
        if self.scene.graph_mode {
            ui.add_space(14.0);
            Self::heading(ui, "CAMERAS");
            self.scene.ws.cameras_ui(ui);
        }
    }

    /// Tempo help, the events log and offline analysis.
    fn analysis_panel(&mut self, ui: &mut egui::Ui) {
        Self::heading(ui, "TEMPO AND VIEW");
        let mut meters = self.shown(Pane::Meters);
        if ui.checkbox(&mut meters, "Show signal meters (M)").changed() {
            self.set_shown(Pane::Meters, meters);
        }
        ui.checkbox(&mut self.show_waveform, "Waveform overlay");
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
        Self::heading(ui, "EVENTS");
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

        // ---------------- Footer ----------------
        ui.add_space(14.0);
        ui.vertical(|ui| {
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

    fn preview(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        if response.double_clicked() {
            self.toggle_fullscreen(ui.ctx());
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, PREVIEW_BG);

        let live = matches!(self.engine.source(), Source::Live(_));
        let signal_frame = self.sync.frame(self.signals.latest(), live);
        self.scene.graph_visible = self.graph_shown && !self.fullscreen;
        self.scene.paint(ui, frame, rect, &signal_frame);
        self.sync.draw(&painter, rect);
        if self.scene.graph_mode && !self.fullscreen {
            self.viewer_overlays(ui, rect);
            self.scene.ws.viewer_handles(ui, rect);
        }

        let snap = self.waveform.read();
        if self.show_waveform {
            draw_waveform(&painter, rect, &snap.columns);
        }
        let has_source = !matches!(self.engine.source(), Source::None);

        let label_color = Color32::from_gray(200);
        if self.show_fps && !self.fullscreen {
            let size = self
                .scene
                .render_size()
                .map(|(w, h)| format!(" · {w}×{h}"))
                .unwrap_or_default();
            let text = format!(
                "{:.0} fps · {:.1} ms{size}",
                self.scene.fps, self.scene.frame_ms
            );
            let pos = rect.left_top() + Vec2::new(10.0, 8.0);
            let galley = painter.layout_no_wrap(text, egui::FontId::monospace(12.0), label_color);
            painter.rect_filled(
                Rect::from_min_size(pos - Vec2::splat(4.0), galley.size() + Vec2::splat(8.0)),
                CornerRadius::same(3),
                Color32::from_black_alpha(150),
            );
            painter.galley(pos, galley, label_color);
        }
        if let Some(cap) = self.scene.viewer_caption().filter(|_| !self.fullscreen) {
            let galley = painter.layout_no_wrap(cap, egui::FontId::proportional(12.5), label_color);
            let pos = rect.left_bottom() + Vec2::new(10.0, -galley.size().y - 10.0);
            painter.rect_filled(
                Rect::from_min_size(pos - Vec2::splat(4.0), galley.size() + Vec2::splat(8.0)),
                CornerRadius::same(3),
                Color32::from_black_alpha(160),
            );
            painter.galley(pos, galley, crate::graph_editor::ACCENT);
        }
        self.scene
            .perf
            .draw_overlay(&painter, rect, !self.fullscreen);
        if !has_source && !self.fullscreen {
            let msg = "No sound yet. Open a song or start an input in the Sources panel.";
            let pos = rect.center_bottom() - Vec2::new(0.0, 40.0);
            let galley =
                painter.layout_no_wrap(msg.into(), egui::FontId::proportional(15.0), label_color);
            let r = Rect::from_center_size(pos, galley.size() + Vec2::new(16.0, 10.0));
            painter.rect_filled(r, CornerRadius::same(4), Color32::from_black_alpha(160));
            painter.galley(r.min + Vec2::new(8.0, 5.0), galley, label_color);
        }
    }

    /// Wipe compare and picture-in-picture of the selected node.
    fn viewer_overlays(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let ws = &mut self.scene.ws;
        let Some(sel) = ws.selected_picture() else {
            return;
        };
        let Some(tex) = ws.thumbs.get(&sel).copied() else {
            return;
        };
        let painter = ui.painter_at(rect);
        let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        match ws.viewer {
            ViewerMode::Wipe => {
                let x = rect.left() + rect.width() * ws.wipe;
                let left = Rect::from_min_max(rect.min, Pos2::new(x, rect.bottom()));
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(ws.wipe, 1.0));
                painter.image(tex, left, uv, Color32::WHITE);
                let handle = Rect::from_center_size(
                    Pos2::new(x, rect.center().y),
                    Vec2::new(14.0, rect.height()),
                );
                let r = ui.interact(handle, ui.scope_id().with("wipe"), Sense::drag());
                if r.dragged()
                    && let Some(p) = r.interact_pointer_pos()
                {
                    ws.wipe = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                }
                let col = if r.hovered() || r.dragged() {
                    theme::ACCENT
                } else {
                    Color32::from_white_alpha(200)
                };
                painter.line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    Stroke::new(2.0, col),
                );
                painter.circle_filled(Pos2::new(x, rect.center().y), 7.0, col);
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
            // No preview while the viewer shows its handles: it's in the way.
            ViewerMode::Program
                if ws.pip
                    && !ws.graph.node(sel).is_some_and(|n| {
                        ws.editor.selected.len() == 1 && crate::handles::has_handles(n)
                    }) =>
            {
                let w = (rect.width() * 0.26).clamp(120.0, 420.0);
                let h = w * rect.height() / rect.width().max(1.0);
                let r = Rect::from_min_size(
                    Pos2::new(rect.right() - w - 12.0, rect.top() + 12.0),
                    Vec2::new(w, h),
                );
                painter.rect_filled(r.expand(2.0), 4.0, Color32::from_black_alpha(200));
                painter.image(tex, r, full, Color32::WHITE);
                painter.rect_stroke(
                    r.expand(1.0),
                    3.0,
                    Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.7)),
                    egui::StrokeKind::Outside,
                );
                if let Some(n) = ws.graph.node(sel) {
                    let text = format!("{:02} {}", sel.0, n.label);
                    let g = painter.layout_no_wrap(
                        text,
                        egui::FontId::proportional(11.5),
                        Color32::WHITE,
                    );
                    let at = r.left_bottom() + Vec2::new(6.0, -g.size().y - 5.0);
                    painter.rect_filled(
                        Rect::from_min_size(at - Vec2::splat(3.0), g.size() + Vec2::splat(6.0)),
                        3.0,
                        Color32::from_black_alpha(170),
                    );
                    painter.galley(at, g, Color32::WHITE);
                }
                // Click the PiP to view that node full size.
                let resp = ui.interact(r, ui.scope_id().with("pip"), Sense::click());
                if resp.clicked() {
                    ws.viewer = ViewerMode::Selected;
                }
                resp.on_hover_text("Selected node. Click to view it full size (V toggles back).");
            }
            _ => {}
        }
    }

    /// The bar across the top: panels, viewer mode, graph options.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(
                RichText::new("VISUAL ENGINE")
                    .strong()
                    .size(12.5)
                    .color(theme::ACCENT),
            );
            ui.add_space(14.0);
            self.layout_menu(ui);
            self.panels_menu(ui);
            ui.separator();
            if self.scene.graph_mode {
                let ws = &mut self.scene.ws;
                ui.label(RichText::new("Viewer").small().weak());
                ui.selectable_value(&mut ws.viewer, ViewerMode::Program, "Program")
                    .on_hover_text("The final output (V)");
                ui.selectable_value(&mut ws.viewer, ViewerMode::Selected, "Selected")
                    .on_hover_text("The selected node's picture (V)");
                ui.selectable_value(&mut ws.viewer, ViewerMode::Wipe, "Wipe")
                    .on_hover_text("Split screen: selected node | program. Drag the line (W)");
                ui.toggle_value(&mut ws.pip, "PiP")
                    .on_hover_text("Small picture of the selected node over the program");
                ui.separator();
                ui.toggle_value(&mut ws.editor.snap, "Snap")
                    .on_hover_text("Snap nodes to the grid (G)");
                ui.toggle_value(&mut ws.editor.thumbnails, "Thumbnails")
                    .on_hover_text("Live pictures inside nodes");
                ui.toggle_value(&mut ws.editor.show_cost, "Cost").on_hover_text(
                    "GPU time each node takes, in milliseconds (needs a GPU that can measure it). At 60 fps the whole frame has 16.7 ms.",
                );
                if ui
                    .add_enabled(ws.history.can_undo(), egui::Button::new("↶"))
                    .on_hover_text("Undo (Ctrl+Z)")
                    .clicked()
                {
                    let _ = ws.history.undo(&mut ws.graph);
                }
                if ui
                    .add_enabled(ws.history.can_redo(), egui::Button::new("↷"))
                    .on_hover_text("Redo (Ctrl+Shift+Z)")
                    .clicked()
                {
                    let _ = ws.history.redo(&mut ws.graph);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("?")
                    .on_hover_text("Graph shortcuts. H shows the performance keys.")
                    .clicked()
                {
                    self.set_shown(Pane::Graph, true);
                    self.scene.ws.editor.show_help = !self.scene.ws.editor.show_help;
                }
                let gpu = match &self.scene.ws.gpu {
                    Some((_, total)) if self.scene.graph_mode => format!(" · GPU {total:.2} ms"),
                    _ => String::new(),
                };
                ui.label(
                    RichText::new(format!(
                        "{:.0} fps · {:.1} ms{gpu}",
                        self.scene.fps, self.scene.frame_ms
                    ))
                    .small()
                    .monospace()
                    .weak(),
                )
                .on_hover_text(
                    "Frames per second and time per frame. GPU = time the graphics card spent on the graph's nodes (each node shows its own share).",
                );
            });
        });
    }

    /// Pick a ready-made layout.
    fn layout_menu(&mut self, ui: &mut egui::Ui) {
        let text = RichText::new(format!("Layout: {}", self.layout.name())).strong();
        ui.menu_button(text, |ui| {
            ui.label(
                RichText::new("Drag any panel's tab to move it, split it or stack it.")
                    .small()
                    .weak(),
            );
            for l in Layout::ALL {
                if ui.button(l.name()).on_hover_text(l.hint()).clicked() {
                    self.layout = l;
                    self.dock = dock::build(l);
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .button(format!("Reset {}", self.layout.name()))
                .on_hover_text("Put every panel back where this layout has it")
                .clicked()
            {
                self.dock = dock::build(self.layout);
                ui.close();
            }
        })
        .response
        .on_hover_text("Ready-made panel layouts");
    }

    /// Show or hide each panel.
    fn panels_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(RichText::new("Panels").strong(), |ui| {
            for pane in Pane::ALL {
                let mut on = self.shown(pane);
                if ui
                    .checkbox(&mut on, pane.title())
                    .on_hover_text(pane.hint())
                    .changed()
                {
                    self.set_shown(pane, on);
                }
            }
            ui.separator();
            ui.checkbox(&mut self.show_strip, "Signal strip")
                .on_hover_text("Levels, hits, tempo and beat along the bottom");
        })
        .response
        .on_hover_text("Show or hide panels");
    }

    /// The thin strip along the bottom: band levels, hits, tempo, phase.
    fn signal_strip(&self, ui: &mut egui::Ui) {
        let sig = |n: &str| self.scene.signal(n);
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let p = ui.painter_at(rect);
        let cy = rect.center().y;
        let mut x = rect.left() + 10.0;
        let font = egui::FontId::proportional(10.5);
        let weak = theme::TEXT_WEAK;
        // Six bands.
        for (name, label) in [
            ("sub_fast", "SUB"),
            ("bass_fast", "BASS"),
            ("low_mid_fast", "LMID"),
            ("mid_fast", "MID"),
            ("high_mid_fast", "HMID"),
            ("air_fast", "AIR"),
        ] {
            let g = p.layout_no_wrap(label.into(), font.clone(), weak);
            p.galley(Pos2::new(x, cy - g.size().y * 0.5), g.clone(), weak);
            x += g.size().x + 4.0;
            let bar = Rect::from_min_size(Pos2::new(x, cy - 3.0), Vec2::new(46.0, 6.0));
            p.rect_filled(bar, 2.0, Color32::from_gray(34));
            let v = (sig(name) / 3.0).clamp(0.0, 1.0);
            let mut f = bar;
            f.set_width(bar.width() * v);
            p.rect_filled(f, 2.0, Color32::from_rgb(90, 170, 255));
            x += 54.0;
        }
        x += 6.0;
        // Hits.
        for (name, label, col) in [
            ("kick_age", "K", Color32::from_rgb(255, 120, 80)),
            ("snare_age", "S", Color32::from_rgb(255, 210, 90)),
            ("hat_age", "H", Color32::from_rgb(140, 230, 255)),
        ] {
            let lit = (-sig(name) / 0.12).exp().clamp(0.0, 1.0);
            let c = Pos2::new(x + 7.0, cy);
            p.circle_filled(c, 7.0, lerp(Color32::from_gray(40), col, lit));
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                Color32::BLACK,
            );
            x += 19.0;
        }
        x += 10.0;
        let bpm = sig("bpm");
        let text = if bpm > 1.0 {
            format!("{bpm:.1} BPM")
        } else {
            "— BPM".into()
        };
        let g = p.layout_no_wrap(text, egui::FontId::monospace(11.5), theme::TEXT);
        p.galley(Pos2::new(x, cy - g.size().y * 0.5), g.clone(), theme::TEXT);
        x += g.size().x + 12.0;
        // Beat dots (4 per bar), then bar and phrase progress.
        let bar_phase = sig("bar_phase").clamp(0.0, 0.9999);
        let beat_in_bar = (bar_phase * 4.0).floor() as i32;
        let beat_lit = (1.0 - sig("beat_phase")).clamp(0.0, 1.0).powf(3.0);
        for i in 0..4 {
            let on = i == beat_in_bar;
            let col = if on {
                lerp(Color32::from_gray(90), theme::ACCENT, beat_lit.max(0.35))
            } else {
                Color32::from_gray(48)
            };
            p.circle_filled(Pos2::new(x + 5.0 + i as f32 * 13.0, cy), 4.5, col);
        }
        x += 58.0;
        for (label, v) in [("BAR", bar_phase), ("PHRASE", sig("phrase_phase"))] {
            let g = p.layout_no_wrap(label.into(), font.clone(), weak);
            p.galley(Pos2::new(x, cy - g.size().y * 0.5), g.clone(), weak);
            x += g.size().x + 4.0;
            let bar = Rect::from_min_size(Pos2::new(x, cy - 2.0), Vec2::new(60.0, 4.0));
            p.rect_filled(bar, 2.0, Color32::from_gray(34));
            let mut f = bar;
            f.set_width(bar.width() * v.clamp(0.0, 1.0));
            p.rect_filled(f, 2.0, theme::ACCENT.gamma_multiply(0.8));
            x += 70.0;
        }
        let _ = x;
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// The 48 kHz feed's waveform, drawn as an overlay across the middle.
fn draw_waveform(painter: &egui::Painter, rect: Rect, columns: &[[f32; 2]]) {
    let band = Rect::from_center_size(
        rect.center(),
        Vec2::new(rect.width() - 48.0, (rect.height() * 0.4).min(320.0)),
    );
    let mid = band.center().y;
    let n = columns.len();
    if n == 0 {
        return;
    }
    let half = band.height() * 0.5;
    let stroke = Stroke::new(1.5, WAVE_COLOR.gamma_multiply(0.8));
    for (i, [lo, hi]) in columns.iter().enumerate() {
        let x = band.left() + band.width() * (i as f32 + 0.5) / n as f32;
        let y_top = mid - hi.clamp(-1.0, 1.0) * half;
        let y_bot = mid - lo.clamp(-1.0, 1.0) * half;
        let (y0, y1) = if (y_bot - y_top) < 1.0 {
            (mid - 0.5, mid + 0.5)
        } else {
            (y_top, y_bot)
        };
        painter.line_segment([Pos2::new(x, y0), Pos2::new(x, y1)], stroke);
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
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let ws = &self.scene.ws;
        let prefs = Prefs {
            dock: Some(self.dock.clone()),
            layout: self.layout,
            show_strip: self.show_strip,
            viewer: ws.viewer,
            pip: ws.pip,
            snap: ws.editor.snap,
            thumbnails: ws.editor.thumbnails,
            show_fps: self.show_fps,
            show_cost: ws.editor.show_cost,
            node_scale: ws.editor.node_scale,
            av_offset_ms: self.sync.offset_ms,
            media_pool: ws.pool.items.clone(),
        };
        eframe::set_value(storage, PREFS_KEY, &prefs);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_loading();
        self.poll_offline();
        self.check_stream_errors();
        // The preview is live, so keep redrawing.
        ctx.request_repaint();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Keyboard: F11 fullscreen, Esc leaves it, M meters, I FPS counter,
        // plus the performance keys (see perform.rs / H in the app).
        // (Ignored while typing in a text field.)
        let typing = ui.ctx().egui_wants_keyboard_input();
        // The sync test takes Space as its tap key while it listens.
        self.sync.capture_taps(ui.ctx());
        if let Source::File(p) = self.engine.source() {
            p.set_analysis_lead(self.sync.player_lead());
        }
        self.sync.window(ui.ctx());
        // Node-graph keys go first so the performance keys don't eat them
        // (Shift+Space = add node, Tab = hide/show the graph).
        if !typing && self.scene.graph_mode {
            let (search, tab, v, w) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::SHIFT, egui::Key::Space),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::V),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::W),
                )
            });
            let ws = &mut self.scene.ws;
            if v {
                ws.viewer = if ws.viewer == ViewerMode::Selected {
                    ViewerMode::Program
                } else {
                    ViewerMode::Selected
                };
            }
            if w {
                ws.viewer = if ws.viewer == ViewerMode::Wipe {
                    ViewerMode::Program
                } else {
                    ViewerMode::Wipe
                };
            }
            if search {
                self.set_shown(Pane::Graph, true);
                self.scene.ws.editor.open_search(ui.ctx());
            }
            if tab {
                let on = self.shown(Pane::Graph);
                self.set_shown(Pane::Graph, !on);
            }
        }
        self.scene.handle_keys(ui.ctx());
        let (dropped, hovering, pointer) = ui.input(|i| {
            (
                i.raw.dropped_files.clone(),
                !i.raw.hovered_files.is_empty(),
                i.pointer.hover_pos(),
            )
        });
        for f in dropped {
            let path = f.path().to_path_buf();
            if path.as_os_str().is_empty() {
                continue;
            }
            match media::kind_of(&path) {
                Some(media::Kind::Audio) => self.load_song(path),
                Some(_) => {
                    let graph_on = self.shown(Pane::Graph);
                    let ws = &mut self.scene.ws;
                    ws.pool.add(&path);
                    if self.scene.graph_mode
                        && graph_on
                        && let Some(p) = pointer.filter(|p| ws.editor.last_rect.contains(*p))
                    {
                        ws.editor
                            .pending_media
                            .push((crate::media_pool::MediaDrop::File(path), p));
                    }
                }
                None => {}
            }
        }
        if hovering {
            let rect = ui.ctx().content_rect();
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("file-hover"),
            ));
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(140));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop pictures or videos (onto the graph to add them there), or a song",
                egui::FontId::proportional(18.0),
                crate::theme::ACCENT,
            );
        }
        let (f11, esc, m, f) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::F11),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::M),
                i.key_pressed(egui::Key::I),
            )
        });
        if f11 || (esc && self.fullscreen) {
            self.toggle_fullscreen(ui.ctx());
        }
        if m && !typing {
            let on = self.shown(Pane::Meters);
            self.set_shown(Pane::Meters, !on);
        }
        if f && !typing {
            self.show_fps = !self.show_fps;
        }

        // Manual rhythm controls (keyboard).
        let (tap, resync) =
            ui.input(|i| (i.key_pressed(egui::Key::T), i.key_pressed(egui::Key::R)));
        if tap && !typing {
            self.send(Command::Tap);
        }
        if resync && !typing {
            self.send(Command::ResyncDownbeat);
        }

        let sf = self.signals.latest().clone();
        let song_pos = match self.engine.source() {
            Source::File(p) => Some(p.position_secs()),
            _ => None,
        };
        self.rhythm_view.update(&sf, song_pos);

        if self.fullscreen {
            egui::CentralPanel::no_frame().show(ui, |ui| self.preview(ui, frame));
            return;
        }

        egui::Panel::top("toolbar")
            .exact_size(32.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::DEEP)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| self.toolbar(ui));

        if self.show_strip {
            egui::Panel::bottom("strip")
                .exact_size(26.0)
                .frame(egui::Frame::NONE.fill(theme::DEEP))
                .show(ui, |ui| self.signal_strip(ui));
        }

        self.graph_shown = self.scene.graph_mode && self.shown(Pane::Graph);
        let mut tree = std::mem::replace(
            &mut self.dock,
            egui_tiles::Tree::empty(egui::Id::unique("swap")),
        );
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let mut panes = Panes {
                app: self,
                frame,
                signals: &sf,
            };
            tree.ui(&mut panes, ui);
        });
        self.dock = tree;
    }
}

/// Draws each dock panel.
struct Panes<'a> {
    app: &'a mut VisualEngineApp,
    frame: &'a eframe::Frame,
    signals: &'a signals::SignalFrame,
}

impl egui_tiles::Behavior<Pane> for Panes<'_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        pane.title().into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile: egui_tiles::TileId,
        pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        let app = &mut *self.app;
        let padded = |ui: &mut egui::Ui, fill: Color32, add: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::NONE
                .fill(fill)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| add(ui));
                });
        };
        match pane {
            Pane::Viewer => app.preview(ui, self.frame),
            Pane::Graph => {
                if app.scene.graph_mode {
                    egui::Frame::NONE
                        .fill(crate::graph_editor::CANVAS_BG)
                        .show(ui, |ui| {
                            ui.set_min_size(ui.available_size());
                            app.scene.graph_panel(ui);
                        });
                } else {
                    padded(ui, PANEL_BG, &mut |ui| {
                        ui.label(
                            RichText::new(
                                "Classic scenes are on. Switch the Inspector to Node graph \
                                 to build with nodes.",
                            )
                            .weak(),
                        );
                    });
                }
            }
            Pane::Inspector => padded(ui, PANEL_BG, &mut |ui| app.scene.controls(ui)),
            Pane::Sources => padded(ui, PANEL_BG, &mut |ui| app.sources_panel(ui)),
            Pane::Media => padded(ui, PANEL_BG, &mut |ui| app.media_panel(ui)),
            Pane::Looks => padded(ui, PANEL_BG, &mut |ui| app.looks_panel(ui)),
            Pane::Analysis => padded(ui, PANEL_BG, &mut |ui| app.analysis_panel(ui)),
            Pane::Meters => {
                app.update_hop_rate(self.signals.hop_index);
                let rate = app.hops_per_sec;
                let sf = self.signals;
                egui::Frame::NONE.fill(PANEL_BG).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    debug_panel::show(ui, sf, rate, &app.rhythm_view);
                });
            }
        }
        egui_tiles::UiResponse::None
    }

    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> Color32 {
        theme::DEEP
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        3.0
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
