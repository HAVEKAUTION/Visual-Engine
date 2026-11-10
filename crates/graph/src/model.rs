//! The graph: nodes, the wires between them, and groups.
//!
//! The model is kept in a **canonical order** (nodes and groups by id, links by
//! destination then source) so that two graphs with the same content compare
//! equal and save to the same text, whatever order the edits happened in.
//! Edits go through [`crate::Command`] so every one can be undone.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use signals::modulation::Shape;

use crate::types::{MASK_PORT, NodeSpec, Param, PortDef, PortType};

/// A node's number. Nodes are numbered in creation order and numbers are never
/// reused, like Resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(pub u32);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}", self.0)
    }
}

/// The size a node renders at, relative to its output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    #[default]
    Inherit,
    Half,
    Quarter,
    Custom {
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    /// Type name from its [`NodeSpec`].
    pub kind: String,
    pub label: String,
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
    pub params: Vec<Param>,
    /// Position in the editor.
    #[serde(default)]
    pub pos: [f32; 2],
    /// Pass the input straight through (or output nothing).
    #[serde(default)]
    pub bypass: bool,
    #[serde(default)]
    pub resolution: Resolution,
    /// Colour tag chosen by the user (`None` = the type's own colour).
    #[serde(default)]
    pub colour: Option<[u8; 3]>,
    /// Hide the live picture inside the node in the editor (view only).
    #[serde(default)]
    pub hide_thumb: bool,
}

impl Node {
    /// Bring a node saved by an older version up to date with its type:
    /// plugs and settings added since are added (settings at their
    /// defaults, in the spec's order). Nothing is removed or changed.
    /// Returns true if anything was added.
    pub fn add_missing_from(&mut self, spec: &NodeSpec) -> bool {
        let mut changed = false;
        for (i, port) in spec.inputs.iter().enumerate() {
            if self.input(&port.name).is_none() {
                // Before the automatic mask plug, which always comes last.
                let at = self
                    .inputs
                    .iter()
                    .position(|p| p.name == MASK_PORT)
                    .unwrap_or(self.inputs.len())
                    .min(i);
                self.inputs.insert(at, port.clone());
                changed = true;
            }
        }
        for (i, def) in spec.params.iter().enumerate() {
            if self.param(&def.name).is_none() {
                let at = i.min(self.params.len());
                self.params.insert(at, Param::new(def.clone()));
                changed = true;
            }
        }
        changed
    }

    pub fn input(&self, name: &str) -> Option<&PortDef> {
        self.inputs.iter().find(|p| p.name == name)
    }

    pub fn output(&self, name: &str) -> Option<&PortDef> {
        self.outputs.iter().find(|p| p.name == name)
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.def.name == name)
    }

    pub fn param_mut(&mut self, name: &str) -> Option<&mut Param> {
        self.params.iter_mut().find(|p| p.def.name == name)
    }

    /// Does this node put out a picture?
    pub fn makes_image(&self) -> bool {
        self.outputs.iter().any(|p| p.ty == PortType::Image)
    }
}

/// An output plug: node + output name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OutputRef {
    pub node: NodeId,
    pub port: String,
}

impl OutputRef {
    pub fn new(node: NodeId, port: &str) -> Self {
        Self {
            node,
            port: port.into(),
        }
    }
}

/// Where a wire ends: an input plug, or a parameter (Signal wires only).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Endpoint {
    Input { node: NodeId, port: String },
    Param { node: NodeId, param: String },
}

impl Endpoint {
    pub fn input(node: NodeId, port: &str) -> Self {
        Endpoint::Input {
            node,
            port: port.into(),
        }
    }

    pub fn param(node: NodeId, param: &str) -> Self {
        Endpoint::Param {
            node,
            param: param.into(),
        }
    }

    pub fn node(&self) -> NodeId {
        match self {
            Endpoint::Input { node, .. } | Endpoint::Param { node, .. } => *node,
        }
    }
}

/// How a Signal wire into a parameter moves it: the same shape and depth as
/// the mapping rule's modulation slots (the wire's value is the source).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireMod {
    pub shape: Shape,
    /// Fraction of the parameter's range; negative pushes the other way.
    pub depth: f32,
    pub enabled: bool,
}

impl Default for WireMod {
    fn default() -> Self {
        Self {
            shape: Shape::Linear,
            depth: 0.5,
            enabled: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub from: OutputRef,
    pub to: Endpoint,
    /// Set for wires into parameters.
    #[serde(default)]
    pub modulation: Option<WireMod>,
}

impl Link {
    pub fn new(from: OutputRef, to: Endpoint) -> Self {
        Self {
            from,
            to,
            modulation: None,
        }
    }

    /// Same wire (same ends), whatever its modulation settings.
    pub fn same_ends(&self, other: &Link) -> bool {
        self.from == other.from && self.to == other.to
    }

    fn sort_key(&self) -> (&Endpoint, &OutputRef) {
        (&self.to, &self.from)
    }
}

/// A parameter inside a group, published as one of the group's controls
/// (these become performance knobs and pads).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PublishedParam {
    pub label: String,
    pub node: NodeId,
    pub param: String,
}

/// A coloured, titled area of the graph (a backdrop) for keeping things tidy.
/// Nodes inside it move with it. Purely visual: it never changes the picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub id: u32,
    pub label: String,
    /// x, y, width, height in graph units.
    pub rect: [f32; 4],
    pub colour: [u8; 3],
}

/// A set of nodes shown as one node. Groups can sit inside other groups
/// (`parent`); a node belongs to at most one group (the innermost).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub label: String,
    #[serde(default)]
    pub parent: Option<GroupId>,
    pub nodes: Vec<NodeId>,
    #[serde(default)]
    pub published: Vec<PublishedParam>,
    /// Shown as one box in the editor (purely visual).
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GraphError {
    #[error("there is no node {0}")]
    NoSuchNode(NodeId),
    #[error("node {0} already exists")]
    NodeExists(NodeId),
    #[error("node {node} has no {what} called \"{name}\"")]
    NoSuchPort {
        node: NodeId,
        what: &'static str,
        name: String,
    },
    #[error("a {from} wire can't go into a {to} input")]
    TypeMismatch {
        from: &'static str,
        to: &'static str,
    },
    #[error("only Signal wires can drive a parameter")]
    NotASignal,
    #[error("\"{0}\" can't be modulated (only number parameters can)")]
    NotModulatable(String),
    #[error("\"{0}\" needs a different kind of value")]
    WrongValue(String),
    #[error("that connection would make a loop")]
    Cycle,
    #[error("that wire is already there")]
    AlreadyConnected,
    #[error("there is no such wire")]
    NoSuchLink,
    #[error("there is no frame {0}")]
    NoSuchFrame(u32),
    #[error("there is no group {0}")]
    NoSuchGroup(u32),
    #[error("group {0}: {1}")]
    BadGroup(u32, String),
    #[error("could not read the graph: {0}")]
    Parse(String),
}

/// Graph file format version, written into saved files.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    #[serde(default = "format_version")]
    pub version: u32,
    /// Next node / group number to hand out.
    next_node: u32,
    next_group: u32,
    nodes: Vec<Node>,
    links: Vec<Link>,
    groups: Vec<Group>,
    #[serde(default)]
    frames: Vec<Frame>,
    #[serde(default)]
    next_frame: u32,
}

fn format_version() -> u32 {
    FORMAT_VERSION
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    pub fn new() -> Self {
        Self {
            version: FORMAT_VERSION,
            next_node: 1,
            next_group: 1,
            nodes: Vec::new(),
            links: Vec::new(),
            groups: Vec::new(),
            frames: Vec::new(),
            next_frame: 1,
        }
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn frame(&self, id: u32) -> Option<&Frame> {
        self.frames.iter().find(|f| f.id == id)
    }

    /// The next free frame number (reserve it by adding the frame).
    pub fn new_frame_id(&mut self) -> u32 {
        self.next_frame = self.next_frame.max(1);
        let id = self.next_frame;
        self.next_frame += 1;
        id
    }

    /// Add a frame, or replace the one with the same id. Returns the old one.
    pub(crate) fn put_frame_raw(&mut self, f: Frame) -> Option<Frame> {
        self.next_frame = self.next_frame.max(f.id + 1);
        match self.frames.iter().position(|x| x.id == f.id) {
            Some(i) => Some(std::mem::replace(&mut self.frames[i], f)),
            None => {
                let i = self.frames.partition_point(|x| x.id < f.id);
                self.frames.insert(i, f);
                None
            }
        }
    }

    pub(crate) fn take_frame(&mut self, id: u32) -> Option<Frame> {
        let i = self.frames.iter().position(|f| f.id == id)?;
        Some(self.frames.remove(i))
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn links(&self) -> &[Link] {
        &self.links
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// Bring nodes saved by an older version up to date (see
    /// [`Node::add_missing_from`]); `spec_for` gives the current spec of a
    /// kind, or `None` to leave it alone. Returns how many changed.
    pub fn upgrade_nodes(&mut self, spec_for: impl Fn(&str) -> Option<NodeSpec>) -> usize {
        let mut n = 0;
        for node in &mut self.nodes {
            if let Some(spec) = spec_for(&node.kind)
                && node.add_missing_from(&spec)
            {
                n += 1;
            }
        }
        n
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes
            .binary_search_by_key(&id, |n| n.id)
            .ok()
            .map(|i| &self.nodes[i])
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> Result<&mut Node, GraphError> {
        match self.nodes.binary_search_by_key(&id, |n| n.id) {
            Ok(i) => Ok(&mut self.nodes[i]),
            Err(_) => Err(GraphError::NoSuchNode(id)),
        }
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// The group a node belongs to directly, if any.
    pub fn group_of(&self, node: NodeId) -> Option<&Group> {
        self.groups.iter().find(|g| g.nodes.contains(&node))
    }

    /// The groups a node sits in, innermost first.
    pub fn groups_around(&self, node: NodeId) -> Vec<GroupId> {
        let mut out = Vec::new();
        let mut g = self.group_of(node).map(|g| g.id);
        while let Some(id) = g {
            if out.contains(&id) {
                break; // never loop, even on a bad file
            }
            out.push(id);
            g = self.group(id).and_then(|g| g.parent);
        }
        out
    }

    /// A group's parents, innermost first (not including itself).
    pub fn group_ancestors(&self, id: GroupId) -> Vec<GroupId> {
        let mut out = Vec::new();
        let mut g = self.group(id).and_then(|g| g.parent);
        while let Some(p) = g {
            if out.contains(&p) || p == id {
                break;
            }
            out.push(p);
            g = self.group(p).and_then(|g| g.parent);
        }
        out
    }

    /// How many groups `id` sits inside.
    pub fn group_depth(&self, id: GroupId) -> usize {
        self.group_ancestors(id).len()
    }

    /// Every node in a group, including its sub-groups', in id order.
    pub fn group_members(&self, id: GroupId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self
            .nodes
            .iter()
            .map(|n| n.id)
            .filter(|n| self.groups_around(*n).contains(&id))
            .collect();
        out.sort();
        out
    }

    /// One step that puts `nodes` into a new group called `label`, nested in
    /// the innermost group they all share. Nodes from deeper groups bring
    /// their whole sub-group along (it becomes a sub-group of the new one).
    pub fn group_nodes(
        &mut self,
        nodes: &[NodeId],
        label: &str,
    ) -> Result<(GroupId, crate::Command), GraphError> {
        use crate::Command;
        if nodes.is_empty() {
            return Err(GraphError::BadGroup(0, "nothing selected to group".into()));
        }
        for n in nodes {
            self.node(*n).ok_or(GraphError::NoSuchNode(*n))?;
        }
        let chains: Vec<Vec<GroupId>> = nodes.iter().map(|n| self.groups_around(*n)).collect();
        // The innermost group every node is inside.
        let shared = chains[0]
            .iter()
            .copied()
            .find(|g| chains.iter().all(|c| c.contains(g)));
        let mut direct = Vec::new();
        let mut children = Vec::new();
        for (n, chain) in nodes.iter().zip(&chains) {
            let below: Vec<GroupId> = chain
                .iter()
                .copied()
                .take_while(|g| Some(*g) != shared)
                .collect();
            match below.last() {
                None => direct.push(*n),
                Some(top) => {
                    if !children.contains(top) {
                        children.push(*top);
                    }
                }
            }
        }
        let id = self.new_group_id();
        let mut cmds = Vec::new();
        // The shared group lets go of the nodes first (its published controls
        // are put back last, once the new group holds those nodes).
        let parent_after = shared.and_then(|p| self.group(p)).map(|p| {
            let mut p = p.clone();
            p.nodes.retain(|n| !direct.contains(n));
            p
        });
        if let Some(p) = &parent_after {
            cmds.push(Command::PutGroup(Group {
                published: Vec::new(),
                ..p.clone()
            }));
        }
        cmds.push(Command::PutGroup(Group {
            id,
            label: label.to_string(),
            parent: shared,
            nodes: direct,
            published: Vec::new(),
            collapsed: false,
        }));
        for c in children {
            if let Some(g) = self.group(c) {
                cmds.push(Command::PutGroup(Group {
                    parent: Some(id),
                    ..g.clone()
                }));
            }
        }
        if let Some(p) = parent_after {
            cmds.push(Command::PutGroup(p));
        }
        Ok((id, Command::Batch(cmds)))
    }

    /// A new node for `spec` with the next free number (not yet in the graph:
    /// add it with [`crate::Command::AddNode`]). Nodes that make a picture
    /// get an optional `mask` input automatically.
    pub fn new_node(&mut self, spec: &NodeSpec) -> Node {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        let mut inputs = spec.inputs.clone();
        let makes_image = spec.outputs.iter().any(|p| p.ty == PortType::Image);
        if makes_image && !inputs.iter().any(|p| p.name == MASK_PORT) {
            inputs.push(PortDef {
                name: MASK_PORT.into(),
                label: "Mask".into(),
                ty: PortType::Mask,
                optional: true,
            });
        }
        Node {
            id,
            kind: spec.kind.clone(),
            label: spec.label.clone(),
            inputs,
            outputs: spec.outputs.clone(),
            params: spec.params.iter().cloned().map(Param::new).collect(),
            pos: [0.0, 0.0],
            bypass: false,
            resolution: Resolution::Inherit,
            colour: None,
            hide_thumb: false,
        }
    }

    /// Hand out the next node number without making a node (for pasting
    /// copies of existing nodes).
    pub fn reserve_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        id
    }

    /// The next free group number (reserve it by adding the group).
    pub fn new_group_id(&mut self) -> GroupId {
        let id = GroupId(self.next_group);
        self.next_group += 1;
        id
    }

    /// The wire plugged into an input, if any.
    pub fn input_link(&self, node: NodeId, port: &str) -> Option<&Link> {
        self.links.iter().find(
            |l| matches!(&l.to, Endpoint::Input { node: n, port: p } if *n == node && p == port),
        )
    }

    /// Signal wires driving a parameter.
    pub fn param_links(&self, node: NodeId, param: &str) -> impl Iterator<Item = &Link> {
        self.links.iter().filter(move |l| {
            matches!(&l.to, Endpoint::Param { node: n, param: p } if *n == node && p == param)
        })
    }

    /// Nodes that `node` feeds, directly or not (including itself).
    pub fn downstream(&self, node: NodeId) -> BTreeSet<NodeId> {
        let mut seen = BTreeSet::from([node]);
        let mut todo = vec![node];
        while let Some(n) = todo.pop() {
            for l in self.links.iter().filter(|l| l.from.node == n) {
                let next = l.to.node();
                if seen.insert(next) {
                    todo.push(next);
                }
            }
        }
        seen
    }

    /// Check that `link` could be added: both ends exist, types match, no loop,
    /// not a duplicate. (Replacing what is already in an input is allowed.)
    pub fn check_link(&self, link: &Link) -> Result<(), GraphError> {
        let from_node = self
            .node(link.from.node)
            .ok_or(GraphError::NoSuchNode(link.from.node))?;
        let out = from_node
            .output(&link.from.port)
            .ok_or_else(|| GraphError::NoSuchPort {
                node: from_node.id,
                what: "output",
                name: link.from.port.clone(),
            })?;
        let to_id = link.to.node();
        let to_node = self.node(to_id).ok_or(GraphError::NoSuchNode(to_id))?;
        match &link.to {
            Endpoint::Input { port, .. } => {
                let inp = to_node.input(port).ok_or_else(|| GraphError::NoSuchPort {
                    node: to_id,
                    what: "input",
                    name: port.clone(),
                })?;
                if !inp.ty.accepts(out.ty) {
                    return Err(GraphError::TypeMismatch {
                        from: out.ty.name(),
                        to: inp.ty.name(),
                    });
                }
            }
            Endpoint::Param { param, .. } => {
                let p = to_node.param(param).ok_or_else(|| GraphError::NoSuchPort {
                    node: to_id,
                    what: "parameter",
                    name: param.clone(),
                })?;
                if out.ty != PortType::Signal {
                    return Err(GraphError::NotASignal);
                }
                if !p.def.is_modulatable() {
                    return Err(GraphError::NotModulatable(param.clone()));
                }
            }
        }
        if self.links.iter().any(|l| l.same_ends(link)) {
            return Err(GraphError::AlreadyConnected);
        }
        // A loop: the source is already fed by the destination.
        if self.downstream(to_id).contains(&link.from.node) {
            return Err(GraphError::Cycle);
        }
        Ok(())
    }

    // ---- low-level mutation, used by commands (keeps canonical order) ----

    pub(crate) fn insert_node(&mut self, node: Node) -> Result<(), GraphError> {
        match self.nodes.binary_search_by_key(&node.id, |n| n.id) {
            Ok(_) => Err(GraphError::NodeExists(node.id)),
            Err(i) => {
                self.next_node = self.next_node.max(node.id.0 + 1);
                self.nodes.insert(i, node);
                Ok(())
            }
        }
    }

    pub(crate) fn take_node(&mut self, id: NodeId) -> Result<Node, GraphError> {
        match self.nodes.binary_search_by_key(&id, |n| n.id) {
            Ok(i) => Ok(self.nodes.remove(i)),
            Err(_) => Err(GraphError::NoSuchNode(id)),
        }
    }

    pub(crate) fn insert_link(&mut self, link: Link) {
        let i = self
            .links
            .partition_point(|l| l.sort_key() < link.sort_key());
        self.links.insert(i, link);
    }

    pub(crate) fn take_link(&mut self, link: &Link) -> Result<Link, GraphError> {
        let i = self
            .links
            .iter()
            .position(|l| l.same_ends(link))
            .ok_or(GraphError::NoSuchLink)?;
        Ok(self.links.remove(i))
    }

    pub(crate) fn links_mut(&mut self) -> &mut Vec<Link> {
        &mut self.links
    }

    pub(crate) fn groups_mut(&mut self) -> &mut Vec<Group> {
        &mut self.groups
    }

    pub(crate) fn put_group_raw(&mut self, g: Group) -> Option<Group> {
        self.next_group = self.next_group.max(g.id.0 + 1);
        match self.groups.iter().position(|x| x.id == g.id) {
            Some(i) => Some(std::mem::replace(&mut self.groups[i], g)),
            None => {
                let i = self.groups.partition_point(|x| x.id < g.id);
                self.groups.insert(i, g);
                None
            }
        }
    }

    /// Check a group before adding / replacing it.
    pub(crate) fn check_group(&self, g: &Group) -> Result<(), GraphError> {
        let bad = |msg: String| GraphError::BadGroup(g.id.0, msg);
        for n in &g.nodes {
            if self.node(*n).is_none() {
                return Err(bad(format!("node {n} doesn't exist")));
            }
            if let Some(other) = self.group_of(*n)
                && other.id != g.id
            {
                return Err(bad(format!("node {n} is already in group {}", other.id.0)));
            }
        }
        // Parent chain must exist and not loop back to this group.
        let mut parent = g.parent;
        let mut steps = 0;
        while let Some(p) = parent {
            if p == g.id || steps > self.groups.len() {
                return Err(bad("groups can't contain themselves".into()));
            }
            parent = self
                .group(p)
                .ok_or_else(|| bad(format!("parent group {} doesn't exist", p.0)))?
                .parent;
            steps += 1;
        }
        for pp in &g.published {
            let in_group = self.group_contains(g, pp.node);
            let has = self
                .node(pp.node)
                .and_then(|n| n.param(&pp.param))
                .is_some();
            if !in_group || !has {
                return Err(bad(format!(
                    "published control \"{}\" points at a parameter outside the group",
                    pp.label
                )));
            }
        }
        Ok(())
    }

    /// Is `node` in `g` or in one of its sub-groups?
    pub fn group_contains(&self, g: &Group, node: NodeId) -> bool {
        if g.nodes.contains(&node) {
            return true;
        }
        self.groups
            .iter()
            .filter(|c| c.parent == Some(g.id))
            .any(|c| self.group_contains(c, node))
    }

    // ---- saving and loading ----

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap_or_default()
    }

    /// Load and check a graph. Files edited by hand or saved by an older
    /// version are checked link by link, so a broken file is reported instead
    /// of loading something invalid.
    pub fn from_ron(text: &str) -> Result<Self, GraphError> {
        let loaded: Graph = ron::from_str(text).map_err(|e| GraphError::Parse(e.to_string()))?;
        loaded.validated()
    }

    /// Rebuild this graph through the normal checks (canonical order,
    /// valid links and groups).
    pub fn validated(self) -> Result<Self, GraphError> {
        let mut g = Graph {
            version: FORMAT_VERSION,
            next_node: self.next_node,
            next_group: self.next_group,
            nodes: Vec::new(),
            links: Vec::new(),
            groups: Vec::new(),
            frames: Vec::new(),
            next_frame: self.next_frame,
        };
        for f in self.frames {
            if f.rect.iter().all(|x| x.is_finite()) {
                g.put_frame_raw(f);
            }
        }
        for n in self.nodes {
            g.insert_node(n)?;
        }
        for l in self.links {
            g.check_link(&l)?;
            g.insert_link(l);
        }
        // Parents before children.
        let mut pending = self.groups;
        while !pending.is_empty() {
            let before = pending.len();
            let mut rest = Vec::new();
            for grp in pending {
                if grp.parent.is_none_or(|p| g.group(p).is_some()) {
                    g.put_group_raw(grp);
                } else {
                    rest.push(grp);
                }
            }
            if rest.len() == before {
                return Err(GraphError::BadGroup(
                    rest[0].id.0,
                    "parent group missing or groups contain each other".into(),
                ));
            }
            pending = rest;
        }
        // Check groups once all are in (published controls may point into
        // sub-groups).
        for grp in g.groups.clone() {
            g.check_group(&grp)?;
        }
        Ok(g)
    }
}
