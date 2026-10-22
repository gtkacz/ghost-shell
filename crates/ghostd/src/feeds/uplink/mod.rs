//! The `uplink` topic: where this machine's live connections lead, for the uplink globe.
//!
//! Nothing runs until some client wants the topic, and everything stops once none does; the
//! GeoIP databases are not even opened before the first want, and are unmapped whenever it is
//! unwanted, so their resident pages count only while a globe is shown. Every 5 s while it is
//! wanted:
//! 1. the established TCP and connected UDP sockets are read from `/proc/net`;
//! 2. remotes outside the public internet are dropped;
//! 3. each remote is located, and its autonomous system looked up, in the offline databases;
//! 4. each is labelled: by `[uplink.labels]`, by its organisation, by `AS<n>`, or by its address;
//! 5. sockets sharing a label and a city become one endpoint, and those with no city are counted
//!    but not shown;
//! 6. each online Tailscale peer with a direct path to a public address is added as
//!    `TAILNET:<HOST>`;
//! 7. the 8 busiest endpoints are published, if anything changed.
//!
//! No address is ever resolved to a name, and nothing leaves the machine but the request to
//! tailscaled's local socket.

pub mod geo;
pub mod owners;
pub mod sockets;

use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::panic;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ghost_proto::{Endpoint, UplinkSnapshot, topics};
use tokio::task::spawn_blocking;
use tokio::time::{Instant, MissedTickBehavior, interval};
use tracing::debug;

use self::geo::{Geo, MmdbGeo, geoip_dir};
use self::owners::Owners;
use self::sockets::{ProcNetSockets, RemoteSocket, SocketSource, is_public};
use super::tailscale::{TAILSCALE_EVERY, TAILSCALED_SOCKET, TailscaleClient, TailscalePeer};
use super::{until_unwanted, until_wanted};
use crate::server::{Feed, FeedContext};
use crate::sysroot::SysRoot;

const UPLINK_EVERY: Duration = Duration::from_secs(5);
/// The most arcs the globe draws (user, 2026-10-21).
pub const MAX_ENDPOINTS: usize = 8;
/// The `[uplink.labels]` value that shows the process owning the socket.
pub const PROCESS_LABEL: &str = "process";
const ASN_PREFIX: &str = "AS";
const TAILNET_PREFIX: &str = "TAILNET:";
/// A tailnet peer's traffic is tailscaled's one UDP socket, which no `/proc/net` row ties to it.
const TAILNET_PEER_SOCKETS: u32 = 1;

/// Reads the host's `/proc`, the databases in `uplink.geoip_dir` and tailscaled's usual socket.
/// The directory is read from the config when the topic is first wanted.
pub fn feed() -> Feed {
    Feed::new(topics::UPLINK, |mut context| async move {
        until_wanted(&mut context.demand, topics::UPLINK).await;
        let dir = geoip_dir(&context.config.borrow().uplink.geoip_dir);
        let geo = Arc::new(MmdbGeo::open(&dir));
        let tailscale = TailscaleClient::new(PathBuf::from(TAILSCALED_SOCKET));
        let sockets = Arc::new(ProcNetSockets::new(SysRoot::host()));
        run(
            sockets,
            geo,
            Owners::new(SysRoot::host()),
            tailscale,
            context,
        )
        .await;
    })
}

/// As [`feed`], with every source injected. Label overrides still come from the config.
pub fn feed_on(
    sockets: Arc<dyn SocketSource>,
    geo: Arc<dyn Geo>,
    owners: Owners,
    tailscale: TailscaleClient,
) -> Feed {
    Feed::new(topics::UPLINK, move |context| {
        run(sockets, geo, owners, tailscale, context)
    })
}

async fn run(
    sockets: Arc<dyn SocketSource>,
    geo: Arc<dyn Geo>,
    mut owners: Owners,
    tailscale: TailscaleClient,
    mut context: FeedContext,
) {
    let mut published: Option<UplinkSnapshot> = None;
    loop {
        until_wanted(&mut context.demand, topics::UPLINK).await;
        let mut peers = Peers::new(&tailscale);
        let mut ticks = interval(UPLINK_EVERY);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                () = until_unwanted(&mut context.demand, topics::UPLINK) => {
                    geo.release();
                    break;
                }
                _ = ticks.tick() => {}
            }
            let peers = peers.current().await.to_vec();
            let overrides = context.config.borrow().uplink.labels.clone();
            let (sockets, geo) = (Arc::clone(&sockets), Arc::clone(&geo));
            // A walk of `/proc/*/fd` takes some 10 ms here; off the reactor it delays neither the
            // socket nor the other feeds.
            let cycle = spawn_blocking(move || {
                let started = std::time::Instant::now();
                geo.refresh();
                let snapshot = snapshot(&sockets.sockets(), &*geo, &mut owners, &overrides, &peers);
                debug!(
                    elapsed_us = started.elapsed().as_micros(),
                    total = snapshot.total,
                    shown = snapshot.endpoints.len(),
                    "uplink cycle"
                );
                (owners, snapshot)
            });
            let snapshot;
            (owners, snapshot) = cycle
                .await
                .unwrap_or_else(|error| panic::resume_unwind(error.into_panic()));
            if published.as_ref() != Some(&snapshot) {
                context.publisher.publish(topics::UPLINK, &snapshot);
                published = Some(snapshot);
            }
        }
    }
}

/// Tailscale's peers, asked for at most every [`TAILSCALE_EVERY`].
struct Peers<'a> {
    tailscale: &'a TailscaleClient,
    peers: Vec<TailscalePeer>,
    asked_at: Option<Instant>,
}

impl<'a> Peers<'a> {
    fn new(tailscale: &'a TailscaleClient) -> Self {
        Self {
            tailscale,
            peers: Vec::new(),
            asked_at: None,
        }
    }

    async fn current(&mut self) -> &[TailscalePeer] {
        if self
            .asked_at
            .is_none_or(|asked_at| asked_at.elapsed() >= TAILSCALE_EVERY)
        {
            self.asked_at = Some(Instant::now());
            self.peers = self
                .tailscale
                .status()
                .await
                .map(|status| status.peers)
                .unwrap_or_default();
        }
        &self.peers
    }
}

/// What the databases know of one remote address.
struct Remote {
    city: Option<(f32, f32)>,
    org: Option<(u32, String)>,
    shows_owner: bool,
}

fn snapshot(
    sockets: &[RemoteSocket],
    geo: &dyn Geo,
    owners: &mut Owners,
    overrides: &BTreeMap<String, String>,
    peers: &[TailscalePeer],
) -> UplinkSnapshot {
    let public: Vec<&RemoteSocket> = sockets
        .iter()
        .filter(|socket| is_public(&socket.remote))
        .collect();
    let mut remotes: HashMap<IpAddr, Remote> = HashMap::new();
    for socket in &public {
        remotes.entry(socket.remote).or_insert_with(|| {
            let org = geo.asn(socket.remote);
            Remote {
                city: geo.city(socket.remote),
                shows_owner: override_for(org.as_ref(), overrides) == Some(PROCESS_LABEL),
                org,
            }
        });
    }
    // Only sockets whose label is their process cost a walk of `/proc/*/fd`.
    let wanted_owners: Vec<(u64, u32)> = public
        .iter()
        .filter(|socket| remotes[&socket.remote].shows_owner)
        .map(|socket| (socket.inode, socket.uid))
        .collect();
    let owner_names = owners.names(&wanted_owners);

    // Keyed by the place's bits, since floats do not hash.
    let mut groups: HashMap<(String, Option<(u32, u32)>), u32> = HashMap::new();
    for socket in &public {
        let remote = &remotes[&socket.remote];
        let owner = owner_names.get(&socket.inode).map(String::as_str);
        let label = label_for(remote.org.as_ref(), overrides, owner, socket.remote);
        let place = remote.city.map(|(lat, lon)| (lat.to_bits(), lon.to_bits()));
        *groups.entry((label, place)).or_default() += 1;
    }
    let tailnet: Vec<(String, Option<(f32, f32)>)> = peers
        .iter()
        .filter(|peer| peer.online)
        .filter_map(|peer| {
            let address = peer.cur_addr.filter(|address| is_public(&address.ip()))?;
            let label = format!("{TAILNET_PREFIX}{}", peer.host_name.to_uppercase());
            Some((label, geo.city(address.ip())))
        })
        .collect();
    let total = groups.len() + tailnet.len();

    let mut endpoints: Vec<Endpoint> = groups
        .into_iter()
        .filter_map(|((label, place), sockets)| {
            let (lat, lon) = place?;
            let (lat, lon) = (f32::from_bits(lat), f32::from_bits(lon));
            Some(Endpoint {
                label,
                lat,
                lon,
                sockets,
                tailnet: false,
            })
        })
        .chain(tailnet.into_iter().filter_map(|(label, city)| {
            let (lat, lon) = city?;
            Some(Endpoint {
                label,
                lat,
                lon,
                sockets: TAILNET_PEER_SOCKETS,
                tailnet: true,
            })
        }))
        .collect();
    endpoints.sort_by(|a, b| {
        b.sockets
            .cmp(&a.sockets)
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.lat.total_cmp(&b.lat))
            .then_with(|| a.lon.total_cmp(&b.lon))
    });
    endpoints.truncate(MAX_ENDPOINTS);
    UplinkSnapshot {
        endpoints,
        total: u32::try_from(total).unwrap_or(u32::MAX),
        attribution: geo.attribution().to_owned(),
    }
}

/// The `[uplink.labels]` value for an organisation: by `AS<n>` first, then by its name, both
/// in any case.
fn override_for<'a>(
    org: Option<&(u32, String)>,
    overrides: &'a BTreeMap<String, String>,
) -> Option<&'a str> {
    let (number, name) = org?;
    let asn = format!("{ASN_PREFIX}{number}");
    let find = |key: &str| {
        let key = key.to_lowercase();
        overrides
            .iter()
            .find(|(candidate, _)| candidate.to_lowercase() == key)
            .map(|(_, value)| value.as_str())
    };
    find(&asn).or_else(|| {
        Some(name)
            .filter(|name| !name.is_empty())
            .and_then(|name| find(name))
    })
}

/// An endpoint's label: its override's fixed text, or its owning process's `comm` for
/// `"process"`; else its organisation, uppercased; else `AS<n>`; else its address.
pub fn label_for(
    org: Option<&(u32, String)>,
    overrides: &BTreeMap<String, String>,
    owner: Option<&str>,
    ip: IpAddr,
) -> String {
    match override_for(org, overrides) {
        Some(PROCESS_LABEL) => {
            if let Some(owner) = owner {
                return owner.to_uppercase();
            }
        }
        Some(text) => return text.to_owned(),
        None => {}
    }
    match org {
        Some((_, name)) if !name.is_empty() => name.to_uppercase(),
        Some((number, _)) => format!("{ASN_PREFIX}{number}"),
        None => ip.to_string(),
    }
}
