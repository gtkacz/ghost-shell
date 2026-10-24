//! Reduces a notification body to the markup subset the shell renders, and to plain text.

const ALLOWED_TAGS: [&str; 4] = ["b", "i", "u", "a"];
const LONGEST_ENTITY: usize = 10;

/// Splits a body into its sanitised markup and its plain text.
///
/// The markup keeps `b`, `i`, `u` and `a` with only an `href`; any other tag is dropped and its
/// text kept. A body with an unterminated tag is not parsed at all: the markup escapes it whole
/// and the plain text is the body as sent.
pub fn sanitise(body: &str) -> (String, String) {
    scan(body).unwrap_or_else(|| (escape(body), body.to_owned()))
}

fn scan(body: &str) -> Option<(String, String)> {
    let mut markup = String::with_capacity(body.len());
    let mut text = String::with_capacity(body.len());
    let mut open: Vec<&str> = Vec::new();
    let mut rest = body;
    while let Some(c) = rest.chars().next() {
        let after = &rest[c.len_utf8()..];
        if c == '<' && after.starts_with(|n: char| n.is_ascii_alphabetic() || n == '/') {
            let end = tag_end(after)?;
            emit_tag(&after[..end], &mut markup, &mut open);
            rest = &after[end + 1..];
        } else if let Some((decoded, length)) = c.eq(&'&').then(|| entity(after)).flatten() {
            text.push(decoded);
            push_escaped(&mut markup, decoded);
            rest = &after[length..];
        } else {
            text.push(c);
            push_escaped(&mut markup, c);
            rest = after;
        }
    }
    for name in open.into_iter().rev() {
        markup.push_str(&format!("</{name}>"));
    }
    Some((markup, text))
}

/// The byte offset of the `>` that closes a tag, skipping any inside quotes.
fn tag_end(after_open: &str) -> Option<usize> {
    let mut quote = None;
    for (at, c) in after_open.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(at),
            (None, _) => {}
        }
    }
    None
}

fn emit_tag(inner: &str, markup: &mut String, open: &mut Vec<&'static str>) {
    if let Some(closing) = inner.strip_prefix('/') {
        let name = closing.trim().to_ascii_lowercase();
        if open.last().is_some_and(|top| *top == name) {
            open.pop();
            markup.push_str(&format!("</{name}>"));
        }
        return;
    }
    let name_end = inner
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(inner.len());
    let (name, attributes) = inner.split_at(name_end);
    let Some(tag) = ALLOWED_TAGS
        .into_iter()
        .find(|tag| tag.eq_ignore_ascii_case(name))
    else {
        return;
    };
    if tag == "a" {
        if let Some(href) = only_href(attributes) {
            markup.push_str(&format!(
                "<a href=\"{}\">",
                escape(&href).replace('"', "&quot;")
            ));
            open.push(tag);
        }
    } else if attributes.trim().is_empty() {
        markup.push_str(&format!("<{tag}>"));
        open.push(tag);
    }
}

/// The decoded value of `href="…"` when it is the tag's only attribute.
fn only_href(attributes: &str) -> Option<String> {
    let value = attributes
        .trim()
        .strip_prefix("href")?
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let quoted = &value[1..];
    let close = quoted.find(quote)?;
    if !quoted[close + 1..].trim().is_empty() {
        return None;
    }
    Some(decode(&quoted[..close]))
}

/// The character an entity at the start of `after_amp` names, and how many bytes it spans past
/// the `&`.
fn entity(after_amp: &str) -> Option<(char, usize)> {
    let end = after_amp
        .char_indices()
        .take(LONGEST_ENTITY)
        .find(|(_, c)| *c == ';')?
        .0;
    let decoded = match &after_amp[..end] {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        numeric => {
            let digits = numeric.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16),
                None => digits.parse(),
            };
            char::from_u32(code.ok()?).filter(|c| *c != '\0')?
        }
    };
    Some((decoded, end + 1))
}

fn decode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        let after = &rest[c.len_utf8()..];
        match c.eq(&'&').then(|| entity(after)).flatten() {
            Some((decoded, length)) => {
                out.push(decoded);
                rest = &after[length..];
            }
            None => {
                out.push(c);
                rest = after;
            }
        }
    }
    out
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        _ => out.push(c),
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    text.chars().for_each(|c| push_escaped(&mut out, c));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_keeps_only_the_allowed_tags() {
        assert_eq!(
            sanitise("<b>x</b><script>y</script><a href='u'>z</a>"),
            ("<b>x</b>y<a href=\"u\">z</a>".to_owned(), "xyz".to_owned())
        );
    }

    #[test]
    fn invalid_markup_is_escaped_whole() {
        assert_eq!(
            sanitise("a <b c"),
            ("a &lt;b c".to_owned(), "a <b c".to_owned())
        );
    }

    #[test]
    fn entities_are_decoded_in_plain_text() {
        assert_eq!(
            sanitise("&amp;&lt;"),
            ("&amp;&lt;".to_owned(), "&<".to_owned())
        );
    }

    #[test]
    fn numeric_entities_decode_and_unknown_ones_stay_literal() {
        assert_eq!(
            sanitise("&#65;&#x42;&bogus;"),
            ("AB&amp;bogus;".to_owned(), "AB&bogus;".to_owned())
        );
    }

    #[test]
    fn href_is_escaped_and_other_attributes_are_dropped() {
        assert_eq!(
            sanitise("<a href='a&amp;b\"c'>l</a>").0,
            "<a href=\"a&amp;b&quot;c\">l</a>"
        );
    }

    #[test]
    fn tags_with_attributes_other_than_href_are_dropped() {
        assert_eq!(sanitise("<b style=\"x\">t</b>").0, "t");
        assert_eq!(sanitise("<a onclick=\"x\" href=\"u\">t</a>").0, "t");
    }

    #[test]
    fn unbalanced_tags_are_closed_and_stray_closers_dropped() {
        assert_eq!(sanitise("</i><b>x<u>y").0, "<b>x<u>y</u></b>");
    }

    #[test]
    fn a_lone_angle_bracket_is_plain_text() {
        assert_eq!(
            sanitise("1 < 2 > 0"),
            ("1 &lt; 2 &gt; 0".to_owned(), "1 < 2 > 0".to_owned())
        );
    }
}
