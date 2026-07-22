//! Renders a Snapshot as a human-readable terminal table, grouped by Category.
//! A Report is always derived from a Snapshot and never fetches (ADR-0001).
//! NotApplicable shows as N/A and Failed is flagged — never a misleading 0.

use time::format_description::well_known::Rfc3339;

use crate::model::{Category, Outcome, Snapshot};
use crate::rollup;

const CATEGORY_ORDER: [Category; 4] = [
    Category::Code,
    Category::Downloads,
    Category::Citations,
    Category::Attention,
];

struct Row {
    name: String,
    value: String,
    window: String,
    provider: String,
    detail: String,
}

/// Render the Snapshot as a terminal-friendly string, grouped by identity then
/// Category so a batch of identifiers stays readable.
pub fn render_terminal(snapshot: &Snapshot) -> String {
    let mut out = String::new();

    let created = snapshot
        .created_at
        .format(&Rfc3339)
        .unwrap_or_else(|_| snapshot.created_at.to_string());
    out.push_str(&format!(
        "boast {} — as of {created}\n",
        snapshot.tool_version
    ));

    for identity in &snapshot.identities {
        out.push_str(&format!("\n━━ {identity} ━━\n"));

        for md in snapshot.descriptions().filter(|d| &d.identity == identity) {
            let summary = md.summary();
            if !summary.is_empty() {
                out.push_str(&format!("{summary}\n"));
            }
        }

        for category in CATEGORY_ORDER {
            let rows = rows_for(snapshot, identity, category);
            if rows.is_empty() {
                continue;
            }
            out.push_str(&format!("── {} ──\n", category.label()));

            let w_name = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
            let w_value = rows.iter().map(|r| r.value.len()).max().unwrap_or(0);
            let w_window = rows.iter().map(|r| r.window.len()).max().unwrap_or(0);
            let w_provider = rows.iter().map(|r| r.provider.len()).max().unwrap_or(0);

            for r in rows {
                let mut line = format!(
                    "  {name:<w_name$}  {value:>w_value$}  {window:<w_window$}  {provider:<w_provider$}",
                    name = r.name,
                    value = r.value,
                    window = r.window,
                    provider = r.provider,
                );
                if !r.detail.is_empty() {
                    line.push_str(&format!("  {}", r.detail));
                }
                // Trim trailing whitespace left by empty columns.
                out.push_str(line.trim_end());
                out.push('\n');
            }
        }
    }

    let downloads = snapshot
        .metrics()
        .filter(|m| m.category == Category::Downloads);
    let rollups = rollup::compute(downloads);
    if !rollups.is_empty() {
        out.push_str("\n═══ Downloads Rollup (derived — see channels above) ═══\n");
        for r in &rollups {
            // Named by identity, not just provider — two Identities on the
            // same provider (e.g. two crates.io packages) must still be told
            // apart, per CONTEXT.md's "name every Metric it includes".
            let breakdown: Vec<String> = r
                .channels
                .iter()
                .map(|c| format!("{} ({})", c.identity, c.value))
                .collect();
            out.push_str(&format!(
                "  {} {} = {}\n",
                r.total,
                r.window.describe(),
                breakdown.join(" + "),
            ));
        }
    }

    if snapshot.has_failures() {
        out.push_str(
            "\n⚠ partial snapshot: some metrics failed to fetch (see FAILED rows); exit code 1.\n",
        );
    }

    out
}

/// Build the display rows for one identity and Category from the Snapshot.
fn rows_for(snapshot: &Snapshot, identity: &str, category: Category) -> Vec<Row> {
    let mut rows = Vec::new();
    for result in &snapshot.results {
        if result.identity != identity || result.category != category {
            continue;
        }
        match &result.outcome {
            Outcome::Values { metrics, .. } => {
                for m in metrics {
                    rows.push(Row {
                        name: m.name.clone(),
                        value: m.value.to_string(),
                        window: m.window.describe(),
                        provider: m.provider.clone(),
                        detail: m.note.clone().unwrap_or_default(),
                    });
                }
            }
            Outcome::NotApplicable { note } => rows.push(Row {
                name: result.provider.clone(),
                value: "N/A".to_string(),
                window: String::new(),
                provider: String::new(),
                detail: note.clone(),
            }),
            Outcome::Failed { error } => rows.push(Row {
                name: result.provider.clone(),
                value: "FAILED".to_string(),
                window: String::new(),
                provider: String::new(),
                detail: error.clone(),
            }),
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FetchResult, Metric, MetricValue, Window};
    use time::OffsetDateTime;

    fn snapshot_with(results: Vec<FetchResult>) -> Snapshot {
        Snapshot {
            schema_version: Snapshot::SCHEMA_VERSION,
            tool: "boast".into(),
            tool_version: "0.1.0".into(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            identities: vec!["doi:10.1/x".into()],
            results,
        }
    }

    fn metric(name: &str, value: MetricValue) -> Metric {
        Metric {
            name: name.into(),
            category: Category::Citations,
            value,
            window: Window::Cumulative,
            provider: "openalex".into(),
            identity: "doi:10.1/x".into(),
            as_of: OffsetDateTime::UNIX_EPOCH,
            source: "https://api.openalex.org/works/doi:10.1/x".into(),
            note: None,
        }
    }

    #[test]
    fn renders_values_grouped_under_category() {
        let snap = snapshot_with(vec![FetchResult {
            provider: "openalex".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![metric("citations", MetricValue::Count(1421))],
                metadata: None,
            },
        }]);
        let out = render_terminal(&snap);
        assert!(out.contains("Citations"));
        assert!(out.contains("citations"));
        assert!(out.contains("1421"));
        assert!(!out.contains("partial snapshot"));
    }

    #[test]
    fn shows_na_and_failed_distinctly_never_zero() {
        let mut snap = snapshot_with(vec![
            FetchResult {
                provider: "openalex".into(),
                identity: "doi:10.2/y".into(),
                category: Category::Citations,
                outcome: Outcome::NotApplicable {
                    note: "not found in OpenAlex".into(),
                },
            },
            FetchResult {
                provider: "openalex".into(),
                identity: "doi:10.3/z".into(),
                category: Category::Citations,
                outcome: Outcome::Failed {
                    error: "rate limited (429)".into(),
                },
            },
        ]);
        snap.identities = vec!["doi:10.2/y".into(), "doi:10.3/z".into()];
        let out = render_terminal(&snap);
        assert!(out.contains("N/A"));
        assert!(out.contains("FAILED"));
        assert!(!out.contains(" 0 "));
        assert!(out.contains("partial snapshot"));
    }

    #[test]
    fn groups_results_under_per_identity_banners() {
        let mut snap = snapshot_with(vec![
            FetchResult {
                provider: "openalex".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Citations,
                outcome: Outcome::Values {
                    metrics: vec![metric("citations", MetricValue::Count(1421))],
                    metadata: None,
                },
            },
            FetchResult {
                provider: "github".into(),
                identity: "github:o/n".into(),
                category: Category::Code,
                outcome: Outcome::Values {
                    metrics: vec![Metric {
                        category: Category::Code,
                        provider: "github".into(),
                        identity: "github:o/n".into(),
                        ..metric("stars", MetricValue::Count(42))
                    }],
                    metadata: None,
                },
            },
        ]);
        snap.identities = vec!["doi:10.1/x".into(), "github:o/n".into()];
        let out = render_terminal(&snap);
        assert!(out.contains("━━ doi:10.1/x ━━"));
        assert!(out.contains("━━ github:o/n ━━"));
        // The DOI banner precedes its Citations block; the repo banner its Code block.
        let doi_pos = out.find("doi:10.1/x").unwrap();
        let repo_pos = out.find("github:o/n").unwrap();
        assert!(doi_pos < repo_pos);
        assert!(out.contains("stars"));
    }

    fn downloads_metric(provider: &str, identity: &str, value: u64, window: Window) -> Metric {
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

    fn downloads_result(provider: &str, identity: &str, value: u64, window: Window) -> FetchResult {
        FetchResult {
            provider: provider.into(),
            identity: identity.into(),
            category: Category::Downloads,
            outcome: Outcome::Values {
                metrics: vec![downloads_metric(provider, identity, value, window)],
                metadata: None,
            },
        }
    }

    #[test]
    fn shows_a_labelled_rollup_across_channels_with_a_shared_window() {
        let mut snap = snapshot_with(vec![
            downloads_result("crates.io", "crates:boast", 100, Window::Cumulative),
            downloads_result("bioconda", "conda:bioconda/boast", 50, Window::Cumulative),
        ]);
        snap.identities = vec!["crates:boast".into(), "conda:bioconda/boast".into()];

        let out = render_terminal(&snap);
        assert!(out.contains("Downloads Rollup"));
        assert!(out.contains("derived"));
        assert!(out.contains("150")); // the summed total
                                      // Named by identity so two Identities on the same provider (e.g. two
                                      // crates.io packages) would still be distinguishable.
        assert!(out.contains("crates:boast (100)"));
        assert!(out.contains("conda:bioconda/boast (50)"));
    }

    #[test]
    fn rollup_distinguishes_two_identities_on_the_same_provider() {
        let mut snap = snapshot_with(vec![
            downloads_result("crates.io", "crates:boast", 100, Window::Cumulative),
            downloads_result("crates.io", "crates:boast-cli", 25, Window::Cumulative),
        ]);
        snap.identities = vec!["crates:boast".into(), "crates:boast-cli".into()];

        let out = render_terminal(&snap);
        assert!(out.contains("125")); // the summed total
        assert!(out.contains("crates:boast (100)"));
        assert!(out.contains("crates:boast-cli (25)"));
    }

    #[test]
    fn no_rollup_when_windows_are_incompatible_or_theres_only_one_channel() {
        let mut snap = snapshot_with(vec![downloads_result(
            "crates.io",
            "crates:boast",
            100,
            Window::Cumulative,
        )]);
        snap.identities = vec!["crates:boast".into()];
        assert!(!render_terminal(&snap).contains("Rollup"));

        let mut snap = snapshot_with(vec![
            downloads_result("crates.io", "crates:boast", 100, Window::Cumulative),
            downloads_result(
                "homebrew",
                "homebrew:boast",
                50,
                Window::Trailing { days: 30 },
            ),
        ]);
        snap.identities = vec!["crates:boast".into(), "homebrew:boast".into()];
        assert!(!render_terminal(&snap).contains("Rollup"));
    }

    #[test]
    fn two_rollup_groups_share_one_heading() {
        let mut snap = snapshot_with(vec![
            downloads_result("crates.io", "crates:boast", 100, Window::Cumulative),
            downloads_result("bioconda", "conda:bioconda/boast", 50, Window::Cumulative),
            downloads_result("pypi", "pypi:boast", 7, Window::Trailing { days: 30 }),
            downloads_result(
                "homebrew",
                "homebrew:boast",
                3,
                Window::Trailing { days: 30 },
            ),
        ]);
        snap.identities = vec![
            "crates:boast".into(),
            "conda:bioconda/boast".into(),
            "pypi:boast".into(),
            "homebrew:boast".into(),
        ];

        let out = render_terminal(&snap);
        assert_eq!(out.matches("Downloads Rollup").count(), 1);
        assert!(out.contains("150 all-time"));
        assert!(out.contains("10 last 30 days"));
    }
}
