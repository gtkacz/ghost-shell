use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ghost_proto::{
    ClientMsg, PROTO_VERSION, ServerMsg, calls, decode_client_line, encode_line, is_valid_topic,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use tracing::{info, warn};

use super::state::Outbox;
use super::{Call, Shared, invalid_topic, line};

/// The longest line a client may send, which also caps a provider's published value.
const MAX_LINE_BYTES: usize = 64 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
// Long enough to hand a parting error to a client that is reading, short enough that one that is
// not cannot hold its connection open.
const ERROR_FLUSH_TIMEOUT: Duration = Duration::from_secs(1);
// accept() fails at once while the process is out of file descriptors; waiting keeps that from
// spinning.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
const EXPECTED_SUBSCRIBE: &str = "expected subscribe";
const EXPECTED_PUBLISH: &str = "expected publish";
const UNKNOWN_METHOD: &str = "unknown method";
const PROVIDERS_CANNOT_CALL: &str = "providers cannot call";

fn too_long() -> String {
    format!("line exceeds {MAX_LINE_BYTES} bytes")
}

fn handshake_expected() -> String {
    format!("expected hello or provide with proto {PROTO_VERSION}")
}

async fn write_queued(
    mut writer: OwnedWriteHalf,
    mut queued: mpsc::UnboundedReceiver<Arc<str>>,
    unsent: Arc<AtomicUsize>,
) {
    while let Some(line) = queued.recv().await {
        if writer.write_all(line.as_bytes()).await.is_err() {
            return;
        }
        unsent.fetch_sub(line.len(), Ordering::Relaxed);
    }
    // The session is over; the client may already be gone, and it is being closed either way.
    let _ = writer.shutdown().await;
}

enum Inbound {
    Line(String),
    TooLong,
    Closed,
}

/// Reads newline-terminated lines of at most [`MAX_LINE_BYTES`].
struct LineReader {
    reader: BufReader<OwnedReadHalf>,
    line: Vec<u8>,
}

impl LineReader {
    fn new(reader: OwnedReadHalf) -> Self {
        Self {
            reader: BufReader::new(reader),
            line: Vec::new(),
        }
    }

    /// The next line without its newline. A read error counts as a close: the peer is gone.
    async fn next(&mut self) -> Inbound {
        self.line.clear();
        // One byte past the limit leaves room for the newline ending a line of exactly the limit.
        let limit = (MAX_LINE_BYTES + 1) as u64;
        let read = (&mut self.reader)
            .take(limit)
            .read_until(b'\n', &mut self.line)
            .await;
        match read {
            Ok(0) | Err(_) => Inbound::Closed,
            Ok(_) if self.line.last() == Some(&b'\n') => {
                self.line.pop();
                Inbound::Line(String::from_utf8_lossy(&self.line).into_owned())
            }
            Ok(_) if self.line.len() > MAX_LINE_BYTES => Inbound::TooLong,
            // The peer closed after a last line without a newline.
            Ok(_) => Inbound::Line(String::from_utf8_lossy(&self.line).into_owned()),
        }
    }
}

pub(super) async fn accept_forever(listener: UnixListener, shared: Shared) -> Infallible {
    // Owning the sessions ties them to `serve`: dropping it closes every connection.
    let mut sessions = JoinSet::new();
    loop {
        match listener.accept().await {
            Ok((stream, _address)) => {
                while sessions.try_join_next().is_some() {}
                sessions.spawn(handle_connection(stream, shared.clone()));
            }
            Err(error) => {
                warn!(%error, "cannot accept a connection");
                sleep(ACCEPT_RETRY_DELAY).await;
            }
        }
    }
}

async fn handle_connection(stream: UnixStream, shared: Shared) {
    let pid = stream
        .peer_cred()
        .ok()
        .and_then(|credentials| credentials.pid())
        .and_then(|pid| u32::try_from(pid).ok());
    let (reader, mut writer) = stream.into_split();
    let mut reader = LineReader::new(reader);
    let first = match timeout(HANDSHAKE_TIMEOUT, reader.next()).await {
        Ok(Inbound::Line(line)) => decode_client_line(&line).ok(),
        Ok(Inbound::TooLong) => return reject(&mut writer, &too_long()).await,
        Ok(Inbound::Closed) => return,
        Err(_elapsed) => None,
    };
    match first {
        Some(ClientMsg::Hello {
            proto: PROTO_VERSION,
            topics,
        }) => serve_ui(&shared, reader, writer, pid, topics).await,
        Some(ClientMsg::Provide {
            proto: PROTO_VERSION,
            name,
            topics,
        }) => serve_provider(&shared, reader, writer, name, topics).await,
        _ => reject(&mut writer, &handshake_expected()).await,
    }
}

/// Sends a parting error. Only used on sockets nothing else has written to, so it cannot block on
/// a full send buffer.
async fn reject(writer: &mut OwnedWriteHalf, message: &str) {
    let line = encode_line(&ServerMsg::Error {
        message: message.to_owned(),
    });
    // The client may already be gone, and it is being disconnected either way.
    let _ = writer.write_all(line.as_bytes()).await;
    let _ = writer.shutdown().await;
}

async fn serve_ui(
    shared: &Shared,
    mut reader: LineReader,
    writer: OwnedWriteHalf,
    pid: Option<u32>,
    initial_topics: Vec<String>,
) {
    let (outbox, queued) = Outbox::new();
    let mut writer_task = tokio::spawn(write_queued(writer, queued, Arc::clone(&outbox.unsent)));
    let (id, mut released) = shared.lock().register_ui(outbox.clone(), pid);

    let outcome = match subscribe(shared, id, initial_topics) {
        Ok(()) => read_subscriptions(shared, id, &mut reader, &mut released).await,
        Err(problem) => Ending::Refused(problem),
    };

    shared.lock().remove_ui(id);
    match outcome {
        Ending::Refused(problem) => {
            let farewell = line(&ServerMsg::Error { message: problem });
            // A client too far behind to take the error is closed all the same.
            let _queued = outbox.push(&farewell);
            // With every sender gone, the writer flushes the error and then closes the socket.
            drop(outbox);
            if timeout(ERROR_FLUSH_TIMEOUT, &mut writer_task)
                .await
                .is_err()
            {
                writer_task.abort();
            }
        }
        Ending::Closed | Ending::Dropped => writer_task.abort(),
    }
}

enum Ending {
    /// The client hung up.
    Closed,
    /// The hub dropped the client for not reading.
    Dropped,
    /// The client broke the protocol and gets this error before the close.
    Refused(String),
}

async fn read_subscriptions(
    shared: &Shared,
    id: u64,
    reader: &mut LineReader,
    released: &mut oneshot::Receiver<()>,
) -> Ending {
    loop {
        let inbound = tokio::select! {
            _ = &mut *released => return Ending::Dropped,
            inbound = reader.next() => inbound,
        };
        let message = match inbound {
            Inbound::Line(line) => decode_client_line(&line).map_err(|error| error.to_string()),
            Inbound::TooLong => Err(too_long()),
            Inbound::Closed => return Ending::Closed,
        };
        let handled = message.and_then(|message| match message {
            ClientMsg::Subscribe { topics } => subscribe(shared, id, topics),
            ClientMsg::Call { method, args } => {
                forward_call(shared, id, method, args);
                Ok(())
            }
            _ => Err(EXPECTED_SUBSCRIBE.to_owned()),
        });
        if let Err(problem) = handled {
            return Ending::Refused(problem);
        }
    }
}

/// Passes a known method on to the feeds, and answers an unknown one with a `CallError`.
fn forward_call(shared: &Shared, id: u64, method: String, args: serde_json::Value) {
    let mut state = shared.lock();
    if calls::ALL.contains(&method.as_str()) {
        state.call(Call { method, args });
    } else {
        let refusal = line(&ServerMsg::CallError {
            method,
            message: UNKNOWN_METHOD.to_owned(),
        });
        state.send(id, &refusal);
    }
}

fn subscribe(shared: &Shared, id: u64, topics: Vec<String>) -> Result<(), String> {
    if let Some(invalid) = topics.iter().find(|topic| !is_valid_topic(topic)) {
        return Err(invalid_topic(invalid));
    }
    shared.lock().subscribe(id, topics);
    Ok(())
}

async fn serve_provider(
    shared: &Shared,
    mut reader: LineReader,
    mut writer: OwnedWriteHalf,
    name: String,
    topics: Vec<String>,
) {
    let claimed = shared.lock().claim(&name, &topics);
    let id = match claimed {
        Ok(id) => id,
        Err(problem) => return reject(&mut writer, &problem).await,
    };
    info!(provider = %name, ?topics, "provider connected");
    let problem = relay_publishes(shared, &mut reader, &mut writer, &name, &topics).await;
    shared.lock().release(id, &topics);
    info!(provider = %name, "provider disconnected");
    if let Some(problem) = problem {
        warn!(provider = %name, problem, "provider broke the protocol");
        reject(&mut writer, &problem).await;
    }
}

/// Providers only publish, so a call is answered with an error but does not end the session.
async fn refuse_call(writer: &mut OwnedWriteHalf, method: String) {
    let refusal = encode_line(&ServerMsg::CallError {
        method,
        message: PROVIDERS_CANNOT_CALL.to_owned(),
    });
    // A provider that never reads must not hold its session open past the flush limit.
    let _ = timeout(ERROR_FLUSH_TIMEOUT, writer.write_all(refusal.as_bytes())).await;
}

/// Relays the provider's publishes until it hangs up (`None`) or breaks a rule (the error).
async fn relay_publishes(
    shared: &Shared,
    reader: &mut LineReader,
    writer: &mut OwnedWriteHalf,
    name: &str,
    declared: &[String],
) -> Option<String> {
    loop {
        let message = match reader.next().await {
            Inbound::Line(line) => decode_client_line(&line).map_err(|error| error.to_string()),
            Inbound::TooLong => Err(too_long()),
            Inbound::Closed => return None,
        };
        if let Ok(ClientMsg::Call { method, .. }) = message {
            refuse_call(writer, method).await;
            continue;
        }
        let handled = message.and_then(|message| match message {
            ClientMsg::Publish { topic, data } => {
                if !is_valid_topic(&topic) {
                    Err(invalid_topic(&topic))
                } else if !declared.contains(&topic) {
                    Err(format!("topic {topic:?} is not declared by {name:?}"))
                } else {
                    shared.lock().publish(&topic, data);
                    Ok(())
                }
            }
            _ => Err(EXPECTED_PUBLISH.to_owned()),
        });
        if let Err(problem) = handled {
            return Some(problem);
        }
    }
}
