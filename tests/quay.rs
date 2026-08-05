//! End-to-end tests for the Quay.io Provider, driven from recorded responses
//! through the real orchestrator and terminal Report.
//!
//! Quay is the container registry that actually carries Bioconda's traffic —
//! the auto-built `quay.io/biocontainers/<pkg>` images — so these tests are
//! also the answer to "does boast cover biocontainers?".
//!
//! The load-bearing case is the Rollup one, and it is a *negative*: Quay
//! publishes only a rolling daily series, so its Window is trailing and can
//! never be summed with the all-time counts from conda, crates.io, PyPI, and
//! GitHub releases. Nothing special-cases that — it falls out of the Window
//! model (ADR-0002 rule 2) — which is exactly why it needs a test: a
//! regression would be silent, and would inflate a grant sentence.

use boast::model::{Identity, Outcome, PackageId, Project, Registry, Window};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::MockTransport;

const QUAY_CASSETTE: &str = include_str!("cassettes/quay_biocontainers_samtools.json");
const CONDA_CASSETTE: &str = include_str!("cassettes/conda_bioconda_samtools.json");
const DOCKER_CASSETTE: &str = include_str!("cassettes/docker_biocontainers_samtools.json");

fn quay(name: &str) -> Identity {
    Identity::Package(PackageId {
        registry: Registry::Quay,
        name: name.into(),
    })
}

fn conda(name: &str) -> Identity {
    Identity::Package(PackageId {
        registry: Registry::Conda,
        name: name.into(),
    })
}

#[test]
fn quay_image_flows_into_downloads_category() {
    let transport = MockTransport::new().on(
        "quay.io/api/v1/repository/biocontainers/samtools",
        200,
        QUAY_CASSETTE,
    );

    let project = Project::new(vec![quay("biocontainers/samtools")]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);
    assert!(!snapshot.has_failures());

    let report = render_terminal(&snapshot);
    assert!(report.contains("quay:biocontainers/samtools"));
    assert!(report.contains("Downloads"));
    assert!(report.contains("1786502"));
    // The window is reported as measured, so a reader knows the figure covers
    // three months rather than the project's lifetime.
    assert!(report.contains("last 92 days"), "got: {report}");
}

/// The regression the ticket turns on. A trailing Quay figure must never be
/// folded into an all-time total — 1.79M pulls added to a lifetime conda
/// count would produce a number describing no coherent span of time at all.
#[test]
fn quay_pulls_never_join_an_all_time_rollup() {
    let transport = MockTransport::new()
        .on(
            "api.anaconda.org/package/bioconda/samtools",
            200,
            CONDA_CASSETTE,
        )
        .on("quay.io/api/v1/repository/", 200, QUAY_CASSETTE);

    let project = Project::new(vec![
        conda("bioconda/samtools"),
        quay("biocontainers/samtools"),
    ]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);
    assert!(!snapshot.has_failures());

    // Both numbers were really fetched — this is a Window incompatibility,
    // not a missing Metric.
    let quay_metric = snapshot
        .metrics()
        .find(|m| m.provider == "quay")
        .expect("quay should have produced a Metric");
    assert_eq!(quay_metric.window, Window::Trailing { days: 92 });

    let report = render_terminal(&snapshot);
    assert!(report.contains("8897787"), "conda's own row should show");
    assert!(report.contains("1786502"), "quay's own row should show");

    // Only two Downloads Metrics exist and their Windows differ, so there is
    // no compatible pair to sum and no Rollup section at all.
    assert!(
        !report.contains("Downloads Rollup"),
        "a trailing pull count must not form a Rollup with an all-time count, got: {report}"
    );
    // Belt and braces on the arithmetic itself: the sum must appear nowhere.
    assert!(!report.contains("10684289"), "got: {report}");
}

/// The sharper version of the test above: with a *cumulative* Docker Hub pull
/// count present, an all-time Rollup does form — and Quay must still stay out
/// of it, rather than being swept in because it is also "a container pull".
#[test]
fn an_all_time_rollup_forms_around_quay_without_including_it() {
    let transport = MockTransport::new()
        .on(
            "api.anaconda.org/package/bioconda/samtools",
            200,
            CONDA_CASSETTE,
        )
        .on(
            "hub.docker.com/v2/repositories/biocontainers/samtools/",
            200,
            DOCKER_CASSETTE,
        )
        .on("quay.io/api/v1/repository/", 200, QUAY_CASSETTE);

    let project = Project::new(vec![
        conda("bioconda/samtools"),
        Identity::Package(PackageId {
            registry: Registry::Docker,
            name: "biocontainers/samtools".into(),
        }),
        quay("biocontainers/samtools"),
    ]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);
    assert!(!snapshot.has_failures());

    let report = render_terminal(&snapshot);
    // 8897787 conda + 596335 docker, both all-time.
    assert!(report.contains("Downloads Rollup"));
    assert!(report.contains("9494122"), "got: {report}");
    assert!(report.contains("conda:bioconda/samtools (8897787)"));
    assert!(report.contains("docker:biocontainers/samtools (596335)"));
    assert!(
        !report.contains("quay:biocontainers/samtools (1786502)"),
        "the trailing quay count must not be named as an all-time channel, got: {report}"
    );
    // And the total is the two-channel one, not the three-channel one.
    assert!(!report.contains("11280624"), "got: {report}");
}

/// A quay Metric behind a quoted number still owes its caveat (ADR-0009). With
/// no all-time Rollup to headline, quay's own figure becomes the headline
/// download number in prose, so the note has to follow it there.
#[test]
fn the_pull_caveat_reaches_the_grant_writing_prose_sentence() {
    let transport = MockTransport::new().on("quay.io/api/v1/repository/", 200, QUAY_CASSETTE);

    let project = Project::new(vec![quay("biocontainers/samtools")]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    let prose = boast::report::render_prose(&snapshot);
    assert!(prose.contains("1786502"), "prose should carry the figure");
    assert!(
        prose.contains("not installs by people"),
        "the caveat must travel with the number into prose, got: {prose}"
    );
    assert!(
        prose.contains("not an all-time total"),
        "prose quotes the figure without the Window breakdown, so the rolling \
         window must be stated, got: {prose}"
    );
}

/// Quay answers 401 — never 404 — for a repository an unauthenticated caller
/// cannot see, so the default classification would have called this a
/// transient failure and taken the exit code non-zero for a package that
/// simply isn't on Quay.
#[test]
fn an_unseeable_repository_is_not_applicable_never_a_failure() {
    let transport = MockTransport::new().on(
        "quay.io",
        401,
        r#"{"detail": "Requires authentication", "error_type": "invalid_token"}"#,
    );

    let project = Project::new(vec![quay("biocontainers/definitely-not-real")]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    assert!(
        !snapshot.has_failures(),
        "an absent Quay repository must not make the snapshot partial"
    );

    // Asserted on the Outcome rather than the rendered text: a rendering that
    // happened to omit the row entirely would satisfy a "no zero appears"
    // string check while still having lost the fact (ADR-0002).
    let outcome = snapshot
        .results
        .iter()
        .find(|r| r.provider == "quay")
        .map(|r| &r.outcome)
        .expect("the Quay provider should have produced a result");
    match outcome {
        Outcome::NotApplicable { note } => assert!(note.contains("no public repository")),
        other => panic!("expected NotApplicable, got {other:?}"),
    }

    assert_eq!(snapshot.metrics().count(), 0);
}
