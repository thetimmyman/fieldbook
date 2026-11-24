//! Helper for writing policy files in integration tests with deterministic permissions
//! (`0644` file in a `0755` directory on Unix, or under the Administrators-owned provisioned
//! root on Windows) (`policy-load` steps 8, 16, 17, 19).

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "test support module writes temporary policy files and sets permissions"
)]

use std::path::{Path, PathBuf};

use fieldbook_core::policy::{Writer, WriterFacts, writer_check};

/// Named writer-permission variants for `windows_policy_fixtures_enforce_file_and_parent_writers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterVariant {
    /// Accepted fixture: only Administrators and System can write the file and parent folder.
    Normal,
    /// File is writable by a non-administrator (`Other`).
    FileWritable,
    /// Parent folder is writable by a non-administrator (`Other`).
    ParentWritable,
    /// File owner is a non-administrator runner account (`Other`).
    WrongOwner,
}

/// Writes `text` to `dir/name` with mode `0644` inside a folder of mode `0755` (on Unix) or under
/// the provisioned Administrators-owned temporary root (on Windows), and verifies that the real
/// operating-system writer reader sees only `Administrators`/`System` writers on both file and parent.
#[must_use]
pub fn write_policy(dir: &Path, name: &str, text: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("create policy parent dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))
            .expect("chmod 0755 policy dir");
    }
    let path = dir.join(name);
    std::fs::write(&path, text.as_bytes()).expect("write policy file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("chmod 0644 policy file");
    }
    let facts = real_writers(&path);
    assert_eq!(
        writer_check(&facts),
        Ok(()),
        "write_policy produced a path that fails writer_check: {} -> {facts:?}",
        path.display()
    );
    path
}

/// Queries the real operating-system writer reader for `path` and its parent folder.
#[must_use]
pub fn real_writers(path: &Path) -> WriterFacts {
    fieldbook::io::win::policy_writers(path)
}

/// Prepares or selects a policy file for one of the four [`WriterVariant`] cases in
/// `windows_policy_fixtures_enforce_file_and_parent_writers`.
#[must_use]
pub fn prepare_writer_variant(base_dir: &Path, variant: WriterVariant, text: &str) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("FIELDBOOK_WIN_POLICY_VARIANTS") {
            let root = PathBuf::from(root);
            let sub = match variant {
                WriterVariant::Normal => "normal",
                WriterVariant::FileWritable => "file-writable",
                WriterVariant::ParentWritable => "parent-writable",
                WriterVariant::WrongOwner => "wrong-owner",
            };
            let path = root.join(sub).join("policy.toml");
            if path.exists() {
                return path;
            }
        }
    }

    let sub_name = match variant {
        WriterVariant::Normal => "normal",
        WriterVariant::FileWritable => "file-writable",
        WriterVariant::ParentWritable => "parent-writable",
        WriterVariant::WrongOwner => "wrong-owner",
    };
    let dir = base_dir.join(sub_name);
    let path = write_policy(&dir, "policy.toml", text);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        match variant {
            WriterVariant::Normal => {}
            WriterVariant::FileWritable | WriterVariant::WrongOwner => {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))
                    .expect("chmod 0666 file-writable variant");
            }
            WriterVariant::ParentWritable => {
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777))
                    .expect("chmod 0777 parent-writable variant");
            }
        }
    }

    let _ = Writer::Administrators;
    path
}
