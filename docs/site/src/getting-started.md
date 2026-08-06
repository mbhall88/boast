# Getting started

## Install

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
x86_64/aarch64, Windows x86_64) are attached to every
[GitHub Release](https://github.com/mbhall88/boast/releases).

## Your first report

Point `boast about` at anything with a DOI, and it prints a Report straight to your
terminal — no config, no Manifest, no account:

```
boast about 10.1371/journal.pbio.1002195
```

Output:

```
boast 0.1.1 — as of 2026-08-03T05:43:11Z

━━ doi:10.1371/journal.pbio.1002195 ━━
"Big Data: Astronomical or Genomical?" — Zachary D. Stephens et al., PLOS Biology, 2015
── Citations ──
  citations              1426  all-time                 openalex
  fwci                  59.95  all-time                 openalex  field-weighted citation impact; 1.0 = world average
  citation_percentile   99.96  all-time                 openalex  top 1% in its field, year, and type
  citations              1166  all-time                 crossref  times referenced, per Crossref
  citations              1289  all-time                 dimensions
  recent_citations        165  last two calendar years  dimensions  resets each 1 January; not a rolling 24-month window
  fcr                  116.83  all-time                 dimensions  Field Citation Ratio; 1.0 = world average for the field and year
  rcr                   15.96  all-time                 dimensions  Relative Citation Ratio; 1.0 = NIH-funded benchmark
  citations               581  all-time                 europe_pmc  citation count from Europe PMC
── Attention ──
  open_access         gold  all-time  openalex  OpenAlex open-access status; "closed" means no open-access copy found
  wikipedia_mentions     0  all-time  wikipedia
  altmetric            N/A  Altmetric attention data not collected: no Details Page API key (ALTMETRIC_KEY)

── Notices ──
  This data has been sourced via the Dimensions Metrics API, use of which is subject to the terms at https://dimensions.ai/policies/terms/metrics/. Any use by an unregistered organization is not authorized. Please contact info@dimensions.ai for further information.
  English Wikipedia full-text search hits for this DOI; other-language Wikipedias are not counted
```

## Measure a repository

A repository can be measured without a DOI or package. Pass either `owner/name` or the
full GitHub URL; both forms resolve to the same repository identity:

```
boast about --repo mbhall88/rasusa
boast about --repo https://github.com/mbhall88/rasusa
```

Alongside the repository's Code metrics, the report includes independent indexed-search
estimates from OpenAlex and Europe PMC under Attention:

```
━━ github:mbhall88/rasusa ━━
── Attention ──
  mentions  16  all-time  openalex
  mentions  12  all-time  europe_pmc
```

These are not formal citation counts or verified literal URL occurrences. They are
coverage-limited full-text search estimates; self-mentions count, and a preprint and its
published version can count separately. The two providers are shown side by side and are
never summed. Europe PMC is concentrated in life-sciences literature.

If the piece of software also has a code repository and/or is published on a package
registry, tell boast about those too. The repository adds Code and Attention, a package
adds Downloads, and a DOI adds paper Citations:

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            10.1093/gigascience/giab008
```

Now the Report gains Code and Attention sections for the repo, a Downloads section for the
package, and a Downloads Rollup combining the two channels that share a compatible Window
— on top of everything the bare DOI already produced above:

```
━━ github:samtools/samtools ━━
── Code ──
  stars                 1934  all-time  github
  forks                  613  all-time  github
  watchers                94  all-time  github  users watching the repo (subscribers)
  repo_age_years       14.40  all-time  github  since 2012-03-09
  contributors            108  all-time  github
  release_downloads  2156386  all-time  github  summed across release assets

── Attention ──
  mentions                 407  all-time  openalex
  mentions                1104  all-time  europe_pmc

━━ conda:bioconda/samtools ━━
── Downloads ──
  downloads  9032484  all-time  bioconda

═══ Downloads Rollup (derived — see channels above) ═══
  11188870 all-time = github:samtools/samtools (2156386) + conda:bioconda/samtools (9032484)
```

Every run above already wrote a Snapshot — `boast about` saves one to `snapshots/` by
default (pass `--no-save` to skip that and only print). That first Snapshot is the start
of a history you can [diff against later](./guides/ci-snapshots.md):

```
boast render snapshots/<the-file-it-just-wrote>.json --format markdown
```

Output (the raw Markdown source — this is what you'd commit or paste into a report):

```
# boast Report

_boast 0.1.1 — as of 2026-08-03T05:43:44Z_

## doi:10.1371/journal.pbio.1002195

"Big Data: Astronomical or Genomical?" — Zachary D. Stephens et al., PLOS Biology, 2015

### Citations

| Metric | Value | Window | Provider | Detail |
| --- | --- | --- | --- | --- |
| citations | 1426 | all-time | [openalex](https://api.openalex.org/works/doi:10.1371/journal.pbio.1002195) |  |
| fwci | 59.95 | all-time | [openalex](https://api.openalex.org/works/doi:10.1371/journal.pbio.1002195) | field-weighted citation impact; 1.0 = world average |
...
```

From here, see [Concepts](./concepts.md) for the vocabulary, or jump straight to a
[Guide](./guides/index.md) that matches your situation.
