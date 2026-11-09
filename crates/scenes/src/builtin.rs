//! The built-in scenes, embedded in the binary.

pub struct Builtin {
    pub file_name: &'static str,
    pub source: &'static str,
}

pub const BUILTIN: &[Builtin] = &[
    Builtin {
        file_name: "kaleido_bloom.wgsl",
        source: include_str!("../shaders/kaleido_bloom.wgsl"),
    },
    Builtin {
        file_name: "radial_spectrum.wgsl",
        source: include_str!("../shaders/radial_spectrum.wgsl"),
    },
    Builtin {
        file_name: "crt_glitch.wgsl",
        source: include_str!("../shaders/crt_glitch.wgsl"),
    },
    Builtin {
        file_name: "canopy_breaks.wgsl",
        source: include_str!("../shaders/canopy_breaks.wgsl"),
    },
    Builtin {
        file_name: "warehouse_tunnel.wgsl",
        source: include_str!("../shaders/warehouse_tunnel.wgsl"),
    },
    Builtin {
        file_name: "iron_furnace.wgsl",
        source: include_str!("../shaders/iron_furnace.wgsl"),
    },
    Builtin {
        file_name: "midnight_window.wgsl",
        source: include_str!("../shaders/midnight_window.wgsl"),
    },
    Builtin {
        file_name: "velvet_mirrorball.wgsl",
        source: include_str!("../shaders/velvet_mirrorball.wgsl"),
    },
    Builtin {
        file_name: "aurora_drift.wgsl",
        source: include_str!("../shaders/aurora_drift.wgsl"),
    },
];
