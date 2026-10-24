mod common;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::IpAddr;
use std::os::unix::fs::symlink;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::FakeRoot;
use common::tailscale::{FakeTailscale, TAILSCALE_STATUS, missing_tailscale};
use ghost_proto::{ResolvedConfig, UplinkSnapshot, topics};
use ghostd::feeds::tailscale::TailscaleClient;
use ghostd::feeds::uplink::geo::Geo;
use ghostd::feeds::uplink::owners::Owners;
use ghostd::feeds::uplink::sockets::{RemoteSocket, SocketSource, is_public, parse_proc_net};
use ghostd::feeds::uplink::{self, label_for};
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::sleep;

const TCP_FIXTURE: &str = include_str!("fixtures/proc-net-tcp.txt");
const TCP6_FIXTURE: &str = include_str!("fixtures/proc-net-tcp6.txt");
const UDP_FIXTURE: &str = include_str!("fixtures/proc-net-udp.txt");
const UDP6_FIXTURE: &str = include_str!("fixtures/proc-net-udp6.txt");

const USER_UID: u32 = 1000;
const ROOT_UID: u32 = 0;
const SPOTIFY_INODE: u64 = 5_123_456;
const UNWANTED_FOR: Duration = Duration::from_secs(60);
/// The fixture's direct tailnet peer, `phone`, at its `CurAddr`, which as a documentation
/// address is never shown.
const FIXTURE_PHONE_ADDR: &str = "203.0.113.7";
/// A public address for `phone` to be reachable at instead.
const PHONE_ADDR: &str = "8.8.4.4";
const RELEASE_WAIT: Duration = Duration::from_secs(10);
const UPLINK_EVERY: Duration = Duration::from_secs(5);
const AKAMAI: (u32, &str) = (20940, "Akamai International B.V.");
const SAO_PAULO: (f32, f32) = (-23.55, -46.63);

fn ip() -> IpAddr {
    "1.1.1.1".parse().unwrap()
}

fn ovr(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn established_tcp_and_connected_udp_are_kept() {
    let s = parse_proc_net(TCP_FIXTURE, false, false);
    assert_eq!(s.len(), 4);
    assert_eq!(
        s[0],
        RemoteSocket {
            remote: "192.0.2.10".parse().unwrap(),
            uid: 1000,
            inode: 17_634_145
        }
    );
    // The connected DHCP row has a remote; the three unconnected ones do not.
    let udp = parse_proc_net(UDP_FIXTURE, false, true);
    assert_eq!(
        udp,
        [RemoteSocket {
            remote: "192.168.15.1".parse().unwrap(),
            uid: ROOT_UID,
            inode: 8826
        }]
    );
    let udp6 = parse_proc_net(UDP6_FIXTURE, true, true);
    assert_eq!(
        udp6,
        [RemoteSocket {
            remote: "2001:db8:7::7".parse().unwrap(),
            uid: USER_UID,
            inode: 17_486_768
        }]
    );
}

#[test]
fn a_v4_mapped_v6_remote_reads_as_v4() {
    let tcp6 = parse_proc_net(TCP6_FIXTURE, true, false);
    assert!(
        tcp6.iter()
            .any(|s| s.remote == "203.0.113.9".parse::<IpAddr>().unwrap())
    );
    // Five established rows: four native IPv6 and the mapped one.
    assert_eq!(tcp6.len(), 5);
    assert!(
        tcp6.iter()
            .any(|s| s.remote == "2001:db8:2::2".parse::<IpAddr>().unwrap())
    );
}

#[test]
fn private_and_special_ranges_are_never_shown() {
    for ip in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "192.168.15.5",
        "169.254.1.1",
        "100.122.159.93",
        "224.0.0.1",
        "255.255.255.255",
        "0.0.0.0",
        "192.0.2.10",
        "198.51.100.11",
        "203.0.113.9",
        "2001:db8::1",
        "fd7a:115c::1",
        "fe80::1",
        "ff02::1",
        "::1",
        "::",
        "::ffff:10.1.2.3",
        "::ffff:192.168.15.5",
    ] {
        assert!(!is_public(&ip.parse().unwrap()), "{ip}");
    }
    assert!(is_public(&"1.1.1.1".parse().unwrap()));
    assert!(is_public(&"::ffff:1.1.1.1".parse().unwrap()));
    assert!(is_public(&"2606:4700::1111".parse().unwrap()));
    // Just outside CGNAT on both sides.
    assert!(is_public(&"100.63.255.255".parse().unwrap()));
    assert!(is_public(&"100.128.0.0".parse().unwrap()));
}

#[test]
fn an_override_by_asn_beats_one_by_name() {
    assert_eq!(
        label_for(
            Some(&(16509, "Amazon.com, Inc.".into())),
            &ovr(&[("AS16509", "AWS"), ("amazon.com, inc.", "process")]),
            None,
            ip()
        ),
        "AWS"
    );
}

#[test]
fn process_overrides_fall_back_to_the_org() {
    let akamai = (AKAMAI.0, AKAMAI.1.to_owned());
    assert_eq!(
        label_for(
            Some(&akamai),
            &ovr(&[("AKAMAI INTERNATIONAL B.V.", "process")]),
            None,
            ip()
        ),
        "AKAMAI INTERNATIONAL B.V."
    );
    assert_eq!(
        label_for(
            Some(&akamai),
            &ovr(&[("akamai international b.v.", "process")]),
            Some("spotify"),
            ip()
        ),
        "SPOTIFY"
    );
}

#[test]
fn fixed_text_is_used_as_is_and_no_override_uppercases_the_org() {
    let akamai = (AKAMAI.0, AKAMAI.1.to_owned());
    assert_eq!(
        label_for(Some(&akamai), &ovr(&[("as20940", "Cdn")]), None, ip()),
        "Cdn"
    );
    assert_eq!(
        label_for(Some(&akamai), &ovr(&[]), Some("spotify"), ip()),
        "AKAMAI INTERNATIONAL B.V."
    );
}

#[test]
fn the_label_chain_ends_at_the_ip() {
    assert_eq!(
        label_for(None, &ovr(&[]), None, "9.9.9.9".parse().unwrap()),
        "9.9.9.9"
    );
    assert_eq!(
        label_for(Some(&(64496, String::new())), &ovr(&[]), None, ip()),
        "AS64496"
    );
}

fn write_status(root: &FakeRoot, pid: u32, uid: u32) {
    root.write(
        &format!("proc/{pid}/status"),
        &format!("Name:\tx\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\nGid:\t{uid}\t{uid}\t{uid}\t{uid}\n"),
    );
}

/// `pid` owned by `uid`, named `comm`, holding `inode` at fd 3.
fn write_socket_owner(root: &FakeRoot, pid: u32, uid: u32, comm: &str, inode: u64) {
    write_status(root, pid, uid);
    root.write(&format!("proc/{pid}/comm"), &format!("{comm}\n"));
    root.write(&format!("proc/{pid}/fd/.keep"), "");
    let link = root.root().path(&format!("proc/{pid}/fd/3"));
    symlink(format!("socket:[{inode}]"), link).unwrap();
}

#[test]
fn owners_are_found_through_fd_links_and_only_for_the_socket_uid() {
    let root = FakeRoot::new();
    write_socket_owner(&root, 42, USER_UID, "spotify", SPOTIFY_INODE);
    write_socket_owner(&root, 43, ROOT_UID, "sshd", SPOTIFY_INODE);
    let mut owners = Owners::new(root.root());
    assert_eq!(
        owners.names(&[(SPOTIFY_INODE, USER_UID)])[&SPOTIFY_INODE],
        "spotify"
    );
    let mut owners = Owners::new(root.root());
    assert_eq!(
        owners.names(&[(SPOTIFY_INODE, ROOT_UID)])[&SPOTIFY_INODE],
        "sshd"
    );
    // No process of uid 7 holds it.
    let mut owners = Owners::new(root.root());
    assert!(owners.names(&[(SPOTIFY_INODE, 7)]).is_empty());
}

#[test]
fn owners_are_cached_and_pruned_to_the_current_inodes() {
    let root = FakeRoot::new();
    write_socket_owner(&root, 42, USER_UID, "spotify", SPOTIFY_INODE);
    let mut owners = Owners::new(root.root());
    assert_eq!(owners.names(&[(SPOTIFY_INODE, USER_UID)]).len(), 1);
    // The process is gone, but the socket is still known, so the cache answers.
    root.remove("proc/42");
    assert_eq!(
        owners.names(&[(SPOTIFY_INODE, USER_UID)])[&SPOTIFY_INODE],
        "spotify"
    );
    // Once the socket closes, the cache forgets it.
    assert!(owners.names(&[]).is_empty());
    assert!(owners.names(&[(SPOTIFY_INODE, USER_UID)]).is_empty());
}

/// Sockets handed to the feed, counting how often it reads them.
struct CountingSockets {
    sockets: Vec<RemoteSocket>,
    reads: AtomicUsize,
}

impl CountingSockets {
    fn new(sockets: Vec<RemoteSocket>) -> Arc<Self> {
        Arc::new(Self {
            sockets,
            reads: AtomicUsize::new(0),
        })
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

impl SocketSource for CountingSockets {
    fn sockets(&self) -> Vec<RemoteSocket> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.sockets.clone()
    }
}

/// A database of a few addresses, counting its lookups.
#[derive(Default)]
struct FakeGeo {
    cities: HashMap<IpAddr, (f32, f32)>,
    asns: HashMap<IpAddr, (u32, String)>,
    attribution: &'static str,
    lookups: AtomicUsize,
    releases: AtomicUsize,
}

impl FakeGeo {
    fn lookups(&self) -> usize {
        self.lookups.load(Ordering::SeqCst)
    }

    fn releases(&self) -> usize {
        self.releases.load(Ordering::SeqCst)
    }
}

impl Geo for FakeGeo {
    fn city(&self, ip: IpAddr) -> Option<(f32, f32)> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        self.cities.get(&ip).copied()
    }

    fn asn(&self, ip: IpAddr) -> Option<(u32, String)> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        self.asns.get(&ip).cloned()
    }

    fn attribution(&self) -> &'static str {
        self.attribution
    }

    fn refresh(&self) {}

    fn release(&self) {
        self.releases.fetch_add(1, Ordering::SeqCst);
    }
}

fn socket(remote: &str, inode: u64) -> RemoteSocket {
    RemoteSocket {
        remote: remote.parse().unwrap(),
        uid: USER_UID,
        inode,
    }
}

/// The real feed over injected sockets and database, and everything it publishes.
struct RunningUplink {
    published: mpsc::UnboundedReceiver<(String, Value)>,
    demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    task: JoinHandle<()>,
}

impl RunningUplink {
    fn start(
        sockets: Arc<CountingSockets>,
        geo: Arc<FakeGeo>,
        owners: Owners,
        tailscale: TailscaleClient,
        config: ResolvedConfig,
        wanted: bool,
    ) -> Self {
        let (published_sender, published) = mpsc::unbounded_channel();
        let demand = if wanted {
            BTreeSet::from([topics::UPLINK.to_owned()])
        } else {
            BTreeSet::new()
        };
        let (demand, demand_receiver) = watch::channel(demand);
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
        let feed = uplink::feed_on(sockets, geo, owners, tailscale);
        Self {
            published,
            demand,
            _config: config,
            task: tokio::spawn(feed.run(context)),
        }
    }

    fn want(&self, wanted: bool) {
        let topics = if wanted {
            BTreeSet::from([topics::UPLINK.to_owned()])
        } else {
            BTreeSet::new()
        };
        self.demand.send_replace(topics);
    }

    async fn next_snapshot(&mut self) -> UplinkSnapshot {
        let (topic, data) = self.published.recv().await.expect("the feed stopped");
        assert_eq!(topic, topics::UPLINK);
        serde_json::from_value(data).unwrap()
    }
}

impl Drop for RunningUplink {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The first snapshot of the feed over `sockets` and `geo`, without Tailscale.
async fn first_snapshot(sockets: Vec<RemoteSocket>, geo: FakeGeo) -> UplinkSnapshot {
    let (_dir, tailscale) = missing_tailscale();
    let root = FakeRoot::new();
    let mut uplink = RunningUplink::start(
        CountingSockets::new(sockets),
        Arc::new(geo),
        Owners::new(root.root()),
        tailscale,
        ResolvedConfig::default(),
        true,
    );
    uplink.next_snapshot().await
}

#[tokio::test(start_paused = true)]
async fn endpoints_group_rank_and_cut_at_eight() {
    let mut geo = FakeGeo {
        attribution: "IP GEOLOCATION BY DB-IP",
        ..FakeGeo::default()
    };
    let mut sockets = Vec::new();
    let mut inode = 1;
    for k in 1..=12u8 {
        let remote: IpAddr = format!("9.9.{k}.1").parse().unwrap();
        geo.cities.insert(remote, (f32::from(k), f32::from(k)));
        geo.asns
            .insert(remote, (u32::from(k), format!("Org {k:02}")));
        for _ in 0..k {
            sockets.push(RemoteSocket {
                remote,
                uid: USER_UID,
                inode,
            });
            inode += 1;
        }
    }
    let snap = first_snapshot(sockets, geo).await;
    assert_eq!((snap.endpoints.len(), snap.total), (8, 12));
    assert_eq!(
        snap.endpoints.iter().map(|e| e.sockets).collect::<Vec<_>>(),
        [12, 11, 10, 9, 8, 7, 6, 5]
    );
    assert_eq!(snap.endpoints[0].label, "ORG 12");
    assert_eq!((snap.endpoints[0].lat, snap.endpoints[0].lon), (12.0, 12.0));
    assert_eq!(snap.attribution, "IP GEOLOCATION BY DB-IP");
}

#[tokio::test(start_paused = true)]
async fn private_remotes_are_left_out_and_equal_counts_sort_by_label() {
    let mut geo = FakeGeo::default();
    for (remote, org) in [
        ("1.1.1.1", "Zeta"),
        ("9.9.9.9", "Alpha"),
        ("192.168.15.1", "Lan"),
    ] {
        let remote: IpAddr = remote.parse().unwrap();
        geo.cities.insert(remote, SAO_PAULO);
        geo.asns.insert(remote, (1, org.to_owned()));
    }
    let snap = first_snapshot(
        vec![
            socket("1.1.1.1", 1),
            socket("9.9.9.9", 2),
            socket("192.168.15.1", 3),
            socket("::ffff:192.168.15.1", 4),
        ],
        geo,
    )
    .await;
    assert_eq!(
        snap.endpoints
            .iter()
            .map(|e| e.label.as_str())
            .collect::<Vec<_>>(),
        ["ALPHA", "ZETA"]
    );
    assert_eq!(snap.total, 2);
}

#[tokio::test(start_paused = true)]
async fn a_process_override_labels_the_endpoint_by_its_owner() {
    let remote: IpAddr = "1.1.1.1".parse().unwrap();
    let mut geo = FakeGeo::default();
    geo.cities.insert(remote, SAO_PAULO);
    geo.asns.insert(remote, (AKAMAI.0, AKAMAI.1.to_owned()));
    let root = FakeRoot::new();
    write_socket_owner(&root, 42, USER_UID, "spotify", SPOTIFY_INODE);
    let mut config = ResolvedConfig::default();
    config.uplink.labels = ovr(&[("akamai international b.v.", "process")]);
    let (_dir, tailscale) = missing_tailscale();
    let mut uplink = RunningUplink::start(
        CountingSockets::new(vec![socket("1.1.1.1", SPOTIFY_INODE)]),
        Arc::new(geo),
        Owners::new(root.root()),
        tailscale,
        config,
        true,
    );
    let snap = uplink.next_snapshot().await;
    assert_eq!(snap.endpoints.len(), 1);
    assert_eq!(snap.endpoints[0].label, "SPOTIFY");
}

#[tokio::test(start_paused = true)]
async fn a_direct_tailnet_peer_is_located_and_a_relayed_one_is_not() {
    let mut geo = FakeGeo::default();
    geo.cities.insert(PHONE_ADDR.parse().unwrap(), SAO_PAULO);
    let tailscale =
        FakeTailscale::start_with(TAILSCALE_STATUS.replace(FIXTURE_PHONE_ADDR, PHONE_ADDR));
    let root = FakeRoot::new();
    let mut uplink = RunningUplink::start(
        CountingSockets::new(Vec::new()),
        Arc::new(geo),
        Owners::new(root.root()),
        tailscale.client(),
        ResolvedConfig::default(),
        true,
    );
    let snap = uplink.next_snapshot().await;
    assert!(
        snap.endpoints
            .iter()
            .any(|e| e.tailnet && e.label == "TAILNET:PHONE")
    );
    assert_eq!(snap.endpoints.iter().filter(|e| e.tailnet).count(), 1);
    assert_eq!(snap.total, 1);
}

#[tokio::test(start_paused = true)]
async fn no_database_still_publishes() {
    let snap = first_snapshot(
        vec![socket("1.1.1.1", 1), socket("9.9.9.9", 2)],
        FakeGeo::default(),
    )
    .await;
    assert_eq!((snap.endpoints.len(), snap.attribution.as_str()), (0, ""));
    // Both remotes are still counted, each under its own address.
    assert_eq!(snap.total, 2);
}

#[tokio::test(start_paused = true)]
async fn nothing_is_read_while_unwanted() {
    let sockets = CountingSockets::new(vec![socket("1.1.1.1", 1)]);
    let geo = Arc::new(FakeGeo::default());
    let tailscale = FakeTailscale::start();
    let root = FakeRoot::new();
    let mut uplink = RunningUplink::start(
        Arc::clone(&sockets),
        Arc::clone(&geo),
        Owners::new(root.root()),
        tailscale.client(),
        ResolvedConfig::default(),
        false,
    );
    sleep(UNWANTED_FOR).await;
    assert_eq!((geo.lookups(), sockets.reads()), (0, 0));
    assert!(tailscale.requests().is_empty());
    assert!(uplink.published.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn a_tailnet_peer_at_a_non_public_address_is_not_shown() {
    let mut geo = FakeGeo::default();
    // Even a database that places the address cannot put it on the globe.
    geo.cities
        .insert(FIXTURE_PHONE_ADDR.parse().unwrap(), SAO_PAULO);
    let tailscale = FakeTailscale::start();
    let root = FakeRoot::new();
    let mut uplink = RunningUplink::start(
        CountingSockets::new(Vec::new()),
        Arc::new(geo),
        Owners::new(root.root()),
        tailscale.client(),
        ResolvedConfig::default(),
        true,
    );
    let snap = uplink.next_snapshot().await;
    assert!(snap.endpoints.is_empty());
    assert_eq!(snap.total, 0);
}

#[tokio::test(start_paused = true)]
async fn the_databases_are_released_while_unwanted_and_used_again_once_wanted() {
    let mut geo = FakeGeo::default();
    geo.cities.insert(ip(), SAO_PAULO);
    let geo = Arc::new(geo);
    let (_dir, tailscale) = missing_tailscale();
    let root = FakeRoot::new();
    let mut uplink = RunningUplink::start(
        CountingSockets::new(vec![socket("1.1.1.1", 1)]),
        Arc::clone(&geo),
        Owners::new(root.root()),
        tailscale,
        ResolvedConfig::default(),
        true,
    );
    uplink.next_snapshot().await;
    assert_eq!(geo.releases(), 0);

    uplink.want(false);
    sleep(RELEASE_WAIT).await;
    assert_eq!(geo.releases(), 1);
    let lookups_when_released = geo.lookups();
    sleep(UNWANTED_FOR).await;
    assert_eq!(geo.lookups(), lookups_when_released);

    uplink.want(true);
    sleep(UPLINK_EVERY).await;
    assert!(geo.lookups() > lookups_when_released);
    assert_eq!(geo.releases(), 1);
}
