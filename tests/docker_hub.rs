//! End-to-end tests for the Docker Hub Provider, driven from recorded
//! responses through the real orchestrator and terminal Report.
//!
//! The Rollup case is the one that carries a decision rather than a mechanism:
//! container pulls *do* join the Downloads total (issue #71), on the grounds
//! that the Rollup names every channel it sums, so the inflation stays visible
//! in the breakdown. That makes the accompanying Notice load-bearing, not
//! decorative — both are asserted together.

use boast::model::{Identity, Outcome, PackageId, Project, Registry};
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

/// The regression this whole design turns on. Prose is the sentence people
/// paste into a grant, and it shows the Rollup total *without* the
/// per-channel breakdown that makes container-pull inflation visible in the
/// terminal Report. If the caveat doesn't travel here, including pulls in the
/// total silently overstates reach in the one place that matters most
/// (ADR-0009).
#[test]
fn the_pull_caveat_reaches_the_grant_writing_prose_sentence() {
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

    let prose = boast::report::render_prose(&snapshot);
    assert!(prose.contains("9494122"), "prose should carry the total");
    assert!(
        prose.contains("not installs by people"),
        "the caveat must travel with the number into prose, got: {prose}"
    );
}

#[test]
fn a_nonexistent_image_is_not_applicable_never_zero() {
    let transport = MockTransport::new().on("hub.docker.com", 404, "");

    let project = Project::new(vec![docker("biocontainers/definitely-not-real")]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    // A 404 is a legitimate absence, not a failure.
    assert!(!snapshot.has_failures());

    // Asserted on the Outcome rather than the rendered text: a rendering that
    // happened to omit the row entirely would satisfy a "no zero appears"
    // string check while still having lost the fact (ADR-0002).
    let outcome = snapshot
        .results
        .iter()
        .find(|r| r.provider == "dockerhub")
        .map(|r| &r.outcome)
        .expect("the Docker Hub provider should have produced a result");
    match outcome {
        Outcome::NotApplicable { note } => assert!(note.contains("not found on Docker Hub")),
        other => panic!("expected NotApplicable, got {other:?}"),
    }

    // And no Metric was invented for it, so nothing can render as 0.
    assert_eq!(snapshot.metrics().count(), 0);
}
