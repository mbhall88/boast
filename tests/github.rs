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
