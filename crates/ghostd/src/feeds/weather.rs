//! The `weather` topic: Open-Meteo's current weather, today's high and low, the coming hours and
//! the US AQI at `location.{lat, lon}`.
//!
//! While some client wants the topic, the forecast is fetched every 15 min and air quality every
//! 30 min (spec §12). A failed fetch is retried after 60 s, then after twice as long each time, up
//! to its cadence. The last good values stay published, marked stale once the forecast is two
//! forecast cadences old.

use std::cmp::Reverse;
use std::future::pending;
use std::sync::Arc;
use std::time::Duration;

use ghost_proto::{RainPeak, ResolvedConfig, WeatherHour, WeatherSnapshot, topics};
use serde::Deserialize;
use tokio::task::spawn_blocking;
use tokio::time::{Instant, sleep_until};
use tracing::warn;

use super::http::{Fetch, FetchError};
use super::{unix_now_s, wanted};
use crate::server::{Feed, FeedContext};

const FORECAST_EVERY: Duration = Duration::from_mins(15);
const AIR_EVERY: Duration = Duration::from_mins(30);
const FIRST_RETRY: Duration = Duration::from_secs(60);
const STALE_AFTER: Duration = FORECAST_EVERY.saturating_mul(2);
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

#[derive(Deserialize)]
struct Hourly {
    time: Vec<u64>,
    temperature_2m: Vec<f32>,
    precipitation_probability: Vec<u8>,
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
    us_aqi: u32,
}

/// Reads a response to [`forecast_url`] as of `now_s`, in seconds since the Unix epoch.
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
        .map(|((&at_s, &temp_c), &rain_pct)| WeatherHour {
            at_s,
            temp_c,
            rain_pct,
        });
    let window = now_s..=now_s + RAIN_PEAK_WINDOW_S;
    let rain_peak = entries
        .clone()
        .filter(|entry| window.contains(&entry.at_s))
        .max_by_key(|entry| (entry.rain_pct, Reverse(entry.at_s)))
        .filter(|entry| entry.rain_pct > 0)
        .map(|entry| RainPeak {
            pct: entry.rain_pct,
            at_s: entry.at_s,
        });
    Ok(Forecast {
        temp_c: current.temperature_2m,
        code: current.weather_code,
        condition: condition(current.weather_code),
        is_day: current.is_day != 0,
        humidity_pct: current.relative_humidity_2m,
        wind_kmh: current.wind_speed_10m,
        high_c,
        low_c,
        hours: entries
            .filter(|entry| entry.at_s > now_s)
            .take(HOURS_SHOWN)
            .collect(),
        rain_peak,
    })
}

/// Reads the US AQI from a response to [`air_url`].
pub fn parse_air(json: &str) -> Result<u32, WeatherError> {
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
    Feed::new(topics::WEATHER, move |context| run(fetch, context))
}

async fn run(fetch: Arc<dyn Fetch>, mut context: FeedContext) {
    let mut weather = Weather::at(place_in(&context.config.borrow_and_update()));
    context
        .publisher
        .publish(topics::WEATHER, &weather.snapshot());
    loop {
        let wake = weather
            .next_wake()
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
            () = sleep_until_or_forever(wake) => {
                if weather.wake(&fetch).await {
                    context.publisher.publish(topics::WEATHER, &weather.snapshot());
                }
            }
        }
    }
}

async fn sleep_until_or_forever(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => pending().await,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Place {
    lat: f64,
    lon: f64,
}

fn place_in(config: &ResolvedConfig) -> Option<Place> {
    Some(Place {
        lat: config.location.lat?,
        lon: config.location.lon?,
    })
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
    at: Instant,
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
            forecasts: Schedule::due_now(FORECAST_EVERY),
            air: Schedule::due_now(AIR_EVERY),
        }
    }

    /// When a fetch or the forecast going stale is next due; never without a place.
    fn next_wake(&self) -> Option<Instant> {
        self.place?;
        let fetch = self.forecasts.due.min(self.air.due);
        let stale_at = self.stale_at().unwrap_or(fetch);
        Some(fetch.min(stale_at))
    }

    /// When the forecast goes stale, unless there is none or it already has.
    fn stale_at(&self) -> Option<Instant> {
        self.forecast
            .as_ref()
            .filter(|_| !self.stale)
            .map(|fetched| fetched.at + STALE_AFTER)
    }

    /// Does whatever is due, and returns whether the snapshot changed.
    async fn wake(&mut self, fetch: &Arc<dyn Fetch>) -> bool {
        let Some(place) = self.place else {
            return false;
        };
        let mut changed = false;
        if self.forecasts.is_due() {
            let url = forecast_url(place.lat, place.lon);
            let result = fetch_and_parse(fetch, url, parse_forecast_now).await;
            if let Some((forecast, updated_s)) = self.forecasts.record("forecast", result) {
                self.forecast = Some(Fetched {
                    forecast,
                    at: Instant::now(),
                    updated_s,
                });
                self.stale = false;
                changed = true;
            }
        }
        if self.air.is_due() {
            let result = fetch_and_parse(fetch, air_url(place.lat, place.lon), parse_air).await;
            if let Some(aqi) = self.air.record("air quality", result) {
                self.aqi = Some(aqi);
                changed = true;
            }
        }
        if self
            .stale_at()
            .is_some_and(|stale_at| stale_at <= Instant::now())
        {
            self.stale = true;
            changed = true;
        }
        changed
    }

    fn snapshot(&self) -> WeatherSnapshot {
        let Some(Fetched {
            forecast,
            updated_s,
            ..
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

/// When one kind of fetch is next due, and how long it waits after its next failure.
struct Schedule {
    every: Duration,
    due: Instant,
    retry: Duration,
}

impl Schedule {
    fn due_now(every: Duration) -> Self {
        Self {
            every,
            due: Instant::now(),
            retry: FIRST_RETRY,
        }
    }

    fn is_due(&self) -> bool {
        self.due <= Instant::now()
    }

    /// Schedules the next fetch after `result`, and returns its value if it has one.
    fn record<T>(&mut self, what: &str, result: Result<T, WeatherError>) -> Option<T> {
        match result {
            Ok(value) => {
                self.due = Instant::now() + self.every;
                self.retry = FIRST_RETRY;
                Some(value)
            }
            Err(error) => {
                warn!(
                    %error,
                    "no {what} from Open-Meteo; retrying in {} s",
                    self.retry.as_secs()
                );
                self.due = Instant::now() + self.retry;
                self.retry = (self.retry * 2).min(self.every);
                None
            }
        }
    }
}

/// Fetches `url` and reads the body with `parse`, both on a blocking thread.
async fn fetch_and_parse<T: Send + 'static>(
    fetch: &Arc<dyn Fetch>,
    url: String,
    parse: fn(&str) -> Result<T, WeatherError>,
) -> Result<T, WeatherError> {
    let fetch = Arc::clone(fetch);
    spawn_blocking(move || parse(&fetch.get(&url)?))
        .await
        .expect("fetching and parsing never panic")
}

/// Reads a forecast as of now, and returns that time too.
fn parse_forecast_now(json: &str) -> Result<(Forecast, u64), WeatherError> {
    let now_s = unix_now_s();
    Ok((parse_forecast(json, now_s)?, now_s))
}
