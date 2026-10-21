//! Translating INMET's event and severity, from the user's checked entries, the built-in glossary,
//! the online translator's stored answers, and last the online translator, MyMemory.

use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ghost_proto::WeatherAlert;
use jiff::Timestamp;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use serde_json::Value;
use tracing::warn;
use ureq::Agent;

use crate::glossary::{self, Glossary, Term};
use crate::store::{self, Store, Stored};
use crate::{IssuedAlert, Wording, null_as_default};

/// The language INMET writes in.
pub const SOURCE_LANGUAGE: &str = "pt-BR";
const PAIR_SEPARATOR: char = '|';
const MYMEMORY_URL: &str = "https://api.mymemory.translated.net/get";
const MYMEMORY_SOURCE: &str = "mymemory";
const MYMEMORY_OK: u64 = 200;
/// RFC 3986's unreserved characters stay as they are; everything else is percent-encoded.
const QUERY_VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
// So that a failing or rate-limited translator is not asked about the same term every fetch.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// Unicode's Combining Diacritical Marks, which carry the accents of text written decomposed.
const COMBINING_MARKS: RangeInclusive<char> = '\u{0300}'..='\u{036F}';

/// A blocking HTTP GET, behind a trait so that tests can answer instead of MyMemory.
pub trait Fetch {
    /// The body of a successful response to a GET of `url`.
    fn get(&self, url: &str) -> Result<String, ureq::Error>;
}

impl Fetch for Agent {
    fn get(&self, url: &str) -> Result<String, ureq::Error> {
        Agent::get(self, url).call()?.body_mut().read_to_string()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("INMET writes in {SOURCE_LANGUAGE}, not {0}")]
pub struct UnsupportedSource(String);

#[derive(Debug, thiserror::Error)]
enum AskError {
    #[error("MyMemory cannot be reached: {0}")]
    Http(#[from] ureq::Error),
    #[error("MyMemory's answer is not a translation: {0}")]
    Json(#[from] serde_json::Error),
    #[error("MyMemory answered with status {0}")]
    Status(Value),
    #[error("MyMemory gave the term back untranslated")]
    Untranslated,
}

/// MyMemory's answer; only what is used is read.
#[derive(Deserialize)]
struct Answer {
    /// Left untyped because MyMemory sends some error statuses as strings.
    #[serde(rename = "responseStatus")]
    status: Value,
    #[serde(rename = "responseData", default, deserialize_with = "null_as_default")]
    data: AnswerData,
}

#[derive(Default, Deserialize)]
struct AnswerData {
    #[serde(
        rename = "translatedText",
        default,
        deserialize_with = "null_as_default"
    )]
    translated_text: String,
}

struct Found {
    text: String,
    /// It came from the online translator, now or through an entry nobody has checked.
    auto: bool,
}

/// Translates INMET's event and severity from pt-BR into one language.
pub struct Translator<F> {
    /// Such as `pt-BR|en-US`: the store's key for these translations, and MyMemory's `langpair`.
    pair: String,
    glossary: Option<&'static Glossary>,
    store: PathBuf,
    fetch: F,
    /// The online translator's answers in this run, by normalised term, so that a store that
    /// cannot be read or written still costs each term one request.
    learned: HashMap<String, String>,
    /// When the online translator last failed on each term, by normalised term.
    failed: HashMap<String, Instant>,
}

impl<F: Fetch> Translator<F> {
    /// Translates into `to` with the store at `store`, asking `fetch` for what neither the store
    /// nor the glossary knows. Only `from` = pt-BR is accepted, the language INMET writes in.
    pub fn new(from: &str, to: &str, store: PathBuf, fetch: F) -> Result<Self, UnsupportedSource> {
        if from != SOURCE_LANGUAGE {
            return Err(UnsupportedSource(from.to_owned()));
        }
        Ok(Self {
            pair: format!("{from}{PAIR_SEPARATOR}{to}"),
            glossary: glossary::glossary(to),
            store,
            fetch,
            learned: HashMap::new(),
            failed: HashMap::new(),
        })
    }

    /// Hands `publish` the alerts of `issued` with their event and severity translated from what
    /// is already known, then asks the online translator about the terms nothing knows and, if it
    /// learned any, hands `publish` the alerts again. A slow or unreachable translator therefore
    /// never holds the alerts back.
    ///
    /// A term is translated by the first of: a store entry the user has checked, the built-in
    /// glossary, a store entry the online translator gave, and an answer it gave earlier in this
    /// run. The translator is asked about a term only when none of these has it and it has not
    /// failed on that term within the hour; its answers are stored. The store is read again at
    /// each call that has alerts, so the user's edits apply at the next fetch.
    pub fn publish_translated<E>(
        &mut self,
        issued: &[IssuedAlert],
        now: Instant,
        mut publish: impl FnMut(Vec<WeatherAlert>) -> Result<(), E>,
    ) -> Result<(), E> {
        if issued.is_empty() {
            return publish(Vec::new());
        }
        let store = Store::open(&self.store);
        publish(self.alerts(issued, &store))?;
        if self.learn(issued, &store, now) {
            publish(self.alerts(issued, &store))?;
        }
        Ok(())
    }

    fn alerts(&self, issued: &[IssuedAlert], store: &Store) -> Vec<WeatherAlert> {
        let (events, severities) = self.terms();
        issued
            .iter()
            .map(|issued| {
                let title = self.known(&issued.event, events, store);
                let severity = self.known(&issued.severity, severities, store);
                translated(&issued.alert, title, severity)
            })
            .collect()
    }

    /// The glossary's events and severities, or none when no glossary translates into `to`.
    fn terms(&self) -> (&'static [Term], &'static [Term]) {
        self.glossary.map_or((&[], &[]), |glossary| {
            (glossary.events, glossary.severities)
        })
    }

    /// The translation of `wording` from what is already known, or `None` to keep INMET's words.
    fn known(&self, wording: &Wording, terms: &'static [Term], store: &Store) -> Option<Found> {
        if wording.text.is_empty() {
            return None;
        }
        let key = normalise(&wording.text);
        let stored = store.find(&self.pair, &key);
        if let Some(checked) = stored.filter(|stored| !stored.auto) {
            return Some(Found {
                text: checked.text.to_uppercase(),
                auto: false,
            });
        }
        if let Some(text) = glossary::find(terms, wording.id, &key) {
            return Some(Found {
                text: text.to_owned(),
                auto: false,
            });
        }
        stored
            .map(|stored| stored.text.to_uppercase())
            .or_else(|| self.learned.get(&key).cloned())
            .map(|text| Found { text, auto: true })
    }

    /// Asks the online translator about each term of `issued` that nothing knows, once, and
    /// stores its answers; true if it gave any. After its first failure the fetch's other unknown
    /// terms count as failed too, so an unreachable translator costs one request per fetch.
    fn learn(&mut self, issued: &[IssuedAlert], store: &Store, now: Instant) -> bool {
        let unknown = self.unknown(issued, store, now);
        let mut learned = Vec::new();
        for (index, (key, text)) in unknown.iter().enumerate() {
            match self.online(text) {
                Ok(translation) => {
                    let stored = Stored {
                        text: translation.clone(),
                        auto: true,
                        source: MYMEMORY_SOURCE.to_owned(),
                        at: format!("{:.0}", Timestamp::now()),
                    };
                    learned.push((text.clone(), stored));
                    self.failed.remove(key);
                    self.learned.insert(key.clone(), translation);
                }
                Err(error) => {
                    warn!(
                        term = %text,
                        %error,
                        unasked = unknown.len() - index - 1,
                        "showing INMET's words; these terms are asked again in an hour at the \
                         earliest"
                    );
                    for (key, _) in &unknown[index..] {
                        self.failed.insert(key.clone(), now);
                    }
                    break;
                }
            }
        }
        if learned.is_empty() {
            return false;
        }
        store::merge(&self.store, &self.pair, learned);
        true
    }

    /// The terms of `issued` that nothing knows and that the online translator has not failed on
    /// within the hour, each once, as its normalised key and INMET's text, in the order they come.
    fn unknown(
        &self,
        issued: &[IssuedAlert],
        store: &Store,
        now: Instant,
    ) -> Vec<(String, String)> {
        let (events, severities) = self.terms();
        let mut unknown: Vec<(String, String)> = Vec::new();
        let wordings = issued
            .iter()
            .flat_map(|issued| [(&issued.event, events), (&issued.severity, severities)]);
        for (wording, terms) in wordings {
            if wording.text.is_empty() || self.known(wording, terms, store).is_some() {
                continue;
            }
            let key = normalise(&wording.text);
            let failed_lately = self
                .failed
                .get(&key)
                .is_some_and(|failed| now.duration_since(*failed) < RETRY_AFTER);
            if !failed_lately && unknown.iter().all(|(seen, _)| *seen != key) {
                unknown.push((key, wording.text.clone()));
            }
        }
        unknown
    }

    /// MyMemory's translation of `text`, uppercased as the rest of the alert line is.
    fn online(&self, text: &str) -> Result<String, AskError> {
        let body = self.fetch.get(&mymemory_url(text, &self.pair))?;
        let answer: Answer = serde_json::from_str(&body)?;
        if answer.status.as_u64() != Some(MYMEMORY_OK) {
            return Err(AskError::Status(answer.status));
        }
        let translated = answer.data.translated_text.trim();
        if translated.is_empty() || normalise(translated) == normalise(text) {
            return Err(AskError::Untranslated);
        }
        Ok(translated.to_uppercase())
    }
}

/// `alert` showing `title` and `severity` where they were found, each with the issuer's words it
/// replaces.
fn translated(alert: &WeatherAlert, title: Option<Found>, severity: Option<Found>) -> WeatherAlert {
    let auto_translated = [&title, &severity]
        .into_iter()
        .flatten()
        .any(|found| found.auto);
    let (title, original_title) = shown(&alert.title, title);
    let (severity, original_severity) = shown(&alert.severity, severity);
    WeatherAlert {
        title,
        severity,
        original_title,
        original_severity,
        auto_translated,
        ..alert.clone()
    }
}

/// What a field shows: its translation and the issuer's words it replaces, or those words alone.
fn shown(issued: &str, translation: Option<Found>) -> (String, Option<String>) {
    match translation {
        Some(found) => (found.text, Some(issued.to_owned())),
        None => (issued.to_owned(), None),
    }
}

/// The request for `text` alone: no key, no email, nothing about the user.
fn mymemory_url(text: &str, pair: &str) -> String {
    format!(
        "{MYMEMORY_URL}?q={}&langpair={}",
        utf8_percent_encode(text, QUERY_VALUE),
        utf8_percent_encode(pair, QUERY_VALUE)
    )
}

/// `text` with its spaces collapsed, lowercased and without accents, so that ` TEMPESTADE `
/// matches `Tempestade` and `DECLINIO  DE TEMPERATURA` matches `Declínio de Temperatura`.
pub(crate) fn normalise(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            word.to_lowercase()
                .chars()
                .filter(|letter| !COMBINING_MARKS.contains(letter))
                .map(unaccented)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// Portuguese marks letters with only these accents, so this table, with the combining marks of
// decomposed text dropped, stands in for Unicode decomposition without a new crate.
fn unaccented(letter: char) -> char {
    match letter {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        other => other,
    }
}
