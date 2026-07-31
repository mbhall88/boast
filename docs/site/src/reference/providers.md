<!--
GENERATED FILE — do not edit by hand.
Regenerate with docs/site/generate-reference.sh, run from the repo root.
-->

# Providers reference

Every Provider in boast's default registry, which Category it serves, whether it's
enabled by default, and what environment variable (if any) it needs a key in. This page
is generated from `boast providers` — the same command you can run yourself to check
what's obtainable before running `about`.

```
PROVIDER    CATEGORY   DEFAULT  KEY
github      Code       yes      optional: GITHUB_TOKEN (not set)
crates.io   Downloads  yes      none
anaconda    Downloads  yes      none
pypi        Downloads  yes      none
homebrew    Downloads  yes      none
openalex    Citations  yes      none
crossref    Citations  yes      none
dimensions  Citations  yes      none
europe_pmc  Citations  yes      none
wikipedia   Attention  yes      none
altmetric   Attention  yes      required: ALTMETRIC_KEY (not set)
```

An optional key raises a rate limit or unlocks extra Metrics but isn't required; a
required key means that Provider reports every Metric as not-applicable until it's set
(never as zero — see [ADR-0002](../design/0002-metric-honesty-model.md)).
