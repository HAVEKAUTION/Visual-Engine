//! The node graph: the core of the app (see `DIRECTION_UPDATE_v2.md`).
//!
//! This crate is **only the data model**: nodes with typed plugs, wires,
//! groups, saving/loading and undo/redo. It knows nothing about the GPU or the
//! editor, so the evaluation (render side) and the editor UI can each be
//! redesigned without touching it.
//!
//! - [`types`]: port types (Image, Mask, Signal, Trigger), parameters, node specs.
//! - [`model`]: [`Graph`], [`Node`], [`Link`], [`Group`] and the connection rules.
//! - [`command`]: every edit as a reversible [`Command`], plus [`History`].

pub mod command;
pub mod model;
pub mod types;

pub use command::{Command, History};
pub use model::{
    Endpoint, Graph, GraphError, Group, GroupId, Link, Node, NodeId, OutputRef,
    PublishedParam, Resolution, WireMod,
};
pub use types::{NodeSpec, Param, ParamDef, ParamValue, PortDef, PortType};

#[cfg(test)]
mod tests;
