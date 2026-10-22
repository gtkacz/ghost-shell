//! The capture thread: a PipeWire main loop of its own, with one stream on the default sink's
//! monitor whose audio is analysed on that thread as it arrives.

use std::io::{self, Cursor};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::spa;
use pw::stream::{Stream, StreamBox, StreamFlags, StreamState};
use spa::param::ParamType;
use spa::param::audio::{AudioFormat, AudioInfoRaw, MAX_CHANNELS};
use spa::pod::serialize::PodSerializer;
use spa::pod::{Object, Pod, Value};
use spa::utils::{Direction, SpaTypes};
use tokio::sync::broadcast;
use tokio::task::spawn_blocking;
use tokio::time::timeout;
use tracing::warn;

use super::analyzer::{Analyzer, Bands};
use super::{Frame, MIN_HOLD};

const THREAD_NAME: &str = "ghostd-spectrum";
const NODE_NAME: &str = "ghostd-spectrum";
const APP_NAME: &str = "ghostd";
const RATE: u32 = 48_000;
const CHANNELS: u32 = 2;
const POSITIONS: [u32; CHANNELS as usize] = [
    spa::sys::SPA_AUDIO_CHANNEL_FL,
    spa::sys::SPA_AUDIO_CHANNEL_FR,
];
const SAMPLE_BYTES: usize = size_of::<f32>();
/// A reader that falls behind skips to the newest frames instead of holding the capture up.
const QUEUED_FRAMES: usize = 2;
const BUFFERS_HELD: u32 = 2;
/// How long stopping waits for the thread. A PipeWire call stuck past it must not hang the
/// daemon, so such a thread is left to end on its own.
const STOP_GRACE: Duration = Duration::from_secs(1);

/// A running capture and the frames it analyses, which end when the capture does.
pub(super) struct Capture {
    pub(super) frames: broadcast::Receiver<Frame>,
    connected: Arc<AtomicBool>,
    running: Option<Running>,
}

/// What only a capture with a thread of its own has.
struct Running {
    quit: pw::channel::Sender<()>,
    thread: JoinHandle<()>,
}

impl Capture {
    /// Starts the capture thread. Without PipeWire, the thread logs why and its frames end at once.
    pub(super) fn start() -> io::Result<Self> {
        let (frames_tx, frames) = broadcast::channel(QUEUED_FRAMES);
        let (quit, quit_rx) = pw::channel::channel();
        let connected = Arc::new(AtomicBool::new(false));
        let thread = thread::Builder::new().name(THREAD_NAME.to_owned()).spawn({
            let connected = Arc::clone(&connected);
            move || {
                if let Err(error) = capture(frames_tx, quit_rx, &connected) {
                    warn!(%error, "audio output capture failed; audio.spectrum stays dark");
                }
            }
        })?;
        Ok(Self {
            frames,
            connected,
            running: Some(Running { quit, thread }),
        })
    }

    /// A capture without a thread, whose frames come from the test.
    #[cfg(test)]
    pub(super) fn detached(frames: broadcast::Receiver<Frame>, connected: bool) -> Self {
        Self {
            frames,
            connected: Arc::new(AtomicBool::new(connected)),
            running: None,
        }
    }

    /// Whether PipeWire took the stream, that is whether it ever reached `Paused` or `Streaming`.
    pub(super) fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Quits the loop, which disconnects the stream, and joins the thread within [`STOP_GRACE`].
    pub(super) async fn stop(self) {
        let Some(Running { quit, thread }) = self.running else {
            return;
        };
        // A loop that has already ended has nothing left to quit, and one the message cannot reach
        // is left behind by the bounded join below all the same.
        let _ = quit.send(());
        match timeout(STOP_GRACE, spawn_blocking(move || thread.join())).await {
            Err(_) => warn!("the audio capture did not stop within {STOP_GRACE:?}; leaving it"),
            Ok(Ok(Err(_))) => warn!("the audio capture thread panicked"),
            Ok(_) => {}
        }
    }
}

/// Captures until `quit` receives or PipeWire ends the stream, sending every frame on `frames`, and
/// sets `connected` once PipeWire takes the stream.
fn capture(
    frames: broadcast::Sender<Frame>,
    quit: pw::channel::Receiver<()>,
    connected: &Arc<AtomicBool>,
) -> Result<(), pw::Error> {
    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_rc(None)?;
    let properties = pw::properties::properties! {
        *pw::keys::STREAM_CAPTURE_SINK => "true",
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::NODE_NAME => NODE_NAME,
        *pw::keys::APP_NAME => APP_NAME,
        // A visualiser must never keep the sink, and with it the DAC and the graph, awake.
        *pw::keys::NODE_PASSIVE => "true",
    };
    let stream = StreamBox::new(&core, NODE_NAME, properties)?;
    let _quit = quit.attach(main_loop.loop_(), {
        let main_loop = main_loop.clone();
        move |()| main_loop.quit()
    });
    let listener = stream
        .add_local_listener_with_user_data(Analysis::new(frames))
        .state_changed({
            let main_loop = main_loop.clone();
            let connected = Arc::clone(connected);
            move |_, _, _, state| {
                let reason = match state {
                    StreamState::Error(message) => message,
                    StreamState::Unconnected => "PipeWire disconnected".to_owned(),
                    StreamState::Connecting => return,
                    StreamState::Paused | StreamState::Streaming => {
                        connected.store(true, Ordering::Relaxed);
                        return;
                    }
                };
                warn!(%reason, "the audio output capture stopped");
                main_loop.quit();
            }
        })
        .process(|stream, analysis| analysis.process(stream))
        .register()?;
    let format = format();
    let mut params = [Pod::from_bytes(&format).expect("a serialised pod reads back")];
    // Without RT_PROCESS, buffers are processed on this thread rather than PipeWire's realtime one.
    stream.connect(
        Direction::Input,
        None,
        StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;
    main_loop.run();
    // Unregistered first, so that this disconnect is not reported as PipeWire's.
    drop(listener);
    stream.disconnect()
}

/// The only format offered, which PipeWire converts the monitor to.
fn format() -> Vec<u8> {
    let mut info = AudioInfoRaw::new();
    info.set_format(AudioFormat::F32LE);
    info.set_rate(RATE);
    info.set_channels(CHANNELS);
    let mut position = [0; MAX_CHANNELS];
    position[..POSITIONS.len()].copy_from_slice(&POSITIONS);
    info.set_position(position);
    let format = Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(format))
        .expect("serialising into memory cannot fail")
        .0
        .into_inner()
}

/// What turns the stream's buffers into frames for the feed.
struct Analysis {
    analyzer: Analyzer,
    pacing: Pacing,
    samples: Vec<f32>,
    bands: Vec<Bands>,
    frames: broadcast::Sender<Frame>,
}

impl Analysis {
    fn new(frames: broadcast::Sender<Frame>) -> Self {
        Self {
            analyzer: Analyzer::new(RATE),
            pacing: Pacing::default(),
            samples: Vec::new(),
            bands: Vec::new(),
            frames,
        }
    }

    fn process(&mut self, stream: &Stream) {
        let Some(mut buffer) = stream.dequeue_buffer() else {
            return;
        };
        let Some(data) = buffer.datas_mut().first_mut() else {
            return;
        };
        let start = data.chunk().offset() as usize;
        let end = start + data.chunk().size() as usize;
        // A buffer without audio is no sign that audio is flowing again.
        let Some(bytes) = data
            .data()
            .and_then(|bytes| bytes.get(start..end))
            .filter(|bytes| !bytes.is_empty())
        else {
            return;
        };
        let (samples, _) = bytes.as_chunks::<SAMPLE_BYTES>();
        self.samples.clear();
        self.samples
            .extend(samples.iter().map(|&sample| f32::from_le_bytes(sample)));
        let channels = CHANNELS as usize;
        // The feed has shown the bands dark after such a gap, so the audio that resumes must not
        // rise from the bands, or mix with the samples, of what played before it.
        if self
            .pacing
            .arrived(Instant::now(), self.samples.len() / channels)
        {
            self.analyzer = Analyzer::new(RATE);
        }
        self.analyzer.push(&self.samples, channels, &mut self.bands);
        let hold = self.pacing.hold;
        for bands in self.bands.drain(..) {
            // There is no receiver only once the feed has stopped reading this capture.
            let _ = self.frames.send(Frame { bands, hold });
        }
    }
}

/// When the latest buffer arrived, and how long the frames analysed from it hold the bands.
#[derive(Default)]
struct Pacing {
    last_buffer: Option<Instant>,
    // Only read once a buffer has arrived, which sets it.
    hold: Duration,
}

impl Pacing {
    /// Records a buffer of `frames` audio frames arriving at `now`. Returns whether it ends a gap
    /// that outlasted the previous buffer's hold, which the feed has shown as dark.
    fn arrived(&mut self, now: Instant, frames: usize) -> bool {
        let ends_a_gap = self
            .last_buffer
            .is_some_and(|last| now.duration_since(last) >= self.hold);
        self.last_buffer = Some(now);
        self.hold = hold_for(frames);
        ends_a_gap
    }
}

/// How long the frames of a buffer of `frames` audio frames hold the bands: at least [`MIN_HOLD`],
/// and two buffers' time, since the next buffer of a steady stream arrives one buffer's time later.
fn hold_for(frames: usize) -> Duration {
    let buffers_time = Duration::from_secs(1) * BUFFERS_HELD * frames as u32 / RATE;
    buffers_time.max(MIN_HOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default quantum: 21 ms at 48 kHz.
    const SMALL_BUFFER: usize = 1024;
    /// PipeWire's hard ceiling on a quantum (`quantum-limit`; the default `max-quantum` is 2048): 171 ms at 48 kHz.
    const LARGE_BUFFER: usize = 8192;
    const LARGE_BUFFER_GAP: Duration = Duration::from_millis(170);
    const ARRIVALS: u32 = 10;

    #[test]
    fn a_buffer_holds_its_frames_for_three_frame_periods_or_two_buffers() {
        let two_large_buffers = Duration::from_millis(341)..Duration::from_millis(342);

        assert_eq!(hold_for(SMALL_BUFFER), MIN_HOLD);
        assert!(two_large_buffers.contains(&hold_for(LARGE_BUFFER)));
    }

    #[test]
    fn large_buffers_a_buffer_apart_keep_the_analyzer() {
        let start = Instant::now();
        let mut pacing = Pacing::default();

        let restarts = (0..ARRIVALS)
            .filter(|&arrival| pacing.arrived(start + LARGE_BUFFER_GAP * arrival, LARGE_BUFFER))
            .count();

        assert_eq!(restarts, 0);
    }

    #[test]
    fn a_gap_as_long_as_the_last_hold_restarts_the_analyzer() {
        let start = Instant::now();
        let mut pacing = Pacing::default();
        assert!(!pacing.arrived(start, SMALL_BUFFER), "the first buffer");

        assert!(!pacing.arrived(start + MIN_HOLD / 2, SMALL_BUFFER));
        assert!(pacing.arrived(start + MIN_HOLD / 2 + MIN_HOLD, LARGE_BUFFER));
        let large_hold = hold_for(LARGE_BUFFER);
        let resumed = start + MIN_HOLD / 2 + MIN_HOLD + large_hold;
        assert!(pacing.arrived(resumed, SMALL_BUFFER));
    }
}
