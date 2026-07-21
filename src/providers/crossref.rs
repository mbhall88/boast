//! Crossref metadata Provider: authoritative bibliographic metadata (title,
//! authors, journal, year) that labels the paper, plus Crossref's own citation
//! count (`is-referenced-by-count`) as a second, independent Citations Metric.

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{
    Category, Identity, Metric, MetricValue, Outcome, PaperId, PaperMetadata, Window,
};
use crate::provider::{classify_status, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://api.crossref.org/works/";

pub struct Crossref;

#[derive(Debug, Deserialize)]
struct CrossrefEnvelope {
    message: Option<CrossrefWork>,
}

#[derive(Debug, Deserialize)]
struct CrossrefWork {
    title: Option<Vec<String>>,
    #[serde(rename = "container-title")]
    container_title: Option<Vec<String>>,
    author: Option<Vec<CrossrefAuthor>>,
    issued: Option<CrossrefDate>,
    #[serde(rename = "is-referenced-by-count")]
    is_referenced_by_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CrossrefAuthor {
    given: Option<String>,
    family: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrossrefDate {
    #[serde(rename = "date-parts")]
    date_parts: Option<Vec<Vec<i64>>>,
}

impl CrossrefAuthor {
    fn display_name(&self) -> Option<String> {
        match (&self.given, &self.family) {
            (Some(g), Some(f)) => Some(format!("{g} {f}")),
            (None, Some(f)) => Some(f.clone()),
            (Some(g), None) => Some(g.clone()),
            (None, None) => None,
        }
    }
}

impl Crossref {
    fn doi(identity: &Identity) -> Option<&str> {
        match identity {
            Identity::Paper(PaperId::Doi(d)) => Some(d),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let work = match serde_json::from_str::<CrossrefEnvelope>(body) {
            Ok(env) => match env.message {
                Some(w) => w,
                None => {
                    return Outcome::Failed {
                        error: "Crossref response had no message".into(),
                    }
                }
            },
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Crossref response: {e}"),
                }
            }
        };

        let as_of = OffsetDateTime::now_utc();
        let mut metrics = Vec::new();

        if let Some(citations) = work.is_referenced_by_count {
            metrics.push(Metric {
                name: "citations".into(),
                category: Category::Citations,
                value: MetricValue::Count(citations),
                window: Window::Cumulative,
                provider: "crossref".into(),
                identity: canonical.into(),
                as_of,
                source: url.into(),
                note: Some("times referenced, per Crossref".into()),
            });
        }

        let title = work.title.and_then(|t| t.into_iter().next());
        let container = work.container_title.and_then(|t| t.into_iter().next());
        let authors: Vec<String> = work
            .author
            .unwrap_or_default()
            .iter()
            .filter_map(CrossrefAuthor::display_name)
            .collect();
        let year = work
            .issued
            .and_then(|d| d.date_parts)
            .and_then(|dp| dp.into_iter().next())
            .and_then(|first| first.into_iter().next())
            .map(|y| y as i32);

        let has_metadata =
            title.is_some() || container.is_some() || !authors.is_empty() || year.is_some();
        let metadata = has_metadata.then(|| PaperMetadata {
            identity: canonical.into(),
            title,
            authors,
            container,
            year,
            provider: "crossref".into(),
            source: url.into(),
        });

        if metrics.is_empty() && metadata.is_none() {
            Outcome::NotApplicable {
                note: "Crossref returned no metadata for this DOI".into(),
            }
        } else {
            Outcome::Values { metrics, metadata }
        }
    }
}

impl Provider for Crossref {
    fn name(&self) -> &'static str {
        "crossref"
    }

    fn category(&self) -> Category {
        Category::Citations
    }

    fn supports(&self, identity: &Identity) -> bool {
        // Crossref is keyed by DOI; a bare PMID can't be resolved here.
        matches!(identity, Identity::Paper(PaperId::Doi(_)))
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let doi = match Self::doi(identity) {
            Some(d) => d,
            None => {
                return Outcome::NotApplicable {
                    note: "Crossref requires a DOI".into(),
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

        match classify_status(resp.status, "Crossref", "not found in Crossref") {
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
    fn parses_metadata_and_citation_count_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/crossref_work.json");
        let t = MockTransport::new().on("api.crossref.org/works/", 200, cassette);

        let (metrics, metadata) = match Crossref.fetch(&doi(), &t) {
            Outcome::Values { metrics, metadata } => (metrics, metadata),
            other => panic!("expected Values, got {other:?}"),
        };

        let cites = metrics.iter().find(|m| m.name == "citations").unwrap();
        assert_eq!(cites.value, MetricValue::Count(1161));
        assert_eq!(cites.provider, "crossref");
        assert_eq!(cites.category, Category::Citations);

        let md = metadata.expect("metadata present");
        assert_eq!(
            md.title.as_deref(),
            Some("Big Data: Astronomical or Genomical?")
        );
        assert_eq!(md.container.as_deref(), Some("PLOS Biology"));
        assert_eq!(md.year, Some(2015));
        assert_eq!(
            md.authors.first().map(String::as_str),
            Some("Zachary D. Stephens")
        );
        assert_eq!(md.authors.len(), 3);
    }

    #[test]
    fn missing_doi_record_is_not_applicable_not_zero() {
        let t = MockTransport::new().on("api.crossref.org/works/", 404, "Resource not found.");
        assert!(matches!(
            Crossref.fetch(&doi(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn does_not_support_pmid() {
        let pmid = Identity::Paper(PaperId::Pmid("31234567".into()));
        assert!(!Crossref.supports(&pmid));
        assert!(Crossref.supports(&doi()));
    }

    #[test]
    fn server_error_and_transport_error_are_failed() {
        let t500 = MockTransport::new().on("api.crossref.org/works/", 503, "");
        assert!(matches!(
            Crossref.fetch(&doi(), &t500),
            Outcome::Failed { .. }
        ));

        let terr =
            MockTransport::new().on_error("api.crossref.org/works/", TransportError::Timeout);
        assert!(matches!(
            Crossref.fetch(&doi(), &terr),
            Outcome::Failed { .. }
        ));
    }
}
