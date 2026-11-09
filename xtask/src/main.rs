//! Development automation for the repository. It is never shipped.
//!
//! This is the only crate that starts processes and reads the environment besides the
//! `fieldbook` binary, so it lifts those bans below. Commands:
//!
//! - `doctor [--ci]`: check the installed tools against `.config/tools.toml`.
//! - `setup [--no-hooks] [TOOL...]`: install the pinned tools and enable the git hooks.
//! - `docs [--check]`: regenerate generated documentation blocks or check for drift.
//! - `guard-statuses BASE HEAD`: fail when an agent's commits certify their own work.
//! - `denylist FILE`: fail when a term from FILE appears in tracked content or pushed history.
//! - `harness [--edition DIR]`: open every page of an edition in headless Edge (or Chromium) with a net-log.
//! - `hosted-limits DIR`: fail if a file in DIR is over 25 MiB or DIR holds more than 20,000 files (Cloudflare Workers static assets limits).
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
    reason = "xtask starts git, cargo, curl and the browser, and reads the environment and files"
)]

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the packaging consumers and edition_release use allowlist helpers"
    )
)]
mod allowlist;
mod audit_doc;
mod binscan;
mod cli_doc;
mod compat;
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
mod docs;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "raw-edition and release-workflow register their archive providers next"
    )
)]
mod edition_release;
mod errors;
mod golden_wrap;
#[cfg(test)]
mod graph;
mod guard;
mod guard_fields;
mod harness;
mod hosted;
#[cfg(test)]
mod literals;
mod proc;
mod tools;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the packaging consumers and edition_release use zipdet helpers"
    )
)]
mod zipdet;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <command>
  doctor [--ci]                     check the installed tools against .config/tools.toml
  setup [--no-hooks] [TOOL...]      install the pinned tools and enable the git hooks
  docs [--check]                    regenerate generated documentation blocks or check for drift
  guard-statuses BASE HEAD          fail if an agent's commits raise a status or weaken a seed or regression case
  denylist FILE                     fail if a term from FILE appears in tracked content or pushed history
  compat --base FILE                verify published compatibility bindings and vectors against FILE
  binscan EXE --build silent|ai     check imported OS symbols of EXE against its allowlist and group bans
  archive-release [FLAGS]           copy a verified release set and compatibility evidence into a durable archive
  verify-archive DIR [FLAGS]        reopen and verify a durable archive and its stream predecessor
  edition-impact [FLAGS]            inspect tagged editions and verified archives for a source version
  harness [--edition DIR]           open every page and renderable original in headless Edge (or Chromium) with a net-log
  hosted-limits DIR                 fail if a file in DIR is over 25 MiB or DIR holds more than 20,000 files (Cloudflare Workers static assets limits)";

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
        ["docs"] => return docs::command(&root, false),
        ["docs", "--check"] => return docs::command(&root, true),
        ["guard-statuses", base, head] => guard::command(&root, base, head),
        ["denylist", file] => denylist::command(&root, &PathBuf::from(file)),
        ["denylist", file, "--updates", updates] => {
            denylist::pushed(&root, &PathBuf::from(file), &PathBuf::from(updates))
        }
        ["compat", "--base", file] => compat::command(&root, &PathBuf::from(file)),
        ["binscan", exe, "--build", build] => binscan::command(&root, &PathBuf::from(exe), build),
        ["archive-release", rest @ ..] => edition_release::command_archive_release(rest),
        ["verify-archive", rest @ ..] => edition_release::command_verify_archive(rest),
        ["edition-impact", rest @ ..] => edition_release::command_edition_impact(&root, rest),
        ["harness", rest @ ..] => harness::command(&root, rest),
        ["hosted-limits", dir] => hosted::command(&PathBuf::from(dir)),
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
