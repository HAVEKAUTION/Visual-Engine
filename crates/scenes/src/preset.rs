//! Presets: a scene plus all its slider values, modulations and palette,
//! saved as a RON file in `presets/<scene>/<name>.ron`.
//!
//! The keyboard bank (keys 1–9) lives in `presets/bank.ron`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use signals::modulation::ModSlot;

use crate::palette::PaletteChoice;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    /// Scene file name, e.g. `warehouse_tunnel.wgsl`.
    pub scene: String,
    /// Base value per input name.
    #[serde(default)]
    pub values: BTreeMap<String, Vec<f32>>,
    /// Modulations per input name.
    #[serde(default)]
    pub mods: BTreeMap<String, Vec<ModSlot>>,
    #[serde(default)]
    pub palette: PaletteChoice,
}

impl Preset {
    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap_or_default()
    }

    pub fn from_ron(s: &str) -> Result<Self, String> {
        ron::from_str(s).map_err(|e| e.to_string())
    }
}

/// Number of keyboard slots (keys 1–9).
pub const BANK_SLOTS: usize = 9;

/// Preset ids (`scene_stem/name`) on keys 1–9.
pub type Bank = [Option<String>; BANK_SLOTS];

#[derive(Clone, Debug)]
pub struct PresetEntry {
    /// `scene_stem/file_stem`, unique.
    pub id: String,
    pub path: PathBuf,
    pub preset: Preset,
}

pub struct PresetLibrary {
    dir: Option<PathBuf>,
    entries: Vec<PresetEntry>,
    /// Files that could not be read: (file, error).
    pub errors: Vec<(String, String)>,
    last_scan: Option<Instant>,
}

/// Make a name safe to use as a file name.
pub fn file_stem_for(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() { "preset".into() } else { s }
}

impl PresetLibrary {
    pub fn new(dir: Option<PathBuf>) -> Self {
        if let Some(d) = &dir
            && let Err(e) = std::fs::create_dir_all(d)
        {
            log::warn!("could not create presets folder {}: {e}", d.display());
        }
        let mut lib = Self {
            dir,
            entries: Vec::new(),
            errors: Vec::new(),
            last_scan: None,
        };
        lib.scan();
        lib
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn entries(&self) -> &[PresetEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&PresetEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Presets for one scene file.
    pub fn for_scene<'a>(&'a self, scene_file: &'a str) -> impl Iterator<Item = &'a PresetEntry> {
        self.entries
            .iter()
            .filter(move |e| e.preset.scene == scene_file)
    }

    /// Re-read the folder at most once a second.
    pub fn poll(&mut self) {
        if self
            .last_scan
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1))
        {
            self.scan();
        }
    }

    pub fn scan(&mut self) {
        self.last_scan = Some(Instant::now());
        let Some(dir) = self.dir.clone() else {
            return;
        };
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        let subdirs = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for sub in subdirs {
            let Some(scene_stem) = sub.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let Ok(rd) = std::fs::read_dir(&sub) else {
                continue;
            };
            for f in rd.filter_map(|e| e.ok()).map(|e| e.path()) {
                if !f.extension().is_some_and(|x| x.eq_ignore_ascii_case("ron")) {
                    continue;
                }
                let stem = f
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let id = format!("{scene_stem}/{stem}");
                match std::fs::read_to_string(&f)
                    .map_err(|e| e.to_string())
                    .and_then(|s| Preset::from_ron(&s))
                {
                    Ok(preset) => entries.push(PresetEntry {
                        id,
                        path: f,
                        preset,
                    }),
                    Err(e) => errors.push((id, e)),
                }
            }
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        self.entries = entries;
        self.errors = errors;
    }

    /// Save (overwriting a preset with the same name for the same scene).
    /// Returns its id.
    pub fn save(&mut self, preset: &Preset) -> Result<String, String> {
        let dir = self.dir.as_ref().ok_or("no presets folder")?;
        let scene_stem = preset.scene.trim_end_matches(".wgsl");
        let sub = dir.join(scene_stem);
        std::fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
        let stem = file_stem_for(&preset.name);
        let path = sub.join(format!("{stem}.ron"));
        std::fs::write(&path, preset.to_ron()).map_err(|e| e.to_string())?;
        self.scan();
        Ok(format!("{scene_stem}/{stem}"))
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let path = self.get(id).ok_or("no such preset")?.path.clone();
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        self.scan();
        Ok(())
    }

    fn bank_path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("bank.ron"))
    }

    /// The saved key bank, if there is one.
    pub fn load_bank(&self) -> Option<Bank> {
        let s = std::fs::read_to_string(self.bank_path()?).ok()?;
        let v: Vec<Option<String>> = ron::from_str(&s).ok()?;
        Some(std::array::from_fn(|i| v.get(i).cloned().flatten()))
    }

    pub fn save_bank(&self, bank: &Bank) -> Result<(), String> {
        let path = self.bank_path().ok_or("no presets folder")?;
        let v: Vec<Option<String>> = bank.to_vec();
        let s = ron::ser::to_string_pretty(&v, ron::ser::PrettyConfig::default())
            .map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())
    }

    /// A starting bank: the first preset of each scene, in `scene_order`.
    pub fn default_bank(&self, scene_order: &[&str]) -> Bank {
        let mut ids = scene_order
            .iter()
            .filter_map(|scene| self.for_scene(scene).next().map(|e| e.id.clone()));
        std::array::from_fn(|_| ids.next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signals::modulation::{Shape, Source, Wave};

    fn sample() -> Preset {
        let mut values = BTreeMap::new();
        values.insert("speed".to_string(), vec![1.5]);
        values.insert("tint".to_string(), vec![1.0, 0.5, 0.0, 1.0]);
        let mut mods = BTreeMap::new();
        mods.insert(
            "speed".to_string(),
            vec![
                ModSlot::new(
                    Source::Signal("kick_age".into()),
                    Shape::Envelope { decay: 0.2 },
                    0.3,
                ),
                ModSlot::new(
                    Source::Lfo {
                        wave: Wave::Sine,
                        beats: 8.0,
                    },
                    Shape::Linear,
                    -0.1,
                ),
            ],
        );
        Preset {
            name: "Peak time".into(),
            scene: "warehouse_tunnel.wgsl".into(),
            values,
            mods,
            palette: PaletteChoice::Named("Acid".into()),
        }
    }

    #[test]
    fn ron_round_trip() {
        let p = sample();
        let text = p.to_ron();
        assert_eq!(Preset::from_ron(&text).unwrap(), p);
        // Hand-written files can leave fields out.
        let short = Preset::from_ron(r#"(name: "x", scene: "a.wgsl")"#).unwrap();
        assert!(short.values.is_empty());
        assert_eq!(short.palette, PaletteChoice::Scene);
    }

    #[test]
    fn save_scan_delete_and_bank() {
        let dir = std::env::temp_dir().join(format!("ve-presets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = PresetLibrary::new(Some(dir.clone()));
        let id = lib.save(&sample()).unwrap();
        assert_eq!(id, "warehouse_tunnel/Peak time");
        assert_eq!(lib.entries().len(), 1);
        assert_eq!(lib.for_scene("warehouse_tunnel.wgsl").count(), 1);

        let bank = lib.default_bank(&["other.wgsl", "warehouse_tunnel.wgsl"]);
        assert_eq!(bank[0].as_deref(), Some("warehouse_tunnel/Peak time"));
        assert_eq!(bank[1], None);
        lib.save_bank(&bank).unwrap();
        assert_eq!(lib.load_bank().unwrap(), bank);

        std::fs::write(dir.join("warehouse_tunnel").join("broken.ron"), "(oops").unwrap();
        lib.scan();
        assert_eq!(lib.errors.len(), 1);

        lib.delete(&id).unwrap();
        assert!(lib.entries().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_file_names() {
        assert_eq!(file_stem_for("Peak / time?"), "Peak _ time_");
        assert_eq!(file_stem_for("   "), "preset");
    }

    /// Every example preset shipped in the repo must load and match its scene:
    /// known inputs of the right size, known signals and palettes.
    #[test]
    fn shipped_presets_match_their_scenes() {
        let Some(dir) = crate::source_preset_dir() else {
            return;
        };
        let lib = PresetLibrary::new(Some(dir));
        assert!(lib.errors.is_empty(), "{:?}", lib.errors);
        assert!(!lib.entries().is_empty());
        for e in lib.entries() {
            let p = &e.preset;
            let b = crate::builtin::BUILTIN
                .iter()
                .find(|b| b.file_name == p.scene)
                .unwrap_or_else(|| panic!("{}: unknown scene {}", e.id, p.scene));
            let meta = crate::SceneMeta::parse(b.source, "x").unwrap();
            for (name, v) in &p.values {
                let i = meta
                    .input_index(name)
                    .unwrap_or_else(|| panic!("{}: no input {name}", e.id));
                assert_eq!(
                    v.len(),
                    meta.inputs[i].default_value().len(),
                    "{}: {name}",
                    e.id
                );
            }
            for (name, slots) in &p.mods {
                let i = meta
                    .input_index(name)
                    .unwrap_or_else(|| panic!("{}: no input {name}", e.id));
                assert_eq!(
                    meta.inputs[i].kind,
                    crate::InputKind::Float,
                    "{}: {name}",
                    e.id
                );
                for s in slots {
                    if let signals::modulation::Source::Signal(n) = &s.source {
                        assert!(
                            crate::uniforms::signal_index(n).is_some(),
                            "{}: signal {n}",
                            e.id
                        );
                    }
                }
            }
            if let PaletteChoice::Named(n) = &p.palette {
                assert!(crate::palette::named(n).is_some(), "{}: palette {n}", e.id);
            }
        }
        if let Some(bank) = lib.load_bank() {
            for id in bank.iter().flatten() {
                assert!(lib.get(id).is_some(), "bank: {id} missing");
            }
        }
    }
}
