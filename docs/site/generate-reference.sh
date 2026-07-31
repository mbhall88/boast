#!/usr/bin/env bash
# Regenerates the two reference pages that must never hand-drift from the
# real CLI:
#   - src/reference/providers.md, from `boast providers`
#   - src/reference/cli.md, from every subcommand's `--help`
# CI re-runs this and fails the build if either committed page has drifted
# (.github/workflows/docs.yml).
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

# Cleared explicitly: the providers page's (not set)/(set) column reflects
# the *generating* machine's environment, not boast's own behaviour, and
# would otherwise make the page's content depend on whoever last ran this.
export GITHUB_TOKEN=
export ALTMETRIC_KEY=

providers_out=docs/site/src/reference/providers.md
table=$(cargo run --quiet -- providers)

cat > "$providers_out" <<EOF
<!--
GENERATED FILE — do not edit by hand.
Regenerate with docs/site/generate-reference.sh, run from the repo root.
-->

# Providers reference

Every Provider in boast's default registry, which Category it serves, whether it's
enabled by default, and what environment variable (if any) it needs a key in. This page
is generated from \`boast providers\` — the same command you can run yourself to check
what's obtainable before running \`about\`.

\`\`\`
$table
\`\`\`

An optional key raises a rate limit or unlocks extra Metrics but isn't required; a
required key means that Provider reports every Metric as not-applicable until it's set
(never as zero — see [ADR-0002](../design/0002-metric-honesty-model.md)).
EOF

cli_out=docs/site/src/reference/cli.md
{
    cat <<EOF
<!--
GENERATED FILE — do not edit by hand.
Regenerate with docs/site/generate-reference.sh, run from the repo root.
-->

# CLI reference

Every subcommand and flag, straight from \`--help\`.

## \`boast\`

\`\`\`
$(cargo run --quiet -- --help)
\`\`\`
EOF

    for cmd in about render diff providers init; do
        cat <<EOF

## \`boast $cmd\`

\`\`\`
$(cargo run --quiet -- "$cmd" --help)
\`\`\`
EOF
    done
} > "$cli_out"
