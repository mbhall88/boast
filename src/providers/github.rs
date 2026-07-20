//! GitHub repository Provider: Code-Category reach metrics for a repo —
//! stars, forks, watchers, contributors, total release-asset downloads, and
//! repo age. Uses `GITHUB_TOKEN` from the environment when present to raise
//! the API rate limit.

use serde::Deserialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, RepoId, Window};
use crate::provider::Provider;
use crate::transport::Transport;

const API: &str = "https://api.github.com";

pub struct GitHub {
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRepo {
    stargazers_count: Option<u64>,
    forks_count: Option<u64>,
    subscribers_count: Option<u64>,
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    #[serde(default)]
    download_count: u64,
}

impl GitHub {
    pub fn new() -> Self {
        Self {
            token: std::env::var("GITHUB_TOKEN").ok().filter(|s| !s.is_empty()),
        }
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    /// Owned request headers; a view over these is passed to the transport.
    fn request_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![
            ("Accept".into(), "application/vnd.github+json".into()),
            ("X-GitHub-Api-Version".into(), "2022-11-28".into()),
        ];
        if let Some(token) = &self.token {
            headers.push(("Authorization".into(), format!("Bearer {token}")));
        }
        headers
    }
}

impl Default for GitHub {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for GitHub {
    fn name(&self) -> &'static str {
        "github"
    }

    fn category(&self) -> Category {
        Category::Code
    }

    fn supports(&self, identity: &Identity) -> bool {
        matches!(identity, Identity::Repo(_))
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let repo = match identity {
            Identity::Repo(r) => r,
            _ => {
                return Outcome::NotApplicable {
                    note: "GitHub requires a repository".into(),
                }
            }
        };
        let owned = self.request_headers();
        let headers: Vec<(&str, &str)> = owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let base = repo_url(repo);
        let resp = match transport.get_with_headers(&base, &headers) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        let core: GhRepo = match resp.status {
            200 => match serde_json::from_str(&resp.body) {
                Ok(c) => c,
                Err(e) => {
                    return Outcome::Failed {
                        error: format!("unexpected GitHub response: {e}"),
                    }
                }
            },
            404 => {
                return Outcome::NotApplicable {
                    note: "repository not found on GitHub".into(),
                }
            }
            403 => {
                return Outcome::Failed {
                    error: "GitHub rate limit or access denied (403); set GITHUB_TOKEN".into(),
                }
            }
            429 => {
                return Outcome::Failed {
                    error: "rate limited by GitHub (429)".into(),
                }
            }
            s if (500..600).contains(&s) => {
                return Outcome::Failed {
                    error: format!("GitHub server error ({s})"),
                }
            }
            s => {
                return Outcome::Failed {
                    error: format!("unexpected GitHub status ({s})"),
                }
            }
        };

        let canonical = identity.canonical();
        let as_of = OffsetDateTime::now_utc();
        let mut metrics = Vec::new();
        let mut push = |name: &str, value: MetricValue, source: &str, note: Option<&str>| {
            metrics.push(Metric {
                name: name.into(),
                category: Category::Code,
                value,
                window: Window::Cumulative,
                provider: "github".into(),
                identity: canonical.clone(),
                as_of,
                source: source.into(),
                note: note.map(str::to_string),
            });
        };

        if let Some(stars) = core.stargazers_count {
            push("stars", MetricValue::Count(stars), &base, None);
        }
        if let Some(forks) = core.forks_count {
            push("forks", MetricValue::Count(forks), &base, None);
        }
        if let Some(watchers) = core.subscribers_count {
            push(
                "watchers",
                MetricValue::Count(watchers),
                &base,
                Some("users watching the repo (subscribers)"),
            );
        }
        if let Some(created) = core.created_at.as_deref() {
            if let Some((years, since)) = age_years(created) {
                push(
                    "repo_age_years",
                    MetricValue::Real(years),
                    &base,
                    Some(&format!("since {since}")),
                );
            }
        }

        // Contributors and release downloads are secondary calls; if one fails
        // we omit that metric (never report 0) rather than failing the whole repo.
        let contributors_url = format!("{base}/contributors?per_page=1&anon=true");
        if let Some(count) = contributor_count(transport, &contributors_url, &headers) {
            push(
                "contributors",
                MetricValue::Count(count),
                &contributors_url,
                None,
            );
        }
        let releases_url = format!("{base}/releases?per_page=100");
        if let Some(total) = release_download_total(transport, &releases_url, &headers) {
            push(
                "release_downloads",
                MetricValue::Count(total),
                &releases_url,
                Some("summed across release assets"),
            );
        }

        Outcome::Values {
            metrics,
            metadata: None,
        }
    }
}

fn repo_url(repo: &RepoId) -> String {
    format!("{API}/repos/{}/{}", repo.owner, repo.name)
}

/// Age in years, plus the creation date (YYYY-MM-DD), from an RFC3339 timestamp.
fn age_years(created_at: &str) -> Option<(f64, String)> {
    let created = OffsetDateTime::parse(created_at, &Rfc3339).ok()?;
    let seconds = (OffsetDateTime::now_utc() - created).whole_seconds().max(0);
    let years = seconds as f64 / (365.25 * 24.0 * 3600.0);
    let date = created_at
        .split('T')
        .next()
        .unwrap_or(created_at)
        .to_string();
    Some((years, date))
}

/// Contributor count via the `Link` header trick: request one per page and read
/// the `rel="last"` page number. Falls back to the body length for a single page.
fn contributor_count(
    transport: &dyn Transport,
    url: &str,
    headers: &[(&str, &str)],
) -> Option<u64> {
    let resp = transport.get_with_headers(url, headers).ok()?;
    if resp.status != 200 {
        return None;
    }
    if let Some(link) = resp.header("link") {
        if let Some(last) = parse_last_page(link) {
            return Some(last);
        }
    }
    // No pagination: the count is however many came back (0 or 1 at per_page=1).
    let arr: Vec<serde_json::Value> = serde_json::from_str(&resp.body).unwrap_or_default();
    Some(arr.len() as u64)
}

/// Extract the `page=N` value from the `rel="last"` entry of a Link header.
/// The `page` query parameter must be delimited by `?` or `&` so we don't
/// accidentally read `per_page=1`.
fn parse_last_page(link: &str) -> Option<u64> {
    for part in link.split(',') {
        if !part.contains("rel=\"last\"") {
            continue;
        }
        for delim in ["&page=", "?page="] {
            if let Some(idx) = part.find(delim) {
                let digits: String = part[idx + delim.len()..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                return digits.parse().ok();
            }
        }
    }
    None
}

/// Sum `download_count` across all release assets.
fn release_download_total(
    transport: &dyn Transport,
    url: &str,
    headers: &[(&str, &str)],
) -> Option<u64> {
    let resp = transport.get_with_headers(url, headers).ok()?;
    if resp.status != 200 {
        return None;
    }
    let releases: Vec<GhRelease> = serde_json::from_str(&resp.body).ok()?;
    Some(
        releases
            .iter()
            .flat_map(|r| r.assets.iter())
            .map(|a| a.download_count)
            .sum(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PaperId;
    use crate::transport::{MockTransport, TransportError};

    fn repo() -> Identity {
        Identity::Repo(RepoId::parse("BurntSushi/ripgrep").unwrap())
    }

    fn full_transport() -> MockTransport {
        MockTransport::new()
            .on_with_headers(
                "/contributors",
                200,
                "[{}]",
                &[(
                    "Link",
                    "<https://api.github.com/repositories/53631945/contributors?per_page=1&page=2>; rel=\"next\", \
                     <https://api.github.com/repositories/53631945/contributors?per_page=1&page=477>; rel=\"last\"",
                )],
            )
            .on(
                "/releases",
                200,
                include_str!("../../tests/cassettes/github_releases.json"),
            )
            // The repo core route is registered last so the more specific
            // sub-paths match first.
            .on(
                "api.github.com/repos/",
                200,
                include_str!("../../tests/cassettes/github_repo.json"),
            )
    }

    fn metric<'a>(metrics: &'a [Metric], name: &str) -> &'a Metric {
        metrics.iter().find(|m| m.name == name).expect(name)
    }

    #[test]
    fn parses_all_code_metrics() {
        let metrics = match GitHub::new().fetch(&repo(), &full_transport()) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metric(&metrics, "stars").value, MetricValue::Count(66356));
        assert_eq!(metric(&metrics, "forks").value, MetricValue::Count(2651));
        assert_eq!(metric(&metrics, "watchers").value, MetricValue::Count(294));
        assert_eq!(
            metric(&metrics, "contributors").value,
            MetricValue::Count(477)
        );
        // 7381 + 404 + 1007182 + 13027
        assert_eq!(
            metric(&metrics, "release_downloads").value,
            MetricValue::Count(1027994)
        );
        assert!(matches!(
            metric(&metrics, "repo_age_years").value,
            MetricValue::Real(y) if y > 9.0
        ));
        for m in &metrics {
            assert_eq!(m.category, Category::Code);
            assert_eq!(m.provider, "github");
        }
    }

    #[test]
    fn missing_repo_is_not_applicable_not_zero() {
        let t = MockTransport::new().on("api.github.com/repos/", 404, "");
        assert!(matches!(
            GitHub::new().fetch(&repo(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn rate_limit_and_transport_error_are_failed() {
        let t403 = MockTransport::new().on("api.github.com/repos/", 403, "");
        assert!(matches!(
            GitHub::new().fetch(&repo(), &t403),
            Outcome::Failed { .. }
        ));
        let terr = MockTransport::new()
            .on_error("api.github.com/repos/", TransportError::ConnectionFailed);
        assert!(matches!(
            GitHub::new().fetch(&repo(), &terr),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn secondary_call_failure_omits_metric_never_zero() {
        // Repo core succeeds; contributors + releases are 500 → those metrics
        // are simply absent (not reported as 0).
        let t = MockTransport::new()
            .on("/contributors", 500, "")
            .on("/releases", 500, "")
            .on(
                "api.github.com/repos/",
                200,
                include_str!("../../tests/cassettes/github_repo.json"),
            );
        let metrics = match GitHub::new().fetch(&repo(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert!(metrics.iter().any(|m| m.name == "stars"));
        assert!(!metrics.iter().any(|m| m.name == "contributors"));
        assert!(!metrics.iter().any(|m| m.name == "release_downloads"));
    }

    #[test]
    fn does_not_support_papers() {
        let paper = Identity::Paper(PaperId::Doi("10.1/x".into()));
        assert!(!GitHub::new().supports(&paper));
        assert!(GitHub::new().supports(&repo()));
    }

    #[test]
    fn request_headers_include_bearer_only_when_token_present() {
        let with = GitHub {
            token: Some("secret".into()),
        };
        let headers = with.request_headers();
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer secret"));

        let without = GitHub { token: None };
        let headers = without.request_headers();
        assert!(!headers.iter().any(|(k, _)| k == "Authorization"));
        assert!(headers.iter().any(|(k, _)| k == "Accept"));
    }

    #[test]
    fn parse_last_page_reads_rel_last_and_ignores_per_page() {
        // Realistic GitHub Link header: `per_page=1` must not be mistaken for `page`.
        let link = "<https://api.github.com/x?per_page=1&page=2>; rel=\"next\", \
                    <https://api.github.com/x?per_page=1&page=477>; rel=\"last\"";
        assert_eq!(parse_last_page(link), Some(477));
        assert_eq!(
            parse_last_page("<https://x?per_page=1&page=2>; rel=\"next\""),
            None
        );
    }
}
