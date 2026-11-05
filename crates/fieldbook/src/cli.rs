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

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level subcommands (`AD-17`).
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Commands {
    /// Check the Corpus and print findings.
    Check(CheckArgs),
    /// Add a source or source version to the Corpus.
    Add(AddArgs),
    /// Inspect or verify the audit journal.
    Audit(AuditArgs),
}

/// Arguments for `fieldbook add`.
#[derive(Debug, Clone, PartialEq, Eq, clap::Args)]
pub struct AddArgs {
    /// Path to the source file to add.
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
