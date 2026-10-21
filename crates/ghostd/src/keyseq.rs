//! Qt key combinations, in the integer form KGlobalAccel takes over D-Bus.

use std::ops::Range;

const SEPARATOR: char = '+';
const NO_MODIFIERS: i32 = 0;
const SHIFT: i32 = 0x0200_0000;
const MODIFIERS: [(&str, i32); 4] = [
    ("shift", SHIFT),
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
const LEADING_ZERO: char = '0';
const FUNCTION_KEY_COUNT: i32 = 24;
const KEY_F1: i32 = 0x0100_0030;
const FUNCTION_KEY_CODES: Range<i32> = KEY_F1..KEY_F1 + FUNCTION_KEY_COUNT;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeySeqError {
    #[error("cannot parse {0:?} as a key sequence")]
    Unparsable(String),
    #[error("{0:?} needs Meta, Ctrl or Alt")]
    NoMetaCtrlOrAlt(String),
}

/// Parses one combination such as `Meta+Shift+M`: modifiers joined by `+`, then exactly one key.
/// Names may be in any case, and only F1 to F24 may go without Meta, Ctrl or Alt.
pub fn parse(s: &str) -> Result<i32, KeySeqError> {
    let mut names = s.split(SEPARATOR).rev();
    let key = names.next().and_then(key_code);
    let modifiers = names.try_fold(NO_MODIFIERS, |all, name| Some(all | modifier_bit(name)?));
    let (Some(key), Some(modifiers)) = (key, modifiers) else {
        return Err(KeySeqError::Unparsable(s.to_owned()));
    };
    // KGlobalAccel takes its keys from every app, so a bare letter, or one with only Shift, could
    // no longer be typed.
    if modifiers & !SHIFT == NO_MODIFIERS && !FUNCTION_KEY_CODES.contains(&key) {
        return Err(KeySeqError::NoMetaCtrlOrAlt(s.to_owned()));
    }
    Ok(modifiers | key)
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
            let digits = name.strip_prefix(FUNCTION_KEY_PREFIX)?;
            if digits.starts_with(LEADING_ZERO) {
                return None;
            }
            let number: i32 = digits.parse().ok()?;
            (1..=FUNCTION_KEY_COUNT)
                .contains(&number)
                .then(|| KEY_F1 + number - 1)
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
        assert_eq!(parse("F5").unwrap(), 0x0100_0034);
        assert_eq!(parse("Meta+Space").unwrap(), 0x1000_0020);
    }

    #[test]
    fn rejects_bad_key_sequences() {
        assert_eq!(
            parse("Meta+").unwrap_err().to_string(),
            "cannot parse \"Meta+\" as a key sequence"
        );
        assert!(parse("Meta+M+K").is_err());
        assert!(parse("Hyper+M").is_err());
        assert!(parse("Meta+F01").is_err());
        assert!(parse("Meta+F25").is_err());
    }

    #[test]
    fn only_function_keys_may_go_without_meta_ctrl_or_alt() {
        assert_eq!(
            parse("M").unwrap_err().to_string(),
            "\"M\" needs Meta, Ctrl or Alt"
        );
        assert_eq!(
            parse("Shift+M").unwrap_err().to_string(),
            "\"Shift+M\" needs Meta, Ctrl or Alt"
        );
        assert!(parse("Space").is_err());
        assert_eq!(parse("Meta+Shift+M").unwrap(), 0x1200_004D);
        assert_eq!(parse("F24").unwrap(), 0x0100_0047);
        assert_eq!(parse("Shift+F5").unwrap(), 0x0300_0034);
    }
}
