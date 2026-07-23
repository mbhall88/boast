//! Command-line surface. Subcommands (`about`, `render`, `diff`), plus a
//! bare-identifier shortcut so `boast 10.1234/x` means `boast about
//! 10.1234/x`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use time::macros::format_description;

use crate::diff;
use crate::model::{Identity, IdentityError, PackageId, Project, RepoId, Snapshot};
use crate::orchestrator;
use crate::providers::default_providers_with_topic;
use crate::report::{render_markdown, render_prose, render_terminal};
use crate::transport::{RetryingTransport, UreqTransport};

/// Subcommands recognised as the first positional token. Anything else is
/// treated as a bare identifier for `about`.
const SUBCOMMANDS: &[&str] = &["about", "render", "diff", "help"];

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

    /// Render a stored Snapshot as Markdown or prose. Never touches the
    /// network (ADR-0001) — offline and deterministic for a given Snapshot.
    Render(RenderArgs),

    /// Compare two stored Snapshots and report the change in each shared
    /// Metric. Never touches the network (ADR-0001).
    Diff(DiffArgs),
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// The earlier Snapshot JSON file.
    #[arg(value_name = "OLD")]
    pub old: PathBuf,

    /// The later Snapshot JSON file.
    #[arg(value_name = "NEW")]
    pub new: PathBuf,
}

#[derive(Debug, Args)]
pub struct RenderArgs {
    /// Path to a Snapshot JSON file written by `boast about`.
    #[arg(value_name = "SNAPSHOT")]
    pub snapshot: PathBuf,

    /// Output format.
    #[arg(short = 'f', long = "format", value_enum, default_value_t = Format::Markdown)]
    pub format: Format,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Format {
    /// Category-grouped Markdown Report — the primary saved artifact.
    Markdown,
    /// A single grant-ready sentence summarising the headline Metrics.
    Prose,
}

#[derive(Debug, Args)]
pub struct AboutArgs {
    /// Identifiers: a DOI, doi.org URL, `pmid:12345678`, a github.com URL,
    /// `owner/name`, or a package as `registry:name` (e.g. `crates:boast`).
    #[arg(value_name = "IDENTIFIER")]
    pub targets: Vec<String>,

    /// A GitHub repository as `owner/name` (alternative to a positional; repeatable).
    #[arg(short = 'r', long = "repo", value_name = "OWNER/NAME")]
    pub repos: Vec<String>,

    /// A distribution package as `registry:name`, e.g. `crates:boast`
    /// (alternative to a positional; repeatable).
    #[arg(short = 'p', long = "package", value_name = "REGISTRY:NAME")]
    pub packages: Vec<String>,

    /// Read identifiers from a file (one per line; `#` comments and blank lines
    /// ignored). Use `-` for stdin. Repeatable.
    #[arg(short = 'f', long = "from-file", value_name = "FILE")]
    pub from_file: Vec<PathBuf>,

    /// GitHub topic to rank repositories within, overriding each repo's own
    /// declared topics (see the Cohort disclaimer in the report).
    #[arg(short = 't', long = "topic", value_name = "TOPIC")]
    pub topic: Option<String>,

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
        Command::Render(args) => run_render(args),
        Command::Diff(args) => run_diff(args),
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

/// Parse each `value` with `parse`, wrap successes into an Identity with
/// `wrap`, and push them onto `identities`. A bad identifier is a usage
/// error, so this returns the CLI exit code on the first failure.
fn extend_identities<T>(
    identities: &mut Vec<Identity>,
    values: &[String],
    parse: impl Fn(&str) -> Result<T, IdentityError>,
    wrap: impl Fn(T) -> Identity,
) -> Result<(), i32> {
    for value in values {
        match parse(value) {
            Ok(parsed) => identities.push(wrap(parsed)),
            Err(e) => {
                tracing::error!("{e}");
                return Err(2);
            }
        }
    }
    Ok(())
}

fn run_about(args: AboutArgs) -> i32 {
    // Parse every identifier up front; a bad one is a usage error.
    let mut identities = Vec::new();
    if let Err(code) = extend_identities(&mut identities, &args.targets, Identity::parse, |id| id) {
        return code;
    }
    if let Err(code) =
        extend_identities(&mut identities, &args.repos, RepoId::parse, Identity::Repo)
    {
        return code;
    }
    if let Err(code) = extend_identities(
        &mut identities,
        &args.packages,
        PackageId::parse,
        Identity::Package,
    ) {
        return code;
    }
    for path in &args.from_file {
        let content = match read_source(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("could not read {}: {e}", display_source(path));
                return 2;
            }
        };
        match parse_identifier_lines(&content) {
            Ok(ids) => identities.extend(ids),
            Err(e) => {
                tracing::error!("{}: {e}", display_source(path));
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
    let transport = RetryingTransport::new(UreqTransport::new());
    let providers = default_providers_with_topic(args.topic.clone());

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

/// Read and parse a stored Snapshot JSON file. `Err` carries the CLI exit
/// code to return immediately; the error itself is already logged.
fn load_snapshot(path: &std::path::Path) -> Result<Snapshot, i32> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        tracing::error!("could not read {}: {e}", path.display());
        2
    })?;
    serde_json::from_str(&content).map_err(|e| {
        tracing::error!("could not parse {} as a Snapshot: {e}", path.display());
        2
    })
}

/// Read a stored Snapshot and print it in the requested format. Never
/// touches the network — a pure read + render over already-fetched data
/// (ADR-0001), so the exit code still reflects any `Failed` outcomes the
/// Snapshot itself recorded, even though nothing was fetched this run.
fn run_render(args: RenderArgs) -> i32 {
    let snapshot = match load_snapshot(&args.snapshot) {
        Ok(s) => s,
        Err(code) => return code,
    };

    match args.format {
        // Markdown already ends each of its lines (and so the whole string)
        // in a newline; prose is one bare sentence with no newline of its
        // own, so it gets one here to end the line cleanly.
        Format::Markdown => print!("{}", render_markdown(&snapshot)),
        Format::Prose => println!("{}", render_prose(&snapshot)),
    }

    if snapshot.has_failures() {
        1
    } else {
        0
    }
}

/// Diff two stored Snapshots and print the growth in each shared Metric.
/// Never touches the network (ADR-0001). Exits non-zero if either Snapshot
/// itself recorded a `Failed` outcome — the diff may be based on incomplete
/// data, even though nothing was fetched this run.
fn run_diff(args: DiffArgs) -> i32 {
    let old = match load_snapshot(&args.old) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let new = match load_snapshot(&args.new) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let d = diff::compute(&old, &new);
    print!("{}", diff::render(&old, &new, &d));

    if old.has_failures() || new.has_failures() {
        1
    } else {
        0
    }
}

/// Read a `--from-file` source: a path, or `-` for stdin.
fn read_source(path: &std::path::Path) -> std::io::Result<String> {
    if path.as_os_str() == "-" {
        std::io::read_to_string(std::io::stdin())
    } else {
        std::fs::read_to_string(path)
    }
}

fn display_source(path: &std::path::Path) -> String {
    if path.as_os_str() == "-" {
        "<stdin>".to_string()
    } else {
        path.display().to_string()
    }
}

/// Parse one identifier per line, ignoring blank lines and `#` comments. On a
/// bad line, return an error naming the 1-based line number.
fn parse_identifier_lines(content: &str) -> Result<Vec<Identity>, String> {
    let mut identities = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match Identity::parse(line) {
            Ok(id) => identities.push(id),
            Err(e) => return Err(format!("line {}: {e}", i + 1)),
        }
    }
    Ok(identities)
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
        let Command::About(a) = bare.command else {
            panic!("expected About")
        };
        assert_eq!(a.targets, vec!["10.1/x".to_string()]);
    }

    #[test]
    fn cli_parses_package_flag() {
        let cli = Cli::try_parse_from(norm(&["boast", "--package", "crates:boast"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert_eq!(a.packages, vec!["crates:boast".to_string()]);
    }

    #[test]
    fn render_is_a_real_subcommand_never_swallowed_into_an_implicit_about() {
        assert_eq!(
            norm(&["boast", "render", "snap.json"]),
            vec!["boast", "render", "snap.json"]
        );
    }

    #[test]
    fn cli_parses_render_with_default_and_explicit_format() {
        let cli = Cli::try_parse_from(norm(&["boast", "render", "snap.json"])).unwrap();
        let Command::Render(r) = cli.command else {
            panic!("expected Render")
        };
        assert_eq!(r.snapshot, PathBuf::from("snap.json"));
        assert!(matches!(r.format, Format::Markdown));

        let cli = Cli::try_parse_from(norm(&["boast", "render", "snap.json", "--format", "prose"]))
            .unwrap();
        let Command::Render(r) = cli.command else {
            panic!("expected Render")
        };
        assert!(matches!(r.format, Format::Prose));
    }

    #[test]
    fn cli_rejects_an_unknown_render_format() {
        let err = Cli::try_parse_from(norm(&["boast", "render", "snap.json", "--format", "html"]))
            .unwrap_err();
        assert!(err.to_string().contains("markdown"));
    }

    #[test]
    fn diff_is_a_real_subcommand_never_swallowed_into_an_implicit_about() {
        assert_eq!(
            norm(&["boast", "diff", "old.json", "new.json"]),
            vec!["boast", "diff", "old.json", "new.json"]
        );
    }

    #[test]
    fn cli_parses_diff_with_two_positional_snapshots() {
        let cli = Cli::try_parse_from(norm(&["boast", "diff", "old.json", "new.json"])).unwrap();
        let Command::Diff(d) = cli.command else {
            panic!("expected Diff")
        };
        assert_eq!(d.old, PathBuf::from("old.json"));
        assert_eq!(d.new, PathBuf::from("new.json"));
    }

    #[test]
    fn cli_diff_requires_both_snapshots() {
        let err = Cli::try_parse_from(norm(&["boast", "diff", "old.json"])).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("new"));
    }

    #[test]
    fn parses_identifier_file_with_comments_and_mixed_kinds() {
        let content = "\
# my grant tools
10.1371/journal.pbio.1002195

  samtools/samtools
pmid:31234567
";
        let ids = parse_identifier_lines(content).unwrap();
        assert_eq!(ids.len(), 3);
        assert!(matches!(ids[1], crate::model::Identity::Repo(_)));

        let err = parse_identifier_lines("10.1/x\nnot an id\n").unwrap_err();
        assert!(err.contains("line 2"));
    }
}
