# Getting started

## Install

### Homebrew (macOS/Linux)

```
brew install mbhall88/tap/boast
```

### Shell script (macOS/Linux)

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mbhall88/boast/releases/latest/download/boast-installer.sh | sh
```

### PowerShell (Windows)

```
powershell -ExecutionPolicy Bypass -c "irm https://github.com/mbhall88/boast/releases/latest/download/boast-installer.ps1 | iex"
```

### Docker

```
docker run --rm ghcr.io/mbhall88/boast:latest about 10.1234/journal.xyz
```

### cargo

```
cargo install boast --locked
```

### From source

```
git clone https://github.com/mbhall88/boast
cd boast
cargo install --path . --locked
```

Prebuilt binaries (Linux x86_64/aarch64/armv7 — all statically linked, musl — and macOS
x86_64/aarch64, Windows x86_64) are attached to every
[GitHub Release](https://github.com/mbhall88/boast/releases).

## Your first Report

Point `boast about` at anything with a DOI, and it prints a Report straight to your
terminal — no config, no Manifest, no account:

```
boast about 10.1371/journal.pbio.1002195
```

If the piece of software also has a code repository and/or is published on a package
registry, tell boast about those too, so every Category has something to report on:

```
boast about --repo samtools/samtools \
            --package conda:bioconda/samtools \
            10.1371/journal.pbio.1002195
```

Every run above already wrote a Snapshot — `boast about` saves one to `snapshots/` by
default (pass `--no-save` to skip that and only print). That first Snapshot is the start
of a history you can [diff against later](./guides/ci-snapshots.md):

```
boast render snapshots/<the-file-it-just-wrote>.json --format markdown
```

From here, see [Concepts](./concepts.md) for the vocabulary, or jump straight to a
[Guide](./guides/index.md) that matches your situation.
