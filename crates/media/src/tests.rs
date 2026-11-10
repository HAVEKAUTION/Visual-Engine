use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::*;
use crate::ffmpeg::command;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ve-media-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    d
}

/// A 2-second 64×36 test clip at 10 fps whose frame number is readable from
/// its brightness (frame n is grey level ~ 12·n).
fn make_clip(dir: &std::path::Path) -> Option<PathBuf> {
    if !have_ffmpeg() {
        eprintln!("ffmpeg not installed: skipping video test");
        return None;
    }
    let out = dir.join("clip.mp4");
    let ok = command(ffmpeg_path())
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
        .arg(&out)
        .status()
        .is_ok_and(|s| s.success());
    ok.then_some(out)
}

fn grey(p: &Picture) -> f32 {
    let px = &p.rgba;
    px.chunks(4).map(|c| c[1] as f32).sum::<f32>() / (px.len() / 4) as f32
}

fn wait_for(mut f: impl FnMut() -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(15) {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn kinds_by_extension() {
    assert_eq!(kind_of(std::path::Path::new("a/b.PNG")), Some(Kind::Image));
    assert_eq!(kind_of(std::path::Path::new("x.mov")), Some(Kind::Video));
    assert_eq!(kind_of(std::path::Path::new("x.flac")), Some(Kind::Audio));
    assert_eq!(kind_of(std::path::Path::new("x.txt")), None);
}

#[test]
fn decode_sizes_keep_shape_and_are_even() {
    assert_eq!(video::decode_size(1920, 1080, 1280), (1280, 720));
    assert_eq!(video::decode_size(1080, 1920, 960), (540, 960));
    assert_eq!(video::decode_size(640, 360, 1280), (640, 360));
    let (w, h) = video::decode_size(1001, 333, 640);
    assert!(w % 2 == 0 && h % 2 == 0 && w <= 640);
}

#[test]
fn images_load_in_the_background_and_scale_down() {
    let dir = temp_dir("img");
    let path = dir.join("a.png");
    let mut img = image::RgbaImage::new(300, 100);
    for p in img.pixels_mut() {
        *p = image::Rgba([255, 0, 0, 128]);
    }
    img.save(&path).unwrap();
    let pic = load_image(&path, 150).unwrap();
    assert_eq!((pic.width, pic.height), (150, 50));
    assert_eq!(&pic.rgba[..4], &[255, 0, 0, 128]);

    let mut lib = MediaLibrary::new();
    assert!(wait_for(|| matches!(lib.image(&path), Status::Ready(_))));
    assert!(matches!(
        lib.image(&dir.join("missing.png")),
        Status::Loading | Status::Failed(_)
    ));
    assert!(wait_for(|| matches!(
        lib.image(&dir.join("missing.png")),
        Status::Failed(_)
    )));
}

#[test]
fn video_probes_streams_and_seeks() {
    let dir = temp_dir("vid");
    let Some(clip) = make_clip(&dir) else { return };
    let info = probe(&clip).unwrap();
    assert_eq!((info.width, info.height), (64, 36));
    assert!((info.fps - 10.0).abs() < 0.01);
    assert_eq!(info.frames, 20);

    let v = VideoStream::open(&clip, Quality::High);
    // Ask for frame 5: it arrives, and it's brighter than frame 1.
    v.request(5);
    assert!(wait_for(|| v.has_frame(5)));
    let f5 = grey(&v.frame_near(5).unwrap());
    // Seek backwards.
    v.request(1);
    assert!(wait_for(|| v.has_frame(1)));
    let f1 = grey(&v.frame_near(1).unwrap());
    assert!(f5 > f1 + 30.0, "frame 5 {f5} vs frame 1 {f1}");
    // Play through: the whole clip ends up cached.
    for i in 0..20 {
        v.request(i);
        assert!(wait_for(|| v.has_frame(i)), "frame {i}");
    }
    assert_eq!(v.cached_frames(), 20);
    // Asking past the end clamps to the last frame.
    v.request(500);
    assert!(v.frame_near(500).is_some());

    let thumb = thumbnail(&clip, 32).unwrap();
    assert_eq!((thumb.width, thumb.height), (32, 18));
}

#[test]
fn missing_video_fails_cleanly() {
    let v = VideoStream::open(
        std::path::Path::new("definitely/not/here.mp4"),
        Quality::Low,
    );
    assert!(wait_for(|| matches!(v.info(), Status::Failed(_))));
    assert!(v.frame_near(0).is_none());
}

fn solid(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Picture {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            v.extend_from_slice(&f(x, y));
        }
    }
    Picture::new(w, h, v)
}

#[test]
fn features_measure_brightness_and_colour() {
    let red = solid(64, 36, |_, _| [255, 0, 0, 255]);
    assert!((red.features.luma - 0.2126).abs() < 1e-3);
    assert!(hue_of(red.features.chroma) < 0.01 || hue_of(red.features.chroma) > 0.99);
    let blue = solid(64, 36, |_, _| [0, 0, 255, 255]);
    assert!((hue_of(blue.features.chroma) - 2.0 / 3.0).abs() < 0.01);
    let grey = solid(64, 36, |_, _| [128, 128, 128, 255]);
    assert!(grey.features.chroma[0].hypot(grey.features.chroma[1]) < 1e-4);
    // Left half white: the grid sees it on the left only.
    let half = solid(
        64,
        36,
        |x, _| if x < 32 { [255; 4] } else { [0, 0, 0, 255] },
    );
    assert!(half.features.grid[0] > 0.99 && half.features.grid[GRID_W - 1] < 0.01);
    assert!((half.features.luma - 0.5).abs() < 0.01);
}

#[test]
fn video_signals_see_motion_where_it_happens() {
    let dt = 1.0 / 30.0;
    let mut s = VideoSignals::new();
    // A still picture: no motion, brightness settles at 1 (typical).
    let still = solid(64, 36, |_, _| [100, 100, 100, 255]);
    for _ in 0..90 {
        s.update(Some((still.id, &still.features)), dt);
    }
    assert!(s.motion < 0.05, "still motion {}", s.motion);
    assert!(
        (s.brightness - 1.0).abs() < 0.05,
        "brightness {}",
        s.brightness
    );
    // A box flickering in the top-right corner: motion, placed top right.
    let mut pics = Vec::new();
    for i in 0..60 {
        let on = i % 2 == 0;
        pics.push(solid(64, 36, move |x, y| {
            if on && x >= 48 && y < 9 {
                [255; 4]
            } else {
                [100, 100, 100, 255]
            }
        }));
    }
    for p in &pics {
        s.update(Some((p.id, &p.features)), dt);
    }
    assert!(s.motion > 1.0, "moving motion {}", s.motion);
    assert!(
        s.motion_x > 0.7 && s.motion_y > 0.7,
        "at {} {}",
        s.motion_x,
        s.motion_y
    );
    // Frozen (same picture keeps coming): motion fades away.
    let last = pics.last().unwrap();
    for _ in 0..60 {
        s.update(Some((last.id, &last.features)), dt);
    }
    assert!(s.motion < 0.1, "frozen motion {}", s.motion);
    // Same pictures, same time steps: same signals.
    let run = || {
        let mut v = VideoSignals::new();
        let mut out = [0.0; 5];
        for p in &pics {
            out = v.update(Some((p.id, &p.features)), dt);
        }
        out
    };
    assert_eq!(run(), run());
}

#[test]
fn camera_modes_parse_and_default_sensibly() {
    let m = CameraMode::parse("1280x720@30").unwrap();
    assert_eq!((m.width, m.height, m.fps), (1280, 720, 30));
    assert_eq!(m.key(), "1280x720@30");
    assert!(CameraMode::parse("auto").is_none());
    let mk = |w, h, fps| CameraMode {
        width: w,
        height: h,
        fps,
    };
    let modes = vec![
        mk(640, 480, 30),
        mk(1280, 720, 30),
        mk(1920, 1080, 5),
        mk(3840, 2160, 30),
    ];
    assert_eq!(camera::choose_mode(&modes, None), Some(mk(1280, 720, 30)));
    assert_eq!(
        camera::choose_mode(&modes, Some(mk(1920, 1080, 5))),
        Some(mk(1920, 1080, 5))
    );
    assert_eq!(
        camera::choose_mode(&modes, Some(mk(800, 600, 30))),
        Some(mk(640, 480, 30))
    );
}

/// Needs a real camera: `cargo test -p media -- --ignored`.
#[test]
#[ignore]
fn real_camera_delivers_frames() {
    let list = cameras().unwrap();
    eprintln!("cameras: {list:?}");
    let Some(dev) = list.first() else { return };
    let cam = CameraStream::open(&dev.name, None);
    assert!(wait_for(|| cam.latest().is_some()), "{:?}", cam.info());
    let Status::Ready(info) = cam.info() else {
        panic!("{:?}", cam.info())
    };
    std::thread::sleep(Duration::from_secs(2));
    let Status::Ready(info2) = cam.info() else {
        panic!()
    };
    eprintln!(
        "{} {} {} · {} modes · {:.1} fps · decode {:.1} ms · latency ≈ {:.0} ms",
        info.name,
        info.mode.key(),
        info.format,
        info.modes.len(),
        info2.fps,
        info2.decode_ms,
        info2.latency_ms(0.0)
    );
    let (pic, _) = cam.latest().unwrap();
    assert_eq!((pic.width, pic.height), (info.mode.width, info.mode.height));
    drop(cam);
    // Reopening straight away works (the old thread lets go).
    let again = CameraStream::open(
        &dev.name,
        Some(CameraMode {
            width: 640,
            height: 480,
            fps: 30,
        }),
    );
    assert!(wait_for(|| again.latest().is_some()), "{:?}", again.info());
    let (pic, _) = again.latest().unwrap();
    assert_eq!((pic.width, pic.height), (640, 480));
}
