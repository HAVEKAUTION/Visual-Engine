//! The scene preview and the scene panel: picker, presets, sliders with
//! modulation, palette, macros, shader errors, hot reload, crossfades and the
//! FPS counter.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use egui::{Color32, RichText};
use render::{Master, Renderer, ScenePipeline};
use scenes::preset::{BANK_SLOTS, Bank, Preset, PresetLibrary};
use scenes::{InputKind, ParamState, SceneLibrary, SignalState};
use signals::SignalFrame;
use signals::modulation::ModContext;

use crate::mod_editor;
use crate::perform::{KeyAction, MACROS, Perform};

const ERROR_COLOR: Color32 = Color32::from_rgb(255, 110, 110);
const OK_COLOR: Color32 = Color32::from_rgb(120, 230, 140);
const ACCENT: Color32 = Color32::from_rgb(255, 200, 100);

/// A scene loaded on one of the renderer's decks.
struct Deck {
    file_name: String,
    pipeline: ScenePipeline,
    params: ParamState,
}

/// A crossfade in progress: the live deck fades in, `from` fades out.
struct Fade {
    from: usize,
    started: f64,
    secs: f32,
}

pub struct SceneView {
    library: SceneLibrary,
    presets: PresetLibrary,
    bank: Bank,
    renderer: Option<Renderer>,
    decks: [Option<Deck>; render::DECKS],
    /// The deck being shown and edited.
    live: usize,
    fade: Option<Fade>,
    /// Remembered settings for scenes that aren't showing.
    saved: HashMap<String, ParamState>,
    /// Compile errors by file name.
    errors: HashMap<String, String>,
    /// File the user picked (may be broken; then the live deck is the last good one).
    selected: String,
    /// Preset last loaded on the live deck.
    current_preset: Option<String>,
    preset_name: String,
    preset_status: Option<(String, bool)>,
    /// Which input's modulation editor is open.
    mod_open: Option<usize>,
    signals: SignalState,
    pub perf: Perform,
    /// Render clock (real seconds).
    clock: f64,
    texture: Option<(egui::TextureId, u64)>,
    last_frame: Instant,
    fps_mark: (u32, Instant),
    pub fps: f32,
    pub frame_ms: f32,
    last_reload: Option<(String, Instant, bool)>,
}

/// The source-tree folder `name` (when running from a checkout, so files are
/// editable live), otherwise `name` next to the program. Overridable with an
/// environment variable.
fn data_dir(env: &str, source: Option<PathBuf>, name: &str) -> Option<PathBuf> {
    if let Ok(d) = std::env::var(env) {
        return Some(PathBuf::from(d));
    }
    source.or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(name)))
    })
}

impl SceneView {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let renderer = cc
            .wgpu_render_state
            .as_ref()
            .map(|rs| Renderer::new(&rs.device, &rs.queue));
        let library = SceneLibrary::new(data_dir(
            "VISUAL_ENGINE_SCENES",
            scenes::source_scene_dir(),
            "scenes",
        ));
        let presets = PresetLibrary::new(data_dir(
            "VISUAL_ENGINE_PRESETS",
            scenes::source_preset_dir(),
            "presets",
        ));
        if let Some(d) = library.dir() {
            log::info!("scenes folder: {}", d.display());
        }
        if let Some(d) = presets.dir() {
            log::info!(
                "presets folder: {} ({} presets)",
                d.display(),
                presets.entries().len()
            );
        }
        let order: Vec<&str> = scenes::builtin::BUILTIN
            .iter()
            .map(|b| b.file_name)
            .collect();
        let bank = presets
            .load_bank()
            .unwrap_or_else(|| presets.default_bank(&order));
        let mut view = Self {
            library,
            presets,
            bank,
            renderer,
            decks: [None, None],
            live: 0,
            fade: None,
            saved: HashMap::new(),
            errors: HashMap::new(),
            selected: String::new(),
            current_preset: None,
            preset_name: String::new(),
            preset_status: None,
            mod_open: None,
            signals: SignalState::new(),
            perf: Perform::default(),
            clock: 0.0,
            texture: None,
            last_frame: Instant::now(),
            fps_mark: (0, Instant::now()),
            fps: 0.0,
            frame_ms: 0.0,
            last_reload: None,
        };
        if let Some(first) = view.bank[0].clone() {
            view.load_preset(&first, false);
        }
        if view.decks[view.live].is_none() {
            let first = view.library.entries().first().map(|e| e.file_name.clone());
            if let Some(f) = first {
                view.load_scene(&f, None, false);
            }
        }
        view
    }

    fn live_deck(&self) -> Option<&Deck> {
        self.decks[self.live].as_ref()
    }

    /// Compile a scene file and build its GPU pipeline.
    fn build(&mut self, file_name: &str) -> Option<(ScenePipeline, scenes::CompiledScene)> {
        let renderer = self.renderer.as_ref()?;
        let entry = self.library.get(file_name)?;
        let result = scenes::compile(&entry.source, file_name)
            .and_then(|c| renderer.create_scene(&c).map(|p| (p, c)));
        match result {
            Ok(ok) => {
                self.errors.remove(file_name);
                Some(ok)
            }
            Err(e) => {
                log::warn!("scene {file_name} failed to compile:\n{e}");
                self.errors.insert(file_name.to_string(), e);
                None
            }
        }
    }

    /// Crossfade length in seconds, from the setting in beats and the tempo.
    fn fade_secs(&self) -> f32 {
        let bpm = self.signals.get("bpm").unwrap_or(120.0).clamp(40.0, 240.0);
        self.perf.crossfade_beats * 60.0 / bpm
    }

    /// Retire a deck, remembering its settings.
    fn drop_deck(&mut self, i: usize) {
        if let Some(d) = self.decks[i].take() {
            self.saved.insert(d.file_name, d.params);
        }
        if let Some(r) = &mut self.renderer {
            r.clear_feedback(i);
        }
    }

    /// Show `file_name` (optionally with a preset), crossfading if `fade`.
    /// If it doesn't compile, the current scene keeps running and the error is
    /// shown.
    fn load_scene(&mut self, file_name: &str, preset: Option<&Preset>, fade: bool) -> bool {
        self.selected = file_name.to_string();
        let Some((pipeline, compiled)) = self.build(file_name) else {
            return false;
        };
        let mut params = match self.saved.remove(file_name) {
            Some(p) => p.remap(&compiled.meta),
            None => ParamState::new(&compiled.meta),
        };
        if let Some(p) = preset {
            params.apply_preset(p);
        }
        // Finish any crossfade that is still running.
        if let Some(f) = self.fade.take() {
            self.drop_deck(f.from);
        }
        let deck = Deck {
            file_name: file_name.to_string(),
            pipeline,
            params,
        };
        let secs = if fade { self.fade_secs() } else { 0.0 };
        if secs > 0.01 && self.decks[self.live].is_some() {
            let target = 1 - self.live;
            self.drop_deck(target);
            self.decks[target] = Some(deck);
            self.fade = Some(Fade {
                from: self.live,
                started: self.clock,
                secs,
            });
            self.live = target;
        } else {
            let live = self.live;
            self.drop_deck(live);
            self.decks[live] = Some(deck);
        }
        self.mod_open = None;
        log::info!("scene: {}", compiled.meta.name);
        true
    }

    /// Load a preset by id.
    fn load_preset(&mut self, id: &str, fade: bool) {
        let Some(entry) = self.presets.get(id) else {
            self.perf.toast(format!("Preset \"{id}\" not found"));
            return;
        };
        let preset = entry.preset.clone();
        if self.load_scene(&preset.scene, Some(&preset), fade) {
            self.current_preset = Some(id.to_string());
            self.preset_name = preset.name.clone();
            self.perf.toast(preset.name.clone());
        }
    }

    /// Keyboard actions from performance mode.
    pub fn handle_keys(&mut self, ctx: &egui::Context) {
        for a in self.perf.handle_keys(ctx) {
            match a {
                KeyAction::Preset(slot) => match self.bank[slot].clone() {
                    Some(id) => self.load_preset(&id, true),
                    None => self.perf.toast(format!("Nothing on key {}", slot + 1)),
                },
            }
        }
    }

    /// Hot reload: recompile files that changed on disk.
    fn poll_files(&mut self) {
        self.presets.poll();
        for file in self.library.poll() {
            if self.library.get(&file).is_none() {
                self.errors.remove(&file);
                continue;
            }
            let on_deck: Vec<usize> = (0..render::DECKS)
                .filter(|&i| self.decks[i].as_ref().is_some_and(|d| d.file_name == file))
                .collect();
            let is_selected = self.selected == file;
            if on_deck.is_empty() && !is_selected {
                // Just refresh the error badge in the picker.
                if let Some(r) = &self.renderer
                    && let Some(e) = self.library.get(&file)
                {
                    match scenes::compile(&e.source, &file).and_then(|c| r.create_scene(&c)) {
                        Ok(_) => {
                            self.errors.remove(&file);
                        }
                        Err(err) => {
                            self.errors.insert(file.clone(), err);
                        }
                    }
                }
                continue;
            }
            let built = self.build(&file);
            self.last_reload = Some((file.clone(), Instant::now(), built.is_some()));
            let Some((_, compiled)) = built else {
                continue; // keep the last good version running
            };
            log::info!("reloaded {file}");
            if on_deck.is_empty() {
                // The picked scene was broken and is now fixed: switch to it.
                self.load_scene(&file, None, false);
                continue;
            }
            for i in on_deck {
                // Each deck needs its own pipeline (own params buffer).
                let Some(pipeline) = self
                    .renderer
                    .as_ref()
                    .and_then(|r| r.create_scene(&compiled).ok())
                else {
                    continue;
                };
                if let Some(d) = self.decks[i].as_mut() {
                    // Same scene: keep sliders, modulations, clocks and feedback.
                    d.params = d.params.remap(&compiled.meta);
                    d.pipeline = pipeline;
                }
            }
        }
    }

    /// Render a frame into `rect` and paint it. Call once per UI frame.
    pub fn paint(
        &mut self,
        ui: &mut egui::Ui,
        frame: &eframe::Frame,
        rect: egui::Rect,
        signal_frame: &SignalFrame,
    ) {
        self.poll_files();

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.clock += dt as f64;
        self.fps_mark.0 += 1;
        let since = self.fps_mark.1.elapsed().as_secs_f32();
        if since >= 0.5 {
            self.fps = self.fps_mark.0 as f32 / since;
            self.frame_ms = 1000.0 * since / self.fps_mark.0 as f32;
            self.fps_mark = (0, now);
        }

        let ppp = ui.ctx().pixels_per_point();
        let size = (
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        );
        // Global speed scales time for the scenes.
        let sdt = dt * self.perf.speed;
        self.signals
            .update(signal_frame, sdt, [size.0 as f32, size.1 as f32]);
        self.perf
            .update(dt, self.signals.get("beat_count").unwrap_or(0.0));

        // Crossfade progress.
        let mut mix_b = self.live as f32;
        if let Some(f) = &self.fade {
            let t = ((self.clock - f.started) as f32 / f.secs).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t);
            if t >= 1.0 {
                let from = f.from;
                self.fade = None;
                self.drop_deck(from);
            } else {
                mix_b = if self.live == 1 { t } else { 1.0 - t };
            }
        }

        let (Some(renderer), Some(rs)) = (self.renderer.as_mut(), frame.wgpu_render_state()) else {
            return;
        };
        renderer.begin_frame(size);
        let beats = self.signals.get("beats").unwrap_or(0.0) as f64;
        let time = self.signals.get("time").unwrap_or(0.0) as f64;
        for (i, deck) in self.decks.iter_mut().enumerate() {
            let Some(deck) = deck else { continue };
            {
                let sig = &self.signals;
                let lookup = |n: &str| sig.get(n);
                let ctx = ModContext {
                    signal: &lookup,
                    beats,
                    time,
                    controls: &self.perf.macros,
                };
                deck.params.update(&ctx, self.perf.intensity);
            }
            deck.params.advance(&self.signals, sdt);
            self.signals.set_palette(&deck.params.current_palette());
            renderer.render(
                i,
                &deck.pipeline,
                self.signals.as_bytes(),
                deck.params.block(),
            );
        }
        renderer.present(Master {
            mix_b,
            gain: self.perf.gain(),
            flash: self.perf.flash(),
        });

        let Some(view) = renderer.display_view() else {
            return;
        };
        let generation = renderer.generation();
        let mut egui_renderer = rs.renderer.write();
        let id = match self.texture {
            Some((id, g)) if g == generation => id,
            Some((id, _)) => {
                egui_renderer.update_egui_texture_from_wgpu_texture(
                    &rs.device,
                    view,
                    eframe::wgpu::FilterMode::Linear,
                    id,
                );
                id
            }
            None => egui_renderer.register_native_texture(
                &rs.device,
                view,
                eframe::wgpu::FilterMode::Linear,
            ),
        };
        drop(egui_renderer);
        self.texture = Some((id, generation));
        ui.painter().image(
            id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    pub fn render_size(&self) -> Option<(u32, u32)> {
        self.renderer.as_ref().and_then(|r| r.size())
    }

    /// The scene panel.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("SCENE").strong());
        ui.separator();
        if self.renderer.is_none() {
            ui.label(RichText::new("No GPU renderer available.").color(ERROR_COLOR));
            return;
        }
        self.scene_picker(ui);
        self.error_status(ui);
        self.scene_info(ui);

        ui.add_space(6.0);
        egui::CollapsingHeader::new(RichText::new("PRESETS").strong())
            .default_open(true)
            .show(ui, |ui| self.presets_ui(ui));
        egui::CollapsingHeader::new(RichText::new("CONTROLS").strong())
            .default_open(true)
            .show(ui, |ui| self.inputs_ui(ui));
        egui::CollapsingHeader::new(RichText::new("COLOURS").strong())
            .default_open(true)
            .show(ui, |ui| {
                if let Some(d) = self.decks[self.live].as_mut() {
                    let own = *d.params.scene_palette();
                    mod_editor::palette_ui(ui, &mut d.params.palette, &own);
                    ui.label(
                        RichText::new("Swaps the colours of the scene for any palette.")
                            .small()
                            .weak(),
                    );
                }
            });
        egui::CollapsingHeader::new(RichText::new("PERFORMANCE").strong())
            .default_open(false)
            .show(ui, |ui| self.performance_ui(ui));
        egui::CollapsingHeader::new(RichText::new("MACROS").strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Manual controls. Use them as a modulation source (\"Macro 1\"…), \
                         then move them by hand. In M4 they can follow MIDI knobs.",
                    )
                    .small()
                    .weak(),
                );
                ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(60.0);
                for i in 0..MACROS {
                    ui.add(
                        egui::Slider::new(&mut self.perf.macros[i], 0.0..=1.0)
                            .text(format!("M{}", i + 1)),
                    );
                }
            });

        ui.add_space(12.0);
        if let Some(dir) = self.library.dir().map(|d| d.to_path_buf()) {
            ui.label(
                RichText::new(
                    "Edit any .wgsl file in the scenes folder and save: it reloads live.",
                )
                .small()
                .weak(),
            );
            if ui.button("📁 Open scenes folder").clicked() {
                open_folder(&dir);
            }
        }
    }

    fn scene_picker(&mut self, ui: &mut egui::Ui) {
        // (file, name, genres, broken)
        let names: Vec<(String, String, String, bool)> = self
            .library
            .entries()
            .iter()
            // Effects (fx_*.wgsl) are added as layers, not picked as scenes.
            .filter(|e| scenes::SceneMeta::parse(&e.source, "x").map_or(true, |m| !m.is_effect()))
            .map(|e| {
                let stem = e.file_name.trim_end_matches(".wgsl");
                let (name, genres) = scenes::SceneMeta::parse(&e.source, stem)
                    .map(|m| (m.name, m.genres.join(", ")))
                    .unwrap_or_else(|_| (e.file_name.clone(), String::new()));
                let broken = self.errors.contains_key(&e.file_name);
                (e.file_name.clone(), name, genres, broken)
            })
            .collect();
        let current = names
            .iter()
            .find(|(f, ..)| *f == self.selected)
            .map(|(_, n, ..)| n.clone())
            .unwrap_or_default();
        let mut pick = None;
        egui::ComboBox::from_id_salt("scene-picker")
            .width(ui.available_width())
            .height(600.0)
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (file, name, genres, broken) in &names {
                    let title = if *broken {
                        format!("⚠ {name}")
                    } else {
                        name.clone()
                    };
                    let resp =
                        ui.selectable_label(*file == self.selected, RichText::new(title).strong());
                    if !genres.is_empty() {
                        ui.label(RichText::new(format!("    {genres}")).small().weak());
                    }
                    if resp.clicked() {
                        pick = Some(file.clone());
                    }
                }
            });
        if let Some(f) = pick
            && self.live_deck().is_none_or(|d| d.file_name != f)
            && self.load_scene(&f, None, true)
        {
            self.current_preset = None;
        }
    }

    fn error_status(&mut self, ui: &mut egui::Ui) {
        if let Some(err) = self.errors.get(&self.selected).cloned() {
            ui.add_space(6.0);
            let note = if self.live_deck().is_some() {
                "Shader error: still showing the last working version."
            } else {
                "Shader error:"
            };
            ui.label(RichText::new(note).color(ERROR_COLOR).strong());
            egui::ScrollArea::both()
                .id_salt("shader-error")
                .max_height(200.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(err).monospace().small().color(ERROR_COLOR));
                });
        } else if let Some((file, at, true)) = &self.last_reload
            && at.elapsed().as_secs_f32() < 3.0
        {
            ui.label(
                RichText::new(format!("Reloaded {file}"))
                    .color(OK_COLOR)
                    .small(),
            );
        }
    }

    fn scene_info(&self, ui: &mut egui::Ui) {
        let Some(d) = self.live_deck() else { return };
        let meta = d.params.meta();
        ui.add_space(4.0);
        let mut info = String::new();
        if !meta.author.is_empty() {
            info += &format!("by {}", meta.author);
        }
        if let Some(style) = meta.style.as_str() {
            info += &format!(" · {style}");
        }
        if !meta.genres.is_empty() {
            ui.label(
                RichText::new(format!("Made for: {}", meta.genres.join(", ")))
                    .small()
                    .color(OK_COLOR),
            );
        }
        if !info.is_empty() {
            ui.label(RichText::new(info).small().weak());
        }
        if !meta.description.is_empty() {
            ui.label(RichText::new(&meta.description).small());
        }
    }

    fn inputs_ui(&mut self, ui: &mut egui::Ui) {
        let live = self.live;
        let Some(d) = self.decks[live].as_mut() else {
            return;
        };
        let meta = d.params.meta().clone();
        ui.horizontal(|ui| {
            if ui
                .small_button("Reset all")
                .on_hover_text("Back to the scene's defaults (no modulation, scene colours)")
                .clicked()
            {
                d.params.reset_inputs();
            }
            ui.label(
                RichText::new("~ = modulate (link to the music)")
                    .small()
                    .weak(),
            );
        });
        if meta.inputs.is_empty() {
            ui.label(RichText::new("This scene has no controls.").weak());
        }
        for (i, input) in meta.inputs.iter().enumerate() {
            let p = &mut d.params;
            match input.kind {
                InputKind::Float => {
                    let mut open = self.mod_open == Some(i);
                    let eff = p.effective[i][0];
                    mod_editor::float_row(
                        ui,
                        input,
                        &mut p.values[i][0],
                        eff,
                        &mut p.mods[i],
                        &mut open,
                        i,
                    );
                    if open {
                        self.mod_open = Some(i);
                    } else if self.mod_open == Some(i) {
                        self.mod_open = None;
                    }
                }
                InputKind::Bool => {
                    let mut on = p.values[i][0] > 0.5;
                    if ui.checkbox(&mut on, input.label()).changed() {
                        p.values[i][0] = if on { 1.0 } else { 0.0 };
                    }
                }
                InputKind::Choice => {
                    let cur = p.values[i][0].round().max(0.0) as usize;
                    egui::ComboBox::from_label(input.label())
                        .selected_text(input.options.get(cur).cloned().unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (k, o) in input.options.iter().enumerate() {
                                if ui.selectable_label(k == cur, o).clicked() {
                                    p.values[i][0] = k as f32;
                                }
                            }
                        });
                }
                InputKind::Color => {
                    ui.horizontal(|ui| {
                        let v = &mut p.values[i];
                        let mut c = [v[0], v[1], v[2], v[3]];
                        ui.color_edit_button_rgba_unmultiplied(&mut c);
                        v.copy_from_slice(&c);
                        ui.label(input.label());
                    });
                }
            }
        }
    }

    fn presets_ui(&mut self, ui: &mut egui::Ui) {
        let Some(scene_file) = self.live_deck().map(|d| d.file_name.clone()) else {
            return;
        };
        let ids: Vec<(String, String)> = self
            .presets
            .for_scene(&scene_file)
            .map(|e| (e.id.clone(), e.preset.name.clone()))
            .collect();
        if ids.is_empty() {
            ui.label(
                RichText::new("No presets for this scene yet.")
                    .small()
                    .weak(),
            );
        }
        let mut load = None;
        let mut delete = None;
        let mut bank_changed = false;
        for (id, name) in &ids {
            ui.horizontal(|ui| {
                let key = self.bank.iter().position(|b| b.as_deref() == Some(id));
                let key_text = key
                    .map(|k| format!("{}", k + 1))
                    .unwrap_or_else(|| "–".into());
                egui::ComboBox::from_id_salt(("key", id))
                    .width(34.0)
                    .selected_text(RichText::new(key_text).color(ACCENT))
                    .show_ui(ui, |ui| {
                        ui.label(RichText::new("Put on key").small().weak());
                        for k in 0..BANK_SLOTS {
                            let who = self.bank[k]
                                .as_deref()
                                .map(|b| format!(" (now {b})"))
                                .unwrap_or_default();
                            if ui
                                .selectable_label(key == Some(k), format!("{}{who}", k + 1))
                                .clicked()
                            {
                                for b in self.bank.iter_mut() {
                                    if b.as_deref() == Some(id) {
                                        *b = None;
                                    }
                                }
                                self.bank[k] = Some(id.clone());
                                bank_changed = true;
                            }
                        }
                        if ui.selectable_label(key.is_none(), "no key").clicked() {
                            for b in self.bank.iter_mut() {
                                if b.as_deref() == Some(id) {
                                    *b = None;
                                }
                            }
                            bank_changed = true;
                        }
                    })
                    .response
                    .on_hover_text("Number key that switches to this preset");
                let current = self.current_preset.as_deref() == Some(id.as_str());
                if ui.selectable_label(current, name).clicked() {
                    load = Some(id.clone());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Delete this preset")
                        .clicked()
                    {
                        delete = Some(id.clone());
                    }
                });
            });
        }
        if bank_changed && let Err(e) = self.presets.save_bank(&self.bank) {
            self.preset_status = Some((format!("Could not save keys: {e}"), false));
        }
        if let Some(id) = load {
            self.load_preset(&id, true);
        }
        if let Some(id) = delete {
            match self.presets.delete(&id) {
                Ok(()) => {
                    for b in self.bank.iter_mut() {
                        if b.as_deref() == Some(id.as_str()) {
                            *b = None;
                        }
                    }
                    let _ = self.presets.save_bank(&self.bank);
                    self.preset_status = Some(("Deleted".into(), true));
                }
                Err(e) => self.preset_status = Some((format!("Could not delete: {e}"), false)),
            }
        }

        ui.horizontal(|ui| {
            let w = (ui.available_width() - 85.0).max(60.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name)
                    .hint_text("Preset name")
                    .desired_width(w),
            );
            let can = !self.preset_name.trim().is_empty();
            if ui
                .add_enabled(can, egui::Button::new("💾 Save"))
                .on_hover_text("Save the sliders, modulations and colours (same name = overwrite)")
                .clicked()
                && let Some(d) = self.live_deck()
            {
                let preset = d.params.to_preset(self.preset_name.trim(), &d.file_name);
                match self.presets.save(&preset) {
                    Ok(id) => {
                        self.preset_status = Some((format!("Saved \"{}\"", preset.name), true));
                        self.current_preset = Some(id);
                    }
                    Err(e) => self.preset_status = Some((format!("Could not save: {e}"), false)),
                }
            }
        });
        if let Some((msg, ok)) = &self.preset_status {
            ui.label(
                RichText::new(msg)
                    .small()
                    .color(if *ok { OK_COLOR } else { ERROR_COLOR }),
            );
        }
        for (file, err) in &self.presets.errors {
            ui.label(
                RichText::new(format!("{file}: {err}"))
                    .small()
                    .color(ERROR_COLOR),
            );
        }
    }

    fn performance_ui(&mut self, ui: &mut egui::Ui) {
        let p = &mut self.perf;
        ui.spacing_mut().slider_width = (ui.available_width() - 190.0).max(60.0);
        ui.add(
            egui::Slider::new(&mut p.crossfade_beats, 0.0..=16.0)
                .step_by(0.5)
                .text("crossfade beats"),
        )
        .on_hover_text("How long preset changes take (0 = instant cut)");
        ui.add(egui::Slider::new(&mut p.intensity, 0.0..=2.0).text("intensity (↑↓)"))
            .on_hover_text("Scales every modulation");
        ui.add(
            egui::Slider::new(&mut p.speed, 0.25..=4.0)
                .logarithmic(true)
                .text("speed (←→)"),
        );
        ui.add(egui::Slider::new(&mut p.brightness, 0.0..=1.5).text("brightness"));
        ui.horizontal(|ui| {
            let label = if p.blackout {
                "Blackout ON (Space)"
            } else {
                "Blackout (Space)"
            };
            if ui.selectable_label(p.blackout, label).clicked() {
                p.toggle_blackout();
            }
            if ui.selectable_label(p.flash_armed(), "Flash (F)").clicked() {
                p.arm_flash();
            }
        });
        if ui.button("Hotkey sheet (H)").clicked() {
            p.show_help = !p.show_help;
        }
    }
}

fn open_folder(dir: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let cmd = "xdg-open";
    if let Err(e) = std::process::Command::new(cmd).arg(dir).spawn() {
        log::warn!("could not open {}: {e}", dir.display());
    }
}
