//! Runs the enabled Providers over a Project's Identities and assembles a
//! Snapshot. Best-effort: one Provider's failure never blocks the others; each
//! Provider×Identity fetch is recorded as its own Outcome (ADR-0002).
//!
//! Fetches run concurrently across hosts but strictly serially within one
//! host — see ADR-0007. Every current Provider maps 1:1 onto a distinct
//! host, so a Provider's name stands in for its host.

use std::collections::VecDeque;
use std::sync::Mutex;

use time::OffsetDateTime;
use tracing::{debug, warn};

use crate::model::{FetchResult, Identity, Outcome, Project, Snapshot};
use crate::provider::Provider;
use crate::transport::Transport;

/// Default cap on worker threads (see `run_with_concurrency`), and what
/// `run` and the CLI's `-j`/`--threads` default to. The current default
/// Provider registry only ever has ~11 hosts, so this is mostly a safety
/// ceiling for a future larger set of Providers, not a real-world limit.
pub const DEFAULT_CONCURRENCY: usize = 8;

/// One Provider×Identity fetch to perform. Built once, up front, so the job
/// list's order — identity-major, provider-minor, same as the old nested
/// loop — is fixed before any thread starts, independent of completion order.
struct Job<'a> {
    provider: &'a dyn Provider,
    identity: &'a Identity,
    canonical: String,
}

/// Fetch every applicable Provider for every Identity and return a Snapshot,
/// using the default concurrency cap ([`DEFAULT_CONCURRENCY`]). See
/// [`run_with_concurrency`] to tune it.
pub fn run(
    project: &Project,
    providers: &[Box<dyn Provider>],
    transport: &dyn Transport,
) -> Snapshot {
    run_with_concurrency(project, providers, transport, DEFAULT_CONCURRENCY)
}

/// Fetch every applicable Provider for every Identity and return a Snapshot.
///
/// `max_concurrent_hosts` caps how many distinct hosts are fetched from at
/// once — it never allows more than one request in flight against the *same*
/// host, no matter how high it's set (ADR-0007). Raising it past the number
/// of hosts a Project actually touches (at most the size of the Provider
/// registry — ~11 by default) buys nothing: there's no further axis to
/// parallelise on. Lowering it (down to `1`, fully sequential) is the
/// meaningful direction to tune, e.g. to open fewer simultaneous connections
/// on a constrained network. `0` is treated as `1`, since nothing would ever
/// run otherwise.
pub fn run_with_concurrency(
    project: &Project,
    providers: &[Box<dyn Provider>],
    transport: &dyn Transport,
    max_concurrent_hosts: usize,
) -> Snapshot {
    let max_concurrent_hosts = max_concurrent_hosts.max(1);
    let mut jobs: Vec<Job> = Vec::new();
    for identity in &project.identities {
        let canonical = identity.canonical();
        for provider in providers {
            if provider.supports(identity) {
                jobs.push(Job {
                    provider: provider.as_ref(),
                    identity,
                    canonical: canonical.clone(),
                });
            }
        }
    }

    // Group job indices by host (Provider name), preserving first-seen
    // order, so a host's jobs form one queue that's always drained strictly
    // in order by whichever worker thread picks it up — polite pools like
    // Crossref/OpenAlex must never see two concurrent requests from us.
    let mut host_order: Vec<&str> = Vec::new();
    let mut host_queues: Vec<VecDeque<usize>> = Vec::new();
    for (index, job) in jobs.iter().enumerate() {
        let host = job.provider.name();
        let slot = match host_order.iter().position(|h| *h == host) {
            Some(slot) => slot,
            None => {
                host_order.push(host);
                host_queues.push(VecDeque::new());
                host_order.len() - 1
            }
        };
        host_queues[slot].push_back(index);
    }
    let host_count = host_queues.len();

    // Each job writes exactly once, to its own pre-assigned slot, so the
    // final `results` order is the job order above — never completion order.
    let slots: Vec<Mutex<Option<FetchResult>>> = jobs.iter().map(|_| Mutex::new(None)).collect();
    let queues: Mutex<VecDeque<VecDeque<usize>>> = Mutex::new(host_queues.into_iter().collect());

    std::thread::scope(|scope| {
        for _ in 0..host_count.min(max_concurrent_hosts) {
            scope.spawn(|| {
                loop {
                    let Some(queue) = queues.lock().unwrap().pop_front() else {
                        break;
                    };
                    for job_index in queue {
                        let job = &jobs[job_index];
                        debug!(provider = job.provider.name(), identity = %job.canonical, "fetching");
                        let outcome = job.provider.fetch(job.identity, transport);
                        match &outcome {
                            Outcome::Values { metrics, .. } => {
                                debug!(provider = job.provider.name(), identity = %job.canonical, metrics = metrics.len(), "fetched")
                            }
                            Outcome::NotApplicable { note } => {
                                debug!(provider = job.provider.name(), identity = %job.canonical, %note, "not applicable")
                            }
                            Outcome::Failed { error } => {
                                warn!(provider = job.provider.name(), identity = %job.canonical, %error, "fetch failed")
                            }
                        }
                        *slots[job_index].lock().unwrap() = Some(FetchResult {
                            provider: job.provider.name().to_string(),
                            // Second clone of `canonical` (the first built
                            // the Job): `job` is `&Job` shared from `jobs`,
                            // so nothing can move out of it here.
                            identity: job.canonical.clone(),
                            category: job.provider.category(),
                            outcome,
                        });
                    }
                }
            });
        }
    });

    let results = slots
        .into_iter()
        .map(|slot| {
            slot.into_inner()
                .unwrap()
                .expect("every job slot is written exactly once before threads join")
        })
        .collect();

    Snapshot {
        schema_version: Snapshot::SCHEMA_VERSION,
        tool: "boast".to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: OffsetDateTime::now_utc(),
        identities: project.identities.iter().map(|i| i.canonical()).collect(),
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, Identity, Outcome, PaperId};
    use crate::providers::default_providers;
    use crate::transport::{MockTransport, TransportError};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn project() -> Project {
        Project::new(vec![Identity::Paper(PaperId::Doi("10.1/x".into()))])
    }

    #[test]
    fn assembles_snapshot_with_metrics_and_no_failures() {
        let body = r#"{"cited_by_count": 10, "fwci": 2.0, "citation_normalized_percentile": null}"#;
        // OpenAlex, Dimensions, and Europe PMC return metrics; Crossref has no record for this DOI.
        let t = MockTransport::new()
            .on("api.openalex.org/works/doi:", 200, body)
            .on("api.crossref.org/works/", 404, "")
            .on(
                "metrics-api.dimensions.ai/doi/",
                200,
                r#"{"times_cited": 5, "field_citation_ratio": null, "relative_citation_ratio": null, "license": null}"#,
            )
            .on(
                "ebi.ac.uk/europepmc/webservices/rest/search",
                200,
                r#"{"hitCount":1,"resultList":{"result":[{"citedByCount":3}]}}"#,
            )
            .on(
                "en.wikipedia.org/w/api.php",
                200,
                r#"{"query":{"searchinfo":{"totalhits":2}}}"#,
            )
            // Only reached if ALTMETRIC_KEY happens to be set locally.
            .on("api.altmetric.com/v1/fetch/doi/", 200, r#"{"score":1.0}"#);

        let snap = run(&project(), &default_providers(), &t);

        assert_eq!(snap.schema_version, Snapshot::SCHEMA_VERSION);
        assert_eq!(snap.identities, vec!["doi:10.1/x".to_string()]);
        assert!(!snap.has_failures());
        // OpenAlex(2) + Dimensions(1) + Europe PMC(1) + Wikipedia(1); Crossref
        // has no record. Altmetric contributes 0 unless ALTMETRIC_KEY happens
        // to be set in the environment running this test (see tests/skeleton.rs
        // for the same guard against that ambient-env dependency).
        assert_eq!(snap.metrics().count(), 5 + altmetric_metric_count());
    }

    /// How many Metrics `Altmetric::new()` contributes in these tests' mocked
    /// `{"score":1.0}` response: 0 unless `ALTMETRIC_KEY` happens to be set in
    /// the environment running the test (in which case it's a real fetch, not
    /// the early no-key `NotApplicable`).
    fn altmetric_metric_count() -> usize {
        let has_key = std::env::var("ALTMETRIC_KEY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if has_key {
            1
        } else {
            0
        }
    }

    #[test]
    fn records_failure_without_aborting() {
        // OpenAlex fails; Crossref, Dimensions, and Europe PMC are fine — one
        // dead Provider must not block others.
        let t = MockTransport::new()
            .on_error(
                "api.openalex.org/works/doi:",
                TransportError::ConnectionFailed,
            )
            .on("api.crossref.org/works/", 404, "")
            .on("metrics-api.dimensions.ai/doi/", 404, "")
            .on(
                "ebi.ac.uk/europepmc/webservices/rest/search",
                200,
                r#"{"hitCount":0,"resultList":{"result":[]}}"#,
            )
            .on("en.wikipedia.org/w/api.php", 404, "")
            // Only reached if ALTMETRIC_KEY happens to be set locally.
            .on("api.altmetric.com/v1/fetch/doi/", 404, "");
        let snap = run(&project(), &default_providers(), &t);

        assert!(snap.has_failures());
        assert_eq!(snap.metrics().count(), 0);
        assert!(matches!(snap.results[0].outcome, Outcome::Failed { .. }));
    }

    /// A route for every citation Provider other than OpenAlex, so a
    /// `RetryingTransport` composition test can isolate what happens to
    /// OpenAlex's own Outcome without the others panicking on an unmatched URL.
    fn transport_with_only_openalex_left_open(openalex: MockTransport) -> MockTransport {
        openalex
            .on("api.crossref.org/works/", 404, "")
            .on("metrics-api.dimensions.ai/doi/", 404, "")
            .on(
                "ebi.ac.uk/europepmc/webservices/rest/search",
                200,
                r#"{"hitCount":0,"resultList":{"result":[]}}"#,
            )
            .on("en.wikipedia.org/w/api.php", 404, "")
            // Only reached if ALTMETRIC_KEY happens to be set locally.
            .on("api.altmetric.com/v1/fetch/doi/", 404, "")
    }

    #[test]
    fn a_429_then_200_composes_through_retry_and_provider_into_a_real_outcome_value() {
        // Exercises issue #3 AC1 end-to-end: a 429 then a 200 must reach the
        // orchestrator as an `Outcome::Values`, not just a 200 `HttpResponse`.
        let body =
            r#"{"cited_by_count": 10, "fwci": null, "citation_normalized_percentile": null}"#;
        let mock = transport_with_only_openalex_left_open(
            MockTransport::new()
                .on_sequence("api.openalex.org/works/doi:", &[(429, ""), (200, body)]),
        );
        let transport = crate::transport::RetryingTransport::with_policy_and_sleep(
            mock,
            Default::default(),
            |_| {},
        );

        let snap = run(&project(), &default_providers(), &transport);

        assert!(!snap.has_failures());
        let openalex = snap
            .results
            .iter()
            .find(|r| r.provider == "openalex")
            .unwrap();
        assert!(matches!(openalex.outcome, Outcome::Values { .. }));
    }

    #[test]
    fn persistent_429_composes_through_retry_and_provider_into_a_bounded_failed_outcome() {
        // Exercises issue #3 AC2 end-to-end: retries must be bounded (never
        // forever) and still resolve to a real `Outcome::Failed`.
        let mock = transport_with_only_openalex_left_open(MockTransport::new().on(
            "api.openalex.org/works/doi:",
            429,
            "",
        ));
        let policy = crate::transport::RetryPolicy {
            max_retries: 2,
            base_delay: std::time::Duration::from_millis(1),
        };
        let transport =
            crate::transport::RetryingTransport::with_policy_and_sleep(mock, policy, |_| {});

        let snap = run(&project(), &default_providers(), &transport);

        assert!(snap.has_failures());
        let openalex = snap
            .results
            .iter()
            .find(|r| r.provider == "openalex")
            .unwrap();
        assert!(matches!(openalex.outcome, Outcome::Failed { .. }));
    }

    /// A `Provider` test double that sleeps for a fixed delay before
    /// returning, so tests can force a specific completion order.
    struct SlowProvider {
        name: &'static str,
        delay: Duration,
    }

    impl Provider for SlowProvider {
        fn name(&self) -> &'static str {
            self.name
        }

        fn category(&self) -> Category {
            Category::Code
        }

        fn supports(&self, _identity: &Identity) -> bool {
            true
        }

        fn fetch(&self, _identity: &Identity, _transport: &dyn Transport) -> Outcome {
            std::thread::sleep(self.delay);
            Outcome::NotApplicable {
                note: self.name.to_string(),
            }
        }
    }

    #[test]
    fn results_are_ordered_by_job_order_not_completion_order() {
        // Two distinct-host Providers on one Project; the first-declared one
        // is by far the slower, so it finishes *last*. If Snapshot results
        // were assembled in completion order (a race), "fast" would come
        // first — it must not: ordering must match job order for byte-
        // identical Snapshot JSON across runs (ADR-0007).
        let providers: Vec<Box<dyn Provider>> = vec![
            Box::new(SlowProvider {
                name: "slow",
                delay: Duration::from_millis(60),
            }),
            Box::new(SlowProvider {
                name: "fast",
                delay: Duration::from_millis(0),
            }),
        ];
        let t = MockTransport::new();

        let snap = run(&project(), &providers, &t);

        let order: Vec<&str> = snap.results.iter().map(|r| r.provider.as_str()).collect();
        assert_eq!(order, vec!["slow", "fast"]);
    }

    /// A `Provider` test double that asserts it is never entered while
    /// another fetch on the same shared `busy` flag is still in flight —
    /// i.e. that two jobs sharing a host never overlap.
    struct GuardedProvider {
        busy: Arc<AtomicBool>,
        delay: Duration,
    }

    impl Provider for GuardedProvider {
        fn name(&self) -> &'static str {
            "guarded"
        }

        fn category(&self) -> Category {
            Category::Code
        }

        fn supports(&self, _identity: &Identity) -> bool {
            true
        }

        fn fetch(&self, _identity: &Identity, _transport: &dyn Transport) -> Outcome {
            assert!(
                !self.busy.swap(true, Ordering::SeqCst),
                "two fetches on the same host ran concurrently"
            );
            std::thread::sleep(self.delay);
            self.busy.store(false, Ordering::SeqCst);
            Outcome::NotApplicable {
                note: "guarded".to_string(),
            }
        }
    }

    #[test]
    fn same_host_jobs_never_run_concurrently() {
        // A Project with two Identities, both supported by the one
        // `GuardedProvider` — both jobs land on the same host queue and must
        // be drained strictly serially, never in parallel.
        let busy = Arc::new(AtomicBool::new(false));
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(GuardedProvider {
            busy,
            delay: Duration::from_millis(20),
        })];
        let project = Project::new(vec![
            Identity::Paper(PaperId::Doi("10.1/a".into())),
            Identity::Paper(PaperId::Doi("10.1/b".into())),
        ]);
        let t = MockTransport::new();

        let snap = run(&project, &providers, &t);

        assert_eq!(snap.results.len(), 2);
    }

    /// A `Provider` test double that records how many `CountingProvider`
    /// instances are inside `fetch` at once, and the peak seen across the
    /// whole run.
    struct CountingProvider {
        name: &'static str,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    impl Provider for CountingProvider {
        fn name(&self) -> &'static str {
            self.name
        }

        fn category(&self) -> Category {
            Category::Code
        }

        fn supports(&self, _identity: &Identity) -> bool {
            true
        }

        fn fetch(&self, _identity: &Identity, _transport: &dyn Transport) -> Outcome {
            let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(15));
            self.active.fetch_sub(1, Ordering::SeqCst);
            Outcome::NotApplicable {
                note: "counting".to_string(),
            }
        }
    }

    /// 12 single-use, distinctly-named `CountingProvider`s sharing one
    /// `active`/`peak` pair, all supporting the one Identity in `project()` —
    /// every job is immediately runnable, the shape that would spawn
    /// unbounded threads if a cap weren't enforced.
    fn many_counting_providers(
        active: &Arc<AtomicUsize>,
        peak: &Arc<AtomicUsize>,
    ) -> Vec<Box<dyn Provider>> {
        const NAMES: [&str; 12] = [
            "h0", "h1", "h2", "h3", "h4", "h5", "h6", "h7", "h8", "h9", "h10", "h11",
        ];
        NAMES
            .iter()
            .map(|&name| {
                Box::new(CountingProvider {
                    name,
                    active: active.clone(),
                    peak: peak.clone(),
                }) as Box<dyn Provider>
            })
            .collect()
    }

    #[test]
    fn concurrency_is_bounded_even_with_more_hosts_than_the_cap() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let providers = many_counting_providers(&active, &peak);
        let t = MockTransport::new();

        let snap = run(&project(), &providers, &t);

        assert_eq!(snap.results.len(), providers.len());
        let observed_peak = peak.load(Ordering::SeqCst);
        assert!(
            observed_peak <= DEFAULT_CONCURRENCY,
            "peak concurrency {observed_peak} exceeded the default bound of {DEFAULT_CONCURRENCY}"
        );
        assert!(
            observed_peak >= 2,
            "sanity check: expected some real concurrency, saw peak {observed_peak}"
        );
    }

    #[test]
    fn threads_flag_lowers_the_concurrency_cap_below_the_default() {
        // Same shape as the test above, but with `max_concurrent_hosts`
        // pinned to 1 — proving the knob actually throttles concurrency
        // (the "go down" direction the CLI's -j/--threads exists for), not
        // just that the built-in default happens to be small already.
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let providers = many_counting_providers(&active, &peak);
        let t = MockTransport::new();

        let snap = run_with_concurrency(&project(), &providers, &t, 1);

        assert_eq!(snap.results.len(), providers.len());
        assert_eq!(
            peak.load(Ordering::SeqCst),
            1,
            "max_concurrent_hosts=1 must fully serialize, even across distinct hosts"
        );
    }

    #[test]
    fn zero_max_concurrent_hosts_is_treated_as_one_rather_than_hanging() {
        // A worker count computed as `host_count.min(0)` would spawn no
        // threads at all, and every job would sit in its queue forever —
        // this pins that `0` is coerced to `1` instead.
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(SlowProvider {
            name: "only",
            delay: Duration::from_millis(0),
        })];
        let t = MockTransport::new();

        let snap = run_with_concurrency(&project(), &providers, &t, 0);

        assert_eq!(snap.results.len(), 1);
    }
}
