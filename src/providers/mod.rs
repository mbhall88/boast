//! The curated default set of Providers.

pub mod altmetric;
pub mod anaconda;
pub mod crates_io;
pub mod crossref;
pub mod dimensions;
pub mod docker_hub;
pub mod europe_pmc;
pub mod galaxy;
pub mod github;
pub mod homebrew;
pub mod openalex;
pub mod pypi;
pub mod quay;
pub mod wikipedia;

use crate::model::{Category, CohortSelection, Identity, PaperId};
use crate::provider::{KeyRequirement, Provider};
use crate::report::CATEGORY_ORDER;
use std::time::Duration;

/// Percent-encode a query-parameter value using RFC 3986's unreserved set.
/// Provider search APIs use punctuation as query syntax, so encoding the full
/// value keeps quotes, slashes, and operators data rather than URL syntax.
pub(crate) fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The Providers enabled by default. Later tickets add more here.
pub fn default_providers() -> Vec<Box<dyn Provider>> {
    default_providers_with_topic(None)
}

/// The default set, with an explicit GitHub Cohort `topic` override threaded to
/// the GitHub Provider. `None` lets each repo rank within every topic it declares.
pub fn default_providers_with_topic(topic: Option<String>) -> Vec<Box<dyn Provider>> {
    let selection = topic
        .map(|topic| CohortSelection::Exact(vec![topic]))
        .unwrap_or(CohortSelection::Declared);
    default_providers_with_github(selection, None)
}

pub fn default_providers_with_github(
    selection: CohortSelection,
    wait_limit: Option<Duration>,
) -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(openalex::OpenAlex),
        Box::new(crossref::Crossref),
        Box::new(dimensions::Dimensions),
        Box::new(europe_pmc::EuropePmc),
        Box::new(wikipedia::Wikipedia),
        Box::new(altmetric::Altmetric::new()),
        Box::new(github::GitHub::with_cohort_options(selection, wait_limit)),
        Box::new(crates_io::CratesIo),
        Box::new(anaconda::Anaconda),
        Box::new(pypi::Pypi),
        Box::new(homebrew::Homebrew),
        Box::new(docker_hub::DockerHub),
        Box::new(quay::Quay),
    ]
}

/// Providers that exist but are never fetched by a default run — a user
/// must explicitly opt in, via `--enable-provider <name>` or a Manifest's
/// `enable_providers` (issue #79). Galaxy is the first: a repository-level
/// CoDex lookup that most Projects have no presence in, so it would add a
/// mostly-empty request to every default run rather than a useful one.
pub fn optional_providers() -> Vec<Box<dyn Provider>> {
    vec![Box::new(galaxy::Galaxy)]
}

/// Resolve `--enable-provider`/Manifest `enable_providers` names into the
/// optional Providers they name. An unknown name is an error listing every
/// recognised optional Provider, since every caller treats it as a CLI usage
/// error (exit code 2) — there is no silent partial-enable.
pub fn resolve_optional_providers(names: &[String]) -> Result<Vec<Box<dyn Provider>>, String> {
    let all = optional_providers();
    let known: Vec<&str> = all.iter().map(|p| p.name()).collect();
    for name in names {
        if !known.contains(&name.as_str()) {
            return Err(format!(
                "unknown optional provider '{name}' (available: {})",
                known.join(", ")
            ));
        }
    }
    Ok(all
        .into_iter()
        .filter(|p| names.iter().any(|n| n == p.name()))
        .collect())
}

/// How many default Providers fetch metrics for a Paper Identity — the
/// per-work request cost `boast init --orcid` warns about before expanding a
/// large record. Computed from the real registry (never hard-coded), so the
/// stderr warning, the generated Manifest's header, and this count can never
/// drift apart.
pub fn paper_provider_count() -> usize {
    let sample = Identity::Paper(PaperId::Doi("10.0/0".to_string()));
    default_providers()
        .iter()
        .filter(|p| p.supports(&sample))
        .count()
}

/// Render `default` plus `optional` as a table for `boast providers`: name,
/// Category, default-enabled status, and key requirement (issue #16).
/// Grouped in [`CATEGORY_ORDER`], the same display order every other Report
/// uses.
///
/// Every Provider in `default` reads "yes" in the DEFAULT column and every
/// Provider in `optional` reads "no" (issue #79 — Galaxy is the first
/// non-default Provider, enabled only via `--enable-provider`/a Manifest's
/// `enable_providers`). A Provider named in both would be listed twice; the
/// two registries are disjoint by construction (see
/// [`crate::providers::optional_providers`]'s doc comment), so callers don't
/// need to de-duplicate here.
///
/// `key_is_set` looks up whether a named environment variable currently has
/// a non-empty value. Taking it as a parameter — the same seam pattern as
/// [`crate::transport::Transport`] — lets tests fake environment state
/// instead of mutating the real process environment, which is global and
/// shared across every test running in this process.
pub fn render_providers(
    default: &[Box<dyn Provider>],
    optional: &[Box<dyn Provider>],
    key_is_set: impl Fn(&str) -> bool,
) -> String {
    struct Row {
        name: &'static str,
        category: Category,
        is_default: bool,
        key: String,
    }

    let rows_from = |providers: &[Box<dyn Provider>],
                     is_default: bool,
                     key_is_set: &dyn Fn(&str) -> bool|
     -> Vec<Row> {
        providers
            .iter()
            .map(|p| Row {
                name: p.name(),
                category: p.category(),
                is_default,
                key: describe_key(p.key_requirement(), key_is_set),
            })
            .collect()
    };

    let mut rows: Vec<Row> = rows_from(default, true, &key_is_set);
    rows.extend(rows_from(optional, false, &key_is_set));
    rows.sort_by_key(|r| {
        CATEGORY_ORDER
            .iter()
            .position(|c| *c == r.category)
            .unwrap_or(usize::MAX)
    });

    const YES: &str = "yes";
    const NO: &str = "no";
    let w_name = "PROVIDER"
        .len()
        .max(rows.iter().map(|r| r.name.len()).max().unwrap_or(0));
    let w_category = "CATEGORY".len().max(
        rows.iter()
            .map(|r| r.category.label().len())
            .max()
            .unwrap_or(0),
    );
    let w_default = "DEFAULT".len().max(YES.len()).max(NO.len());

    let mut out = String::new();
    out.push_str(&format!(
        "{:<w_name$}  {:<w_category$}  {:<w_default$}  KEY\n",
        "PROVIDER", "CATEGORY", "DEFAULT",
    ));
    for r in rows {
        let default_col = if r.is_default { YES } else { NO };
        out.push_str(&format!(
            "{:<w_name$}  {:<w_category$}  {default_col:<w_default$}  {}\n",
            r.name,
            r.category.label(),
            r.key,
        ));
    }
    out
}

/// The `KEY` column's text for one Provider: `key_is_set` is only consulted
/// for `Optional`/`Required`, never for `None`, so a keyless Provider's row
/// never depends on environment state at all.
fn describe_key(requirement: KeyRequirement, key_is_set: &dyn Fn(&str) -> bool) -> String {
    let (label, env_var) = match requirement {
        KeyRequirement::None => return "none".to_string(),
        KeyRequirement::Optional { env_var } => ("optional", env_var),
        KeyRequirement::Required { env_var } => ("required", env_var),
    };
    let status = if key_is_set(env_var) {
        "set"
    } else {
        "not set"
    };
    format!("{label}: {env_var} ({status})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Identity, Outcome};
    use crate::transport::Transport;

    struct Fake {
        name: &'static str,
        category: Category,
        key_requirement: KeyRequirement,
    }

    impl Provider for Fake {
        fn name(&self) -> &'static str {
            self.name
        }

        fn category(&self) -> Category {
            self.category
        }

        fn supports(&self, _identity: &Identity) -> bool {
            true
        }

        fn fetch(&self, _identity: &Identity, _transport: &dyn Transport) -> Outcome {
            unimplemented!("render_providers never calls fetch")
        }

        fn key_requirement(&self) -> KeyRequirement {
            self.key_requirement
        }
    }

    fn fake(
        name: &'static str,
        category: Category,
        key_requirement: KeyRequirement,
    ) -> Box<dyn Provider> {
        Box::new(Fake {
            name,
            category,
            key_requirement,
        })
    }

    #[test]
    fn rows_are_grouped_in_the_shared_category_display_order() {
        let providers: Vec<Box<dyn Provider>> = vec![
            fake("z_downloads", Category::Downloads, KeyRequirement::None),
            fake("a_code", Category::Code, KeyRequirement::None),
            fake("m_attention", Category::Attention, KeyRequirement::None),
        ];
        let out = render_providers(&providers, &[], |_| false);
        let code_pos = out.find("a_code").unwrap();
        let downloads_pos = out.find("z_downloads").unwrap();
        let attention_pos = out.find("m_attention").unwrap();
        assert!(code_pos < downloads_pos);
        assert!(downloads_pos < attention_pos);
    }

    #[test]
    fn keyless_optional_and_required_are_clearly_distinguished() {
        let providers: Vec<Box<dyn Provider>> = vec![
            fake("keyless", Category::Citations, KeyRequirement::None),
            fake(
                "optional",
                Category::Code,
                KeyRequirement::Optional {
                    env_var: "OPT_TOKEN",
                },
            ),
            fake(
                "required",
                Category::Attention,
                KeyRequirement::Required { env_var: "REQ_KEY" },
            ),
        ];
        let out = render_providers(&providers, &[], |name| name == "OPT_TOKEN");

        let row = |needle: &str| out.lines().find(|l| l.contains(needle)).unwrap();
        assert!(row("keyless").contains("none"));
        assert!(row("optional").contains("optional: OPT_TOKEN (set)"));
        assert!(row("required").contains("required: REQ_KEY (not set)"));
    }

    #[test]
    fn a_default_provider_is_marked_yes_and_an_optional_one_is_marked_no() {
        let default: Vec<Box<dyn Provider>> = vec![fake("d", Category::Code, KeyRequirement::None)];
        let optional: Vec<Box<dyn Provider>> =
            vec![fake("o", Category::Usage, KeyRequirement::None)];
        let out = render_providers(&default, &optional, |_| false);

        let row = |needle: &str| out.lines().find(|l| l.contains(needle)).unwrap();
        assert!(row("d").contains("yes"));
        assert!(row("o").contains("no"));
    }

    #[test]
    fn paper_provider_count_matches_the_real_registrys_paper_supporting_providers() {
        // openalex, crossref, dimensions, europe_pmc, wikipedia, altmetric.
        assert_eq!(paper_provider_count(), 6);
    }

    #[test]
    fn reflects_the_real_registry_with_no_hard_coded_drift() {
        let default = default_providers();
        let optional = optional_providers();
        let out = render_providers(&default, &optional, |_| false);

        for p in default.iter().chain(&optional) {
            assert!(out.contains(p.name()), "missing {} in output", p.name());
        }
        // Header plus exactly one row per registered Provider, default or optional.
        assert_eq!(out.lines().count(), default.len() + optional.len() + 1);
        assert!(out.contains("required: ALTMETRIC_KEY"));
        assert!(out.contains("optional: GITHUB_TOKEN"));
    }

    #[test]
    fn the_real_registrys_galaxy_row_is_under_usage_and_marked_not_default() {
        let out = render_providers(&default_providers(), &optional_providers(), |_| false);
        let row = out.lines().find(|l| l.starts_with("galaxy ")).unwrap();
        assert!(row.contains("Usage"));
        assert!(row.contains(" no "));
        assert!(row.contains("none"));
    }

    #[test]
    fn galaxy_is_optional_never_part_of_the_default_registry() {
        assert!(!default_providers().iter().any(|p| p.name() == galaxy::NAME));
        assert!(optional_providers()
            .iter()
            .any(|p| p.name() == galaxy::NAME));
    }

    #[test]
    fn resolve_optional_providers_finds_a_known_name() {
        let resolved = resolve_optional_providers(&["galaxy".to_string()]).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name(), galaxy::NAME);
    }

    #[test]
    fn resolve_optional_providers_rejects_an_unknown_name_naming_the_known_ones() {
        let Err(err) = resolve_optional_providers(&["not-a-real-provider".to_string()]) else {
            panic!("expected an error");
        };
        assert!(err.contains("not-a-real-provider"));
        assert!(err.contains("galaxy"));
    }

    #[test]
    fn resolve_optional_providers_of_an_empty_list_is_empty() {
        assert!(resolve_optional_providers(&[]).unwrap().is_empty());
    }
}
