//! Development automation for the repository. It is never shipped.
//!
//! This is the only crate that starts processes and reads the environment besides the
//! `fieldbook` binary, so it lifts those bans below. Commands:
//!
//! - `doctor [--ci]`: check the installed tools against `.config/tools.toml`.
//! - `setup [--no-hooks] [TOOL...]`: install the pinned tools and enable the git hooks.
//! - `docs [--check]`: regenerate generated documentation blocks or check for drift.
//! - `cards --check [--closing SLUG]`: lint the card files, PLAN.md, links, AGENTS.md and the documents (writes nothing).
//! - `new-card SLUG`: start `docs/cards/SLUG.md` from the template in `docs/cards/README.md`.
//! - `scope BASE HEAD`: fail when a pull request that deletes a card changes a path the card does not list.
//! - `guard-statuses BASE HEAD`: fail when an agent's commits certify their own work.
//! - `denylist FILE`: fail when a term from FILE appears in tracked content or pushed history.
//! - `edition`: check and build the Corpus, run the browser harness on that tree and write the edition and originals zips into `out/dist`.
//! - `edition-compare --left DIR --right DIR`: verify two delivery folders and require equal payloads, apart from the browser attestations.
//! - `edition-raw --out DIR`: package the raw edition floor from a clean commit.
//! - `raw-edition-check TAG --base FILE --evidence-out DIR`: check a raw edition tag and collect its evidence.
//! - `raw-edition-compare LOCAL DOWNLOADED`: require a downloaded raw delivery set to equal the local one.
//! - `harness [--edition DIR]`: open every page of an edition in headless Edge (or Chromium) with a net-log.
//! - `hosted-limits DIR`: fail if a file in DIR is over 25 MiB or DIR holds more than 20,000 files (Cloudflare Workers static assets limits).
//! - `extract VERSION...` and `extract --source VERSION --text FILE`: write corpus/sources/text/<VERSION>/ (one text file per page and extraction.toml) from the stored original, at home; the second form attaches supplied text.
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
mod cards;
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
mod docs_lints;
mod edition;
mod edition_raw;
mod edition_release;
mod errors;
mod extract;
mod golden_wrap;
#[cfg(test)]
mod graph;
mod guard;
mod guard_fields;
mod harness;
mod hosted;
mod links;
#[cfg(test)]
mod literals;
mod proc;
mod raw_release;
mod scope;
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
  cards --check [--closing SLUG]    lint the cards, PLAN.md, links, AGENTS.md and the documents (writes nothing)
  new-card SLUG                     start docs/cards/SLUG.md from the template in docs/cards/README.md
  scope BASE HEAD                   fail if a pull request that deletes a card changes a path its Files and Registration lines do not list
  guard-statuses BASE HEAD          fail if an agent's commits raise a status or weaken a seed or regression case
  denylist FILE                     fail if a term from FILE appears in tracked content or pushed history
  compat --base FILE                verify published compatibility bindings and vectors against FILE
  binscan EXE --build silent|ai     check imported OS symbols of EXE against its allowlist and group bans
  archive-release [FLAGS]           copy a verified release set and compatibility evidence into a durable archive
  verify-archive DIR [FLAGS]        reopen and verify a durable archive and its stream predecessor
  edition                           check, build, test in the browser and package the edition and originals zips into out/dist
  edition-compare --left DIR --right DIR
                                    verify two delivery folders and require equal payloads, apart from the browser attestations
  edition-raw --out DIR [FLAGS]     package the raw edition floor (paired archives) from a clean commit
  raw-edition-check TAG --base FILE --evidence-out DIR
                                    check an owner-created raw edition tag and collect its evidence
  raw-edition-compare LOCAL DOWNLOADED
                                    require a downloaded raw delivery set to equal the local one
  edition-impact [FLAGS]            inspect tagged editions and verified archives for a source version
  harness [--edition DIR]           open every page and renderable original in headless Edge (or Chromium) with a net-log
  hosted-limits DIR                 fail if a file in DIR is over 25 MiB or DIR holds more than 20,000 files (Cloudflare Workers static assets limits)
  extract VERSION...                write the text pages and extraction.toml of stored originals (home only; PDFs need pdftotext)
  extract --source VERSION --text FILE
                                    attach supplied UTF-8 text to a stored original of any type (method supplied)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = std::env::var_os("XTASK_REPO_ROOT").map_or_else(proc::repo_root, PathBuf::from);
    if let Err(message) = raw_release::register_raw_provider() {
        eprintln!("error: {message}");
        return ExitCode::from(3);
    }
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
        ["cards", rest @ ..] => cards::command(&root, rest),
        ["new-card", slug] => cards::new_card(&root, slug),
        ["scope", base, head] => scope::command(&root, base, head),
        ["guard-statuses", base, head] => guard::command(&root, base, head),
        ["denylist", file] => denylist::command(&root, &PathBuf::from(file)),
        ["denylist", file, "--updates", updates] => {
            denylist::pushed(&root, &PathBuf::from(file), &PathBuf::from(updates))
        }
        ["compat", "--base", file] => compat::command(&root, &PathBuf::from(file)),
        ["binscan", exe, "--build", build] => binscan::command(&root, &PathBuf::from(exe), build),
        ["archive-release", rest @ ..] => edition_release::command_archive_release(rest),
        ["verify-archive", rest @ ..] => edition_release::command_verify_archive(rest),
        ["edition", rest @ ..] => edition::command_edition(&root, rest),
        ["edition-compare", rest @ ..] => edition::command_edition_compare(rest),
        ["edition-raw", rest @ ..] => edition_raw::command_edition_raw(&root, rest),
        ["raw-edition-check", rest @ ..] => raw_release::command_raw_edition_check(&root, rest),
        ["raw-edition-compare", rest @ ..] => raw_release::command_raw_edition_compare(rest),
        ["edition-impact", rest @ ..] => edition_release::command_edition_impact(&root, rest),
        ["harness", rest @ ..] => harness::command(&root, rest),
        ["hosted-limits", dir] => hosted::command(&PathBuf::from(dir)),
        ["setup", rest @ ..] => tools::setup(&root, rest),
        ["extract", "--source", version, "--text", file] => {
            extract::command_supplied(&root, version, &PathBuf::from(file))
        }
        ["extract", versions @ ..]
            if !versions.is_empty() && versions.iter().all(|v| !v.starts_with('-')) =>
        {
            extract::command(&root, versions)
        }
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
