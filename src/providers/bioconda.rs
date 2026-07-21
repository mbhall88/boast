//! Bioconda/anaconda.org download-count Provider: the package's cumulative
//! download total across all versions and platforms (ADR-0003).

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PackageId, Registry, Window};
use crate::provider::{classify_status, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://api.anaconda.org/package/bioconda/";

pub struct Bioconda;

#[derive(Debug, Deserialize)]
struct AnacondaPackage {
    ndownloads: Option<u64>,
}

impl Bioconda {
    fn package(identity: &Identity) -> Option<&PackageId> {
        match identity {
            Identity::Package(p) if p.registry == Registry::Bioconda => Some(p),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let pkg: AnacondaPackage = match serde_json::from_str(body) {
            Ok(p) => p,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Bioconda response: {e}"),
                }
            }
        };

        match pkg.ndownloads {
            Some(downloads) => Outcome::Values {
                metrics: vec![Metric {
                    name: "downloads".into(),
                    category: Category::Downloads,
                    value: MetricValue::Count(downloads),
                    window: Window::Cumulative,
                    provider: "bioconda".into(),
                    identity: canonical.into(),
                    as_of: OffsetDateTime::now_utc(),
                    source: url.into(),
                    note: None,
                }],
                metadata: None,
            },
            None => Outcome::NotApplicable {
                note: "Bioconda returned no download count".into(),
            },
        }
    }
}

impl Provider for Bioconda {
    fn name(&self) -> &'static str {
        "bioconda"
    }

    fn category(&self) -> Category {
        Category::Downloads
    }

    fn supports(&self, identity: &Identity) -> bool {
        Self::package(identity).is_some()
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let Some(package) = Self::package(identity) else {
            return Outcome::NotApplicable {
                note: "Bioconda only supports bioconda packages".into(),
            };
        };
        let url = format!("{API_BASE}{}", package.name);
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match classify_status(resp.status, "Bioconda", "not found on Bioconda") {
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

    fn package() -> Identity {
        Identity::Package(PackageId {
            registry: Registry::Bioconda,
            name: "samtools".into(),
        })
    }

    #[test]
    fn parses_downloads_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/bioconda_samtools.json");
        let t =
            MockTransport::new().on("api.anaconda.org/package/bioconda/samtools", 200, cassette);

        let outcome = Bioconda.fetch(&package(), &t);
        let metrics = match outcome {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 1);
        let downloads = &metrics[0];
        assert_eq!(downloads.name, "downloads");
        assert_eq!(downloads.category, Category::Downloads);
        assert_eq!(downloads.value, MetricValue::Count(8_897_787));
        assert_eq!(downloads.window, Window::Cumulative);
        assert_eq!(downloads.provider, "bioconda");
        assert_eq!(downloads.identity, "bioconda:samtools");
    }

    #[test]
    fn nonexistent_package_is_not_applicable_not_zero() {
        let t = MockTransport::new().on(
            "api.anaconda.org/package/bioconda/",
            404,
            r#"{"error":"could not be found"}"#,
        );
        assert!(matches!(
            Bioconda.fetch(&package(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn rate_limit_and_server_error_are_failed() {
        let t429 = MockTransport::new().on("api.anaconda.org/package/bioconda/", 429, "");
        assert!(matches!(
            Bioconda.fetch(&package(), &t429),
            Outcome::Failed { .. }
        ));

        let t503 = MockTransport::new().on("api.anaconda.org/package/bioconda/", 503, "");
        assert!(matches!(
            Bioconda.fetch(&package(), &t503),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn transport_error_is_failed() {
        let t = MockTransport::new().on_error(
            "api.anaconda.org/package/bioconda/",
            TransportError::ConnectionFailed,
        );
        assert!(matches!(
            Bioconda.fetch(&package(), &t),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn does_not_support_papers_repos_or_other_registries() {
        let doi = Identity::Paper(PaperId::Doi("10.1/x".into()));
        let repo = Identity::Repo(RepoId::parse("owner/name").unwrap());
        let crate_pkg = Identity::Package(PackageId {
            registry: Registry::Crates,
            name: "boast".into(),
        });
        assert!(!Bioconda.supports(&doi));
        assert!(!Bioconda.supports(&repo));
        assert!(!Bioconda.supports(&crate_pkg));
        assert!(Bioconda.supports(&package()));
    }
}
