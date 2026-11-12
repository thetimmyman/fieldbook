//! Retained naming and parsing kernel for the Backup `folder@1` adapter (published compatibility binding).
//!
//! **Responsibility.** Retain frozen naming, formatting, and directory parsing rules
//! for backup snapshots and `.partial` directories under `folder@1`.
//!
//! **Rules.** Pure: no I/O. This module is frozen. Any change is a new `backup_folder_v2` file.

/// Canonical directory prefix for backup snapshot directories.
pub const PREFIX: &str = "fieldbook-backup-";

/// Suffix identifying an in-progress partial snapshot directory.
pub const PARTIAL_SUFFIX: &str = ".partial";

/// Format a completed snapshot directory name for snapshot index `n`.
///
/// Formats as `"fieldbook-backup-{n:04}"` (e.g. `"fieldbook-backup-0007"`).
#[must_use]
pub fn snapshot_dir_name(n: u32) -> String {
    format!("{PREFIX}{n:04}")
}

/// Format a partial snapshot directory name for snapshot index `n`.
///
/// Formats as `"fieldbook-backup-{n:04}.partial"` (e.g. `"fieldbook-backup-0007.partial"`).
#[must_use]
pub fn partial_dir_name(n: u32) -> String {
    format!("{PREFIX}{n:04}{PARTIAL_SUFFIX}")
}

/// Check whether `name` matches the partial snapshot directory naming rule.
///
/// Returns `true` if `name` starts with [`PREFIX`] and ends with [`PARTIAL_SUFFIX`].
/// This matches the leftover partial cleanup and removal listing rules.
#[must_use]
pub fn is_partial_dir_name(name: &str) -> bool {
    name.starts_with(PREFIX) && name.ends_with(PARTIAL_SUFFIX)
}

/// Parse a snapshot directory name into its numeric snapshot index.
///
/// Follows the exact rule of `list_existing_snapshots` in `backup_folder.rs`:
/// returns `None` for `.partial` or malformed directory names.
///
/// Note: The original `backup_folder.rs` parsed through `fieldbook_core::backup::Number`,
/// which is a type alias for `u32`. This function reproduces that exact accept/reject
/// behaviour on a plain `u32` (requiring decimal digits without signs or whitespace).
#[must_use]
pub fn parse_snapshot_dir_name(name: &str) -> Option<u32> {
    if !name.starts_with(PREFIX) || name.ends_with(PARTIAL_SUFFIX) {
        return None;
    }
    let num_str = name.strip_prefix(PREFIX)?;
    num_str.parse::<u32>().ok()
}

/// Format the content of the `LATEST` pointer file for a completed snapshot.
///
/// Formats as `"{snapshot}\n{manifest_sha256}\n"`.
#[must_use]
pub fn latest_content(snapshot: u32, manifest_sha256: &str) -> String {
    format!("{snapshot}\n{manifest_sha256}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_and_partial_dir_naming() {
        assert_eq!(snapshot_dir_name(7), "fieldbook-backup-0007");
        assert_eq!(partial_dir_name(7), "fieldbook-backup-0007.partial");
    }

    #[test]
    fn boundaries_widening_and_zero() {
        assert_eq!(snapshot_dir_name(0), "fieldbook-backup-0000");
        assert_eq!(partial_dir_name(0), "fieldbook-backup-0000.partial");
        assert_eq!(parse_snapshot_dir_name("fieldbook-backup-0000"), Some(0));
        assert_eq!(parse_snapshot_dir_name("fieldbook-backup-0"), Some(0));

        assert_eq!(snapshot_dir_name(9999), "fieldbook-backup-9999");
        assert_eq!(partial_dir_name(9999), "fieldbook-backup-9999.partial");
        assert_eq!(parse_snapshot_dir_name("fieldbook-backup-9999"), Some(9999));

        assert_eq!(snapshot_dir_name(10000), "fieldbook-backup-10000");
        assert_eq!(partial_dir_name(10000), "fieldbook-backup-10000.partial");
        assert_eq!(
            parse_snapshot_dir_name("fieldbook-backup-10000"),
            Some(10000)
        );
    }

    #[test]
    fn partial_dir_recognition() {
        assert!(is_partial_dir_name("fieldbook-backup-0007.partial"));
        assert!(is_partial_dir_name("fieldbook-backup-10000.partial"));
        assert!(is_partial_dir_name("fieldbook-backup-.partial"));
        assert!(!is_partial_dir_name("fieldbook-backup-0007"));
        assert!(!is_partial_dir_name("other-0007.partial"));
        assert!(!is_partial_dir_name(""));
    }

    #[test]
    fn malformed_and_partial_names_refused_by_parse() {
        assert_eq!(parse_snapshot_dir_name("fieldbook-backup-"), None);
        assert_eq!(parse_snapshot_dir_name("fieldbook-backup-12ab"), None);
        assert_eq!(
            parse_snapshot_dir_name("fieldbook-backup-0007.partial"),
            None
        );
        assert_eq!(parse_snapshot_dir_name(" fieldbook-backup-0007"), None);
        assert_eq!(parse_snapshot_dir_name("fieldbook-backup-0007 "), None);
        assert_eq!(parse_snapshot_dir_name("fieldbook-backup--7"), None);
        assert_eq!(parse_snapshot_dir_name("other-0007"), None);
        assert_eq!(parse_snapshot_dir_name(""), None);
    }

    #[test]
    fn latest_content_formatting() {
        assert_eq!(
            latest_content(7, "manifest-sha256-hex"),
            "7\nmanifest-sha256-hex\n"
        );
        assert_eq!(latest_content(0, ""), "0\n\n");
    }
}
