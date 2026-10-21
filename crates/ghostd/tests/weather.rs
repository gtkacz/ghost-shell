use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ghost_proto::{Location, RainPeak, ResolvedConfig, WeatherSnapshot, topics};
use ghostd::feeds::http::{Fetch, FetchError};
use ghostd::feeds::weather::{
    air_url, condition, feed_with_clock, forecast_url, parse_air, parse_forecast,
};
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep, sleep_until, timeout, timeout_at};

const FORECAST: &str = include_str!("fixtures/open-meteo-forecast.json");
/// The forecast fixture with nulls: hour 2's chance of rain, and hours 4 and 9's temperatures.
const FORECAST_NULLS: &str = include_str!("fixtures/open-meteo-forecast-nulls.json");
const AIR: &str = include_str!("fixtures/open-meteo-air.json");
const AIR_NULL: &str = include_str!("fixtures/open-meteo-air-null.json");
const AIR_HOST: &str = "https://air-quality-api.open-meteo.com/";
/// The fixture's first hourly entry, 2026-10-21T19:00Z, which is also its current reading.
const FIRST_HOUR_S: u64 = 1_792_609_200;
const HOUR_S: u64 = 3600;
/// 19:13:20Z, between the fixture's first and second hourly entries.
const NOW_S: u64 = 1_792_610_000;
const SAO_PAULO: (f64, f64) = (-23.55, -46.63);
const RIO: (f64, f64) = (-22.91, -43.17);
const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(3600);
const DAY: Duration = Duration::from_secs(24 * 3600);

/// The start of the fixture's hourly entry `index`.
fn hour(index: u64) -> u64 {
    FIRST_HOUR_S + index * HOUR_S
}

#[test]
fn urls_are_exact() {
    assert_eq!(
        forecast_url(-23.55, -46.63),
        "https://api.open-meteo.com/v1/forecast?latitude=-23.55&longitude=-46.63\
         &current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m,is_day\
         &hourly=temperature_2m,precipitation_probability\
         &daily=temperature_2m_max,temperature_2m_min\
         &timezone=auto&timeformat=unixtime&forecast_days=2"
    );
    assert_eq!(
        air_url(-23.55, -46.63),
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude=-23.55&longitude=-46.63\
         &current=us_aqi&timeformat=unixtime"
    );
}

#[test]
fn parses_current_today_and_six_hours() {
    let forecast = parse_forecast(FORECAST, NOW_S).unwrap();

    assert_eq!(forecast.temp_c, 18.0);
    assert_eq!(forecast.code, 95);
    assert_eq!(forecast.condition, "THUNDERSTORM");
    assert!(forecast.is_day);
    assert_eq!(forecast.humidity_pct, 88);
    assert_eq!(forecast.wind_kmh, 31.0);
    assert_eq!((forecast.high_c, forecast.low_c), (22.0, 14.0));
    assert_eq!(forecast.hours[0].at_s, 1_792_612_800);
    let hours: Vec<(u64, f32, u8)> = forecast
        .hours
        .iter()
        .map(|entry| (entry.at_s, entry.temp_c, entry.rain_pct))
        .collect();
    assert_eq!(
        hours,
        [
            (hour(1), 18.0, 80),
            (hour(2), 17.0, 95),
            (hour(3), 17.0, 70),
            (hour(4), 16.0, 40),
            (hour(5), 15.0, 20),
            (hour(6), 15.0, 10),
        ]
    );
    // On the hour, the next full hour is the one after.
    let on_the_hour = parse_forecast(FORECAST, hour(1)).unwrap();
    assert_eq!(on_the_hour.hours[0].at_s, hour(2));
}

#[test]
fn rain_peak_is_highest_then_earliest_within_twelve_hours() {
    let peak = |now_s| parse_forecast(FORECAST, now_s).unwrap().rain_peak;

    // Hours 2 and 9 tie at 95, and the 100s at hours 0 and 13 fall just outside the window.
    assert_eq!(
        peak(NOW_S),
        Some(RainPeak {
            pct: 95,
            at_s: hour(2)
        })
    );
    // Both ends of the window count.
    assert_eq!(
        peak(hour(0)),
        Some(RainPeak {
            pct: 100,
            at_s: hour(0)
        })
    );
    assert_eq!(
        peak(hour(1)),
        Some(RainPeak {
            pct: 100,
            at_s: hour(13)
        })
    );
}

#[test]
fn rain_peak_is_none_when_dry() {
    let mut dry: Value = serde_json::from_str(FORECAST).unwrap();
    for chance in dry["hourly"]["precipitation_probability"]
        .as_array_mut()
        .unwrap()
    {
        *chance = 0.into();
    }

    let forecast = parse_forecast(&dry.to_string(), NOW_S).unwrap();

    assert_eq!(forecast.rain_peak, None);
}

#[test]
fn wmo_codes_map_to_labels() {
    let table = [
        (0, "CLEAR"),
        (1, "MAINLY CLEAR"),
        (2, "PARTLY CLOUDY"),
        (3, "OVERCAST"),
        (45, "FOG"),
        (48, "RIME FOG"),
        (51, "DRIZZLE"),
        (53, "DRIZZLE"),
        (55, "DRIZZLE"),
        (56, "FREEZING DRIZZLE"),
        (57, "FREEZING DRIZZLE"),
        (61, "LIGHT RAIN"),
        (63, "RAIN"),
        (65, "HEAVY RAIN"),
        (66, "FREEZING RAIN"),
        (67, "FREEZING RAIN"),
        (71, "LIGHT SNOW"),
        (73, "SNOW"),
        (75, "HEAVY SNOW"),
        (77, "SNOW GRAINS"),
        (80, "SHOWERS"),
        (81, "SHOWERS"),
        (82, "VIOLENT SHOWERS"),
        (85, "SNOW SHOWERS"),
        (86, "SNOW SHOWERS"),
        (95, "THUNDERSTORM"),
        (96, "THUNDERSTORM WITH HAIL"),
        (99, "THUNDERSTORM WITH HAIL"),
    ];
    for (code, label) in table {
        assert_eq!(condition(code), label, "code {code}");
    }
    for unlisted in [4, 42, 50, 100, u8::MAX] {
        assert_eq!(condition(unlisted), "UNKNOWN", "code {unlisted}");
    }
}

#[test]
fn hours_and_the_rain_peak_leave_out_null_entries() {
    let forecast = parse_forecast(FORECAST_NULLS, NOW_S).unwrap();

    let hours: Vec<u64> = forecast.hours.iter().map(|entry| entry.at_s).collect();
    assert_eq!(hours, [1, 3, 5, 6, 7, 8].map(hour));
    // Hour 2, the earlier 95 in the plain fixture, has no chance of rain here; hour 9 still counts
    // without its temperature.
    assert_eq!(
        forecast.rain_peak,
        Some(RainPeak {
            pct: 95,
            at_s: hour(9)
        })
    );
}

#[test]
fn parses_us_aqi() {
    assert_eq!(parse_air(AIR).unwrap(), Some(42));
}

#[test]
fn a_null_us_aqi_is_none() {
    assert_eq!(parse_air(AIR_NULL).unwrap(), None);
}

type Hook = Box<dyn FnOnce() + Send>;

/// Answers with the fixtures, or fails while told to, and notes when each URL was asked for.
#[derive(Default)]
struct FakeOpenMeteo {
    requests: Mutex<Vec<(Instant, String)>>,
    failing: AtomicBool,
    null_aqi: AtomicBool,
    during_next_request: Mutex<Option<Hook>>,
}

impl FakeOpenMeteo {
    fn fail(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }

    fn serve_null_aqi(&self) {
        self.null_aqi.store(true, Ordering::SeqCst);
    }

    /// Runs `hook` while the next request is in flight.
    fn during_next_request(&self, hook: impl FnOnce() + Send + 'static) {
        *self.during_next_request.lock().unwrap() = Some(Box::new(hook));
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    /// How long after `start` each forecast (`air` false) or air quality request came.
    fn offsets(&self, start: Instant, air: bool) -> Vec<Duration> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, url)| url.starts_with(AIR_HOST) == air)
            .map(|(at, _)| at.duration_since(start))
            .collect()
    }

    fn urls_since(&self, start: Instant) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| *at >= start)
            .map(|(_, url)| url.clone())
            .collect()
    }
}

impl Fetch for FakeOpenMeteo {
    fn get(&self, url: &str) -> Result<String, FetchError> {
        self.requests
            .lock()
            .unwrap()
            .push((Instant::now(), url.to_owned()));
        let hook = self.during_next_request.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        if self.failing.load(Ordering::SeqCst) {
            return Err(ureq::Error::ConnectionFailed.into());
        }
        let body = match (
            url.starts_with(AIR_HOST),
            self.null_aqi.load(Ordering::SeqCst),
        ) {
            (false, _) => FORECAST,
            (true, false) => AIR,
            (true, true) => AIR_NULL,
        };
        Ok(body.to_owned())
    }
}

/// Wall-clock seconds that follow the paused tokio clock from `NOW_S`, moved on or back by however
/// much the wall clock was jumped on its own.
struct FakeWallClock {
    start: Instant,
    jumped_s: AtomicI64,
}

impl FakeWallClock {
    fn now_s(&self) -> u64 {
        let followed_s = NOW_S + self.start.elapsed().as_secs();
        followed_s.wrapping_add_signed(self.jumped_s.load(Ordering::SeqCst))
    }

    /// A suspend: the wall clock moves on by `duration` while the monotonic clock stands still.
    fn suspend_for(&self, duration: Duration) {
        self.jump_s(duration.as_secs().try_into().unwrap());
    }

    fn set_back(&self, duration: Duration) {
        self.jump_s(-i64::try_from(duration.as_secs()).unwrap());
    }

    fn jump_s(&self, seconds: i64) {
        self.jumped_s.fetch_add(seconds, Ordering::SeqCst);
    }
}

fn config_at(place: Option<(f64, f64)>) -> ResolvedConfig {
    ResolvedConfig {
        location: Location {
            lat: place.map(|(lat, _)| lat),
            lon: place.map(|(_, lon)| lon),
            ..Location::default()
        },
        ..ResolvedConfig::default()
    }
}

/// The weather feed on its own, with its demand, config and output in the test's hands.
struct Running {
    open_meteo: Arc<FakeOpenMeteo>,
    clock: Arc<FakeWallClock>,
    demand: watch::Sender<BTreeSet<String>>,
    config: Arc<watch::Sender<ResolvedConfig>>,
    published: mpsc::UnboundedReceiver<WeatherSnapshot>,
}

impl Running {
    fn start(place: Option<(f64, f64)>) -> Self {
        let open_meteo = Arc::new(FakeOpenMeteo::default());
        let clock = Arc::new(FakeWallClock {
            start: Instant::now(),
            jumped_s: AtomicI64::new(0),
        });
        let (demand, demand_rx) = watch::channel(BTreeSet::new());
        let (config, config_rx) = watch::channel(config_at(place));
        let (sent, published) = mpsc::unbounded_channel();
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                assert_eq!(topic, topics::WEATHER);
                sent.send(serde_json::from_value(data).unwrap()).unwrap();
            }),
            demand: demand_rx,
            config: config_rx,
        };
        let wall_clock = Arc::clone(&clock);
        let weather = feed_with_clock(open_meteo.clone(), Arc::new(move || wall_clock.now_s()));
        tokio::spawn(weather.run(context));
        Self {
            open_meteo,
            clock,
            demand,
            config: Arc::new(config),
            published,
        }
    }

    fn want(&self, wanted: bool) {
        let topics = if wanted {
            BTreeSet::from([topics::WEATHER.to_owned()])
        } else {
            BTreeSet::new()
        };
        self.demand.send_replace(topics);
    }

    fn move_to(&self, place: Option<(f64, f64)>) {
        self.config.send_replace(config_at(place));
    }

    async fn next(&mut self) -> WeatherSnapshot {
        // The paused clock would otherwise run on forever behind a feed that never publishes.
        timeout(DAY, self.published.recv())
            .await
            .expect("nothing was published for a day")
            .expect("the feed stopped")
    }

    async fn expect_nothing_before(&mut self, deadline: Instant) {
        if let Ok(published) = timeout_at(deadline, self.published.recv()).await {
            panic!("published {published:?} before the deadline");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn fetches_only_while_wanted_and_on_cadence() {
    let mut weather = Running::start(Some(SAO_PAULO));
    assert!(!weather.next().await.available);

    sleep(HOUR).await;
    assert_eq!(weather.open_meteo.request_count(), 0);

    let wanted_at = Instant::now();
    weather.want(true);
    let fetched = weather.next().await;
    assert!(fetched.available);
    assert_eq!(fetched.aqi, Some(42));

    sleep_until(wanted_at + 30 * MINUTE + SECOND).await;
    let open_meteo = &weather.open_meteo;
    assert_eq!(
        open_meteo.offsets(wanted_at, false),
        [Duration::ZERO, 15 * MINUTE, 30 * MINUTE]
    );
    assert_eq!(
        open_meteo.offsets(wanted_at, true),
        [Duration::ZERO, 30 * MINUTE]
    );

    weather.want(false);
    sleep(HOUR).await;
    assert_eq!(weather.open_meteo.request_count(), 5);
}

#[tokio::test(start_paused = true)]
async fn failure_keeps_the_last_value_and_goes_stale_after_two_cadences() {
    let mut weather = Running::start(Some(SAO_PAULO));
    weather.next().await;
    let wanted_at = Instant::now();
    weather.want(true);
    let good = weather.next().await;
    weather.open_meteo.fail(true);

    weather
        .expect_nothing_before(wanted_at + 30 * MINUTE - SECOND)
        .await;
    let stale = weather.next().await;

    assert_eq!(wanted_at.elapsed(), 30 * MINUTE);
    assert_eq!(
        stale,
        WeatherSnapshot {
            stale: true,
            ..good.clone()
        }
    );
    // Retries wait 60 s and double up to the cadence: 15 min for the forecast, 30 for the air.
    weather.expect_nothing_before(wanted_at + 50 * MINUTE).await;
    let minutes = |list: &[u32]| -> Vec<Duration> { list.iter().map(|&m| m * MINUTE).collect() };
    assert_eq!(
        weather.open_meteo.offsets(wanted_at, false),
        minutes(&[0, 15, 16, 18, 22, 30, 45])
    );
    assert_eq!(
        weather.open_meteo.offsets(wanted_at, true),
        minutes(&[0, 30, 31, 33, 37, 45])
    );

    weather.open_meteo.fail(false);
    let fresh = weather.next().await;
    assert_eq!(wanted_at.elapsed(), 60 * MINUTE);
    assert!(fresh.available && !fresh.stale);
    assert_eq!(fresh.aqi, Some(42));
}

#[tokio::test(start_paused = true)]
async fn no_location_is_unavailable_without_fetching() {
    let mut weather = Running::start(None);
    weather.want(true);

    assert_eq!(weather.next().await, WeatherSnapshot::default());
    weather.expect_nothing_before(Instant::now() + HOUR).await;
    assert_eq!(weather.open_meteo.request_count(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_location_change_refetches_at_once() {
    let mut weather = Running::start(Some(SAO_PAULO));
    weather.next().await;
    weather.want(true);
    assert!(weather.next().await.available);
    sleep(5 * MINUTE).await;

    let moved_at = Instant::now();
    weather.move_to(Some(RIO));

    // The old place's weather is dropped at once rather than shown for the new one.
    assert_eq!(weather.next().await, WeatherSnapshot::default());
    assert!(weather.next().await.available);
    assert_eq!(moved_at.elapsed(), Duration::ZERO);
    assert_eq!(
        weather.open_meteo.urls_since(moved_at),
        [forecast_url(RIO.0, RIO.1), air_url(RIO.0, RIO.1)]
    );

    weather.move_to(None);
    assert_eq!(weather.next().await, WeatherSnapshot::default());
    let requests = weather.open_meteo.request_count();
    weather.expect_nothing_before(Instant::now() + HOUR).await;
    assert_eq!(weather.open_meteo.request_count(), requests);
}

#[tokio::test(start_paused = true)]
async fn a_response_for_the_place_left_mid_fetch_is_dropped() {
    let mut weather = Running::start(Some(SAO_PAULO));
    weather.next().await;
    let config = Arc::clone(&weather.config);
    weather.open_meteo.during_next_request(move || {
        config.send_replace(config_at(Some(RIO)));
    });
    let wanted_at = Instant::now();
    weather.want(true);

    assert_eq!(weather.next().await, WeatherSnapshot::default());
    assert!(weather.next().await.available);
    assert_eq!(
        weather.open_meteo.urls_since(wanted_at),
        [
            forecast_url(SAO_PAULO.0, SAO_PAULO.1),
            forecast_url(RIO.0, RIO.1),
            air_url(RIO.0, RIO.1),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn after_a_suspend_the_next_wake_marks_stale_and_fetches_again() {
    let mut weather = Running::start(Some(SAO_PAULO));
    weather.next().await;
    let wanted_at = Instant::now();
    weather.want(true);
    let good = weather.next().await;

    weather.clock.suspend_for(8 * HOUR);
    let stale = weather.next().await;

    let noticed_after = wanted_at.elapsed();
    assert!(noticed_after <= 61 * SECOND, "{noticed_after:?}");
    assert_eq!(
        stale,
        WeatherSnapshot {
            stale: true,
            ..good.clone()
        }
    );
    let fresh = weather.next().await;
    assert!(fresh.available && !fresh.stale);
    assert!(fresh.updated_s >= good.updated_s + 8 * HOUR_S);
    let fetched_again = wanted_at.elapsed();
    assert_eq!(
        weather.open_meteo.offsets(wanted_at, false),
        [Duration::ZERO, fetched_again]
    );
    assert_eq!(
        weather.open_meteo.offsets(wanted_at, true),
        [Duration::ZERO, fetched_again]
    );
}

#[tokio::test(start_paused = true)]
async fn a_clock_set_back_does_not_hold_the_next_fetch_back() {
    let mut weather = Running::start(Some(SAO_PAULO));
    weather.next().await;
    let wanted_at = Instant::now();
    weather.want(true);
    weather.next().await;

    weather.clock.set_back(HOUR);
    assert!(weather.next().await.available);

    let fetched_again = wanted_at.elapsed();
    assert!(fetched_again <= 61 * SECOND, "{fetched_again:?}");
    assert_eq!(
        weather.open_meteo.offsets(wanted_at, false),
        [Duration::ZERO, fetched_again]
    );
}

#[tokio::test(start_paused = true)]
async fn a_null_aqi_is_published_as_none_without_retrying() {
    let mut weather = Running::start(Some(SAO_PAULO));
    weather.next().await;
    weather.open_meteo.serve_null_aqi();
    let wanted_at = Instant::now();
    weather.want(true);

    let fetched = weather.next().await;
    assert!(fetched.available);
    assert_eq!(fetched.aqi, None);
    weather
        .expect_nothing_before(wanted_at + 15 * MINUTE - SECOND)
        .await;
    assert_eq!(
        weather.open_meteo.offsets(wanted_at, true),
        [Duration::ZERO]
    );
}
