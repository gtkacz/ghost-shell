use std::path::PathBuf;
use std::{fs, io};

const HOST_ROOT: &str = "/";

/// A filesystem root that `/proc` and `/sys` reads resolve under, so tests can point at a fake tree.
#[derive(Debug, Clone)]
pub struct SysRoot {
    root: PathBuf,
}

impl SysRoot {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn host() -> Self {
        Self::new(HOST_ROOT)
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        // `join` discards the root when handed an absolute path, which would escape a fake tree.
        self.root.join(rel.trim_start_matches('/'))
    }

    pub fn read(&self, rel: &str) -> io::Result<String> {
        fs::read_to_string(self.path(rel))
    }

    /// Entry names under `rel`, sorted so callers see a stable order across reads.
    pub fn list(&self, rel: &str) -> io::Result<Vec<String>> {
        let mut names = fs::read_dir(self.path(rel))?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<io::Result<Vec<_>>>()?;
        names.sort();
        Ok(names)
    }
}
