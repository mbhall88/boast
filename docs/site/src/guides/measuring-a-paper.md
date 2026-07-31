# Measuring a paper

The smallest possible Project: just a paper, identified by DOI or PubMed ID, with no
code repository or package attached. This is the right shape when you're reporting on a
publication itself rather than a specific tool.

```
boast about 10.1371/journal.pbio.1002195
```

A bare PubMed ID works the same way:

```
boast about pmid:31234567
```

This fetches Citations and Attention Metrics only — the Code and Downloads Categories
have nothing to attach to without a repository or package, and are reported as
not-applicable (never as zero; see [ADR-0002](../design/0002-metric-honesty-model.md)).

Each run already writes a Snapshot to `snapshots/` (pass `--no-save` to only print). See
[Diffing the history](./ci-snapshots.md#diffing-the-history-once-you-have-it) to compare
one Snapshot against a later run.
