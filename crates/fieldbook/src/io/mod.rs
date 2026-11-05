//! The process edge: the one place that reads the command line and writes the
//! standard streams, plus Corpus root discovery and tree reading ([`tree`]).
//!
//! Later cards add file access here. Each lifts only the bans it needs, with a
//! `#![expect(..., reason = "...")]` at the top of this module.

#![expect(
    clippy::disallowed_methods,
    reason = "io reads SOURCE_DATE_EPOCH at the process boundary for build metadata"
)]

pub mod atomic;
pub mod dirs;
pub mod journal;
pub mod out;
pub mod root;
pub mod state;
pub mod store_lease;
pub mod tree;
pub mod win;

pub use journal::{Audited, Journal, JournalError, Recovery};
pub use store_lease::{GenerationStatus, LeaseMode, StoreLease};
pub use tree::{Disk, Error, WorldError, find_root, read_file, read_tree};

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use fieldbook_core::CivilDate;
use fieldbook_core::audit::Sha256;
use fieldbook_core::egress::Allowed;
use fieldbook_render::meta::{BuildInfo, BuildProfile};

use crate::cli::EXIT_IO;

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
    use super::write_outcome;
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
