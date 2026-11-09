//! Stand-in for the analysis thread (Milestone 1 replaces the body).
//!
//! It owns the consumer end of the audio feed, keeps the last couple of seconds
//! of 48 kHz mono audio and publishes a waveform snapshot through a triple
//! buffer, so the UI/render thread always reads the newest data without ever
//! waiting on this thread.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use audio::{ANALYSIS_SAMPLE_RATE, FeedConsumer};

/// How much audio the waveform shows.
pub const HISTORY_SECS: f32 = 2.0;
/// Horizontal resolution of the published waveform.
pub const COLUMNS: usize = 1024;

const PUBLISH_INTERVAL: Duration = Duration::from_millis(15);
const POLL_INTERVAL: Duration = Duration::from_millis(4);

/// What the UI reads each frame.
#[derive(Clone, Debug, Default)]
pub struct WaveformSnapshot {
    /// `[min, max]` per column, oldest on the left.
    pub columns: Vec<[f32; 2]>,
    /// Largest absolute sample value in the window.
    pub peak: f32,
    /// Total samples received from the current source.
    pub samples_received: u64,
    /// A source is connected and still producing.
    pub connected: bool,
}

/// Fixed-size ring holding the most recent samples.
pub struct History {
    buf: Vec<f32>,
    write: usize,
}

impl History {
    pub fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            write: 0,
        }
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.write = 0;
    }

    pub fn extend(&mut self, samples: &[f32]) {
        for &s in samples {
            self.buf[self.write] = s;
            self.write = (self.write + 1) % self.buf.len();
        }
    }

    /// Sample `j` in chronological order (0 = oldest).
    fn get(&self, j: usize) -> f32 {
        self.buf[(self.write + j) % self.buf.len()]
    }

    /// Min/max envelope with `columns` columns, oldest first. Returns the peak.
    pub fn envelope(&self, columns: usize, out: &mut Vec<[f32; 2]>) -> f32 {
        let n = self.buf.len();
        out.clear();
        let mut peak = 0.0f32;
        for c in 0..columns {
            let a = c * n / columns;
            let b = ((c + 1) * n / columns).max(a + 1).min(n);
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for j in a..b {
                let s = self.get(j);
                lo = lo.min(s);
                hi = hi.max(s);
            }
            peak = peak.max(lo.abs()).max(hi.abs());
            out.push([lo, hi]);
        }
        peak
    }
}

pub fn spawn(
    feeds: Receiver<FeedConsumer>,
    mut output: triple_buffer::Input<WaveformSnapshot>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("analysis".into())
        .spawn(move || {
            let mut history = History::new((ANALYSIS_SAMPLE_RATE as f32 * HISTORY_SECS) as usize);
            let mut consumer: Option<FeedConsumer> = None;
            let mut received: u64 = 0;
            let mut last_publish = Instant::now();

            loop {
                // A new source replaces the old feed.
                match feeds.try_recv() {
                    Ok(c) => {
                        consumer = Some(c);
                        history.clear();
                        received = 0;
                    }
                    Err(TryRecvError::Disconnected) => return, // app is closing
                    Err(TryRecvError::Empty) => {}
                }

                if let Some(c) = consumer.as_mut() {
                    let n = c.slots();
                    if n > 0
                        && let Ok(chunk) = c.read_chunk(n)
                    {
                        let (a, b) = chunk.as_slices();
                        history.extend(a);
                        history.extend(b);
                        chunk.commit_all();
                        received += n as u64;
                    }
                }

                if last_publish.elapsed() >= PUBLISH_INTERVAL {
                    last_publish = Instant::now();
                    let snap = output.input_buffer_mut();
                    snap.peak = history.envelope(COLUMNS, &mut snap.columns);
                    snap.samples_received = received;
                    snap.connected = consumer.as_ref().is_some_and(|c| !c.is_abandoned());
                    output.publish();
                }

                std::thread::sleep(POLL_INTERVAL);
            }
        })
        .expect("failed to spawn the analysis thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_keeps_only_the_newest_samples_in_order() {
        let mut h = History::new(4);
        h.extend(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let got: Vec<f32> = (0..4).map(|j| h.get(j)).collect();
        assert_eq!(got, vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn envelope_reports_min_max_per_column() {
        let mut h = History::new(8);
        h.extend(&[0.1, -0.2, 0.3, -0.4, 0.5, -0.6, 0.7, -0.8]);
        let mut cols = Vec::new();
        let peak = h.envelope(4, &mut cols);
        assert_eq!(
            cols,
            vec![[-0.2, 0.1], [-0.4, 0.3], [-0.6, 0.5], [-0.8, 0.7]]
        );
        assert_eq!(peak, 0.8);
    }

    #[test]
    fn thread_publishes_what_the_feed_delivers() {
        let (tx, rx) = std::sync::mpsc::channel();
        let (input, mut output) = triple_buffer::triple_buffer(&WaveformSnapshot::default());
        let handle = spawn(rx, input);

        let (mut producer, consumer) = audio::feed::new_feed();
        tx.send(consumer).unwrap();
        audio::feed::push_slice(&mut producer, &[0.5; 1000]);

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snap = output.read();
            if snap.samples_received == 1000 {
                assert!((snap.peak - 0.5).abs() < 1e-6);
                assert_eq!(snap.columns.len(), COLUMNS);
                assert!(snap.connected);
                break;
            }
            assert!(Instant::now() < deadline, "snapshot never arrived");
            std::thread::sleep(Duration::from_millis(5));
        }

        drop(tx);
        handle.join().unwrap();
    }
}
