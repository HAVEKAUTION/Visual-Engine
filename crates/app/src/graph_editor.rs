//! The node graph editor (Resolve-style): a pan/zoom canvas with numbered
//! nodes, colour-coded plugs and wires, a right-click add menu, Shift+Space
//! search, selection (click, Shift/Ctrl+click, box), copy/paste, delete,
//! bypass and undo/redo.
//!
//! Mouse: drag from a plug to wire it; drag a wire off an input to unplug it;
//! drop a blue Signal wire on a node (or on a slider in the Inspector) to
//! make it drive that slider. Scroll = zoom, middle-drag (or Space/Alt +
//! drag) = pan.

use std::collections::{BTreeSet, HashMap};

use egui::epaint::CubicBezierShape;
use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Key, LayerId, Order, PointerButton, Pos2, Rect,
    Sense, Stroke, StrokeKind, Vec2, pos2, vec2,
};
use graph::{Command, Endpoint, Graph, History, Link, Node, NodeId, OutputRef, PortType};
use nodes::Evaluator;
use nodes::registry::{self, CatalogueEntry, Category};
use scenes::SceneLibrary;
use serde::{Deserialize, Serialize};

pub const ACCENT: Color32 = Color32::from_rgb(255, 170, 60);
pub const CANVAS_BG: Color32 = Color32::from_rgb(20, 20, 23);
const GRID: Color32 = Color32::from_rgb(34, 34, 39);
const NODE_BG: Color32 = Color32::from_rgb(40, 40, 46);
const NODE_HEADER: Color32 = Color32::from_rgb(52, 52, 59);
const TEXT: Color32 = Color32::from_gray(225);
const TEXT_DIM: Color32 = Color32::from_gray(140);
const ERROR: Color32 = Color32::from_rgb(255, 110, 110);

const NODE_W: f32 = 170.0;
const HEADER_H: f32 = 24.0;
const ROW_H: f32 = 18.0;
const PORT_R: f32 = 5.0;

/// Wire / plug colour per data type.
pub fn port_colour(ty: PortType) -> Color32 {
    match ty {
        PortType::Image => Color32::from_gray(190),
        PortType::Mask => Color32::from_rgb(90, 210, 120),
        PortType::Signal => Color32::from_rgb(80, 160, 255),
        PortType::Trigger => Color32::from_rgb(255, 150, 50),
    }
}

fn category_colour(c: Category) -> Color32 {
    match c {
        Category::Source => Color32::from_rgb(70, 190, 180),
        Category::Effect => Color32::from_rgb(160, 120, 255),
        Category::Signal => port_colour(PortType::Signal),
        Category::Output => ACCENT,
    }
}

fn category(n: &Node) -> Category {
    registry::category_of(
        &n.kind,
        n.outputs.iter().any(|p| p.ty == PortType::Image),
        n.inputs.iter().any(|p| p.ty == PortType::Image),
    )
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
}

/// Where a node's parts sit, in graph units.
struct Layout {
    rect: Rect,
    inputs: Vec<(String, String, PortType, Pos2)>,
    outputs: Vec<(String, String, PortType, Pos2)>,
    /// Parameters with Signal wires: (param name, label, plug position).
    mods: Vec<(String, String, Pos2)>,
}

fn layout(g: &Graph, n: &Node) -> Layout {
    let min = pos2(n.pos[0], n.pos[1]);
    let row_y = |i: usize| min.y + HEADER_H + ROW_H * (i as f32 + 0.5) + 2.0;
    let inputs: Vec<_> = n
        .inputs
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                p.name.clone(),
                p.label().to_string(),
                p.ty,
                pos2(min.x, row_y(i)),
            )
        })
        .collect();
    let outputs: Vec<_> = n
        .outputs
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                p.name.clone(),
                p.label().to_string(),
                p.ty,
                pos2(min.x + NODE_W, row_y(i)),
            )
        })
        .collect();
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
    Layout {
        rect: Rect::from_min_size(min, vec2(NODE_W, HEADER_H + ROW_H * rows as f32 + 8.0)),
        inputs,
        outputs,
        mods,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    None,
    Nodes,
    Wire { from: OutputRef, ty: PortType },
    Box { start: Pos2 },
    Pan,
}

struct Search {
    text: String,
    /// Where to put the node (graph units).
    at: Pos2,
    /// Where the box sits on screen.
    screen: Pos2,
    pick: usize,
    focus: bool,
}

/// Copied nodes (and the wires between them), as RON on the clipboard, so
/// they can be pasted into another graph too.
#[derive(Serialize, Deserialize)]
struct Clip {
    nodes: Vec<Node>,
    links: Vec<Link>,
}

const CLIP_TAG: &str = "visual-engine-nodes ";

pub struct Editor {
    pan: Vec2,
    zoom: f32,
    drag: Drag,
    pub selected: BTreeSet<NodeId>,
    menu_target: Hit,
    /// Graph position of the last right-click (where added nodes go).
    menu_at: Pos2,
    search: Option<Search>,
    /// A Signal wire was dropped on a node: pick which slider it drives.
    param_pick: Option<(OutputRef, NodeId, Pos2)>,
    framed: bool,
    /// Short message for the user (bad wire etc.).
    pub message: Option<(String, f64)>,
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
            framed: false,
            message: None,
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
        let screen = ctx
            .pointer_hover_pos()
            .unwrap_or_else(|| ctx.content_rect().center());
        self.search = Some(Search {
            text: String::new(),
            at: self.menu_at,
            screen,
            pick: 0,
            focus: true,
        });
    }

    fn say(&mut self, ui: &egui::Ui, msg: impl Into<String>) {
        self.message = Some((msg.into(), ui.input(|i| i.time)));
    }

    pub fn show(&mut self, ui: &mut egui::Ui, mut c: Ctx) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        if !self.framed {
            self.frame_all(c.graph, rect);
            self.framed = true;
        }
        let origin = |s: &Self| rect.min + s.pan;
        let to_screen = |s: &Self, p: Pos2| origin(s) + p.to_vec2() * s.zoom;
        let to_graph = |s: &Self, p: Pos2| ((p - origin(s)) / s.zoom).to_pos2();

        // Zoom around the pointer.
        if resp.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let f = (scroll * 0.0015).exp() * pinch;
            if (f - 1.0).abs() > 1e-4
                && let Some(p) = resp.hover_pos()
            {
                let before = to_graph(self, p);
                self.zoom = (self.zoom * f).clamp(0.3, 2.5);
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

        let layouts: Vec<(NodeId, Layout)> = c
            .graph
            .nodes()
            .iter()
            .map(|n| (n.id, layout(c.graph, n)))
            .collect();
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
                    self.drag = Drag::Nodes;
                }
                Hit::None => {
                    if !add_select {
                        self.selected.clear();
                    }
                    self.drag = Drag::Box { start };
                }
            }
        } else if resp.dragged_by(PointerButton::Primary) && self.drag == Drag::Nodes {
            let d = resp.drag_delta() / self.zoom;
            if d != Vec2::ZERO {
                let moves: Vec<Command> = self
                    .selected
                    .iter()
                    .filter_map(|id| c.graph.node(*id))
                    .map(|n| Command::Move {
                        node: n.id,
                        pos: [n.pos[0] + d.x, n.pos[1] + d.y],
                    })
                    .collect();
                let _ = c
                    .history
                    .exec_merge(c.graph, Command::Batch(moves), Some("move"));
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
                Hit::None => self.selected.clear(),
            }
        }
        if resp.double_clicked()
            && let Some(p) = resp.interact_pointer_pos()
            && hit(self, p) == Hit::None
        {
            self.open_search(ui.ctx());
        }
        if resp.secondary_clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.menu_target = hit(self, p);
            if let Hit::Node(n) = self.menu_target
                && !self.selected.contains(&n)
            {
                self.selected = BTreeSet::from([n]);
            }
        }
        let mut action: Option<MenuAction> = None;
        resp.context_menu(|ui| {
            action = match self.menu_target.clone() {
                Hit::None => add_menu(ui, c.catalogue),
                _ => node_menu(ui, self.selected.len()),
            };
        });

        // ---- Keyboard ----
        let typing = ui.ctx().egui_wants_keyboard_input();
        // Editing keys only act with the mouse over the graph, so a stray key
        // elsewhere never deletes anything.
        let over = resp.hovered() || resp.contains_pointer();
        if !typing {
            let (events, del, redo, undo, bypass, all, home) = ui.input_mut(|i| {
                let cmd = i.modifiers.command;
                (
                    i.events.clone(),
                    !cmd && (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)),
                    i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z)
                        || i.consume_key(egui::Modifiers::COMMAND, Key::Y),
                    i.consume_key(egui::Modifiers::COMMAND, Key::Z),
                    i.consume_key(egui::Modifiers::COMMAND, Key::D),
                    i.consume_key(egui::Modifiers::COMMAND, Key::A),
                    i.key_pressed(Key::Home),
                )
            });
            if redo {
                let _ = c.history.redo(c.graph);
            } else if undo {
                let _ = c.history.undo(c.graph);
            }
            if del && over {
                action = Some(MenuAction::Delete);
            }
            if bypass && over {
                action = Some(MenuAction::Bypass);
            }
            if all && over {
                self.selected = c.graph.nodes().iter().map(|n| n.id).collect();
            }
            if home && over {
                self.frame_all(c.graph, rect);
            }
            for e in events {
                match e {
                    egui::Event::Copy => action = Some(MenuAction::Copy),
                    egui::Event::Cut => action = Some(MenuAction::Cut),
                    egui::Event::Paste(text) if resp.hovered() || !self.selected.is_empty() => {
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
        let step = 24.0 * self.zoom;
        if step > 6.0 {
            let o = origin(self);
            let mut x = rect.left() + (o.x - rect.left()).rem_euclid(step);
            while x < rect.right() {
                let mut y = rect.top() + (o.y - rect.top()).rem_euclid(step);
                while y < rect.bottom() {
                    painter.circle_filled(pos2(x, y), 1.0, GRID);
                    y += step;
                }
                x += step;
            }
        }
        let lay: HashMap<NodeId, &Layout> = layouts.iter().map(|(id, l)| (*id, l)).collect();
        let z = self.zoom;
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
            if *ty == PortType::Signal {
                // Wires glow with their signal.
                let v = c
                    .eval
                    .signal_value(l.from.node)
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                col = lerp_colour(Color32::from_rgb(40, 80, 140), col, 0.35 + 0.65 * v);
            }
            let on = l.modulation.is_none_or(|m| m.enabled);
            if !on {
                col = col.gamma_multiply(0.35);
            }
            wire(
                &painter,
                to_screen(self, *from),
                to_screen(self, to),
                col,
                wire_w,
                z,
            );
        }
        for (id, l) in &layouts {
            let Some(n) = c.graph.node(*id) else { continue };
            self.draw_node(&painter, &c, n, l, &to_screen);
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
                "An empty canvas. Right-click to add your first node,\nor press Shift+Space to search.",
                FontId::proportional(15.0),
                TEXT_DIM,
            );
        }
        // Help line and messages.
        let now = ui.input(|i| i.time);
        let line = match &self.message {
            Some((m, t)) if now - t < 4.0 => (m.clone(), ERROR),
            _ => (
                "Right-click: add node · drag plug → plug: wire · Shift+Space: search · \
                 scroll: zoom · middle-drag: pan · Del · Ctrl+Z · Ctrl+D bypass · Home: fit"
                    .to_string(),
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

        self.search_ui(ui, &mut c);
        self.param_pick_ui(ui, &mut c);
    }

    fn draw_node(
        &self,
        painter: &egui::Painter,
        c: &Ctx,
        n: &Node,
        l: &Layout,
        to_screen: &dyn Fn(&Self, Pos2) -> Pos2,
    ) {
        let z = self.zoom;
        let r = Rect::from_min_max(to_screen(self, l.rect.min), to_screen(self, l.rect.max));
        let round = CornerRadius::same((6.0 * z).round().clamp(2.0, 12.0) as u8);
        let dim = if n.bypass { 0.45 } else { 1.0 };
        let cat = category_colour(category(n));
        let error = c.eval.errors.get(&n.id);
        painter.rect_filled(
            r.translate(vec2(0.0, 3.0 * z)),
            round,
            Color32::from_black_alpha(90),
        );
        painter.rect_filled(
            r,
            round,
            NODE_BG.gamma_multiply(if n.bypass { 0.7 } else { 1.0 }),
        );
        let head = Rect::from_min_size(r.min, vec2(r.width(), HEADER_H * z));
        painter.rect_filled(
            head,
            CornerRadius {
                nw: round.nw,
                ne: round.ne,
                sw: 0,
                se: 0,
            },
            NODE_HEADER,
        );
        painter.rect_filled(
            Rect::from_min_size(head.min, vec2(3.0 * z, head.height())),
            CornerRadius {
                nw: round.nw,
                ..Default::default()
            },
            cat.gamma_multiply(dim),
        );
        let font = FontId::proportional((12.5 * z).clamp(6.0, 20.0));
        let small = FontId::proportional((11.0 * z).clamp(5.0, 18.0));
        let number = format!("{:02}", n.id.0);
        painter.text(
            head.left_center() + vec2(9.0 * z, 0.0),
            Align2::LEFT_CENTER,
            &number,
            font.clone(),
            TEXT_DIM.gamma_multiply(dim),
        );
        let label = truncate(&n.label, ((NODE_W - 60.0) / 7.0) as usize);
        painter.text(
            head.left_center() + vec2(32.0 * z, 0.0),
            Align2::LEFT_CENTER,
            label,
            font,
            TEXT.gamma_multiply(dim),
        );
        let tag = if error.is_some() {
            Some(("⚠", ERROR))
        } else if n.bypass {
            Some(("off", TEXT_DIM))
        } else if c.viewed == Some(n.id) {
            Some(("◉", ACCENT))
        } else {
            None
        };
        if let Some((t, col)) = tag {
            painter.text(
                head.right_center() - vec2(8.0 * z, 0.0),
                Align2::RIGHT_CENTER,
                t,
                small.clone(),
                col,
            );
        }
        let show_labels = z > 0.5;
        for (_, label, ty, at) in &l.inputs {
            let p = to_screen(self, *at);
            port(painter, p, *ty, z, dim);
            if show_labels {
                painter.text(
                    p + vec2(9.0 * z, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    small.clone(),
                    TEXT_DIM.gamma_multiply(dim),
                );
            }
        }
        for (_, _, ty, at) in &l.outputs {
            port(painter, to_screen(self, *at), *ty, z, dim);
        }
        for (name, label, at) in &l.mods {
            let p = to_screen(self, *at);
            port(painter, p, PortType::Signal, z, dim);
            if show_labels {
                let live = c.eval.live_value(n.id, name);
                let text = match live {
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
        // Signal nodes: a live level bar.
        if let Some(v) = c.eval.signal_value(n.id) {
            let y = to_screen(
                self,
                l.outputs.first().map(|o| o.3).unwrap_or(l.rect.center()),
            )
            .y;
            let bar = Rect::from_min_size(
                pos2(r.left() + 10.0 * z, y - 3.0 * z),
                vec2(r.width() * 0.55, 6.0 * z),
            );
            painter.rect_filled(bar, 2.0, Color32::from_gray(28));
            let mut f = bar;
            f.set_width(bar.width() * v.clamp(0.0, 1.0));
            painter.rect_filled(f, 2.0, port_colour(PortType::Signal));
        }
        let stroke = if self.selected.contains(&n.id) {
            Stroke::new(1.5, ACCENT)
        } else if error.is_some() {
            Stroke::new(1.0, ERROR)
        } else {
            Stroke::new(1.0, Color32::from_gray(62))
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
                if ty == PortType::Signal {
                    if node.is_some_and(|n| n.params.iter().any(|p| p.def.is_modulatable())) {
                        self.param_pick = Some((from, n, p));
                    } else {
                        self.say(ui, "That node has no sliders to drive.");
                    }
                    return;
                }
                // Picture dropped on a node: first free input that fits (else the first).
                let fits: Vec<String> = node
                    .map(|nd| {
                        nd.inputs
                            .iter()
                            .filter(|i| i.ty.accepts(ty))
                            .map(|i| i.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                fits.iter()
                    .find(|name| c.graph.input_link(n, name).is_none())
                    .or(fits.first())
                    .map(|name| Endpoint::input(n, name))
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
                    ui.label(egui::RichText::new("Drive which slider?").strong());
                    for p in n.params.iter().filter(|p| p.def.is_modulatable()) {
                        if ui.button(p.def.label()).clicked() {
                            pick = Some(p.def.name.clone());
                        }
                    }
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

    fn search_ui(&mut self, ui: &egui::Ui, c: &mut Ctx) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        let q = s.text.to_lowercase();
        let hits: Vec<&CatalogueEntry> = c
            .catalogue
            .iter()
            .filter(|e| {
                q.is_empty()
                    || e.label.to_lowercase().contains(&q)
                    || e.category.name().to_lowercase().contains(&q)
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
        let mut chosen = None;
        let mut close = esc;
        let area = egui::Area::new(Id::unique("node-search"))
            .order(Order::Foreground)
            .fixed_pos(s.screen)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(240.0);
                    let te = ui.add(
                        egui::TextEdit::singleline(&mut s.text)
                            .hint_text("Add a node… (type to search)"),
                    );
                    if s.focus {
                        te.request_focus();
                        s.focus = false;
                    }
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            for (i, e) in hits.iter().enumerate() {
                                let text = egui::RichText::new(format!(
                                    "{}   {}",
                                    e.label,
                                    e.category.name()
                                ));
                                let r = ui
                                    .selectable_label(i == s.pick, text)
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
        let at = s.at;
        if let Some(kind) = chosen {
            self.add_node(ui, c, &kind, at);
            close = true;
        }
        if close {
            self.search = None;
        }
    }

    fn add_node(&mut self, ui: &egui::Ui, c: &mut Ctx, kind: &str, at: Pos2) {
        let spec = match registry::spec_for(kind, c.library) {
            Ok(s) => s,
            Err(e) => return self.say(ui, e),
        };
        let mut n = c.graph.new_node(&spec);
        n.pos = [at.x - NODE_W * 0.5, at.y - HEADER_H * 0.5];
        let id = n.id;
        match c.history.exec(c.graph, Command::AddNode(Box::new(n))) {
            Ok(()) => self.selected = BTreeSet::from([id]),
            Err(e) => self.say(ui, e.to_string()),
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
            MenuAction::Copy | MenuAction::Cut => {
                if sel.is_empty() {
                    return;
                }
                let clip = Clip {
                    nodes: sel
                        .iter()
                        .filter_map(|id| c.graph.node(*id).cloned())
                        .collect(),
                    links: c
                        .graph
                        .links()
                        .iter()
                        .filter(|l| {
                            self.selected.contains(&l.from.node)
                                && self.selected.contains(&l.to.node())
                        })
                        .cloned()
                        .collect(),
                };
                if let Ok(text) = ron::to_string(&clip) {
                    ui.ctx().copy_text(format!("{CLIP_TAG}{text}"));
                }
                if a == MenuAction::Cut {
                    self.run_action(ui, c, MenuAction::Delete);
                }
            }
            MenuAction::Duplicate => {
                let clip = Clip {
                    nodes: sel
                        .iter()
                        .filter_map(|id| c.graph.node(*id).cloned())
                        .collect(),
                    links: c
                        .graph
                        .links()
                        .iter()
                        .filter(|l| {
                            self.selected.contains(&l.from.node)
                                && self.selected.contains(&l.to.node())
                        })
                        .cloned()
                        .collect(),
                };
                if let Ok(text) = ron::to_string(&clip) {
                    self.paste(ui, c, &format!("{CLIP_TAG}{text}"), Some(vec2(30.0, 30.0)));
                }
            }
            MenuAction::Clear => {
                let cmds = c
                    .graph
                    .nodes()
                    .iter()
                    .map(|n| Command::RemoveNode(n.id))
                    .collect();
                let _ = c.history.exec(c.graph, Command::Batch(cmds));
                self.selected.clear();
            }
            MenuAction::Starter => match nodes::demo::build(c.library) {
                Ok((g, _)) => {
                    let clip = Clip {
                        nodes: g.nodes().to_vec(),
                        links: g.links().to_vec(),
                    };
                    if let Ok(text) = ron::to_string(&clip) {
                        self.paste(ui, c, &format!("{CLIP_TAG}{text}"), None);
                    }
                }
                Err(e) => self.say(ui, e),
            },
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
            let min = clip.nodes.iter().fold(pos2(f32::MAX, f32::MAX), |m, n| {
                pos2(m.x.min(n.pos[0]), m.y.min(n.pos[1]))
            });
            self.menu_at - min
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
            .map(|n| layout(g, n).rect)
            .reduce(|a, b| a.union(b))
        else {
            self.zoom = 1.0;
            self.pan = rect.size() * 0.5;
            return;
        };
        let b = b.expand(40.0);
        self.zoom = (rect.width() / b.width())
            .min(rect.height() / b.height())
            .clamp(0.3, 1.2);
        self.pan = rect.size() * 0.5 - b.center().to_vec2() * self.zoom;
    }
}

#[derive(Clone, Debug, PartialEq)]
enum MenuAction {
    Add(String),
    Delete,
    Bypass,
    Copy,
    Cut,
    Duplicate,
    Clear,
    Starter,
}

fn add_menu(ui: &mut egui::Ui, catalogue: &[CatalogueEntry]) -> Option<MenuAction> {
    let mut pick = None;
    ui.label(egui::RichText::new("Add node").strong());
    for cat in Category::ALL {
        ui.menu_button(cat.name(), |ui| {
            for e in catalogue.iter().filter(|e| e.category == cat) {
                if ui.button(&e.label).on_hover_text(&e.description).clicked() {
                    pick = Some(MenuAction::Add(e.kind.clone()));
                }
            }
        });
    }
    ui.separator();
    if ui.button("Paste").clicked() {
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

fn node_menu(ui: &mut egui::Ui, count: usize) -> Option<MenuAction> {
    let mut pick = None;
    let s = if count > 1 { "s" } else { "" };
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
    ui.separator();
    if ui.button(format!("Delete node{s}   Del")).clicked() {
        pick = Some(MenuAction::Delete);
    }
    if pick.is_some() {
        ui.close();
    }
    pick
}

fn port(painter: &egui::Painter, p: Pos2, ty: PortType, z: f32, dim: f32) {
    let r = (PORT_R * z).clamp(2.5, 8.0);
    painter.circle_filled(p, r, CANVAS_BG);
    painter.circle_stroke(p, r, Stroke::new(1.5, port_colour(ty).gamma_multiply(dim)));
    painter.circle_filled(p, r * 0.45, port_colour(ty).gamma_multiply(dim));
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
