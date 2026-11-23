//! The single bridge to [`fieldbook_win`]: translates operating-system facts into the
//! option and placement types that `fieldbook` and `fieldbook-core` use.

use std::path::{Path, PathBuf};

use fieldbook_core::labels::PlaceFacts;
use fieldbook_win::{Folder, Place, WinError};

pub use fieldbook_win::WriterKind;

/// Resolve the machine's `ProgramData` directory (`FOLDERID_ProgramData`).
#[must_use]
pub fn program_data() -> Option<PathBuf> {
    fieldbook_win::known_folder(Folder::ProgramData).ok()
}

/// Resolve the current user's `LocalAppData` directory (`FOLDERID_LocalAppData`).
#[must_use]
pub fn local_app_data() -> Option<PathBuf> {
    fieldbook_win::known_folder(Folder::LocalAppData).ok()
}

/// Return the current process logon principal (`DOMAIN\name`).
#[must_use]
pub fn principal() -> Option<String> {
    fieldbook_win::principal().ok()
}

/// Check whether the current process token belongs to `group` (`None` when the token or group cannot be read).
#[must_use]
pub fn in_group(group: &str) -> Option<bool> {
    fieldbook_win::in_group(group).ok()
}

/// Return the number of interactive logon sessions on the machine.
#[must_use]
pub fn interactive_logons() -> Option<u32> {
    fieldbook_win::interactive_logons().ok()
}

/// Return the [`WriterKind`] entries that hold write access on `path`.
#[must_use]
pub fn writers(path: &Path) -> Option<Vec<WriterKind>> {
    fieldbook_win::writers(path).ok()
}

/// Add `FILE_ATTRIBUTE_NOT_CONTENT_INDEXED` (`0x2000`) to `path`.
#[must_use]
pub fn mark_not_indexed(path: &Path) -> bool {
    fieldbook_win::mark_not_indexed(path).is_ok()
}

/// Return the final path of the object that the open `file` handle refers to (`None` off
/// Windows or when the handle cannot be queried), read from the handle and not from a pathname.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the final path is read from an open file handle, never from a pathname"
)]
pub fn final_path(file: &std::fs::File) -> Option<PathBuf> {
    fieldbook_win::final_path(file).ok()
}

/// Convert a [`fieldbook_win::Place`] result into [`PlaceFacts`].
///
/// Off Windows (`call == "not windows"`), every fact is `false`. On Windows, a failed
/// system call sets every fact to `true` so a folder the program cannot judge is refused.
#[must_use]
pub const fn facts(res: Result<Place, WinError>) -> PlaceFacts {
    match res {
        Ok(p) => PlaceFacts {
            reparse: p.reparse,
            sync_root: p.sync_root,
            network: p.network,
        },
        Err(err) => {
            if matches!(err.call.as_bytes(), b"not windows") {
                PlaceFacts::local()
            } else {
                PlaceFacts {
                    reparse: true,
                    sync_root: true,
                    network: true,
                }
            }
        }
    }
}

/// Query folder placement facts for `path`.
#[must_use]
pub fn place(path: &Path) -> PlaceFacts {
    facts(fieldbook_win::place(path))
}

#[cfg(windows)]
fn map_writer_kinds(kinds: Option<Vec<WriterKind>>) -> Vec<fieldbook_core::policy::Writer> {
    use fieldbook_core::policy::Writer;
    match kinds {
        Some(list) if !list.is_empty() => list
            .into_iter()
            .map(|k| match k {
                WriterKind::Administrators => Writer::Administrators,
                WriterKind::System => Writer::System,
                WriterKind::Other => Writer::Other,
                WriterKind::Unknown => Writer::Unknown,
            })
            .collect(),
        _ => vec![Writer::Unknown],
    }
}

#[cfg(not(windows))]
fn unix_writers_for(path: &Path) -> Vec<fieldbook_core::policy::Writer> {
    use fieldbook_core::policy::Writer;
    use std::os::unix::fs::PermissionsExt as _;
    let Ok(meta) = std::fs::metadata(path) else {
        return vec![Writer::Administrators];
    };
    let mode = meta.permissions().mode();
    if (mode & 0o002) != 0 {
        vec![Writer::Administrators, Writer::Other]
    } else {
        vec![Writer::Administrators]
    }
}

/// Query policy writer facts for `path` and its parent folder (`policy-load` step 2).
#[must_use]
pub fn policy_writers(path: &Path) -> fieldbook_core::policy::WriterFacts {
    let folder_path = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    #[cfg(windows)]
    {
        let file = map_writer_kinds(writers(path));
        let folder = map_writer_kinds(writers(folder_path));
        fieldbook_core::policy::WriterFacts { file, folder }
    }
    #[cfg(not(windows))]
    {
        let file = unix_writers_for(path);
        let folder = unix_writers_for(folder_path);
        fieldbook_core::policy::WriterFacts { file, folder }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    use std::path::Path;

    use fieldbook_core::labels::PlaceFacts;
    use fieldbook_win::WinError;

    use super::facts;

    #[test]
    fn an_unreadable_place_is_all_true() {
        let unreadable = facts(Err(WinError {
            call: "GetFileAttributesW",
            code: 5,
        }));
        assert_eq!(
            unreadable,
            PlaceFacts {
                reparse: true,
                sync_root: true,
                network: true,
            }
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_no_fact_is_known() {
        let p = Path::new("/tmp");
        assert_eq!(super::program_data(), None);
        assert_eq!(super::local_app_data(), None);
        assert_eq!(super::principal(), None);
        assert_eq!(super::in_group("Everyone"), None);
        assert_eq!(super::interactive_logons(), None);
        assert_eq!(super::writers(p), None);
        assert!(!super::mark_not_indexed(p));
        assert_eq!(super::place(p), PlaceFacts::local());
    }
}
