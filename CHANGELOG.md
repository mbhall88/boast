# Changelog

## [0.1.1](https://github.com/mbhall88/boast/compare/0.1.0...0.1.1) (2026-07-31)


### Bug Fixes

* pin the Rust toolchain to stop release builds drifting from Cargo.toml's MSRV ([#53](https://github.com/mbhall88/boast/issues/53)) ([43e2d60](https://github.com/mbhall88/boast/commit/43e2d6099eef72a0bd2aa757990f9f9f4f60a384))

## 0.1.0 (2026-07-30)


### Features

* add `about` command fetching citation metrics into snapshots ([713bfd1](https://github.com/mbhall88/boast/commit/713bfd163883d546b1a3653af15228636524b8ec))
* add `boast providers` list/status subcommand ([b021a2e](https://github.com/mbhall88/boast/commit/b021a2e750147450b681d5119937c275696ea2df)), closes [#16](https://github.com/mbhall88/boast/issues/16)
* add Attention Category (OA status, Wikipedia mentions, optional Altmetric) ([957c084](https://github.com/mbhall88/boast/commit/957c0848891329717f6920cfd42a86b2e59fbf36)), closes [#15](https://github.com/mbhall88/boast/issues/15)
* add Bioconda, PyPI, and Homebrew download Providers ([c2e0535](https://github.com/mbhall88/boast/commit/c2e053543d5dc58ed58727bbe1f49ef0fe9edaa3)), closes [#12](https://github.com/mbhall88/boast/issues/12)
* add Crossref provider for bibliographic metadata and citations ([fdd1f7b](https://github.com/mbhall88/boast/commit/fdd1f7bfe7549b5b5bb8c6f25cb9295f081a322c))
* Add Dimensions badge Provider for citations, FCR, and RCR ([#29](https://github.com/mbhall88/boast/issues/29)) ([a93a5a9](https://github.com/mbhall88/boast/commit/a93a5a9c347bfd96186f175ebbeb6c6a54ffa263))
* add Dimensions recent_citations (last two calendar years) ([78fbaa3](https://github.com/mbhall88/boast/commit/78fbaa305c19f2b4d3a718033d3ddb393e818e92))
* add Downloads Rollup across compatible Windows ([#28](https://github.com/mbhall88/boast/issues/28)) ([0e0105e](https://github.com/mbhall88/boast/commit/0e0105e0050b83c495e52b27fd976003ffb739fb))
* Add Europe PMC citations Provider ([#30](https://github.com/mbhall88/boast/issues/30)) ([684cdbb](https://github.com/mbhall88/boast/commit/684cdbbb2e43b5901d6bb6f0e1605d07bdc2fa58))
* add GitHub repository provider for the Code category ([1b0e85d](https://github.com/mbhall88/boast/commit/1b0e85dbb3d18b2fd24503fc532c4b0d18b00dfc))
* add package identity and crates.io download Provider ([9e25b71](https://github.com/mbhall88/boast/commit/9e25b7124b140a7ef22b19d0e6cc9f2772d771ac)), closes [#11](https://github.com/mbhall88/boast/issues/11)
* add TOML Manifest, batch about, and init/--save ([ddcd6d8](https://github.com/mbhall88/boast/commit/ddcd6d894735ab62c5c2db277aae13b4821affde)), closes [#14](https://github.com/mbhall88/boast/issues/14)
* **cli:** accept bare owner/name repos and harden the about shim ([0b0dad6](https://github.com/mbhall88/boast/commit/0b0dad69096d253949286e83d85bc29fcde55e22))
* **cli:** read identifiers from a file and group the report by identity ([ec091ff](https://github.com/mbhall88/boast/commit/ec091ff7e5e0b2d3b38bb0d5f1ca366f435b5c48))
* diff two Snapshots -&gt; growth (offline) ([#33](https://github.com/mbhall88/boast/issues/33)) ([a3a7a8b](https://github.com/mbhall88/boast/commit/a3a7a8be31dda28ba083740c415f8719ca2a41d8)), closes [#5](https://github.com/mbhall88/boast/issues/5)
* expand a researcher's ORCID iD into a Manifest via `boast init --orcid` ([#51](https://github.com/mbhall88/boast/issues/51)) ([aac9860](https://github.com/mbhall88/boast/commit/aac98603745c1354a31f1ac6591a35217f722a7e))
* generalize Bioconda provider to any Anaconda.org channel ([1f529ae](https://github.com/mbhall88/boast/commit/1f529ae1d6d7be2dfcd7b08851d4248e2c520c7b))
* parallel fetching, bounded and serial-per-host, with a -j/--threads knob ([#50](https://github.com/mbhall88/boast/issues/50)) ([53c6be5](https://github.com/mbhall88/boast/commit/53c6be51a836fa35a26c76bf8537ea976c9dd880))
* rank repos by stars within their GitHub topic Cohorts ([e87917a](https://github.com/mbhall88/boast/commit/e87917a68b2046e2c5894e6e8a512f46618c6ce7)), closes [#10](https://github.com/mbhall88/boast/issues/10)
* render Snapshot -&gt; Markdown + prose (offline) ([#32](https://github.com/mbhall88/boast/issues/32)) ([121e4ef](https://github.com/mbhall88/boast/commit/121e4ef467836d53963bdbefa86ec56aa9767f00))
* structured logging with tracing and -v/-q verbosity ([3c3fedc](https://github.com/mbhall88/boast/commit/3c3fedc6d8370735ffac408cc0238f3ee5b29ade))
* Transport hardening — retry/backoff + polite-pool identification ([#31](https://github.com/mbhall88/boast/issues/31)) ([d6dfdff](https://github.com/mbhall88/boast/commit/d6dfdffcd617cdf8126d94cc3369628e08bd067c)), closes [#3](https://github.com/mbhall88/boast/issues/3)


### Bug Fixes

* include GitHub release downloads in the Downloads Rollup ([bf716ba](https://github.com/mbhall88/boast/commit/bf716ba80b24656bf2d790838c324ada11bf2470))
* **pypi:** don't overstate precision of pypistats' last-month bucket ([9fd22a6](https://github.com/mbhall88/boast/commit/9fd22a629896466dbf2382e0accef7388cb4364c))
* tighten conda parse-gate to match its own comment, refresh stale docs ([d8c01e0](https://github.com/mbhall88/boast/commit/d8c01e02ce4c5db86271def2e6fa0a1ce493ca27))
