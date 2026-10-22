use std::future::pending;
use std::time::Duration;

use ghost_proto::{
    AlertLevel, Batteries, Battery, BatteryKind, DiskHealth, DriveHealth, Endpoint, HostSnapshot,
    LanLink, LinkState, LinksSnapshot, RainPeak, SkySnapshot, SpectrumSnapshot, TailscaleLink,
    UplinkSnapshot, VpnLink, WeatherAlert, WeatherAlerts, WeatherHour, WeatherSnapshot, topics,
};
use tokio::time::{MissedTickBehavior, interval};

use super::Wave;
use super::sampled::DRIVES;
use crate::feeds::spectrum::{BANDS, FRAMES_PER_S};
use crate::feeds::weather::condition;
use crate::feeds::{unix_now_s, until_unwanted, until_wanted};
use crate::server::Feed;

/// The fake feeds, one per feed topic, plus `weather.alerts` in place of the alerts provider.
pub fn feeds() -> Vec<Feed> {
    vec![
        host(),
        weather(),
        sky(),
        spectrum(),
        alerts(),
        links(),
        disks_health(),
        batteries(),
        uplink(),
    ]
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

const LAN_IFACE: &str = "enp14s0";
const LAN_ADDRESS: &str = "192.168.15.5/24";
const LAN_SPEED_MBPS: u32 = 1000;
const VPN_NAME: &str = "OpenVPN Flxpoint";
const VPN_KIND: &str = "openvpn";
const VPN_IFACE: &str = "tun0";
const TAILNET_ADDRESS: &str = "100.122.159.93";
const TAILNET_PEERS_ONLINE: u32 = 0;
const TAILNET_PEERS: u32 = 1;
const PING_MS: f32 = 12.0;

/// The mockup's network links, published once.
fn links() -> Feed {
    Feed::new(topics::NET_LINKS, |context| async move {
        context
            .publisher
            .publish(topics::NET_LINKS, &mockup_links());
        pending::<()>().await;
    })
}

fn mockup_links() -> LinksSnapshot {
    LinksSnapshot {
        lan: vec![LanLink {
            iface: LAN_IFACE.to_owned(),
            address: Some(LAN_ADDRESS.to_owned()),
            speed_mbps: Some(LAN_SPEED_MBPS),
            state: LinkState::Up,
        }],
        vpns: vec![VpnLink {
            name: VPN_NAME.to_owned(),
            kind: VPN_KIND.to_owned(),
            iface: Some(VPN_IFACE.to_owned()),
            state: LinkState::Up,
        }],
        tailscale: Some(TailscaleLink {
            address: Some(TAILNET_ADDRESS.to_owned()),
            peers_online: TAILNET_PEERS_ONLINE,
            peers_total: TAILNET_PEERS,
            state: LinkState::Up,
        }),
        ping_ms: Some(PING_MS),
    }
}

/// Wear and power-on hours of each fake drive, in [`DRIVES`] order, as fx/udisks2-drives.json read
/// the live ones. UDisks2 still reports the dead WD from an older cache (fx C5), so the shell must
/// leave it out itself; ATA drives report no wear.
const DRIVE_HEALTH: [(Option<u8>, Option<u32>); DRIVES.len()] = [
    (Some(1), Some(2_908)),
    (Some(37), Some(17_773)),
    (Some(1), Some(12_354)),
    (None, Some(33_429)),
    (None, Some(24_408)),
];

/// The drives' health, published once.
fn disks_health() -> Feed {
    Feed::new(topics::DISKS_HEALTH, |context| async move {
        context
            .publisher
            .publish(topics::DISKS_HEALTH, &fake_disk_health());
        pending::<()>().await;
    })
}

const CONTROLLER_ID: &str = "ps-controller-battery-aa:bb:cc:dd:ee:ff";
const CONTROLLER_LABEL: &str = "DUALSENSE";
const CONTROLLER_PCT: f32 = 80.0;
const PHONE_ID: &str = "kdeconnect:0123456789abcdef0123456789abcdef";
const PHONE_LABEL: &str = "PHONE";
// Under the default battery warning, so the fake shows a warn row.
const PHONE_PCT: f32 = 15.0;

/// A controller and a charging phone, published once.
fn batteries() -> Feed {
    Feed::new(topics::BATTERIES, |context| async move {
        context
            .publisher
            .publish(topics::BATTERIES, &fake_batteries());
        pending::<()>().await;
    })
}

fn fake_disk_health() -> DiskHealth {
    DiskHealth {
        drives: DRIVES
            .iter()
            .zip(DRIVE_HEALTH)
            .map(|(drive, (wear_pct, power_on_hours))| DriveHealth {
                serial: drive.serial.to_owned(),
                wear_pct,
                power_on_hours,
            })
            .collect(),
    }
}

fn fake_batteries() -> Batteries {
    Batteries {
        devices: vec![
            Battery {
                id: CONTROLLER_ID.to_owned(),
                label: CONTROLLER_LABEL.to_owned(),
                kind: BatteryKind::Controller,
                pct: Some(CONTROLLER_PCT),
                charging: false,
            },
            Battery {
                id: PHONE_ID.to_owned(),
                label: PHONE_LABEL.to_owned(),
                kind: BatteryKind::Phone,
                pct: Some(PHONE_PCT),
                charging: true,
            },
        ],
    }
}

/// The mockup's `PEERS`: label, latitude, longitude. The mockup gives no socket counts, so they
/// fall one per endpoint down the list, which keeps its order.
const UPLINK_PEERS: [(&str, f32, f32); 6] = [
    ("GITHUB.COM", 39.04, -77.49),
    ("SPOTIFY", 59.33, 18.07),
    ("STEAM", 47.61, -122.33),
    ("DISCORD", 50.11, 8.68),
    ("API.ANTHROPIC.COM", 37.77, -122.42),
    ("TAILNET:PHONE", -22.9, -43.2),
];
const UPLINK_ATTRIBUTION: &str = "IP GEOLOCATION BY DB-IP";

/// The mockup's globe endpoints, published once.
fn uplink() -> Feed {
    Feed::new(topics::UPLINK, |context| async move {
        context.publisher.publish(topics::UPLINK, &mockup_uplink());
        pending::<()>().await;
    })
}

fn mockup_uplink() -> UplinkSnapshot {
    let endpoints: Vec<Endpoint> = UPLINK_PEERS
        .iter()
        .zip((1..=UPLINK_PEERS.len() as u32).rev())
        .map(|(&(label, lat, lon), sockets)| Endpoint {
            label: label.to_owned(),
            lat,
            lon,
            sockets,
            tailnet: label.starts_with("TAILNET:"),
        })
        .collect();
    UplinkSnapshot {
        total: endpoints.len() as u32,
        endpoints,
        attribution: UPLINK_ATTRIBUTION.to_owned(),
    }
}
