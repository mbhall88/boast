# A tool with a repo and packages

The common case for research software: a GitHub repository, one or more package
registries, and (usually) a paper describing it. Giving boast all three means every
Category — Code, Downloads, Citations, Attention — has something to report on.

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            10.1371/journal.pbio.1002195
```

`--package` is repeatable — list every registry the tool is published on:

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            --package crates:samtools \
            10.1371/journal.pbio.1002195
```

Run `boast providers` to see the full registry of Providers, which Category each
serves, and which package registries they cover.

## Save the identifiers for next time

Re-typing `--repo`/`--package`/the DOI on every run gets old fast. `--save` writes a
Manifest capturing exactly the identities (and `--topic`, if given) used in this run:

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            --save manifest.toml \
            10.1371/journal.pbio.1002195
```

From then on:

```
boast about manifest.toml
```

You can also build a Manifest up front, without fetching anything, via `boast init`
(same flags as `about`):

```
boast init --repo samtools/samtools --package conda:bioconda/samtools \
           -o manifest.toml 10.1371/journal.pbio.1002195
```

## Ranking within a Cohort

If the repo is tagged with a GitHub topic (or you want to force one), boast can report
where it ranks by stars among every repo sharing that topic:

```
boast about --repo samtools/samtools --topic bioinformatics 10.1371/journal.pbio.1002195
```

Omit `--topic` and boast ranks within whatever topics the repo has actually declared on
GitHub — see the Cohort entry in [Concepts](../concepts.md) for the disclaimer this
ranking always carries.
