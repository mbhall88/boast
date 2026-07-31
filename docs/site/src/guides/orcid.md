# A researcher's whole publication record

Rather than measuring one tool, you can expand a researcher's [ORCID
iD](https://orcid.org) into one Project per work in their public record — every paper
they've published, each as its own bare-paper Project. An ORCID iD identifies a
*person*, never a *piece of work*, so it isn't an Identity itself; it's an input that
**expands into** a set of them (see
[ADR-0006](../design/0006-orcid-expands-to-identities-it-is-not-one.md)).

```
boast init --orcid 0000-0002-1825-0097 -o manifest.toml
```

This performs a real network fetch against the ORCID public API (unlike the rest of
`init`, which only writes a file from what you already gave it) and writes one
`[[project]]` entry per work that has a DOI or PMID. Works with neither are skipped —
boast has no Provider that can measure them — and a summary of how many were found,
kept, and skipped is printed to stderr.

`--orcid` accepts a bare iD, an `orcid:`-prefixed one, or a full `orcid.org` URL, and is
repeatable if you want one Manifest covering several researchers:

```
boast init --orcid 0000-0002-1825-0097 --orcid 0000-0001-2345-6789 -o manifest.toml
```

It's exclusive with every other identity source (positionals, `--repo`, `--package`,
`--from-file`) — an ORCID expansion has no defensible answer to "which of these works
does that repo belong to?", so mixing them is rejected rather than guessed at.

## Works without a DOI or PMID

Most ORCID records carry works boast can't measure (books, datasets, talks — anything
without a DOI or PMID). By default these are silently dropped. Pass
`--include-unidentified` to list them instead, as commented-out `[[project]]` blocks you
can fill in by hand if one of them does have an identifier ORCID just didn't capture:

```
boast init --orcid 0000-0002-1825-0097 --include-unidentified -o manifest.toml
```

## Running it

Once you have the Manifest:

```
boast about manifest.toml
```

writes one Snapshot per Project. Because that's usually a lot of Projects for one
researcher, the [CI automation guide](./ci-snapshots.md) — built for a single-Project
Manifest — doesn't directly cover this case; see its "Scope" section for the reason and
the workaround.
