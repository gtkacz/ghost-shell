//! The `audio.spectrum` topic: what the default output plays, as 32 bands 30 times a second.
//!
//! Only while some client wants the topic does a thread of its own capture the default sink's
//! monitor through PipeWire, as the node `ghostd-spectrum`; it is stopped as soon as no client
//! wants the topic. Without PipeWire the feed logs once each time the topic becomes wanted.
//!
//! The capture is passive, so it never keeps the sink awake: when nothing else plays, the sink
//! suspends and the frames simply stop. Once no frame has come for three frame periods, one dark
//! frame is published, so the bars go dark instead of freezing. Every capture also ends with a dark
//! frame, because the hub replays the last value to the next subscriber.

mod analyzer;
mod capture;

use std::pin::pin;
use std::time::Duration;

use analyzer::Bands;
pub use analyzer::{BANDS, FRAMES_PER_S};
use ghost_proto::{SpectrumSnapshot, topics};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::time::{Instant, sleep};
use tracing::warn;

use self::capture::Capture;
use super::{until_unwanted, until_wanted};
use crate::server::{Feed, FeedContext, Publisher};

const FRAME_PERIOD: Duration = Duration::from_secs(1).checked_div(FRAMES_PER_S).unwrap();
const STALL_FRAMES: u32 = 3;
/// How long without audio before the bands are shown dark.
const STALL: Duration = FRAME_PERIOD.checked_mul(STALL_FRAMES).unwrap();
const DARK: Bands = [0; BANDS];

/// Why [`publish_frames`] returned.
enum Stop {
    /// No client wants the topic any more.
    Released,
    /// The capture ended on its own.
    Ended,
}

pub fn feed() -> Feed {
    Feed::new(topics::SPECTRUM, |mut context| async move {
        loop {
            until_wanted(&mut context.demand, topics::SPECTRUM).await;
            capture_while_wanted(&mut context).await;
        }
    })
}

/// Runs one capture until the topic is no longer wanted, then stops it. A capture that ends before
/// then leaves the topic dark until it is wanted again.
async fn capture_while_wanted(context: &mut FeedContext) {
    let mut capture = match Capture::start() {
        Ok(capture) => capture,
        Err(error) => {
            warn!(%error, "cannot start the audio capture thread; audio.spectrum stays dark");
            until_unwanted(&mut context.demand, topics::SPECTRUM).await;
            return;
        }
    };
    if let Stop::Ended = publish_frames(&mut capture.frames, context).await {
        until_unwanted(&mut context.demand, topics::SPECTRUM).await;
    }
    capture.stop().await;
}

/// Publishes every frame from `frames` until the topic is released or the frames end, and one dark
/// frame after each [`STALL`] without a frame, counted from the start or the latest frame. Stopping
/// also publishes a dark frame.
async fn publish_frames(
    frames: &mut broadcast::Receiver<Bands>,
    context: &mut FeedContext,
) -> Stop {
    let mut stall = pin!(sleep(STALL));
    let mut is_dark = false;
    let stop = loop {
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::SPECTRUM) => break Stop::Released,
            frame = frames.recv() => match frame {
                Ok(bands) => {
                    publish(&context.publisher, &bands);
                    stall.as_mut().reset(Instant::now() + STALL);
                    is_dark = false;
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break Stop::Ended,
            },
            () = &mut stall, if !is_dark => {
                publish(&context.publisher, &DARK);
                is_dark = true;
            }
        }
    };
    publish(&context.publisher, &DARK);
    stop
}

fn publish(publisher: &Publisher, bands: &Bands) {
    publisher.publish(
        topics::SPECTRUM,
        &SpectrumSnapshot {
            bands: bands.to_vec(),
        },
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ghost_proto::ResolvedConfig;
    use serde_json::Value;
    use tokio::sync::{broadcast, mpsc, watch};
    use tokio::task::JoinHandle;
    use tokio::time::sleep;

    use super::*;
    use crate::server::Publisher;

    const LIT: Bands = [200; BANDS];
    const LONG_SILENCE: u32 = 10;

    /// `publish_frames` running on frames the test sends, with the topic wanted.
    struct Forwarding {
        frames: broadcast::Sender<Bands>,
        demand: watch::Sender<BTreeSet<String>>,
        published: mpsc::UnboundedReceiver<Value>,
        running: JoinHandle<Stop>,
    }

    fn forwarding() -> Forwarding {
        let (frames, mut receiver) = broadcast::channel(BANDS);
        let (published_tx, published) = mpsc::unbounded_channel();
        let (demand, wanted) = watch::channel(BTreeSet::from([topics::SPECTRUM.to_owned()]));
        let (_, config) = watch::channel(ResolvedConfig::default());
        let mut context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                assert_eq!(topic, topics::SPECTRUM);
                published_tx.send(data).unwrap();
            }),
            demand: wanted,
            config,
        };
        let running =
            tokio::spawn(async move { publish_frames(&mut receiver, &mut context).await });
        Forwarding {
            frames,
            demand,
            published,
            running,
        }
    }

    /// The bands of every snapshot published since the last call.
    fn published(published: &mut mpsc::UnboundedReceiver<Value>) -> Vec<Bands> {
        let mut snapshots = Vec::new();
        while let Ok(value) = published.try_recv() {
            let snapshot: SpectrumSnapshot = serde_json::from_value(value).unwrap();
            snapshots.push(snapshot.bands.try_into().unwrap());
        }
        snapshots
    }

    #[tokio::test(start_paused = true)]
    async fn a_stall_goes_dark_once_until_frames_resume() {
        let mut forwarding = forwarding();

        sleep(STALL * LONG_SILENCE).await;
        assert_eq!(
            published(&mut forwarding.published),
            [DARK],
            "a capture that starts silent"
        );

        for _ in 0..LONG_SILENCE {
            forwarding.frames.send(LIT).unwrap();
            sleep(FRAME_PERIOD).await;
        }
        assert_eq!(
            published(&mut forwarding.published),
            [LIT; LONG_SILENCE as usize]
        );

        sleep(STALL * LONG_SILENCE).await;
        assert_eq!(published(&mut forwarding.published), [DARK]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_release_publishes_a_dark_frame() {
        let mut forwarding = forwarding();
        forwarding.frames.send(LIT).unwrap();
        sleep(FRAME_PERIOD).await;

        forwarding.demand.send_replace(BTreeSet::new());
        let stop = forwarding.running.await.unwrap();

        assert!(matches!(stop, Stop::Released));
        assert_eq!(published(&mut forwarding.published), [LIT, DARK]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_capture_that_ends_publishes_a_dark_frame() {
        let mut forwarding = forwarding();
        forwarding.frames.send(LIT).unwrap();
        sleep(FRAME_PERIOD).await;

        drop(forwarding.frames);
        let stop = forwarding.running.await.unwrap();

        assert!(matches!(stop, Stop::Ended));
        assert_eq!(published(&mut forwarding.published), [LIT, DARK]);
    }
}
