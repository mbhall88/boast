//! The Provider trait: a source-specific component that, given an Identity,
//! fetches Metrics from one external service through the [`Transport`] seam.
//! Providers are pluggable; the default set is assembled in [`crate::providers`].

use crate::model::{Category, Identity, Outcome};
use crate::transport::Transport;

pub trait Provider {
    /// Stable short name recorded in Snapshots (e.g. "openalex").
    fn name(&self) -> &'static str;

    /// The Category this Provider's Metrics belong to.
    fn category(&self) -> Category;

    /// Whether this Provider understands the given Identity kind.
    fn supports(&self, identity: &Identity) -> bool;

    /// Fetch metrics for one Identity, returning exactly one Outcome.
    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome;
}
