# Container pulls roll up, with the caveat travelling

## Status

accepted — refines ADR-0002 rule 2, which named a Docker pull as a channel never to be summed

## Context and decision

ADR-0002 rule 2 ends: *"never sums across incomparable channels (a Conda download ≠ a Docker pull ≠ a git clone)"*. Written before any container Provider existed, it settled the question by example rather than by rule, and the example it reached for is the one the Docker Hub Provider (#71) now needs to answer.

The example is right about the facts. A Docker Hub pull is not a conda install: it counts manifest fetches, so CI re-runs, layer probes, and mirror warming all land in the same figure, and the counter never resets — `library/ubuntu` sits near ten billion. Summing it with a conda count produces a total whose magnitude is driven by machine traffic.

But the clause proves more than it should. By that reasoning a crates.io download is not a PyPI download either — different ecosystems, different retry behaviour, different mirror topologies — yet those have been summed since v1 without objection. Every channel is incomparable to every other at *some* resolution. What ADR-0002 actually protects is the reader's ability to see what a total is made of, and the mechanism it built for that is the Rollup's own construction: a Rollup **must name every Metric it includes**. A total that names `docker:biocontainers/samtools (596335)` next to `conda:bioconda/samtools (9054107)` has not hidden anything.

So the rule is restated in terms of what is actually enforceable:

> **A Metric may join a Rollup when its Window is compatible and the Rollup names it. A channel whose unit is weaker than the others' must carry a note explaining how, and that note travels with the total into every format the total appears in — including prose.**

Windows still gate summation exactly as before; that part of rule 2 is untouched. What changes is that channel comparability is handled by disclosure rather than by exclusion.

The second half is not decoration, and it is where the original implementation of #71 was wrong. `render_prose` appended only the headline *citation* notice, so the sentence read:

```
As of 2026-08-05, this project has been downloaded 9650478 times (all-time) across 2 channels.
```

596,335 machine pulls, folded into a grant sentence, with the caveat sitting in a terminal footer the reader of that sentence never sees. Prose is the format that shows a total *without* the per-channel breakdown, which makes it the one format where the caveat is load-bearing rather than supplementary — and it was the only one where it was missing. Any notice attached to a Metric behind a headline number now travels with it.

## Considered options

- **Keep ADR-0002 as written and exclude container pulls from the Rollup.** Honours the existing text with a three-line change to `counts_as_download`, and makes the prose gap moot. Rejected: it treats "incomparable" as a property a channel either has or lacks, which does not survive contact with crates.io-versus-PyPI, and it leaves the reader with two numbers to add up themselves — which they will, without the caveat.
- **Include, and rely on the Notices footer alone.** The terminal and Markdown Reports do show it. Rejected: this was the original #71 implementation, and it is precisely the hole above. A rule that holds in two of three formats is not a rule.
- **Make the Provider non-default / opt-in.** ADR-0003 lists Docker/Quay among pluggable optional sources, so this has real support in the existing record. Rejected for now: no non-default Provider registry exists yet (`providers/mod.rs` notes the `DEFAULT` column reads "yes" throughout for want of a contrast), and building one to dodge a disclosure question is the wrong order. Revisit if an opt-in tier is built for other reasons.
- **Give the Rollup a notion of rollup-ineligibility separate from Window.** Most expressive: a Metric could be shown under Downloads yet marked as never summable. Rejected as speculative with one caller — and it would have re-answered the comparability question as exclusion anyway.

## Consequences

- **ADR-0002 rule 2's parenthetical no longer holds as written** and is amended in place to point here, rather than being left to contradict shipped behaviour. Its substantive requirement — Windows gate summation, a Rollup names its members — is unchanged and is what this ADR leans on.
- **`render_prose` now appends notices from the headline downloads Metrics as well as the headline citation Metric.** This is the ADR-0005 rule applied consistently rather than a new one: a notice attached to a quoted number follows it into every format.
- **ADR-0008's "operational caveats do not belong in prose" still stands**, but its wording narrows too far and is amended to point here. The split is by Outcome kind, not by content: a `Metric.note` on a real Value is a Notice and travels; a `NotApplicable`/`Failed` message is a Provider Note and does not. ADR-0008 described the travelling kind as "the licence notice", because Dimensions' licence text was the only instance then in existence. A caveat qualifying a number that *was* collected is the same kind and travels for the same reason.

- **This changes prose for one existing Provider, deliberately.** PyPI attaches a note explaining that pypistats' "last month" bucket is treated as a trailing 30 days because no exact day boundary is published. That note now follows the figure into prose, where the sentence says "last 30 days" without qualification. It is the same class of defect this ADR was written to fix, found by the same reasoning, so it is fixed rather than grandfathered.
- **A Provider adding a weak-unit channel now owes a note, not just a Metric.** `docker_hub.rs` is the worked example. There is no mechanism forcing this, so it is a review obligation — the same status as the Category a Provider picks.
- **`long_notes` becomes the single definition of "a note long enough to be a notice"**, shared by the whole-Snapshot footer and the new headline-scoped prose path, so the two can never disagree about what qualifies.
- **Nothing about the Snapshot changes.** Notes were always recorded on their Metrics and always survived to the JSON; this was a rendering gap. No schema bump, and existing Snapshots re-render under the new rules.
