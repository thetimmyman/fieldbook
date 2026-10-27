//! The `fieldbook` program: the only shipped crate that touches files, the
//! environment, the clock or exit codes.
//!
//! **Responsibility.** Parse the command line and run commands over the libraries
//! below it. The commands arrive with their cards; for now `--version` is the only one.
//! Command logic lives in this library as functions over values (`run`), so tests need
//! no process. Process input and output live in the [`io`] module, which `main` calls.
//!
//! **Allowed dependencies.** `fieldbook-render`, `fieldbook-core` and `fieldbook-calc`.
//! External crates join through their cards, each at its pin in
//! `docs/reference/dependencies.md`.

pub mod io;

/// What `--version` prints for a build made outside a release: no git commit was given.
const UNRELEASED: &str = "unreleased build";

/// The text printed when the arguments are not understood.
const USAGE: &str = "usage: fieldbook --version\n";

/// What a command produced: text for each stream and a process exit code.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Text for standard output.
    pub stdout: String,
    /// Text for standard error.
    pub stderr: String,
    /// The exit code: 0 clean, 2 usage.
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
        _ => UNRELEASED,
    }
}

/// The line `--version` prints, without its newline.
#[must_use]
pub fn version_line() -> String {
    format!(
        "fieldbook {} ({})",
        env!("CARGO_PKG_VERSION"),
        build_identity()
    )
}

/// Run the command named by `args` (the arguments after the program name).
#[must_use]
pub fn run(args: &[String]) -> Outcome {
    match args {
        [only] if only == "--version" => Outcome {
            stdout: format!("{}\n", version_line()),
            stderr: String::new(),
            code: 0,
        },
        _ => Outcome {
            stdout: String::new(),
            stderr: USAGE.to_owned(),
            code: 2,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{identity, run, version_line};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn version_prints_one_line_and_exits_clean() {
        let outcome = run(&args(&["--version"]));
        assert_eq!(outcome.code, 0);
        assert_eq!(outcome.stdout, format!("{}\n", version_line()));
        assert!(outcome.stderr.is_empty());
    }

    #[test]
    fn no_arguments_print_usage_and_exit_two() {
        let outcome = run(&[]);
        assert_eq!(outcome.code, 2);
        assert!(outcome.stdout.is_empty());
        assert!(outcome.stderr.starts_with("usage: fieldbook"));
    }

    #[test]
    fn anything_else_prints_usage_and_exits_two() {
        for other in [
            &["--help"][..],
            &["check"],
            &["--version", "extra"],
            &["-V"],
        ] {
            assert_eq!(run(&args(other)).code, 2, "{other:?}");
        }
    }

    #[test]
    fn identity_is_the_commit_or_unreleased() {
        assert_eq!(identity(Some("0123abc")), "0123abc");
        assert_eq!(identity(Some("")), "unreleased build");
        assert_eq!(identity(None), "unreleased build");
    }
}
