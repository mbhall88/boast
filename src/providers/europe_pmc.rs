//! Europe PMC Provider: paper citation counts and indexed scholarly repository
//! mentions from a life-sciences source, reported independently from
//! OpenAlex/Crossref. Works from a DOI or a PMID. The search API always answers
//! 200; its own `hitCount` distinguishes "no record" (NotApplicable) from a
//! record with a genuinely-recorded zero citations (`Count(0)`, never coerced
//! to absence).

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{Category, Identity, Metric, MetricValue, Outcome, PaperId, Window};
use crate::provider::{classify_status, Provider};
use crate::providers::percent_encode;
use crate::transport::Transport;

const API_BASE: &str = "https://www.ebi.ac.uk/europepmc/webservices/rest/search";

pub struct EuropePmc;

#[derive(Debug, Deserialize)]
struct EuropePmcResponse {
    #[serde(rename = "hitCount")]
    hit_count: u64,
    #[serde(rename = "resultList")]
    result_list: EuropePmcResultList,
}

#[derive(Debug, Deserialize)]
struct EuropePmcResultList {
    result: Vec<EuropePmcResult>,
}

#[derive(Debug, Deserialize)]
struct EuropePmcResult {
    #[serde(rename = "citedByCount")]
    cited_by_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EuropePmcRepoSearchResponse {
    #[serde(rename = "hitCount")]
    hit_count: Option<u64>,
}

impl EuropePmc {
    /// The Europe PMC search query for a paper Identity: `DOI:...`, or
    /// `EXT_ID:... AND SRC:MED` (Europe PMC's fielded search for a PubMed ID
    /// — a bare `PMID:` field is not recognised by the search API).
    fn query(identity: &Identity) -> Option<String> {
        match identity {
            Identity::Paper(PaperId::Doi(d)) => Some(format!("DOI:{d}")),
            Identity::Paper(PaperId::Pmid(p)) => Some(format!("EXT_ID:{p} AND SRC:MED")),
            Identity::Repo(_) | Identity::Package(_) => None,
        }
    }

    /// The Europe PMC full-text search query for a Repo Identity. The phrase
    /// is derived from the canonical repository identity so accepted CLI
    /// spellings all produce the same query.
    fn repo_query(identity: &Identity) -> Option<String> {
        let canonical = identity.canonical();
        let path = canonical.strip_prefix("github:")?;
        Some(format!("\"github.com/{path}\" AND (SRC:MED OR SRC:PPR)"))
    }

    fn classify_repo(body: &str, url: &str, canonical: &str) -> Outcome {
        let response: EuropePmcRepoSearchResponse = match serde_json::from_str(body) {
            Ok(response) => response,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Europe PMC response: {e}"),
                }
            }
        };
        let count = match response.hit_count {
            Some(count) => count,
            None => {
                return Outcome::Failed {
                    error: "unexpected Europe PMC response: missing hitCount".into(),
                }
            }
        };

        Outcome::Values {
            metrics: vec![Metric {
                name: "mentions".into(),
                category: Category::Attention,
                value: MetricValue::Count(count),
                window: Window::Cumulative,
                provider: "europe_pmc".into(),
                identity: canonical.into(),
                as_of: OffsetDateTime::now_utc(),
                source: url.into(),
                note: Some(
                    "indexed full-text search estimate, not a formal citation or verified literal URL count; partial coverage concentrated in life-sciences literature; self-mentions are included; journal article/preprint versions may be counted separately"
                        .into(),
                ),
            }],
            metadata: None,
        }
    }

    fn classify(body: &str, url: &str, canonical: &str) -> Outcome {
        let parsed: EuropePmcResponse = match serde_json::from_str(body) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: format!("unexpected Europe PMC response: {e}"),
                }
            }
        };

        if parsed.hit_count == 0 {
            return Outcome::NotApplicable {
                note: "not found in Europe PMC".into(),
            };
        }

        let cited_by_count = parsed
            .result_list
            .result
            .first()
            .and_then(|r| r.cited_by_count);

        let citations = match cited_by_count {
            Some(c) => c,
            None => {
                return Outcome::NotApplicable {
                    note: "Europe PMC record has no citation count".into(),
                }
            }
        };

        Outcome::Values {
            metrics: vec![Metric {
                name: "citations".into(),
                category: Category::Citations,
                value: MetricValue::Count(citations),
                window: Window::Cumulative,
                provider: "europe_pmc".into(),
                identity: canonical.into(),
                as_of: OffsetDateTime::now_utc(),
                source: url.into(),
                note: Some("citation count from Europe PMC".into()),
            }],
            metadata: None,
        }
    }
}

impl Provider for EuropePmc {
    fn name(&self) -> &'static str {
        "europe_pmc"
    }

    fn category(&self) -> Category {
        Category::Citations
    }

    fn supports(&self, identity: &Identity) -> bool {
        matches!(identity, Identity::Paper(_) | Identity::Repo(_))
    }

    fn fetch(&self, identity: &Identity, transport: &dyn Transport) -> Outcome {
        if let Some(query) = Self::repo_query(identity) {
            let url = format!(
                "{API_BASE}?query={}&pageSize=1&resultType=idlist&format=json",
                percent_encode(&query)
            );
            let canonical = identity.canonical();
            let resp = match transport.get(&url) {
                Ok(r) => r,
                Err(e) => {
                    return Outcome::Failed {
                        error: e.to_string(),
                    }
                }
            };

            return match classify_status(
                resp.status,
                "Europe PMC",
                "repository search not found in Europe PMC",
            ) {
                Some(outcome) => outcome,
                None => Self::classify_repo(&resp.body, &url, &canonical),
            };
        }

        let query = match Self::query(identity) {
            Some(q) => q,
            None => {
                return Outcome::NotApplicable {
                    note: "Europe PMC only supports papers and repositories".into(),
                }
            }
        };
        let encoded = query.replace(' ', "+");
        let url = format!("{API_BASE}?query={encoded}&format=json");
        let canonical = identity.canonical();

        let resp = match transport.get(&url) {
            Ok(r) => r,
            Err(e) => {
                return Outcome::Failed {
                    error: e.to_string(),
                }
            }
        };

        match classify_status(resp.status, "Europe PMC", "not found in Europe PMC") {
            Some(outcome) => outcome,
            None => Self::classify(&resp.body, &url, &canonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RepoId;
    use crate::transport::{MockTransport, TransportError};

    fn doi() -> Identity {
        Identity::Paper(PaperId::Doi("10.1371/journal.pbio.1002195".into()))
    }

    fn pmid() -> Identity {
        Identity::Paper(PaperId::Pmid("26151137".into()))
    }

    fn repo() -> Identity {
        Identity::Repo(RepoId::parse("mbhall88/rasusa").unwrap())
    }

    #[test]
    fn parses_citation_count_from_cassette() {
        let cassette = include_str!("../../tests/cassettes/europe_pmc_search.json");
        let t =
            MockTransport::new().on("ebi.ac.uk/europepmc/webservices/rest/search", 200, cassette);

        let metrics = match EuropePmc.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 1);
        let cites = &metrics[0];
        assert_eq!(cites.name, "citations");
        assert_eq!(cites.value, MetricValue::Count(581));
        assert_eq!(cites.provider, "europe_pmc");
        assert_eq!(cites.category, Category::Citations);
        assert_eq!(cites.window, Window::Cumulative);
        assert_eq!(cites.identity, "doi:10.1371/journal.pbio.1002195");
    }

    #[test]
    fn works_from_a_pmid_and_encodes_the_space_in_its_query() {
        let body = r#"{"hitCount":1,"resultList":{"result":[{"citedByCount":581}]}}"#;
        let t = MockTransport::new().on("query=EXT_ID:26151137+AND+SRC:MED", 200, body);

        let metrics = match EuropePmc.fetch(&pmid(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics[0].value, MetricValue::Count(581));
        assert_eq!(metrics[0].identity, "pmid:26151137");
    }

    #[test]
    fn record_found_without_a_citation_count_is_not_applicable_not_zero() {
        // hitCount > 0 but the matched result carries no citedByCount field
        // (seen for some non-MED Europe PMC records) — still absence, not 0.
        let body = r#"{"hitCount":1,"resultList":{"result":[{"pmid":"1"}]}}"#;
        let t = MockTransport::new().on("europepmc", 200, body);
        assert!(matches!(
            EuropePmc.fetch(&doi(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn zero_hit_count_is_not_applicable_not_zero() {
        // Europe PMC answers 200 even for an unknown DOI; absence is only
        // visible via hitCount, never a 404 status.
        let body = r#"{"hitCount":0,"resultList":{"result":[]}}"#;
        let t = MockTransport::new().on("europepmc", 200, body);
        assert!(matches!(
            EuropePmc.fetch(&doi(), &t),
            Outcome::NotApplicable { .. }
        ));
    }

    #[test]
    fn a_genuinely_uncited_record_is_a_real_zero() {
        // Distinct from the absent-record case above: a real record with
        // citedByCount 0 must not be conflated with "not found" (ADR-0002).
        let body = r#"{"hitCount":1,"resultList":{"result":[{"citedByCount":0}]}}"#;
        let t = MockTransport::new().on("europepmc", 200, body);
        let metrics = match EuropePmc.fetch(&doi(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics[0].value, MetricValue::Count(0));
    }

    #[test]
    fn supports_papers_and_repos_not_packages() {
        assert!(EuropePmc.supports(&doi()));
        assert!(EuropePmc.supports(&pmid()));
        assert!(EuropePmc.supports(&repo()));
    }

    #[test]
    fn server_error_and_transport_error_are_failed() {
        let t500 = MockTransport::new().on("europepmc", 503, "");
        assert!(matches!(
            EuropePmc.fetch(&doi(), &t500),
            Outcome::Failed { .. }
        ));

        let terr = MockTransport::new().on_error("europepmc", TransportError::Timeout);
        assert!(matches!(
            EuropePmc.fetch(&doi(), &terr),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn malformed_body_is_failed() {
        let t = MockTransport::new().on("europepmc", 200, "not json");
        assert!(matches!(
            EuropePmc.fetch(&doi(), &t),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn searches_repo_full_text_and_reports_an_attention_mention_count() {
        let cassette = include_str!("../../tests/cassettes/europe_pmc_repo_search.json");
        let t = MockTransport::new().on(
            "query=%22github.com%2Fmbhall88%2Frasusa%22%20AND%20%28SRC%3AMED%20OR%20SRC%3APPR%29",
            200,
            cassette,
        );

        let metrics = match EuropePmc.fetch(&repo(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };

        assert_eq!(metrics.len(), 1);
        let mention = &metrics[0];
        assert_eq!(mention.name, "mentions");
        assert_eq!(mention.category, Category::Attention);
        assert_eq!(mention.value, MetricValue::Count(12));
        assert_eq!(mention.window, Window::Cumulative);
        assert_eq!(mention.provider, "europe_pmc");
        assert_eq!(mention.identity, "github:mbhall88/rasusa");
        assert!(mention.source.contains("SRC%3AMED%20OR%20SRC%3APPR"));
        let note = mention.note.as_deref().unwrap();
        assert!(note.contains("indexed full-text search estimate"));
        assert!(note.contains("not a formal citation or verified literal URL count"));
        assert!(note.contains("partial coverage"));
        assert!(note.contains("life-sciences literature"));
        assert!(note.contains("self-mentions are included"));
        assert!(note.contains("versions may be counted separately"));
    }

    #[test]
    fn a_repo_search_with_no_hits_is_a_real_zero() {
        let body = r#"{"hitCount":0,"resultList":{"result":[]}}"#;
        let t = MockTransport::new().on("query=%22github.com%2F", 200, body);
        let metrics = match EuropePmc.fetch(&repo(), &t) {
            Outcome::Values { metrics, .. } => metrics,
            other => panic!("expected Values, got {other:?}"),
        };
        assert_eq!(metrics[0].value, MetricValue::Count(0));
    }

    #[test]
    fn a_repo_search_without_hit_count_is_failed() {
        let body = r#"{"resultList":{"result":[]}}"#;
        let t = MockTransport::new().on("query=%22github.com%2F", 200, body);
        assert!(matches!(
            EuropePmc.fetch(&repo(), &t),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn a_repo_search_transport_failure_is_failed() {
        let t = MockTransport::new().on_error("query=%22github.com%2F", TransportError::Timeout);
        assert!(matches!(
            EuropePmc.fetch(&repo(), &t),
            Outcome::Failed { .. }
        ));
    }
}
