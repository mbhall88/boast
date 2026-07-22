//! End-to-end test of a Downloads Rollup forming across two different
//! package Identities that share a Window — driven entirely from recorded
//! responses, through the real orchestrator and terminal Report.

use boast::model::{Identity, PackageId, Project, Registry};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::MockTransport;

#[test]
fn rollup_sums_two_channels_sharing_a_cumulative_window() {
    let crates_body = include_str!("cassettes/crates_boast.json");
    let conda_body = include_str!("cassettes/conda_bioconda_samtools.json");
    let transport = MockTransport::new()
        .on("crates.io/api/v1/crates/boast", 200, crates_body)
        .on(
            "api.anaconda.org/package/bioconda/samtools",
            200,
            conda_body,
        );

    let project = Project::new(vec![
        Identity::Package(PackageId {
            registry: Registry::Crates,
            name: "boast".into(),
        }),
        Identity::Package(PackageId {
            registry: Registry::Conda,
            name: "bioconda/samtools".into(),
        }),
    ]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);
    assert!(!snapshot.has_failures());

    let report = render_terminal(&snapshot);
    // Both individual channel figures still show under their own identity.
    assert!(report.contains("842617"));
    assert!(report.contains("8897787"));
    // Plus a clearly-labelled, derived Rollup naming both channels and their sum.
    assert!(report.contains("Downloads Rollup"));
    assert!(report.contains("derived"));
    assert!(report.contains("9740404")); // 842617 + 8897787
                                         // Named by identity, so channels stay traceable back to the identity
                                         // sections above even if two Identities ever shared a provider.
    assert!(report.contains("crates:boast (842617)"));
    assert!(report.contains("conda:bioconda/samtools (8897787)"));
}

#[test]
fn no_rollup_for_a_single_package_identity() {
    let crates_body = include_str!("cassettes/crates_boast.json");
    let transport = MockTransport::new().on("crates.io/api/v1/crates/boast", 200, crates_body);

    let project = Project::new(vec![Identity::Package(PackageId {
        registry: Registry::Crates,
        name: "boast".into(),
    })]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);
    assert!(!snapshot.has_failures());

    let report = render_terminal(&snapshot);
    assert!(report.contains("842617"));
    assert!(!report.contains("Rollup"));
}
