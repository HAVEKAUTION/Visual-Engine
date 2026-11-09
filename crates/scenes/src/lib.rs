//! Scenes: WGSL shaders with an ISF-style JSON metadata header.
//!
//! - [`meta`]: the header format (name, inputs, clocks, buffers, passes).
//! - [`uniforms`]: the `audio` signal block every scene receives.
//! - [`params`]: a scene's own `params` block (inputs + audio clocks).
//! - [`compile`]: header + prelude + naga validation → WGSL for the renderer.
//! - [`library`]: built-in scenes plus a watched folder (hot reload).
//! - [`palette`]: colour palettes.
//! - [`preset`]: saved scene settings and modulations.

pub mod builtin;
pub mod compile;
pub mod library;
pub mod meta;
pub mod palette;
pub mod params;
pub mod preset;
pub mod uniforms;

pub use compile::{CompiledScene, VERTEX_ENTRY, compile};
pub use library::SceneLibrary;
pub use meta::{InputKind, SceneMeta, Target};
pub use params::ParamState;
pub use uniforms::SignalState;

/// The folder holding the example presets in this checkout, if it exists.
pub fn source_preset_dir() -> Option<std::path::PathBuf> {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
    d.is_dir().then_some(d)
}

/// The folder holding the built-in scene sources in this checkout, if it
/// exists (it does when running from the source tree with `cargo run`).
pub fn source_scene_dir() -> Option<std::path::PathBuf> {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
    d.is_dir().then_some(d)
}
