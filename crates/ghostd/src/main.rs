use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use std::{env, fs, io, thread};

use ghost_proto::{CpuSnapshot, ResolvedConfig, topics};
use ghostd::collect::{
    CpuCollector, DiskCollector, GpuCollector, GpuSource, MemCollector, NvmlSource,
    SensorCollector, Source, StatvfsFs,
};
use ghostd::config::{DEFAULT_TOML, default_path};
use ghostd::feeds::shortcuts::Registration;
use ghostd::sampler::Sampler;
use ghostd::selfmon::SelfMon;
use ghostd::server::{self, ConfigFeed, Feed, Hub, WarningHook, WidgetDirs};
use ghostd::sysroot::SysRoot;
use ghostd::widgets::default_dirs;
use ghostd::{fake, feeds, notify, providers};
use serde::Serialize;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::watch;
use tracing::{error, info, warn};

const EXIT_USAGE: u8 = 2;
const USAGE: &str = "usage: ghostd run [--socket PATH] [--config PATH] [--widgets-dir DIR] \
                     [--fake] [--tick-ms N]\n       ghostd default-config\n       \
                     ghostd probe <cpu | mem | sensors | gpu | disks | perf>";
const DEFAULT_TICK_MS: NonZeroU32 = NonZeroU32::new(1000).unwrap();
const RUNTIME_DIR_ENV: &str = "XDG_RUNTIME_DIR";
const SOCKET_FILE: &str = "ghostd.sock";
const TOP_PROCESSES: usize = 6;
const PROBE_INTERVAL: Duration = Duration::from_secs(1);
// A collector stuck in a blocking read must not keep a stopped daemon from exiting.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(1);

type Labels = BTreeMap<String, String>;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["run", options @ ..] => run(options),
        ["default-config"] => print_stdout(DEFAULT_TOML),
        ["probe", topic] => probe(topic),
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(EXIT_USAGE)
}

fn print_stdout(text: &str) -> ExitCode {
    // Piping into `head` closes stdout early, which must not panic like `print!` does.
    match io::stdout().write_all(text.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

struct RunOptions {
    socket: Option<PathBuf>,
    config: Option<PathBuf>,
    widgets_dir: Option<PathBuf>,
    fake: bool,
    tick_ms: NonZeroU32,
}

fn parse_run(mut args: &[&str]) -> Option<RunOptions> {
    let mut options = RunOptions {
        socket: None,
        config: None,
        widgets_dir: None,
        fake: false,
        tick_ms: DEFAULT_TICK_MS,
    };
    loop {
        args = match args {
            [] => return Some(options),
            ["--socket", path, rest @ ..] => {
                options.socket = Some(PathBuf::from(path));
                rest
            }
            ["--config", path, rest @ ..] => {
                options.config = Some(PathBuf::from(path));
                rest
            }
            ["--widgets-dir", dir, rest @ ..] => {
                options.widgets_dir = Some(PathBuf::from(dir));
                rest
            }
            ["--fake", rest @ ..] => {
                options.fake = true;
                rest
            }
            ["--tick-ms", ms, rest @ ..] => {
                options.tick_ms = ms.parse().ok()?;
                rest
            }
            _ => return None,
        };
    }
}

fn run(args: &[&str]) -> ExitCode {
    let Some(options) = parse_run(args) else {
        return usage();
    };
    let Some(socket) = options.socket.clone().or_else(default_socket) else {
        eprintln!("ghostd: set {RUNTIME_DIR_ENV} or pass --socket");
        return ExitCode::from(EXIT_USAGE);
    };
    let config_path = match options.config.clone() {
        Some(path) => path,
        None => {
            let path = default_path();
            // Without either variable the default resolves against the working directory.
            if !path.is_absolute() {
                eprintln!("ghostd: set HOME or XDG_CONFIG_HOME, or pass --config");
                return ExitCode::from(EXIT_USAGE);
            }
            path
        }
    };
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .init();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            error!(%error, "cannot start the async runtime");
            return ExitCode::FAILURE;
        }
    };
    let code = runtime.block_on(run_daemon(options, socket, config_path));
    runtime.shutdown_timeout(SHUTDOWN_GRACE);
    code
}

fn default_socket() -> Option<PathBuf> {
    env::var_os(RUNTIME_DIR_ENV)
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join(SOCKET_FILE))
}

async fn run_daemon(options: RunOptions, socket: PathBuf, config_path: PathBuf) -> ExitCode {
    let (mut terminate, mut interrupt) = match (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) {
        (Ok(terminate), Ok(interrupt)) => (terminate, interrupt),
        (Err(error), _) | (_, Err(error)) => {
            error!(%error, "cannot install the signal handlers");
            return ExitCode::FAILURE;
        }
    };
    // Kept until this function returns, which is after the socket file is removed below, so no
    // second daemon can claim the path in between and then lose its socket to that removal.
    let (listener, _socket_lock) = match server::bind(&socket) {
        Ok(bound) => bound,
        Err(error) => {
            error!(%error, "cannot serve");
            return ExitCode::FAILURE;
        }
    };
    info!(socket = %socket.display(), config = %config_path.display(), "listening");

    let config = ConfigFeed::open(&config_path);
    let providers = providers::supervise(&config.current().providers, &socket);
    let (sampler, mut feeds) = if options.fake {
        (fake::sampler(), fake::feeds())
    } else {
        (host_sampler(config.subscribe()), host_feeds())
    };
    let shortcuts = Registration::default();
    // Real in fake mode too, so a smoke run can drive the views over its own private bus.
    feeds.push(feeds::desktop::feed(shortcuts.clone()));
    let hub = Hub {
        config,
        sampler,
        tick_ms: options.tick_ms,
        widget_dirs: widget_search_dirs(options.widgets_dir),
        providers,
        on_warning: desktop_warning(),
        feeds,
    };

    let exit_code = tokio::select! {
        error = server::serve(listener, hub) => {
            // A non-zero exit is what makes `Restart=on-failure` bring the daemon back.
            error!(%error, "the daemon can no longer do its job; exiting");
            ExitCode::FAILURE
        }
        _ = terminate.recv() => {
            info!("SIGTERM received; stopping");
            shortcuts.release().await;
            ExitCode::SUCCESS
        }
        _ = interrupt.recv() => {
            info!("SIGINT received; stopping");
            shortcuts.release().await;
            ExitCode::SUCCESS
        }
    };
    if let Err(error) = fs::remove_file(&socket) {
        warn!(%error, socket = %socket.display(), "cannot remove the socket file");
    }
    exit_code
}

/// The default search order, with `builtin` (from `--widgets-dir`) in place of the built-in dir.
fn widget_search_dirs(builtin: Option<PathBuf>) -> WidgetDirs {
    Box::new(move |config| {
        let mut dirs = default_dirs(&config.widgets.paths);
        if let (Some(first), Some(replacement)) = (dirs.first_mut(), &builtin) {
            first.clone_from(replacement);
        }
        dirs
    })
}

fn desktop_warning() -> WarningHook {
    Box::new(|summary, body| {
        let (summary, body) = (summary.to_owned(), body.to_owned());
        tokio::spawn(async move { notify::send_warning(&summary, &body).await });
    })
}

fn host_sampler(config: watch::Receiver<ResolvedConfig>) -> Sampler {
    let root = SysRoot::host();
    let cpu: Box<dyn Source<CpuSnapshot> + Send> = match CpuCollector::new(&root) {
        Ok(collector) => Box::new(collector),
        Err(error) => {
            warn!(%error, "cannot read the CPU topology; the cpu topic stays empty");
            Box::new(NoCpu)
        }
    };
    let gpu = open_gpu();
    if gpu.is_none() {
        info!("NVML is unavailable; the gpu topic reports no GPU");
    }
    let sensors_root = root.clone();
    let disks_root = root.clone();
    Sampler::new(
        cpu,
        Box::new(MemCollector::new(&root, TOP_PROCESSES)),
        Box::new(Relabeled::new(
            config.clone(),
            sensor_labels,
            move |labels| SensorCollector::new(&sensors_root, labels),
        )),
        Box::new(GpuCollector::new(gpu)),
        Box::new(Relabeled::new(config, disk_labels, move |labels| {
            DiskCollector::new(&disks_root, Box::new(StatvfsFs), labels)
        })),
    )
}

fn host_feeds() -> Vec<Feed> {
    vec![feeds::host::feed(SysRoot::host())]
}

fn open_gpu() -> Option<Box<dyn GpuSource>> {
    NvmlSource::open().map(|source| Box::new(source) as Box<dyn GpuSource>)
}

fn sensor_labels(config: &ResolvedConfig) -> &Labels {
    &config.sensors.labels
}

fn disk_labels(config: &ResolvedConfig) -> &Labels {
    &config.disks.labels
}

/// Stands in for a CPU collector that could not start, so everything else still runs.
struct NoCpu;

impl Source<CpuSnapshot> for NoCpu {
    fn sample(&mut self) -> CpuSnapshot {
        CpuSnapshot::default()
    }
}

/// Rebuilds a collector whenever the labels it was built with change in the live config.
struct Relabeled<C, B> {
    config: watch::Receiver<ResolvedConfig>,
    labels_of: fn(&ResolvedConfig) -> &Labels,
    build: B,
    labels: Labels,
    collector: C,
}

impl<C, B: Fn(&Labels) -> C> Relabeled<C, B> {
    fn new(
        mut config: watch::Receiver<ResolvedConfig>,
        labels_of: fn(&ResolvedConfig) -> &Labels,
        build: B,
    ) -> Self {
        let labels = labels_of(&config.borrow_and_update()).clone();
        Self {
            collector: build(&labels),
            config,
            labels_of,
            build,
            labels,
        }
    }
}

impl<T, C: Source<T>, B: Fn(&Labels) -> C> Source<T> for Relabeled<C, B> {
    fn sample(&mut self) -> T {
        // A closed channel means the config can no longer change, so the current labels stand.
        if self.config.has_changed().unwrap_or(false) {
            let labels = (self.labels_of)(&self.config.borrow_and_update()).clone();
            if labels != self.labels {
                self.collector = (self.build)(&labels);
                self.labels = labels;
            }
        }
        self.collector.sample()
    }

    fn resume(&mut self) {
        self.collector.resume();
    }
}

fn probe(topic: &str) -> ExitCode {
    let root = SysRoot::host();
    let no_labels = Labels::new();
    match topic {
        topics::CPU => match CpuCollector::new(&root) {
            Ok(collector) => probe_source(collector),
            Err(error) => {
                eprintln!("ghostd: cannot read the CPU topology: {error}");
                ExitCode::FAILURE
            }
        },
        topics::MEM => probe_with(
            MemCollector::new(&root, TOP_PROCESSES),
            Baseline::TakenByNew,
        ),
        topics::SENSORS => probe_source(SensorCollector::new(&root, &no_labels)),
        // A machine without a readable GPU is not a failure: it prints `present:false`, exactly
        // what the daemon would publish.
        topics::GPU => probe_source(GpuCollector::new(open_gpu())),
        topics::DISKS => probe_source(DiskCollector::new(&root, Box::new(StatvfsFs), &no_labels)),
        topics::PERF => probe_source(SelfMon::new(root)),
        _ => usage(),
    }
}

/// How a probe gets the baseline that its one printed sample is measured against.
enum Baseline {
    /// A throwaway sample, taken right away.
    SampleFirst,
    /// The collector's constructor already took it. For the memory collector a throwaway sample
    /// would also be its first process scan, microseconds after the constructor, and the sample
    /// printed a second later would only repeat that scan's shares.
    TakenByNew,
}

/// Prints one sample as a line of JSON, taken a second after a baseline sample so that loads and
/// rates cover a real interval.
fn probe_source<T: Serialize>(source: impl Source<T>) -> ExitCode {
    probe_with(source, Baseline::SampleFirst)
}

/// As [`probe_source`], with the baseline taken as `baseline` says.
fn probe_with<T: Serialize>(source: impl Source<T>, baseline: Baseline) -> ExitCode {
    let sample = sample_after(source, baseline, || thread::sleep(PROBE_INTERVAL));
    // Piping into `head` closes stdout early, which must not panic like `println!` does. Writing is
    // the only step that can fail: a snapshot always serialises, since a non-finite float encodes as
    // null and none of its maps has non-string keys.
    let mut stdout = io::stdout().lock();
    let written = serde_json::to_writer(&mut stdout, &sample)
        .map_err(io::Error::from)
        .and_then(|()| writeln!(stdout));
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

/// `wait` stands in for the sleep, so that a test can change what the source reads during it.
fn sample_after<T>(mut source: impl Source<T>, baseline: Baseline, wait: impl FnOnce()) -> T {
    if let Baseline::SampleFirst = baseline {
        source.sample();
    }
    wait();
    source.sample()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;

    use super::*;

    /// Reports the labels it was built with.
    struct BuiltWith(Labels);

    impl Source<Labels> for BuiltWith {
        fn sample(&mut self) -> Labels {
            self.0.clone()
        }
    }

    #[test]
    fn relabeled_rebuilds_only_when_its_own_labels_change() {
        let (config_tx, config_rx) = watch::channel(ResolvedConfig::default());
        let builds = Cell::new(0);
        let mut source = Relabeled::new(config_rx, sensor_labels, |labels| {
            builds.set(builds.get() + 1);
            BuiltWith(labels.clone())
        });
        assert!(source.sample().is_empty());

        config_tx.send_modify(|config| {
            config
                .disks
                .labels
                .insert("SERIAL".to_owned(), "Backup".to_owned());
        });
        source.sample();
        assert_eq!(builds.get(), 1, "a disk label change rebuilt the sensors");

        config_tx.send_modify(|config| {
            config
                .sensors
                .labels
                .insert("k10temp/Tctl".to_owned(), "CPU".to_owned());
        });
        assert_eq!(source.sample()["k10temp/Tctl"], "CPU");
        assert_eq!(builds.get(), 2);
    }

    /// Counts the resumes it is passed.
    struct Resumes<'a>(&'a Cell<u32>);

    impl Source<()> for Resumes<'_> {
        fn sample(&mut self) {}

        fn resume(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn relabeled_passes_a_resume_on() {
        let (_config_tx, config_rx) = watch::channel(ResolvedConfig::default());
        let resumes = Cell::new(0);
        let mut source = Relabeled::new(config_rx, sensor_labels, |_| Resumes(&resumes));

        source.resume();

        assert_eq!(resumes.get(), 1);
    }

    struct CallCount(u32);

    impl Source<u32> for CallCount {
        fn sample(&mut self) -> u32 {
            self.0 += 1;
            self.0
        }
    }

    #[test]
    fn a_probe_takes_a_throwaway_sample_only_when_asked_to() {
        assert_eq!(sample_after(CallCount(0), Baseline::SampleFirst, || {}), 2);
        assert_eq!(sample_after(CallCount(0), Baseline::TakenByNew, || {}), 1);
    }

    const IDLE_TICKS: u64 = 10_000;
    const WINDOW_TICKS: u64 = 3_200;
    const BUSY_TICKS: u64 = 100;
    // 100 of the 3200 ticks that passed, as a percentage.
    const BUSY_SHARE: f32 = 3.125;

    fn write_proc(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    /// A machine with one process that has used `process_ticks` of the `idle_ticks` that passed.
    fn write_machine(root: &Path, idle_ticks: u64, process_ticks: u64) {
        write_proc(
            root,
            "proc/stat",
            &format!("cpu 0 0 0 {idle_ticks} 0 0 0 0\n"),
        );
        write_proc(
            root,
            "proc/meminfo",
            "MemTotal: 1000 kB\nMemAvailable: 500 kB\n",
        );
        // Fields 14 (utime), 22 (starttime) and 24 (rss) are the ones the collector reads.
        write_proc(
            root,
            "proc/1/stat",
            &format!("1 (worker) S 0 0 0 0 0 0 0 0 0 0 {process_ticks} 0 0 0 0 0 0 0 1000 0 8 0\n"),
        );
    }

    fn probe_memory(baseline: Baseline) -> f32 {
        let dir = tempfile::tempdir().unwrap();
        write_machine(dir.path(), IDLE_TICKS, 0);
        let memory = MemCollector::new(&SysRoot::new(dir.path()), TOP_PROCESSES);

        let sample = sample_after(memory, baseline, || {
            write_machine(dir.path(), IDLE_TICKS + WINDOW_TICKS, BUSY_TICKS);
        });

        sample.top[0].cpu
    }

    #[test]
    fn a_memory_probe_reports_shares_over_the_whole_wait() {
        assert_eq!(probe_memory(Baseline::TakenByNew), BUSY_SHARE);
    }

    #[test]
    fn a_throwaway_sample_would_hide_the_wait_from_the_memory_collector() {
        // Its first sample is already the process scan, so a second one only repeats that scan.
        assert_eq!(probe_memory(Baseline::SampleFirst), 0.0);
    }
}
