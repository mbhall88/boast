//! Diffs two Snapshots into per-Metric growth, purely offline (ADR-0001):
//! only Metrics sharing Provider, Identity, Name, *and* Window are compared
//! (ADR-0002 — a Rollup/diff may only ever combine exactly-equal Windows).
//! A Metric with no counterpart, or one whose Window changed, is reported
//! explicitly rather than silently dropped or forced into a comparison.

use time::format_description::well_known::Rfc3339;

use crate::model::{Category, Metric, MetricValue, Outcome, Snapshot, Window};
use crate::report;

/// One Metric present, with a matching Window, in both Snapshots.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricChange {
    pub provider: String,
    pub identity: String,
    pub name: String,
    pub category: Category,
    pub window: Window,
    pub old_value: MetricValue,
    pub new_value: MetricValue,
}

/// A Metric present under the same Provider/Identity/Name in both Snapshots,
/// but with a different Window — never diffed, since the two values aren't
/// measuring the same span of time.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowChanged {
    pub provider: String,
    pub identity: String,
    pub name: String,
    pub old_window: Window,
    pub new_window: Window,
}

/// The result of diffing two Snapshots: every shared Metric's change, plus
/// every mismatch accounted for explicitly — nothing is silently dropped.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Diff {
    pub changed: Vec<MetricChange>,
    /// Present in `new` with no counterpart in `old` — either a genuinely
    /// new Metric, or one `old` simply has no data point for (e.g. its
    /// Provider×Identity fetch failed in the old Snapshot). Either way,
    /// there is no prior value to compare against.
    pub added: Vec<Metric>,
    /// Present in `old` with no counterpart in `new`, and `new` positively
    /// confirmed why: an explicit `NotApplicable` for that Provider×Identity.
    /// A real, reportable disappearance.
    pub removed: Vec<Metric>,
    /// Present in `old` with no counterpart in `new`, but `new` never
    /// positively confirmed its absence — either that Provider×Identity
    /// fetch was `Failed` in the new Snapshot, or it wasn't attempted at all
    /// (e.g. a different identity/provider set was given this run). Neither
    /// case is confirmed absence, so it's never coerced into `removed`
    /// (ADR-0002: "a missing number and a zero number are different facts" —
    /// the same principle extends to "missing" vs. "confirmed gone").
    pub inconclusive: Vec<Metric>,
    pub window_changed: Vec<WindowChanged>,
}

/// Diff `old` against `new`. A Metric is matched by (Provider, Identity,
/// Name); a match with an equal Window becomes a [`MetricChange`], a match
/// with a differing Window becomes a [`WindowChanged`]. An old Metric with no
/// match is `removed` only when `new` positively confirms absence
/// (`NotApplicable`) for that Provider×Identity; every other case — a
/// `Failed` fetch, or the Provider×Identity not being attempted at all in
/// `new` — is `inconclusive` rather than assumed gone.
///
/// Assumes no two Metrics in one Snapshot share (Provider, Identity, Name) —
/// true for every Provider today (e.g. Homebrew's three trailing-window
/// download counts are named `downloads_30d`/`downloads_90d`/`downloads_365d`,
/// not sharing a name). A future Provider that violated this would still
/// match Metrics correctly by name, but Window-equality checking happens
/// only after the first same-name candidate is picked, so an old Metric
/// could pair with the wrong same-named Window instead of its true match.
pub fn compute(old: &Snapshot, new: &Snapshot) -> Diff {
    let mut diff = Diff::default();
    let new_metrics: Vec<&Metric> = new.metrics().collect();
    let mut matched = vec![false; new_metrics.len()];

    for om in old.metrics() {
        let found = new_metrics.iter().enumerate().find(|(i, nm)| {
            !matched[*i]
                && nm.provider == om.provider
                && nm.identity == om.identity
                && nm.name == om.name
        });
        match found {
            Some((i, nm)) => {
                matched[i] = true;
                if nm.window == om.window {
                    diff.changed.push(MetricChange {
                        provider: om.provider.clone(),
                        identity: om.identity.clone(),
                        name: om.name.clone(),
                        category: om.category,
                        window: om.window.clone(),
                        old_value: om.value.clone(),
                        new_value: nm.value.clone(),
                    });
                } else {
                    diff.window_changed.push(WindowChanged {
                        provider: om.provider.clone(),
                        identity: om.identity.clone(),
                        name: om.name.clone(),
                        old_window: om.window.clone(),
                        new_window: nm.window.clone(),
                    });
                }
            }
            None if confirmed_not_applicable(new, &om.provider, &om.identity) => {
                diff.removed.push(om.clone());
            }
            None => diff.inconclusive.push(om.clone()),
        }
    }

    for (i, nm) in new_metrics.iter().enumerate() {
        if !matched[i] {
            diff.added.push((*nm).clone());
        }
    }

    diff
}

/// Whether `snapshot` positively confirmed this Provider×Identity has no
/// presence (`NotApplicable`) — the only outcome honest enough to justify
/// `removed`. `false` covers both `Failed` *and* no `FetchResult` at all,
/// since neither is a confirmed answer.
fn confirmed_not_applicable(snapshot: &Snapshot, provider: &str, identity: &str) -> bool {
    snapshot.results.iter().any(|r| {
        r.provider == provider
            && r.identity == identity
            && matches!(r.outcome, Outcome::NotApplicable { .. })
    })
}

/// Render a Diff as a terminal-friendly string: shared Metrics grouped by
/// Identity then Category (matching the Report renderers' grouping), then
/// added/removed/inconclusive/window-changed sections, then any Provider
/// notice carried by either Snapshot (ADR-0005 — a notice must survive every
/// Report format, including a diff between two of them).
pub fn render(old: &Snapshot, new: &Snapshot, diff: &Diff) -> String {
    let mut out = String::new();

    let old_at = old
        .created_at
        .format(&Rfc3339)
        .unwrap_or_else(|_| old.created_at.to_string());
    let new_at = new
        .created_at
        .format(&Rfc3339)
        .unwrap_or_else(|_| new.created_at.to_string());
    out.push_str(&format!("boast diff — {old_at} → {new_at}\n"));

    let mut identities = old.identities.clone();
    for id in &new.identities {
        if !identities.contains(id) {
            identities.push(id.clone());
        }
    }

    for identity in &identities {
        let changes: Vec<&MetricChange> = diff
            .changed
            .iter()
            .filter(|c| &c.identity == identity)
            .collect();
        if changes.is_empty() {
            continue;
        }
        out.push_str(&format!("\n━━ {identity} ━━\n"));
        for category in report::CATEGORY_ORDER {
            let rows: Vec<&MetricChange> = changes
                .iter()
                .filter(|c| c.category == category)
                .copied()
                .collect();
            if rows.is_empty() {
                continue;
            }
            out.push_str(&format!("── {} ──\n", category.label()));
            for c in rows {
                out.push_str(&format!(
                    "  {name:<20}  {provider:<12}  {change}\n",
                    name = c.name,
                    provider = c.provider,
                    change = describe_change(&c.old_value, &c.new_value),
                ));
            }
        }
    }

    render_metric_section(&mut out, "Added (no earlier data point)", &diff.added);
    render_metric_section(&mut out, "Removed (no longer present)", &diff.removed);
    render_metric_section(
        &mut out,
        "Inconclusive (not fetched, or fetch failed, in the new Snapshot; not confirmed removed)",
        &diff.inconclusive,
    );

    if !diff.window_changed.is_empty() {
        out.push_str("\nWindow changed (not diffed):\n");
        for w in &diff.window_changed {
            out.push_str(&format!(
                "  {} ({}) {} — {} → {}\n",
                w.name,
                w.provider,
                w.identity,
                w.old_window.describe(),
                w.new_window.describe(),
            ));
        }
    }

    let mut notices = report::provider_notices(old);
    for n in report::provider_notices(new) {
        if !notices.contains(&n) {
            notices.push(n);
        }
    }
    if !notices.is_empty() {
        out.push_str("\n── Notices ──\n");
        for notice in notices {
            out.push_str(&format!("  {notice}\n"));
        }
    }

    out
}

fn render_metric_section(out: &mut String, heading: &str, metrics: &[Metric]) {
    if metrics.is_empty() {
        return;
    }
    out.push_str(&format!("\n{heading}:\n"));
    for m in metrics {
        out.push_str(&format!(
            "  {} ({}) {} — {}\n",
            m.name, m.provider, m.identity, m.value,
        ));
    }
}

/// Describe the change between two values of the same Metric. `Count`/`Real`
/// get a signed numeric delta; other value kinds (or a value whose *kind*
/// changed between runs, which shouldn't happen but is handled rather than
/// panicking) just show old → new, or "(unchanged)" when equal.
fn describe_change(old: &MetricValue, new: &MetricValue) -> String {
    match (old, new) {
        (MetricValue::Count(o), MetricValue::Count(n)) => {
            let delta = *n as i64 - *o as i64;
            format!("{o} → {n} ({delta:+})")
        }
        (MetricValue::Real(o), MetricValue::Real(n)) => {
            format!("{o:.2} → {n:.2} ({:+.2})", n - o)
        }
        _ if old == new => format!("{old} (unchanged)"),
        _ => format!("{old} → {new}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, FetchResult};
    use time::OffsetDateTime;

    fn snapshot_at(secs: i64, results: Vec<FetchResult>) -> Snapshot {
        let identities = results
            .iter()
            .map(|r| r.identity.clone())
            .collect::<Vec<_>>();
        Snapshot {
            schema_version: Snapshot::SCHEMA_VERSION,
            tool: "boast".into(),
            tool_version: "0.1.0".into(),
            created_at: OffsetDateTime::from_unix_timestamp(secs).unwrap(),
            identities,
            results,
        }
    }

    fn metric(name: &str, value: MetricValue, window: Window) -> Metric {
        Metric {
            name: name.into(),
            category: Category::Citations,
            value,
            window,
            provider: "openalex".into(),
            identity: "doi:10.1/x".into(),
            as_of: OffsetDateTime::UNIX_EPOCH,
            source: "https://api.openalex.org/works/doi:10.1/x".into(),
            note: None,
        }
    }

    fn values_result(metrics: Vec<Metric>) -> FetchResult {
        FetchResult {
            provider: metrics[0].provider.clone(),
            identity: metrics[0].identity.clone(),
            category: metrics[0].category,
            outcome: Outcome::Values {
                metrics,
                metadata: None,
            },
        }
    }

    #[test]
    fn matched_metrics_with_equal_windows_are_changed() {
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(1421),
                Window::Cumulative,
            )])],
        );
        let new = snapshot_at(
            100,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(1450),
                Window::Cumulative,
            )])],
        );

        let diff = compute(&old, &new);
        assert_eq!(diff.changed.len(), 1);
        let c = &diff.changed[0];
        assert_eq!(c.old_value, MetricValue::Count(1421));
        assert_eq!(c.new_value, MetricValue::Count(1450));
        assert_eq!(c.provider, "openalex");
        assert_eq!(c.identity, "doi:10.1/x");
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.window_changed.is_empty());
        assert!(diff.inconclusive.is_empty());
    }

    #[test]
    fn unchanged_values_are_still_reported_not_silently_dropped() {
        let m = metric("citations", MetricValue::Count(1421), Window::Cumulative);
        let old = snapshot_at(0, vec![values_result(vec![m.clone()])]);
        let new = snapshot_at(100, vec![values_result(vec![m])]);

        let diff = compute(&old, &new);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].old_value, diff.changed[0].new_value);
    }

    #[test]
    fn different_named_metrics_are_never_conflated() {
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(1421),
                Window::Cumulative,
            )])],
        );
        let new = snapshot_at(
            100,
            vec![values_result(vec![metric(
                "fwci",
                MetricValue::Real(2.0),
                Window::Cumulative,
            )])],
        );

        let diff = compute(&old, &new);
        assert!(diff.changed.is_empty());
        // Neither Snapshot confirmed "citations" absent for this identity —
        // `new` just never produced one — so it's inconclusive, not removed.
        assert_eq!(diff.inconclusive.len(), 1);
        assert_eq!(diff.inconclusive[0].name, "citations");
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].name, "fwci");
    }

    #[test]
    fn different_identities_are_never_conflated() {
        let mut a = metric("citations", MetricValue::Count(1421), Window::Cumulative);
        a.identity = "doi:10.1/x".into();
        let mut b = metric("citations", MetricValue::Count(900), Window::Cumulative);
        b.identity = "doi:10.2/y".into();

        let old = snapshot_at(0, vec![values_result(vec![a])]);
        let new = snapshot_at(100, vec![values_result(vec![b])]);

        let diff = compute(&old, &new);
        assert!(diff.changed.is_empty());
        assert_eq!(diff.inconclusive.len(), 1);
        assert_eq!(diff.added.len(), 1);
    }

    #[test]
    fn a_window_mismatch_is_reported_explicitly_not_diffed() {
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "downloads",
                MetricValue::Count(100),
                Window::Cumulative,
            )])],
        );
        let new = snapshot_at(
            100,
            vec![values_result(vec![metric(
                "downloads",
                MetricValue::Count(30),
                Window::Trailing { days: 30 },
            )])],
        );

        let diff = compute(&old, &new);
        assert!(diff.changed.is_empty());
        assert_eq!(diff.window_changed.len(), 1);
        assert_eq!(diff.window_changed[0].old_window, Window::Cumulative);
        assert_eq!(
            diff.window_changed[0].new_window,
            Window::Trailing { days: 30 }
        );
    }

    #[test]
    fn a_metric_only_in_new_is_added() {
        let old = snapshot_at(0, vec![]);
        let new = snapshot_at(
            100,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(5),
                Window::Cumulative,
            )])],
        );
        let diff = compute(&old, &new);
        assert_eq!(diff.added.len(), 1);
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn a_metric_missing_from_new_is_removed_when_new_has_no_failure_for_it() {
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(5),
                Window::Cumulative,
            )])],
        );
        // The new Snapshot ran and found nothing (NotApplicable) — a
        // confirmed disappearance, not an unknown.
        let new = snapshot_at(
            100,
            vec![FetchResult {
                provider: "openalex".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Citations,
                outcome: Outcome::NotApplicable {
                    note: "not found in OpenAlex".into(),
                },
            }],
        );

        let diff = compute(&old, &new);
        assert_eq!(diff.removed.len(), 1);
        assert!(diff.inconclusive.is_empty());
    }

    #[test]
    fn a_metric_missing_because_the_new_fetch_failed_is_inconclusive_not_removed() {
        // ADR-0002: a transient fetch failure must never be coerced into a
        // confident claim — here, "this metric was removed" would be wrong.
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(5),
                Window::Cumulative,
            )])],
        );
        let new = snapshot_at(
            100,
            vec![FetchResult {
                provider: "openalex".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Citations,
                outcome: Outcome::Failed {
                    error: "rate limited (429)".into(),
                },
            }],
        );

        let diff = compute(&old, &new);
        assert!(diff.removed.is_empty());
        assert_eq!(diff.inconclusive.len(), 1);
        assert_eq!(diff.inconclusive[0].name, "citations");
    }

    #[test]
    fn a_provider_never_attempted_in_new_is_inconclusive_not_removed() {
        // ADR-0002: the new Snapshot has no FetchResult at all for this
        // Provider×Identity (e.g. a different provider/identity set was
        // given this run) — "never checked" is not confirmed absence either.
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(5),
                Window::Cumulative,
            )])],
        );
        let new = snapshot_at(100, vec![]);

        let diff = compute(&old, &new);
        assert!(
            diff.removed.is_empty(),
            "must never assume absence just because it wasn't checked"
        );
        assert_eq!(diff.inconclusive.len(), 1);
        assert_eq!(diff.inconclusive[0].name, "citations");
    }

    // -- render ---------------------------------------------------------

    #[test]
    fn render_shows_a_signed_delta_and_groups_by_category() {
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(1421),
                Window::Cumulative,
            )])],
        );
        let new = snapshot_at(
            100,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(1450),
                Window::Cumulative,
            )])],
        );
        let d = compute(&old, &new);
        let out = render(&old, &new, &d);

        assert!(out.contains("━━ doi:10.1/x ━━"));
        assert!(out.contains("── Citations ──"));
        assert!(out.contains("1421 → 1450 (+29)"));
    }

    #[test]
    fn render_shows_a_negative_delta_without_hiding_a_regression() {
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(100),
                Window::Cumulative,
            )])],
        );
        let new = snapshot_at(
            100,
            vec![values_result(vec![metric(
                "citations",
                MetricValue::Count(90),
                Window::Cumulative,
            )])],
        );
        let d = compute(&old, &new);
        let out = render(&old, &new, &d);
        assert!(out.contains("100 → 90 (-10)"));
    }

    #[test]
    fn render_lists_added_removed_and_inconclusive_sections() {
        let old = snapshot_at(
            0,
            vec![
                values_result(vec![metric(
                    "citations",
                    MetricValue::Count(5),
                    Window::Cumulative,
                )]),
                FetchResult {
                    provider: "dimensions".into(),
                    identity: "doi:10.1/x".into(),
                    category: Category::Citations,
                    outcome: Outcome::Failed {
                        error: "timeout".into(),
                    },
                },
            ],
        );
        let new = snapshot_at(
            100,
            vec![
                FetchResult {
                    provider: "openalex".into(),
                    identity: "doi:10.1/x".into(),
                    category: Category::Citations,
                    outcome: Outcome::Failed {
                        error: "rate limited (429)".into(),
                    },
                },
                values_result(vec![Metric {
                    provider: "dimensions".into(),
                    ..metric("citations", MetricValue::Count(9), Window::Cumulative)
                }]),
            ],
        );

        let d = compute(&old, &new);
        let out = render(&old, &new, &d);
        assert!(out.contains("Added (no earlier data point)"));
        assert!(out.contains("dimensions"));
        assert!(out.contains("Inconclusive"));
        assert!(out.contains("openalex"));
        assert!(!out.contains("Removed (no longer present)"));
    }

    #[test]
    fn render_flags_a_window_change_without_diffing_it() {
        let old = snapshot_at(
            0,
            vec![values_result(vec![metric(
                "downloads",
                MetricValue::Count(100),
                Window::Cumulative,
            )])],
        );
        let new = snapshot_at(
            100,
            vec![values_result(vec![metric(
                "downloads",
                MetricValue::Count(30),
                Window::Trailing { days: 30 },
            )])],
        );
        let d = compute(&old, &new);
        let out = render(&old, &new, &d);
        assert!(out.contains("Window changed"));
        assert!(out.contains("all-time → last 30 days"));
        assert!(!out.contains("── Citations ──"));
    }

    #[test]
    fn render_carries_a_providers_notice_from_either_snapshot() {
        let notice = "x".repeat(120);
        let old = snapshot_at(
            0,
            vec![values_result(vec![Metric {
                note: Some(notice.clone()),
                provider: "dimensions".into(),
                ..metric("citations", MetricValue::Count(5), Window::Cumulative)
            }])],
        );
        let new = snapshot_at(
            100,
            vec![values_result(vec![Metric {
                note: Some(notice.clone()),
                provider: "dimensions".into(),
                ..metric("citations", MetricValue::Count(9), Window::Cumulative)
            }])],
        );
        let d = compute(&old, &new);
        let out = render(&old, &new, &d);
        assert!(out.contains("── Notices ──"));
        // Shared by both Snapshots — must still appear once, not twice.
        assert_eq!(out.matches(notice.as_str()).count(), 1);
    }
}
