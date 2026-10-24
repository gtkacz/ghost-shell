//! The cookies of `Inhibit` calls, which suppress popups until released or until their caller
//! leaves the bus. This set is the single source of truth: the feed reads the watch, never a
//! relayed event, so a call and a disconnect that race cannot leave it stuck.

use std::collections::HashMap;

use tokio::sync::watch;

/// Cookie 0 means "none" to some clients, so the first handed out is 1.
const FIRST_COOKIE: u32 = 1;

#[derive(Debug)]
pub struct Inhibitors {
    /// Cookie to the caller's unique name.
    held: HashMap<u32, String>,
    next: u32,
    /// Updated by every change, so it is only as stale as the lock around this set.
    active: watch::Sender<bool>,
}

impl Inhibitors {
    pub fn new() -> (Self, watch::Receiver<bool>) {
        let (active, receiver) = watch::channel(false);
        let inhibitors = Self {
            held: HashMap::new(),
            next: FIRST_COOKIE,
            active,
        };
        (inhibitors, receiver)
    }

    fn publish(&self) {
        let active = self.active();
        self.active.send_if_modified(|current| {
            let changed = *current != active;
            *current = active;
            changed
        });
    }

    pub fn add(&mut self, who: &str) -> u32 {
        let cookie = self.next;
        self.next = self.next.checked_add(1).unwrap_or(FIRST_COOKIE);
        self.held.insert(cookie, who.to_owned());
        self.publish();
        cookie
    }

    /// Whether `cookie` was held.
    pub fn remove(&mut self, cookie: u32) -> bool {
        let removed = self.held.remove(&cookie).is_some();
        self.publish();
        removed
    }

    /// Drops every cookie of `who`, returning whether it held any.
    pub fn release(&mut self, who: &str) -> bool {
        let before = self.held.len();
        self.held.retain(|_, holder| holder != who);
        self.publish();
        self.held.len() != before
    }

    pub fn active(&self) -> bool {
        !self.held.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_are_nonzero_and_distinct() {
        let (mut inhibitors, _) = Inhibitors::new();
        let first = inhibitors.add(":1.5");
        let second = inhibitors.add(":1.5");
        assert_ne!(first, 0);
        assert_ne!(first, second);
        assert!(inhibitors.active());
    }

    #[test]
    fn releasing_a_caller_drops_only_its_cookies() {
        let (mut inhibitors, _) = Inhibitors::new();
        let kept = inhibitors.add(":1.6");
        inhibitors.add(":1.5");
        assert!(inhibitors.release(":1.5"));
        assert!(!inhibitors.release(":1.5"));
        assert!(inhibitors.remove(kept));
        assert!(!inhibitors.remove(kept));
        assert!(!inhibitors.active());
    }

    #[test]
    fn the_watch_follows_the_set() {
        let (mut inhibitors, watch) = Inhibitors::new();
        assert!(!*watch.borrow());
        let cookie = inhibitors.add(":1.5");
        assert!(*watch.borrow());
        inhibitors.remove(cookie);
        assert!(!*watch.borrow());
        inhibitors.add(":1.6");
        inhibitors.release(":1.6");
        assert!(!*watch.borrow());
    }
}
