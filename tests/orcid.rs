//! End-to-end test of `init --orcid`'s pipeline: a recorded ORCID `/works`
//! response drives expansion into a Manifest whose header, optional
//! commented-out unidentified-work blocks, and `[[project]]` entries are
//! exactly what `boast::cli::run_init_orcid` assembles and writes — proven
//! here by reassembling them the same way and confirming the result still
//! parses as a valid Manifest (ADR-0006: the generated file must remain
//! runnable as written, with or without `--include-unidentified`).

use time::OffsetDateTime;

use boast::manifest::Manifest;
use boast::model::{OrcidId, PaperId};
use boast::orcid::{self, OrcidWork};
use boast::providers::paper_provider_count;
use boast::transport::MockTransport;

fn orcid() -> OrcidId {
    OrcidId::parse("0000-0002-1825-0097").unwrap()
}

fn transport() -> MockTransport {
    let cassette = include_str!("cassettes/orcid_works.json");
    MockTransport::new().on(
        "pub.orcid.org/v3.0/0000-0002-1825-0097/works",
        200,
        cassette,
    )
}

#[test]
fn expansion_yields_one_project_per_identified_work_and_counts_the_rest() {
    let t = transport();
    let expansion = orcid::expand(&orcid(), &t).unwrap();

    let identified: Vec<PaperId> = expansion.identified().cloned().collect();
    assert_eq!(identified.len(), 3);
    assert_eq!(expansion.unidentified().count(), 1);

    let manifest = Manifest::from_orcid_works(&identified, None);
    assert_eq!(manifest.projects.len(), 3);
    for (i, project) in manifest.projects.iter().enumerate() {
        assert_eq!(project.identities.len(), 1, "project {i}");
    }
}

#[test]
fn the_generated_manifest_parses_and_runs_without_include_unidentified() {
    let t = transport();
    let expansion = orcid::expand(&orcid(), &t).unwrap();
    let identified: Vec<PaperId> = expansion.identified().cloned().collect();
    let unidentified_count = expansion.unidentified().count();

    let manifest = Manifest::from_orcid_works(&identified, None);
    let toml_str = manifest.to_toml_string().unwrap();

    let header = orcid::render_header(
        &[orcid()],
        expansion.works.len(),
        unidentified_count,
        OffsetDateTime::now_utc(),
        paper_provider_count(),
    );

    let full = format!("{header}{toml_str}");

    // Inert to the parser: no `--include-unidentified` block was appended,
    // so nothing but comments and real `[[project]]` tables are present.
    let parsed = Manifest::parse(&full).unwrap();
    assert_eq!(parsed, manifest);
    assert_eq!(parsed.projects.len(), 3);
    for (i, project) in parsed.projects.iter().enumerate() {
        project.to_project(i).unwrap();
    }
}

#[test]
fn the_generated_manifest_still_parses_and_runs_with_include_unidentified() {
    let t = transport();
    let expansion = orcid::expand(&orcid(), &t).unwrap();
    let identified: Vec<PaperId> = expansion.identified().cloned().collect();
    let unidentified: Vec<&OrcidWork> = expansion.unidentified().collect();

    let manifest = Manifest::from_orcid_works(&identified, None);
    let toml_str = manifest.to_toml_string().unwrap();

    let header = orcid::render_header(
        &[orcid()],
        expansion.works.len(),
        unidentified.len(),
        OffsetDateTime::now_utc(),
        paper_provider_count(),
    );
    let block = orcid::render_unidentified_block(&unidentified);

    let full = format!("{header}{block}\n{toml_str}");

    // The commented block is entirely `#`-prefixed, so the parser sees only
    // the real `[[project]]` entries — the skipped work never becomes a
    // broken or placeholder identity (ADR-0006).
    let parsed = Manifest::parse(&full).unwrap();
    assert_eq!(parsed, manifest);
    assert!(full.contains("A Poster on Nothing in Particular"));
    assert!(!full.contains("doi:FIXME"));
}
