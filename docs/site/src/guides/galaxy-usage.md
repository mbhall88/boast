# Galaxy tool-execution usage

[Galaxy](https://galaxyproject.org/) is a web platform for running bioinformatics tools
without writing code. Many command-line tools also ship as a Galaxy wrapper, and the
[Galaxy Tool CoDex](https://github.com/galaxyproject/galaxy_codex) project publishes
cross-server execution and user-account totals for every wrapper it knows about — real
evidence of use beyond stars and downloads, if your tool has one.

Galaxy is **optional and off by default**. Most Projects have no Galaxy wrapper at all,
so fetching it on every run would add a mostly-empty request rather than a useful one.
Check `boast providers` to see it listed under Usage with `DEFAULT` set to `no`.

## Enabling it

```
boast about --repo mbhall88/rasusa --enable-provider galaxy
```

`--enable-provider` is repeatable and generic — it isn't Galaxy-specific — so a future
optional Provider uses the same flag. An unknown name is a usage error:

```
$ boast about --repo owner/name --enable-provider not-a-real-provider
error: unknown optional provider 'not-a-real-provider' (available: galaxy)
```

Galaxy only understands repository Identities (`--repo`, a GitHub URL, or the bare
`owner/name` positional); it has no package- or paper-level presence to report on.

## What each Metric means

```
━━ github:mbhall88/rasusa ━━
── Usage ──
  runs              104  all-time  galaxy
  user_accounts       24  all-time  galaxy
  public_instances     2  all-time  galaxy  available on UseGalaxy.eu, UseGalaxy.fr

── Notices ──
  Galaxy CoDex republishes cumulative tool-execution counts on its own refresh schedule, so this figure may lag the underlying Galaxy servers
  Runs: aggregated sum. Users: max unique per wrapper (to prevent over-counting) — not a count of distinct people, since CoDex's per-server totals can count one account once per server. Both are cumulative CoDex figures that may lag the underlying Galaxy servers.
```

`runs` and `user_accounts` each carry a caveat long enough to be shown once in the
Notices footer rather than repeated inline on every row — see
[Concepts](../concepts.md) for how boast decides between the two.

- **`runs`** — cumulative tool executions across every Galaxy wrapper CoDex has matched
  to this repository, summed across CoDex's tracked servers.
- **`user_accounts`** — the *maximum* per-wrapper account count, not a sum (see below).
  This is not a count of distinct people: CoDex's own figures can count one account
  separately per server, so treat it as "at least this many accounts," never as a
  headcount.
- **`public_instances`** — how many of the four major public Galaxy servers
  (UseGalaxy.org, UseGalaxy.org.au, UseGalaxy.eu, UseGalaxy.fr) have the tool installed,
  regardless of whether it's actually been run there. The Metric's note names which ones
  matched.

A repository with no matching CoDex entry is `NotApplicable`, never a real zero — most
repositories simply have no Galaxy wrapper.

## Multi-suite aggregation

One upstream tool can have several Galaxy wrappers, one per subcommand — CoDex calls
each a "suite." `vcflib` alone maps to 23. When more than one suite matches a
repository:

- **`runs` is summed** across every matched suite — it's a genuine execution count, and
  summing doesn't over-count anything.
- **`user_accounts` is *not* summed** — the maximum across the matched suites is used
  instead, since the same account can run more than one subcommand of the same tool.
  Summing would inflate "how many accounts used this tool" into "how many
  (account, subcommand) pairs were used."
- **`public_instances` is a union** — available if *any* matched suite is installed
  there.

Every matched suite ID is recorded in a Provider Note so the aggregation is auditable:

```
── Provider Notes ──
  galaxy: Matched 23 Galaxy CoDex suites: vcf2tsv, vcfaddinfo, vcfallelicprimitives, ... — github:vcflib/vcflib
```

Matching is exact, on CoDex's `Homepage` field only (normalised the same way boast
normalises any GitHub URL) — never on CoDex's `Suite source`, which names the wrapper
repository (typically a shared multi-tool repo like `galaxyproject/tools-iuc`), not the
upstream Project actually being measured.

## Why the numbers may lag

CoDex republishes its dataset on its own refresh schedule, independent of the live
Galaxy servers — `runs` and `user_accounts` both carry a note saying so, since they're
cumulative counts CoDex derives from its own snapshot. Treat these figures as "at least
this much use, as of whenever CoDex last refreshed," not a live server query.
`public_instances` doesn't carry the same caveat: installation is a current-state fact,
not a count that accumulates staleness the same way. See
[ADR-0011](../design/0011-optional-providers-and-galaxy-codex-as-source-of-truth.md) for
why CoDex was chosen over the live Galaxy Europe dashboard and ToolShed's install counts.

## Persisting the choice in a Manifest

`--save`/`boast init` persist an explicit `--enable-provider` into the Manifest's
`enable_providers`, so a scheduled run doesn't need to repeat the flag:

```toml
[[project]]
identities = ["github:mbhall88/rasusa"]
enable_providers = ["galaxy"]
```

`boast about manifest.toml` then fetches Galaxy for that Project automatically. An
explicit `--enable-provider` on the command line overrides a Manifest's own selection
entirely for that run, the same override rule `--topic` already applies to Cohort
selection.
