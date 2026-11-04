//! The process edge: the one place that reads the command line and writes the
//! standard streams, plus Corpus root discovery and tree reading ([`tree`]).
//!
//! Later cards add file access here. Each lifts only the bans it needs, with a
//! `#![expect(..., reason = "...")]` at the top of this module.

pub mod tree;

pub use tree::{Disk, WorldError, find_root, read_tree};

use std::io::Write;
use std::process::ExitCode;

use crate::cli::EXIT_IO;

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
