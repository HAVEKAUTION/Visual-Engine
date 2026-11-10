//! Turn a scene file into checked WGSL: parse the header, append the prelude
//! and generated structs, and validate everything with naga so errors come
//! back as readable text with line numbers from the user's file.

use crate::meta::SceneMeta;
use crate::params::ParamLayout;
use crate::uniforms;

const PRELUDE: &str = include_str!("prelude.wgsl");

/// Name of the shared vertex shader entry point.
pub const VERTEX_ENTRY: &str = "vs_fullscreen";

/// A scene that parsed and validated.
#[derive(Debug, Clone)]
pub struct CompiledScene {
    pub meta: SceneMeta,
    /// Complete WGSL (user code + prelude), ready for wgpu.
    pub wgsl: String,
    /// The scene reads its own previous frame (`prev(...)` / `prev_frame`),
    /// so it needs two output textures; others need only one.
    pub reads_prev: bool,
}

/// The full WGSL for a scene: user source first, then the generated code.
pub fn full_source(user: &str, meta: &SceneMeta) -> String {
    let mut s = String::with_capacity(user.len() + PRELUDE.len() + 4096);
    s += user;
    if !user.ends_with('\n') {
        s.push('\n');
    }
    s += PRELUDE;
    s += "\n";
    s += &uniforms::wgsl_struct();
    s += "\n";
    s += &ParamLayout::wgsl_struct(meta);
    // Named readers for an effect's picture inputs: `img_base(uv)`, ...
    for (i, img) in meta.images.iter().enumerate() {
        let tex = ["image_a", "image_b", "image_c", "image_d"][i.min(3)];
        s += &format!(
            "fn img_{}(uv: vec2<f32>) -> vec4<f32> {{ return {tex}(uv); }}
",
            img.name
        );
    }
    s
}

/// naga draws its error boxes with line-drawing characters the UI font
/// doesn't have; swap them for ASCII.
fn plain(msg: String) -> String {
    msg.replace(['┌', '└'], "+")
        .replace('─', "-")
        .replace('│', "|")
        .replace('·', ".")
}

/// Parse and validate. `file_name` is used in error messages.
pub fn compile(source: &str, file_name: &str) -> Result<CompiledScene, String> {
    let stem = file_name.trim_end_matches(".wgsl");
    let meta = SceneMeta::parse(source, stem)?;
    let wgsl = full_source(source, &meta);

    let module = naga::front::wgsl::parse_str(&wgsl)
        .map_err(|e| plain(e.emit_to_string_with_path(&wgsl, file_name)))?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .map_err(|e| plain(e.emit_to_string_with_path(&wgsl, file_name)))?;

    for pass in &meta.passes {
        let ok = module
            .entry_points
            .iter()
            .any(|e| e.name == pass.entry && e.stage == naga::ShaderStage::Fragment);
        if !ok {
            return Err(format!(
                "{file_name}: no fragment shader called `{}`. Add\n\n@fragment\nfn {}(in: VsOut) -> @location(0) vec4<f32> {{ ... }}",
                pass.entry, pass.entry
            ));
        }
    }
    // Look at the user's code only (the prelude always mentions prev_frame).
    let reads_prev = source.contains("prev(") || source.contains("prev_frame");
    Ok(CompiledScene {
        meta,
        wgsl,
        reads_prev,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"/*{"name": "m", "inputs": [{"name": "k", "type": "float"}, {"name": "c", "type": "color"}],
        "clocks": [{"name": "spin", "signal": "bass_slow"}]}*/
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let m = mel_at(in.uv.x) + audio.kick_age + params.k + params.spin;
    return vec4<f32>(params.c.rgb * m, 1.0) + prev(in.uv) + sample_a(in.uv) + sample_b(in.uv);
}
"#;

    #[test]
    fn compiles_with_prelude() {
        let c = compile(MINIMAL, "m.wgsl").unwrap();
        assert!(c.wgsl.contains("struct Signals"));
    }

    #[test]
    fn errors_point_at_user_lines() {
        let src = "/*{\"name\": \"x\"}*/\n@fragment\nfn fs_main(in: VsOut) -> @location(0) vec4<f32> {\n    return vec4<f32>(oops, 1.0);\n}\n";
        let e = compile(src, "x.wgsl").unwrap_err();
        assert!(e.contains("oops"), "{e}");
        assert!(e.contains("x.wgsl:4:"), "{e}");
    }

    #[test]
    fn missing_entry_point_is_reported() {
        let src =
            "@fragment\nfn other(in: VsOut) -> @location(0) vec4<f32> { return vec4<f32>(1.0); }\n";
        let e = compile(src, "y.wgsl").unwrap_err();
        assert!(e.contains("fs_main"), "{e}");
    }

    #[test]
    fn effects_get_named_image_readers() {
        let src = r#"/*{"kind": "effect", "images": [{"name": "base"}, {"name": "top"}]}*/
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return img_base(in.uv) + img_top(in.uv) * luma(image_b(in.uv).rgb);
}
"#;
        compile(src, "fx.wgsl").unwrap();
    }

    #[test]
    fn built_in_scenes_compile() {
        for b in crate::builtin::BUILTIN {
            if let Err(e) = compile(b.source, b.file_name) {
                panic!("{}:\n{e}", b.file_name);
            }
        }
    }
}
