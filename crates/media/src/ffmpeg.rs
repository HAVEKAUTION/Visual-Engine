//! Finding and running the `ffmpeg` / `ffprobe` programs.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct FfmpegError(pub String);

impl std::fmt::Display for FfmpegError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where to find a program: `VISUAL_ENGINE_FFMPEG` (a folder, or the ffmpeg
/// file itself), next to our own executable, else the PATH.
fn find(name: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    if let Some(dir) = std::env::var_os("VISUAL_ENGINE_FFMPEG") {
        let p = PathBuf::from(dir);
        let p = if p.is_dir() {
            p.join(&exe)
        } else {
            p.with_file_name(&exe)
        };
        if p.exists() {
            return p;
        }
    }
    if let Ok(me) = std::env::current_exe()
        && let Some(dir) = me.parent()
    {
        let p = dir.join(&exe);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(exe)
}

pub fn ffmpeg_path() -> PathBuf {
    find("ffmpeg")
}

pub fn ffprobe_path() -> PathBuf {
    find("ffprobe")
}

/// A command that never opens a console window on Windows.
pub(crate) fn command(program: PathBuf) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Can we run ffmpeg and ffprobe?
pub fn have_ffmpeg() -> bool {
    let ok = |p: PathBuf| {
        command(p)
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    ok(ffmpeg_path()) && ok(ffprobe_path())
}

pub(crate) const MISSING: &str = "Video needs ffmpeg, which isn't installed. \
Windows: open a terminal and run  winget install Gyan.FFmpeg  then restart the app. \
macOS: run  brew install ffmpeg  then restart the app.";
