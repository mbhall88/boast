//! Renders a Snapshot into a terminal table, a Markdown Report, or a
//! grant-ready prose sentence. A Report is always derived from a Snapshot and
//! never fetches (ADR-0001). NotApplicable shows as N/A and Failed is
//! flagged — never a misleading 0.

use time::format_description::well_known::Rfc3339;

use crate::model::{Category, MetricValue, Outcome, Snapshot, Window};
use crate::rollup;

/// Display order for Categories, shared with [`crate::diff`]'s renderer so
/// both group Metrics identically.
pub(crate) const CATEGORY_ORDER: [Category; 4] = [
    Category::Code,
    Category::Downloads,
    Category::Citations,
    Category::Attention,
];

/// A `Metric.note` at or under this length is a short interpretive gloss
/// (e.g. "field-weighted citation impact; 1.0 = world average") and stays
/// inline on its row. Past it, a note is treated as a Provider-level notice
/// (e.g. a licence/terms notice) and promoted to the once-per-run footer
/// instead — see ADR-0005 for why it isn't logged to stderr instead.
const INLINE_DETAIL_LIMIT: usize = 80;

struct Row {
    name: String,
    value: String,
    window: String,
    provider: String,
    detail: String,
    /// The Metric's source URL, empty for a `NotApplicable`/`Failed` row
    /// (there's nothing to attribute). Only Markdown links it — a terminal
    /// table has nowhere useful to put a raw URL.
    source: String,
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
                let line = format!(
                    "  {name:<w_name$}  {value:>w_value$}  {window:<w_window$}  {provider:<w_provider$}",
                    name = r.name,
                    value = r.value,
                    window = r.window,
                    provider = r.provider,
                );
                // Trim trailing whitespace left by empty columns. A notice
                // over the inline limit is skipped here — it appears once in
                // the footer below, not repeated on every row that carries it.
                out.push_str(line.trim_end());
                if !r.detail.is_empty() && r.detail.len() <= INLINE_DETAIL_LIMIT {
                    out.push_str(&format!("  {}", r.detail));
                }
                out.push('\n');
            }
        }
    }

    let downloads = snapshot.metrics().filter(|m| rollup::counts_as_download(m));
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

    let notices = provider_notices(snapshot);
    if !notices.is_empty() {
        out.push_str("\n── Notices ──\n");
        for notice in notices {
            out.push_str(&format!("  {notice}\n"));
        }
    }

    let provider_notes = provider_operational_notes(snapshot);
    if !provider_notes.is_empty() {
        out.push_str("\n── Provider Notes ──\n");
        for note in provider_notes {
            out.push_str(&format!(
                "  {} ({}): {} — {}\n",
                note.provider,
                note.kind.label(),
                note.message,
                format_covered_identities(&note.identities),
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

/// Render the Snapshot as a grant-friendly Markdown Report, grouped by
/// identity then Category — the primary saved artifact for pasting into a
/// document or README (never fetches; see ADR-0001).
pub fn render_markdown(snapshot: &Snapshot) -> String {
    let mut out = String::new();

    let created = snapshot
        .created_at
        .format(&Rfc3339)
        .unwrap_or_else(|_| snapshot.created_at.to_string());
    out.push_str(&format!(
        "# boast Report\n\n_boast {} — as of {created}_\n",
        snapshot.tool_version
    ));

    for identity in &snapshot.identities {
        out.push_str(&format!("\n## {identity}\n"));

        for md in snapshot.descriptions().filter(|d| &d.identity == identity) {
            let summary = md.summary();
            if !summary.is_empty() {
                out.push_str(&format!("\n{summary}\n"));
            }
        }

        for category in CATEGORY_ORDER {
            let rows = rows_for(snapshot, identity, category);
            if rows.is_empty() {
                continue;
            }
            out.push_str(&format!("\n### {}\n\n", category.label()));
            out.push_str("| Metric | Value | Window | Provider | Detail |\n");
            out.push_str("| --- | --- | --- | --- | --- |\n");

            for r in rows {
                let detail = if !r.detail.is_empty() && r.detail.len() <= INLINE_DETAIL_LIMIT {
                    escape_md_cell(&r.detail)
                } else {
                    String::new()
                };
                // Link the Provider to its source URL when there is one, so
                // the number is attributed to a clickable source, not just a
                // name — Markdown's advantage over the terminal table.
                let provider = if r.source.is_empty() {
                    escape_md_cell(&r.provider)
                } else {
                    format!("[{}]({})", escape_md_cell(&r.provider), r.source)
                };
                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    escape_md_cell(&r.name),
                    escape_md_cell(&r.value),
                    escape_md_cell(&r.window),
                    provider,
                    detail,
                ));
            }
        }
    }

    let downloads = snapshot.metrics().filter(|m| rollup::counts_as_download(m));
    let rollups = rollup::compute(downloads);
    if !rollups.is_empty() {
        out.push_str("\n## Downloads Rollup (derived — see channels above)\n\n");
        for r in &rollups {
            let breakdown: Vec<String> = r
                .channels
                .iter()
                .map(|c| format!("{} ({})", c.identity, c.value))
                .collect();
            out.push_str(&format!(
                "- **{}** {} = {}\n",
                r.total,
                r.window.describe(),
                breakdown.join(" + "),
            ));
        }
    }

    let notices = provider_notices(snapshot);
    if !notices.is_empty() {
        out.push_str("\n## Notices\n\n");
        for notice in notices {
            out.push_str(&format!("- {notice}\n"));
        }
    }

    let provider_notes = provider_operational_notes(snapshot);
    if !provider_notes.is_empty() {
        out.push_str("\n## Provider Notes\n\n");
        for note in provider_notes {
            out.push_str(&format!(
                "- **{}** ({}): {} — {}\n",
                escape_md_cell(note.provider),
                note.kind.label(),
                escape_md_cell(note.message),
                escape_md_cell(&format_covered_identities(&note.identities)),
            ));
        }
    }

    if snapshot.has_failures() {
        out.push_str(
            "\n> ⚠ **partial snapshot**: some metrics failed to fetch (see FAILED rows); exit code 1.\n",
        );
    }

    out
}

/// Escape a value bound for a Markdown table cell: a literal `|` would split
/// the row into an extra column, and an embedded newline would break out of
/// the row entirely.
fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// Render a single grant-ready sentence summarising the Snapshot's headline
/// Metrics (never fetches; see ADR-0001). Only Metrics that actually resolved
/// to a Value are named — an absent or failed Metric is silently omitted
/// rather than implied as 0 (ADR-0002); a partial Snapshot is flagged in a
/// trailing caveat instead of being folded into the headline numbers. If the
/// headline citation count carries a Provider-level notice (e.g. Dimensions'
/// licence text), that notice is appended too — quoting a keyed/terms-gated
/// number without its terms is exactly what ADR-0005 exists to prevent, and
/// prose is as much a paste target as the Markdown Report.
pub fn render_prose(snapshot: &Snapshot) -> String {
    let date_fmt = time::macros::format_description!("[year]-[month]-[day]");
    let date = snapshot
        .created_at
        .format(&date_fmt)
        .unwrap_or_else(|_| snapshot.created_at.to_string());

    let mut clauses = Vec::new();
    let mut citation_notice = None;
    if let Some((count, provider, notice)) = headline_citations(snapshot) {
        clauses.push(format!("has been cited {count} times ({provider})"));
        citation_notice = notice;
    }
    if let Some(stars) = headline_stars(snapshot) {
        clauses.push(format!("has {stars} GitHub stars"));
    }
    if let Some(headline) = headline_downloads(snapshot) {
        clauses.push(match headline {
            DownloadsHeadline::Rollup {
                total,
                channels,
                window,
            } => format!(
                "has been downloaded {total} times ({}) across {channels} channels",
                window.describe(),
            ),
            DownloadsHeadline::Single {
                total,
                provider,
                window,
            } => format!(
                "has been downloaded {total} times ({provider}, {})",
                window.describe(),
            ),
        });
    }

    let mut sentence = if clauses.is_empty() {
        format!("As of {date}, no headline metrics were available for this Snapshot.")
    } else {
        format!("As of {date}, this project {}.", join_with_and(&clauses))
    };

    if let Some(notice) = citation_notice {
        sentence.push(' ');
        sentence.push_str(&notice);
    }

    if snapshot.has_failures() {
        sentence.push_str(" (Note: some metrics failed to fetch and are not reflected here.)");
    }

    sentence
}

/// Join clauses into an English list with an Oxford comma: `"a"`, `"a and
/// b"`, `"a, b, and c"`.
fn join_with_and(clauses: &[String]) -> String {
    match clauses {
        [] => String::new(),
        [only] => only.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// The largest "citations" Metric across the whole Snapshot, with the
/// Provider it came from — the "cite the higher one honestly and attribute
/// it" headline number (see user story 27) — plus its Provider-level notice,
/// if it carries one (over [`INLINE_DETAIL_LIMIT`], e.g. Dimensions' licence
/// text), so a caller can't headline a number while silently dropping the
/// terms attached to it (ADR-0005).
///
/// Chosen across the *whole* Snapshot, not per Identity: correct for the
/// common case of one Project naming one paper. A Snapshot spanning several
/// distinct papers would blend their counts into one unattributed clause —
/// an accepted v1 simplification matching prose's "a single sentence" brief,
/// not a per-Identity breakdown.
fn headline_citations(snapshot: &Snapshot) -> Option<(u64, String, Option<String>)> {
    snapshot
        .metrics()
        .filter(|m| m.category == Category::Citations && m.name == "citations")
        .filter_map(|m| match m.value {
            MetricValue::Count(c) => {
                let notice = m
                    .note
                    .as_ref()
                    .filter(|n| n.len() > INLINE_DETAIL_LIMIT)
                    .cloned();
                Some((c, m.provider.clone(), notice))
            }
            _ => None,
        })
        .max_by_key(|(c, _, _)| *c)
}

/// The largest "stars" Metric across the whole Snapshot. Whole-Snapshot, not
/// per Identity — see [`headline_citations`]'s doc comment for why.
fn headline_stars(snapshot: &Snapshot) -> Option<u64> {
    snapshot
        .metrics()
        .filter(|m| m.category == Category::Code && m.name == "stars")
        .filter_map(|m| match m.value {
            MetricValue::Count(c) => Some(c),
            _ => None,
        })
        .max()
}

enum DownloadsHeadline {
    Rollup {
        total: u64,
        channels: usize,
        window: Window,
    },
    Single {
        total: u64,
        provider: String,
        window: Window,
    },
}

/// The headline download figure: the largest labelled Rollup if two or more
/// channels share a Window, otherwise the single largest raw channel — so a
/// Project with only one download channel still gets a headline number.
///
/// When Rollups exist in more than one Window (e.g. an all-time total and a
/// separate 30-day total), the one with the larger raw total wins — usually
/// the cumulative figure, simply because it's the bigger number. This never
/// misleads on its own: the rendered clause always states which Window the
/// number covers (`window.describe()`), so "biggest wins" is a deliberate,
/// labelled tie-break rather than a claim that it's the most representative.
fn headline_downloads(snapshot: &Snapshot) -> Option<DownloadsHeadline> {
    let downloads: Vec<_> = snapshot
        .metrics()
        .filter(|m| rollup::counts_as_download(m))
        .collect();

    let rollups = rollup::compute(downloads.iter().copied());
    if let Some(best) = rollups.into_iter().max_by_key(|r| r.total) {
        return Some(DownloadsHeadline::Rollup {
            total: best.total,
            channels: best.channels.len(),
            window: best.window,
        });
    }

    downloads
        .into_iter()
        .filter_map(|m| match m.value {
            MetricValue::Count(c) => Some((c, m.provider.clone(), m.window.clone())),
            _ => None,
        })
        .max_by_key(|(c, _, _)| *c)
        .map(|(total, provider, window)| DownloadsHeadline::Single {
            total,
            provider,
            window,
        })
}

/// Every distinct Provider-level notice (a `Metric.note` over
/// [`INLINE_DETAIL_LIMIT`]) across the whole Snapshot, in first-seen order
/// and de-duplicated — so a notice shared by several Identities (e.g. the
/// same Dimensions licence text on every DOI) is shown once per run. Shared
/// with [`crate::diff`]'s renderer, which merges this across both Snapshots
/// being diffed (ADR-0005: a notice must survive every Report format).
pub(crate) fn provider_notices(snapshot: &Snapshot) -> Vec<String> {
    let mut notices = Vec::new();
    for m in snapshot.metrics() {
        if let Some(note) = &m.note {
            if note.len() > INLINE_DETAIL_LIMIT && !notices.contains(note) {
                notices.push(note.clone());
            }
        }
    }
    notices
}

/// Which Outcome a [`ProviderNote`] came from. Kept in the dedup key
/// alongside Provider (ADR-0002/ADR-0008): a `NotApplicable` and a `Failed`
/// must never merge even when their text happens to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutcomeKind {
    NotApplicable,
    Failed,
}

impl OutcomeKind {
    /// Matches the value shown in the row's own Value column, so the footer
    /// entry and the row it explains use the same word.
    pub(crate) fn label(self) -> &'static str {
        match self {
            OutcomeKind::NotApplicable => "N/A",
            OutcomeKind::Failed => "FAILED",
        }
    }
}

/// One `── Provider Notes ──` entry: a `NotApplicable`/`Failed` message over
/// [`INLINE_DETAIL_LIMIT`], with every Identity it was seen on. Structured
/// rather than pre-formatted so `render_terminal` and `render_markdown` (and
/// potentially `diff::render`, see #64) can each frame it their own way.
pub(crate) struct ProviderNote<'a> {
    pub provider: &'a str,
    pub kind: OutcomeKind,
    pub message: &'a str,
    pub identities: Vec<&'a str>,
}

/// Sibling to [`provider_notices`]: scans `NotApplicable`/`Failed` Outcomes
/// instead of `Metric.note`, since neither Outcome carries a Metric for
/// `provider_notices` to find (ADR-0008 — this is deliberately a new
/// function, not a widening of `provider_notices`, which stays scoped to
/// licence-notice text). Dedups on `(provider, outcome kind, message)` in
/// first-seen order; Identity is excluded from the key so one shared message
/// collapses to a single entry naming every Identity it covers.
pub(crate) fn provider_operational_notes(snapshot: &Snapshot) -> Vec<ProviderNote<'_>> {
    let mut notes: Vec<ProviderNote<'_>> = Vec::new();
    for result in &snapshot.results {
        let (kind, message) = match &result.outcome {
            Outcome::NotApplicable { note } => (OutcomeKind::NotApplicable, note.as_str()),
            Outcome::Failed { error } => (OutcomeKind::Failed, error.as_str()),
            Outcome::Values { .. } => continue,
        };
        if message.len() <= INLINE_DETAIL_LIMIT {
            continue;
        }
        match notes
            .iter_mut()
            .find(|n| n.provider == result.provider && n.kind == kind && n.message == message)
        {
            Some(existing) => existing.identities.push(&result.identity),
            None => notes.push(ProviderNote {
                provider: &result.provider,
                kind,
                message,
                identities: vec![&result.identity],
            }),
        }
    }
    notes
}

/// A [`ProviderNote`]'s covered Identities may run to hundreds under an
/// ORCID expansion (ADR-0006, ~118 works); past a small threshold the list is
/// truncated to `a, b, c, and N more` so one shared message doesn't turn its
/// own footer entry into a wall of identifiers.
const IDENTITY_PREVIEW_LIMIT: usize = 3;

fn format_covered_identities(identities: &[&str]) -> String {
    if identities.len() <= IDENTITY_PREVIEW_LIMIT {
        let owned: Vec<String> = identities.iter().map(|s| s.to_string()).collect();
        join_with_and(&owned)
    } else {
        let shown = identities[..IDENTITY_PREVIEW_LIMIT].join(", ");
        let more = identities.len() - IDENTITY_PREVIEW_LIMIT;
        format!("{shown}, and {more} more")
    }
}

/// Build the display rows for one identity and Category from the Snapshot.
/// A `Values` outcome buckets each Metric by its *own* `category` — a single
/// Provider fetch can carry Metrics belonging to different Categories (e.g.
/// OpenAlex's open-access status is Attention, alongside its Citations
/// metrics from the same call) — matching `crate::diff`'s grouping. A
/// `NotApplicable`/`Failed` outcome has no per-Metric breakdown to consult,
/// so it falls back to the FetchResult's (i.e. the Provider's) own Category.
fn rows_for(snapshot: &Snapshot, identity: &str, category: Category) -> Vec<Row> {
    let mut rows = Vec::new();
    for result in &snapshot.results {
        if result.identity != identity {
            continue;
        }
        match &result.outcome {
            Outcome::Values { metrics, .. } => {
                for m in metrics {
                    if m.category != category {
                        continue;
                    }
                    rows.push(Row {
                        name: m.name.clone(),
                        value: m.value.to_string(),
                        window: m.window.describe(),
                        provider: m.provider.clone(),
                        detail: m.note.clone().unwrap_or_default(),
                        source: m.source.clone(),
                    });
                }
            }
            Outcome::NotApplicable { note } if result.category == category => rows.push(Row {
                name: result.provider.clone(),
                value: "N/A".to_string(),
                window: String::new(),
                provider: String::new(),
                detail: note.clone(),
                source: String::new(),
            }),
            Outcome::Failed { error } if result.category == category => rows.push(Row {
                name: result.provider.clone(),
                value: "FAILED".to_string(),
                window: String::new(),
                provider: String::new(),
                detail: error.clone(),
                source: String::new(),
            }),
            Outcome::NotApplicable { .. } | Outcome::Failed { .. } => {}
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
    fn long_detail_moves_to_the_notices_footer_short_detail_stays_inline() {
        let long_note = "x".repeat(120);
        let snap = snapshot_with(vec![FetchResult {
            provider: "dimensions".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![
                    Metric {
                        note: Some(long_note.clone()),
                        provider: "dimensions".into(),
                        ..metric("citations", MetricValue::Count(5))
                    },
                    Metric {
                        note: Some("short".into()),
                        provider: "dimensions".into(),
                        ..metric("fcr", MetricValue::Real(1.0))
                    },
                ],
                metadata: None,
            },
        }]);
        let out = render_terminal(&snap);
        assert!(out.contains("── Notices ──"));
        // The long note lives in the footer, not appended to its row.
        let citations_line = out.lines().find(|l| l.contains("citations")).unwrap();
        assert!(!citations_line.contains(&long_note));
        let footer_line = out.lines().find(|l| l.contains(&long_note)).unwrap();
        assert!(!footer_line.contains("citations"));
        // The short note still shares a line with its row.
        let short_line = out.lines().find(|l| l.contains("short")).unwrap();
        assert!(short_line.contains("fcr"));
    }

    #[test]
    fn identical_notices_across_identities_are_shown_once() {
        let notice = "x".repeat(120);
        let dimensions_metric = |identity: &str| Metric {
            note: Some(notice.clone()),
            provider: "dimensions".into(),
            identity: identity.into(),
            ..metric("citations", MetricValue::Count(5))
        };
        let mut snap = snapshot_with(vec![
            FetchResult {
                provider: "dimensions".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Citations,
                outcome: Outcome::Values {
                    metrics: vec![dimensions_metric("doi:10.1/x")],
                    metadata: None,
                },
            },
            FetchResult {
                provider: "dimensions".into(),
                identity: "doi:10.2/y".into(),
                category: Category::Citations,
                outcome: Outcome::Values {
                    metrics: vec![dimensions_metric("doi:10.2/y")],
                    metadata: None,
                },
            },
        ]);
        snap.identities = vec!["doi:10.1/x".into(), "doi:10.2/y".into()];

        let out = render_terminal(&snap);
        assert_eq!(
            out.matches(notice.as_str()).count(),
            1,
            "a notice shared by two Identities must appear once per run, not once per Identity"
        );
    }

    #[test]
    fn long_not_applicable_and_failed_messages_reach_provider_notes_in_both_renderers() {
        // The original defect: a NotApplicable/Failed message over
        // INLINE_DETAIL_LIMIT had no promotion path at all and vanished
        // silently from both renderers. This is the regression test the
        // fix's brief calls out as the criterion that matters most.
        let na_note = "x".repeat(141);
        let failed_error = "y".repeat(155);
        let mut snap = snapshot_with(vec![
            FetchResult {
                provider: "dimensions".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Citations,
                outcome: Outcome::NotApplicable {
                    note: na_note.clone(),
                },
            },
            FetchResult {
                provider: "altmetric".into(),
                identity: "doi:10.2/y".into(),
                category: Category::Attention,
                outcome: Outcome::Failed {
                    error: failed_error.clone(),
                },
            },
        ]);
        snap.identities = vec!["doi:10.1/x".into(), "doi:10.2/y".into()];

        let terminal = render_terminal(&snap);
        assert!(
            terminal.contains(&na_note),
            "long NotApplicable note missing from terminal output"
        );
        assert!(
            terminal.contains(&failed_error),
            "long Failed error missing from terminal output"
        );
        assert!(terminal.contains("── Provider Notes ──"));

        let markdown = render_markdown(&snap);
        assert!(
            markdown.contains(&na_note),
            "long NotApplicable note missing from markdown output"
        );
        assert!(
            markdown.contains(&failed_error),
            "long Failed error missing from markdown output"
        );
        assert!(markdown.contains("## Provider Notes"));
    }

    #[test]
    fn short_not_applicable_note_still_renders_inline_control() {
        let mut snap = snapshot_with(vec![FetchResult {
            provider: "openalex".into(),
            identity: "doi:10.2/y".into(),
            category: Category::Citations,
            outcome: Outcome::NotApplicable {
                note: "not found in OpenAlex".into(),
            },
        }]);
        snap.identities = vec!["doi:10.2/y".into()];

        let terminal = render_terminal(&snap);
        let row = terminal.lines().find(|l| l.contains("openalex")).unwrap();
        assert!(row.contains("not found in OpenAlex"));
        assert!(!terminal.contains("── Provider Notes ──"));
    }

    #[test]
    fn long_metric_note_still_lands_in_notices_not_provider_notes() {
        let long_note = "x".repeat(120);
        let snap = snapshot_with(vec![FetchResult {
            provider: "dimensions".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![Metric {
                    note: Some(long_note.clone()),
                    provider: "dimensions".into(),
                    ..metric("citations", MetricValue::Count(5))
                }],
                metadata: None,
            },
        }]);
        let out = render_terminal(&snap);
        assert!(out.contains("── Notices ──"));
        assert!(!out.contains("── Provider Notes ──"));
        let notices_and_after = out.split("── Notices ──").nth(1).unwrap();
        assert!(!notices_and_after.contains("Provider Notes"));
    }

    #[test]
    fn one_message_shared_by_several_identities_on_one_provider_and_kind_is_one_entry() {
        let note = "n".repeat(90);
        let mut snap = snapshot_with(vec![
            FetchResult {
                provider: "altmetric".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Attention,
                outcome: Outcome::NotApplicable { note: note.clone() },
            },
            FetchResult {
                provider: "altmetric".into(),
                identity: "doi:10.2/y".into(),
                category: Category::Attention,
                outcome: Outcome::NotApplicable { note: note.clone() },
            },
        ]);
        snap.identities = vec!["doi:10.1/x".into(), "doi:10.2/y".into()];

        let out = render_terminal(&snap);
        assert_eq!(
            out.matches(note.as_str()).count(),
            1,
            "one message shared by two Identities on the same Provider+kind must appear once"
        );
        assert!(out.contains("doi:10.1/x"));
        assert!(out.contains("doi:10.2/y"));
    }

    #[test]
    fn identical_text_from_two_providers_produces_two_entries() {
        let note = "n".repeat(90);
        let mut snap = snapshot_with(vec![
            FetchResult {
                provider: "altmetric".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Attention,
                outcome: Outcome::NotApplicable { note: note.clone() },
            },
            FetchResult {
                provider: "dimensions".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Citations,
                outcome: Outcome::NotApplicable { note: note.clone() },
            },
        ]);
        snap.identities = vec!["doi:10.1/x".into()];

        let out = render_terminal(&snap);
        assert_eq!(
            out.matches(note.as_str()).count(),
            2,
            "byte-identical text from two Providers must not merge"
        );
        assert!(out.contains("altmetric"));
        assert!(out.contains("dimensions"));
    }

    #[test]
    fn not_applicable_and_failed_with_identical_text_produce_two_entries() {
        let text = "z".repeat(90);
        let mut snap = snapshot_with(vec![
            FetchResult {
                provider: "altmetric".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Attention,
                outcome: Outcome::NotApplicable { note: text.clone() },
            },
            FetchResult {
                provider: "altmetric".into(),
                identity: "doi:10.2/y".into(),
                category: Category::Attention,
                outcome: Outcome::Failed {
                    error: text.clone(),
                },
            },
        ]);
        snap.identities = vec!["doi:10.1/x".into(), "doi:10.2/y".into()];

        let out = render_terminal(&snap);
        assert_eq!(
            out.matches(text.as_str()).count(),
            2,
            "a NotApplicable and a Failed with byte-identical text must not merge"
        );
    }

    #[test]
    fn provider_notes_rows_gain_no_marker_pointer_or_preview() {
        let note = "n".repeat(90);
        let snap = snapshot_with(vec![FetchResult {
            provider: "altmetric".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Attention,
            outcome: Outcome::NotApplicable { note: note.clone() },
        }]);
        let out = render_terminal(&snap);
        let row = out
            .lines()
            .find(|l| l.contains("altmetric") && l.contains("N/A"))
            .unwrap();
        // The row itself carries no truncated preview and no "see below"
        // pointer — the footer is the only place the text appears.
        assert!(!row.contains(&note[..20]));
        assert!(!row.to_lowercase().contains("see below"));
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
    fn a_single_providers_metrics_can_split_across_categories() {
        // OpenAlex's citations metrics and its open-access status share one
        // FetchResult but belong to different Categories in the Report.
        let snap = snapshot_with(vec![FetchResult {
            provider: "openalex".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![
                    metric("citations", MetricValue::Count(1421)),
                    Metric {
                        category: Category::Attention,
                        ..metric("open_access", MetricValue::Text("gold".into()))
                    },
                ],
                metadata: None,
            },
        }]);
        let out = render_terminal(&snap);
        assert!(out.contains("── Citations ──"));
        assert!(out.contains("── Attention ──"));
        let citations_block = out.split("── Attention ──").next().unwrap();
        assert!(citations_block.contains("citations"));
        assert!(!citations_block.contains("open_access"));
        let attention_block = out.split("── Attention ──").nth(1).unwrap();
        assert!(attention_block.contains("open_access"));
        assert!(attention_block.contains("gold"));
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
    fn a_key_gated_providers_no_key_reason_is_visible_in_the_rendered_report_not_just_the_outcome()
    {
        // Regression check for issue #15 AC3: "the Report explicitly marks
        // the rich breakdown 'not collected (no key)'" — the Provider's
        // Outcome carrying the right note isn't enough on its own; it must
        // actually reach rendered output under the Attention section. Uses
        // the Provider's own constant (not a retyped literal) so this can't
        // silently drift out of sync with what `altmetric.rs` actually says.
        use crate::providers::altmetric;
        let snap = snapshot_with(vec![FetchResult {
            provider: "altmetric".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Attention,
            outcome: Outcome::NotApplicable {
                note: altmetric::NO_KEY_NOTE.into(),
            },
        }]);
        let terminal = render_terminal(&snap);
        assert!(terminal.contains("── Attention ──"));
        assert!(terminal.contains("altmetric"));
        assert!(terminal.contains("N/A"));
        assert!(terminal.contains(altmetric::NO_KEY_NOTE));

        let markdown = render_markdown(&snap);
        assert!(markdown.contains("### Attention"));
        assert!(markdown.contains(altmetric::NO_KEY_NOTE));
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

    /// GitHub's `release_downloads` Metric displays under Code, not Downloads
    /// (see CONTEXT.md's Category glossary) but still counts toward the
    /// Downloads Rollup (`rollup::counts_as_download`).
    fn release_downloads_result(identity: &str, value: u64) -> FetchResult {
        FetchResult {
            provider: "github".into(),
            identity: identity.into(),
            category: Category::Code,
            outcome: Outcome::Values {
                metrics: vec![Metric {
                    name: "release_downloads".into(),
                    category: Category::Code,
                    value: MetricValue::Count(value),
                    window: Window::Cumulative,
                    provider: "github".into(),
                    identity: identity.into(),
                    as_of: OffsetDateTime::UNIX_EPOCH,
                    source: "https://api.github.com/repos/owner/name/releases".into(),
                    note: None,
                }],
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

    #[test]
    fn github_release_downloads_row_stays_under_code_but_still_joins_the_rollup() {
        let mut snap = snapshot_with(vec![
            downloads_result("crates.io", "crates:boast", 100, Window::Cumulative),
            release_downloads_result("github:owner/name", 50),
        ]);
        snap.identities = vec!["crates:boast".into(), "github:owner/name".into()];

        let out = render_terminal(&snap);
        // The row itself renders in its own Identity's Code section, not Downloads.
        let github_block = out.split("━━ github:owner/name ━━").nth(1).unwrap();
        assert!(github_block.contains("── Code ──"));
        assert!(github_block.contains("release_downloads"));
        assert!(!github_block.contains("── Downloads ──"));
        // But it still contributes to the Rollup total.
        assert!(out.contains("Downloads Rollup"));
        assert!(out.contains("150")); // 100 (crates.io) + 50 (release_downloads)
    }

    // -- render_markdown --------------------------------------------------

    #[test]
    fn render_markdown_groups_values_under_a_category_table() {
        let snap = snapshot_with(vec![FetchResult {
            provider: "openalex".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![metric("citations", MetricValue::Count(1421))],
                metadata: None,
            },
        }]);
        let out = render_markdown(&snap);
        assert!(out.contains("### Citations"));
        assert!(out.contains("| Metric | Value | Window | Provider | Detail |"));
        assert!(out.contains("| citations | 1421 | all-time |"));
        assert!(!out.contains("partial snapshot"));
    }

    #[test]
    fn render_markdown_links_the_provider_to_its_source_url() {
        let snap = snapshot_with(vec![FetchResult {
            provider: "openalex".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![metric("citations", MetricValue::Count(1421))],
                metadata: None,
            },
        }]);
        let out = render_markdown(&snap);
        assert!(out.contains("[openalex](https://api.openalex.org/works/doi:10.1/x)"));
    }

    #[test]
    fn render_markdown_na_and_failed_rows_have_no_source_to_link() {
        let mut snap = snapshot_with(vec![FetchResult {
            provider: "openalex".into(),
            identity: "doi:10.2/y".into(),
            category: Category::Citations,
            outcome: Outcome::NotApplicable {
                note: "not found in OpenAlex".into(),
            },
        }]);
        snap.identities = vec!["doi:10.2/y".into()];
        let out = render_markdown(&snap);
        assert!(!out.contains("]("));
    }

    #[test]
    fn render_markdown_shows_na_and_failed_rows_never_zero() {
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
        let out = render_markdown(&snap);
        assert!(out.contains("N/A"));
        assert!(out.contains("FAILED"));
        assert!(!out.contains("| 0 |"));
        assert!(out.contains("partial snapshot"));
    }

    #[test]
    fn render_markdown_includes_rollup_and_deduplicated_notices() {
        let notice = "x".repeat(120);
        let mut snap = snapshot_with(vec![
            downloads_result("crates.io", "crates:boast", 100, Window::Cumulative),
            downloads_result("bioconda", "conda:bioconda/boast", 50, Window::Cumulative),
            FetchResult {
                provider: "dimensions".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Citations,
                outcome: Outcome::Values {
                    metrics: vec![Metric {
                        note: Some(notice.clone()),
                        provider: "dimensions".into(),
                        ..metric("citations", MetricValue::Count(5))
                    }],
                    metadata: None,
                },
            },
        ]);
        snap.identities = vec![
            "crates:boast".into(),
            "conda:bioconda/boast".into(),
            "doi:10.1/x".into(),
        ];
        let out = render_markdown(&snap);
        assert!(out.contains("## Downloads Rollup"));
        assert!(out.contains("150"));
        assert!(out.contains("## Notices"));
        assert_eq!(out.matches(notice.as_str()).count(), 1);
    }

    #[test]
    fn render_markdown_escapes_pipe_and_newline_in_a_table_cell() {
        let snap = snapshot_with(vec![FetchResult {
            provider: "example".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![Metric {
                    note: Some("a | pipe\nand a newline".into()),
                    provider: "example".into(),
                    ..metric("citations", MetricValue::Count(1))
                }],
                metadata: None,
            },
        }]);
        let out = render_markdown(&snap);
        assert!(out.contains("a \\| pipe and a newline"));
        // The stray pipe must not have split the table into an extra column.
        assert!(!out.contains("| pipe\nand"));
    }

    // -- render_prose -------------------------------------------------------

    #[test]
    fn render_prose_picks_the_highest_citation_count_and_attributes_it() {
        let snap = snapshot_with(vec![
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
                provider: "crossref".into(),
                identity: "doi:10.1/x".into(),
                category: Category::Citations,
                outcome: Outcome::Values {
                    metrics: vec![Metric {
                        provider: "crossref".into(),
                        ..metric("citations", MetricValue::Count(900))
                    }],
                    metadata: None,
                },
            },
        ]);
        let out = render_prose(&snap);
        assert!(out.contains("1421"));
        assert!(out.contains("openalex"));
        assert!(!out.contains("900"));
    }

    #[test]
    fn render_prose_includes_stars_and_a_downloads_rollup() {
        let mut snap = snapshot_with(vec![
            FetchResult {
                provider: "github".into(),
                identity: "github:o/n".into(),
                category: Category::Code,
                outcome: Outcome::Values {
                    metrics: vec![Metric {
                        category: Category::Code,
                        provider: "github".into(),
                        identity: "github:o/n".into(),
                        ..metric("stars", MetricValue::Count(342))
                    }],
                    metadata: None,
                },
            },
            downloads_result("crates.io", "crates:boast", 100, Window::Cumulative),
            downloads_result("bioconda", "conda:bioconda/boast", 50, Window::Cumulative),
        ]);
        snap.identities = vec![
            "github:o/n".into(),
            "crates:boast".into(),
            "conda:bioconda/boast".into(),
        ];
        let out = render_prose(&snap);
        assert!(out.contains("342"));
        assert!(out.contains("150"));
        assert!(out.contains("2 channels"));
        assert!(out.ends_with('.'));
    }

    #[test]
    fn render_prose_falls_back_to_a_single_channel_when_theres_no_rollup() {
        let snap = snapshot_with(vec![downloads_result(
            "crates.io",
            "crates:boast",
            100,
            Window::Cumulative,
        )]);
        let out = render_prose(&snap);
        assert!(out.contains("100"));
        assert!(out.contains("crates.io"));
    }

    #[test]
    fn render_prose_says_no_headline_metrics_for_an_empty_snapshot() {
        let out = render_prose(&snapshot_with(vec![]));
        assert!(out.contains("no headline metrics"));
        assert!(!out.contains(" 0 "));
    }

    #[test]
    fn render_prose_flags_a_partial_snapshot_without_faking_a_number() {
        let snap = snapshot_with(vec![
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
                outcome: Outcome::Failed {
                    error: "rate limited (429)".into(),
                },
            },
        ]);
        let out = render_prose(&snap);
        assert!(out.contains("1421"));
        assert!(out.to_lowercase().contains("some metrics failed"));
    }

    #[test]
    fn render_prose_carries_a_providers_notice_when_the_headline_metric_needs_one() {
        // ADR-0005: a keyed/terms-gated number must never be quoted without
        // its terms, even in the one-sentence prose format.
        let notice = "This data is subject to terms; see https://example.com/terms.".repeat(2);
        let snap = snapshot_with(vec![FetchResult {
            provider: "dimensions".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![Metric {
                    note: Some(notice.clone()),
                    provider: "dimensions".into(),
                    ..metric("citations", MetricValue::Count(1421))
                }],
                metadata: None,
            },
        }]);
        let out = render_prose(&snap);
        assert!(out.contains("1421"));
        assert!(out.contains(&notice));
    }

    #[test]
    fn render_prose_omits_a_short_interpretive_note_never_treats_it_as_a_notice() {
        // A short gloss (under the notice threshold) is not a Provider
        // licence/terms notice and must not be tacked onto the sentence.
        let snap = snapshot_with(vec![FetchResult {
            provider: "openalex".into(),
            identity: "doi:10.1/x".into(),
            category: Category::Citations,
            outcome: Outcome::Values {
                metrics: vec![Metric {
                    note: Some("short gloss".into()),
                    ..metric("citations", MetricValue::Count(1421))
                }],
                metadata: None,
            },
        }]);
        let out = render_prose(&snap);
        assert!(!out.contains("short gloss"));
    }
}
