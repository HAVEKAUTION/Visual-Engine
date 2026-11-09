//! The scene preview and the scene panel: picker, input sliders, shader
//! errors, hot reload and the FPS counter.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use egui::{Color32, RichText};
use render::{Renderer, ScenePipeline};
use scenes::{InputKind, ParamState, SceneLibrary, SignalState};
use signals::SignalFrame;

const ERROR_COLOR: Color32 = Color32::from_rgb(255, 110, 110);
const OK_COLOR: Color32 = Color32::from_rgb(120, 230, 140);

struct Active {
    file_name: String,
    pipeline: ScenePipeline,
    params: ParamState,
}

pub struct SceneView {
    library: SceneLibrary,
    renderer: Option<Renderer>,
    active: Option<Active>,
    /// Remembered slider settings for scenes that aren't showing.
    saved: HashMap<String, ParamState>,
    /// Compile errors by file name.
    errors: HashMap<String, String>,
    /// File the user picked (may be broken; then `active` is the last good one).
    selected: String,
    signals: SignalState,
    texture: Option<(egui::TextureId, u64)>,
    last_frame: Instant,
    fps_mark: (u32, Instant),
    pub fps: f32,
    pub frame_ms: f32,
    last_reload: Option<(String, Instant, bool)>,
}

/// The scenes folder: the source tree's when running from a checkout (so the
/// built-ins are editable live), otherwise `scenes` next to the program.
fn scenes_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("VISUAL_ENGINE_SCENES") {
        return Some(PathBuf::from(d));
    }
    scenes::source_scene_dir().or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("scenes")))
    })
}

impl SceneView {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let renderer = cc
            .wgpu_render_state
            .as_ref()
            .map(|rs| Renderer::new(&rs.device, &rs.queue));
        let library = SceneLibrary::new(scenes_dir());
        if let Some(d) = library.dir() {
            log::info!("scenes folder: {}", d.display());
        }
        let mut view = Self {
            library,
            renderer,
            active: None,
            saved: HashMap::new(),
            errors: HashMap::new(),
            selected: String::new(),
            signals: SignalState::new(),
            texture: None,
            last_frame: Instant::now(),
            fps_mark: (0, Instant::now()),
            fps: 0.0,
            frame_ms: 0.0,
            last_reload: None,
        };
        let first = view.library.entries().first().map(|e| e.file_name.clone());
        if let Some(f) = first {
            view.select(&f);
        }
        view
    }

    /// Compile a scene file; on success build its GPU pipeline too.
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

    /// Switch to another scene. If it doesn't compile, the current one keeps
    /// running and the error is shown.
    pub fn select(&mut self, file_name: &str) {
        self.selected = file_name.to_string();
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.file_name == file_name)
        {
            return;
        }
        let Some((pipeline, compiled)) = self.build(file_name) else {
            return;
        };
        if let Some(old) = self.active.take() {
            self.saved.insert(old.file_name, old.params);
        }
        let params = match self.saved.remove(file_name) {
            Some(p) => p.remap(&compiled.meta),
            None => ParamState::new(&compiled.meta),
        };
        log::info!("scene: {}", compiled.meta.name);
        self.active = Some(Active {
            file_name: file_name.to_string(),
            pipeline,
            params,
        });
        if let Some(r) = &mut self.renderer {
            r.clear_feedback();
        }
    }

    /// Hot reload: recompile files that changed on disk.
    fn poll_files(&mut self) {
        for file in self.library.poll() {
            let exists = self.library.get(&file).is_some();
            if !exists {
                self.errors.remove(&file);
                continue;
            }
            let is_active = self.active.as_ref().is_some_and(|a| a.file_name == file);
            let is_selected = self.selected == file;
            if !(is_active || is_selected) {
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
            let Some((pipeline, compiled)) = built else {
                continue; // keep the last good version running
            };
            log::info!("reloaded {file}");
            match &mut self.active {
                Some(a) if a.file_name == file => {
                    // Same scene: keep slider values, clocks and feedback.
                    a.params = a.params.remap(&compiled.meta);
                    a.pipeline = pipeline;
                }
                _ => {
                    // The picked scene was broken and is now fixed: switch to it.
                    self.select(&file);
                }
            }
        }
        // The active scene's file was deleted: keep running the last version.
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
        self.signals
            .update(signal_frame, dt, [size.0 as f32, size.1 as f32]);

        let (Some(renderer), Some(active), Some(rs)) = (
            self.renderer.as_mut(),
            self.active.as_mut(),
            frame.wgpu_render_state(),
        ) else {
            return;
        };
        active.params.advance(&self.signals, dt);
        renderer.render(
            &active.pipeline,
            self.signals.as_bytes(),
            active.params.block(),
            size,
        );

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

        // ---- picker ----
        let names: Vec<(String, String, bool)> = self
            .library
            .entries()
            .iter()
            .map(|e| {
                let name =
                    scenes::SceneMeta::parse(&e.source, e.file_name.trim_end_matches(".wgsl"))
                        .map(|m| m.name)
                        .unwrap_or_else(|_| e.file_name.clone());
                (
                    e.file_name.clone(),
                    name,
                    self.errors.contains_key(&e.file_name),
                )
            })
            .collect();
        let current = names
            .iter()
            .find(|(f, _, _)| *f == self.selected)
            .map(|(_, n, _)| n.clone())
            .unwrap_or_default();
        let mut pick = None;
        egui::ComboBox::from_id_salt("scene-picker")
            .width(ui.available_width())
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (file, name, broken) in &names {
                    let label = if *broken {
                        format!("⚠ {name}")
                    } else {
                        name.clone()
                    };
                    if ui.selectable_label(*file == self.selected, label).clicked() {
                        pick = Some(file.clone());
                    }
                }
            });
        if let Some(f) = pick {
            self.select(&f);
        }

        // ---- error / reload status ----
        if let Some(err) = self.errors.get(&self.selected).cloned() {
            ui.add_space(6.0);
            let note = if self.active.is_some() {
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

        let Some(active) = self.active.as_mut() else {
            return;
        };
        let meta = active.params.meta().clone();
        ui.add_space(4.0);
        let mut info = String::new();
        if !meta.author.is_empty() {
            info += &format!("by {}", meta.author);
        }
        if let Some(style) = meta.style.as_str() {
            info += &format!(" · {style}");
        }
        if !info.is_empty() {
            ui.label(RichText::new(info).small().weak());
        }
        if !meta.description.is_empty() {
            ui.label(RichText::new(&meta.description).small());
        }

        // ---- inputs ----
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("CONTROLS").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Reset").clicked() {
                    active.params.reset_inputs();
                }
            });
        });
        ui.separator();
        if meta.inputs.is_empty() {
            ui.label(RichText::new("This scene has no controls.").weak());
        }
        ui.spacing_mut().slider_width = (ui.available_width() - 80.0).max(80.0);
        for (i, input) in meta.inputs.iter().enumerate() {
            let v = &mut active.params.values[i];
            match input.kind {
                InputKind::Float => {
                    let (lo, hi) = input.range();
                    ui.label(input.label());
                    ui.add(egui::Slider::new(&mut v[0], lo..=hi));
                }
                InputKind::Bool => {
                    let mut on = v[0] > 0.5;
                    if ui.checkbox(&mut on, input.label()).changed() {
                        v[0] = if on { 1.0 } else { 0.0 };
                    }
                }
                InputKind::Color => {
                    ui.horizontal(|ui| {
                        let mut c = [v[0], v[1], v[2], v[3]];
                        ui.color_edit_button_rgba_unmultiplied(&mut c);
                        v.copy_from_slice(&c);
                        ui.label(input.label());
                    });
                }
            }
        }

        // ---- folder ----
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
            ui.label(RichText::new(dir.display().to_string()).small().weak());
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
