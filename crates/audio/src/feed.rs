//! The lock-free hand-off from audio callbacks to the analysis thread.
//!
//! Each time the audio source changes, a fresh SPSC ring buffer is created: the
//! [`Producer`] moves into the new audio callback and the [`Consumer`] is sent to
//! the analysis thread (see [`crate::AudioEngine::new`]). The ring carries mono
//! `f32` samples at [`crate::ANALYSIS_SAMPLE_RATE`].

use rtrb::{Consumer, Producer, RingBuffer};

/// Ring capacity: 4 s of 48 kHz audio, far more than the analysis thread
/// ever lags behind. If it ever fills, new samples are dropped (never blocks).
pub const FEED_CAPACITY: usize = crate::ANALYSIS_SAMPLE_RATE as usize * 4;

pub type FeedProducer = Producer<f32>;
pub type FeedConsumer = Consumer<f32>;

pub fn new_feed() -> (FeedProducer, FeedConsumer) {
    RingBuffer::new(FEED_CAPACITY)
}

/// Push as many samples as fit. Real-time safe: no allocation, no locks.
#[inline]
pub fn push_slice(producer: &mut FeedProducer, data: &[f32]) {
    let n = data.len().min(producer.slots());
    if n == 0 {
        return;
    }
    if let Ok(chunk) = producer.write_chunk_uninit(n) {
        chunk.fill_from_iter(data[..n].iter().copied());
    }
}

/// Push `count` zeros (as many as fit). Real-time safe.
#[inline]
pub fn push_silence(producer: &mut FeedProducer, count: usize) {
    let n = count.min(producer.slots());
    if n == 0 {
        return;
    }
    if let Ok(chunk) = producer.write_chunk_uninit(n) {
        chunk.fill_from_iter(std::iter::repeat_n(0.0, n));
    }
}

/// Push a single sample if there is room. Real-time safe.
#[inline]
pub fn push_one(producer: &mut FeedProducer, sample: f32) {
    let _ = producer.push(sample);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushes_and_drops_when_full_without_blocking() {
        let (mut p, mut c) = RingBuffer::new(4);
        push_slice(&mut p, &[1.0, 2.0, 3.0]);
        push_silence(&mut p, 5); // only one slot left
        push_one(&mut p, 9.0); // full: dropped
        let chunk = c.read_chunk(c.slots()).unwrap();
        let (a, b) = chunk.as_slices();
        let got: Vec<f32> = a.iter().chain(b).copied().collect();
        assert_eq!(got, vec![1.0, 2.0, 3.0, 0.0]);
    }
}
