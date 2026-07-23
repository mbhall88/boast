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

/// Classify a fetched HTTP status other than 200 into an Outcome (ADR-0002:
/// `NotApplicable`/`Failed` are never coerced to a value). Returns `None` for
/// a 200 status, meaning the caller should parse the body itself.
///
/// `provider_name` is the display name interpolated into the rate-limit/
/// server-error/unexpected-status wording, which is identical across every
/// Provider; `not_found` is the Provider's own NotApplicable wording for a
/// 404, since that one varies in shape (e.g. "not found in OpenAlex" vs
/// "repository not found on GitHub"). Only called from within this crate's
/// own Providers, so it's not part of the library's public surface.
pub(crate) fn classify_status(
    status: u16,
    provider_name: &str,
    not_found: &str,
) -> Option<Outcome> {
    match status {
        200 => None,
        404 => Some(Outcome::NotApplicable {
            note: not_found.to_string(),
        }),
        429 => Some(Outcome::Failed {
            error: format!("rate limited by {provider_name} (429)"),
        }),
        s if crate::transport::is_transient_status(s) => Some(Outcome::Failed {
            error: format!("{provider_name} server error ({s})"),
        }),
        s => Some(Outcome::Failed {
            error: format!("unexpected {provider_name} status ({s})"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_status_yields_none() {
        assert_eq!(classify_status(200, "Example", "not found"), None);
    }

    #[test]
    fn not_found_uses_the_given_wording_verbatim() {
        assert_eq!(
            classify_status(404, "Example", "repository not found on Example"),
            Some(Outcome::NotApplicable {
                note: "repository not found on Example".into()
            })
        );
    }

    #[test]
    fn rate_limit_server_error_and_unexpected_status_use_shared_templates() {
        assert_eq!(
            classify_status(429, "Example", "not found"),
            Some(Outcome::Failed {
                error: "rate limited by Example (429)".into()
            })
        );
        assert_eq!(
            classify_status(503, "Example", "not found"),
            Some(Outcome::Failed {
                error: "Example server error (503)".into()
            })
        );
        assert_eq!(
            classify_status(599, "Example", "not found"),
            Some(Outcome::Failed {
                error: "Example server error (599)".into()
            })
        );
        assert_eq!(
            classify_status(301, "Example", "not found"),
            Some(Outcome::Failed {
                error: "unexpected Example status (301)".into()
            })
        );
    }
}
