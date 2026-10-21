use ghost_alerts_inmet::{parse_alerts, provide_line};
use ghost_proto::{AlertLevel, WeatherAlert};

const AREA: &str = "3550308";
/// INMET's shape without the polygons. Alert 3 is dated in plain `YYYY-MM-DD`, the others as INMET
/// sends them, at a UTC midnight that is really the local date.
const SAMPLE: &str = include_str!("fixtures/sample.json");
/// 2026-10-21 18:00 in São Paulo (UTC−3), alert 1's start.
const STORM_START_S: u64 = 1_792_616_400;
/// 2026-10-22 06:00 in São Paulo.
const STORM_END_S: u64 = 1_792_659_600;
/// 2026-10-22 09:00 in São Paulo.
const DRY_AIR_START_S: u64 = 1_792_670_400;
/// 2026-10-22 21:00 in São Paulo, given as `21:00:00`.
const DRY_AIR_END_S: u64 = 1_792_713_600;

fn sample_alerts(json: &str) -> Vec<WeatherAlert> {
    parse_alerts(json, AREA).unwrap()
}

fn ids(alerts: &[WeatherAlert]) -> Vec<&str> {
    alerts.iter().map(|alert| alert.id.as_str()).collect()
}

fn by_id<'a>(alerts: &'a [WeatherAlert], id: &str) -> &'a WeatherAlert {
    alerts
        .iter()
        .find(|alert| alert.id == id)
        .unwrap_or_else(|| panic!("no alert {id} in {alerts:?}"))
}

#[test]
fn keeps_only_alerts_for_the_area() {
    assert_eq!(ids(&sample_alerts(SAMPLE)), ["1", "3"]);
}

#[test]
fn geocode_lists_tolerate_spaces() {
    // Alert 3 lists the area after ", ".
    assert!(ids(&sample_alerts(SAMPLE)).contains(&"3"));
    assert_eq!(ids(&parse_alerts(SAMPLE, "3509502").unwrap()), ["3"]);
}

#[test]
fn maps_inmet_severities() {
    let alerts = sample_alerts(SAMPLE);
    let storm = by_id(&alerts, "1");
    assert_eq!(storm.level, AlertLevel::Crit);
    assert_eq!(storm.severity, "PERIGO");
    assert_eq!(storm.title, "TEMPESTADE");
    assert_eq!(storm.risks.len(), 1);
    assert!(storm.risks[0].starts_with("Chuva entre 30 e 60 mm/h"));
    let dry_air = by_id(&alerts, "3");
    assert_eq!(dry_air.level, AlertLevel::Warn);
    assert_eq!(dry_air.severity, "PERIGO POTENCIAL");
    assert_eq!(dry_air.title, "BAIXA UMIDADE");

    let gravest = sample_alerts(&SAMPLE.replace("\"Perigo\"", "\"Grande Perigo\""));
    assert_eq!(by_id(&gravest, "1").level, AlertLevel::Crit);
    assert_eq!(by_id(&gravest, "1").severity, "GRANDE PERIGO");

    let unknown = sample_alerts(&SAMPLE.replace("\"Perigo Potencial\"", "\"Atenção\""));
    assert_eq!(by_id(&unknown, "3").level, AlertLevel::Warn);
}

#[test]
fn reads_brasilia_time() {
    let alerts = sample_alerts(SAMPLE);
    let storm = by_id(&alerts, "1");
    assert_eq!((storm.start_s, storm.end_s), (STORM_START_S, STORM_END_S));
    let dry_air = by_id(&alerts, "3");
    assert_eq!(
        (dry_air.start_s, dry_air.end_s),
        (DRY_AIR_START_S, DRY_AIR_END_S)
    );
}

#[test]
fn duplicate_ids_across_today_and_future_appear_once() {
    let alerts = sample_alerts(SAMPLE);

    assert_eq!(alerts.iter().filter(|alert| alert.id == "1").count(), 1);
    // The copy in `futuro` starts at 20:00 and is only a warning.
    assert_eq!(by_id(&alerts, "1").start_s, STORM_START_S);
    assert_eq!(by_id(&alerts, "1").level, AlertLevel::Crit);
}

#[test]
fn unparsable_time_skips_the_entry() {
    let alerts = sample_alerts(SAMPLE);
    let kept = ids(&alerts);

    assert!(!kept.contains(&"4"), "18h is not a time of day");
    assert!(
        !kept.contains(&"5"),
        "a date with a time of its own may be a real UTC instant"
    );
    assert!(kept.contains(&"1") && kept.contains(&"3"));
}

#[test]
fn sorts_by_start() {
    let later_first = r#"{"hoje": [
        {"id": 8, "geocodes": "3550308", "descricao": "Tempestade", "severidade": "Perigo",
         "data_inicio": "2026-10-23", "hora_inicio": "10:00", "data_fim": "2026-10-23", "hora_fim": "12:00"},
        {"id": 9, "geocodes": "3550308", "descricao": "Tempestade", "severidade": "Perigo",
         "data_inicio": "2026-10-22", "hora_inicio": "10:00", "data_fim": "2026-10-22", "hora_fim": "12:00"}
    ], "futuro": []}"#;

    assert_eq!(ids(&sample_alerts(later_first)), ["9", "8"]);
}

#[test]
fn provide_line_is_exact() {
    assert_eq!(
        provide_line(),
        "{\"t\":\"provide\",\"proto\":1,\"name\":\"inmet\",\"topics\":[\"weather.alerts\"]}\n"
    );
}
