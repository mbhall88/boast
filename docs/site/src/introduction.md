# boast

`boast` gathers impact and reach metrics for a piece of research software (and/or its
associated paper) from across code hosts, package registries, and citation/attention
databases, so you can make evidence-backed statements about a tool's impact — the kind
you make in a grant proposal.

Point it at a **Project** — identified by any of a code repository, distribution
packages, and/or a paper — and it gathers reach **Metrics** across four **Categories**
(Code, Downloads, Citations, Attention) from a curated set of pluggable **Providers**,
records them in a durable, timestamped **Snapshot** with full provenance, and renders
grant-ready **Reports** (terminal, Markdown, and an auto-written prose sentence).

Reproducibility and honesty are first principles: dated, attributable Snapshots you can
commit and re-render, metrics that are never silently coerced to zero, and totals that
never mix incompatible time windows.

```
boast about samtools/samtools
```

- **[Getting started](./getting-started.md)** — install boast and run your first Report.
- **[Concepts](./concepts.md)** — the vocabulary this site and the CLI both use.
- **[Guides](./guides/index.md)** — worked examples for common setups.
- **[Providers reference](./reference/providers.md)** and
  **[CLI reference](./reference/cli.md)** — what boast can fetch, and every flag it has.
- **[Design decisions](./design/index.md)** — the trade-offs behind how boast works.
