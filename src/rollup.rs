//! Derived Rollup totals across compatible Metrics (ADR-0002: Windows gate
//! summation). A Rollup is always computed from already-fetched Metrics at
//! render time — it is never itself fetched, never stored in a Snapshot, and
//! always labelled as derived rather than presented as a single authoritative
//! figure (see the Rollup glossary entry in `CONTEXT.md`).

use crate::model::{Category, Metric, MetricValue, Window};
use crate::providers::github;

/// A derived total across two or more Metrics sharing an exactly-equal
/// Window. Names every channel it includes, per CONTEXT.md's Rollup entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Rollup {
    pub window: Window,
    pub total: u64,
    pub channels: Vec<RollupChannel>,
}

/// One Metric's contribution to a Rollup.
#[derive(Debug, Clone, PartialEq)]
pub struct RollupChannel {
    pub provider: String,
    pub identity: String,
    pub value: u64,
}

/// Group `metrics` by exactly-equal Window and sum each group of two or more
/// into a Rollup — the only definition of "compatible" ADR-0002 allows: a
/// cumulative crates.io count and a trailing 30-day Homebrew count are never
/// mixed, even though both are "downloads", because their Windows differ.
///
/// A Window with only one contributing Metric is left out of the result
/// entirely — there is nothing to roll up, so the caller's own Metric rows
/// already show the figure. Only `MetricValue::Count` values can contribute;
/// this is purely a defensive filter since every Metric eligible for a download
/// (see [`counts_as_download`]) is a Count, but a Rollup is a count total and
/// must never silently coerce or skip a non-Count value into one.
///
/// Group order matches the order Windows are first encountered, and channel
/// order within a group matches input order, so output is deterministic for
/// a given input — no dependency on hash-map iteration order. `compute`
/// itself is Category-agnostic — it groups whatever Metrics the caller
/// passes in; [`counts_as_download`] is the caller-side eligibility check
/// used to select the Downloads Rollup's inputs.
pub fn compute<'a>(metrics: impl IntoIterator<Item = &'a Metric>) -> Vec<Rollup> {
    let mut groups: Vec<(Window, Vec<RollupChannel>)> = Vec::new();

    for m in metrics {
        let value = match &m.value {
            MetricValue::Count(v) => *v,
            _ => continue,
        };
        let channel = RollupChannel {
            provider: m.provider.clone(),
            identity: m.identity.clone(),
            value,
        };
        match groups.iter_mut().find(|(w, _)| *w == m.window) {
            Some((_, channels)) => channels.push(channel),
            None => groups.push((m.window.clone(), vec![channel])),
        }
    }

    groups
        .into_iter()
        .filter(|(_, channels)| channels.len() >= 2)
        .map(|(window, channels)| Rollup {
            total: channels.iter().map(|c| c.value).sum(),
            window,
            channels,
        })
        .collect()
}

/// Whether `m` counts as a "download" for the Downloads Rollup, independent
/// of which Category it displays under in a Report. Every `Category::Downloads`
/// Metric qualifies. GitHub's `release_downloads` is the one deliberate
/// exception: it displays under Code (see CONTEXT.md's Category glossary and
/// the spec's Code Provider bullet), but is still, semantically, a
/// per-channel download count — the spec's own Downloads Provider bullet
/// already names GitHub release assets there with Rollup eligibility, so it
/// still contributes to the total even though its Report row lives elsewhere.
pub fn counts_as_download(m: &Metric) -> bool {
    m.category == Category::Downloads
        || (m.provider == github::NAME && m.name == github::RELEASE_DOWNLOADS_METRIC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;
    use time::OffsetDateTime;

    fn metric(provider: &str, identity: &str, value: u64, window: Window) -> Metric {
        Metric {
            name: "downloads".into(),
            category: Category::Downloads,
            value: MetricValue::Count(value),
            window,
            provider: provider.into(),
            identity: identity.into(),
            as_of: OffsetDateTime::UNIX_EPOCH,
            source: format!("https://example.com/{provider}"),
            note: None,
        }
    }

    #[test]
    fn sums_two_metrics_sharing_a_window() {
        let metrics = [
            metric("crates.io", "crates:boast", 100, Window::Cumulative),
            metric("bioconda", "conda:bioconda/boast", 50, Window::Cumulative),
        ];
        let rollups = compute(&metrics);
        assert_eq!(rollups.len(), 1);
        assert_eq!(rollups[0].total, 150);
        assert_eq!(rollups[0].window, Window::Cumulative);
        assert_eq!(rollups[0].channels.len(), 2);
        assert_eq!(rollups[0].channels[0].provider, "crates.io");
        assert_eq!(rollups[0].channels[1].provider, "bioconda");
    }

    #[test]
    fn never_mixes_incompatible_windows() {
        // A cumulative crates.io count and a trailing 30-day Homebrew count
        // both being "downloads" is not enough — their Windows differ.
        let metrics = [
            metric("crates.io", "crates:boast", 100, Window::Cumulative),
            metric(
                "homebrew",
                "homebrew:boast",
                50,
                Window::Trailing { days: 30 },
            ),
        ];
        assert_eq!(compute(&metrics), Vec::new());
    }

    #[test]
    fn different_trailing_day_counts_are_incompatible() {
        // Homebrew's 90-day figure and PyPI's 30-day figure are both
        // Trailing, but not the *same* Trailing window.
        let metrics = [
            metric(
                "homebrew",
                "homebrew:boast",
                50,
                Window::Trailing { days: 90 },
            ),
            metric("pypi", "pypi:boast", 100, Window::Trailing { days: 30 }),
        ];
        assert_eq!(compute(&metrics), Vec::new());
    }

    #[test]
    fn a_single_contributing_metric_is_not_rolled_up() {
        let metrics = [metric("crates.io", "crates:boast", 100, Window::Cumulative)];
        assert_eq!(compute(&metrics), Vec::new());
    }

    #[test]
    fn separate_rollups_form_per_compatible_window() {
        let metrics = [
            metric("crates.io", "crates:boast", 100, Window::Cumulative),
            metric("bioconda", "conda:bioconda/boast", 50, Window::Cumulative),
            metric(
                "homebrew",
                "homebrew:boast",
                30,
                Window::Trailing { days: 30 },
            ),
            metric("pypi", "pypi:boast", 1000, Window::Trailing { days: 30 }),
        ];
        let rollups = compute(&metrics);
        assert_eq!(rollups.len(), 2);
        assert_eq!(rollups[0].window, Window::Cumulative);
        assert_eq!(rollups[0].total, 150);
        assert_eq!(rollups[1].window, Window::Trailing { days: 30 });
        assert_eq!(rollups[1].total, 1030);
    }

    #[test]
    fn three_or_more_compatible_metrics_all_contribute() {
        let metrics = [
            metric("crates.io", "crates:boast", 100, Window::Cumulative),
            metric("bioconda", "conda:bioconda/boast", 50, Window::Cumulative),
            metric(
                "conda-forge",
                "conda:conda-forge/boast",
                25,
                Window::Cumulative,
            ),
        ];
        let rollups = compute(&metrics);
        assert_eq!(rollups.len(), 1);
        assert_eq!(rollups[0].total, 175);
        assert_eq!(rollups[0].channels.len(), 3);
    }

    #[test]
    fn non_count_values_never_silently_contribute() {
        let mut real_valued = metric("openalex", "doi:10.1/x", 0, Window::Cumulative);
        real_valued.value = MetricValue::Real(2.5);
        let metrics = [
            metric("crates.io", "crates:boast", 100, Window::Cumulative),
            real_valued,
        ];
        // Only one Metric with a Count value shares the window, so nothing rolls up.
        assert_eq!(compute(&metrics), Vec::new());
    }

    fn code_metric(name: &str, provider: &str, value: u64, window: Window) -> Metric {
        Metric {
            name: name.into(),
            category: Category::Code,
            value: MetricValue::Count(value),
            window,
            provider: provider.into(),
            identity: "github:owner/name".into(),
            as_of: OffsetDateTime::UNIX_EPOCH,
            source: "https://api.github.com/repos/owner/name/releases".into(),
            note: None,
        }
    }

    #[test]
    fn github_release_downloads_counts_as_a_download_despite_its_code_category() {
        let m = code_metric("release_downloads", "github", 500, Window::Cumulative);
        assert!(counts_as_download(&m));
    }

    #[test]
    fn a_code_metric_that_isnt_release_downloads_does_not_count() {
        let stars = code_metric("stars", "github", 500, Window::Cumulative);
        assert!(!counts_as_download(&stars));
        // Same metric name on a different (hypothetical) provider must not
        // accidentally qualify — the exception is this one Provider's Metric.
        let same_name_elsewhere =
            code_metric("release_downloads", "example", 500, Window::Cumulative);
        assert!(!counts_as_download(&same_name_elsewhere));
    }

    #[test]
    fn every_downloads_category_metric_counts() {
        let m = metric("crates.io", "crates:boast", 100, Window::Cumulative);
        assert!(counts_as_download(&m));
    }

    #[test]
    fn release_downloads_joins_the_rollup_with_other_download_channels() {
        let metrics = [
            metric("crates.io", "crates:boast", 100, Window::Cumulative),
            code_metric("release_downloads", "github", 50, Window::Cumulative),
        ];
        let eligible: Vec<&Metric> = metrics.iter().filter(|m| counts_as_download(m)).collect();
        let rollups = compute(eligible);
        assert_eq!(rollups.len(), 1);
        assert_eq!(rollups[0].total, 150);
    }
}
