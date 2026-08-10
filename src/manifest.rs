//! The optional TOML **Manifest**: a committable, secret-free file listing
//! one or more Projects for repeatable or batch `boast about` runs (see
//! `CONTEXT.md`). Holds identities and optional exact or prioritised Cohort
//! selections; runtime-only waiting and secrets never enter it.
//! Secrets (`GITHUB_TOKEN`, …) are read from the environment, never the
//! Manifest — any unrecognised field (e.g. a mistakenly-pasted token) is a
//! hard parse error rather than a silent drop, so a secret can never end up
//! committed without the author noticing. Never hand-authored: `boast init`
//! and `about --save` generate one from a run's identities.

use serde::{Deserialize, Serialize};

use crate::model::{CohortSelection, Identity, IdentityError, PaperId, Project};

/// A Manifest is one or more Projects, each its own `[[project]]` TOML table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(rename = "project")]
    pub projects: Vec<ManifestProject>,
}

/// One Project's worth of Manifest settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestProject {
    /// Identity strings in the same syntax accepted on the command line, e.g.
    /// `doi:10.x/y`, `github:owner/name`, `crates:boast`.
    pub identities: Vec<String>,
    /// Legacy singular exact GitHub Cohort selection. Read-compatible only;
    /// newly generated Manifests write `topics`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub topic: Option<String>,
    /// Exact Cohorts, in collection order. Newly generated Manifests use this
    /// plural form; `topic` remains read-compatible for older files.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub topics: Vec<String>,
    /// Declared Cohorts to attempt first, in order, before the remainder.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub priority_topics: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("invalid manifest: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("could not serialise manifest: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("project {index}: {source}")]
    Identity {
        index: usize,
        #[source]
        source: IdentityError,
    },
    #[error("manifest has no [[project]] entries")]
    Empty,
    #[error("project {index}: topic, topics, and priority_topics are mutually exclusive")]
    ConflictingTopics { index: usize },
}

impl Manifest {
    /// Parse a Manifest from TOML text. Rejects (never silently drops) any
    /// field outside the declared Manifest schema, so a manifest can never carry a
    /// secret without the parse failing loudly.
    pub fn parse(toml_str: &str) -> Result<Manifest, ManifestError> {
        let manifest: Manifest = toml::from_str(toml_str)?;
        if manifest.projects.is_empty() {
            return Err(ManifestError::Empty);
        }
        for (index, project) in manifest.projects.iter().enumerate() {
            let modes = usize::from(project.topic.is_some())
                + usize::from(!project.topics.is_empty())
                + usize::from(!project.priority_topics.is_empty());
            if modes > 1 {
                return Err(ManifestError::ConflictingTopics { index });
            }
        }
        Ok(manifest)
    }

    /// Serialise back to TOML text, e.g. for `init`/`about --save`.
    pub fn to_toml_string(&self) -> Result<String, ManifestError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Build a Manifest for one Project, reflecting a run's identities and
    /// Cohort selection — the shared basis for both `init` and `about --save`.
    pub fn from_identities(identities: &[Identity], topic: Option<&str>) -> Manifest {
        let selection = topic
            .map(|topic| CohortSelection::Exact(vec![topic.to_string()]))
            .unwrap_or(CohortSelection::Declared);
        Self::from_identities_with_selection(identities, selection)
    }

    pub fn from_identities_with_selection(
        identities: &[Identity],
        selection: CohortSelection,
    ) -> Manifest {
        let (topics, priority_topics) = selection_fields(selection);
        Manifest {
            projects: vec![ManifestProject {
                identities: identities.iter().map(|id| id.canonical()).collect(),
                topic: None,
                topics,
                priority_topics,
            }],
        }
    }

    /// Build a Manifest from Paper Identities expanded from ORCID, one Project per
    /// work (ADR-0006) — `init --orcid`'s counterpart to `from_identities`'s
    /// single Project.
    pub fn from_orcid_works(works: &[PaperId], topic: Option<&str>) -> Manifest {
        let selection = topic
            .map(|topic| CohortSelection::Exact(vec![topic.to_string()]))
            .unwrap_or(CohortSelection::Declared);
        Self::from_orcid_works_with_selection(works, selection)
    }

    pub fn from_orcid_works_with_selection(
        works: &[PaperId],
        selection: CohortSelection,
    ) -> Manifest {
        let (topics, priority_topics) = selection_fields(selection);
        Manifest {
            projects: works
                .iter()
                .map(|id| ManifestProject {
                    identities: vec![Identity::Paper(id.clone()).canonical()],
                    topic: None,
                    topics: topics.clone(),
                    priority_topics: priority_topics.clone(),
                })
                .collect(),
        }
    }
}

impl ManifestProject {
    pub fn cohort_selection(&self) -> CohortSelection {
        if !self.topics.is_empty() {
            CohortSelection::Exact(self.topics.clone())
        } else if !self.priority_topics.is_empty() {
            CohortSelection::Priority(self.priority_topics.clone())
        } else if let Some(topic) = &self.topic {
            CohortSelection::Exact(vec![topic.clone()])
        } else {
            CohortSelection::Declared
        }
    }

    /// Parse this entry's identity strings into a Project. `index` is this
    /// entry's 0-based position among `Manifest.projects`, named on error so
    /// a bad identity in a large Manifest is easy to locate.
    pub fn to_project(&self, index: usize) -> Result<Project, ManifestError> {
        let identities = self
            .identities
            .iter()
            .map(|s| Identity::parse(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| ManifestError::Identity { index, source })?;
        Ok(Project::new(identities))
    }
}

fn selection_fields(selection: CohortSelection) -> (Vec<String>, Vec<String>) {
    match selection {
        CohortSelection::Declared => (Vec::new(), Vec::new()),
        CohortSelection::Exact(topics) => (topics, Vec::new()),
        CohortSelection::Priority(topics) => (Vec::new(), topics),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PaperId;

    #[test]
    fn parses_a_minimal_single_project_manifest() {
        let toml_str = r#"
            [[project]]
            identities = ["doi:10.1371/journal.pbio.1002195"]
        "#;
        let manifest = Manifest::parse(toml_str).unwrap();
        assert_eq!(manifest.projects.len(), 1);
        assert_eq!(
            manifest.projects[0].identities,
            vec!["doi:10.1371/journal.pbio.1002195".to_string()]
        );
        assert_eq!(manifest.projects[0].topic, None);
    }

    #[test]
    fn parses_multiple_projects_with_a_topic() {
        let toml_str = r#"
            [[project]]
            identities = ["doi:10.1/x"]

            [[project]]
            identities = ["github:samtools/samtools", "conda:bioconda/samtools"]
            topic = "bioinformatics"
        "#;
        let manifest = Manifest::parse(toml_str).unwrap();
        assert_eq!(manifest.projects.len(), 2);
        assert_eq!(
            manifest.projects[1].topic.as_deref(),
            Some("bioinformatics")
        );
    }

    #[test]
    fn rejects_a_manifest_with_no_projects() {
        assert!(matches!(Manifest::parse(""), Err(ManifestError::Parse(_))));
        assert!(matches!(
            Manifest::parse("project = []"),
            Err(ManifestError::Empty)
        ));
    }

    #[test]
    fn rejects_a_secret_slipped_into_a_project_table() {
        let toml_str = r#"
            [[project]]
            identities = ["doi:10.1/x"]
            github_token = "ghp_shouldnotbehere"
        "#;
        let err = Manifest::parse(toml_str).unwrap_err();
        assert!(matches!(err, ManifestError::Parse(_)));
    }

    #[test]
    fn rejects_a_secret_at_the_manifest_top_level() {
        let toml_str = r#"
            altmetric_key = "shouldnotbehere"

            [[project]]
            identities = ["doi:10.1/x"]
        "#;
        let err = Manifest::parse(toml_str).unwrap_err();
        assert!(matches!(err, ManifestError::Parse(_)));
    }

    #[test]
    fn a_bad_identity_names_its_project_index() {
        let toml_str = r#"
            [[project]]
            identities = ["doi:10.1/x"]

            [[project]]
            identities = ["not a valid identity"]
        "#;
        let manifest = Manifest::parse(toml_str).unwrap();
        let err = manifest.projects[1].to_project(1).unwrap_err();
        assert!(matches!(err, ManifestError::Identity { index: 1, .. }));
        assert!(err.to_string().contains("project 1"));
    }

    #[test]
    fn to_project_parses_every_identity_kind() {
        let toml_str = r#"
            [[project]]
            identities = ["doi:10.1/x", "github:owner/name", "crates:boast"]
        "#;
        let manifest = Manifest::parse(toml_str).unwrap();
        let project = manifest.projects[0].to_project(0).unwrap();
        assert_eq!(project.identities.len(), 3);
        assert_eq!(
            project.identities[0],
            Identity::Paper(PaperId::Doi("10.1/x".into()))
        );
    }

    #[test]
    fn from_identities_round_trips_through_toml() {
        let identities = vec![
            Identity::parse("10.1371/journal.pbio.1002195").unwrap(),
            Identity::parse("crates:boast").unwrap(),
        ];
        let manifest = Manifest::from_identities(&identities, Some("bioinformatics"));
        let toml_str = manifest.to_toml_string().unwrap();

        let parsed = Manifest::parse(&toml_str).unwrap();
        assert_eq!(parsed, manifest);
        assert_eq!(parsed.projects[0].topics, ["bioinformatics"]);

        let project = parsed.projects[0].to_project(0).unwrap();
        assert_eq!(project.identities, identities);
    }

    #[test]
    fn from_orcid_works_writes_one_project_per_work() {
        let works = vec![
            PaperId::Doi("10.1/x".into()),
            PaperId::Pmid("31234567".into()),
        ];
        let manifest = Manifest::from_orcid_works(&works, Some("bioinformatics"));
        assert_eq!(manifest.projects.len(), 2);
        assert_eq!(
            manifest.projects[0].identities,
            vec!["doi:10.1/x".to_string()]
        );
        assert_eq!(
            manifest.projects[1].identities,
            vec!["pmid:31234567".to_string()]
        );
        assert_eq!(manifest.projects[1].topics, ["bioinformatics"]);

        let toml_str = manifest.to_toml_string().unwrap();
        let parsed = Manifest::parse(&toml_str).unwrap();
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn from_identities_omits_topic_when_absent() {
        let identities = vec![Identity::parse("10.1/x").unwrap()];
        let manifest = Manifest::from_identities(&identities, None);
        let toml_str = manifest.to_toml_string().unwrap();
        assert!(!toml_str.contains("topic"));
    }

    #[test]
    fn legacy_topic_parses_while_plural_exact_and_priority_fields_round_trip() {
        let legacy = Manifest::parse(
            r#"[[project]]
identities = ["github:owner/repo"]
topic = "legacy"
"#,
        )
        .unwrap();
        assert_eq!(
            legacy.projects[0].cohort_selection(),
            CohortSelection::Exact(vec!["legacy".into()])
        );

        for selection in [
            CohortSelection::Exact(vec!["second".into(), "first".into()]),
            CohortSelection::Priority(vec!["important".into(), "next".into()]),
        ] {
            let manifest = Manifest::from_identities_with_selection(
                &[Identity::parse("github:owner/repo").unwrap()],
                selection.clone(),
            );
            let toml = manifest.to_toml_string().unwrap();
            let parsed = Manifest::parse(&toml).unwrap();
            assert_eq!(parsed.projects[0].cohort_selection(), selection);
            assert!(!toml.contains("\ntopic ="), "{toml}");
        }
    }

    #[test]
    fn manifest_rejects_combined_topic_modes() {
        for conflicting in [
            "topic = \"legacy\"\ntopics = [\"exact\"]",
            "topic = \"legacy\"\npriority_topics = [\"first\"]",
            "topics = [\"exact\"]\npriority_topics = [\"first\"]",
        ] {
            let toml =
                format!("[[project]]\nidentities = [\"github:owner/repo\"]\n{conflicting}\n");
            let error = Manifest::parse(&toml).unwrap_err();
            assert!(error.to_string().contains("mutually exclusive"), "{error}");
        }
    }
}
