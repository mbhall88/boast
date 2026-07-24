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
        // OpenAlex, Dimensions, and Europe PMC return metrics; Crossref has no record for this DOI.
        let t = MockTransport::new()
            .on("api.openalex.org/works/doi:", 200, body)
            .on("api.crossref.org/works/", 404, "")
            .on(
                "metrics-api.dimensions.ai/doi/",
                200,
                r#"{"times_cited": 5, "field_citation_ratio": null, "relative_citation_ratio": null, "license": null}"#,
            )
            .on(
                "ebi.ac.uk/europepmc/webservices/rest/search",
                200,
                r#"{"hitCount":1,"resultList":{"result":[{"citedByCount":3}]}}"#,
            )
            .on(
                "en.wikipedia.org/w/api.php",
                200,
                r#"{"query":{"searchinfo":{"totalhits":2}}}"#,
            )
            // Only reached if ALTMETRIC_KEY happens to be set locally.
            .on("api.altmetric.com/v1/fetch/doi/", 200, r#"{"score":1.0}"#);

        let snap = run(&project(), &default_providers(), &t);

        assert_eq!(snap.schema_version, Snapshot::SCHEMA_VERSION);
        assert_eq!(snap.identities, vec!["doi:10.1/x".to_string()]);
        assert!(!snap.has_failures());
        // OpenAlex(2) + Dimensions(1) + Europe PMC(1) + Wikipedia(1); Crossref
        // has no record. Altmetric contributes 0 unless ALTMETRIC_KEY happens
        // to be set in the environment running this test (see tests/skeleton.rs
        // for the same guard against that ambient-env dependency).
        assert_eq!(snap.metrics().count(), 5 + altmetric_metric_count());
    }

    /// How many Metrics `Altmetric::new()` contributes in these tests' mocked
    /// `{"score":1.0}` response: 0 unless `ALTMETRIC_KEY` happens to be set in
    /// the environment running the test (in which case it's a real fetch, not
    /// the early no-key `NotApplicable`).
    fn altmetric_metric_count() -> usize {
        let has_key = std::env::var("ALTMETRIC_KEY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if has_key {
            1
        } else {
            0
        }
    }

    #[test]
    fn records_failure_without_aborting() {
        // OpenAlex fails; Crossref, Dimensions, and Europe PMC are fine — one
        // dead Provider must not block others.
        let t = MockTransport::new()
            .on_error(
                "api.openalex.org/works/doi:",
                TransportError::ConnectionFailed,
            )
            .on("api.crossref.org/works/", 404, "")
            .on("metrics-api.dimensions.ai/doi/", 404, "")
            .on(
                "ebi.ac.uk/europepmc/webservices/rest/search",
                200,
                r#"{"hitCount":0,"resultList":{"result":[]}}"#,
            )
            .on("en.wikipedia.org/w/api.php", 404, "")
            // Only reached if ALTMETRIC_KEY happens to be set locally.
            .on("api.altmetric.com/v1/fetch/doi/", 404, "");
        let snap = run(&project(), &default_providers(), &t);

        assert!(snap.has_failures());
        assert_eq!(snap.metrics().count(), 0);
        assert!(matches!(snap.results[0].outcome, Outcome::Failed { .. }));
    }

    /// A route for every citation Provider other than OpenAlex, so a
    /// `RetryingTransport` composition test can isolate what happens to
    /// OpenAlex's own Outcome without the others panicking on an unmatched URL.
    fn transport_with_only_openalex_left_open(openalex: MockTransport) -> MockTransport {
        openalex
            .on("api.crossref.org/works/", 404, "")
            .on("metrics-api.dimensions.ai/doi/", 404, "")
            .on(
                "ebi.ac.uk/europepmc/webservices/rest/search",
                200,
                r#"{"hitCount":0,"resultList":{"result":[]}}"#,
            )
            .on("en.wikipedia.org/w/api.php", 404, "")
            // Only reached if ALTMETRIC_KEY happens to be set locally.
            .on("api.altmetric.com/v1/fetch/doi/", 404, "")
    }

    #[test]
    fn a_429_then_200_composes_through_retry_and_provider_into_a_real_outcome_value() {
        // Exercises issue #3 AC1 end-to-end: a 429 then a 200 must reach the
        // orchestrator as an `Outcome::Values`, not just a 200 `HttpResponse`.
        let body =
            r#"{"cited_by_count": 10, "fwci": null, "citation_normalized_percentile": null}"#;
        let mock = transport_with_only_openalex_left_open(
            MockTransport::new()
                .on_sequence("api.openalex.org/works/doi:", &[(429, ""), (200, body)]),
        );
        let transport = crate::transport::RetryingTransport::with_policy_and_sleep(
            mock,
            Default::default(),
            |_| {},
        );

        let snap = run(&project(), &default_providers(), &transport);

        assert!(!snap.has_failures());
        let openalex = snap
            .results
            .iter()
            .find(|r| r.provider == "openalex")
            .unwrap();
        assert!(matches!(openalex.outcome, Outcome::Values { .. }));
    }

    #[test]
    fn persistent_429_composes_through_retry_and_provider_into_a_bounded_failed_outcome() {
        // Exercises issue #3 AC2 end-to-end: retries must be bounded (never
        // forever) and still resolve to a real `Outcome::Failed`.
        let mock = transport_with_only_openalex_left_open(MockTransport::new().on(
            "api.openalex.org/works/doi:",
            429,
            "",
        ));
        let policy = crate::transport::RetryPolicy {
            max_retries: 2,
            base_delay: std::time::Duration::from_millis(1),
        };
        let transport =
            crate::transport::RetryingTransport::with_policy_and_sleep(mock, policy, |_| {});

        let snap = run(&project(), &default_providers(), &transport);

        assert!(snap.has_failures());
        let openalex = snap
            .results
            .iter()
            .find(|r| r.provider == "openalex")
            .unwrap();
        assert!(matches!(openalex.outcome, Outcome::Failed { .. }));
    }
}
