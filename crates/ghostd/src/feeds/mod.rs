//! Built-in topics published from async tasks, each started once by `serve`.

pub mod desktop;
pub mod host;
pub mod http;
pub mod shortcuts;
pub mod weather;

use std::future::{pending, poll_fn};
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_core::Stream;

use crate::server::Demand;

/// Whether some UI client subscribes to `topic` now.
pub fn wanted(demand: &Demand, topic: &str) -> bool {
    demand.borrow().contains(topic)
}

/// Resolves once some UI client subscribes to `topic`, at once if one already does.
pub async fn until_wanted(demand: &mut Demand, topic: &str) {
    // The hub holds the sender until it stops, so a closed channel means no client can ever want
    // the topic again.
    if demand
        .wait_for(|topics| topics.contains(topic))
        .await
        .is_err()
    {
        pending::<()>().await;
    }
}

/// The wall clock in whole seconds since the Unix epoch, or 0 if it is set before 1970.
pub(crate) fn unix_now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since_epoch| since_epoch.as_secs())
}

/// The next item of `stream`, or `None` once it has ended.
pub(crate) async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}
