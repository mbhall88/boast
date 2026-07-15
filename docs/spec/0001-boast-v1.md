# boast v1 — a reproducible research-impact aggregator

## Problem Statement

When I write grant proposals, progress reports, or promotional material for a piece of research software (I work in bioinformatics, but this generalises), I regularly need to make evidence-backed statements about the tool's reach and impact: how many downloads it has had, how many times its paper has been cited, how many GitHub stars it has, and how it stands relative to similar work. Today I assemble these numbers by hand from a scatter of sources — GitHub, Bioconda, PyPI, OpenAlex, Dimensions, and more. That is tedious, easy to get wrong, impossible to reproduce, and hard to defend after the fact ("what exactly were the numbers on the day I quoted them?"). I want one robust, reproducible way to gather these metrics and turn them into a dated, quotable summary — and I want it to be good enough that other people can use it too.

## Solution

`boast` — a single-binary Rust CLI plus a library crate. You point it at a **Project** (a piece of research work identified by any of: a code repository, one or more distribution packages, and one or more papers) given either inline on the command line or via a small **Manifest** file, and it fetches reach **Metrics** across four **Categories** — Code, Downloads, Citations, Attention — from a curated set of pluggable **Providers**. It records everything in a durable, timestamped **Snapshot** with full provenance, then renders grant-ready **Reports** (a terminal table, Markdown, and an auto-written prose sentence). A bare paper (`boast 10.1234/journal.xyz`) is a one-liner; a multi-identity tool is either flags or a saved Manifest. Snapshots are append-only and committable, so the figures you quote are reproducible months later, and growth over time falls out of diffing two Snapshots.

The name reads as a sentence: `boast about samtools`.

## User Stories

1. As a tool author, I want to look up my tool's impact metrics with a single command, so that I can stop assembling them by hand from many websites.
2. As a grant writer, I want a dated summary of a tool's reach, so that I can make defensible claims in a proposal.
3. As a tool author, I want to describe my Project by its repository, so that I can get code-host metrics for it.
4. As a tool author, I want to describe my Project by one or more distribution packages (e.g. `bioconda:samtools`, `pypi:pysam`, `crates:boast`), so that I can get download counts per channel.
5. As a researcher, I want to describe my Project by a paper's DOI, so that I can get its citation metrics.
6. As a researcher, I want to give a PubMed ID instead of a DOI (`pmid:31234567`), so that I can use whichever identifier I have to hand.
7. As a user, I want a bare DOI/PMID/repo on the command line to "just work" without a subcommand, so that the common one-off case is frictionless.
8. As a user, I want to combine a repo, packages, and a paper into one Project, so that I get a single aggregated impact story rather than three separate lookups.
9. As a user with several tools, I want to list many Projects in one Manifest file, so that I can gather metrics for all of them in a batch.
10. As a user, I want the tool to generate a Manifest for me from a run, so that I never have to hand-author a config file to get reproducibility.
11. As a repeat user, I want to re-run from a saved Manifest, so that I can track the same Project month over month.
12. As a tool author, I want GitHub stars, forks, watchers, and contributor count, so that I can quote community-adoption signals.
13. As a tool author, I want the total download count of my GitHub release assets, so that I can count users who install prebuilt binaries.
14. As a tool author, I want my repository's age / first-release date, so that I can contextualise the other numbers ("X stars in Y years").
15. As a tool author, I want Bioconda/anaconda download counts, so that I can report usage through the dominant bioinformatics channel.
16. As a tool author, I want PyPI download counts, so that I can report Python-package usage.
17. As a tool author, I want crates.io download counts, so that I can report Rust-crate usage.
18. As a tool author, I want Homebrew install counts, so that I can report usage by Homebrew users.
19. As a user, I want each download channel reported separately, so that I never conflate a Conda download with a Docker pull or a git clone.
20. As a user, I want an optional summed total across channels that names exactly which channels it includes, so that I can decide for myself whether to quote the sum.
21. As a user, I want the tool to refuse to sum metrics whose coverage windows differ (e.g. an all-time crates count with a 365-day Homebrew count), so that my totals are honest.
22. As a researcher, I want a headline citation count for my paper, so that I can state how often it has been cited.
23. As a researcher, I want a field-normalized citation metric (FWCI and a citation percentile), so that I can say my paper is in the top X% of its field.
24. As a researcher, I want authoritative bibliographic metadata (title, authors, journal), so that the Report correctly identifies the paper I measured.
25. As a researcher, I want a life-sciences-native citation cross-check (Europe PMC), so that a bio reviewer sees a source they recognise.
26. As a researcher, I want Dimensions' citation count plus its Field Citation Ratio (FCR) and Relative Citation Ratio (RCR), so that I have additional field-normalized figures.
27. As a user quoting citations, I want two independent counts reported side by side with their sources, so that I can cite the higher one honestly and attribute it.
28. As a tool author, I want to know how my repo ranks by stars among other repos carrying a given GitHub topic, so that I have a hands-free peer comparison.
29. As a user, I want the topic-based ranking to always state the topic and disclaim that GitHub topics are inconsistently applied, so that I don't overstate the ranking's authority.
30. As a researcher, I want to know whether my paper is open access, so that I can speak to its accessibility.
31. As a researcher, I want a count of Wikipedia mentions of my paper without needing any API key, so that I have a baseline attention signal for free.
32. As a researcher who has an Altmetric key, I want the richer attention breakdown (news, blogs, policy, patents, social, Mendeley), so that I can speak to broader attention.
33. As a user without an Altmetric key, I want the Report to clearly say the rich attention breakdown was not collected because no key was present, so that its absence is never mistaken for zero attention.
34. As a user, I want a metric that legitimately doesn't exist for my Project (e.g. no npm package) shown as "N/A", never as 0, so that absence isn't read as failure.
35. As a user, I want a metric that failed to fetch (rate limit, timeout, 5xx) marked as failed with the error, never as 0, so that a transient problem isn't mistaken for a real value.
36. As a user, I want one dead Provider to never abort the whole run, so that I still get every other metric.
37. As a user, I want transient failures retried with backoff, so that a momentary rate-limit doesn't cost me data.
38. As a user (or CI), I want the process to exit non-zero when any metric is still in a failed state, so that I can tell a complete Snapshot from a partial one before I quote it.
39. As a user, I want every run to write a durable Snapshot recording each Metric's value, Provider, Identity, as-of timestamp, coverage Window, and source, so that my numbers are reproducible and attributable.
40. As a user, I want Snapshots to be append-only files I can commit next to my grant draft, so that I have a permanent record of what I quoted and when.
41. As a user, I want to re-render an old Snapshot into a report without re-fetching, so that I can reproduce previously quoted figures exactly.
42. As a user, I want `boast about` to always fetch live, so that a Snapshot's timestamp truly means "these numbers were real at this instant."
43. As a user, I want `render` and `diff` to never touch the network, so that I can work with captured data offline and deterministically.
44. As a user, I want to diff two Snapshots, so that I can show growth over time (e.g. citations or stars gained).
45. As a user, I want a pretty terminal table by default, so that a bare run is immediately useful.
46. As a grant writer, I want a Markdown report I can paste straight into a document or README, so that I can drop the numbers into my writing.
47. As a grant writer, I want an auto-written prose sentence summarising the headline metrics, so that I have a ready-made statement to adapt.
48. As a user, I want the machine-readable Snapshot (JSON) written on every run regardless of the human format chosen, so that the durable record always exists.
49. As a user, I want to list the available Providers and see which need a key or are enabled, so that I understand what data I can get.
50. As a user, I want my GitHub token read from the environment (not the Manifest), so that secrets never end up committed alongside identities.
51. As a user, I want to be warned loudly when no GitHub token is present, so that I understand why repo metrics/ranking may be rate-limited.
52. As a considerate API consumer, I want the tool to send my contact email in requests to OpenAlex/Crossref (the "polite pool"), so that my requests are faster and well-behaved.
53. As a Manifest author, I want the Manifest to contain only identities and settings and never secrets, so that it is safe to commit and share.
54. As an extender, I want to add a new Provider (e.g. CRAN, Docker/Quay, Semantic Scholar) without modifying the core, so that the tool can grow to other ecosystems.
55. As a user of another ecosystem, I want to enable optional Providers via the Manifest/flags, so that I can measure R, JS, or containerised tools.
56. As a new user, I want the tool to be installable from crates.io, Bioconda, and Homebrew, so that I can get it through the channels I already use.
57. As a user, I want the Report to group metrics by Category (Code, Downloads, Citations, Attention), so that the summary is easy to read.
58. As a user, I want partial data visibly flagged in the Report, so that I don't accidentally quote incomplete results.

## Implementation Decisions

- **Language / form factor.** A Rust CLI plus an importable library crate. MIT licensed. Distributed via crates.io, Bioconda, and Homebrew.
- **Central entity.** A **Project** aggregates zero or more **Identities**, each of a known kind: a code repository, a distribution package (registry + name), or a paper (DOI/PMID). A paper-only lookup is a Project with a single paper Identity.
- **Identity syntax.** Repo: `owner/name` or a GitHub URL (host inferred, GitHub default). Package: `registry:name` (e.g. `bioconda:samtools`, `pypi:pysam`, `crates:boast`). Paper: a bare DOI or `pmid:<id>` (PMID resolved to DOI as needed).
- **Providers.** Metrics come from pluggable **Providers** behind a common trait: given an Identity, a Provider returns zero or more **Metrics** or a non-value **Outcome**. The tool ships a curated default set; further Providers are optional and enabled via Manifest/flags. New Providers are added without touching the core.
- **Default Provider set by Category.**
  - *Code:* GitHub — stars, forks, watchers, contributors, release-download total, repo age, and the topic-based **Cohort** rank.
  - *Downloads:* Bioconda/anaconda, PyPI, GitHub release assets, crates.io, Homebrew — reported per-channel, with an optional labelled **Rollup**.
  - *Citations:* OpenAlex (count + FWCI + `citation_normalized_percentile`), Crossref (metadata), Europe PMC (life-sciences cross-check), Dimensions badge API (count + FCR + RCR).
  - *Attention:* open-access status (OpenAlex) + Wikipedia mentions as keyless-lite defaults; Altmetric as an opt-in, key-gated Provider.
- **Ranking.** Paper "standing among similar work" is delivered by the field-normalized metrics that already exist for free (OpenAlex percentile/FWCI, Dimensions FCR/RCR), not a hand-rolled ranking. Repo peer comparison is a GitHub-**topic** Cohort (rank by stars among repos with a topic); the topic is read from the repo's own topics or set explicitly, and every Cohort result names its topic and discloses that topics are inconsistently applied.
- **Snapshot-centric architecture** (see ADR-0001). `boast about` fetches live and writes a timestamped, append-only, machine-readable **Snapshot** with full provenance; **Reports** are always rendered from Snapshots and never fetch. `about` is always-live (no cross-run cache); `render`/`diff` are always-offline. Snapshots carry a versioned schema. There is deliberately no "patch one Provider into an existing Snapshot" — a re-run makes a fresh, internally-consistent Snapshot.
- **Metric honesty model** (see ADR-0002). Every Provider×Identity fetch resolves to exactly one **Outcome**: `Value`, `NotApplicable` (shown N/A, never 0), or `Failed` (the error is recorded, never 0). Every Metric carries a coverage **Window** (cumulative / trailing / periodic); a **Rollup** may only combine compatible Windows and must name its members. Runs are best-effort with retries/backoff and exit non-zero if any `Failed` outcomes remain.
- **Data-source strategy and deliberate exclusions** (see ADR-0003). Notably: no Google Scholar (no API, ToS, fragile); Altmetric is key-gated since 10 Nov 2025; Crossref Event Data was sunset 23 Apr 2026 (hence keyless attention is "lite"); GitHub "used by"/dependents is opt-in only (scraped, no API); issue/PR counts omitted from defaults.
- **CLI shape.** Subcommands: `boast about <thing>` (fetch → Snapshot → terminal Report), `render` (Snapshot → Markdown/prose/…, offline), `diff` (Snapshot × Snapshot, offline), `providers` (list/status), `init` (write a Manifest). A bare identifier implies `about`.
- **Configuration and secrets.** The Manifest holds Projects only and is committable. Secrets (`GITHUB_TOKEN`, `ALTMETRIC_KEY`, …) come from the environment / `.env`; the polite-pool contact email from env or a user config file. The tool warns when no GitHub token is present.
- **Report formats (v1).** Terminal table (default), Markdown (primary saved artifact), and a prose snippet. JSON Snapshot always written. HTML (with over-time charts) and CSV are later renderers over the same Snapshot.
- **HTTP transport abstraction.** All Provider network access flows through a single transport trait — the one injected seam (see Testing Decisions).
- **No host-native dependencies** (see ADR-0004). No crate that requires a host-installed native library. TLS is **rustls**, never OpenSSL/`native-tls` (e.g. `reqwest` with `default-features = false` + `rustls-tls`, or a rustls-based client such as `ureq`); pure-Rust crates are preferred and `*-sys` crates avoided, so cross-compilation and static `*-musl` builds just work. The concrete HTTP client sits behind the transport seam, so it stays swappable. CI builds/releases static musl targets to prove the constraint.

## Testing Decisions

- **What makes a good test here.** Tests assert *external behavior* — the Outcome and Metrics a Provider produces from a given API response, the contents of a rendered Report, the result of a diff — never internal structure. Because the whole tool is built for honesty, the highest-value tests are the ones that pin the honesty rules: a 404 becomes `NotApplicable` (not 0), a 429/timeout becomes `Failed` (not 0), and incompatible Windows refuse to Rollup.
- **The single seam: the HTTP transport.** Every Provider reaches the network through one transport trait. In tests this trait returns *recorded real API responses* (cassette/fixture style, captured once from the live APIs — OpenAlex, GitHub, Bioconda/anaconda, PyPI, crates.io, Homebrew, Crossref, Europe PMC, Dimensions — and refreshed deliberately). This one seam deterministically and offline exercises: each Provider's response parsing; Outcome classification; the orchestrator assembling a Snapshot from multiple Providers; and retry/backoff (transport yields 429 then 200).
- **Pure tests, no seam needed, for everything downstream of the Snapshot:** Renderers (Snapshot → each output format), `diff` (two Snapshots → growth), the Rollup/Window compatibility rules, and Snapshot JSON (de)serialization + schema-version round-trips. These are pure functions tested with hand-constructed Snapshot values.
- **A thin layer of CLI-level integration tests** rides over the same transport seam to cover argument parsing, the bare-identifier shortcut, exit codes (non-zero on unresolved `Failed`), and file outputs.
- **Modules tested:** each default Provider (parsing + Outcome), the fetch orchestrator, the Snapshot model + serialization, the Rollup/Window logic, each Renderer, `diff`, and the CLI wiring.
- **Prior art.** This is a greenfield repo; this spec establishes the fixture/cassette pattern as the project's first test harness. Subsequent Providers should follow the same recorded-response convention.

## Out of Scope

- **Auto-discovery** of a Project's identities from a bare name or a single anchor (reading `CITATION.cff`, mapping a repo to a package, etc.). v1 requires identities to be given explicitly; discovery is a later, clearly-labelled helper.
- **Google Scholar** in any form (see ADR-0003).
- **A hosted web service / web UI.** v1 is a local CLI + library.
- **HTML and CSV Reports**, and **over-time charts** — later renderers over the same Snapshot (`diff` gives raw growth numbers in v1).
- **Optional Providers beyond the default set** as first-class defaults: GitLab/Bitbucket, Docker/Quay pulls, CRAN, Bioconductor, npm, Semantic Scholar. The trait supports them; they are not shipped-on by default.
- **Rich Altmetric attention as a core feature** — it is opt-in and key-gated.
- **GitHub "used by"/dependents** as a default metric — opt-in only, explicitly caveated as scraped.
- **Issue/PR counts** — omitted as activity/maintenance signals rather than reach.
- **Any database or server-side state** — Snapshots are files.

## Further Notes

- The differentiator versus an ad-hoc script is *provenance and reproducibility*: dated, attributable Snapshots you can commit and re-render, plus the honesty rules that make understating-by-accident and silently-overstating both impossible.
- The external-data landscape shifted materially in late 2025 / early 2026 (Altmetric key-gating; Crossref Event Data sunset). ADR-0003 records why the default sources look the way they do so these choices aren't re-litigated later.
- The domain vocabulary used throughout (Project, Identity, Provider, Metric, Window, Rollup, Outcome, Snapshot, Report, Cohort, Category, Manifest) is defined in `CONTEXT.md`.
