mod common;

use std::time::Duration;

use common::FakeRoot;
use ghost_proto::{NetIface, NetSnapshot};
use ghostd::collect::NetCollector;

const ONE_SECOND: Duration = Duration::from_secs(1);
const PROC_NET_DEV: &str = include_str!("fixtures/proc-net-dev.txt");
const PROC_NET_ROUTE: &str = include_str!("fixtures/proc-net-route.txt");
const DEV_HEADER_LINES: usize = 2;

// `RTF_UP | RTF_GATEWAY`, as on the fixture's default route.
const ROUTE_UP: &str = "0003";
// `RTF_GATEWAY` alone: the route exists but is not up.
const ROUTE_NOT_UP: &str = "0002";
const ANY_ADDRESS: &str = "00000000";
const SPLIT_DESTINATION: &str = "0000000A";
const SPLIT_MASK: &str = "0000FFFF";

const ENP_RX_START: u64 = 25_420_914_177;
const ENP_TX_START: u64 = 69_487_781_121;
const GIGABIT_BPS: u64 = 125_000_000;

/// Writes `/proc/net/dev` in the kernel's layout: the fixture's two header lines, then one row per
/// interface with its name right-aligned and the counters it does not set at zero.
fn write_dev(fake: &FakeRoot, interfaces: &[(&str, u64, u64)]) {
    let mut text: String = PROC_NET_DEV
        .lines()
        .take(DEV_HEADER_LINES)
        .map(|line| format!("{line}\n"))
        .collect();
    for (name, rx, tx) in interfaces {
        text.push_str(&format!(
            "{name:>6}: {rx} 0 0 0 0 0 0 0 {tx} 0 0 0 0 0 0 0\n"
        ));
    }
    fake.write("proc/net/dev", &text);
}

/// One `/proc/net/route` row: interface, destination, flags, metric and mask.
type Route<'a> = (&'a str, &'a str, &'a str, u32, &'a str);

fn write_routes(fake: &FakeRoot, routes: &[Route]) {
    let header = PROC_NET_ROUTE.lines().next().unwrap();
    let mut text = format!("{header}\n");
    for (iface, destination, flags, metric, mask) in routes {
        text.push_str(&format!(
            "{iface}\t{destination}\t00000000\t{flags}\t0\t0\t{metric}\t{mask}\t0\t0\t0\n"
        ));
    }
    fake.write("proc/net/route", &text);
}

fn write_link(fake: &FakeRoot, iface: &str, operstate: &str, flags: &str) {
    fake.write(
        &format!("sys/class/net/{iface}/operstate"),
        &format!("{operstate}\n"),
    );
    fake.write(
        &format!("sys/class/net/{iface}/flags"),
        &format!("{flags}\n"),
    );
}

/// This machine as the fixtures show it: enp14s0 is the default route, tun0 has split routes only.
fn machine() -> FakeRoot {
    let fake = FakeRoot::new();
    fake.write("proc/net/route", PROC_NET_ROUTE);
    fake.write("proc/net/dev", PROC_NET_DEV);
    write_link(&fake, "lo", "unknown", "0x9");
    write_link(&fake, "enp14s0", "up", "0x1003");
    write_link(&fake, "tailscale0", "unknown", "0x11d1");
    write_link(&fake, "tun0", "unknown", "0x1091");
    fake
}

fn iface<'a>(snapshot: &'a NetSnapshot, name: &str) -> &'a NetIface {
    snapshot
        .interfaces
        .iter()
        .find(|iface| iface.name == name)
        .unwrap_or_else(|| panic!("no {name} in {snapshot:?}"))
}

fn names(snapshot: &NetSnapshot) -> Vec<&str> {
    snapshot
        .interfaces
        .iter()
        .map(|iface| iface.name.as_str())
        .collect()
}

#[test]
fn rates_follow_the_default_route() {
    let fake = machine();
    write_dev(&fake, &[("enp14s0", 0, 0), ("tun0", 50_877, 558_731)]);
    let mut net = NetCollector::new(&fake.root());

    net.sample_after(ONE_SECOND);
    write_dev(
        &fake,
        &[("enp14s0", GIGABIT_BPS, 4_000), ("tun0", 50_877, 558_731)],
    );
    let s = net.sample_after(ONE_SECOND);

    assert_eq!(s.default_iface.as_deref(), Some("enp14s0"));
    assert_eq!(s.rx_bps, 125_000_000);
    assert_eq!(s.tx_bps, 4_000);
    assert_eq!((iface(&s, "tun0").rx_bps, iface(&s, "tun0").tx_bps), (0, 0));
}

#[test]
fn rates_are_bytes_over_the_time_that_passed() {
    let fake = machine();
    write_dev(&fake, &[("enp14s0", 0, 0)]);
    let mut net = NetCollector::new(&fake.root());

    net.sample_after(ONE_SECOND);
    write_dev(&fake, &[("enp14s0", 3_000, 1_000)]);
    let s = net.sample_after(2 * ONE_SECOND);

    assert_eq!((s.rx_bps, s.tx_bps), (1_500, 500));
}

#[test]
fn the_lowest_metric_default_route_wins() {
    let fake = machine();
    write_routes(
        &fake,
        &[
            ("enp14s0", ANY_ADDRESS, ROUTE_UP, 100, ANY_ADDRESS),
            ("tun0", ANY_ADDRESS, ROUTE_UP, 50, ANY_ADDRESS),
        ],
    );
    write_dev(&fake, &[("enp14s0", 0, 0), ("tun0", 0, 0)]);
    let mut net = NetCollector::new(&fake.root());

    net.sample_after(ONE_SECOND);
    write_dev(&fake, &[("enp14s0", 9_000, 9_000), ("tun0", 2_000, 1_000)]);
    let s = net.sample_after(ONE_SECOND);

    assert_eq!(s.default_iface.as_deref(), Some("tun0"));
    assert_eq!((s.rx_bps, s.tx_bps), (2_000, 1_000));
}

#[test]
fn a_default_route_that_is_not_up_is_passed_over() {
    let fake = machine();
    write_routes(
        &fake,
        &[
            ("tun0", ANY_ADDRESS, ROUTE_NOT_UP, 50, ANY_ADDRESS),
            ("enp14s0", ANY_ADDRESS, ROUTE_UP, 100, ANY_ADDRESS),
        ],
    );
    let mut net = NetCollector::new(&fake.root());

    let s = net.sample_after(ONE_SECOND);

    assert_eq!(s.default_iface.as_deref(), Some("enp14s0"));
}

#[test]
fn a_counter_that_goes_backwards_reads_zero_not_a_spike() {
    let fake = machine();
    write_dev(&fake, &[("enp14s0", 9_000_000_000, 7_000_000_000)]);
    let mut net = NetCollector::new(&fake.root());

    net.sample_after(ONE_SECOND);
    write_dev(&fake, &[("enp14s0", 1_000, 7_000_000_500)]);
    let s = net.sample_after(ONE_SECOND);
    write_dev(&fake, &[("enp14s0", 4_000, 7_000_001_000)]);
    let after = net.sample_after(ONE_SECOND);

    assert_eq!(iface(&s, "enp14s0").rx_bps, 0);
    assert_eq!(s.rx_bps, 0);
    assert_eq!(iface(&s, "enp14s0").tx_bps, 500);
    // The reset counter is the baseline from then on.
    assert_eq!(after.rx_bps, 3_000);
}

#[test]
fn an_interface_that_appears_reads_zero_first() {
    let fake = machine();
    write_dev(&fake, &[("enp14s0", ENP_RX_START, ENP_TX_START)]);
    let mut net = NetCollector::new(&fake.root());

    let before = net.sample_after(ONE_SECOND);
    write_dev(
        &fake,
        &[
            ("enp14s0", ENP_RX_START, ENP_TX_START),
            ("tun0", 5_000_000_000, 5_000_000_000),
        ],
    );
    let s = net.sample_after(ONE_SECOND);

    assert_eq!(names(&before), ["enp14s0"]);
    assert_eq!(iface(&s, "tun0").rx_bps, 0);
    assert_eq!(iface(&s, "tun0").tx_bps, 0);
}

#[test]
fn an_interface_that_vanishes_and_returns_reads_zero_again() {
    let fake = machine();
    let with_tun = |rx: u64| [("enp14s0", 0, 0), ("tun0", rx, 0)];
    write_dev(&fake, &with_tun(1_000));
    let mut net = NetCollector::new(&fake.root());

    net.sample_after(ONE_SECOND);
    write_dev(&fake, &[("enp14s0", 0, 0)]);
    let gone = net.sample_after(ONE_SECOND);
    write_dev(&fake, &with_tun(8_000_000_000));
    let back = net.sample_after(ONE_SECOND);

    assert_eq!(names(&gone), ["enp14s0"]);
    assert_eq!(iface(&back, "tun0").rx_bps, 0);
}

#[test]
fn no_default_route_leaves_the_headline_at_zero() {
    let fake = machine();
    write_routes(
        &fake,
        &[("tun0", SPLIT_DESTINATION, ROUTE_UP, 50, SPLIT_MASK)],
    );
    write_dev(&fake, &[("enp14s0", 0, 0), ("tun0", 0, 0)]);
    let mut net = NetCollector::new(&fake.root());

    net.sample_after(ONE_SECOND);
    write_dev(&fake, &[("enp14s0", 5_000, 5_000), ("tun0", 5_000, 5_000)]);
    let s = net.sample_after(ONE_SECOND);

    assert_eq!(iface(&s, "tun0").rx_bps, 5_000);
    assert_eq!((s.default_iface, s.rx_bps, s.tx_bps), (None, 0, 0));
}

#[test]
fn unreadable_tables_read_as_no_interfaces_and_no_route() {
    let fake = FakeRoot::new();
    let mut net = NetCollector::new(&fake.root());

    let s = net.sample_after(ONE_SECOND);

    assert_eq!(s, NetSnapshot::default());
}

#[test]
fn loopback_is_left_out() {
    let fake = machine();
    let mut net = NetCollector::new(&fake.root());

    let s = net.sample_after(ONE_SECOND);

    assert_eq!(names(&s), ["enp14s0", "tailscale0", "tun0"]);
}

#[test]
fn a_tunnel_reporting_unknown_but_up_is_up() {
    let fake = machine();
    write_link(&fake, "tun0", "unknown", "0x1091");
    let mut net = NetCollector::new(&fake.root());

    let s = net.sample_after(ONE_SECOND);

    assert!(iface(&s, "tun0").oper_up);
    assert!(iface(&s, "tailscale0").oper_up);
}

#[test]
fn a_tunnel_reporting_unknown_with_up_clear_is_down() {
    let fake = machine();
    write_link(&fake, "tun0", "unknown", "0x1090");
    let mut net = NetCollector::new(&fake.root());

    let s = net.sample_after(ONE_SECOND);

    assert!(!iface(&s, "tun0").oper_up);
}

#[test]
fn operstate_up_is_up_and_down_is_down_whatever_the_flags() {
    let fake = machine();
    write_link(&fake, "tailscale0", "down", "0x1003");
    let mut net = NetCollector::new(&fake.root());

    let s = net.sample_after(ONE_SECOND);

    assert!(iface(&s, "enp14s0").oper_up);
    assert!(!iface(&s, "tailscale0").oper_up);
}
