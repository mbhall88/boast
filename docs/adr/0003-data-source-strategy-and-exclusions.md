# Data source strategy and deliberate exclusions

## Status

accepted

## Context

Impact data comes from many services with different coverage, cost, terms, and stability. That set changed materially in late 2025 and early 2026. This ADR records *why* the default Provider set looks the way it does, including the deliberate exclusions, so nobody re-litigates them in six months.

## Decision

**Default set (all keyless unless noted), grouped by Category:**

- **Code:** GitHub (stars, forks, watchers, contributors, total release downloads, repo age, Cohort rank by topic).
- **Downloads:** Anaconda.org (any channel — bioconda, conda-forge, or otherwise), PyPI, GitHub release assets, crates.io, Homebrew — reported per-channel, with a labelled Rollup.
- **Citations:** OpenAlex (headline count + field-normalized **FWCI** + `citation_normalized_percentile`), Crossref (authoritative metadata), Europe PMC (life sciences cross-check), Dimensions badge API (count + recent count from the last two calendar years + FCR + RCR).
- **Attention:** open-access status (OpenAlex), Wikipedia mentions, and indexed scholarly repository mentions (OpenAlex + Europe PMC) as keyless baseline defaults; Altmetric as an opt-in richer Provider.

**Ranking.** A paper's "standing among similar work" is delivered by field-normalized metrics that already exist for free — OpenAlex percentile/FWCI and Dimensions FCR/RCR — rather than any manually constructed ranking. A repo's peer comparison is a **GitHub topic Cohort** (rank by stars among repos carrying a topic), chosen because it needs no manual setup and is reproducible; its dependence on inconsistent topic tagging is disclosed in every Report, not hidden.

## Deliberate exclusions (the less obvious part)

- **No Google Scholar.** No official API, robots.txt forbids automated access, and it blocks scrapers aggressively; reliable access needs a paid third-party proxy. A built-in scraper would be the one component that silently breaks and can get a user's IP blocked, which is fatal for a tool that must be reliable and reproducible. Excluded despite it being a metric users personally like.
- **Altmetric is key-gated, not default-free.** As of **10 November 2025** Altmetric's Details-Page API requires an API key for all users; the old free badge endpoint now 403s. Rich attention data requires a key. That is a property of Altmetric's service, not a limitation of `boast`. `ALTMETRIC_KEY` must specifically be a **Details Page API** key (the `/v1/fetch/doi/{doi}` lookup for one article that this Provider calls) — **Altmetric Explorer**, the institutional analytics dashboard product, is a different API with its own key/secret pair that will not authenticate here (confirmed directly against the live API: an Explorer credential gets a clear "API key … not recognized"). A Details Page API key comes from either an institutional licence (ask your library — many universities that pay for Explorer *don't* automatically also license the Details Page API for individual researchers) or Altmetric's SRAD (Scientometric Research Access to Data) program, a free application-based route for non-commercial research. Neither is instant, so don't expect a same-day key. The exact field-name shape this Provider parses (`score`, `cited_by_msm_count`, `cited_by_feeds_count`, `cited_by_policies_count`, `cited_by_patents_count`, `cited_by_tweeters_count`, `readers.mendeley`) was cross-checked against public documentation and a real third-party client's source, but has never been confirmed against a live successful response — nobody involved in building this had Details Page API access. If you get real access, running the Provider once and comparing output against what you see on the paper's own Altmetric page is the one remaining gap; a response this Provider can't recognise at all comes back `Failed`, not a silent zero, specifically so a schema mismatch can't masquerade as "no attention."
- **No Crossref Event Data.** The main free, keyless attention feed (Wikipedia/news/blog/social mentions of a DOI) was **sunset on 23 April 2026**; its replacement only exposes dataset-citation relationships. This is why keyless attention is "lite" (OA status + Wikipedia) rather than a full attention donut.
- **GitHub "used by / dependents" is opt-in, not default.** Arguably the best reach signal for a library, but there is no API — the count only exists on the scraped `/network/dependents` HTML page and breaks when GitHub changes markup. Offered as an optional metric with an explicit caveat rather than a default.
- **Issue/PR counts omitted** from the default Code set — they read as activity/maintenance, not reach.

## Consequences

- Providers are pluggable behind a common trait so paid/optional sources (Semantic Scholar, Altmetric, GitLab, Docker/Quay, CRAN, Bioconductor, npm) and future replacements slot in without touching the core.
- Reports must carry source attribution and the topic Cohort disclaimer, because the credibility of a claim depends on which Provider produced it.
