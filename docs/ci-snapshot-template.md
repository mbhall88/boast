# Snapshot history in CI

A copy-paste GitHub Actions workflow for *your own* repo (not boast's) that runs `boast
about` on a schedule, commits the resulting Snapshot, and keeps a rolling Markdown Report
up to date — so `boast diff` has real history to compare against with zero manual work.

This is deliberately **YAML you copy and edit**, not a published Action or reusable
workflow. Everyone adopting it customises something (the Manifest path, the schedule, which
identities it covers), and a template invites that where a versioned Action would fight it.
When a scheduled run fails at 3am it's almost always a rate limit or a network blip — plain
YAML has no indirection to dig through to find out why.

## Prerequisites

A [Manifest](../CONTEXT.md) listing the single Project you want to track (see "Scope"
below), committed to the repo. Build one once with:

```
boast init --repo owner/name --package crates:name 10.1234/journal.xyz
```

Commit the resulting `manifest.toml`. (`boast init --orcid <ORCID iD>` builds a Manifest too,
but typically lists *many* Projects — one per publication — which this template's report step
doesn't cover; see "Scope" below before using it here.)

## The workflow

Save as `.github/workflows/impact-snapshot.yml`:

```yaml
name: Impact snapshot

on:
  schedule:
    # 03:00 UTC on the 1st of every month. Citations, downloads, and stars
    # move slowly — weekly mostly adds diff noise and repo churn for near-
    # identical numbers. Monthly gives a clean twelve-points-a-year series,
    # matching how these figures actually get quoted ("as of March 2026").
    - cron: '0 3 1 * *'
  workflow_dispatch: {} # lets you trigger a run by hand to test the workflow

permissions:
  contents: write # needed to commit and push the Snapshot + report

jobs:
  snapshot:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install boast
        run: |
          set -euo pipefail
          curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mbhall88/boast/releases/latest/download/boast-installer.sh | sh
          echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"

      # Deliberately NOT `continue-on-error` and NOT `|| true`: `boast about`
      # exits 1 if any Provider fetch failed (rate limit, timeout, ...), and
      # that should still turn this job red so a real, persistent problem
      # doesn't go unnoticed. What it must NOT do is stop the Snapshot from
      # being written or committed — a partial Snapshot is honest data (a
      # `Failed` Outcome is recorded, never dropped), and skipping the commit
      # would additionally punch a silent gap in the history that a later
      # `diff` couldn't explain.
      - name: Run boast about
        run: boast about manifest.toml --snapshot-dir snapshots
        env:
          # Auto-provided by Actions — raises GitHub's API rate limit for the
          # Code category. No repo secret needed for this one.
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # Optional: only set this repo secret if you have an Altmetric
          # Details Page API key. Without it, Altmetric metrics are reported
          # as not-applicable rather than fetched — everything else still runs.
          ALTMETRIC_KEY: ${{ secrets.ALTMETRIC_KEY }}

      # `if: always()` so this and the commit step below still run even when
      # the step above exited non-zero. `boast render` itself also exits 1
      # for a Snapshot recording a `Failed` Outcome, so on a partial failure
      # this step goes red too, alongside `boast about` above — expected,
      # not a second problem: IMPACT.md is still written correctly (the
      # FAILED row included, per ADR-0002) and still gets committed below.
      - name: Regenerate the rolling report
        if: always()
        run: |
          set -euo pipefail
          newest=$(find snapshots -maxdepth 1 -name '*.json' | sort | tail -n1)
          boast render "$newest" --format markdown > IMPACT.md

      - name: Commit snapshot and report
        if: always()
        run: |
          set -euo pipefail
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add snapshots IMPACT.md
          git diff --cached --quiet && exit 0
          git commit -m "chore: monthly impact snapshot"
          git push
```

## The knobs

- **Schedule** — the `cron:` line. `'0 3 1 * *'` is monthly; tighten it (e.g. `'0 3 * * 1'`
  for weekly on Mondays) if your numbers move fast enough to be worth the extra `diff` noise
  and repo churn.
- **Manifest path** — `manifest.toml` in the `boast about` step. Point it at wherever your
  Manifest lives if it isn't at the repo root.
- **Snapshot directory** — `--snapshot-dir snapshots` (boast's own default). Snapshots are
  named by boast itself from the run timestamp (`YYYYMMDDTHHMMSSZ.json` when driven by bare
  identifiers) — already unique, already lexically sortable, already carrying the as-of time.
  When a Manifest drives the run, boast also suffixes the filename with the Project's own
  identity (e.g. `20260301T030001Z-doi-10.1234-journal.xyz.json`) so that
  multiple Projects sharing one Manifest never collide — still lexically sortable, just not a
  bare timestamp.
- **Report filename** — `IMPACT.md`, overwritten every run rather than timestamped. It's
  offline and deterministic (`boast render` never touches the network — see
  [ADR-0001](adr/0001-snapshot-centric-architecture.md)), so regenerating it is nearly free,
  and a stable filename gives you one current page to link to from your README instead of
  hundreds of near-identical Markdown files accumulating over the years.

Make sure `snapshots/` (and `IMPACT.md`) aren't excluded by your repo's `.gitignore` — it's
an easy thing to have picked up from a template that assumed the opposite.

**Why commit Snapshots instead of uploading them as workflow artifacts?** Artifacts expire
(90-day retention by default) and would silently evaporate the accumulating history that's
the entire point of this workflow — and you can't `diff` two of them without downloading
both by hand first. A committed file has neither problem.

### One Project per Manifest

This template's "Regenerate the rolling report" step renders whichever Snapshot file sorts
last — correct as long as your Manifest lists a single `[[project]]` (the common case:
tracking your own tool's reach, which is why the Prerequisites example above builds a
one-Project Manifest). A Manifest listing several Projects makes `boast about manifest.toml`
write one Snapshot file per Project on every run; the "newest" pick above then only covers
whichever Project's file happens to sort last, silently leaving the others out of `IMPACT.md`.
If you need one report covering several Projects, render each Project's own newest Snapshot
into its own file (e.g. loop over the distinct filename suffixes) rather than trying to
squeeze them into one `IMPACT.md`, or run this workflow once per Project against separate
single-Project Manifests.

## Diffing the history once you have it

Once you've got two or more committed Snapshots, compare any pair directly (filenames carry
the Project's own identity suffix, per "The knobs" above):

```
boast diff snapshots/20260301T030001Z-doi-10.1234-journal.xyz.json \
           snapshots/20260401T030001Z-doi-10.1234-journal.xyz.json
```

## Keys as repo secrets

- `GITHUB_TOKEN` — the workflow above uses the token Actions injects automatically
  (`secrets.GITHUB_TOKEN`); you don't need to create anything. It only raises the rate limit
  for GitHub repo metrics — omitting it still works, just at the unauthenticated 60
  requests/hour limit.
- `ALTMETRIC_KEY` — optional, and only relevant if you have an Altmetric **Details Page
  API** key (not an Explorer key — they're different products with different credentials).
  Add it under **Settings → Secrets and variables → Actions → New repository secret** on
  your repo. Without it, Attention-category Altmetric metrics report as not-applicable; every
  other Provider is unaffected.

Run `boast providers` to see the full, current list of which Providers need which key.
