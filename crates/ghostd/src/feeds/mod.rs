//! Built-in topics published from async tasks, each started once by `serve`.

pub mod actions;
pub mod batteries;
pub mod desktop;
pub mod health;
pub mod host;
pub mod http;
mod kwin_script;
pub mod launcher_entries;
pub mod links;
pub mod session;
pub mod shortcuts;
pub mod sky;
pub mod spectrum;
pub mod tailscale;
pub mod ticker;
pub mod uplink;
pub mod weather;
pub mod windows;

use std::future::{pending, poll_fn};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::{SystemTime, UNIX_EPOCH};

use futures_core::Stream;
use ghost_proto::ResolvedConfig;

use crate::server::Demand;

/// The wall clock in seconds since the Unix epoch, passed in so that tests can move it on its own.
pub type WallClock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// A place on Earth, in degrees, north and east being positive.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Place {
    lat: f64,
    lon: f64,
}

/// The place at `location.{lat, lon}`, or `None` unless both are set.
fn place_in(config: &ResolvedConfig) -> Option<Place> {
    Some(Place {
        lat: config.location.lat?,
        lon: config.location.lon?,
    })
}

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

/// Resolves once no UI client subscribes to `topic`, at once if none does.
pub async fn until_unwanted(demand: &mut Demand, topic: &str) {
    // As in `until_wanted`, a closed channel means the demand can never change again.
    if demand
        .wait_for(|topics| !topics.contains(topic))
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

/// Drops every item `stream` has already queued.
pub(crate) fn drain<S: Stream + Unpin>(stream: &mut S) {
    let mut context = Context::from_waker(Waker::noop());
    while let Poll::Ready(Some(_)) = Pin::new(&mut *stream).poll_next(&mut context) {}
}
