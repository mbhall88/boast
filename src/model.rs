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

/// The result of one Provider×Identity fetch — exactly one of three states.
/// NotApplicable and Failed are never coerced to a zero Value (ADR-0002).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    /// One or more real Metrics were produced.
    Values { metrics: Vec<Metric> },
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
            Outcome::Values { metrics } => metrics.as_slice(),
            _ => &[],
        })
    }
}

/// One external handle a Project links to. v1 knows papers; repos and packages
/// arrive in later tickets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Identity {
    Paper(PaperId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scheme", content = "id", rename_all = "snake_case")]
pub enum PaperId {
    Doi(String),
    Pmid(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("could not recognise '{0}' as a DOI or PubMed ID (repos/packages arrive in a later release)")]
    Unrecognised(String),
    #[error("empty identifier")]
    Empty,
}

impl Identity {
    /// Canonical string form used in Snapshots and Reports, e.g. `doi:10.x`.
    pub fn canonical(&self) -> String {
        match self {
            Identity::Paper(PaperId::Doi(d)) => format!("doi:{d}"),
            Identity::Paper(PaperId::Pmid(p)) => format!("pmid:{p}"),
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
        assert!(matches!(
            Identity::parse("owner/repo"),
            Err(IdentityError::Unrecognised(_))
        ));
        assert!(matches!(Identity::parse("   "), Err(IdentityError::Empty)));
    }

    #[test]
    fn canonical_forms() {
        assert_eq!(Identity::parse("10.1/x").unwrap().canonical(), "doi:10.1/x");
        assert_eq!(Identity::parse("pmid:42").unwrap().canonical(), "pmid:42");
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

        let json = serde_json::to_string(&snap).unwrap();
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, back);
    }
}
