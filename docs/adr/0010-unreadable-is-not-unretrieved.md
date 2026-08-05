# An unreadable channel is NotApplicable, not Failed

## Status

accepted — refines ADR-0002 rule 1, whose Failed examples include "missing key"

## Context and decision

ADR-0002 rule 1 defines the three-state Outcome, and lists what belongs in each: `NotApplicable` is "the Identity legitimately has no presence on that channel", `Failed` is "a transient error: rate limit, timeout, 5xx, **missing key**".

The Quay.io Provider (#72) forced the question that list papers over. Quay answers **401, never 404**, for any repository an unauthenticated caller cannot see — verified live against a missing repository, a missing namespace, and by construction a private one. The three are deliberately indistinguishable, so the registry cannot be enumerated. There is no 404 to key off.

Read literally, ADR-0002 sends that 401 to `Failed`: it is an auth response, and for a genuinely private repository "the number exists but wasn't retrievable" is precisely true. But `Failed` makes two claims that are both wrong here, and rule 3 turns the second into user-visible damage:

1. **That retrying might work.** It cannot. `RetryingTransport` doesn't retry 401 (correctly — it isn't transient), and boast holds no Quay credential to retry *with*. The Provider is keyless by design; there is no configuration in which this 401 becomes a number.
2. **That the Snapshot is partial.** Rule 3 exits non-zero when any `Failed` remains, so `boast about --package quay:biocontainers/typo` would report a broken run — and, worse, a Manifest naming one package not published on Quay would permanently fail every scheduled run that includes it. A package's absence from a registry is not a defect in the fetch.

So the rule is restated in terms of what the caller can act on:

> **`Failed` means the number is retrievable and this attempt didn't get it. Where no retry and no available configuration could ever yield a number, the Outcome is `NotApplicable`, whatever status code carried that news — with any ambiguity disclosed in the note.**

This is not new behaviour so much as the existing practice written down. `Altmetric` already classes a missing `ALTMETRIC_KEY` as `NotApplicable` (`altmetric.rs`'s `NO_KEY_NOTE`), and CONTEXT.md's **Provider Note** glossary entry already gives "no API key configured" as a `NotApplicable` example — directly contradicting ADR-0002 rule 1's own parenthetical. The contradiction has been latent since v1; Quay is the first case where it changes an Outcome.

The disclosure half is load-bearing. Quay's 401 genuinely is ambiguous, so the note says so rather than asserting an absence it cannot verify: *"no public repository on Quay.io (Quay answers alike for a missing or a private repository, so this may be a private image)"*. `NotApplicable` here claims only that there is no **public** presence — which is exactly what was observed.

## Considered options

- **Send the 401 to `Failed`, per rule 1's literal text.** Honours the ADR as written and needs no code. Rejected: it marks every run touching a package that simply isn't on Quay as partial, and invites the user to retry something that can never succeed. It would make the exit code — the thing rule 3 exists to keep meaningful — fire on a non-event.
- **Distinguish missing from private before classifying.** Most honest in principle. Rejected: impossible by construction. Quay's anti-enumeration behaviour exists specifically to deny this distinction to unauthenticated callers, and boast is an unauthenticated caller by design.
- **Add a fourth Outcome state (e.g. `Unreadable`).** Expressive, and would keep rule 1's wording intact. Rejected as a large blast radius — Outcome is serialized into every Snapshot (a schema bump), matched in the orchestrator, both renderers, and `diff` — for one Provider's status code, and it would still have to pick an exit-code behaviour, which is the actual question. `NotApplicable` plus an honest note already answers it.
- **Give the Provider an optional `QUAY_TOKEN` so private repos resolve.** Would make the 401 a real "missing key" and put it back under rule 1 legitimately. Rejected as out of scope and low value: boast measures *public* reach, and a private image has none to measure by definition.

## Consequences

- **ADR-0002 rule 1's "missing key" example no longer holds as written** and is amended in place to point here. The three-state model and the never-coerce-to-0 rule are untouched — this is about which of two existing states a case lands in, not about inventing leniency.
- **A Provider may now classify a non-404 status as `NotApplicable`**, where the status means "you will never see this". `quay.rs` is the worked example; `provider::classify_status` remains the default for everything else, and still sends unrecognised statuses to `Failed`. Nothing is relaxed by default: a Provider has to opt in, deliberately, per status.
- **The exit code keeps meaning what rule 3 says it means.** A partial Snapshot is still distinguishable from a complete one — this change removes a class of false positive from that signal rather than weakening it.
- **The risk is a genuine outage misread as absence.** If Quay were to start 401-ing broadly — auth-gating the API, or rate-limiting via 401 rather than 429 — every lookup would report N/A with a zero exit code, and a reader would see "not published on Quay" when the truth is "Quay is closed". This is the real cost of the decision, accepted because the alternative misclassifies the common case to protect against a hypothetical one, and because the note keeps the reason visible in the Report either way. Worth revisiting if Quay's behaviour changes.
- **`NotApplicable` notes now carry disclosure obligations, not just a reason.** Where a Provider cannot distinguish absence from invisibility, the note says so. A note that flatly asserted "not found on Quay.io" would be stating something the response never established.
