//! The node-graph workspace: the graph, its undo history, its evaluator, the
//! editor, the Inspector, and autosave (every 30 s and on exit; restored on
//! the next launch, so a crash loses at most 30 s of work).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Stroke, StrokeKind};
use graph::{
    Command, Endpoint, Graph, Group, GroupId, History, Link, NodeId, OutputRef, ParamValue,
    PublishedParam, Resolution,
};
use nodes::Evaluator;
use nodes::registry::{self, AUDIO_SIGNAL, CatalogueEntry, PROGRAM_OUT};
use scenes::SceneLibrary;
use scenes::uniforms::SIGNAL_NAMES;
use signals::modulation::Source;

use crate::graph_editor::{self, ACCENT, Editor, GROUP_COLOUR, port_colour};
use crate::mod_editor::{self, MOD_COLOR, SIGNAL_SOURCES};

const AUTOSAVE_EVERY: Duration = Duration::from_secs(30);
const ERROR_COLOR: Color32 = Color32::from_rgb(255, 110, 110);

/// Where the autosave lives: `VISUAL_ENGINE_DATA`, else the usual per-user
/// app-data folder.
fn data_folder() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("VISUAL_ENGINE_DATA") {
        return Some(PathBuf::from(d));
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("VisualEngine"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/VisualEngine"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/visual-engine"))
    }
}

/// The natural wire setting for a signal source: hits decay, phases pulse,
/// levels pass through gently.
pub fn default_wire(g: &Graph, from: &OutputRef) -> graph::WireMod {
    let name = g.node(from.node).and_then(|n| {
        if n.kind != AUDIO_SIGNAL {
            return None;
        }
        match n.param("signal").map(|p| &p.value) {
            Some(ParamValue::Choice(i)) => SIGNAL_NAMES.get(*i as usize).map(|s| s.to_string()),
            _ => None,
        }
    });
    let slot = mod_editor::new_slot(match name {
        Some(n) => Source::Signal(n),
        None => Source::Control(0),
    });
    graph::WireMod {
        shape: slot.shape,
        depth: slot.depth,
        enabled: true,
    }
}

/// What the big viewer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewerMode {
    /// The final output (what the audience sees).
    Program,
    /// The selected node's picture.
    Selected,
    /// Split screen: selected node on the left, program on the right.
    Wipe,
}

pub struct Workspace {
    pub graph: Graph,
    pub history: History,
    pub eval: Evaluator,
    pub editor: Editor,
    catalogue: Vec<CatalogueEntry>,
    saved: Graph,
    last_save: Instant,
    path: Option<PathBuf>,
    /// One-off message for the user (e.g. "graph restored").
    pub notice: Option<String>,
    pub viewer: ViewerMode,
    /// Small picture-in-picture of the selected node over the program.
    pub pip: bool,
    /// Wipe line position, 0..1 across the viewer.
    pub wipe: f32,
    /// Live picture of each node (egui textures), for thumbnails, PiP and wipe.
    pub thumbs: HashMap<NodeId, egui::TextureId>,
    /// GPU milliseconds per node and in total (None: the GPU can't tell).
    pub gpu: Option<(HashMap<NodeId, f32>, f32)>,
}

impl Workspace {
    pub fn new(library: &SceneLibrary) -> Self {
        let path = data_folder().map(|d| d.join("autosave.graph.ron"));
        let mut notice = None;
        let restored = path.as_ref().and_then(|p| {
            let text = std::fs::read_to_string(p).ok()?;
            match Graph::from_ron(&text) {
                Ok(g) => {
                    notice = Some("Welcome back. Your graph from last time is restored.".into());
                    Some(g)
                }
                Err(e) => {
                    log::warn!("autosave unreadable ({e}); keeping a copy");
                    let _ = std::fs::rename(p, p.with_extension("broken.ron"));
                    notice = Some("Last session's graph couldn't be read, so here's the starter graph. A copy was kept.".into());
                    None
                }
            }
        });
        let graph = restored
            .or_else(|| nodes::demo::build(library).ok().map(|(g, _)| g))
            .unwrap_or_default();
        Self {
            saved: graph.clone(),
            graph,
            history: History::new(),
            eval: Evaluator::new(1 << 32),
            editor: Editor::default(),
            catalogue: registry::catalogue(library),
            last_save: Instant::now(),
            path,
            notice,
            viewer: ViewerMode::Program,
            pip: true,
            wipe: 0.5,
            thumbs: HashMap::new(),
            gpu: None,
        }
    }

    /// The main output (the first Program Out node).
    pub fn program(&self) -> Option<NodeId> {
        self.graph
            .nodes()
            .iter()
            .find(|n| n.kind == PROGRAM_OUT)
            .map(|n| n.id)
    }

    /// The selected picture node (if exactly one, and it isn't the output).
    pub fn selected_picture(&self) -> Option<NodeId> {
        if self.editor.selected.len() != 1 {
            return None;
        }
        let id = *self.editor.selected.first()?;
        let n = self.graph.node(id)?;
        (n.makes_image() && Some(id) != self.program()).then_some(id)
    }

    /// The node the big viewer shows.
    pub fn viewed(&self) -> Option<NodeId> {
        match self.viewer {
            ViewerMode::Program | ViewerMode::Wipe => self.program(),
            ViewerMode::Selected => self.selected_picture().or_else(|| self.program()),
        }
    }

    /// The nodes to evaluate at full size this frame (program first).
    pub fn targets(&self) -> Vec<NodeId> {
        let mut t: Vec<NodeId> = self.program().into_iter().collect();
        if let Some(v) = self.selected_picture() {
            t.push(v);
        }
        t
    }

    /// Extra nodes to run for their thumbnails and readouts: every signal
    /// node always (cheap), every picture node when thumbnails are on.
    pub fn previews(&self, thumbnails: bool) -> Vec<NodeId> {
        self.graph
            .nodes()
            .iter()
            .filter(|n| (thumbnails && !n.hide_thumb) || !n.makes_image())
            .map(|n| n.id)
            .collect()
    }

    /// Caption for the viewer when it isn't showing the program.
    pub fn viewer_caption(&self) -> Option<String> {
        let label = |id: NodeId| {
            self.graph
                .node(id)
                .map(|n| format!("{:02} {}", id.0, n.label))
        };
        match self.viewer {
            ViewerMode::Selected => self
                .selected_picture()
                .and_then(label)
                .map(|l| format!("Viewing {l}")),
            ViewerMode::Wipe => Some(match self.selected_picture().and_then(label) {
                Some(l) => format!("◀ {l}   |   Program ▶"),
                None => "Wipe: select a node to compare it with the program".into(),
            }),
            ViewerMode::Program => None,
        }
    }

    /// A scene/effect file changed on disk.
    pub fn file_changed(&mut self, file: &str, library: &SceneLibrary) {
        self.eval.invalidate(file);
        self.catalogue = registry::catalogue(library);
    }

    /// Autosave when due. Call once per frame.
    pub fn tick(&mut self) {
        if self.last_save.elapsed() >= AUTOSAVE_EVERY {
            self.save_now();
        }
    }

    pub fn save_now(&mut self) {
        self.last_save = Instant::now();
        if self.graph == self.saved {
            return;
        }
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Write a temporary file, then swap it in, so a crash mid-write never
        // leaves a half-written autosave.
        let tmp = path.with_extension("tmp");
        match std::fs::write(&tmp, self.graph.to_ron()).and_then(|_| std::fs::rename(&tmp, path)) {
            Ok(()) => self.saved = self.graph.clone(),
            Err(e) => log::warn!("autosave failed: {e}"),
        }
    }

    pub fn editor_ui(&mut self, ui: &mut egui::Ui, library: &SceneLibrary) {
        if !ui.input(|i| i.pointer.any_down()) {
            self.history.seal();
        }
        let viewed = self.selected_picture().or_else(|| self.viewed());
        self.editor.show(
            ui,
            graph_editor::Ctx {
                graph: &mut self.graph,
                history: &mut self.history,
                eval: &self.eval,
                catalogue: &self.catalogue,
                library,
                viewed,
                thumbs: &self.thumbs,
                gpu: self.gpu.as_ref().map(|g| &g.0),
            },
        );
    }

    /// The Inspector: everything about the selected node.
    pub fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(n) = &self.notice {
            let mut close = false;
            egui::Frame::group(ui.style())
                .fill(Color32::from_rgb(34, 30, 24))
                .show(ui, |ui| {
                    ui.label(RichText::new(n).color(ACCENT));
                    close = ui.small_button("OK").clicked();
                });
            if close {
                self.notice = None;
            }
            ui.add_space(6.0);
        }
        self.editor.check_focus(&self.graph);
        if let Some(gid) = self.editor.focus_group {
            self.group_inspector(ui, gid);
            return;
        }
        let id = match (self.editor.selected.len(), self.editor.selected.first()) {
            (1, Some(id)) => *id,
            (0, _) => {
                empty_state(
                    ui,
                    "Nothing selected.",
                    "Click a node to tweak it here. Right-click the graph below to add one, \
                     then drag from a plug to wire it up.",
                );
                return;
            }
            (k, _) => {
                empty_state(
                    ui,
                    &format!("{k} nodes selected."),
                    "Ctrl+G groups them, Ctrl+D bypasses them, Del removes them, Ctrl+C \
                     copies them.",
                );
                return;
            }
        };
        let Some(node) = self.graph.node(id).cloned() else {
            return;
        };
        let entry = self.catalogue.iter().find(|e| e.kind == node.kind);

        // Header: number, name, bypass, resolution.
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:02}", id.0)).weak().size(18.0));
            let mut label = node.label.clone();
            let r = ui.add(
                egui::TextEdit::singleline(&mut label)
                    .font(egui::TextStyle::Heading)
                    .desired_width(ui.available_width()),
            );
            if r.changed() {
                let _ = self.history.exec_merge(
                    &mut self.graph,
                    Command::SetLabel { node: id, label },
                    Some(&format!("label{}", id.0)),
                );
            }
        });
        if let Some(e) = entry {
            ui.label(RichText::new(&e.description).small().weak());
        }
        let around = self.graph.groups_around(id);
        if !around.is_empty() {
            let mut pick = None;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("In group").small().weak());
                for gid in around.iter().rev() {
                    if let Some(g) = self.graph.group(*gid)
                        && ui
                            .small_button(RichText::new(&g.label).color(GROUP_COLOUR))
                            .on_hover_text("Show the group and its macro knobs")
                            .clicked()
                    {
                        pick = Some(*gid);
                    }
                }
            });
            if let Some(gid) = pick {
                self.editor.select_group(&self.graph, gid);
            }
        }
        if let Some(ms) = self.gpu.as_ref().and_then(|g| g.0.get(&id)) {
            ui.label(
                RichText::new(format!("GPU {ms:.2} ms per frame"))
                    .small()
                    .weak(),
            )
            .on_hover_text(
                "At 60 fps the whole frame has 16.7 ms. Half or quarter size makes a node cheaper.",
            );
        }
        if let Some(e) = self.eval.errors.get(&id) {
            ui.label(RichText::new(format!("⚠ {e}")).small().color(ERROR_COLOR));
        }
        ui.horizontal(|ui| {
            let mut on = !node.bypass;
            let text = if on { "● On" } else { "○ Bypassed" };
            if ui
                .toggle_value(&mut on, text)
                .on_hover_text("Ctrl+D. Bypassed nodes pass their picture straight through.")
                .changed()
            {
                let _ = self.history.exec(
                    &mut self.graph,
                    Command::SetBypass {
                        node: id,
                        bypass: !on,
                    },
                );
            }
            if node.makes_image() {
                let names = [
                    (Resolution::Inherit, "Full size"),
                    (Resolution::Half, "Half size"),
                    (Resolution::Quarter, "Quarter size"),
                ];
                let current = names
                    .iter()
                    .find(|(r, _)| *r == node.resolution)
                    .map(|(_, n)| *n)
                    .unwrap_or("Custom");
                egui::ComboBox::from_id_salt(("res", id.0))
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (r, name) in names {
                            if ui.selectable_label(node.resolution == r, name).clicked() {
                                let _ = self.history.exec(
                                    &mut self.graph,
                                    Command::SetResolution {
                                        node: id,
                                        resolution: r,
                                    },
                                );
                            }
                        }
                    })
                    .response
                    .on_hover_text("Smaller = faster. Good for blurry or soft layers.");
            }
        });
        ui.separator();

        if node.params.is_empty() {
            let text = if node.kind == PROGRAM_OUT {
                "This is what the audience sees. Plug the last node of your chain into it."
            } else {
                "No settings."
            };
            ui.label(RichText::new(text).weak());
        }
        for p in &node.params {
            self.param_ui(ui, id, p);
            ui.add_space(4.0);
        }
    }

    /// A setting's label, marked ◆ when it's a macro knob of a group.
    fn published_label(&self, node: NodeId, param: &str, label: &str) -> RichText {
        let published = self.graph.groups().iter().any(|g| {
            g.published
                .iter()
                .any(|pp| pp.node == node && pp.param == param)
        });
        if published {
            RichText::new(format!("◆ {label}")).color(GROUP_COLOUR)
        } else {
            RichText::new(label)
        }
    }

    /// Right-click a setting's label: publish it as a macro knob on any
    /// group around the node (or take it off again).
    fn publish_menu(&mut self, resp: &egui::Response, node: NodeId, param: &str, label: &str) {
        let around = self.graph.groups_around(node);
        if around.is_empty() {
            return;
        }
        let resp = resp
            .clone()
            .on_hover_text("Right-click to publish this setting as a macro knob on its group.");
        let mut pick: Option<(GroupId, Option<usize>)> = None;
        resp.context_menu(|ui| {
            for gid in &around {
                let Some(g) = self.graph.group(*gid) else {
                    continue;
                };
                match g
                    .published
                    .iter()
                    .position(|pp| pp.node == node && pp.param == param)
                {
                    Some(i) => {
                        if ui
                            .button(format!("Remove from \"{}\" knobs", g.label))
                            .clicked()
                        {
                            pick = Some((*gid, Some(i)));
                        }
                    }
                    None => {
                        if ui
                            .button(format!("Publish to group \"{}\"", g.label))
                            .on_hover_text(
                                "Shows as a knob on the group's box and in its Inspector",
                            )
                            .clicked()
                        {
                            pick = Some((*gid, None));
                        }
                    }
                }
            }
        });
        let Some((gid, remove)) = pick else { return };
        let Some(g) = self.graph.group(gid) else {
            return;
        };
        let mut g = g.clone();
        match remove {
            Some(i) => {
                g.published.remove(i);
            }
            None => {
                let mut name = label.to_string();
                if g.published.iter().any(|pp| pp.label == name) {
                    let node_label = self
                        .graph
                        .node(node)
                        .map(|n| n.label.clone())
                        .unwrap_or_default();
                    name = format!("{node_label} {label}");
                }
                g.published.push(PublishedParam {
                    label: name,
                    node,
                    param: param.to_string(),
                });
            }
        }
        let _ = self.history.exec(&mut self.graph, Command::PutGroup(g));
    }

    /// The Inspector for a whole group: name, fold, macro knobs, contents.
    fn group_inspector(&mut self, ui: &mut egui::Ui, gid: GroupId) {
        let Some(g) = self.graph.group(gid).cloned() else {
            return;
        };
        let members = self.graph.group_members(gid);
        let put = |ws: &mut Self, g: Group, key: Option<&str>| {
            let _ = ws
                .history
                .exec_merge(&mut ws.graph, Command::PutGroup(g), key);
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("GROUP").small().color(GROUP_COLOUR));
            let mut label = g.label.clone();
            let r = ui.add(
                egui::TextEdit::singleline(&mut label)
                    .font(egui::TextStyle::Heading)
                    .desired_width(ui.available_width()),
            );
            if r.changed() {
                put(
                    self,
                    Group { label, ..g.clone() },
                    Some(&format!("glabel{}", gid.0)),
                );
            }
        });
        let ms: f32 = self
            .gpu
            .as_ref()
            .map(|gpu| members.iter().filter_map(|m| gpu.0.get(m)).sum())
            .unwrap_or(0.0);
        let mut info = format!("{} nodes", members.len());
        if let Some(p) = g.parent.and_then(|p| self.graph.group(p)) {
            info += &format!(" · inside \"{}\"", p.label);
        }
        if ms > 0.0 {
            info += &format!(" · GPU {ms:.2} ms");
        }
        ui.label(RichText::new(info).small().weak());
        ui.horizontal(|ui| {
            let fold = if g.collapsed {
                "▾ Open"
            } else {
                "▸ Fold into a box"
            };
            if ui.button(fold).clicked() {
                put(
                    self,
                    Group {
                        collapsed: !g.collapsed,
                        ..g.clone()
                    },
                    None,
                );
            }
            if ui
                .button("Ungroup")
                .on_hover_text("Ctrl+Shift+G. Keeps the nodes.")
                .clicked()
            {
                let _ = self
                    .history
                    .exec(&mut self.graph, Command::RemoveGroup(gid));
                self.editor.focus_group = None;
            }
        });
        ui.separator();
        ui.label(RichText::new("Macro knobs").strong());
        if g.published.is_empty() {
            ui.label(
                RichText::new(
                    "None yet. Select a node inside, right-click any setting's name → \
                     Publish to group. Its knob then shows here and on the folded box, so \
                     you can play the whole group from a few controls.",
                )
                .small()
                .weak(),
            );
        }
        let mut change: Option<Group> = None;
        for (i, pp) in g.published.iter().enumerate() {
            let Some(param) = self
                .graph
                .node(pp.node)
                .and_then(|n| n.param(&pp.param))
                .cloned()
            else {
                continue;
            };
            ui.push_id(("macro", gid.0, i), |ui| {
                ui.horizontal(|ui| {
                    let mut label = pp.label.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut label).desired_width(120.0))
                        .on_hover_text(format!(
                            "Drives node {} · {}. Type to rename the knob.",
                            pp.node,
                            param.def.label()
                        ))
                        .changed()
                    {
                        let mut ng = g.clone();
                        ng.published[i].label = label;
                        change = Some(ng);
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new("↑").small())
                        .clicked()
                    {
                        let mut ng = g.clone();
                        ng.published.swap(i, i - 1);
                        change = Some(ng);
                    }
                    if ui
                        .add_enabled(i + 1 < g.published.len(), egui::Button::new("↓").small())
                        .clicked()
                    {
                        let mut ng = g.clone();
                        ng.published.swap(i, i + 1);
                        change = Some(ng);
                    }
                    if ui
                        .small_button("✕")
                        .on_hover_text("Remove this knob (the setting stays)")
                        .clicked()
                    {
                        let mut ng = g.clone();
                        ng.published.remove(i);
                        change = Some(ng);
                    }
                });
                self.param_ui(ui, pp.node, &param);
            });
            ui.add_space(4.0);
        }
        if let Some(ng) = change {
            put(self, ng, Some(&format!("gknobs{}", gid.0)));
        }
        ui.separator();
        ui.label(RichText::new("Inside").strong());
        let mut pick = None;
        for m in &members {
            if let Some(n) = self.graph.node(*m)
                && ui
                    .selectable_label(false, format!("{:02}  {}", m.0, n.label))
                    .clicked()
            {
                pick = Some(*m);
            }
        }
        if let Some(m) = pick {
            self.editor.selected = std::collections::BTreeSet::from([m]);
            self.editor.focus_group = None;
        }
    }

    fn param_ui(&mut self, ui: &mut egui::Ui, id: NodeId, p: &graph::Param) {
        let name = p.def.name.clone();
        let key = format!("p{}:{name}", id.0);
        let set = |ws: &mut Self, value: ParamValue| {
            let _ = ws.history.exec_merge(
                &mut ws.graph,
                Command::SetParam {
                    node: id,
                    param: name.clone(),
                    value,
                },
                Some(&key),
            );
        };
        match &p.value {
            ParamValue::Float(v) => {
                let dropping = self.editor.dragging_signal().cloned();
                let frame = egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .corner_radius(4.0);
                let inner = frame.show(ui, |ui| self.float_ui(ui, id, p, *v));
                let rect = inner.response.rect;
                if let Some(from) = dropping {
                    let over = ui.rect_contains_pointer(rect);
                    ui.painter().rect_stroke(
                        rect,
                        4.0,
                        Stroke::new(
                            if over { 2.0 } else { 1.0 },
                            port_colour(graph::PortType::Signal).gamma_multiply(if over {
                                1.0
                            } else {
                                0.4
                            }),
                        ),
                        StrokeKind::Inside,
                    );
                    if over && ui.input(|i| i.pointer.any_released()) {
                        let link = Link {
                            modulation: Some(default_wire(&self.graph, &from)),
                            ..Link::new(from, Endpoint::param(id, &p.def.name))
                        };
                        if let Err(e) = self.history.exec(&mut self.graph, Command::Connect(link)) {
                            self.editor.message = Some((e.to_string(), ui.input(|i| i.time)));
                        }
                        self.editor.end_wire();
                    }
                }
            }
            ParamValue::Bool(b) => {
                let mut b = *b;
                let label = self.published_label(id, &p.def.name, p.def.label());
                let r = ui.checkbox(&mut b, label);
                if r.changed() {
                    set(self, ParamValue::Bool(b));
                }
                self.publish_menu(&r, id, &p.def.name, p.def.label());
            }
            ParamValue::Choice(i) => {
                let friendly = |o: &str| {
                    SIGNAL_SOURCES
                        .iter()
                        .find(|(k, _)| *k == o)
                        .map(|(_, l)| l.to_string())
                        .unwrap_or_else(|| o.replace('_', " "))
                };
                let current = p
                    .def
                    .options
                    .get(*i as usize)
                    .map(|o| friendly(o))
                    .unwrap_or_default();
                let label = self.published_label(id, &p.def.name, p.def.label());
                let label_resp = ui.horizontal(|ui| {
                    let r = ui.label(label);
                    egui::ComboBox::from_id_salt(("choice", id.0, &p.def.name))
                        .selected_text(current)
                        .height(360.0)
                        .show_ui(ui, |ui| {
                            for (k, o) in p.def.options.iter().enumerate() {
                                if ui.selectable_label(k as u32 == *i, friendly(o)).clicked() {
                                    set(self, ParamValue::Choice(k as u32));
                                }
                            }
                        });
                    r
                });
                self.publish_menu(&label_resp.inner, id, &p.def.name, p.def.label());
            }
            ParamValue::Colour(c) => {
                let mut c = *c;
                ui.horizontal(|ui| {
                    ui.label(p.def.label());
                    if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                        set(self, ParamValue::Colour(c));
                    }
                });
            }
            ParamValue::Text(t) => {
                let mut t = t.clone();
                ui.horizontal(|ui| {
                    ui.label(p.def.label());
                    if ui.text_edit_singleline(&mut t).changed() {
                        set(self, ParamValue::Text(t));
                    }
                });
            }
        }
    }

    /// A number slider with its modulations and Signal wires.
    fn float_ui(&mut self, ui: &mut egui::Ui, id: NodeId, p: &graph::Param, base: f32) {
        let name = &p.def.name;
        let (lo, hi) = (p.def.min, p.def.max);
        let live = self.eval.live_value(id, name);
        let wires: Vec<Link> = self.graph.param_links(id, name).cloned().collect();
        let modulated = !wires.is_empty() || p.mods.iter().any(|m| m.enabled);
        let open_id = ui.scope_id().with(("mods-open", id.0, name));
        let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
        let label = self.published_label(id, name, p.def.label());
        let label_resp = ui.horizontal(|ui| {
            let r = ui.label(label);
            if modulated && let Some(v) = live {
                ui.label(RichText::new(format!("→ {v:.2}")).small().color(MOD_COLOR));
            }
            r
        });
        self.publish_menu(&label_resp.inner, id, name, p.def.label());
        ui.horizontal(|ui| {
            let mut v = base;
            ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(60.0);
            let resp = ui.add(egui::Slider::new(&mut v, lo..=hi).show_value(false));
            if modulated && let Some(l) = live {
                let r = resp.rect;
                let t = ((l - lo) / (hi - lo)).clamp(0.0, 1.0);
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
                egui::DragValue::new(&mut v)
                    .speed((hi - lo) / 300.0)
                    .range(lo..=hi)
                    .max_decimals(3),
            );
            if v != base {
                let _ = self.history.exec_merge(
                    &mut self.graph,
                    Command::SetParam {
                        node: id,
                        param: name.clone(),
                        value: ParamValue::Float(v),
                    },
                    Some(&format!("p{}:{name}", id.0)),
                );
            }
            let n = p.mods.len();
            let label = if n == 0 {
                "~".to_string()
            } else {
                format!("~{n}")
            };
            let btn = egui::Button::new(RichText::new(label).color(if modulated {
                MOD_COLOR
            } else {
                ui.visuals().text_color()
            }))
            .selected(open);
            if ui
                .add(btn)
                .on_hover_text(
                    "Modulate: make this follow the music, the beat, an LFO or a macro. \
                     Or drag a blue wire from a Signal node onto it.",
                )
                .clicked()
            {
                open = !open;
            }
        });
        // Signal wires plugged into this slider.
        for l in &wires {
            let Some(m) = l.modulation else { continue };
            let src = self
                .graph
                .node(l.from.node)
                .map(|n| format!("{:02} {}", n.id.0, n.label))
                .unwrap_or_default();
            ui.horizontal(|ui| {
                let mut m2 = m;
                ui.checkbox(&mut m2.enabled, "");
                ui.label(
                    RichText::new(format!("⟵ {src}"))
                        .small()
                        .color(port_colour(graph::PortType::Signal)),
                );
                egui::ComboBox::from_id_salt(("wshape", id.0, name, l.from.node.0))
                    .selected_text(m2.shape.name())
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for s in mod_editor::SHAPES {
                            if ui
                                .selectable_label(
                                    std::mem::discriminant(&s) == std::mem::discriminant(&m2.shape),
                                    s.name(),
                                )
                                .clicked()
                            {
                                m2.shape = s;
                            }
                        }
                    });
                ui.add(
                    egui::DragValue::new(&mut m2.depth)
                        .speed(0.005)
                        .range(-1.0..=1.0)
                        .max_decimals(2),
                )
                .on_hover_text(
                    "How far it moves the slider (fraction of its range; negative = the other way)",
                );
                if m2 != m {
                    let _ = self.history.exec_merge(
                        &mut self.graph,
                        Command::SetWireMod {
                            from: l.from.clone(),
                            to: l.to.clone(),
                            modulation: m2,
                        },
                        Some(&format!("w{}:{name}:{}", id.0, l.from.node.0)),
                    );
                }
                if ui.small_button("✕").on_hover_text("Unplug").clicked() {
                    let _ = self
                        .history
                        .exec(&mut self.graph, Command::Disconnect(l.clone()));
                }
            });
        }
        if open {
            egui::Frame::group(ui.style())
                .fill(Color32::from_rgb(28, 26, 22))
                .show(ui, |ui| {
                    let mut mods = p.mods.clone();
                    let mut remove = None;
                    for (k, slot) in mods.iter_mut().enumerate() {
                        if k > 0 {
                            ui.separator();
                        }
                        if mod_editor::slot_ui(ui, slot, (id.0 as usize) * 1000 + k) {
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
                                if let Some(s) = mod_editor::source_menu(ui) {
                                    mods.push(mod_editor::new_slot(s));
                                }
                            });
                    });
                    if mods != p.mods {
                        let _ = self.history.exec_merge(
                            &mut self.graph,
                            Command::SetMods {
                                node: id,
                                param: name.clone(),
                                mods,
                            },
                            Some(&format!("m{}:{name}", id.0)),
                        );
                    }
                });
        }
        ui.data_mut(|d| d.insert_temp(open_id, open));
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.save_now();
    }
}

fn empty_state(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(title)
                .size(16.0)
                .color(Color32::from_gray(200)),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(body).weak());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use graph::{Command, PortType};
    use signals::modulation::Shape;

    #[test]
    fn wires_from_hits_decay_and_levels_pass() {
        let mut g = Graph::new();
        let spec = registry::builtin_spec(AUDIO_SIGNAL).unwrap();
        let n = g.new_node(&spec);
        let id = n.id;
        g.apply(Command::AddNode(Box::new(n))).unwrap();
        let from = OutputRef::new(id, registry::VALUE_OUT);
        assert_eq!(spec.outputs[0].ty, PortType::Signal);
        // Default signal is a level: passes straight through.
        assert_eq!(default_wire(&g, &from).shape, Shape::Linear);
        g.apply(Command::SetParam {
            node: id,
            param: "signal".into(),
            value: ParamValue::Choice(registry::signal_choice("kick_age")),
        })
        .unwrap();
        assert!(matches!(
            default_wire(&g, &from).shape,
            Shape::Envelope { .. }
        ));
    }
}
