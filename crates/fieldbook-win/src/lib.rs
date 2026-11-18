//! Windows operating-system facts for Fieldbook: known folders, the logon principal,
//! group membership, interactive logon counts, folder placement (reparse points, cloud sync
//! roots and network drives), file or folder writers, the not-content-indexed flag and the
//! final path of an open file handle.
//!
//! Only `fieldbook` (specifically `crates/fieldbook/src/io/win.rs`) may depend on this crate.
//! All first-party `unsafe` calls live in `src/sys.rs` under `#[cfg(windows)]`.

use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod sys;

/// Known machine or user folders queried through `SHGetKnownFolderPath`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    /// `FOLDERID_ProgramData` (`C:\ProgramData`).
    ProgramData,
    /// `FOLDERID_LocalAppData` (`%LOCALAPPDATA%` from the token, without reading the environment).
    LocalAppData,
}

/// Classification of an account that holds write rights on a file or folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WriterKind {
    /// Built-in `Administrators` group (`WinBuiltinAdministratorsSid`).
    Administrators,
    /// Local `SYSTEM` account (`WinLocalSystemSid`).
    System,
    /// Any other resolved principal, or a missing (`NULL`) DACL.
    Other,
    /// An access-control entry or principal that could not be inspected.
    Unknown,
}

/// Operating-system placement facts for a folder or its nearest existing ancestor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    /// Whether the folder or any folder above it is a reparse point (junction, symlink, mount).
    pub reparse: bool,
    /// Whether `CfGetSyncRootInfoByPath` reports a registered cloud sync root at or above it.
    pub sync_root: bool,
    /// Whether the path is on a UNC share or remote network drive (`DRIVE_REMOTE`).
    pub network: bool,
}

/// Failure from a Windows system call (or `"not windows"` on non-Windows targets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinError {
    /// Name of the system call that failed.
    pub call: &'static str,
    /// Operating-system error code (`GetLastError()` or `HRESULT` code).
    pub code: u32,
}

impl fmt::Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed (code {})", self.call, self.code)
    }
}

impl std::error::Error for WinError {}

#[cfg(not(windows))]
const NOT_WINDOWS: WinError = WinError {
    call: "not windows",
    code: 0,
};

/// Resolve the path of `folder` from the operating system without reading environment variables.
pub fn known_folder(folder: Folder) -> Result<PathBuf, WinError> {
    #[cfg(windows)]
    {
        sys::known_folder(folder)
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Err(NOT_WINDOWS)
    }
}

/// Return the current process token's logon principal as `DOMAIN\name`.
pub fn principal() -> Result<String, WinError> {
    #[cfg(windows)]
    {
        sys::principal()
    }
    #[cfg(not(windows))]
    {
        Err(NOT_WINDOWS)
    }
}

/// Check whether the current process token is a member of `group` (a string SID or account name).
pub fn in_group(group: &str) -> Result<bool, WinError> {
    #[cfg(windows)]
    {
        sys::in_group(group)
    }
    #[cfg(not(windows))]
    {
        let _ = group;
        Err(NOT_WINDOWS)
    }
}

/// Count interactive logon sessions (active or disconnected sessions with a non-empty user name).
pub fn interactive_logons() -> Result<u32, WinError> {
    #[cfg(windows)]
    {
        sys::interactive_logons()
    }
    #[cfg(not(windows))]
    {
        Err(NOT_WINDOWS)
    }
}

/// Query reparse-point, cloud sync-root and network-drive facts for `path` (or its nearest existing ancestor).
pub fn place(path: &Path) -> Result<Place, WinError> {
    #[cfg(windows)]
    {
        sys::place(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(NOT_WINDOWS)
    }
}

/// Inspect the DACL and owner of `path` and return the [`WriterKind`] entries that hold write access.
pub fn writers(path: &Path) -> Result<Vec<WriterKind>, WinError> {
    #[cfg(windows)]
    {
        sys::writers(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(NOT_WINDOWS)
    }
}

/// Add `FILE_ATTRIBUTE_NOT_CONTENT_INDEXED` (`0x2000`) to `path` while preserving all other attributes.
pub fn mark_not_indexed(path: &Path) -> Result<(), WinError> {
    #[cfg(windows)]
    {
        sys::mark_not_indexed(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(NOT_WINDOWS)
    }
}

/// Return the normalized final path (`\\?\` form) of the object that the open `file` handle
/// refers to, read from the handle itself through `GetFinalPathNameByHandleW`.
#[expect(
    clippy::disallowed_types,
    reason = "the final path is read from an open file handle, never from a pathname"
)]
pub fn final_path(file: &std::fs::File) -> Result<PathBuf, WinError> {
    #[cfg(windows)]
    {
        sys::final_path(file)
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        Err(NOT_WINDOWS)
    }
}
