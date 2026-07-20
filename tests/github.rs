//! End-to-end test of a repo Identity flowing through the orchestrator into
//! the Code Category — driven entirely from recorded GitHub responses.

use boast::model::{Identity, Project, RepoId};
use boast::orchestrator;
use boast::providers::default_providers;
use boast::report::render_terminal;
use boast::transport::MockTransport;
use boast::Category;

#[test]
fn github_repo_flows_into_code_category() {
    let repo_body = include_str!("cassettes/github_repo.json");
    let releases = include_str!("cassettes/github_releases.json");
    let transport = MockTransport::new()
        .on_with_headers(
            "/contributors",
            200,
            "[{}]",
            &[(
                "Link",
                "<https://api.github.com/x?per_page=1&page=477>; rel=\"last\"",
            )],
        )
        .on("/releases", 200, releases)
        .on("api.github.com/repos/", 200, repo_body);

    let project = Project::new(vec![Identity::Repo(
        RepoId::parse("BurntSushi/ripgrep").unwrap(),
    )]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);

    // Paper providers are skipped for a repo Identity, so no failures arise
    // from unmocked paper endpoints.
    assert!(!snapshot.has_failures());

    // stars, forks, watchers, repo_age_years, contributors, release_downloads
    let code_metrics = snapshot
        .metrics()
        .filter(|m| m.category == Category::Code)
        .count();
    assert_eq!(code_metrics, 6);

    let report = render_terminal(&snapshot);
    assert!(report.contains("── Code ──"));
    assert!(report.contains("66356")); // stars
    assert!(report.contains("477")); // contributors
    assert!(report.contains("1027994")); // summed release downloads
}

#[test]
fn github_topic_cohort_ranks_by_stars_with_disclaimer() {
    // The repo declares three topics; the Cohort ranks it by stars within each
    // one, from recorded search responses (2 above → rank 3 of 4821).
    let repo_body = include_str!("cassettes/github_repo_topics.json");
    let cohort = include_str!("cassettes/github_search_cohort.json");
    let rank = include_str!("cassettes/github_search_rank.json");
    let transport = MockTransport::new()
        .on("stars:", 200, rank) // star-bounded rank query (most specific first)
        .on("search/repositories", 200, cohort) // cohort-size query
        .on_with_headers(
            "/contributors",
            200,
            "[{}]",
            &[(
                "Link",
                "<https://api.github.com/x?per_page=1&page=128>; rel=\"last\"",
            )],
        )
        .on(
            "/releases",
            200,
            include_str!("cassettes/github_releases.json"),
        )
        .on("api.github.com/repos/", 200, repo_body);

    let project = Project::new(vec![Identity::Repo(
        RepoId::parse("nf-core/rnaseq").unwrap(),
    )]);
    let snapshot = orchestrator::run(&project, &default_providers(), &transport);
    assert!(!snapshot.has_failures());

    // One Cohort rank per declared topic, all in the Code Category.
    let cohort: Vec<_> = snapshot
        .metrics()
        .filter(|m| m.name.starts_with("cohort_rank"))
        .collect();
    assert_eq!(cohort.len(), 3);
    assert!(cohort.iter().all(|m| m.category == Category::Code));

    let report = render_terminal(&snapshot);
    assert!(report.contains("#3 of 4821")); // rank within each cohort
    assert!(report.contains("cohort_rank (rna-seq)")); // each topic is named
    assert!(report.contains("cohort_rank (nextflow)"));
    assert!(report.contains("cohort_rank (bioinformatics)"));
    assert!(report.contains("inconsistently applied")); // the disclaimer
}
