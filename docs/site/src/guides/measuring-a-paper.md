# Measuring a paper

The smallest possible Project: just a paper, identified by DOI or PubMed ID, with no
code repository or package attached. This is the right shape when you're reporting on a
publication itself rather than a specific tool.

```
boast about 10.1371/journal.pbio.1002195
```

Output — Citations and Attention Metrics only. Code and Downloads have nothing to attach
to without a repository or package, so they're omitted entirely rather than shown as
zero (see [ADR-0002](../design/0002-metric-honesty-model.md); the full transcript,
including what a Code/Downloads section looks like once a repo and package are added, is
in [Getting started](../getting-started.md#your-first-report)):

```
boast 0.1.1 — as of 2026-08-03T05:43:11Z

━━ doi:10.1371/journal.pbio.1002195 ━━
"Big Data: Astronomical or Genomical?" — Zachary D. Stephens et al., PLOS Biology, 2015
── Citations ──
  citations              1426  all-time                 openalex
  ...
```

A bare PubMed ID works the same way:

```
boast about pmid:26151137
```

Same paper, different identifier — but not quite the same output. A few Providers
(Crossref, Dimensions) only look papers up by DOI, so a PMID-only Project sees fewer of
them than a DOI one does:

```
boast 0.1.1 — as of 2026-08-03T05:46:47Z

━━ pmid:26151137 ━━
── Citations ──
  citations             1426  all-time  openalex
  fwci                 59.95  all-time  openalex  field-weighted citation impact; 1.0 = world average
  citation_percentile  99.96  all-time  openalex  top 1% in its field, year, and type
  citations              581  all-time  europe_pmc  citation count from Europe PMC
```

Each run already writes a Snapshot to `snapshots/` (pass `--no-save` to only print). See
[Diffing the history](./ci-snapshots.md#diffing-the-history-once-you-have-it) to compare
one Snapshot against a later run.
