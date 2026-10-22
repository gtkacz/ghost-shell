//! A record of the calls a fake D-Bus service answers, for fakes of services the bus could start.

use std::sync::{Arc, Mutex, PoisonError};

use zbus::message::{Flags, Header};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    Method,
    PropertyRead,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub kind: CallKind,
    pub member: String,
    pub no_auto_start: bool,
}

#[derive(Clone, Default)]
pub struct Calls(Arc<Mutex<Vec<Call>>>);

impl Calls {
    pub fn method(&self, header: &Header<'_>, member: &str) {
        self.note(CallKind::Method, header, member);
    }

    /// zbus passes no header when it reads a property for its own change signal, which is no call.
    pub fn property(&self, header: Option<&Header<'_>>, member: &str) {
        if let Some(header) = header {
            self.note(CallKind::PropertyRead, header, member);
        }
    }

    fn note(&self, kind: CallKind, header: &Header<'_>, member: &str) {
        self.lock().push(Call {
            kind,
            member: member.to_owned(),
            no_auto_start: header.primary().flags().contains(Flags::NO_AUTO_START),
        });
    }

    pub fn count(&self) -> usize {
        self.lock().len()
    }

    pub fn count_of(&self, member: &str) -> usize {
        self.lock()
            .iter()
            .filter(|call| call.member == member)
            .count()
    }

    /// Method calls other than `allowed`; property reads do not count.
    pub fn method_calls_except(&self, allowed: &[&str]) -> usize {
        self.lock()
            .iter()
            .filter(|call| {
                call.kind == CallKind::Method && !allowed.contains(&call.member.as_str())
            })
            .count()
    }

    /// Every call so far asked the bus not to start the service for it, so an absent service
    /// stays absent.
    pub fn assert_none_could_start_the_service(&self) {
        let calls = self.lock();
        let starting: Vec<_> = calls.iter().filter(|call| !call.no_auto_start).collect();
        assert!(
            starting.is_empty(),
            "calls without NO_AUTO_START: {starting:?}"
        );
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Call>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
