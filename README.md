# boast

`boast about samtools`

A reproducible research-impact aggregator. Point `boast` at a **Project** — a piece of
research software identified by any of a code repository, distribution packages, and/or a
paper — and it gathers reach **Metrics** across four **Categories** (Code, Downloads,
Citations, Attention) from a curated set of pluggable **Providers**, records them in a
durable, timestamped **Snapshot** with full provenance, and renders grant-ready **Reports**
(terminal, Markdown, and an auto-written prose sentence).

Built for evidence-backed statements about a tool's impact — the kind you make in a grant
proposal — with reproducibility and honesty as first principles: dated, attributable
Snapshots you can commit and re-render, metrics that are never silently coerced to zero, and
totals that never mix incompatible time windows.

> Status: **design phase.** No code yet — the design lives in `CONTEXT.md` (domain glossary),
> `docs/adr/` (architecture decisions), and `docs/spec/` (the v1 spec).

## The idea in one command

```
boast about 10.1234/journal.xyz          # a bare paper — one-liner
boast about --repo owner/tool \          # a full Project
            --package conda:bioconda/tool \
            --doi 10.1234/journal.xyz
boast render snapshots/2026-07-16.json --format markdown
boast diff  snapshots/2026-01.json snapshots/2026-07.json
```

## Design docs

- [`CONTEXT.md`](./CONTEXT.md) — the domain glossary (Project, Provider, Metric, Snapshot, …)
- [`docs/adr/`](./docs/adr/) — architecture decision records
- [`docs/spec/0001-boast-v1.md`](./docs/spec/0001-boast-v1.md) — the v1 spec

## License

MIT © 2026 Michael Hall
