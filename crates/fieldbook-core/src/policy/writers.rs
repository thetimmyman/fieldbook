//! Policy file and parent folder writer verification (`writer_check`, `FB0418`)
//! (`policy-load`, `ADR 0004 §3`, `docs/reference/policy.md#where-it-is-read-from`).

use std::fmt;
use std::path::Path;

use crate::check::Code;

/// Classification of an account or group that holds write access to a policy file or folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Writer {
    /// Built-in Administrators group (`S-1-5-32-544`).
    Administrators,
    /// Local System account (`S-1-5-18`).
    System,
    /// Any other known principal or group (non-administrator write access).
    Other,
    /// Unresolvable or unreadable access-control entry (fails closed).
    Unknown,
}

/// Operating-system write-access facts for a policy file and its parent folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterFacts {
    /// Principals with write access to the policy file itself.
    pub file: Vec<Writer>,
    /// Principals with write access to the policy file's parent folder.
    pub folder: Vec<Writer>,
}

impl WriterFacts {
    /// Returns writer facts where only `Administrators` and `System` can write both file and folder.
    #[must_use]
    pub fn admins_only() -> Self {
        Self {
            file: vec![Writer::Administrators, Writer::System],
            folder: vec![Writer::Administrators, Writer::System],
        }
    }
}

/// Why [`writer_check`] refused a policy file or its parent folder (`FB0418`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WriterWhy {
    /// The policy file lists a non-administrator (`Other`) writer.
    FileOther,
    /// The policy folder lists a non-administrator (`Other`) writer.
    FolderOther,
    /// The policy file's writers contain an `Unknown` entry or could not be read.
    FileUnknown,
    /// The policy folder's writers contain an `Unknown` entry or could not be read.
    FolderUnknown,
}

impl WriterWhy {
    /// Returns the diagnostic code (`FB0418`).
    #[must_use]
    pub const fn code(self) -> Code {
        Code::Fb0418
    }

    /// Closed reason word used in the `Closed` observation digest (`closed\n<reason>\n<file-fact>\n<writer-fact>`).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::FileOther => "file_writable",
            Self::FolderOther => "folder_writable",
            Self::FileUnknown => "file_unknown",
            Self::FolderUnknown => "folder_unknown",
        }
    }

    /// Human-readable reason sentence (names no account or principal).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileOther => "the policy file is writable by a non-administrator",
            Self::FolderOther => "the policy folder is writable by a non-administrator",
            Self::FileUnknown => "the policy file's writers could not be verified",
            Self::FolderUnknown => "the policy folder's writers could not be verified",
        }
    }

    /// Formats the `FB0418` refusal message for `path`, naming the path and no account.
    #[must_use]
    pub fn format_refusal(self, path: &Path) -> String {
        format!("error[FB0418]: {}: {}", path.display(), self.as_str())
    }
}

impl fmt::Display for WriterWhy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for WriterWhy {}

/// Checks that only [`Writer::Administrators`] and [`Writer::System`] can write the policy file
/// and its parent folder (`FB0418`).
///
/// Refuses when `facts.file` or `facts.folder` is empty or lists any [`Writer::Other`] or
/// [`Writer::Unknown`] entry.
///
/// # Errors
///
/// Returns [`WriterWhy`] (`FB0418`) when any non-administrator or unknown writer is present.
pub fn writer_check(facts: &WriterFacts) -> Result<(), WriterWhy> {
    if facts.file.is_empty() || facts.file.contains(&Writer::Unknown) {
        return Err(WriterWhy::FileUnknown);
    }
    if facts.file.contains(&Writer::Other) {
        return Err(WriterWhy::FileOther);
    }
    if facts.folder.is_empty() || facts.folder.contains(&Writer::Unknown) {
        return Err(WriterWhy::FolderUnknown);
    }
    if facts.folder.contains(&Writer::Other) {
        return Err(WriterWhy::FolderOther);
    }
    Ok(())
}
