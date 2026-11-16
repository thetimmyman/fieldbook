//! State folder resolution, `settings.toml` role/baseline reader, operating-system
//! principal lookup, and the single [`open`] entry point to [`Journal::open`] (`audit-journal`).

#![expect(
    clippy::disallowed_methods,
    reason = "io::state reads settings.toml and /etc/passwd for the operating-system principal"
)]

use std::fmt;
use std::path::{Path, PathBuf};

use fieldbook_core::audit::{
    Principal, Role, SettingsBaseline, Via, parse_settings_role_and_baseline,
};
use fieldbook_core::check::Code;

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

/// Backup configuration parsed from `[backup]` and `[[backup.target]]` in `settings.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSettings {
    /// Default target identifier when `--target` is omitted.
    pub default: Option<String>,
    /// Number of days before warning about a stale backup (default 7).
    pub warn_after_days: u32,
    /// Number of newest snapshots to retain (default 7).
    pub keep_last: usize,
    /// Number of weekly snapshots to retain (default 4).
    pub keep_weekly: usize,
    /// Configured backup destinations.
    pub targets: Vec<BackupTargetConfig>,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            default: None,
            warn_after_days: 7,
            keep_last: 7,
            keep_weekly: 4,
            targets: Vec::new(),
        }
    }
}

/// A single backup target parsed from `[[backup.target]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupTargetConfig {
    /// Unique identifier for this target.
    pub id: String,
    /// Target kind (`folder`, `it-location`, or `synced`).
    pub kind: String,
    /// Target directory path.
    pub path: PathBuf,
    /// Optional subfolder inclusion filter (e.g. `["audit"]`).
    pub include: Option<Vec<String>>,
    /// Whether this target is designated company storage (default false).
    pub company_storage: bool,
}

/// Errors arising from parsing or reading backup settings from `settings.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupSettingsError {
    /// Filesystem error reading `settings.toml`.
    Io(String),
    /// TOML syntax or structural error.
    Toml(String),
    /// An unknown key was encountered in `[backup]` or `[[backup.target]]`.
    UnknownKey {
        /// Diagnostic finding code ([`Code::Fb0812`]).
        code: Code,
        /// Name of the unknown key.
        key: String,
    },
    /// A value in `settings.toml` is invalid.
    InvalidValue {
        /// Name of the key with the invalid value.
        key: &'static str,
        /// Explanation of why the value is invalid.
        reason: String,
    },
    /// A required field is missing from a target.
    MissingField {
        /// Name of the missing field.
        key: &'static str,
    },
}

impl BackupSettingsError {
    /// Return the finding code if this error carries one.
    #[must_use]
    pub const fn code(&self) -> Option<Code> {
        match self {
            Self::UnknownKey { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// Return the offending key name if this error names one.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::UnknownKey { key, .. } => Some(key.as_str()),
            Self::InvalidValue { key, .. } | Self::MissingField { key } => Some(*key),
            Self::Io(_) | Self::Toml(_) => None,
        }
    }
}

impl fmt::Display for BackupSettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "I/O error reading settings: {msg}"),
            Self::Toml(msg) => write!(f, "invalid settings.toml: {msg}"),
            Self::UnknownKey { code, key } => {
                write!(f, "{code}: unknown key `{key}` in backup settings")
            }
            Self::InvalidValue { key, reason } => {
                write!(f, "invalid value for `{key}`: {reason}")
            }
            Self::MissingField { key } => write!(f, "missing required field `{key}`"),
        }
    }
}

impl std::error::Error for BackupSettingsError {}

impl BackupSettings {
    /// Parse backup configuration from `settings.toml` text.
    ///
    /// Reads only the `[backup]` table and each `[[backup.target]]` table.
    /// An unknown key in either table returns [`BackupSettingsError::UnknownKey`] with [`Code::Fb0812`].
    /// Other tables in `settings.toml` are ignored.
    ///
    /// # Errors
    ///
    /// Returns [`BackupSettingsError`] if the TOML is invalid, contains an unknown key in `[backup]`
    /// or `[[backup.target]]`, or has invalid or missing values.
    pub fn parse(text: &str) -> Result<Self, BackupSettingsError> {
        let value: toml::Value =
            toml::from_str(text).map_err(|err| BackupSettingsError::Toml(err.to_string()))?;

        let Some(table) = value.as_table() else {
            return Ok(Self::default());
        };

        let Some(backup_val) = table.get("backup") else {
            return Ok(Self::default());
        };

        let Some(backup_table) = backup_val.as_table() else {
            return Err(BackupSettingsError::Toml(
                "`backup` must be a table".to_owned(),
            ));
        };

        let mut default = None;
        let mut warn_after_days = 7u32;
        let mut keep_last = 7usize;
        let mut keep_weekly = 4usize;
        let mut raw_targets = Vec::new();

        for (k, v) in backup_table {
            match k.as_str() {
                "default" => {
                    let s = v
                        .as_str()
                        .ok_or_else(|| BackupSettingsError::InvalidValue {
                            key: "default",
                            reason: "expected string".to_owned(),
                        })?;
                    default = Some(s.to_owned());
                }
                "warn_after_days" => {
                    let i = v
                        .as_integer()
                        .ok_or_else(|| BackupSettingsError::InvalidValue {
                            key: "warn_after_days",
                            reason: "expected non-negative integer".to_owned(),
                        })?;
                    warn_after_days =
                        u32::try_from(i).map_err(|_| BackupSettingsError::InvalidValue {
                            key: "warn_after_days",
                            reason: "integer out of range for u32".to_owned(),
                        })?;
                }
                "keep_last" => {
                    let i = v
                        .as_integer()
                        .ok_or_else(|| BackupSettingsError::InvalidValue {
                            key: "keep_last",
                            reason: "expected non-negative integer".to_owned(),
                        })?;
                    keep_last =
                        usize::try_from(i).map_err(|_| BackupSettingsError::InvalidValue {
                            key: "keep_last",
                            reason: "integer out of range for usize".to_owned(),
                        })?;
                }
                "keep_weekly" => {
                    let i = v
                        .as_integer()
                        .ok_or_else(|| BackupSettingsError::InvalidValue {
                            key: "keep_weekly",
                            reason: "expected non-negative integer".to_owned(),
                        })?;
                    keep_weekly =
                        usize::try_from(i).map_err(|_| BackupSettingsError::InvalidValue {
                            key: "keep_weekly",
                            reason: "integer out of range for usize".to_owned(),
                        })?;
                }
                "target" => {
                    if let Some(arr) = v.as_array() {
                        for item in arr {
                            let t = item.as_table().ok_or_else(|| {
                                BackupSettingsError::Toml(
                                    "`backup.target` elements must be tables".to_owned(),
                                )
                            })?;
                            raw_targets.push(t);
                        }
                    } else if let Some(t) = v.as_table() {
                        raw_targets.push(t);
                    } else {
                        return Err(BackupSettingsError::Toml(
                            "`backup.target` must be an array of tables".to_owned(),
                        ));
                    }
                }
                unknown => {
                    return Err(BackupSettingsError::UnknownKey {
                        code: Code::Fb0812,
                        key: unknown.to_owned(),
                    });
                }
            }
        }

        let mut targets = Vec::with_capacity(raw_targets.len());
        for target_table in raw_targets {
            let mut id = None;
            let mut kind = None;
            let mut path = None;
            let mut include = None;
            let mut company_storage = false;

            for (k, v) in target_table {
                match k.as_str() {
                    "id" => {
                        let s = v
                            .as_str()
                            .ok_or_else(|| BackupSettingsError::InvalidValue {
                                key: "id",
                                reason: "expected string".to_owned(),
                            })?;
                        if s.is_empty() {
                            return Err(BackupSettingsError::InvalidValue {
                                key: "id",
                                reason: "id must not be empty".to_owned(),
                            });
                        }
                        id = Some(s.to_owned());
                    }
                    "kind" => {
                        let s = v
                            .as_str()
                            .ok_or_else(|| BackupSettingsError::InvalidValue {
                                key: "kind",
                                reason: "expected string".to_owned(),
                            })?;
                        if !matches!(s, "folder" | "it-location" | "synced") {
                            return Err(BackupSettingsError::InvalidValue {
                                key: "kind",
                                reason: format!(
                                    "expected 'folder', 'it-location', or 'synced', got '{s}'"
                                ),
                            });
                        }
                        kind = Some(s.to_owned());
                    }
                    "path" => {
                        let s = v
                            .as_str()
                            .ok_or_else(|| BackupSettingsError::InvalidValue {
                                key: "path",
                                reason: "expected string".to_owned(),
                            })?;
                        if s.is_empty() {
                            return Err(BackupSettingsError::InvalidValue {
                                key: "path",
                                reason: "path must not be empty".to_owned(),
                            });
                        }
                        path = Some(PathBuf::from(s));
                    }
                    "include" => {
                        let arr =
                            v.as_array()
                                .ok_or_else(|| BackupSettingsError::InvalidValue {
                                    key: "include",
                                    reason: "expected array of strings".to_owned(),
                                })?;
                        let mut inc = Vec::with_capacity(arr.len());
                        for elem in arr {
                            let s =
                                elem.as_str()
                                    .ok_or_else(|| BackupSettingsError::InvalidValue {
                                        key: "include",
                                        reason: "array elements must be strings".to_owned(),
                                    })?;
                            inc.push(s.to_owned());
                        }
                        include = Some(inc);
                    }
                    "company_storage" => {
                        let b = v
                            .as_bool()
                            .ok_or_else(|| BackupSettingsError::InvalidValue {
                                key: "company_storage",
                                reason: "expected boolean".to_owned(),
                            })?;
                        company_storage = b;
                    }
                    unknown => {
                        return Err(BackupSettingsError::UnknownKey {
                            code: Code::Fb0812,
                            key: unknown.to_owned(),
                        });
                    }
                }
            }

            let id = id.ok_or(BackupSettingsError::MissingField { key: "id" })?;
            let kind = kind.ok_or(BackupSettingsError::MissingField { key: "kind" })?;
            let path = path.ok_or(BackupSettingsError::MissingField { key: "path" })?;

            targets.push(BackupTargetConfig {
                id,
                kind,
                path,
                include,
                company_storage,
            });
        }

        Ok(Self {
            default,
            warn_after_days,
            keep_last,
            keep_weekly,
            targets,
        })
    }
}

/// Read backup settings from `settings.toml` inside `state_dir`.///
/// An absent `settings.toml` returns the default [`BackupSettings`] with no targets.
///
/// # Errors
///
/// Returns [`BackupSettingsError`] if `settings.toml` exists but cannot be read, or if
/// the backup configuration contains syntax errors or unknown keys.
pub fn read_backup_settings(state_dir: &Path) -> Result<BackupSettings, BackupSettingsError> {
    let settings_path = state_dir.join("settings.toml");
    let content = match std::fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BackupSettings::default());
        }
        Err(err) => return Err(BackupSettingsError::Io(err.to_string())),
    };
    BackupSettings::parse(&content)
}

/// Returns whether `settings.toml` in `state_dir` names a `role` key.
///
/// An absent file, an unreadable file and a file that is not valid TOML all answer `false`:
/// `status` then prints the lines to add and says that `verify` is off.
#[must_use]
pub fn has_role(state_dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(state_dir.join("settings.toml")) else {
        return false;
    };
    text.parse::<toml::Table>()
        .is_ok_and(|table| table.get("role").is_some())
}
