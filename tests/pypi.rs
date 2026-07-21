//! End-to-end test of a PyPI package Identity flowing through the
//! orchestrator into the Downloads Category — driven from a recorded
//! pypistats.org response.

use boast::model::{Identity, PackageId, Project, Registry, Window};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::MockTransport;
use boast::Category;

#[test]
fn pypi_package_flows_into_downloads_category() {
    let cassette = include_str!("cassettes/pypi_pysam.json");
    let transport =
        MockTransport::new().on("pypistats.org/api/packages/pysam/recent", 200, cassette);

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Pypi,
        name: "pysam".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    assert!(!snapshot.has_failures());

    let downloads: Vec<_> = snapshot
        .metrics()
        .filter(|m| m.category == Category::Downloads)
        .collect();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].provider, "pypi");
    assert_eq!(downloads[0].window, Window::Trailing { days: 30 });

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Downloads ──"));
    assert!(report.contains("1150697"));
}

#[test]
fn nonexistent_pypi_package_is_not_applicable_never_zero() {
    let transport = MockTransport::new().on("pypistats.org/api/packages/", 404, "404");

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Pypi,
        name: "does-not-exist-xyz".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    assert!(!snapshot.has_failures());
    assert_eq!(snapshot.metrics().count(), 0);

    let report = render_terminal(&snapshot);
    assert!(report.contains("N/A"));
    assert!(!report.contains(" 0 "));
}
