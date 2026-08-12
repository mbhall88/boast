//! Galaxy CoDex Provider for tool-execution usage stats (issue #79): matches
//! a GitHub repository Identity against the wrapper "suites" published by
//! the Galaxy Tool CoDex project and aggregates cumulative execution counts,
//! a max unique-user-account figure, and public-instance availability into
//! the Usage Category.
//!
//! **Optional, disabled by default** — this is the first non-default
//! Provider (see [`crate::providers::optional_providers`]): enable it with
//! `--enable-provider galaxy` or a Manifest's `enable_providers`. One tool
//! can have several Galaxy wrappers, one per subcommand (a "suite" in
//! CoDex's own vocabulary) — vcflib alone maps to 23 — so every matched
//! suite's `Homepage` is compared, never `Suite source` (the wrapper
//! repository under `galaxyproject/tools-iuc` or similar, not the upstream
//! Project being measured).
//!
//! Fetches the one file CoDex republishes independently of its dated
//! per-server SQL exports, `communities/all/resources/tools.json`, and
//! treats it as the authoritative published dataset. `Metric.as_of` is
//! always when boast fetched it, never CoDex's own regeneration date — the
//! file's cumulative figures may lag the underlying Galaxy servers by an
//! amount CoDex does not itself publish, which is why both cumulative-count
//! Metrics (`runs` via [`RUNS_LAG_NOTE`], `user_accounts` via
//! [`USER_ACCOUNTS_CAVEAT_NOTE`]) carry that caveat. `public_instances`
//! doesn't: availability is a current-state fact CoDex derives the same way
//! either way, not a count that can drift stale in the same sense.

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, RepoId, Window};
use crate::provider::Provider;
use crate::transport::{is_transient_status, Transport};

pub const NAME: &str = "galaxy";

const TOOLS_JSON_URL: &str = "https://raw.githubusercontent.com/galaxyproject/galaxy_codex/main/communities/all/resources/tools.json";

const MAJOR_INSTANCES: [&str; 4] = [
    "UseGalaxy.org",
    "UseGalaxy.org.au",
    "UseGalaxy.eu",
    "UseGalaxy.fr",
];

/// Why a cumulative CoDex figure isn't a live count. Deliberately longer
/// than [`crate::report::INLINE_DETAIL_LIMIT`] so it reaches the Notices
/// footer (ADR-0005) rather than being silently dropped from every row.
pub const RUNS_LAG_NOTE: &str = "Galaxy CoDex republishes cumulative tool-execution counts \
     on its own refresh schedule, so this figure may lag the underlying Galaxy servers";

/// The multi-suite aggregation rule, close to the issue's own suggested
/// wording ("Runs: aggregated sum. Users: max unique per wrapper (to prevent
/// over-counting)."), plus the "not distinct people" caveat the issue's
/// triage thread insisted on (a main-server user total can count one account
/// once per server) and the same lag caveat [`RUNS_LAG_NOTE`] carries, since
/// `user_accounts` is drawn from the same cumulative CoDex snapshot as
/// `runs`. Also deliberately over [`crate::report::INLINE_DETAIL_LIMIT`].
pub const USER_ACCOUNTS_CAVEAT_NOTE: &str =
    "Runs: aggregated sum. Users: max unique per wrapper (to prevent over-counting) — not a \
     count of distinct people, since CoDex's per-server totals can count one account once per \
     server. Both are cumulative CoDex figures that may lag the underlying Galaxy servers.";

#[derive(Debug, Deserialize)]
struct CodexSuite {
    #[serde(rename = "Suite ID")]
    suite_id: String,
    #[serde(rename = "Homepage")]
    homepage: Option<String>,
    #[serde(rename = "Suite runs on main servers")]
    runs: u64,
    #[serde(rename = "Suite users on main servers")]
    users: u64,
    #[serde(rename = "Number of tools on UseGalaxy.org (Main)")]
    tools_usegalaxy_org: u64,
    #[serde(rename = "Number of tools on UseGalaxy.org.au")]
    tools_usegalaxy_org_au: u64,
    #[serde(rename = "Number of tools on UseGalaxy.eu")]
    tools_usegalaxy_eu: u64,
    #[serde(rename = "Number of tools on UseGalaxy.fr")]
    tools_usegalaxy_fr: u64,
}

pub struct Galaxy;

impl Galaxy {
    fn repo(identity: &Identity) -> Option<&RepoId> {
        match identity {
            Identity::Repo(r) => Some(r),
            _ => None,
        }
    }

    /// A CoDex `Homepage` matches `repo` only when it's recognisably a
    /// GitHub URL — the same discipline `Identity::parse` applies to a
    /// positional GitHub URL (only `github.com/`/`git@github.com:` counts),
    /// and the same prefix-stripping shape `RepoId::parse` itself uses, so a
    /// look-alike host (`not-github.com/owner/name`) can't slip past a bare
    /// substring check. Without this guard, `RepoId::parse`'s bare
    /// `owner/name` fallback (meant for the CLI's explicit `--repo`
    /// shorthand) would treat an arbitrary non-GitHub Homepage's first two
    /// path segments as if they were a repository, risking a false match —
    /// CoDex has hundreds of non-GitHub Homepage values (lab websites,
    /// ReadTheDocs, Bioconductor).
    fn matches(repo: &RepoId, homepage: &str) -> bool {
        let mut rest = homepage.trim();
        for scheme in ["https://", "http://"] {
            rest = rest.strip_prefix(scheme).unwrap_or(rest);
        }
        let lower = rest.to_ascii_lowercase();
        let is_github_url = lower.starts_with("github.com/")
            || lower.starts_with("www.github.com/")
            || homepage.starts_with("git@github.com:");
        if !is_github_url {
            return false;
        }
        RepoId::parse(homepage).is_ok_and(|h| {
            h.owner.eq_ignore_ascii_case(&repo.owner) && h.name.eq_ignore_ascii_case(&repo.name)
        })
    }

    /// The union of major public instances any matched suite is available
    /// on, in [`MAJOR_INSTANCES`] order — "available" means CoDex counts at
    /// least one tool installed there, independent of whether it has ever
    /// been run.
    fn matched_instances(matched: &[&CodexSuite]) -> Vec<&'static str> {
        let mut names = Vec::new();
        if matched.iter().any(|s| s.tools_usegalaxy_org > 0) {
            names.push(MAJOR_INSTANCES[0]);
        }
        if matched.iter().any(|s| s.tools_usegalaxy_org_au > 0) {
            names.push(MAJOR_INSTANCES[1]);
        }
        if matched.iter().any(|s| s.tools_usegalaxy_eu > 0) {
            names.push(MAJOR_INSTANCES[2]);
        }
        if matched.iter().any(|s| s.tools_usegalaxy_fr > 0) {
            names.push(MAJOR_INSTANCES[3]);
        }
        names
    }

    fn classify(body: &str, repo: &RepoId, url: &str, canonical: &str) -> Outcome {
        let suites: Vec<CodexSuite> = match serde_json::from_str(body) {
            Ok(s) => s,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Galaxy CoDex response: {e}"),
                }
            }
        };

        let matched: Vec<&CodexSuite> = suites
            .iter()
            .filter(|s| {
                s.homepage
                    .as_deref()
                    .is_some_and(|h| Self::matches(repo, h))
            })
            .collect();

        // No exact CoDex match is NotApplicable, never a real zero (ADR-0002)
        // — most repositories simply have no Galaxy wrapper at all.
        if matched.is_empty() {
            return Outcome::NotApplicable {
                note: "no Galaxy CoDex suite's Homepage matches this repository".into(),
            };
        }

        let total_runs: u64 = matched.iter().map(|s| s.runs).sum();
        let max_users: u64 = matched.iter().map(|s| s.users).max().unwrap_or(0);
        let instances = Self::matched_instances(&matched);
        let as_of = OffsetDateTime::now_utc();

        let mut suite_ids: Vec<&str> = matched.iter().map(|s| s.suite_id.as_str()).collect();
        suite_ids.sort_unstable();
        let plural = if suite_ids.len() == 1 { "" } else { "s" };
        let audit_note = format!(
            "Matched {} Galaxy CoDex suite{plural}: {}",
            suite_ids.len(),
            suite_ids.join(", "),
        );

        let instances_note = if instances.is_empty() {
            format!(
                "not available on any of the four major public Galaxy instances ({})",
                MAJOR_INSTANCES.join(", ")
            )
        } else {
            format!("available on {}", instances.join(", "))
        };

        Outcome::Values {
            provider_notes: vec![audit_note],
            metrics: vec![
                Metric {
                    name: "runs".into(),
                    category: Category::Usage,
                    value: MetricValue::Count(total_runs),
                    window: Window::Cumulative,
                    provider: NAME.into(),
                    identity: canonical.into(),
                    as_of,
                    source: url.into(),
                    note: Some(RUNS_LAG_NOTE.into()),
                },
                Metric {
                    name: "user_accounts".into(),
                    category: Category::Usage,
                    value: MetricValue::Count(max_users),
                    window: Window::Cumulative,
                    provider: NAME.into(),
                    identity: canonical.into(),
                    as_of,
                    source: url.into(),
                    note: Some(USER_ACCOUNTS_CAVEAT_NOTE.into()),
                },
                Metric {
                    name: "public_instances".into(),
                    category: Category::Usage,
                    value: MetricValue::Count(instances.len() as u64),
                    window: Window::Cumulative,
                    provider: NAME.into(),
                    identity: canonical.into(),
                    as_of,
                    source: url.into(),
                    note: Some(instances_note),
                },
            ],
            metadata: None,
        }
    }

    /// `tools.json` is one fixed URL shared by every Identity, unlike a
    /// per-repo endpoint — so a non-200 here means the *fetch* failed, never
    /// that this repository has no Galaxy presence. Deliberately not named
    /// or shaped like `provider::classify_status`, and deliberately not
    /// calling it: that helper's 404-is-NotApplicable default assumes a
    /// per-Identity URL, but a 404 on this shared file means the file moved,
    /// which is `Failed`, not evidence about any one repository.
    fn classify_shared_file_status(status: u16) -> Option<Outcome> {
        if status == 200 {
            return None;
        }
        let error = if status == 429 {
            "rate limited by Galaxy CoDex (429)".to_string()
        } else if is_transient_status(status) {
            format!("Galaxy CoDex server error ({status})")
        } else {
            format!("unexpected Galaxy CoDex status ({status})")
        };
        Some(Outcome::Failed { error })
    }
}

impl Provider for Galaxy {
    fn name(&self) -> &'static str {
        NAME
    }

    fn category(&self) -> Category {
        Category::Usage
    }

    fn supports(&self, identity: &Identity) -> bool {
        Self::repo(identity).is_some()
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let Some(repo) = Self::repo(identity) else {
            return Outcome::NotApplicable {
                note: "Galaxy CoDex only supports GitHub repository identities".into(),
            };
        };
        let canonical = identity.canonical();

        let resp = match transport.get(TOOLS_JSON_URL) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match Self::classify_shared_file_status(resp.status) {
            Some(outcome) => outcome,
            None => Self::classify(&resp.body, repo, TOOLS_JSON_URL, &canonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PackageId, PaperId, Registry, RepoHost};
    use crate::transport::{MockTransport, TransportError};

    const CASSETTE: &str = include_str!("../../tests/cassettes/galaxy_codex_tools.json");

    fn repo(owner: &str, name: &str) -> Identity {
        Identity::Repo(RepoId {
            host: RepoHost::GitHub,
            owner: owner.into(),
            name: name.into(),
        })
    }

    fn values(outcome: Outcome) -> (Vec<Metric>, Vec<String>) {
        match outcome {
            Outcome::Values {
                metrics,
                provider_notes,
                ..
            } => (metrics, provider_notes),
            other => panic!("expected Values, got {other:?}"),
        }
    }

    fn metric<'a>(metrics: &'a [Metric], name: &str) -> &'a Metric {
        metrics
            .iter()
            .find(|m| m.name == name)
            .unwrap_or_else(|| panic!("no '{name}' metric among {metrics:?}"))
    }

    #[test]
    fn a_single_matched_suite_emits_cumulative_usage_metrics() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);

        let (metrics, notes) = values(Galaxy.fetch(&repo("mbhall88", "rasusa"), &t));

        assert_eq!(metrics.len(), 3);
        for m in &metrics {
            assert_eq!(m.category, Category::Usage);
            assert_eq!(m.window, Window::Cumulative);
            assert_eq!(m.provider, "galaxy");
            assert_eq!(m.identity, "github:mbhall88/rasusa");
        }
        assert_eq!(metric(&metrics, "runs").value, MetricValue::Count(104));
        assert_eq!(
            metric(&metrics, "user_accounts").value,
            MetricValue::Count(24)
        );
        // rasusa is available on eu + fr only (2 of the 4 major instances).
        assert_eq!(
            metric(&metrics, "public_instances").value,
            MetricValue::Count(2)
        );
        assert!(metric(&metrics, "public_instances")
            .note
            .as_deref()
            .unwrap()
            .contains("UseGalaxy.eu, UseGalaxy.fr"));

        assert_eq!(notes, vec!["Matched 1 Galaxy CoDex suite: rasusa"]);
    }

    #[test]
    fn a_suite_available_everywhere_reports_all_four_major_instances() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);
        let (metrics, _) = values(Galaxy.fetch(&repo("tseemann", "shovill"), &t));
        assert_eq!(
            metric(&metrics, "public_instances").value,
            MetricValue::Count(4)
        );
    }

    #[test]
    fn multiple_matched_suites_sum_runs_and_take_the_maximum_users() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);

        let (metrics, notes) = values(Galaxy.fetch(&repo("owner", "toolx"), &t));

        // toolx-align (100 runs, 10 users) + toolx-call (50 runs, 40 users).
        assert_eq!(metric(&metrics, "runs").value, MetricValue::Count(150));
        // Users are the maximum across wrappers, never summed (10 + 40 would
        // over-count one account using both subcommands).
        assert_eq!(
            metric(&metrics, "user_accounts").value,
            MetricValue::Count(40)
        );
        // Union of instances: toolx-align is on org+eu, toolx-call is on fr.
        assert_eq!(
            metric(&metrics, "public_instances").value,
            MetricValue::Count(3)
        );

        assert_eq!(
            notes,
            vec!["Matched 2 Galaxy CoDex suites: toolx-align, toolx-call"]
        );
    }

    #[test]
    fn every_metric_carries_a_caveat_long_enough_for_the_notices_footer() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);
        let (metrics, _) = values(Galaxy.fetch(&repo("mbhall88", "rasusa"), &t));

        for name in ["runs", "user_accounts"] {
            let note = metric(&metrics, name).note.clone().unwrap();
            assert!(
                note.len() > crate::report::INLINE_DETAIL_LIMIT,
                "{name}'s note must reach the Notices footer, not be squeezed inline"
            );
        }
    }

    #[test]
    fn no_matching_suite_is_not_applicable_never_zero() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);

        match Galaxy.fetch(&repo("someone", "not-on-galaxy"), &t) {
            Outcome::NotApplicable { note } => assert!(note.contains("no Galaxy CoDex suite")),
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    /// The wrapper repository named in `Suite source` (`galaxyproject/tools-iuc`
    /// in the fixture) must never be treated as a match — only `Homepage` (the
    /// upstream Project) is compared.
    #[test]
    fn the_suite_source_wrapper_repository_is_never_matched() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);

        match Galaxy.fetch(&repo("galaxyproject", "tools-iuc"), &t) {
            Outcome::NotApplicable { .. } => {}
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    /// A non-GitHub Homepage (hundreds exist in the real dataset — lab
    /// sites, ReadTheDocs, Bioconductor) must never be coerced into a repo
    /// match via `RepoId::parse`'s bare `owner/name` fallback. The fixture's
    /// "unrelated-tool" entry has `Homepage: "https://etetoolkit.org/docs/latest"`
    /// — without the github.com guard, `RepoId::parse` would happily read
    /// that as owner `etetoolkit.org`, name `docs`, and falsely match here.
    #[test]
    fn a_non_github_homepage_never_false_matches() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);

        match Galaxy.fetch(&repo("etetoolkit.org", "docs"), &t) {
            Outcome::NotApplicable { .. } => {}
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    /// A look-alike host is not GitHub, even though it contains the literal
    /// substring "github.com/" — `"not-github.com/"` ends with exactly that
    /// substring, so a naive `.contains("github.com/")` guard would have
    /// wrongly treated this as a GitHub URL and gone on to `RepoId::parse`
    /// it. The real guard checks the string *starts with* the host (after
    /// stripping a scheme), the same way `RepoId::parse` itself does.
    #[test]
    fn a_look_alike_host_containing_github_com_as_a_substring_never_matches() {
        let body = r#"[{
            "Suite ID": "lookalike",
            "Homepage": "https://not-github.com/owner/toolx",
            "Suite runs on main servers": 1,
            "Suite users on main servers": 1,
            "Number of tools on UseGalaxy.org (Main)": 0,
            "Number of tools on UseGalaxy.org.au": 0,
            "Number of tools on UseGalaxy.eu": 0,
            "Number of tools on UseGalaxy.fr": 0
        }]"#;
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, body);

        match Galaxy.fetch(&repo("owner", "toolx"), &t) {
            Outcome::NotApplicable { .. } => {}
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    /// A suite with no Homepage at all must be skipped, not panic or match.
    #[test]
    fn a_null_homepage_suite_never_matches_anything() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, CASSETTE);
        match Galaxy.fetch(&repo("example", "no-homepage-tool"), &t) {
            Outcome::NotApplicable { .. } => {}
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_fails_rather_than_reporting_a_value() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 200, "not json");

        match Galaxy.fetch(&repo("mbhall88", "rasusa"), &t) {
            Outcome::Failed { error } => assert!(error.contains("unexpected Galaxy CoDex")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// Unlike a per-Identity endpoint, a 404 on this single shared file means
    /// the fetch itself failed — never that one repository lacks a match.
    #[test]
    fn a_404_on_the_shared_file_is_failed_not_not_applicable() {
        let t = MockTransport::new().on("raw.githubusercontent.com", 404, "");

        match Galaxy.fetch(&repo("mbhall88", "rasusa"), &t) {
            Outcome::Failed { error } => assert!(error.contains("404")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn rate_limit_and_server_error_are_failed() {
        for status in [429, 503] {
            let t = MockTransport::new().on("raw.githubusercontent.com", status, "");
            assert!(
                matches!(
                    Galaxy.fetch(&repo("mbhall88", "rasusa"), &t),
                    Outcome::Failed { .. }
                ),
                "status {status} should be Failed"
            );
        }
    }

    #[test]
    fn a_transport_error_fails_rather_than_reporting_a_value() {
        let t = MockTransport::new().on_error(
            "raw.githubusercontent.com",
            TransportError::ConnectionFailed,
        );

        match Galaxy.fetch(&repo("mbhall88", "rasusa"), &t) {
            Outcome::Failed { error } => assert!(error.contains("connection failed")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn supports_only_repo_identities() {
        assert!(Galaxy.supports(&repo("mbhall88", "rasusa")));
        assert!(!Galaxy.supports(&Identity::Package(PackageId {
            registry: Registry::Crates,
            name: "boast".into(),
        })));
        assert!(!Galaxy.supports(&Identity::Paper(PaperId::Doi("10.0/0".into()))));
    }

    #[test]
    fn fetch_on_an_unsupported_identity_is_not_applicable() {
        let t = MockTransport::new();
        match Galaxy.fetch(&Identity::Paper(PaperId::Doi("10.0/0".into())), &t) {
            Outcome::NotApplicable { .. } => {}
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }
}
