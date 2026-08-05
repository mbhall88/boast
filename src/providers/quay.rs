//! Quay.io Provider for container pull counts: the sum of the daily pull
//! series Quay publishes for a repository, over whatever window that series
//! turns out to span (ADR-0003).
//!
//! This is the Provider that actually answers "does boast cover
//! biocontainers?" — Bioconda's auto-built per-package containers live at
//! `quay.io/biocontainers/<pkg>`, not on Docker Hub. (Docker Hub's
//! `biocontainers/` org is the older hand-curated set; its `samtools` image
//! was last pushed in 2019.) The scale gap is the tell: ~1.79M pulls in three
//! months here against 596k *all-time* for the stale Docker Hub image.
//!
//! Quay has no all-time total — only the rolling daily series — so unlike
//! Docker Hub these Metrics carry a [`Window::Trailing`] and are therefore
//! window-incompatible with every cumulative downloads channel. They never
//! join the all-time Downloads Rollup, and no special-casing makes that
//! happen: it is the Window model doing the job it was designed for
//! (ADR-0002 rule 2).

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PackageId, Registry, Window};
use crate::provider::{classify_status, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://quay.io/api/v1/repository/";

pub const NAME: &str = "quay";

/// Why a pull total is not an install total, and why this one is a rolling
/// window rather than a lifetime figure. Deliberately longer than
/// [`crate::report::INLINE_DETAIL_LIMIT`] so it renders in the Notices footer
/// (ADR-0005) rather than being truncated into an inline detail column, since
/// the caveat is the point. The window length is left to the Metric's own
/// Window, which is derived per response and so cannot be stated here.
pub const PULL_SEMANTICS_NOTE: &str =
    "Quay.io pull counts record image fetches by machines, not installs by people, and CI \
     re-pulls dominate for a biocontainer; Quay publishes only a rolling daily series, so \
     this is not an all-time total";

pub struct Quay;

#[derive(Debug, Deserialize)]
struct QuayRepository {
    stats: Option<Vec<DailyPulls>>,
}

#[derive(Debug, Deserialize)]
struct DailyPulls {
    count: u64,
}

impl Quay {
    fn package(identity: &Identity) -> Option<&PackageId> {
        match identity {
            Identity::Package(p) if p.registry == Registry::Quay => Some(p),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let repo: QuayRepository = match serde_json::from_str(body) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Quay.io response: {e}"),
                }
            }
        };

        // An absent or empty series is no data at all, so there is no number
        // to report and none is invented (ADR-0002). A *populated* series
        // summing to zero is different, and is reported as the real 0 it is:
        // Quay's series is dense, spelling out zero-pull days explicitly, so
        // "no pulls in this window" is a measurement rather than a gap.
        let Some(stats) = repo.stats.filter(|s| !s.is_empty()) else {
            return Outcome::NotApplicable {
                note: "Quay.io returned no pull counts for this repository".into(),
            };
        };

        // The Window is however many daily buckets Quay actually returned —
        // 92 at the time of writing, but asserting a fixed 90 or 92 would be a
        // provenance lie the first time Quay changes its retention. `as u32`
        // cannot truncate: a series long enough to overflow would be a
        // multi-gigabyte response body.
        //
        // Counting buckets (rather than spanning the `date` fields) is exact
        // only because Quay's series is dense: zero-pull days come back as
        // explicit `count: 0` entries, verified against a low-traffic
        // repository. Were Quay to start omitting them, the span between the
        // first and last `date` would become the honest derivation.
        //
        // Known imprecision, shared with every trailing Provider here: the
        // series ends *yesterday*, while `as_of` below is now, and `Window`
        // carries no anchor date — so "last 92 days" is offset a day from the
        // 92 days Quay actually measured. Anchoring a Window is a model-wide
        // change, not a Quay one.
        let days = stats.len() as u32;
        let pulls: u64 = stats.iter().map(|d| d.count).sum();

        Outcome::Values {
            metrics: vec![Metric {
                name: "pulls".into(),
                category: Category::Downloads,
                value: MetricValue::Count(pulls),
                window: Window::Trailing { days },
                provider: NAME.into(),
                identity: canonical.into(),
                as_of: OffsetDateTime::now_utc(),
                source: url.into(),
                note: Some(PULL_SEMANTICS_NOTE.into()),
            }],
            metadata: None,
        }
    }
}

impl Provider for Quay {
    fn name(&self) -> &'static str {
        NAME
    }

    fn category(&self) -> Category {
        Category::Downloads
    }

    fn supports(&self, identity: &Identity) -> bool {
        Self::package(identity).is_some()
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        let Some(package) = Self::package(identity) else {
            return Outcome::NotApplicable {
                note: "Quay.io only supports quay packages".into(),
            };
        };
        if package.name.split_once('/').is_none() {
            return Outcome::NotApplicable {
                note: "quay package name must be 'namespace/name'".into(),
            };
        }
        // `includeStats` is what asks for the daily series at all — without it
        // the response has no `stats` key. `includeTags` is off because the
        // tag list is ~8x the rest of the payload (141 tags for
        // `biocontainers/samtools`) and nothing here reads it: the repository
        // is per-package, so identity maps at repo level with no tag handling.
        let url = format!(
            "{API_BASE}{}?includeStats=true&includeTags=false",
            package.name
        );
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        // Quay answers 401, never 404, for anything an unauthenticated caller
        // cannot see — a missing repository, a missing namespace, and a
        // private repository are deliberately indistinguishable, so the
        // registry can't be enumerated. Left to `classify_status` this would
        // become a `Failed`, claiming a number that is retrievable and merely
        // wasn't retrieved; no retry can ever succeed here, and the exit code
        // would go non-zero for a package simply not published on Quay. It is
        // a legitimate absence of any *public* presence — see ADR-0010, which
        // this is the worked example for. The note discloses the ambiguity
        // rather than asserting an absence the response never established.
        if resp.status == 401 {
            return Outcome::NotApplicable {
                note: "no public repository on Quay.io (Quay answers alike for a missing \
                       or a private repository, so this may be a private image)"
                    .into(),
            };
        }

        match classify_status(resp.status, "Quay.io", "not found on Quay.io") {
            Some(outcome) => outcome,
            None => Self::classify(&resp.body, &url, &canonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PaperId, RepoHost, RepoId};
    use crate::transport::{MockTransport, TransportError};

    const CASSETTE: &str = include_str!("../../tests/cassettes/quay_biocontainers_samtools.json");

    fn package(name: &str) -> Identity {
        Identity::Package(PackageId {
            registry: Registry::Quay,
            name: name.into(),
        })
    }

    fn values(outcome: Outcome) -> Vec<Metric> {
        match outcome {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        }
    }

    #[test]
    fn sums_the_daily_series_from_cassette() {
        let t = MockTransport::new().on(
            "quay.io/api/v1/repository/biocontainers/samtools",
            200,
            CASSETTE,
        );

        let metrics = values(Quay.fetch(&package("biocontainers/samtools"), &t));

        assert_eq!(metrics.len(), 1);
        let m = &metrics[0];
        assert_eq!(m.name, "pulls");
        assert_eq!(m.category, Category::Downloads);
        assert_eq!(m.value, MetricValue::Count(1_786_502));
        assert_eq!(m.provider, "quay");
        assert_eq!(m.identity, "quay:biocontainers/samtools");
    }

    /// The Window is a claim about what the number covers, so it must be read
    /// off the response rather than assumed — the cassette's 92 points are
    /// what Quay returned on the day it was recorded, not a documented
    /// retention guarantee.
    #[test]
    fn the_window_is_derived_from_the_series_never_hardcoded() {
        let t = MockTransport::new().on("quay.io", 200, CASSETTE);
        let metrics = values(Quay.fetch(&package("biocontainers/samtools"), &t));
        assert_eq!(metrics[0].window, Window::Trailing { days: 92 });

        // A shorter series must move the Window with it, or the figure would
        // be labelled as covering days it never saw.
        let short = r#"{"stats": [{"date": "2026-08-03", "count": 5},
                                  {"date": "2026-08-04", "count": 7}]}"#;
        let t = MockTransport::new().on("quay.io", 200, short);
        let metrics = values(Quay.fetch(&package("biocontainers/samtools"), &t));
        assert_eq!(metrics[0].window, Window::Trailing { days: 2 });
        assert_eq!(metrics[0].value, MetricValue::Count(12));
    }

    /// A pull is a weaker unit than an install and the window is rolling
    /// rather than lifetime, so both caveats must travel with every Metric.
    #[test]
    fn every_metric_carries_the_pull_semantics_caveat() {
        let t = MockTransport::new().on("quay.io", 200, CASSETTE);

        let metrics = values(Quay.fetch(&package("biocontainers/samtools"), &t));
        assert_eq!(metrics[0].note.as_deref(), Some(PULL_SEMANTICS_NOTE));
        assert!(
            PULL_SEMANTICS_NOTE.len() > crate::report::INLINE_DETAIL_LIMIT,
            "the caveat must reach the Notices footer, not be squeezed inline"
        );
    }

    #[test]
    fn an_absent_or_empty_series_is_not_applicable_never_zero() {
        for body in [r#"{"name": "samtools"}"#, r#"{"stats": []}"#] {
            let t = MockTransport::new().on("quay.io", 200, body);
            match Quay.fetch(&package("biocontainers/samtools"), &t) {
                Outcome::NotApplicable { note } => assert!(note.contains("no pull counts")),
                other => panic!("expected NotApplicable for {body}, got {other:?}"),
            }
        }
    }

    /// The counterpart to the test above, and the reason the two can't share
    /// one branch: Quay's series is dense, so a run of explicit zero-count
    /// days is a real measurement of no pulls, not missing data. Reporting it
    /// as NotApplicable would discard a fact Quay actually stated.
    #[test]
    fn a_populated_series_of_zeroes_is_a_real_zero_not_not_applicable() {
        let body = r#"{"stats": [{"date": "2026-08-03", "count": 0},
                                 {"date": "2026-08-04", "count": 0}]}"#;
        let t = MockTransport::new().on("quay.io", 200, body);

        let metrics = values(Quay.fetch(&package("biocontainers/samtools"), &t));
        assert_eq!(metrics[0].value, MetricValue::Count(0));
        assert_eq!(metrics[0].window, Window::Trailing { days: 2 });
    }

    /// Quay's anti-enumeration behaviour, verified live against
    /// `biocontainers/nope-not-real` and a nonexistent namespace: both answer
    /// 401, never 404. Classified as a legitimate absence rather than a
    /// transient failure (ADR-0010), since no retry can ever turn it into a
    /// number and a `Failed` would take the process exit code non-zero for a
    /// package that simply isn't published on Quay.
    #[test]
    fn an_unauthenticated_401_is_a_legitimate_absence_not_a_failure() {
        let t = MockTransport::new().on(
            "quay.io",
            401,
            r#"{"detail": "Requires authentication", "error_type": "invalid_token"}"#,
        );

        match Quay.fetch(&package("biocontainers/nope"), &t) {
            Outcome::NotApplicable { note } => {
                assert!(note.contains("no public repository on Quay.io"));
                // The ambiguity is disclosed, not hidden: this genuinely may
                // be a private image rather than a missing one.
                assert!(note.contains("private"));
            }
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    /// Quay isn't observed to return 404, but if it ever does it means the
    /// same thing, and must not fall through to the "unexpected status"
    /// Failed arm.
    #[test]
    fn a_404_is_also_not_applicable() {
        let t = MockTransport::new().on("quay.io", 404, "");

        match Quay.fetch(&package("biocontainers/nope"), &t) {
            Outcome::NotApplicable { note } => assert!(note.contains("not found on Quay.io")),
            other => panic!("expected NotApplicable, got {other:?}"),
        }
    }

    #[test]
    fn rate_limit_and_server_error_are_failed() {
        for status in [429, 503] {
            let t = MockTransport::new().on("quay.io", status, "");
            assert!(
                matches!(
                    Quay.fetch(&package("biocontainers/samtools"), &t),
                    Outcome::Failed { .. }
                ),
                "status {status} should be Failed"
            );
        }
    }

    #[test]
    fn malformed_json_fails_rather_than_reporting_a_value() {
        let t = MockTransport::new().on("quay.io", 200, "not json");

        match Quay.fetch(&package("biocontainers/samtools"), &t) {
            Outcome::Failed { error } => assert!(error.contains("unexpected Quay.io response")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn a_transport_error_fails_rather_than_reporting_a_value() {
        let t = MockTransport::new().on_error("quay.io", TransportError::ConnectionFailed);

        match Quay.fetch(&package("biocontainers/samtools"), &t) {
            Outcome::Failed { error } => assert!(error.contains("connection failed")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// The daily series only exists when `includeStats` is asked for, and the
    /// tag block is dead weight, so both query parameters are load-bearing.
    #[test]
    fn requests_the_stats_series_without_the_tag_list() {
        let t = MockTransport::new().on("includeStats=true&includeTags=false", 200, CASSETTE);
        assert!(matches!(
            Quay.fetch(&package("biocontainers/samtools"), &t),
            Outcome::Values { .. }
        ));
    }

    #[test]
    fn supports_only_quay_packages() {
        assert!(Quay.supports(&package("biocontainers/samtools")));
        assert!(!Quay.supports(&Identity::Package(PackageId {
            registry: Registry::Docker,
            name: "biocontainers/samtools".into(),
        })));
        assert!(!Quay.supports(&Identity::Package(PackageId {
            registry: Registry::Conda,
            name: "bioconda/samtools".into(),
        })));
        assert!(!Quay.supports(&Identity::Repo(RepoId {
            host: RepoHost::GitHub,
            owner: "lh3".into(),
            name: "minimap2".into(),
        })));
        assert!(!Quay.supports(&Identity::Paper(PaperId::Doi("10.0/0".into()))));
    }
}
