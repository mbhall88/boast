//! Homebrew download-count Provider, via formulae.brew.sh: install counts
//! over the three trailing windows Homebrew's own analytics publish — 30,
//! 90, and 365 days — all from one formula-info fetch (ADR-0003).

use std::collections::HashMap;

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PackageId, Registry, Window};
use crate::provider::{classify_status, Provider};
use crate::transport::Transport;

const API_BASE: &str = "https://formulae.brew.sh/api/formula/";

/// The trailing windows Homebrew's analytics publish, paired with the JSON
/// key that names each one.
const PERIODS: [(&str, u32); 3] = [("30d", 30), ("90d", 90), ("365d", 365)];

pub struct Homebrew;

#[derive(Debug, Deserialize)]
struct BrewFormula {
    analytics: Option<BrewAnalytics>,
}

#[derive(Debug, Deserialize)]
struct BrewAnalytics {
    install: HashMap<String, HashMap<String, u64>>,
}

impl Homebrew {
    fn package(identity: &Identity) -> Option<&PackageId> {
        match identity {
            Identity::Package(p) if p.registry == Registry::Homebrew => Some(p),
            _ => None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str, name: &str) -> Outcome {
        let formula: BrewFormula = match serde_json::from_str(body) {
            Ok(f) => f,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Homebrew response: {e}"),
                }
            }
        };

        let Some(analytics) = formula.analytics else {
            return Outcome::NotApplicable {
                note: "Homebrew has no install analytics for this formula".into(),
            };
        };

        let as_of = OffsetDateTime::now_utc();
        // A period genuinely absent from the response is omitted, never
        // reported as a fabricated 0 (ADR-0002) — Homebrew's own analytics
        // only include entries it has data for.
        let metrics: Vec<Metric> = PERIODS
            .iter()
            .filter_map(|(key, days)| {
                let installs = *analytics.install.get(*key)?.get(name)?;
                Some(Metric {
                    name: format!("downloads_{key}"),
                    category: Category::Downloads,
                    value: MetricValue::Count(installs),
                    window: Window::Trailing { days: *days },
                    provider: "homebrew".into(),
                    identity: canonical.into(),
                    as_of,
                    source: url.into(),
                    note: None,
                })
            })
            .collect();

        if metrics.is_empty() {
            Outcome::NotApplicable {
                note: "Homebrew has no install analytics for this formula".into(),
            }
        } else {
            Outcome::Values {
                metrics,
                metadata: None,
            }
        }
    }
}

impl Provider for Homebrew {
    fn name(&self) -> &'static str {
        "homebrew"
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
                note: "Homebrew only supports homebrew packages".into(),
            };
        };
        let url = format!("{API_BASE}{}.json", package.name);
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match classify_status(resp.status, "Homebrew", "not found on Homebrew") {
            Some(outcome) => outcome,
            None => Self::classify(&resp.body, &url, &canonical, &package.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PaperId, RepoId};
    use crate::transport::{MockTransport, TransportError};

    fn package() -> Identity {
        Identity::Package(PackageId {
            registry: Registry::Homebrew,
            name: "samtools".into(),
        })
    }

    fn metric<'a>(metrics: &'a [Metric], name: &str) -> &'a Metric {
        metrics.iter().find(|m| m.name == name).expect(name)
    }

    #[test]
    fn parses_all_three_trailing_windows_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/homebrew_samtools.json");
        let t =
            MockTransport::new().on("formulae.brew.sh/api/formula/samtools.json", 200, cassette);

        let metrics = match Homebrew.fetch(&package(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics.len(), 3);

        let d30 = metric(&metrics, "downloads_30d");
        assert_eq!(d30.value, MetricValue::Count(466));
        assert_eq!(d30.window, Window::Trailing { days: 30 });

        let d90 = metric(&metrics, "downloads_90d");
        assert_eq!(d90.value, MetricValue::Count(1152));
        assert_eq!(d90.window, Window::Trailing { days: 90 });

        let d365 = metric(&metrics, "downloads_365d");
        assert_eq!(d365.value, MetricValue::Count(5714));
        assert_eq!(d365.window, Window::Trailing { days: 365 });

        for m in &metrics {
            assert_eq!(m.category, Category::Downloads);
            assert_eq!(m.provider, "homebrew");
            assert_eq!(m.identity, "homebrew:samtools");
        }
    }

    #[test]
    fn missing_period_is_omitted_never_zero() {
        let body = r#"{"analytics": {"install": {"30d": {"samtools": 466}}}}"#;
        let t = MockTransport::new().on("formulae.brew.sh/api/formula/", 200, body);
        let metrics = match Homebrew.fetch(&package(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].name, "downloads_30d");
    }

    #[test]
    fn no_analytics_at_all_is_not_applicable_not_zero() {
        let t = MockTransport::new().on("formulae.brew.sh/api/formula/", 200, r#"{}"#);
        assert!(matches!(
            Homebrew.fetch(&package(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn nonexistent_formula_is_not_applicable_not_zero() {
        let t = MockTransport::new().on(
            "formulae.brew.sh/api/formula/",
            404,
            "<!doctype html>Page not found",
        );
        assert!(matches!(
            Homebrew.fetch(&package(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn rate_limit_and_server_error_are_failed() {
        let t429 = MockTransport::new().on("formulae.brew.sh/api/formula/", 429, "");
        assert!(matches!(
            Homebrew.fetch(&package(), &t429),
            Outcome::Failed { .. }
        ));

        let t503 = MockTransport::new().on("formulae.brew.sh/api/formula/", 503, "");
        assert!(matches!(
            Homebrew.fetch(&package(), &t503),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn transport_error_is_failed() {
        let t = MockTransport::new().on_error(
            "formulae.brew.sh/api/formula/",
            TransportError::ConnectionFailed,
        );
        assert!(matches!(
            Homebrew.fetch(&package(), &t),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn does_not_support_papers_repos_or_other_registries() {
        let doi = Identity::Paper(PaperId::Doi("10.1/x".into()));
        let repo = Identity::Repo(RepoId::parse("owner/name").unwrap());
        let crate_pkg = Identity::Package(PackageId {
            registry: Registry::Crates,
            name: "boast".into(),
        });
        assert!(!Homebrew.supports(&doi));
        assert!(!Homebrew.supports(&repo));
        assert!(!Homebrew.supports(&crate_pkg));
        assert!(Homebrew.supports(&package()));
    }
}
