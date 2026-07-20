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

    /// Increase logging verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short = 'v', long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Silence all logging except errors.
    #[arg(short = 'q', long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Fetch metrics for a Project, write a Snapshot, and print a report.
    About(AboutArgs),
}

#[derive(Debug, Args)]
pub struct AboutArgs {
    /// Identifiers: a DOI, doi.org URL, `pmid:12345678`, a github.com URL, or `owner/name`.
    #[arg(value_name = "IDENTIFIER")]
    pub targets: Vec<String>,

    /// A GitHub repository as `owner/name` (alternative to a positional; repeatable).
    #[arg(short = 'r', long = "repo", value_name = "OWNER/NAME")]
    pub repos: Vec<String>,

    /// Directory to write the Snapshot into.
    #[arg(short = 'd', long, default_value = "snapshots", value_name = "DIR")]
    pub snapshot_dir: PathBuf,

    /// Print the report but do not write a Snapshot file.
    #[arg(short = 'n', long)]
    pub no_save: bool,
}

/// Recognised global flags — all valueless — skipped when locating the
/// subcommand so they can precede an implicit `about`.
fn is_global_flag(token: &str) -> bool {
    matches!(
        token,
        "-q" | "--quiet" | "-h" | "--help" | "-V" | "--version" | "--verbose"
    ) || (token.starts_with("-v") && token[1..].chars().all(|c| c == 'v'))
}

/// Insert an implicit `about` subcommand when the user gave a bare identifier
/// (or an `about`-specific flag) instead of a subcommand. Leading global flags
/// are skipped, so `boast -q owner/repo` and `boast --repo owner/name` both work.
fn normalize_args(mut args: Vec<String>) -> Vec<String> {
    let mut i = 1;
    while i < args.len() {
        if is_global_flag(&args[i]) {
            i += 1;
            continue;
        }
        // First non-global token: either a subcommand (leave it) or the start
        // of `about`'s arguments (insert `about` before it).
        if !SUBCOMMANDS.contains(&args[i].as_str()) {
            args.insert(i, "about".to_string());
        }
        break;
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

    init_logging(cli.verbose, cli.quiet);

    match cli.command {
        Command::About(args) => run_about(args),
    }
}

/// Install the tracing subscriber writing to stderr. `RUST_LOG` overrides the
/// level chosen by `-v`/`-q`.
fn init_logging(verbose: u8, quiet: bool) {
    use tracing_subscriber::EnvFilter;

    // `boast_level` controls our own crate; `global_level` caps noisy
    // dependencies (ureq, rustls, …) so `-vv` doesn't spew TLS internals.
    let (boast_level, global_level) = if quiet {
        ("error", "error")
    } else {
        match verbose {
            0 => ("warn", "warn"),
            1 => ("info", "warn"),
            2 => ("debug", "warn"),
            _ => ("trace", "warn"),
        }
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("boast={boast_level},{global_level}")));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .with_target(false)
        .with_writer(std::io::stderr)
        .compact()
        .init();
}

fn run_about(args: AboutArgs) -> i32 {
    // Parse every identifier up front; a bad one is a usage error.
    let mut identities = Vec::new();
    for target in &args.targets {
        match Identity::parse(target) {
            Ok(id) => identities.push(id),
            Err(e) => {
                tracing::error!("{e}");
                return 2;
            }
        }
    }
    for repo in &args.repos {
        match RepoId::parse(repo) {
            Ok(id) => identities.push(Identity::Repo(id)),
            Err(e) => {
                tracing::error!("{e}");
                return 2;
            }
        }
    }

    if identities.is_empty() {
        tracing::error!("no identifiers given (pass a DOI/PMID/repo, or --repo owner/name)");
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
        tracing::warn!(
            "GITHUB_TOKEN is not set; GitHub metrics use the unauthenticated rate limit \
             (60 requests/hour) and may be throttled. Set GITHUB_TOKEN to raise it."
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
                tracing::error!("could not write snapshot: {e}");
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
        // Bare owner/name repo shorthand, optionally alongside a DOI.
        assert_eq!(
            norm(&["boast", "owner/repo", "10.1/x"]),
            vec!["boast", "about", "owner/repo", "10.1/x"]
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
    fn about_flags_without_subcommand_get_implicit_about() {
        // `--repo`'s value must not be mistaken for the subcommand position.
        assert_eq!(
            norm(&["boast", "--repo", "owner/name"]),
            vec!["boast", "about", "--repo", "owner/name"]
        );
        assert_eq!(
            norm(&["boast", "-d", "out", "10.1/x"]),
            vec!["boast", "about", "-d", "out", "10.1/x"]
        );
    }

    #[test]
    fn global_flags_are_skipped_and_preserved() {
        // Global flags may precede either an implicit or explicit subcommand.
        assert_eq!(
            norm(&["boast", "-q", "10.1/x"]),
            vec!["boast", "-q", "about", "10.1/x"]
        );
        assert_eq!(
            norm(&["boast", "-vv", "about", "10.1/x"]),
            vec!["boast", "-vv", "about", "10.1/x"]
        );
        // Only global flags → leave for clap (help/version).
        assert_eq!(norm(&["boast", "--version"]), vec!["boast", "--version"]);
    }

    #[test]
    fn cli_parses_bare_and_explicit_forms_equivalently() {
        let bare = Cli::try_parse_from(norm(&["boast", "10.1/x"])).unwrap();
        let Command::About(a) = bare.command;
        assert_eq!(a.targets, vec!["10.1/x".to_string()]);
    }
}
