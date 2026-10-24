//! Usage windows and sessions of the LLM coding agents, one submodule per provider.

pub mod claude;
pub mod codex;

use ghost_proto::{ProviderUsage, UsageWindow};

pub const FIVE_HOURS_S: u64 = 5 * 3600;
pub const SEVEN_DAYS_S: u64 = 7 * 24 * 3600;

/// A window as the widget should see it at `now_s`: a reset at or before now zeroes it and drops the reset.
pub fn window_as_of(
    length_s: u64,
    used_pct: f64,
    resets_at_s: Option<u64>,
    now_s: u64,
) -> UsageWindow {
    match resets_at_s {
        Some(reset) if reset > now_s => UsageWindow {
            length_s,
            used_pct,
            resets_at_s,
        },
        _ => UsageWindow {
            length_s,
            used_pct: 0.0,
            resets_at_s: None,
        },
    }
}

pub fn unavailable(id: &str, label: &str, reason: &str) -> ProviderUsage {
    ProviderUsage {
        id: id.to_owned(),
        label: label.to_owned(),
        plan: None,
        available: false,
        reason: Some(reason.to_owned()),
        read_at_s: 0,
        windows: Vec::new(),
    }
}
