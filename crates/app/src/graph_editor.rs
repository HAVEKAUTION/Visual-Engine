//! The node graph editor (Resolve-style): a pan/zoom canvas with numbered
//! nodes, colour-coded plugs and wires, live thumbnails, Frames (coloured
//! backdrops that keep areas tidy), snap-to-grid, a right-click add menu,
//! Shift+Space search, S/P/L quick insert, selection, copy/paste, delete,
//! bypass and undo/redo.
//!
//! Mouse: drag from a plug to wire it; drag a wire off an input to unplug it;
//! drop a wire on empty space to add a node already wired to it; drop a blue
//! Signal wire on a node (or on a slider in the Inspector) to make it drive
//! that slider. Scroll = zoom, middle-drag (or Space/Alt + drag) = pan.

use std::collections::{BTreeSet, HashMap};

use egui::epaint::CubicBezierShape;
use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Key, LayerId, Order, PointerButton, Pos2, Rect,
    Sense, Stroke, StrokeKind, Vec2, pos2, vec2,
};
use graph::{
    Command, Endpoint, Frame, Graph, History, Link, Node, NodeId, OutputRef, ParamValue, PortType,
};
use nodes::Evaluator;
use nodes::registry::{self, CatalogueEntry, Category};
use scenes::SceneLibrary;
use serde::{Deserialize, Serialize};

use crate::theme;

pub const ACCENT: Color32 = theme::ACCENT;
pub const CANVAS_BG: Color32 = theme::DEEP;
const GRID_DOT: Color32 = Color32::from_rgb(36, 36, 42);
const GRID_DOT_MAJOR: Color32 = Color32::from_rgb(50, 50, 58);
const NODE_BG: Color32 = Color32::from_rgb(36, 36, 42);
const NODE_HEADER: Color32 = Color32::from_rgb(48, 48, 56);
const TEXT: Color32 = theme::TEXT;
const TEXT_DIM: Color32 = Color32::from_gray(135);
const ERROR: Color32 = theme::ERROR;

const NODE_W: f32 = 170.0;
const HEADER_H: f32 = 24.0;
const ROW_H: f32 = 18.0;
const PORT_R: f32 = 5.0;
const THUMB_H: f32 = (NODE_W - 12.0) * 9.0 / 16.0;
/// Snap grid (graph units).
pub const GRID: f32 = 16.0;
const FRAME_TITLE_H: f32 = 24.0;

/// Colour tags offered for nodes and frames.
pub const TAG_COLOURS: [[u8; 3]; 8] = [
    [226, 92, 92],
    [236, 148, 64],
    [230, 204, 80],
    [110, 200, 100],
    [70, 190, 180],
    [80, 150, 240],
    [150, 110, 240],
    [220, 100, 190],
];

/// Wire / plug colour per data type.
pub fn port_colour(ty: PortType) -> Color32 {
    match ty {
        PortType::Image => Color32::from_gray(190),
        PortType::Mask => Color32::from_rgb(90, 210, 120),
        PortType::Signal => Color32::from_rgb(80, 160, 255),
        PortType::Trigger => Color32::from_rgb(255, 150, 50),
    }
}

pub fn category_colour(c: Category) -> Color32 {
    match c {
        Category::Source => Color32::from_rgb(70, 190, 180),
        Category::Effect => Color32::from_rgb(160, 120, 255),
        Category::Signal => port_colour(PortType::Signal),
        Category::Trigger => port_colour(PortType::Trigger),
        Category::Output => ACCENT,
    }
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn category(n: &Node) -> Category {
    registry::category_of(
        &n.kind,
        n.outputs.iter().any(|p| p.ty == PortType::Image),
        n.inputs.iter().any(|p| p.ty == PortType::Image),
    )
}

fn type_name(ty: PortType) -> &'static str {
    match ty {
        PortType::Image => "picture",
        PortType::Mask => "mask",
        PortType::Signal => "signal",
        PortType::Trigger => "trigger",
    }
}

/// What is under the pointer.
#[derive(Clone, Debug, PartialEq)]
enum Hit {
    None,
    Node(NodeId),
    Input(NodeId, String),
    Output(NodeId, String),
    /// The row of a parameter that has Signal wires plugged in.
    ModRow(NodeId, String),
    FrameTitle(u32),
    FrameResize(u32),
}

/// Where a node's parts sit, in graph units.
struct Layout {
    rect: Rect,
    inputs: Vec<(String, String, PortType, Pos2)>,
    outputs: Vec<(String, String, PortType, Pos2)>,
    /// Parameters with Signal wires: (param name, label, plug position).
    mods: Vec<(String, String, Pos2)>,
    thumb: Option<Rect>,
}

fn layout(g: &Graph, n: &Node, thumbs: bool) -> Layout {
    let min = pos2(n.pos[0], n.pos[1]);
    let row_y = |i: usize| min.y + HEADER_H + ROW_H * (i as f32 + 0.5) + 2.0;
    let ports = |list: &[graph::PortDef], x: f32| -> Vec<(String, String, PortType, Pos2)> {
        list.iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.name.clone(),
                    p.label().to_string(),
                    p.ty,
                    pos2(x, row_y(i)),
                )
            })
            .collect()
    };
    let inputs = ports(&n.inputs, min.x);
    let outputs = ports(&n.outputs, min.x + NODE_W);
    let first = inputs.len().max(outputs.len());
    let mods: Vec<_> = n
        .params
        .iter()
        .filter(|p| g.param_links(n.id, &p.def.name).next().is_some())
        .enumerate()
        .map(|(i, p)| {
            (
                p.def.name.clone(),
                p.def.label().to_string(),
                pos2(min.x, row_y(first + i)),
            )
        })
        .collect();
    let rows = (first + mods.len()).max(1);
    let body = HEADER_H + ROW_H * rows as f32 + 8.0;
    let thumb = (thumbs && n.makes_image())
        .then(|| Rect::from_min_size(min + vec2(6.0, body), vec2(NODE_W - 12.0, THUMB_H)));
    let h = body + thumb.map_or(0.0, |t| t.height() + 6.0);
    Layout {
        rect: Rect::from_min_size(min, vec2(NODE_W, h)),
        inputs,
        outputs,
        mods,
        thumb,
    }
}

fn frame_rect(f: &Frame) -> Rect {
    Rect::from_min_size(pos2(f.rect[0], f.rect[1]), vec2(f.rect[2], f.rect[3]))
}

fn snap(v: f32, on: bool) -> f32 {
    if on { (v / GRID).round() * GRID } else { v }
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    None,
    /// Moving nodes (and frames carried along): start positions + total offset.
    Nodes {
        start: Vec<(NodeId, [f32; 2])>,
        offset: Vec2,
    },
    Frame {
        id: u32,
        start: [f32; 4],
        nodes: Vec<(NodeId, [f32; 2])>,
        offset: Vec2,
    },
    FrameResize {
        id: u32,
        start: [f32; 4],
        offset: Vec2,
    },
    Wire {
        from: OutputRef,
        ty: PortType,
    },
    Box {
        start: Pos2,
    },
    Pan,
}

/// What a newly added node should be wired into.
#[derive(Clone, Debug, PartialEq)]
enum Insert {
    None,
    /// S: after this node, taking over its outputs.
    Serial(NodeId),
    /// P: beside this node, blended 50/50 with it by a new Merge.
    Parallel(NodeId),
    /// L: on top of this node, by a new Merge.
    Layer(NodeId),
    /// Dropped a wire on empty space: plug it into the new node.
    FromWire(OutputRef, PortType),
}

struct Search {
    text: String,
    /// Where to put the node (graph units).
    at: Pos2,
    /// Where the box sits on screen.
    screen: Pos2,
    pick: usize,
    focus: bool,
    insert: Insert,
}

/// Copied nodes (and the wires between them), as RON on the clipboard, so
/// they can be pasted into another graph too.
#[derive(Serialize, Deserialize)]
struct Clip {
    nodes: Vec<Node>,
    links: Vec<Link>,
    #[serde(default)]
    frames: Vec<Frame>,
}

const CLIP_TAG: &str = "visual-engine-nodes ";

pub struct Editor {
    pan: Vec2,
    zoom: f32,
    drag: Drag,
    pub selected: BTreeSet<NodeId>,
    menu_target: Hit,
    /// Graph position of the pointer / last right-click (where nodes go).
    menu_at: Pos2,
    search: Option<Search>,
    /// A Signal wire was dropped on a node: pick which slider it drives.
    param_pick: Option<(OutputRef, NodeId, Pos2)>,
    /// Renaming a frame: (id, text, screen position).
    frame_rename: Option<(u32, String, Pos2)>,
    framed: bool,
    /// When each trigger node last fired (for the flash).
    fired: HashMap<NodeId, f64>,
    /// Short message for the user (bad wire etc.).
    pub message: Option<(String, f64)>,
    pub snap: bool,
    pub thumbnails: bool,
    pub show_help: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            drag: Drag::None,
            selected: BTreeSet::new(),
            menu_target: Hit::None,
            menu_at: Pos2::ZERO,
            search: None,
            param_pick: None,
            frame_rename: None,
            framed: false,
            fired: HashMap::new(),
            message: None,
            snap: true,
            thumbnails: true,
            show_help: false,
        }
    }
}

/// Everything the editor works on.
pub struct Ctx<'a> {
    pub graph: &'a mut Graph,
    pub history: &'a mut History,
    pub eval: &'a Evaluator,
    pub catalogue: &'a [CatalogueEntry],
    pub library: &'a SceneLibrary,
    /// Node shown in the viewer.
    pub viewed: Option<NodeId>,
    /// Live picture of each node, for thumbnails.
    pub thumbs: &'a HashMap<NodeId, egui::TextureId>,
}

impl Editor {
    /// A blue Signal wire is being dragged (the Inspector accepts drops).
    pub fn dragging_signal(&self) -> Option<&OutputRef> {
        match &self.drag {
            Drag::Wire {
                from,
                ty: PortType::Signal,
            } => Some(from),
            _ => None,
        }
    }

    /// The Inspector took the dropped wire.
    pub fn end_wire(&mut self) {
        self.drag = Drag::None;
    }

    /// Open the search-to-add box (Shift+Space).
    pub fn open_search(&mut self, ctx: &egui::Context) {
        self.open_search_with(ctx, Insert::None);
    }

    fn open_search_with(&mut self, ctx: &egui::Context, insert: Insert) {
        let screen = ctx
            .pointer_hover_pos()
            .unwrap_or_else(|| ctx.content_rect().center());
        self.search = Some(Search {
            text: String::new(),
            at: self.menu_at,
            screen,
            pick: 0,
            focus: true,
            insert,
        });
    }

    fn say(&mut self, ui: &egui::Ui, msg: impl Into<String>) {
        self.message = Some((msg.into(), ui.input(|i| i.time)));
    }

    pub fn show(&mut self, ui: &mut egui::Ui, mut c: Ctx) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let now = ui.input(|i| i.time);
        let thumbs_on = self.thumbnails;
        if !self.framed {
            self.frame_all(c.graph, rect);
            self.framed = true;
        }
        let origin = |s: &Self| rect.min + s.pan;
        let to_screen = |s: &Self, p: Pos2| origin(s) + p.to_vec2() * s.zoom;
        let to_graph = |s: &Self, p: Pos2| ((p - origin(s)) / s.zoom).to_pos2();

        // Zoom around the pointer.
        if resp.hovered() && self.search.is_none() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let f = (scroll * 0.0015).exp() * pinch;
            if (f - 1.0).abs() > 1e-4
                && let Some(p) = resp.hover_pos()
            {
                let before = to_graph(self, p);
                self.zoom = (self.zoom * f).clamp(0.25, 2.5);
                self.pan = p - rect.min - before.to_vec2() * self.zoom;
            }
        }
        let pointer = ui.input(|i| i.pointer.hover_pos());
        if let Some(p) = pointer
            && rect.contains(p)
            && self.search.is_none()
        {
            self.menu_at = to_graph(self, p);
        }

        // Trigger flashes.
        for n in c.graph.nodes() {
            if n.outputs.iter().any(|o| o.ty == PortType::Trigger)
                && c.eval.signal_value(n.id).is_some_and(|v| v > 0.5)
            {
                self.fired.insert(n.id, now);
            }
        }
        self.fired.retain(|id, _| c.graph.node(*id).is_some());

        let layouts: Vec<(NodeId, Layout)> = c
            .graph
            .nodes()
            .iter()
            .map(|n| (n.id, layout(c.graph, n, thumbs_on)))
            .collect();
        let frames: Vec<Frame> = c.graph.frames().to_vec();
        let hit = |s: &Self, p: Pos2| -> Hit {
            let gp = to_graph(s, p);
            let r = (PORT_R * 1.8).max(8.0 / s.zoom);
            for (id, l) in layouts.iter().rev() {
                for (name, _, _, at) in &l.inputs {
                    if at.distance(gp) < r {
                        return Hit::Input(*id, name.clone());
                    }
                }
                for (name, _, _, at) in &l.outputs {
                    if at.distance(gp) < r {
                        return Hit::Output(*id, name.clone());
                    }
                }
                for (name, _, at) in &l.mods {
                    if at.distance(gp) < r {
                        return Hit::ModRow(*id, name.clone());
                    }
                }
            }
            for (id, l) in layouts.iter().rev() {
                if l.rect.contains(gp) {
                    return Hit::Node(*id);
                }
            }
            for f in frames.iter().rev() {
                let fr = frame_rect(f);
                let handle = Rect::from_min_max(fr.max - vec2(18.0, 18.0), fr.max);
                if handle.contains(gp) {
                    return Hit::FrameResize(f.id);
                }
                let title = Rect::from_min_size(fr.min, vec2(fr.width(), FRAME_TITLE_H));
                if title.contains(gp) {
                    return Hit::FrameTitle(f.id);
                }
            }
            Hit::None
        };

        // ---- Mouse ----
        let (space, mods) = ui.input(|i| (i.key_down(Key::Space), i.modifiers));
        let add_select = mods.shift || mods.command;
        if resp.dragged_by(PointerButton::Middle)
            || (resp.dragged_by(PointerButton::Primary)
                && (space || mods.alt)
                && matches!(self.drag, Drag::None | Drag::Pan))
        {
            self.pan += resp.drag_delta();
            self.drag = Drag::Pan;
        } else if resp.drag_started_by(PointerButton::Primary) {
            let start = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
            match hit(self, start) {
                Hit::Output(n, port) => {
                    let ty = c
                        .graph
                        .node(n)
                        .and_then(|n| n.output(&port))
                        .map(|p| p.ty)
                        .unwrap_or(PortType::Image);
                    self.drag = Drag::Wire {
                        from: OutputRef::new(n, &port),
                        ty,
                    };
                }
                Hit::Input(n, port) => {
                    if let Some(l) = c.graph.input_link(n, &port).cloned() {
                        let ty = c
                            .graph
                            .node(l.from.node)
                            .and_then(|s| s.output(&l.from.port))
                            .map(|p| p.ty)
                            .unwrap_or(PortType::Image);
                        let _ = c.history.exec(c.graph, Command::Disconnect(l.clone()));
                        self.drag = Drag::Wire { from: l.from, ty };
                    }
                }
                Hit::ModRow(n, param) => {
                    if let Some(l) = c.graph.param_links(n, &param).last().cloned() {
                        let _ = c.history.exec(c.graph, Command::Disconnect(l.clone()));
                        self.drag = Drag::Wire {
                            from: l.from,
                            ty: PortType::Signal,
                        };
                    }
                }
                Hit::Node(n) => {
                    if !self.selected.contains(&n) {
                        if !add_select {
                            self.selected.clear();
                        }
                        self.selected.insert(n);
                    }
                    let start = self
                        .selected
                        .iter()
                        .filter_map(|id| c.graph.node(*id))
                        .map(|n| (n.id, n.pos))
                        .collect();
                    self.drag = Drag::Nodes {
                        start,
                        offset: Vec2::ZERO,
                    };
                }
                Hit::FrameTitle(id) => {
                    if let Some(f) = c.graph.frame(id) {
                        let fr = frame_rect(f);
                        let nodes = layouts
                            .iter()
                            .filter(|(_, l)| fr.contains(l.rect.center()))
                            .filter_map(|(id, _)| c.graph.node(*id).map(|n| (n.id, n.pos)))
                            .collect();
                        self.drag = Drag::Frame {
                            id,
                            start: f.rect,
                            nodes,
                            offset: Vec2::ZERO,
                        };
                    }
                }
                Hit::FrameResize(id) => {
                    if let Some(f) = c.graph.frame(id) {
                        self.drag = Drag::FrameResize {
                            id,
                            start: f.rect,
                            offset: Vec2::ZERO,
                        };
                    }
                }
                Hit::None => {
                    if !add_select {
                        self.selected.clear();
                    }
                    self.drag = Drag::Box { start };
                }
            }
        } else if resp.dragged_by(PointerButton::Primary) {
            let d = resp.drag_delta() / self.zoom;
            let snap_on = self.snap;
            let mut cmds = Vec::new();
            match &mut self.drag {
                Drag::Nodes { start, offset } => {
                    *offset += d;
                    for (id, p) in start.iter() {
                        cmds.push(Command::Move {
                            node: *id,
                            pos: [
                                snap(p[0] + offset.x, snap_on),
                                snap(p[1] + offset.y, snap_on),
                            ],
                        });
                    }
                }
                Drag::Frame {
                    id,
                    start,
                    nodes,
                    offset,
                } => {
                    *offset += d;
                    let dx = snap(start[0] + offset.x, snap_on) - start[0];
                    let dy = snap(start[1] + offset.y, snap_on) - start[1];
                    if let Some(f) = c.graph.frame(*id) {
                        cmds.push(Command::PutFrame(Frame {
                            rect: [start[0] + dx, start[1] + dy, start[2], start[3]],
                            ..f.clone()
                        }));
                    }
                    for (n, p) in nodes.iter() {
                        cmds.push(Command::Move {
                            node: *n,
                            pos: [p[0] + dx, p[1] + dy],
                        });
                    }
                }
                Drag::FrameResize { id, start, offset } => {
                    *offset += d;
                    if let Some(f) = c.graph.frame(*id) {
                        let w = snap(start[2] + offset.x, snap_on).max(120.0);
                        let h = snap(start[3] + offset.y, snap_on).max(60.0);
                        cmds.push(Command::PutFrame(Frame {
                            rect: [start[0], start[1], w, h],
                            ..f.clone()
                        }));
                    }
                }
                _ => {}
            }
            if !cmds.is_empty() {
                let _ = c
                    .history
                    .exec_merge(c.graph, Command::Batch(cmds), Some("move"));
            }
        }
        if resp.drag_stopped() {
            let end = resp.interact_pointer_pos().or(pointer);
            match std::mem::replace(&mut self.drag, Drag::None) {
                Drag::Wire { from, ty } => {
                    if let Some(p) = end.filter(|p| rect.contains(*p)) {
                        let target = hit(self, p);
                        self.drop_wire(ui, &mut c, from, ty, target, p);
                    }
                }
                Drag::Box { start } => {
                    if let Some(p) = end {
                        let b = Rect::from_two_pos(to_graph(self, start), to_graph(self, p));
                        for (id, l) in &layouts {
                            if b.intersects(l.rect) {
                                self.selected.insert(*id);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            match hit(self, p) {
                Hit::Node(n) | Hit::Input(n, _) | Hit::Output(n, _) | Hit::ModRow(n, _) => {
                    if add_select {
                        if !self.selected.remove(&n) {
                            self.selected.insert(n);
                        }
                    } else {
                        self.selected = BTreeSet::from([n]);
                    }
                }
                _ => self.selected.clear(),
            }
        }
        if resp.double_clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            match hit(self, p) {
                Hit::None => self.open_search(ui.ctx()),
                Hit::FrameTitle(id) => {
                    if let Some(f) = c.graph.frame(id) {
                        self.frame_rename = Some((id, f.label.clone(), p));
                    }
                }
                _ => {}
            }
        }
        if resp.secondary_clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.menu_target = hit(self, p);
            if let Hit::Node(n) | Hit::Input(n, _) | Hit::Output(n, _) | Hit::ModRow(n, _) =
                self.menu_target
                && !self.selected.contains(&n)
            {
                self.selected = BTreeSet::from([n]);
            }
        }
        let mut action: Option<MenuAction> = None;
        let selected_count = self.selected.len();
        let menu_target = self.menu_target.clone();
        resp.context_menu(|ui| {
            action = match &menu_target {
                Hit::None => add_menu(ui, c.catalogue, selected_count),
                Hit::FrameTitle(id) | Hit::FrameResize(id) => frame_menu(ui, *id),
                _ => node_menu(ui, selected_count),
            };
        });

        // Hover help: what a node or plug is.
        if matches!(self.drag, Drag::None)
            && self.search.is_none()
            && let Some(p) = pointer.filter(|p| rect.contains(*p))
        {
            let tip = match hit(self, p) {
                Hit::Input(n, port) | Hit::Output(n, port) => c.graph.node(n).and_then(|node| {
                    let def = node.input(&port).or_else(|| node.output(&port))?;
                    let mut t = format!(
                        "{} · {} plug",
                        if def.label().is_empty() {
                            def.name.as_str()
                        } else {
                            def.label()
                        },
                        type_name(def.ty)
                    );
                    if def.ty == PortType::Mask {
                        t += "\nLimits where this node's effect shows (masks arrive in M5).";
                    }
                    Some(t)
                }),
                Hit::Node(n) => c.graph.node(n).map(|node| {
                    let desc = c
                        .catalogue
                        .iter()
                        .find(|e| e.kind == node.kind)
                        .map(|e| e.description.clone())
                        .unwrap_or_default();
                    match c.eval.errors.get(&n) {
                        Some(e) => format!("{desc}\n\n⚠ {e}"),
                        None => desc,
                    }
                }),
                _ => None,
            };
            if let Some(t) = tip.filter(|t| !t.is_empty()) {
                resp.clone().on_hover_text_at_pointer(t);
            }
        }

        // ---- Keyboard ----
        let typing = ui.ctx().egui_wants_keyboard_input();
        // Editing keys only act with the mouse over the graph, so a stray key
        // elsewhere never deletes anything.
        let over = resp.hovered() || resp.contains_pointer();
        if !typing {
            let keys = ui.input_mut(|i| {
                let cmd = i.modifiers.command;
                let none = egui::Modifiers::NONE;
                Keys {
                    events: i.events.clone(),
                    del: !cmd && (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)),
                    redo: i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z)
                        || i.consume_key(egui::Modifiers::COMMAND, Key::Y),
                    undo: i.consume_key(egui::Modifiers::COMMAND, Key::Z),
                    bypass: i.consume_key(egui::Modifiers::COMMAND, Key::D),
                    all: i.consume_key(egui::Modifiers::COMMAND, Key::A),
                    home: i.key_pressed(Key::Home),
                    serial: !cmd && i.key_pressed(Key::S),
                    parallel: !cmd && i.key_pressed(Key::P),
                    layer: !cmd && i.key_pressed(Key::L),
                    grid: !cmd && i.key_pressed(Key::G),
                    help: i.consume_key(none, Key::Questionmark)
                        || i.consume_key(egui::Modifiers::SHIFT, Key::Questionmark)
                        || i.consume_key(egui::Modifiers::SHIFT, Key::Slash),
                    esc: i.key_pressed(Key::Escape),
                }
            });
            if keys.redo {
                let _ = c.history.redo(c.graph);
            } else if keys.undo {
                let _ = c.history.undo(c.graph);
            }
            if keys.help {
                self.show_help = !self.show_help;
            }
            if keys.esc {
                self.show_help = false;
            }
            if over {
                if keys.del {
                    action = Some(MenuAction::Delete);
                }
                if keys.bypass {
                    action = Some(MenuAction::Bypass);
                }
                if keys.all {
                    self.selected = c.graph.nodes().iter().map(|n| n.id).collect();
                }
                if keys.home {
                    self.frame_all(c.graph, rect);
                }
                if keys.grid {
                    self.snap = !self.snap;
                    self.say(
                        ui,
                        if self.snap {
                            "Snap to grid on"
                        } else {
                            "Snap to grid off"
                        },
                    );
                }
                let one = (self.selected.len() == 1)
                    .then(|| *self.selected.first().unwrap())
                    .filter(|id| c.graph.node(*id).is_some_and(|n| n.makes_image()));
                for (pressed, make) in [
                    (keys.serial, Insert::Serial as fn(NodeId) -> Insert),
                    (keys.parallel, Insert::Parallel),
                    (keys.layer, Insert::Layer),
                ] {
                    if pressed {
                        match one {
                            Some(id) => {
                                if let Some(n) = c.graph.node(id) {
                                    self.menu_at = pos2(
                                        n.pos[0] + NODE_W * 1.5 + 48.0,
                                        n.pos[1] + HEADER_H * 0.5,
                                    );
                                }
                                self.open_search_with(ui.ctx(), make(id));
                            }
                            None => self.say(
                                ui,
                                "Select one picture node first (S / P / L add after it).",
                            ),
                        }
                    }
                }
            }
            for e in keys.events {
                match e {
                    egui::Event::Copy if over => action = Some(MenuAction::Copy),
                    egui::Event::Cut if over => action = Some(MenuAction::Cut),
                    egui::Event::Paste(text) if over => {
                        self.paste(ui, &mut c, &text, None);
                    }
                    _ => {}
                }
            }
        }
        if let Some(a) = action {
            self.run_action(ui, &mut c, a);
        }

        // ---- Drawing ----
        painter.rect_filled(rect, 0.0, CANVAS_BG);
        let step = GRID * self.zoom;
        if step > 5.0 {
            let o = origin(self);
            let first = |o: f32, lo: f32| lo + (o - lo).rem_euclid(step);
            let mut x = first(o.x, rect.left());
            while x < rect.right() {
                let gx = ((x - o.x) / step).round() as i64;
                let mut y = first(o.y, rect.top());
                while y < rect.bottom() {
                    let gy = ((y - o.y) / step).round() as i64;
                    let major = gx % 8 == 0 && gy % 8 == 0;
                    painter.circle_filled(
                        pos2(x, y),
                        if major { 1.4 } else { 0.9 },
                        if major { GRID_DOT_MAJOR } else { GRID_DOT },
                    );
                    y += step;
                }
                x += step;
            }
        }
        let z = self.zoom;
        // Frames (under everything).
        for f in c.graph.frames() {
            let r = Rect::from_min_max(
                to_screen(self, frame_rect(f).min),
                to_screen(self, frame_rect(f).max),
            );
            let col = rgb(f.colour);
            painter.rect_filled(r, 6.0, col.gamma_multiply(0.09));
            painter.rect_stroke(
                r,
                6.0,
                Stroke::new(1.0, col.gamma_multiply(0.45)),
                StrokeKind::Inside,
            );
            let title = Rect::from_min_size(r.min, vec2(r.width(), FRAME_TITLE_H * z));
            painter.rect_filled(
                title,
                CornerRadius {
                    nw: 6,
                    ne: 6,
                    sw: 0,
                    se: 0,
                },
                col.gamma_multiply(0.28),
            );
            painter.text(
                title.left_center() + vec2(10.0 * z, 0.0),
                Align2::LEFT_CENTER,
                &f.label,
                FontId::proportional((13.0 * z).clamp(7.0, 22.0)),
                TEXT,
            );
            // Resize grip.
            let g = r.max - vec2(5.0, 5.0);
            for k in 0..3 {
                let o = 4.0 * k as f32;
                painter.line_segment(
                    [g - vec2(10.0 - o, 0.0), g - vec2(0.0, 10.0 - o)],
                    Stroke::new(1.0, col.gamma_multiply(0.6)),
                );
            }
        }
        let lay: HashMap<NodeId, &Layout> = layouts.iter().map(|(id, l)| (*id, l)).collect();
        let wire_w = (2.0 * z).clamp(1.0, 3.0);
        for l in c.graph.links() {
            let (Some(a), Some(b)) = (lay.get(&l.from.node), lay.get(&l.to.node())) else {
                continue;
            };
            let Some((_, _, ty, from)) = a.outputs.iter().find(|o| o.0 == l.from.port) else {
                continue;
            };
            let to = match &l.to {
                Endpoint::Input { port, .. } => b.inputs.iter().find(|i| &i.0 == port).map(|i| i.3),
                Endpoint::Param { param, .. } => b.mods.iter().find(|m| &m.0 == param).map(|m| m.2),
            };
            let Some(to) = to else { continue };
            let mut col = port_colour(*ty);
            let mut w = wire_w;
            match ty {
                PortType::Signal => {
                    // Wires glow with their signal.
                    let v = c
                        .eval
                        .signal_value(l.from.node)
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    col = lerp_colour(Color32::from_rgb(40, 80, 140), col, 0.35 + 0.65 * v);
                }
                PortType::Trigger => {
                    let f = self.flash(l.from.node, now);
                    col = lerp_colour(Color32::from_rgb(120, 70, 30), col, 0.3 + 0.7 * f);
                    w += 1.5 * f;
                }
                _ => {}
            }
            if !l.modulation.is_none_or(|m| m.enabled) {
                col = col.gamma_multiply(0.35);
            }
            wire(
                &painter,
                to_screen(self, *from),
                to_screen(self, to),
                col,
                w,
                z,
            );
        }
        for (id, l) in &layouts {
            let Some(n) = c.graph.node(*id) else { continue };
            self.draw_node(&painter, &c, n, l, &to_screen, now);
        }
        // The wire being dragged, on top of everything (it may leave the panel).
        if let (Drag::Wire { from, ty }, Some(p)) = (&self.drag, pointer)
            && let Some(l) = lay.get(&from.node)
            && let Some(o) = l.outputs.iter().find(|o| o.0 == from.port)
        {
            let fg = ui
                .ctx()
                .layer_painter(LayerId::new(Order::Foreground, Id::unique("wire-drag")));
            wire(
                &fg,
                to_screen(self, o.3),
                p,
                port_colour(*ty),
                wire_w + 0.5,
                z,
            );
            // Light up the plugs it can go into.
            for (id, l) in &layouts {
                for (name, _, _, at) in &l.inputs {
                    let link = Link::new(from.clone(), Endpoint::input(*id, name));
                    if c.graph.check_link(&link).is_ok() {
                        painter.circle_stroke(
                            to_screen(self, *at),
                            PORT_R * z + 3.0,
                            Stroke::new(1.5, port_colour(*ty)),
                        );
                    }
                }
            }
        }
        if let (Drag::Box { start }, Some(p)) = (&self.drag, pointer) {
            let b = Rect::from_two_pos(*start, p);
            painter.rect_filled(b, 2.0, ACCENT.gamma_multiply(0.08));
            painter.rect_stroke(b, 2.0, Stroke::new(1.0, ACCENT), StrokeKind::Inside);
        }
        if c.graph.nodes().is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "An empty canvas. Right-click to add your first node,\n\
                 press Shift+Space to search, or right-click → Add starter graph.",
                FontId::proportional(15.0),
                TEXT_DIM,
            );
        }
        // Status line: messages, else a hint.
        let line = match &self.message {
            Some((m, t)) if now - t < 4.0 => (m.clone(), ACCENT),
            _ => (
                format!(
                    "Right-click: add · drag plug→plug: wire · drop a wire on empty space: add + wire · \
                     S/P/L: insert after selected · Shift+Space: search · ?: all shortcuts{}",
                    if self.snap {
                        " · snap on (G)"
                    } else {
                        " · snap off (G)"
                    }
                ),
                TEXT_DIM,
            ),
        };
        painter.text(
            rect.left_bottom() + vec2(10.0, -8.0),
            Align2::LEFT_BOTTOM,
            line.0,
            FontId::proportional(11.5),
            line.1,
        );

        if self.show_help {
            help_overlay(&painter, rect);
        }
        self.search_ui(ui, &mut c);
        self.param_pick_ui(ui, &mut c);
        self.frame_rename_ui(ui, &mut c);
    }

    /// 1 just after a trigger node fired, fading to 0.
    fn flash(&self, node: NodeId, now: f64) -> f32 {
        self.fired
            .get(&node)
            .map(|t| (-(now - t) as f32 / 0.15).exp())
            .unwrap_or(0.0)
    }

    fn draw_node(
        &self,
        painter: &egui::Painter,
        c: &Ctx,
        n: &Node,
        l: &Layout,
        to_screen: &dyn Fn(&Self, Pos2) -> Pos2,
        now: f64,
    ) {
        let z = self.zoom;
        let r = Rect::from_min_max(to_screen(self, l.rect.min), to_screen(self, l.rect.max));
        if !painter.clip_rect().intersects(r.expand(20.0)) {
            return; // off screen
        }
        let round = CornerRadius::same((6.0 * z).round().clamp(2.0, 12.0) as u8);
        let dim = if n.bypass { 0.45 } else { 1.0 };
        let cat = category_colour(category(n));
        let tag = n.colour.map(rgb).unwrap_or(cat);
        let error = c.eval.errors.get(&n.id);
        painter.rect_filled(
            r.translate(vec2(0.0, 3.0 * z)),
            round,
            Color32::from_black_alpha(90),
        );
        let body = if n.colour.is_some() {
            lerp_colour(NODE_BG, tag, 0.10)
        } else {
            NODE_BG
        };
        painter.rect_filled(
            r,
            round,
            body.gamma_multiply(if n.bypass { 0.7 } else { 1.0 }),
        );
        let head = Rect::from_min_size(r.min, vec2(r.width(), HEADER_H * z));
        let head_fill = lerp_colour(
            NODE_HEADER,
            tag,
            if n.colour.is_some() { 0.45 } else { 0.14 },
        );
        painter.rect_filled(
            head,
            CornerRadius {
                nw: round.nw,
                ne: round.ne,
                sw: 0,
                se: 0,
            },
            head_fill.gamma_multiply(dim),
        );
        painter.rect_filled(
            Rect::from_min_size(head.min, vec2(4.0 * z, head.height())),
            CornerRadius {
                nw: round.nw,
                ..Default::default()
            },
            cat.gamma_multiply(dim),
        );
        let font = FontId::proportional((12.5 * z).clamp(6.0, 20.0));
        let small = FontId::proportional((11.0 * z).clamp(5.0, 18.0));
        painter.text(
            head.left_center() + vec2(10.0 * z, 0.0),
            Align2::LEFT_CENTER,
            format!("{:02}", n.id.0),
            font.clone(),
            TEXT_DIM.gamma_multiply(dim),
        );
        painter.text(
            head.left_center() + vec2(32.0 * z, 0.0),
            Align2::LEFT_CENTER,
            truncate(&n.label, 17),
            font,
            TEXT.gamma_multiply(dim),
        );
        let badge = if error.is_some() {
            Some(("⚠", ERROR))
        } else if n.bypass {
            Some(("off", TEXT_DIM))
        } else if c.viewed == Some(n.id) {
            Some(("◉", ACCENT))
        } else {
            None
        };
        if let Some((t, col)) = badge {
            painter.text(
                head.right_center() - vec2(8.0 * z, 0.0),
                Align2::RIGHT_CENTER,
                t,
                small.clone(),
                col,
            );
        }
        let show_labels = z > 0.5;
        for (name, label, ty, at) in &l.inputs {
            let p = to_screen(self, *at);
            let linked = c.graph.input_link(n.id, name).is_some();
            port(painter, p, *ty, z, dim, linked);
            if show_labels {
                let text = if label.is_empty() { name } else { label };
                painter.text(
                    p + vec2(9.0 * z, 0.0),
                    Align2::LEFT_CENTER,
                    text,
                    small.clone(),
                    TEXT_DIM.gamma_multiply(dim * if *ty == PortType::Mask { 0.6 } else { 1.0 }),
                );
            }
        }
        for (name, _, ty, at) in &l.outputs {
            let linked = c
                .graph
                .links()
                .iter()
                .any(|k| k.from.node == n.id && &k.from.port == name);
            port(painter, to_screen(self, *at), *ty, z, dim, linked);
        }
        for (name, label, at) in &l.mods {
            let p = to_screen(self, *at);
            port(painter, p, PortType::Signal, z, dim, true);
            if show_labels {
                let text = match c.eval.live_value(n.id, name) {
                    Some(v) => format!("~ {label}  {v:.2}"),
                    None => format!("~ {label}"),
                };
                painter.text(
                    p + vec2(9.0 * z, 0.0),
                    Align2::LEFT_CENTER,
                    text,
                    small.clone(),
                    port_colour(PortType::Signal).gamma_multiply(dim),
                );
            }
        }
        // Signal / trigger nodes: a live readout.
        if let Some(v) = c.eval.signal_value(n.id) {
            let out_ty = n.outputs.first().map(|o| o.ty).unwrap_or(PortType::Signal);
            let y = to_screen(
                self,
                l.outputs.first().map(|o| o.3).unwrap_or(l.rect.center()),
            )
            .y;
            let x0 = r.left() + if n.inputs.is_empty() { 10.0 } else { 70.0 } * z;
            if out_ty == PortType::Trigger {
                let f = self.flash(n.id, now);
                painter.circle_filled(
                    pos2(x0 + 6.0 * z, y),
                    5.0 * z,
                    lerp_colour(Color32::from_gray(45), port_colour(PortType::Trigger), f),
                );
            } else {
                let bar = Rect::from_min_max(
                    pos2(x0, y - 3.0 * z),
                    pos2(r.right() - 48.0 * z, y + 3.0 * z),
                );
                if bar.width() > 4.0 {
                    painter.rect_filled(bar, 2.0, Color32::from_gray(26));
                    let mut f = bar;
                    f.set_width(bar.width() * v.clamp(0.0, 1.0));
                    painter.rect_filled(f, 2.0, port_colour(PortType::Signal));
                }
                if show_labels {
                    painter.text(
                        pos2(r.right() - 12.0 * z, y),
                        Align2::RIGHT_CENTER,
                        format!("{v:.2}"),
                        small.clone(),
                        TEXT_DIM,
                    );
                }
            }
        }
        // Thumbnail of the node's picture.
        if let Some(t) = l.thumb {
            let tr = Rect::from_min_max(to_screen(self, t.min), to_screen(self, t.max));
            painter.rect_filled(tr, 3.0, Color32::BLACK);
            if let Some(tex) = c.thumbs.get(&n.id) {
                painter.image(
                    *tex,
                    tr,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE.gamma_multiply(dim),
                );
            } else if show_labels {
                painter.text(
                    tr.center(),
                    Align2::CENTER_CENTER,
                    if n.bypass { "bypassed" } else { "no picture" },
                    small.clone(),
                    TEXT_DIM,
                );
            }
        }
        let stroke = if self.selected.contains(&n.id) {
            Stroke::new(2.0, ACCENT)
        } else if error.is_some() {
            Stroke::new(1.0, ERROR)
        } else {
            Stroke::new(1.0, Color32::from_gray(58))
        };
        painter.rect_stroke(r, round, stroke, StrokeKind::Outside);
        if let Some(e) = error
            && z > 0.5
        {
            painter.text(
                r.left_bottom() + vec2(0.0, 4.0),
                Align2::LEFT_TOP,
                truncate(e.lines().next().unwrap_or(""), 60),
                small,
                ERROR,
            );
        }
    }

    fn drop_wire(
        &mut self,
        ui: &egui::Ui,
        c: &mut Ctx,
        from: OutputRef,
        ty: PortType,
        at: Hit,
        p: Pos2,
    ) {
        let to = match at {
            Hit::Input(n, port) => Some(Endpoint::input(n, &port)),
            Hit::ModRow(n, param) => Some(Endpoint::param(n, &param)),
            Hit::Node(n) if n == from.node => None,
            Hit::Node(n) => {
                let node = c.graph.node(n);
                let fits: Vec<String> = node
                    .map(|nd| {
                        nd.inputs
                            .iter()
                            .filter(|i| {
                                i.ty.accepts(ty)
                                    && !(i.ty == PortType::Mask && ty == PortType::Image)
                            })
                            .map(|i| i.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                if ty == PortType::Signal && fits.is_empty() {
                    if node.is_some_and(|n| n.params.iter().any(|p| p.def.is_modulatable())) {
                        self.param_pick = Some((from, n, p));
                    } else {
                        self.say(ui, "That node has nothing a signal can drive.");
                    }
                    return;
                }
                // First free input that fits (else the first).
                fits.iter()
                    .find(|name| c.graph.input_link(n, name).is_none())
                    .or(fits.first())
                    .map(|name| Endpoint::input(n, name))
            }
            Hit::None => {
                // Dropped on empty space: add a node already wired to it.
                self.open_search_with(ui.ctx(), Insert::FromWire(from, ty));
                return;
            }
            _ => None,
        };
        let Some(to) = to else { return };
        let mut link = Link::new(from, to);
        if matches!(link.to, Endpoint::Param { .. }) {
            link.modulation = Some(crate::graph_view::default_wire(c.graph, &link.from));
        }
        if let Err(e) = c.graph.check_link(&link) {
            self.say(ui, e.to_string());
            return;
        }
        if let Err(e) = c.history.exec(c.graph, Command::Connect(link)) {
            self.say(ui, e.to_string());
        }
    }

    fn param_pick_ui(&mut self, ui: &egui::Ui, c: &mut Ctx) {
        let Some((from, node, at)) = self.param_pick.clone() else {
            return;
        };
        let Some(n) = c.graph.node(node) else {
            self.param_pick = None;
            return;
        };
        let mut pick = None;
        let mut close = ui.input(|i| i.key_pressed(Key::Escape));
        let area = egui::Area::new(Id::unique("param-pick"))
            .order(Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new("Drive which setting?").strong());
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for p in n.params.iter().filter(|p| p.def.is_modulatable()) {
                                if ui.button(p.def.label()).clicked() {
                                    pick = Some(p.def.name.clone());
                                }
                            }
                        });
                    if ui.small_button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if ui.input(|i| i.pointer.any_pressed())
            && !area
                .response
                .rect
                .contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or_default())
        {
            close = true;
        }
        if let Some(param) = pick {
            let link = Link {
                modulation: Some(crate::graph_view::default_wire(c.graph, &from)),
                ..Link::new(from, Endpoint::param(node, &param))
            };
            if let Err(e) = c.history.exec(c.graph, Command::Connect(link)) {
                self.say(ui, e.to_string());
            }
            close = true;
        }
        if close {
            self.param_pick = None;
        }
    }

    fn frame_rename_ui(&mut self, ui: &egui::Ui, c: &mut Ctx) {
        let Some((id, mut text, at)) = self.frame_rename.take() else {
            return;
        };
        let mut done = false;
        let mut cancel = ui.input(|i| i.key_pressed(Key::Escape));
        egui::Area::new(Id::unique("frame-rename"))
            .order(Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(200.0));
                    r.request_focus();
                    if r.lost_focus() {
                        if ui.input(|i| i.key_pressed(Key::Enter)) {
                            done = true;
                        } else {
                            cancel = true;
                        }
                    }
                });
            });
        if done {
            if let Some(f) = c.graph.frame(id) {
                let _ = c.history.exec(
                    c.graph,
                    Command::PutFrame(Frame {
                        label: text,
                        ..f.clone()
                    }),
                );
            }
        } else if !cancel {
            self.frame_rename = Some((id, text, at));
        }
    }

    fn search_ui(&mut self, ui: &egui::Ui, c: &mut Ctx) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        let q = s.text.to_lowercase();
        let fits = |e: &CatalogueEntry| match &s.insert {
            Insert::None => true,
            Insert::Serial(_) => e.category == Category::Effect,
            Insert::Parallel(_) | Insert::Layer(_) => {
                matches!(e.category, Category::Source | Category::Effect)
            }
            Insert::FromWire(_, ty) => {
                e.inputs.iter().any(|i| i.accepts(*ty))
                    || (*ty == PortType::Signal && e.modulatable)
            }
        };
        let hits: Vec<&CatalogueEntry> = c
            .catalogue
            .iter()
            .filter(|e| fits(e))
            .filter(|e| {
                q.is_empty()
                    || e.label.to_lowercase().contains(&q)
                    || e.category.name().to_lowercase().contains(&q)
                    || e.description.to_lowercase().contains(&q)
            })
            .collect();
        let (up, down, enter, esc) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
            )
        });
        if down {
            s.pick = (s.pick + 1).min(hits.len().saturating_sub(1));
        }
        if up {
            s.pick = s.pick.saturating_sub(1);
        }
        s.pick = s.pick.min(hits.len().saturating_sub(1));
        let title = match &s.insert {
            Insert::None => "Add a node",
            Insert::Serial(_) => "Insert after (S)",
            Insert::Parallel(_) => "Add in parallel (P)",
            Insert::Layer(_) => "Add as a layer on top (L)",
            Insert::FromWire(..) => "Add a node on this wire",
        };
        let mut chosen = None;
        let mut close = esc;
        let area = egui::Area::new(Id::unique("node-search"))
            .order(Order::Foreground)
            .fixed_pos(s.screen)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(270.0);
                    ui.label(egui::RichText::new(title).small().color(TEXT_DIM));
                    let te = ui.add(
                        egui::TextEdit::singleline(&mut s.text)
                            .hint_text("type to search…")
                            .desired_width(f32::INFINITY),
                    );
                    if s.focus {
                        te.request_focus();
                        s.focus = false;
                    }
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            let mut last = None;
                            for (i, e) in hits.iter().enumerate() {
                                if last != Some(e.category) {
                                    last = Some(e.category);
                                    ui.label(
                                        egui::RichText::new(e.category.name())
                                            .small()
                                            .color(category_colour(e.category)),
                                    );
                                }
                                let r = ui
                                    .selectable_label(i == s.pick, format!("  {}", e.label))
                                    .on_hover_text(&e.description);
                                if i == s.pick && (up || down) {
                                    r.scroll_to_me(None);
                                }
                                if r.clicked() {
                                    chosen = Some(e.kind.clone());
                                }
                            }
                            if hits.is_empty() {
                                ui.label(egui::RichText::new("Nothing by that name.").weak());
                            }
                        });
                });
            });
        if enter && let Some(e) = hits.get(s.pick) {
            chosen = Some(e.kind.clone());
        }
        if ui.input(|i| i.pointer.any_pressed())
            && !area
                .response
                .rect
                .contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or_default())
        {
            close = true;
        }
        let (at, insert, screen) = (s.at, s.insert.clone(), s.screen);
        if let Some(kind) = chosen {
            self.search = None;
            if let Some(id) = self.add_node(ui, c, &kind, at) {
                self.wire_new(ui, c, id, insert, screen);
            }
            return;
        }
        if close {
            self.search = None;
        }
    }

    /// Wire a freshly added node according to how it was asked for.
    fn wire_new(&mut self, ui: &egui::Ui, c: &mut Ctx, new: NodeId, insert: Insert, screen: Pos2) {
        let Some(node) = c.graph.node(new).cloned() else {
            return;
        };
        let first_image = node
            .inputs
            .iter()
            .find(|p| p.ty == PortType::Image)
            .map(|p| p.name.clone());
        let image_out = node
            .outputs
            .iter()
            .find(|p| p.ty == PortType::Image)
            .map(|p| p.name.clone());
        let mut cmds = Vec::new();
        match insert {
            Insert::None => return,
            Insert::FromWire(from, ty) => {
                let input = node
                    .inputs
                    .iter()
                    .find(|p| p.ty.accepts(ty) && p.ty != PortType::Mask)
                    .or_else(|| node.inputs.iter().find(|p| p.ty.accepts(ty)));
                match input {
                    Some(p) => cmds.push(Command::Connect(Link::new(
                        from,
                        Endpoint::input(new, &p.name),
                    ))),
                    None if ty == PortType::Signal => {
                        self.param_pick = Some((from, new, screen));
                        return;
                    }
                    None => return,
                }
            }
            Insert::Serial(after) => {
                let (Some(input), Some(out)) = (first_image, image_out) else {
                    return;
                };
                let Some(src_out) = c
                    .graph
                    .node(after)
                    .and_then(|n| n.outputs.iter().find(|p| p.ty == PortType::Image))
                    .map(|p| p.name.clone())
                else {
                    return;
                };
                if let Some(a) = c.graph.node(after) {
                    cmds.push(Command::Move {
                        node: new,
                        pos: [a.pos[0] + NODE_W + 48.0, a.pos[1]],
                    });
                }
                // Take over where `after` was going, then feed from it.
                for l in c.graph.links().iter().filter(|l| {
                    l.from.node == after
                        && l.from.port == src_out
                        && matches!(l.to, Endpoint::Input { .. })
                }) {
                    cmds.push(Command::Connect(Link::new(
                        OutputRef::new(new, &out),
                        l.to.clone(),
                    )));
                }
                cmds.push(Command::Connect(Link::new(
                    OutputRef::new(after, &src_out),
                    Endpoint::input(new, &input),
                )));
            }
            Insert::Parallel(beside) | Insert::Layer(beside) => {
                let parallel = matches!(insert, Insert::Parallel(_));
                let Some(out) = image_out else { return };
                let Some(b) = c.graph.node(beside).cloned() else {
                    return;
                };
                let Some(b_out) = b
                    .outputs
                    .iter()
                    .find(|p| p.ty == PortType::Image)
                    .map(|p| p.name.clone())
                else {
                    return;
                };
                let merge_kind = registry::shader_kind("fx_merge.wgsl");
                let spec = match registry::spec_for(&merge_kind, c.library) {
                    Ok(s) => s,
                    Err(e) => return self.say(ui, e),
                };
                let mut m = c.graph.new_node(&spec);
                m.label = if parallel { "Parallel Mix" } else { "Layer" }.into();
                m.pos = [b.pos[0] + 2.0 * (NODE_W + 48.0), b.pos[1]];
                if let Some(p) = m.param_mut("mode") {
                    p.value = ParamValue::Choice(0); // Normal
                }
                if let Some(p) = m.param_mut("opacity") {
                    p.value = ParamValue::Float(if parallel { 0.5 } else { 1.0 });
                }
                let mid = m.id;
                cmds.push(Command::AddNode(Box::new(m)));
                cmds.push(Command::Move {
                    node: new,
                    pos: [b.pos[0] + NODE_W + 48.0, b.pos[1] + 160.0],
                });
                // Same input as the node beside it.
                if let (Some(input), Some(src)) = (
                    &first_image,
                    b.inputs
                        .iter()
                        .find(|p| p.ty == PortType::Image)
                        .and_then(|p| c.graph.input_link(beside, &p.name)),
                ) {
                    cmds.push(Command::Connect(Link::new(
                        src.from.clone(),
                        Endpoint::input(new, input),
                    )));
                }
                // The merge takes over where `beside` was going.
                for l in c.graph.links().iter().filter(|l| {
                    l.from.node == beside
                        && l.from.port == b_out
                        && matches!(l.to, Endpoint::Input { .. })
                }) {
                    cmds.push(Command::Connect(Link::new(
                        OutputRef::new(mid, registry::IMAGE_OUT),
                        l.to.clone(),
                    )));
                }
                cmds.push(Command::Connect(Link::new(
                    OutputRef::new(beside, &b_out),
                    Endpoint::input(mid, "base"),
                )));
                cmds.push(Command::Connect(Link::new(
                    OutputRef::new(new, &out),
                    Endpoint::input(mid, "top"),
                )));
            }
        }
        // Fold into the "add" step so one Ctrl+Z removes the lot.
        let _ = c.history.undo(c.graph);
        let add = Command::AddNode(Box::new(node));
        cmds.insert(0, add);
        match c.history.exec(c.graph, Command::Batch(cmds)) {
            Ok(()) => self.selected = BTreeSet::from([new]),
            Err(e) => {
                self.say(ui, e.to_string());
            }
        }
    }

    fn add_node(&mut self, ui: &egui::Ui, c: &mut Ctx, kind: &str, at: Pos2) -> Option<NodeId> {
        let spec = match registry::spec_for(kind, c.library) {
            Ok(s) => s,
            Err(e) => {
                self.say(ui, e);
                return None;
            }
        };
        let mut n = c.graph.new_node(&spec);
        n.pos = [
            snap(at.x - NODE_W * 0.5, self.snap),
            snap(at.y - HEADER_H * 0.5, self.snap),
        ];
        let id = n.id;
        match c.history.exec(c.graph, Command::AddNode(Box::new(n))) {
            Ok(()) => {
                self.selected = BTreeSet::from([id]);
                Some(id)
            }
            Err(e) => {
                self.say(ui, e.to_string());
                None
            }
        }
    }

    fn clip_of_selection(&self, g: &Graph) -> Clip {
        Clip {
            nodes: self
                .selected
                .iter()
                .filter_map(|id| g.node(*id).cloned())
                .collect(),
            links: g
                .links()
                .iter()
                .filter(|l| {
                    self.selected.contains(&l.from.node) && self.selected.contains(&l.to.node())
                })
                .cloned()
                .collect(),
            frames: Vec::new(),
        }
    }

    fn run_action(&mut self, ui: &egui::Ui, c: &mut Ctx, a: MenuAction) {
        let sel: Vec<NodeId> = self.selected.iter().copied().collect();
        match a {
            MenuAction::Add(kind) => {
                let at = self.menu_at;
                self.add_node(ui, c, &kind, at);
            }
            MenuAction::Delete => {
                if sel.is_empty() {
                    return;
                }
                let cmds = sel.iter().map(|id| Command::RemoveNode(*id)).collect();
                let _ = c.history.exec(c.graph, Command::Batch(cmds));
                self.selected.clear();
            }
            MenuAction::Bypass => {
                let any_on = sel
                    .iter()
                    .filter_map(|id| c.graph.node(*id))
                    .any(|n| !n.bypass);
                let cmds: Vec<Command> = sel
                    .iter()
                    .map(|id| Command::SetBypass {
                        node: *id,
                        bypass: any_on,
                    })
                    .collect();
                if !cmds.is_empty() {
                    let _ = c.history.exec(c.graph, Command::Batch(cmds));
                }
            }
            MenuAction::Colour(col) => {
                let cmds: Vec<Command> = sel
                    .iter()
                    .map(|id| Command::SetColour {
                        node: *id,
                        colour: col,
                    })
                    .collect();
                if !cmds.is_empty() {
                    let _ = c.history.exec(c.graph, Command::Batch(cmds));
                }
            }
            MenuAction::Copy | MenuAction::Cut => {
                if sel.is_empty() {
                    return;
                }
                if let Ok(text) = ron::to_string(&self.clip_of_selection(c.graph)) {
                    ui.ctx().copy_text(format!("{CLIP_TAG}{text}"));
                }
                if a == MenuAction::Cut {
                    self.run_action(ui, c, MenuAction::Delete);
                }
            }
            MenuAction::Duplicate => {
                if let Ok(text) = ron::to_string(&self.clip_of_selection(c.graph)) {
                    self.paste(ui, c, &format!("{CLIP_TAG}{text}"), Some(vec2(32.0, 32.0)));
                }
            }
            MenuAction::Clear => {
                let mut cmds: Vec<Command> = c
                    .graph
                    .nodes()
                    .iter()
                    .map(|n| Command::RemoveNode(n.id))
                    .collect();
                cmds.extend(c.graph.frames().iter().map(|f| Command::RemoveFrame(f.id)));
                let _ = c.history.exec(c.graph, Command::Batch(cmds));
                self.selected.clear();
            }
            MenuAction::Starter => match nodes::demo::build(c.library) {
                Ok((g, _)) => {
                    let clip = Clip {
                        nodes: g.nodes().to_vec(),
                        links: g.links().to_vec(),
                        frames: g.frames().to_vec(),
                    };
                    if let Ok(text) = ron::to_string(&clip) {
                        self.paste(ui, c, &format!("{CLIP_TAG}{text}"), None);
                    }
                }
                Err(e) => self.say(ui, e),
            },
            MenuAction::AddFrame => {
                let id = c.graph.new_frame_id();
                let bounds = self
                    .selected
                    .iter()
                    .filter_map(|id| c.graph.node(*id))
                    .map(|n| layout(c.graph, n, self.thumbnails).rect)
                    .reduce(|a, b| a.union(b));
                let r = match bounds {
                    Some(b) => Rect::from_min_max(
                        b.min - vec2(GRID * 1.5, FRAME_TITLE_H + GRID),
                        b.max + vec2(GRID * 1.5, GRID * 1.5),
                    ),
                    None => Rect::from_min_size(self.menu_at, vec2(320.0, 200.0)),
                };
                let s = self.snap;
                let f = Frame {
                    id,
                    label: "Frame".into(),
                    rect: [
                        snap(r.min.x, s),
                        snap(r.min.y, s),
                        snap(r.width(), s),
                        snap(r.height(), s),
                    ],
                    colour: TAG_COLOURS[(id as usize + 4) % TAG_COLOURS.len()],
                };
                let _ = c.history.exec(c.graph, Command::PutFrame(f));
            }
            MenuAction::FrameColour(id, col) => {
                if let Some(f) = c.graph.frame(id) {
                    let _ = c.history.exec(
                        c.graph,
                        Command::PutFrame(Frame {
                            colour: col,
                            ..f.clone()
                        }),
                    );
                }
            }
            MenuAction::RenameFrame(id) => {
                if let Some(f) = c.graph.frame(id) {
                    let at = ui.input(|i| i.pointer.hover_pos()).unwrap_or(Pos2::ZERO);
                    self.frame_rename = Some((id, f.label.clone(), at));
                }
            }
            MenuAction::SelectInFrame(id) => {
                if let Some(f) = c.graph.frame(id) {
                    let fr = frame_rect(f);
                    self.selected = c
                        .graph
                        .nodes()
                        .iter()
                        .filter(|n| fr.contains(layout(c.graph, n, self.thumbnails).rect.center()))
                        .map(|n| n.id)
                        .collect();
                }
            }
            MenuAction::RemoveFrame(id) => {
                let _ = c.history.exec(c.graph, Command::RemoveFrame(id));
            }
        }
    }

    /// Paste copied nodes as new nodes (new numbers), keeping their wires.
    /// `offset` = shift from the originals; `None` = put them at the pointer.
    fn paste(&mut self, ui: &egui::Ui, c: &mut Ctx, text: &str, offset: Option<Vec2>) {
        let Some(body) = text.strip_prefix(CLIP_TAG) else {
            return;
        };
        let clip: Clip = match ron::from_str(body) {
            Ok(c) => c,
            Err(_) => return self.say(ui, "The clipboard doesn't hold nodes I can read."),
        };
        if clip.nodes.is_empty() {
            return;
        }
        let offset = offset.unwrap_or_else(|| {
            let min = clip
                .nodes
                .iter()
                .map(|n| n.pos)
                .chain(clip.frames.iter().map(|f| [f.rect[0], f.rect[1]]))
                .fold(pos2(f32::MAX, f32::MAX), |m, p| {
                    pos2(m.x.min(p[0]), m.y.min(p[1]))
                });
            let to = pos2(
                snap(self.menu_at.x, self.snap),
                snap(self.menu_at.y, self.snap),
            );
            to - min
        });
        let mut map = HashMap::new();
        let mut cmds = Vec::new();
        for n in &clip.nodes {
            let id = c.graph.reserve_node_id();
            map.insert(n.id, id);
            let mut copy = n.clone();
            copy.id = id;
            copy.pos = [n.pos[0] + offset.x, n.pos[1] + offset.y];
            cmds.push(Command::AddNode(Box::new(copy)));
        }
        for l in &clip.links {
            let (Some(from), Some(to)) = (map.get(&l.from.node), map.get(&l.to.node())) else {
                continue;
            };
            let mut link = l.clone();
            link.from.node = *from;
            link.to = match &l.to {
                Endpoint::Input { port, .. } => Endpoint::input(*to, port),
                Endpoint::Param { param, .. } => Endpoint::param(*to, param),
            };
            cmds.push(Command::Connect(link));
        }
        for f in &clip.frames {
            let id = c.graph.new_frame_id();
            cmds.push(Command::PutFrame(Frame {
                id,
                rect: [
                    f.rect[0] + offset.x,
                    f.rect[1] + offset.y,
                    f.rect[2],
                    f.rect[3],
                ],
                ..f.clone()
            }));
        }
        match c.history.exec(c.graph, Command::Batch(cmds)) {
            Ok(()) => self.selected = map.values().copied().collect(),
            Err(e) => self.say(ui, e.to_string()),
        }
    }

    /// Zoom and pan so every node fits in `rect`.
    fn frame_all(&mut self, g: &Graph, rect: Rect) {
        let Some(b) = g
            .nodes()
            .iter()
            .map(|n| layout(g, n, self.thumbnails).rect)
            .chain(g.frames().iter().map(frame_rect))
            .reduce(|a, b| a.union(b))
        else {
            self.zoom = 1.0;
            self.pan = rect.size() * 0.5;
            return;
        };
        let b = b.expand(40.0);
        self.zoom = (rect.width() / b.width())
            .min(rect.height() / b.height())
            .clamp(0.3, 1.1);
        self.pan = rect.size() * 0.5 - b.center().to_vec2() * self.zoom;
    }
}

struct Keys {
    events: Vec<egui::Event>,
    del: bool,
    redo: bool,
    undo: bool,
    bypass: bool,
    all: bool,
    home: bool,
    serial: bool,
    parallel: bool,
    layer: bool,
    grid: bool,
    help: bool,
    esc: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum MenuAction {
    Add(String),
    Delete,
    Bypass,
    Colour(Option<[u8; 3]>),
    Copy,
    Cut,
    Duplicate,
    Clear,
    Starter,
    AddFrame,
    FrameColour(u32, [u8; 3]),
    RenameFrame(u32),
    SelectInFrame(u32),
    RemoveFrame(u32),
}

fn add_menu(
    ui: &mut egui::Ui,
    catalogue: &[CatalogueEntry],
    selected: usize,
) -> Option<MenuAction> {
    let mut pick = None;
    ui.label(egui::RichText::new("Add node").small().color(TEXT_DIM));
    for cat in Category::ALL {
        let label = egui::RichText::new(cat.name()).color(category_colour(cat));
        ui.menu_button(label, |ui| {
            for e in catalogue.iter().filter(|e| e.category == cat) {
                if ui.button(&e.label).on_hover_text(&e.description).clicked() {
                    pick = Some(MenuAction::Add(e.kind.clone()));
                }
            }
        });
    }
    ui.separator();
    let frame_label = if selected > 0 {
        "Frame the selected nodes"
    } else {
        "Add a frame here"
    };
    if ui
        .button(frame_label)
        .on_hover_text(
            "A coloured backdrop to keep an area tidy. Drag its title to move everything in it.",
        )
        .clicked()
    {
        pick = Some(MenuAction::AddFrame);
    }
    if ui.button("Paste   Ctrl+V").clicked() {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }
    if ui
        .button("Add starter graph")
        .on_hover_text("A small example to learn from")
        .clicked()
    {
        pick = Some(MenuAction::Starter);
    }
    if ui
        .button("Clear graph")
        .on_hover_text("Ctrl+Z brings it back")
        .clicked()
    {
        pick = Some(MenuAction::Clear);
    }
    if pick.is_some() {
        ui.close();
    }
    pick
}

fn swatches(ui: &mut egui::Ui, none: bool) -> Option<Option<[u8; 3]>> {
    let mut pick = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for c in TAG_COLOURS {
            let (r, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
            ui.painter().rect_filled(r, 3.0, rgb(c));
            if resp.hovered() {
                ui.painter().rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.5, Color32::WHITE),
                    StrokeKind::Outside,
                );
            }
            if resp.clicked() {
                pick = Some(Some(c));
            }
        }
        if none && ui.small_button("none").clicked() {
            pick = Some(None);
        }
    });
    pick
}

fn node_menu(ui: &mut egui::Ui, count: usize) -> Option<MenuAction> {
    let mut pick = None;
    let s = if count > 1 { "s" } else { "" };
    ui.label(egui::RichText::new("Colour").small().color(TEXT_DIM));
    if let Some(c) = swatches(ui, true) {
        pick = Some(MenuAction::Colour(c));
    }
    ui.separator();
    if ui.button(format!("Bypass{s}   Ctrl+D")).clicked() {
        pick = Some(MenuAction::Bypass);
    }
    if ui.button(format!("Duplicate{s}")).clicked() {
        pick = Some(MenuAction::Duplicate);
    }
    if ui.button("Copy   Ctrl+C").clicked() {
        pick = Some(MenuAction::Copy);
    }
    if ui.button("Cut   Ctrl+X").clicked() {
        pick = Some(MenuAction::Cut);
    }
    if ui.button("Put in a frame").clicked() {
        pick = Some(MenuAction::AddFrame);
    }
    ui.separator();
    if ui.button(format!("Delete node{s}   Del")).clicked() {
        pick = Some(MenuAction::Delete);
    }
    if pick.is_some() {
        ui.close();
    }
    pick
}

fn frame_menu(ui: &mut egui::Ui, id: u32) -> Option<MenuAction> {
    let mut pick = None;
    ui.label(egui::RichText::new("Frame colour").small().color(TEXT_DIM));
    if let Some(Some(c)) = swatches(ui, false) {
        pick = Some(MenuAction::FrameColour(id, c));
    }
    ui.separator();
    if ui.button("Rename   (double-click title)").clicked() {
        pick = Some(MenuAction::RenameFrame(id));
    }
    if ui.button("Select the nodes inside").clicked() {
        pick = Some(MenuAction::SelectInFrame(id));
    }
    if ui.button("Remove frame (keeps the nodes)").clicked() {
        pick = Some(MenuAction::RemoveFrame(id));
    }
    if pick.is_some() {
        ui.close();
    }
    pick
}

/// The `?` sheet: every graph shortcut.
fn help_overlay(painter: &egui::Painter, rect: Rect) {
    const ROWS: &[(&str, &str)] = &[
        ("Right-click", "add a node / node, frame menus"),
        ("Shift+Space, double-click", "search for a node by name"),
        ("Drag plug → plug", "connect (only matching types fit)"),
        ("Drag a wire off an input", "unplug it"),
        ("Drop a wire on empty space", "add a node already wired"),
        (
            "Drop a blue wire on a node",
            "pick a setting for it to drive",
        ),
        ("S", "insert an effect after the selected node"),
        ("P", "add a parallel branch (50/50 mix)"),
        ("L", "add a layer on top"),
        ("Click / Shift+click / drag box", "select"),
        ("Del", "delete selected"),
        ("Ctrl+D", "bypass selected"),
        ("Ctrl+C / X / V", "copy / cut / paste (between graphs too)"),
        ("Ctrl+Z, Ctrl+Shift+Z", "undo, redo"),
        ("Ctrl+A", "select all"),
        ("G", "snap to grid on / off"),
        ("Scroll, middle-drag", "zoom, pan (or Space/Alt + drag)"),
        ("Home", "fit everything in view"),
        ("Tab", "hide / show the graph"),
        ("?  or  Esc", "close this sheet"),
    ];
    let w = 520.0_f32.min(rect.width() - 20.0);
    let h = (ROWS.len() as f32 * 19.0 + 44.0).min(rect.height() - 10.0);
    let r = Rect::from_center_size(rect.center(), vec2(w, h));
    painter.rect_filled(r, 8.0, Color32::from_black_alpha(225));
    painter.rect_stroke(r, 8.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);
    painter.text(
        r.left_top() + vec2(16.0, 12.0),
        Align2::LEFT_TOP,
        "Node graph shortcuts",
        FontId::proportional(15.0),
        ACCENT,
    );
    for (i, (k, v)) in ROWS.iter().enumerate() {
        let y = r.top() + 38.0 + i as f32 * 19.0;
        if y > r.bottom() - 10.0 {
            break;
        }
        painter.text(
            pos2(r.left() + 16.0, y),
            Align2::LEFT_TOP,
            *k,
            FontId::proportional(12.5),
            TEXT,
        );
        painter.text(
            pos2(r.left() + 16.0 + w * 0.44, y),
            Align2::LEFT_TOP,
            *v,
            FontId::proportional(12.5),
            TEXT_DIM,
        );
    }
}

fn port(painter: &egui::Painter, p: Pos2, ty: PortType, z: f32, dim: f32, linked: bool) {
    let r = (PORT_R * z).clamp(2.5, 8.0);
    let col = port_colour(ty).gamma_multiply(
        dim * if ty == PortType::Mask && !linked {
            0.6
        } else {
            1.0
        },
    );
    painter.circle_filled(p, r, CANVAS_BG);
    painter.circle_stroke(p, r, Stroke::new(1.5, col));
    painter.circle_filled(p, r * if linked { 0.55 } else { 0.3 }, col);
}

fn wire(painter: &egui::Painter, a: Pos2, b: Pos2, col: Color32, w: f32, z: f32) {
    let dx = ((b.x - a.x).abs() * 0.5).max(40.0 * z);
    let shape = CubicBezierShape::from_points_stroke(
        [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b],
        false,
        Color32::TRANSPARENT,
        Stroke::new(w, col),
    );
    painter.add(shape);
}

fn lerp_colour(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}
