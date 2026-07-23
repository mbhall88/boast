# Provider licence/terms notices surface in Reports, once per run — not in logs

## Status

accepted

## Context and decision

Some Providers' terms require visible attribution wherever their data is displayed — e.g. the Dimensions Metrics API terms ask for "an attribution [on] the page where the metrics are displayed" (added alongside the Dimensions Provider, #8). A `boast` Report is rendered from a Snapshot and can be rendered long after the original fetch: `render`/`diff` work purely offline on stored Snapshot JSON with no re-fetch (ADR-0001). A notice printed only to stderr during the original `about` run would not exist by the time someone later runs `render` on the saved Snapshot, hands the Snapshot file to a teammate, or pastes a rendered Report into a grant draft — arguably the paradigmatic "page where the metrics are displayed."

So a Provider's licence/terms notice is recorded on the Metric it accompanies (`Metric.note`), inside the Snapshot, so it survives serialization and offline re-rendering — and it is shown in the Report, not logged. To avoid repeating the same notice once per Identity (e.g. several DOIs all carrying the same Dimensions boilerplate), the terminal Report treats any note past a length threshold as a Provider-level notice rather than a per-row interpretive gloss, de-duplicates by exact text, and prints each distinct notice once in a footer section — so a run over many DOIs still shows the notice exactly once, not once per DOI.

## Considered options

- **Log to stderr instead of the Report.** Keeps the table clean and matches how these terms are usually satisfied in practice (a webpage embedding a Provider's JS badge, not a CLI). Rejected: a Snapshot is meant to be rendered again later without re-running the fetch (ADR-0001), and a log line from the original run wouldn't be there for that later render or for a teammate handed just the Snapshot file.
- **Print inline on every Metric/row that carries the notice.** Simplest, but produces a duplicated, table-breaking wall of text as soon as more than one Identity shares the Provider (a ~250-character notice on every DOI's `citations` row). Rejected in favour of a de-duplicated, once-per-run footer.

## Consequences

- `Metric.note` does double duty for v1 — a short interpretive gloss (e.g. "field-weighted citation impact; 1.0 = world average") or a Provider's legal notice — distinguished purely by length in the renderer. A future Provider needing the same treatment reuses this without a Snapshot schema change; if the distinction ever needs to be explicit, promoting it to a typed field is the escape hatch.
- Reports must carry a duplicate-free but *complete* set of the notices behind the numbers they display — the Report is the artifact people paste into a grant proposal, so attribution travels with it, not with the terminal session that produced it.
