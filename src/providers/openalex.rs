//! OpenAlex citation Provider: the headline citation count plus the
//! field-weighted citation impact (FWCI) and citation percentile — all free,
//! no key. This is the walking skeleton's payload Provider.

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PaperId, Window};
use crate::provider::Provider;
use crate::transport::Transport;

const API_BASE: &str = "https://api.openalex.org/works/";

pub struct OpenAlex;

#[derive(Debug, Deserialize)]
struct OaWork {
    cited_by_count: Option<u64>,
    fwci: Option<f64>,
    citation_normalized_percentile: Option<OaPercentile>,
}

#[derive(Debug, Deserialize)]
struct OaPercentile {
    value: Option<f64>,
    is_in_top_1_percent: Option<bool>,
    is_in_top_10_percent: Option<bool>,
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
        matches!(identity, Identity::Paper(_))
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let selector = match Self::selector(identity) {
            Some(s) => s,
            None => {
                return Outcome::NotApplicable {
                    note: "OpenAlex only supports papers".into(),
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

        match resp.status {
            200 => Self::classify(&resp.body, &url, &canonical),
            404 => Outcome::NotApplicable {
                note: "not found in OpenAlex".into(),
            },
            429 => Outcome::Failed {
                error: "rate limited by OpenAlex (429)".into(),
            },
            s if (500..600).contains(&s) => Outcome::Failed {
                error: format!("OpenAlex server error ({s})"),
            },
            s => Outcome::Failed {
                error: format!("unexpected OpenAlex status ({s})"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PaperId;
    use crate::transport::{MockTransport, TransportError};

    fn doi() -> Identity {
        Identity::Paper(PaperId::Doi("10.1371/journal.pbio.1002195".into()))
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

        assert_eq!(metrics.len(), 3);
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
    }

    #[test]
    fn handles_null_fwci_and_percentile() {
        let body = r#"{"cited_by_count": 5, "fwci": null, "citation_normalized_percentile": null}"#;
        let t = MockTransport::new().on("works/doi:", 200, body);
        match OpenAlex.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => {
                assert_eq!(metrics.len(), 1);
                assert_eq!(metrics[0].value, MetricValue::Count(5));
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
}
