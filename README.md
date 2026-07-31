# boast

`boast about samtools/samtools`

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

## Install

```
cargo install boast --locked
```

Homebrew, a shell/PowerShell installer script, Docker, and prebuilt binaries are all
covered on the [docs site](https://mbhall88.github.io/boast/getting-started.html), along
with everything else: concepts, guides, the Providers and CLI reference, automating
snapshots in CI, and the design decisions behind how boast works.

## Try it

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            10.1371/journal.pbio.1002195
```

## License

MIT © 2026 Michael Hall
