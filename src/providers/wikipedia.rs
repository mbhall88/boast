//! Wikipedia mentions Provider: a keyless proxy for "how many Wikipedia
//! articles cite this paper", counted via English Wikipedia's own full-text
//! search for the DOI string. Free, no key (ADR-0003) — the closest
//! available replacement for Crossref Event Data's now-sunset Wikipedia
//! mentions feed, which is why the default Attention Category is "lite"
//! rather than backed by a purpose-built mentions API.

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PaperId, Window};
use crate::provider::{classify_status, Provider};
use crate::providers::percent_encode;
use crate::transport::Transport;

const API_BASE: &str = "https://en.wikipedia.org/w/api.php";

pub struct Wikipedia;

#[derive(Debug, Deserialize)]
struct SearchResponse {
    query: Option<SearchQuery>,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    searchinfo: Option<SearchInfo>,
}

#[derive(Debug, Deserialize)]
struct SearchInfo {
    totalhits: Option<u64>,
}

impl Wikipedia {
    fn doi(identity: &Identity) -> Option<&str> {
        match identity {
            Identity::Paper(PaperId::Doi(d)) => Some(d),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let parsed: SearchResponse = match serde_json::from_str(body) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Wikipedia response: {e}"),
                }
            }
        };

        // Unlike, say, Europe PMC's per-record `citedByCount` (legitimately
        // absent on some real records), `searchinfo.totalhits` is a
        // structural part of every MediaWiki search response regardless of
        // hit count (confirmed against the live API) — its absence means the
        // response shape wasn't what was expected, not a legitimate zero, so
        // it's `Failed` rather than `NotApplicable`.
        let hits = parsed
            .query
            .and_then(|q| q.searchinfo)
            .and_then(|s| s.totalhits);
        let count = match hits {
            Some(c) => c,
            None => {
                return Outcome::Failed {
                    error: "unexpected Wikipedia response: missing searchinfo.totalhits".into(),
                }
            }
        };

        Outcome::Values {
            metrics: vec![Metric {
                name: "wikipedia_mentions".into(),
                category: Category::Attention,
                value: MetricValue::Count(count),
                window: Window::Cumulative,
                provider: "wikipedia".into(),
                identity: canonical.into(),
                as_of: OffsetDateTime::now_utc(),
                source: url.into(),
                note: Some(
                    "English Wikipedia full-text search hits for this DOI; \
                     other-language Wikipedias are not counted"
                        .into(),
                ),
            }],
            metadata: None,
        }
    }
}

impl Provider for Wikipedia {
    fn name(&self) -> &'static str {
        "wikipedia"
    }

    fn category(&self) -> Category {
        Category::Attention
    }

    fn supports(&self, identity: &Identity) -> bool {
        matches!(identity, Identity::Paper(PaperId::Doi(_)))
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let doi = match Self::doi(identity) {
            Some(d) => d,
            None => {
                return Outcome::NotApplicable {
                    note: "Wikipedia mention search requires a DOI".into(),
                }
            }
        };
        let url = format!(
            "{API_BASE}?action=query&list=search&format=json&srlimit=1&srsearch=%22{}%22",
            percent_encode(doi)
        );
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match classify_status(resp.status, "Wikipedia", "not found in Wikipedia search") {
            Some(outcome) => outcome,
            None => Self::classify(&resp.body, &url, &canonical),
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

    #[test]
    fn parses_mention_count_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/wikipedia_search.json");
        let t = MockTransport::new().on("en.wikipedia.org/w/api.php", 200, cassette);

        let metrics = match Wikipedia.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 1);
        let m = &metrics[0];
        assert_eq!(m.name, "wikipedia_mentions");
        assert_eq!(m.value, MetricValue::Count(9));
        assert_eq!(m.category, Category::Attention);
        assert_eq!(m.window, Window::Cumulative);
        assert_eq!(m.provider, "wikipedia");
        assert_eq!(m.identity, "doi:10.1371/journal.pbio.1002195");
    }

    #[test]
    fn zero_hits_is_a_genuine_zero_not_absence() {
        let body = r#"{"query":{"searchinfo":{"totalhits":0},"search":[]}}"#;
        let t = MockTransport::new().on("en.wikipedia.org/w/api.php", 200, body);
        let metrics = match Wikipedia.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics[0].value, MetricValue::Count(0));
    }

    #[test]
    fn the_doi_is_percent_encoded_and_quoted_for_an_exact_phrase_search() {
        let t = MockTransport::new().on(
            "srsearch=%2210.1371%2Fjournal.pbio.1002195%22",
            200,
            r#"{"query":{"searchinfo":{"totalhits":1}}}"#,
        );
        let metrics = match Wikipedia.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics[0].value, MetricValue::Count(1));
    }

    #[test]
    fn does_not_support_pmid_repo_or_package() {
        assert!(!Wikipedia.supports(&Identity::Paper(PaperId::Pmid("31234567".into()))));
        assert!(!Wikipedia.supports(&Identity::Repo(
            crate::model::RepoId::parse("owner/name").unwrap()
        )));
        assert!(Wikipedia.supports(&doi()));
    }

    #[test]
    fn server_error_and_transport_error_are_failed() {
        let t500 = MockTransport::new().on("en.wikipedia.org/w/api.php", 503, "");
        assert!(matches!(
            Wikipedia.fetch(&doi(), &t500),
            Outcome::Failed { .. }
        ));

        let terr =
            MockTransport::new().on_error("en.wikipedia.org/w/api.php", TransportError::Timeout);
        assert!(matches!(
            Wikipedia.fetch(&doi(), &terr),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn malformed_body_is_failed() {
        let t = MockTransport::new().on("en.wikipedia.org/w/api.php", 200, "not json");
        assert!(matches!(
            Wikipedia.fetch(&doi(), &t),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn missing_searchinfo_is_failed_not_a_silent_zero() {
        let body = r#"{"query":{}}"#;
        let t = MockTransport::new().on("en.wikipedia.org/w/api.php", 200, body);
        assert!(matches!(
            Wikipedia.fetch(&doi(), &t),
            Outcome::Failed { .. }
        ));
    }
}
