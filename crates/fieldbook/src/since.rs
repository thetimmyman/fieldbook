//! `build --since`: read the earlier edition whose `edition.toml` the changes page diffs ids
//! against. Only this crate reads the file; `fieldbook-render` receives the parsed
//! [`EditionInfo`] as a value, so render and core do no I/O.

use std::fmt;
use std::path::Path;

use fieldbook_core::edition::EditionInfo;

use crate::cli::{EXIT_IO, EXIT_USAGE};

/// Why a `--since` edition was refused, and the exit code `build` returns for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinceError {
    /// The file could not be read.
    Unreadable {
        /// The file that was read.
        path: String,
        /// Why the read failed.
        reason: String,
    },
    /// The file is not a valid `edition.toml`.
    Unparsable {
        /// The file that was read.
        path: String,
        /// Why parsing failed.
        reason: String,
    },
    /// The earlier edition is not older than the edition being built.
    OutOfOrder {
        /// The earlier edition's name, as its `edition.toml` gives it.
        previous: String,
        /// The edition being built, as its newest `changes.md` heading names it.
        current: String,
    },
}

impl SinceError {
    /// The exit code `build` returns for this error: [`EXIT_USAGE`] for an unparsable or
    /// out-of-order edition, [`EXIT_IO`] for an unreadable one.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Unreadable { .. } => EXIT_IO,
            Self::Unparsable { .. } | Self::OutOfOrder { .. } => EXIT_USAGE,
        }
    }
}

impl fmt::Display for SinceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, reason } | Self::Unparsable { path, reason } => {
                write!(f, "error: {path}: {reason}")
            }
            Self::OutOfOrder { previous, current } => write!(
                f,
                "error: --since names `{previous}`, which is not earlier than the edition being built (`{current}`)"
            ),
        }
    }
}

impl std::error::Error for SinceError {}

/// Collapse a reason to one line: parse errors from `toml` carry line breaks, and a refused
/// `--since` answers with a plain one-line message.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read the earlier edition named by `--since`: `arg` is the path of that edition's folder
/// (reads `edition.toml` in it) or of its `edition.toml`, parsed with [`EditionInfo::parse`].
///
/// # Errors
///
/// Returns [`SinceError::Unreadable`] if the file cannot be read and [`SinceError::Unparsable`]
/// if it is not a valid `edition.toml`.
pub fn read_since(arg: &str) -> Result<EditionInfo, SinceError> {
    let path = Path::new(arg);
    let file = if path.is_dir() {
        path.join("edition.toml")
    } else {
        path.to_path_buf()
    };
    let display = file.display().to_string();
    let bytes = crate::io::tree::read_file(&file).map_err(|err| SinceError::Unreadable {
        path: display.clone(),
        reason: one_line(&err.to_string()),
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|err| SinceError::Unparsable {
        path: display.clone(),
        reason: one_line(&err.to_string()),
    })?;
    EditionInfo::parse(text).map_err(|err| SinceError::Unparsable {
        path: display,
        reason: one_line(&err.to_string()),
    })
}

/// Refuse `previous` when both editions are numbered and `previous`'s number is not lower than
/// the number of the edition being built (`current_heading`, the newest `changes.md` heading).
/// An unnumbered current build refuses nothing here.
///
/// # Errors
///
/// Returns [`SinceError::OutOfOrder`] when both are numbered and `previous` is not earlier.
pub fn check_order(
    previous: &EditionInfo,
    current_heading: Option<&str>,
) -> Result<(), SinceError> {
    let Some(heading) = current_heading else {
        return Ok(());
    };
    if previous.number == 0 {
        return Ok(());
    }
    let current_number = fieldbook_core::ids::EditionId::parse(heading)
        .map_err(|_| SinceError::Unparsable {
            path: heading.to_owned(),
            reason: "not a valid edition id".to_owned(),
        })?
        .number();
    if previous.number >= current_number {
        return Err(SinceError::OutOfOrder {
            previous: previous.edition.clone(),
            current: heading.to_owned(),
        });
    }
    Ok(())
}
