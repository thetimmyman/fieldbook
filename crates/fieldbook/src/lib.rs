//! The `fieldbook` program: the only shipped crate that touches files, the
//! environment, the clock or exit codes.
//!
//! **Responsibility.** Parse the command line and run commands over the libraries
//! below it. Command logic lives in this library as functions over values (`run`), so tests need
//! no process. Process input and output live in the [`io`] module, which `main` calls.
//!
//! **Allowed dependencies.** `fieldbook-render`, `fieldbook-core` and `fieldbook-calc`.
//! External crates join through their cards, each at its pin in
//! `docs/reference/dependencies.md`.

pub mod cli;
pub mod io;

pub use cli::Cli;
pub use io::WorldError;

use std::ffi::OsString;
use std::path::{Component, Path};

use clap::Parser;
use fieldbook_core::CivilDate;
use fieldbook_core::check::{self, Host, Options, Report, Tree};
use fieldbook_core::report::CheckReport;

use crate::cli::{CheckArgs, Commands, EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE};

/// The environment seam supplied by [`io::Disk`] in production or a test double in unit tests.
pub trait World {
    /// Read the in-memory Corpus [`Tree`] from [`Self::tree_root`].
    fn read_tree(&self) -> Result<Tree, WorldError>;

    /// Return today's calendar date from the system clock when `--today` is not provided.
    fn today(&self) -> CivilDate;

    /// Return the working directory captured at the start of the run.
    fn cwd(&self) -> &Path;

    /// Return the Corpus root directory (the directory containing `corpus/`).
    fn tree_root(&self) -> &Path;
}

/// What a command produced: text for each stream and a process exit code.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Text for standard output.
    pub stdout: String,
    /// Text for standard error.
    pub stderr: String,
    /// The exit code: 0 clean, 1 findings, 2 usage, 3 I/O.
    pub code: u8,
}

/// The build identity: the git commit when `FIELDBOOK_GIT_SHA` was set at build time.
#[must_use]
pub fn build_identity() -> &'static str {
    identity(option_env!("FIELDBOOK_GIT_SHA"))
}

fn identity(git_sha: Option<&'static str>) -> &'static str {
    match git_sha {
        Some(sha) if !sha.is_empty() => sha,
        _ => fieldbook_core::strings::unreleased_build(),
    }
}

/// The line `--version` prints, without its newline.
#[must_use]
pub fn version_line() -> String {
    format!(
        "{} {} ({})",
        fieldbook_core::strings::binary(),
        env!("CARGO_PKG_VERSION"),
        build_identity()
    )
}

/// Format a [`Report`] as the `--json` payload of `fieldbook check`.
#[must_use]
pub fn render_check_json(report: &Report, strict: bool, today: CivilDate) -> String {
    let payload = CheckReport::from_report(report, strict, today);
    let mut json = serde_json::to_string_pretty(&payload).unwrap_or_default();
    json.push('\n');
    json
}

/// Run the command named by `args` (the arguments after the program name) against `world`.
#[must_use]
pub fn run(args: &[String], world: &dyn World) -> Outcome {
    if let [only] = args
        && only == "--version"
    {
        return Outcome {
            stdout: format!("{}\n", version_line()),
            stderr: String::new(),
            code: EXIT_OK,
        };
    }

    let argv =
        std::iter::once(fieldbook_core::strings::binary()).chain(args.iter().map(String::as_str));
    let parsed = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: err.to_string(),
                code: EXIT_USAGE,
            };
        }
    };
    if parsed.version {
        let mut cmd = <Cli as clap::CommandFactory>::command();
        return Outcome {
            stdout: String::new(),
            stderr: format!("{}\n", cmd.render_usage()),
            code: EXIT_USAGE,
        };
    }

    match parsed.command {
        Commands::Check(check_args) => run_check(&check_args, world),
    }
}

fn run_check(args: &CheckArgs, world: &dyn World) -> Outcome {
    let today = args.today.unwrap_or_else(|| world.today());
    let tree = match world.read_tree() {
        Ok(tree) => tree,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };

    let host = Host::new(tree, today);
    let options = Options::new(args.strict);
    let mut report = check::run(&host, &options);

    if !args.paths.is_empty() {
        let filters: Vec<Vec<OsString>> = args
            .paths
            .iter()
            .map(|p| normalize_against(world.cwd(), p))
            .collect();
        let tree_root = world.tree_root();
        report.diagnostics.retain(|diag| {
            let diag_components = normalize_against(tree_root, Path::new(&diag.path));
            filters
                .iter()
                .any(|filter| components_match_or_descend(&diag_components, filter))
        });
    }

    let code = if report.is_ok() {
        EXIT_OK
    } else {
        EXIT_FINDINGS
    };
    let stdout = if args.json {
        render_check_json(&report, args.strict, today)
    } else {
        report.render_text()
    };

    Outcome {
        stdout,
        stderr: String::new(),
        code,
    }
}

fn normalize_against(base: &Path, path: &Path) -> Vec<OsString> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let mut out: Vec<OsString> = Vec::new();
    let mut rooted = false;
    for component in joined.components() {
        match component {
            Component::Prefix(prefix) => {
                out.push(prefix.as_os_str().to_os_string());
            }
            Component::RootDir => {
                rooted = true;
                out.push(component.as_os_str().to_os_string());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = out.last() {
                    let is_root = rooted && out.len() == 1;
                    if !is_root && last != ".." {
                        out.pop();
                        continue;
                    }
                }
                if !rooted {
                    out.push(OsString::from(".."));
                }
            }
            Component::Normal(part) => {
                out.push(part.to_os_string());
            }
        }
    }
    out
}

fn components_match_or_descend(path: &[OsString], filter: &[OsString]) -> bool {
    path.len() >= filter.len() && path[..filter.len()] == *filter
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use fieldbook_core::CivilDate;
    use fieldbook_core::check::Tree;

    use super::{World, WorldError, identity, run, version_line};

    struct FakeWorld {
        cwd: PathBuf,
        tree_root: PathBuf,
    }

    impl Default for FakeWorld {
        fn default() -> Self {
            Self {
                cwd: PathBuf::from("/repo"),
                tree_root: PathBuf::from("/repo"),
            }
        }
    }

    impl World for FakeWorld {
        fn read_tree(&self) -> Result<Tree, WorldError> {
            Ok(Tree::new())
        }

        fn today(&self) -> CivilDate {
            CivilDate::from_ymd(2026, 3, 1).expect("valid date")
        }

        fn cwd(&self) -> &Path {
            &self.cwd
        }

        fn tree_root(&self) -> &Path {
            &self.tree_root
        }
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn version_prints_one_line_and_exits_clean() {
        let world = FakeWorld::default();
        let outcome = run(&args(&["--version"]), &world);
        assert_eq!(outcome.code, 0);
        assert_eq!(outcome.stdout, format!("{}\n", version_line()));
        assert!(outcome.stderr.is_empty());
    }

    #[test]
    fn no_arguments_print_usage_and_exit_two() {
        let world = FakeWorld::default();
        let outcome = run(&[], &world);
        assert_eq!(outcome.code, 2);
        assert!(outcome.stdout.is_empty());
        assert!(
            outcome
                .stderr
                .to_ascii_lowercase()
                .contains("usage: fieldbook")
        );
    }

    #[test]
    fn anything_else_prints_usage_and_exits_two() {
        let world = FakeWorld::default();
        for other in [&["--help"][..], &["--version", "extra"], &["-V"]] {
            assert_eq!(run(&args(other), &world).code, 2, "{other:?}");
        }
    }

    #[test]
    fn identity_is_the_commit_or_unreleased() {
        assert_eq!(identity(Some("0123abc")), "0123abc");
        assert_eq!(
            identity(Some("")),
            fieldbook_core::strings::unreleased_build()
        );
        assert_eq!(identity(None), fieldbook_core::strings::unreleased_build());
    }
}
