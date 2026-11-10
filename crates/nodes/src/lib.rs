//! Built-in node types and the graph evaluator.
//!
//! - [`registry`]: what each node type looks like (plugs, parameters).
//! - [`eval`]: runs a graph each frame on the GPU.
//! - [`demo`]: a small hard-coded test graph (temporary, until the node
//!   editor arrives).

pub mod build;
pub mod demo;
pub mod eval;
pub mod layers;
pub mod registry;
pub mod switch;
pub mod templates;

pub use eval::{Evaluator, FrameInput, effective_value};
