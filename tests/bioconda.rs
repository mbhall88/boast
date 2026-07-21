//! End-to-end test of a Bioconda package Identity flowing through the
//! orchestrator into the Downloads Category — driven from a recorded
//! anaconda.org response.

use boast::model::{Identity, PackageId, Project, Registry};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::MockTransport;
use boast::Category;

#[test]
fn bioconda_package_flows_into_downloads_category() {
    let cassette = include_str!("cassettes/bioconda_samtools.json");
    let transport =
        MockTransport::new().on("api.anaconda.org/package/bioconda/samtools", 200, cassette);

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Bioconda,
        name: "samtools".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    assert!(!snapshot.has_failures());

    let downloads: Vec<_> = snapshot
        .metrics()
        .filter(|m| m.category == Category::Downloads)
        .collect();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].provider, "bioconda");

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Downloads ──"));
    assert!(report.contains("8897787"));
}

#[test]
fn nonexistent_bioconda_package_is_not_applicable_never_zero() {
    let transport = MockTransport::new().on(
        "api.anaconda.org/package/bioconda/",
        404,
        r#"{"error":"could not be found"}"#,
    );

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Bioconda,
        name: "does-not-exist-xyz".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    assert!(!snapshot.has_failures());
    assert_eq!(snapshot.metrics().count(), 0);

    let report = render_terminal(&snapshot);
    assert!(report.contains("N/A"));
    assert!(!report.contains(" 0 "));
}
