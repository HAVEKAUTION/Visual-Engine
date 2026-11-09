//! Pure, deterministic DSP: spectrum, bands, auto-gain, smoothing, centroid
//! and loudness (maths spec items 1–4 and 9). Same samples in, same signals
//! out, live or offline.
//!
//! Onsets, tempo, beat phase and structure (items 5–8) arrive in M1 part 2.

pub mod analyzer;
pub mod bands;
pub mod config;
pub mod smoothing;

pub use analyzer::Analyzer;
pub use config::AnalysisConfig;
