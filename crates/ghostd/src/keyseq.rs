//! Qt key combinations, in the integer form KGlobalAccel takes over D-Bus.

use std::ops::RangeInclusive;

const SEPARATOR: char = '+';
const MODIFIERS: [(&str, i32); 4] = [
    ("shift", 0x0200_0000),
    ("ctrl", 0x0400_0000),
    ("alt", 0x0800_0000),
    ("meta", 0x1000_0000),
];
const NAMED_KEYS: [(&str, i32); 4] = [
    ("space", 0x20),
    ("tab", 0x0100_0001),
    ("return", 0x0100_0004),
    ("escape", 0x0100_0000),
];
const FUNCTION_KEY_PREFIX: [char; 2] = ['f', 'F'];
const FUNCTION_KEYS: RangeInclusive<i32> = 1..=24;
const KEY_F1: i32 = 0x0100_0030;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot parse {0:?} as a key sequence")]
pub struct KeySeqError(String);

/// Parses one combination such as `Meta+Shift+M`: modifiers joined by `+`, then exactly one key.
/// Names may be in any case.
pub fn parse(s: &str) -> Result<i32, KeySeqError> {
    let mut names = s.split(SEPARATOR).rev();
    let key = names.next().and_then(key_code);
    let modifiers = names.try_fold(0, |all, name| Some(all | modifier_bit(name)?));
    match (key, modifiers) {
        (Some(key), Some(modifiers)) => Ok(modifiers | key),
        _ => Err(KeySeqError(s.to_owned())),
    }
}

fn modifier_bit(name: &str) -> Option<i32> {
    MODIFIERS
        .iter()
        .find(|(modifier, _)| name.eq_ignore_ascii_case(modifier))
        .map(|&(_, bit)| bit)
}

fn key_code(name: &str) -> Option<i32> {
    if let Some(&(_, code)) = NAMED_KEYS
        .iter()
        .find(|(key, _)| name.eq_ignore_ascii_case(key))
    {
        return Some(code);
    }
    match name.as_bytes() {
        // Qt's codes for letters and digits are their uppercase ASCII codes.
        [key] if key.is_ascii_alphanumeric() => Some(i32::from(key.to_ascii_uppercase())),
        _ => {
            let number: i32 = name.strip_prefix(FUNCTION_KEY_PREFIX)?.parse().ok()?;
            FUNCTION_KEYS
                .contains(&number)
                .then(|| KEY_F1 + number - FUNCTION_KEYS.start())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_sequences() {
        assert_eq!(parse("Meta+M").unwrap(), 0x1000_004D);
        assert_eq!(parse("Meta+Shift+M").unwrap(), 0x1200_004D);
        assert_eq!(parse("ctrl+alt+f12").unwrap(), 0x0D00_003B);
    }

    #[test]
    fn rejects_bad_key_sequences() {
        assert_eq!(
            parse("Meta+").unwrap_err().to_string(),
            "cannot parse \"Meta+\" as a key sequence"
        );
        assert!(parse("Meta+M+K").is_err());
        assert!(parse("Hyper+M").is_err());
    }
}
