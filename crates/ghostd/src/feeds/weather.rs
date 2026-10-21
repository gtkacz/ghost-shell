//! The `weather` topic: Open-Meteo's current weather, today's high and low, the coming hours and
//! the US AQI at `location.{lat, lon}`.
//!
//! While some client wants the topic, the forecast is fetched every 15 min and air quality every
//! 30 min (spec §12). A failed fetch is retried after 60 s, then after twice as long each time, up
//! to its cadence. The last good values stay published, marked stale once the forecast is two
//! forecast cadences old.
//!
//! Ages and due times are wall-clock seconds, because tokio's clock stands still while the machine
//! is suspended: a forecast from before a night's sleep must read stale, and be fetched again,
//! within a minute of waking.

use std::cmp::Reverse;
use std::future::pending;
use std::sync::Arc;
use std::time::Duration;

use ghost_proto::{RainPeak, ResolvedConfig, WeatherHour, WeatherSnapshot, topics};
use serde::Deserialize;
use tokio::sync::watch;
use tokio::task::spawn_blocking;
use tokio::time::sleep;
use tracing::warn;

use super::http::{Fetch, FetchError};
use super::{Place, WallClock, place_in, unix_now_s, wanted};
use crate::server::{Feed, FeedContext};

const FORECAST_EVERY_S: u64 = 15 * 60;
const AIR_EVERY_S: u64 = 30 * 60;
const FIRST_RETRY_S: u64 = 60;
const STALE_AFTER_S: u64 = 2 * FORECAST_EVERY_S;
// Tokio's timers stop during a suspend and nothing reports the resume, so while the topic is
// wanted the feed looks at the wall clock at least this often.
const WALL_CLOCK_CHECK: Duration = Duration::from_secs(60);
const HOUR_S: u64 = 60 * 60;
const HOURS_SHOWN: usize = 6;
const RAIN_PEAK_WINDOW_S: u64 = 12 * HOUR_S;

pub fn forecast_url(lat: f64, lon: f64) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m,is_day\
         &hourly=temperature_2m,precipitation_probability\
         &daily=temperature_2m_max,temperature_2m_min\
         &timezone=auto&timeformat=unixtime&forecast_days=2"
    )
}

pub fn air_url(lat: f64, lon: f64) -> String {
    format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={lat}&longitude={lon}\
         &current=us_aqi&timeformat=unixtime"
    )
}

#[derive(Debug, thiserror::Error)]
pub enum WeatherError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("unexpected response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the forecast has no high and low for today")]
    NoToday,
}

/// A forecast response, with the hours taken from the time it was read at.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub temp_c: f32,
    pub code: u8,
    pub condition: &'static str,
    pub is_day: bool,
    pub humidity_pct: u8,
    pub wind_kmh: f32,
    pub high_c: f32,
    pub low_c: f32,
    pub hours: Vec<WeatherHour>,
    pub rain_peak: Option<RainPeak>,
}

#[derive(Deserialize)]
struct ForecastResponse {
    current: Current,
    hourly: Hourly,
    daily: Daily,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f32,
    relative_humidity_2m: u8,
    weather_code: u8,
    wind_speed_10m: f32,
    is_day: u8,
}

/// Open-Meteo sends `null` for an hour that its models do not cover.
#[derive(Deserialize)]
struct Hourly {
    time: Vec<u64>,
    temperature_2m: Vec<Option<f32>>,
    precipitation_probability: Vec<Option<u8>>,
}

#[derive(Deserialize)]
struct Daily {
    temperature_2m_max: Vec<f32>,
    temperature_2m_min: Vec<f32>,
}

#[derive(Deserialize)]
struct AirResponse {
    current: AirCurrent,
}

#[derive(Deserialize)]
struct AirCurrent {
    us_aqi: Option<u32>,
}

/// Reads a response to [`forecast_url`] as of `now_s`, in seconds since the Unix epoch. An hour
/// with no temperature or no chance of rain is left out of `hours`, and one with no chance of rain
/// out of `rain_peak`.
pub fn parse_forecast(json: &str, now_s: u64) -> Result<Forecast, WeatherError> {
    let ForecastResponse {
        current,
        hourly,
        daily,
    } = serde_json::from_str(json)?;
    let (Some(&high_c), Some(&low_c)) = (
        daily.temperature_2m_max.first(),
        daily.temperature_2m_min.first(),
    ) else {
        return Err(WeatherError::NoToday);
    };
    let entries = hourly
        .time
        .iter()
        .zip(&hourly.temperature_2m)
        .zip(&hourly.precipitation_probability)
        .map(|((&at_s, &temp_c), &rain_pct)| (at_s, temp_c, rain_pct));
    let window = now_s..=now_s + RAIN_PEAK_WINDOW_S;
    let rain_peak = entries
        .clone()
        .filter_map(|(at_s, _, rain_pct)| {
            Some(RainPeak {
                pct: rain_pct?,
                at_s,
            })
        })
        .filter(|peak| window.contains(&peak.at_s))
        .max_by_key(|peak| (peak.pct, Reverse(peak.at_s)))
        .filter(|peak| peak.pct > 0);
    let hours = entries
        .filter(|&(at_s, ..)| at_s > now_s)
        .filter_map(|(at_s, temp_c, rain_pct)| {
            Some(WeatherHour {
                at_s,
                temp_c: temp_c?,
                rain_pct: rain_pct?,
            })
        })
        .take(HOURS_SHOWN)
        .collect();
    Ok(Forecast {
        temp_c: current.temperature_2m,
        code: current.weather_code,
        condition: condition(current.weather_code),
        is_day: current.is_day != 0,
        humidity_pct: current.relative_humidity_2m,
        wind_kmh: current.wind_speed_10m,
        high_c,
        low_c,
        hours,
        rain_peak,
    })
}

/// Reads the US AQI from a response to [`air_url`]; `None` when Open-Meteo has none to give.
pub fn parse_air(json: &str) -> Result<Option<u32>, WeatherError> {
    let response: AirResponse = serde_json::from_str(json)?;
    Ok(response.current.us_aqi)
}

/// The label for a WMO weather code, as Open-Meteo's table groups them.
pub fn condition(code: u8) -> &'static str {
    match code {
        0 => "CLEAR",
        1 => "MAINLY CLEAR",
        2 => "PARTLY CLOUDY",
        3 => "OVERCAST",
        45 => "FOG",
        48 => "RIME FOG",
        51 | 53 | 55 => "DRIZZLE",
        56 | 57 => "FREEZING DRIZZLE",
        61 => "LIGHT RAIN",
        63 => "RAIN",
        65 => "HEAVY RAIN",
        66 | 67 => "FREEZING RAIN",
        71 => "LIGHT SNOW",
        73 => "SNOW",
        75 => "HEAVY SNOW",
        77 => "SNOW GRAINS",
        80 | 81 => "SHOWERS",
        82 => "VIOLENT SHOWERS",
        85 | 86 => "SNOW SHOWERS",
        95 => "THUNDERSTORM",
        96 | 99 => "THUNDERSTORM WITH HAIL",
        _ => "UNKNOWN",
    }
}

/// Publishes `weather` for the configured place, fetching with `fetch` as the module docs
/// describe. Without a place it publishes the unavailable default and fetches nothing.
pub fn feed(fetch: Arc<dyn Fetch>) -> Feed {
    feed_with_clock(fetch, Arc::new(unix_now_s))
}

/// As [`feed`], reading the wall clock from `clock`.
pub fn feed_with_clock(fetch: Arc<dyn Fetch>, clock: WallClock) -> Feed {
    Feed::new(topics::WEATHER, move |context| run(fetch, clock, context))
}

async fn run(fetch: Arc<dyn Fetch>, clock: WallClock, mut context: FeedContext) {
    let mut weather = Weather::at(place_in(&context.config.borrow_and_update()));
    context
        .publisher
        .publish(topics::WEATHER, &weather.snapshot());
    loop {
        let wake = weather
            .next_wake(clock())
            .filter(|_| wanted(&context.demand, topics::WEATHER));
        tokio::select! {
            // A new place is taken before anything due runs, so nothing is fetched for the old one.
            biased;
            Ok(()) = context.config.changed() => {
                let place = place_in(&context.config.borrow_and_update());
                if place != weather.place {
                    weather = Weather::at(place);
                    context.publisher.publish(topics::WEATHER, &weather.snapshot());
                }
            }
            Ok(()) = context.demand.changed() => {}
            () = sleep_or_forever(wake) => {
                // Published before fetching, since the fetch can take up to its timeout.
                if weather.mark_stale(clock()) {
                    context.publisher.publish(topics::WEATHER, &weather.snapshot());
                }
                if weather.fetch_due(&fetch, &clock, &context.config).await {
                    context.publisher.publish(topics::WEATHER, &weather.snapshot());
                }
            }
        }
    }
}

async fn sleep_or_forever(duration: Option<Duration>) {
    match duration {
        Some(duration) => sleep(duration).await,
        None => pending().await,
    }
}

/// What the feed knows about one place, and when it next fetches for it.
struct Weather {
    place: Option<Place>,
    forecast: Option<Fetched>,
    aqi: Option<u32>,
    stale: bool,
    forecasts: Schedule,
    air: Schedule,
}

struct Fetched {
    forecast: Forecast,
    updated_s: u64,
}

impl Weather {
    /// Knows nothing yet, so both fetches are due at once.
    fn at(place: Option<Place>) -> Self {
        Self {
            place,
            forecast: None,
            aqi: None,
            stale: false,
            forecasts: Schedule::new(FORECAST_EVERY_S),
            air: Schedule::new(AIR_EVERY_S),
        }
    }

    /// How long to sleep before a fetch or the forecast going stale may be due, at most
    /// [`WALL_CLOCK_CHECK`]; never without a place.
    fn next_wake(&self, now_s: u64) -> Option<Duration> {
        self.place?;
        let stale_in_s = self
            .stale_at()
            .map_or(u64::MAX, |stale_at| stale_at.saturating_sub(now_s));
        let wait_s = self
            .forecasts
            .wait_s(now_s)
            .min(self.air.wait_s(now_s))
            .min(stale_in_s);
        Some(Duration::from_secs(wait_s).min(WALL_CLOCK_CHECK))
    }

    /// When the forecast goes stale, unless there is none or it already has.
    fn stale_at(&self) -> Option<u64> {
        self.forecast
            .as_ref()
            .filter(|_| !self.stale)
            .map(|fetched| fetched.updated_s + STALE_AFTER_S)
    }

    /// Marks the forecast stale once it is old enough, and returns whether it just became so.
    fn mark_stale(&mut self, now_s: u64) -> bool {
        let is_newly_stale = self.stale_at().is_some_and(|stale_at| stale_at <= now_s);
        if is_newly_stale {
            self.stale = true;
        }
        is_newly_stale
    }

    /// Runs the fetches that are due, and returns whether the snapshot changed. A response that
    /// arrives after `config` has moved to another place is dropped, along with what is left to do.
    async fn fetch_due(
        &mut self,
        fetch: &Arc<dyn Fetch>,
        clock: &WallClock,
        config: &watch::Receiver<ResolvedConfig>,
    ) -> bool {
        let Some(place) = self.place else {
            return false;
        };
        let has_moved = || place_in(&config.borrow()) != Some(place);
        let mut changed = false;
        if self.forecasts.is_due(clock()) {
            let body = get(fetch, forecast_url(place.lat, place.lon)).await;
            if has_moved() {
                return false;
            }
            let now_s = clock();
            let result = body
                .map_err(WeatherError::from)
                .and_then(|body| parse_forecast(&body, now_s));
            if let Some(forecast) = self.forecasts.record("forecast", now_s, result) {
                self.forecast = Some(Fetched {
                    forecast,
                    updated_s: now_s,
                });
                self.stale = false;
                changed = true;
            }
        }
        if self.air.is_due(clock()) {
            let body = get(fetch, air_url(place.lat, place.lon)).await;
            if has_moved() {
                return false;
            }
            let now_s = clock();
            let result = body
                .map_err(WeatherError::from)
                .and_then(|body| parse_air(&body));
            if let Some(aqi) = self.air.record("air quality", now_s, result) {
                self.aqi = aqi;
                changed = true;
            }
        }
        changed
    }

    fn snapshot(&self) -> WeatherSnapshot {
        let Some(Fetched {
            forecast,
            updated_s,
        }) = &self.forecast
        else {
            return WeatherSnapshot::default();
        };
        WeatherSnapshot {
            available: true,
            stale: self.stale,
            updated_s: *updated_s,
            temp_c: forecast.temp_c,
            code: forecast.code,
            condition: forecast.condition.to_owned(),
            is_day: forecast.is_day,
            humidity_pct: forecast.humidity_pct,
            wind_kmh: forecast.wind_kmh,
            high_c: forecast.high_c,
            low_c: forecast.low_c,
            hours: forecast.hours.clone(),
            rain_peak: forecast.rain_peak.clone(),
            aqi: self.aqi,
        }
    }
}

/// When one kind of fetch last ended and how long it waits after that, in wall-clock seconds.
struct Schedule {
    every_s: u64,
    retry_s: u64,
    last: Option<Attempt>,
}

#[derive(Clone, Copy)]
struct Attempt {
    ended_s: u64,
    wait_s: u64,
}

impl Schedule {
    fn new(every_s: u64) -> Self {
        Self {
            every_s,
            retry_s: FIRST_RETRY_S,
            last: None,
        }
    }

    /// Seconds until the next fetch is due, 0 when it is due now.
    fn wait_s(&self, now_s: u64) -> u64 {
        match self.last {
            // A clock set back before the last attempt would otherwise hold the next one back by
            // as much as it moved.
            Some(last) if now_s >= last.ended_s => {
                (last.ended_s + last.wait_s).saturating_sub(now_s)
            }
            _ => 0,
        }
    }

    fn is_due(&self, now_s: u64) -> bool {
        self.wait_s(now_s) == 0
    }

    /// Schedules the next fetch after `result`, which ended at `now_s`, and returns its value if
    /// it has one.
    fn record<T>(&mut self, what: &str, now_s: u64, result: Result<T, WeatherError>) -> Option<T> {
        match result {
            Ok(value) => {
                self.last = Some(Attempt {
                    ended_s: now_s,
                    wait_s: self.every_s,
                });
                self.retry_s = FIRST_RETRY_S;
                Some(value)
            }
            Err(error) => {
                warn!(
                    %error,
                    "no {what} from Open-Meteo; retrying in {} s",
                    self.retry_s
                );
                self.last = Some(Attempt {
                    ended_s: now_s,
                    wait_s: self.retry_s,
                });
                self.retry_s = (self.retry_s * 2).min(self.every_s);
                None
            }
        }
    }
}

/// GETs `url` on a blocking thread.
async fn get(fetch: &Arc<dyn Fetch>, url: String) -> Result<String, FetchError> {
    let fetch = Arc::clone(fetch);
    spawn_blocking(move || fetch.get(&url))
        .await
        .expect("a fetch never panics")
}
