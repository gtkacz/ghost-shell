//! Translating INMET's event and severity, from the user's checked entries, the built-in glossary,
//! the online translator's stored answers, and last the online translator, MyMemory.

use std::collections::HashMap;
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
use crate::store::{Store, Stored};
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

    /// Each alert of `issued`, its event and severity translated where a translation is found:
    /// first a store entry the user has checked, then the built-in glossary, then a store entry
    /// the online translator gave, and last the online translator, asked about a term only when no
    /// answer is stored and it has not failed on that term within the hour. Its answers are stored.
    ///
    /// The store is read again at each call that has alerts, so the user's edits apply at the next
    /// fetch.
    pub fn translate(&mut self, issued: Vec<IssuedAlert>, now: Instant) -> Vec<WeatherAlert> {
        if issued.is_empty() {
            return Vec::new();
        }
        let mut store = Store::open(&self.store);
        let alerts = issued
            .into_iter()
            .map(|issued| self.alert(issued, &mut store, now))
            .collect();
        store.save();
        alerts
    }

    fn alert(&mut self, issued: IssuedAlert, store: &mut Store, now: Instant) -> WeatherAlert {
        let (events, severities): (&'static [Term], &'static [Term]) =
            self.glossary.map_or((&[], &[]), |glossary| {
                (glossary.events, glossary.severities)
            });
        let title = self.translation(&issued.event, events, store, now);
        let severity = self.translation(&issued.severity, severities, store, now);
        let auto_translated = [&title, &severity]
            .into_iter()
            .flatten()
            .any(|found| found.auto);
        let alert = issued.alert;
        let (title, original_title) = shown(alert.title, title);
        let (severity, original_severity) = shown(alert.severity, severity);
        WeatherAlert {
            title,
            severity,
            original_title,
            original_severity,
            auto_translated,
            ..alert
        }
    }

    /// The translation of `wording`, or `None` to keep INMET's words.
    fn translation(
        &mut self,
        wording: &Wording,
        terms: &'static [Term],
        store: &mut Store,
        now: Instant,
    ) -> Option<Found> {
        if wording.text.is_empty() {
            return None;
        }
        let key = normalise(&wording.text);
        let stored = store.find(&self.pair, &key).cloned();
        if let Some(checked) = stored.as_ref().filter(|stored| !stored.auto) {
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
        let known = stored
            .map(|stored| stored.text)
            .or_else(|| self.learned.get(&key).cloned());
        known
            .map(|text| text.to_uppercase())
            .or_else(|| self.ask(wording, key, store, now))
            .map(|text| Found { text, auto: true })
    }

    fn ask(
        &mut self,
        wording: &Wording,
        key: String,
        store: &mut Store,
        now: Instant,
    ) -> Option<String> {
        let failed_lately = self
            .failed
            .get(&key)
            .is_some_and(|failed| now.duration_since(*failed) < RETRY_AFTER);
        if failed_lately {
            return None;
        }
        match self.online(&wording.text) {
            Ok(text) => {
                let stored = Stored {
                    text: text.clone(),
                    auto: true,
                    source: MYMEMORY_SOURCE.to_owned(),
                    at: format!("{:.0}", Timestamp::now()),
                };
                store.insert(&self.pair, &wording.text, stored);
                self.failed.remove(&key);
                self.learned.insert(key, text.clone());
                Some(text)
            }
            Err(error) => {
                warn!(
                    term = %wording.text,
                    %error,
                    "showing INMET's words; the term is asked again in an hour at the earliest"
                );
                self.failed.insert(key, now);
                None
            }
        }
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

/// What a field shows: its translation and the issuer's words it replaces, or those words alone.
fn shown(issued: String, translation: Option<Found>) -> (String, Option<String>) {
    match translation {
        Some(found) => (found.text, Some(issued)),
        None => (issued, None),
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

/// `text` trimmed, lowercased and without accents, so that ` TEMPESTADE ` matches `Tempestade`
/// and `DECLINIO` matches `Declínio`.
pub(crate) fn normalise(text: &str) -> String {
    text.trim().to_lowercase().chars().map(unaccented).collect()
}

// Portuguese marks letters with only these accents, so a table stands in for Unicode decomposition.
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
