# Paid Dimensions Metadata

`boast` does not currently integrate with the paid Dimensions DSL API for
research categories, concepts, funder metadata, or similar fields.

## Why this is out of scope

The existing Dimensions Provider uses the free, keyless Metrics API for
citation counts and field-normalised measures. The richer DSL API is a
separate institutional product with different authentication, terms, query
semantics, and access requirements.

Its metadata also does not fit cleanly into the existing Code, Downloads,
Citations, or Attention Categories. Supporting it would therefore require a
product decision about what belongs in a Report, as well as implementation of
another key-gated Provider. That complexity is not justified without evidence
that users want the richer metadata.

Reconsider this decision if a user requests the feature and can describe which
Dimensions fields they need and how they expect to use them.

## Prior requests

- #35 — "Explore richer Dimensions data (research categories, concepts) beyond the free Metrics API"
