//! End-to-end test of a package Identity flowing through the orchestrator
//! into the Downloads Category — driven entirely from a recorded crates.io
//! response.

use boast::model::{Identity, PackageId, Project, Registry};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::MockTransport;
use boast::Category;

#[test]
fn crates_io_package_flows_into_downloads_category() {
    let cassette = include_str!("cassettes/crates_boast.json");
    let transport = MockTransport::new().on("crates.io/api/v1/crates/boast", 200, cassette);

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Crates,
        name: "boast".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    // Only the crates.io Provider supports a package Identity, so no
    // failures arise from unmocked paper/repo endpoints.
    assert!(!snapshot.has_failures());

    let downloads: Vec<_> = snapshot
        .metrics()
        .filter(|m| m.category == Category::Downloads)
        .collect();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].name, "downloads");
    assert_eq!(downloads[0].provider, "crates.io");

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Downloads ──"));
    assert!(report.contains("842617"));
}

#[test]
fn nonexistent_crate_is_reported_as_not_applicable_never_zero() {
    let transport = MockTransport::new().on(
        "crates.io/api/v1/crates/",
        404,
        r#"{"errors":[{"detail":"Not Found"}]}"#,
    );

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Crates,
        name: "does-not-exist-xyz".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    assert!(!snapshot.has_failures());
    assert_eq!(snapshot.metrics().count(), 0);

    let report = render_terminal(&snapshot);
    assert!(report.contains("N/A"));
    assert!(!report.contains(" 0 "));
}
