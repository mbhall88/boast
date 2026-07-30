# busybox:musl (~0.85 MB compressed) rather than scratch (0 MB, no shell at
# all — awkward to debug) or alpine (3.67 MB) — see issue #41. No
# ca-certificates layer is needed: `boast` is statically linked against
# `webpki-roots`, not the OS trust store (ADR-0004), so it carries its own
# TLS roots.
FROM busybox:musl

# Set by `docker buildx build --platform ...`; matches the per-arch binary
# paths .github/workflows/publish-docker.yml populates before this build —
# nothing is compiled inside this image.
ARG TARGETARCH
COPY binaries/linux/${TARGETARCH}/boast /usr/local/bin/boast

ENTRYPOINT ["/usr/local/bin/boast"]
