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

── Attention ──
  mentions                            383  all-time  openalex
  mentions                            855  all-time  europe_pmc

── Notices ──
  #12 of 15328 repos tagged 'bioinformatics'; GitHub topics are inconsistently applied
  #2 of 434 repos tagged 'sequence-alignment'; GitHub topics are inconsistently applied
  #1 of 6 repos tagged 'spliced-alignment'; GitHub topics are inconsistently applied
  indexed full-text search estimate, not a formal citation or verified literal URL count; partial coverage; self-mentions are included; article/preprint versions may be counted separately
  indexed full-text search estimate, not a formal citation or verified literal URL count; partial coverage concentrated in life-sciences literature; self-mentions are included; journal article/preprint versions may be counted separately
```

A reproducible research impact aggregator. Point `boast` at a **Project** — a piece of
research software identified by any of a code repository, distribution packages, and/or a
paper — and it gathers reach **Metrics** across four **Categories** (Code, Downloads,
Citations, Attention) from a curated set of pluggable **Providers**, records them in a
durable, timestamped **Snapshot** with full provenance, and renders **Reports** for grant
writing (terminal, Markdown, and an automatically written prose sentence).

Built for statements backed by evidence about a tool's impact — the kind you make in a grant
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

## Try it

Give boast a repository, package, and paper DOI and every Category reports in. The repo
contributes Code metrics plus independent OpenAlex and Europe PMC scholarly-mention
estimates; the package contributes Downloads and a labelled Rollup; Provider Notes explain
anything that couldn't be collected:

```
boast about --repo lh3/minimap2 10.1093/bioinformatics/bty191 --package conda:bioconda/minimap2
```

```
boast 0.3.0 — as of 2026-08-04T06:45:44Z

━━ doi:10.1093/bioinformatics/bty191 ━━
"Minimap2: pairwise alignment for nucleotide sequences" — Heng Li, Bioinformatics, 2018
── Citations ──
  citations              17232  all-time                 openalex
  fwci                  326.81  all-time                 openalex  field-weighted citation impact; 1.0 = world average
  citation_percentile   100.00  all-time                 openalex  top 1% in its field, year, and type
  citations              15403  all-time                 crossref  times referenced, per Crossref
  citations              16329  all-time                 dimensions
  recent_citations        6207  last two calendar years  dimensions  resets each 1 January; not a rolling 24-month window
  fcr                  1768.19  all-time                 dimensions  Field Citation Ratio; 1.0 = world average for the field and year
  rcr                   310.53  all-time                 dimensions  Relative Citation Ratio; 1.0 = NIH-funded benchmark
  citations              11563  all-time                 europe_pmc  citation count from Europe PMC
── Attention ──
  open_access         bronze  all-time  openalex  OpenAlex open-access status; "closed" means no open-access copy found
  wikipedia_mentions       3  all-time  wikipedia
  altmetric              N/A

━━ github:lh3/minimap2 ━━
── Code ──
  stars                               2228  all-time  github
  forks                                471  all-time  github
  watchers                              81  all-time  github  users watching the repo (subscribers)
  repo_age_years                      9.04  all-time  github  since 2017-07-18
  contributors                          51  all-time  github
  release_downloads                 301223  all-time  github  summed across release assets
  cohort_rank (bioinformatics)          12  all-time  github
  cohort_rank (genomics)                 4  all-time  github  #4 of 4322 repos tagged 'genomics'; GitHub topics are inconsistently applied
  cohort_rank (sequence-alignment)       2  all-time  github
  cohort_rank (spliced-alignment)        1  all-time  github

── Attention ──
  mentions                            383  all-time  openalex
  mentions                            855  all-time  europe_pmc

━━ conda:bioconda/minimap2 ━━
── Downloads ──
  downloads  1426981  all-time  bioconda

═══ Downloads Rollup (derived — see channels above) ═══
  1728204 all-time = github:lh3/minimap2 (301223) + conda:bioconda/minimap2 (1426981)

── Notices ──
  This data has been sourced via the Dimensions Metrics API, use of which is subject to the terms at https://dimensions.ai/policies/terms/metrics/. Any use by an unregistered organization is not authorized. Please contact info@dimensions.ai for further information.
  English Wikipedia full-text search hits for this DOI; other-language Wikipedias are not counted
  #12 of 15328 repos tagged 'bioinformatics'; GitHub topics are inconsistently applied
  #2 of 434 repos tagged 'sequence-alignment'; GitHub topics are inconsistently applied
  #1 of 6 repos tagged 'spliced-alignment'; GitHub topics are inconsistently applied
  indexed full-text search estimate, not a formal citation or verified literal URL count; partial coverage; self-mentions are included; article/preprint versions may be counted separately
  indexed full-text search estimate, not a formal citation or verified literal URL count; partial coverage concentrated in life-sciences literature; self-mentions are included; journal article/preprint versions may be counted separately

── Provider Notes ──
  altmetric (N/A): Altmetric attention data not collected: no Details Page API key (ALTMETRIC_KEY). An institutional licence or Altmetric's SRAD program provides one. — doi:10.1093/bioinformatics/bty191
```

## License

MIT © 2026 Michael Hall
