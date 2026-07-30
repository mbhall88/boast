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

## The idea in one command

```
boast about 10.1234/journal.xyz          # a bare paper — one-liner
boast about --repo owner/tool \          # a full Project
            --package conda:bioconda/tool \
            10.1234/journal.xyz
boast render snapshots/2026-07-16.json --format markdown
boast diff  snapshots/2026-01.json snapshots/2026-07.json
```

## Installation

### Homebrew (macOS/Linux)

```
brew install mbhall88/tap/boast
```

### Shell script (macOS/Linux)

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mbhall88/boast/releases/latest/download/boast-installer.sh | sh
```

### PowerShell (Windows)

```
powershell -ExecutionPolicy Bypass -c "irm https://github.com/mbhall88/boast/releases/latest/download/boast-installer.ps1 | iex"
```

### Docker

```
docker run --rm ghcr.io/mbhall88/boast:latest about 10.1234/journal.xyz
```

### cargo

```
cargo install boast --locked
```

### From source

```
git clone https://github.com/mbhall88/boast
cd boast
cargo install --path . --locked
```

Prebuilt binaries (Linux x86_64/aarch64/armv7 — all statically linked, musl — and macOS
x86_64/aarch64, Windows x86_64) are attached to every [GitHub
Release](https://github.com/mbhall88/boast/releases).

## Design docs

- [`CONTEXT.md`](./CONTEXT.md) — the domain glossary (Project, Provider, Metric, Snapshot, …)
- [`docs/adr/`](./docs/adr/) — architecture decision records
- [`docs/spec/0001-boast-v1.md`](./docs/spec/0001-boast-v1.md) — the v1 spec

## License

MIT © 2026 Michael Hall
