//! crates.io Provider for crate download counts: the crate's cumulative download total —
//! the first Downloads Category Provider (ADR-0003).

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PackageId, Registry, Window};
use crate::provider::{classify_status, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://crates.io/api/v1/crates/";

pub struct CratesIo;

#[derive(Debug, Deserialize)]
struct CrateEnvelope {
    #[serde(rename = "crate")]
    krate: CrateInfo,
}

#[derive(Debug, Deserialize)]
struct CrateInfo {
    downloads: Option<u64>,
}

impl CratesIo {
    fn package(identity: &Identity) -> Option<&PackageId> {
        match identity {
            Identity::Package(p) if p.registry == Registry::Crates => Some(p),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let env: CrateEnvelope = match serde_json::from_str(body) {
            Ok(e) => e,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected crates.io response: {e}"),
                }
            }
        };

        match env.krate.downloads {
            Some(downloads) => Outcome::Values {
                metrics: vec![Metric {
                    name: "downloads".into(),
                    category: Category::Downloads,
                    value: MetricValue::Count(downloads),
                    window: Window::Cumulative,
                    provider: "crates.io".into(),
                    identity: canonical.into(),
                    as_of: OffsetDateTime::now_utc(),
                    source: url.into(),
                    note: None,
                }],
                metadata: None,
            },
            None => Outcome::NotApplicable {
                note: "crates.io returned no download count".into(),
            },
        }
    }
}

impl Provider for CratesIo {
    fn name(&self) -> &'static str {
        "crates.io"
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
                note: "crates.io only supports crates.io packages".into(),
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

        match classify_status(resp.status, "crates.io", "not found on crates.io") {
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
            registry: Registry::Crates,
            name: "boast".into(),
        })
    }

    #[test]
    fn parses_downloads_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/crates_boast.json");
        let t = MockTransport::new().on("crates.io/api/v1/crates/boast", 200, cassette);

        let outcome = CratesIo.fetch(&package(), &t);
        let metrics = match outcome {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 1);
        let downloads = &metrics[0];
        assert_eq!(downloads.name, "downloads");
        assert_eq!(downloads.category, Category::Downloads);
        assert_eq!(downloads.value, MetricValue::Count(842_617));
        assert_eq!(downloads.window, Window::Cumulative);
        assert_eq!(downloads.provider, "crates.io");
        assert_eq!(downloads.identity, "crates:boast");
    }

    #[test]
    fn nonexistent_crate_is_not_applicable_not_zero() {
        let t = MockTransport::new().on(
            "crates.io/api/v1/crates/",
            404,
            r#"{"errors":[{"detail":"Not Found"}]}"#,
        );
        assert!(matches!(
            CratesIo.fetch(&package(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn rate_limit_and_server_error_are_failed() {
        let t429 = MockTransport::new().on("crates.io/api/v1/crates/", 429, "");
        assert!(matches!(
            CratesIo.fetch(&package(), &t429),
            Outcome::Failed { .. }
        ));

        let t503 = MockTransport::new().on("crates.io/api/v1/crates/", 503, "");
        assert!(matches!(
            CratesIo.fetch(&package(), &t503),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn transport_error_is_failed() {
        let t = MockTransport::new()
            .on_error("crates.io/api/v1/crates/", TransportError::ConnectionFailed);
        assert!(matches!(
            CratesIo.fetch(&package(), &t),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn does_not_support_papers_or_repos() {
        let doi = Identity::Paper(PaperId::Doi("10.1/x".into()));
        let repo = Identity::Repo(RepoId::parse("owner/name").unwrap());
        assert!(!CratesIo.supports(&doi));
        assert!(!CratesIo.supports(&repo));
        assert!(CratesIo.supports(&package()));
    }
}
