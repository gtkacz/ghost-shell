//! The `audio.spectrum` topic: what the default output plays, as 32 bands 30 times a second.
//!
//! Only while some client wants the topic does a thread of its own capture the default sink's
//! monitor through PipeWire, as the node `ghostd-spectrum`; it is stopped as soon as no client
//! wants the topic. Without PipeWire the feed logs once each time the topic becomes wanted and
//! publishes nothing.

mod analyzer;
mod capture;

pub use analyzer::{BANDS, FRAMES_PER_S};
use ghost_proto::{SpectrumSnapshot, topics};
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;

use self::capture::Capture;
use super::{until_unwanted, until_wanted};
use crate::server::{Feed, FeedContext};

pub fn feed() -> Feed {
    Feed::new(topics::SPECTRUM, |mut context| async move {
        loop {
            until_wanted(&mut context.demand, topics::SPECTRUM).await;
            capture_while_wanted(&mut context).await;
        }
    })
}

/// Publishes every frame of one capture until the topic is no longer wanted, then stops it. A
/// capture that ends before then leaves the topic quiet until it is wanted again.
async fn capture_while_wanted(context: &mut FeedContext) {
    let mut capture = match Capture::start() {
        Ok(capture) => capture,
        Err(error) => {
            warn!(%error, "cannot start the audio capture thread; audio.spectrum publishes nothing");
            until_unwanted(&mut context.demand, topics::SPECTRUM).await;
            return;
        }
    };
    loop {
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::SPECTRUM) => break,
            frame = capture.frames.recv() => match frame {
                Ok(bands) => context.publisher.publish(
                    topics::SPECTRUM,
                    &SpectrumSnapshot {
                        bands: bands.to_vec(),
                    },
                ),
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => {
                    until_unwanted(&mut context.demand, topics::SPECTRUM).await;
                    break;
                }
            },
        }
    }
    capture.stop().await;
}
