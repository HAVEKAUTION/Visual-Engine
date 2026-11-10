//! RGBA8 pictures, still images and thumbnails.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// One decoded picture: straight (not premultiplied) RGBA8, rows top to
/// bottom, no padding. `id` is unique per picture, so a renderer can tell
/// whether it already uploaded this one.
#[derive(Clone, Debug)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
    pub id: u64,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl Picture {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        debug_assert_eq!(rgba.len(), (width * height * 4) as usize);
        Self {
            width,
            height,
            rgba: Arc::new(rgba),
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn bytes(&self) -> usize {
        self.rgba.len()
    }
}

/// Load a still image, scaled down (keeping its shape) so neither side is
/// over `max_side`.
pub fn load_image(path: &Path, max_side: u32) -> Result<Picture, String> {
    if !path.exists() {
        return Err(format!("file not found: {}", path.display()));
    }
    let img = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| format!("can't read {}: {e}", file_name(path)))?;
    let img = if img.width() > max_side || img.height() > max_side {
        img.resize(max_side, max_side, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(Picture::new(w, h, rgba.into_raw()))
}

/// A small picture of any media file (images directly, videos via ffmpeg at
/// 10% in). Blocking: call it on a background thread.
pub fn thumbnail(path: &Path, max_side: u32) -> Result<Picture, String> {
    match crate::kind_of(path) {
        Some(crate::Kind::Image) => load_image(path, max_side),
        Some(crate::Kind::Video) => crate::video::grab_frame(path, 0.1, max_side),
        _ => Err("not a picture or video".into()),
    }
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
