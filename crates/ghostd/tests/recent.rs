use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ghost_proto::{RecentFile, RecentFiles, ResolvedConfig, topics};
use ghostd::feeds::recent;
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(1);
const QUIET_FOR: Duration = Duration::from_millis(600);
const XBEL: &str = "recently-used.xbel";
const SHOWN: usize = 5;

struct Sandbox {
    dir: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let sandbox = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(sandbox.home()).unwrap();
        fs::create_dir_all(sandbox.data_home()).unwrap();
        sandbox
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn data_home(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn xbel(&self) -> PathBuf {
        self.data_home().join(XBEL)
    }

    /// Creates an empty file at `relative` below the sandbox and returns its path.
    fn file(&self, relative: &str) -> PathBuf {
        let path = self.dir.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();
        path
    }

    fn write_xbel(&self, bookmarks: &[(String, u32)]) {
        fs::write(self.xbel(), xbel(bookmarks)).unwrap();
    }

    /// Replaces the xbel the way GLib does: a new file renamed over the old one.
    fn replace_xbel(&self, bookmarks: &[(String, u32)]) {
        let staged = self.data_home().join("staged");
        fs::write(&staged, xbel(bookmarks)).unwrap();
        fs::rename(staged, self.xbel()).unwrap();
    }
}

fn xbel(bookmarks: &[(String, u32)]) -> String {
    let body: String = bookmarks
        .iter()
        .map(|(href, second)| {
            format!(
                "<bookmark href=\"{href}\" added=\"2026-01-01T00:00:00Z\" \
                 modified=\"2026-01-01T00:00:{second:02}Z\" visited=\"2026-01-01T00:00:00Z\">\
                 <info><metadata owner=\"http://freedesktop.org\"><mime:mime-type type=\"text/plain\"/>\
                 </metadata></info></bookmark>"
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <xbel version=\"1.0\" xmlns:bookmark=\"http://www.freedesktop.org/standards/desktop-bookmarks\" \
         xmlns:mime=\"http://www.freedesktop.org/standards/shared-mime-info\">{body}</xbel>"
    )
}

fn url(path: &Path) -> (String, u32) {
    (format!("file://{}", path.display()), 0)
}

fn at(path: &Path, second: u32) -> (String, u32) {
    (format!("file://{}", path.display()), second)
}

struct Running {
    published: mpsc::UnboundedReceiver<(String, Value)>,
    demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    _task: JoinHandle<()>,
}

impl Running {
    fn start(sandbox: &Sandbox) -> Self {
        let (sender, published) = mpsc::unbounded_channel();
        let (demand, demand_receiver) = watch::channel(BTreeSet::new());
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
            calls: tokio::sync::broadcast::channel(64).1,
            topics: tokio::sync::broadcast::channel(1).1,
        };
        let feed = recent::feed_at(sandbox.xbel(), sandbox.home());
        let task = tokio::spawn(feed.run(context));
        Self {
            published,
            demand,
            _config: config,
            _task: task,
        }
    }

    fn want(&self) {
        self.demand
            .send(BTreeSet::from([topics::RECENT.to_owned()]))
            .unwrap();
    }

    fn unwant(&self) {
        self.demand.send(BTreeSet::new()).unwrap();
    }

    async fn next(&mut self) -> RecentFiles {
        let (topic, data) = timeout(WAIT, self.published.recv())
            .await
            .expect("nothing was published in time")
            .unwrap();
        assert_eq!(topic, topics::RECENT);
        serde_json::from_value(data).unwrap()
    }

    async fn expect_quiet(&mut self) {
        assert!(
            timeout(QUIET_FOR, self.published.recv()).await.is_err(),
            "something was published"
        );
    }
}

async fn listed(sandbox: &Sandbox) -> RecentFiles {
    let mut running = Running::start(sandbox);
    running.want();
    running.next().await
}

fn names(files: &RecentFiles) -> Vec<&str> {
    files.files.iter().map(|file| file.name.as_str()).collect()
}

#[tokio::test]
async fn newest_five_files_first() {
    let sandbox = Sandbox::new();
    let paths: Vec<PathBuf> = (0..7)
        .map(|index| sandbox.file(&format!("docs/f{index}.txt")))
        .collect();
    let bookmarks: Vec<_> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| at(path, 10 + u32::try_from(index).unwrap()))
        .collect();
    sandbox.write_xbel(&bookmarks);

    let files = listed(&sandbox).await;

    assert_eq!(files.files.len(), SHOWN);
    assert_eq!(
        names(&files),
        ["f6.txt", "f5.txt", "f4.txt", "f3.txt", "f2.txt"]
    );
    assert!(files.files[0].modified_s > files.files[1].modified_s);
    assert_eq!(files.files[0].path, paths[6].to_string_lossy());
}

#[tokio::test]
async fn non_file_urls_are_skipped() {
    let sandbox = Sandbox::new();
    let kept = sandbox.file("docs/kept.txt");
    sandbox.write_xbel(&[
        ("http://example.invalid/page.html".to_owned(), 30),
        ("trash:///gone.txt".to_owned(), 29),
        at(&kept, 1),
    ]);

    assert_eq!(names(&listed(&sandbox).await), ["kept.txt"]);
}

#[tokio::test]
async fn missing_files_are_skipped() {
    let sandbox = Sandbox::new();
    let kept = sandbox.file("docs/kept.txt");
    let gone = sandbox.dir.path().join("docs/gone.txt");
    sandbox.write_xbel(&[at(&gone, 30), at(&kept, 1)]);

    assert_eq!(names(&listed(&sandbox).await), ["kept.txt"]);
}

#[tokio::test]
async fn a_path_listed_twice_appears_once_with_its_newest_time() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("docs/twice.txt");
    sandbox.write_xbel(&[at(&file, 5), at(&file, 9)]);

    let files = listed(&sandbox).await;

    assert_eq!(names(&files), ["twice.txt"]);
    assert_eq!(files.files[0].modified_s % 60, 9);
}

#[tokio::test]
async fn percent_encoded_paths_decode() {
    let sandbox = Sandbox::new();
    let spaced = sandbox.file("docs/a b.txt");
    let encoded = spaced.to_string_lossy().replace(' ', "%20");
    sandbox.write_xbel(&[(format!("file://{encoded}"), 1)]);

    let files = listed(&sandbox).await;

    assert_eq!(names(&files), ["a b.txt"]);
    assert_eq!(files.files[0].path, spaced.to_string_lossy());
}

#[tokio::test]
async fn home_folder_is_shortened() {
    let sandbox = Sandbox::new();
    let download = sandbox.file("home/Downloads/report.pdf");
    let outside = sandbox.file("elsewhere/notes.txt");
    sandbox.write_xbel(&[at(&download, 2), at(&outside, 1)]);

    let files = listed(&sandbox).await;

    assert_eq!(
        files
            .files
            .iter()
            .map(|f| f.dir.clone())
            .collect::<Vec<_>>(),
        [
            "~/Downloads".to_owned(),
            outside.parent().unwrap().to_string_lossy().into_owned()
        ]
    );
}

#[tokio::test]
async fn rewritten_file_republishes() {
    let sandbox = Sandbox::new();
    let first = sandbox.file("docs/first.txt");
    let second = sandbox.file("docs/second.txt");
    sandbox.write_xbel(&[url(&first)]);
    let mut running = Running::start(&sandbox);
    running.want();
    assert_eq!(names(&running.next().await), ["first.txt"]);

    sandbox.replace_xbel(&[url(&second)]);

    assert_eq!(names(&running.next().await), ["second.txt"]);
}

#[tokio::test]
async fn malformed_xbel_publishes_an_empty_list() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("docs/a.txt");
    fs::write(
        sandbox.xbel(),
        format!(
            "<xbel><bookmark href=\"file://{}\" modified=\"2026-01-01T00:00:00Z\"></xbel>",
            file.display()
        ),
    )
    .unwrap();

    assert_eq!(listed(&sandbox).await, RecentFiles::default());
}

#[tokio::test]
async fn an_absent_xbel_publishes_an_empty_list() {
    let sandbox = Sandbox::new();

    assert_eq!(listed(&sandbox).await, RecentFiles::default());
}

#[tokio::test]
async fn no_watch_while_unwanted() {
    let sandbox = Sandbox::new();
    let first = sandbox.file("docs/first.txt");
    let second = sandbox.file("docs/second.txt");
    sandbox.write_xbel(&[url(&first)]);
    let mut running = Running::start(&sandbox);
    running.want();
    running.next().await;
    running.unwant();
    running.expect_quiet().await;

    sandbox.replace_xbel(&[url(&second)]);

    running.expect_quiet().await;
    running.want();
    assert_eq!(
        running.next().await.files,
        [RecentFile {
            path: second.to_string_lossy().into_owned(),
            name: "second.txt".to_owned(),
            dir: second.parent().unwrap().to_string_lossy().into_owned(),
            modified_s: 1_767_225_600,
        }]
    );
}

#[tokio::test]
async fn an_oversize_xbel_publishes_an_empty_list() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("docs/a.txt");
    sandbox.write_xbel(&[url(&file)]);
    let oversize = fs::OpenOptions::new()
        .write(true)
        .open(sandbox.xbel())
        .unwrap();
    oversize.set_len(17 * 1024 * 1024).unwrap();

    assert_eq!(listed(&sandbox).await, RecentFiles::default());
}

#[tokio::test]
async fn urls_naming_a_host_are_skipped() {
    let sandbox = Sandbox::new();
    let kept = sandbox.file("docs/kept.txt");
    sandbox.write_xbel(&[("file://host/docs/kept.txt".to_owned(), 30), at(&kept, 1)]);

    assert_eq!(names(&listed(&sandbox).await), ["kept.txt"]);
}
