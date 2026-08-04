# Architecture centred on Snapshots

## Status

accepted

## Context and decision

`boast` produces numbers that people quote in grant proposals ("as of March 2026, 2,400 citations"). Those claims must stay defensible after the underlying metrics move on. We therefore make a durable, append-only **Snapshot** — not a printed report — the primary artifact of a run.

`boast about` fetches live and writes a timestamped, machine-readable Snapshot recording every Metric with full provenance (Provider, Identity, value, as-of timestamp, Window, source). A **Report** (terminal table, Markdown, prose, HTML, CSV) is *always* rendered from a Snapshot and never fetches data itself. `render` and `diff` operate purely offline on stored Snapshots; growth-over-time comes from diffing Snapshots, so no database is needed — just committable files.

## Considered options

- **Stateless print-and-forget.** Simplest, but nothing is reproducible after the fact: re-running next month silently yields different numbers with no record of what was originally quoted. Rejected — reproducibility is the whole point.
- **A database of metrics over time.** More power (queries, dashboards) but heavy operational surface for a personal, shareable CLI. Rejected for v1 in favour of append-only JSON files that live next to the grant draft in git.

## Consequences

- Snapshots are internally consistent as-of a single moment. There is deliberately **no** "refresh one failed Provider into an existing Snapshot" — a re-run produces a new Snapshot rather than a patchwork of fetch times.
- `about` is always-live (no cross-run cache that could serve stale numbers); `render`/`diff` are always-offline. The two verbs mean "get the truth now" vs "work with truths already captured."
- The Snapshot is the compatibility surface: it carries a versioned schema so old Snapshots remain renderable as the tool evolves.
