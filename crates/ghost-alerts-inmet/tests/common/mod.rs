// Every test binary compiles this module but uses only part of it.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ghost_alerts_inmet::{Fetch, IssuedAlert, Translator, parse_alerts};
use ghost_proto::WeatherAlert;
use serde_json::{Value, json};
use tempfile::TempDir;

pub const AREA: &str = "3550308";
pub const SAMPLE: &str = include_str!("../fixtures/sample.json");
pub const MINUTE: Duration = Duration::from_secs(60);
/// A term the glossary lacks, with an accent and a space to encode.
pub const DRY_FOG: &str = "Névoa Seca";
pub const DRY_FOG_URL: &str =
    "https://api.mymemory.translated.net/get?q=N%C3%A9voa%20Seca&langpair=pt-BR%7Cen-US";
const STORE: &str = "ghost-shell/translations.json";

pub type Reply = Result<String, ureq::Error>;

/// Answers in MyMemory's place with `replies`, in order, and keeps every URL it is asked for.
#[derive(Clone, Default)]
pub struct FakeMyMemory {
    replies: Rc<RefCell<VecDeque<Reply>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

impl FakeMyMemory {
    pub fn replying(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self {
            replies: Rc::new(RefCell::new(replies.into_iter().collect())),
            ..Self::default()
        }
    }

    pub fn asked(&self) -> Vec<String> {
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
pub fn translated(text: &str) -> Reply {
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
pub fn aviso(id: u64, event: (&str, Option<u64>), severity: (&str, Option<u64>)) -> Value {
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

pub fn issued(avisos: &[Value]) -> Vec<IssuedAlert> {
    parse_alerts(&json!({"hoje": avisos, "futuro": []}).to_string(), AREA).unwrap()
}

pub fn store_in(dir: &TempDir) -> PathBuf {
    dir.path().join(STORE)
}

pub fn into_english<F: Fetch>(store: &Path, fetch: F) -> Translator<F> {
    Translator::new("pt-BR", "en-US", store.to_owned(), fetch).unwrap()
}

pub fn write_store(store: &Path, contents: &str) {
    fs::create_dir_all(store.parent().unwrap()).unwrap();
    fs::write(store, contents).unwrap();
}

pub fn read_store(store: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(store).unwrap()).unwrap()
}

/// Every publication of one fetch, first to last.
pub fn publications<F: Fetch>(
    translator: &mut Translator<F>,
    issued: &[IssuedAlert],
    now: Instant,
) -> Vec<Vec<WeatherAlert>> {
    let mut published = Vec::new();
    translator
        .publish_translated(issued, now, |alerts| {
            published.push(alerts);
            Ok::<(), Infallible>(())
        })
        .unwrap();
    published
}

/// What the dashboard shows once one fetch is done: its last publication.
pub fn last<F: Fetch>(
    translator: &mut Translator<F>,
    issued: &[IssuedAlert],
    now: Instant,
) -> Vec<WeatherAlert> {
    publications(translator, issued, now).pop().unwrap()
}

/// Title, severity, their originals, and whether an online translator was involved.
pub fn shown(alert: &WeatherAlert) -> (&str, &str, Option<&str>, Option<&str>, bool) {
    (
        &alert.title,
        &alert.severity,
        alert.original_title.as_deref(),
        alert.original_severity.as_deref(),
        alert.auto_translated,
    )
}

pub fn titles(alerts: &[WeatherAlert]) -> Vec<&str> {
    alerts.iter().map(|alert| alert.title.as_str()).collect()
}
