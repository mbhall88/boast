//! End-to-end tests for the Galaxy CoDex Provider (issue #79), driven from a
//! recorded `tools.json` fixture through the real orchestrator, terminal
//! Report, and CLI's optional-Provider registry.
//!
//! Galaxy is the first Provider that is never part of a default run — the
//! load-bearing case here is negative, same shape as Quay's Rollup test:
//! `default_providers()` must never include it, and even when it *is*
//! enabled alongside real Downloads channels, its Usage-Category Metrics
//! must never be swept into the Downloads Rollup despite sharing the exact
//! same Cumulative Window (the thing that would otherwise make `rollup::compute`
//! merge them).

use boast::model::{Identity, PackageId, Project, Registry, RepoHost, RepoId, Snapshot};
use boast::orchestrator;
use boast::providers::{crates_io::CratesIo, default_providers, docker_hub::DockerHub, galaxy};
use boast::report::{render_markdown, render_terminal};
use boast::transport::MockTransport;
use boast::Category;

const CASSETTE: &str = include_str!("cassettes/galaxy_codex_tools.json");

fn repo(owner: &str, name: &str) -> Identity {
    Identity::Repo(RepoId {
        host: RepoHost::GitHub,
        owner: owner.into(),
        name: name.into(),
    })
}

#[test]
fn galaxy_is_absent_from_the_default_registry_and_never_fetched_by_a_default_run() {
    assert!(!default_providers().iter().any(|p| p.name() == galaxy::NAME));

    // A `MockTransport` panics on any unmatched URL (see
    // `transport::tests::mock_panics_on_unmatched`), so if Galaxy were ever
    // accidentally wired into `default_providers()`, this run would panic on
    // the unstubbed `raw.githubusercontent.com` request rather than silently
    // passing. Only GitHub's own endpoints are stubbed.
    let transport = MockTransport::new()
        .on("api.github.com/repos/mbhall88/rasusa", 200, "{}")
        .on("api.github.com/repos/mbhall88/rasusa/releases", 200, "[]")
        .on(
            "api.github.com/search/repositories",
            200,
            r#"{"items": []}"#,
        )
        .on("api.openalex.org/works", 200, r#"{"meta": {"count": 0}}"#)
        .on("www.ebi.ac.uk/europepmc", 200, r#"{"hitCount": 0}"#);
    let project = Project::new(vec![repo("mbhall88", "rasusa")]);
    orchestrator::run(&project, &default_providers(), &transport);
}

#[test]
fn a_matched_repo_flows_into_the_usage_category_between_downloads_and_citations() {
    let transport = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);
    let project = Project::new(vec![repo("mbhall88", "rasusa")]);
    let providers: Vec<Box<dyn boast::provider::Provider>> = vec![Box::new(galaxy::Galaxy)];
    let snapshot = orchestrator::run(&project, &providers, &transport);

    assert!(!snapshot.has_failures());
    let usage: Vec<_> = snapshot
        .metrics()
        .filter(|m| m.category == Category::Usage)
        .collect();
    assert_eq!(usage.len(), 3);

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Usage ──"));
    let usage_pos = report.find("── Usage ──").unwrap();
    // No Downloads/Citations section exists in this single-Provider run, so
    // this only pins that the section itself renders under the right title
    // (the shared CATEGORY_ORDER placement is covered at the unit level in
    // report.rs); the ordering claim is asserted below, with all sections
    // present.
    assert!(usage_pos > 0);

    let markdown = render_markdown(&snapshot);
    assert!(markdown.contains("### Usage"));
    assert!(markdown.contains("| runs | 104 |"));

    // Round-trip the whole Snapshot through JSON, the same durable form
    // `about` writes to disk — a Usage Metric must survive it exactly.
    let json = serde_json::to_string(&snapshot).unwrap();
    let back: Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snapshot, back);
    assert!(back
        .metrics()
        .any(|m| m.category == Category::Usage && m.name == "runs"));
}

#[test]
fn category_order_places_usage_between_downloads_and_citations() {
    // Combine a Downloads Provider (crates.io) and Galaxy in one run so both
    // sections render, and assert their relative order in the terminal
    // Report — the concrete, user-visible form of CONTEXT.md's Category
    // glossary ordering (Code, Downloads, Usage, Citations, Attention).
    let crates_cassette = include_str!("cassettes/crates_boast.json");
    let transport = MockTransport::new()
        .on("crates.io/api/v1/crates/boast", 200, crates_cassette)
        .on("raw.githubusercontent.com", 200, CASSETTE);

    let project = Project::new(vec![
        Identity::Package(PackageId {
            registry: Registry::Crates,
            name: "boast".into(),
        }),
        repo("mbhall88", "rasusa"),
    ]);
    let providers: Vec<Box<dyn boast::provider::Provider>> =
        vec![Box::new(CratesIo), Box::new(galaxy::Galaxy)];
    let snapshot = orchestrator::run(&project, &providers, &transport);
    assert!(!snapshot.has_failures());

    let report = render_terminal(&snapshot);
    let downloads_pos = report.find("── Downloads ──").unwrap();
    let usage_pos = report.find("── Usage ──").unwrap();
    assert!(
        downloads_pos < usage_pos,
        "Downloads must render before Usage:\n{report}"
    );
}

/// The regression this Provider is riskiest for: Galaxy's `runs` Metric is
/// `Window::Cumulative`, the exact same Window real Downloads channels use —
/// `rollup::compute` groups purely by Window equality, so without the
/// Category-based `counts_as_download` filter upstream of it, Galaxy's count
/// would silently inflate the Downloads Rollup total.
#[test]
fn galaxy_usage_metrics_never_join_the_downloads_rollup_despite_sharing_its_window() {
    let crates_cassette = include_str!("cassettes/crates_boast.json");
    let docker_cassette = include_str!("cassettes/docker_biocontainers_samtools.json");
    let transport = MockTransport::new()
        .on("crates.io/api/v1/crates/boast", 200, crates_cassette)
        .on(
            "hub.docker.com/v2/repositories/biocontainers/samtools/",
            200,
            docker_cassette,
        )
        .on("raw.githubusercontent.com", 200, CASSETTE);

    let project = Project::new(vec![
        Identity::Package(PackageId {
            registry: Registry::Crates,
            name: "boast".into(),
        }),
        Identity::Package(PackageId {
            registry: Registry::Docker,
            name: "biocontainers/samtools".into(),
        }),
        // rasusa matches the fixture's single "rasusa" suite: 104 cumulative
        // runs — if it ever leaked into the Rollup, 842617 + 596335 + 104 =
        // 1439056 would replace the correct 1438952 total below.
        repo("mbhall88", "rasusa"),
    ]);
    let providers: Vec<Box<dyn boast::provider::Provider>> = vec![
        Box::new(CratesIo),
        Box::new(DockerHub),
        Box::new(galaxy::Galaxy),
    ];
    let snapshot = orchestrator::run(&project, &providers, &transport);
    assert!(!snapshot.has_failures());

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Usage ──"));
    assert!(report.contains("═══ Downloads Rollup"));

    let rollup_start = report.find("═══ Downloads Rollup").unwrap();
    let rollup_end = report[rollup_start..]
        .find("── Notices ──")
        .map(|i| rollup_start + i)
        .unwrap_or(report.len());
    let rollup_section = &report[rollup_start..rollup_end];
    assert!(
        rollup_section.contains("1438952"),
        "expected the crates.io + docker total only:\n{rollup_section}"
    );
    assert!(
        !rollup_section.contains("galaxy"),
        "Galaxy must never be named as a Downloads Rollup channel:\n{rollup_section}"
    );
    assert!(
        !rollup_section.contains("1439056"),
        "Galaxy's 104 runs must not have been summed into the Downloads total:\n{rollup_section}"
    );
}

#[test]
fn cli_enable_provider_selects_galaxy_alongside_the_default_registry() {
    let mut providers = default_providers();
    providers
        .extend(boast::providers::resolve_optional_providers(&["galaxy".to_string()]).unwrap());
    assert!(providers.iter().any(|p| p.name() == galaxy::NAME));
    assert_eq!(
        providers
            .iter()
            .filter(|p| p.name() == galaxy::NAME)
            .count(),
        1
    );
}
