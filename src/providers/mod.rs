//! The curated default set of Providers.

pub mod anaconda;
pub mod crates_io;
pub mod crossref;
pub mod dimensions;
pub mod europe_pmc;
pub mod github;
pub mod homebrew;
pub mod openalex;
pub mod pypi;

use crate::provider::Provider;

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
        Box::new(github::GitHub::with_topic(topic)),
        Box::new(crates_io::CratesIo),
        Box::new(anaconda::Anaconda),
        Box::new(pypi::Pypi),
        Box::new(homebrew::Homebrew),
    ]
}
