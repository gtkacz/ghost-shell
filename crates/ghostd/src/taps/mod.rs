pub mod claude;
pub mod codex;

use std::io::{self, Read};
use std::path::Path;
use std::{fs, process};

pub use claude::claude_statusline_tap;
pub use codex::codex_hook;

fn read_stdin() -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    io::stdin().lock().read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Renames a sibling temp file into place so a watcher never reads a torn file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let staged = dir.join(format!("{name}.tmp.{}", process::id()));
    fs::write(&staged, bytes)?;
    fs::rename(&staged, path).inspect_err(|_| {
        let _ = fs::remove_file(&staged);
    })
}
