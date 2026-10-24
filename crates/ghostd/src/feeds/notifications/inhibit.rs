//! The cookies of `Inhibit` calls, which suppress popups until released or until their caller
//! leaves the bus.

use std::collections::HashMap;

/// Cookie 0 means "none" to some clients, so the first handed out is 1.
const FIRST_COOKIE: u32 = 1;

#[derive(Debug)]
pub struct Inhibitors {
    /// Cookie to the caller's unique name.
    held: HashMap<u32, String>,
    next: u32,
}

impl Default for Inhibitors {
    fn default() -> Self {
        Self {
            held: HashMap::new(),
            next: FIRST_COOKIE,
        }
    }
}

impl Inhibitors {
    pub fn add(&mut self, who: &str) -> u32 {
        let cookie = self.next;
        self.next = self.next.checked_add(1).unwrap_or(FIRST_COOKIE);
        self.held.insert(cookie, who.to_owned());
        cookie
    }

    /// Whether `cookie` was held.
    pub fn remove(&mut self, cookie: u32) -> bool {
        self.held.remove(&cookie).is_some()
    }

    /// Drops every cookie of `who`, returning whether it held any.
    pub fn release(&mut self, who: &str) -> bool {
        let before = self.held.len();
        self.held.retain(|_, holder| holder != who);
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
        let mut inhibitors = Inhibitors::default();
        let first = inhibitors.add(":1.5");
        let second = inhibitors.add(":1.5");
        assert_ne!(first, 0);
        assert_ne!(first, second);
        assert!(inhibitors.active());
    }

    #[test]
    fn releasing_a_caller_drops_only_its_cookies() {
        let mut inhibitors = Inhibitors::default();
        let kept = inhibitors.add(":1.6");
        inhibitors.add(":1.5");
        assert!(inhibitors.release(":1.5"));
        assert!(!inhibitors.release(":1.5"));
        assert!(inhibitors.remove(kept));
        assert!(!inhibitors.remove(kept));
        assert!(!inhibitors.active());
    }
}
