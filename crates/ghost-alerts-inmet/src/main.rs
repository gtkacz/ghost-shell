//! `ghost-alerts-inmet --area <code>`: a ghostd provider that publishes INMET's weather alerts for
//! one area on `weather.alerts`, fetched every 10 minutes.

use std::env;
use std::io::{self, IsTerminal, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ghost_alerts_inmet::{ParseError, SOURCE, parse_alerts, provide_line, publish_line};
use ghost_proto::WeatherAlerts;
use tracing::{error, info, warn};
use ureq::Agent;

const USAGE: &str = "usage: ghost-alerts-inmet --area <code>";
const AREA_FLAG: &str = "--area";
const EXIT_USAGE: u8 = 2;
/// Where ghostd tells its providers to connect.
const SOCKET_ENV: &str = "GHOST_SOCKET";
const ALERTS_URL: &str = "https://apiprevmet3.inmet.gov.br/avisos/ativos";
const FETCH_INTERVAL: Duration = Duration::from_secs(10 * 60);
const FIRST_RETRY: Duration = Duration::from_secs(60);
const RETRY_GROWTH: u32 = 2;
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
// INMET's firewall resets connections that send curl's default User-Agent, so every request names
// itself, as ghostd's own requests do.
const USER_AGENT: &str = concat!("ghost-shell/", env!("CARGO_PKG_VERSION"));
// ghostd answers a refused claim at once, so a refused provider exits before it fetches anything
// rather than fetching once per restart.
const CLAIM_SETTLE: Duration = Duration::from_secs(1);
// Room for the one error line ghostd may send before it closes; it is only logged.
const ERROR_BUFFER_BYTES: usize = 1024;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [flag, area] = args.as_slice() else {
        return usage();
    };
    if flag != AREA_FLAG || area.is_empty() {
        return usage();
    }
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .init();
    let Some(socket) = env::var_os(SOCKET_ENV) else {
        error!("{SOCKET_ENV} is not set; ghostd sets it for the providers it starts");
        return ExitCode::from(EXIT_USAGE);
    };
    let ended = provide(Path::new(&socket), area);
    // Reconnecting is left to ghostd, whose supervisor starts the provider again with a backoff.
    error!(%ended, "stopping");
    ExitCode::FAILURE
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(EXIT_USAGE)
}

/// Why the provider stopped serving ghostd.
#[derive(Debug, thiserror::Error)]
enum Ended {
    #[error("cannot connect to ghostd: {0}")]
    Connect(io::Error),
    #[error("cannot write to ghostd: {0}")]
    Write(io::Error),
    #[error("cannot read from ghostd: {0}")]
    Read(io::Error),
    #[error("ghostd closed the connection")]
    Closed,
    #[error("ghostd ended the session: {0}")]
    Refused(String),
}

#[derive(Debug, thiserror::Error)]
enum FetchError {
    #[error("cannot fetch INMET's alerts: {0}")]
    Http(#[from] ureq::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

/// Claims `weather.alerts` and publishes the area's alerts every `FETCH_INTERVAL` until the
/// connection to ghostd fails. A failed fetch publishes nothing, so ghostd keeps the last alerts,
/// and is retried after `FIRST_RETRY`, then twice as long each time up to `FETCH_INTERVAL`.
fn provide(socket: &Path, area: &str) -> Ended {
    let mut daemon = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(error) => return Ended::Connect(error),
    };
    if let Err(error) = daemon.write_all(provide_line().as_bytes()) {
        return Ended::Write(error);
    }
    if let Err(ended) = wait(&mut daemon, CLAIM_SETTLE) {
        return ended;
    }
    info!(area, "providing weather.alerts");
    let agent = Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .user_agent(USER_AGENT)
        .build()
        .new_agent();
    let mut retry = FIRST_RETRY;
    loop {
        let pause = match fetch(&agent, area) {
            Ok(alerts) => {
                if let Err(error) = daemon.write_all(publish_line(&alerts).as_bytes()) {
                    return Ended::Write(error);
                }
                info!(alerts = alerts.alerts.len(), "published");
                retry = FIRST_RETRY;
                FETCH_INTERVAL
            }
            Err(error) => {
                warn!(%error, "nothing published; retrying in {retry:?}");
                let pause = retry;
                retry = (retry * RETRY_GROWTH).min(FETCH_INTERVAL);
                pause
            }
        };
        if let Err(ended) = wait(&mut daemon, pause) {
            return ended;
        }
    }
}

fn fetch(agent: &Agent, area: &str) -> Result<WeatherAlerts, FetchError> {
    let body = agent.get(ALERTS_URL).call()?.body_mut().read_to_string()?;
    Ok(WeatherAlerts {
        source: SOURCE.to_owned(),
        updated_s: unix_now_s(),
        alerts: parse_alerts(&body, area)?,
    })
}

fn unix_now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Sleeps for `period` while watching the connection, which ghostd writes to only to end it:
/// a provider it refused, or whose daemon is gone, would otherwise go on fetching for nobody.
fn wait(daemon: &mut UnixStream, period: Duration) -> Result<(), Ended> {
    let deadline = Instant::now() + period;
    let mut buffer = [0; ERROR_BUFFER_BYTES];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(());
        }
        daemon.set_read_timeout(Some(left)).map_err(Ended::Read)?;
        match daemon.read(&mut buffer) {
            Ok(0) => return Err(Ended::Closed),
            Ok(read) => {
                let message = String::from_utf8_lossy(&buffer[..read]);
                return Err(Ended::Refused(message.trim_end().to_owned()));
            }
            Err(error) if is_timeout_or_interrupt(&error) => {}
            Err(error) => return Err(Ended::Read(error)),
        }
    }
}

fn is_timeout_or_interrupt(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}
