//! The built-in translations of INMET's wording, by target language.

use crate::translate::normalise;

/// One of INMET's terms and its translation.
pub(crate) struct Term {
    /// INMET's number for the term, listed only once it has been seen in INMET's feed.
    pub id: Option<u64>,
    pub portuguese: &'static str,
    pub translation: &'static str,
}

pub(crate) struct Glossary {
    /// The language tag the translations are in.
    pub to: &'static str,
    /// By `id_severidade`.
    pub severities: &'static [Term],
    /// By `id_condicao_severa`.
    pub events: &'static [Term],
}

// INMET names only a short, fixed set of events and three severities, so a reviewed table covers
// nearly every alert, and only a term it lacks is ever sent to the online translator. A term is
// found by its number first, in case INMET rewords it, then by its text. Grande Perigo's number is
// left out: 8 would follow from 6 and 7, but it has not been seen, and a wrong number would show
// the wrong severity where its text alone is safe.
pub(crate) const GLOSSARIES: &[Glossary] = &[Glossary {
    to: "en-US",
    severities: &[
        Term {
            id: Some(6),
            portuguese: "Perigo Potencial",
            translation: "POTENTIAL DANGER",
        },
        Term {
            id: Some(7),
            portuguese: "Perigo",
            translation: "DANGER",
        },
        Term {
            id: None,
            portuguese: "Grande Perigo",
            translation: "GREAT DANGER",
        },
    ],
    events: &[
        Term {
            id: Some(38),
            portuguese: "Tempestade",
            translation: "STORM",
        },
        Term {
            id: Some(24),
            portuguese: "Chuvas Intensas",
            translation: "HEAVY RAIN",
        },
        Term {
            id: None,
            portuguese: "Acumulado de Chuva",
            translation: "RAIN ACCUMULATION",
        },
        Term {
            id: None,
            portuguese: "Vendaval",
            translation: "GALE",
        },
        Term {
            id: None,
            portuguese: "Onda de Calor",
            translation: "HEAT WAVE",
        },
        Term {
            id: Some(23),
            portuguese: "Baixa Umidade",
            translation: "LOW HUMIDITY",
        },
        Term {
            id: None,
            portuguese: "Declínio de Temperatura",
            translation: "TEMPERATURE DROP",
        },
        Term {
            id: None,
            portuguese: "Geada",
            translation: "FROST",
        },
        Term {
            id: None,
            portuguese: "Ressaca",
            translation: "STORM SURGE",
        },
        Term {
            id: None,
            portuguese: "Ventos Costeiros",
            translation: "COASTAL WINDS",
        },
        Term {
            id: None,
            portuguese: "Neve",
            translation: "SNOW",
        },
        Term {
            id: None,
            portuguese: "Granizo",
            translation: "HAIL",
        },
    ],
}];

/// The glossary that translates into `to`, if one is built in.
pub(crate) fn glossary(to: &str) -> Option<&'static Glossary> {
    GLOSSARIES.iter().find(|glossary| glossary.to == to)
}

/// The translation of the term numbered `id`, or else of the one whose text normalises to `key`.
pub(crate) fn find(terms: &'static [Term], id: Option<u64>, key: &str) -> Option<&'static str> {
    id.and_then(|id| terms.iter().find(|term| term.id == Some(id)))
        .or_else(|| terms.iter().find(|term| normalise(term.portuguese) == key))
        .map(|term| term.translation)
}
