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
}
