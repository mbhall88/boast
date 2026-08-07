# Researcher-Level Metrics

`boast` does not report researcher-level measures such as h-index, i10-index,
total works, or total citations.

## Why this is out of scope

`boast` measures the reach of a piece of research work. Its Projects and
Identities represent papers, repositories, and distribution packages, not
people. Treating an ORCID iD as a measurable Identity would shift the product
from showing how far work has reached towards judging how accomplished a
researcher is.

ORCID support remains an input feature: `boast init --orcid` expands a
researcher's self-curated ORCID record into one Project per identifiable work.
This preserves the work-centric model while supporting the grant-writing use
case.

Reconsider this decision if users specifically request researcher-level
metrics and there is a clear reason they belong in `boast` rather than a
separate tool or report.

## Prior requests

- #46 — "Researcher-level metrics: should an ORCID become an Identity in its own right?"
