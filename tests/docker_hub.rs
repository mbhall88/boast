//! End-to-end tests for the Docker Hub Provider, driven from recorded
//! responses through the real orchestrator and terminal Report.
//!
//! The Rollup case is the one that carries a decision rather than a mechanism:
//! container pulls *do* join the Downloads total (issue #71), on the grounds
//! that the Rollup names every channel it sums, so the inflation stays visible
//! in the breakdown. That makes the accompanying Notice load-bearing, not
//! decorative — both are asserted together.

use boast::model::{Identity, PackageId, Project, Registry};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::MockTransport;

fn docker(name: &str) -> Identity {
    Identity::Package(PackageId {
        registry: Registry::Docker,
        name: name.into(),
    })
}

#[test]
fn docker_image_flows_into_downloads_category() {
    let body = include_str!("cassettes/docker_biocontainers_samtools.json");
    let transport = MockTransport::new().on(
        "hub.docker.com/v2/repositories/biocontainers/samtools/",
        200,
        body,
    );

    let project = Project::new(vec![docker("biocontainers/samtools")]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);
    assert!(!snapshot.has_failures());

    let report = render_terminal(&snapshot);
    assert!(report.contains("docker:biocontainers/samtools"));
    assert!(report.contains("Downloads"));
    assert!(report.contains("596335"));
}

#[test]
fn container_pulls_join_the_downloads_rollup_and_carry_their_caveat() {
    let conda_body = include_str!("cassettes/conda_bioconda_samtools.json");
    let docker_body = include_str!("cassettes/docker_biocontainers_samtools.json");
    let transport = MockTransport::new()
        .on(
            "api.anaconda.org/package/bioconda/samtools",
            200,
            conda_body,
        )
        .on(
            "hub.docker.com/v2/repositories/biocontainers/samtools/",
            200,
            docker_body,
        );

    let project = Project::new(vec![
        Identity::Package(PackageId {
            registry: Registry::Conda,
            name: "bioconda/samtools".into(),
        }),
        docker("biocontainers/samtools"),
    ]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);
    assert!(!snapshot.has_failures());

    let report = render_terminal(&snapshot);
    assert!(report.contains("Downloads Rollup"));
    assert!(report.contains("9494122")); // 8897787 conda + 596335 docker
    assert!(report.contains("conda:bioconda/samtools (8897787)"));
    assert!(report.contains("docker:biocontainers/samtools (596335)"));

    // The total above mixes installs with machine fetches, so the reader must
    // be told. Without this the Rollup would be quietly misleading — which is
    // the whole reason the decision to include it was conditional on a Notice.
    assert!(report.contains("Notices"));
    assert!(report.contains("not installs by people"));
}

#[test]
fn a_nonexistent_image_is_not_applicable_never_zero() {
    let transport = MockTransport::new().on("hub.docker.com", 404, "");

    let project = Project::new(vec![docker("biocontainers/definitely-not-real")]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    // A 404 is a legitimate absence, not a failure, and never renders as 0.
    assert!(!snapshot.has_failures());
    let report = render_terminal(&snapshot);
    assert!(report.contains("N/A") || report.contains("not found on Docker Hub"));
    assert!(!report.contains(" 0 "));
}
