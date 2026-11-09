//! Audio input sources and playback.
//!
//! Every source (a song file or a live input device) delivers **mono 48 kHz**
//! samples to the analysis thread through a lock-free ring buffer (see
//! [`feed`]). The audio callbacks follow the real-time rules: no allocation, no
//! locks, no I/O and no logging.

pub mod decode;
pub mod feed;
pub mod input;
pub mod player;
pub mod resample;
pub mod song;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use anyhow::Result;

pub use feed::{FeedConsumer, FeedProducer};
pub use input::{InputDevice, LiveInput, list_input_devices};
pub use player::{FilePlayer, default_output_rate};
pub use song::Song;

/// Sample rate of the signal handed to the analysis thread.
pub const ANALYSIS_SAMPLE_RATE: u32 = 48_000;

/// The currently active audio source.
#[derive(Default)]
pub enum Source {
    #[default]
    None,
    File(FilePlayer),
    Live(LiveInput),
}

/// Owns the active source and creates a new analysis feed for each one.
pub struct AudioEngine {
    feed_tx: Sender<FeedConsumer>,
    source: Source,
}

impl AudioEngine {
    /// Returns the engine plus the receiving end on which the analysis thread
    /// gets a new [`FeedConsumer`] every time the source changes.
    pub fn new() -> (Self, Receiver<FeedConsumer>) {
        let (feed_tx, feed_rx) = channel();
        (
            Self {
                feed_tx,
                source: Source::None,
            },
            feed_rx,
        )
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    fn new_feed(&self) -> FeedProducer {
        let (producer, consumer) = feed::new_feed();
        // If the analysis thread has gone away there is nobody to feed; ignore.
        let _ = self.feed_tx.send(consumer);
        producer
    }

    /// Stop whatever is playing or capturing.
    pub fn stop(&mut self) {
        self.source = Source::None;
    }

    /// Switch to playing `song` (paused at the start).
    pub fn start_song(&mut self, song: Song, volume: f32) -> Result<()> {
        self.stop();
        let player = FilePlayer::start(song, self.new_feed(), volume)?;
        self.source = Source::File(player);
        Ok(())
    }

    /// Switch to capturing from a live input device.
    pub fn start_live(&mut self, device: &InputDevice) -> Result<()> {
        self.stop();
        let live = LiveInput::start(device, self.new_feed())?;
        self.source = Source::Live(live);
        Ok(())
    }
}

/// Decode and prepare a song on a background thread. The result arrives on
/// the returned channel.
pub fn load_song_in_background(path: PathBuf) -> Receiver<Result<Song>> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("song-loader".into())
        .spawn(move || {
            let result = default_output_rate().and_then(|rate| Song::load(&path, rate));
            let _ = tx.send(result);
        })
        .expect("failed to spawn the song loader thread");
    rx
}
