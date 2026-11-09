//! End-to-end rhythm and structure tests on synthetic music.

use analysis::{AnalysisConfig, Analyzer};
use signals::SignalFrame;
use std::f64::consts::TAU;

const FS: f64 = 48_000.0;

/// Deterministic white noise in [-1, 1].
struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

/// A synthetic kick drum `t` seconds after it starts.
fn kick(t: f64) -> f32 {
    const LEN: f64 = 0.4;
    if !(0.0..LEN).contains(&t) {
        return 0.0;
    }
    // Pitch sweeps 150 → 45 Hz; phase is the integral of the frequency.
    let phase = TAU * (45.0 * t + 105.0 / 30.0 * (1.0 - (-30.0 * t).exp()));
    // Exponential decay with a smooth fade to zero (no click at the end).
    let fade = (1.0 - t / LEN).powi(2);
    (phase.sin() * (-9.0 * t).exp() * fade) as f32
}

struct Pattern {
    bpm: f64,
    secs: f64,
    /// Add closed hats on every 8th note.
    hats: bool,
    /// Seconds of silence appended.
    tail_silence: f64,
    gain: f32,
}

/// Render the pattern; returns samples and the true kick times.
fn render(p: &Pattern) -> (Vec<f32>, Vec<f64>) {
    let beat = 60.0 / p.bpm;
    let n = (p.secs * FS) as usize;
    let mut noise = Noise(7);
    // Hats are high-passed noise (two first differences: 12 dB/octave), like
    // a real hi-hat, so they don't leak into the kick band.
    let (mut d1, mut d2) = (0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n + (p.tail_silence * FS) as usize);
    let mut kicks = Vec::new();
    let mut k = 0.0;
    while k < p.secs {
        kicks.push(k);
        k += beat;
    }
    for i in 0..n {
        let t = i as f64 / FS;
        let b = t / beat;
        let since_kick = (b - b.floor()) * beat;
        let mut x = 0.8 * kick(since_kick);
        if p.hats {
            let since_hat = ((b * 2.0) - (b * 2.0).floor()) * beat / 2.0;
            let w = noise.next();
            let h1 = w - d1;
            let h2 = h1 - d2;
            d1 = w;
            d2 = h1;
            x += 0.15 * h2 * (-since_hat as f32 * 60.0).exp();
        }
        out.push(x * p.gain);
    }
    out.extend(std::iter::repeat_n(0.0, (p.tail_silence * FS) as usize));
    (out, kicks)
}

/// Run the analyser and collect every frame.
fn analyse(x: &[f32]) -> Vec<SignalFrame> {
    let mut a = Analyzer::new(AnalysisConfig::default());
    let mut frames = Vec::new();
    a.push(x, |f| frames.push(f.clone()));
    frames
}

/// Times of every distinct `kick` trigger.
fn kick_times(frames: &[SignalFrame]) -> Vec<f64> {
    frames
        .iter()
        .filter(|f| f.rhythm.kick.fired)
        .map(|f| f.rhythm.kick.last_time)
        .collect()
}

fn pattern_128() -> Pattern {
    Pattern {
        bpm: 128.0,
        secs: 20.0,
        hats: false,
        tail_silence: 0.0,
        gain: 0.5,
    }
}

#[test]
fn tempo_of_128_bpm_kicks_is_128_not_64_or_256() {
    let (x, _) = render(&pattern_128());
    let frames = analyse(&x);
    let last = frames.last().unwrap();
    assert!(last.rhythm.tempo_locked);
    assert!(
        (last.rhythm.bpm - 128.0).abs() <= 1.0,
        "bpm {}",
        last.rhythm.bpm
    );
    // ...and it stayed there for the whole second half.
    for f in &frames[frames.len() / 2..] {
        assert!((f.rhythm.bpm - 128.0).abs() <= 1.0, "bpm {}", f.rhythm.bpm);
    }
}

#[test]
fn tempo_survives_eighth_note_hats() {
    let (x, _) = render(&Pattern {
        hats: true,
        ..pattern_128()
    });
    let last = analyse(&x).pop().unwrap();
    assert!(
        (last.rhythm.bpm - 128.0).abs() <= 1.0,
        "bpm {}",
        last.rhythm.bpm
    );
}

#[test]
fn kick_onsets_land_within_20ms_of_the_kicks() {
    for hats in [false, true] {
        let (x, truth) = render(&Pattern {
            hats,
            ..pattern_128()
        });
        // The first second is the detector's warm-up (its median window is
        // still filling, and the kick at t = 0 has no earlier frame to differ
        // from), so judge from 1 s on.
        let found: Vec<f64> = kick_times(&analyse(&x))
            .into_iter()
            .filter(|&t| t > 1.0)
            .collect();
        let truth: Vec<f64> = truth.into_iter().filter(|&t| t > 1.0).collect();
        let mut errors = Vec::new();
        for &t in &truth {
            let nearest = found
                .iter()
                .map(|&f| f - t)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))
                .unwrap();
            errors.push(nearest);
        }
        let worst = errors.iter().fold(0.0f64, |m, e| m.max(e.abs()));
        let mean = errors.iter().sum::<f64>() / errors.len() as f64;
        println!(
            "hats={hats}: {} kicks, {} onsets, mean error {:+.1} ms, worst {:.1} ms",
            truth.len(),
            found.len(),
            mean * 1000.0,
            worst * 1000.0
        );
        assert!(
            worst <= 0.020,
            "worst kick timing error {:.1} ms",
            worst * 1e3
        );
        // One onset per kick: no misses, no doubles.
        assert_eq!(found.len(), truth.len());
    }
}

#[test]
fn beat_phase_locks_onto_the_kicks() {
    let (x, truth) = render(&pattern_128());
    let frames = analyse(&x);
    // After 10 s, the frame nearest each kick should be near phase 0.
    for &t in truth.iter().filter(|&&t| t > 10.0) {
        let f = frames
            .iter()
            .min_by(|a, b| (a.time - t).abs().total_cmp(&(b.time - t).abs()))
            .unwrap();
        // Phase advances ~0.023 per hop at 128 BPM; allow one hop either side.
        let p = f.rhythm.beat_phase;
        let dist = p.min(1.0 - p);
        assert!(dist < 0.06, "phase {p} at kick {t:.3}");
    }
}

#[test]
fn silence_has_no_onsets_and_holds_bpm() {
    // Pure silence: nothing fires, tempo never locks.
    let frames = analyse(&vec![0.0; 10 * FS as usize]);
    let last = frames.last().unwrap();
    assert_eq!(last.rhythm.onset.count, 0);
    assert_eq!(last.rhythm.kick.count, 0);
    assert!(!last.rhythm.tempo_locked);

    // Music then 10 s of silence: no onsets in the silence, BPM held.
    let (x, _) = render(&Pattern {
        tail_silence: 10.0,
        ..pattern_128()
    });
    let frames = analyse(&x);
    let in_silence: Vec<_> = frames
        .iter()
        .filter(|f| f.time > 20.5 && f.rhythm.onset.fired)
        .collect();
    assert!(in_silence.is_empty(), "{} false onsets", in_silence.len());
    let last = frames.last().unwrap();
    assert!(
        (last.rhythm.bpm - 128.0).abs() <= 1.0,
        "bpm {}",
        last.rhythm.bpm
    );
    assert!(last.rhythm.tempo_locked);
}

#[test]
fn quiet_music_still_finds_onsets_and_tempo() {
    let (x, truth) = render(&Pattern {
        gain: 0.02,
        ..pattern_128()
    });
    let frames = analyse(&x);
    let bpm = frames.last().unwrap().rhythm.bpm;
    assert!((bpm - 128.0).abs() <= 1.0, "bpm {bpm}");
    let found = kick_times(&frames).iter().filter(|&&t| t > 1.0).count();
    assert_eq!(found, truth.iter().filter(|&&t| t > 1.0).count());
}

#[test]
fn bars_and_phrases_count_up() {
    let (x, _) = render(&pattern_128());
    let last = analyse(&x).pop().unwrap();
    // 20 s at 128 BPM ≈ 42.7 beats ≈ 10.7 bars (the grid starts at the first lock).
    assert!(last.rhythm.bar_count >= 9, "{}", last.rhythm.bar_count);
    assert!(last.rhythm.bar_in_phrase < 8);
    assert!((0.0..1.0).contains(&last.rhythm.phrase_phase));
}

/// A tiny synthetic track at 128 BPM: `groove` bars, then `build` bars (rising
/// noise riser, hats doubling up), then `drop` bars (kick + heavy sub bass).
/// Returns samples and the time the drop starts.
fn render_song(groove: u32, build: u32, drop: u32) -> (Vec<f32>, f64) {
    let bpm = 128.0;
    let beat = 60.0 / bpm;
    let bar = 4.0 * beat;
    let t_build = groove as f64 * bar;
    let t_drop = t_build + build as f64 * bar;
    let total = t_drop + drop as f64 * bar;
    let mut noise = Noise(11);
    let (mut d1, mut d2, mut r1) = (0.0f32, 0.0f32, 0.0f32);
    let n = (total * FS) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / FS;
        let b = t / beat;
        let since_kick = (b - b.floor()) * beat;
        let w = noise.next();
        let h1 = w - d1;
        let h2 = h1 - d2;
        d1 = w;
        d2 = h1;
        // Hats: 8ths, or 16ths in the second half of the build.
        let sixteenths = t >= t_build + (t_drop - t_build) / 2.0 && t < t_drop;
        let div = if sixteenths { 4.0 } else { 2.0 };
        let since_hat = ((b * div) - (b * div).floor()) * beat / div;
        let hat = 0.15 * h2 * (-since_hat as f32 * 60.0).exp();
        let mut x = 0.5 * kick(since_kick) + hat;
        if t >= t_build && t < t_drop {
            // Riser: high-passed noise swelling over the build.
            let ramp = ((t - t_build) / (t_drop - t_build)) as f32;
            let r = w - r1;
            r1 = w;
            x += (0.02 + 0.4 * ramp * ramp) * r;
        }
        if t >= t_drop {
            x += 0.3 * kick(since_kick) + 0.45 * (TAU * 55.0 * t).sin() as f32;
        }
        out.push(0.5 * x);
    }
    (out, t_drop)
}

#[test]
fn build_then_drop_is_detected() {
    let (x, t_drop) = render_song(8, 8, 8);
    let frames = analyse(&x);
    let last = frames.last().unwrap();
    let s = &last.structure;
    let beat = 60.0 / 128.0;

    // A build was flagged during the build section.
    let builds: Vec<f64> = frames
        .iter()
        .filter(|f| f.structure.build.fired)
        .map(|f| f.time)
        .collect();
    println!("builds at {builds:?}");
    assert!(
        builds
            .iter()
            .any(|&t| t > t_drop - 8.0 * 4.0 * beat && t <= t_drop + beat),
        "no build before the drop: {builds:?}"
    );

    // Exactly one drop, on the right beat.
    assert_eq!(s.drop.count, 1, "drops: {}", s.drop.count);
    println!(
        "drop at {:.3} (true {t_drop:.3}), ratio {:.1}",
        s.drop.last_time, s.drop.strength
    );
    assert!((s.drop.last_time - t_drop).abs() < 0.5 * beat);

    // The drop starts a phrase: shortly after it we are in bar 1, beat 1.
    // (Checked just after beat 2: a drop is confirmed one beat after it lands.)
    let f = frames
        .iter()
        .find(|f| f.time > t_drop + beat + 0.05)
        .unwrap();
    assert_eq!(f.rhythm.bar_in_phrase, 0);
    assert_eq!(f.rhythm.beat_in_bar, 1);
    // No new "build" straight after the drop (its loudness jump isn't a build).
    assert!(
        builds
            .iter()
            .all(|&t| t < t_drop || t > t_drop + 8.0 * 4.0 * beat),
        "build flagged right after the drop: {builds:?}"
    );

    // The sound changed a lot: at least one section change from the build on.
    let sections: Vec<f64> = frames
        .iter()
        .filter(|f| f.structure.section_change.fired)
        .map(|f| f.time)
        .collect();
    println!("section changes at {sections:?}");
    assert!(sections.iter().any(|&t| t > t_drop - 8.0 * 4.0 * beat));
}

#[test]
fn steady_groove_has_no_structure_events() {
    let (x, _) = render_song(32, 0, 0);
    let last = analyse(&x).pop().unwrap();
    let s = &last.structure;
    assert_eq!(s.build.count, 0, "false builds");
    assert_eq!(s.drop.count, 0, "false drops");
    assert_eq!(s.section_change.count, 0, "false section changes");
}
