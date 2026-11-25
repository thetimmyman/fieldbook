//! Command-line argument definition (`clap` derive `Cli`, `AD-17`).

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fieldbook_core::{CivilDate, SourceId};

/// Clean run with zero error findings (`AD-17`).
pub const EXIT_OK: u8 = 0;

/// Run completed with at least one error finding (`AD-17`).
pub const EXIT_FINDINGS: u8 = 1;

/// Command-line usage error or invalid argument (`AD-17`).
pub const EXIT_USAGE: u8 = 2;

/// Input or output failure (`AD-17`).
pub const EXIT_IO: u8 = 3;

/// The four exit codes of `AD-17` and their one-line meanings.
pub const EXIT_CODES: &[(u8, &str)] = &[
    (
        EXIT_OK,
        "Clean (zero errors; warnings do not change the exit code)",
    ),
    (EXIT_FINDINGS, "Findings (at least one error)"),
    (
        EXIT_USAGE,
        "Usage error (invalid command-line arguments or configuration refusal)",
    ),
    (EXIT_IO, "I/O or operational failure"),
];

/// Top-level command-line parser for the `fieldbook` program.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(
    version,
    disable_version_flag = true,
    about = "Offline finance reference and Corpus toolkit."
)]
pub struct Cli {
    /// Print version and build identity.
    #[arg(long = "version")]
    pub version: bool,

    /// Path to the state folder (defaults to `.fieldbook/` beside the Corpus).
    #[arg(long = "state", value_name = "PATH", global = true)]
    pub state: Option<PathBuf>,

    /// Hidden test-hook override for the policy file path (`#[cfg(feature = "test-hooks")]`).
    #[cfg(feature = "test-hooks")]
    #[arg(long = "policy", value_name = "FILE", global = true, hide = true)]
    pub policy: Option<PathBuf>,

    /// Hidden test-hook override for the machine `ProgramData` directory (`#[cfg(feature = "test-hooks")]`).
    #[cfg(feature = "test-hooks")]
    #[arg(long = "program-data", value_name = "DIR", global = true, hide = true)]
    pub program_data: Option<PathBuf>,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level subcommands (`AD-17`).
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Commands {
    /// Check the Corpus and print findings.
    Check(CheckArgs),
    /// Normalize registers and page bodies under `corpus/`.
    Fmt(FmtArgs),
    /// Build the static site and workbooks into an edition directory.
    Build(BuildArgs),
    /// Add a source or source version to the Corpus.
    Add(AddArgs),
    /// Create a new page or register row in its entry state.
    New(NewArgs),
    /// Inspect or verify the audit journal.
    Audit(AuditArgs),
    /// Find an id or a word in the Corpus; list the versions of a source.
    Search(SearchArgs),
    /// Show what needs work, and optionally write the desk pages.
    Status(StatusArgs),
    /// Record that the owner (or lead) verified a claim, at a terminal.
    Verify(VerifyArgs),
    /// Create, verify, or list backup snapshots.
    Backup(BackupArgs),
    /// Restore a backup snapshot into a new folder or run the restore drill.
    Restore(RestoreArgs),
    /// Show the active company policy or validate a draft policy file.
    Policy(PolicyArgs),
    /// Inspect AI settings, adapter readiness, policy allow sets and key state.
    Ai(AiArgs),
    /// Build a plain-text context pack for a question from the permitted view.
    Context(ContextArgs),
}

/// Arguments for `fieldbook fmt`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct FmtArgs {
    /// List each file that would change and exit 1 without writing.
    #[arg(long)]
    pub check: bool,
}

/// Arguments for `fieldbook verify`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct VerifyArgs {
    /// The claim to verify (`CL-AAA-nnnn`).
    #[arg(value_name = "CLAIM-ID")]
    pub claim: String,

    /// The calendar date (YYYY-MM-DD) the original was read at the locator.
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub on: CivilDate,
}

/// Arguments for `fieldbook search`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct SearchArgs {
    /// Print the results as a JSON object.
    #[arg(long)]
    pub json: bool,

    /// List every matching unit in rank order instead of grouping sections into their page.
    #[arg(long)]
    pub units: bool,

    /// The words to search for; they are joined with single spaces, so quoting is optional. Put `--` before a word that starts with `-`.
    #[arg(value_name = "QUERY", required = true, num_args = 1..)]
    pub words: Vec<String>,
}

/// Arguments for `fieldbook new`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct NewArgs {
    /// Target object kind (`page`, `claim`, `question`, `conflict`, or `term`).
    #[command(subcommand)]
    pub command: NewSubcommand,
}

/// Subcommands under `fieldbook new`.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum NewSubcommand {
    /// Create a new Corpus page from a page-type template.
    Page(NewPageArgs),
    /// Append a new claim row in entry state to `claims.csv`.
    Claim(NewClaimArgs),
    /// Append a new open question row to `questions.csv`.
    Question(NewQuestionArgs),
    /// Append a new unresolved conflict row to `conflicts.csv`.
    Conflict(NewConflictArgs),
    /// Append a new glossary term row to `glossary.csv`.
    Term(NewTermArgs),
}

/// Arguments for `fieldbook new page`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct NewPageArgs {
    /// Confirm that the new page is public and may enter the repository Corpus.
    #[arg(long)]
    pub confirm_public: bool,

    /// Page identifier (`<area>-<slug>`).
    #[arg(value_name = "AREA-SLUG")]
    pub id: Option<String>,

    /// Page type: concept, procedure, reference, case, clause-card, calc, checklist, template
    /// or brief.
    #[arg(long = "type", value_name = "TYPE")]
    pub ty: Option<String>,

    /// Subarea of the page's area, which names the folder the page is written in.
    #[arg(long, value_name = "SUBAREA")]
    pub subarea: Option<String>,
}

/// Arguments for `fieldbook new claim`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct NewClaimArgs {
    /// Confirm that the new claim is public and may enter the repository Corpus.
    #[arg(long)]
    pub confirm_public: bool,

    /// Area code (`cmp`, `est`, ...).
    #[arg(long, value_name = "AREA")]
    pub area: Option<String>,

    /// Source version identifier (`S-nnnn-vN`) in `sources.csv`.
    #[arg(long, value_name = "VERSION-ID")]
    pub source: Option<String>,

    /// Locator within the source version (for example `p. 12` or `sec. 1.1`).
    #[arg(long, value_name = "LOCATOR")]
    pub at: Option<String>,

    /// Verbatim quote from the source version.
    #[arg(long, value_name = "TEXT")]
    pub quote: Option<String>,
}

/// Arguments for `fieldbook new question`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct NewQuestionArgs {
    /// Confirm that the new question is public and may enter the repository Corpus.
    #[arg(long)]
    pub confirm_public: bool,

    /// Area code (`cmp`, `est`, ...).
    #[arg(long, value_name = "AREA")]
    pub area: Option<String>,

    /// Question text.
    #[arg(value_name = "TEXT")]
    pub text: Option<String>,
}

/// Arguments for `fieldbook new conflict`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct NewConflictArgs {
    /// Confirm that the new conflict is public and may enter the repository Corpus.
    #[arg(long)]
    pub confirm_public: bool,

    /// Area code (`cmp`, `est`, ...).
    #[arg(long, value_name = "AREA")]
    pub area: Option<String>,
}

/// Arguments for `fieldbook new term`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct NewTermArgs {
    /// Confirm that the new glossary term is public and may enter the repository Corpus.
    #[arg(long)]
    pub confirm_public: bool,

    /// Owner area code (`cmp`, `est`, ...).
    #[arg(long, value_name = "AREA")]
    pub area: Option<String>,

    /// The term id (lowercase snake_case); without it the next `<area>_term_<n>` is used.
    #[arg(long, value_name = "TERM-ID")]
    pub id: Option<String>,
}

/// Arguments for `fieldbook build`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct BuildArgs {
    /// Parent output directory (`DIR/edition` holds the built site).
    #[arg(long = "out", value_name = "DIR", default_value = "out")]
    pub out: PathBuf,

    /// Rebuild when the Corpus changes (checks every 500 ms; Ctrl-C stops).
    #[arg(
        long,
        help = "Rebuild when the Corpus changes (checks every 500 ms; Ctrl-C stops)"
    )]
    pub watch: bool,

    /// Path of an earlier edition's folder or `edition.toml`, for `changes.html`.
    #[arg(long, value_name = "EDITION")]
    pub since: Option<String>,
}

/// Arguments for `fieldbook add`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct AddArgs {
    /// Path to the source file or folder to add.
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Existing source ID (`S-nnnn`) when adding a new version of an existing source.
    #[arg(long, value_name = "SOURCE-ID")]
    pub source: Option<SourceId>,

    /// Append the register row without storing a copy of the original file.
    #[arg(long)]
    pub metadata_only: bool,

    /// Confirm that the unlabelled file is public and may enter the repository Corpus.
    #[arg(long)]
    pub confirm_public: bool,

    /// Descend into subfolders when adding a folder.
    #[arg(long)]
    pub recursive: bool,

    /// Print the report as a JSON object.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `fieldbook check`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct CheckArgs {
    /// Treat strict-level rules as errors.
    #[arg(long)]
    pub strict: bool,

    /// Calendar date (YYYY-MM-DD) for freshness and as_of checks.
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub today: Option<CivilDate>,

    /// Print the report as a JSON object.
    #[arg(long)]
    pub json: bool,

    /// Optional paths (relative to the working directory) to filter diagnostics.
    #[arg(value_name = "PATHS")]
    pub paths: Vec<PathBuf>,
}

/// Arguments for `fieldbook status`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct StatusArgs {
    /// Calendar date (YYYY-MM-DD) for freshness, instead of the clock.
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub today: Option<CivilDate>,

    /// Write the desk pages into `pages/` of the state folder.
    #[arg(long)]
    pub page: bool,
}

/// Arguments for `fieldbook audit`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct AuditArgs {
    /// Audit subcommand (`verify` or `show`).
    #[command(subcommand)]
    pub command: AuditSubcommand,
}

/// Subcommands under `fieldbook audit`.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum AuditSubcommand {
    /// Verify the hash chain, vocabulary and envelope of the audit journal.
    Verify(AuditVerifyArgs),
    /// Show events or the head of the audit journal.
    Show(AuditShowArgs),
}

/// Arguments for `fieldbook audit verify`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct AuditVerifyArgs {
    /// Expected 64-character lowercase hexadecimal head hash.
    #[arg(long, value_name = "HEX")]
    pub expect_head: Option<String>,

    /// Expected event sequence length.
    #[arg(long, value_name = "N")]
    pub expect_seq: Option<u64>,

    /// Live-anchor sequence number (requires `--anchor-head`).
    #[arg(long, value_name = "N")]
    pub anchor_seq: Option<u64>,

    /// Live-anchor 64-character lowercase hexadecimal hash at `--anchor-seq`.
    #[arg(long, value_name = "HEX")]
    pub anchor_head: Option<String>,

    /// Move a broken chain aside to `journal.broken-<seq>` and start a new chain.
    #[arg(long)]
    pub repair: bool,

    /// Print the report as a JSON object.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `fieldbook audit show`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct AuditShowArgs {
    /// Number of newest matching events to show (default 50).
    #[arg(long, value_name = "N", default_value_t = 50)]
    pub last: usize,

    /// Filter events by event kind.
    #[arg(long, value_name = "K")]
    pub kind: Option<String>,

    /// Filter events by affected or referenced record ID.
    #[arg(long, value_name = "ID")]
    pub id: Option<String>,

    /// Filter events by label ID.
    #[arg(long, value_name = "L")]
    pub label: Option<String>,

    /// Show only the journal sequence count and head hash (`events` array is empty).
    #[arg(long)]
    pub head: bool,

    /// Print the report as a JSON object.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `fieldbook backup`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BackupArgs {
    /// Backup target identifier in settings.toml.
    #[arg(long, value_name = "ID")]
    pub target: Option<String>,

    /// Destination directory for a one-off backup.
    #[arg(long, value_name = "DIR")]
    pub to: Option<PathBuf>,

    /// Perform checks and report what would be written without writing anything.
    #[arg(long)]
    pub dry_run: bool,

    /// Backup subcommand (verify or list).
    #[command(subcommand)]
    pub command: Option<BackupSubcommand>,
}

/// Subcommands under `fieldbook backup`.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum BackupSubcommand {
    /// Verify a backup snapshot or raw copy.
    Verify(BackupVerifyArgs),
    /// List backup snapshots in a target.
    List(BackupListArgs),
}

/// Arguments for `fieldbook backup verify`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct BackupVerifyArgs {
    /// Backup target identifier in settings.toml.
    #[arg(long, value_name = "ID", conflicts_with = "in_dir")]
    pub target: Option<String>,

    /// Inspect snapshots in directory DIR.
    #[arg(long = "in", value_name = "DIR", conflicts_with = "target")]
    pub in_dir: Option<PathBuf>,

    /// Snapshot number to verify.
    #[arg(long, value_name = "N")]
    pub snapshot: Option<u64>,

    /// Verify a raw copy of data folders at PATH.
    #[arg(long, value_name = "PATH")]
    pub raw: Option<PathBuf>,

    /// Support catalogue evidence file.
    #[arg(long, value_name = "FILE")]
    pub support_catalogue: Option<PathBuf>,

    /// Expected SHA-256 digest of support catalogue evidence.
    #[arg(long, value_name = "HEX")]
    pub expect_support_sha256: Option<String>,
}

/// Arguments for `fieldbook backup list`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct BackupListArgs {
    /// Backup target identifier in settings.toml.
    #[arg(long, value_name = "ID", conflicts_with = "in_dir")]
    pub target: Option<String>,

    /// Inspect snapshots in directory DIR.
    #[arg(long = "in", value_name = "DIR", conflicts_with = "target")]
    pub in_dir: Option<PathBuf>,

    /// Print the report as a JSON object.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `fieldbook restore`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct RestoreArgs {
    /// Snapshot number to restore.
    #[arg(long, value_name = "N")]
    pub snapshot: u32,
    /// Restore into this new or empty folder.
    #[arg(long, value_name = "DIR", conflicts_with = "drill")]
    pub to: Option<PathBuf>,
    /// Restore into the drill folder under the state folder, check the copy, then delete it.
    #[arg(long)]
    pub drill: bool,
    /// Backup target identifier in settings.toml.
    #[arg(long, value_name = "ID", conflicts_with = "from")]
    pub target: Option<String>,
    /// Read the snapshot from this folder instead of a settings target.
    #[arg(long, value_name = "DIR")]
    pub from: Option<PathBuf>,
}

/// Arguments for `fieldbook policy`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct PolicyArgs {
    /// Policy subcommand (`show` or `check`).
    #[command(subcommand)]
    pub command: PolicySubcommand,
}

/// Subcommands under `fieldbook policy`.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum PolicySubcommand {
    /// Print the active policy state, rules, destination rows, and effective table.
    Show(PolicyShowArgs),
    /// Validate a draft policy file with no side effects.
    Check(PolicyCheckArgs),
}

/// Arguments for `fieldbook policy show`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct PolicyShowArgs {
    /// Print the policy report as a JSON object.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `fieldbook policy check`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct PolicyCheckArgs {
    /// Path to the draft policy file to validate.
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Print the validation result as a JSON object.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `fieldbook ai`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct AiArgs {
    /// AI subcommand (`status`).
    #[command(subcommand)]
    pub command: AiSubcommand,
}

/// Subcommands under `fieldbook ai`.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum AiSubcommand {
    /// Show AI settings, configured adapters, policy allow sets and key state.
    Status(AiStatusArgs),
}

/// Arguments for `fieldbook ai status`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct AiStatusArgs {
    /// Print the AI status report as a JSON object.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `fieldbook context`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct ContextArgs {
    /// Question text used to select and budget the context pack.
    #[arg(value_name = "QUESTION")]
    pub question: String,

    /// Restrict selection to one area (`area:<code>`) or lead with one page (`page:<id>`).
    #[arg(long, value_name = "SCOPE")]
    pub scope: Option<String>,

    /// Maximum Unicode-scalar character budget (defaults to `[assistant] pack_budget_chars`).
    #[arg(long, value_name = "N", allow_hyphen_values = true)]
    pub budget: Option<String>,

    /// Comma-separated exclusion overrides (`tier-d`, `superseded`, `contradicted`).
    #[arg(long, value_name = "CLASSES")]
    pub include: Option<String>,

    /// Print the context report as a JSON object.
    #[arg(long)]
    pub json: bool,
}
