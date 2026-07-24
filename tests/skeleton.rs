//! End-to-end test of the pipeline over the HTTP-transport seam: recorded
//! OpenAlex, Crossref, Dimensions, and Europe PMC responses drive a Project
//! through the orchestrator into a Snapshot and a rendered Report — with no
//! network access.

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
fn cassettes_drive_full_pipeline() {
    let openalex = include_str!("cassettes/openalex_work.json");
    let crossref = include_str!("cassettes/crossref_work.json");
    let dimensions = include_str!("cassettes/dimensions_metrics.json");
    let europe_pmc = include_str!("cassettes/europe_pmc_search.json");
    let wikipedia = include_str!("cassettes/wikipedia_search.json");
    let altmetric = include_str!("cassettes/altmetric_fetch.json");
    let transport = MockTransport::new()
        .on("api.openalex.org/works/doi:", 200, openalex)
        .on("api.crossref.org/works/", 200, crossref)
        .on("metrics-api.dimensions.ai/doi/", 200, dimensions)
        .on(
            "ebi.ac.uk/europepmc/webservices/rest/search",
            200,
            europe_pmc,
        )
        .on("en.wikipedia.org/w/api.php", 200, wikipedia)
        // Only reached if ALTMETRIC_KEY happens to be set in the environment
        // running this test; otherwise Altmetric never calls the transport.
        .on("api.altmetric.com/v1/fetch/doi/", 200, altmetric);

    let snapshot = orchestrator::run(&paper(), &default_providers(), &transport);

    assert_eq!(snapshot.schema_version, boast::Snapshot::SCHEMA_VERSION);
    assert!(!snapshot.has_failures());
    // 4 from OpenAlex (citations, fwci, percentile, open_access) + 1 from Crossref
    // (citations) + 4 from Dimensions (citations, recent_citations, fcr, rcr)
    // + 1 from Europe PMC (citations) + 1 from Wikipedia (wikipedia_mentions).
    // Altmetric contributes 0 unless ALTMETRIC_KEY happens to be set locally.
    let expect_altmetric = std::env::var("ALTMETRIC_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let expected = 11 + if expect_altmetric { 7 } else { 0 };
    assert_eq!(snapshot.metrics().count(), expected);
    // Crossref contributes one paper description.
    assert_eq!(snapshot.descriptions().count(), 1);

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Citations ──"));
    assert!(report.contains("1421")); // OpenAlex citations
    assert!(report.contains("1161")); // Crossref citations
    assert!(report.contains("1285")); // Dimensions citations
    assert!(report.contains("161")); // Dimensions recent_citations
    assert!(report.contains("116.66")); // Dimensions FCR
    assert!(report.contains("15.96")); // Dimensions RCR
    assert!(report.contains("581")); // Europe PMC citations
    assert!(report.contains("Big Data: Astronomical or Genomical?")); // metadata title
    assert!(report.contains("PLOS Biology")); // metadata journal
    assert!(report.contains("Dimensions Metrics API")); // licence/terms notice, never hidden
    assert!(report.contains("── Attention ──"));
    assert!(report.contains("open_access"));
    assert!(report.contains("gold")); // OpenAlex open-access status
    assert!(report.contains("wikipedia_mentions"));
    if expect_altmetric {
        assert!(report.contains("attention_score"));
    } else {
        // Issue #15 AC3: absence must be explicit in the Report, not silent.
        assert!(report.contains("not collected"));
        assert!(report.contains("ALTMETRIC_KEY"));
    }
    assert!(!report.contains("partial snapshot"));
}

#[test]
fn transport_failure_yields_failed_outcome_and_partial_report() {
    let transport = MockTransport::new()
        .on_error("api.openalex.org/works/doi:", TransportError::Timeout)
        .on_error("api.crossref.org/works/", TransportError::Timeout)
        .on_error("metrics-api.dimensions.ai/doi/", TransportError::Timeout)
        .on_error(
            "ebi.ac.uk/europepmc/webservices/rest/search",
            TransportError::Timeout,
        )
        .on_error("en.wikipedia.org/w/api.php", TransportError::Timeout)
        // Only reached if ALTMETRIC_KEY happens to be set in the environment
        // running this test; otherwise Altmetric never calls the transport.
        .on_error("api.altmetric.com/v1/fetch/doi/", TransportError::Timeout);

    let snapshot = orchestrator::run(&paper(), &default_providers(), &transport);

    assert!(snapshot.has_failures()); // drives the non-zero exit code
    assert_eq!(snapshot.metrics().count(), 0);
    assert!(render_terminal(&snapshot).contains("partial snapshot"));
}
