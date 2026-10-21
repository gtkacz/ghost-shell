//! INMET's active weather alerts (`avisos/ativos`), read into the `weather.alerts` shape for one
//! area, and the provider protocol lines that carry them to ghostd.

use std::collections::HashSet;

use ghost_proto::{AlertLevel, ClientMsg, PROTO_VERSION, WeatherAlert, WeatherAlerts, topics};
use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use serde::{Deserialize, Deserializer};
use tracing::warn;

/// The name the provider claims its topic under.
pub const PROVIDER_NAME: &str = "inmet";
/// Who the published alerts come from.
pub const SOURCE: &str = "INMET";

// Read through the tzdb rather than as a fixed UTC−3, in case daylight saving returns.
const TIME_ZONE: &str = "America/Sao_Paulo";
// INMET writes each date as a UTC midnight, but the date is São Paulo's own calendar date.
const MIDNIGHT_UTC_SUFFIX: &str = "T00:00:00.000Z";
const DATE_LENGTH: usize = 10;
const TIME_DESIGNATOR: char = 'T';
const DATE_SEPARATOR: char = '-';
const DATE_DIGITS: [usize; 3] = [4, 2, 2];
const TIME_SEPARATOR: char = ':';
const HOUR_MINUTE_DIGITS: [usize; 2] = [2, 2];
const HOUR_MINUTE_SECOND_DIGITS: [usize; 3] = [2, 2, 2];
const GEOCODE_SEPARATOR: char = ',';
const POTENTIAL_DANGER: &str = "PERIGO POTENCIAL";
const DANGER: &str = "PERIGO";
const GREAT_DANGER: &str = "GRANDE PERIGO";

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("INMET's answer is not its alert list: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the system time zone database has no {TIME_ZONE}; is tzdata installed? ({0})")]
    TimeZone(jiff::Error),
}

#[derive(Deserialize)]
struct Avisos {
    #[serde(default, deserialize_with = "null_as_default")]
    hoje: Vec<Aviso>,
    #[serde(default, deserialize_with = "null_as_default")]
    futuro: Vec<Aviso>,
}

/// One alert as INMET sends it; only the fields the dashboard shows are read. Every field but `id`
/// may be missing or null, so that one odd alert elsewhere in Brazil cannot hide the area's.
#[derive(Deserialize)]
struct Aviso {
    id: u64,
    #[serde(default, deserialize_with = "null_as_default")]
    geocodes: String,
    #[serde(default, deserialize_with = "null_as_default")]
    descricao: String,
    #[serde(default, deserialize_with = "null_as_default")]
    severidade: String,
    #[serde(default, deserialize_with = "null_as_default")]
    data_inicio: String,
    #[serde(default, deserialize_with = "null_as_default")]
    hora_inicio: String,
    #[serde(default, deserialize_with = "null_as_default")]
    data_fim: String,
    #[serde(default, deserialize_with = "null_as_default")]
    hora_fim: String,
    #[serde(default, deserialize_with = "null_as_default")]
    riscos: Vec<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    encerrado: bool,
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// The alerts of `hoje`, then `futuro`, that cover `area`, an IBGE municipality code such as
/// `3550308`, and have not ended, sorted by start. An id listed twice keeps its first entry that
/// can be read, and an entry whose times cannot be read is skipped and logged.
pub fn parse_alerts(json: &str, area: &str) -> Result<Vec<WeatherAlert>, ParseError> {
    let avisos: Avisos = serde_json::from_str(json)?;
    let zone = TimeZone::get(TIME_ZONE).map_err(ParseError::TimeZone)?;
    let current: Vec<Aviso> = avisos
        .hoje
        .into_iter()
        .chain(avisos.futuro)
        .filter(|aviso| !aviso.encerrado && covers(aviso, area))
        .collect();
    // Once per fetch rather than per alert: a serialiser change affects every date at once.
    if let Some(date) = current
        .iter()
        .flat_map(|aviso| [&aviso.data_inicio, &aviso.data_fim])
        .find(|date| has_unfamiliar_suffix(date))
    {
        warn!(
            date = %date,
            "INMET's date format has changed; reading each date's first ten characters as the date \
             in São Paulo"
        );
    }
    let mut seen = HashSet::new();
    let mut alerts: Vec<WeatherAlert> = current
        .into_iter()
        .filter_map(|aviso| alert(aviso, &zone))
        .filter(|alert| seen.insert(alert.id.clone()))
        .collect();
    alerts.sort_by_key(|alert| alert.start_s);
    Ok(alerts)
}

fn has_unfamiliar_suffix(date: &str) -> bool {
    date.get(DATE_LENGTH..)
        .is_some_and(|suffix| suffix.starts_with(TIME_DESIGNATOR) && suffix != MIDNIGHT_UTC_SUFFIX)
}

fn covers(aviso: &Aviso, area: &str) -> bool {
    aviso
        .geocodes
        .split(GEOCODE_SEPARATOR)
        .any(|code| code.trim() == area)
}

fn alert(aviso: Aviso, zone: &TimeZone) -> Option<WeatherAlert> {
    let start_s = instant(&aviso.data_inicio, &aviso.hora_inicio, zone);
    let end_s = instant(&aviso.data_fim, &aviso.hora_fim, zone);
    let (Some(start_s), Some(end_s)) = (start_s, end_s) else {
        warn!(
            id = aviso.id,
            start = format!("{} {}", aviso.data_inicio, aviso.hora_inicio),
            end = format!("{} {}", aviso.data_fim, aviso.hora_fim),
            "skipping an alert whose times cannot be read"
        );
        return None;
    };
    let severity = aviso.severidade.trim().to_uppercase();
    Some(WeatherAlert {
        id: aviso.id.to_string(),
        title: aviso.descricao.to_uppercase(),
        level: level(aviso.id, &severity),
        severity,
        start_s,
        end_s,
        risks: aviso.riscos,
    })
}

fn level(id: u64, severity: &str) -> AlertLevel {
    match severity {
        POTENTIAL_DANGER => AlertLevel::Warn,
        DANGER | GREAT_DANGER => AlertLevel::Crit,
        unknown => {
            warn!(
                id,
                severity = unknown,
                "unknown INMET severity; shown as a warning"
            );
            AlertLevel::Warn
        }
    }
}

/// Seconds since the Unix epoch of `date` at `time` in `zone`, or `None` unless the date is
/// `YYYY-MM-DD`, alone or followed by a `T…` suffix that is ignored, and the time is `HH:MM` or
/// `HH:MM:SS`.
fn instant(date: &str, time: &str, zone: &TimeZone) -> Option<u64> {
    let (date, suffix) = date.split_at_checked(DATE_LENGTH)?;
    if !(suffix.is_empty() || suffix.starts_with(TIME_DESIGNATOR)) {
        return None;
    }
    let [year, month, day] = numbers(date, DATE_SEPARATOR, DATE_DIGITS)?;
    let [hour, minute, second] = numbers(time, TIME_SEPARATOR, HOUR_MINUTE_DIGITS)
        .map(|[hour, minute]| [hour, minute, 0])
        .or_else(|| numbers(time, TIME_SEPARATOR, HOUR_MINUTE_SECOND_DIGITS))?;
    let date = Date::new(year, narrow(month)?, narrow(day)?).ok()?;
    let time = Time::new(narrow(hour)?, narrow(minute)?, narrow(second)?, 0).ok()?;
    let timestamp = zone.to_timestamp(date.to_datetime(time)).ok()?;
    u64::try_from(timestamp.as_second()).ok()
}

/// The numbers `text` holds between `separator`s, each written with exactly as many digits as
/// `widths` gives it.
fn numbers<const N: usize>(text: &str, separator: char, widths: [usize; N]) -> Option<[i16; N]> {
    let mut parts = text.split(separator);
    let mut numbers = [0; N];
    for (number, width) in numbers.iter_mut().zip(widths) {
        let part = parts.next()?;
        if part.len() != width || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *number = part.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

fn narrow(number: i16) -> Option<i8> {
    i8::try_from(number).ok()
}

/// The handshake that claims `weather.alerts`.
pub fn provide_line() -> String {
    line(&ClientMsg::Provide {
        proto: PROTO_VERSION,
        name: PROVIDER_NAME.to_owned(),
        topics: vec![topics::WEATHER_ALERTS.to_owned()],
    })
}

/// Publishes `alerts` on `weather.alerts`.
pub fn publish_line(alerts: &WeatherAlerts) -> String {
    line(&ClientMsg::Publish {
        topic: topics::WEATHER_ALERTS.to_owned(),
        data: serde_json::to_value(alerts).expect("WeatherAlerts serializes to JSON"),
    })
}

fn line(message: &ClientMsg) -> String {
    let mut line = serde_json::to_string(message).expect("a client message serializes to JSON");
    line.push('\n');
    line
}
