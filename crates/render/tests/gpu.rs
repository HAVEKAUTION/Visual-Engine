//! Headless GPU tests. They skip (pass) if this machine has no usable GPU.

use render::Renderer;
use scenes::{ParamState, SignalState};
use signals::SignalFrame;

const SIZE: (u32, u32) = (480, 270);

/// Starting several GPU devices at the same moment can hang the driver (seen
/// on Windows when the tests run in parallel), so tests start them in turn.
static GPU_START: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let _one_at_a_time = GPU_START.lock().unwrap_or_else(|e| e.into_inner());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// Render `frames` frames of `source` with a loud, kicking fake signal and
/// return the last image (RGBA8).
fn render_scene(r: &mut Renderer, source: &str, name: &str, frames: u32) -> Vec<u8> {
    let compiled = scenes::compile(source, name).unwrap_or_else(|e| panic!("{e}"));
    let pipeline = r.create_scene(&compiled).unwrap_or_else(|e| panic!("{e}"));
    let mut params = ParamState::new(&compiled.meta);
    let mut sig = SignalState::new();
    let mut f = SignalFrame::default();
    for (i, b) in f.bands.iter_mut().enumerate() {
        b.level = 1.0 + 0.1 * i as f32;
        b.fast = b.level;
        b.slow = 1.0;
    }
    f.loudness.fast = 1.0;
    f.loudness.slow = 1.0;
    f.mel = [1.0; 32];
    f.centroid_hue = 0.4;
    for frame in 0..frames {
        f.hop_index += 2;
        if frame % 10 == 0 {
            f.rhythm.kick.fire(frame as f64, 2.0);
            f.rhythm.snare.fire(frame as f64, 2.0);
        }
        sig.update(&f, 1.0 / 60.0, [SIZE.0 as f32, SIZE.1 as f32]);
        params.advance(&sig, 1.0 / 60.0);
        sig.set_palette(&params.current_palette());
        r.begin_frame(SIZE);
        r.render(0, &pipeline, sig.as_bytes(), params.block());
        r.present(render::Master::default());
    }
    let (w, h, px) = r.read_display().expect("readback");
    assert_eq!((w, h), SIZE);
    px
}

fn brightness(px: &[u8]) -> f64 {
    px.chunks(4)
        .map(|p| p[0] as f64 + p[1] as f64 + p[2] as f64)
        .sum::<f64>()
        / (px.len() / 4) as f64
}

#[test]
fn built_in_scenes_draw_something() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU: skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    for b in scenes::builtin::BUILTIN {
        // Effects and keys need a picture plugged in; the graph tests
        // cover them (and the masks).
        if b.file_name.starts_with("fx_") || b.file_name.starts_with("mask_") {
            continue;
        }
        r.clear_feedback(0);
        let px = render_scene(&mut r, b.source, b.file_name, 30);
        if let Ok(dir) = std::env::var("VE_DUMP_DIR") {
            write_bmp(
                &std::path::Path::new(&dir).join(format!("{}.bmp", b.file_name)),
                SIZE.0,
                SIZE.1,
                &px,
            );
        }
        let lum = brightness(&px);
        assert!(lum > 5.0, "{} is black (mean {lum})", b.file_name);
        // Not one flat colour.
        let first = &px[..4];
        assert!(px.chunks(4).any(|p| p != first), "{} is flat", b.file_name);
    }
}

#[test]
fn only_feedback_scenes_keep_two_pictures() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU: skipped");
        return;
    };
    let flat = "/*{\"name\": \"flat\"}*/
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.5, 0.5, 0.5, 1.0);
}
";
    let echo = "/*{\"name\": \"echo\"}*/
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return prev(in.uv) * 0.5 + vec4<f32>(0.25, 0.0, 0.0, 1.0);
}
";
    let picture = SIZE.0 as u64 * SIZE.1 as u64 * 8;
    let mut r = Renderer::new(&device, &queue);
    render_scene(&mut r, flat, "flat.wgsl", 3);
    let display = SIZE.0 as u64 * SIZE.1 as u64 * 4;
    assert_eq!(
        r.gpu_bytes(),
        display + picture,
        "a plain scene keeps one picture"
    );
    r.clear_feedback(0);
    let px = render_scene(&mut r, echo, "echo.wgsl", 12);
    assert_eq!(r.gpu_bytes(), display + 2 * picture, "feedback keeps two");
    // Feedback still adds up: 0.25 · (1 + 0.5 + 0.25 + …) → 0.5 (shown as-is: 128;
    // without feedback it would stay at 0.25 = 64).
    assert!((px[0] as i32 - 128).abs() <= 4, "feedback red {}", px[0]);
}

#[test]
fn persistent_buffer_accumulates_and_feedback_works() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU: skipped");
        return;
    };
    // Buffer A gains 0.1 every frame; the output shows A. After 5 frames: 0.5.
    let src = r#"/*{"name": "acc", "buffers": [{"name": "A", "persistent": true}],
        "passes": [{"entry": "fs_acc", "target": "A"}, {"entry": "fs_main"}]}*/
@fragment
fn fs_acc(in: VsOut) -> @location(0) vec4<f32> {
    return sample_a(in.uv) + vec4<f32>(0.1, 0.0, 0.0, 0.0);
}
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Green = last frame's green + 0.2 (feedback through prev_frame).
    return vec4<f32>(sample_a(in.uv).r, prev(in.uv).g + 0.2, 0.0, 1.0);
}
"#;
    let mut r = Renderer::new(&device, &queue);
    let px = render_scene(&mut r, src, "acc.wgsl", 5);
    // Display is sRGB-encoded, and the present pass shows scene values as-is:
    // 0.5 → 128, green capped at 1.0 → 255.
    let (red, green) = (px[0] as i32, px[1] as i32);
    assert!((red - 128).abs() <= 2, "red {red}");
    assert_eq!(green, 255);

    // Same with a non-persistent buffer: A is black at the start of every frame.
    let once = src.replace("\"persistent\": true", "\"persistent\": false");
    r.clear_feedback(0);
    let px = render_scene(&mut r, &once, "once.wgsl", 5);
    let red = px[0] as i32;
    assert!((red - 26).abs() <= 2, "red {red}"); // 0.1 → 26
}

/// Debug aid: `VE_DUMP_DIR=folder cargo test -p render` saves each scene as a BMP.
fn write_bmp(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let mut out = Vec::new();
    let size = 54 + w * h * 4;
    out.extend_from_slice(b"BM");
    for v in [size, 0, 54, 40] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    for v in [0u32, w * h * 4, 2835, 2835, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for p in rgba.chunks(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    std::fs::write(path, out).unwrap();
}

#[test]
fn decks_crossfade_and_master_controls() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU: skipped");
        return;
    };
    let solid = |r: f32, b: f32| {
        format!(
            "@fragment\nfn fs_main(in: VsOut) -> @location(0) vec4<f32> {{ return vec4<f32>({r:.1}, 0.0, {b:.1}, 1.0); }}\n"
        )
    };
    let mut r = Renderer::new(&device, &queue);
    let red = r
        .create_scene(&scenes::compile(&solid(1.0, 0.0), "red.wgsl").unwrap())
        .unwrap();
    let blue = r
        .create_scene(&scenes::compile(&solid(0.0, 1.0), "blue.wgsl").unwrap())
        .unwrap();
    let mut sig = SignalState::new();
    sig.update(&SignalFrame::default(), 0.016, [8.0, 8.0]);
    let pixel = |r: &mut Renderer, master: render::Master| {
        r.begin_frame((8, 8));
        r.render(0, &red, sig.as_bytes(), &[0.0; 4]);
        r.render(1, &blue, sig.as_bytes(), &[0.0; 4]);
        r.present(master);
        let (_, _, px) = r.read_display().unwrap();
        [px[0] as i32, px[1] as i32, px[2] as i32]
    };
    let m = render::Master::default();
    assert_eq!(pixel(&mut r, m), [255, 0, 0]);
    assert_eq!(
        pixel(&mut r, render::Master { mix_b: 1.0, ..m }),
        [0, 0, 255]
    );
    let half = pixel(&mut r, render::Master { mix_b: 0.5, ..m });
    assert!(
        (half[0] - 128).abs() <= 2 && (half[2] - 128).abs() <= 2,
        "{half:?}"
    );
    assert_eq!(pixel(&mut r, render::Master { gain: 0.0, ..m }), [0, 0, 0]);
    assert_eq!(
        pixel(
            &mut r,
            render::Master {
                gain: 0.0,
                flash: 1.0,
                ..m
            }
        ),
        [255, 255, 255]
    );
}
