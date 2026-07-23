//! Dimensions badge Provider: citation count, Field Citation Ratio (FCR), and
//! Relative Citation Ratio (RCR) from the free, keyless Dimensions Metrics
//! API. The API's own licence/terms notice is recorded on the citations
//! Metric and surfaced in Reports rather than hidden (ADR-0003, ADR-0005).

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PaperId, Window};
use crate::provider::{classify_status, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://metrics-api.dimensions.ai/doi/";

pub struct Dimensions;

#[derive(Debug, Deserialize)]
struct DimensionsMetrics {
    times_cited: Option<u64>,
    field_citation_ratio: Option<f64>,
    relative_citation_ratio: Option<f64>,
    license: Option<String>,
}

impl Dimensions {
    fn doi(identity: &Identity) -> Option<&str> {
        match identity {
            Identity::Paper(PaperId::Doi(d)) => Some(d),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let parsed: DimensionsMetrics = match serde_json::from_str(body) {
            Ok(m) => m,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Dimensions response: {e}"),
                }
            }
        };

        let as_of = OffsetDateTime::now_utc();
        let mut metrics = Vec::new();

        // The licence/terms notice is attached to the headline citations
        // Metric so it is always shown in Reports, never hidden (ADR-0003).
        if let Some(citations) = parsed.times_cited {
            metrics.push(Metric {
                name: "citations".into(),
                category: Category::Citations,
                value: MetricValue::Count(citations),
                window: Window::Cumulative,
                provider: "dimensions".into(),
                identity: canonical.into(),
                as_of,
                source: url.into(),
                note: parsed.license.clone(),
            });
        }

        if let Some(fcr) = parsed.field_citation_ratio {
            metrics.push(Metric {
                name: "fcr".into(),
                category: Category::Citations,
                value: MetricValue::Real(fcr),
                window: Window::Cumulative,
                provider: "dimensions".into(),
                identity: canonical.into(),
                as_of,
                source: url.into(),
                note: Some(
                    "Field Citation Ratio; 1.0 = world average for the field and year".into(),
                ),
            });
        }

        if let Some(rcr) = parsed.relative_citation_ratio {
            metrics.push(Metric {
                name: "rcr".into(),
                category: Category::Citations,
                value: MetricValue::Real(rcr),
                window: Window::Cumulative,
                provider: "dimensions".into(),
                identity: canonical.into(),
                as_of,
                source: url.into(),
                note: Some("Relative Citation Ratio; 1.0 = NIH-funded benchmark".into()),
            });
        }

        if metrics.is_empty() {
            Outcome::NotApplicable {
                note: "Dimensions returned no citation metrics for this record".into(),
            }
        } else {
            Outcome::Values {
                metrics,
                metadata: None,
            }
        }
    }
}

impl Provider for Dimensions {
    fn name(&self) -> &'static str {
        "dimensions"
    }

    fn category(&self) -> Category {
        Category::Citations
    }

    fn supports(&self, identity: &Identity) -> bool {
        // The Dimensions Metrics API is keyed by DOI only.
        matches!(identity, Identity::Paper(PaperId::Doi(_)))
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let doi = match Self::doi(identity) {
            Some(d) => d,
            None => {
                return Outcome::NotApplicable {
                    note: "Dimensions requires a DOI".into(),
                }
            }
        };
        let url = format!("{API_BASE}{doi}");
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match classify_status(resp.status, "Dimensions", "not found in Dimensions") {
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

    const LICENSE_NOTICE: &str = "This data has been sourced via the Dimensions Metrics API, use of which is subject to the terms at https://dimensions.ai/policies/terms/metrics/. Any use by an unregistered organization is not authorized. Please contact info@dimensions.ai for further information.";

    #[test]
    fn parses_citations_fcr_and_rcr_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/dimensions_metrics.json");
        let t = MockTransport::new().on("metrics-api.dimensions.ai/doi/", 200, cassette);

        let metrics = match Dimensions.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 3);

        let cites = metrics.iter().find(|m| m.name == "citations").unwrap();
        assert_eq!(cites.value, MetricValue::Count(1285));
        assert_eq!(cites.provider, "dimensions");
        assert_eq!(cites.category, Category::Citations);
        assert_eq!(cites.window, Window::Cumulative);

        let fcr = metrics.iter().find(|m| m.name == "fcr").unwrap();
        assert_eq!(fcr.value, MetricValue::Real(116.66));

        let rcr = metrics.iter().find(|m| m.name == "rcr").unwrap();
        assert_eq!(rcr.value, MetricValue::Real(15.96));
    }

    #[test]
    fn licence_notice_is_recorded_exactly_once() {
        let cassette = include_str!("../../tests/cassettes/dimensions_metrics.json");
        let t = MockTransport::new().on("metrics-api.dimensions.ai/doi/", 200, cassette);

        let metrics = match Dimensions.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        let with_notice: Vec<&Metric> = metrics
            .iter()
            .filter(|m| m.note.as_deref() == Some(LICENSE_NOTICE))
            .collect();
        assert_eq!(
            with_notice.len(),
            1,
            "licence notice must appear exactly once"
        );
        // Attached to the headline citations Metric, the one Report readers see first.
        assert_eq!(with_notice[0].name, "citations");
    }

    #[test]
    fn handles_null_fcr_and_rcr() {
        // A very recent paper: cited, but too new for field-normalized metrics.
        let body = r#"{"doi":"10.1/x","times_cited":964,"recent_citations":964,"relative_citation_ratio":null,"field_citation_ratio":null,"license":"terms apply"}"#;
        let t = MockTransport::new().on("metrics-api.dimensions.ai/doi/", 200, body);

        let metrics = match Dimensions.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].name, "citations");
        assert_eq!(metrics[0].value, MetricValue::Count(964));
        assert_eq!(metrics[0].note.as_deref(), Some("terms apply"));
    }

    #[test]
    fn not_found_is_not_applicable_not_zero() {
        let body = r#"{"status":404,"error":"NOT FOUND","message":"Sorry, we can't find a record for that publication!"}"#;
        let t = MockTransport::new().on("metrics-api.dimensions.ai/doi/", 404, body);
        assert!(matches!(
            Dimensions.fetch(&doi(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn does_not_support_pmid() {
        let pmid = Identity::Paper(PaperId::Pmid("31234567".into()));
        assert!(!Dimensions.supports(&pmid));
        assert!(Dimensions.supports(&doi()));
    }

    #[test]
    fn does_not_support_repo_or_package() {
        assert!(!Dimensions.supports(&Identity::Repo(
            crate::model::RepoId::parse("owner/name").unwrap()
        )));
    }

    #[test]
    fn server_error_and_transport_error_are_failed() {
        let t500 = MockTransport::new().on("metrics-api.dimensions.ai/doi/", 503, "");
        assert!(matches!(
            Dimensions.fetch(&doi(), &t500),
            Outcome::Failed { .. }
        ));

        let terr = MockTransport::new()
            .on_error("metrics-api.dimensions.ai/doi/", TransportError::Timeout);
        assert!(matches!(
            Dimensions.fetch(&doi(), &terr),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn malformed_body_is_failed() {
        let t = MockTransport::new().on("metrics-api.dimensions.ai/doi/", 200, "not json");
        assert!(matches!(
            Dimensions.fetch(&doi(), &t),
            Outcome::Failed { .. }
        ));
    }
}
