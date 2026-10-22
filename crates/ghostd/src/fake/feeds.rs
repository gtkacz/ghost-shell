use std::future::pending;
use std::time::Duration;

use ghost_proto::{
    AlertLevel, HostSnapshot, RainPeak, SkySnapshot, SpectrumSnapshot, WeatherAlert, WeatherAlerts,
    WeatherHour, WeatherSnapshot, topics,
};
use tokio::time::{MissedTickBehavior, interval};

use super::Wave;
use crate::feeds::spectrum::{BANDS, FRAMES_PER_S};
use crate::feeds::weather::condition;
use crate::feeds::{unix_now_s, until_unwanted, until_wanted};
use crate::server::Feed;

/// The fake feeds, one per feed topic, plus `weather.alerts` in place of the alerts provider.
pub fn feeds() -> Vec<Feed> {
    vec![host(), weather(), sky(), spectrum(), alerts()]
}

const HOSTNAME: &str = "tkacz-workstation";
const KERNEL: &str = "7.2.11-200.fc44.x86_64";
// The mockup's uptime of 2:14:07.
const UPTIME_S: u64 = 8047;

fn host() -> Feed {
    Feed::new(topics::HOST, |context| async move {
        context.publisher.publish(
            topics::HOST,
            &HostSnapshot {
                hostname: HOSTNAME.to_owned(),
                kernel: KERNEL.to_owned(),
                boot_time_s: unix_now_s().saturating_sub(UPTIME_S),
            },
        );
        pending::<()>().await;
    })
}

const WEATHER_TEMP_C: f32 = 18.0;
const WEATHER_CODE: u8 = 95;
const WEATHER_HUMIDITY_PCT: u8 = 88;
const WEATHER_WIND_KMH: f32 = 31.0;
const WEATHER_HIGH_C: f32 = 22.0;
const WEATHER_LOW_C: f32 = 14.0;
const WEATHER_AQI: u32 = 42;
/// The temperature and chance of rain for each of the next hours.
const WEATHER_HOURS: [(f32, u8); 6] = [
    (18.0, 80),
    (17.0, 95),
    (17.0, 70),
    (16.0, 40),
    (15.0, 20),
    (15.0, 10),
];
const RAIN_PEAK_HOUR: usize = 1;
const HOUR_S: u64 = 60 * 60;

/// The mockup's weather, published once.
fn weather() -> Feed {
    Feed::new(topics::WEATHER, |context| async move {
        context
            .publisher
            .publish(topics::WEATHER, &mockup_weather(unix_now_s()));
        pending::<()>().await;
    })
}

fn mockup_weather(now_s: u64) -> WeatherSnapshot {
    let next_hour_s = (now_s / HOUR_S + 1) * HOUR_S;
    let hours: Vec<WeatherHour> = WEATHER_HOURS
        .iter()
        .zip(0..)
        .map(|(&(temp_c, rain_pct), index)| WeatherHour {
            at_s: next_hour_s + index * HOUR_S,
            temp_c,
            rain_pct,
        })
        .collect();
    let peak = &hours[RAIN_PEAK_HOUR];
    WeatherSnapshot {
        available: true,
        stale: false,
        updated_s: now_s,
        temp_c: WEATHER_TEMP_C,
        code: WEATHER_CODE,
        condition: condition(WEATHER_CODE).to_owned(),
        // The mockup's rain peaks at 22H, two hours ahead, so it is evening there.
        is_day: false,
        humidity_pct: WEATHER_HUMIDITY_PCT,
        wind_kmh: WEATHER_WIND_KMH,
        high_c: WEATHER_HIGH_C,
        low_c: WEATHER_LOW_C,
        rain_peak: Some(RainPeak {
            pct: peak.rain_pct,
            at_s: peak.at_s,
        }),
        hours,
        aqi: Some(WEATHER_AQI),
    }
}

const ALERT_SOURCE: &str = "INMET";
const ALERT_ID: &str = "1";
const ALERT_TITLE: &str = "TEMPESTADE";
const ALERT_SEVERITY: &str = "PERIGO POTENCIAL";
const ALERT_RISK: &str = "Chuva entre 20 e 30 mm/h ou até 50 mm/dia, ventos intensos (40-60 km/h). \
                          Baixo risco de corte de energia elétrica, queda de galhos de árvores, \
                          alagamentos e de descargas elétricas.";
const ALERT_STARTS_IN_S: u64 = HOUR_S;
const ALERT_ENDS_IN_S: u64 = 12 * HOUR_S;

/// The mockup's amber storm warning, from an hour to twelve hours from now, published once.
fn alerts() -> Feed {
    Feed::new(topics::WEATHER_ALERTS, |context| async move {
        context
            .publisher
            .publish(topics::WEATHER_ALERTS, &mockup_alerts(unix_now_s()));
        pending::<()>().await;
    })
}

fn mockup_alerts(now_s: u64) -> WeatherAlerts {
    WeatherAlerts {
        source: ALERT_SOURCE.to_owned(),
        updated_s: now_s,
        alerts: vec![WeatherAlert {
            id: ALERT_ID.to_owned(),
            title: ALERT_TITLE.to_owned(),
            severity: ALERT_SEVERITY.to_owned(),
            original_title: None,
            original_severity: None,
            auto_translated: false,
            level: AlertLevel::Warn,
            start_s: now_s + ALERT_STARTS_IN_S,
            end_s: now_s + ALERT_ENDS_IN_S,
            risks: vec![ALERT_RISK.to_owned()],
        }],
    }
}

/// 05:17 and 18:52 in UTC-3; the shell shows only the time of day.
const SKY_SUNRISE_S: u64 = 1_792_570_620;
const SKY_SUNSET_S: u64 = 1_792_619_520;
const SKY_MOON_PHASE: f32 = 0.295;
const SKY_MOON_ILLUMINATION: f32 = 0.64;
const SKY_MOON_NAME: &str = "WAXING GIBBOUS";

/// The mockup's sky, published once.
fn sky() -> Feed {
    Feed::new(topics::SKY, |context| async move {
        context.publisher.publish(
            topics::SKY,
            &SkySnapshot {
                available: true,
                sunrise_s: Some(SKY_SUNRISE_S),
                sunset_s: Some(SKY_SUNSET_S),
                moon_phase: SKY_MOON_PHASE,
                moon_illumination: SKY_MOON_ILLUMINATION,
                moon_name: SKY_MOON_NAME.to_owned(),
            },
        );
        pending::<()>().await;
    })
}

const SPECTRUM_LEVEL: Wave = Wave::new(0.0, 255.0, 45.0);
// Each band runs the wave a little apart from the one below it, so the bands ripple instead of
// moving together.
const SPECTRUM_PHASE_STEP: f64 = 1.0 / BANDS as f64;

/// Sine-shaped bands 30 times a second, but only while some client wants them, and one dark frame
/// on release, as the real feed does.
fn spectrum() -> Feed {
    Feed::new(topics::SPECTRUM, |mut context| async move {
        let mut frame = 0;
        loop {
            until_wanted(&mut context.demand, topics::SPECTRUM).await;
            let mut frames = interval(Duration::from_secs(1) / FRAMES_PER_S);
            frames.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    () = until_unwanted(&mut context.demand, topics::SPECTRUM) => break,
                    _ = frames.tick() => {
                        frame += 1;
                        context.publisher.publish(topics::SPECTRUM, &fake_spectrum(frame));
                    }
                }
            }
            let dark = SpectrumSnapshot {
                bands: vec![0; BANDS],
            };
            context.publisher.publish(topics::SPECTRUM, &dark);
        }
    })
}

fn fake_spectrum(frame: u64) -> SpectrumSnapshot {
    SpectrumSnapshot {
        bands: (0..BANDS)
            .map(|band| {
                SPECTRUM_LEVEL
                    .shifted(band as f64 * SPECTRUM_PHASE_STEP)
                    .at(frame) as u8
            })
            .collect(),
    }
}
