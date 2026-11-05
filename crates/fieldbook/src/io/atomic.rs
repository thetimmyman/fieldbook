//! Atomic file writer for `fieldbook` (`add-new-and-edition`).
//!
//! **Responsibility.** Write a file via a temporary file in the same directory, flush and
//! sync it, and rename it into place, retrying on sharing violations and cleaning up the
//! temporary file on any failure. Every function in this module requires an [`Audited`] token.

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "io::atomic is the single authorized file writer outside io::journal"
)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use fieldbook_core::audit::Sha256;

use crate::io::{Audited, Error};

static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(1);

const IN_USE_RETRIES: usize = 3;
const IN_USE_RETRY_DELAY: Duration = Duration::from_millis(200);

/// Write `bytes` to `path` atomically under `_audited`: write to a temporary file in the same
/// directory, flush and sync it, rename it into `path`, and return the [`Sha256`] of `bytes`.
///
/// On a sharing violation or access-denied error against a non-directory target, retries
/// 3 times, 200 ms apart, removes the temporary file, and returns [`Error::InUse`]. On any
/// other failure, removes the temporary file and returns [`Error::Io`].
///
/// # Errors
///
/// Returns [`Error::InUse`] if `path` remains locked after retries, or [`Error::Io`] on any
/// other filesystem failure.
pub fn write_file(_audited: &Audited, path: &Path, bytes: &[u8]) -> Result<Sha256, Error> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|err| Error::new(parent, err.to_string()))?;
    }

    let file_name = path
        .file_name()
        .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
    let seq = NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tmp_path = parent.join(format!(".{file_name}.tmp-{pid}-{seq}"));

    let write_res = (|| -> Result<(), std::io::Error> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_data()?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(Error::new(path, err.to_string()));
    }

    if let Err(first_err) = std::fs::rename(&tmp_path, path) {
        let target_is_dir = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
        let is_sharing_or_access = !target_is_dir
            && (first_err.kind() == std::io::ErrorKind::PermissionDenied
                || matches!(first_err.raw_os_error(), Some(5 | 32 | 33)));

        if is_sharing_or_access {
            let mut last_err = first_err;
            for _ in 0..IN_USE_RETRIES {
                std::thread::sleep(IN_USE_RETRY_DELAY);
                match std::fs::rename(&tmp_path, path) {
                    Ok(()) => return Ok(Sha256::digest(bytes)),
                    Err(err) => last_err = err,
                }
            }
            let _ = std::fs::remove_file(&tmp_path);
            let still_in_use = last_err.kind() == std::io::ErrorKind::PermissionDenied
                || matches!(last_err.raw_os_error(), Some(5 | 32 | 33));
            if still_in_use {
                return Err(Error::InUse(path.to_path_buf()));
            }
            return Err(Error::new(path, last_err.to_string()));
        }

        let _ = std::fs::remove_file(&tmp_path);
        return Err(Error::new(path, first_err.to_string()));
    }

    Ok(Sha256::digest(bytes))
}
