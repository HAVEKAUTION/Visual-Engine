//! Every change to a graph is a [`Command`]. Applying one returns its exact
//! opposite, so undo/redo is just "apply what came back" ([`History`]).

use serde::{Deserialize, Serialize};
use signals::modulation::ModSlot;

use crate::model::{
    Endpoint, Graph, GraphError, Group, GroupId, Link, Node, NodeId, OutputRef, Resolution, WireMod,
};
use crate::types::ParamValue;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Add a node made with [`Graph::new_node`].
    AddNode(Box<Node>),
    /// Remove a node with its wires (and take it out of its group).
    RemoveNode(NodeId),
    /// Add a wire. A wire into an input that already has one replaces it.
    Connect(Link),
    /// Remove the wire with these ends.
    Disconnect(Link),
    SetParam {
        node: NodeId,
        param: String,
        value: ParamValue,
    },
    /// Replace a parameter's modulations (mapping rule slots).
    SetMods {
        node: NodeId,
        param: String,
        mods: Vec<ModSlot>,
    },
    /// Change how a Signal wire drives its parameter.
    SetWireMod {
        from: OutputRef,
        to: Endpoint,
        modulation: WireMod,
    },
    SetLabel {
        node: NodeId,
        label: String,
    },
    Move {
        node: NodeId,
        pos: [f32; 2],
    },
    SetBypass {
        node: NodeId,
        bypass: bool,
    },
    SetResolution {
        node: NodeId,
        resolution: Resolution,
    },
    /// Add a group, or replace the group with the same id (members,
    /// published controls, label).
    PutGroup(Group),
    /// Remove a group (ungroup). Its sub-groups move up to its parent; the
    /// nodes stay in the graph.
    RemoveGroup(GroupId),
    /// Several commands as one step. If one fails, the others are undone.
    Batch(Vec<Command>),
}

impl Graph {
    /// Apply `cmd`. Returns the command that undoes it; on error the graph is
    /// unchanged.
    pub fn apply(&mut self, cmd: Command) -> Result<Command, GraphError> {
        Ok(match cmd {
            Command::AddNode(node) => {
                let id = node.id;
                self.insert_node(*node)?;
                Command::RemoveNode(id)
            }
            Command::RemoveNode(id) => {
                self.node(id).ok_or(GraphError::NoSuchNode(id))?;
                let mut undo = Vec::new();
                // Groups that mention the node, as they were.
                let touched: Vec<Group> = self
                    .groups()
                    .iter()
                    .filter(|g| g.nodes.contains(&id) || g.published.iter().any(|p| p.node == id))
                    .cloned()
                    .collect();
                for g in self.groups_mut() {
                    g.nodes.retain(|n| *n != id);
                    g.published.retain(|p| p.node != id);
                }
                let (gone, kept): (Vec<Link>, Vec<Link>) = std::mem::take(self.links_mut())
                    .into_iter()
                    .partition(|l| l.from.node == id || l.to.node() == id);
                *self.links_mut() = kept;
                let node = self.take_node(id)?;
                undo.push(Command::AddNode(Box::new(node)));
                undo.extend(touched.into_iter().map(Command::PutGroup));
                undo.extend(gone.into_iter().map(Command::Connect));
                Command::Batch(undo)
            }
            Command::Connect(mut link) => {
                self.check_link(&link)?;
                if matches!(link.to, Endpoint::Param { .. }) {
                    link.modulation.get_or_insert_with(WireMod::default);
                } else {
                    link.modulation = None;
                }
                let replaced = match &link.to {
                    Endpoint::Input { node, port } => self.input_link(*node, port).cloned(),
                    Endpoint::Param { .. } => None,
                };
                if let Some(old) = &replaced {
                    self.take_link(old)?;
                }
                let undo_disconnect = Command::Disconnect(link.clone());
                self.insert_link(link);
                match replaced {
                    Some(old) => Command::Batch(vec![undo_disconnect, Command::Connect(old)]),
                    None => undo_disconnect,
                }
            }
            Command::Disconnect(link) => Command::Connect(self.take_link(&link)?),
            Command::SetParam { node, param, value } => {
                let p = self.node_mut(node)?.param_mut(&param).ok_or_else(|| {
                    GraphError::NoSuchPort {
                        node,
                        what: "parameter",
                        name: param.clone(),
                    }
                })?;
                let value = p
                    .def
                    .sanitise(value)
                    .ok_or_else(|| GraphError::WrongValue(param.clone()))?;
                let old = std::mem::replace(&mut p.value, value);
                Command::SetParam {
                    node,
                    param,
                    value: old,
                }
            }
            Command::SetMods { node, param, mods } => {
                let p = self.node_mut(node)?.param_mut(&param).ok_or_else(|| {
                    GraphError::NoSuchPort {
                        node,
                        what: "parameter",
                        name: param.clone(),
                    }
                })?;
                if !p.def.is_modulatable() && !mods.is_empty() {
                    return Err(GraphError::NotModulatable(param));
                }
                let old = std::mem::replace(&mut p.mods, mods);
                Command::SetMods {
                    node,
                    param,
                    mods: old,
                }
            }
            Command::SetWireMod {
                from,
                to,
                modulation,
            } => {
                let probe = Link::new(from.clone(), to.clone());
                let link = self
                    .links_mut()
                    .iter_mut()
                    .find(|l| l.same_ends(&probe) && l.modulation.is_some())
                    .ok_or(GraphError::NoSuchLink)?;
                let old = link.modulation.replace(modulation).unwrap_or_default();
                Command::SetWireMod {
                    from,
                    to,
                    modulation: old,
                }
            }
            Command::SetLabel { node, label } => {
                let old = std::mem::replace(&mut self.node_mut(node)?.label, label);
                Command::SetLabel { node, label: old }
            }
            Command::Move { node, pos } => {
                let old = std::mem::replace(&mut self.node_mut(node)?.pos, pos);
                Command::Move { node, pos: old }
            }
            Command::SetBypass { node, bypass } => {
                let old = std::mem::replace(&mut self.node_mut(node)?.bypass, bypass);
                Command::SetBypass { node, bypass: old }
            }
            Command::SetResolution { node, resolution } => {
                let old = std::mem::replace(&mut self.node_mut(node)?.resolution, resolution);
                Command::SetResolution {
                    node,
                    resolution: old,
                }
            }
            Command::PutGroup(group) => {
                self.check_group(&group)?;
                let id = group.id;
                match self.put_group_raw(group) {
                    Some(old) => Command::PutGroup(old),
                    None => Command::RemoveGroup(id),
                }
            }
            Command::RemoveGroup(id) => {
                let i = self
                    .groups()
                    .iter()
                    .position(|g| g.id == id)
                    .ok_or(GraphError::NoSuchGroup(id.0))?;
                let old = self.groups_mut().remove(i);
                // Undo: put the group back without its published controls,
                // move its sub-groups back in, then restore it fully (the
                // controls may point into those sub-groups).
                let bare = Group {
                    published: Vec::new(),
                    ..old.clone()
                };
                let mut undo = vec![Command::PutGroup(bare)];
                for g in self.groups_mut() {
                    if g.parent == Some(id) {
                        undo.push(Command::PutGroup(g.clone()));
                        g.parent = old.parent;
                    }
                }
                undo.push(Command::PutGroup(old));
                Command::Batch(undo)
            }
            Command::Batch(cmds) => {
                let mut undo = Vec::with_capacity(cmds.len());
                for c in cmds {
                    match self.apply(c) {
                        Ok(u) => undo.push(u),
                        Err(e) => {
                            for u in undo.into_iter().rev() {
                                let _ = self.apply(u);
                            }
                            return Err(e);
                        }
                    }
                }
                undo.reverse();
                Command::Batch(undo)
            }
        })
    }
}

/// Undo / redo stacks for one graph.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<(Command, Option<String>)>,
    redo: Vec<Command>,
}

/// Maximum undo steps kept.
pub const UNDO_LIMIT: usize = 500;

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply `cmd` as a new undo step.
    pub fn exec(&mut self, graph: &mut Graph, cmd: Command) -> Result<(), GraphError> {
        self.exec_merge(graph, cmd, None)
    }

    /// Apply `cmd`; if the last step had the same `merge` key (e.g. one slider
    /// drag), fold it into that step so one undo reverts the whole gesture.
    pub fn exec_merge(
        &mut self,
        graph: &mut Graph,
        cmd: Command,
        merge: Option<&str>,
    ) -> Result<(), GraphError> {
        let undo = graph.apply(cmd)?;
        self.redo.clear();
        if let (Some(key), Some((_, Some(last)))) = (merge, self.undo.last())
            && last == key
        {
            return Ok(()); // keep the older undo, it restores the start state
        }
        self.undo.push((undo, merge.map(str::to_string)));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        Ok(())
    }

    /// Stop merging into the last step (call when a drag ends).
    pub fn seal(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.1 = None;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self, graph: &mut Graph) -> Result<bool, GraphError> {
        let Some((cmd, _)) = self.undo.pop() else {
            return Ok(false);
        };
        let redo = graph.apply(cmd)?;
        self.redo.push(redo);
        Ok(true)
    }

    pub fn redo(&mut self, graph: &mut Graph) -> Result<bool, GraphError> {
        let Some(cmd) = self.redo.pop() else {
            return Ok(false);
        };
        let undo = graph.apply(cmd)?;
        self.undo.push((undo, None));
        Ok(true)
    }
}
