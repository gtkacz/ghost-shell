//! The `windows` topic and the commands that go back to KWin. KWin's desktop script reports the
//! window list over D-Bus and long-polls `Next` for the commands that UI clients `call`.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::future::pending;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ghost_proto::{WindowInfo, WindowsSnapshot, topics};
use serde_json::{Value, json};
use tokio::sync::{Notify, broadcast};
use tokio::time::{Instant, timeout_at};
use tracing::warn;
use zbus::{connection, interface};

use crate::server::{Call, Publisher};

pub(crate) const OBJECT_PATH: &str = "/org/ghostshell/Windows";
/// Under D-Bus's 25 s default call timeout, which KWin's `DBusCall` runs with.
pub(crate) const NEXT_TIMEOUT: Duration = Duration::from_secs(20);
/// A click that waited this long for a poll is not what the user wants any more.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(2);
pub(crate) const QUEUE_CAP: usize = 32;
const COMMAND_PREFIX: &str = "windows.";
const ID_ARG: &str = "id";

#[derive(Debug, Clone, Copy)]
pub struct Timings {
    pub next_timeout: Duration,
    pub stale_after: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            next_timeout: NEXT_TIMEOUT,
            stale_after: STALE_AFTER,
        }
    }
}

#[derive(Default)]
struct Queue {
    commands: Mutex<VecDeque<(Instant, String)>>,
    pushed: Notify,
}

impl Queue {
    fn push(&self, command: String) {
        let mut commands = self.commands.lock().unwrap_or_else(PoisonError::into_inner);
        commands.push_back((Instant::now(), command));
        if commands.len() > QUEUE_CAP {
            commands.pop_front();
        }
        drop(commands);
        // A permit is kept when nobody waits, so a push between a poll's check and its wait is not lost.
        self.pushed.notify_one();
    }

    fn pop_fresh(&self, stale_after: Duration) -> Option<String> {
        let mut commands = self.commands.lock().unwrap_or_else(PoisonError::into_inner);
        while let Some((queued, command)) = commands.pop_front() {
            if queued.elapsed() <= stale_after {
                return Some(command);
            }
        }
        None
    }
}

/// Both halves of the bridge: the window list KWin reports and the commands it polls for.
#[derive(Clone)]
pub struct Bridge {
    publisher: Publisher,
    queue: Arc<Queue>,
    timings: Timings,
}

impl Bridge {
    pub fn new(publisher: Publisher, timings: Timings) -> Self {
        Self {
            publisher,
            queue: Arc::default(),
            timings,
        }
    }

    /// Serves both objects on `bus`; the caller takes the bus name afterwards.
    pub fn serve(&self, bus: connection::Builder<'static>) -> connection::Builder<'static> {
        bus.serve_at(OBJECT_PATH, WindowsService(self.clone()))
            .serve_at(OBJECT_PATH, WindowCommands(self.clone()))
    }

    /// Queues the `windows.*` calls for KWin. Never returns: calls keep coming for as long as the
    /// daemon runs.
    pub async fn forward(self, mut calls: broadcast::Receiver<Call>) -> Infallible {
        loop {
            match calls.recv().await {
                Ok(call) => self.queue_if_window_call(call),
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    warn!(missed, "window calls were dropped: the bridge fell behind");
                }
                Err(broadcast::error::RecvError::Closed) => pending().await,
            }
        }
    }

    fn queue_if_window_call(&self, call: Call) {
        let Some(action) = call.method.strip_prefix(COMMAND_PREFIX) else {
            return;
        };
        let id = call.args.get(ID_ARG).and_then(Value::as_str);
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            warn!(method = call.method, "window call without an id");
            return;
        };
        self.queue
            .push(json!({"action": action, ID_ARG: id}).to_string());
    }
}

struct WindowsService(Bridge);

// One call at a time, in arrival order, so an older list can never land after a newer one.
#[interface(name = "org.ghostshell.Windows1", spawn = false)]
impl WindowsService {
    /// `windows_json` is an array of window objects, most recently activated first.
    fn report(&self, windows_json: &str) {
        match serde_json::from_str::<Vec<WindowInfo>>(windows_json) {
            Ok(windows) => self
                .0
                .publisher
                .publish(topics::WINDOWS, &WindowsSnapshot { windows }),
            Err(error) => warn!(%error, "ignored a malformed window list"),
        }
    }
}

struct WindowCommands(Bridge);

#[interface(name = "org.ghostshell.WindowCommands1")]
impl WindowCommands {
    /// Holds the call until a command is queued or the timeout ends; `""` means none.
    async fn next(&self) -> String {
        let deadline = Instant::now() + self.0.timings.next_timeout;
        loop {
            if let Some(command) = self.0.queue.pop_fresh(self.0.timings.stale_after) {
                return command;
            }
            if timeout_at(deadline, self.0.queue.pushed.notified())
                .await
                .is_err()
            {
                return String::new();
            }
        }
    }
}
