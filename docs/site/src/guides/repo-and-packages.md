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

Many research tools also ship as a container, on two registries boast covers:

- **Docker Hub**, as `docker:namespace/name`. Official images live under `library`, so
  `ubuntu` is `docker:library/ubuntu`.
- **Quay.io**, as `quay:namespace/name`. This is where Bioconda's auto-built per-package
  containers live, so a bioconda recipe gets you `quay:biocontainers/<pkg>` for free.

If you package for Bioconda, reach for Quay. The `biocontainers/` organisation on Docker
Hub is an older, hand-curated set — its `samtools` image was last pushed in 2019 — while
`quay.io/biocontainers` is what the build system actually publishes to, and where the
traffic actually goes:

```
boast about --package quay:biocontainers/samtools \
            --package docker:biocontainers/samtools \
            --package conda:bioconda/samtools
```

Captured on a later day than the run above, so bioconda's count has moved on — which is
the point of Snapshots being dated:

```
━━ quay:biocontainers/samtools ━━
── Downloads ──
  pulls  1786502  last 92 days  quay

━━ docker:biocontainers/samtools ━━
── Downloads ──
  downloads  596337  all-time  dockerhub

━━ conda:bioconda/samtools ━━
── Downloads ──
  downloads  9055038  all-time  bioconda

═══ Downloads Rollup (derived — see channels above) ═══
  9651375 all-time = docker:biocontainers/samtools (596337) + conda:bioconda/samtools (9055038)

── Notices ──
  Quay.io pull counts record image fetches by machines, not installs by people, and CI re-pulls dominate for a biocontainer; Quay publishes only a rolling daily series, so this is not an all-time total
  Docker Hub pull counts record image fetches by machines, not installs by people: CI re-pulls and mirror warming inflate the figure, and it never resets
```

Note the scale: three months on Quay is triple the *lifetime* total of the stale Docker
Hub image.

The two registries land in the Rollup differently, and it's the Window that decides —
not the fact that both count container pulls:

- **Docker Hub publishes an all-time `pull_count`**, so it shares a cumulative Window
  with the conda and crates.io counts and joins the Rollup.
- **Quay publishes only a rolling daily series**, never a lifetime total, so its figure
  is trailing. `boast` reports the window it actually measured — `last 92 days` above,
  read off the length of the series Quay returned rather than assumed — and a trailing
  count can't be summed with all-time ones, so it stays out of the all-time Rollup.

To be precise about the rule: what's excluded is mixing *incompatible* Windows, not
trailing Windows as such. Metrics sharing an exactly-equal trailing Window do roll up
together — a Homebrew 30-day install count and a PyPI 30-day download count form their
own `last 30 days` Rollup, separate from the all-time one. Quay's ~92-day window simply
has nothing else to pair with today.

Read every container number with its Notice in mind: a pull is a much weaker signal than
an install. Both registries count image fetches by machines, so CI re-runs and mirror
warming land in the same figure, and Docker Hub's counter never resets —
`docker:library/ubuntu` sits near ten billion. This is why the Rollup always names each
channel and its own value: the total is only ever as meaningful as the channels you can
see underneath it.

One quirk worth knowing about Quay: it answers a lookup for an image you can't see with
"requires authentication" rather than "not found", and it does that identically whether
the image is private or simply doesn't exist. `boast` reports that as N/A with a note
saying so, not as a failed fetch — no amount of retrying will turn it into a number, so
it doesn't make your Snapshot partial.

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
