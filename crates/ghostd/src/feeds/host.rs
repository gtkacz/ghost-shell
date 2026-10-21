//! The `host` topic: the machine's name, kernel and boot time.

use std::future::pending;

use ghost_proto::{HostSnapshot, topics};
use tokio::task::spawn_blocking;

use crate::collect::read_text;
use crate::server::Feed;
use crate::sysroot::SysRoot;

const HOSTNAME: &str = "proc/sys/kernel/hostname";
const KERNEL_RELEASE: &str = "proc/sys/kernel/osrelease";
const PROC_STAT: &str = "proc/stat";
const BOOT_TIME_PREFIX: &str = "btime ";

/// Publishes the host once at start, since none of its values changes without a reboot or an
/// explicit `hostnamectl`. A value that cannot be read is published as `""` or 0.
pub fn feed(root: SysRoot) -> Feed {
    Feed::new(topics::HOST, move |context| async move {
        let host = spawn_blocking(move || read_host(&root))
            .await
            .expect("reading the host files never panics");
        context.publisher.publish(topics::HOST, &host);
        pending::<()>().await;
    })
}

fn read_host(root: &SysRoot) -> HostSnapshot {
    HostSnapshot {
        hostname: read_text(root, HOSTNAME).unwrap_or_default(),
        kernel: read_text(root, KERNEL_RELEASE).unwrap_or_default(),
        boot_time_s: boot_time_s(root).unwrap_or_default(),
    }
}

fn boot_time_s(root: &SysRoot) -> Option<u64> {
    root.read(PROC_STAT)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix(BOOT_TIME_PREFIX))?
        .trim()
        .parse()
        .ok()
}
