//! Development automation for the repository. It is never shipped.
//!
//! This is the only crate that starts processes and reads the environment besides the
//! `fieldbook` binary, so it lifts those bans below. Commands:
//!
//! - `doctor [--ci]`: check the installed tools against `.config/tools.toml`.
//! - `setup [--no-hooks] [TOOL...]`: install the pinned tools and enable the git hooks.
//! - `guard-statuses BASE HEAD`: fail when the agent's commits touch protected paths.
//! - `denylist FILE`: fail when a term from FILE appears in tracked content or pushed history.
//!
//! Exit codes follow `fieldbook`: 0 clean, 1 findings, 2 usage, 3 an operation failed.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "xtask is a command-line tool; its output is the product"
)]
#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "xtask starts git, cargo and curl, and reads the environment and files"
)]

mod denylist;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the checker consumes budgets next")
)]
mod deps_budget;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the complete checker consumes graph measurements")
)]
mod deps_graph;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the complete dependency checker consumes these configuration facts"
    )
)]
mod deps_inputs;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the graph measurement consumer follows this metadata producer"
    )
)]
mod deps_metadata;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the full dependency-input consumer registers this producer"
    )
)]
mod deps_references;
#[cfg(test)]
mod graph;
mod guard;
mod guard_foundation;
mod proc;
mod tools;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <command>
  doctor [--ci]                     check the installed tools against .config/tools.toml
  setup [--no-hooks] [TOOL...]      install the pinned tools and enable the git hooks
  guard-statuses BASE HEAD          fail if commits by the agent author touch corpus/ or seed-* files
  denylist FILE                     fail if a term from FILE appears in tracked content or pushed history";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = proc::repo_root();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["doctor"] => tools::doctor(&root, ci_from_env()),
        ["doctor", "--ci"] => tools::doctor(&root, true),
        ["guard-statuses", base, head] => guard::command(&root, base, head),
        ["denylist", file] => denylist::command(&root, &PathBuf::from(file)),
        ["denylist", file, "--updates", updates] => {
            denylist::pushed(&root, &PathBuf::from(file), &PathBuf::from(updates))
        }
        ["setup", rest @ ..] => tools::setup(&root, rest),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(3)
        }
    }
}

/// CI runs check exact tool versions; a developer machine checks minimums.
fn ci_from_env() -> bool {
    std::env::var_os("CI").is_some_and(|value| !value.is_empty())
}
