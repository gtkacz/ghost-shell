//! The history file and the notification pictures. The feed reads the file once at start; every
//! write runs here, off its loop, because the daemon's runtime has a single thread.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;
use tokio::task::spawn_blocking;
use tracing::{info, warn};

use super::history::History;
use super::images::{self, ImageData};

/// The history holds what the user said in private, so no other user may read it.
const FILE_MODE: u32 = 0o600;
const TMP_EXTENSION: &str = "tmp";
/// A flood of pictures must not pile up in memory while the disk catches up: each can be 64 MiB.
const QUEUED_JOBS: usize = 8;

pub enum Job {
    Save { json: String, keep: Vec<u32> },
    Image { id: u32, image: ImageData },
}

/// A picture that is now a file.
pub struct Written {
    pub id: u32,
    pub path: PathBuf,
}

/// The history in `path`, or an empty one when it is missing or unusable.
pub fn load(path: &Path) -> History {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            info!(path = %path.display(), "no notification history yet; starting empty");
            return History::new();
        }
        Err(error) => {
            warn!(%error, path = %path.display(), "cannot read the notification history; starting empty");
            return History::new();
        }
    };
    History::from_json(&text).unwrap_or_else(|error| {
        warn!(%error, path = %path.display(), "the notification history is unusable; starting empty");
        History::new()
    })
}

/// Replaces `path` whole, so a crash never leaves half a history.
fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(TMP_EXTENSION);
    let written = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(FILE_MODE)
        .open(&tmp)
        .and_then(|mut file| file.write_all(contents.as_bytes()))
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

fn run(job: Job, history: &Path, images_dir: &Path, written: &mpsc::UnboundedSender<Written>) {
    match job {
        Job::Save { json, keep } => {
            if let Err(error) = write_atomic(history, &json) {
                warn!(%error, path = %history.display(), "cannot save the notification history");
            }
            if !images_dir.as_os_str().is_empty() {
                images::prune(images_dir, &keep);
            }
        }
        Job::Image { id, image } => match images::write_png(images_dir, id, &image) {
            Ok(path) => {
                // The feed is gone only while the daemon stops.
                let _ = written.send(Written { id, path });
            }
            Err(error) => warn!(%error, id, "cannot write a notification image"),
        },
    }
}

/// Starts the task that runs the jobs one at a time, in the order they were queued, so a prune
/// never removes a picture whose record the saved history already lists.
pub fn spawn_writer(
    history: PathBuf,
    images_dir: PathBuf,
    written: mpsc::UnboundedSender<Written>,
) -> mpsc::Sender<Job> {
    let (jobs, mut queue) = mpsc::channel(QUEUED_JOBS);
    tokio::spawn(async move {
        while let Some(job) = queue.recv().await {
            let (history, images_dir, written) =
                (history.clone(), images_dir.clone(), written.clone());
            if let Err(error) =
                spawn_blocking(move || run(job, &history, &images_dir, &written)).await
            {
                warn!(%error, "a notification file job panicked");
            }
        }
    });
    jobs
}
