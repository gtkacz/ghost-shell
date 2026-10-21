use std::io;
use std::sync::{Arc, Mutex};

use ghost_alerts_inmet::{Wording, parse_alerts, provide_line};
use ghost_proto::{AlertLevel, WeatherAlert};

const AREA: &str = "3550308";
/// INMET's shape without the polygons. Alert 3 is dated in plain `YYYY-MM-DD` and alert 5 in a
/// form INMET does not use; the rest as INMET sends them, at a UTC midnight that is really the
/// local date. Alert 6 has ended, alert 7 is null throughout, and alerts 8 and 9, for the area,
/// are malformed: 8 has a string id and 9 a number for its geocodes. Alerts 1 to 3 carry the
/// numbers INMET has been seen to give their event and severity.
const SAMPLE: &str = include_str!("fixtures/sample.json");
/// 2026-10-21 18:00 in São Paulo (UTC−3), alert 1's start.
const STORM_START_S: u64 = 1_792_616_400;
/// 2026-10-22 06:00 in São Paulo.
const STORM_END_S: u64 = 1_792_659_600;
/// 2026-10-22 09:00 in São Paulo.
const DRY_AIR_START_S: u64 = 1_792_670_400;
/// 2026-10-22 21:00 in São Paulo, given as `21:00:00`.
const DRY_AIR_END_S: u64 = 1_792_713_600;

fn alerts_in(json: &str, area: &str) -> Vec<WeatherAlert> {
    parse_alerts(json, area)
        .unwrap()
        .into_iter()
        .map(|issued| issued.alert)
        .collect()
}

fn sample_alerts(json: &str) -> Vec<WeatherAlert> {
    alerts_in(json, AREA)
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
    assert_eq!(ids(&alerts_in(SAMPLE, "3509502")), ["3"]);
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

    let untidy = sample_alerts(&SAMPLE.replace("\"Perigo\"", "\" perigo \""));
    assert_eq!(by_id(&untidy, "1").level, AlertLevel::Crit);
    assert_eq!(by_id(&untidy, "1").severity, "PERIGO");
}

#[test]
fn reads_the_wording_and_its_numbers_untranslated() {
    let issued = parse_alerts(SAMPLE, AREA).unwrap();

    let wording: Vec<(&Wording, &Wording)> = issued
        .iter()
        .map(|issued| (&issued.event, &issued.severity))
        .collect();
    let term = |id, text: &str| Wording {
        id,
        text: text.to_owned(),
    };
    assert_eq!(
        wording,
        [
            (&term(Some(38), "Tempestade"), &term(Some(7), "Perigo")),
            (
                &term(Some(23), "Baixa Umidade"),
                &term(Some(6), "Perigo Potencial")
            ),
        ]
    );
    let storm = &issued[0].alert;
    assert_eq!(
        (&storm.original_title, &storm.original_severity),
        (&None, &None)
    );
    assert!(!storm.auto_translated);
}

#[test]
fn a_number_of_another_type_is_read_as_none_and_keeps_the_alert() {
    let json = document(
        &[entry(50, "2026-10-22", "09:00").replace(
            r#""descricao": "Tempestade""#,
            r#""descricao": " Tempestade ", "id_condicao_severa": "38", "id_severidade": null"#,
        )],
        &[],
    );

    let issued = parse_alerts(&json, AREA).unwrap();

    assert_eq!(issued.len(), 1);
    assert_eq!(
        (&issued[0].event, issued[0].severity.id),
        (
            &Wording {
                id: None,
                text: "Tempestade".to_owned()
            },
            None
        )
    );
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
    assert!(!kept.contains(&"5"), "21/10/2026 is not YYYY-MM-DD");
    assert!(kept.contains(&"1") && kept.contains(&"3"));
}

#[test]
fn ended_alerts_are_skipped() {
    assert!(!ids(&sample_alerts(SAMPLE)).contains(&"6"));

    let reopened = SAMPLE.replace("\"encerrado\": true", "\"encerrado\": false");
    assert!(ids(&sample_alerts(&reopened)).contains(&"6"));
}

/// An alert for the area that starts on `date` at `time` and ends that day at 23:00.
fn entry(id: u64, date: &str, time: &str) -> String {
    format!(
        r#"{{"id": {id}, "geocodes": "{AREA}", "descricao": "Tempestade", "severidade": "Perigo",
            "data_inicio": "{date}", "hora_inicio": "{time}", "data_fim": "{date}", "hora_fim": "23:00"}}"#
    )
}

fn document(hoje: &[String], futuro: &[String]) -> String {
    format!(
        r#"{{"hoje": [{}], "futuro": [{}]}}"#,
        hoje.join(", "),
        futuro.join(", ")
    )
}

#[test]
fn nulls_read_as_empty() {
    // Alert 7 in the fixture, for no area at all, is null throughout.
    assert_eq!(ids(&sample_alerts(SAMPLE)), ["1", "3"]);

    let sparse = document(
        &[entry(20, "2026-10-22", "09:00").replace(
            r#""descricao": "Tempestade""#,
            r#""descricao": null, "riscos": null, "encerrado": null"#,
        )],
        &[],
    );
    let alerts = sample_alerts(&sparse);

    assert_eq!(ids(&alerts), ["20"]);
    assert_eq!(alerts[0].title, "");
    assert!(alerts[0].risks.is_empty());
    assert_eq!(alerts[0].level, AlertLevel::Crit);
}

#[test]
fn a_bad_first_copy_does_not_hide_a_good_later_one() {
    let json = document(
        &[entry(30, "2026-10-22", "18h")],
        &[entry(30, "2026-10-22", "09:00")],
    );

    let alerts = sample_alerts(&json);

    assert_eq!(ids(&alerts), ["30"]);
    assert_eq!(alerts[0].start_s, DRY_AIR_START_S);
}

/// Collects everything logged while it is the default subscriber.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn logged_while<T>(run: impl FnOnce() -> T) -> (T, String) {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let result = tracing::subscriber::with_default(subscriber, run);
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    (result, log)
}

#[test]
fn a_malformed_entry_is_skipped_alone() {
    let (alerts, log) = logged_while(|| sample_alerts(SAMPLE));

    assert_eq!(ids(&alerts), ["1", "3"]);
    assert_eq!(log.matches("unexpected shape").count(), 2, "got {log:?}");
}

#[test]
fn a_changed_date_suffix_still_reads_the_local_date_and_warns_once() {
    let json = document(
        &[
            entry(40, "2026-10-22T03:00:00.000Z", "09:00"),
            entry(41, "2026-10-22T03:00:00-03:00", "09:00"),
        ],
        &[],
    );

    let (alerts, log) = logged_while(|| sample_alerts(&json));

    assert_eq!(ids(&alerts), ["40", "41"]);
    assert!(alerts.iter().all(|alert| alert.start_s == DRY_AIR_START_S));
    assert_eq!(log.matches("date format").count(), 1, "got {log:?}");

    let (_, quiet) = logged_while(|| sample_alerts(SAMPLE));
    assert!(!quiet.contains("date format"), "got {quiet:?}");
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

#[test]
fn severities_are_classified_after_normalising_them() {
    for (severidade, expected) in [
        ("Peri\u{301}go", AlertLevel::Crit),
        (" perigo\u{a0}", AlertLevel::Crit),
        ("Grande\u{a0}Perigo", AlertLevel::Crit),
        ("GRANDE  PERI\u{301}GO", AlertLevel::Crit),
        ("Perigo\u{a0}Potencial", AlertLevel::Warn),
        ("Perigo Pote\u{302}ncial", AlertLevel::Warn),
    ] {
        let json = document(
            &[entry(60, "2026-10-22", "09:00").replace(
                r#""severidade": "Perigo""#,
                &format!(r#""severidade": "{severidade}""#),
            )],
            &[],
        );

        let (alerts, log) = logged_while(|| sample_alerts(&json));

        assert_eq!(alerts[0].level, expected, "{severidade:?}");
        // Warn is also what an unknown severity falls back to, so it must not have been unknown.
        assert!(
            !log.contains("unknown INMET severity"),
            "{severidade:?}: {log}"
        );
    }
}
