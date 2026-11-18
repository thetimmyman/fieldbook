//! The `fieldbook` program: the only shipped crate that touches files, the
//! environment, the clock or exit codes.
//!
//! **Responsibility.** Parse the command line and run commands over the libraries
//! below it. Command logic lives in this library as functions over values (`run`), so tests need
//! no process. Process input and output live in the [`io`] module, which `main` calls.
//!
//! **Allowed dependencies.** `fieldbook-render`, `fieldbook-core`, `fieldbook-calc` and `fieldbook-win`.
//! External crates join through their cards, each at its pin in
//! `docs/reference/dependencies.md`.

pub mod adapters;
pub mod add;
pub mod asked;
pub mod audit;
pub mod backup;
pub mod build;
pub mod cli;
pub mod ctx;
pub mod io;
pub mod new;
pub mod pages;
pub mod report;
pub mod restore;
pub mod search;
pub mod since;
pub mod status;
pub mod verbs;
pub mod verify;

pub use backup::render_backup_list_json;
pub use cli::Cli;
pub use fieldbook_core::labels::PlaceFacts;
pub use fieldbook_core::manifest::SupportFacts;
pub use io::{Audited, Error, StoreLease, WorldError};

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use clap::Parser;
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{Now, Principal, Sha256};
use fieldbook_core::check::{self, Host, Options, Report, Tree};
use fieldbook_core::egress::Allowed;
use fieldbook_core::report::CheckReport;

use crate::cli::{CheckArgs, Commands, EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE};

/// The environment seam supplied by [`io::Disk`] in production or a test double in unit tests.
pub trait World {
    /// Read a coherent in-memory Corpus [`Tree`] from [`Self::tree_root`].
    fn read_tree(&self) -> Result<Tree, WorldError>;

    /// Acquire an exclusive [`StoreLease`] over `state/store.lock`.
    fn write_lease(&self) -> Result<StoreLease, WorldError>;

    /// Read the in-memory Corpus [`Tree`] under an already-held [`StoreLease`] without relocking.
    fn read_tree_under(&self, lease: &StoreLease) -> Result<Tree, WorldError>;

    /// Read raw bytes from `path` if it exists; return `Ok(None)` when absent.
    fn read_file(&self, path: &Path) -> Result<Option<Vec<u8>>, WorldError>;

    /// Atomically write `bytes` to `path` under an active [`Audited`] token.
    fn write_file(
        &self,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError>;

    /// Atomically write `bytes` to `path` outside the data roots under `allowed` and `audited`.
    fn write_outside(
        &self,
        allowed: &Allowed,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError>;

    /// Return whether a company store exists on this layout (`false` on the Home layout).
    fn has_company_store(&self) -> bool;

    /// Return today's calendar date from the system clock when `--today` is not provided.
    fn today(&self) -> CivilDate;

    /// Return the current UTC event timestamp (`Now`).
    fn now(&self) -> Now;

    /// Return the OS or local principal (`Principal`).
    fn principal(&self) -> Principal;

    /// Return the working directory captured at the start of the run.
    fn cwd(&self) -> &Path;

    /// Return the Corpus root directory (the directory containing `corpus/`).
    fn tree_root(&self) -> &Path;

    /// Return the resolved default state directory (`.fieldbook/` beside the Corpus unless overridden).
    fn state_dir(&self) -> &Path;

    /// Return whether standard input is connected to an interactive terminal.
    fn stdin_is_terminal(&self) -> bool;

    /// Read one line from standard input, without its line ending; empty at end of input.
    fn read_line(&self) -> String;

    /// Write `<flag>: ` to standard error and flush it before the caller reads a line.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError`] when the prompt cannot be written or flushed.
    fn prompt(&self, flag: &str) -> Result<(), WorldError>;

    /// Return folder placement facts for `path`.
    fn place(&self, path: &Path) -> PlaceFacts;

    /// Return facts about public support provenance: retained verified catalogues and completed public writes.
    fn support_facts(&self) -> SupportFacts;

    /// Reset the drill workspace under `<state>/drill`.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError`] if the drill workspace cannot be reset.
    fn reset_drill(&self, audited: &Audited) -> Result<(), WorldError>;

    /// Remove the drill workspace under `<state>/drill`.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError`] if the drill workspace cannot be removed.
    fn remove_drill(&self, audited: &Audited) -> Result<(), WorldError>;

    /// Return the registry id of the Store adapter that this world writes through.
    ///
    /// Every world names its store: `folder@1` for the folder store that [`io::Disk`] is,
    /// `memory@0` for an in-memory double, or the inner world's id for a double that wraps one.
    /// There is no default, so a new double cannot silently pass as a folder store.
    fn store_id(&self) -> &'static str;

    /// Return the operation's effective policy (`Floor` until a policy loader overrides it).
    fn effective_policy(&self) -> fieldbook_core::policy::EffectivePolicy {
        fieldbook_core::policy::EffectivePolicy::floor(self.has_company_store())
    }

    /// Read `docs/maintenance.csv` from the repository root.
    ///
    /// An absent file is the empty string: `status` prints `Unavailable` and does not fail.
    fn read_maintenance(&self) -> Result<String, WorldError>;

    /// Read the state lines `status` prints after its sections.
    ///
    /// A part whose data is absent or cannot be read adds no line and is never an error.
    fn read_state_summary(
        &self,
        today: fieldbook_core::CivilDate,
    ) -> Result<fieldbook_core::status::StateSummary, WorldError>;
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

/// Extract the `--state` flag and [`crate::cli::BuildArgs`] if `args` invokes `build --watch`.
#[must_use]
pub fn build_watch_args(args: &[String]) -> Option<(Option<PathBuf>, crate::cli::BuildArgs)> {
    let argv =
        std::iter::once(fieldbook_core::strings::binary()).chain(args.iter().map(String::as_str));
    let parsed = Cli::try_parse_from(argv).ok()?;
    match parsed.command {
        Commands::Build(build_args) if build_args.watch => Some((parsed.state, build_args)),
        _ => None,
    }
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
    io::journal::reset_thread_wrote_store();
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

    let mut outcome = match parsed.command {
        Commands::Check(check_args) => {
            if let Some(flag) = parsed.state.as_deref() {
                let state_dir = io::state::resolve(world.tree_root(), world.cwd(), Some(flag));
                let overridden = StateOverrideWorld {
                    inner: world,
                    state_dir,
                };
                run_check(&check_args, &overridden)
            } else {
                run_check(&check_args, world)
            }
        }
        Commands::Add(add_args) => {
            if let Some(flag) = parsed.state.as_deref() {
                let state_dir = io::state::resolve(world.tree_root(), world.cwd(), Some(flag));
                let overridden = StateOverrideWorld {
                    inner: world,
                    state_dir,
                };
                add::run(&add_args, &overridden)
            } else {
                add::run(&add_args, world)
            }
        }
        Commands::Build(build_args) => {
            if build_args.watch {
                return Outcome {
                    stdout: String::new(),
                    stderr: "error: --watch needs a terminal; run it from the program\n"
                        .to_string(),
                    code: EXIT_USAGE,
                };
            }
            if let Some(flag) = parsed.state.as_deref() {
                let state_dir = io::state::resolve(world.tree_root(), world.cwd(), Some(flag));
                let overridden = StateOverrideWorld {
                    inner: world,
                    state_dir,
                };
                build::run(&build_args, &overridden)
            } else {
                build::run(&build_args, world)
            }
        }
        Commands::New(new_args) => {
            if let Some(flag) = parsed.state.as_deref() {
                let state_dir = io::state::resolve(world.tree_root(), world.cwd(), Some(flag));
                let overridden = StateOverrideWorld {
                    inner: world,
                    state_dir,
                };
                new::run(&new_args, &overridden)
            } else {
                new::run(&new_args, world)
            }
        }
        Commands::Audit(audit_args) => audit::run(&audit_args, parsed.state.as_deref(), world),
        Commands::Search(search_args) => {
            if let Some(flag) = parsed.state.as_deref() {
                let state_dir = io::state::resolve(world.tree_root(), world.cwd(), Some(flag));
                let overridden = StateOverrideWorld {
                    inner: world,
                    state_dir,
                };
                search::run(&search_args, &overridden)
            } else {
                search::run(&search_args, world)
            }
        }
        Commands::Status(status_args) => {
            let policy = world.effective_policy();
            if let Some(flag) = parsed.state.as_deref() {
                let state_dir = io::state::resolve(world.tree_root(), world.cwd(), Some(flag));
                let overridden = StateOverrideWorld {
                    inner: world,
                    state_dir,
                };
                status::run_with_policy(&status_args, &overridden, &policy)
            } else {
                status::run_with_policy(&status_args, world, &policy)
            }
        }
        Commands::Verify(verify_args) => {
            if let Some(flag) = parsed.state.as_deref() {
                let state_dir = io::state::resolve(world.tree_root(), world.cwd(), Some(flag));
                let overridden = StateOverrideWorld {
                    inner: world,
                    state_dir,
                };
                verify::run(&verify_args, &overridden)
            } else {
                verify::run(&verify_args, world)
            }
        }
        Commands::Backup(backup_args) => backup::run(&backup_args, parsed.state.as_deref(), world),
        Commands::Restore(restore_args) => {
            restore::run(&restore_args, parsed.state.as_deref(), world)
        }
    };

    if io::journal::thread_wrote_store() && (outcome.code == 0 || outcome.code == 1) {
        let state_dir = if let Some(flag) = parsed.state.as_deref() {
            io::state::resolve(world.tree_root(), world.cwd(), Some(flag))
        } else {
            world.state_dir().to_path_buf()
        };
        let settings = io::state::read_backup_settings(&state_dir).unwrap_or_default();
        if let Some(msg) = backup::reminder(&state_dir, world.today(), settings.warn_after_days) {
            outcome.stderr.push_str(&msg);
            if !outcome.stderr.ends_with('\n') {
                outcome.stderr.push('\n');
            }
        }
    }

    outcome
}

pub(crate) struct StateOverrideWorld<'a> {
    pub(crate) inner: &'a dyn World,
    pub(crate) state_dir: PathBuf,
}

impl World for StateOverrideWorld<'_> {
    fn read_tree(&self) -> Result<Tree, WorldError> {
        io::journal::read_tree_coherent(self.inner.tree_root(), &self.state_dir)
    }

    fn write_lease(&self) -> Result<StoreLease, WorldError> {
        io::journal::acquire_write_lease(self.inner.tree_root(), &self.state_dir)
    }

    fn read_tree_under(&self, lease: &StoreLease) -> Result<Tree, WorldError> {
        io::journal::read_tree_under_lease(self.inner.tree_root(), &self.state_dir, lease)
    }

    fn read_file(&self, path: &Path) -> Result<Option<Vec<u8>>, WorldError> {
        self.inner.read_file(path)
    }

    fn write_file(
        &self,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError> {
        let disk = io::Disk::with_state(self.inner.tree_root(), &self.state_dir);
        disk.write_file(audited, path, bytes)
    }

    fn write_outside(
        &self,
        allowed: &Allowed,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, WorldError> {
        self.inner.write_outside(allowed, audited, path, bytes)
    }

    fn has_company_store(&self) -> bool {
        self.inner.has_company_store()
    }

    fn today(&self) -> CivilDate {
        self.inner.today()
    }

    fn now(&self) -> Now {
        self.inner.now()
    }

    fn principal(&self) -> Principal {
        self.inner.principal()
    }

    fn cwd(&self) -> &Path {
        self.inner.cwd()
    }

    fn tree_root(&self) -> &Path {
        self.inner.tree_root()
    }

    fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    fn stdin_is_terminal(&self) -> bool {
        self.inner.stdin_is_terminal()
    }

    fn read_line(&self) -> String {
        self.inner.read_line()
    }

    fn prompt(&self, flag: &str) -> Result<(), WorldError> {
        self.inner.prompt(flag)
    }

    fn place(&self, path: &Path) -> PlaceFacts {
        self.inner.place(path)
    }

    fn store_id(&self) -> &'static str {
        self.inner.store_id()
    }

    fn effective_policy(&self) -> fieldbook_core::policy::EffectivePolicy {
        self.inner.effective_policy()
    }

    fn read_maintenance(&self) -> Result<String, WorldError> {
        self.inner.read_maintenance()
    }

    fn read_state_summary(
        &self,
        today: fieldbook_core::CivilDate,
    ) -> Result<fieldbook_core::status::StateSummary, WorldError> {
        Ok(io::state_summary(&self.state_dir, today))
    }

    /// Support facts follow the selected state directory specified by `--state`.
    ///
    /// Facts that equal what the default State yields are derived from the default State:
    /// ignore them; anything else was injected by a test world: keep it. Otherwise,
    /// support facts are read from the selected State directory only.
    fn support_facts(&self) -> SupportFacts {
        let selected =
            io::Disk::with_state(self.inner.tree_root(), &self.state_dir).support_facts();
        let inner_default =
            io::Disk::with_state(self.inner.tree_root(), self.inner.state_dir()).support_facts();
        let inner = self.inner.support_facts();
        // facts that equal what the default State yields are derived from the default State: ignore them;
        // anything else was injected by a test world: keep it.
        if inner == inner_default {
            selected
        } else {
            inner
        }
    }

    fn reset_drill(&self, audited: &Audited) -> Result<(), WorldError> {
        let disk = io::Disk::with_state(self.inner.tree_root(), &self.state_dir);
        disk.reset_drill(audited)
    }

    fn remove_drill(&self, audited: &Audited) -> Result<(), WorldError> {
        let disk = io::Disk::with_state(self.inner.tree_root(), &self.state_dir);
        disk.remove_drill(audited)
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
    use fieldbook_core::audit::{Now, Principal};
    use fieldbook_core::check::Tree;

    use super::{
        PlaceFacts, StoreLease, SupportFacts, World, WorldError, identity, run, version_line,
    };

    struct FakeWorld {
        cwd: PathBuf,
        tree_root: PathBuf,
        state_dir: PathBuf,
    }

    impl Default for FakeWorld {
        fn default() -> Self {
            Self {
                cwd: PathBuf::from("/repo"),
                tree_root: PathBuf::from("/repo"),
                state_dir: PathBuf::from("/repo/.fieldbook"),
            }
        }
    }

    impl World for FakeWorld {
        fn read_tree(&self) -> Result<Tree, WorldError> {
            Ok(Tree::new())
        }

        fn write_lease(&self) -> Result<StoreLease, WorldError> {
            Err(WorldError::new(&self.state_dir, "unsupported in FakeWorld"))
        }

        fn read_tree_under(&self, _lease: &StoreLease) -> Result<Tree, WorldError> {
            Ok(Tree::new())
        }

        fn read_file(&self, _path: &Path) -> Result<Option<Vec<u8>>, WorldError> {
            Ok(None)
        }

        fn write_file(
            &self,
            _audited: &super::Audited,
            path: &Path,
            _bytes: &[u8],
        ) -> Result<fieldbook_core::audit::Sha256, WorldError> {
            Err(WorldError::new(path, "unsupported in FakeWorld"))
        }

        fn write_outside(
            &self,
            _allowed: &fieldbook_core::egress::Allowed,
            _audited: &super::Audited,
            path: &Path,
            _bytes: &[u8],
        ) -> Result<fieldbook_core::audit::Sha256, WorldError> {
            Err(WorldError::new(path, "unsupported in FakeWorld"))
        }

        fn has_company_store(&self) -> bool {
            false
        }

        fn stdin_is_terminal(&self) -> bool {
            false
        }

        fn read_line(&self) -> String {
            String::new()
        }

        fn prompt(&self, _flag: &str) -> Result<(), WorldError> {
            Ok(())
        }

        fn today(&self) -> CivilDate {
            CivilDate::from_ymd(2026, 3, 1).expect("valid date")
        }

        fn now(&self) -> Now {
            Now::new(self.today(), 43_200).expect("valid now")
        }

        fn principal(&self) -> Principal {
            Principal::local()
        }

        fn cwd(&self) -> &Path {
            &self.cwd
        }

        fn tree_root(&self) -> &Path {
            &self.tree_root
        }

        fn state_dir(&self) -> &Path {
            &self.state_dir
        }

        fn place(&self, _path: &Path) -> PlaceFacts {
            PlaceFacts::local()
        }

        fn support_facts(&self) -> SupportFacts {
            SupportFacts::default()
        }

        fn reset_drill(&self, _audited: &super::Audited) -> Result<(), WorldError> {
            Err(WorldError::OutOfScope(self.state_dir.join("drill")))
        }

        fn remove_drill(&self, _audited: &super::Audited) -> Result<(), WorldError> {
            Err(WorldError::OutOfScope(self.state_dir.join("drill")))
        }

        fn read_maintenance(&self) -> Result<String, WorldError> {
            Ok(String::new())
        }

        fn read_state_summary(
            &self,
            _today: fieldbook_core::CivilDate,
        ) -> Result<fieldbook_core::status::StateSummary, WorldError> {
            Ok(fieldbook_core::status::StateSummary::default())
        }

        fn store_id(&self) -> &'static str {
            "memory@0"
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
    fn anything_else_prints_usage_and_exit_two() {
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
