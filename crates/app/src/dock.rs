//! The dockable workspace: every panel is a tab you can drag to another
//! place, split beside another panel, resize or hide. Three ready-made
//! layouts (Build, Perform, Stream); the current layout is remembered.

use egui_tiles::{Container, Linear, LinearDir, Tabs, TileId, Tiles, Tree};
use serde::{Deserialize, Serialize};

/// One panel of the workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    Viewer,
    Graph,
    Inspector,
    Sources,
    Media,
    Looks,
    Analysis,
    Meters,
}

impl Pane {
    pub const ALL: [Pane; 8] = [
        Pane::Viewer,
        Pane::Graph,
        Pane::Inspector,
        Pane::Sources,
        Pane::Media,
        Pane::Looks,
        Pane::Analysis,
        Pane::Meters,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Pane::Viewer => "Viewer",
            Pane::Graph => "Node Graph",
            Pane::Inspector => "Inspector",
            Pane::Sources => "Sources",
            Pane::Media => "Media",
            Pane::Looks => "Looks",
            Pane::Analysis => "Analysis",
            Pane::Meters => "Meters",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Pane::Viewer => "What the audience sees (or the selected node)",
            Pane::Graph => "The node graph: right-click to add, drag plugs to wire (Tab)",
            Pane::Inspector => "Settings of the selected node",
            Pane::Sources => "Where the sound comes from (song, input, computer audio) and cameras",
            Pane::Media => "Your pictures and videos",
            Pane::Looks => "Signature looks (coming in M6)",
            Pane::Analysis => "Tempo, tap, events, offline analysis",
            Pane::Meters => "Every signal as a meter (M)",
        }
    }
}

/// A ready-made layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// Building visuals: everything in view.
    Build,
    /// Playing live: a big viewer, sources and inspector at the side.
    Perform,
    /// Streaming / presenting: almost all viewer.
    Stream,
}

impl Layout {
    pub const ALL: [Layout; 3] = [Layout::Build, Layout::Perform, Layout::Stream];

    pub fn name(self) -> &'static str {
        match self {
            Layout::Build => "Build",
            Layout::Perform => "Perform",
            Layout::Stream => "Stream",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Layout::Build => "Everything in view: sources, viewer, inspector, graph",
            Layout::Perform => "A big viewer, with sources and the inspector beside it",
            Layout::Stream => "Nearly all viewer, sources in a slim column",
        }
    }
}

fn tabs(tiles: &mut Tiles<Pane>, panes: &[Pane], hidden: &[Pane]) -> TileId {
    let ids: Vec<TileId> = panes.iter().map(|p| tiles.insert_pane(*p)).collect();
    for (p, id) in panes.iter().zip(&ids) {
        if hidden.contains(p) {
            tiles.set_visible(*id, false);
        }
    }
    let first = panes.iter().position(|p| !hidden.contains(p)).unwrap_or(0);
    let mut t = Tabs::new(ids.clone());
    t.set_active(ids[first]);
    tiles.insert_container(t)
}

fn split(tiles: &mut Tiles<Pane>, dir: LinearDir, parts: &[(TileId, f32)]) -> TileId {
    let mut l = Linear::new(dir, parts.iter().map(|p| p.0).collect());
    for (id, share) in parts {
        l.shares.set_share(*id, *share);
    }
    tiles.insert_container(Container::Linear(l))
}

/// A fresh layout. Every pane is in it (some hidden), so the Panels menu
/// can show any of them.
pub fn build(layout: Layout) -> Tree<Pane> {
    use LinearDir::{Horizontal as H, Vertical as V};
    let mut t = Tiles::default();
    let root = match layout {
        Layout::Build => {
            let left = tabs(
                &mut t,
                &[Pane::Sources, Pane::Media, Pane::Looks, Pane::Analysis],
                &[],
            );
            let viewer = tabs(&mut t, &[Pane::Viewer], &[]);
            let inspector = tabs(&mut t, &[Pane::Inspector], &[]);
            let top = split(&mut t, H, &[(left, 0.22), (viewer, 0.5), (inspector, 0.28)]);
            let bottom = tabs(&mut t, &[Pane::Graph, Pane::Meters], &[Pane::Meters]);
            split(&mut t, V, &[(top, 0.58), (bottom, 0.42)])
        }
        Layout::Perform => {
            let viewer = tabs(&mut t, &[Pane::Viewer], &[]);
            let side = tabs(
                &mut t,
                &[Pane::Sources, Pane::Inspector, Pane::Media, Pane::Looks],
                &[],
            );
            let top = split(&mut t, H, &[(viewer, 0.74), (side, 0.26)]);
            let bottom = tabs(
                &mut t,
                &[Pane::Graph, Pane::Analysis, Pane::Meters],
                &[Pane::Meters],
            );
            split(&mut t, V, &[(top, 0.75), (bottom, 0.25)])
        }
        Layout::Stream => {
            let viewer = tabs(&mut t, &[Pane::Viewer], &[]);
            let side = tabs(
                &mut t,
                &[
                    Pane::Sources,
                    Pane::Inspector,
                    Pane::Media,
                    Pane::Looks,
                    Pane::Analysis,
                    Pane::Graph,
                    Pane::Meters,
                ],
                &[Pane::Graph, Pane::Meters, Pane::Looks, Pane::Analysis],
            );
            split(&mut t, H, &[(viewer, 0.82), (side, 0.18)])
        }
    };
    Tree::new(egui::Id::unique("workspace"), root, t)
}

/// A saved layout is usable if every pane is in it exactly once.
pub fn is_complete(tree: &Tree<Pane>) -> bool {
    Pane::ALL.iter().all(|p| {
        tree.tiles
            .tiles()
            .filter(|t| matches!(t, egui_tiles::Tile::Pane(x) if x == p))
            .count()
            == 1
    })
}

pub fn find(tree: &Tree<Pane>, pane: Pane) -> Option<TileId> {
    tree.tiles.find_pane(&pane)
}

/// Is this pane shown (not hidden; its tab may still be behind another)?
pub fn is_shown(tree: &Tree<Pane>, pane: Pane) -> bool {
    find(tree, pane).is_some_and(|id| tree.tiles.is_visible(id))
}

/// Show or hide a pane. Showing it also brings its tab to the front.
pub fn set_shown(tree: &mut Tree<Pane>, pane: Pane, shown: bool) {
    let Some(id) = find(tree, pane) else { return };
    tree.tiles.set_visible(id, shown);
    if shown {
        tree.make_active(|_, tile| matches!(tile, egui_tiles::Tile::Pane(p) if *p == pane));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_has_every_pane_once() {
        for l in Layout::ALL {
            let tree = build(l);
            assert!(is_complete(&tree), "{l:?}");
            assert!(is_shown(&tree, Pane::Viewer), "{l:?}");
            assert!(!is_shown(&tree, Pane::Meters), "{l:?}");
        }
    }

    #[test]
    fn panes_hide_and_show_and_the_layout_saves() {
        let mut tree = build(Layout::Build);
        set_shown(&mut tree, Pane::Graph, false);
        assert!(!is_shown(&tree, Pane::Graph));
        set_shown(&mut tree, Pane::Meters, true);
        assert!(is_shown(&tree, Pane::Meters));
        let text = ron::to_string(&tree).unwrap();
        let back: Tree<Pane> = ron::from_str(&text).unwrap();
        assert!(is_complete(&back));
        assert!(!is_shown(&back, Pane::Graph));
        assert!(is_shown(&back, Pane::Meters));
    }
}
