//! Pure planning of a restore: where a snapshot may be written and the rename lines the owner runs.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::backup::BackupToml;
use crate::check::Code;

/// The five steps of a drill, in order.
pub const DRILL_STEPS: [&str; 5] = ["snapshot", "restore", "check", "journal", "fingerprint"];

/// What the caller learned about the destination folder (the caller does the I/O; this module does none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestinationFacts {
    /// Whether the destination folder exists.
    pub exists: bool,
    /// Whether the destination folder is empty (meaningful only when `exists` is true).
    pub empty: bool,
}

/// Which rule a refused destination broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreRefusalKind {
    /// The destination exists and is not empty.
    NotEmpty,
    /// The destination is a live root.
    IsLiveRoot,
    /// The destination contains a live root.
    ContainsLiveRoot,
    /// The destination lies inside a live root.
    InsideLiveRoot,
}

/// A refusal of a restore destination; always FB0815.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreRefusal {
    /// Which rule a refused destination broke.
    pub kind: RestoreRefusalKind,
    /// Explanation naming which rule failed.
    pub detail: String,
}

impl RestoreRefusal {
    /// Diagnostic code of the refusal (always [`Code::Fb0815`]).
    #[must_use]
    pub const fn code(&self) -> Code {
        Code::Fb0815
    }
}

impl fmt::Display for RestoreRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code().id(), self.detail)
    }
}

impl std::error::Error for RestoreRefusal {}

/// One root a restore writes: the snapshot's `data/<name>/` goes to `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRoot {
    /// Name of the root in the snapshot.
    pub name: String,
    /// Destination path where this root will be written.
    pub path: PathBuf,
}

/// One file a restore writes to its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    /// Absolute destination file path.
    pub path: PathBuf,
    /// Expected SHA-256 digest in lowercase hexadecimal.
    pub sha256: String,
    /// Byte length of the file.
    pub bytes: u64,
}

/// The validated plan of a restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    /// The target destination folder.
    pub destination: PathBuf,
    /// The planned roots to restore.
    pub roots: Vec<PlannedRoot>,
}

/// Plans the restore of a snapshot into `destination`, validating destination facts and live roots.
///
/// # Errors
///
/// Returns [`RestoreRefusal`] if:
/// - `destination` matches, contains, or lies inside any live root.
/// - `destination` exists and is not empty.
pub fn plan_restore(
    info: &BackupToml,
    destination: &Path,
    facts: DestinationFacts,
    live_roots: &[PathBuf],
) -> Result<RestorePlan, RestoreRefusal> {
    for root in live_roots {
        if destination == root {
            return Err(RestoreRefusal {
                kind: RestoreRefusalKind::IsLiveRoot,
                detail: format!("the destination is a live root: {}", root.display()),
            });
        }
        if root.starts_with(destination) {
            return Err(RestoreRefusal {
                kind: RestoreRefusalKind::ContainsLiveRoot,
                detail: format!("the destination contains a live root: {}", root.display()),
            });
        }
        if destination.starts_with(root) {
            return Err(RestoreRefusal {
                kind: RestoreRefusalKind::InsideLiveRoot,
                detail: format!(
                    "the destination lies inside a live root: {}",
                    root.display()
                ),
            });
        }
    }

    if facts.exists && !facts.empty {
        return Err(RestoreRefusal {
            kind: RestoreRefusalKind::NotEmpty,
            detail: "the destination is not empty".to_owned(),
        });
    }

    let roots = info
        .roots
        .iter()
        .map(|name| PlannedRoot {
            name: name.clone(),
            path: destination.join(name),
        })
        .collect();

    Ok(RestorePlan {
        destination: destination.to_path_buf(),
        roots,
    })
}

/// Returns the destination-relative path of one `SHA256SUMS` entry of a snapshot.
///
/// A restore joins this path to its destination, so the entry must be `data/<root>/<file>` with
/// `<root>` one of the snapshot's `roots` and every part a plain name. An empty part, `.`, `..`,
/// a backslash, a control character, a trailing dot or space (which Windows drops), or a part
/// the host reads as anything but one file name (a drive prefix, or a colon on Windows) would
/// let a crafted snapshot write outside the destination.
///
/// # Errors
///
/// Returns the reason when `entry` is not such a path.
pub fn snapshot_entry_tail<'a>(entry: &'a str, roots: &[String]) -> Result<&'a str, String> {
    let Some(tail) = entry.strip_prefix("data/") else {
        return Err(format!("`{entry}` is not under data/"));
    };
    let mut parts = tail.split('/');
    let root = parts.next().unwrap_or("");
    if !roots.iter().any(|name| name == root) {
        return Err(format!("`{entry}` is not under a root of the snapshot"));
    }
    let mut depth = 0_usize;
    for part in std::iter::once(root).chain(parts) {
        let mut components = Path::new(part).components();
        let plain = matches!(
            (components.next(), components.next()),
            (Some(Component::Normal(name)), None) if name == std::ffi::OsStr::new(part)
        );
        // Windows drops a trailing dot or space from a name, which turns `.. ` into `..`; the
        // rule is the same on every host because a snapshot made on one is restored on another.
        let rewritten =
            part.ends_with('.') || part.ends_with(' ') || part.contains(char::is_control);
        if !plain || rewritten || part.contains('\\') || (cfg!(windows) && part.contains(':')) {
            return Err(format!(
                "`{entry}` has a part that is not a plain file name"
            ));
        }
        depth += 1;
    }
    if depth < 2 {
        return Err(format!("`{entry}` names a root, not a file"));
    }
    Ok(tail)
}

/// Shell dialect for rename instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreShell {
    /// POSIX shell syntax (`mv`).
    Sh,
    /// PowerShell syntax (`Move-Item`).
    PowerShell,
}

fn quote_sh(path_str: &str) -> String {
    format!("'{}'", path_str.replace('\'', r"'\''"))
}

fn quote_pwsh(path_str: &str) -> String {
    format!("'{}'", path_str.replace('\'', "''"))
}

/// Generates shell rename commands for each planned root found in `live`.
///
/// Roots in `plan` that do not appear in `live` produce no commands. A restored folder that lies
/// inside a live folder an earlier line (or its own first line) moves aside is named where that
/// move leaves it, under `<live>.old`, so every line can run in the order printed.
#[must_use]
pub fn rename_help(
    plan: &RestorePlan,
    live: &[(String, PathBuf)],
    shell: RestoreShell,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut moved_aside: Vec<(&Path, PathBuf)> = Vec::new();
    for root in &plan.roots {
        let Some((_, live_path)) = live.iter().find(|(name, _)| name == &root.name) else {
            continue;
        };
        let live_str = live_path.display().to_string();
        let live_old_str = format!("{live_str}.old");
        moved_aside.push((live_path.as_path(), PathBuf::from(&live_old_str)));
        let mut restored = root.path.clone();
        for (from, to) in &moved_aside {
            if let Ok(tail) = restored.strip_prefix(from) {
                restored = to.join(tail);
            }
        }
        let restored_str = restored.display().to_string();

        match shell {
            RestoreShell::Sh => {
                lines.push(format!(
                    "mv {} {}",
                    quote_sh(&live_str),
                    quote_sh(&live_old_str)
                ));
                lines.push(format!(
                    "mv {} {}",
                    quote_sh(&restored_str),
                    quote_sh(&live_str)
                ));
            }
            RestoreShell::PowerShell => {
                lines.push(format!(
                    "Move-Item -LiteralPath {} -Destination {}",
                    quote_pwsh(&live_str),
                    quote_pwsh(&live_old_str)
                ));
                lines.push(format!(
                    "Move-Item -LiteralPath {} -Destination {}",
                    quote_pwsh(&restored_str),
                    quote_pwsh(&live_str)
                ));
            }
        }
    }
    lines
}
