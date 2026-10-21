//! The HTTPS client the weather feed fetches with, behind a trait so that tests can answer instead.

use std::time::Duration;

use ureq::Agent;

// The user's own storm_alert.py gives Open-Meteo the same 15 s.
const TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("ghost-shell/", env!("CARGO_PKG_VERSION"));

/// Why a GET returned no body: no connection, a timeout, a status other than 2xx, and so on.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct FetchError(#[from] ureq::Error);

/// A blocking HTTP GET, so callers run it on a blocking thread.
pub trait Fetch: Send + Sync {
    /// The body of a successful response to a GET of `url`.
    fn get(&self, url: &str) -> Result<String, FetchError>;
}

/// Fetches with ureq over rustls, giving up on a request 15 s after it starts.
pub struct Ureq {
    agent: Agent,
}

impl Default for Ureq {
    fn default() -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .user_agent(USER_AGENT)
            .build()
            .new_agent();
        Self { agent }
    }
}

impl Fetch for Ureq {
    fn get(&self, url: &str) -> Result<String, FetchError> {
        Ok(self.agent.get(url).call()?.body_mut().read_to_string()?)
    }
}
