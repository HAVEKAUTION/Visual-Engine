//! The Media pool: the pictures and videos you've brought in, with
//! thumbnails, and the cameras plugged in. Drag one into the graph to make
//! an Image / Video / Camera node. Files dropped on the window from
//! Explorer / Finder land here too.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use egui::{Color32, RichText, Sense, vec2};
use media::Kind;

use crate::theme;

/// What's being dragged from the pool (the graph accepts it).
#[derive(Clone, Debug)]
pub enum MediaDrop {
    File(PathBuf),
    /// A camera, by name.
    Camera(String),
}

enum Thumb {
    Loading,
    Ready(egui::TextureHandle, (u32, u32)),
    Failed(String),
}

/// Something the pool asks the app to do.
pub enum PoolAction {
    /// Make a node for this file / camera in the middle of the graph.
    AddToGraph(MediaDrop),
}

type CameraList = Result<Vec<media::CameraDevice>, String>;

pub struct MediaPool {
    pub items: Vec<PathBuf>,
    thumbs: HashMap<PathBuf, Thumb>,
    tx: mpsc::Sender<(PathBuf, Result<media::Picture, String>)>,
    rx: mpsc::Receiver<(PathBuf, Result<media::Picture, String>)>,
    ffmpeg: Option<bool>,
    /// Cameras found (None = not looked yet), and the search under way.
    pub cameras: Option<CameraList>,
    camera_search: Option<mpsc::Receiver<CameraList>>,
}

impl Default for MediaPool {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            items: Vec::new(),
            thumbs: HashMap::new(),
            tx,
            rx,
            ffmpeg: None,
            cameras: None,
            camera_search: None,
        }
    }
}

const THUMB_W: f32 = 72.0;
const THUMB_H: f32 = 40.5;

impl MediaPool {
    /// Add a picture or video file (others are ignored). Returns true if it
    /// is a picture or video.
    pub fn add(&mut self, path: &Path) -> bool {
        if !matches!(media::kind_of(path), Some(Kind::Image | Kind::Video)) {
            return false;
        }
        if !self.items.iter().any(|p| p == path) {
            self.items.push(path.to_path_buf());
        }
        true
    }

    /// Make sure files the graph uses are listed.
    pub fn sync<'a>(&mut self, used: impl IntoIterator<Item = &'a Path>) {
        for p in used {
            self.add(p);
        }
    }

    /// Look for cameras again (in the background).
    pub fn find_cameras(&mut self) {
        if self.camera_search.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("find cameras".into())
            .spawn(move || {
                let _ = tx.send(media::cameras());
            });
        if spawned.is_ok() {
            self.camera_search = Some(rx);
        }
    }

    /// Camera names, once found.
    pub fn camera_names(&mut self) -> Vec<String> {
        if self.cameras.is_none() {
            self.find_cameras();
        }
        match &self.cameras {
            Some(Ok(list)) => list.iter().map(|c| c.name.clone()).collect(),
            _ => Vec::new(),
        }
    }

    fn poll_cameras(&mut self) {
        if let Some(rx) = &self.camera_search {
            match rx.try_recv() {
                Ok(r) => {
                    self.cameras = Some(r);
                    self.camera_search = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.camera_search = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    /// The cameras list (shown in the Sources panel): drag one into the graph.
    pub fn cameras_panel(&mut self, ui: &mut egui::Ui) -> Option<PoolAction> {
        let mut action = None;
        self.cameras_ui(ui, &mut action);
        action
    }

    fn cameras_ui(&mut self, ui: &mut egui::Ui, action: &mut Option<PoolAction>) {
        self.poll_cameras();
        if self.cameras.is_none() && self.camera_search.is_none() {
            self.find_cameras();
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Cameras").small().strong());
            if self.camera_search.is_some() {
                ui.add(egui::Spinner::new().size(11.0));
            } else if ui
                .small_button("\u{E036}")
                .on_hover_text("Look for cameras again (after plugging one in)")
                .clicked()
            {
                self.find_cameras();
            }
        });
        match &self.cameras {
            Some(Err(e)) => {
                ui.label(RichText::new(e).small().color(theme::error()));
            }
            Some(Ok(list)) if list.is_empty() => {
                ui.label(
                    RichText::new("No camera found. Plug one in and press \u{E036}.")
                        .small()
                        .weak(),
                );
            }
            Some(Ok(list)) => {
                for cam in list.clone() {
                    let resp = ui
                        .add(
                            egui::Label::new(format!("\u{E10E} {}", truncate(&cam.name, 30)))
                                .sense(Sense::click_and_drag()),
                        )
                        .on_hover_text("Drag into the graph, or double-click to add it.");
                    if resp.hovered() {
                        ui.painter().rect_stroke(
                            resp.rect.expand(2.0),
                            4.0,
                            egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.5)),
                            egui::StrokeKind::Outside,
                        );
                    }
                    if resp.drag_started() {
                        egui::DragAndDrop::set_payload(
                            ui.ctx(),
                            MediaDrop::Camera(cam.name.clone()),
                        );
                    }
                    if resp.dragged() {
                        carry_label(ui, &cam.name);
                    }
                    if resp.double_clicked() {
                        *action = Some(PoolAction::AddToGraph(MediaDrop::Camera(cam.name)));
                    }
                }
            }
            None => {}
        }
        ui.add_space(4.0);
    }

    fn thumb(&mut self, path: &Path) -> &Thumb {
        if !self.thumbs.contains_key(path) {
            self.thumbs.insert(path.to_path_buf(), Thumb::Loading);
            let tx = self.tx.clone();
            let p = path.to_path_buf();
            let _ = std::thread::Builder::new()
                .name("thumbnail".into())
                .spawn(move || {
                    let r = media::thumbnail(&p, 160);
                    let _ = tx.send((p, r));
                });
        }
        &self.thumbs[path]
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<PoolAction> {
        // Thumbnails that finished.
        while let Ok((path, r)) = self.rx.try_recv() {
            let t = match r {
                Ok(pic) => {
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [pic.width as usize, pic.height as usize],
                        &pic.rgba,
                    );
                    let name = path.display().to_string();
                    Thumb::Ready(
                        ui.ctx()
                            .load_texture(name, img, egui::TextureOptions::LINEAR),
                        (pic.width, pic.height),
                    )
                }
                Err(e) => Thumb::Failed(e),
            };
            self.thumbs.insert(path, t);
        }
        let mut action = None;
        ui.horizontal(|ui| {
            if ui
                .button("\u{E3D4} Import…")
                .on_hover_text("Add pictures or videos. You can also drop files on the window.")
                .clicked()
                && let Some(files) = rfd::FileDialog::new()
                    .add_filter(
                        "Pictures and videos",
                        &[media::IMAGE_EXTS, media::VIDEO_EXTS].concat(),
                    )
                    .pick_files()
            {
                for f in files {
                    self.add(&f);
                }
            }
            ui.label(
                RichText::new(format!("{} items", self.items.len()))
                    .weak()
                    .small(),
            );
        });
        if self
            .items
            .iter()
            .any(|p| media::kind_of(p) == Some(Kind::Video))
        {
            let ok = *self.ffmpeg.get_or_insert_with(media::have_ffmpeg);
            if !ok {
                ui.label(
                    RichText::new(
                        "Videos need ffmpeg. Open a terminal and run  winget install \
                         Gyan.FFmpeg  then restart the app.",
                    )
                    .small()
                    .color(theme::error()),
                );
            }
        }
        if self.items.is_empty() {
            ui.label(
                RichText::new(
                    "Drop pictures or videos here (or anywhere on the window), then drag \
                     them into the graph. They react to the music like everything else.",
                )
                .small()
                .weak(),
            );
            return action;
        }
        let mut remove = None;
        for path in self.items.clone() {
            let missing = !path.exists();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let kind = media::kind_of(&path);
            let thumb_info = match (missing, self.thumb(&path)) {
                (true, _) => None,
                (_, Thumb::Ready(tex, size)) => Some((tex.id(), *size)),
                _ => None,
            };
            let failed = match self.thumbs.get(&path) {
                Some(Thumb::Failed(e)) => Some(e.clone()),
                _ => None,
            };
            let row = ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(THUMB_W, THUMB_H), Sense::hover());
                ui.painter().rect_filled(r, 3.0, Color32::BLACK);
                if let Some((tex, (w, h))) = thumb_info {
                    // Fit inside the box.
                    let s = (THUMB_W / w as f32).min(THUMB_H / h as f32);
                    let size = vec2(w as f32 * s, h as f32 * s);
                    let ir = egui::Rect::from_center_size(r.center(), size);
                    ui.painter().image(
                        tex,
                        ir,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else if !missing && failed.is_none() {
                    ui.put(r, egui::Spinner::new().size(14.0));
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(truncate(&name, 26)).color(if missing {
                        theme::error()
                    } else {
                        theme::text()
                    }));
                    let tag = match (missing, kind) {
                        (true, _) => "missing".to_string(),
                        (_, Some(Kind::Video)) => "video".to_string(),
                        _ => "picture".to_string(),
                    };
                    ui.label(RichText::new(tag).small().weak());
                });
            });
            let resp = ui
                .interact(
                    row.response.rect,
                    ui.scope_id().with(("pool", &path)),
                    Sense::click_and_drag(),
                )
                .on_hover_text(match &failed {
                    Some(e) => format!("{}\n\u{E4E0} {e}", path.display()),
                    None => format!(
                        "{}\nDrag into the graph, or double-click to add it.",
                        path.display()
                    ),
                });
            if resp.hovered() {
                ui.painter().rect_stroke(
                    row.response.rect.expand(2.0),
                    4.0,
                    egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.5)),
                    egui::StrokeKind::Outside,
                );
            }
            if resp.drag_started() && !missing {
                egui::DragAndDrop::set_payload(ui.ctx(), MediaDrop::File(path.clone()));
            }
            if resp.dragged() && !missing {
                carry_label(ui, &name);
            }
            if resp.double_clicked() && !missing {
                action = Some(PoolAction::AddToGraph(MediaDrop::File(path.clone())));
            }
            resp.context_menu(|ui| {
                if ui.button("Add to graph").clicked() {
                    action = Some(PoolAction::AddToGraph(MediaDrop::File(path.clone())));
                    ui.close();
                }
                if ui.button("Remove from pool").clicked() {
                    remove = Some(path.clone());
                    ui.close();
                }
            });
        }
        if let Some(p) = remove {
            self.items.retain(|x| *x != p);
            self.thumbs.remove(&p);
        }
        action
    }
}

/// Show what's being carried next to the pointer.
fn carry_label(ui: &egui::Ui, name: &str) {
    if let Some(p) = ui.ctx().pointer_interact_pos() {
        egui::Area::new(egui::Id::unique("pool-drag"))
            .order(egui::Order::Tooltip)
            .fixed_pos(p + vec2(12.0, 12.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(format!("\u{E3D4} {name}"));
                });
            });
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
