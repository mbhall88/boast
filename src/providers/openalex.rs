//! OpenAlex Provider: paper citation and normalisation Metrics, open-access
//! status, and indexed scholarly repository mentions — all free, no key. This
//! is the walking skeleton's payload Provider. A paper request can answer both
//! Citations and Attention in one call; repository searches add an independent
//! Attention estimate from indexed full text.

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PaperId, Window};
use crate::provider::{classify_status, Provider};
use crate::providers::percent_encode;
use crate::transport::Transport;

const API_BASE: &str = "https://api.openalex.org/works/";
const SEARCH_API_BASE: &str = "https://api.openalex.org/works";

pub struct OpenAlex;

#[derive(Debug, Deserialize)]
struct OaWork {
    cited_by_count: Option<u64>,
    fwci: Option<f64>,
    citation_normalized_percentile: Option<OaPercentile>,
    open_access: Option<OaOpenAccess>,
}

#[derive(Debug, Deserialize)]
struct OaPercentile {
    value: Option<f64>,
    is_in_top_1_percent: Option<bool>,
    is_in_top_10_percent: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OaOpenAccess {
    oa_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OaSearchResponse {
    meta: Option<OaSearchMeta>,
}

#[derive(Debug, Deserialize)]
struct OaSearchMeta {
    count: Option<u64>,
}

impl OpenAlex {
    /// The OpenAlex selector for a paper Identity (e.g. `doi:10.x`, `pmid:123`).
    fn selector(identity: &Identity) -> Option<String> {
        match identity {
            Identity::Paper(PaperId::Doi(d)) => Some(format!("doi:{d}")),
            Identity::Paper(PaperId::Pmid(p)) => Some(format!("pmid:{p}")),
            Identity::Repo(_) | Identity::Package(_) => None,
        }
    }

    /// The OpenAlex full-text search filter for a Repo Identity. The phrase is
    /// derived from the canonical repository identity so URL, SSH, and
    /// shorthand input forms all produce the same query.
    fn repo_filter(identity: &Identity) -> Option<String> {
        let canonical = identity.canonical();
        let path = canonical.strip_prefix("github:")?;
        Some(format!(
            "fulltext.search:\"github.com/{path}\",type:article|preprint"
        ))
    }

    fn classify_repo(body: &str, url: &str, identity_canonical: &str) -> Outcome {
        let response: OaSearchResponse = match serde_json::from_str(body) {
            Ok(response) => response,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected OpenAlex response: {e}"),
                }
            }
        };
        let count = match response.meta.and_then(|meta| meta.count) {
            Some(count) => count,
            None => {
                return Outcome::Failed {
                    error: "unexpected OpenAlex response: missing meta.count".into(),
                }
            }
        };

        Outcome::Values {
            metrics: vec![Metric {
                name: "mentions".into(),
                category: Category::Attention,
                value: MetricValue::Count(count),
                window: Window::Cumulative,
                provider: "openalex".into(),
                identity: identity_canonical.into(),
                as_of: OffsetDateTime::now_utc(),
                source: url.into(),
                note: Some(
                    "indexed full-text search estimate, not a formal citation or verified literal URL count; partial coverage; self-mentions are included; article/preprint versions may be counted separately"
                        .into(),
                ),
            }],
            metadata: None,
        }
    }

    fn classify(body: &str, url: &str, identity_canonical: &str) -> Outcome {
        let work: OaWork = match serde_json::from_str(body) {
            Ok(w) => w,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected OpenAlex response: {e}"),
                }
            }
        };

        let as_of = OffsetDateTime::now_utc();
        let mut metrics = Vec::new();

        if let Some(citations) = work.cited_by_count {
            metrics.push(Metric {
                name: "citations".into(),
                category: Category::Citations,
                value: MetricValue::Count(citations),
                window: Window::Cumulative,
                provider: "openalex".into(),
                identity: identity_canonical.into(),
                as_of,
                source: url.into(),
                note: None,
            });
        }

        if let Some(fwci) = work.fwci {
            metrics.push(Metric {
                name: "fwci".into(),
                category: Category::Citations,
                value: MetricValue::Real(fwci),
                window: Window::Cumulative,
                provider: "openalex".into(),
                identity: identity_canonical.into(),
                as_of,
                source: url.into(),
                note: Some("field-weighted citation impact; 1.0 = world average".into()),
            });
        }

        if let Some(p) = work.citation_normalized_percentile {
            if let Some(v) = p.value {
                let note = match (
                    p.is_in_top_1_percent.unwrap_or(false),
                    p.is_in_top_10_percent.unwrap_or(false),
                ) {
                    (true, _) => Some("top 1% in its field, year, and type".into()),
                    (false, true) => Some("top 10% in its field, year, and type".into()),
                    _ => None,
                };
                metrics.push(Metric {
                    name: "citation_percentile".into(),
                    category: Category::Citations,
                    value: MetricValue::Real(v * 100.0),
                    window: Window::Cumulative,
                    provider: "openalex".into(),
                    identity: identity_canonical.into(),
                    as_of,
                    source: url.into(),
                    note,
                });
            }
        }

        if let Some(status) = work.open_access.and_then(|oa| oa.oa_status) {
            metrics.push(Metric {
                name: "open_access".into(),
                category: Category::Attention,
                value: MetricValue::Text(status),
                window: Window::Cumulative,
                provider: "openalex".into(),
                identity: identity_canonical.into(),
                as_of,
                source: url.into(),
                note: Some(
                    "OpenAlex open-access status; \"closed\" means no open-access copy found"
                        .into(),
                ),
            });
        }

        if metrics.is_empty() {
            Outcome::NotApplicable {
                note: "OpenAlex returned no citation metrics for this record".into(),
            }
        } else {
            Outcome::Values {
                metrics,
                metadata: None,
            }
        }
    }
}

impl Provider for OpenAlex {
    fn name(&self) -> &'static str {
        "openalex"
    }

    fn category(&self) -> Category {
        Category::Citations
    }

    fn supports(&self, identity: &Identity) -> bool {
        matches!(identity, Identity::Paper(_) | Identity::Repo(_))
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        if let Some(filter) = Self::repo_filter(identity) {
            let url = format!(
                "{SEARCH_API_BASE}?filter={}&per-page=1&select=id",
                percent_encode(&filter)
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

            return match classify_status(
                resp.status,
                "OpenAlex",
                "repository search not found in OpenAlex",
            ) {
                Some(outcome) => outcome,
                None => Self::classify_repo(&resp.body, &url, &canonical),
            };
        }

        let selector = match Self::selector(identity) {
            Some(s) => s,
            None => {
                return Outcome::NotApplicable {
                    note: "OpenAlex only supports papers and repositories".into(),
                }
            }
        };
        let url = format!("{API_BASE}{selector}");
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match classify_status(resp.status, "OpenAlex", "not found in OpenAlex") {
            Some(outcome) => outcome,
            None => Self::classify(&resp.body, &url, &canonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PaperId, RepoId};
    use crate::transport::{MockTransport, TransportError};

    fn doi() -> Identity {
        Identity::Paper(PaperId::Doi("10.1371/journal.pbio.1002195".into()))
    }

    fn repo() -> Identity {
        Identity::Repo(RepoId::parse("mbhall88/rasusa").unwrap())
    }

    #[test]
    fn parses_full_response_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/openalex_work.json");
        let t = MockTransport::new().on("api.openalex.org/works/doi:", 200, cassette);

        let outcome = OpenAlex.fetch(&doi(), &t);
        let metrics = match outcome {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 4);
        let cites = metrics.iter().find(|m| m.name == "citations").unwrap();
        assert_eq!(cites.value, MetricValue::Count(1421));
        assert_eq!(cites.window, Window::Cumulative);
        assert_eq!(cites.provider, "openalex");
        assert_eq!(cites.identity, "doi:10.1371/journal.pbio.1002195");

        let fwci = metrics.iter().find(|m| m.name == "fwci").unwrap();
        assert_eq!(fwci.value, MetricValue::Real(59.9063));

        let pct = metrics
            .iter()
            .find(|m| m.name == "citation_percentile")
            .unwrap();
        assert_eq!(pct.value, MetricValue::Real(99.956924));
        assert_eq!(
            pct.note.as_deref(),
            Some("top 1% in its field, year, and type")
        );

        let oa = metrics.iter().find(|m| m.name == "open_access").unwrap();
        assert_eq!(oa.value, MetricValue::Text("gold".into()));
        assert_eq!(oa.category, Category::Attention);
        assert_eq!(oa.window, Window::Cumulative);
    }

    #[test]
    fn handles_null_fwci_and_percentile_and_omits_open_access_when_absent() {
        let body = r#"{"cited_by_count": 5, "fwci": null, "citation_normalized_percentile": null}"#;
        let t = MockTransport::new().on("works/doi:", 200, body);
        match OpenAlex.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => {
                assert_eq!(metrics.len(), 1);
                assert_eq!(metrics[0].value, MetricValue::Count(5));
                assert!(!metrics.iter().any(|m| m.name == "open_access"));
            }
            other => panic!("expected Values, got {other:?}"),
        }
    }

    #[test]
    fn not_found_is_not_applicable_not_zero() {
        let t = MockTransport::new().on("works/doi:", 404, "<!doctype html>404");
        assert!(matches!(
            OpenAlex.fetch(&doi(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn rate_limit_and_server_error_are_failed() {
        let t429 = MockTransport::new().on("works/doi:", 429, "");
        assert!(matches!(
            OpenAlex.fetch(&doi(), &t429),
            Outcome::Failed { .. }
        ));

        let t503 = MockTransport::new().on("works/doi:", 503, "");
        assert!(matches!(
            OpenAlex.fetch(&doi(), &t503),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn transport_error_is_failed() {
        let t = MockTransport::new().on_error("works/doi:", TransportError::Timeout);
        assert!(matches!(OpenAlex.fetch(&doi(), &t), Outcome::Failed { .. }));
    }

    #[test]
    fn searches_repo_full_text_and_reports_an_attention_mention_count() {
        let cassette = include_str!("../../tests/cassettes/openalex_repo_search.json");
        let t = MockTransport::new().on(
            "filter=fulltext.search%3A%22github.com%2Fmbhall88%2Frasusa%22%2Ctype%3Aarticle%7Cpreprint",
            200,
            cassette,
        );

        let metrics = match OpenAlex.fetch(&repo(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 1);
        let mention = &metrics[0];
        assert_eq!(mention.name, "mentions");
        assert_eq!(mention.category, Category::Attention);
        assert_eq!(mention.value, MetricValue::Count(16));
        assert_eq!(mention.window, Window::Cumulative);
        assert_eq!(mention.provider, "openalex");
        assert_eq!(mention.identity, "github:mbhall88/rasusa");
        assert!(mention.source.contains("type%3Aarticle%7Cpreprint"));
        let note = mention.note.as_deref().unwrap();
        assert!(note.contains("indexed full-text search estimate"));
        assert!(note.contains("not a formal citation or verified literal URL count"));
        assert!(note.contains("partial coverage"));
        assert!(note.contains("self-mentions are included"));
        assert!(note.contains("versions may be counted separately"));
    }

    #[test]
    fn a_repo_search_with_no_hits_is_a_real_zero() {
        let body = r#"{"meta":{"count":0},"results":[]}"#;
        let t = MockTransport::new().on("fulltext.search%3A%22github.com%2F", 200, body);
        let metrics = match OpenAlex.fetch(&repo(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics[0].value, MetricValue::Count(0));
    }

    #[test]
    fn a_repo_search_without_meta_count_is_failed() {
        let body = r#"{"meta":{},"results":[]}"#;
        let t = MockTransport::new().on("fulltext.search%3A%22github.com%2F", 200, body);
        assert!(matches!(
            OpenAlex.fetch(&repo(), &t),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn a_repo_search_transport_failure_is_failed() {
        let t = MockTransport::new().on_error(
            "fulltext.search%3A%22github.com%2F",
            TransportError::ConnectionFailed,
        );
        assert!(matches!(
            OpenAlex.fetch(&repo(), &t),
            Outcome::Failed { .. }
        ));
    }
}
