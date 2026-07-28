# Fetches run parallel across hosts, strictly serial within one host

## Status

accepted

## Context and decision

`orchestrator::run` used to be a plain nested `for identity { for provider { … } }` loop — every fetch sequential. That's fine for one DOI (~6 Providers), but an ORCID-expanded Manifest (see ADR-0006) can hold ~118 works, i.e. ~118 × 6 ≈ 700 sequential requests: a multi-minute floor before `RetryingTransport`'s backoff makes a bad run worse.

We decided: **fetches run concurrently across hosts, but never concurrently against the same host.** Crossref and OpenAlex operate "polite pools" that expect a sane request rate from a single client; firing many concurrent requests at one host would get us throttled or blocked — worse than the sequential status quo. Because every current Provider maps 1:1 onto a distinct host, parallelising across Providers yields roughly 6× speedup while staying *politer* than a naive "parallelise everything" approach: each host still sees requests one at a time, just from several hosts at once.

Concretely, `orchestrator::run` builds the full Provider×Identity job list up front (same order as the old nested loop), groups jobs by Provider name (standing in for host), and runs a bounded pool of `std::thread::scope` worker threads that each pull one host's job queue at a time and drain it strictly in order before picking up another. `ureq` is blocking, so threads are the natural fit; `tokio` is unnecessary and would cut against ADR-0004's minimal-dependency posture. No new dependencies were added. This requires `Provider` and `Transport` to be `Sync`, since fetches now happen through a shared reference from multiple threads.

## Considered options

- **Parallelise across everything (identities × providers), bounded only by a global limit.** Simpler, and would yield a bigger speedup on a Project with many Identities against the same Provider. Rejected: it reintroduces the exact problem this ADR exists to avoid — concurrent requests to Crossref/OpenAlex's polite pools — for a speedup that isn't the bottleneck (there are ~11 hosts but rarely more than a handful of same-host Identities in one Project).
- **`tokio` + `async` Providers.** Would parallelise more cheaply than OS threads at very high fan-out. Rejected: fan-out here is bounded by the number of distinct hosts (~11), not thousands of connections, so async's main advantage doesn't apply; it would add a large dependency tree purely for a workload threads already handle well, cutting against ADR-0004.
- **Unbounded thread-per-job.** Simplest possible parallel version. Rejected outright: a 700-job Manifest run would spawn 700 OS threads, and nothing bounds that as Manifests grow.

## Consequences

- **Snapshot result ordering had to become explicitly deterministic**, not just incidentally so. Snapshots are committed artifacts (see the CI-snapshot ticket): if results were serialised in completion order, every run would reorder its JSON and produce spurious diffs, breaking the reproducibility premise ADR-0001 relies on. `run` now pre-assigns each job a slot by its position in the original (identity-major, provider-minor) job list and writes into that slot regardless of which thread finishes when, so the final `results` order is always the job order — proven by a test that deliberately reverses completion order and asserts the output order is unaffected.
- **`Provider` and `Transport` gained a `Sync` supertrait bound.** Every real implementation satisfied it automatically (plain data, no interior mutability) except `MockTransport`'s scripted-sequence replies, which moved from `RefCell` to `Mutex` — an internal change only; its public test API is unchanged.
- **A Provider's name is used as its host key**, rather than parsing the actual request URL's host. This is accurate today (every Provider maps 1:1 onto one host) but is an assumption, not an invariant enforced by the type system — a future Provider that fans out across multiple real hosts, or two Providers sharing one host, would silently violate the "never concurrent within a host" guarantee this ADR relies on.
- **Concurrency is bounded by a default of 8 (`orchestrator::DEFAULT_CONCURRENCY`), user-tunable down via `about -j`/`--threads`**, independent of how many hosts or Identities are involved, so worker-thread count never scales with Manifest size. Raising it past the number of hosts a Project actually touches (at most the Provider registry's size, ~11 by default) buys nothing — there's no further axis to parallelise on, since a host's own queue is always drained strictly serially regardless of the cap. The meaningful direction to tune is down (to `1`, fully sequential), e.g. to open fewer simultaneous connections on a constrained network; `0` is rejected by the CLI and treated as `1` by the library function, since it would otherwise leave every job queued with no worker to run it.
