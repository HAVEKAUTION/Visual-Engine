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
