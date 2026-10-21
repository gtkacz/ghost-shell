mod manifest;
mod resolve;

pub use manifest::{
    Kind, Manifest, Setting, SettingType, SizeHints, WIDGET_API, default_dirs, discover,
};
pub use resolve::resolve;
