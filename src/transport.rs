//! The single HTTP-transport seam. Every Provider reaches the network only
//! through the [`Transport`] trait, so the whole system can be driven offline
//! from recorded responses in tests (see [`MockTransport`]). The production
//! implementation is rustls-only (ADR-0004).

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;
use thiserror::Error;

/// A raw HTTP response as seen by a Provider. Note that 4xx/5xx are *not*
/// errors here — the status is handed back so the Provider can classify the
/// Outcome (404 → NotApplicable, 429/5xx → Failed). Response headers are
/// exposed (lower-cased keys) for things like pagination `Link` headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub url: String,
    pub headers: HashMap<String, String>,
}

impl HttpResponse {
    /// Look up a response header case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
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

/// The one seam. Given a URL (and optional request headers, e.g. an auth
/// token), return a response or a transport failure.
///
/// `Sync`: the orchestrator calls through a shared `&dyn Transport` from
/// multiple threads at once (bounded, one per host), so every implementation
/// must be safe to call concurrently.
pub trait Transport: Sync {
    fn get_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<HttpResponse, TransportError>;

    /// Convenience for the common no-extra-headers case.
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        self.get_with_headers(url, &[])
    }
}

/// Reads the polite-pool contact email from `BOAST_CONTACT_EMAIL`. Read only
/// from the environment, never from a Manifest — the Manifest holds Projects
/// and settings only and is meant to be committed/shared, never secrets or
/// personal contact details (see docs/spec "Configuration and secrets").
pub fn polite_contact_email() -> Option<String> {
    std::env::var("BOAST_CONTACT_EMAIL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// The tool's User-Agent, optionally extended with a contact email so
/// OpenAlex/Crossref admit requests to their faster "polite pool".
pub fn build_user_agent(contact_email: Option<&str>) -> String {
    let repo = "https://github.com/mbhall88/boast";
    match contact_email {
        Some(email) => format!(
            "boast/{} (+{repo}; mailto:{email})",
            env!("CARGO_PKG_VERSION")
        ),
        None => format!("boast/{} (+{repo})", env!("CARGO_PKG_VERSION")),
    }
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
            user_agent: build_user_agent(polite_contact_email().as_deref()),
        }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for UreqTransport {
    fn get_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<HttpResponse, TransportError> {
        let mut request = self
            .agent
            .get(url)
            .header("User-Agent", self.user_agent.as_str());
        for (key, value) in headers {
            request = request.header(*key, *value);
        }
        match request.call() {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                let headers = resp
                    .headers()
                    .iter()
                    .map(|(name, value)| {
                        (
                            name.as_str().to_ascii_lowercase(),
                            value.to_str().unwrap_or("").to_string(),
                        )
                    })
                    .collect();
                let body = resp
                    .body_mut()
                    .read_to_string()
                    .map_err(|e| TransportError::Body(e.to_string()))?;
                Ok(HttpResponse {
                    status,
                    body,
                    url: url.to_string(),
                    headers,
                })
            }
            Err(ureq::Error::Timeout(_)) => Err(TransportError::Timeout),
            Err(ureq::Error::ConnectionFailed) => Err(TransportError::ConnectionFailed),
            Err(e) => Err(TransportError::Other(e.to_string())),
        }
    }
}

/// A transient HTTP status worth retrying: 429 (rate limited) or any 5xx
/// (server error). A 4xx other than 429 is a permanent classification
/// (`NotApplicable`, etc.) and must never be retried. Shared with
/// `provider::classify_status`'s own 429/5xx handling so the two stay in
/// sync on what counts as transient.
pub(crate) fn is_transient_status(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

/// A transient transport failure worth retrying. `Other` (which also
/// swallows some real network blips ureq doesn't distinguish as a timeout or
/// connection failure — DNS, TLS, a broken pipe) and `Body` are not retried:
/// since we can't positively identify them as transient, retrying risks
/// masking a permanent error instead.
fn is_transient_error(err: &TransportError) -> bool {
    matches!(
        err,
        TransportError::Timeout | TransportError::ConnectionFailed
    )
}

/// Bounded exponential-backoff policy for retrying a transient failure
/// before giving up. A persistently failing fetch still ends as an error
/// status/`Err` once the budget is spent — never retried forever — leaving
/// the existing Provider classification (`classify_status`) to turn it into
/// `Failed`.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Retries attempted after the first try (`max_retries = 3` means up to
    /// 4 total attempts).
    pub max_retries: u32,
    /// Delay before the first retry; doubles on each subsequent retry.
    pub base_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(500),
        }
    }
}

/// Wraps any [`Transport`] with bounded exponential backoff over transient
/// failures (429, 5xx, timeout, connection failure), so a momentary
/// rate-limit or blip doesn't cost a Provider its data.
pub struct RetryingTransport<T: Transport> {
    inner: T,
    policy: RetryPolicy,
    sleep: Box<dyn Fn(Duration) + Send + Sync>,
}

impl<T: Transport> RetryingTransport<T> {
    /// Wrap `inner` with the default retry policy, sleeping for real between
    /// attempts.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            policy: RetryPolicy::default(),
            sleep: Box::new(std::thread::sleep),
        }
    }

    /// Wrap `inner` with an explicit policy and a custom `sleep`, so tests can
    /// exercise retry/backoff without a real wall-clock delay. Crate-internal
    /// only — not part of the public API surface.
    #[cfg(test)]
    pub(crate) fn with_policy_and_sleep(
        inner: T,
        policy: RetryPolicy,
        sleep: impl Fn(Duration) + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner,
            policy,
            sleep: Box::new(sleep),
        }
    }
}

impl<T: Transport> Transport for RetryingTransport<T> {
    fn get_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<HttpResponse, TransportError> {
        let mut attempt = 0;
        loop {
            let outcome = self.inner.get_with_headers(url, headers);
            let transient = match &outcome {
                Ok(resp) => is_transient_status(resp.status),
                Err(e) => is_transient_error(e),
            };
            if !transient || attempt >= self.policy.max_retries {
                return outcome;
            }
            (self.sleep)(self.policy.base_delay * 2u32.pow(attempt));
            attempt += 1;
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
    Http {
        status: u16,
        body: String,
        headers: HashMap<String, String>,
    },
    Error(TransportError),
    /// A scripted queue of `(status, body)` replies: each call consumes the
    /// next entry in order; once only one is left, it repeats. `Mutex` (not
    /// `RefCell`) so `MockTransport` stays `Sync` — the orchestrator may call
    /// a route from multiple threads (never concurrently for the *same*
    /// host, but `MockTransport` itself has no way to know that).
    Sequence(Mutex<VecDeque<(u16, String)>>),
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
                headers: HashMap::new(),
            },
        ));
        self
    }

    /// Reply with `status`, `body`, and response headers for a matching URL.
    pub fn on_with_headers(
        mut self,
        url_contains: &str,
        status: u16,
        body: impl Into<String>,
        headers: &[(&str, &str)],
    ) -> Self {
        let headers = headers
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
            .collect();
        self.routes.push((
            url_contains.to_string(),
            MockReply::Http {
                status,
                body: body.into(),
                headers,
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

    /// Reply with a scripted sequence of `(status, body)` responses for a
    /// matching URL: each call consumes the next entry in order; once only
    /// one is left, it repeats. Lets tests script retry/backoff behaviour
    /// (e.g. 429 then 200).
    pub fn on_sequence(mut self, url_contains: &str, replies: &[(u16, &str)]) -> Self {
        self.routes.push((
            url_contains.to_string(),
            MockReply::Sequence(Mutex::new(
                replies.iter().map(|(s, b)| (*s, b.to_string())).collect(),
            )),
        ));
        self
    }
}

impl Transport for MockTransport {
    fn get_with_headers(
        &self,
        url: &str,
        _headers: &[(&str, &str)],
    ) -> Result<HttpResponse, TransportError> {
        for (needle, reply) in &self.routes {
            if url.contains(needle) {
                return match reply {
                    MockReply::Http {
                        status,
                        body,
                        headers,
                    } => Ok(HttpResponse {
                        status: *status,
                        body: body.clone(),
                        url: url.to_string(),
                        headers: headers.clone(),
                    }),
                    MockReply::Error(e) => Err(e.clone()),
                    MockReply::Sequence(queue) => {
                        let mut queue = queue.lock().unwrap();
                        let (status, body) = if queue.len() > 1 {
                            queue.pop_front().expect("checked non-empty above")
                        } else {
                            queue
                                .front()
                                .cloned()
                                .expect("on_sequence given an empty reply list")
                        };
                        Ok(HttpResponse {
                            status,
                            body,
                            url: url.to_string(),
                            headers: HashMap::new(),
                        })
                    }
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
    fn mock_exposes_response_headers_case_insensitively() {
        let t = MockTransport::new().on_with_headers(
            "contributors",
            200,
            "[]",
            &[("Link", "<...&page=477>; rel=\"last\"")],
        );
        let r = t
            .get("https://api.github.com/repos/o/n/contributors")
            .unwrap();
        assert!(r.header("link").unwrap().contains("page=477"));
    }

    #[test]
    #[should_panic(expected = "no route registered")]
    fn mock_panics_on_unmatched() {
        let _ = MockTransport::new().get("https://unmocked.example");
    }

    #[test]
    fn mock_sequence_advances_per_call_then_repeats_the_last_reply() {
        let t = MockTransport::new().on_sequence("example.com", &[(429, ""), (200, "ok")]);
        assert_eq!(t.get("https://example.com").unwrap().status, 429);
        assert_eq!(t.get("https://example.com").unwrap().status, 200);
        // Exhausted: the last scripted reply keeps repeating.
        assert_eq!(t.get("https://example.com").unwrap().status, 200);
    }

    #[test]
    fn builds_user_agent_with_and_without_a_contact_email() {
        let bare = build_user_agent(None);
        assert!(bare.starts_with("boast/"));
        assert!(bare.contains("github.com/mbhall88/boast"));
        assert!(!bare.contains("mailto:"));

        let polite = build_user_agent(Some("me@example.com"));
        assert!(polite.contains("mailto:me@example.com"));
        assert!(polite.contains("github.com/mbhall88/boast"));
    }

    /// Records every delay passed to a fake `sleep`, so tests can assert
    /// backoff happened (and how many times) without a real wall-clock wait.
    fn recording_sleep() -> (
        impl Fn(Duration) + Send + Sync + 'static,
        std::sync::Arc<std::sync::Mutex<Vec<Duration>>>,
    ) {
        let delays = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = delays.clone();
        (move |d: Duration| recorder.lock().unwrap().push(d), delays)
    }

    #[test]
    fn a_429_then_200_succeeds_after_one_backoff_at_the_transport_layer() {
        let inner = MockTransport::new().on_sequence("example.com", &[(429, ""), (200, "ok-body")]);
        let (sleep, delays) = recording_sleep();
        let t = RetryingTransport::with_policy_and_sleep(inner, RetryPolicy::default(), sleep);

        let resp = t.get("https://example.com").unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "ok-body");
        assert_eq!(delays.lock().unwrap().len(), 1, "exactly one retry backoff");
    }

    #[test]
    fn persistent_429_is_bounded_and_ends_as_the_last_response() {
        let inner = MockTransport::new().on("example.com", 429, "still limited");
        let (sleep, delays) = recording_sleep();
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay: Duration::from_millis(1),
        };
        let t = RetryingTransport::with_policy_and_sleep(inner, policy, sleep);

        let resp = t.get("https://example.com").unwrap();
        assert_eq!(resp.status, 429, "never coerced into a fake success");
        // Bounded: exactly max_retries backoffs, never retried forever.
        assert_eq!(delays.lock().unwrap().len(), 2);
    }

    #[test]
    fn persistent_timeout_is_bounded_and_ends_as_an_error() {
        let inner = MockTransport::new().on_error("example.com", TransportError::Timeout);
        let (sleep, delays) = recording_sleep();
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay: Duration::from_millis(1),
        };
        let t = RetryingTransport::with_policy_and_sleep(inner, policy, sleep);

        assert_eq!(t.get("https://example.com"), Err(TransportError::Timeout));
        assert_eq!(delays.lock().unwrap().len(), 2);
    }

    #[test]
    fn backoff_delay_doubles_each_retry() {
        let inner = MockTransport::new().on("example.com", 503, "");
        let (sleep, delays) = recording_sleep();
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_millis(10),
        };
        let t = RetryingTransport::with_policy_and_sleep(inner, policy, sleep);

        let _ = t.get("https://example.com");
        let seen = delays.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![
                Duration::from_millis(10),
                Duration::from_millis(20),
                Duration::from_millis(40),
            ]
        );
    }

    #[test]
    fn success_and_permanent_failure_are_never_retried() {
        let inner = MockTransport::new()
            .on("ok.example", 200, "fine")
            .on("missing.example", 404, "nope")
            .on_error("bad.example", TransportError::Other("nope".into()));
        let (sleep, delays) = recording_sleep();
        let t = RetryingTransport::with_policy_and_sleep(inner, RetryPolicy::default(), sleep);

        assert_eq!(t.get("https://ok.example").unwrap().status, 200);
        assert_eq!(t.get("https://missing.example").unwrap().status, 404);
        assert!(t.get("https://bad.example").is_err());
        assert!(
            delays.lock().unwrap().is_empty(),
            "no backoff for a success or a permanent failure"
        );
    }
}
