# No host-native dependencies; rustls, not OpenSSL

## Status

accepted

## Context and decision

`boast` is meant to be trivially installable and distributable everywhere (crates.io, Bioconda, Homebrew) and to cross-compile cleanly, including to static `*-musl` targets. The classic blocker to that is a dependency that links a host C library — above all **OpenSSL**, which routinely breaks cross-compilation and static linking.

We therefore adopt a dependency policy: **no crate that requires a host-installed native library.** Concretely:

- **TLS is rustls, never `openssl`/`native-tls`.** The HTTP stack must be built with rustls (e.g. `reqwest` with `default-features = false` + `rustls-tls`, or a rustls-based client such as `ureq`). `native-tls`/`openssl`/`openssl-sys` are prohibited anywhere in the tree.
- **Prefer pure-Rust crates and avoid `*-sys` crates.** Parsing, serialisation (serde/JSON/TOML), and everything else should be pure Rust so that a cross-build needs only a Rust toolchain and the target, not a cross C toolchain or vendored system libraries.
- **CI builds and releases static `x86_64-unknown-linux-musl` (and other) targets** to prove the constraint holds and to ship dependency-free binaries.

## Considered options

- **`reqwest` with the default `native-tls`/OpenSSL backend** — the most common Rust HTTP setup, but it drags in OpenSSL and the cross-compilation/static-linking pain we are explicitly avoiding. Rejected.
- **rustls with `aws-lc-rs` vs `ring` backend** — both avoid host OpenSSL and cross-compile far more cleanly than OpenSSL; either is acceptable. If a build environment makes `aws-lc-rs`'s C/asm awkward, the `ring` backend is the fallback. This is an internal knob, not a user-facing decision.

## Consequences

- The concrete HTTP client is contained behind the single HTTP-transport seam (see the v1 spec and ADR-0001/0002), so swapping clients — or TLS backends — is a localised change that does not touch Providers or the Snapshot model.
- Any future Provider or feature that would pull in a host-native dependency must be reworked or rejected; this constraint outranks convenience.
- rustls validates against a bundled/vendored root store (e.g. `webpki-roots`) rather than the host trust store, keeping behaviour identical across platforms — an intended consequence, not an oversight.
