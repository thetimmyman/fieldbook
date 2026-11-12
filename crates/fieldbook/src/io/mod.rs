//! The process edge: the one place that reads the command line and writes the
//! standard streams, plus Corpus root discovery and tree reading ([`tree`]).
//!
//! Later cards add file access here. Each lifts only the bans it needs, with a
//! `#![expect(..., reason = "...")]` at the top of this module.

#![expect(
    clippy::disallowed_methods,
    reason = "io reads SOURCE_DATE_EPOCH and traverses input folders at the process boundary"
)]

pub mod atomic;
pub mod backup_classify;
pub mod backup_folder;
pub mod dirs;
pub mod journal;
pub mod out;
pub mod root;
pub mod state;
pub mod store_lease;
pub mod tree;
pub mod win;

pub use journal::{
    Audited, BackupRecovery, Journal, JournalError, PreparedBackupBegin, Recovery, RestoreEndInfo,
};
pub use state::{BackupSettings, BackupSettingsError, BackupTargetConfig, read_backup_settings};
pub use store_lease::{GenerationStatus, LeaseMode, StoreLease};
pub use tree::{Disk, Error, WorldError, find_root, read_file, read_tree};

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use fieldbook_core::CivilDate;
use fieldbook_core::audit::Sha256;
use fieldbook_core::egress::Allowed;
use fieldbook_render::meta::{BuildInfo, BuildProfile};

use crate::cli::EXIT_IO;

/// A candidate file discovered during `fieldbook add <folder>` traversal (`add-folder-mode`).
#[derive(Debug, Clone)]
pub struct FolderCandidate {
    /// Forward-slash relative path under the target folder.
    pub rel_name: String,
    /// Full disk path to the file.
    pub disk_path: PathBuf,
}

/// Resolve an `add` input path against `cwd` (with `state_dir` fallback for relative missing
/// paths) and inspect whether it is a directory (`add-folder-mode`).
///
/// # Errors
///
/// Returns [`WorldError`] if the path does not exist or cannot be inspected.
pub fn inspect_add_input(
    file_arg: &Path,
    cwd: &Path,
    state_dir: &Path,
) -> Result<(PathBuf, bool), WorldError> {
    let primary_path = if file_arg.is_absolute() {
        file_arg.to_path_buf()
    } else {
        cwd.join(file_arg)
    };
    let (input_path, meta) = match std::fs::symlink_metadata(&primary_path) {
        Ok(m) => (primary_path, m),
        Err(primary_err) => {
            if !file_arg.is_absolute() && primary_err.kind() == std::io::ErrorKind::NotFound {
                let state_fallback = state_dir.join(file_arg);
                if let Ok(m) = std::fs::symlink_metadata(&state_fallback) {
                    (state_fallback, m)
                } else {
                    return Err(WorldError::new(&primary_path, primary_err.to_string()));
                }
            } else {
                return Err(WorldError::new(&primary_path, primary_err.to_string()));
            }
        }
    };
    Ok((input_path, meta.is_dir()))
}

/// Enumerate candidate files inside `folder_path` in deterministic byte order of their
/// forward-slash relative path, skipping symbolic links (`add-folder-mode`).
///
/// # Errors
///
/// Returns [`WorldError`] if a directory entry cannot be read or inspected.
pub fn enumerate_folder_candidates(
    folder_path: &Path,
    recursive: bool,
) -> Result<Vec<FolderCandidate>, WorldError> {
    let mut out = Vec::new();
    enumerate_folder_inner(folder_path, "", recursive, &mut out)?;
    out.sort_by(|a, b| a.rel_name.as_bytes().cmp(b.rel_name.as_bytes()));
    Ok(out)
}

fn enumerate_folder_inner(
    dir_path: &Path,
    prefix: &str,
    recursive: bool,
    out: &mut Vec<FolderCandidate>,
) -> Result<(), WorldError> {
    let read_dir =
        std::fs::read_dir(dir_path).map_err(|err| WorldError::new(dir_path, err.to_string()))?;
    for entry_res in read_dir {
        let entry = entry_res.map_err(|err| WorldError::new(dir_path, err.to_string()))?;
        let entry_path = entry.path();
        let meta = std::fs::symlink_metadata(&entry_path)
            .map_err(|err| WorldError::new(&entry_path, err.to_string()))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel_name = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if ft.is_dir() {
            if recursive {
                enumerate_folder_inner(&entry_path, &rel_name, recursive, out)?;
            }
        } else if ft.is_file() {
            out.push(FolderCandidate {
                rel_name,
                disk_path: entry_path,
            });
        }
    }
    Ok(())
}

/// Read `SOURCE_DATE_EPOCH` and compile-time `FIELDBOOK_GIT_SHA` at the process boundary
/// and construct [`BuildInfo`] (`build-v0` step 2).
#[must_use]
pub fn build_info_from_env() -> BuildInfo {
    let raw_epoch = std::env::var("SOURCE_DATE_EPOCH").ok();
    let parsed_epoch = raw_epoch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|sec| {
            CivilDate::from_epoch_seconds(sec)
                .ok()
                .map(|date| (date, sec))
        });
    let (built, epoch_opt) = match parsed_epoch {
        Some((date, sec)) => (Some(date), Some(sec)),
        None => (None, None),
    };
    let commit = option_env!("FIELDBOOK_GIT_SHA").map(str::to_owned);
    BuildInfo::new(built, commit, BuildProfile::Silent, epoch_opt)
}

impl Disk {
    /// Atomically write `bytes` to `path` under `audited` via [`atomic::write_in_root`],
    /// selecting the [`root::RootPath`] whose folder holds `path` (longest whole-component match).
    pub fn write_file(
        &self,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, Error> {
        let roots = self.roots();
        let mut best: Option<(&root::RootPath, &Path, usize)> = None;
        for candidate in [&roots.state, &roots.home_state, &roots.corpus] {
            if let Ok(rel) = path.strip_prefix(candidate.path()) {
                let depth = candidate.path().components().count();
                if best.is_none_or(|(_, _, b_depth)| depth > b_depth) {
                    best = Some((candidate, rel, depth));
                }
            }
        }
        let Some((selected_root, rel, _)) = best else {
            return Err(Error::OutOfScope(path.to_path_buf()));
        };
        atomic::write_in_root(selected_root, rel, bytes, audited)
    }

    /// Atomically write `bytes` to `path` outside the data roots under `allowed` and `audited`
    /// via [`atomic::write_outside`].
    pub fn write_outside(
        &self,
        allowed: &Allowed,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<Sha256, Error> {
        let _ = self;
        atomic::write_outside(allowed, path, bytes, audited)
    }

    /// Return whether a company store exists on this layout (`false` on the Home layout).
    #[must_use]
    pub const fn has_company_store(&self) -> bool {
        let _ = self;
        false
    }

    /// Return whether standard input is connected to an interactive terminal.
    #[must_use]
    pub fn stdin_is_terminal(&self) -> bool {
        let _ = self;
        stdin_is_terminal()
    }

    /// Read one line from standard input.
    #[must_use]
    pub fn read_line(&self) -> String {
        let _ = self;
        read_line()
    }

    /// Write `<flag>: ` to standard error and flush immediately.
    pub fn prompt(&self, flag: &str) -> Result<(), Error> {
        let _ = self;
        prompt(flag)
    }

    /// Return folder placement facts for `path` via [`win::place`].
    #[must_use]
    pub fn place(&self, path: &Path) -> fieldbook_core::labels::PlaceFacts {
        let _ = self;
        win::place(path)
    }
}

/// Return whether standard input is connected to an interactive terminal.
#[must_use]
pub fn stdin_is_terminal() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal()
}

/// Read one line from standard input, stripping any trailing `\n` or `\r\n`.
#[must_use]
pub fn read_line() -> String {
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    line
}

/// Write `<flag>: ` to `out` and flush immediately, mapping any write or flush error to [`WorldError`].
pub fn write_prompt_to<W: Write>(mut out: W, flag: &str) -> Result<(), WorldError> {
    out.write_all(flag.as_bytes())
        .and_then(|()| out.write_all(b": "))
        .and_then(|()| out.flush())
        .map_err(|err| WorldError::new("stderr", err.to_string()))
}

/// Write `<flag>: ` to standard error and flush immediately.
pub fn prompt(flag: &str) -> Result<(), WorldError> {
    write_prompt_to(&mut std::io::stderr().lock(), flag)
}

/// Run the program: read the arguments, run the command, write its output.
///
/// A closed pipe is supported; any other output failure is an I/O failure.
#[must_use]
pub fn run_process() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let outcome = match Disk::current() {
        Ok(disk) => crate::run(&args, &disk),
        Err(err) => crate::Outcome {
            stdout: String::new(),
            stderr: format!("{err}\n"),
            code: EXIT_IO,
        },
    };
    ExitCode::from(write_outcome(
        &outcome,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    ))
}

fn write_outcome(outcome: &crate::Outcome, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let out = stdout
        .write_all(outcome.stdout.as_bytes())
        .and_then(|()| stdout.flush());
    let err = stderr
        .write_all(outcome.stderr.as_bytes())
        .and_then(|()| stderr.flush());
    if [out, err].iter().any(
        |result| matches!(result, Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe),
    ) {
        EXIT_IO
    } else {
        outcome.code
    }
}

#[cfg(test)]
mod tests {
    use super::{write_outcome, write_prompt_to};
    use std::io::{self, Write};

    struct FailsAfter {
        remaining: usize,
        kind: io::ErrorKind,
    }

    impl Write for FailsAfter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::from(self.kind));
            }
            let count = self.remaining.min(bytes.len());
            self.remaining -= count;
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailsOnFlush;

    impl Write for FailsOnFlush {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::Other))
        }
    }

    #[test]
    fn flush_failure_is_io_exit() {
        let outcome = crate::Outcome {
            stdout: "output".into(),
            stderr: "diagnostic".into(),
            code: 0,
        };
        assert_eq!(
            write_outcome(&outcome, &mut FailsOnFlush, &mut Vec::new()),
            3
        );
        assert_eq!(
            write_outcome(&outcome, &mut Vec::new(), &mut FailsOnFlush),
            3
        );
        assert_eq!(write_outcome(&outcome, &mut Vec::new(), &mut Vec::new()), 0);
    }

    #[test]
    fn partial_output_failure_is_io_exit() {
        let outcome = crate::Outcome {
            stdout: "output".into(),
            stderr: "diagnostic".into(),
            code: 0,
        };
        for on_stderr in [false, true] {
            let mut failing = FailsAfter {
                remaining: 2,
                kind: io::ErrorKind::Other,
            };
            let mut good = Vec::new();
            let code = if on_stderr {
                write_outcome(&outcome, &mut good, &mut failing)
            } else {
                write_outcome(&outcome, &mut failing, &mut good)
            };
            assert_eq!(code, 3);
            assert!(!good.is_empty(), "the other stream is still written");
        }
    }

    #[test]
    fn prompt_writes_the_flag_and_flushes() {
        struct Spy {
            bytes: Vec<u8>,
            flushed_after: Option<usize>,
        }
        impl Write for Spy {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flushed_after = Some(self.bytes.len());
                Ok(())
            }
        }
        let mut spy = Spy {
            bytes: Vec::new(),
            flushed_after: None,
        };
        write_prompt_to(&mut spy, "text").expect("prompt");
        assert_eq!(spy.bytes, b"text: ");
        assert_eq!(
            spy.flushed_after,
            Some(6),
            "flushed once the prompt is written"
        );
    }

    #[test]
    fn a_failed_prompt_write_or_flush_is_an_error() {
        let failing_write = FailsAfter {
            remaining: 0,
            kind: io::ErrorKind::WriteZero,
        };
        assert!(write_prompt_to(failing_write, "text").is_err());
        assert!(write_prompt_to(FailsOnFlush, "text").is_err());
    }

    #[test]
    fn only_broken_pipe_preserves_command_exit() {
        let outcome = crate::Outcome {
            stdout: "output".into(),
            stderr: "diagnostic".into(),
            code: 2,
        };
        let mut broken = FailsAfter {
            remaining: 1,
            kind: io::ErrorKind::BrokenPipe,
        };
        assert_eq!(write_outcome(&outcome, &mut broken, &mut Vec::new()), 2);
        let mut broken = FailsAfter {
            remaining: 0,
            kind: io::ErrorKind::BrokenPipe,
        };
        let mut full = FailsAfter {
            remaining: 0,
            kind: io::ErrorKind::WriteZero,
        };
        assert_eq!(write_outcome(&outcome, &mut broken, &mut full), 3);
    }
}
