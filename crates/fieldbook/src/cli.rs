//! Command-line argument definition (`clap` derive `Cli`, `AD-17`).

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fieldbook_core::CivilDate;

/// Clean run with zero error findings (`AD-17`).
pub const EXIT_OK: u8 = 0;

/// Run completed with at least one error finding (`AD-17`).
pub const EXIT_FINDINGS: u8 = 1;

/// Command-line usage error or invalid argument (`AD-17`).
pub const EXIT_USAGE: u8 = 2;

/// Input or output failure (`AD-17`).
pub const EXIT_IO: u8 = 3;

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

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level subcommands (`AD-17`).
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Commands {
    /// Check the Corpus and print findings.
    Check(CheckArgs),
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
