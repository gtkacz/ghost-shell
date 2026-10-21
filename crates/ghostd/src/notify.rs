use std::collections::HashMap;
use std::time::Duration;

use tokio::time::timeout;
use tracing::warn;
use zbus::{Connection, Variant};

const APP_NAME: &str = "ghost-shell";
const DESTINATION: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";
const NOTIFY_METHOD: &str = "Notify";
const WARNING_ICON: &str = "dialog-warning";
// 0 asks for a new notification rather than replacing an earlier one.
const NEW_NOTIFICATION: u32 = 0;
// -1 leaves how long the popup stays to the notification server.
const SERVER_DEFAULT_TIMEOUT: i32 = -1;
// A notification server that hangs must not leave the warning task waiting forever.
const NOTIFY_TIMEOUT: Duration = Duration::from_secs(5);

/// Shows a desktop notification through `org.freedesktop.Notifications` on the session bus.
///
/// A failure, such as a session without a bus or a notification server, is logged and otherwise
/// ignored: a warning that cannot be shown must not take anything else down with it.
pub async fn send_warning(summary: &str, body: &str) {
    match timeout(NOTIFY_TIMEOUT, notify(summary, body)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(%error, summary, "cannot show a desktop notification"),
        Err(_elapsed) => warn!(
            summary,
            "the notification server did not answer within {NOTIFY_TIMEOUT:?}"
        ),
    }
}

/// The body is markup to a notification server, so a `&` or `<` in a path or a config snippet
/// would otherwise be swallowed or garble the very message that names the problem.
fn escape_markup(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

async fn notify(summary: &str, body: &str) -> zbus::Result<()> {
    // Warnings are rare, so a connection per warning is cheaper than holding one open for good.
    let connection = Connection::session().await?;
    let actions: Vec<&str> = Vec::new();
    let hints: HashMap<&str, Variant<'_>> = HashMap::new();
    connection
        .call_method(
            Some(DESTINATION),
            OBJECT_PATH,
            Some(INTERFACE),
            NOTIFY_METHOD,
            &(
                APP_NAME,
                NEW_NOTIFICATION,
                WARNING_ICON,
                summary,
                escape_markup(body),
                actions,
                hints,
                SERVER_DEFAULT_TIMEOUT,
            ),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_characters_are_escaped_ampersand_first() {
        assert_eq!(
            escape_markup("a & b < c > d &lt;"),
            "a &amp; b &lt; c &gt; d &amp;lt;"
        );
    }

    #[test]
    fn text_without_markup_is_unchanged() {
        let text = "layout.views.v1: unknown field \"sise\" / 攻殻";

        assert_eq!(escape_markup(text), text);
    }
}
