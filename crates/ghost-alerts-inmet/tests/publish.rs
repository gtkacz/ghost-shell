mod common;

use std::convert::Infallible;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use common::{
    DRY_FOG, DRY_FOG_URL, FakeMyMemory, MINUTE, Reply, aviso, into_english, issued, last,
    publications, read_store, store_in, titles, translated, write_store,
};
use ghost_alerts_inmet::{Fetch, IssuedAlert};
use ghost_proto::WeatherAlert;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn the_first_publication_comes_before_any_online_request() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::replying([translated("Dry fog")]);
    let mut translator = into_english(&store_in(&dir), mymemory.clone());
    let alerts = issued(&[
        aviso(1, ("Tempestade", Some(38)), ("Perigo", Some(7))),
        aviso(2, (DRY_FOG, None), ("Perigo", Some(7))),
    ]);
    let mut published: Vec<(usize, Vec<WeatherAlert>)> = Vec::new();

    translator
        .publish_translated(&alerts, Instant::now(), |alerts| {
            published.push((mymemory.asked().len(), alerts));
            Ok::<(), Infallible>(())
        })
        .unwrap();

    let published: Vec<(usize, Vec<&str>)> = published
        .iter()
        .map(|(asked, alerts)| (*asked, titles(alerts)))
        .collect();
    assert_eq!(
        published,
        [
            (0, vec!["STORM", "NÉVOA SECA"]),
            (1, vec!["STORM", "DRY FOG"])
        ]
    );
}

#[test]
fn only_a_fetch_that_learns_something_publishes_again_and_only_once() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::replying([translated("Dry fog"), translated("Fog")]);
    let mut translator = into_english(&store_in(&dir), mymemory.clone());
    let alerts = issued(&[
        aviso(1, (DRY_FOG, None), ("Perigo", Some(7))),
        aviso(2, ("Nevoeiro", None), ("Perigo", Some(7))),
        aviso(3, ("Tempestade", Some(38)), ("Perigo", Some(7))),
    ]);
    let now = Instant::now();

    let learning = publications(&mut translator, &alerts, now);
    let known = publications(&mut translator, &alerts, now);
    let none = publications(&mut translator, &[], now);

    assert_eq!(learning.len(), 2);
    assert_eq!(titles(&learning[0]), ["NÉVOA SECA", "NEVOEIRO", "STORM"]);
    assert_eq!(titles(&learning[1]), ["DRY FOG", "FOG", "STORM"]);
    assert_eq!(known, [learning[1].clone()]);
    assert_eq!(none, [Vec::<WeatherAlert>::new()]);
    assert_eq!(mymemory.asked().len(), 2);
}

#[test]
fn a_failure_stops_the_fetchs_other_requests_for_the_hour() {
    let dir = TempDir::new().unwrap();
    let mymemory = FakeMyMemory::replying([
        Err(ureq::Error::StatusCode(503)),
        translated("Dry fog"),
        translated("Fog"),
        translated("Haze"),
    ]);
    let mut translator = into_english(&store_in(&dir), mymemory.clone());
    let alerts = issued(&[
        aviso(1, (DRY_FOG, None), ("Perigo", Some(7))),
        aviso(2, ("Nevoeiro", None), ("Perigo", Some(7))),
        aviso(3, ("Bruma", None), ("Perigo", Some(7))),
    ]);
    let start = Instant::now();

    let failing = publications(&mut translator, &alerts, start);
    let within_the_hour = publications(&mut translator, &alerts, start + 59 * MINUTE);

    assert_eq!(mymemory.asked(), [DRY_FOG_URL]);
    for published in [&failing, &within_the_hour] {
        assert_eq!(published.len(), 1);
        assert_eq!(titles(&published[0]), ["NÉVOA SECA", "NEVOEIRO", "BRUMA"]);
    }

    let recovered = last(&mut translator, &alerts, start + 61 * MINUTE);

    assert_eq!(mymemory.asked().len(), 4);
    assert_eq!(titles(&recovered), ["DRY FOG", "FOG", "HAZE"]);
}

/// Answers as `replies` does, but first saves `edited` over the store, as a user editing it while
/// the request is out would.
struct EditedDuringTheRequest {
    store: PathBuf,
    edited: String,
    replies: FakeMyMemory,
}

impl Fetch for EditedDuringTheRequest {
    fn get(&self, url: &str) -> Reply {
        fs::write(&self.store, &self.edited).unwrap();
        self.replies.get(url)
    }
}

#[test]
fn an_edit_made_during_a_request_survives_the_save() {
    let dir = TempDir::new().unwrap();
    let store = store_in(&dir);
    write_store(
        &store,
        r#"{"pt-BR|en-US": {"Vendaval": {"text": "WINDSTORM", "auto": true}}}"#,
    );
    let checked = json!({"text": "HAZE", "auto": false, "source": "", "at": ""});
    let corrected = json!({"text": "WIND GUSTS", "auto": false, "source": "", "at": ""});
    let edited = json!({"pt-BR|en-US": {"Vendaval": corrected, "névoa seca": checked}});
    let replies = FakeMyMemory::replying([translated("Dry fog"), translated("Fog")]);
    let mut translator = into_english(
        &store,
        EditedDuringTheRequest {
            store: store.clone(),
            edited: edited.to_string(),
            replies: replies.clone(),
        },
    );
    let alerts: Vec<IssuedAlert> = issued(&[
        aviso(1, (DRY_FOG, None), ("Perigo", Some(7))),
        aviso(2, ("Nevoeiro", None), ("Perigo", Some(7))),
    ]);

    last(&mut translator, &alerts, Instant::now());

    let saved = &read_store(&store)["pt-BR|en-US"];
    assert_eq!(saved["Vendaval"], corrected);
    assert_eq!(saved["névoa seca"], checked);
    assert!(saved.get(DRY_FOG).is_none(), "{saved}");
    assert_eq!(
        (&saved["Nevoeiro"]["text"], &saved["Nevoeiro"]["auto"]),
        (&json!("FOG"), &json!(true))
    );

    let next = last(&mut translator, &alerts, Instant::now());

    assert_eq!(titles(&next), ["HAZE", "FOG"]);
    assert_eq!(replies.asked().len(), 2);
}
