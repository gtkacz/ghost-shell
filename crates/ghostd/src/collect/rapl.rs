use std::time::Duration;

use tracing::info;

use super::read_text;
use crate::sysroot::SysRoot;

const POWERCAP_DIR: &str = "sys/class/powercap";
const PACKAGE_ZONE_NAME: &str = "package-0";
const ZONE_NAME: &str = "name";
const ENERGY_COUNTER: &str = "energy_uj";
const COUNTER_RANGE: &str = "max_energy_range_uj";
// A real wrap within one sample spends a sliver of the range, whereas a reset from a low reading
// reads as nearly the whole range, which is tens of kilowatts. Half the range needs no tuned ceiling.
const WRAP_PLAUSIBLE_DIVISOR: u64 = 2;

/// Watts drawn by the CPU package, from the energy counter of the powercap zone `package-0`.
///
/// The counter is in microjoules and wraps at the zone's own `max_energy_range_uj`, so a reading
/// below the previous one is a wrap, not negative energy.
pub(super) struct PackagePower {
    root: SysRoot,
    energy_rel: String,
    range_rel: String,
    previous_uj: Option<u64>,
    reported_unreadable: bool,
}

impl PackagePower {
    /// `None` when the machine has no zone named `package-0`.
    pub fn find(root: &SysRoot) -> Option<Self> {
        // The control-type `intel-rapl` entry has no `name` file, so an unreadable name is skipped.
        let zone = root.list(POWERCAP_DIR).ok()?.into_iter().find(|entry| {
            read_text(root, &format!("{POWERCAP_DIR}/{entry}/{ZONE_NAME}")).as_deref()
                == Some(PACKAGE_ZONE_NAME)
        })?;
        Some(Self {
            root: root.clone(),
            energy_rel: format!("{POWERCAP_DIR}/{zone}/{ENERGY_COUNTER}"),
            range_rel: format!("{POWERCAP_DIR}/{zone}/{COUNTER_RANGE}"),
            previous_uj: None,
            reported_unreadable: false,
        })
    }

    pub fn sample(&mut self, elapsed: Duration) -> Option<f32> {
        // The counter is root-only until the udev rule is installed, and that can happen while the
        // daemon runs, so it is retried every sample.
        let Some(now_uj) =
            read_text(&self.root, &self.energy_rel).and_then(|text| text.parse().ok())
        else {
            if !std::mem::replace(&mut self.reported_unreadable, true) {
                info!(path = %self.energy_rel, "cannot read the RAPL energy counter; package power stays hidden");
            }
            self.previous_uj = None;
            return None;
        };
        let before_uj = self.previous_uj.replace(now_uj)?;
        let spent_uj = if now_uj >= before_uj {
            now_uj - before_uj
        } else {
            self.wrapped_delta(before_uj, now_uj)?
        };
        let micros = elapsed.as_micros();
        (micros > 0).then(|| (spent_uj as f64 / micros as f64) as f32)
    }

    /// `None` when the range is unknown, the previous reading lies beyond it, or the wrapped delta
    /// exceeds `1 / WRAP_PLAUSIBLE_DIVISOR` of it: each means the counter was reset rather than
    /// wrapped, so the energy spent is unknowable.
    fn wrapped_delta(&self, before_uj: u64, now_uj: u64) -> Option<u64> {
        let range_uj: u64 = read_text(&self.root, &self.range_rel)?.parse().ok()?;
        let wrapped_uj = range_uj.checked_sub(before_uj)?.checked_add(now_uj)?;
        (wrapped_uj <= range_uj / WRAP_PLAUSIBLE_DIVISOR).then_some(wrapped_uj)
    }
}
