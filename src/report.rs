//! Renders a Snapshot as a human-readable terminal table, grouped by Category.
//! A Report is always derived from a Snapshot and never fetches (ADR-0001).
//! NotApplicable shows as N/A and Failed is flagged — never a misleading 0.

use time::format_description::well_known::Rfc3339;

use crate::model::{Category, Outcome, Snapshot};

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

/// Render the Snapshot as a terminal-friendly string.
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
    out.push_str(&format!("identities: {}\n", snapshot.identities.join(", ")));

    for category in CATEGORY_ORDER {
        let rows = rows_for(snapshot, category);
        if rows.is_empty() {
            continue;
        }
        out.push('\n');
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

    if snapshot.has_failures() {
        out.push_str(
            "\n⚠ partial snapshot: some metrics failed to fetch (see FAILED rows); exit code 1.\n",
        );
    }

    out
}

/// Build the display rows for one Category from the Snapshot's results.
fn rows_for(snapshot: &Snapshot, category: Category) -> Vec<Row> {
    let mut rows = Vec::new();
    for result in &snapshot.results {
        if result.category != category {
            continue;
        }
        match &result.outcome {
            Outcome::Values { metrics } => {
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
                name: result.identity.clone(),
                value: "N/A".to_string(),
                window: String::new(),
                provider: result.provider.clone(),
                detail: note.clone(),
            }),
            Outcome::Failed { error } => rows.push(Row {
                name: result.identity.clone(),
                value: "FAILED".to_string(),
                window: String::new(),
                provider: result.provider.clone(),
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
        let snap = snapshot_with(vec![
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
        let out = render_terminal(&snap);
        assert!(out.contains("N/A"));
        assert!(out.contains("FAILED"));
        assert!(!out.contains(" 0 "));
        assert!(out.contains("partial snapshot"));
    }
}
