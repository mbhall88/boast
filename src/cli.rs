//! Command-line surface. Subcommands (`about`, `render`, `diff`, `providers`,
//! `init`), plus a bare-identifier shortcut so `boast 10.1234/x` means
//! `boast about 10.1234/x`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use time::macros::format_description;
use time::OffsetDateTime;

use crate::diff;
use crate::manifest::Manifest;
use crate::model::{Identity, IdentityError, OrcidId, PackageId, Project, RepoId, Snapshot};
use crate::orchestrator;
use crate::orcid::{self, OrcidWork};
use crate::providers::{
    default_providers, default_providers_with_topic, paper_provider_count, render_providers,
};
use crate::report::{render_markdown, render_prose, render_terminal};
use crate::transport::{RetryingTransport, UreqTransport};

/// Subcommands recognised as the first positional token. Anything else is
/// treated as a bare identifier for `about`.
const SUBCOMMANDS: &[&str] = &["about", "render", "diff", "providers", "init", "help"];

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
    /// Fetch metrics for a Project, write a Snapshot, and print a report. A
    /// single `.toml` positional (see `boast init`) is loaded as a Manifest
    /// instead, running every Project it lists.
    About(AboutArgs),

    /// Render a stored Snapshot as Markdown or prose. Never touches the
    /// network (ADR-0001) — offline and deterministic for a given Snapshot.
    Render(RenderArgs),

    /// Compare two stored Snapshots and report the change in each shared
    /// Metric. Never touches the network (ADR-0001).
    Diff(DiffArgs),

    /// List the registered Providers: Category, default-enabled status, and
    /// key requirement. Never touches the network.
    Providers,

    /// Write a Manifest TOML file from identifiers, without fetching — unless
    /// `--orcid` expands a researcher's record, which does (see its own help).
    Init(InitArgs),
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

/// Identifier sources shared by `about` and `init`: a positional list plus
/// the `--repo`/`--package`/`--from-file` alternatives. Flattened into both
/// so the two subcommands can't drift apart on how they accept identifiers.
#[derive(Debug, Args)]
pub struct IdentitySourceArgs {
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
}

impl IdentitySourceArgs {
    /// True if `--repo`, `--package`, or `--from-file` were given (positionals
    /// excluded — callers care about those separately). Shared by
    /// `manifest_positional`'s "nothing but a bare Manifest path" check and
    /// `run_init_orcid`'s "nothing but `--orcid`" exclusivity check.
    fn has_flag_sources(&self) -> bool {
        !self.repos.is_empty() || !self.packages.is_empty() || !self.from_file.is_empty()
    }
}

#[derive(Debug, Args)]
pub struct AboutArgs {
    #[command(flatten)]
    pub sources: IdentitySourceArgs,

    /// GitHub topic to rank repositories within, overriding each repo's own
    /// declared topics (see the Cohort disclaimer in the report). When the
    /// input is a Manifest, this overrides every Project's own topic too.
    #[arg(short = 't', long = "topic", value_name = "TOPIC")]
    pub topic: Option<String>,

    /// Directory to write the Snapshot into.
    #[arg(short = 'd', long, default_value = "snapshots", value_name = "DIR")]
    pub snapshot_dir: PathBuf,

    /// Print the report but do not write a Snapshot file.
    #[arg(short = 'n', long)]
    pub no_save: bool,

    /// After fetching, also write a Manifest reflecting the identities (and
    /// `--topic`) used in this run, so a future run can `boast about <file>`
    /// instead of re-typing them. Not available when the input is itself a
    /// Manifest — use `boast init` to build one up front instead.
    #[arg(short = 's', long = "save", value_name = "FILE")]
    pub save: Option<PathBuf>,

    /// Maximum number of distinct hosts fetched from concurrently. Never
    /// more than one request is in flight against the *same* host no matter
    /// how high this is set (ADR-0007). Raising it past the number
    /// of hosts a Project actually touches (at most the Provider registry's
    /// size, ~11 by default) buys nothing; lower it to open fewer
    /// simultaneous connections.
    #[arg(
        short = 'j',
        long = "threads",
        default_value_t = orchestrator::DEFAULT_CONCURRENCY,
        value_parser = parse_at_least_one,
        value_name = "N"
    )]
    pub threads: usize,
}

/// `--threads`' value parser: `usize::from_str` plus a clear rejection of
/// `0` (which would leave every fetch queued forever with no worker to run
/// it — see `orchestrator::run_with_concurrency`'s doc comment).
fn parse_at_least_one(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(0) => Err("must be at least 1".to_string()),
        Ok(n) => Ok(n),
        Err(_) => Err(format!("'{s}' is not a valid number")),
    }
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[command(flatten)]
    pub sources: IdentitySourceArgs,

    /// GitHub topic to record in the Manifest for this Project's Cohort ranking.
    #[arg(short = 't', long = "topic", value_name = "TOPIC")]
    pub topic: Option<String>,

    /// Where to write the Manifest.
    #[arg(
        short = 'o',
        long = "output",
        default_value = "manifest.toml",
        value_name = "FILE"
    )]
    pub output: PathBuf,

    /// Expand a researcher's ORCID iD (bare, `orcid:`-prefixed, or an
    /// orcid.org URL) into a Manifest of every work with a DOI or PMID, one
    /// Project per work (ADR-0006; repeatable). **Performs a network
    /// fetch** — unlike the rest of `init`, which is otherwise offline.
    /// Exclusive with positionals/`--repo`/`--package`/`--from-file`: an
    /// ORCID expansion has no defensible answer to "which of these works
    /// does that repo belong to?"
    #[arg(short = 'O', long = "orcid", value_name = "ORCID")]
    pub orcid: Vec<String>,

    /// With `--orcid`, also list works with neither a DOI nor a PMID (and so
    /// were skipped) as commented-out `[[project]]` blocks you can fill in
    /// by hand. Off by default: most ORCID records carry many such works.
    #[arg(short = 'u', long = "include-unidentified")]
    pub include_unidentified: bool,
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
        Command::Providers => run_providers(),
        Command::Init(args) => run_init(args),
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
    if let Some(manifest_path) = manifest_positional(&args) {
        if let Some(save_path) = &args.save {
            tracing::error!(
                "--save {} cannot be combined with a Manifest input ({}); it already is one",
                save_path.display(),
                manifest_path.display()
            );
            return 2;
        }
        return run_about_manifest(manifest_path, &args);
    }

    let identities = match parse_identities(&args.sources) {
        Ok(ids) => ids,
        Err(code) => return code,
    };

    if identities.is_empty() {
        tracing::error!("no identifiers given (pass a DOI/PMID/repo, or --repo owner/name)");
        return 2;
    }

    warn_if_missing_github_token(&identities);

    let project = Project::new(identities);
    let transport = RetryingTransport::new(UreqTransport::new());
    let providers = default_providers_with_topic(args.topic.clone());

    let snapshot =
        orchestrator::run_with_concurrency(&project, &providers, &transport, args.threads);

    if let Err(code) = print_and_save_snapshot(&snapshot, &args, None) {
        return code;
    }

    if let Some(save_path) = &args.save {
        if let Err(code) = save_manifest(&project.identities, args.topic.as_deref(), save_path) {
            return code;
        }
    }

    if snapshot.has_failures() {
        1
    } else {
        0
    }
}

/// A bare `boast about manifest.toml` — a single positional target ending in
/// `.toml`, with no other identity source given — loads a Manifest instead of
/// parsing the target as an Identity. Combined with any of `--repo`/
/// `--package`/`--from-file`, the target is left to the normal Identity path
/// (and fails there with a clear "unrecognised identifier" error), since
/// mixing a Manifest with ad-hoc identities on the same run is ambiguous.
fn manifest_positional(args: &AboutArgs) -> Option<&std::path::Path> {
    let sources = &args.sources;
    if sources.targets.len() != 1 || sources.has_flag_sources() {
        return None;
    }
    let path = std::path::Path::new(&sources.targets[0]);
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        .then_some(path)
}

/// Run every Project listed in a Manifest, printing and writing a Snapshot
/// for each (`about` always fetches live — ADR-0001). `--topic`, if given,
/// overrides every Project's own manifest topic, the same way it already
/// overrides a repo's own declared topics for a single-Project run.
fn run_about_manifest(path: &std::path::Path, args: &AboutArgs) -> i32 {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("could not read manifest {}: {e}", path.display());
            return 2;
        }
    };
    let manifest = match Manifest::parse(&content) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("{}: {e}", path.display());
            return 2;
        }
    };

    let transport = RetryingTransport::new(UreqTransport::new());
    let mut had_failures = false;

    for (index, entry) in manifest.projects.iter().enumerate() {
        let project = match entry.to_project(index) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("{e}");
                return 2;
            }
        };

        warn_if_missing_github_token(&project.identities);

        let topic = args.topic.clone().or_else(|| entry.topic.clone());
        let providers = default_providers_with_topic(topic);

        let snapshot =
            orchestrator::run_with_concurrency(&project, &providers, &transport, args.threads);

        if index > 0 {
            println!();
        }
        let suffix = project
            .identities
            .first()
            .map(|id| sanitize_filename(&id.canonical()));
        if let Err(code) = print_and_save_snapshot(&snapshot, args, suffix.as_deref()) {
            return code;
        }

        had_failures |= snapshot.has_failures();
    }

    if had_failures {
        1
    } else {
        0
    }
}

/// Print a Snapshot's terminal Report and, unless `--no-save`, write it to
/// `args.snapshot_dir` — shared by `about`'s single-Project run and each
/// iteration of a Manifest batch run.
fn print_and_save_snapshot(
    snapshot: &Snapshot,
    args: &AboutArgs,
    suffix: Option<&str>,
) -> Result<(), i32> {
    print!("{}", render_terminal(snapshot));

    if !args.no_save {
        match write_snapshot(snapshot, &args.snapshot_dir, suffix) {
            Ok(path) => println!("\nSnapshot written to {}", path.display()),
            Err(e) => {
                tracing::error!("could not write snapshot: {e}");
                return Err(2);
            }
        }
    }
    Ok(())
}

/// Turn an Identity's canonical form into a filesystem-safe filename
/// component, e.g. `doi:10.1/x` -> `doi-10.1-x`, so a Manifest batch run
/// never has two Projects' Snapshots collide on the same timestamp.
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Parse an `IdentitySourceArgs`' targets/`--repo`/`--package`/`--from-file`
/// into identities. Shared by `about`'s CLI-identity flow and `init`. A bad
/// identifier is a usage error (exit code 2, already logged).
fn parse_identities(sources: &IdentitySourceArgs) -> Result<Vec<Identity>, i32> {
    let mut identities = Vec::new();
    extend_identities(&mut identities, &sources.targets, Identity::parse, |id| id)?;
    extend_identities(
        &mut identities,
        &sources.repos,
        RepoId::parse,
        Identity::Repo,
    )?;
    extend_identities(
        &mut identities,
        &sources.packages,
        PackageId::parse,
        Identity::Package,
    )?;
    for path in &sources.from_file {
        let content = read_source(path).map_err(|e| {
            tracing::error!("could not read {}: {e}", display_source(path));
            2
        })?;
        let ids = parse_identifier_lines(&content).map_err(|e| {
            tracing::error!("{}: {e}", display_source(path));
            2
        })?;
        identities.extend(ids);
    }
    Ok(identities)
}

/// Warn loudly if repo metrics were requested without a token to raise limits.
fn warn_if_missing_github_token(identities: &[Identity]) {
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
}

/// Write a Manifest reflecting `identities`/`topic` to `path` — the shared
/// basis for `about --save` and `init`.
fn save_manifest(
    identities: &[Identity],
    topic: Option<&str>,
    path: &std::path::Path,
) -> Result<(), i32> {
    let manifest = Manifest::from_identities(identities, topic);
    let toml_str = manifest.to_toml_string().map_err(|e| {
        tracing::error!("could not serialise manifest: {e}");
        2
    })?;
    std::fs::write(path, toml_str).map_err(|e| {
        tracing::error!("could not write manifest {}: {e}", path.display());
        2
    })?;
    println!("Manifest written to {}", path.display());
    Ok(())
}

/// Write a Manifest TOML file from identifiers, without fetching. Lets you
/// build up a Manifest incrementally before ever running `about`.
fn run_init(args: InitArgs) -> i32 {
    if !args.orcid.is_empty() {
        return run_init_orcid(&args);
    }
    if args.include_unidentified {
        tracing::warn!("--include-unidentified has no effect without --orcid");
    }

    let identities = match parse_identities(&args.sources) {
        Ok(ids) => ids,
        Err(code) => return code,
    };

    if identities.is_empty() {
        tracing::error!("no identifiers given (pass a DOI/PMID/repo, or --repo owner/name)");
        return 2;
    }

    match save_manifest(&identities, args.topic.as_deref(), &args.output) {
        Ok(()) => 0,
        Err(code) => code,
    }
}

/// `init --orcid`: expand every given ORCID iD's works (ADR-0006) into a
/// Manifest, one Project per DOI/PMID-bearing work. A network path — unlike
/// the rest of `init` — so the caution about its cost (per-work Provider
/// fan-out) is surfaced both here on stderr and in the generated file's own
/// header, from the one shared count `paper_provider_count` computes.
fn run_init_orcid(args: &InitArgs) -> i32 {
    let sources = &args.sources;
    if !sources.targets.is_empty() || sources.has_flag_sources() {
        tracing::error!(
            "--orcid cannot be combined with other identity sources (positionals, --repo, \
             --package, --from-file)"
        );
        return 2;
    }

    let mut orcids = Vec::with_capacity(args.orcid.len());
    for value in &args.orcid {
        match OrcidId::parse(value) {
            Ok(id) => orcids.push(id),
            Err(_) => {
                tracing::error!(
                    "'{value}' is not a valid ORCID iD (expected e.g. 0000-0002-1825-0097)"
                );
                return 2;
            }
        }
    }

    let transport = RetryingTransport::new(UreqTransport::new());
    let provider_count = paper_provider_count();

    let mut identified = Vec::new();
    let mut unidentified: Vec<OrcidWork> = Vec::new();
    let mut total = 0usize;

    for id in &orcids {
        let expansion = match orcid::expand(id, &transport) {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("{e}");
                return 2;
            }
        };
        let this_total = expansion.works.len();
        let this_identified = expansion.identified().count();
        let this_skipped = this_total - this_identified;
        tracing::warn!(
            "orcid:{id}: {this_total} work{ws} in record; {this_identified} have a DOI/PMID and \
             will be written to {output}, {this_skipped} were skipped (no DOI or PMID — not \
             measurable).",
            ws = orcid::plural(this_total),
            output = args.output.display(),
        );
        total += this_total;
        for work in expansion.works {
            match work {
                OrcidWork::Identified { id } => identified.push(id),
                unident => unidentified.push(unident),
            }
        }
    }

    if identified.is_empty() {
        tracing::error!(
            "no works with a DOI or PMID found across {} ORCID record(s)",
            orcids.len()
        );
        return 2;
    }

    let estimated_requests = identified.len() * provider_count;
    tracing::warn!(
        "running `boast about` over {} work{ws} ≈ {estimated_requests} requests across \
         {provider_count} Providers",
        identified.len(),
        ws = orcid::plural(identified.len()),
    );

    let manifest = Manifest::from_orcid_works(&identified, args.topic.as_deref());
    let toml_str = match manifest.to_toml_string() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("could not serialise manifest: {e}");
            return 2;
        }
    };

    let mut out = orcid::render_header(
        &orcids,
        total,
        unidentified.len(),
        OffsetDateTime::now_utc(),
        provider_count,
    );
    if args.include_unidentified && !unidentified.is_empty() {
        let refs: Vec<&OrcidWork> = unidentified.iter().collect();
        out.push_str(&orcid::render_unidentified_block(&refs));
        out.push('\n');
    }
    out.push_str(&toml_str);

    if let Err(e) = std::fs::write(&args.output, out) {
        tracing::error!("could not write manifest {}: {e}", args.output.display());
        return 2;
    }
    println!("Manifest written to {}", args.output.display());
    0
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

/// List the registered Providers. Never touches the network — the registry
/// itself, not any live data, is what's being reported on.
fn run_providers() -> i32 {
    let providers = default_providers();
    print!(
        "{}",
        render_providers(&providers, |env_var| std::env::var(env_var)
            .ok()
            .filter(|s| !s.is_empty())
            .is_some())
    );
    0
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

/// Write a Snapshot into `dir`, named by its `created_at` timestamp. `suffix`
/// (a Manifest batch run's per-Project filename component, see
/// `sanitize_filename`) disambiguates multiple Snapshots written in the same
/// run whose timestamps might otherwise collide; a single-Project `about`
/// run passes `None` and keeps the plain timestamp-only filename.
fn write_snapshot(
    snapshot: &crate::model::Snapshot,
    dir: &std::path::Path,
    suffix: Option<&str>,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let fmt = format_description!("[year][month][day]T[hour][minute][second]Z");
    let stamp = snapshot
        .created_at
        .format(&fmt)
        .unwrap_or_else(|_| "snapshot".to_string());
    let filename = match suffix {
        Some(s) => format!("{stamp}-{s}.json"),
        None => format!("{stamp}.json"),
    };
    let path = dir.join(filename);
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
        assert_eq!(a.sources.targets, vec!["10.1/x".to_string()]);
    }

    #[test]
    fn cli_parses_package_flag() {
        let cli = Cli::try_parse_from(norm(&["boast", "--package", "crates:boast"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert_eq!(a.sources.packages, vec!["crates:boast".to_string()]);
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
    fn providers_is_a_real_subcommand_never_swallowed_into_an_implicit_about() {
        assert_eq!(norm(&["boast", "providers"]), vec!["boast", "providers"]);
    }

    #[test]
    fn cli_parses_providers_with_no_arguments() {
        let cli = Cli::try_parse_from(norm(&["boast", "providers"])).unwrap();
        assert!(matches!(cli.command, Command::Providers));
    }

    #[test]
    fn run_providers_lists_the_real_registry_and_always_succeeds() {
        assert_eq!(run_providers(), 0);
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

    #[test]
    fn init_is_a_real_subcommand_never_swallowed_into_an_implicit_about() {
        assert_eq!(
            norm(&["boast", "init", "10.1/x"]),
            vec!["boast", "init", "10.1/x"]
        );
    }

    #[test]
    fn cli_parses_init_with_default_and_explicit_output() {
        let cli = Cli::try_parse_from(norm(&["boast", "init", "10.1/x"])).unwrap();
        let Command::Init(i) = cli.command else {
            panic!("expected Init")
        };
        assert_eq!(i.sources.targets, vec!["10.1/x".to_string()]);
        assert_eq!(i.output, PathBuf::from("manifest.toml"));

        let cli = Cli::try_parse_from(norm(&["boast", "init", "10.1/x", "--output", "mine.toml"]))
            .unwrap();
        let Command::Init(i) = cli.command else {
            panic!("expected Init")
        };
        assert_eq!(i.output, PathBuf::from("mine.toml"));
    }

    #[test]
    fn cli_parses_about_save_flag() {
        let cli = Cli::try_parse_from(norm(&["boast", "10.1/x", "--save", "out.toml"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert_eq!(a.save, Some(PathBuf::from("out.toml")));
    }

    #[test]
    fn threads_defaults_to_the_orchestrator_default() {
        let cli = Cli::try_parse_from(norm(&["boast", "10.1/x"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert_eq!(a.threads, orchestrator::DEFAULT_CONCURRENCY);
    }

    #[test]
    fn threads_flag_accepts_a_short_and_long_form() {
        let cli = Cli::try_parse_from(norm(&["boast", "10.1/x", "-j", "1"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert_eq!(a.threads, 1);

        let cli = Cli::try_parse_from(norm(&["boast", "10.1/x", "--threads", "3"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert_eq!(a.threads, 3);
    }

    #[test]
    fn threads_flag_rejects_zero_with_a_clear_error() {
        let err = Cli::try_parse_from(norm(&["boast", "10.1/x", "--threads", "0"])).unwrap_err();
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn a_single_toml_positional_is_treated_as_a_manifest() {
        let cli = Cli::try_parse_from(norm(&["boast", "manifest.toml"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert!(manifest_positional(&a).is_some());
        assert_eq!(
            manifest_positional(&a).unwrap(),
            std::path::Path::new("manifest.toml")
        );
    }

    #[test]
    fn a_toml_positional_combined_with_other_identity_sources_is_not_a_manifest() {
        let cli =
            Cli::try_parse_from(norm(&["boast", "manifest.toml", "--repo", "owner/name"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert!(manifest_positional(&a).is_none());
    }

    #[test]
    fn a_non_toml_positional_is_not_a_manifest() {
        let cli = Cli::try_parse_from(norm(&["boast", "10.1/x"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert!(manifest_positional(&a).is_none());
    }

    #[test]
    fn sanitize_filename_replaces_unsafe_characters() {
        assert_eq!(sanitize_filename("doi:10.1/x"), "doi-10.1-x");
        assert_eq!(sanitize_filename("github:owner/name"), "github-owner-name");
    }

    #[test]
    fn about_save_combined_with_a_manifest_input_is_rejected() {
        let cli =
            Cli::try_parse_from(norm(&["boast", "manifest.toml", "--save", "out.toml"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        assert_eq!(run_about(a), 2);
    }

    #[test]
    fn orcid_flag_accepts_short_and_long_form_and_is_repeatable() {
        let cli = Cli::try_parse_from(norm(&[
            "boast",
            "init",
            "-O",
            "0000-0002-1825-0097",
            "--orcid",
            "0000-0001-2345-6789",
        ]))
        .unwrap();
        let Command::Init(i) = cli.command else {
            panic!("expected Init")
        };
        assert_eq!(
            i.orcid,
            vec![
                "0000-0002-1825-0097".to_string(),
                "0000-0001-2345-6789".to_string()
            ]
        );
    }

    #[test]
    fn include_unidentified_flag_accepts_short_and_long_form() {
        let cli = Cli::try_parse_from(norm(&[
            "boast",
            "init",
            "--orcid",
            "0000-0002-1825-0097",
            "-u",
        ]))
        .unwrap();
        let Command::Init(i) = cli.command else {
            panic!("expected Init")
        };
        assert!(i.include_unidentified);

        let cli = Cli::try_parse_from(norm(&[
            "boast",
            "init",
            "--orcid",
            "0000-0002-1825-0097",
            "--include-unidentified",
        ]))
        .unwrap();
        let Command::Init(i) = cli.command else {
            panic!("expected Init")
        };
        assert!(i.include_unidentified);
    }

    #[test]
    fn about_cannot_structurally_receive_an_orcid_flag() {
        // `--orcid` lives only on `InitArgs`, not the shared `IdentitySourceArgs`
        // (ADR-0006) — `about` must reject it as an unknown argument.
        let err = Cli::try_parse_from(norm(&["boast", "about", "--orcid", "0000-0002-1825-0097"]))
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unexpected"));
    }

    #[test]
    fn init_orcid_combined_with_other_identity_sources_is_rejected() {
        let cli = Cli::try_parse_from(norm(&[
            "boast",
            "init",
            "--orcid",
            "0000-0002-1825-0097",
            "--repo",
            "owner/name",
        ]))
        .unwrap();
        let Command::Init(i) = cli.command else {
            panic!("expected Init")
        };
        assert_eq!(run_init(i), 2);
    }

    #[test]
    fn init_rejects_a_malformed_orcid_value() {
        let cli = Cli::try_parse_from(norm(&["boast", "init", "--orcid", "not-an-orcid"])).unwrap();
        let Command::Init(i) = cli.command else {
            panic!("expected Init")
        };
        assert_eq!(run_init(i), 2);
    }

    #[test]
    fn about_orcid_identifier_is_refused_with_the_dedicated_actionable_error_not_the_catch_all() {
        let cli = Cli::try_parse_from(norm(&["boast", "0000-0002-1825-0097"])).unwrap();
        let Command::About(a) = cli.command else {
            panic!("expected About")
        };
        // `run_about` only surfaces its exit code (2, same as any other bad
        // identifier) — the dedicated-vs-generic distinction lives in which
        // `IdentityError` variant `parse_identities` hit internally, so assert
        // on that directly rather than on the exit code alone.
        assert!(matches!(
            Identity::parse(&a.sources.targets[0]),
            Err(IdentityError::IsOrcid(_))
        ));
        assert_eq!(run_about(a), 2);
    }
}
