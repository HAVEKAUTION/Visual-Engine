//! Video files: probing and a background decoder with a frame cache.
//!
//! The decoder thread runs `ffmpeg` and reads raw RGBA frames from its
//! output, starting where the player wants to be. Frames go into a cache
//! (up to a memory budget), so loops, ping-pong and scrubbing over a clip
//! that fits are instant after the first pass. Asking for a frame far from
//! where ffmpeg is reading restarts it there (a seek).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

use crate::ffmpeg::{MISSING, command, ffmpeg_path, ffprobe_path};
use crate::picture::file_name;
use crate::{Picture, Status};

/// Decode size: the longest side is at most this many pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Quality {
    Low,
    Medium,
    #[default]
    High,
    Full,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Medium, Quality::High, Quality::Full];
    pub const NAMES: [&'static str; 4] =
        ["Low (640)", "Medium (960)", "High (1280)", "Full (1920)"];

    pub fn from_index(i: u32) -> Self {
        Self::ALL.get(i as usize).copied().unwrap_or_default()
    }

    pub fn max_side(self) -> u32 {
        match self {
            Quality::Low => 640,
            Quality::Medium => 960,
            Quality::High => 1280,
            Quality::Full => 1920,
        }
    }
}

/// Memory the frame cache of one video may use.
const CACHE_BYTES: usize = 512 << 20;

#[derive(Clone, Debug, PartialEq)]
pub struct VideoInfo {
    /// The file's own size.
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Number of frames (estimated from the duration if the file doesn't say).
    pub frames: i64,
    pub seconds: f64,
    pub codec: String,
}

impl VideoInfo {
    /// The size frames are decoded at for `q` (shape kept, even numbers).
    pub fn decode_size(&self, q: Quality) -> (u32, u32) {
        decode_size(self.width, self.height, q.max_side())
    }
}

pub(crate) fn decode_size(w: u32, h: u32, max_side: u32) -> (u32, u32) {
    let (w, h) = (w.max(2), h.max(2));
    let s = (max_side as f64 / w.max(h) as f64).min(1.0);
    let even = |x: f64| ((x / 2.0).round() as u32 * 2).max(2);
    (even(w as f64 * s), even(h as f64 * s))
}

fn parse_rate(s: &str) -> Option<f64> {
    let mut it = s.split('/');
    let a: f64 = it.next()?.trim().parse().ok()?;
    let b: f64 = it
        .next()
        .map(|b| b.trim().parse().ok())
        .unwrap_or(Some(1.0))?;
    (b != 0.0 && a > 0.0).then(|| a / b)
}

/// Read a video file's size, frame rate and length (runs ffprobe).
pub fn probe(path: &Path) -> Result<VideoInfo, String> {
    if !path.exists() {
        return Err(format!("file not found: {}", path.display()));
    }
    let out = command(ffprobe_path())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,avg_frame_rate,nb_frames,duration,codec_name:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|_| MISSING.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "can't read {}: {}",
            file_name(path),
            err.lines().next().unwrap_or("unknown error")
        ));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let stream = v["streams"]
        .get(0)
        .ok_or_else(|| format!("{} has no video in it", file_name(path)))?;
    let num = |x: &Value| -> Option<f64> {
        x.as_f64()
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    };
    let width = num(&stream["width"]).unwrap_or(0.0) as u32;
    let height = num(&stream["height"]).unwrap_or(0.0) as u32;
    let fps = stream["avg_frame_rate"]
        .as_str()
        .and_then(parse_rate)
        .or_else(|| stream["r_frame_rate"].as_str().and_then(parse_rate))
        .filter(|f| *f > 1.0 && *f < 1000.0)
        .unwrap_or(30.0);
    let seconds = num(&stream["duration"])
        .or_else(|| num(&v["format"]["duration"]))
        .unwrap_or(0.0);
    let frames = num(&stream["nb_frames"])
        .map(|f| f as i64)
        .filter(|f| *f > 0)
        .unwrap_or_else(|| (seconds * fps).round() as i64)
        .max(1);
    if width == 0 || height == 0 {
        return Err(format!("{} has no picture size", file_name(path)));
    }
    Ok(VideoInfo {
        width,
        height,
        fps,
        frames,
        seconds: if seconds > 0.0 {
            seconds
        } else {
            frames as f64 / fps
        },
        codec: stream["codec_name"].as_str().unwrap_or("?").to_string(),
    })
}

/// Spawn ffmpeg writing raw RGBA frames of `size` from `at` seconds.
fn spawn(
    path: &Path,
    at: f64,
    size: (u32, u32),
    fps: f64,
    frames: Option<u32>,
) -> std::io::Result<Child> {
    let mut c = command(ffmpeg_path());
    c.args(["-nostdin", "-v", "error"]);
    if at > 0.0 {
        c.args(["-ss", &format!("{at:.4}")]);
    }
    c.arg("-i").arg(path);
    c.args(["-an", "-sn", "-dn"]);
    c.args([
        "-vf",
        &format!("scale={}:{}:flags=bilinear", size.0, size.1),
        "-r",
        &format!("{fps:.6}"),
    ]);
    if let Some(n) = frames {
        c.args(["-frames:v", &n.to_string()]);
    }
    c.args(["-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"]);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// One frame at `fraction` (0..1) of a video, for thumbnails. Blocking.
pub fn grab_frame(path: &Path, fraction: f64, max_side: u32) -> Result<Picture, String> {
    let info = probe(path)?;
    let size = decode_size(info.width, info.height, max_side);
    let at = (info.seconds * fraction.clamp(0.0, 1.0)).max(0.0);
    let mut child = spawn(path, at, size, info.fps, Some(1)).map_err(|_| MISSING.to_string())?;
    let mut buf = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut buf);
    }
    let _ = child.wait();
    let n = (size.0 * size.1 * 4) as usize;
    if buf.len() < n {
        return Err(format!("couldn't decode a frame of {}", file_name(path)));
    }
    buf.truncate(n);
    Ok(Picture::new(size.0, size.1, buf))
}

struct State {
    info: Status<VideoInfo>,
    /// Frame the player wants now.
    want: Option<i64>,
    cache: BTreeMap<i64, Picture>,
    quit: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    child: Mutex<Option<Child>>,
}

/// A video file being decoded in the background. Ask for frames with
/// [`Self::request`], take the best available with [`Self::frame_near`].
pub struct VideoStream {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    path: PathBuf,
}

impl VideoStream {
    pub fn open(path: &Path, quality: Quality) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                info: Status::Loading,
                want: None,
                cache: BTreeMap::new(),
                quit: false,
            }),
            wake: Condvar::new(),
            child: Mutex::new(None),
        });
        let s = shared.clone();
        let p = path.to_path_buf();
        let thread = std::thread::Builder::new()
            .name(format!("video {}", file_name(path)))
            .spawn(move || decoder(&s, &p, quality))
            .ok();
        Self {
            shared,
            thread,
            path: path.to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Size, rate and length, once probed.
    pub fn info(&self) -> Status<VideoInfo> {
        match self.shared.state.try_lock() {
            Ok(s) => s.info.clone(),
            Err(_) => Status::Loading,
        }
    }

    /// The player wants frame `index` now (the decoder works towards it).
    pub fn request(&self, index: i64) {
        if let Ok(mut s) = self.shared.state.lock()
            && s.want != Some(index)
        {
            s.want = Some(index);
            self.shared.wake.notify_one();
        }
    }

    /// The decoded frame closest to `index` (at or before it if possible).
    /// Never waits for the decoder.
    pub fn frame_near(&self, index: i64) -> Option<Picture> {
        let s = self.shared.state.try_lock().ok()?;
        s.cache
            .range(..=index)
            .next_back()
            .or_else(|| s.cache.range(index..).next())
            .map(|(_, p)| p.clone())
    }

    /// Is exactly this frame decoded?
    pub fn has_frame(&self, index: i64) -> bool {
        self.shared
            .state
            .lock()
            .is_ok_and(|s| s.cache.contains_key(&index))
    }

    /// Frames in the cache (for stats and tests).
    pub fn cached_frames(&self) -> usize {
        self.shared.state.lock().map(|s| s.cache.len()).unwrap_or(0)
    }
}

impl Drop for VideoStream {
    fn drop(&mut self) {
        if let Ok(mut s) = self.shared.state.lock() {
            s.quit = true;
        }
        self.shared.wake.notify_all();
        if let Ok(mut c) = self.shared.child.lock()
            && let Some(mut child) = c.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The decoder thread.
fn decoder(sh: &Shared, path: &Path, quality: Quality) {
    let info = match probe(path) {
        Ok(i) => i,
        Err(e) => {
            if let Ok(mut s) = sh.state.lock() {
                s.info = Status::Failed(e);
            }
            return;
        }
    };
    if let Ok(mut s) = sh.state.lock() {
        s.info = Status::Ready(info.clone());
    }
    let size = info.decode_size(quality);
    let frame_bytes = (size.0 * size.1 * 4) as usize;
    let max_frames = (CACHE_BYTES / frame_bytes).max(8) as i64;
    // Read this far ahead of the wanted frame.
    let ahead = ((info.fps * 0.75).ceil() as i64).clamp(4, max_frames / 2);
    let mut frames = info.frames;
    // The open ffmpeg output and the index of the next frame it will give.
    let mut pipe: Option<(std::process::ChildStdout, i64)> = None;

    let stop = |sh: &Shared| {
        if let Ok(mut c) = sh.child.lock()
            && let Some(mut child) = c.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    };

    loop {
        // What to decode next?
        let target = {
            let Ok(mut s) = sh.state.lock() else { return };
            loop {
                if s.quit {
                    drop(s);
                    stop(sh);
                    return;
                }
                if let Some(want) = s.want {
                    let want = want.clamp(0, frames - 1);
                    let missing =
                        (want..(want + ahead).min(frames)).find(|i| !s.cache.contains_key(i));
                    if let Some(t) = missing {
                        break t;
                    }
                }
                s = match sh.wake.wait_timeout(s, Duration::from_millis(100)) {
                    Ok((g, _)) => g,
                    Err(_) => return,
                };
            }
        };
        // Keep reading if ffmpeg is just before the target; else seek there.
        let reuse =
            matches!(&pipe, Some((_, next)) if *next <= target && target - *next <= ahead * 2);
        if !reuse {
            stop(sh);
            pipe = None;
            match spawn(path, target as f64 / info.fps, size, info.fps, None) {
                Ok(mut child) => {
                    if let Some(out) = child.stdout.take() {
                        pipe = Some((out, target));
                    }
                    if let Ok(mut c) = sh.child.lock() {
                        *c = Some(child);
                    }
                }
                Err(_) => {
                    if let Ok(mut s) = sh.state.lock() {
                        s.info = Status::Failed(MISSING.into());
                    }
                    return;
                }
            }
        }
        let Some((out, next)) = pipe.as_mut() else {
            continue;
        };
        let mut buf = vec![0u8; frame_bytes];
        match out.read_exact(&mut buf) {
            Ok(()) => {
                let index = *next;
                *next += 1;
                let Ok(mut s) = sh.state.lock() else { return };
                s.cache.insert(index, Picture::new(size.0, size.1, buf));
                // Over budget: drop the frames furthest from where we play.
                let want = s.want.unwrap_or(index);
                while s.cache.len() as i64 > max_frames {
                    let (lo, hi) = match (s.cache.keys().next(), s.cache.keys().next_back()) {
                        (Some(a), Some(b)) => (*a, *b),
                        _ => break,
                    };
                    let k = if want - lo > hi - want { lo } else { hi };
                    s.cache.remove(&k);
                }
            }
            Err(_) => {
                // End of the file (or ffmpeg stopped): the real length is known now.
                let reached = *next;
                pipe = None;
                stop(sh);
                if reached < frames && reached > 0 {
                    frames = reached;
                } else if reached == 0 || reached <= target {
                    // Seeking past the real end: shorten and try again.
                    frames = target.max(1);
                }
                if let Ok(mut s) = sh.state.lock()
                    && let Status::Ready(i) = &mut s.info
                {
                    i.frames = frames;
                }
                if frames <= 1 && reached == 0 {
                    if let Ok(mut s) = sh.state.lock() {
                        s.info = Status::Failed(format!("couldn't decode {}", file_name(path)));
                    }
                    return;
                }
            }
        }
    }
}
