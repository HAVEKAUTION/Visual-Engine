//! Live cameras, through `nokhwa` (Media Foundation on Windows, AVFoundation
//! on macOS, V4L2 on Linux).
//!
//! Each open camera has its own capture thread, which decodes every frame to
//! RGBA and keeps only the newest (latest frame wins). The render thread
//! takes it with `try_lock`, so it never waits for the camera.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nokhwa::Camera;
use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{ApiBackend, CameraFormat, FrameFormat, RequestedFormat, RequestedFormatType};

use crate::{Picture, Status};

/// A camera plugged into this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraDevice {
    pub name: String,
}

/// A size and frame rate a camera can capture at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CameraMode {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl CameraMode {
    /// Read "1280x720@30".
    pub fn parse(s: &str) -> Option<Self> {
        let (size, fps) = s.trim().split_once('@')?;
        let (w, h) = size.split_once('x')?;
        Some(Self {
            width: w.trim().parse().ok()?,
            height: h.trim().parse().ok()?,
            fps: fps.trim().parse().ok()?,
        })
    }

    /// The text [`Self::parse`] reads.
    pub fn key(&self) -> String {
        format!("{}x{}@{}", self.width, self.height, self.fps)
    }

    pub fn size_label(&self) -> String {
        format!("{} × {}", self.width, self.height)
    }
}

/// What an open camera is doing.
#[derive(Clone, Debug)]
pub struct CameraInfo {
    pub name: String,
    pub mode: CameraMode,
    /// How the camera sends frames (NV12, MJPEG...).
    pub format: String,
    /// Every size and rate it offers.
    pub modes: Arc<Vec<CameraMode>>,
    /// Frames per second actually arriving (cameras slow down in the dark).
    pub fps: f32,
    /// Milliseconds to turn a frame into a picture.
    pub decode_ms: f32,
}

impl CameraInfo {
    /// A fair estimate of how late the picture is, in ms: one frame of
    /// exposure and transfer, plus decoding, plus `waited_ms` before it was
    /// drawn. (The driver doesn't say when the frame was really taken; the
    /// sync test in the app measures the whole chain.)
    pub fn latency_ms(&self, waited_ms: f32) -> f32 {
        1000.0 / self.fps.max(1.0) + self.decode_ms + waited_ms
    }
}

/// The cameras on this computer. Blocking (can take a moment): call it on a
/// background thread.
pub fn cameras() -> Result<Vec<CameraDevice>, String> {
    permission()?;
    let list = nokhwa::query(ApiBackend::Auto).map_err(|e| format!("can't list cameras: {e}"))?;
    Ok(list
        .into_iter()
        .map(|c| CameraDevice {
            name: c.human_name(),
        })
        .collect())
}

#[cfg(target_os = "macos")]
fn permission() -> Result<(), String> {
    if nokhwa::nokhwa_check() {
        return Ok(());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    nokhwa::nokhwa_initialize(move |ok| {
        let _ = tx.send(ok);
    });
    match rx.recv_timeout(Duration::from_secs(120)) {
        Ok(true) => Ok(()),
        _ => Err(
            "Camera access isn't allowed. Open System Settings → Privacy & Security → \
                  Camera and switch it on for this app (or your Terminal), then restart."
                .into(),
        ),
    }
}

#[cfg(not(target_os = "macos"))]
fn permission() -> Result<(), String> {
    Ok(())
}

struct State {
    status: Status<CameraInfo>,
    latest: Option<(Picture, Instant)>,
}

struct Shared {
    state: Mutex<State>,
    quit: AtomicBool,
}

/// A camera being captured in the background.
pub struct CameraStream {
    shared: Arc<Shared>,
    name: String,
    mode: Option<CameraMode>,
}

impl CameraStream {
    /// Open the camera called `name` (empty = the first one) at `mode`
    /// (`None` = a good default: about 1280 × 720 at up to 30 fps).
    pub fn open(name: &str, mode: Option<CameraMode>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                status: Status::Loading,
                latest: None,
            }),
            quit: AtomicBool::new(false),
        });
        let s = shared.clone();
        let n = name.to_string();
        let spawned = std::thread::Builder::new()
            .name(format!("camera {name}"))
            .spawn(move || {
                if let Err(e) = capture(&s, &n, mode)
                    && let Ok(mut st) = s.state.lock()
                {
                    st.status = Status::Failed(e);
                }
            });
        if let Err(e) = spawned
            && let Ok(mut st) = shared.state.lock()
        {
            st.status = Status::Failed(e.to_string());
        }
        Self {
            shared,
            name: name.to_string(),
            mode,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The mode it was asked for (`None` = default).
    pub fn mode(&self) -> Option<CameraMode> {
        self.mode
    }

    /// Opening / running / failed. Never waits.
    pub fn info(&self) -> Status<CameraInfo> {
        match self.shared.state.try_lock() {
            Ok(s) => s.status.clone(),
            Err(_) => Status::Loading,
        }
    }

    /// The newest frame and when it arrived. Never waits.
    pub fn latest(&self) -> Option<(Picture, Instant)> {
        self.shared.state.try_lock().ok()?.latest.clone()
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        // The thread stops after its current frame and closes the camera.
        // Not joined: that could make the render thread wait for a frame.
        self.shared.quit.store(true, Ordering::Relaxed);
    }
}

/// How much we like each way of receiving frames (cheap to decode first).
fn format_rank(f: FrameFormat) -> u32 {
    match f {
        FrameFormat::NV12 => 0,
        FrameFormat::YUYV => 1,
        FrameFormat::MJPEG => 2,
        FrameFormat::RAWRGB | FrameFormat::RAWBGR => 3,
        FrameFormat::GRAY => 4,
    }
}

/// The distinct sizes and rates in `formats`, sorted.
fn modes_of(formats: &[CameraFormat]) -> Vec<CameraMode> {
    let mut m: Vec<CameraMode> = formats
        .iter()
        .filter(|f| f.frame_rate() > 0 && f.width() > 0 && f.height() > 0)
        .map(|f| CameraMode {
            width: f.width(),
            height: f.height(),
            fps: f.frame_rate(),
        })
        .collect();
    m.sort();
    m.dedup();
    m
}

/// The mode to use: the asked one if offered (else the closest), or the
/// default (largest up to 1920 × 1080 at the best rate up to 30 fps).
pub(crate) fn choose_mode(modes: &[CameraMode], want: Option<CameraMode>) -> Option<CameraMode> {
    if let Some(w) = want {
        return modes.iter().copied().min_by_key(|m| {
            let area = |m: &CameraMode| (m.width * m.height) as i64;
            (area(m) - area(&w)).abs() / 1000 + (m.fps as i64 - w.fps as i64).abs() * 50
        });
    }
    let best_fps = modes.iter().map(|m| m.fps.min(30)).max()?;
    modes
        .iter()
        .copied()
        .filter(|m| m.fps >= best_fps && m.width <= 1920 && m.height <= 1080)
        .max_by_key(|m| (m.width * m.height, std::cmp::Reverse(m.fps)))
        .or_else(|| modes.first().copied())
}

fn capture(sh: &Shared, name: &str, want: Option<CameraMode>) -> Result<(), String> {
    permission()?;
    let list = nokhwa::query(ApiBackend::Auto).map_err(|e| format!("can't list cameras: {e}"))?;
    let device = if name.trim().is_empty() {
        list.first()
    } else {
        list.iter().find(|c| c.human_name() == name)
    }
    .ok_or_else(|| {
        if name.trim().is_empty() {
            "No camera found. Plug one in, then press ⟳.".to_string()
        } else {
            format!("Camera \"{name}\" isn't connected. Plug it in, or pick another camera.")
        }
    })?;
    let index = device.index().clone();
    let human = device.human_name();

    // A camera that was just closed can take a moment to free up.
    let started = Instant::now();
    let mut cam = loop {
        match Camera::new(
            index.clone(),
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::None),
        ) {
            Ok(c) => break c,
            Err(e) if started.elapsed() > Duration::from_secs(3) => {
                return Err(format!(
                    "Can't open \"{human}\": {e}. Is another app using it?"
                ));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
        if sh.quit.load(Ordering::Relaxed) {
            return Ok(());
        }
    };
    let formats = cam
        .compatible_camera_formats()
        .map_err(|e| format!("\"{human}\" didn't say what it can do: {e}"))?;
    let modes = modes_of(&formats);
    let mode = choose_mode(&modes, want).ok_or_else(|| format!("\"{human}\" offers no video"))?;
    let format = formats
        .iter()
        .filter(|f| {
            f.width() == mode.width && f.height() == mode.height && f.frame_rate() == mode.fps
        })
        .min_by_key(|f| format_rank(f.format()))
        .copied()
        .ok_or_else(|| "camera format went missing".to_string())?;
    cam.set_camera_requset(RequestedFormat::new::<RgbAFormat>(
        RequestedFormatType::Exact(format),
    ))
    .map_err(|e| format!("\"{human}\" refused {}: {e}", mode.key()))?;
    cam.open_stream()
        .map_err(|e| format!("Can't start \"{human}\": {e}. Is another app using it?"))?;

    let mut info = CameraInfo {
        name: human.clone(),
        mode,
        format: format.format().to_string(),
        modes: Arc::new(modes),
        fps: mode.fps as f32,
        decode_ms: 0.0,
    };
    if let Ok(mut s) = sh.state.lock() {
        s.status = Status::Ready(info.clone());
    }
    let mut last: Option<Instant> = None;
    let mut failures = 0;
    while !sh.quit.load(Ordering::Relaxed) {
        let frame = match cam.frame() {
            Ok(f) => f,
            Err(e) => {
                failures += 1;
                if failures > 50 {
                    let _ = cam.stop_stream();
                    return Err(format!("\"{human}\" stopped sending pictures: {e}"));
                }
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
        };
        failures = 0;
        let got = Instant::now();
        let Ok(img) = frame.decode_image::<RgbAFormat>() else {
            continue;
        };
        let (w, h) = img.dimensions();
        let pic = Picture::new(w, h, img.into_raw());
        let done = Instant::now();
        // Smoothed rate and decode time, for the latency estimate.
        let decode = (done - got).as_secs_f32() * 1000.0;
        info.decode_ms += 0.1 * (decode - info.decode_ms);
        if let Some(prev) = last {
            let dt = (got - prev).as_secs_f32().max(1e-3);
            info.fps += 0.1 * (1.0 / dt - info.fps);
        }
        last = Some(got);
        if let Ok(mut s) = sh.state.lock() {
            s.latest = Some((pic, done));
            s.status = Status::Ready(info.clone());
        }
    }
    let _ = cam.stop_stream();
    Ok(())
}
