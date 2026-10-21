use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ghost_alerts_inmet::{Fetch, IssuedAlert, Translator, parse_alerts};
use ghost_proto::WeatherAlert;
use serde_json::{Value, json};
use tempfile::TempDir;

const AREA: &str = "3550308";
const SAMPLE: &str = include_str!("fixtures/sample.json");
const STORE: &str = "ghost-shell/translations.json";
const MINUTE: Duration = Duration::from_secs(60);
/// A term the glossary lacks, with an accent and a space to encode.
const DRY_FOG: &str = "Névoa Seca";
const DRY_FOG_URL: &str =
    "https://api.mymemory.translated.net/get?q=N%C3%A9voa%20Seca&langpair=pt-BR%7Cen-US";

type Reply = Result<String, ureq::Error>;

/// Answers in MyMemory's place with `replies`, in order, and keeps every URL it is asked for.
#[derive(Clone, Default)]
struct FakeMyMemory {
    replies: Rc<RefCell<VecDeque<Reply>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

impl FakeMyMemory {
    fn replying(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self {
            replies: Rc::new(RefCell::new(replies.into_iter().collect())),
            ..Self::default()
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.borrow().clone()
    }
}

impl Fetch for FakeMyMemory {
    fn get(&self, url: &str) -> Reply {
        self.asked.borrow_mut().push(url.to_owned());
        self.replies
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| panic!("MyMemory was asked about {url} unexpectedly"))
    }
}

/// MyMemory's answer when it translates.
fn translated(text: &str) -> Reply {
    Ok(json!({
        "responseData": {"translatedText": text, "match": 0.85},
        "quotaFinished": false,
        "responseDetails": "",
        "responseStatus": 200,
        "matches": []
    })
    .to_string())
}

/// An alert for the area whose event and severity are each INMET's text and, maybe, its number.
fn aviso(id: u64, event: (&str, Option<u64>), severity: (&str, Option<u64>)) -> Value {
    json!({
        "id": id,
        "geocodes": AREA,
        "descricao": event.0,
        "id_condicao_severa": event.1,
        "severidade": severity.0,
        "id_severidade": severity.1,
        "data_inicio": "2026-10-22T00:00:00.000Z",
        "hora_inicio": "09:00",
        "data_fim": "2026-10-22T00:00:00.000Z",
        "hora_fim": "21:00",
        "encerrado": false,
        "riscos": []
    })
}

fn issued(avisos: &[Value]) -> Vec<IssuedAlert> {
    parse_alerts(&json!({"hoje": avisos, "futuro": []}).to_string(), AREA).unwrap()
}

fn store_in(dir: &TempDir) -> PathBuf {
    dir.path().join(STORE)
}

fn into_english(store: &Path, mymemory: &FakeMyMemory) -> Translator<FakeMyMemory> {
    Translator::new("pt-BR", "en-US", store.to_owned(), mymemory.clone()).unwrap()
}

fn write_store(store: &Path, contents: &str) {
    fs::create_dir_all(store.parent().unwrap()).unwrap();
    fs::write(store, contents).unwrap();
}

fn read_store(store: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(store).unwrap()).unwrap()
}

/// Title, severity, their originals, and whether an online translator was involved.
fn shown(alert: &WeatherAlert) -> (&str, &str, Option<&str>, Option<&str>, bool) {
    (
        &alert.title,
        &alert.severity,
        alert.original_title.as_deref(),
        alert.original_severity.as_deref(),
        alert.auto_translated,
    )
}

#[test]
fn the_glossary_translates_by_inmets_numbers() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::default();
    let mut translator = into_english(&store_in(&dir), &mymemory);

    let sample = translator.translate(parse_alerts(SAMPLE, AREA).unwrap(), Instant::now());
    let reworded = translator.translate(
        issued(&[aviso(
            1,
            ("Tempestade Local", Some(38)),
            ("Laranja", Some(7)),
        )]),
        Instant::now(),
    );

    assert_eq!(
        sample.iter().map(shown).collect::<Vec<_>>(),
        [
            ("STORM", "DANGER", Some("TEMPESTADE"), Some("PERIGO"), false),
            (
                "LOW HUMIDITY",
                "POTENTIAL DANGER",
                Some("BAIXA UMIDADE"),
                Some("PERIGO POTENCIAL"),
                false
            ),
        ]
    );
    assert_eq!(
        shown(&reworded[0]),
        (
            "STORM",
            "DANGER",
            Some("TEMPESTADE LOCAL"),
            Some("LARANJA"),
            false
        )
    );
    assert!(mymemory.asked().is_empty());
}

#[test]
fn the_glossary_falls_back_to_the_text_when_the_number_is_unknown() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::default();
    let mut translator = into_english(&store_in(&dir), &mymemory);

    let alerts = translator.translate(
        issued(&[
            aviso(1, ("Vendaval", Some(999)), ("Grande Perigo", None)),
            aviso(2, ("Geada", None), ("Perigo", Some(99))),
        ]),
        Instant::now(),
    );

    assert_eq!(
        alerts.iter().map(shown).collect::<Vec<_>>(),
        [
            (
                "GALE",
                "GREAT DANGER",
                Some("VENDAVAL"),
                Some("GRANDE PERIGO"),
                false
            ),
            ("FROST", "DANGER", Some("GEADA"), Some("PERIGO"), false),
        ]
    );
    assert!(mymemory.asked().is_empty());
}

#[test]
fn the_glossary_ignores_case_accents_and_surrounding_space() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::default();
    let mut translator = into_english(&store_in(&dir), &mymemory);

    let alerts = translator.translate(
        issued(&[
            aviso(
                1,
                ("DECLINIO DE TEMPERATURA", None),
                (" perigo potencial ", None),
            ),
            aviso(2, ("declínio de temperatura", None), ("PERÍGO", None)),
        ]),
        Instant::now(),
    );

    assert_eq!(
        alerts.iter().map(shown).collect::<Vec<_>>(),
        [
            (
                "TEMPERATURE DROP",
                "POTENTIAL DANGER",
                Some("DECLINIO DE TEMPERATURA"),
                Some("PERIGO POTENCIAL"),
                false
            ),
            (
                "TEMPERATURE DROP",
                "DANGER",
                Some("DECLÍNIO DE TEMPERATURA"),
                Some("PERÍGO"),
                false
            ),
        ]
    );
    assert!(mymemory.asked().is_empty());
}

#[test]
fn a_checked_entry_beats_the_glossary_which_beats_an_automatic_one() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    write_store(
        &store,
        r#"{"pt-BR|en-US": {
            "Tempestade": {"text": "Thunderstorm", "auto": false},
            "Perigo": {"text": "WARNING", "auto": true, "source": "mymemory", "at": "2026-10-21T18:00:00Z"},
            "Vendaval": {"text": "WINDSTORM", "auto": true, "source": "mymemory", "at": "2026-10-21T18:00:00Z"},
            "Nevoeiro": {"text": "FOG", "auto": true, "source": "mymemory", "at": "2026-10-21T18:00:00Z"}
        }}"#,
    );
    let mymemory = FakeMyMemory::default();
    let mut translator = into_english(&store, &mymemory);

    let alerts = translator.translate(
        issued(&[
            aviso(1, ("Tempestade", Some(38)), ("Perigo", Some(7))),
            aviso(2, ("Vendaval", None), ("Perigo Potencial", Some(6))),
            aviso(3, ("NEVOEIRO", None), ("Perigo", Some(7))),
        ]),
        Instant::now(),
    );

    assert_eq!(
        alerts.iter().map(shown).collect::<Vec<_>>(),
        [
            (
                "THUNDERSTORM",
                "DANGER",
                Some("TEMPESTADE"),
                Some("PERIGO"),
                false
            ),
            (
                "GALE",
                "POTENTIAL DANGER",
                Some("VENDAVAL"),
                Some("PERIGO POTENCIAL"),
                false
            ),
            ("FOG", "DANGER", Some("NEVOEIRO"), Some("PERIGO"), true),
        ]
    );
    assert!(mymemory.asked().is_empty());
}

#[test]
fn an_unknown_term_is_asked_once_and_stored_as_automatic() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    let french = json!({"Tempestade": {"text": "ORAGE", "auto": false, "source": "", "at": ""}});
    write_store(&store, &json!({"pt-BR|fr-FR": french}).to_string());
    let mymemory = FakeMyMemory::replying([translated(" Dry fog ")]);
    let mut translator = into_english(&store, &mymemory);
    let twice = issued(&[
        aviso(1, (DRY_FOG, None), ("Perigo", Some(7))),
        aviso(2, ("névoa seca", None), ("Perigo Potencial", Some(6))),
    ]);

    let alerts = translator.translate(twice.clone(), Instant::now());

    assert_eq!(
        alerts.iter().map(shown).collect::<Vec<_>>(),
        [
            (
                "DRY FOG",
                "DANGER",
                Some("NÉVOA SECA"),
                Some("PERIGO"),
                true
            ),
            (
                "DRY FOG",
                "POTENTIAL DANGER",
                Some("NÉVOA SECA"),
                Some("PERIGO POTENCIAL"),
                true
            ),
        ]
    );
    assert_eq!(mymemory.asked(), [DRY_FOG_URL]);
    let saved = read_store(&store);
    let entry = &saved["pt-BR|en-US"][DRY_FOG];
    assert_eq!(
        (&entry["text"], &entry["auto"], &entry["source"]),
        (&json!("DRY FOG"), &json!(true), &json!("mymemory"))
    );
    assert!(
        entry["at"]
            .as_str()
            .unwrap()
            .parse::<jiff::Timestamp>()
            .is_ok()
    );
    assert_eq!(saved["pt-BR|fr-FR"], french);

    translator.translate(twice.clone(), Instant::now());
    let restarted = FakeMyMemory::default();
    let again = into_english(&store, &restarted).translate(twice, Instant::now());

    assert_eq!(mymemory.asked().len(), 1);
    assert!(restarted.asked().is_empty());
    assert_eq!(shown(&again[0]).0, "DRY FOG");
}

#[test]
fn a_failed_answer_keeps_inmets_words_and_is_asked_again_only_after_an_hour() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    let mymemory =
        FakeMyMemory::replying([Err(ureq::Error::StatusCode(503)), translated("Dry fog")]);
    let mut translator = into_english(&store, &mymemory);
    let alerts = issued(&[aviso(1, (DRY_FOG, None), ("Perigo", Some(7)))]);
    let start = Instant::now();

    let failed = translator.translate(alerts.clone(), start);
    let within_the_hour = translator.translate(alerts.clone(), start + 59 * MINUTE);

    for alert in [&failed[0], &within_the_hour[0]] {
        assert_eq!(
            shown(alert),
            ("NÉVOA SECA", "DANGER", None, Some("PERIGO"), false)
        );
    }
    assert_eq!(mymemory.asked().len(), 1);
    assert!(!store.exists());

    let after_the_hour = translator.translate(alerts, start + 61 * MINUTE);

    assert_eq!(mymemory.asked().len(), 2);
    assert_eq!(shown(&after_the_hour[0]).0, "DRY FOG");
}

#[test]
fn answers_that_are_not_translations_count_as_failures() {
    let quota = json!({
        "responseData": {"translatedText": "MYMEMORY WARNING: YOU USED ALL AVAILABLE FREE TRANSLATIONS FOR TODAY."},
        "responseStatus": 429
    });
    let bad_pair = json!({
        "responseData": {"translatedText": "INVALID LANGUAGE PAIR SPECIFIED."},
        "responseStatus": "403"
    });
    let no_data = json!({"responseData": null, "responseStatus": 200});
    for reply in [
        Ok(quota.to_string()),
        Ok(bad_pair.to_string()),
        Ok(no_data.to_string()),
        translated("  "),
        translated("NEVOA seca"),
        Ok("<html>busy</html>".to_owned()),
    ] {
        let dir = TempDir::new().unwrap();
        let store = store_in(&dir);
        let mymemory = FakeMyMemory::replying([reply]);

        let alerts = into_english(&store, &mymemory).translate(
            issued(&[aviso(1, (DRY_FOG, None), ("Perigo", Some(7)))]),
            Instant::now(),
        );

        assert_eq!(
            shown(&alerts[0]),
            ("NÉVOA SECA", "DANGER", None, Some("PERIGO"), false)
        );
        assert!(!store.exists());
    }
}

#[test]
fn a_corrupt_store_is_treated_as_empty_and_left_as_it_is() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    let corrupt = "{\"pt-BR|en-US\": {\"Vendaval\": ";
    write_store(&store, corrupt);
    let mymemory = FakeMyMemory::replying([translated("Dry fog")]);
    let mut translator = into_english(&store, &mymemory);
    let alerts = issued(&[aviso(1, (DRY_FOG, None), ("Perigo", Some(7)))]);

    let first = translator.translate(alerts.clone(), Instant::now());
    let second = translator.translate(alerts, Instant::now());

    for alert in [&first[0], &second[0]] {
        assert_eq!(
            shown(alert),
            (
                "DRY FOG",
                "DANGER",
                Some("NÉVOA SECA"),
                Some("PERIGO"),
                true
            )
        );
    }
    assert_eq!(mymemory.asked(), [DRY_FOG_URL]);
    assert_eq!(fs::read_to_string(&store).unwrap(), corrupt);
}

#[test]
fn an_empty_event_is_never_sent() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::default();

    let alerts = into_english(&store_in(&dir), &mymemory).translate(
        issued(&[aviso(1, ("", None), ("Perigo", Some(7)))]),
        Instant::now(),
    );

    assert_eq!(
        shown(&alerts[0]),
        ("", "DANGER", None, Some("PERIGO"), false)
    );
}

#[test]
fn a_language_without_a_glossary_asks_the_translator_in_that_language() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::replying([translated("Orage"), translated("Danger")]);
    let mut translator =
        Translator::new("pt-BR", "fr-FR", store_in(&dir), mymemory.clone()).unwrap();

    let alerts = translator.translate(
        issued(&[aviso(1, ("Tempestade", Some(38)), ("Perigo", Some(7)))]),
        Instant::now(),
    );

    assert_eq!(
        shown(&alerts[0]),
        ("ORAGE", "DANGER", Some("TEMPESTADE"), Some("PERIGO"), true)
    );
    assert_eq!(
        mymemory.asked(),
        [
            "https://api.mymemory.translated.net/get?q=Tempestade&langpair=pt-BR%7Cfr-FR",
            "https://api.mymemory.translated.net/get?q=Perigo&langpair=pt-BR%7Cfr-FR"
        ]
    );
}

#[test]
fn only_inmets_own_language_is_translated_from() {
    let dir = TempDir::new().unwrap();
    for from in ["es-ES", "pt-br", "pt"] {
        let refused = Translator::new(from, "en-US", store_in(&dir), FakeMyMemory::default());

        assert_eq!(
            refused.err().map(|error| error.to_string()),
            Some(format!("INMET writes in pt-BR, not {from}"))
        );
    }
}
