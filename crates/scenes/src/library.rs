//! The scene library: built-in scenes plus every `.wgsl` file in the scenes
//! folder, watched for changes (hot reload).
//!
//! A file on disk with the same name as a built-in replaces it, so the
//! built-ins can be edited live. Watching is done by polling modification
//! times a few times a second: simple, and robust to editors that save by
//! writing a temporary file and renaming it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::builtin::BUILTIN;

/// How often the folder is checked.
pub const POLL_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Debug, Clone)]
pub struct SceneEntry {
    /// File name, e.g. `kaleido_bloom.wgsl`. Unique.
    pub file_name: String,
    /// The file on disk, if there is one.
    pub path: Option<PathBuf>,
    pub source: String,
    /// The embedded copy, for built-ins.
    pub builtin: Option<&'static str>,
    stamp: Option<(SystemTime, u64)>,
}

impl SceneEntry {
    pub fn is_builtin(&self) -> bool {
        self.builtin.is_some()
    }
}

pub struct SceneLibrary {
    dir: Option<PathBuf>,
    entries: Vec<SceneEntry>,
    last_poll: Option<Instant>,
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

impl SceneLibrary {
    /// A library over `dir` (created if missing). `None` = built-ins only.
    pub fn new(dir: Option<PathBuf>) -> Self {
        if let Some(d) = &dir
            && let Err(e) = std::fs::create_dir_all(d)
        {
            log::warn!("could not create scenes folder {}: {e}", d.display());
        }
        let entries = BUILTIN
            .iter()
            .map(|b| SceneEntry {
                file_name: b.file_name.to_string(),
                path: None,
                source: b.source.to_string(),
                builtin: Some(b.source),
                stamp: None,
            })
            .collect();
        let mut lib = Self {
            dir,
            entries,
            last_poll: None,
        };
        lib.scan();
        lib
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn entries(&self) -> &[SceneEntry] {
        &self.entries
    }

    pub fn get(&self, file_name: &str) -> Option<&SceneEntry> {
        self.entries.iter().find(|e| e.file_name == file_name)
    }

    /// Check the folder if [`POLL_INTERVAL`] has passed. Returns the file names
    /// whose source changed (edited, added, or deleted back to the built-in).
    pub fn poll(&mut self) -> Vec<String> {
        if self.last_poll.is_some_and(|t| t.elapsed() < POLL_INTERVAL) {
            return Vec::new();
        }
        self.scan()
    }

    /// Check the folder now.
    pub fn scan(&mut self) -> Vec<String> {
        self.last_poll = Some(Instant::now());
        let mut changed = Vec::new();
        let Some(dir) = self.dir.clone() else {
            return changed;
        };
        let mut on_disk: Vec<(String, PathBuf)> = match std::fs::read_dir(&dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .is_some_and(|x| x.eq_ignore_ascii_case("wgsl"))
                })
                .filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), p)))
                .collect(),
            Err(_) => Vec::new(),
        };
        on_disk.sort();

        // Files that disappeared: built-ins fall back to the embedded copy,
        // others are removed.
        self.entries.retain_mut(|e| {
            if e.path.is_none() || on_disk.iter().any(|(n, _)| *n == e.file_name) {
                return true;
            }
            match e.builtin {
                Some(src) => {
                    e.path = None;
                    e.stamp = None;
                    if e.source != src {
                        e.source = src.to_string();
                        changed.push(e.file_name.clone());
                    }
                    true
                }
                None => {
                    changed.push(e.file_name.clone());
                    false
                }
            }
        });

        for (name, path) in on_disk {
            let st = stamp(&path);
            let idx = self.entries.iter().position(|e| e.file_name == name);
            if let Some(i) = idx
                && self.entries[i].stamp == st
                && st.is_some()
            {
                continue; // unchanged
            }
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("could not read {}: {e}", path.display());
                    continue;
                }
            };
            match idx {
                Some(i) => {
                    let e = &mut self.entries[i];
                    e.stamp = st;
                    e.path = Some(path);
                    if e.source != source {
                        e.source = source;
                        changed.push(name);
                    }
                }
                None => {
                    self.entries.push(SceneEntry {
                        file_name: name.clone(),
                        path: Some(path),
                        source,
                        builtin: None,
                        stamp: st,
                    });
                    changed.push(name);
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("visual-engine-scenes-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn built_ins_without_a_folder() {
        let lib = SceneLibrary::new(None);
        assert_eq!(lib.entries().len(), BUILTIN.len());
        assert!(lib.entries().iter().all(|e| e.is_builtin()));
    }

    #[test]
    fn picks_up_new_edited_and_deleted_files() {
        let dir = temp_dir("watch");
        let mut lib = SceneLibrary::new(Some(dir.clone()));
        let n = lib.entries().len();

        let f = dir.join("mine.wgsl");
        std::fs::write(&f, "// v1").unwrap();
        assert_eq!(lib.scan(), vec!["mine.wgsl".to_string()]);
        assert_eq!(lib.entries().len(), n + 1);
        assert!(lib.scan().is_empty());

        std::fs::write(&f, "// version 2").unwrap();
        assert_eq!(lib.scan(), vec!["mine.wgsl".to_string()]);
        assert_eq!(lib.get("mine.wgsl").unwrap().source, "// version 2");

        // Overriding a built-in, then deleting the override.
        let b = BUILTIN[0].file_name;
        std::fs::write(dir.join(b), "// override").unwrap();
        assert_eq!(lib.scan(), vec![b.to_string()]);
        assert_eq!(lib.get(b).unwrap().source, "// override");
        std::fs::remove_file(dir.join(b)).unwrap();
        assert_eq!(lib.scan(), vec![b.to_string()]);
        assert_eq!(lib.get(b).unwrap().source, BUILTIN[0].source);

        std::fs::remove_file(&f).unwrap();
        assert_eq!(lib.scan(), vec!["mine.wgsl".to_string()]);
        assert_eq!(lib.entries().len(), n);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
