use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
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

    /// Replaces the contents of `buf` with the file's bytes, so a hot loop can reuse one allocation.
    pub fn read_into(&self, rel: &str, buf: &mut Vec<u8>) -> io::Result<()> {
        buf.clear();
        // `Take` hides `File::read_to_end`, which stats and seeks to presize the buffer. Procfs
        // files report a size of 0, so those two syscalls per file buy nothing.
        File::open(self.path(rel))?
            .take(u64::MAX)
            .read_to_end(buf)?;
        Ok(())
    }

    /// Entry names under `rel`, sorted so callers see a stable order across reads.
    pub fn list(&self, rel: &str) -> io::Result<Vec<String>> {
        let mut names = fs::read_dir(self.path(rel))?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<io::Result<Vec<_>>>()?;
        names.sort();
        Ok(names)
    }

    /// Entry names under `rel` in directory order, for hot loops that have no use for `list`'s sort.
    ///
    /// Entries that fail to read are dropped: in `/proc` they are processes that exited mid-walk.
    pub fn entries(&self, rel: &str) -> io::Result<impl Iterator<Item = OsString> + use<>> {
        Ok(fs::read_dir(self.path(rel))?
            .filter_map(|entry| entry.ok().map(|entry| entry.file_name())))
    }
}
