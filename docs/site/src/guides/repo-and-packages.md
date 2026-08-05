# A tool with a repository and packages

The common case for research software: a GitHub repository, one or more package
registries, and (usually) a paper describing it. Giving boast all three means every
Category — Code, Downloads, Citations, Attention — has something to report on.

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            10.1093/gigascience/giab008
```

`--package` is repeatable — list every registry the tool is published on. samtools is
also on Homebrew, so:

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            --package homebrew:samtools \
            10.1093/gigascience/giab008
```

Output (Code, Downloads per channel, and a Downloads Rollup — Citations/Attention are
the same shape shown in [Getting started](../getting-started.md#your-first-report), so
only the sections that are new here are shown):

```
━━ github:samtools/samtools ━━
── Code ──
  stars                 1934  all-time  github
  forks                  613  all-time  github
  watchers                94  all-time  github  users watching the repo (subscribers)
  repo_age_years       14.40  all-time  github  since 2012-03-09
  contributors            108  all-time  github
  release_downloads  2156386  all-time  github  summed across release assets

━━ conda:bioconda/samtools ━━
── Downloads ──
  downloads  9032484  all-time  bioconda

━━ homebrew:samtools ━━
── Downloads ──
  downloads_30d    503  last 30 days   homebrew
  downloads_90d   1169  last 90 days   homebrew
  downloads_365d  5566  last 365 days  homebrew

═══ Downloads Rollup (derived — see channels above) ═══
  11188870 all-time = github:samtools/samtools (2156386) + conda:bioconda/samtools (9032484)
```

Homebrew's own Metrics don't join the Rollup — they're all trailing Windows (30/90/365
day), and a Rollup can only sum Metrics that share a compatible Window (see
[Concepts](../concepts.md), Rollup and Window); mixing a trailing count in with two
all-time counts would misrepresent the total, so it stays out.

Run `boast providers` to see the full registry of Providers, which Category each
serves, and which package registries they cover.

## Container images

Many research tools also ship as a container. Docker Hub images are addressed as
`docker:namespace/name` — official images live under `library`, so `ubuntu` is
`docker:library/ubuntu`:

```
boast about --package docker:biocontainers/samtools \
            --package conda:bioconda/samtools
```

```
━━ docker:biocontainers/samtools ━━
── Downloads ──
  downloads  596335  all-time  dockerhub

━━ conda:bioconda/samtools ━━
── Downloads ──
  downloads  9054107  all-time  bioconda

═══ Downloads Rollup (derived — see channels above) ═══
  9650442 all-time = docker:biocontainers/samtools (596335) + conda:bioconda/samtools (9054107)

── Notices ──
  Docker Hub pull counts record image fetches by machines, not installs by people: CI re-pulls and mirror warming inflate the figure, and it never resets
```

Unlike Homebrew, a Docker Hub pull count *is* cumulative, so it shares a Window with the
conda and crates.io counts and does join the Rollup. Read that total with the Notice in
mind: a pull is a much weaker signal than an install. Docker Hub counts every image fetch
by a machine, so CI re-runs and mirror warming land in the same figure, and the counter
never resets — `docker:library/ubuntu` sits near ten billion. This is why the Rollup
always names each channel and its own value: the total is only ever as meaningful as the
channels you can see underneath it.

GitHub's container registry (`ghcr.io`) has no equivalent Provider, because GHCR
publishes no pull statistics — neither the OCI registry API nor GitHub's Packages API
exposes a download count. An image hosted only there can't contribute to Downloads at
all.

## Save the identifiers for next time

Re-typing `--repo`/`--package`/the DOI on every run gets old fast. `--save` writes a
Manifest capturing exactly the identities (and `--topic`, if given) used in this run:

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            --save manifest.toml \
            10.1093/gigascience/giab008
```

From then on:

```
boast about manifest.toml
```

You can also build a Manifest up front, without fetching anything, via `boast init`
(same flags as `about`):

```
boast init --repo samtools/samtools --package conda:bioconda/samtools \
           --package homebrew:samtools -o manifest.toml 10.1093/gigascience/giab008
```

`manifest.toml` now contains, offline, with nothing fetched:

```toml
[[project]]
identities = [
    "doi:10.1093/gigascience/giab008",
    "github:samtools/samtools",
    "conda:bioconda/samtools",
    "homebrew:samtools",
]
```

## Ranking within a cohort

If the repo is tagged with a GitHub topic (or you want to force one), boast can report
where it ranks by stars among every repo sharing that topic:

```
boast about --repo samtools/samtools --topic bioinformatics 10.1093/gigascience/giab008
```

The Code section gains a `cohort_rank` row, and a matching disclaimer appears in
Notices:

```
── Code ──
  ...
  cohort_rank (bioinformatics)       16  all-time  github

── Notices ──
  #16 of 15275 repos tagged 'bioinformatics'; GitHub topics are inconsistently applied
```

Omit `--topic` and boast ranks within whatever topics the repo has actually declared on
GitHub — see the Cohort entry in [Concepts](../concepts.md) for the disclaimer this
ranking always carries.
