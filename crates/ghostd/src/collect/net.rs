use std::collections::HashMap;
use std::time::{Duration, Instant};

use ghost_proto::{NetIface, NetSnapshot};

use super::{Source, bytes_per_second, read_text};
use crate::sysroot::SysRoot;

const PROC_NET_DEV: &str = "proc/net/dev";
const PROC_NET_ROUTE: &str = "proc/net/route";
const SYS_CLASS_NET: &str = "sys/class/net";
const OPERSTATE_FILE: &str = "operstate";
const FLAGS_FILE: &str = "flags";
const LOOPBACK: &str = "lo";

const OPERSTATE_UP: &str = "up";
// Tunnels and loopback have no carrier to report, so they read `unknown` while carrying traffic.
const OPERSTATE_UNKNOWN: &str = "unknown";
const IFF_UP: u32 = 0x1;

// `/proc/net/dev` opens with two header lines. After each `name:` come eight receive counters,
// then eight transmit counters, each list starting with bytes.
const DEV_HEADER_LINES: usize = 2;
const DEV_NAME_SEPARATOR: char = ':';
const COUNTERS_PER_DIRECTION: usize = 8;

const ROUTE_HEADER_LINES: usize = 1;
// A destination and mask of all zeroes make a default route.
const ANY_ADDRESS: &str = "00000000";
const RTF_UP: u32 = 0x1;

const HEX_PREFIX: &str = "0x";
const HEX_RADIX: u32 = 16;

// Holds either table in one read; a larger file grows its buffer once and keeps it.
const FILE_BUFFER_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy)]
struct Counters {
    rx: u64,
    tx: u64,
}

/// Rates of every interface in `/proc/net/dev` but loopback, with the default-route interface's as
/// the headline.
///
/// Every sample reads `/proc/net/dev`, `/proc/net/route` and each interface's `operstate`, and its
/// `flags` too when the operstate is `unknown`.
pub struct NetCollector {
    root: SysRoot,
    /// By interface name, from the previous sample only, so an interface that vanishes and comes
    /// back is new again.
    previous: HashMap<String, Counters>,
    last_sampled: Option<Instant>,
    dev: Vec<u8>,
    route: Vec<u8>,
}

impl NetCollector {
    pub fn new(root: &SysRoot) -> Self {
        Self {
            root: root.clone(),
            previous: HashMap::new(),
            last_sampled: None,
            dev: Vec::with_capacity(FILE_BUFFER_BYTES),
            route: Vec::with_capacity(FILE_BUFFER_BYTES),
        }
    }

    /// Samples with the time since the previous sample supplied by the caller.
    pub fn sample_after(&mut self, elapsed: Duration) -> NetSnapshot {
        let dev = read_table(&self.root, PROC_NET_DEV, &mut self.dev);
        let route = read_table(&self.root, PROC_NET_ROUTE, &mut self.route);
        let mut counters = HashMap::with_capacity(self.previous.len());
        let interfaces: Vec<NetIface> = parse_dev(dev)
            .filter(|(name, _)| *name != LOOPBACK)
            .map(|(name, now)| {
                let (rx_bps, tx_bps) = self.previous.get(name).map_or((0, 0), |before| {
                    (
                        bytes_per_second(now.rx, before.rx, elapsed),
                        bytes_per_second(now.tx, before.tx, elapsed),
                    )
                });
                counters.insert(name.to_owned(), now);
                NetIface {
                    name: name.to_owned(),
                    oper_up: is_oper_up(&self.root, name),
                    rx_bps,
                    tx_bps,
                }
            })
            .collect();
        self.previous = counters;

        let default_iface = default_route(route).map(str::to_owned);
        let (rx_bps, tx_bps) = default_iface
            .as_deref()
            .and_then(|name| interfaces.iter().find(|iface| iface.name == name))
            .map_or((0, 0), |iface| (iface.rx_bps, iface.tx_bps));
        NetSnapshot {
            default_iface,
            rx_bps,
            tx_bps,
            interfaces,
        }
    }
}

impl Source<NetSnapshot> for NetCollector {
    fn sample(&mut self) -> NetSnapshot {
        let now = Instant::now();
        let elapsed = self
            .last_sampled
            .replace(now)
            .map_or(Duration::ZERO, |before| now.duration_since(before));
        self.sample_after(elapsed)
    }
}

/// The file's text, or an empty table when it cannot be read.
fn read_table<'a>(root: &SysRoot, rel: &str, buf: &'a mut Vec<u8>) -> &'a str {
    if root.read_into(rel, buf).is_err() {
        buf.clear();
    }
    std::str::from_utf8(buf).unwrap_or_default()
}

fn parse_dev(text: &str) -> impl Iterator<Item = (&str, Counters)> {
    text.lines().skip(DEV_HEADER_LINES).filter_map(|line| {
        let (name, counters) = line.split_once(DEV_NAME_SEPARATOR)?;
        let mut counters = counters.split_ascii_whitespace();
        let rx = counters.next()?.parse().ok()?;
        let tx = counters.nth(COUNTERS_PER_DIRECTION - 1)?.parse().ok()?;
        Some((name.trim(), Counters { rx, tx }))
    })
}

/// The interface of the lowest-metric default route that is up; the first listed wins a tie.
fn default_route(table: &str) -> Option<&str> {
    table
        .lines()
        .skip(ROUTE_HEADER_LINES)
        .filter_map(parse_default_route)
        .min_by_key(|(_, metric)| *metric)
        .map(|(iface, _)| iface)
}

/// The interface and metric of a `/proc/net/route` row that is a default route and up.
fn parse_default_route(line: &str) -> Option<(&str, u32)> {
    let mut columns = line.split_ascii_whitespace();
    let (iface, destination, _gateway, flags, _ref_count, _use, metric, mask) = (
        columns.next()?,
        columns.next()?,
        columns.next()?,
        columns.next()?,
        columns.next()?,
        columns.next()?,
        columns.next()?,
        columns.next()?,
    );
    let is_up = parse_hex(flags)? & RTF_UP != 0;
    let is_default = destination == ANY_ADDRESS && mask == ANY_ADDRESS;
    (is_up && is_default).then_some((iface, metric.parse().ok()?))
}

fn is_oper_up(root: &SysRoot, iface: &str) -> bool {
    let read = |file: &str| read_text(root, &format!("{SYS_CLASS_NET}/{iface}/{file}"));
    match read(OPERSTATE_FILE).as_deref() {
        Some(OPERSTATE_UP) => true,
        Some(OPERSTATE_UNKNOWN) => read(FLAGS_FILE)
            .as_deref()
            .and_then(parse_hex)
            .is_some_and(|flags| flags & IFF_UP != 0),
        _ => false,
    }
}

/// Route flags are bare hex digits, and sysfs `flags` carry a `0x` prefix.
fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.strip_prefix(HEX_PREFIX).unwrap_or(text);
    u32::from_str_radix(digits, HEX_RADIX).ok()
}
