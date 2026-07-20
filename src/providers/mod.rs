//! The curated default set of Providers.

pub mod crossref;
pub mod openalex;

use crate::provider::Provider;

/// The Providers enabled by default. Later tickets add more here.
pub fn default_providers() -> Vec<Box<dyn Provider>> {
    vec![Box::new(openalex::OpenAlex), Box::new(crossref::Crossref)]
}
