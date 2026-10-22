//! Connected sockets and their remote ends, from the kernel's `/proc/net` tables.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use tracing::debug;

use crate::sysroot::SysRoot;

/// The tables, with whether each holds IPv6 and whether it is UDP.
const TABLES: [(&str, bool, bool); 4] = [
    ("proc/net/tcp", false, false),
    ("proc/net/tcp6", true, false),
    ("proc/net/udp", false, true),
    ("proc/net/udp6", true, true),
];
/// `TCP_ESTABLISHED`, which a connected UDP socket reports too.
const ESTABLISHED: &str = "01";
const REMOTE_COLUMN: usize = 2;
const STATE_COLUMN: usize = 3;
const UID_COLUMN: usize = 7;
const INODE_COLUMN: usize = 9;
const HEX: u32 = 16;
const HEX_PER_WORD: usize = 8;
const V6_WORDS: usize = 4;
const CGNAT: (Ipv4Addr, u32) = (Ipv4Addr::new(100, 64, 0, 0), 10);
const DOCUMENTATION_V6: (Ipv6Addr, u32) = (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteSocket {
    /// An IPv4-mapped IPv6 address is already its IPv4 address.
    pub remote: IpAddr,
    pub uid: u32,
    pub inode: u64,
}

/// Where the feed takes its sockets from, so tests can hand it addresses no fixture could hold.
pub trait SocketSource: Send + Sync {
    fn sockets(&self) -> Vec<RemoteSocket>;
}

/// The four `/proc/net` tables under a root.
pub struct ProcNetSockets {
    root: SysRoot,
}

impl ProcNetSockets {
    pub fn new(root: SysRoot) -> Self {
        Self { root }
    }
}

impl SocketSource for ProcNetSockets {
    fn sockets(&self) -> Vec<RemoteSocket> {
        TABLES
            .iter()
            .flat_map(|&(table, v6, udp)| match self.root.read(table) {
                Ok(text) => parse_proc_net(&text, v6, udp),
                // A kernel without IPv6 has no `tcp6` or `udp6`.
                Err(error) => {
                    debug!(%error, table, "cannot read a socket table");
                    Vec::new()
                }
            })
            .collect()
    }
}

/// The established TCP sockets, or the connected UDP ones, of a `/proc/net/{tcp,udp}{,6}` table.
pub fn parse_proc_net(text: &str, v6: bool, udp: bool) -> Vec<RemoteSocket> {
    text.lines()
        .filter_map(|line| parse_row(line, v6))
        // A UDP socket reports `01` once `connect` has given it a remote.
        .filter(|socket| !udp || !socket.remote.is_unspecified())
        .collect()
}

fn parse_row(line: &str, v6: bool) -> Option<RemoteSocket> {
    let columns: Vec<&str> = line.split_whitespace().collect();
    if *columns.get(STATE_COLUMN)? != ESTABLISHED {
        return None;
    }
    let (address, _port) = columns.get(REMOTE_COLUMN)?.split_once(':')?;
    let remote = if v6 {
        let address = parse_v6(address)?;
        address
            .to_ipv4_mapped()
            .map_or(IpAddr::V6(address), IpAddr::V4)
    } else {
        IpAddr::V4(Ipv4Addr::from(parse_word(address)?))
    };
    Some(RemoteSocket {
        remote,
        uid: columns.get(UID_COLUMN)?.parse().ok()?,
        inode: columns.get(INODE_COLUMN)?.parse().ok()?,
    })
}

/// The kernel prints each 32-bit word of an address as the native-endian integer it stores, so
/// its bytes in memory, which are in network order, come back with `to_ne_bytes`.
fn parse_word(hex: &str) -> Option<[u8; 4]> {
    if hex.len() != HEX_PER_WORD {
        return None;
    }
    Some(u32::from_str_radix(hex, HEX).ok()?.to_ne_bytes())
}

fn parse_v6(hex: &str) -> Option<Ipv6Addr> {
    if hex.len() != HEX_PER_WORD * V6_WORDS || !hex.is_ascii() {
        return None;
    }
    let mut octets = [0; 16];
    for (word, chunk) in octets.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let start = word * HEX_PER_WORD;
        chunk.copy_from_slice(&parse_word(&hex[start..start + HEX_PER_WORD])?);
    }
    Some(Ipv6Addr::from(octets))
}

/// Whether `ip` is somewhere on the public internet, so worth placing on a globe. An
/// IPv4-mapped IPv6 address is judged as its IPv4 address.
pub fn is_public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(*v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(*v6),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || in_v4_prefix(ip, CGNAT))
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || in_v6_prefix(ip, DOCUMENTATION_V6))
}

fn in_v4_prefix(ip: Ipv4Addr, (network, length): (Ipv4Addr, u32)) -> bool {
    let mask = u32::MAX << (u32::BITS - length);
    u32::from(ip) & mask == u32::from(network) & mask
}

fn in_v6_prefix(ip: Ipv6Addr, (network, length): (Ipv6Addr, u32)) -> bool {
    let mask = u128::MAX << (u128::BITS - length);
    u128::from(ip) & mask == u128::from(network) & mask
}
