//! State folder resolution, `settings.toml` role/baseline reader, operating-system
//! principal lookup, and the single [`open`] entry point to [`Journal::open`] (`audit-journal`).

#![expect(
    clippy::disallowed_methods,
    reason = "io::state reads settings.toml and /etc/passwd for the operating-system principal"
)]

use std::path::{Path, PathBuf};

use fieldbook_core::audit::{
    Principal, Role, SettingsBaseline, Via, parse_settings_role_and_baseline,
};

use crate::World;
use crate::io::journal::{Journal, JournalError};

/// Resolve the state folder path: `--state` / `FIELDBOOK_STATE` (relative to `cwd` when relative),
/// or `.fieldbook/` beside `corpus_root` when absent.
#[must_use]
pub fn resolve(corpus_root: &Path, cwd: &Path, state_flag: Option<&Path>) -> PathBuf {
    match state_flag {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => cwd.join(p),
        None => corpus_root.join(".fieldbook"),
    }
}

/// Read `settings.toml` inside `state_dir` (`Role::Agent` and `SettingsBaseline::absent()` when
/// missing) and compute its role and typed [`SettingsBaseline`].
///
/// # Errors
///
/// Returns [`JournalError`] if `settings.toml` exists and cannot be read or contains invalid
/// UTF-8, invalid TOML, or an invalid `role` value.
pub fn read_role_and_baseline(state_dir: &Path) -> Result<(Role, SettingsBaseline), JournalError> {
    let settings_path = state_dir.join("settings.toml");
    let raw = match std::fs::read(&settings_path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(JournalError::io(&settings_path, err.to_string())),
    };
    parse_settings_role_and_baseline(raw.as_deref())
        .map_err(|err| JournalError::Settings(err.to_string()))
}

/// Resolve the operating-system logon principal without reading environment variables.
///
/// On Windows, queries the process token via [`crate::io::win::principal`].
/// Off Windows, maps the UID of `/proc/self` through `/etc/passwd` to `os:local\<account>`.
#[must_use]
pub fn os_principal() -> Principal {
    #[cfg(windows)]
    {
        if let Some(acct) = crate::io::win::principal()
            && let Ok(p) = Principal::parse(&format!("os:{acct}"))
        {
            return p;
        }
        Principal::parse("os:local\\unknown").unwrap_or_else(|_| Principal::local())
    }
    #[cfg(not(windows))]
    {
        if let Some(account) = unix_account_from_proc_self()
            && let Ok(p) = Principal::parse(&format!("os:local\\{account}"))
        {
            return p;
        }
        Principal::parse("os:local\\unknown").unwrap_or_else(|_| Principal::local())
    }
}

#[cfg(not(windows))]
fn unix_account_from_proc_self() -> Option<String> {
    use std::os::unix::fs::MetadataExt as _;
    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    let uid_str = uid.to_string();
    for line in passwd.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split(':');
        let name = parts.next()?;
        let _pw = parts.next()?;
        let row_uid = parts.next()?;
        if row_uid == uid_str && !name.is_empty() {
            return Some(name.to_owned());
        }
    }
    None
}

/// Open the audit journal in `state_dir` for `world` and `via`.
///
/// # Errors
///
/// Returns [`JournalError`] if the lock cannot be acquired, the tail hash is broken,
/// `settings.toml` is invalid, or an initial event cannot be written.
pub fn open<'a>(
    world: &'a dyn World,
    state_dir: &Path,
    via: Via,
) -> Result<Journal<'a>, JournalError> {
    Journal::open(world, state_dir, via)
}
