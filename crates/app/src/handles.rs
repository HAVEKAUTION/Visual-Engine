//! Handles drawn on the viewer for the selected Transform or Corner Pin
//! node: drag them to move, scale, turn or pin the picture. They change the
//! node's settings (one undo step per drag), so the Inspector, signals and
//! saved files all see the same values.

use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use graph::{Command, Graph, History, Node, ParamValue};
use nodes::registry;

use crate::theme;

pub const TRANSFORM: &str = "fx_transform.wgsl";
pub const CORNER_PIN: &str = "fx_corner_pin.wgsl";

/// Corner Pin's corners, clockwise from top-left: (x param, y param).
const CORNERS: [(&str, &str); 4] = [
    ("tl_x", "tl_y"),
    ("tr_x", "tr_y"),
    ("br_x", "br_y"),
    ("bl_x", "bl_y"),
];

/// Does this node have viewer handles?
pub fn has_handles(node: &Node) -> bool {
    matches!(
        registry::shader_file(&node.kind),
        Some(TRANSFORM | CORNER_PIN)
    )
}

fn value(node: &Node, name: &str) -> f32 {
    match node.param(name).map(|p| (&p.value, p.def.min, p.def.max)) {
        Some((ParamValue::Float(v), _, _)) => *v,
        _ => 0.0,
    }
}

fn clamp_to(node: &Node, name: &str, v: f32) -> f32 {
    match node.param(name) {
        Some(p) if p.def.max > p.def.min => v.clamp(p.def.min, p.def.max),
        _ => v,
    }
}

/// Rotate (x, y) counter-clockwise by `a` radians (the shaders' `rot2`).
fn rot(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    vec2(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// The viewer's picture space: `xy` is centred, y up, x in -aspect..aspect
/// (like the shaders' `in.xy`); `uv` is 0..1 from the top-left.
struct Space {
    rect: Rect,
    aspect: f32,
}

impl Space {
    fn screen_of_xy(&self, p: Vec2) -> Pos2 {
        pos2(
            self.rect.left() + (p.x / self.aspect + 1.0) * 0.5 * self.rect.width(),
            self.rect.top() + (1.0 - p.y) * 0.5 * self.rect.height(),
        )
    }
    fn to_xy(&self, s: Pos2) -> Vec2 {
        vec2(
            ((s.x - self.rect.left()) / self.rect.width() * 2.0 - 1.0) * self.aspect,
            1.0 - (s.y - self.rect.top()) / self.rect.height() * 2.0,
        )
    }
    fn screen_of_uv(&self, uv: Vec2) -> Pos2 {
        self.rect.min + uv * self.rect.size()
    }
    fn to_uv(&self, s: Pos2) -> Vec2 {
        (s - self.rect.min) / self.rect.size()
    }
}

/// Where the Transform node puts the input picture: its centre and its
/// four corners (clockwise from top-left) in viewer `xy`.
pub fn transform_frame(zoom: f32, angle: f32, mv: Vec2, aspect: f32) -> (Vec2, [Vec2; 4]) {
    // The shader shows input point p at xy = zoom · R(θ) · (p + move).
    let theta = angle * std::f32::consts::TAU;
    let place = |p: Vec2| rot(p + mv, theta) * zoom;
    let corners = [
        vec2(-aspect, 1.0),
        vec2(aspect, 1.0),
        vec2(aspect, -1.0),
        vec2(-aspect, -1.0),
    ];
    (place(Vec2::ZERO), corners.map(place))
}

/// The Transform settings that put the picture's centre at `centre`
/// (viewer `xy`), keeping zoom and angle.
pub fn move_for_centre(centre: Vec2, zoom: f32, angle: f32) -> Vec2 {
    rot(centre / zoom.max(1e-3), -angle * std::f32::consts::TAU)
}

/// Zoom and angle (turns, 0..1) that put the corner `corner` (input `p`,
/// i.e. (±aspect, ±1)) at `to`, around the picture centre `centre`.
pub fn zoom_angle_for_corner(corner: Vec2, to: Vec2, centre: Vec2) -> (f32, f32) {
    let v = to - centre;
    let zoom = v.length() / corner.length().max(1e-6);
    let theta = v.y.atan2(v.x) - corner.y.atan2(corner.x);
    (zoom, (theta / std::f32::consts::TAU).rem_euclid(1.0))
}

fn handle(ui: &egui::Ui, at: Pos2, id: egui::Id, big: bool) -> egui::Response {
    let r = if big { 9.0 } else { 7.0 };
    let resp = ui.interact(
        Rect::from_center_size(at, Vec2::splat(r * 2.6)),
        id,
        Sense::drag(),
    );
    let hot = resp.hovered() || resp.dragged();
    let p = ui.painter();
    p.circle_filled(at, r + 2.0, Color32::from_black_alpha(160));
    p.circle_filled(
        at,
        r,
        if hot {
            theme::ACCENT
        } else {
            Color32::from_gray(235)
        },
    );
    p.circle_stroke(at, r, Stroke::new(1.5, theme::ACCENT));
    resp
}

fn outline(ui: &egui::Ui, pts: &[Pos2; 4]) {
    let p = ui.painter();
    for i in 0..4 {
        let (a, b) = (pts[i], pts[(i + 1) % 4]);
        p.line_segment([a, b], Stroke::new(3.0, Color32::from_black_alpha(140)));
        p.line_segment([a, b], Stroke::new(1.5, theme::ACCENT));
    }
}

fn set_many(g: &mut Graph, history: &mut History, node: &Node, values: &[(&str, f32)], key: &str) {
    let cmds = values
        .iter()
        .map(|(name, v)| Command::SetParam {
            node: node.id,
            param: (*name).into(),
            value: ParamValue::Float(clamp_to(node, name, *v)),
        })
        .collect();
    let _ = history.exec_merge(g, Command::Batch(cmds), Some(key));
}

/// Draw the handles for `node` over the viewer `rect` and apply drags.
/// Returns a short hint while the pointer is on a handle.
pub fn show(
    ui: &egui::Ui,
    rect: Rect,
    g: &mut Graph,
    history: &mut History,
    node: &Node,
) -> Option<&'static str> {
    let space = Space {
        rect,
        aspect: rect.width() / rect.height().max(1.0),
    };
    let base = ui.scope_id().with(("viewer-handles", node.id.0));
    let key = format!("handles{}", node.id.0);
    let mut hint = None;
    let mut ended = false;
    match registry::shader_file(&node.kind) {
        Some(TRANSFORM) => {
            let zoom = value(node, "zoom");
            let angle = value(node, "angle");
            let mv = vec2(value(node, "move_x"), value(node, "move_y"));
            let (centre, corners) = transform_frame(zoom, angle, mv, space.aspect);
            let pts = corners.map(|c| space.screen_of_xy(c));
            outline(ui, &pts);
            let input_corners = [
                vec2(-space.aspect, 1.0),
                vec2(space.aspect, 1.0),
                vec2(space.aspect, -1.0),
                vec2(-space.aspect, -1.0),
            ];
            for (i, at) in pts.iter().enumerate() {
                let r = handle(ui, *at, base.with(i), false);
                if r.hovered() || r.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                    hint = Some("Drag a corner to scale and turn (hold Shift: scale only)");
                }
                if r.dragged()
                    && let Some(p) = r.interact_pointer_pos()
                {
                    let (z, a) = zoom_angle_for_corner(input_corners[i], space.to_xy(p), centre);
                    let a = if ui.input(|i| i.modifiers.shift) {
                        angle
                    } else {
                        a
                    };
                    // Keep the centre where it is under the new zoom and angle.
                    let m = move_for_centre(centre, z.clamp(0.2, 4.0), a);
                    set_many(
                        g,
                        history,
                        node,
                        &[("zoom", z), ("angle", a), ("move_x", m.x), ("move_y", m.y)],
                        &key,
                    );
                }
                ended |= r.drag_stopped();
            }
            let r = handle(ui, space.screen_of_xy(centre), base.with("centre"), true);
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::Move);
                hint = Some("Drag the middle to move the picture");
            }
            if r.dragged()
                && let Some(p) = r.interact_pointer_pos()
            {
                let m = move_for_centre(space.to_xy(p), zoom, angle);
                set_many(g, history, node, &[("move_x", m.x), ("move_y", m.y)], &key);
            }
            ended |= r.drag_stopped();
        }
        Some(CORNER_PIN) => {
            let uv: [Vec2; 4] = CORNERS.map(|(x, y)| vec2(value(node, x), value(node, y)));
            let pts = uv.map(|c| space.screen_of_uv(c));
            outline(ui, &pts);
            for (i, at) in pts.iter().enumerate() {
                let r = handle(ui, *at, base.with(i), false);
                if r.hovered() || r.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                    hint = Some("Drag a corner to pin it there");
                }
                if r.dragged()
                    && let Some(p) = r.interact_pointer_pos()
                {
                    let c = space.to_uv(p);
                    let (x, y) = CORNERS[i];
                    set_many(g, history, node, &[(x, c.x), (y, c.y)], &key);
                }
                ended |= r.drag_stopped();
            }
            let mid = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / 4.0;
            let r = handle(ui, mid.to_pos2(), base.with("centre"), true);
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::Move);
                hint = Some("Drag the middle to move all four corners");
            }
            if r.dragged() {
                let d = r.drag_delta() / rect.size();
                let mut values = Vec::new();
                for (k, (x, y)) in CORNERS.iter().enumerate() {
                    values.push((*x, uv[k].x + d.x));
                    values.push((*y, uv[k].y + d.y));
                }
                set_many(g, history, node, &values, &key);
            }
            ended |= r.drag_stopped();
        }
        _ => {}
    }
    if ended {
        history.seal();
    }
    hint
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn transform_handles_match_the_shader() {
        let aspect = 16.0 / 9.0;
        let (zoom, angle, mv) = (1.7, 0.13, vec2(0.2, -0.3));
        let (centre, corners) = transform_frame(zoom, angle, mv, aspect);
        // The shader: p = R(-θ)·xy / zoom - move. Each handle must map back
        // to the input point it stands for.
        let back = |xy: Vec2| rot(xy, -angle * std::f32::consts::TAU) / zoom - mv;
        assert!(close(back(centre), Vec2::ZERO));
        assert!(close(back(corners[0]), vec2(-aspect, 1.0)));
        assert!(close(back(corners[2]), vec2(aspect, -1.0)));
        // Moving the centre and dragging a corner give back the settings.
        assert!(close(move_for_centre(centre, zoom, angle), mv));
        let (z, a) = zoom_angle_for_corner(vec2(aspect, 1.0), corners[1], centre);
        assert!(
            (z - zoom).abs() < 1e-4 && (a - angle).abs() < 1e-4,
            "{z} {a}"
        );
    }

    #[test]
    fn screen_and_picture_space_round_trip() {
        let s = Space {
            rect: Rect::from_min_size(pos2(100.0, 50.0), vec2(640.0, 360.0)),
            aspect: 640.0 / 360.0,
        };
        let p = pos2(321.0, 97.0);
        assert!((s.screen_of_xy(s.to_xy(p)) - p).length() < 1e-3);
        assert!((s.screen_of_uv(s.to_uv(p)) - p).length() < 1e-3);
        // Top-left of the viewer is xy (-aspect, 1).
        assert!(close(s.to_xy(s.rect.min), vec2(-s.aspect, 1.0)));
    }
}
