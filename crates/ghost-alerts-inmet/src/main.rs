//! `ghost-alerts-inmet --area <code> [--from pt-BR --to <language>]`: a ghostd provider that
//! publishes INMET's weather alerts for one area on `weather.alerts`, fetched every 10 minutes, with
//! their event and severity translated when a language is given.

use std::env;
use std::io::{self, IsTerminal, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ghost_alerts_inmet::{
    IssuedAlert, ParseError, SOURCE, Translator, default_store_path, parse_alerts, provide_line,
    publish_line,
};
use ghost_proto::{WeatherAlert, WeatherAlerts};
use tracing::{error, info, warn};
use ureq::Agent;

const USAGE: &str = "usage: ghost-alerts-inmet --area <code> [--from <language> --to <language>]";
const AREA_FLAG: &str = "--area";
const FROM_FLAG: &str = "--from";
const TO_FLAG: &str = "--to";
const EXIT_USAGE: u8 = 2;
/// Where ghostd tells its providers to connect.
const SOCKET_ENV: &str = "GHOST_SOCKET";
const ALERTS_URL: &str = "https://apiprevmet3.inmet.gov.br/avisos/ativos";
const FETCH_INTERVAL: Duration = Duration::from_secs(10 * 60);
const FIRST_RETRY: Duration = Duration::from_secs(60);
const RETRY_GROWTH: u32 = 2;
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
// The monotonic clock and the socket's read timeout both stop during a suspend and nothing reports
// the resume, so a wait looks at the wall clock at least this often, as ghostd's weather feed does.
const WALL_CLOCK_CHECK: Duration = Duration::from_secs(60);
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
    let Some(options) = options(&args) else {
        return usage();
    };
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .init();
    let Some(socket) = env::var_os(SOCKET_ENV) else {
        error!("{SOCKET_ENV} is not set; ghostd sets it for the providers it starts");
        return ExitCode::from(EXIT_USAGE);
    };
    let ended = provide(Path::new(&socket), &options);
    // Reconnecting is left to ghostd, whose supervisor starts the provider again with a backoff.
    error!(%ended, "stopping");
    ExitCode::FAILURE
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(EXIT_USAGE)
}

struct Options<'a> {
    area: &'a str,
    /// The language to translate from, and the one to translate into.
    languages: Option<(&'a str, &'a str)>,
}

fn options(args: &[String]) -> Option<Options<'_>> {
    let (area, languages) = match args {
        [area_flag, area] if area_flag == AREA_FLAG => (area, None),
        [area_flag, area, from_flag, from, to_flag, to]
            if area_flag == AREA_FLAG && from_flag == FROM_FLAG && to_flag == TO_FLAG =>
        {
            (area, Some((from.as_str(), to.as_str())))
        }
        _ => return None,
    };
    (!area.is_empty()).then_some(Options {
        area: area.as_str(),
        languages,
    })
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
fn provide(socket: &Path, options: &Options) -> Ended {
    let area = options.area;
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
    let mut translator = options.languages.and_then(|(from, to)| {
        Translator::new(from, to, default_store_path(), agent.clone())
            .inspect_err(|error| warn!(%error, "publishing INMET's words untranslated"))
            .ok()
    });
    let mut retry = FIRST_RETRY;
    loop {
        let pause = match fetch(&agent, area) {
            Ok(fetched) => {
                let mut send = |alerts| publish(&mut daemon, fetched.updated_s, alerts);
                let published = match translator.as_mut() {
                    Some(translator) => {
                        translator.publish_translated(&fetched.issued, Instant::now(), send)
                    }
                    None => send(
                        fetched
                            .issued
                            .into_iter()
                            .map(|issued| issued.alert)
                            .collect(),
                    ),
                };
                if let Err(error) = published {
                    return Ended::Write(error);
                }
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

/// The area's alerts as INMET issued them, and when they were fetched.
struct Fetched {
    updated_s: u64,
    issued: Vec<IssuedAlert>,
}

fn fetch(agent: &Agent, area: &str) -> Result<Fetched, FetchError> {
    let body = agent.get(ALERTS_URL).call()?.body_mut().read_to_string()?;
    Ok(Fetched {
        updated_s: unix_now_s(),
        issued: parse_alerts(&body, area)?,
    })
}

fn publish(daemon: &mut UnixStream, updated_s: u64, alerts: Vec<WeatherAlert>) -> io::Result<()> {
    let alerts = WeatherAlerts {
        source: SOURCE.to_owned(),
        updated_s,
        alerts,
    };
    daemon.write_all(publish_line(&alerts).as_bytes())?;
    info!(alerts = alerts.alerts.len(), "published");
    Ok(())
}

fn unix_now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Sleeps for `period` of wall-clock time while watching the connection, which ghostd writes to
/// only to end it: a provider it refused, or whose daemon is gone, would otherwise go on fetching
/// for nobody.
fn wait(daemon: &mut UnixStream, period: Duration) -> Result<(), Ended> {
    let began = SystemTime::now();
    let mut buffer = [0; ERROR_BUFFER_BYTES];
    while let Some(timeout) = read_timeout(began, period, SystemTime::now()) {
        daemon
            .set_read_timeout(Some(timeout))
            .map_err(Ended::Read)?;
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
    Ok(())
}

/// How long the next read may block, at most [`WALL_CLOCK_CHECK`], or `None` once `now` is
/// `period` past `began`. A clock set back before `began` ends the wait too, as it would otherwise
/// hold the next fetch back by as much as it moved.
fn read_timeout(began: SystemTime, period: Duration, now: SystemTime) -> Option<Duration> {
    let elapsed = now.duration_since(began).ok()?;
    let left = period.saturating_sub(elapsed);
    (!left.is_zero()).then(|| left.min(WALL_CLOCK_CHECK))
}

fn is_timeout_or_interrupt(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-21 18:00 in São Paulo.
    const BEGAN_S: u64 = 1_792_616_400;
    const NIGHT: Duration = Duration::from_secs(8 * 60 * 60);
    const HALF_A_MINUTE: Duration = Duration::from_secs(30);

    fn began() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(BEGAN_S)
    }

    #[test]
    fn a_wait_whose_deadline_has_passed_fetches_now() {
        // A night's suspend, which the monotonic clock and the read timeout never saw.
        assert_eq!(read_timeout(began(), FETCH_INTERVAL, began() + NIGHT), None);
        assert_eq!(
            read_timeout(began(), FETCH_INTERVAL, began() + FETCH_INTERVAL),
            None
        );
    }

    #[test]
    fn no_read_waits_longer_than_the_wall_clock_check() {
        assert_eq!(
            read_timeout(began(), FETCH_INTERVAL, began()),
            Some(WALL_CLOCK_CHECK)
        );
        let near_the_end = began() + FETCH_INTERVAL - HALF_A_MINUTE;
        assert_eq!(
            read_timeout(began(), FETCH_INTERVAL, near_the_end),
            Some(HALF_A_MINUTE)
        );
        assert_eq!(
            read_timeout(began(), CLAIM_SETTLE, began()),
            Some(CLAIM_SETTLE)
        );
    }

    #[test]
    fn a_clock_set_back_before_the_wait_began_fetches_now() {
        assert_eq!(read_timeout(began(), FETCH_INTERVAL, began() - NIGHT), None);
    }
}
