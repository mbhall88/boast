//! Altmetric attention-breakdown Provider: the richer, opt-in half of the
//! Attention Category (ADR-0003) — an overall Attention Score plus a
//! news/blog/policy/patent/social/Mendeley-readers breakdown, gated behind
//! `ALTMETRIC_KEY` since Altmetric's Details Page API stopped answering
//! keyless requests on 10 November 2025. Without the key this Provider still
//! produces a visible `NotApplicable` row explaining *why* nothing was
//! collected, so its absence is never mistaken for zero attention (user
//! story 33).
//!
//! **`ALTMETRIC_KEY` must be a Details Page API key.** Altmetric Explorer
//! (an institutional analytics dashboard) is a *different product* with its
//! own key/secret pair that will not authenticate here — confirmed directly
//! against the live API, which returns a clear "API key ... not recognized"
//! for an Explorer credential. A Details Page API key needs either an
//! institutional licence (ask your library) or Altmetric's SRAD
//! (Scientometric Research Access to Data) program; see ADR-0003.
//!
//! Field names follow Altmetric's long-stable `cited_by_*_count`/`readers`
//! convention (unchanged across the classic free `v1/doi` endpoint and the
//! current keyed `v1/fetch/doi` endpoint, and depended on by every
//! third-party Altmetric client this Provider's shape was cross-checked
//! against) — **but this has only been checked against public documentation
//! and third-party client source, never against a real successful response**
//! (no Details Page API key was available while writing this). Every field
//! is read as `Option`, so an unexpected response shape degrades to omitting
//! that one Metric rather than failing the whole fetch; if *every* field
//! comes back absent, that's treated as `Failed` rather than a confirmed
//! zero (see the doc comment on the `metrics.is_empty()` branch below) —
//! specifically because that scenario is the one most likely to mean this
//! schema assumption was wrong, not that the paper genuinely has no
//! attention. If you have real Details Page API access, running this
//! Provider once and comparing the result against what you see is the one
//! remaining gap in confidence here.
//!
//! The API key is only ever placed in the URL actually sent to the
//! Transport, never in a Metric's `source` (which prefers Altmetric's own
//! public `details_url`) or in any error text this Provider constructs
//! itself (`classify_status`'s templates and the explicit 403 case never
//! touch the URL) — a Snapshot is a committable, shareable artifact, and a
//! leaked key in one would defeat the entire point of keeping secrets out
//! of it (see "Configuration and secrets" in the spec). The one path this
//! Provider doesn't control is `Failed{error: e.to_string()}` on a raw
//! transport failure (timeout, DNS, connection refused): that text comes
//! from `ureq`'s own `Error::Display`, which was checked manually (not
//! unit-tested — see the tests below) not to embed the request URL.

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PaperId, Window};
use crate::provider::{classify_status, KeyRequirement, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://api.altmetric.com/v1/fetch";

/// Shown as the `NotApplicable` note whenever `ALTMETRIC_KEY` isn't set — the
/// Report's visible answer to issue #15 AC3 ("not collected", distinct from
/// zero). Kept under `report`'s `INLINE_DETAIL_LIMIT` (80 chars) so it
/// actually renders on the row instead of being silently dropped (a
/// `NotApplicable`/`Failed` detail has no footer-promotion path the way a
/// `Metric.note` does — see `report::provider_notices`, which only scans
/// `Values` metrics). Exposed as a constant, not a literal duplicated in
/// tests, so report.rs's rendering test can't silently drift out of sync
/// with what this Provider actually says.
pub(crate) const NO_KEY_NOTE: &str =
    "Altmetric attention data not collected: no Details Page API key (ALTMETRIC_KEY)";

pub struct Altmetric {
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AltmetricResponse {
    score: Option<f64>,
    details_url: Option<String>,
    cited_by_msm_count: Option<u64>,
    cited_by_feeds_count: Option<u64>,
    cited_by_policies_count: Option<u64>,
    cited_by_patents_count: Option<u64>,
    cited_by_tweeters_count: Option<u64>,
    readers: Option<AltmetricReaders>,
}

#[derive(Debug, Deserialize)]
struct AltmetricReaders {
    mendeley: Option<u64>,
}

impl Altmetric {
    pub fn new() -> Self {
        Self {
            key: std::env::var("ALTMETRIC_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
        }
    }

    pub fn has_key(&self) -> bool {
        self.key.is_some()
    }

    /// The `(kind, id)` Altmetric's `/fetch/{kind}/{id}` path expects, or
    /// `None` for an Identity kind Altmetric doesn't track (Attention data
    /// only exists for papers).
    fn endpoint(identity: &Identity) -> Option<(&'static str, &str)> {
        match identity {
            Identity::Paper(PaperId::Doi(d)) => Some(("doi", d.as_str())),
            Identity::Paper(PaperId::Pmid(p)) => Some(("pmid", p.as_str())),
            Identity::Repo(_) | Identity::Package(_) => None,
        }
    }

    fn classify(body: &str, public_url: &str, canonical: &str) -> Outcome {
        let parsed: AltmetricResponse = match serde_json::from_str(body) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Altmetric response: {e}"),
                }
            }
        };

        let as_of = OffsetDateTime::now_utc();
        // Prefer Altmetric's own public details page as attribution when the
        // response carries one; it's a more useful link than the bare API
        // call, and never carries the key (unlike the URL actually fetched).
        let source = parsed.details_url.as_deref().unwrap_or(public_url);
        let mut metrics = Vec::new();
        let mut push = |name: &str, value: MetricValue, note: Option<&str>| {
            metrics.push(Metric {
                name: name.into(),
                category: Category::Attention,
                value,
                window: Window::Cumulative,
                provider: "altmetric".into(),
                identity: canonical.into(),
                as_of,
                source: source.into(),
                note: note.map(str::to_string),
            });
        };

        if let Some(score) = parsed.score {
            push(
                "attention_score",
                MetricValue::Real(score),
                Some("Altmetric Attention Score; a weighted count of attention, not a percentile"),
            );
        }
        if let Some(count) = parsed.cited_by_msm_count {
            push("news_mentions", MetricValue::Count(count), None);
        }
        if let Some(count) = parsed.cited_by_feeds_count {
            push("blog_mentions", MetricValue::Count(count), None);
        }
        if let Some(count) = parsed.cited_by_policies_count {
            push("policy_mentions", MetricValue::Count(count), None);
        }
        if let Some(count) = parsed.cited_by_patents_count {
            push("patent_mentions", MetricValue::Count(count), None);
        }
        if let Some(count) = parsed.cited_by_tweeters_count {
            push(
                "social_mentions",
                MetricValue::Count(count),
                Some("X/Twitter mentions tracked by Altmetric; other social sources not included"),
            );
        }
        if let Some(count) = parsed.readers.and_then(|r| r.mendeley) {
            push("mendeley_readers", MetricValue::Count(count), None);
        }

        if metrics.is_empty() {
            // A 200 response with none of the known fields present is treated
            // as `Failed`, not `NotApplicable`: Altmetric only ever creates a
            // tracked record once it has detected at least one mention, so a
            // *legitimate* zero-attention record shouldn't exist to 200 on in
            // the first place — a 200 with nothing recognisable in it is far
            // more likely this Provider's assumed field names (never
            // confirmed against a real successful response — see the module
            // doc comment) not matching what Altmetric actually sent back.
            // Claiming a confident "no attention" here would be exactly the
            // kind of silent wrong-shape-looks-like-zero failure ADR-0002
            // exists to prevent.
            Outcome::Failed {
                error: "Altmetric response missing all expected fields (not a confirmed zero)"
                    .into(),
            }
        } else {
            Outcome::Values {
                metrics,
                metadata: None,
            }
        }
    }
}

impl Default for Altmetric {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for Altmetric {
    fn name(&self) -> &'static str {
        "altmetric"
    }

    fn category(&self) -> Category {
        Category::Attention
    }

    fn supports(&self, identity: &Identity) -> bool {
        Self::endpoint(identity).is_some()
    }

    fn key_requirement(&self) -> KeyRequirement {
        KeyRequirement::Required {
            env_var: "ALTMETRIC_KEY",
        }
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let (kind, id) = match Self::endpoint(identity) {
            Some(e) => e,
            None => {
                return Outcome::NotApplicable {
                    note: "Altmetric requires a paper (DOI or PubMed ID)".into(),
                }
            }
        };
        let public_url = format!("{API_BASE}/{kind}/{id}");
        let canonical = identity.canonical();

        let key = match &self.key {
            Some(k) => k,
            None => {
                return Outcome::NotApplicable {
                    note: NO_KEY_NOTE.into(),
                }
            }
        };
        let fetch_url = format!("{public_url}?key={key}");

        let resp = match transport.get(&fetch_url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        if resp.status == 403 {
            return Outcome::Failed {
                error: "Altmetric API key rejected (403); check ALTMETRIC_KEY".into(),
            };
        }
        match classify_status(resp.status, "Altmetric", "not found in Altmetric") {
            Some(outcome) => outcome,
            None => Self::classify(&resp.body, &public_url, &canonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{MockTransport, TransportError};

    fn doi() -> Identity {
        Identity::Paper(PaperId::Doi("10.1371/journal.pbio.1002195".into()))
    }

    fn keyed() -> Altmetric {
        Altmetric {
            key: Some("test-key".into()),
        }
    }

    fn keyless() -> Altmetric {
        Altmetric { key: None }
    }

    #[test]
    fn declares_altmetric_key_as_required_regardless_of_whether_one_is_set() {
        assert_eq!(
            keyless().key_requirement(),
            KeyRequirement::Required {
                env_var: "ALTMETRIC_KEY"
            }
        );
        assert_eq!(
            keyed().key_requirement(),
            KeyRequirement::Required {
                env_var: "ALTMETRIC_KEY"
            }
        );
    }

    #[test]
    fn without_a_key_nothing_is_collected_and_the_reason_is_explicit() {
        // No route registered: this must never even attempt a network call.
        let t = MockTransport::new();
        let outcome = keyless().fetch(&doi(), &t);
        match outcome {
            Outcome::NotApplicable { note } => {
                assert!(note.contains("ALTMETRIC_KEY"));
                assert!(note.contains("not collected"));
            }
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    #[test]
    fn with_a_key_parses_the_full_breakdown_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/altmetric_fetch.json");
        let t = MockTransport::new().on("api.altmetric.com/v1/fetch/doi/", 200, cassette);

        let metrics = match keyed().fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 7);
        let score = metrics
            .iter()
            .find(|m| m.name == "attention_score")
            .unwrap();
        assert_eq!(score.value, MetricValue::Real(214.35));
        assert_eq!(score.category, Category::Attention);

        let news = metrics.iter().find(|m| m.name == "news_mentions").unwrap();
        assert_eq!(news.value, MetricValue::Count(12));
        let blogs = metrics.iter().find(|m| m.name == "blog_mentions").unwrap();
        assert_eq!(blogs.value, MetricValue::Count(8));
        let policy = metrics
            .iter()
            .find(|m| m.name == "policy_mentions")
            .unwrap();
        assert_eq!(policy.value, MetricValue::Count(2));
        let patents = metrics
            .iter()
            .find(|m| m.name == "patent_mentions")
            .unwrap();
        assert_eq!(patents.value, MetricValue::Count(1));
        let social = metrics
            .iter()
            .find(|m| m.name == "social_mentions")
            .unwrap();
        assert_eq!(social.value, MetricValue::Count(180));
        let mendeley = metrics
            .iter()
            .find(|m| m.name == "mendeley_readers")
            .unwrap();
        assert_eq!(mendeley.value, MetricValue::Count(240));

        // Attributed to the public details page, not the keyed API URL.
        for m in &metrics {
            assert_eq!(m.source, "https://www.altmetric.com/details/987654");
        }
    }

    #[test]
    fn the_api_key_never_appears_in_a_metrics_source() {
        let cassette = include_str!("../../tests/cassettes/altmetric_fetch.json");
        let t = MockTransport::new().on("api.altmetric.com/v1/fetch/doi/", 200, cassette);
        let metrics = match keyed().fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        for m in &metrics {
            assert!(!m.source.contains("test-key"));
        }

        // A response with no details_url falls back to the keyless public URL.
        let bare =
            MockTransport::new().on("api.altmetric.com/v1/fetch/doi/", 200, r#"{"score": 1.0}"#);
        let metrics = match keyed().fetch(&doi(), &bare) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert!(!metrics[0].source.contains("key="));
    }

    // `a_rejected_key_is_a_distinct_failed_reason` (below) and
    // `not_found_is_not_applicable_not_zero` already cover that the 403 and
    // 404 error paths never echo the fetch URL (they're fixed templates in
    // `classify_status`/the explicit 403 branch, neither of which touches
    // the URL at all). The remaining `Failed{error: e.to_string()}` path on
    // a raw transport failure isn't unit-tested here: `MockTransport`'s
    // scripted errors don't echo the request URL back, so no test built on
    // it could actually catch a real URL-in-error-text leak. That path
    // instead relies on `ureq`'s own `Error::Display` never embedding the
    // request URL — verified manually against ureq 3.3.0 (DNS-failure and
    // connection-refused cases), not something this crate controls or can
    // unit-test; see the module doc comment.

    #[test]
    fn missing_key_is_not_applicable_never_a_network_call_or_a_failure() {
        let t = MockTransport::new().on("api.altmetric.com", 200, r#"{"score": 1.0}"#);
        assert!(matches!(
            keyless().fetch(&doi(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn not_found_is_not_applicable_not_zero() {
        let t = MockTransport::new().on("api.altmetric.com/v1/fetch/doi/", 404, "");
        assert!(matches!(
            keyed().fetch(&doi(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn a_rejected_key_is_a_distinct_failed_reason() {
        let t = MockTransport::new().on("api.altmetric.com/v1/fetch/doi/", 403, "");
        match keyed().fetch(&doi(), &t) {
            Outcome::Failed { error } => assert!(error.contains("ALTMETRIC_KEY")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn server_error_and_transport_error_are_failed() {
        let t500 = MockTransport::new().on("api.altmetric.com/v1/fetch/doi/", 503, "");
        assert!(matches!(
            keyed().fetch(&doi(), &t500),
            Outcome::Failed { .. }
        ));

        let terr = MockTransport::new()
            .on_error("api.altmetric.com/v1/fetch/doi/", TransportError::Timeout);
        assert!(matches!(
            keyed().fetch(&doi(), &terr),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn malformed_body_is_failed() {
        let t = MockTransport::new().on("api.altmetric.com/v1/fetch/doi/", 200, "not json");
        assert!(matches!(keyed().fetch(&doi(), &t), Outcome::Failed { .. }));
    }

    #[test]
    fn a_response_with_no_recognized_fields_is_failed_never_a_confirmed_zero() {
        // Valid JSON, 200 status, but none of the fields this Provider looks
        // for are present — the scenario a schema mismatch would produce.
        // ADR-0002: this must never be reported as a confident "no
        // attention" (`NotApplicable`), since that would be indistinguishable
        // from a real zero-attention record to a Report reader.
        let t = MockTransport::new().on("api.altmetric.com/v1/fetch/doi/", 200, "{}");
        match keyed().fetch(&doi(), &t) {
            Outcome::Failed { error } => {
                assert!(error.contains("not a confirmed zero"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn supports_papers_by_doi_or_pmid_not_repos_or_packages() {
        let pmid = Identity::Paper(PaperId::Pmid("31234567".into()));
        assert!(keyless().supports(&doi()));
        assert!(keyless().supports(&pmid));
        assert!(!keyless().supports(&Identity::Repo(
            crate::model::RepoId::parse("owner/name").unwrap()
        )));
    }

    #[test]
    fn fetches_a_pmid_via_the_pmid_path() {
        let pmid = Identity::Paper(PaperId::Pmid("31234567".into()));
        let t = MockTransport::new().on(
            "api.altmetric.com/v1/fetch/pmid/31234567",
            200,
            r#"{"score": 1.0}"#,
        );
        assert!(matches!(keyed().fetch(&pmid, &t), Outcome::Values { .. }));
    }
}
