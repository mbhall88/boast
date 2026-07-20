//! Runs the enabled Providers over a Project's Identities and assembles a
//! Snapshot. Best-effort: one Provider's failure never blocks the others; each
//! Provider×Identity fetch is recorded as its own Outcome (ADR-0002).

use time::OffsetDateTime;
use tracing::{debug, warn};

use crate::model::{FetchResult, Outcome, Project, Snapshot};
use crate::provider::Provider;
use crate::transport::Transport;

/// Fetch every applicable Provider for every Identity and return a Snapshot.
pub fn run(
    project: &Project,
    providers: &[Box<dyn Provider>],
    transport: &dyn Transport,
) -> Snapshot {
    let mut results = Vec::new();

    for identity in &project.identities {
        let canonical = identity.canonical();
        for provider in providers {
            if !provider.supports(identity) {
                continue;
            }
            debug!(provider = provider.name(), identity = %canonical, "fetching");
            let outcome = provider.fetch(identity, transport);
            match &outcome {
                Outcome::Values { metrics, .. } => {
                    debug!(provider = provider.name(), identity = %canonical, metrics = metrics.len(), "fetched")
                }
                Outcome::NotApplicable { note } => {
                    debug!(provider = provider.name(), identity = %canonical, %note, "not applicable")
                }
                Outcome::Failed { error } => {
                    warn!(provider = provider.name(), identity = %canonical, %error, "fetch failed")
                }
            }
            results.push(FetchResult {
                provider: provider.name().to_string(),
                identity: canonical.clone(),
                category: provider.category(),
                outcome,
            });
        }
    }

    Snapshot {
        schema_version: Snapshot::SCHEMA_VERSION,
        tool: "boast".to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: OffsetDateTime::now_utc(),
        identities: project.identities.iter().map(|i| i.canonical()).collect(),
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Identity, Outcome, PaperId};
    use crate::providers::default_providers;
    use crate::transport::{MockTransport, TransportError};

    fn project() -> Project {
        Project::new(vec![Identity::Paper(PaperId::Doi("10.1/x".into()))])
    }

    #[test]
    fn assembles_snapshot_with_metrics_and_no_failures() {
        let body = r#"{"cited_by_count": 10, "fwci": 2.0, "citation_normalized_percentile": null}"#;
        // OpenAlex returns metrics; Crossref has no record for this DOI.
        let t = MockTransport::new()
            .on("api.openalex.org/works/doi:", 200, body)
            .on("api.crossref.org/works/", 404, "");

        let snap = run(&project(), &default_providers(), &t);

        assert_eq!(snap.schema_version, Snapshot::SCHEMA_VERSION);
        assert_eq!(snap.identities, vec!["doi:10.1/x".to_string()]);
        assert!(!snap.has_failures());
        assert_eq!(snap.metrics().count(), 2);
    }

    #[test]
    fn records_failure_without_aborting() {
        // OpenAlex fails; Crossref is fine — one dead Provider must not block others.
        let t = MockTransport::new()
            .on_error(
                "api.openalex.org/works/doi:",
                TransportError::ConnectionFailed,
            )
            .on("api.crossref.org/works/", 404, "");
        let snap = run(&project(), &default_providers(), &t);

        assert!(snap.has_failures());
        assert_eq!(snap.metrics().count(), 0);
        assert!(matches!(snap.results[0].outcome, Outcome::Failed { .. }));
    }
}
