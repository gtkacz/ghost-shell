//! The `audio.spectrum` topic: what the default output plays, as 32 bands 30 times a second.
//!
//! Only while some client wants the topic does a thread of its own capture the default sink's
//! monitor through PipeWire, as the node `ghostd-spectrum`; it is stopped as soon as no client
//! wants the topic. Without PipeWire the feed logs once each time the topic becomes wanted, and
//! retries with a backoff while the topic stays wanted.
//!
//! The capture is passive, so it never keeps the sink awake: when nothing else plays, the sink
//! suspends and the frames simply stop. Each frame holds the bands for at least three frame
//! periods, and for two buffers when PipeWire's buffers are longer than that. When the hold runs
//! out with no frame after it, one dark frame is published, so the bars go dark instead of
//! freezing. Every capture also ends with a dark frame, because the hub replays the last value to
//! the next subscriber.

mod analyzer;
mod capture;

use std::io;
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
const MIN_HOLD_FRAMES: u32 = 3;
/// The least time a frame holds the bands before they are shown dark.
const MIN_HOLD: Duration = FRAME_PERIOD.checked_mul(MIN_HOLD_FRAMES).unwrap();
const DARK: Bands = [0; BANDS];
/// The wait before the first retry after a capture ends, and again after any capture PipeWire took.
const RETRY_FIRST: Duration = Duration::from_secs(1);
/// The longest wait between retries, so a PipeWire that stays down logs at most twice a minute.
const RETRY_CAP: Duration = Duration::from_secs(30);

/// One frame of the spectrum, and how long it holds the bands if no frame follows it.
#[derive(Debug, Clone, Copy)]
struct Frame {
    bands: Bands,
    hold: Duration,
}

/// Why [`publish_frames`] returned.
enum Stop {
    /// No client wants the topic any more.
    Released,
    /// The capture ended on its own.
    Ended,
}

pub fn feed() -> Feed {
    Feed::new(topics::SPECTRUM, |context| run(context, Capture::start))
}

/// Each time the topic becomes wanted, captures with `start` until it is released.
async fn run(mut context: FeedContext, mut start: impl FnMut() -> io::Result<Capture>) {
    loop {
        until_wanted(&mut context.demand, topics::SPECTRUM).await;
        capture_while_wanted(&mut context, &mut start).await;
    }
}

/// Captures with `start` until the topic is no longer wanted, then stops the capture. A capture
/// that ends or fails to start before then is retried after [`RETRY_FIRST`], doubling up to
/// [`RETRY_CAP`]; one that PipeWire took starts the waits over.
async fn capture_while_wanted(
    context: &mut FeedContext,
    start: &mut impl FnMut() -> io::Result<Capture>,
) {
    let mut delay = RETRY_FIRST;
    loop {
        let connected = match start() {
            Ok(mut capture) => {
                let stop = publish_frames(&mut capture.frames, context).await;
                let connected = capture.connected();
                capture.stop().await;
                if let Stop::Released = stop {
                    return;
                }
                connected
            }
            Err(error) => {
                warn!(%error, "cannot start the audio capture thread; audio.spectrum stays dark");
                false
            }
        };
        if connected {
            delay = RETRY_FIRST;
        }
        tokio::select! {
            () = until_unwanted(&mut context.demand, topics::SPECTRUM) => return,
            () = sleep(delay) => {}
        }
        delay = (delay * 2).min(RETRY_CAP);
    }
}

/// Publishes every frame from `frames` until the topic is released or the frames end, and one dark
/// frame each time the latest frame's hold runs out, or [`MIN_HOLD`] passes from the start with no
/// frame. Stopping also publishes a dark frame.
async fn publish_frames(
    frames: &mut broadcast::Receiver<Frame>,
    context: &mut FeedContext,
) -> Stop {
    let mut stall = pin!(sleep(MIN_HOLD));
    let mut is_dark = false;
    let stop = loop {
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::SPECTRUM) => break Stop::Released,
            frame = frames.recv() => match frame {
                Ok(frame) => {
                    publish(&context.publisher, &frame.bands);
                    stall.as_mut().reset(Instant::now() + frame.hold);
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
    use std::sync::{Arc, Mutex};

    use ghost_proto::ResolvedConfig;
    use serde_json::Value;
    use tokio::sync::{broadcast, mpsc, watch};
    use tokio::task::JoinHandle;
    use tokio::time::sleep;

    use super::*;
    use crate::server::Publisher;

    const LIT: Bands = [200; BANDS];
    const LIT_FRAME: Frame = Frame {
        bands: LIT,
        hold: MIN_HOLD,
    };
    const LONG_SILENCE: u32 = 10;
    // An 8192-frame quantum at 48 kHz: buffers 170 ms apart, each holding its frames for two.
    const LONG_BUFFER_GAP: Duration = Duration::from_millis(170);
    const LONG_BUFFER_HOLD: Duration = Duration::from_millis(341);

    /// `publish_frames` running on frames the test sends, with the topic wanted.
    struct Forwarding {
        frames: broadcast::Sender<Frame>,
        demand: watch::Sender<BTreeSet<String>>,
        published: mpsc::UnboundedReceiver<Value>,
        running: JoinHandle<Stop>,
    }

    /// A feed context whose demand starts with the topic `wanted` or not, and the ends that drive the
    /// demand and read what is published.
    fn context(
        wanted: bool,
    ) -> (
        FeedContext,
        watch::Sender<BTreeSet<String>>,
        mpsc::UnboundedReceiver<Value>,
    ) {
        let (published_tx, published) = mpsc::unbounded_channel();
        let topics = if wanted {
            BTreeSet::from([topics::SPECTRUM.to_owned()])
        } else {
            BTreeSet::new()
        };
        let (demand, receiver) = watch::channel(topics);
        let (_, config) = watch::channel(ResolvedConfig::default());
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                assert_eq!(topic, topics::SPECTRUM);
                published_tx.send(data).unwrap();
            }),
            demand: receiver,
            config,
            calls: tokio::sync::broadcast::channel(64).1,
            topics: tokio::sync::broadcast::channel(1).1,
        };
        (context, demand, published)
    }

    fn forwarding() -> Forwarding {
        let (frames, mut receiver) = broadcast::channel(BANDS);
        let (mut context, demand, published) = context(true);
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

        sleep(MIN_HOLD * LONG_SILENCE).await;
        assert_eq!(
            published(&mut forwarding.published),
            [DARK],
            "a capture that starts silent"
        );

        for _ in 0..LONG_SILENCE {
            forwarding.frames.send(LIT_FRAME).unwrap();
            sleep(FRAME_PERIOD).await;
        }
        assert_eq!(
            published(&mut forwarding.published),
            [LIT; LONG_SILENCE as usize]
        );

        sleep(MIN_HOLD * LONG_SILENCE).await;
        assert_eq!(published(&mut forwarding.published), [DARK]);
    }

    #[tokio::test(start_paused = true)]
    async fn frames_from_long_buffers_stay_lit_between_buffers() {
        let mut forwarding = forwarding();

        for _ in 0..LONG_SILENCE {
            forwarding
                .frames
                .send(Frame {
                    bands: LIT,
                    hold: LONG_BUFFER_HOLD,
                })
                .unwrap();
            sleep(LONG_BUFFER_GAP).await;
        }
        assert_eq!(
            published(&mut forwarding.published),
            [LIT; LONG_SILENCE as usize]
        );

        sleep(LONG_BUFFER_HOLD * LONG_SILENCE).await;
        assert_eq!(published(&mut forwarding.published), [DARK]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_release_publishes_a_dark_frame() {
        let mut forwarding = forwarding();
        forwarding.frames.send(LIT_FRAME).unwrap();
        sleep(FRAME_PERIOD).await;

        forwarding.demand.send_replace(BTreeSet::new());
        let stop = forwarding.running.await.unwrap();

        assert!(matches!(stop, Stop::Released));
        assert_eq!(published(&mut forwarding.published), [LIT, DARK]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_capture_that_ends_publishes_a_dark_frame() {
        let mut forwarding = forwarding();
        forwarding.frames.send(LIT_FRAME).unwrap();
        sleep(FRAME_PERIOD).await;

        drop(forwarding.frames);
        let stop = forwarding.running.await.unwrap();

        assert!(matches!(stop, Stop::Ended));
        assert_eq!(published(&mut forwarding.published), [LIT, DARK]);
    }

    const RETRY_DELAYS: [Duration; 7] = [
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(4),
        Duration::from_secs(8),
        Duration::from_secs(16),
        Duration::from_secs(30),
        Duration::from_secs(30),
    ];
    const SLACK: Duration = Duration::from_millis(1);

    #[derive(Clone, Copy)]
    enum Outcome {
        StartFails,
        EndsUnconnected,
        EndsConnected,
    }

    /// `run` on a connector the test controls: attempt `n` gets `outcome(n)`, and every start is
    /// recorded.
    struct Retrying {
        starts: Arc<Mutex<Vec<Instant>>>,
        demand: watch::Sender<BTreeSet<String>>,
        _published: mpsc::UnboundedReceiver<Value>,
        running: JoinHandle<()>,
    }

    fn retrying(wanted: bool, outcome: impl Fn(usize) -> Outcome + Send + 'static) -> Retrying {
        let starts = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&starts);
        let (context, demand, _published) = context(wanted);
        let start = move || {
            let attempt = {
                let mut starts = record.lock().unwrap();
                starts.push(Instant::now());
                starts.len() - 1
            };
            match outcome(attempt) {
                Outcome::StartFails => Err(io::Error::other("no thread")),
                ended => {
                    // The sender is dropped: the capture has ended.
                    let (_, frames) = broadcast::channel(1);
                    Ok(Capture::detached(
                        frames,
                        matches!(ended, Outcome::EndsConnected),
                    ))
                }
            }
        };
        Retrying {
            starts,
            demand,
            _published,
            running: tokio::spawn(run(context, start)),
        }
    }

    impl Retrying {
        fn count(&self) -> usize {
            self.starts.lock().unwrap().len()
        }

        fn gaps(&self) -> Vec<Duration> {
            self.starts
                .lock()
                .unwrap()
                .windows(2)
                .map(|pair| pair[1] - pair[0])
                .collect()
        }
    }

    #[tokio::test(start_paused = true)]
    async fn retries_start_after_one_second_and_double_to_the_cap() {
        for outcome in [Outcome::EndsUnconnected, Outcome::StartFails] {
            let run = retrying(true, move |_| outcome);
            sleep(RETRY_DELAYS.iter().sum::<Duration>() + SLACK).await;
            assert_eq!(run.gaps(), RETRY_DELAYS);
            run.running.abort();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_successful_connection_resets_the_delay() {
        let run = retrying(true, |attempt| {
            if attempt == 3 {
                Outcome::EndsConnected
            } else {
                Outcome::EndsUnconnected
            }
        });
        sleep(Duration::from_secs(1 + 2 + 4 + 1 + 2) + SLACK).await;
        // Attempt 3 connected before it ended.
        assert_eq!(run.gaps(), [1, 2, 4, 1, 2].map(Duration::from_secs));
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_is_attempted_while_unwanted() {
        let run = retrying(false, |_| Outcome::EndsUnconnected);
        sleep(Duration::from_secs(120)).await;
        assert_eq!(run.count(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_release_cancels_the_pending_retry_and_a_later_want_starts_over() {
        let run = retrying(true, |_| Outcome::EndsUnconnected);
        // Attempts at 0, 1 and 3 s; the next is due at 7 s.
        sleep(Duration::from_secs(1 + 2) + SLACK).await;
        assert_eq!(run.count(), 3);

        run.demand.send_replace(BTreeSet::new());
        sleep(Duration::from_secs(120)).await;
        assert_eq!(run.count(), 3);

        run.demand
            .send_replace(BTreeSet::from([topics::SPECTRUM.to_owned()]));
        sleep(RETRY_FIRST + SLACK).await;
        // One at once, one RETRY_FIRST later.
        assert_eq!(run.count(), 5);
        assert_eq!(run.gaps().last(), Some(&RETRY_FIRST));
    }
}
