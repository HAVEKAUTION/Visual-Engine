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
];
