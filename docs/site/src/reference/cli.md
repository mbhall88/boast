<!--
GENERATED FILE — do not edit by hand.
Regenerate with docs/site/generate-reference.sh, run from the repo root.
-->

# CLI reference

Every subcommand and flag, straight from `--help`.

## `boast`

```
Gather reach and impact metrics for a research tool or paper into dated, quotable snapshots.

Usage: boast [OPTIONS] <COMMAND>

Commands:
  about      Fetch metrics for a Project, write a Snapshot, and print a report. A single `.toml` positional (see `boast init`) is loaded as a Manifest instead, running every Project it lists
  render     Render a stored Snapshot as Markdown or prose. Never touches the network (ADR-0001) — offline and deterministic for a given Snapshot
  diff       Compare two stored Snapshots and report the change in each shared Metric. Never touches the network (ADR-0001)
  providers  List the registered Providers: Category, default-enabled status, and key requirement. Never touches the network
  init       Write a Manifest TOML file from identifiers, without fetching — unless `--orcid` expands a researcher's record, which does (see its own help)
  help       Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...  Increase logging verbosity (-v info, -vv debug, -vvv trace)
  -q, --quiet       Silence all logging except errors
  -h, --help        Print help
  -V, --version     Print version
```

## `boast about`

```
Fetch metrics for a Project, write a Snapshot, and print a report. A single `.toml` positional (see `boast init`) is loaded as a Manifest instead, running every Project it lists

Usage: boast about [OPTIONS] [IDENTIFIER]...

Arguments:
  [IDENTIFIER]...  Identifiers: a DOI, doi.org URL, `pmid:12345678`, a github.com URL, `owner/name`, or a package as `registry:name` (e.g. `crates:boast`)

Options:
  -r, --repo <OWNER/NAME>        A GitHub repository as `owner/name` (alternative to a positional; repeatable)
  -p, --package <REGISTRY:NAME>  A distribution package as `registry:name`, e.g. `crates:boast` (alternative to a positional; repeatable)
  -f, --from-file <FILE>         Read identifiers from a file (one per line; `#` comments and blank lines ignored). Use `-` for stdin. Repeatable
  -t, --topic <TOPIC>            GitHub topic to rank repositories within, overriding each repo's own declared topics (see the Cohort disclaimer in the report). When the input is a Manifest, this overrides every Project's own topic too
  -d, --snapshot-dir <DIR>       Directory to write the Snapshot into [default: snapshots]
  -n, --no-save                  Print the report but do not write a Snapshot file
  -v, --verbose...               Increase logging verbosity (-v info, -vv debug, -vvv trace)
  -q, --quiet                    Silence all logging except errors
  -s, --save <FILE>              After fetching, also write a Manifest reflecting the identities (and `--topic`) used in this run, so a future run can `boast about <file>` instead of re-typing them. Not available when the input is itself a Manifest — use `boast init` to build one up front instead
  -j, --threads <N>              Maximum number of distinct hosts fetched from concurrently. Never more than one request is in flight against the *same* host no matter how high this is set (ADR-0007). Raising it past the number of hosts a Project actually touches (at most the Provider registry's size, ~11 by default) buys nothing; lower it to open fewer simultaneous connections [default: 8]
  -h, --help                     Print help
```

## `boast render`

```
Render a stored Snapshot as Markdown or prose. Never touches the network (ADR-0001) — offline and deterministic for a given Snapshot

Usage: boast render [OPTIONS] <SNAPSHOT>

Arguments:
  <SNAPSHOT>
          Path to a Snapshot JSON file written by `boast about`

Options:
  -f, --format <FORMAT>
          Output format

          Possible values:
          - markdown: Category-grouped Markdown Report — the primary saved artifact
          - prose:    A single grant-ready sentence summarising the headline Metrics
          
          [default: markdown]

  -v, --verbose...
          Increase logging verbosity (-v info, -vv debug, -vvv trace)

  -q, --quiet
          Silence all logging except errors

  -h, --help
          Print help (see a summary with '-h')
```

## `boast diff`

```
Compare two stored Snapshots and report the change in each shared Metric. Never touches the network (ADR-0001)

Usage: boast diff [OPTIONS] <OLD> <NEW>

Arguments:
  <OLD>  The earlier Snapshot JSON file
  <NEW>  The later Snapshot JSON file

Options:
  -v, --verbose...  Increase logging verbosity (-v info, -vv debug, -vvv trace)
  -q, --quiet       Silence all logging except errors
  -h, --help        Print help
```

## `boast providers`

```
List the registered Providers: Category, default-enabled status, and key requirement. Never touches the network

Usage: boast providers [OPTIONS]

Options:
  -v, --verbose...  Increase logging verbosity (-v info, -vv debug, -vvv trace)
  -q, --quiet       Silence all logging except errors
  -h, --help        Print help
```

## `boast init`

```
Write a Manifest TOML file from identifiers, without fetching — unless `--orcid` expands a researcher's record, which does (see its own help)

Usage: boast init [OPTIONS] [IDENTIFIER]...

Arguments:
  [IDENTIFIER]...  Identifiers: a DOI, doi.org URL, `pmid:12345678`, a github.com URL, `owner/name`, or a package as `registry:name` (e.g. `crates:boast`)

Options:
  -r, --repo <OWNER/NAME>        A GitHub repository as `owner/name` (alternative to a positional; repeatable)
  -p, --package <REGISTRY:NAME>  A distribution package as `registry:name`, e.g. `crates:boast` (alternative to a positional; repeatable)
  -f, --from-file <FILE>         Read identifiers from a file (one per line; `#` comments and blank lines ignored). Use `-` for stdin. Repeatable
  -t, --topic <TOPIC>            GitHub topic to record in the Manifest for this Project's Cohort ranking
  -o, --output <FILE>            Where to write the Manifest [default: manifest.toml]
  -O, --orcid <ORCID>            Expand a researcher's ORCID iD (bare, `orcid:`-prefixed, or an orcid.org URL) into a Manifest of every work with a DOI or PMID, one Project per work (ADR-0006; repeatable). **Performs a network fetch** — unlike the rest of `init`, which is otherwise offline. Exclusive with positionals/`--repo`/`--package`/`--from-file`: an ORCID expansion has no defensible answer to "which of these works does that repo belong to?"
  -v, --verbose...               Increase logging verbosity (-v info, -vv debug, -vvv trace)
  -q, --quiet                    Silence all logging except errors
  -u, --include-unidentified     With `--orcid`, also list works with neither a DOI nor a PMID (and so were skipped) as commented-out `[[project]]` blocks you can fill in by hand. Off by default: most ORCID records carry many such works
  -h, --help                     Print help
```
