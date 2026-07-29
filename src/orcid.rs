//! Expands an ORCID iD into the Paper Identities it has claimed, via ORCID's
//! own public works API (`pub.orcid.org/v3.0/{orcid}/works` — keyless,
//! self-curated). An ORCID is an input *expander*, not an Identity
//! (ADR-0006): this module only ever produces `PaperId` values for
//! `boast init --orcid` to write into a Manifest; no Provider ever sees an
//! ORCID.

use serde::Deserialize;
use time::OffsetDateTime;

use crate::model::{OrcidId, PaperId};
use crate::transport::{Transport, TransportError};

const API_BASE: &str = "https://pub.orcid.org/v3.0";

/// One work from an ORCID record, classified by whether it carries a DOI or
/// PMID (and so can become a Paper Identity) or neither (an **unidentified
/// work** — a poster, talk, dataset, or abstract that ADR-0006 says must be
/// skipped, never guessed at).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrcidWork {
    Identified {
        id: PaperId,
    },
    Unidentified {
        title: Option<String>,
        year: Option<i32>,
        kind: Option<String>,
    },
}

/// The result of expanding one ORCID iD: every work in the record (ORCID's
/// own dedup groups), in the order ORCID returned them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrcidExpansion {
    pub works: Vec<OrcidWork>,
}

impl OrcidExpansion {
    pub fn identified(&self) -> impl Iterator<Item = &PaperId> {
        self.works.iter().filter_map(|w| match w {
            OrcidWork::Identified { id } => Some(id),
            OrcidWork::Unidentified { .. } => None,
        })
    }

    pub fn unidentified(&self) -> impl Iterator<Item = &OrcidWork> {
        self.works
            .iter()
            .filter(|w| matches!(w, OrcidWork::Unidentified { .. }))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OrcidError {
    #[error("{0}")]
    Transport(#[from] TransportError),
    #[error("no ORCID record found for {0}")]
    NotFound(OrcidId),
    #[error("ORCID responded with status {status} for {orcid}")]
    Status { orcid: OrcidId, status: u16 },
    #[error("unexpected ORCID response for {orcid}: {source}")]
    Parse {
        orcid: OrcidId,
        source: serde_json::Error,
    },
}

/// Fetch and classify every work in `orcid`'s ORCID record. `Accept:
/// application/json` is required: ORCID's public API answers with XML by
/// default, not JSON, without it.
pub fn expand(orcid: &OrcidId, transport: &dyn Transport) -> Result<OrcidExpansion, OrcidError> {
    let url = format!("{API_BASE}/{orcid}/works");
    let resp = transport.get_with_headers(&url, &[("Accept", "application/json")])?;
    match resp.status {
        200 => {}
        404 => return Err(OrcidError::NotFound(orcid.clone())),
        status => {
            return Err(OrcidError::Status {
                orcid: orcid.clone(),
                status,
            })
        }
    }

    let parsed: OrcidWorksResponse =
        serde_json::from_str(&resp.body).map_err(|source| OrcidError::Parse {
            orcid: orcid.clone(),
            source,
        })?;

    Ok(OrcidExpansion {
        works: parsed.group.into_iter().map(classify_group).collect(),
    })
}

#[derive(Debug, Deserialize)]
struct OrcidWorksResponse {
    #[serde(default)]
    group: Vec<OrcidGroup>,
}

#[derive(Debug, Deserialize)]
struct OrcidGroup {
    #[serde(rename = "external-ids")]
    external_ids: OrcidExternalIds,
    #[serde(rename = "work-summary", default)]
    work_summary: Vec<OrcidWorkSummary>,
}

#[derive(Debug, Deserialize)]
struct OrcidExternalIds {
    #[serde(rename = "external-id", default)]
    external_id: Vec<OrcidExternalId>,
}

#[derive(Debug, Deserialize)]
struct OrcidExternalId {
    #[serde(rename = "external-id-type")]
    external_id_type: String,
    #[serde(rename = "external-id-value")]
    external_id_value: String,
    #[serde(rename = "external-id-relationship")]
    external_id_relationship: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrcidWorkSummary {
    title: Option<OrcidWorkTitle>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "publication-date")]
    publication_date: Option<OrcidPublicationDate>,
}

#[derive(Debug, Deserialize)]
struct OrcidWorkTitle {
    title: Option<OrcidTitleValue>,
}

#[derive(Debug, Deserialize)]
struct OrcidTitleValue {
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrcidPublicationDate {
    year: Option<OrcidYear>,
}

#[derive(Debug, Deserialize)]
struct OrcidYear {
    value: Option<String>,
}

/// Classify one ORCID dedup group into a single work. Only external IDs
/// whose relationship is `self` name *this* work — anything else (e.g. an
/// ISSN `part-of` its journal) is bibliographic context, not this work's own
/// identifier. A DOI is preferred over a PMID when a work carries both, since
/// more Providers key off DOI.
fn classify_group(group: OrcidGroup) -> OrcidWork {
    let self_ids: Vec<&OrcidExternalId> = group
        .external_ids
        .external_id
        .iter()
        .filter(|e| e.external_id_relationship.as_deref() == Some("self"))
        .collect();

    let doi = self_ids
        .iter()
        .find(|e| e.external_id_type.eq_ignore_ascii_case("doi"));
    let pmid = self_ids
        .iter()
        .find(|e| e.external_id_type.eq_ignore_ascii_case("pmid"));

    let id = doi
        .map(|e| PaperId::Doi(e.external_id_value.clone()))
        .or_else(|| pmid.map(|e| PaperId::Pmid(e.external_id_value.clone())));

    match id {
        Some(id) => OrcidWork::Identified { id },
        None => {
            let summary = group.work_summary.first();
            OrcidWork::Unidentified {
                title: summary
                    .and_then(|s| s.title.as_ref())
                    .and_then(|t| t.title.as_ref())
                    .and_then(|v| v.value.clone()),
                year: summary
                    .and_then(|s| s.publication_date.as_ref())
                    .and_then(|d| d.year.as_ref())
                    .and_then(|y| y.value.as_ref())
                    .and_then(|v| v.parse::<i32>().ok()),
                kind: summary.and_then(|s| s.kind.clone()),
            }
        }
    }
}

/// The header comment prepended to a Manifest generated by `init --orcid`
/// (`toml::to_string_pretty` can't emit comments — ADR-0006's consequences).
/// Names every ORCID expanded, the total/skipped counts, and the per-work
/// request cost, so the caution is visible the moment the user opens the
/// file to prune it. `total` and `skipped` are already summed across every
/// `orcid` passed.
pub fn render_header(
    orcids: &[OrcidId],
    total: usize,
    skipped: usize,
    generated_at: OffsetDateTime,
    provider_count: usize,
) -> String {
    let flags = orcids
        .iter()
        .map(|o| format!("--orcid {o}"))
        .collect::<Vec<_>>()
        .join(" ");
    let written = total - skipped;
    let date = generated_at
        .format(time::macros::format_description!("[year]-[month]-[day]"))
        .expect("a fixed literal date format never fails");
    format!(
        "# Generated by `boast init {flags}` on {date} — {total} work{ws}.\n\
         # {written} written below; {skipped} skipped (no DOI/PMID).\n\
         # Re-run with --include-unidentified to list the skipped works for completion.\n\
         # Each remaining work costs ~{provider_count} API requests when you run `boast about`.\n",
        ws = plural(total),
    )
}

/// Commented-out `[[project]]` blocks for every unidentified work, so a user
/// who knows the missing DOI can uncomment and fill it in. Never a
/// placeholder identity like `doi:FIXME` — that would make the freshly
/// generated Manifest fail to parse on the very next `boast about`
/// (ADR-0006's consequences).
pub fn render_unidentified_block(unidentified: &[&OrcidWork]) -> String {
    let n = unidentified.len();
    let mut out = format!(
        "# ─── {n} work{s} with no DOI or PMID in your ORCID record ───\n\
         # These can't be measured as-is. If you know the DOI, uncomment the block and\n\
         # fill it in — and consider adding it to your ORCID record so it's there next time.\n",
        s = plural(n),
    );
    for work in unidentified {
        let OrcidWork::Unidentified { title, year, kind } = work else {
            continue;
        };
        out.push_str("#\n");
        out.push_str(&format!(
            "# \"{title}\" ({year}, {kind})\n",
            title = title.as_deref().unwrap_or("untitled work"),
            year = year
                .map(|y| y.to_string())
                .unwrap_or_else(|| "n.d.".to_string()),
            kind = kind.as_deref().unwrap_or("unknown"),
        ));
        out.push_str("# [[project]]\n");
        out.push_str("# identities = [\"doi:\"]\n");
    }
    out
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    fn orcid() -> OrcidId {
        OrcidId::parse("0000-0002-1825-0097").unwrap()
    }

    #[test]
    fn expands_a_recorded_orcid_response_into_identified_and_unidentified_works() {
        let cassette = include_str!("../tests/cassettes/orcid_works.json");
        let t = MockTransport::new().on(
            "pub.orcid.org/v3.0/0000-0002-1825-0097/works",
            200,
            cassette,
        );

        let expansion = expand(&orcid(), &t).unwrap();

        assert_eq!(expansion.works.len(), 4);
        let identified: Vec<_> = expansion.identified().collect();
        assert_eq!(identified.len(), 3);
        assert_eq!(identified[0], &PaperId::Doi("10.5555/12345680".to_string()));
        let unidentified: Vec<_> = expansion.unidentified().collect();
        assert_eq!(unidentified.len(), 1);
    }

    #[test]
    fn a_work_with_both_doi_and_pmid_prefers_the_doi() {
        let cassette = include_str!("../tests/cassettes/orcid_works.json");
        let t = MockTransport::new().on(
            "pub.orcid.org/v3.0/0000-0002-1825-0097/works",
            200,
            cassette,
        );

        let expansion = expand(&orcid(), &t).unwrap();
        let identified: Vec<_> = expansion.identified().collect();
        assert!(identified.contains(&&PaperId::Doi("10.5555/999900001111".to_string())));
        assert!(!identified.contains(&&PaperId::Pmid("26151137".to_string())));
    }

    #[test]
    fn a_pmid_only_work_is_identified_by_pmid() {
        let cassette = include_str!("../tests/cassettes/orcid_works.json");
        let t = MockTransport::new().on(
            "pub.orcid.org/v3.0/0000-0002-1825-0097/works",
            200,
            cassette,
        );

        let expansion = expand(&orcid(), &t).unwrap();
        let identified: Vec<_> = expansion.identified().collect();
        assert!(identified.contains(&&PaperId::Pmid("31234567".to_string())));
    }

    #[test]
    fn a_non_self_external_id_is_never_mistaken_for_the_works_own_identifier() {
        // The cassette's fourth group has only a `part-of` ISSN, no `self`
        // doi/pmid — it must be unidentified, not misread as identified by
        // the ISSN's relationship.
        let cassette = include_str!("../tests/cassettes/orcid_works.json");
        let t = MockTransport::new().on(
            "pub.orcid.org/v3.0/0000-0002-1825-0097/works",
            200,
            cassette,
        );

        let expansion = expand(&orcid(), &t).unwrap();
        let unidentified: Vec<_> = expansion.unidentified().collect();
        assert_eq!(unidentified.len(), 1);
        assert!(matches!(
            unidentified[0],
            OrcidWork::Unidentified { title: Some(t), .. } if t == "A Poster on Nothing in Particular"
        ));
    }

    #[test]
    fn not_found_orcid_is_a_dedicated_error() {
        let t = MockTransport::new().on("pub.orcid.org/v3.0/0000-0002-1825-0097/works", 404, "");
        assert!(matches!(expand(&orcid(), &t), Err(OrcidError::NotFound(_))));
    }

    #[test]
    fn server_error_and_transport_error_are_reported() {
        let t500 = MockTransport::new().on("pub.orcid.org/v3.0/0000-0002-1825-0097/works", 503, "");
        assert!(matches!(
            expand(&orcid(), &t500),
            Err(OrcidError::Status { status: 503, .. })
        ));

        let terr = MockTransport::new().on_error(
            "pub.orcid.org/v3.0/0000-0002-1825-0097/works",
            crate::transport::TransportError::Timeout,
        );
        assert!(matches!(
            expand(&orcid(), &terr),
            Err(OrcidError::Transport(_))
        ));
    }

    #[test]
    fn malformed_body_is_a_parse_error() {
        let t = MockTransport::new().on(
            "pub.orcid.org/v3.0/0000-0002-1825-0097/works",
            200,
            "not json",
        );
        assert!(matches!(
            expand(&orcid(), &t),
            Err(OrcidError::Parse { .. })
        ));
    }

    #[test]
    fn render_header_computes_written_from_total_minus_skipped_and_names_every_orcid() {
        let a = OrcidId::parse("0000-0002-1825-0097").unwrap();
        let b = OrcidId::parse("0000-0001-2345-6789").unwrap();
        let header = render_header(
            &[a, b],
            190,
            40,
            OffsetDateTime::from_unix_timestamp(1785283200).unwrap(),
            6,
        );
        assert!(header.contains("--orcid 0000-0002-1825-0097 --orcid 0000-0001-2345-6789"));
        assert!(header.contains("190 works"));
        assert!(header.contains("150 written below; 40 skipped"));
        assert!(header.contains("~6 API requests"));
        assert!(header.contains("boast about"));
        assert!(header.contains("--include-unidentified"));
    }

    #[test]
    fn render_header_singular_work_gets_no_trailing_s() {
        let header = render_header(
            &[orcid()],
            1,
            0,
            OffsetDateTime::from_unix_timestamp(1785283200).unwrap(),
            6,
        );
        assert!(header.contains("1 work.") && !header.contains("1 works"));
    }

    #[test]
    fn render_unidentified_block_lists_title_year_and_type_as_a_commented_project() {
        let work = OrcidWork::Unidentified {
            title: Some("A Poster on Nothing in Particular".to_string()),
            year: Some(2020),
            kind: Some("conference-poster".to_string()),
        };
        let out = render_unidentified_block(&[&work]);
        assert!(out.contains("1 work with no DOI or PMID"));
        assert!(out.contains("\"A Poster on Nothing in Particular\" (2020, conference-poster)"));
        assert!(out.contains("# [[project]]"));
        assert!(out.contains("# identities = [\"doi:\"]"));
        // Every line must be commented, so the block is inert to the parser.
        assert!(out.lines().all(|l| l.starts_with('#')));
    }

    #[test]
    fn render_unidentified_block_falls_back_for_missing_metadata() {
        let work = OrcidWork::Unidentified {
            title: None,
            year: None,
            kind: None,
        };
        let out = render_unidentified_block(&[&work]);
        assert!(out.contains("\"untitled work\" (n.d., unknown)"));
    }
}
