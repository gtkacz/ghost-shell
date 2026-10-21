mod load;

pub use load::{ConfigError, default_path, load_path, load_str, load_str_with_user};

pub const DEFAULT_TOML: &str = include_str!("default.toml");
