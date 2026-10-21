//! The `sky` topic: sunrise, sunset and the moon at `location.{lat, lon}`, worked out here from the
//! wall clock, so nothing is fetched.
//!
//! While some client wants the topic it is recomputed every minute (spec §12), and also at once
//! when the topic becomes wanted, since its last value may be hours old, and when the location
//! changes.
//!
//! Each computation reads the wall clock afresh, because tokio's clock stands still while the
//! machine is suspended: the first wake after a resume comes at most a minute late and is already
//! right.

use std::f64::consts::TAU;
use std::future::pending;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::time::Duration;

use ghost_proto::{ResolvedConfig, SkySnapshot, topics};
use nix::libc;
use tokio::time::{Instant, sleep_until};
use tracing::warn;

use super::weather::WallClock;
use super::{unix_now_s, wanted};
use crate::server::{Demand, Feed, FeedContext, Publisher};

const RECOMPUTE_EVERY: Duration = Duration::from_secs(60);
const SECONDS_PER_DAY: i64 = 86_400;
/// 2000-01-06 18:14 UTC, a new moon, in seconds since the Unix epoch.
const REFERENCE_NEW_MOON_S: i64 = 947_182_440;
const SYNODIC_MONTH_DAYS: f64 = 29.530_588_853;
/// The mean month keeps time with the real phases to only about half a day, so a principal phase
/// is named for the day either side of its exact instant.
const PRINCIPAL_PHASE_SPAN: f64 = 1.0 / SYNODIC_MONTH_DAYS;
/// The sun's upper limb on the horizon: its 16' radius and 34' of refraction past the 90°.
const SUNRISE_ZENITH_DEG: f64 = 90.833;
/// 2000-01-01 12:00 UTC, the epoch of NOAA's Julian centuries, in seconds since the Unix epoch.
const J2000_S: i64 = 946_728_000;
const JULIAN_CENTURY_S: f64 = 36_525.0 * 86_400.0;
const MINUTES_TO_NOON: f64 = 720.0;
/// The sun crosses 15° of longitude an hour.
const MINUTES_PER_DEGREE: f64 = 4.0;
/// Days in 400 Gregorian years, which is the whole cycle of leap days.
const DAYS_PER_ERA: i64 = 146_097;
/// Days from 0000-03-01, where [`days_from_civil`] counts from, to 1970-01-01.
const DAYS_FROM_ERA_START_TO_EPOCH: i64 = 719_468;
/// `struct tm` counts its years from 1900.
const TM_YEAR_BASE: i32 = 1900;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moon {
    /// How far through the lunar cycle, from 0 at a new moon up to 1 at the next.
    pub phase: f64,
    /// The lit fraction of the disc, from 0 to 1.
    pub illumination: f64,
    pub name: &'static str,
}

/// The moon at `at_s`, in seconds since the Unix epoch, counting mean synodic months from the new
/// moon of 2000-01-06.
pub fn moon(at_s: i64) -> Moon {
    let days = (at_s - REFERENCE_NEW_MOON_S) as f64 / SECONDS_PER_DAY as f64;
    let phase = (days / SYNODIC_MONTH_DAYS).rem_euclid(1.0);
    Moon {
        phase,
        illumination: (1.0 - (TAU * phase).cos()) / 2.0,
        name: phase_name(phase),
    }
}

fn phase_name(phase: f64) -> &'static str {
    // The new moon is listed at both ends because the cycle wraps around it.
    const PRINCIPAL: [(f64, &str); 5] = [
        (0.0, "NEW"),
        (0.25, "FIRST QUARTER"),
        (0.5, "FULL"),
        (0.75, "LAST QUARTER"),
        (1.0, "NEW"),
    ];
    let principal = PRINCIPAL
        .iter()
        .find(|(exact, _)| (phase - exact).abs() <= PRINCIPAL_PHASE_SPAN);
    if let Some(&(_, name)) = principal {
        return name;
    }
    match phase {
        ..0.25 => "WAXING CRESCENT",
        ..0.5 => "WAXING GIBBOUS",
        ..0.75 => "WANING GIBBOUS",
        _ => "WANING CRESCENT",
    }
}

/// Sunrise and sunset, in seconds since the Unix epoch, on the calendar day `date` (year, month,
/// day) at `lat` and `lon` in degrees, north and east being positive. The day is taken in mean
/// solar time at `lon`, which is the local day wherever the clocks keep to the longitude.
/// `None` when the sun stays up or down all day.
pub fn sun_times(lat: f64, lon: f64, date: (i32, u8, u8)) -> Option<(i64, i64)> {
    let midnight_s = days_from_civil(date) * SECONDS_PER_DAY;
    let rise_s = sun_event_s(lat, lon, midnight_s, true)?;
    let set_s = sun_event_s(lat, lon, midnight_s, false)?;
    Some((rise_s, set_s))
}

/// When the sun rises or sets after the UTC midnight at `midnight_s`, by NOAA's solar calculator.
fn sun_event_s(lat: f64, lon: f64, midnight_s: i64, rising: bool) -> Option<i64> {
    let minutes_after_midnight = |read_at_s: f64| -> Option<f64> {
        let sun = sun_at((read_at_s - J2000_S as f64) / JULIAN_CENTURY_S);
        let hour_angle = horizon_hour_angle(lat, sun.declination)?;
        let signed_hour_angle = if rising { hour_angle } else { -hour_angle };
        Some(
            MINUTES_TO_NOON
                - MINUTES_PER_DEGREE * (lon + signed_hour_angle)
                - sun.equation_of_time_min,
        )
    };
    // The sun moves a little in the hours it takes to get from midnight to the event, so the first
    // answer is read again at the time it gave.
    let first_guess = minutes_after_midnight(midnight_s as f64)?;
    let event = minutes_after_midnight(midnight_s as f64 + first_guess * 60.0)?;
    Some(midnight_s + (event * 60.0).round() as i64)
}

struct Sun {
    /// In radians.
    declination: f64,
    equation_of_time_min: f64,
}

/// The sun `t` Julian centuries after J2000, from the series in NOAA's "General Solar Position
/// Calculations", whose coefficients are written out as published.
fn sun_at(t: f64) -> Sun {
    let mean_longitude_deg = 280.46646 + t * (36000.76983 + t * 0.0003032);
    let mean_anomaly = (357.52911 + t * (35999.05029 - t * 0.0001537)).to_radians();
    let eccentricity = 0.016708634 - t * (0.000042037 + t * 0.0000001267);
    let center_deg = mean_anomaly.sin() * (1.914602 - t * (0.004817 + t * 0.000014))
        + (2.0 * mean_anomaly).sin() * (0.019993 - t * 0.000101)
        + (3.0 * mean_anomaly).sin() * 0.000289;
    let moon_node = (125.04 - 1934.136 * t).to_radians();
    let apparent_longitude =
        (mean_longitude_deg + center_deg - 0.00569 - 0.00478 * moon_node.sin()).to_radians();
    let mean_obliquity_deg =
        23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60.0) / 60.0;
    let obliquity = (mean_obliquity_deg + 0.00256 * moon_node.cos()).to_radians();
    let declination = (obliquity.sin() * apparent_longitude.sin()).asin();

    let mean_longitude = mean_longitude_deg.to_radians();
    let y = (obliquity / 2.0).tan().powi(2);
    let equation_of_time_rad = y * (2.0 * mean_longitude).sin()
        - 2.0 * eccentricity * mean_anomaly.sin()
        + 4.0 * eccentricity * y * mean_anomaly.sin() * (2.0 * mean_longitude).cos()
        - 0.5 * y * y * (4.0 * mean_longitude).sin()
        - 1.25 * eccentricity * eccentricity * (2.0 * mean_anomaly).sin();
    Sun {
        declination,
        equation_of_time_min: MINUTES_PER_DEGREE * equation_of_time_rad.to_degrees(),
    }
}

/// How far from solar noon, in degrees, the sun crosses the horizon at `lat`; `None` when it
/// never does.
fn horizon_hour_angle(lat: f64, declination: f64) -> Option<f64> {
    let lat = lat.to_radians();
    let zenith = SUNRISE_ZENITH_DEG.to_radians();
    let cosine = zenith.cos() / (lat.cos() * declination.cos()) - lat.tan() * declination.tan();
    (-1.0..=1.0)
        .contains(&cosine)
        .then(|| cosine.acos().to_degrees())
}

/// The days from 1970-01-01 to a Gregorian date, by Howard Hinnant's `days_from_civil`, which
/// counts the year from March so that its leap day comes last.
fn days_from_civil((year, month, day): (i32, u8, u8)) -> i64 {
    let (year, month, day) = (i64::from(year), i64::from(month), i64::from(day));
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * DAYS_PER_ERA + day_of_era - DAYS_FROM_ERA_START_TO_EPOCH
}

/// The calendar date at `at_s` in the machine's time zone, as (year, month, day).
fn local_date(at_s: i64) -> Option<(i32, u8, u8)> {
    let at = at_s as libc::time_t;
    let mut broken_down = MaybeUninit::<libc::tm>::uninit();
    // glibc's `localtime_r` keeps the zone it first read. A zone changed under a running daemon
    // would skew the date by a day at most, a small error for the sun times, so a `tzset` per
    // computation is not worth it.
    // SAFETY: `localtime_r` writes a `tm` through the pointer it is given, which is valid and
    // unaliased, and returns null if it wrote none.
    let local = unsafe {
        if libc::localtime_r(&at, broken_down.as_mut_ptr()).is_null() {
            return None;
        }
        broken_down.assume_init()
    };
    Some((
        local.tm_year + TM_YEAR_BASE,
        u8::try_from(local.tm_mon + 1).ok()?,
        u8::try_from(local.tm_mday).ok()?,
    ))
}

/// Publishes `sky` for the configured place, from the wall clock. Without a place it publishes
/// the unavailable default and computes nothing.
pub fn feed() -> Feed {
    feed_with_clock(Arc::new(unix_now_s))
}

/// As [`feed`], reading the wall clock from `clock`.
pub fn feed_with_clock(clock: WallClock) -> Feed {
    Feed::new(topics::SKY, move |context| run(clock, context))
}

async fn run(clock: WallClock, mut context: FeedContext) {
    let mut place = place_in(&context.config.borrow_and_update());
    publish(&context.publisher, place, &clock);
    let mut due = is_recomputing(place, &context.demand).then(|| Instant::now() + RECOMPUTE_EVERY);
    loop {
        if is_recomputing(place, &context.demand) {
            // The topic may have gone hours unwanted, so what it last said is due at once when it
            // is wanted again.
            due.get_or_insert_with(Instant::now);
        } else {
            due = None;
        }
        tokio::select! {
            biased;
            Ok(()) = context.config.changed() => {
                let moved_to = place_in(&context.config.borrow_and_update());
                if moved_to != place {
                    place = moved_to;
                    publish(&context.publisher, place, &clock);
                }
            }
            Ok(()) = context.demand.changed() => {}
            () = sleep_until_due(due) => {
                publish(&context.publisher, place, &clock);
                due = Some(Instant::now() + RECOMPUTE_EVERY);
            }
        }
    }
}

/// Whether the topic is due a value every minute: it needs a place and a client that wants it.
fn is_recomputing(place: Option<Place>, demand: &Demand) -> bool {
    place.is_some() && wanted(demand, topics::SKY)
}

async fn sleep_until_due(due: Option<Instant>) {
    match due {
        Some(due) => sleep_until(due).await,
        None => pending().await,
    }
}

fn publish(publisher: &Publisher, place: Option<Place>, clock: &WallClock) {
    publisher.publish(topics::SKY, &snapshot(place, clock() as i64));
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

/// The sky at `place` at `now_s`, with the sun times of the machine's calendar day.
fn snapshot(place: Option<Place>, now_s: i64) -> SkySnapshot {
    let Some(Place { lat, lon }) = place else {
        return SkySnapshot::default();
    };
    let Some(date) = local_date(now_s) else {
        warn!("cannot read the local date; the sky topic reports nothing");
        return SkySnapshot::default();
    };
    let sun = sun_times(lat, lon, date);
    let moon = moon(now_s);
    SkySnapshot {
        available: true,
        sunrise_s: sun.and_then(|(rise_s, _)| u64::try_from(rise_s).ok()),
        sunset_s: sun.and_then(|(_, set_s)| u64::try_from(set_s).ok()),
        moon_phase: moon.phase as f32,
        moon_illumination: moon.illumination as f32,
        moon_name: moon.name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE_S: i64 = 60;
    const HOUR_S: i64 = 60 * MINUTE_S;
    const TOLERANCE_S: i64 = 2 * MINUTE_S;
    const SAO_PAULO: Place = Place {
        lat: -23.55,
        lon: -46.63,
    };
    const SVALBARD: Place = Place {
        lat: 78.22,
        lon: 15.65,
    };

    fn utc(year: i32, month: u8, day: u8, hour: i64, minute: i64) -> i64 {
        days_from_civil((year, month, day)) * SECONDS_PER_DAY + hour * HOUR_S + minute * MINUTE_S
    }

    fn assert_close(actual: i64, expected: i64, tolerance_s: i64) {
        assert!(
            (actual - expected).abs() <= tolerance_s,
            "{actual} is {} s from {expected}",
            actual - expected
        );
    }

    /// The moon `offset_s` after the instant `phase` of the way through the cycle that follows
    /// the reference new moon.
    fn moon_at_phase(phase: f64, offset_s: i64) -> Moon {
        let cycle_s = SYNODIC_MONTH_DAYS * SECONDS_PER_DAY as f64;
        moon(REFERENCE_NEW_MOON_S + (phase * cycle_s).round() as i64 + offset_s)
    }

    #[test]
    fn london_midsummer() {
        let (rise, set) = sun_times(51.4769, -0.0005, (2026, 6, 21)).unwrap();
        assert_close(rise, utc(2026, 6, 21, 3, 43), 120);
        assert_close(set, utc(2026, 6, 21, 20, 21), 120);
    }

    #[test]
    fn svalbard_midnight_sun_has_no_sunrise() {
        assert_eq!(sun_times(78.22, 15.65, (2026, 6, 21)), None);
    }

    #[test]
    fn svalbard_polar_night_has_no_sunrise() {
        assert_eq!(sun_times(78.22, 15.65, (2026, 12, 21)), None);
    }

    #[test]
    fn sao_paulo_in_spring() {
        let (rise, set) = sun_times(-23.55, -46.63, (2026, 10, 21)).unwrap();
        // 05:29 and 18:14 BRT, from the USNO almanac algorithm, which differs from NOAA's.
        assert_close(rise, utc(2026, 10, 21, 8, 29), TOLERANCE_S);
        assert_close(set, utc(2026, 10, 21, 21, 14), TOLERANCE_S);
    }

    #[test]
    fn a_day_is_the_one_that_is_local_to_the_longitude() {
        // Auckland's summer sunrise falls on the previous UTC day.
        let (rise, set) = sun_times(-36.85, 174.76, (2026, 12, 21)).unwrap();
        assert_close(rise, utc(2026, 12, 20, 16, 58), TOLERANCE_S);
        assert_close(set, utc(2026, 12, 21, 7, 39), TOLERANCE_S);
        // And Honolulu's summer sunset on the next.
        let (rise, set) = sun_times(21.31, -157.86, (2026, 6, 21)).unwrap();
        assert_close(rise, utc(2026, 6, 21, 15, 50), TOLERANCE_S);
        assert_close(set, utc(2026, 6, 22, 5, 16), TOLERANCE_S);
    }

    #[test]
    fn civil_dates_count_days_from_the_unix_epoch() {
        assert_eq!(days_from_civil((1970, 1, 1)), 0);
        assert_eq!(days_from_civil((1969, 12, 31)), -1);
        assert_eq!(
            days_from_civil((2024, 3, 1)) - days_from_civil((2024, 2, 28)),
            2
        );
        assert_eq!(
            days_from_civil((2100, 3, 1)) - days_from_civil((2100, 2, 28)),
            1
        );
        // The same instants the moon tests start from.
        assert_eq!(utc(2000, 1, 6, 18, 14), REFERENCE_NEW_MOON_S);
        assert_eq!(utc(2024, 4, 8, 18, 21), 1_712_600_460);
        assert_eq!(utc(2024, 4, 23, 23, 49), 1_713_916_140);
    }

    #[test]
    fn eclipse_new_moon() {
        let m = moon(1_712_600_460);
        assert!(m.illumination < 0.02);
        assert_eq!(m.name, "NEW");
    }

    #[test]
    fn april_2024_full_moon() {
        let m = moon(1_713_916_140);
        assert!(m.illumination > 0.98);
        assert_eq!(m.name, "FULL");
    }

    #[test]
    fn the_cycle_runs_through_eight_names() {
        let names: Vec<&str> = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875]
            .iter()
            .map(|&phase| moon_at_phase(phase, 0).name)
            .collect();

        assert_eq!(
            names,
            [
                "NEW",
                "WAXING CRESCENT",
                "FIRST QUARTER",
                "WAXING GIBBOUS",
                "FULL",
                "WANING GIBBOUS",
                "LAST QUARTER",
                "WANING CRESCENT",
            ]
        );
    }

    #[test]
    fn a_principal_phase_lasts_a_day_either_side_of_its_instant() {
        let day_s = SECONDS_PER_DAY;
        let within = day_s - HOUR_S;
        let beyond = day_s + HOUR_S;
        let name = |phase, offset_s| moon_at_phase(phase, offset_s).name;

        for (phase, principal) in [
            (0.25, "FIRST QUARTER"),
            (0.5, "FULL"),
            (0.75, "LAST QUARTER"),
        ] {
            assert_eq!(
                name(phase, -within),
                principal,
                "{principal} within a day before"
            );
            assert_eq!(
                name(phase, within),
                principal,
                "{principal} within a day after"
            );
        }
        assert_eq!(name(0.25, -beyond), "WAXING CRESCENT");
        assert_eq!(name(0.25, beyond), "WAXING GIBBOUS");
        assert_eq!(name(0.5, -beyond), "WAXING GIBBOUS");
        assert_eq!(name(0.5, beyond), "WANING GIBBOUS");
        assert_eq!(name(0.75, -beyond), "WANING GIBBOUS");
        assert_eq!(name(0.75, beyond), "WANING CRESCENT");
        // The new moon spans the end of one cycle and the start of the next.
        assert_eq!(name(0.0, -within), "NEW");
        assert_eq!(name(0.0, within), "NEW");
        assert_eq!(name(0.0, -beyond), "WANING CRESCENT");
        assert_eq!(name(0.0, beyond), "WAXING CRESCENT");
    }

    #[test]
    fn illumination_follows_the_phase() {
        assert!(moon_at_phase(0.0, 0).illumination < 1e-9);
        assert!((moon_at_phase(0.25, 0).illumination - 0.5).abs() < 1e-3);
        assert!(moon_at_phase(0.5, 0).illumination > 1.0 - 1e-9);
        assert!((moon_at_phase(0.75, 0).illumination - 0.5).abs() < 1e-3);
    }

    #[test]
    fn the_phase_stays_in_the_cycle_before_the_reference_too() {
        let ten_days_before = REFERENCE_NEW_MOON_S - 10 * SECONDS_PER_DAY;

        let phase = moon(ten_days_before).phase;

        assert_close(
            (phase * 1e6).round() as i64,
            ((1.0 - 10.0 / SYNODIC_MONTH_DAYS) * 1e6).round() as i64,
            1,
        );
        for hours in 0..24 * 90 {
            let m = moon(ten_days_before + hours * HOUR_S);
            assert!((0.0..1.0).contains(&m.phase), "phase {}", m.phase);
            assert!((0.0..=1.0).contains(&m.illumination));
        }
    }

    #[test]
    fn no_location_is_the_unavailable_default() {
        assert_eq!(
            snapshot(None, utc(2026, 10, 21, 19, 13)),
            SkySnapshot::default()
        );
    }

    #[test]
    fn a_place_gets_the_sun_and_moon_for_its_local_day() {
        let now_s = utc(2026, 10, 21, 15, 0);

        let sky = snapshot(Some(SAO_PAULO), now_s);

        assert!(sky.available);
        let (Some(rise_s), Some(set_s)) = (sky.sunrise_s, sky.sunset_s) else {
            panic!("São Paulo has a sunrise and a sunset: {sky:?}");
        };
        // The machine's own date can be a day either side of this instant's UTC date.
        assert_close(rise_s as i64, now_s, 36 * HOUR_S);
        assert!(rise_s < set_s);
        let moon = moon(now_s);
        assert_eq!(sky.moon_phase, moon.phase as f32);
        assert_eq!(sky.moon_illumination, moon.illumination as f32);
        assert_eq!(sky.moon_name, moon.name);
    }

    #[test]
    fn polar_day_keeps_the_moon_but_has_no_sun_times() {
        let sky = snapshot(Some(SVALBARD), utc(2026, 6, 21, 12, 0));

        assert!(sky.available);
        assert_eq!((sky.sunrise_s, sky.sunset_s), (None, None));
        assert!(!sky.moon_name.is_empty());
    }

    #[test]
    fn the_local_date_is_at_most_a_day_from_the_utc_date() {
        let instants = [
            utc(2026, 10, 21, 3, 0),
            utc(2026, 10, 21, 19, 13),
            utc(2026, 12, 31, 23, 30),
            utc(2027, 1, 1, 0, 30),
            utc(2028, 2, 29, 12, 0),
        ];
        for at_s in instants {
            let local = local_date(at_s).expect("the local date of a recent instant");
            let days_apart = days_from_civil(local) - at_s.div_euclid(SECONDS_PER_DAY);
            assert!(days_apart.abs() <= 1, "{local:?} for {at_s}");
        }
    }
}
