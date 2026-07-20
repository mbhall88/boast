//! Command-line surface. Subcommands (`about`, and later `render`/`diff`/…),
//! plus a bare-identifier shortcut so `boast 10.1234/x` means `boast about
//! 10.1234/x`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use time::macros::format_description;

use crate::model::{Identity, Project, RepoId};
use crate::orchestrator;
use crate::providers::default_providers;
use crate::report::render_terminal;
use crate::transport::UreqTransport;

/// Subcommands recognised as the first positional token. Anything else is
/// treated as a bare identifier for `about`.
const SUBCOMMANDS: &[&str] = &["about", "help"];

#[derive(Debug, Parser)]
#[command(
    name = "boast",
    version,
    about = "Gather reach and impact metrics for a research tool or paper into dated, quotable snapshots."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Fetch metrics for a Project, write a Snapshot, and print a report.
    About(AboutArgs),
}

#[derive(Debug, Args)]
pub struct AboutArgs {
    /// Identifiers: a DOI, a doi.org URL, `pmid:12345678`, or a github.com repo URL.
    #[arg(value_name = "IDENTIFIER")]
    pub targets: Vec<String>,

    /// A GitHub repository as `owner/name` (repeatable).
    #[arg(short = 'r', long = "repo", value_name = "OWNER/NAME")]
    pub repos: Vec<String>,

    /// Directory to write the Snapshot into.
    #[arg(short = 'd', long, default_value = "snapshots", value_name = "DIR")]
    pub snapshot_dir: PathBuf,

    /// Print the report but do not write a Snapshot file.
    #[arg(short = 'n', long)]
    pub no_save: bool,
}

/// Insert an implicit `about` when the first positional token is a bare
/// identifier rather than a known subcommand.
fn normalize_args(mut args: Vec<String>) -> Vec<String> {
    // If there's no positional (only help/version flags), leave args for clap.
    if let Some(idx) = args.iter().skip(1).position(|a| !a.starts_with('-')) {
        let token = &args[idx + 1];
        if !SUBCOMMANDS.contains(&token.as_str()) {
            args.insert(idx + 1, "about".to_string());
        }
    }
    args
}

/// Entry point used by `main`. Returns the process exit code.
pub fn main() -> i32 {
    let args = normalize_args(std::env::args().collect());
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            // clap prints help/errors itself; use its own exit code.
            e.print().ok();
            return if e.use_stderr() { 2 } else { 0 };
        }
    };

    match cli.command {
        Command::About(args) => run_about(args),
    }
}

fn run_about(args: AboutArgs) -> i32 {
    // Parse every identifier up front; a bad one is a usage error.
    let mut identities = Vec::new();
    for target in &args.targets {
        match Identity::parse(target) {
            Ok(id) => identities.push(id),
            Err(e) => {
                eprintln!("error: {e}");
                return 2;
            }
        }
    }
    for repo in &args.repos {
        match RepoId::parse(repo) {
            Ok(id) => identities.push(Identity::Repo(id)),
            Err(e) => {
                eprintln!("error: {e}");
                return 2;
            }
        }
    }

    if identities.is_empty() {
        eprintln!("error: no identifiers given (pass a DOI/PMID/repo, or --repo owner/name)");
        return 2;
    }

    // Warn loudly if repo metrics were requested without a token to raise limits.
    let wants_repo = identities.iter().any(|i| matches!(i, Identity::Repo(_)));
    if wants_repo
        && std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .is_none()
    {
        eprintln!(
            "warning: GITHUB_TOKEN is not set — GitHub metrics use the unauthenticated \
             rate limit (60 requests/hour) and may be throttled. Set GITHUB_TOKEN to raise it."
        );
    }

    let project = Project::new(identities);
    let transport = UreqTransport::new();
    let providers = default_providers();

    let snapshot = orchestrator::run(&project, &providers, &transport);

    print!("{}", render_terminal(&snapshot));

    if !args.no_save {
        match write_snapshot(&snapshot, &args.snapshot_dir) {
            Ok(path) => println!("\nSnapshot written to {}", path.display()),
            Err(e) => {
                eprintln!("error: could not write snapshot: {e}");
                return 2;
            }
        }
    }

    if snapshot.has_failures() {
        1
    } else {
        0
    }
}

fn write_snapshot(
    snapshot: &crate::model::Snapshot,
    dir: &std::path::Path,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let fmt = format_description!("[year][month][day]T[hour][minute][second]Z");
    let stamp = snapshot
        .created_at
        .format(&fmt)
        .unwrap_or_else(|_| "snapshot".to_string());
    let path = dir.join(format!("{stamp}.json"));
    let json = serde_json::to_string_pretty(snapshot).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(args: &[&str]) -> Vec<String> {
        normalize_args(args.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn bare_identifier_gets_implicit_about() {
        assert_eq!(norm(&["boast", "10.1/x"]), vec!["boast", "about", "10.1/x"]);
        assert_eq!(
            norm(&["boast", "pmid:42"]),
            vec!["boast", "about", "pmid:42"]
        );
    }

    #[test]
    fn explicit_about_is_untouched() {
        assert_eq!(
            norm(&["boast", "about", "10.1/x"]),
            vec!["boast", "about", "10.1/x"]
        );
    }

    #[test]
    fn flags_before_identifier_are_handled() {
        // A leading global flag shouldn't be treated as the positional.
        assert_eq!(norm(&["boast", "--version"]), vec!["boast", "--version"]);
    }

    #[test]
    fn cli_parses_bare_and_explicit_forms_equivalently() {
        let bare = Cli::try_parse_from(norm(&["boast", "10.1/x"])).unwrap();
        let Command::About(a) = bare.command;
        assert_eq!(a.targets, vec!["10.1/x".to_string()]);
    }
}
