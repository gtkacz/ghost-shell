mod common;

use std::fs;
use std::time::Instant;

use common::{
    AREA, DRY_FOG, DRY_FOG_URL, FakeMyMemory, MINUTE, SAMPLE, aviso, into_english, issued, last,
    read_store, shown, store_in, translated, write_store,
};
use ghost_alerts_inmet::{Translator, parse_alerts};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn the_glossary_translates_by_inmets_numbers() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::default();
    let mut translator = into_english(&store_in(&dir), mymemory.clone());

    let sample = last(
        &mut translator,
        &parse_alerts(SAMPLE, AREA).unwrap(),
        Instant::now(),
    );
    let reworded = last(
        &mut translator,
        &issued(&[aviso(
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
    let mut translator = into_english(&store_in(&dir), mymemory.clone());

    let alerts = last(
        &mut translator,
        &issued(&[
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
    let mut translator = into_english(&store_in(&dir), mymemory.clone());

    let alerts = last(
        &mut translator,
        &issued(&[
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
fn matching_collapses_inner_spaces_and_folds_decomposed_accents() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    write_store(
        &store,
        r#"{"pt-BR|en-US": {"Nevoeiro  Denso": {"text": "THICK FOG", "auto": false}}}"#,
    );
    let mymemory = FakeMyMemory::default();
    let mut translator = into_english(&store, mymemory.clone());

    let alerts = last(
        &mut translator,
        &issued(&[
            aviso(
                1,
                ("Acumulado  de\u{a0}Chuva", None),
                ("Perigo\u{a0}Potencial", None),
            ),
            aviso(
                2,
                ("Decli\u{301}nio de Temperatura", None),
                ("Perigo", None),
            ),
            aviso(3, ("nevoeiro denso", None), ("Perigo", None)),
        ]),
        Instant::now(),
    );

    let shown: Vec<(&str, &str)> = alerts
        .iter()
        .map(|alert| (alert.title.as_str(), alert.severity.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("RAIN ACCUMULATION", "POTENTIAL DANGER"),
            ("TEMPERATURE DROP", "DANGER"),
            ("THICK FOG", "DANGER"),
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
    let mut translator = into_english(&store, mymemory.clone());

    let alerts = last(
        &mut translator,
        &issued(&[
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
fn a_checked_entry_wins_among_entries_that_normalise_alike() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    // "NEVOEIRO" sorts first, so taking the first match would show MIST.
    write_store(
        &store,
        r#"{"pt-BR|en-US": {
            "NEVOEIRO": {"text": "MIST", "auto": true},
            "Nevoeiro": {"text": "FOG", "auto": false}
        }}"#,
    );
    let mymemory = FakeMyMemory::default();

    let alerts = last(
        &mut into_english(&store, mymemory.clone()),
        &issued(&[aviso(1, ("nevoeiro", None), ("Perigo", Some(7)))]),
        Instant::now(),
    );

    assert_eq!(
        shown(&alerts[0]),
        ("FOG", "DANGER", Some("NEVOEIRO"), Some("PERIGO"), false)
    );
    assert!(mymemory.asked().is_empty());
}

#[test]
fn an_entry_with_blank_text_counts_as_none() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    write_store(
        &store,
        r#"{"pt-BR|en-US": {
            "Tempestade": {"text": " ", "auto": false},
            "Nevoeiro": {"text": "", "auto": true}
        }}"#,
    );
    let mymemory = FakeMyMemory::replying([translated("Fog")]);

    let alerts = last(
        &mut into_english(&store, mymemory.clone()),
        &issued(&[
            aviso(1, ("Tempestade", Some(38)), ("Perigo", Some(7))),
            aviso(2, ("Nevoeiro", None), ("Perigo", Some(7))),
        ]),
        Instant::now(),
    );

    assert_eq!(
        alerts.iter().map(shown).collect::<Vec<_>>(),
        [
            ("STORM", "DANGER", Some("TEMPESTADE"), Some("PERIGO"), false),
            ("FOG", "DANGER", Some("NEVOEIRO"), Some("PERIGO"), true),
        ]
    );
    assert_eq!(
        mymemory.asked(),
        ["https://api.mymemory.translated.net/get?q=Nevoeiro&langpair=pt-BR%7Cen-US"]
    );
    assert_eq!(read_store(&store)["pt-BR|en-US"]["Nevoeiro"]["text"], "FOG");
}

#[test]
fn an_unknown_term_is_asked_once_and_stored_as_automatic() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    let french = json!({"Tempestade": {"text": "ORAGE", "auto": false, "source": "", "at": ""}});
    write_store(&store, &json!({"pt-BR|fr-FR": french}).to_string());
    let mymemory = FakeMyMemory::replying([translated(" Dry fog ")]);
    let mut translator = into_english(&store, mymemory.clone());
    let twice = issued(&[
        aviso(1, (DRY_FOG, None), ("Perigo", Some(7))),
        aviso(2, ("névoa seca", None), ("Perigo Potencial", Some(6))),
    ]);

    let alerts = last(&mut translator, &twice, Instant::now());

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

    last(&mut translator, &twice, Instant::now());
    let restarted = FakeMyMemory::default();
    let again = last(
        &mut into_english(&store, restarted.clone()),
        &twice,
        Instant::now(),
    );

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
    let mut translator = into_english(&store, mymemory.clone());
    let alerts = issued(&[aviso(1, (DRY_FOG, None), ("Perigo", Some(7)))]);
    let start = Instant::now();

    let failed = last(&mut translator, &alerts, start);
    let within_the_hour = last(&mut translator, &alerts, start + 59 * MINUTE);

    for alert in [&failed[0], &within_the_hour[0]] {
        assert_eq!(
            shown(alert),
            ("NÉVOA SECA", "DANGER", None, Some("PERIGO"), false)
        );
    }
    assert_eq!(mymemory.asked().len(), 1);
    assert!(!store.exists());

    let after_the_hour = last(&mut translator, &alerts, start + 61 * MINUTE);

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

        let alerts = last(
            &mut into_english(&store, FakeMyMemory::replying([reply])),
            &issued(&[aviso(1, (DRY_FOG, None), ("Perigo", Some(7)))]),
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
    let mut translator = into_english(&store, mymemory.clone());
    let alerts = issued(&[aviso(1, (DRY_FOG, None), ("Perigo", Some(7)))]);

    let first = last(&mut translator, &alerts, Instant::now());
    let second = last(&mut translator, &alerts, Instant::now());

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

    let alerts = last(
        &mut into_english(&store_in(&dir), FakeMyMemory::default()),
        &issued(&[aviso(1, ("", None), ("Perigo", Some(7)))]),
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

    let alerts = last(
        &mut translator,
        &issued(&[aviso(1, ("Tempestade", Some(38)), ("Perigo", Some(7)))]),
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
