//! The core domain model: Identity, Project, Metric (with its Window), Outcome,
//! and Snapshot. See `CONTEXT.md` for the glossary these types encode, and
//! ADR-0001 (snapshot-centric) and ADR-0002 (metric honesty) for the rules.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

/// The family a Metric belongs to; groups the Report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Code,
    Downloads,
    Citations,
    Attention,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Code => "Code",
            Category::Downloads => "Downloads",
            Category::Citations => "Citations",
            Category::Attention => "Attention",
        }
    }
}

/// The span of time a Metric's value covers. Two Metrics may only be summed
/// (into a Rollup) when their Windows are equal — enforced later, see ADR-0002.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Window {
    /// All-time (e.g. crates.io total downloads, a citation count).
    Cumulative,
    /// A rolling period (e.g. Homebrew 365-day installs).
    Trailing { days: u32 },
    /// A named bucket (e.g. citations in year "2023").
    Periodic { label: String },
}

impl Window {
    pub fn describe(&self) -> String {
        match self {
            Window::Cumulative => "all-time".to_string(),
            Window::Trailing { days } => format!("last {days} days"),
            Window::Periodic { label } => label.clone(),
        }
    }
}

/// A Metric's measured value. Kept as a tagged union so counts, normalized
/// ratios, percentiles, and flags all round-trip precisely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum MetricValue {
    Count(u64),
    Real(f64),
    Text(String),
    Flag(bool),
}

impl std::fmt::Display for MetricValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricValue::Count(n) => write!(f, "{n}"),
            MetricValue::Real(x) => write!(f, "{x:.2}"),
            MetricValue::Text(s) => write!(f, "{s}"),
            MetricValue::Flag(b) => write!(f, "{}", if *b { "yes" } else { "no" }),
        }
    }
}

/// A single measured quantity of reach, carrying full provenance: the value,
/// the Provider it came from, the Identity it describes, when it was fetched
/// (`as_of`), the coverage Window, and the source URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub category: Category,
    pub value: MetricValue,
    pub window: Window,
    pub provider: String,
    pub identity: String,
    #[serde(with = "time::serde::rfc3339")]
    pub as_of: OffsetDateTime,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
}

/// Descriptive bibliographic metadata for a paper Identity. This is *not* a
/// reach Metric (it has no value, Window, or Category); it labels the paper in
/// Reports so a reader knows what the numbers describe. Carried alongside the
/// reach Metrics a Provider fetched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperMetadata {
    pub identity: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub authors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub year: Option<i32>,
    pub provider: String,
    pub source: String,
}

impl PaperMetadata {
    /// A one-line human summary, e.g. `"Title" — Stephens et al., PLOS Biology, 2015`.
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(title) = &self.title {
            parts.push(format!("\"{title}\""));
        }
        let mut tail: Vec<String> = Vec::new();
        if let Some(first) = self.authors.first() {
            if self.authors.len() > 1 {
                tail.push(format!("{first} et al."));
            } else {
                tail.push(first.clone());
            }
        }
        if let Some(container) = &self.container {
            tail.push(container.clone());
        }
        if let Some(year) = self.year {
            tail.push(year.to_string());
        }
        if !tail.is_empty() {
            parts.push(tail.join(", "));
        }
        parts.join(" — ")
    }
}

/// The result of one Provider×Identity fetch — exactly one of three states.
/// NotApplicable and Failed are never coerced to a zero Value (ADR-0002).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    /// One or more real Metrics were produced, plus any descriptive metadata
    /// the Provider resolved for the Identity.
    Values {
        metrics: Vec<Metric>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        metadata: Option<PaperMetadata>,
    },
    /// The Identity legitimately has no presence on this channel (shown N/A).
    NotApplicable { note: String },
    /// A transient error; the number exists but wasn't retrievable now.
    Failed { error: String },
}

/// One row of a Snapshot: a Provider's Outcome for one Identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchResult {
    pub provider: String,
    pub identity: String,
    pub category: Category,
    pub outcome: Outcome,
}

/// The primary durable artifact: a timestamped, machine-readable record of one
/// run, with every Outcome recorded explicitly (ADR-0001).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub tool: String,
    pub tool_version: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub identities: Vec<String>,
    pub results: Vec<FetchResult>,
}

impl Snapshot {
    pub const SCHEMA_VERSION: u32 = 1;

    /// True if any fetch is still in the `Failed` state (drives the exit code).
    pub fn has_failures(&self) -> bool {
        self.results
            .iter()
            .any(|r| matches!(r.outcome, Outcome::Failed { .. }))
    }

    /// Every successfully-fetched Metric across all results.
    pub fn metrics(&self) -> impl Iterator<Item = &Metric> {
        self.results.iter().flat_map(|r| match &r.outcome {
            Outcome::Values { metrics, .. } => metrics.as_slice(),
            _ => &[],
        })
    }

    /// Descriptive metadata resolved for the Project's identities.
    pub fn descriptions(&self) -> impl Iterator<Item = &PaperMetadata> {
        self.results.iter().filter_map(|r| match &r.outcome {
            Outcome::Values {
                metadata: Some(md), ..
            } => Some(md),
            _ => None,
        })
    }
}

/// One external handle a Project links to. v1 knows papers and code
/// repositories; packages arrive in later tickets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Identity {
    Paper(PaperId),
    Repo(RepoId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scheme", content = "id", rename_all = "snake_case")]
pub enum PaperId {
    Doi(String),
    Pmid(String),
}

/// A code repository on a known host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoId {
    pub host: RepoHost,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoHost {
    GitHub,
}

impl RepoHost {
    fn prefix(self) -> &'static str {
        match self {
            RepoHost::GitHub => "github",
        }
    }
}

impl RepoId {
    /// Parse `owner/name`, a `github.com/owner/name` URL (with optional scheme,
    /// `.git`, or extra path), or an `git@github.com:owner/name` SSH form.
    pub fn parse(input: &str) -> Result<RepoId, IdentityError> {
        let s = input.trim().trim_end_matches('/');
        if s.is_empty() {
            return Err(IdentityError::Empty);
        }
        let mut rest = s;
        for scheme in ["https://", "http://"] {
            rest = rest.strip_prefix(scheme).unwrap_or(rest);
        }
        rest = rest.strip_prefix("www.").unwrap_or(rest);
        rest = rest.strip_prefix("git@github.com:").unwrap_or(rest);
        rest = rest.strip_prefix("github.com/").unwrap_or(rest);
        rest = rest.strip_prefix("github:").unwrap_or(rest);
        rest = rest.strip_suffix(".git").unwrap_or(rest);

        let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 {
            return Err(IdentityError::Unrecognised(input.to_string()));
        }
        let (owner, name) = (parts[0], parts[1]);
        if owner.chars().any(char::is_whitespace) || name.chars().any(char::is_whitespace) {
            return Err(IdentityError::Unrecognised(input.to_string()));
        }
        Ok(RepoId {
            host: RepoHost::GitHub,
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("could not recognise '{0}' as a DOI, PubMed ID, or GitHub repo (packages arrive in a later release)")]
    Unrecognised(String),
    #[error("empty identifier")]
    Empty,
}

impl Identity {
    /// Canonical string form used in Snapshots and Reports, e.g. `doi:10.x` or
    /// `github:owner/name`.
    pub fn canonical(&self) -> String {
        match self {
            Identity::Paper(PaperId::Doi(d)) => format!("doi:{d}"),
            Identity::Paper(PaperId::Pmid(p)) => format!("pmid:{p}"),
            Identity::Repo(r) => format!("{}:{}/{}", r.host.prefix(), r.owner, r.name),
        }
    }

    /// Parse a user-supplied identifier. Accepts `doi:...`, a bare `10.x/...`
    /// DOI, a `https://doi.org/...` URL, or `pmid:12345678`.
    pub fn parse(input: &str) -> Result<Identity, IdentityError> {
        let s = input.trim();
        if s.is_empty() {
            return Err(IdentityError::Empty);
        }
        let lower = s.to_ascii_lowercase();

        if let Some(rest) = lower.strip_prefix("pmid:") {
            let id = rest.trim();
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                return Ok(Identity::Paper(PaperId::Pmid(id.to_string())));
            }
            return Err(IdentityError::Unrecognised(input.to_string()));
        }

        if let Some(rest) = s.strip_prefix("doi:").or_else(|| s.strip_prefix("DOI:")) {
            return Ok(Identity::Paper(PaperId::Doi(rest.trim().to_string())));
        }

        // https://doi.org/10.x or doi.org/10.x
        if let Some(idx) = lower.find("doi.org/") {
            let doi = &s[idx + "doi.org/".len()..];
            if doi.starts_with("10.") {
                return Ok(Identity::Paper(PaperId::Doi(doi.trim().to_string())));
            }
        }

        // GitHub repo URL (positional). The bare `owner/name` shorthand is only
        // accepted via the explicit `--repo` flag, to avoid guessing.
        if lower.contains("github.com/") || lower.starts_with("git@github.com:") {
            return RepoId::parse(s).map(Identity::Repo);
        }

        // Bare DOI: starts with "10." and contains a slash.
        if s.starts_with("10.") && s.contains('/') {
            return Ok(Identity::Paper(PaperId::Doi(s.to_string())));
        }

        Err(IdentityError::Unrecognised(input.to_string()))
    }
}

/// The central entity: a piece of research work linking one or more Identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub identities: Vec<Identity>,
}

impl Project {
    pub fn new(identities: Vec<Identity>) -> Self {
        Self { identities }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_doi() {
        assert_eq!(
            Identity::parse("10.1371/journal.pbio.1002195").unwrap(),
            Identity::Paper(PaperId::Doi("10.1371/journal.pbio.1002195".into()))
        );
    }

    #[test]
    fn parses_doi_prefix_and_url() {
        let want = Identity::Paper(PaperId::Doi("10.1/x".into()));
        assert_eq!(Identity::parse("doi:10.1/x").unwrap(), want);
        assert_eq!(Identity::parse("https://doi.org/10.1/x").unwrap(), want);
    }

    #[test]
    fn parses_pmid() {
        assert_eq!(
            Identity::parse("pmid:31234567").unwrap(),
            Identity::Paper(PaperId::Pmid("31234567".into()))
        );
    }

    #[test]
    fn rejects_unrecognised() {
        // A bare `owner/repo` is NOT a positional identity — it must come via --repo.
        assert!(matches!(
            Identity::parse("owner/repo"),
            Err(IdentityError::Unrecognised(_))
        ));
        assert!(matches!(Identity::parse("   "), Err(IdentityError::Empty)));
    }

    #[test]
    fn parses_github_repo_urls_and_shorthand() {
        let want = RepoId {
            host: RepoHost::GitHub,
            owner: "BurntSushi".into(),
            name: "ripgrep".into(),
        };
        // Positional URL forms parse to a Repo identity.
        for input in [
            "https://github.com/BurntSushi/ripgrep",
            "github.com/BurntSushi/ripgrep",
            "https://github.com/BurntSushi/ripgrep.git",
            "https://github.com/BurntSushi/ripgrep/tree/master",
            "git@github.com:BurntSushi/ripgrep.git",
        ] {
            assert_eq!(
                Identity::parse(input).unwrap(),
                Identity::Repo(want.clone())
            );
        }
        // The `--repo` shorthand goes through RepoId::parse directly.
        assert_eq!(RepoId::parse("BurntSushi/ripgrep").unwrap(), want);
        assert!(matches!(
            RepoId::parse("noslash"),
            Err(IdentityError::Unrecognised(_))
        ));
    }

    #[test]
    fn canonical_forms() {
        assert_eq!(Identity::parse("10.1/x").unwrap().canonical(), "doi:10.1/x");
        assert_eq!(Identity::parse("pmid:42").unwrap().canonical(), "pmid:42");
        assert_eq!(
            Identity::Repo(RepoId::parse("BurntSushi/ripgrep").unwrap()).canonical(),
            "github:BurntSushi/ripgrep"
        );
    }

    #[test]
    fn snapshot_round_trips_and_reports_failures() {
        let snap = Snapshot {
            schema_version: Snapshot::SCHEMA_VERSION,
            tool: "boast".into(),
            tool_version: "0.1.0".into(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            identities: vec!["doi:10.1/x".into()],
            results: vec![
                FetchResult {
                    provider: "openalex".into(),
                    identity: "doi:10.1/x".into(),
                    category: Category::Citations,
                    outcome: Outcome::Values {
                        metrics: vec![Metric {
                            name: "citations".into(),
                            category: Category::Citations,
                            value: MetricValue::Count(1421),
                            window: Window::Cumulative,
                            provider: "openalex".into(),
                            identity: "doi:10.1/x".into(),
                            as_of: OffsetDateTime::UNIX_EPOCH,
                            source: "https://api.openalex.org/works/doi:10.1/x".into(),
                            note: None,
                        }],
                        metadata: Some(PaperMetadata {
                            identity: "doi:10.1/x".into(),
                            title: Some("A Paper".into()),
                            authors: vec!["A. Author".into(), "B. Author".into()],
                            container: Some("Journal".into()),
                            year: Some(2015),
                            provider: "crossref".into(),
                            source: "https://api.crossref.org/works/10.1/x".into(),
                        }),
                    },
                },
                FetchResult {
                    provider: "openalex".into(),
                    identity: "doi:10.2/y".into(),
                    category: Category::Citations,
                    outcome: Outcome::Failed {
                        error: "rate limited (429)".into(),
                    },
                },
            ],
        };

        assert!(snap.has_failures());
        assert_eq!(snap.metrics().count(), 1);
        assert_eq!(snap.descriptions().count(), 1);
        assert_eq!(
            snap.descriptions().next().unwrap().summary(),
            "\"A Paper\" — A. Author et al., Journal, 2015"
        );

        let json = serde_json::to_string(&snap).unwrap();
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, back);
    }
}
