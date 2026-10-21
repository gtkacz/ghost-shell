//! The capture thread: a PipeWire main loop of its own, with one stream on the default sink's
//! monitor whose audio is analysed on that thread as it arrives.

use std::io::{self, Cursor};
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

use super::STALL;
use super::analyzer::{Analyzer, Bands};

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
/// How long stopping waits for the thread. A PipeWire call stuck past it must not hang the
/// daemon, so such a thread is left to end on its own.
const STOP_GRACE: Duration = Duration::from_secs(1);

/// A running capture and the frames it analyses, which end when the capture does.
pub(super) struct Capture {
    pub(super) frames: broadcast::Receiver<Bands>,
    quit: pw::channel::Sender<()>,
    thread: JoinHandle<()>,
}

impl Capture {
    /// Starts the capture thread. Without PipeWire, the thread logs why and its frames end at once.
    pub(super) fn start() -> io::Result<Self> {
        let (frames_tx, frames) = broadcast::channel(QUEUED_FRAMES);
        let (quit, quit_rx) = pw::channel::channel();
        let thread = thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                if let Err(error) = capture(frames_tx, quit_rx) {
                    warn!(%error, "audio output capture failed; audio.spectrum stays dark");
                }
            })?;
        Ok(Self {
            frames,
            quit,
            thread,
        })
    }

    /// Quits the loop, which disconnects the stream, and joins the thread within [`STOP_GRACE`].
    pub(super) async fn stop(self) {
        // A loop that has already ended has nothing left to quit, and one the message cannot reach
        // is left behind by the bounded join below all the same.
        let _ = self.quit.send(());
        let thread = self.thread;
        match timeout(STOP_GRACE, spawn_blocking(move || thread.join())).await {
            Err(_) => warn!("the audio capture did not stop within {STOP_GRACE:?}; leaving it"),
            Ok(Ok(Err(_))) => warn!("the audio capture thread panicked"),
            Ok(_) => {}
        }
    }
}

/// Captures until `quit` receives or PipeWire ends the stream, sending every frame on `frames`.
fn capture(
    frames: broadcast::Sender<Bands>,
    quit: pw::channel::Receiver<()>,
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
            move |_, _, _, state| {
                let reason = match state {
                    StreamState::Error(message) => message,
                    StreamState::Unconnected => "PipeWire disconnected".to_owned(),
                    StreamState::Connecting | StreamState::Paused | StreamState::Streaming => {
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
    samples: Vec<f32>,
    bands: Vec<Bands>,
    frames: broadcast::Sender<Bands>,
    last_buffer: Option<Instant>,
}

impl Analysis {
    fn new(frames: broadcast::Sender<Bands>) -> Self {
        Self {
            analyzer: Analyzer::new(RATE),
            samples: Vec::new(),
            bands: Vec::new(),
            frames,
            last_buffer: None,
        }
    }

    fn process(&mut self, stream: &Stream) {
        let Some(mut buffer) = stream.dequeue_buffer() else {
            return;
        };
        let now = Instant::now();
        // The feed has shown the bands dark after such a gap, so the audio that resumes must not
        // rise from the bands, or mix with the samples, of what played before it.
        if self
            .last_buffer
            .is_some_and(|last| now.duration_since(last) >= STALL)
        {
            self.analyzer = Analyzer::new(RATE);
        }
        self.last_buffer = Some(now);
        let Some(data) = buffer.datas_mut().first_mut() else {
            return;
        };
        let start = data.chunk().offset() as usize;
        let end = start + data.chunk().size() as usize;
        let Some(bytes) = data.data().and_then(|bytes| bytes.get(start..end)) else {
            return;
        };
        let (samples, _) = bytes.as_chunks::<SAMPLE_BYTES>();
        self.samples.clear();
        self.samples
            .extend(samples.iter().map(|&sample| f32::from_le_bytes(sample)));
        self.analyzer
            .push(&self.samples, CHANNELS as usize, &mut self.bands);
        for bands in self.bands.drain(..) {
            // There is no receiver only once the feed has stopped reading this capture.
            let _ = self.frames.send(bands);
        }
    }
}
