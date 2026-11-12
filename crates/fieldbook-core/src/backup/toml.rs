//! `backup.toml` parsing, rendering, and manifest SHA-256 identity calculation.
//!
//! **Responsibility.** Strict TOML parsing with unknown fields denied, canonical rendering,
//! and exact manifest byte encoding and SHA-256 digest computation.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

use serde::{Deserialize, Serialize};

use fieldbook_calc::CivilDate;

use crate::audit::Sha256;

use super::{BackupError, Number};

/// Parsed metadata representing a snapshot's `backup.toml` manifest file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupToml {
    /// Format version of the backup metadata (`1`).
    pub backup_version: u32,
    /// Semantic version of the tool that produced the snapshot.
    pub tool_version: String,
    /// Number of this snapshot.
    pub snapshot: Number,
    /// UTC timestamp of snapshot creation in canonical ISO 8601 format (`YYYY-MM-DDTHH:MM:SSZ`).
    pub created_utc: String,
    /// Sorted list of label identifiers covering the items in the snapshot.
    pub labels: Vec<String>,
    /// Sorted list of store root names copied into the snapshot.
    pub roots: Vec<String>,
    /// Total number of files in `data/`.
    pub files: u64,
    /// Total byte length of files in `data/`.
    pub bytes: u64,
    /// Corpus fingerprint computed over `data/corpus/` entries in `SHA256SUMS`.
    pub fingerprint: String,
    /// Sequence number of the `backup.begin` journal event included in the snapshot.
    pub journal_seq: u64,
    /// Journal head hash at `backup.begin`.
    pub journal_head: String,
    /// State of the journal at the time of backup (`ok` or `broken`).
    pub journal_state: String,
    /// SHA-256 digest of the canonical manifest identity binding metadata and checksums.
    pub manifest_sha256: String,
}

impl BackupToml {
    /// Parses a `backup.toml` string, rejecting unknown keys and verifying structural invariants.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError`] if TOML parsing fails, unknown keys are present, or any field
    /// fails validation.
    pub fn parse(text: &str) -> Result<Self, BackupError> {
        let parsed: Self = toml::from_str(text).map_err(|e| BackupError::Toml(e.to_string()))?;

        if parsed.backup_version != 1 {
            return Err(BackupError::InvalidVersion(parsed.backup_version));
        }

        if parse_canonical_utc(&parsed.created_utc).is_none() {
            return Err(BackupError::InvalidValue {
                key: "created_utc",
                reason: format!(
                    "`{}` is not a canonical UTC timestamp (YYYY-MM-DDTHH:MM:SSZ)",
                    parsed.created_utc
                ),
            });
        }

        if parsed.journal_state != "ok"
            && parsed.journal_state != crate::strings::cite_statuses()[0]
        {
            return Err(BackupError::InvalidValue {
                key: "journal_state",
                reason: format!("expected `ok` or `broken`, got `{}`", parsed.journal_state),
            });
        }

        if !is_lower_hex_64(&parsed.fingerprint) {
            return Err(BackupError::InvalidValue {
                key: "fingerprint",
                reason: "must be 64 lowercase hex characters".to_owned(),
            });
        }

        if !is_lower_hex_64(&parsed.journal_head) {
            return Err(BackupError::InvalidValue {
                key: "journal_head",
                reason: "must be 64 lowercase hex characters".to_owned(),
            });
        }

        if !is_lower_hex_64(&parsed.manifest_sha256) {
            return Err(BackupError::InvalidValue {
                key: "manifest_sha256",
                reason: "must be 64 lowercase hex characters".to_owned(),
            });
        }

        check_strictly_sorted(&parsed.labels, "labels")?;
        check_strictly_sorted(&parsed.roots, "roots")?;

        Ok(parsed)
    }

    /// Renders this metadata into canonical `backup.toml` text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("backup_version = {}\n", self.backup_version));
        out.push_str(&format!("tool_version = \"{}\"\n", self.tool_version));
        out.push_str(&format!("snapshot = {}\n", self.snapshot));
        out.push_str(&format!("created_utc = \"{}\"\n", self.created_utc));
        out.push_str(&format!("labels = {:?}\n", self.labels));
        out.push_str(&format!("roots = {:?}\n", self.roots));
        out.push_str(&format!("files = {}\n", self.files));
        out.push_str(&format!("bytes = {}\n", self.bytes));
        out.push_str(&format!("fingerprint = \"{}\"\n", self.fingerprint));
        out.push_str(&format!("journal_seq = {}\n", self.journal_seq));
        out.push_str(&format!("journal_head = \"{}\"\n", self.journal_head));
        out.push_str(&format!("journal_state = \"{}\"\n", self.journal_state));
        out.push_str(&format!("manifest_sha256 = \"{}\"\n", self.manifest_sha256));
        out
    }
}

/// Parses and validates a canonical UTC timestamp string in `YYYY-MM-DDTHH:MM:SSZ` form.
#[must_use]
pub fn parse_canonical_utc(text: &str) -> Option<(CivilDate, u32)> {
    let bytes = text.as_bytes();
    if bytes.len() != 20 || bytes[10] != b'T' || bytes[19] != b'Z' {
        return None;
    }
    let date_str = &text[..10];
    let Ok(date) = CivilDate::parse(date_str) else {
        return None;
    };
    let h1 = bytes[11].checked_sub(b'0')?;
    let h2 = bytes[12].checked_sub(b'0')?;
    if bytes[13] != b':' {
        return None;
    }
    let m1 = bytes[14].checked_sub(b'0')?;
    let m2 = bytes[15].checked_sub(b'0')?;
    if bytes[16] != b':' {
        return None;
    }
    let s1 = bytes[17].checked_sub(b'0')?;
    let s2 = bytes[18].checked_sub(b'0')?;

    if h1 > 9 || h2 > 9 || m1 > 9 || m2 > 9 || s1 > 9 || s2 > 9 {
        return None;
    }
    let hour = u32::from(h1) * 10 + u32::from(h2);
    let min = u32::from(m1) * 10 + u32::from(m2);
    let sec = u32::from(s1) * 10 + u32::from(s2);

    if hour >= 24 || min >= 60 || sec >= 60 {
        return None;
    }

    Some((date, hour * 3600 + min * 60 + sec))
}

fn is_lower_hex_64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn check_strictly_sorted(list: &[String], field: &'static str) -> Result<(), BackupError> {
    for window in list.windows(2) {
        if window[0].as_bytes() >= window[1].as_bytes() {
            return Err(BackupError::UnsortedList {
                field,
                member: window[1].clone(),
            });
        }
    }
    Ok(())
}

fn validate_canonical_sums(sums: &str) -> Result<(), BackupError> {
    if sums.is_empty() {
        return Ok(());
    }
    if !sums.ends_with('\n') {
        return Err(BackupError::NonCanonicalSums(
            "checksum text does not end with newline".to_owned(),
        ));
    }
    let mut prev_path: Option<&str> = None;
    for (line_no, line) in sums.lines().enumerate() {
        if line.is_empty() {
            return Err(BackupError::NonCanonicalSums(format!(
                "empty line at line {}",
                line_no + 1
            )));
        }
        let Some((sha, path)) = line.split_once("  ") else {
            return Err(BackupError::NonCanonicalSums(format!(
                "line {} does not contain two-space separator: `{line}`",
                line_no + 1
            )));
        };
        if !is_lower_hex_64(sha) {
            return Err(BackupError::NonCanonicalSums(format!(
                "line {} has invalid sha256 digest `{sha}`",
                line_no + 1
            )));
        }
        if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
            return Err(BackupError::NonCanonicalSums(format!(
                "line {} has non-canonical relative path `{path}`",
                line_no + 1
            )));
        }
        if let Some(prev) = prev_path
            && prev.as_bytes() >= path.as_bytes()
        {
            return Err(BackupError::NonCanonicalSums(format!(
                "line {} path `{path}` is not strictly sorted after `{prev}`",
                line_no + 1
            )));
        }
        prev_path = Some(path);
    }
    Ok(())
}

/// Encodes the canonical byte sequence for manifest identity according to Step 28.
///
/// # Errors
///
/// Returns [`BackupError`] if metadata values are invalid, lists are unsorted, or `sums`
/// is noncanonical.
pub fn encode_manifest_bytes(info: &BackupToml, sums: &str) -> Result<Vec<u8>, BackupError> {
    if info.backup_version != 1 {
        return Err(BackupError::InvalidVersion(info.backup_version));
    }
    if parse_canonical_utc(&info.created_utc).is_none() {
        return Err(BackupError::InvalidValue {
            key: "created_utc",
            reason: format!(
                "`{}` is not a canonical UTC timestamp (YYYY-MM-DDTHH:MM:SSZ)",
                info.created_utc
            ),
        });
    }
    check_strictly_sorted(&info.labels, "labels")?;
    check_strictly_sorted(&info.roots, "roots")?;
    validate_canonical_sums(sums)?;

    let mut enc = String::new();
    enc.push_str("backup-manifest 1\n");
    enc.push_str(&format!("backup_version {}\n", info.backup_version));
    enc.push_str(&format!(
        "tool_version {}:{}\n",
        info.tool_version.len(),
        info.tool_version
    ));
    enc.push_str(&format!("snapshot {}\n", info.snapshot));
    enc.push_str(&format!("created_utc {}\n", info.created_utc));

    enc.push_str("labels");
    if info.labels.is_empty() {
        enc.push_str(" 0\n");
    } else {
        enc.push_str(&format!(" {}", info.labels.len()));
        for l in &info.labels {
            enc.push_str(&format!(" {}:{}", l.len(), l));
        }
        enc.push('\n');
    }

    enc.push_str("roots");
    if info.roots.is_empty() {
        enc.push_str(" 0\n");
    } else {
        enc.push_str(&format!(" {}", info.roots.len()));
        for r in &info.roots {
            enc.push_str(&format!(" {}:{}", r.len(), r));
        }
        enc.push('\n');
    }

    enc.push_str(&format!("files {}\n", info.files));
    enc.push_str(&format!("bytes {}\n", info.bytes));
    enc.push_str(&format!(
        "fingerprint {}:{}\n",
        info.fingerprint.len(),
        info.fingerprint
    ));
    enc.push_str(&format!("journal_seq {}\n", info.journal_seq));
    enc.push_str(&format!(
        "journal_head {}:{}\n",
        info.journal_head.len(),
        info.journal_head
    ));
    enc.push_str(&format!(
        "journal_state {}:{}\n",
        info.journal_state.len(),
        info.journal_state
    ));
    enc.push_str(&format!("sums {}:{}\n", sums.len(), sums));

    Ok(enc.into_bytes())
}

/// Computes the pure manifest SHA-256 identity digest from `BackupToml` metadata and `SHA256SUMS` text.
///
/// Implements Step 28:
/// UTF-8 ASCII header `backup-manifest 1\n`, then one line per field in fixed order:
/// `backup_version`, `tool_version`, `snapshot`, `created_utc`, `labels`, `roots`, `files`, `bytes`,
/// `fingerprint`, `journal_seq`, `journal_head`, `journal_state`, `sums`.
///
/// # Errors
///
/// Returns [`BackupError`] if metadata is malformed or `sums` is not canonical.
pub fn manifest_sha256(info: &BackupToml, sums: &str) -> Result<Sha256, BackupError> {
    let bytes = encode_manifest_bytes(info, sums)?;
    Ok(Sha256::digest(&bytes))
}
