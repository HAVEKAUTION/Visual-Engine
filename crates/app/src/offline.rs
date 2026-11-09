//! "Analyse file offline": run the analyser over a whole song as fast as
//! possible and save a CSV next to it, one row per analysis hop.
//!
//! It uses the same decoder, mono mix, resampler and analyser as live
//! playback, so the CSV shows exactly what the visuals would see (for a
//! straight play-through from the start).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use analysis::{AnalysisConfig, Analyzer};
use anyhow::{Context, Result};
use signals::{Band, SignalFrame};

/// Summary shown in the UI when the analysis finishes.
#[derive(Clone, Debug)]
pub struct OfflineReport {
    pub csv_path: PathBuf,
    pub duration_secs: f64,
    pub elapsed_secs: f64,
    pub median_bpm: Option<f32>,
    pub onsets: u32,
    pub kicks: u32,
    pub snares: u32,
    pub hats: u32,
    pub builds: u32,
    pub drops: u32,
    pub section_changes: u32,
}

impl OfflineReport {
    pub fn summary(&self) -> String {
        let bpm = self
            .median_bpm
            .map(|b| format!("{b:.1} BPM"))
            .unwrap_or_else(|| "no tempo".into());
        format!(
            "{bpm} · {} onsets: {} kicks, {} snares, {} hats · {} builds, {} drops, {} section changes · \
             {:.0} s of audio in {:.1} s",
            self.onsets,
            self.kicks,
            self.snares,
            self.hats,
            self.builds,
            self.drops,
            self.section_changes,
            self.duration_secs,
            self.elapsed_secs
        )
    }
}

/// `song.mp3` → `song.mp3.analysis.csv` in the same folder.
pub fn csv_path_for(song: &Path) -> PathBuf {
    let mut name = song.file_name().unwrap_or_default().to_os_string();
    name.push(".analysis.csv");
    song.with_file_name(name)
}

pub fn analyse_in_background(path: PathBuf) -> Receiver<Result<OfflineReport>> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("offline-analysis".into())
        .spawn(move || {
            let _ = tx.send(analyse_file(&path));
        })
        .expect("failed to spawn the offline analysis thread");
    rx
}

pub fn analyse_file(path: &Path) -> Result<OfflineReport> {
    let started = Instant::now();
    let samples = audio::song::load_analysis_signal(path)?;
    let csv_path = csv_path_for(path);
    let file = File::create(&csv_path)
        .with_context(|| format!("could not create {}", csv_path.display()))?;
    let mut out = BufWriter::new(file);
    let mut report = analyse_to_csv(&samples, &mut out)?;
    out.flush()?;
    report.csv_path = csv_path;
    report.elapsed_secs = started.elapsed().as_secs_f64();
    Ok(report)
}

pub const CSV_HEADER: &str = "time_s,sub,bass,low_mid,mid,high_mid,air,loudness_db,centroid_hz,\
onset,kick,snare,hat,bpm,tempo_locked,beat_phase,bar_phase,phrase_phase,beat_in_bar,\
bar_in_phrase,bar,building,build_amount,events";

/// Analyse 48 kHz mono `samples`, writing one CSV row per hop.
///
/// Band columns are the *fast* normalised levels (1.0 = typical). Onset columns
/// are empty, or hold the precise onset time (s) on the hop it was detected.
/// `events` lists `build@t` / `drop@t` / `section_change@t` on the hop they are
/// detected, where `t` is when the event happened.
pub fn analyse_to_csv(samples: &[f32], out: &mut impl Write) -> Result<OfflineReport> {
    let cfg = AnalysisConfig {
        sample_rate: audio::ANALYSIS_SAMPLE_RATE,
        ..Default::default()
    };
    let mut analyzer = Analyzer::new(cfg);
    writeln!(out, "{CSV_HEADER}")?;
    let mut bpms = Vec::new();
    let mut last = SignalFrame::default();
    let mut io_error = None;
    analyzer.push(samples, |f| {
        if io_error.is_none()
            && let Err(e) = write_row(out, f)
        {
            io_error = Some(e);
        }
        if f.rhythm.tempo_locked {
            bpms.push(f.rhythm.bpm);
        }
        last = f.clone();
    });
    if let Some(e) = io_error {
        return Err(e).context("could not write the CSV");
    }
    bpms.sort_by(f32::total_cmp);
    let r = &last.rhythm;
    let s = &last.structure;
    Ok(OfflineReport {
        csv_path: PathBuf::new(),
        duration_secs: samples.len() as f64 / audio::ANALYSIS_SAMPLE_RATE as f64,
        elapsed_secs: 0.0,
        median_bpm: bpms.get(bpms.len() / 2).copied(),
        onsets: r.onset.count,
        kicks: r.kick.count,
        snares: r.snare.count,
        hats: r.hat.count,
        builds: s.build.count,
        drops: s.drop.count,
        section_changes: s.section_change.count,
    })
}

fn write_row(out: &mut impl Write, f: &SignalFrame) -> std::io::Result<()> {
    let onset = |t: &signals::Trigger| {
        if t.fired {
            format!("{:.4}", t.last_time)
        } else {
            String::new()
        }
    };
    let r = &f.rhythm;
    let s = &f.structure;
    // "name@time": the event's own time, which can be a little before the hop
    // that detected it (a drop is confirmed one beat after it lands).
    let mut events = Vec::new();
    for (t, name) in [
        (&s.build, "build"),
        (&s.drop, "drop"),
        (&s.section_change, "section_change"),
    ] {
        if t.fired {
            events.push(format!("{name}@{:.3}", t.last_time));
        }
    }
    write!(out, "{:.4}", f.time)?;
    for b in Band::ALL {
        write!(out, ",{:.3}", f.band(b).fast)?;
    }
    writeln!(
        out,
        ",{:.1},{:.0},{},{},{},{},{:.2},{},{:.3},{:.3},{:.3},{},{},{},{},{:.2},{}",
        f.loudness.energy_db,
        f.centroid_hz,
        onset(&r.onset),
        onset(&r.kick),
        onset(&r.snare),
        onset(&r.hat),
        r.bpm,
        r.tempo_locked as u8,
        r.beat_phase,
        r.bar_phase,
        r.phrase_phase,
        r.beat_in_bar + 1,
        r.bar_in_phrase + 1,
        r.bar_count,
        s.building as u8,
        s.build_amount,
        events.join(";"),
    )
}

/// Show the file in Explorer / Finder.
pub fn reveal_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(Path::new(".")))
        .spawn();
    if let Err(e) = result {
        log::warn!("could not open the file manager: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_name_sits_next_to_the_song() {
        let p = csv_path_for(Path::new("/music/My Song.mp3"));
        assert_eq!(p, Path::new("/music/My Song.mp3.analysis.csv"));
    }

    #[test]
    fn writes_one_row_per_hop_with_matching_columns() {
        // 2 s of clicks at 120 BPM.
        let mut x = vec![0.0f32; 96_000];
        for k in 0..4 {
            for i in 0..200 {
                x[k * 24_000 + i] = if i % 2 == 0 { 0.8 } else { -0.8 };
            }
        }
        let mut buf = Vec::new();
        let report = analyse_to_csv(&x, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], CSV_HEADER);
        assert_eq!(lines.len() - 1, 96_000 / 512); // one row per hop
        let cols = CSV_HEADER.split(',').count();
        for l in &lines[1..] {
            assert_eq!(l.split(',').count(), cols, "{l}");
        }
        assert!(report.onsets >= 3, "{report:?}");
        assert!((report.duration_secs - 2.0).abs() < 1e-9);
    }
}
