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

generated_header="<!--
GENERATED FILE — do not edit by hand.
Regenerate with docs/site/generate-reference.sh, run from the repo root.
-->"

providers_out=docs/site/src/reference/providers.md
table=$(cargo run --quiet -- providers)

cat > "$providers_out" <<EOF
$generated_header

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
top_help=$(cargo run --quiet -- --help)
# Subcommands come straight out of `--help`'s own "Commands:" block (minus
# clap's built-in `help`, which has nothing of its own worth a page) rather
# than being hand-listed here — otherwise a new subcommand could ship
# without ever getting a reference section, and this script's own drift
# check would have nothing to catch it with.
cmds=$(awk '/^Commands:/{f=1; next} /^$/{f=0} f{print $1}' <<<"$top_help" | grep -v '^help$')

{
    cat <<EOF
$generated_header

# CLI reference

Every subcommand and flag, straight from \`--help\`.

## \`boast\`

\`\`\`
$top_help
\`\`\`
EOF

    for cmd in $cmds; do
        cat <<EOF

## \`boast $cmd\`

\`\`\`
$(cargo run --quiet -- "$cmd" --help)
\`\`\`
EOF
    done
} > "$cli_out"
