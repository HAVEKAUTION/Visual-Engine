//! Headless GPU tests for the graph evaluator. They skip (pass) if this
//! machine has no usable GPU.

use graph::{Command, Endpoint, Graph, Link, NodeId, OutputRef};
use nodes::registry::{self, IMAGE_OUT, PROGRAM_OUT};
use nodes::{Evaluator, FrameInput};
use render::{Master, Renderer, Show};
use scenes::{SceneLibrary, SignalState};
use signals::SignalFrame;

const SIZE: (u32, u32) = (320, 180);

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// Run `graph` for `frames` frames with a loud, kicking fake signal and
/// return the last picture (RGBA8) plus what was shown.
fn run(graph: &Graph, out: NodeId, frames: u32) -> Option<(Vec<u8>, Show, Evaluator)> {
    let (device, queue) = gpu()?;
    let lib = SceneLibrary::new(None);
    let mut r = Renderer::new(&device, &queue);
    let mut ev = Evaluator::new(1000);
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
    let mut shown = Show::Black;
    for frame in 0..frames {
        f.hop_index += 2;
        if frame % 10 == 0 {
            f.rhythm.kick.fire(frame as f64, 2.0);
        }
        sig.update(&f, 1.0 / 60.0, [SIZE.0 as f32, SIZE.1 as f32]);
        r.begin_frame(SIZE);
        shown = ev.run(
            graph,
            &lib,
            &mut r,
            FrameInput {
                signals: &mut sig,
                dt: 1.0 / 60.0,
                controls: &[0.0; 8],
                intensity: 1.0,
                output_size: SIZE,
                proxy: 1.0,
            },
            &[out],
            &[],
        );
        r.present_from([shown, Show::Black], Master::default());
        r.end_frame();
    }
    let (_, _, px) = r.read_display()?;
    Some((px, shown, ev))
}

fn brightness(px: &[u8]) -> f64 {
    px.chunks(4)
        .map(|p| p[0] as f64 + p[1] as f64 + p[2] as f64)
        .sum::<f64>()
        / (px.len() / 4) as f64
}

fn add(g: &mut Graph, kind: &str) -> NodeId {
    let spec = registry::spec_for(kind, &SceneLibrary::new(None)).unwrap();
    let n = g.new_node(&spec);
    let id = n.id;
    g.apply(Command::AddNode(Box::new(n))).unwrap();
    id
}

fn wire(g: &mut Graph, a: NodeId, b: NodeId, port: &str) {
    g.apply(Command::Connect(Link::new(
        OutputRef::new(a, IMAGE_OUT),
        Endpoint::input(b, port),
    )))
    .unwrap();
}

#[test]
fn demo_graph_draws_and_is_deterministic() {
    let lib = SceneLibrary::new(None);
    let (g, out) = nodes::demo::build(&lib).unwrap();
    let Some((a, shown, ev)) = run(&g, out, 40) else {
        eprintln!("no GPU: skipped");
        return;
    };
    assert!(ev.errors.is_empty(), "{:?}", ev.errors);
    assert!(matches!(shown, Show::Node(_)));
    assert!(brightness(&a) > 5.0, "demo graph is black");
    // Same graph, same signals, same steps: the same picture, exactly.
    let (b, _, _) = run(&g, out, 40).unwrap();
    assert!(a == b, "two runs of the same graph differ");
    // And after a save/load round trip.
    let g2 = Graph::from_ron(&g.to_ron()).unwrap();
    let (c, _, _) = run(&g2, out, 40).unwrap();
    assert!(a == c, "saved and loaded graph renders differently");
}

#[test]
fn bypass_and_broken_nodes_pass_the_picture_through() {
    let tunnel = registry::shader_kind("warehouse_tunnel.wgsl");
    // Reference: tunnel straight into the output.
    let mut direct = Graph::new();
    let t = add(&mut direct, &tunnel);
    let o = add(&mut direct, PROGRAM_OUT);
    wire(&mut direct, t, o, "image");
    let Some((reference, _, _)) = run(&direct, o, 20) else {
        eprintln!("no GPU: skipped");
        return;
    };
    assert!(brightness(&reference) > 5.0);

    // Tunnel → bypassed Transform → output: identical.
    let mut g = Graph::new();
    let t = add(&mut g, &tunnel);
    let fx = add(&mut g, &registry::shader_kind("fx_transform.wgsl"));
    let o = add(&mut g, PROGRAM_OUT);
    wire(&mut g, t, fx, "input");
    wire(&mut g, fx, o, "image");
    g.apply(Command::SetBypass {
        node: fx,
        bypass: true,
    })
    .unwrap();
    let (px, _, _) = run(&g, o, 20).unwrap();
    assert!(px == reference, "bypass changed the picture");

    // A node whose shader can't be found: error badge, picture passes through.
    let mut g = Graph::new();
    let t = add(&mut g, &tunnel);
    let mut spec = registry::spec_for(
        &registry::shader_kind("fx_transform.wgsl"),
        &SceneLibrary::new(None),
    )
    .unwrap();
    spec.kind = registry::shader_kind("missing.wgsl");
    let n = g.new_node(&spec);
    let fx = n.id;
    g.apply(Command::AddNode(Box::new(n))).unwrap();
    let o = add(&mut g, PROGRAM_OUT);
    wire(&mut g, t, fx, "input");
    wire(&mut g, fx, o, "image");
    let (px, _, ev) = run(&g, o, 20).unwrap();
    assert!(ev.errors.contains_key(&fx));
    assert!(
        px == reference,
        "broken node did not pass its input through"
    );
}

#[test]
fn nothing_plugged_in_shows_black() {
    let mut g = Graph::new();
    let o = add(&mut g, PROGRAM_OUT);
    let Some((px, shown, _)) = run(&g, o, 2) else {
        eprintln!("no GPU: skipped");
        return;
    };
    assert!(matches!(shown, Show::Black));
    assert!(brightness(&px) < 1.0);
}

/// With GPU timestamps, every drawn node gets a time; without, there are none.
#[test]
fn gpu_cost_per_node_is_measured_when_supported() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        return;
    };
    let timed = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    eprintln!("GPU timestamps supported: {timed}");
    let desc = wgpu::DeviceDescriptor {
        required_features: if timed {
            wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        },
        ..Default::default()
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&desc)) else {
        return;
    };
    let lib = SceneLibrary::new(None);
    let (g, out) = nodes::demo::build(&lib).unwrap();
    let mut r = Renderer::new(&device, &queue);
    assert_eq!(r.can_time_nodes(), timed);
    let mut ev = Evaluator::new(1000);
    let mut sig = SignalState::new();
    let f = SignalFrame::default();
    for _ in 0..12 {
        sig.update(&f, 1.0 / 60.0, [SIZE.0 as f32, SIZE.1 as f32]);
        r.begin_frame(SIZE);
        let shown = ev.run(
            &g,
            &lib,
            &mut r,
            FrameInput {
                signals: &mut sig,
                dt: 1.0 / 60.0,
                controls: &[0.0; 8],
                intensity: 1.0,
                output_size: SIZE,
                proxy: 1.0,
            },
            &[out],
            &[],
        );
        r.present_from([shown, Show::Black], Master::default());
        r.end_frame();
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    }
    // One more frame start collects the last readings.
    r.begin_frame(SIZE);
    match r.node_times() {
        None => assert!(!timed),
        Some((times, total)) => {
            // Picture nodes of the starter graph: tunnel, transform, aurora, merge.
            let picture_nodes = g.nodes().iter().filter(|n| n.makes_image()).count();
            assert_eq!(times.len(), picture_nodes, "{times:?}");
            assert!(times.values().all(|ms| *ms >= 0.0 && *ms < 1000.0));
            assert!(total >= 0.0);
        }
    }
}

/// Render `graph` until `done(picture)` or a timeout (media loads in the
/// background). Returns the last picture.
fn render_until(
    graph: &Graph,
    out: NodeId,
    mut done: impl FnMut(&[u8]) -> bool,
) -> Option<(Vec<u8>, Evaluator)> {
    let (device, queue) = gpu()?;
    let lib = SceneLibrary::new(None);
    let mut r = Renderer::new(&device, &queue);
    let mut ev = Evaluator::new(1000);
    let mut sig = SignalState::new();
    let f = SignalFrame::default();
    let start = std::time::Instant::now();
    loop {
        sig.update(&f, 1.0 / 60.0, [SIZE.0 as f32, SIZE.1 as f32]);
        r.begin_frame(SIZE);
        let shown = ev.run(
            graph,
            &lib,
            &mut r,
            FrameInput {
                signals: &mut sig,
                dt: 1.0 / 60.0,
                controls: &[0.0; 8],
                intensity: 1.0,
                output_size: SIZE,
                proxy: 1.0,
            },
            &[out],
            &[],
        );
        r.present_from([shown, Show::Black], Master::default());
        r.end_frame();
        let (_, _, px) = r.read_display()?;
        if done(&px) || start.elapsed().as_secs() > 15 {
            return Some((px, ev));
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

fn set(g: &mut Graph, node: NodeId, param: &str, value: graph::ParamValue) {
    g.apply(Command::SetParam {
        node,
        param: param.into(),
        value,
    })
    .unwrap();
}

#[test]
fn image_node_shows_its_file_and_missing_files_report_an_error() {
    let dir = std::env::temp_dir().join(format!("ve-gpu-img-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("red.png");
    image::RgbaImage::from_pixel(32, 18, image::Rgba([255, 0, 0, 255]))
        .save(&path)
        .unwrap();
    let mut g = Graph::new();
    let img = add(&mut g, registry::IMAGE);
    let out = add(&mut g, PROGRAM_OUT);
    wire(&mut g, img, out, "image");
    set(
        &mut g,
        img,
        registry::FILE_PARAM,
        graph::ParamValue::Text(path.display().to_string()),
    );
    let red = |px: &[u8]| px[0] > 200 && px[1] < 40 && px[2] < 40;
    let Some((px, _)) = render_until(&g, out, |px| red(&px[px.len() / 2..])) else {
        return; // no GPU
    };
    let mid = &px[px.len() / 2..];
    assert!(red(mid), "expected red, got {:?}", &mid[..4]);

    set(
        &mut g,
        img,
        registry::FILE_PARAM,
        graph::ParamValue::Text(dir.join("gone.png").display().to_string()),
    );
    let (_, ev) = render_until(&g, out, |_| true).unwrap();
    assert!(ev.errors.get(&img).is_some_and(|e| e.contains("not found")));
}

#[test]
fn video_node_scrubs_to_the_asked_position() {
    if !media::have_ffmpeg() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("ve-gpu-vid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let clip = dir.join("ramp.mp4");
    // Brightness rises over the clip: frame n is grey 12·n.
    let ok = std::process::Command::new(media::ffmpeg_path())
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x36:r=10:d=2,geq=lum='12*N':cb=128:cr=128",
            "-c:v",
            "libx264",
            "-qp",
            "0",
            "-pix_fmt",
            "yuv444p",
        ])
        .arg(&clip)
        .status()
        .is_ok_and(|s| s.success());
    assert!(ok);
    let mut g = Graph::new();
    let v = add(&mut g, registry::VIDEO);
    let out = add(&mut g, PROGRAM_OUT);
    wire(&mut g, v, out, "image");
    set(
        &mut g,
        v,
        registry::FILE_PARAM,
        graph::ParamValue::Text(clip.display().to_string()),
    );
    let scrub = registry::VIDEO_MODES
        .iter()
        .position(|m| *m == "Scrub")
        .unwrap() as u32;
    set(&mut g, v, "mode", graph::ParamValue::Choice(scrub));
    set(&mut g, v, "fit", graph::ParamValue::Choice(2)); // stretch: fills the frame

    set(&mut g, v, "position", graph::ParamValue::Float(0.0));
    let Some((dark, _)) = render_until(&g, out, |px| px[px.len() / 2 + 3] == 255) else {
        return;
    };
    set(&mut g, v, "position", graph::ParamValue::Float(1.0));
    let (light, ev) = render_until(&g, out, |px| brightness(px) > 300.0).unwrap();
    assert!(
        brightness(&light) > brightness(&dark) + 200.0,
        "start {} end {}",
        brightness(&dark),
        brightness(&light)
    );
    assert!(
        ev.media_info(v)
            .is_some_and(|i| i.contains("frame 20 / 20")),
        "{:?}",
        ev.media_info(v)
    );
    // It feeds the video signals. Playing, the picture keeps changing:
    // that is motion.
    assert_eq!(ev.video_signal_source(), Some(v));
    set(&mut g, v, "mode", graph::ParamValue::Choice(0));
    let mut n = 0;
    let (_, ev) = render_until(&g, out, |_| {
        n += 1;
        n > 90
    })
    .unwrap();
    let [motion, _, _, bright, _] = ev.video_signals(v).unwrap();
    assert!(motion > 0.2, "motion {motion}");
    assert!(bright > 0.2, "brightness {bright}");
}

/// One renderer and evaluator kept across frames (for tests that change
/// the graph between frames).
struct Session {
    r: Renderer,
    ev: Evaluator,
    sig: SignalState,
    lib: SceneLibrary,
}

impl Session {
    fn new() -> Option<Self> {
        let (device, queue) = gpu()?;
        Some(Self {
            r: Renderer::new(&device, &queue),
            ev: Evaluator::new(1000),
            sig: SignalState::new(),
            lib: SceneLibrary::new(None),
        })
    }

    fn frame(&mut self, g: &Graph, out: NodeId) -> Vec<u8> {
        self.sig.update(
            &SignalFrame::default(),
            1.0 / 60.0,
            [SIZE.0 as f32, SIZE.1 as f32],
        );
        self.r.begin_frame(SIZE);
        let shown = self.ev.run(
            g,
            &self.lib,
            &mut self.r,
            FrameInput {
                signals: &mut self.sig,
                dt: 1.0 / 60.0,
                controls: &[0.0; 8],
                intensity: 1.0,
                output_size: SIZE,
                proxy: 1.0,
            },
            &[out],
            &[],
        );
        self.r.present_from([shown, Show::Black], Master::default());
        self.r.end_frame();
        self.r
            .read_display()
            .map(|(_, _, px)| px)
            .unwrap_or_default()
    }

    /// Run until every Image node has loaded its picture.
    fn settle(&mut self, g: &Graph, out: NodeId, images: &[NodeId]) {
        let start = std::time::Instant::now();
        loop {
            self.frame(g, out);
            let ready = images
                .iter()
                .all(|i| self.ev.media_info(*i).is_some_and(|s| s.contains('×')));
            if ready || start.elapsed().as_secs() > 15 {
                // One more so everything downstream has the pictures.
                self.frame(g, out);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

fn pixel(px: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE.0 + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

/// An Image node showing a picture made by `f(x, y)` (stretched to fill).
fn picture_node(g: &mut Graph, name: &str, f: impl Fn(u32, u32) -> [u8; 4]) -> NodeId {
    let dir = std::env::temp_dir().join(format!("ve-gpu-m5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.png"));
    image::RgbaImage::from_fn(64, 36, |x, y| image::Rgba(f(x, y)))
        .save(&path)
        .unwrap();
    let n = add(g, registry::IMAGE);
    set(
        g,
        n,
        registry::FILE_PARAM,
        graph::ParamValue::Text(path.display().to_string()),
    );
    set(g, n, "fit", graph::ParamValue::Choice(2));
    n
}

#[test]
fn every_blend_mode_on_the_gpu_matches_the_reference_maths() {
    let Some(mut s) = Session::new() else { return };
    let (b, f) = ([51u8, 128, 204, 255], [153u8, 77, 230, 255]);
    let mut g = Graph::new();
    let back = picture_node(&mut g, "back", |_, _| b);
    let front = picture_node(&mut g, "front", |_, _| f);
    let mix = add(&mut g, registry::LAYER_MIXER);
    let out = add(&mut g, PROGRAM_OUT);
    wire(&mut g, back, mix, &registry::layer_port(1));
    wire(&mut g, front, mix, &registry::layer_port(2));
    wire(&mut g, mix, out, "image");
    s.settle(&g, out, &[back, front]);
    let unit = |c: [u8; 4]| c.map(|v| v as f32 / 255.0);
    for linear in [false, true] {
        set(&mut g, mix, "linear", graph::ParamValue::Bool(linear));
        for (m, name) in scenes::blend::BLEND_MODES.iter().enumerate() {
            set(
                &mut g,
                mix,
                &registry::layer_param("mode", 2),
                graph::ParamValue::Choice(m as u32),
            );
            let px = s.frame(&g, out);
            let got = pixel(&px, SIZE.0 / 2, SIZE.1 / 2);
            let want = scenes::blend::blend_over(unit(b), unit(f), m, 1.0, linear);
            for c in 0..3 {
                let w = (want[c].clamp(0.0, 1.0) * 255.0).round() as i32;
                assert!(
                    (got[c] as i32 - w).abs() <= 2,
                    "{name} (linear {linear}): got {got:?}, want {:?}",
                    want.map(|v| (v * 255.0).round())
                );
            }
        }
    }
    // Merge uses the same maths: Hard Light at half opacity.
    let merge = add(&mut g, &registry::shader_kind("fx_merge.wgsl"));
    wire(&mut g, back, merge, "base");
    wire(&mut g, front, merge, "top");
    wire(&mut g, merge, out, "image");
    let hard = scenes::blend::mode_index("Hard Light").unwrap();
    set(
        &mut g,
        merge,
        "mode",
        graph::ParamValue::Choice(hard as u32),
    );
    set(&mut g, merge, "opacity", graph::ParamValue::Float(0.5));
    s.frame(&g, out);
    let got = pixel(&s.frame(&g, out), 10, 10);
    let want = scenes::blend::blend_over(unit(b), unit(f), hard, 0.5, false);
    for c in 0..3 {
        assert!(
            (got[c] as f32 - want[c] * 255.0).abs() <= 2.0,
            "merge {got:?} {want:?}"
        );
    }
}

#[test]
fn masks_limit_any_node_and_layers_solo_and_mute() {
    let Some(mut s) = Session::new() else { return };
    let mut g = Graph::new();
    let blue = picture_node(&mut g, "blue", |_, _| [0, 0, 255, 255]);
    let red = picture_node(&mut g, "red", |_, _| [255, 0, 0, 255]);
    // White on the left half, black on the right.
    let half = picture_node(&mut g, "half", |x, _| {
        if x < 32 { [255; 4] } else { [0, 0, 0, 255] }
    });
    let mix = add(&mut g, registry::LAYER_MIXER);
    let out = add(&mut g, PROGRAM_OUT);
    wire(&mut g, blue, mix, &registry::layer_port(1));
    wire(&mut g, red, mix, &registry::layer_port(2));
    wire(&mut g, mix, out, "image");
    s.settle(&g, out, &[blue, red]);
    let left = |px: &[u8]| pixel(px, SIZE.0 / 4, SIZE.1 / 2);
    let right = |px: &[u8]| pixel(px, SIZE.0 * 3 / 4, SIZE.1 / 2);
    // Red (Normal) on top of blue.
    let px = s.frame(&g, out);
    assert!(left(&px)[0] > 240 && left(&px)[2] < 15, "{:?}", left(&px));
    // Masked: the mix shows on the left; its input (blue) on the right.
    wire(&mut g, half, mix, graph::MASK_PORT);
    s.settle(&g, out, &[half]);
    let px = s.frame(&g, out);
    assert!(left(&px)[0] > 240, "left {:?}", left(&px));
    assert!(
        right(&px)[2] > 240 && right(&px)[0] < 15,
        "right {:?}",
        right(&px)
    );
    // A masked source: the picture on the left, nothing (black) on the right.
    let l = g.input_link(mix, graph::MASK_PORT).unwrap().clone();
    g.apply(Command::Disconnect(l)).unwrap();
    wire(&mut g, half, red, graph::MASK_PORT);
    wire(&mut g, red, out, "image");
    let px = s.frame(&g, out);
    assert!(left(&px)[0] > 240, "{:?}", left(&px));
    assert!(right(&px)[..3].iter().all(|v| *v < 15), "{:?}", right(&px));
    // Back to the mixer: mute the top layer → blue; solo it → red alone.
    let l = g.input_link(red, graph::MASK_PORT).unwrap().clone();
    g.apply(Command::Disconnect(l)).unwrap();
    wire(&mut g, mix, out, "image");
    set(
        &mut g,
        mix,
        &registry::layer_param("mute", 2),
        graph::ParamValue::Bool(true),
    );
    let px = s.frame(&g, out);
    assert!(
        left(&px)[2] > 240 && left(&px)[0] < 15,
        "muted {:?}",
        left(&px)
    );
    set(
        &mut g,
        mix,
        &registry::layer_param("solo", 2),
        graph::ParamValue::Bool(true),
    );
    let px = s.frame(&g, out);
    assert!(
        left(&px)[0] > 240 && left(&px)[2] < 15,
        "solo {:?}",
        left(&px)
    );
}

#[test]
fn masks_and_keys_cut_where_they_should() {
    let Some(mut s) = Session::new() else { return };
    let kind = |f: &str| registry::shader_kind(f);
    let centre = |px: &[u8]| pixel(px, SIZE.0 / 2, SIZE.1 / 2);
    let left = |px: &[u8]| pixel(px, SIZE.0 / 8, SIZE.1 / 2);
    let mid = |px: &[u8]| pixel(px, SIZE.0 / 2, SIZE.1 / 2);
    let right = |px: &[u8]| pixel(px, SIZE.0 * 7 / 8, SIZE.1 / 2);
    let mut g = Graph::new();
    let out = add(&mut g, PROGRAM_OUT);

    // Shape Mask: a white circle in the middle, black in the corner.
    let shape = add(&mut g, &kind("mask_shape.wgsl"));
    wire(&mut g, shape, out, "image");
    let px = s.frame(&g, out);
    assert!(centre(&px)[0] > 245, "{:?}", centre(&px));
    assert!(pixel(&px, 2, 2)[0] < 10, "{:?}", pixel(&px, 2, 2));

    // Key Mixer: big circle minus small circle = a ring.
    let small = add(&mut g, &kind("mask_shape.wgsl"));
    set(&mut g, shape, "size", graph::ParamValue::Float(0.8));
    set(&mut g, small, "size", graph::ParamValue::Float(0.4));
    let km = add(&mut g, &kind("mask_key_mixer.wgsl"));
    wire(&mut g, shape, km, "a");
    wire(&mut g, small, km, "b");
    set(&mut g, km, "op", graph::ParamValue::Choice(1));
    wire(&mut g, km, out, "image");
    let px = s.frame(&g, out);
    let ring = pixel(
        &px,
        SIZE.0 / 2,
        SIZE.1 / 2 - (0.6 * SIZE.1 as f32 / 2.0) as u32,
    );
    assert!(centre(&px)[0] < 10, "hole {:?}", centre(&px));
    assert!(ring[0] > 245, "ring {:?}", ring);

    // Gradient Mask: dark on the left, bright on the right.
    let grad = add(&mut g, &kind("mask_gradient.wgsl"));
    wire(&mut g, grad, out, "image");
    let px = s.frame(&g, out);
    assert!(
        left(&px)[0] < 80 && right(&px)[0] > 175,
        "{:?} {:?}",
        left(&px),
        right(&px)
    );

    // Keys, as a layer over blue.
    let blue = picture_node(&mut g, "kblue", |_, _| [0, 0, 255, 255]);
    // Left: green screen. Middle: greenish grey (spill). Right: red.
    let shot = picture_node(&mut g, "kshot", |x, _| match x {
        0..21 => [25, 190, 64, 255],
        21..43 => [120, 170, 120, 255],
        _ => [230, 20, 20, 255],
    });
    let chroma = add(&mut g, &kind("mask_chroma_key.wgsl"));
    wire(&mut g, shot, chroma, "input");
    let mix = add(&mut g, registry::LAYER_MIXER);
    wire(&mut g, blue, mix, &registry::layer_port(1));
    wire(&mut g, chroma, mix, &registry::layer_port(2));
    wire(&mut g, mix, out, "image");
    s.settle(&g, out, &[blue, shot]);
    let px = s.frame(&g, out);
    assert!(
        left(&px)[2] > 240 && left(&px)[1] < 20,
        "keyed {:?}",
        left(&px)
    );
    assert!(right(&px)[0] > 220, "kept {:?}", right(&px));
    let m = mid(&px);
    assert!(m[2] < 200, "the grey stays {:?}", m);
    assert!((m[1] as i32 - m[0] as i32) < 30, "spill removed {:?}", m);

    // Luma Key: keep the bright parts (red and green are bright enough,
    // a dark picture is not).
    let dark = picture_node(&mut g, "kdark", |x, _| {
        if x < 32 {
            [250, 250, 250, 255]
        } else {
            [20, 20, 20, 255]
        }
    });
    let luma = add(&mut g, &kind("mask_luma_key.wgsl"));
    wire(&mut g, dark, luma, "input");
    wire(&mut g, luma, mix, &registry::layer_port(2));
    s.settle(&g, out, &[dark]);
    let px = s.frame(&g, out);
    assert!(left(&px)[0] > 240, "bright kept {:?}", left(&px));
    assert!(
        right(&px)[2] > 240 && right(&px)[0] < 20,
        "dark keyed {:?}",
        right(&px)
    );
}

#[test]
fn motion_mask_lights_up_where_the_picture_changes() {
    let Some(mut s) = Session::new() else { return };
    let mut g = Graph::new();
    let out = add(&mut g, PROGRAM_OUT);
    let black = picture_node(&mut g, "mblack", |_, _| [0, 0, 0, 255]);
    let white = picture_node(&mut g, "mwhite", |_, _| [255; 4]);
    let mix = add(&mut g, registry::LAYER_MIXER);
    wire(&mut g, black, mix, &registry::layer_port(1));
    wire(&mut g, white, mix, &registry::layer_port(2));
    let motion = add(&mut g, &registry::shader_kind("mask_motion.wgsl"));
    set(&mut g, motion, "trail", graph::ParamValue::Float(0.0));
    wire(&mut g, mix, motion, "input");
    wire(&mut g, motion, out, "image");
    s.settle(&g, out, &[black, white]);
    // The pictures appearing counts as movement; let it pass.
    for _ in 0..12 {
        s.frame(&g, out);
    }
    let centre = |px: &[u8]| pixel(px, SIZE.0 / 2, SIZE.1 / 2);
    let px = s.frame(&g, out);
    assert!(centre(&px)[0] < 10, "still: {:?}", centre(&px));
    // The white layer goes away: everything changed this frame.
    set(
        &mut g,
        mix,
        &registry::layer_param("mute", 2),
        graph::ParamValue::Bool(true),
    );
    let px = s.frame(&g, out);
    assert!(centre(&px)[0] > 245, "moved: {:?}", centre(&px));
    // A camera sends a new picture every 4th drawn frame: the change holds
    // in between instead of flickering.
    for _ in 0..3 {
        let px = s.frame(&g, out);
        assert!(centre(&px)[0] > 245, "held: {:?}", centre(&px));
    }
    // Still for longer than that: black again.
    for _ in 0..10 {
        s.frame(&g, out);
    }
    let px = s.frame(&g, out);
    assert!(centre(&px)[0] < 10, "still again: {:?}", centre(&px));
}

#[test]
fn corner_pin_places_the_picture_between_its_corners() {
    let Some(mut s) = Session::new() else { return };
    let mut g = Graph::new();
    let out = add(&mut g, PROGRAM_OUT);
    // Red on the left half, blue on the right.
    let pic = picture_node(&mut g, "pin", |x, _| {
        if x < 32 {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        }
    });
    let pin = add(&mut g, &registry::shader_kind("fx_corner_pin.wgsl"));
    wire(&mut g, pic, pin, "input");
    wire(&mut g, pin, out, "image");
    // A half-size picture in the middle.
    let corners = |g: &mut Graph, l: f32, r: f32| {
        for (name, v) in [
            ("tl_x", l),
            ("tl_y", 0.25),
            ("tr_x", r),
            ("tr_y", 0.25),
            ("br_x", r),
            ("br_y", 0.75),
            ("bl_x", l),
            ("bl_y", 0.75),
        ] {
            set(g, pin, name, graph::ParamValue::Float(v));
        }
    };
    corners(&mut g, 0.25, 0.75);
    s.settle(&g, out, &[pic]);
    let at = |px: &[u8], u: f32| pixel(px, (SIZE.0 as f32 * u) as u32, SIZE.1 / 2);
    let px = s.frame(&g, out);
    assert!(
        at(&px, 0.3)[0] > 240 && at(&px, 0.3)[2] < 15,
        "{:?}",
        at(&px, 0.3)
    );
    assert!(
        at(&px, 0.7)[2] > 240 && at(&px, 0.7)[0] < 15,
        "{:?}",
        at(&px, 0.7)
    );
    assert!(
        at(&px, 0.1)[..3].iter().all(|v| *v < 10),
        "outside {:?}",
        at(&px, 0.1)
    );
    // Swap left and right: the picture is mirrored.
    corners(&mut g, 0.75, 0.25);
    let px = s.frame(&g, out);
    assert!(at(&px, 0.3)[2] > 240, "mirrored {:?}", at(&px, 0.3));
    // A perspective quad (right edge taller) still covers its middle.
    set(&mut g, pin, "tr_y", graph::ParamValue::Float(0.05));
    set(&mut g, pin, "br_y", graph::ParamValue::Float(0.95));
    let px = s.frame(&g, out);
    assert!(
        at(&px, 0.5)[..3].iter().any(|v| *v > 200),
        "{:?}",
        at(&px, 0.5)
    );
}

#[test]
fn compositing_demo_builds_and_draws() {
    let lib = SceneLibrary::new(None);
    // No media and no camera: the scene alone, other layers muted, no
    // webcam nodes (they would open the first camera).
    let (g, out) = nodes::demo::compositing(&lib, &nodes::demo::DemoMedia::default()).unwrap();
    assert!(!g.nodes().iter().any(|n| n.kind == registry::CAMERA));
    let mix = g
        .nodes()
        .iter()
        .find(|n| n.kind == registry::LAYER_MIXER)
        .unwrap();
    for i in 2..=6 {
        assert_eq!(
            mix.param(&registry::layer_param("mute", i))
                .map(|p| &p.value),
            Some(&graph::ParamValue::Bool(true)),
            "layer {i}"
        );
    }
    let Some((px, _, _)) = run(&g, out, 30) else {
        return;
    };
    assert!(brightness(&px) > 5.0, "the scene shows");
    // With a camera named, the webcam chain is there (not run here).
    let media = nodes::demo::DemoMedia {
        camera: Some("Test cam".into()),
        ..Default::default()
    };
    let (g, _) = nodes::demo::compositing(&lib, &media).unwrap();
    assert!(g.nodes().iter().any(|n| n.kind == registry::CAMERA));
    assert_eq!(g.frames().len(), 6);
}

#[test]
fn auto_cutout_removes_a_plain_background() {
    let Some(mut s) = Session::new() else { return };
    let mut g = Graph::new();
    let out = add(&mut g, PROGRAM_OUT);
    // A red square on white.
    let pic = picture_node(&mut g, "logo", |x, y| {
        if (20..44).contains(&x) && (10..26).contains(&y) {
            [220, 20, 30, 255]
        } else {
            [250, 250, 250, 255]
        }
    });
    let cut = add(&mut g, &registry::shader_kind("mask_auto_cutout.wgsl"));
    wire(&mut g, pic, cut, "input");
    wire(&mut g, cut, out, "image");
    s.settle(&g, out, &[pic]);
    let px = s.frame(&g, out);
    let corner = pixel(&px, 4, 4);
    let middle = pixel(&px, SIZE.0 / 2, SIZE.1 / 2);
    assert!(
        corner[..3].iter().all(|v| *v < 10),
        "background left: {corner:?}"
    );
    assert!(
        middle[0] > 200 && middle[1] < 40,
        "subject lost: {middle:?}"
    );
}

#[test]
fn transform_keeps_a_cut_out_see_through() {
    // Cut-out logo → Transform (smaller, empty edges) → over green: the green
    // must show round it, not black.
    let Some(mut s) = Session::new() else { return };
    let mut g = Graph::new();
    let out = add(&mut g, PROGRAM_OUT);
    let green = picture_node(&mut g, "tk_green", |_, _| [0, 200, 0, 255]);
    let logo = picture_node(&mut g, "tk_logo", |x, y| {
        if (20..44).contains(&x) && (10..26).contains(&y) {
            [220, 20, 30, 255]
        } else {
            [250, 250, 250, 255]
        }
    });
    let cut = add(&mut g, &registry::shader_kind("mask_auto_cutout.wgsl"));
    let place = add(&mut g, &registry::shader_kind("fx_transform.wgsl"));
    let merge = add(&mut g, &registry::shader_kind("fx_merge.wgsl"));
    wire(&mut g, logo, cut, "input");
    wire(&mut g, cut, place, "input");
    set(&mut g, place, "zoom", graph::ParamValue::Float(0.5));
    set(&mut g, place, "edges", graph::ParamValue::Choice(0));
    set(&mut g, merge, "mode", graph::ParamValue::Choice(0)); // Normal
    wire(&mut g, green, merge, "base");
    wire(&mut g, place, merge, "top");
    wire(&mut g, merge, out, "image");
    s.settle(&g, out, &[green, logo]);
    let px = s.frame(&g, out);
    let corner = pixel(&px, 2, 2);
    let middle = pixel(&px, SIZE.0 / 2, SIZE.1 / 2);
    assert!(corner[1] > 150 && corner[0] < 40, "covered: {corner:?}");
    assert!(middle[0] > 150 && middle[1] < 60, "logo lost: {middle:?}");
}

#[test]
fn switch_moves_on_in_turn_and_at_random() {
    let Some(mut s) = Session::new() else { return };
    let mut g = Graph::new();
    let out = add(&mut g, PROGRAM_OUT);
    let red = picture_node(&mut g, "sw_red", |_, _| [255, 0, 0, 255]);
    let blue = picture_node(&mut g, "sw_blue", |_, _| [0, 0, 255, 255]);
    let sw = add(&mut g, &registry::shader_kind("fx_switch.wgsl"));
    wire(&mut g, red, sw, "a");
    wire(&mut g, blue, sw, "b");
    wire(&mut g, sw, out, "image");
    set(&mut g, sw, "count", graph::ParamValue::Choice(0)); // 2 pictures
    set(&mut g, sw, "transition", graph::ParamValue::Choice(0)); // cut
    s.settle(&g, out, &[red, blue]);
    let mid = |px: &[u8]| pixel(px, SIZE.0 / 2, SIZE.1 / 2);
    assert!(mid(&s.frame(&g, out))[0] > 240, "starts on picture 1");
    set(&mut g, sw, "show", graph::ParamValue::Float(1.0));
    s.frame(&g, out);
    assert!(mid(&s.frame(&g, out))[2] > 240, "in turn: picture 2");
    // Random never repeats: with two pictures every change flips.
    set(&mut g, sw, "order", graph::ParamValue::Choice(1));
    for (k, show) in [0.3f32, 0.6, 0.9].into_iter().enumerate() {
        let before = mid(&s.frame(&g, out));
        set(&mut g, sw, "show", graph::ParamValue::Float(show));
        s.frame(&g, out);
        let after = mid(&s.frame(&g, out));
        assert_ne!(
            before[0] > 128,
            after[0] > 128,
            "change {k} kept the same picture"
        );
    }
}

#[test]
fn displace_and_liquid_leave_a_picture_alone_when_idle() {
    let Some(mut s) = Session::new() else { return };
    let mut g = Graph::new();
    let out = add(&mut g, PROGRAM_OUT);
    let pic = picture_node(&mut g, "idle", |x, _| {
        if x < 32 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        }
    });
    let disp = add(&mut g, &registry::shader_kind("fx_displace.wgsl"));
    let liquid = add(&mut g, &registry::shader_kind("fx_liquid.wgsl"));
    set(&mut g, liquid, "wobble", graph::ParamValue::Float(0.0));
    set(&mut g, liquid, "ripple", graph::ParamValue::Float(0.0));
    wire(&mut g, pic, disp, "input");
    wire(&mut g, disp, liquid, "input");
    wire(&mut g, liquid, out, "image");
    s.settle(&g, out, &[pic]);
    let px = s.frame(&g, out);
    let left = pixel(&px, SIZE.0 / 4, SIZE.1 / 2);
    let right = pixel(&px, SIZE.0 * 3 / 4, SIZE.1 / 2);
    assert!(left[0] > 240 && left[1] < 15, "{left:?}");
    assert!(right[1] > 240 && right[0] < 15, "{right:?}");
}
