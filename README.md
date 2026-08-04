# boast

```
boast about --repo lh3/minimap2
```

```
boast 0.3.0 — as of 2026-08-04T06:35:40Z

━━ github:lh3/minimap2 ━━
── Code ──
  stars                               2228  all-time  github
  forks                                471  all-time  github
  watchers                              81  all-time  github  users watching the repo (subscribers)
  repo_age_years                      9.04  all-time  github  since 2017-07-18
  contributors                          51  all-time  github
  release_downloads                 301222  all-time  github  summed across release assets
  cohort_rank (bioinformatics)          12  all-time  github
  cohort_rank (genomics)                 4  all-time  github  #4 of 4322 repos tagged 'genomics'; GitHub topics are inconsistently applied
  cohort_rank (sequence-alignment)       2  all-time  github
  cohort_rank (spliced-alignment)        1  all-time  github

── Notices ──
  #12 of 15328 repos tagged 'bioinformatics'; GitHub topics are inconsistently applied
  #2 of 434 repos tagged 'sequence-alignment'; GitHub topics are inconsistently applied
  #1 of 6 repos tagged 'spliced-alignment'; GitHub topics are inconsistently applied
```

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
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mbhall88/boast/releases/latest/download/boast-installer.sh | sh
```

Homebrew, a PowerShell installer script, Docker, cargo, and prebuilt binaries for every
platform are all covered on the
[docs site](https://mbhall88.github.io/boast/getting-started.html), along with everything
else: concepts, guides, the Providers and CLI reference, automating snapshots in CI, and the
design decisions behind how boast works.

## License

MIT © 2026 Michael Hall
