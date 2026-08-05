//! Docker Hub Provider for container image pull counts: the image's cumulative
//! pull total across every tag, for any namespace on Docker Hub — see ADR-0003.
//!
//! A `docker` package Identity names its namespace in the package name itself
//! (`namespace/name`, e.g. `biocontainers/samtools`), since every image lives
//! under an account or organisation; official images sit under `library`, so
//! `ubuntu` is `docker:library/ubuntu`. `PackageId::parse` already rejects a
//! `docker` name without a namespace, so a missing split here should be
//! unreachable in practice, but is handled defensively rather than panicking.
//!
//! The count is cumulative and so joins the Downloads Rollup alongside
//! crates.io, Anaconda.org, PyPI, and GitHub release assets. It is a weaker
//! unit than those, though — see [`PULL_SEMANTICS_NOTE`] — which is why every
//! Metric carries that caveat rather than standing on its own.

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PackageId, Registry, Window};
use crate::provider::{classify_status, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://hub.docker.com/v2/repositories/";

pub const NAME: &str = "dockerhub";

/// Why a pull total is not an install total. Docker Hub counts manifest
/// fetches, so CI re-pulls, layer probes, and mirror warming all land in the
/// same figure, and it never resets — `library/ubuntu` sits near ten billion.
/// Deliberately longer than [`crate::report::INLINE_DETAIL_LIMIT`] so it
/// renders in the Notices footer (ADR-0005) rather than being truncated into
/// an inline detail column, since the caveat is the point.
pub const PULL_SEMANTICS_NOTE: &str =
    "Docker Hub pull counts record image fetches by machines, not installs by people: \
     CI re-pulls and mirror warming inflate the figure, and it never resets";

pub struct DockerHub;

#[derive(Debug, Deserialize)]
struct DockerRepository {
    pull_count: Option<u64>,
}

impl DockerHub {
    fn package(identity: &Identity) -> Option<&PackageId> {
        match identity {
            Identity::Package(p) if p.registry == Registry::Docker => Some(p),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let repo: DockerRepository = match serde_json::from_str(body) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Docker Hub response: {e}"),
                }
            }
        };

        match repo.pull_count {
            Some(pulls) => Outcome::Values {
                metrics: vec![Metric {
                    name: "downloads".into(),
                    category: Category::Downloads,
                    value: MetricValue::Count(pulls),
                    window: Window::Cumulative,
                    provider: NAME.into(),
                    identity: canonical.into(),
                    as_of: OffsetDateTime::now_utc(),
                    source: url.into(),
                    note: Some(PULL_SEMANTICS_NOTE.into()),
                }],
                metadata: None,
            },
            None => Outcome::NotApplicable {
                note: "Docker Hub returned no pull count".into(),
            },
        }
    }
}

impl Provider for DockerHub {
    fn name(&self) -> &'static str {
        NAME
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
                note: "Docker Hub only supports docker packages".into(),
            };
        };
        if package.name.split_once('/').is_none() {
            return Outcome::NotApplicable {
                note: "docker package name must be 'namespace/name'".into(),
            };
        }
        // Docker Hub's v2 API requires the trailing slash; without it the
        // request redirects and the transport surfaces the redirect instead.
        let url = format!("{API_BASE}{}/", package.name);
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match classify_status(resp.status, "Docker Hub", "not found on Docker Hub") {
            Some(outcome) => outcome,
            None => Self::classify(&resp.body, &url, &canonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PaperId, RepoHost, RepoId};
    use crate::transport::{MockTransport, TransportError};

    fn package(name: &str) -> Identity {
        Identity::Package(PackageId {
            registry: Registry::Docker,
            name: name.into(),
        })
    }

    fn values(outcome: Outcome) -> Vec<Metric> {
        match outcome {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        }
    }

    #[test]
    fn parses_pull_count_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/docker_biocontainers_samtools.json");
        let t = MockTransport::new().on(
            "hub.docker.com/v2/repositories/biocontainers/samtools/",
            200,
            cassette,
        );

        let metrics = values(DockerHub.fetch(&package("biocontainers/samtools"), &t));

        assert_eq!(metrics.len(), 1);
        let m = &metrics[0];
        assert_eq!(m.name, "downloads");
        assert_eq!(m.category, Category::Downloads);
        assert_eq!(m.value, MetricValue::Count(596_335));
        assert_eq!(m.provider, "dockerhub");
        assert_eq!(m.identity, "docker:biocontainers/samtools");
    }

    /// The cumulative Window is what admits this Metric to the Downloads
    /// Rollup, so it is asserted rather than left implicit.
    #[test]
    fn pull_count_is_cumulative_so_it_can_roll_up() {
        let cassette = include_str!("../../tests/cassettes/docker_biocontainers_samtools.json");
        let t = MockTransport::new().on("hub.docker.com", 200, cassette);

        let metrics = values(DockerHub.fetch(&package("biocontainers/samtools"), &t));
        assert_eq!(metrics[0].window, Window::Cumulative);
        assert!(crate::rollup::counts_as_download(&metrics[0]));
    }

    /// A pull is a weaker unit than an install, and the Rollup sums it in
    /// regardless, so the caveat must travel with every Metric.
    #[test]
    fn every_metric_carries_the_pull_semantics_caveat() {
        let cassette = include_str!("../../tests/cassettes/docker_biocontainers_samtools.json");
        let t = MockTransport::new().on("hub.docker.com", 200, cassette);

        let metrics = values(DockerHub.fetch(&package("biocontainers/samtools"), &t));
        assert_eq!(metrics[0].note.as_deref(), Some(PULL_SEMANTICS_NOTE));
        assert!(
            PULL_SEMANTICS_NOTE.len() > crate::report::INLINE_DETAIL_LIMIT,
            "the caveat must reach the Notices footer, not be squeezed inline"
        );
    }

    #[test]
    fn a_missing_pull_count_is_not_applicable_never_zero() {
        let t = MockTransport::new().on("hub.docker.com", 200, r#"{"name": "samtools"}"#);

        match DockerHub.fetch(&package("biocontainers/samtools"), &t) {
            Outcome::NotApplicable { note } => assert!(note.contains("no pull count")),
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_image_is_not_applicable() {
        let t = MockTransport::new().on("hub.docker.com", 404, "");

        match DockerHub.fetch(&package("biocontainers/nope"), &t) {
            Outcome::NotApplicable { note } => assert!(note.contains("not found on Docker Hub")),
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_fails_rather_than_reporting_a_value() {
        let t = MockTransport::new().on("hub.docker.com", 200, "not json");

        match DockerHub.fetch(&package("biocontainers/samtools"), &t) {
            Outcome::Failed { error } => assert!(error.contains("unexpected Docker Hub response")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn a_transport_error_fails_rather_than_reporting_a_value() {
        let t = MockTransport::new().on_error("hub.docker.com", TransportError::ConnectionFailed);

        match DockerHub.fetch(&package("biocontainers/samtools"), &t) {
            Outcome::Failed { error } => assert!(error.contains("connection failed")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn supports_only_docker_packages() {
        assert!(DockerHub.supports(&package("biocontainers/samtools")));
        assert!(!DockerHub.supports(&Identity::Package(PackageId {
            registry: Registry::Conda,
            name: "bioconda/samtools".into(),
        })));
        assert!(!DockerHub.supports(&Identity::Repo(RepoId {
            host: RepoHost::GitHub,
            owner: "lh3".into(),
            name: "minimap2".into(),
        })));
        assert!(!DockerHub.supports(&Identity::Paper(PaperId::Doi("10.0/0".into()))));
    }
}
