//! The question log file `asked/asked.csv` in the state folder (`ask-log`).
//!
//! **Responsibility.** Read the file ([`read_rows`]), rewrite it whole through the atomic
//! writer under a state-only [`Audited`] token ([`write_rows`]), and change one row's outcome
//! ([`set_outcome`]). The rules live in `fieldbook_core::asked`.

#![expect(
    clippy::disallowed_methods,
    reason = "io::asked reads asked/asked.csv from the state folder"
)]

use std::fmt;
use std::path::{Path, PathBuf};

use fieldbook_core::asked::{self, AskedError, AskedId, AskedRow, Outcome};

use crate::World;
use crate::io::journal::Journal;
use crate::io::{Audited, StoreLease, WorldError};

/// The log's path relative to the state folder.
pub const REL_PATH: &str = "asked/asked.csv";

/// The log's path inside `state_dir`.
#[must_use]
pub fn path(state_dir: &Path) -> PathBuf {
    state_dir.join("asked").join("asked.csv")
}

/// Why the log cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The file exists and cannot be read.
    Io(String),
    /// The file does not parse; it is left alone.
    Parse(AskedError),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "cannot read {REL_PATH}: {msg}"),
            Self::Parse(err) => write!(f, "{REL_PATH} does not parse: {err}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// Reads the rows of the log in `state_dir`; `None` when the file is absent.
///
/// # Errors
///
/// Returns [`ReadError`] when the file cannot be read or does not parse.
pub fn read_rows(state_dir: &Path) -> Result<Option<Vec<AskedRow>>, ReadError> {
    match std::fs::read(path(state_dir)) {
        Ok(bytes) => asked::parse(&bytes).map(Some).map_err(ReadError::Parse),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(ReadError::Io(err.to_string())),
    }
}

/// Writes `rows` as the whole log in the state folder through the atomic writer; the folder and
/// the header are created with the first row.
///
/// # Errors
///
/// Returns [`WorldError`] when the write fails or `audited` does not allow it.
pub fn write_rows(
    world: &dyn World,
    audited: &Audited,
    rows: &[AskedRow],
) -> Result<(), WorldError> {
    world
        .write_file(
            audited,
            &path(world.state_dir()),
            asked::render(rows).as_bytes(),
        )
        .map(|_| ())
}

/// Changes the outcome of row `id` from `answered` to `to` (`cite_failed` or `flagged`) and
/// rewrites the log, under `lease` when the caller holds the store write lease or a short one
/// taken here. It appends no journal event: the `ask` event keeps the first-run outcome.
///
/// # Errors
///
/// Returns a message, naming the id when it is unknown, when the lease or the state-only token
/// cannot be taken, the log cannot be read, the row cannot take `to`, or the write fails.
pub fn set_outcome(
    world: &dyn World,
    journal: &Journal<'_>,
    id: AskedId,
    to: Outcome,
    lease: Option<&StoreLease>,
) -> Result<(), String> {
    let owned;
    let lease = if let Some(lease) = lease {
        lease
    } else {
        owned = world.write_lease().map_err(|err| err.to_string())?;
        &owned
    };
    let audited = journal.state_scope(lease).map_err(|err| err.to_string())?;
    let rows = read_rows(world.state_dir())
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    if !rows.iter().any(|row| row.id == id) {
        return Err(format!("unknown asked id {id}"));
    }
    let next = asked::with_outcome(&rows, id, to).ok_or_else(|| {
        format!(
            "asked id {id} cannot become {}: only an answered row changes",
            to.as_str()
        )
    })?;
    write_rows(world, &audited, &next).map_err(|err| err.to_string())
}
