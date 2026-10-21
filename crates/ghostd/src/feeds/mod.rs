//! Built-in topics published from async tasks, each started once by `serve`.

pub mod host;

use std::future::pending;

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
