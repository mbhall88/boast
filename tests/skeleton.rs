//! End-to-end test of the walking skeleton over the HTTP-transport seam:
//! a recorded OpenAlex response drives a Project through the orchestrator into
//! a Snapshot and a rendered Report — with no network access.

use boast::model::{Identity, PaperId, Project};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::{MockTransport, TransportError};

fn paper() -> Project {
    Project::new(vec![Identity::Paper(PaperId::Doi(
        "10.1371/journal.pbio.1002195".into(),
    ))])
}

#[test]
fn cassette_drives_full_pipeline() {
    let cassette = include_str!("cassettes/openalex_work.json");
    let transport = MockTransport::new().on("api.openalex.org/works/doi:", 200, cassette);

    let snapshot = orchestrator::run(&paper(), &default_providers(), &transport);

    assert_eq!(snapshot.schema_version, boast::Snapshot::SCHEMA_VERSION);
    assert!(!snapshot.has_failures());
    assert_eq!(snapshot.metrics().count(), 3);

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Citations ──"));
    assert!(report.contains("1421")); // citations
    assert!(report.contains("59.91")); // fwci
    assert!(!report.contains("partial snapshot"));
}

#[test]
fn transport_failure_yields_failed_outcome_and_partial_report() {
    let transport =
        MockTransport::new().on_error("api.openalex.org/works/doi:", TransportError::Timeout);

    let snapshot = orchestrator::run(&paper(), &default_providers(), &transport);

    assert!(snapshot.has_failures()); // drives the non-zero exit code
    assert_eq!(snapshot.metrics().count(), 0);
    assert!(render_terminal(&snapshot).contains("partial snapshot"));
}
