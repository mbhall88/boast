# Metric honesty model

## Status

accepted

## Context and decision

`boast` exists to make impact claims that survive scrutiny, so the data model is built to make *understating impossible to do by accident* and *overstating impossible to do silently*. Three rules are load-bearing:

1. **Three-state Outcome.** Every Provider×Identity fetch resolves to exactly one of `Value` (a real number), `NotApplicable` (the Identity legitimately has no presence on that channel — e.g. samtools has no npm package), or `Failed` (a transient error: rate limit, timeout, 5xx, missing key). `NotApplicable` and `Failed` are **never coerced to 0** — a missing number and a zero number are different facts, and conflating them silently understates a tool's reach.

2. **Windows gate summation.** Every Metric carries a coverage **Window** — `cumulative` (all-time), `trailing` (rolling N days, e.g. Homebrew's 365-day installs), or `periodic` (a named bucket). Metrics may only be combined into a **Rollup** when their Windows are compatible, and a Rollup must name every Metric it includes. The tool never silently sums an all-time crates.io count with a 365-day Homebrew count, and never sums across incomparable channels (a Conda download ≠ a Docker pull ≠ a git clone).

3. **Best-effort with a truthful exit code.** One dead Provider never blocks the rest; transient failures get retries with backoff; but the process **exits non-zero if any `Failed` outcomes remain**, so a partial Snapshot is distinguishable from a complete one *before* anyone quotes it. Reports visibly mark partial data.

## Considered options

- **Coerce missing/failed to 0 and always exit 0.** Simpler code and prettier tables, but it turns the tool into something that quietly lies in the exact direction that damages a grant. Rejected outright.
- **Fail-fast on the first Provider error.** Robustness theatre — one rate-limited API would abort an otherwise-complete run. Rejected in favour of best-effort + explicit per-fetch Outcomes.

## Consequences

- The Snapshot schema must represent `NotApplicable`/`Failed` explicitly (with the error), not by omission.
- Callers (CI, scripts, the user) can gate on exit code to avoid quoting incomplete data.
