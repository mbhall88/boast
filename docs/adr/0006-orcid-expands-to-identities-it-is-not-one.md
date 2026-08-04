# ORCID expands to Identities; it is not one

## Status

accepted

## Context and decision

An **ORCID iD** identifies a *researcher*. Every existing **Identity** — a paper, a code repository, a distribution package — identifies a *piece of work* that a Provider can fetch Metrics for. Supporting ORCID therefore forced a choice about which of those two things it is.

We decided: **an ORCID is an input expander, not an Identity.** `boast init --orcid <ORCID>` resolves it, once, into the set of Paper Identities the researcher has claimed, and writes them to a Manifest. The `Identity` enum is unchanged, and no Provider ever receives an ORCID.

Two consequences follow directly, and are deliberate rather than incidental:

- **`boast about orcid:…` is refused**, with an error pointing at `boast init --orcid`. `Identity::parse` recognises the ORCID shape (bare, `orcid:`-prefixed, and URL forms) *solely in order to give a better refusal* than the generic "could not recognise" catch-all.
- **Expansion produces one Project per work.** `CONTEXT.md` defines a Project as "a single piece of research work", so a researcher's 118 papers are 118 Projects, not one Project with 118 papers. This reuses the existing Manifest batch pipeline whole, with no new orchestration.

The expansion reads ORCID's own public API (`pub.orcid.org/v3.0/{orcid}/works`), which is keyless and returns the researcher's **self-curated** record.

## Considered options

- **Make ORCID a new Identity kind** (`Identity::Researcher`), with a Provider emitting Metrics about the researcher — h-index, i10-index, works count, total citations. These are available without a key: OpenAlex's `/authors/{orcid}` endpoint returns all of them in one call, and h-index is relevant to the grant-writing use case boast exists to serve. Rejected **for now, on product grounds rather than technical ones**: it would require rewriting `CONTEXT.md`'s definitions of both **Project** ("a single piece of research work") and **Identity** ("one external handle a Project links to"), because a researcher is neither. That shifts boast from *"how far did this piece of work reach"* toward *"how accomplished is this person"* — a decision that deserves to be made deliberately, not to arrive as a side effect of adding ORCID support. Tracked separately; this ADR would need superseding if it ships.

- **Expand via OpenAlex's author→works path** (`/authors/{orcid}` → `works_api_url`) instead of ORCID's own API. It finds work the researcher never claimed, but attribution is *algorithmic* and produces false positives on common names. That would spend requests measuring other people's papers and report the total as yours. Rejected: "what I claim as mine" is the better source of truth than "what an algorithm infers is mine", and a thin ORCID record is best fixed at ORCID, where it benefits the researcher everywhere rather than only here.

- **Let `boast about orcid:…` run directly**, expanding and fetching in one command. Rejected: expansion is cheap (one request) but the run it triggers is not — six Providers support papers, so ~118 works is ~700 requests. Putting a mandatory, reviewable artifact between the two makes the expensive step deliberate, and gives the user somewhere to prune before spending it. This is the same reasoning that already makes a Manifest a *generated save-file* rather than a hand-authored config.

## Consequences

- **`init` is no longer categorically offline.** It gains a network path. This is legal — ADR-0001 constrains only `about` (always-live) and `render`/`diff` (always-offline) — but it is a change in character, so `init`'s help text must say so rather than let users assume otherwise.

- **Works with neither a DOI nor a PMID cannot become Identities and are skipped.** Their *count* is always written into the generated Manifest's header, whether or not the user asks to see them listed. Silently shortening the record would understate a researcher's output — the same failure shape ADR-0002 forbids for Metrics ("we couldn't look it up" must never read as "it isn't there"), applied here to a Manifest instead.

- **The generated Manifest must remain valid and runnable as written.** Skipped works are therefore emitted as *commented-out* blocks under `--include-unidentified`, never as placeholder identities like `doi:FIXME` — a placeholder would make the freshly generated file fail to parse on the very next command, shipping the user something broken by default.

- Because expansion is one-per-work, a large record produces a large Manifest and a long run. That cost is surfaced up front, at `init` time, as a computed warning naming the actual request count — not discovered later when `about` runs.
