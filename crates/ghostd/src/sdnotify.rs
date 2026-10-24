//! The systemd readiness notification, for `Type=notify` units.

use std::env;
use std::os::fd::AsRawFd;

use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, UnixAddr, sendto, socket};
use tracing::warn;

const NOTIFY_SOCKET: &str = "NOTIFY_SOCKET";
const ABSTRACT_PREFIX: char = '@';
const READY: &[u8] = b"READY=1";

/// Tells systemd the service is up. Does nothing outside a `Type=notify` unit, and a failure only
/// costs the unit its start-up timeout, so it is logged and not raised.
pub fn ready() {
    let Some(target) = env::var_os(NOTIFY_SOCKET) else {
        return;
    };
    let Some(target) = target.to_str() else {
        warn!("{NOTIFY_SOCKET} is not UTF-8; READY=1 not sent");
        return;
    };
    if let Err(error) = send_ready(target) {
        warn!(%error, target, "cannot send READY=1");
    }
}

/// Sends `READY=1` as one datagram to `target`: a filesystem path, or `@name` for an abstract
/// socket.
pub fn send_ready(target: &str) -> nix::Result<()> {
    let address = match target.strip_prefix(ABSTRACT_PREFIX) {
        Some(name) => UnixAddr::new_abstract(name.as_bytes())?,
        None => UnixAddr::new(target)?,
    };
    let socket = socket(
        AddressFamily::Unix,
        SockType::Datagram,
        SockFlag::SOCK_CLOEXEC,
        None,
    )?;
    sendto(socket.as_raw_fd(), READY, &address, MsgFlags::empty())?;
    Ok(())
}
