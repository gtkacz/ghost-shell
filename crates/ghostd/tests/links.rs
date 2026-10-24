mod common;

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::bus::{BUS_WAIT, PrivateBus};
use common::nm::{ACTIVATING, ACTIVE_ACTIVATED, FakeNm, NmShape, SAVED_VPN_NAME, VPN_NAME};
use common::tailscale::{FakeTailscale, TAILSCALE_STATUS, missing_tailscale};
use futures_core::future::BoxFuture;
use ghost_proto::{LanLink, LinkState, LinksSnapshot, ResolvedConfig, VpnLink, topics};
use ghostd::feeds::links::{self, ping::Pinger};
use ghostd::feeds::tailscale::{MAX_REPLY_BYTES, TailscaleClient, TailscalePeer};
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, yield_now};
use tokio::time::sleep;

const TAILSCALE_REQUEST: &str =
    "GET /localapi/v0/status HTTP/1.0\r\nHost: local-tailscaled.sock\r\n\r\n";
const PING_RTT: Duration = Duration::from_millis(12);
const PING_HOST: &str = "192.0.2.1";
const NM_LATE_BY: Duration = Duration::from_secs(5);
const ABSENT_FOR: Duration = Duration::from_secs(30);
const UNWANTED_FOR: Duration = Duration::from_secs(60);
const PING_EVERY: Duration = Duration::from_secs(5);
const LEAST_PINGS_IN_30_S: usize = 5;

#[derive(Default)]
struct CountingPinger {
    hosts: Mutex<Vec<Ipv4Addr>>,
}

impl CountingPinger {
    fn count(&self) -> usize {
        self.hosts.lock().unwrap().len()
    }
}

impl Pinger for CountingPinger {
    fn ping(&self, host: Ipv4Addr) -> BoxFuture<'_, Option<Duration>> {
        self.hosts.lock().unwrap().push(host);
        Box::pin(async { Some(PING_RTT) })
    }
}

/// The real feed on a private bus, and everything it publishes.
struct RunningLinks {
    published: mpsc::UnboundedReceiver<(String, Value)>,
    demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    task: JoinHandle<()>,
}

impl RunningLinks {
    async fn start(
        bus: &PrivateBus,
        tailscale: TailscaleClient,
        pinger: Arc<CountingPinger>,
        ping: &str,
    ) -> Self {
        let mut config = ResolvedConfig::default();
        config.network.ping = ping.to_owned();
        let (published_sender, published) = mpsc::unbounded_channel();
        let wanted = BTreeSet::from([topics::NET_LINKS.to_owned()]);
        let (demand, demand_receiver) = watch::channel(wanted);
        let (config, config_receiver) = watch::channel(config);
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                published_sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
            calls: tokio::sync::broadcast::channel(64).1,
            topics: tokio::sync::broadcast::channel(1).1,
        };
        let feed = links::feed_on(bus.connect().await, tailscale, pinger);
        Self {
            published,
            demand,
            _config: config,
            task: tokio::spawn(feed.run(context)),
        }
    }

    fn unwant(&self) {
        self.demand.send_replace(BTreeSet::new());
    }

    /// Waits on the wall clock rather than tokio's: under a paused clock tokio jumps ahead
    /// whenever it would wait on the private bus, which runs in another process. Spinning keeps
    /// the clock still while the bus answers.
    async fn next_snapshot(&mut self) -> LinksSnapshot {
        let deadline = std::time::Instant::now() + BUS_WAIT;
        loop {
            match self.published.try_recv() {
                Ok((topic, data)) => {
                    assert_eq!(topic, topics::NET_LINKS);
                    return serde_json::from_value(data).unwrap();
                }
                Err(TryRecvError::Empty) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "nothing was published in time"
                    );
                    yield_now().await;
                }
                Err(TryRecvError::Disconnected) => panic!("the feed stopped"),
            }
        }
    }

    async fn snapshot_where(&mut self, wanted: impl Fn(&LinksSnapshot) -> bool) -> LinksSnapshot {
        loop {
            let snapshot = self.next_snapshot().await;
            if wanted(&snapshot) {
                return snapshot;
            }
        }
    }

    /// The last snapshot published so far.
    fn latest(&mut self) -> Option<LinksSnapshot> {
        let mut latest = None;
        while let Ok((_, data)) = self.published.try_recv() {
            latest = Some(serde_json::from_value(data).unwrap());
        }
        latest
    }
}

fn vpn<'a>(snapshot: &'a LinksSnapshot, name: &str) -> &'a VpnLink {
    snapshot
        .vpns
        .iter()
        .find(|vpn| vpn.name == name)
        .unwrap_or_else(|| panic!("no VPN named {name:?} in {snapshot:?}"))
}

fn has_links(snapshot: &LinksSnapshot) -> bool {
    !snapshot.lan.is_empty()
}

/// The first snapshot with LAN links from the feed over a fake NetworkManager of `shape`,
/// without Tailscale.
async fn links_with(bus: &PrivateBus, shape: NmShape) -> LinksSnapshot {
    let _nm = FakeNm::start(bus, shape).await;
    let (_dir, tailscale) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(bus, tailscale, pinger, PING_HOST).await;
    let snapshot = running.snapshot_where(has_links).await;
    running.task.abort();
    snapshot
}

#[tokio::test]
async fn lan_and_vpn_links_come_from_network_manager() {
    let bus = PrivateBus::start();
    let snap = links_with(&bus, NmShape::default()).await;

    assert_eq!(
        snap.lan,
        [LanLink {
            iface: "enp14s0".into(),
            address: Some("192.168.15.5/24".into()),
            speed_mbps: Some(1000),
            state: LinkState::Up,
        }]
    );
    assert_eq!(snap.vpns[0].name, VPN_NAME);
    assert_eq!(snap.vpns[0].kind, "openvpn");
    assert_eq!(snap.vpns[0].iface.as_deref(), Some("tun0"));
    assert_eq!(snap.vpns[0].state, LinkState::Up);
}

#[tokio::test]
async fn network_manager_methods_cannot_start_it() {
    let bus = PrivateBus::start();
    let nm = FakeNm::start(&bus, NmShape::default()).await;
    let (_dir, tailscale) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(&bus, tailscale, pinger, PING_HOST).await;

    let snap = running.snapshot_where(|snap| !snap.vpns.is_empty()).await;

    assert_eq!(
        snap.vpns[0].name, VPN_NAME,
        "the saved connections were read"
    );
    assert_eq!(nm.calls.auto_starting_methods(), 0);
}

#[tokio::test]
async fn a_saved_vpn_that_is_not_active_reads_down() {
    let bus = PrivateBus::start();
    let snap = links_with(&bus, NmShape::default()).await;

    let work = vpn(&snap, SAVED_VPN_NAME);
    assert_eq!(work.state, LinkState::Down);
    assert_eq!(
        (work.kind.as_str(), work.iface.as_deref()),
        ("wireguard", None)
    );
}

#[tokio::test]
async fn tun_connections_are_not_vpns() {
    let bus = PrivateBus::start();
    // NM lists tailscale0 and tun0 as saved and active connections of type tun.
    let snap = links_with(&bus, NmShape::default()).await;

    let names: Vec<&str> = snap.vpns.iter().map(|vpn| vpn.name.as_str()).collect();
    assert_eq!(names, [VPN_NAME, SAVED_VPN_NAME]);
    assert!(
        snap.vpns
            .iter()
            .all(|v| v.iface.as_deref() != Some("tailscale0"))
    );
}

#[tokio::test]
async fn a_vpn_whose_ip_config_matches_no_device_has_no_iface() {
    let bus = PrivateBus::start();
    let shape = NmShape {
        tun0_carries_the_vpn_config: false,
        ..NmShape::default()
    };
    let snap = links_with(&bus, shape).await;

    assert_eq!(vpn(&snap, VPN_NAME).iface, None);
    assert_eq!(vpn(&snap, VPN_NAME).state, LinkState::Up);
}

#[tokio::test]
async fn a_device_without_the_wired_interface_has_no_speed() {
    let bus = PrivateBus::start();
    let shape = NmShape {
        wired_speed: false,
        ..NmShape::default()
    };
    let snap = links_with(&bus, shape).await;

    assert_eq!(snap.lan[0].speed_mbps, None);
    assert_eq!(snap.lan[0].address.as_deref(), Some("192.168.15.5/24"));
}

#[tokio::test]
async fn the_first_snapshot_already_has_network_managers_links() {
    let bus = PrivateBus::start();
    let _nm = FakeNm::start(&bus, NmShape::default()).await;
    let (_dir, tailscale) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(&bus, tailscale, pinger, PING_HOST).await;

    let first = running.next_snapshot().await;

    assert_eq!((first.lan.len(), first.vpns.len()), (1, 2));
}

#[tokio::test]
async fn an_active_connection_changing_state_is_read_again() {
    let bus = PrivateBus::start();
    let shape = NmShape {
        vpn_state: ACTIVATING,
        ..NmShape::default()
    };
    let nm = FakeNm::start(&bus, shape).await;
    let (_dir, tailscale) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(&bus, tailscale, pinger, PING_HOST).await;
    let before = running.snapshot_where(has_links).await;
    assert_eq!(vpn(&before, VPN_NAME).state, LinkState::Connecting);

    nm.set_vpn_state(ACTIVE_ACTIVATED).await;

    let after = running
        .snapshot_where(|snap| vpn(snap, VPN_NAME).state == LinkState::Up)
        .await;
    assert_eq!(vpn(&after, VPN_NAME).iface.as_deref(), Some("tun0"));
}

#[tokio::test(start_paused = true)]
async fn network_manager_appearing_later_is_read_in_full() {
    let bus = PrivateBus::start();
    let nm = FakeNm::serve(&bus, NmShape::default()).await;
    let (_dir, tailscale) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(&bus, tailscale, pinger, PING_HOST).await;
    let before = running.next_snapshot().await;

    sleep(NM_LATE_BY).await;
    nm.claim().await;

    let after = running.snapshot_where(has_links).await;
    assert!(before.lan.is_empty() && before.vpns.is_empty());
    assert_eq!((after.lan.len(), after.vpns.len()), (1, 2));
    assert!(nm.calls.count() > 0);
}

#[tokio::test(start_paused = true)]
async fn network_manager_absent_publishes_empty_lists_and_keeps_running() {
    let bus = PrivateBus::start();
    let (_dir, tailscale) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(&bus, tailscale, Arc::clone(&pinger), PING_HOST).await;
    let first = running.next_snapshot().await;

    sleep(ABSENT_FOR).await;

    let snap = running.latest().unwrap_or(first);
    assert!(snap.lan.is_empty() && snap.vpns.is_empty());
    assert_eq!(snap.ping_ms, Some(PING_RTT.as_secs_f32() * 1000.0));
    assert!(!running.task.is_finished());
    assert!(pinger.count() >= LEAST_PINGS_IN_30_S);
}

#[tokio::test]
async fn tailscale_status_counts_online_peers() {
    let bus = PrivateBus::start();
    let tailscale = FakeTailscale::start();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(&bus, tailscale.client(), pinger, PING_HOST).await;

    let snap = running
        .snapshot_where(|snap| snap.tailscale.is_some())
        .await;

    let t = snap.tailscale.unwrap();
    assert_eq!(
        (t.peers_online, t.peers_total, t.state),
        (1, 2, LinkState::Up)
    );
    assert_eq!(t.address.as_deref(), Some("100.64.0.1"));
}

#[tokio::test]
async fn tailscale_is_asked_for_its_status_over_http_1_0() {
    let tailscale = FakeTailscale::start();

    let status = tailscale.client().status().await.unwrap();

    assert_eq!(tailscale.requests(), [TAILSCALE_REQUEST]);
    assert_eq!(status.backend_state, "Running");
    assert_eq!(
        status.self_ips,
        [
            "100.64.0.1".parse::<IpAddr>().unwrap(),
            "fd7a:115c:a1e0::1".parse().unwrap()
        ]
    );
}

#[tokio::test]
async fn a_peer_without_a_direct_path_has_no_current_address() {
    let tailscale = FakeTailscale::start();

    let status = tailscale.client().status().await.unwrap();

    assert_eq!(
        status.peers,
        [
            TailscalePeer {
                host_name: "phone".into(),
                online: true,
                cur_addr: Some("203.0.113.7:41641".parse::<SocketAddr>().unwrap()),
            },
            TailscalePeer {
                host_name: "laptop".into(),
                online: false,
                cur_addr: None,
            },
        ]
    );
}

/// The fixture, padded with trailing spaces so that the whole reply, header included, is `bytes`
/// long.
fn status_reply_of(bytes: u64) -> String {
    let body_bytes = usize::try_from(bytes).unwrap() - FakeTailscale::HEADER.len();
    let padding = " ".repeat(body_bytes - TAILSCALE_STATUS.len());
    format!("{TAILSCALE_STATUS}{padding}")
}

#[tokio::test]
async fn a_reply_at_the_cap_still_parses() {
    let tailscale = FakeTailscale::start_with(status_reply_of(MAX_REPLY_BYTES));

    let status = tailscale.client().status().await;

    assert_eq!(
        status.map(|status| status.backend_state).as_deref(),
        Some("Running")
    );
}

#[tokio::test]
async fn a_reply_over_the_cap_reads_none() {
    let tailscale = FakeTailscale::start_with(status_reply_of(MAX_REPLY_BYTES + 1));

    assert!(tailscale.client().status().await.is_none());
}

#[tokio::test]
async fn a_missing_tailscale_socket_reads_none() {
    let bus = PrivateBus::start();
    let (_dir, tailscale) = missing_tailscale();
    let (_other_dir, probe) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(&bus, tailscale, pinger, PING_HOST).await;

    let snap = running.snapshot_where(|snap| snap.ping_ms.is_some()).await;

    assert!(snap.tailscale.is_none());
    assert!(probe.status().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn nothing_runs_while_unwanted() {
    let bus = PrivateBus::start();
    let nm = FakeNm::start(&bus, NmShape::default()).await;
    let tailscale = FakeTailscale::start();
    let pinger = Arc::new(CountingPinger::default());
    let mut running =
        RunningLinks::start(&bus, tailscale.client(), Arc::clone(&pinger), PING_HOST).await;
    running.unwant();

    sleep(UNWANTED_FOR).await;

    assert_eq!(
        (nm.calls.count(), tailscale.requests().len(), pinger.count()),
        (0, 0, 0)
    );
    assert!(running.latest().is_none());
}

#[tokio::test(start_paused = true)]
async fn releasing_the_topic_stops_every_lane() {
    let bus = PrivateBus::start();
    let nm = FakeNm::start(&bus, NmShape::default()).await;
    let tailscale = FakeTailscale::start();
    let pinger = Arc::new(CountingPinger::default());
    let mut running =
        RunningLinks::start(&bus, tailscale.client(), Arc::clone(&pinger), PING_HOST).await;
    running
        .snapshot_where(|snap| has_links(snap) && snap.tailscale.is_some())
        .await;

    running.unwant();
    sleep(PING_EVERY).await;
    let released = (nm.calls.count(), tailscale.requests().len(), pinger.count());
    sleep(UNWANTED_FOR).await;

    assert_eq!(
        (nm.calls.count(), tailscale.requests().len(), pinger.count()),
        released
    );
    assert!(!running.task.is_finished());
}

#[tokio::test(start_paused = true)]
async fn an_empty_ping_host_never_pings() {
    let bus = PrivateBus::start();
    let (_dir, tailscale) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let mut running = RunningLinks::start(&bus, tailscale, Arc::clone(&pinger), "").await;
    let first = running.next_snapshot().await;

    sleep(ABSENT_FOR).await;

    let snap = running.latest().unwrap_or(first);
    assert_eq!(pinger.count(), 0);
    assert!(snap.ping_ms.is_none());
}

#[tokio::test(start_paused = true)]
async fn the_configured_host_is_pinged_every_five_seconds() {
    let bus = PrivateBus::start();
    let (_dir, tailscale) = missing_tailscale();
    let pinger = Arc::new(CountingPinger::default());
    let _running = RunningLinks::start(&bus, tailscale, Arc::clone(&pinger), PING_HOST).await;

    sleep(PING_EVERY * 3 + PING_EVERY / 2).await;

    let host: Ipv4Addr = PING_HOST.parse().unwrap();
    assert_eq!(*pinger.hosts.lock().unwrap(), [host; 4]);
}
