//! The single HTTP-transport seam. Every Provider reaches the network only
//! through the [`Transport`] trait, so the whole system can be driven offline
//! from recorded responses in tests (see [`MockTransport`]). The production
//! implementation is rustls-only (ADR-0004).

use std::time::Duration;
use thiserror::Error;

/// A raw HTTP response as seen by a Provider. Note that 4xx/5xx are *not*
/// errors here — the status is handed back so the Provider can classify the
/// Outcome (404 → NotApplicable, 429/5xx → Failed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub url: String,
}

/// A genuine transport failure (no HTTP status was obtained). These become
/// `Failed` Outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("request timed out")]
    Timeout,
    #[error("connection failed")]
    ConnectionFailed,
    #[error("transport error: {0}")]
    Other(String),
    #[error("could not read response body: {0}")]
    Body(String),
}

/// The one seam. Given a URL, return a response or a transport failure.
pub trait Transport {
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError>;
}

/// Default User-Agent identifying the tool.
pub fn default_user_agent() -> String {
    format!(
        "boast/{} (+{})",
        env!("CARGO_PKG_VERSION"),
        "https://github.com/mbhall88/boast"
    )
}

/// Production transport: a pooled ureq agent over rustls. Configured so that
/// HTTP error statuses are returned as responses (not errors), letting
/// Providers see 404 vs 429 vs 5xx.
pub struct UreqTransport {
    agent: ureq::Agent,
    user_agent: String,
}

impl UreqTransport {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self {
            agent: config.into(),
            user_agent: default_user_agent(),
        }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for UreqTransport {
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        match self
            .agent
            .get(url)
            .header("User-Agent", self.user_agent.as_str())
            .call()
        {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                let body = resp
                    .body_mut()
                    .read_to_string()
                    .map_err(|e| TransportError::Body(e.to_string()))?;
                Ok(HttpResponse {
                    status,
                    body,
                    url: url.to_string(),
                })
            }
            Err(ureq::Error::Timeout(_)) => Err(TransportError::Timeout),
            Err(ureq::Error::ConnectionFailed) => Err(TransportError::ConnectionFailed),
            Err(e) => Err(TransportError::Other(e.to_string())),
        }
    }
}

/// Offline transport for tests and cassettes. Routes are matched by URL
/// substring in registration order; an unmatched URL panics so tests never
/// accidentally hit the network.
#[derive(Default)]
pub struct MockTransport {
    routes: Vec<(String, MockReply)>,
}

enum MockReply {
    Http { status: u16, body: String },
    Error(TransportError),
}

impl MockTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reply with `status` and `body` for any URL containing `url_contains`.
    pub fn on(mut self, url_contains: &str, status: u16, body: impl Into<String>) -> Self {
        self.routes.push((
            url_contains.to_string(),
            MockReply::Http {
                status,
                body: body.into(),
            },
        ));
        self
    }

    /// Reply with a transport error for any URL containing `url_contains`.
    pub fn on_error(mut self, url_contains: &str, err: TransportError) -> Self {
        self.routes
            .push((url_contains.to_string(), MockReply::Error(err)));
        self
    }
}

impl Transport for MockTransport {
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        for (needle, reply) in &self.routes {
            if url.contains(needle) {
                return match reply {
                    MockReply::Http { status, body } => Ok(HttpResponse {
                        status: *status,
                        body: body.clone(),
                        url: url.to_string(),
                    }),
                    MockReply::Error(e) => Err(e.clone()),
                };
            }
        }
        panic!("MockTransport: no route registered for URL {url}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_matches_by_substring_in_order() {
        let t = MockTransport::new()
            .on("works/doi:10.1", 200, "ok-body")
            .on_error("timeout", TransportError::Timeout);

        let r = t.get("https://api.openalex.org/works/doi:10.1/x").unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, "ok-body");

        assert_eq!(
            t.get("https://example.com/timeout"),
            Err(TransportError::Timeout)
        );
    }

    #[test]
    #[should_panic(expected = "no route registered")]
    fn mock_panics_on_unmatched() {
        let _ = MockTransport::new().get("https://unmocked.example");
    }
}
