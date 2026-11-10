//! Media sources: still images and video files, decoded off the render
//! thread and handed over as RGBA8 pictures (latest frame wins).
//!
//! **Video decoding uses the `ffmpeg` program as a child process** (piping raw
//! RGBA frames), not the ffmpeg libraries linked in. Why:
//! - no build-time dependency (linking ffmpeg needs its dev libraries plus
//!   LLVM/clang for bindgen, painful on Windows and macOS);
//! - a broken file or codec crash takes down a child process, never the app;
//! - every codec the installed ffmpeg has works: H.264, H.265, ProRes, VP9,
//!   AV1, HAP...;
//! - the same program will do export (M10).
//!
//! The cost is a process per playing video and a restart when seeking far,
//! which the frame cache hides for typical VJ loops (they fit entirely).

mod camera;
mod features;
mod ffmpeg;
mod picture;
mod video;

pub use camera::{CameraDevice, CameraInfo, CameraMode, CameraStream, cameras};
pub use features::{Features, GRID_H, GRID_W, VIDEO_SIGNAL_NAMES, VideoSignals, features, hue_of};
pub use ffmpeg::{FfmpegError, ffmpeg_path, ffprobe_path, have_ffmpeg};
pub use picture::{Picture, load_image, thumbnail};
pub use video::{Quality, VideoInfo, VideoStream, probe};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What kind of file this is, by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
    Audio,
}

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];
pub const VIDEO_EXTS: &[&str] = &[
    "mp4", "mov", "m4v", "mkv", "webm", "avi", "wmv", "mpg", "mpeg", "hap", "mxf", "ts",
];
pub const AUDIO_EXTS: &[&str] = &["mp3", "wav", "flac", "ogg", "oga", "m4a", "aac"];

pub fn kind_of(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if IMAGE_EXTS.contains(&ext.as_str()) {
        Some(Kind::Image)
    } else if VIDEO_EXTS.contains(&ext.as_str()) {
        Some(Kind::Video)
    } else if AUDIO_EXTS.contains(&ext.as_str()) {
        Some(Kind::Audio)
    } else {
        None
    }
}

/// An image being loaded in the background, or loaded, or failed.
#[derive(Clone, Debug)]
pub enum Status<T> {
    Loading,
    Ready(T),
    Failed(String),
}

type Slot = Arc<Mutex<Status<Picture>>>;

/// All media the graph uses, opened on demand and closed when no longer used.
/// Keyed by file path and decode settings, so two nodes on the same file
/// share one decoder.
#[derive(Default)]
pub struct MediaLibrary {
    images: HashMap<PathBuf, (Slot, u64)>,
    videos: HashMap<(PathBuf, Quality), (VideoStream, u64)>,
    cameras: HashMap<String, (CameraStream, u64)>,
    frame: u64,
}

/// Unused media is closed after this many frames (about 5 s at 60 fps).
const KEEP_FRAMES: u64 = 300;

impl MediaLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// A still image (starts loading on the first call).
    pub fn image(&mut self, path: &Path) -> Status<Picture> {
        let frame = self.frame;
        let (slot, used) = self.images.entry(path.to_path_buf()).or_insert_with(|| {
            let slot: Slot = Arc::new(Mutex::new(Status::Loading));
            let s = slot.clone();
            let p = path.to_path_buf();
            let spawned = std::thread::Builder::new()
                .name("image load".into())
                .spawn(move || {
                    let r = load_image(&p, 4096);
                    if let Ok(mut g) = s.lock() {
                        *g = match r {
                            Ok(pic) => Status::Ready(pic),
                            Err(e) => Status::Failed(e),
                        };
                    }
                });
            if let Err(e) = spawned
                && let Ok(mut g) = slot.lock()
            {
                *g = Status::Failed(e.to_string());
            }
            (slot, frame)
        });
        *used = frame;
        // Never wait on the loader: if it's busy, say "loading".
        match slot.try_lock() {
            Ok(s) => s.clone(),
            Err(_) => Status::Loading,
        }
    }

    /// A video stream (starts the decoder on the first call).
    pub fn video(&mut self, path: &Path, quality: Quality) -> &mut VideoStream {
        let frame = self.frame;
        let (v, used) = self
            .videos
            .entry((path.to_path_buf(), quality))
            .or_insert_with(|| (VideoStream::open(path, quality), frame));
        *used = frame;
        v
    }

    /// A live camera (opens it on the first call). A camera can only be open
    /// once, so every node on the same camera shares one stream: the first
    /// node asking each frame picks the mode, and the second value is false
    /// for later nodes asking for a different one.
    pub fn camera(&mut self, name: &str, mode: Option<CameraMode>) -> (&CameraStream, bool) {
        let frame = self.frame;
        let reopen = match self.cameras.get(name) {
            Some((c, used)) => c.mode() != mode && *used != frame,
            None => true,
        };
        if reopen {
            // Close anything that may hold the same device first ("" = the
            // first camera, which could be this one).
            self.cameras
                .retain(|n, _| n != name && !n.is_empty() && !name.is_empty());
            self.cameras
                .insert(name.to_string(), (CameraStream::open(name, mode), frame));
        }
        let (c, used) = self.cameras.get_mut(name).expect("just inserted");
        *used = frame;
        let same = c.mode() == mode;
        (c, same)
    }

    /// Close a camera so it opens again on the next use (after replugging).
    pub fn reload_camera(&mut self, name: &str) {
        self.cameras.remove(name);
    }

    /// Call once per frame: closes media nobody asked for in a while.
    pub fn end_frame(&mut self) {
        self.frame += 1;
        let now = self.frame;
        self.images.retain(|_, (_, used)| now - *used < KEEP_FRAMES);
        self.videos.retain(|_, (_, used)| now - *used < KEEP_FRAMES);
        // Cameras close quickly (their light goes off): about 1 s.
        self.cameras.retain(|_, (_, used)| now - *used < 60);
    }

    /// Forget a file so it's read again (e.g. after relinking or editing it).
    pub fn reload(&mut self, path: &Path) {
        self.images.remove(path);
        self.videos.retain(|(p, _), _| p != path);
    }

    /// Open video decoders (for stats).
    pub fn open_videos(&self) -> usize {
        self.videos.len()
    }
}

#[cfg(test)]
mod tests;
