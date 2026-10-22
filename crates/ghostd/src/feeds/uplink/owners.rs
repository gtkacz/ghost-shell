//! The local processes that own sockets, found through their `/proc/<pid>/fd` links.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::Instant;

use tracing::debug;

use crate::sysroot::SysRoot;

const SOCKET_LINK_PREFIX: &str = "socket:[";
const SOCKET_LINK_SUFFIX: &str = "]";
const UID_FIELD: &str = "Uid:";

/// Socket inodes and the `comm` of the process that holds each, remembered while the socket
/// lives, so a process is looked for once per socket rather than once a cycle.
pub struct Owners {
    root: SysRoot,
    /// `None` for a socket no readable process of its uid holds, so it is not looked for again.
    cache: HashMap<u64, Option<String>>,
}

impl Owners {
    pub fn new(root: SysRoot) -> Self {
        Self {
            root,
            cache: HashMap::new(),
        }
    }

    /// The owner's `comm` of each `(inode, uid)` socket that has a findable one. Only processes
    /// of the socket's uid are searched, and the walk stops once every socket is found. The cache
    /// keeps only the inodes asked for now.
    pub fn names(&mut self, wanted: &[(u64, u32)]) -> HashMap<u64, String> {
        let current: HashSet<u64> = wanted.iter().map(|&(inode, _)| inode).collect();
        self.cache.retain(|inode, _| current.contains(inode));
        let mut missing: HashMap<u64, u32> = wanted
            .iter()
            .filter(|(inode, _)| !self.cache.contains_key(inode))
            .copied()
            .collect();
        if !missing.is_empty() {
            self.walk(&mut missing);
        }
        wanted
            .iter()
            .filter_map(|(inode, _)| Some((*inode, self.cache.get(inode)?.clone()?)))
            .collect()
    }

    fn walk(&mut self, missing: &mut HashMap<u64, u32>) {
        let started = Instant::now();
        let mut fds_read = 0_usize;
        let uids: HashSet<u32> = missing.values().copied().collect();
        let Ok(pids) = self.root.entries("proc") else {
            return;
        };
        for pid in pids {
            if missing.is_empty() {
                break;
            }
            let Some(pid) = pid
                .to_str()
                .filter(|name| name.bytes().all(|byte| byte.is_ascii_digit()))
            else {
                continue;
            };
            let Some(uid) = self.effective_uid(pid).filter(|uid| uids.contains(uid)) else {
                continue;
            };
            // Another user's fds are unreadable, and a process may exit mid-walk.
            let Ok(fds) = self.root.entries(&format!("proc/{pid}/fd")) else {
                continue;
            };
            let mut comm = None;
            for fd in fds {
                fds_read += 1;
                let link = self
                    .root
                    .path(&format!("proc/{pid}/fd/{}", fd.to_string_lossy()));
                let Some(inode) = fs::read_link(link)
                    .ok()
                    .and_then(|target| socket_inode(&target.to_string_lossy()))
                else {
                    continue;
                };
                if missing.get(&inode) != Some(&uid) {
                    continue;
                }
                missing.remove(&inode);
                let name = comm.get_or_insert_with(|| self.comm(pid)).clone();
                self.cache.insert(inode, name);
                if missing.is_empty() {
                    break;
                }
            }
        }
        for inode in missing.keys() {
            self.cache.insert(*inode, None);
        }
        debug!(
            fds_read,
            elapsed_us = started.elapsed().as_micros(),
            "walked processes for socket owners"
        );
    }

    /// The effective uid, which the kernel stamps on the sockets a process creates.
    fn effective_uid(&self, pid: &str) -> Option<u32> {
        let status = self.root.read(&format!("proc/{pid}/status")).ok()?;
        let uids = status
            .lines()
            .find_map(|line| line.strip_prefix(UID_FIELD))?;
        uids.split_whitespace().nth(1)?.parse().ok()
    }

    fn comm(&self, pid: &str) -> Option<String> {
        let comm = self.root.read(&format!("proc/{pid}/comm")).ok()?;
        Some(comm.trim_end_matches('\n').to_owned())
    }
}

fn socket_inode(target: &str) -> Option<u64> {
    target
        .strip_prefix(SOCKET_LINK_PREFIX)?
        .strip_suffix(SOCKET_LINK_SUFFIX)?
        .parse()
        .ok()
}
