//! The curated default set of Providers.

pub mod altmetric;
pub mod anaconda;
pub mod crates_io;
pub mod crossref;
pub mod dimensions;
pub mod europe_pmc;
pub mod github;
pub mod homebrew;
pub mod openalex;
pub mod pypi;
pub mod wikipedia;

use crate::model::{Category, Identity, PaperId};
use crate::provider::{KeyRequirement, Provider};
use crate::report::CATEGORY_ORDER;

/// The Providers enabled by default. Later tickets add more here.
pub fn default_providers() -> Vec<Box<dyn Provider>> {
    default_providers_with_topic(None)
}

/// The default set, with an explicit GitHub Cohort `topic` override threaded to
/// the GitHub Provider. `None` lets each repo rank within every topic it declares.
pub fn default_providers_with_topic(topic: Option<String>) -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(openalex::OpenAlex),
        Box::new(crossref::Crossref),
        Box::new(dimensions::Dimensions),
        Box::new(europe_pmc::EuropePmc),
        Box::new(wikipedia::Wikipedia),
        Box::new(altmetric::Altmetric::new()),
        Box::new(github::GitHub::with_topic(topic)),
        Box::new(crates_io::CratesIo),
        Box::new(anaconda::Anaconda),
        Box::new(pypi::Pypi),
        Box::new(homebrew::Homebrew),
    ]
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

/// Render the given registry (usually [`default_providers`]) as a table for
/// `boast providers`: name, Category, default-enabled status, and key
/// requirement (issue #16). Grouped in [`CATEGORY_ORDER`], the same display
/// order every other Report uses.
///
/// Every Provider passed in is, by construction, part of the default set —
/// there's no separate optional/non-default registry yet (see the spec's
/// Out-of-Scope list) — so the DEFAULT column reads "yes" throughout; the
/// column exists so the answer stays visible once a non-default Provider
/// exists to contrast it with.
///
/// `key_is_set` looks up whether a named environment variable currently has
/// a non-empty value. Taking it as a parameter — the same seam pattern as
/// [`crate::transport::Transport`] — lets tests fake environment state
/// instead of mutating the real process environment, which is global and
/// shared across every test running in this process.
pub fn render_providers(
    providers: &[Box<dyn Provider>],
    key_is_set: impl Fn(&str) -> bool,
) -> String {
    struct Row {
        name: &'static str,
        category: Category,
        key: String,
    }

    let mut rows: Vec<Row> = providers
        .iter()
        .map(|p| Row {
            name: p.name(),
            category: p.category(),
            key: describe_key(p.key_requirement(), &key_is_set),
        })
        .collect();
    rows.sort_by_key(|r| {
        CATEGORY_ORDER
            .iter()
            .position(|c| *c == r.category)
            .unwrap_or(usize::MAX)
    });

    // Every row's DEFAULT column reads "yes" (see the doc comment above), but
    // its width is still computed rather than hand-padded to a literal, so a
    // future non-"yes" value can't silently fall out of alignment with the
    // header.
    const DEFAULT_COL: &str = "yes";
    let w_name = "PROVIDER"
        .len()
        .max(rows.iter().map(|r| r.name.len()).max().unwrap_or(0));
    let w_category = "CATEGORY".len().max(
        rows.iter()
            .map(|r| r.category.label().len())
            .max()
            .unwrap_or(0),
    );
    let w_default = "DEFAULT".len().max(DEFAULT_COL.len());

    let mut out = String::new();
    out.push_str(&format!(
        "{:<w_name$}  {:<w_category$}  {:<w_default$}  KEY\n",
        "PROVIDER", "CATEGORY", "DEFAULT",
    ));
    for r in rows {
        out.push_str(&format!(
            "{:<w_name$}  {:<w_category$}  {DEFAULT_COL:<w_default$}  {}\n",
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
fn describe_key(requirement: KeyRequirement, key_is_set: &impl Fn(&str) -> bool) -> String {
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
        let out = render_providers(&providers, |_| false);
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
        let out = render_providers(&providers, |name| name == "OPT_TOKEN");

        let row = |needle: &str| out.lines().find(|l| l.contains(needle)).unwrap();
        assert!(row("keyless").contains("none"));
        assert!(row("optional").contains("optional: OPT_TOKEN (set)"));
        assert!(row("required").contains("required: REQ_KEY (not set)"));
    }

    #[test]
    fn every_row_is_marked_default_enabled() {
        let providers: Vec<Box<dyn Provider>> =
            vec![fake("x", Category::Code, KeyRequirement::None)];
        let out = render_providers(&providers, |_| false);
        assert!(out
            .lines()
            .find(|l| l.contains("x"))
            .unwrap()
            .contains("yes"));
    }

    #[test]
    fn paper_provider_count_matches_the_real_registrys_paper_supporting_providers() {
        // openalex, crossref, dimensions, europe_pmc, wikipedia, altmetric.
        assert_eq!(paper_provider_count(), 6);
    }

    #[test]
    fn reflects_the_real_registry_with_no_hard_coded_drift() {
        let providers = default_providers();
        let out = render_providers(&providers, |_| false);

        for p in &providers {
            assert!(out.contains(p.name()), "missing {} in output", p.name());
        }
        // Header plus exactly one row per registered Provider.
        assert_eq!(out.lines().count(), providers.len() + 1);
        assert!(out.contains("required: ALTMETRIC_KEY"));
        assert!(out.contains("optional: GITHUB_TOKEN"));
    }
}
