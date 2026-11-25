//! Atomic file writer for `fieldbook` (`add-new-and-edition`, `core-policy`).
//!
//! **Responsibility.** Write a file via a temporary file in the same directory, flush and
//! sync it, and rename it into place, retrying on sharing violations and cleaning up the
//! temporary file on any failure. Every function in this module requires an [`Audited`] token,
//! and the module exposes exactly two public doors: [`write_in_root`] and [`write_outside`].

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "io::atomic is the single authorized file writer outside io::journal"
)]

use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use fieldbook_core::audit::{Sha256, StoreId};
use fieldbook_core::egress::{Allowed, Operation, Target};

use crate::io::root::RootPath;
use crate::io::{Audited, Error};

static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(1);

const IN_USE_RETRIES: usize = 3;
const IN_USE_RETRY_DELAY: Duration = Duration::from_millis(200);

#[cfg(all(unix, any(target_arch = "aarch64", target_arch = "arm")))]
const O_DIRECTORY_FLAG: i32 = 0o40_000;
#[cfg(all(unix, not(any(target_arch = "aarch64", target_arch = "arm"))))]
const O_DIRECTORY_FLAG: i32 = 0o200_000;

#[cfg(all(unix, any(target_arch = "aarch64", target_arch = "arm")))]
const O_NOFOLLOW_FLAG: i32 = 0o100_000;
#[cfg(all(unix, not(any(target_arch = "aarch64", target_arch = "arm"))))]
const O_NOFOLLOW_FLAG: i32 = 0o400_000;

// `O_PATH`: a descriptor that carries only the directory's identity, so opening a component needs
// traversal (execute) permission and not read permission on every ancestor. Linux-only, like the
// `/proc/self/fd` names this module uses for descriptor-relative operations.
#[cfg(target_os = "linux")]
const O_PATH_FLAG: i32 = 0o10000000;
#[cfg(not(target_os = "linux"))]
const O_PATH_FLAG: i32 = 0;

fn is_symlink_or_reparse(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
            return true;
        }
    }
    false
}

fn is_not_dir_or_symlink_err(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::NotADirectory || matches!(err.raw_os_error(), Some(20 | 40))
}

#[cfg(unix)]
fn fd_entry_path(dir_file: &std::fs::File, child: &OsStr) -> PathBuf {
    use std::os::fd::AsRawFd as _;
    PathBuf::from(format!("/proc/self/fd/{}", dir_file.as_raw_fd())).join(child)
}

#[cfg(unix)]
fn open_root_dir_unix(target_path: &Path) -> Result<std::fs::File, Error> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let root_file = OpenOptions::new()
        .read(true)
        .custom_flags(O_DIRECTORY_FLAG | O_NOFOLLOW_FLAG | O_PATH_FLAG)
        .open("/")
        .map_err(|err| {
            if is_not_dir_or_symlink_err(&err) {
                Error::OutOfScope(target_path.to_path_buf())
            } else {
                Error::new(Path::new("/"), err.to_string())
            }
        })?;
    let meta = root_file
        .metadata()
        .map_err(|err| Error::new(Path::new("/"), err.to_string()))?;
    if is_symlink_or_reparse(&meta) || !meta.is_dir() {
        return Err(Error::OutOfScope(target_path.to_path_buf()));
    }
    Ok(root_file)
}

#[cfg(unix)]
fn walk_existing_dir_unix(
    joined_dir: &Path,
    target_path: &Path,
) -> Result<(std::fs::File, (u64, u64)), Error> {
    use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
    let mut cur_file = open_root_dir_unix(target_path)?;
    for comp in joined_dir.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
            Component::ParentDir => return Err(Error::OutOfScope(target_path.to_path_buf())),
            Component::Normal(part) => {
                let step_in_fd = fd_entry_path(&cur_file, part);
                let lmeta = match std::fs::symlink_metadata(&step_in_fd) {
                    Ok(m) => m,
                    Err(err)
                        if err.kind() == std::io::ErrorKind::NotFound
                            || is_not_dir_or_symlink_err(&err) =>
                    {
                        return Err(Error::OutOfScope(target_path.to_path_buf()));
                    }
                    Err(err) => return Err(Error::new(target_path, err.to_string())),
                };
                if is_symlink_or_reparse(&lmeta) || !lmeta.is_dir() {
                    return Err(Error::OutOfScope(target_path.to_path_buf()));
                }
                let next_file = OpenOptions::new()
                    .read(true)
                    .custom_flags(O_DIRECTORY_FLAG | O_NOFOLLOW_FLAG | O_PATH_FLAG)
                    .open(&step_in_fd)
                    .map_err(|err| {
                        if err.kind() == std::io::ErrorKind::NotFound
                            || is_not_dir_or_symlink_err(&err)
                        {
                            Error::OutOfScope(target_path.to_path_buf())
                        } else {
                            Error::new(target_path, err.to_string())
                        }
                    })?;
                let fmeta = next_file
                    .metadata()
                    .map_err(|err| Error::new(target_path, err.to_string()))?;
                if is_symlink_or_reparse(&fmeta)
                    || !fmeta.is_dir()
                    || (fmeta.dev(), fmeta.ino()) != (lmeta.dev(), lmeta.ino())
                {
                    return Err(Error::OutOfScope(target_path.to_path_buf()));
                }
                cur_file = next_file;
            }
        }
    }
    let meta = cur_file
        .metadata()
        .map_err(|err| Error::new(target_path, err.to_string()))?;
    Ok((cur_file, (meta.dev(), meta.ino())))
}

fn reject_symlink_components(path: &Path) -> Result<(), Error> {
    if path.as_os_str().is_empty() {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|err| Error::new(path, err.to_string()))?
            .join(path)
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
        let mut parts: Vec<&OsStr> = Vec::new();
        for comp in joined.components() {
            match comp {
                Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
                Component::ParentDir => return Err(Error::OutOfScope(path.to_path_buf())),
                Component::Normal(part) => parts.push(part),
            }
        }
        let mut cur_file = open_root_dir_unix(path)?;
        for (idx, part) in parts.iter().copied().enumerate() {
            let step_in_fd = fd_entry_path(&cur_file, part);
            match std::fs::symlink_metadata(&step_in_fd) {
                Ok(lmeta) => {
                    if is_symlink_or_reparse(&lmeta) {
                        return Err(Error::OutOfScope(path.to_path_buf()));
                    }
                    if idx + 1 < parts.len() {
                        if !lmeta.is_dir() {
                            return Err(Error::OutOfScope(path.to_path_buf()));
                        }
                        let next_file = OpenOptions::new()
                            .read(true)
                            .custom_flags(O_DIRECTORY_FLAG | O_NOFOLLOW_FLAG | O_PATH_FLAG)
                            .open(&step_in_fd)
                            .map_err(|err| {
                                if is_not_dir_or_symlink_err(&err) {
                                    Error::OutOfScope(path.to_path_buf())
                                } else {
                                    Error::new(path, err.to_string())
                                }
                            })?;
                        let fmeta = next_file
                            .metadata()
                            .map_err(|err| Error::new(path, err.to_string()))?;
                        if is_symlink_or_reparse(&fmeta)
                            || !fmeta.is_dir()
                            || (fmeta.dev(), fmeta.ino()) != (lmeta.dev(), lmeta.ino())
                        {
                            return Err(Error::OutOfScope(path.to_path_buf()));
                        }
                        cur_file = next_file;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
                Err(err) if is_not_dir_or_symlink_err(&err) => {
                    return Err(Error::OutOfScope(path.to_path_buf()));
                }
                Err(err) => return Err(Error::new(path, err.to_string())),
            }
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        let mut cur = PathBuf::new();
        for comp in joined.components() {
            match comp {
                Component::Prefix(p) => cur.push(p.as_os_str()),
                Component::RootDir => cur.push(comp.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => return Err(Error::OutOfScope(path.to_path_buf())),
                Component::Normal(part) => {
                    cur.push(part);
                    match std::fs::symlink_metadata(&cur) {
                        Ok(meta) => {
                            if is_symlink_or_reparse(&meta) {
                                return Err(Error::OutOfScope(path.to_path_buf()));
                            }
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
                        Err(err) => return Err(Error::new(path, err.to_string())),
                    }
                }
            }
        }
        Ok(())
    }
}

struct BoundDir {
    #[cfg(unix)]
    dir_file: std::fs::File,
    #[cfg(unix)]
    dev_ino: (u64, u64),
    #[cfg(windows)]
    _handles: Vec<std::fs::File>,
    logical_dir: PathBuf,
}

impl BoundDir {
    fn open_or_create(dir: &Path, target_path: &Path) -> Result<Self, Error> {
        let joined = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|err| Error::new(dir, err.to_string()))?
                .join(dir)
        };

        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
            let mut cur_file = open_root_dir_unix(target_path)?;
            let mut cur_logical = PathBuf::from("/");
            for comp in joined.components() {
                match comp {
                    Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
                    Component::ParentDir => {
                        return Err(Error::OutOfScope(target_path.to_path_buf()));
                    }
                    Component::Normal(part) => {
                        let parent_logical = cur_logical.clone();
                        cur_logical.push(part);
                        let step_in_fd = fd_entry_path(&cur_file, part);
                        let next_file = match OpenOptions::new()
                            .read(true)
                            .custom_flags(O_DIRECTORY_FLAG | O_NOFOLLOW_FLAG | O_PATH_FLAG)
                            .open(&step_in_fd)
                        {
                            Ok(f) => f,
                            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                                if std::fs::symlink_metadata(&step_in_fd).is_ok() {
                                    return Err(Error::OutOfScope(target_path.to_path_buf()));
                                }
                                crate::io::journal::run_atomic_stage_hook(
                                    crate::io::journal::AtomicStage::BeforeMkdir,
                                    &cur_logical,
                                );
                                let created_now = match std::fs::create_dir(&step_in_fd) {
                                    Ok(()) => true,
                                    Err(mkdir_err)
                                        if mkdir_err.kind()
                                            == std::io::ErrorKind::AlreadyExists =>
                                    {
                                        false
                                    }
                                    Err(mkdir_err) => {
                                        let _ =
                                            walk_existing_dir_unix(&parent_logical, target_path)?;
                                        return Err(Error::new(
                                            &cur_logical,
                                            mkdir_err.to_string(),
                                        ));
                                    }
                                };
                                let cur_meta = cur_file
                                    .metadata()
                                    .map_err(|e| Error::new(&parent_logical, e.to_string()))?;
                                let walked = walk_existing_dir_unix(&parent_logical, target_path);
                                match walked {
                                    Ok((_, walked_dev_ino))
                                        if (cur_meta.dev(), cur_meta.ino()) == walked_dev_ino => {}
                                    Ok(_) | Err(_) => {
                                        if created_now {
                                            let _ = std::fs::remove_dir(&step_in_fd);
                                        }
                                        return Err(Error::OutOfScope(target_path.to_path_buf()));
                                    }
                                }
                                OpenOptions::new()
                                    .read(true)
                                    .custom_flags(O_DIRECTORY_FLAG | O_NOFOLLOW_FLAG | O_PATH_FLAG)
                                    .open(&step_in_fd)
                                    .map_err(|e| {
                                        if is_not_dir_or_symlink_err(&e) {
                                            Error::OutOfScope(target_path.to_path_buf())
                                        } else {
                                            Error::new(&cur_logical, e.to_string())
                                        }
                                    })?
                            }
                            Err(err) if is_not_dir_or_symlink_err(&err) => {
                                return Err(Error::OutOfScope(target_path.to_path_buf()));
                            }
                            Err(err) => return Err(Error::new(&cur_logical, err.to_string())),
                        };
                        let fmeta = next_file
                            .metadata()
                            .map_err(|e| Error::new(&cur_logical, e.to_string()))?;
                        if is_symlink_or_reparse(&fmeta) || !fmeta.is_dir() {
                            return Err(Error::OutOfScope(target_path.to_path_buf()));
                        }
                        cur_file = next_file;
                    }
                }
            }
            let final_meta = cur_file
                .metadata()
                .map_err(|e| Error::new(&joined, e.to_string()))?;
            let bound = Self {
                dir_file: cur_file,
                dev_ino: (final_meta.dev(), final_meta.ino()),
                logical_dir: joined,
            };
            bound.verify_still_at_path(target_path)?;
            Ok(bound)
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            const FILE_SHARE_READ: u32 = 0x0000_0001;
            const FILE_SHARE_WRITE: u32 = 0x0000_0002;

            let mut cur = PathBuf::new();
            let mut handles = Vec::new();
            for comp in joined.components() {
                match comp {
                    Component::Prefix(p) => cur.push(p.as_os_str()),
                    Component::RootDir => cur.push(comp.as_os_str()),
                    Component::CurDir => {}
                    Component::ParentDir => {
                        return Err(Error::OutOfScope(target_path.to_path_buf()));
                    }
                    Component::Normal(part) => {
                        cur.push(part);
                        match std::fs::symlink_metadata(&cur) {
                            Ok(meta) => {
                                if is_symlink_or_reparse(&meta) || !meta.is_dir() {
                                    return Err(Error::OutOfScope(target_path.to_path_buf()));
                                }
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                                crate::io::journal::run_atomic_stage_hook(
                                    crate::io::journal::AtomicStage::BeforeMkdir,
                                    &cur,
                                );
                                if let Err(mkdir_err) = std::fs::create_dir(&cur)
                                    && mkdir_err.kind() != std::io::ErrorKind::AlreadyExists
                                {
                                    return Err(Error::new(&cur, mkdir_err.to_string()));
                                }
                            }
                            Err(err) => return Err(Error::new(&cur, err.to_string())),
                        }
                        let handle = OpenOptions::new()
                            .read(true)
                            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                            .open(&cur)
                            .map_err(|e| Error::new(&cur, e.to_string()))?;
                        let hmeta = handle
                            .metadata()
                            .map_err(|e| Error::new(&cur, e.to_string()))?;
                        if is_symlink_or_reparse(&hmeta) || !hmeta.is_dir() {
                            return Err(Error::OutOfScope(target_path.to_path_buf()));
                        }
                        handles.push(handle);
                    }
                }
            }
            let bound = Self {
                _handles: handles,
                logical_dir: joined,
            };
            bound.verify_still_at_path(target_path)?;
            Ok(bound)
        }
    }

    fn entry_path(&self, name: &OsStr) -> PathBuf {
        #[cfg(unix)]
        {
            fd_entry_path(&self.dir_file, name)
        }
        #[cfg(not(unix))]
        {
            self.logical_dir.join(name)
        }
    }

    fn verify_still_at_path(&self, target_path: &Path) -> Result<(), Error> {
        #[cfg(unix)]
        {
            let (_, walked_dev_ino) = walk_existing_dir_unix(&self.logical_dir, target_path)?;
            if walked_dev_ino != self.dev_ino {
                return Err(Error::OutOfScope(target_path.to_path_buf()));
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            reject_symlink_components(&self.logical_dir)?;
            let meta = std::fs::symlink_metadata(&self.logical_dir)
                .map_err(|_| Error::OutOfScope(target_path.to_path_buf()))?;
            if is_symlink_or_reparse(&meta) || !meta.is_dir() {
                return Err(Error::OutOfScope(target_path.to_path_buf()));
            }
            Ok(())
        }
    }

    fn check_target_not_symlink(&self, file_name: &OsStr, target_path: &Path) -> Result<(), Error> {
        let entry = self.entry_path(file_name);
        match std::fs::symlink_metadata(&entry) {
            Ok(meta) => {
                if is_symlink_or_reparse(&meta) {
                    return Err(Error::OutOfScope(target_path.to_path_buf()));
                }
                Ok(())
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) if is_not_dir_or_symlink_err(&err) => {
                Err(Error::OutOfScope(target_path.to_path_buf()))
            }
            Err(err) => Err(Error::new(target_path, err.to_string())),
        }
    }

    fn create_new_temp_file(&self, tmp_name: &OsStr) -> Result<std::fs::File, std::io::Error> {
        let entry = self.entry_path(tmp_name);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(O_NOFOLLOW_FLAG)
                .open(entry)
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            const FILE_SHARE_READ: u32 = 0x0000_0001;
            const FILE_SHARE_WRITE: u32 = 0x0000_0002;
            const FILE_SHARE_DELETE: u32 = 0x0000_0004;
            OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(entry)
        }
    }

    /// Returns `true` when `child_name` in this directory still names the file `owned_file` has
    /// open, using only facts a concurrent write cannot change: on Unix the device and inode number
    /// and the file's own name in the descriptor path, on Windows the lock this handle holds.
    ///
    /// Temporary cleanup needs this rather than an identity captured at creation: after a failed
    /// write the file's size and timestamps have changed, so a captured identity no longer matches
    /// and cleanup would leave the partial file behind.
    fn name_is_owned_file(&self, child_name: &OsStr, owned_file: &std::fs::File) -> bool {
        let entry = self.entry_path(child_name);
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd as _;
            use std::os::unix::fs::MetadataExt as _;
            let Ok(fmeta) = owned_file.metadata() else {
                return false;
            };
            let Ok(lmeta) = std::fs::symlink_metadata(&entry) else {
                return false;
            };
            if fmeta.nlink() == 0
                || !lmeta.is_file()
                || (fmeta.dev(), fmeta.ino()) != (lmeta.dev(), lmeta.ino())
            {
                return false;
            }
            let fd_link = PathBuf::from(format!("/proc/self/fd/{}", owned_file.as_raw_fd()));
            if let Ok(resolved_link) = std::fs::read_link(&fd_link)
                && resolved_link.file_name() != Some(child_name)
            {
                return false;
            }
            true
        }
        #[cfg(windows)]
        {
            self.windows_name_is_owned(&entry, owned_file)
        }
    }

    /// Removes `tmp_name` when it is still a plain file. Used where the writer cannot show ownership
    /// by the lock it holds, because taking that lock is itself what failed.
    fn remove_temp_if_plain(&self, tmp_name: &OsStr) {
        let entry = self.entry_path(tmp_name);
        if std::fs::symlink_metadata(&entry)
            .is_ok_and(|m| m.is_file() && !is_symlink_or_reparse(&m))
        {
            let _ = std::fs::remove_file(&entry);
        }
    }

    /// Windows: the entry must be a plain file whose probe blocks on the exclusive lock
    /// `owned_file` holds, which is what shows the name and the handle are the same file. Stable Rust
    /// exposes no file index on Windows, so the lock stands in for one.
    #[cfg(windows)]
    fn windows_name_is_owned(&self, entry: &Path, owned_file: &std::fs::File) -> bool {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;

        let Ok(lmeta) = std::fs::symlink_metadata(entry) else {
            return false;
        };
        if is_symlink_or_reparse(&lmeta) || !lmeta.is_file() {
            return false;
        }
        let Ok(probe) = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(entry)
        else {
            return false;
        };
        if !matches!(probe.try_lock(), Err(std::fs::TryLockError::WouldBlock)) {
            let _ = probe.unlock();
            return false;
        }
        if owned_file.unlock().is_err() {
            return false;
        }
        let probe_locked = probe.try_lock().is_ok();
        if probe_locked {
            let _ = probe.unlock();
        }
        let relocked = owned_file.try_lock().is_ok();
        probe_locked && relocked
    }

    fn same_open_file(
        &self,
        child_name: &OsStr,
        owned_file: &std::fs::File,
        expected: &FileIdentity,
    ) -> bool {
        let entry = self.entry_path(child_name);
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd as _;
            use std::os::unix::fs::MetadataExt as _;
            let Ok(fmeta) = owned_file.metadata() else {
                return false;
            };
            if fmeta.nlink() == 0 || FileIdentity::from_metadata(&fmeta).as_ref() != Some(expected)
            {
                return false;
            }
            let Ok(lmeta) = std::fs::symlink_metadata(&entry) else {
                return false;
            };
            if FileIdentity::from_metadata(&lmeta).as_ref() != Some(expected) {
                return false;
            }
            let fd_link = PathBuf::from(format!("/proc/self/fd/{}", owned_file.as_raw_fd()));
            if let Ok(resolved_link) = std::fs::read_link(&fd_link)
                && resolved_link.file_name() != Some(child_name)
            {
                return false;
            }
            true
        }
        #[cfg(windows)]
        {
            let Ok(fmeta) = owned_file.metadata() else {
                return false;
            };
            if FileIdentity::from_metadata(&fmeta).as_ref() != Some(expected) {
                return false;
            }
            let Ok(lmeta) = std::fs::symlink_metadata(&entry) else {
                return false;
            };
            if FileIdentity::from_metadata(&lmeta).as_ref() != Some(expected) {
                return false;
            }
            self.windows_name_is_owned(&entry, owned_file)
        }
    }

    fn link_entry(&self, from_name: &OsStr, to_name: &OsStr) -> Result<(), std::io::Error> {
        std::fs::hard_link(self.entry_path(from_name), self.entry_path(to_name))
    }

    fn rename_entry(&self, from_name: &OsStr, to_name: &OsStr) -> Result<(), std::io::Error> {
        std::fs::rename(self.entry_path(from_name), self.entry_path(to_name))
    }

    fn target_is_dir(&self, file_name: &OsStr) -> bool {
        std::fs::symlink_metadata(self.entry_path(file_name))
            .is_ok_and(|m| !is_symlink_or_reparse(&m) && m.is_dir())
    }

    fn remove_owned_temp(&self, tmp_name: &OsStr, owned_file: &std::fs::File) {
        if !self.name_is_owned_file(tmp_name, owned_file) {
            return;
        }
        let entry = self.entry_path(tmp_name);
        // Unix: unlink only the name that `name_is_owned_file` just bound to the owned inode,
        // relative to the bound directory descriptor. POSIX has no unlink-by-identity call, so
        // a replacement landing between that check and `unlink` cannot be refused from user
        // space; cleanup then makes no recovery attempt. A byte copy would be a new inode with
        // new ownership, mode and timestamps that hides the loss, and an owned entry moved to
        // another name is left in place rather than unlinked by a second, unverified name.
        #[cfg(unix)]
        {
            crate::io::journal::run_atomic_stage_hook(
                crate::io::journal::AtomicStage::CleanupUnlink,
                &entry,
            );
            let _ = std::fs::remove_file(&entry);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
            const FILE_SHARE_READ: u32 = 0x0000_0001;
            const FILE_SHARE_WRITE: u32 = 0x0000_0002;
            const FILE_SHARE_DELETE: u32 = 0x0000_0004;

            let _ = owned_file.unlock();
            let del_handle = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_DELETE_ON_CLOSE)
                .open(&entry);
            if del_handle.is_err() {
                let _ = std::fs::remove_file(&entry);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    // Windows reports a file's creation time as the time of its last write while the file is
    // open by this process, and a later query can return the original creation time, so it does not
    // identify the file. The identity is the write time, the attributes and the size; the probe
    // lock in `same_open_file` covers the rest.
    #[cfg(windows)]
    last_write_time: u64,
    #[cfg(windows)]
    file_attributes: u32,
    file_size: u64,
}

impl FileIdentity {
    fn from_metadata(meta: &std::fs::Metadata) -> Option<Self> {
        if is_symlink_or_reparse(meta) || !meta.is_file() {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Some(Self {
                dev: meta.dev(),
                ino: meta.ino(),
                file_size: meta.len(),
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt as _;
            Some(Self {
                last_write_time: meta.last_write_time(),
                file_attributes: meta.file_attributes(),
                file_size: meta.file_size(),
            })
        }
    }

    fn from_file(file: &std::fs::File) -> Result<Self, std::io::Error> {
        let meta = file.metadata()?;
        Self::from_metadata(&meta).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "temporary file is not a regular non-reparse file",
            )
        })
    }
}

struct TempFileGuard<'a> {
    dir: &'a BoundDir,
    tmp_name: OsString,
    logical_tmp_path: PathBuf,
    file: std::fs::File,
    expected_identity: FileIdentity,
    armed: bool,
}

impl<'a> TempFileGuard<'a> {
    const fn new(
        dir: &'a BoundDir,
        tmp_name: OsString,
        logical_tmp_path: PathBuf,
        file: std::fs::File,
        expected_identity: FileIdentity,
    ) -> Self {
        Self {
            dir,
            tmp_name,
            logical_tmp_path,
            file,
            expected_identity,
            armed: true,
        }
    }

    const fn file_mut(&mut self) -> &mut std::fs::File {
        &mut self.file
    }

    const fn update_identity(&mut self, identity: FileIdentity) {
        self.expected_identity = identity;
    }

    fn verify_before_rename(&self, target_path: &Path) -> Result<(), Error> {
        if !self
            .dir
            .same_open_file(&self.tmp_name, &self.file, &self.expected_identity)
        {
            return Err(Error::OutOfScope(target_path.to_path_buf()));
        }
        Ok(())
    }

    fn verify_after_rename(&self, dest_name: &OsStr, target_path: &Path) -> Result<(), Error> {
        if !self
            .dir
            .same_open_file(dest_name, &self.file, &self.expected_identity)
        {
            return Err(Error::OutOfScope(target_path.to_path_buf()));
        }
        Ok(())
    }

    const fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            crate::io::journal::run_atomic_stage_hook(
                crate::io::journal::AtomicStage::BeforeCleanup,
                &self.logical_tmp_path,
            );
            self.dir.remove_owned_temp(&self.tmp_name, &self.file);
        }
    }
}

/// Write `bytes` to `rel` inside `root` atomically under `audited`.
///
/// Resolves existing ancestors and remaining components against the canonical `root` before
/// creating any directory or temporary file. Refuses with [`Error::OutOfScope`] if `audited`
/// is state-only and `root.store() != StoreId::State`, if `audited` holds no active lease, or
/// if `rel` escapes `root` via `..`, a symlink, or a Windows junction.
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] on scope or containment violations, [`Error::InUse`] if the
/// target remains locked after retries, or [`Error::Io`] on any other filesystem failure.
pub fn write_in_root(
    root: &RootPath,
    rel: &Path,
    bytes: &[u8],
    audited: &Audited,
) -> Result<Sha256, Error> {
    let raw_path = if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        root.path().join(rel)
    };
    if !audited.has_active_lease() || audited.is_recovery_only() {
        return Err(Error::OutOfScope(raw_path));
    }
    if audited.is_state_only() && root.store() != StoreId::State {
        return Err(Error::OutOfScope(raw_path));
    }
    let resolved = crate::io::root::resolve_in_root(root, rel)?;
    reject_symlink_components(&resolved)?;
    crate::io::journal::run_write_pause_hook(&resolved);
    let rechecked = crate::io::root::resolve_in_root(root, rel)?;
    if rechecked != resolved {
        return Err(Error::OutOfScope(raw_path));
    }
    write_atomic(&resolved, bytes, true)
}

pub(crate) fn check_outside_write_binding(
    allowed: &Allowed,
    path: &Path,
    bytes: &[u8],
) -> Result<PathBuf, Error> {
    reject_symlink_components(path)?;
    let resolved = crate::io::root::resolve_outside_path(path)?;
    reject_symlink_components(&resolved)?;
    let permitted =
        allowed.permits_transfer(&Target::Path(resolved.clone()), &Operation::Write, bytes)
            || allowed.transfers().unwrap_or(&[]).iter().any(|t| {
                if let Target::Path(bound_path) = &t.target
                    && reject_symlink_components(bound_path).is_ok()
                    && let Ok(bound_resolved) = crate::io::root::resolve_outside_path(bound_path)
                    && bound_resolved == resolved
                {
                    allowed.permits_transfer(&t.target, &Operation::Write, bytes)
                } else {
                    false
                }
            });
    if !permitted {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    Ok(resolved)
}

/// Write `bytes` to `path` outside the data roots atomically under `allowed` and `audited`.
///
/// Verifies that `audited` is not state-only and holds an active lease, canonicalizes existing
/// ancestors of `path`, and verifies that `allowed` permits the exact canonical path,
/// [`Operation::Write`], byte count, and SHA-256 digest before creating any directory or
/// temporary file.
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] if `audited` is state-only or inactive, or if `allowed` does
/// not bind the exact target, operation, length, and digest; returns [`Error::InUse`] or
/// [`Error::Io`] on filesystem failure.
pub fn write_outside(
    allowed: &Allowed,
    path: &Path,
    bytes: &[u8],
    audited: &Audited,
) -> Result<Sha256, Error> {
    // An `audit export` token only creates files: the name is published by a hard link that fails
    // when the name exists, so a file another program created after the caller looked is never
    // replaced. Every other token replaces atomically as before.
    let replace = !audited.creates_only();
    write_outside_with(allowed, path, bytes, audited, replace)
}

/// The message of the [`Error`] for a name that exists when the token only creates files.
pub const ALREADY_EXISTS: &str = "already exists";

fn write_outside_with(
    allowed: &Allowed,
    path: &Path,
    bytes: &[u8],
    audited: &Audited,
    replace: bool,
) -> Result<Sha256, Error> {
    if audited.is_state_only() || !audited.has_active_lease() {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    if audited.is_recovery_only() && !audited.is_approved_recovery(allowed) {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    let resolved = check_outside_write_binding(allowed, path, bytes)?;
    crate::io::journal::run_write_pause_hook(&resolved);
    let rechecked = check_outside_write_binding(allowed, path, bytes)?;
    if rechecked != resolved {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    write_atomic(&resolved, bytes, replace)
}

fn write_atomic(path: &Path, bytes: &[u8], replace: bool) -> Result<Sha256, Error> {
    let Some(file_name_os) = path.file_name() else {
        return Err(Error::OutOfScope(path.to_path_buf()));
    };
    if file_name_os.is_empty() || file_name_os == "." || file_name_os == ".." {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let bound_parent = BoundDir::open_or_create(parent, path)?;
    bound_parent.check_target_not_symlink(file_name_os, path)?;

    let file_name = file_name_os.to_string_lossy();
    let seq = NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tmp_name = OsString::from(format!(".{file_name}.tmp-{pid}-{seq}"));
    let logical_tmp_path = parent.join(&tmp_name);

    crate::io::journal::run_atomic_stage_hook(
        crate::io::journal::AtomicStage::BeforeTmpOpen,
        &logical_tmp_path,
    );

    let file = match bound_parent.create_new_temp_file(&tmp_name) {
        Ok(f) => f,
        Err(open_err) => {
            bound_parent.verify_still_at_path(path)?;
            if is_not_dir_or_symlink_err(&open_err) {
                return Err(Error::OutOfScope(path.to_path_buf()));
            }
            return Err(Error::new(path, open_err.to_string()));
        }
    };

    if let Err(lock_err) = file.try_lock() {
        // The lock that would show ownership just failed, so remove the name this handle created a
        // moment ago only while it is still a plain file: the temporary file must not outlive the
        // failed write, and a symlink or a directory at that name is not ours.
        bound_parent.remove_temp_if_plain(&tmp_name);
        return Err(Error::new(path, lock_err.to_string()));
    }

    let initial_identity = match FileIdentity::from_file(&file) {
        Ok(id) => id,
        Err(id_err) => {
            bound_parent.remove_temp_if_plain(&tmp_name);
            return Err(Error::new(path, id_err.to_string()));
        }
    };

    let mut tmp_guard = TempFileGuard::new(
        &bound_parent,
        OsString::clone(&tmp_name),
        logical_tmp_path,
        file,
        initial_identity,
    );

    bound_parent.verify_still_at_path(path)?;
    bound_parent.check_target_not_symlink(file_name_os, path)?;

    let write_res = (|| -> Result<FileIdentity, std::io::Error> {
        let f = tmp_guard.file_mut();
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_data()?;
        FileIdentity::from_file(f)
    })();

    match write_res {
        Ok(written_identity) => tmp_guard.update_identity(written_identity),
        Err(err) => return Err(Error::new(path, err.to_string())),
    }

    crate::io::journal::run_atomic_stage_hook(crate::io::journal::AtomicStage::BeforeRename, path);

    bound_parent.verify_still_at_path(path)?;
    bound_parent.check_target_not_symlink(file_name_os, path)?;
    tmp_guard.verify_before_rename(path)?;

    if !replace {
        // Publish by a hard link, which fails when the name exists, so a file another program
        // created after the caller looked is never replaced. The guard removes the temporary name.
        return match bound_parent.link_entry(&tmp_name, file_name_os) {
            Ok(()) => {
                // A second name for the open file: the entry must be a plain file with the
                // identity of the one this call wrote, not something that raced in.
                let linked = std::fs::symlink_metadata(bound_parent.entry_path(file_name_os))
                    .ok()
                    .and_then(|meta| FileIdentity::from_metadata(&meta));
                if linked.as_ref() != Some(&tmp_guard.expected_identity) {
                    return Err(Error::OutOfScope(path.to_path_buf()));
                }
                Ok(Sha256::digest(bytes))
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(Error::new(path, ALREADY_EXISTS))
            }
            Err(_) => {
                // The sink's file system may not support hard links (some network shares and
                // removable media): create the name exclusively and write it in place. The name
                // still never replaces a file; a failed write removes the partial file.
                let entry = bound_parent.entry_path(file_name_os);
                let created = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&entry);
                match created {
                    Ok(mut out) => {
                        let written = out
                            .write_all(bytes)
                            .and_then(|()| out.flush())
                            .and_then(|()| out.sync_data());
                        drop(out);
                        if let Err(err) = written {
                            let _ = std::fs::remove_file(&entry);
                            return Err(Error::new(path, err.to_string()));
                        }
                        Ok(Sha256::digest(bytes))
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                        Err(Error::new(path, ALREADY_EXISTS))
                    }
                    Err(err) => Err(Error::new(path, err.to_string())),
                }
            }
        };
    }

    if let Err(first_err) = bound_parent.rename_entry(&tmp_name, file_name_os) {
        let target_is_dir = bound_parent.target_is_dir(file_name_os);
        let is_sharing_or_access = !target_is_dir
            && (first_err.kind() == std::io::ErrorKind::PermissionDenied
                || matches!(first_err.raw_os_error(), Some(5 | 32 | 33)));

        if is_sharing_or_access {
            let mut last_err = first_err;
            for _ in 0..IN_USE_RETRIES {
                std::thread::sleep(IN_USE_RETRY_DELAY);
                bound_parent.verify_still_at_path(path)?;
                bound_parent.check_target_not_symlink(file_name_os, path)?;
                tmp_guard.verify_before_rename(path)?;
                match bound_parent.rename_entry(&tmp_name, file_name_os) {
                    Ok(()) => {
                        tmp_guard.verify_after_rename(file_name_os, path)?;
                        tmp_guard.disarm();
                        return Ok(Sha256::digest(bytes));
                    }
                    Err(err) => last_err = err,
                }
            }
            let still_in_use = last_err.kind() == std::io::ErrorKind::PermissionDenied
                || matches!(last_err.raw_os_error(), Some(5 | 32 | 33));
            if still_in_use {
                return Err(Error::InUse(path.to_path_buf()));
            }
            return Err(Error::new(path, last_err.to_string()));
        }

        return Err(Error::new(path, first_err.to_string()));
    }

    tmp_guard.verify_after_rename(file_name_os, path)?;
    tmp_guard.disarm();
    Ok(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The cleanup bug this guards: a write that fails after writing some bytes has changed the
    // temporary file's size and timestamps, so an identity captured when the file was created no
    // longer matches and cleanup used to leave the partial file behind, exactly when storage is
    // short.
    #[test]
    fn cleanup_unlinks_its_temporary_file_after_a_partial_write() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let target = dir.path().join("payload.bin");
        let bound = BoundDir::open_or_create(dir.path(), &target).unwrap_or_else(|e| panic!("{e}"));
        let tmp_name = OsStr::new(".payload.bin.tmp-1-1");
        let mut file = bound
            .create_new_temp_file(tmp_name)
            .unwrap_or_else(|e| panic!("{e}"));
        file.try_lock().unwrap_or_else(|e| panic!("{e}"));
        let at_creation = FileIdentity::from_file(&file).unwrap_or_else(|e| panic!("{e}"));

        file.write_all(&[0u8; 4096])
            .unwrap_or_else(|e| panic!("{e}"));
        let after = FileIdentity::from_file(&file).unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(
            at_creation, after,
            "the test must change the identity the old cleanup compared"
        );

        bound.remove_owned_temp(tmp_name, &file);
        // Windows completes the removal when the last handle to the file closes, so release this one
        // before asking whether the entry is gone.
        drop(file);

        assert!(
            std::fs::symlink_metadata(bound.entry_path(tmp_name)).is_err(),
            "cleanup must remove the temporary file it created, partial or not"
        );
    }

    // The protection cleanup keeps: a name that now holds someone else's file is left alone.
    #[test]
    fn cleanup_leaves_a_replacement_at_its_temporary_name() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let target = dir.path().join("payload.bin");
        let bound = BoundDir::open_or_create(dir.path(), &target).unwrap_or_else(|e| panic!("{e}"));
        let tmp_name = OsStr::new(".payload.bin.tmp-2-2");
        let file = bound
            .create_new_temp_file(tmp_name)
            .unwrap_or_else(|e| panic!("{e}"));
        file.try_lock().unwrap_or_else(|e| panic!("{e}"));
        let replacement = dir.path().join("replacement.bin");
        std::fs::write(&replacement, b"someone else\n").unwrap_or_else(|e| panic!("{e}"));
        std::fs::rename(&replacement, bound.entry_path(tmp_name)).unwrap_or_else(|e| panic!("{e}"));

        bound.remove_owned_temp(tmp_name, &file);

        assert!(
            std::fs::symlink_metadata(bound.entry_path(tmp_name)).is_ok(),
            "a replacement at the temporary name must survive cleanup"
        );
    }
}
