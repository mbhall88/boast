# boast

`boast` gathers impact and reach metrics for a piece of research software (and/or its associated paper) from across code hosts, package registries, and citation/attention databases, so a user can make evidence-backed statements about a tool's impact (e.g. in a grant proposal). (Name is free on crates.io, Bioconda, Homebrew, and PyPI as of 2026-07.)

## Language

<!-- ANCHOR: language -->

**Project**:
The central entity — a single piece of research work that may link to a code repository, one or more distribution packages, and one or more papers. A bare paper lookup (e.g. by DOI) is just a Project whose only linked identity is a paper.
_Avoid_: Tool (too narrow — a Project may be paper-only), Package, Repo

**Metric**:
A single measured quantity of reach for a Project. Every Metric carries: a value, the Provider it came from, the Identity it describes, an **as-of** timestamp (when it was fetched), and a **coverage window** (see Window). A raw number with no window and no as-of is not a Metric.
_Avoid_: Stat, statistic, number

**Outcome**:
The result of one Provider×Identity fetch, always exactly one of: **Value** (a real number), **NotApplicable** (the Identity legitimately has no presence on that channel — shown as N/A, never 0), or **Failed** (a transient error: rate limit, timeout, 5xx, missing key — the number exists but wasn't retrievable). Snapshots record the Outcome explicitly; NotApplicable and Failed are never coerced to 0.
_Avoid_: Status, state, error

**Window**:
The span of time a Metric's value covers. Either **cumulative** (all-time, e.g. crates.io total downloads, GitHub release `download_count`), **trailing** (a rolling period, e.g. Homebrew 365-day installs, PyPI last-month), or **periodic** (a named bucket, e.g. OpenAlex citations in year 2023). Two Metrics may only be summed if their Windows are compatible.
_Avoid_: Period, timeframe, range

**Manifest**:
An optional file listing one or more Projects (their Identities and cohort topics) for repeatable or batch runs. Never required: a single Project can be given inline via CLI flags, and a bare paper (DOI/PMID) needs neither. The tool can generate a Manifest from a run, so it is a save-file, not a hand-authored prerequisite. Holds no secrets.
_Avoid_: Config, spec, input file

**Snapshot**:
The primary durable artifact: a timestamped, machine-readable record of every Metric fetched in one run, each with full provenance (Provider, Identity, value, as-of, Window, source URL/response). Snapshots are append-only; a human-readable Report is rendered from a Snapshot, and Snapshots are diffed to show change over time.
_Avoid_: Run, result, output, cache

**Report**:
A human-readable rendering of one or more Snapshots, always derived from Snapshots and never fetching data itself. v1 renderers: a terminal table (default), Markdown (primary saved artifact), and a **prose snippet** (an auto-written grant-ready sentence). HTML (with over-time charts) and CSV come later.
_Avoid_: Output, summary, document

**Notice**:
A Provider's licence or terms text, recorded on the Metric it accompanies and shown once per Report in a de-duplicated footer, so attribution travels with the artifact rather than with the terminal session that produced it (ADR-0005). De-duplicated by exact text across every Provider and Identity, because the same boilerplate legitimately repeats on each one.
_Avoid_: Disclaimer, licence blurb, footnote, attribution

**Provider Note**:
The explanation carried by a NotApplicable or Failed Outcome — why a fetch legitimately yielded nothing ("no API key configured") or failed to complete ("rate limited after three retries"). Not a Notice: it describes one Provider's attempt on one Identity rather than the terms behind a number, so it appears in its own Report section keyed by Provider and Outcome kind, and never merges with a Notice or across Outcome kinds (ADR-0008).
_Avoid_: Error message, warning, notice, detail

**Cohort**:
The set of repositories a Project's repo is ranked within, defined by a GitHub **topic** (e.g. all repos tagged `rna-seq`, sorted by stars). The topic may be read from the repo's own declared topics or set explicitly in the manifest. A Cohort ranking is always reported with its topic named and a disclaimer that GitHub topics are inconsistently applied.
_Avoid_: Peers, competitors, similar tools, category

**Rollup**:
A derived Metric produced by combining compatible Metrics — e.g. a total-downloads figure summed across channels. A Rollup must name every Metric it includes and their shared Window; it never silently mixes incompatible Windows or channels.
_Avoid_: Total, sum, aggregate

**Category**:
The family a Metric belongs to, used to group the Report. Four in v1: **Code** (stars, forks, contributors, release downloads, cohort rank…), **Downloads** (per-channel package/install counts + Rollup), **Citations** (counts + field-normalized FWCI/percentile/FCR/RCR), and **Attention** (open-access status and Wikipedia mentions keyless by default; full news/blog/policy/patent/social breakdown via Altmetric when a key is present).
_Avoid_: Kind, group, type, section

**Provider**:
A source-specific component that, given an Identity, fetches zero or more Metrics from one external service (GitHub, Bioconda, OpenAlex, …). Providers are pluggable; the system ships a curated default set.
_Avoid_: Source, backend, connector, adapter

**Identity**:
One external handle a Project links to, of a known kind: a code repository, a distribution package (with its registry), or a paper (DOI / PubMed ID). A Provider consumes Identities of the kinds it understands. An Identity always names a *piece of work*, never a person: a researcher identifier (an ORCID iD) is not an Identity but an input that *expands into* a set of them (see ADR-0006).
_Avoid_: Handle, reference, link, target

<!-- ANCHOR_END: language -->
