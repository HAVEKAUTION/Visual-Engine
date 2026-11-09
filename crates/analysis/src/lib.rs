//! Pure, deterministic DSP (maths spec items 1–9): spectrum, bands, auto-gain,
//! smoothing, onsets, tempo, beat clock, structure, centroid and loudness.
//! Same samples in, same signals out, live or offline.

pub mod analyzer;
pub mod bands;
pub mod beat;
pub mod config;
pub mod onset;
pub mod smoothing;
pub mod structure;
pub mod tempo;

pub use analyzer::Analyzer;
pub use config::AnalysisConfig;
