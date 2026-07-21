//! End-to-end test of a Homebrew package Identity flowing through the
//! orchestrator into the Downloads Category — driven from a recorded
//! formulae.brew.sh response.

use boast::model::{Identity, PackageId, Project, Registry, Window};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::MockTransport;
use boast::Category;

#[test]
fn homebrew_package_flows_into_downloads_category_with_all_three_windows() {
    let cassette = include_str!("cassettes/homebrew_samtools.json");
    let transport =
        MockTransport::new().on("formulae.brew.sh/api/formula/samtools.json", 200, cassette);

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Homebrew,
        name: "samtools".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    assert!(!snapshot.has_failures());

    let downloads: Vec<_> = snapshot
        .metrics()
        .filter(|m| m.category == Category::Downloads)
        .collect();
    assert_eq!(downloads.len(), 3);
    let windows: Vec<_> = downloads.iter().map(|m| m.window.clone()).collect();
    assert!(windows.contains(&Window::Trailing { days: 30 }));
    assert!(windows.contains(&Window::Trailing { days: 90 }));
    assert!(windows.contains(&Window::Trailing { days: 365 }));

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Downloads ──"));
    assert!(report.contains("downloads_30d"));
    assert!(report.contains("downloads_90d"));
    assert!(report.contains("downloads_365d"));
}

#[test]
fn nonexistent_homebrew_formula_is_not_applicable_never_zero() {
    let transport = MockTransport::new().on(
        "formulae.brew.sh/api/formula/",
        404,
        "<!doctype html>Page not found",
    );

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Homebrew,
        name: "does-not-exist-xyz".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    assert!(!snapshot.has_failures());
    assert_eq!(snapshot.metrics().count(), 0);

    let report = render_terminal(&snapshot);
    assert!(report.contains("N/A"));
    assert!(!report.contains(" 0 "));
}
