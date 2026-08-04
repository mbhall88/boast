//! PyPI Provider for package download counts, via pypistats.org: the package's downloads
//! for pypistats' own `last_month` recency bucket, reported as a trailing
//! 30-day Window since pypistats doesn't publish an exact day boundary for it
//! (CONTEXT.md classes "PyPI last-month" as trailing; ADR-0003).

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PackageId, Registry, Window};
use crate::provider::{classify_status, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://pypistats.org/api/packages/";

pub struct Pypi;

#[derive(Debug, Deserialize)]
struct RecentDownloads {
    data: Option<RecentData>,
}

#[derive(Debug, Deserialize)]
struct RecentData {
    last_month: Option<u64>,
}

impl Pypi {
    fn package(identity: &Identity) -> Option<&PackageId> {
        match identity {
            Identity::Package(p) if p.registry == Registry::Pypi => Some(p),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let resp: RecentDownloads = match serde_json::from_str(body) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected PyPI response: {e}"),
                }
            }
        };

        match resp.data.and_then(|d| d.last_month) {
            Some(downloads) => Outcome::Values {
                metrics: vec![Metric {
                    name: "downloads".into(),
                    category: Category::Downloads,
                    value: MetricValue::Count(downloads),
                    window: Window::Trailing { days: 30 },
                    provider: "pypi".into(),
                    identity: canonical.into(),
                    as_of: OffsetDateTime::now_utc(),
                    source: url.into(),
                    note: Some(
                        "pypistats' \"last month\" bucket; treated as trailing 30 days since \
                         pypistats doesn't publish an exact day boundary for it"
                            .into(),
                    ),
                }],
                metadata: None,
            },
            None => Outcome::NotApplicable {
                note: "PyPI returned no download count".into(),
            },
        }
    }
}

impl Provider for Pypi {
    fn name(&self) -> &'static str {
        "pypi"
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
                note: "PyPI only supports pypi packages".into(),
            };
        };
        let url = format!("{API_BASE}{}/recent", package.name);
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match classify_status(resp.status, "PyPI", "not found on PyPI") {
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
            registry: Registry::Pypi,
            name: "pysam".into(),
        })
    }

    #[test]
    fn parses_downloads_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/pypi_pysam.json");
        let t = MockTransport::new().on("pypistats.org/api/packages/pysam/recent", 200, cassette);

        let outcome = Pypi.fetch(&package(), &t);
        let metrics = match outcome {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 1);
        let downloads = &metrics[0];
        assert_eq!(downloads.name, "downloads");
        assert_eq!(downloads.category, Category::Downloads);
        assert_eq!(downloads.value, MetricValue::Count(1_150_697));
        assert_eq!(downloads.window, Window::Trailing { days: 30 });
        assert_eq!(downloads.provider, "pypi");
        assert_eq!(downloads.identity, "pypi:pysam");
    }

    #[test]
    fn nonexistent_package_is_not_applicable_not_zero() {
        let t = MockTransport::new().on("pypistats.org/api/packages/", 404, "404");
        assert!(matches!(
            Pypi.fetch(&package(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn rate_limit_and_server_error_are_failed() {
        let t429 = MockTransport::new().on("pypistats.org/api/packages/", 429, "");
        assert!(matches!(
            Pypi.fetch(&package(), &t429),
            Outcome::Failed { .. }
        ));

        let t503 = MockTransport::new().on("pypistats.org/api/packages/", 503, "");
        assert!(matches!(
            Pypi.fetch(&package(), &t503),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn transport_error_is_failed() {
        let t = MockTransport::new().on_error(
            "pypistats.org/api/packages/",
            TransportError::ConnectionFailed,
        );
        assert!(matches!(Pypi.fetch(&package(), &t), Outcome::Failed { .. }));
    }

    #[test]
    fn does_not_support_papers_repos_or_other_registries() {
        let doi = Identity::Paper(PaperId::Doi("10.1/x".into()));
        let repo = Identity::Repo(RepoId::parse("owner/name").unwrap());
        let crate_pkg = Identity::Package(PackageId {
            registry: Registry::Crates,
            name: "boast".into(),
        });
        assert!(!Pypi.supports(&doi));
        assert!(!Pypi.supports(&repo));
        assert!(!Pypi.supports(&crate_pkg));
        assert!(Pypi.supports(&package()));
    }
}
