//! The `net.links` topic: the LAN devices and saved VPNs NetworkManager knows, Tailscale's state,
//! and the round trip to `network.ping`.
//!
//! Nothing runs until some client wants the topic, and everything stops once none does. While it
//! is wanted:
//! - NetworkManager is read in full at once, and again whenever it comes or goes on the bus, its
//!   devices or active connections change, or an active connection changes state;
//! - Tailscale's local API is asked for its status every 10 s;
//! - `network.ping` is pinged every 5 s, unless it is `""`.
//!
//! A snapshot is published only when it differs from the last one, and the first one of each
//! wanted period only once NetworkManager has answered, so it never shows a moment of no links.

mod nm;
pub mod ping;

use std::future::pending;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ghost_proto::{LinkState, LinksSnapshot, ResolvedConfig, TailscaleLink, topics};
use tokio::sync::watch;
use tokio::time::{MissedTickBehavior, interval, timeout};
use tracing::warn;
use zbus::Connection;

use self::nm::{Changes, NmLinks};
use self::ping::{IcmpPinger, Pinger};
use super::tailscale::{TAILSCALED_SOCKET, TailscaleClient, TailscaleStatus};
use super::{until_unwanted, until_wanted};
use crate::server::{Feed, FeedContext, Publisher};

const TAILSCALE_EVERY: Duration = Duration::from_secs(10);
const PING_EVERY: Duration = Duration::from_secs(5);
/// NetworkManager answers a full read in milliseconds; one that takes longer is stuck, and the
/// links read before stay up meanwhile.
const NM_READ_TIMEOUT: Duration = Duration::from_secs(2);
const MS_PER_S: f32 = 1000.0;
const TAILSCALE_RUNNING: &str = "Running";
const TAILSCALE_STARTING: &str = "Starting";

/// Reads NetworkManager on the system bus, Tailscale at its usual socket, and pings over ICMP.
/// The system bus is connected to only once the topic is first wanted.
pub fn feed() -> Feed {
    Feed::new(topics::NET_LINKS, |mut context| async move {
        until_wanted(&mut context.demand, topics::NET_LINKS).await;
        let bus = Connection::system()
            .await
            .inspect_err(|error| {
                warn!(%error, "cannot connect to the system bus; net.links has no LAN or VPN links");
            })
            .ok();
        let tailscale = TailscaleClient::new(PathBuf::from(TAILSCALED_SOCKET));
        run(bus, tailscale, Arc::new(IcmpPinger), context).await;
    })
}

/// As [`feed`], reading NetworkManager on `bus`, Tailscale through `tailscale` and pinging with
/// `pinger`.
pub fn feed_on(bus: Connection, tailscale: TailscaleClient, pinger: Arc<dyn Pinger>) -> Feed {
    Feed::new(topics::NET_LINKS, move |context| {
        run(Some(bus), tailscale, pinger, context)
    })
}

async fn run(
    bus: Option<Connection>,
    tailscale: TailscaleClient,
    pinger: Arc<dyn Pinger>,
    mut context: FeedContext,
) {
    let links = Links::new(context.publisher.clone());
    loop {
        until_wanted(&mut context.demand, topics::NET_LINKS).await;
        links.begin();
        // Dropping the other branches stops them, and with them their match rules and sockets.
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::NET_LINKS) => {}
            () = watch_network_manager(bus.as_ref(), &links) => {}
            () = watch_tailscale(&tailscale, &links) => {}
            () = ping_forever(&*pinger, context.config.clone(), &links) => {}
        }
    }
}

/// The snapshot as the lanes build it, and the one last published.
struct Links {
    publisher: Publisher,
    progress: Mutex<Progress>,
}

#[derive(Default)]
struct Progress {
    snapshot: LinksSnapshot,
    published: Option<LinksSnapshot>,
    network_manager_read: bool,
}

impl Links {
    fn new(publisher: Publisher) -> Self {
        Self {
            publisher,
            progress: Mutex::default(),
        }
    }

    /// Starts a wanted period, whose first snapshot waits for NetworkManager.
    fn begin(&self) {
        self.lock().network_manager_read = false;
    }

    fn set_network_manager(&self, links: Option<NmLinks>) {
        self.update(|progress| {
            progress.network_manager_read = true;
            if let Some(NmLinks { lan, vpns }) = links {
                progress.snapshot.lan = lan;
                progress.snapshot.vpns = vpns;
            }
        });
    }

    fn set_tailscale(&self, tailscale: Option<TailscaleLink>) {
        self.update(|progress| progress.snapshot.tailscale = tailscale);
    }

    fn set_ping_ms(&self, ping_ms: Option<f32>) {
        self.update(|progress| progress.snapshot.ping_ms = ping_ms);
    }

    fn update(&self, change: impl FnOnce(&mut Progress)) {
        let mut progress = self.lock();
        change(&mut progress);
        if !progress.network_manager_read || progress.published.as_ref() == Some(&progress.snapshot)
        {
            return;
        }
        // Published under the lock, so two quick changes can never reach clients swapped.
        self.publisher
            .publish(topics::NET_LINKS, &progress.snapshot);
        progress.published = Some(progress.snapshot.clone());
    }

    fn lock(&self) -> MutexGuard<'_, Progress> {
        self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Reads NetworkManager now and after every change it signals. Without a bus the links stay
/// empty; without its signals they are read once a wanted period.
async fn watch_network_manager(bus: Option<&Connection>, links: &Links) {
    let Some(bus) = bus else {
        links.set_network_manager(Some(NmLinks::default()));
        return pending().await;
    };
    let mut changes = Changes::listen(bus)
        .await
        .inspect_err(|error| {
            warn!(%error, "cannot watch NetworkManager; net.links reads it only when first wanted");
        })
        .ok();
    loop {
        if let Some(changes) = &mut changes {
            // A read covers every change queued before it starts.
            changes.drain();
        }
        let read = timeout(NM_READ_TIMEOUT, nm::read(bus)).await;
        if read.is_err() {
            warn!(
                "NetworkManager did not answer within {NM_READ_TIMEOUT:?}; keeping its last links"
            );
        }
        links.set_network_manager(read.ok());
        match &mut changes {
            Some(changes) => changes.next().await,
            None => return pending().await,
        }
    }
}

async fn watch_tailscale(tailscale: &TailscaleClient, links: &Links) {
    let mut ticks = interval(TAILSCALE_EVERY);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let status = tailscale.status().await;
        links.set_tailscale(status.as_ref().map(tailscale_link));
    }
}

fn tailscale_link(status: &TailscaleStatus) -> TailscaleLink {
    let online = status.peers.iter().filter(|peer| peer.online).count();
    TailscaleLink {
        address: status.self_ips.first().map(ToString::to_string),
        peers_online: u32::try_from(online).unwrap_or(u32::MAX),
        peers_total: u32::try_from(status.peers.len()).unwrap_or(u32::MAX),
        state: match status.backend_state.as_str() {
            TAILSCALE_RUNNING => LinkState::Up,
            TAILSCALE_STARTING => LinkState::Connecting,
            _ => LinkState::Down,
        },
    }
}

/// Pings `network.ping` every [`PING_EVERY`], following the config as it changes, and waits for
/// a change while it is `""`.
async fn ping_forever(
    pinger: &dyn Pinger,
    mut config: watch::Receiver<ResolvedConfig>,
    links: &Links,
) {
    let mut ticks = interval(PING_EVERY);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        let host = ping_host(&config.borrow_and_update());
        let Some(host) = host else {
            links.set_ping_ms(None);
            if config.changed().await.is_err() {
                return pending().await;
            }
            ticks.reset_immediately();
            continue;
        };
        tokio::select! {
            _ = ticks.tick() => {
                let round_trip = pinger.ping(host).await;
                links.set_ping_ms(round_trip.map(|round_trip| round_trip.as_secs_f32() * MS_PER_S));
            }
            Ok(()) = config.changed() => {}
        }
    }
}

/// Loading the config already rejected anything but an IPv4 address or `""`.
fn ping_host(config: &ResolvedConfig) -> Option<Ipv4Addr> {
    config.network.ping.parse().ok()
}
