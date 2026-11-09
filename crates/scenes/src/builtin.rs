//! The built-in scenes and effects, embedded in the binary. Effects are the
//! `fx_*.wgsl` files (`"kind": "effect"`).

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
    Builtin {
        file_name: "fx_merge.wgsl",
        source: include_str!("../shaders/fx_merge.wgsl"),
    },
    Builtin {
        file_name: "fx_glow.wgsl",
        source: include_str!("../shaders/fx_glow.wgsl"),
    },
    Builtin {
        file_name: "fx_trails.wgsl",
        source: include_str!("../shaders/fx_trails.wgsl"),
    },
    Builtin {
        file_name: "fx_mirror.wgsl",
        source: include_str!("../shaders/fx_mirror.wgsl"),
    },
    Builtin {
        file_name: "fx_glitch.wgsl",
        source: include_str!("../shaders/fx_glitch.wgsl"),
    },
    Builtin {
        file_name: "fx_colour.wgsl",
        source: include_str!("../shaders/fx_colour.wgsl"),
    },
    Builtin {
        file_name: "fx_transform.wgsl",
        source: include_str!("../shaders/fx_transform.wgsl"),
    },
    Builtin {
        file_name: "fx_pixelate.wgsl",
        source: include_str!("../shaders/fx_pixelate.wgsl"),
    },
];
