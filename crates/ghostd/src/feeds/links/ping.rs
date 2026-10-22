//! One ICMP echo at a time, over an unprivileged ping socket.

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::os::fd::{AsRawFd, OwnedFd};
use std::time::{Duration, Instant};

use futures_core::future::BoxFuture;
use nix::sys::socket::{
    AddressFamily, MsgFlags, SockFlag, SockProtocol, SockType, SockaddrIn, recv, sendto, socket,
};
use tokio::io::unix::AsyncFd;
use tokio::time::timeout;
use tracing::debug;

const TIMEOUT: Duration = Duration::from_secs(2);
const ECHO_REQUEST: u8 = 8;
const ECHO_REPLY: u8 = 0;
// Each ping opens its own socket, so no earlier reply can arrive on it and one sequence number
// does.
const SEQUENCE: u16 = 1;
const HEADER_LEN: usize = 8;
const SEQUENCE_AT: usize = 6;
// The kernel fills in the identifier and the checksum of a ping socket's echo request.
const REQUEST: [u8; HEADER_LEN] = {
    let sequence = SEQUENCE.to_be_bytes();
    [ECHO_REQUEST, 0, 0, 0, 0, 0, sequence[0], sequence[1]]
};
const REPLY_BUFFER_LEN: usize = 64;

/// Measures the round trip to a host.
pub trait Pinger: Send + Sync {
    /// The round trip of one echo to `host`, or `None` when no reply came.
    fn ping(&self, host: Ipv4Addr) -> BoxFuture<'_, Option<Duration>>;
}

/// Pings through a `SOCK_DGRAM` ICMP socket, which needs no privilege while the user's group is
/// in `net.ipv4.ping_group_range`. Waits 2 s for the reply.
pub struct IcmpPinger;

impl Pinger for IcmpPinger {
    fn ping(&self, host: Ipv4Addr) -> BoxFuture<'_, Option<Duration>> {
        Box::pin(async move {
            let round_trip = match timeout(TIMEOUT, echo(host)).await {
                Ok(round_trip) => round_trip,
                Err(_) => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("no reply within {TIMEOUT:?}"),
                )),
            };
            // An unreachable host is ordinary, and every 5 s would flood the log.
            round_trip
                .inspect_err(|error| debug!(%error, %host, "ping failed"))
                .ok()
        })
    }
}

async fn echo(host: Ipv4Addr) -> io::Result<Duration> {
    let socket = open_socket()?;
    // SAFETY: an `OwnedFd` keeps its descriptor open, and always the same one, until it is
    // dropped, which happens only with the `AsyncFd` that owns it.
    let socket = unsafe { AsyncFd::register(socket) }?;
    let target = SockaddrIn::from(SocketAddrV4::new(host, 0));
    let sent_at = Instant::now();
    socket
        .async_io(tokio::io::Interest::WRITABLE, |fd| {
            Ok(sendto(
                fd.as_raw_fd(),
                &REQUEST,
                &target,
                MsgFlags::empty(),
            )?)
        })
        .await?;
    let mut reply = [0; REPLY_BUFFER_LEN];
    loop {
        let received = socket
            .async_io(tokio::io::Interest::READABLE, |fd| {
                Ok(recv(fd.as_raw_fd(), &mut reply, MsgFlags::empty())?)
            })
            .await?;
        if is_echo_reply(&reply[..received]) {
            return Ok(sent_at.elapsed());
        }
    }
}

fn open_socket() -> io::Result<OwnedFd> {
    Ok(socket(
        AddressFamily::Inet,
        SockType::Datagram,
        SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
        SockProtocol::Icmp,
    )?)
}

/// Whether `packet`, as a ping socket hands it over without the IP header, answers [`REQUEST`].
fn is_echo_reply(packet: &[u8]) -> bool {
    packet.len() >= HEADER_LEN
        && packet[0] == ECHO_REPLY
        && packet[SEQUENCE_AT..HEADER_LEN] == SEQUENCE.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(kind: u8, sequence: u16) -> Vec<u8> {
        let sequence = sequence.to_be_bytes();
        vec![kind, 0, 0xab, 0xcd, 0x12, 0x34, sequence[0], sequence[1]]
    }

    #[test]
    fn an_echo_reply_to_the_sequence_sent_is_the_answer() {
        assert!(is_echo_reply(&reply(ECHO_REPLY, SEQUENCE)));
    }

    #[test]
    fn other_icmp_messages_and_sequences_are_not_the_answer() {
        const DESTINATION_UNREACHABLE: u8 = 3;
        assert!(!is_echo_reply(&reply(DESTINATION_UNREACHABLE, SEQUENCE)));
        assert!(!is_echo_reply(&reply(ECHO_REPLY, SEQUENCE + 1)));
        assert!(!is_echo_reply(
            &reply(ECHO_REPLY, SEQUENCE)[..HEADER_LEN - 1]
        ));
    }
}
