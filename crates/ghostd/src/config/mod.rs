pub mod edit;
mod load;
mod watch;

pub use load::{ConfigError, default_path, load_path, load_str, load_str_with_user};
pub(crate) use load::{MAX_TASK_SHORTCUTS, config_dir, is_hex_colour};
pub(crate) use watch::DEBOUNCE;
pub use watch::{WatchHandle, spawn_watcher};

pub const DEFAULT_TOML: &str = include_str!("default.toml");
