//! Pure audit export planning and verification (`audit-export`, ADR 0004 §12).
//!
//! A plan selects a segment of stored journal lines and carries the exact bytes, the chain
//! link before the segment, and the manifest that records them. Verification recomputes the
//! chain over an exported copy without any state folder, so a copy can be checked anywhere.

use std::fmt;

use crate::check::Code;

use super::Event;
use super::chain::{compute_hash, split_hash_suffix};
use super::vocab::{NIL_UUID, Sha256, ZERO_HASH};

/// Manifest schema version written by [`ExportPlan::manifest_text`].
pub const MANIFEST_VERSION: u32 = 1;

/// Every manifest key, in the fixed order [`ExportPlan::manifest_text`] writes them.
const MANIFEST_KEYS: [&str; 12] = [
    "manifest_version",
    "tool_version",
    "chain_id",
    "writer",
    "store_uid",
    "first_seq",
    "last_seq",
    "prev",
    "head",
    "lines_sha256",
    "policy",
    "file",
];

/// One stored journal line exactly as it sits in the journal, with its line feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportLine {
    /// 1-based sequence number.
    pub seq: u64,
    /// Chain hash stored on the line.
    pub hash: Sha256,
    /// The line bytes exactly as stored, including the trailing line feed.
    pub raw: String,
}

/// Why a segment cannot be planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportError {
    /// `since` is after the last journal line.
    NothingNew {
        /// The last sequence number in the journal.
        last_seq: u64,
    },
    /// `since` is not a sequence number the journal can start a segment at.
    OutOfRange {
        /// The rejected `since` value.
        since: u64,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingNew { last_seq } => {
                write!(f, "nothing new; the journal ends at {last_seq}")
            }
            Self::OutOfRange { since } => write!(f, "the export cannot start at {since}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// A planned export segment, its exact bytes, and the manifest that records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportPlan {
    /// First sequence number in the segment.
    pub first_seq: u64,
    /// Last sequence number in the segment.
    pub last_seq: u64,
    /// Chain hash before the segment (64 zeros when the segment starts at line 1).
    pub prev: Sha256,
    /// Chain hash of the last line in the segment.
    pub head: Sha256,
    /// SHA-256 of the exact segment bytes.
    pub lines_sha256: Sha256,
    /// File name of the exported journal segment.
    pub file_name: String,
    /// File name of the manifest.
    pub manifest_name: String,
    /// Exact bytes of the segment (the lines joined as they are stored).
    pub bytes: Vec<u8>,
    chain_id: Sha256,
    writer: u32,
    store_uid: String,
    policy: Sha256,
    tool_version: String,
}

impl ExportPlan {
    /// Number of journal lines in the segment.
    #[must_use]
    pub fn events(&self) -> usize {
        usize::try_from(self.last_seq.saturating_sub(self.first_seq) + 1).unwrap_or(0)
    }

    /// The fixed-order manifest text, one key a line, ending with a line feed.
    #[must_use]
    pub fn manifest_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(out, "manifest_version = {MANIFEST_VERSION}");
        let _ = writeln!(out, "tool_version = {:?}", self.tool_version);
        let _ = writeln!(out, "chain_id = {:?}", self.chain_id.as_str());
        let _ = writeln!(out, "writer = {}", self.writer);
        let _ = writeln!(out, "store_uid = {:?}", self.store_uid);
        let _ = writeln!(out, "first_seq = {}", self.first_seq);
        let _ = writeln!(out, "last_seq = {}", self.last_seq);
        let _ = writeln!(out, "prev = {:?}", self.prev.as_str());
        let _ = writeln!(out, "head = {:?}", self.head.as_str());
        let _ = writeln!(out, "lines_sha256 = {:?}", self.lines_sha256.as_str());
        let _ = writeln!(out, "policy = {:?}", self.policy.as_str());
        let _ = writeln!(out, "file = {:?}", self.file_name);
        out
    }
}

/// Plan the segment of `lines` from `since` to the last line.
///
/// `chain_id` is the hash of line 1, `store_uid` is the nil UUID when the machine has no
/// company store, and `policy` is the policy digest in force. The exported bytes are the
/// selected lines joined exactly as they are stored; nothing is re-serialised.
///
/// # Errors
///
/// Returns [`ExportError::OutOfRange`] when `since` is 0 and
/// [`ExportError::NothingNew`] when `since` is after the last line, or when a missing
/// previous line leaves the chain link unknown.
pub fn plan_export(
    lines: &[ExportLine],
    chain_id: Sha256,
    writer: u32,
    store_uid: &str,
    since: u64,
    policy: Sha256,
    tool_version: &str,
) -> Result<ExportPlan, ExportError> {
    if since == 0 {
        return Err(ExportError::OutOfRange { since });
    }
    let last_seq = lines.last().map_or(0, |line| line.seq);
    if since > last_seq {
        return Err(ExportError::NothingNew { last_seq });
    }

    let selected: Vec<&ExportLine> = lines.iter().filter(|line| line.seq >= since).collect();
    let Some(first_line) = selected.first() else {
        return Err(ExportError::NothingNew { last_seq });
    };
    let Some(last_line) = selected.last() else {
        return Err(ExportError::NothingNew { last_seq });
    };
    let first_seq = first_line.seq;
    let last = last_line.seq;

    let prev = if first_seq <= 1 {
        Sha256::zero()
    } else {
        match lines.iter().find(|line| line.seq == first_seq - 1) {
            Some(line) => line.hash.clone(),
            None => return Err(ExportError::OutOfRange { since: first_seq }),
        }
    };

    let mut bytes = Vec::new();
    for line in &selected {
        bytes.extend_from_slice(line.raw.as_bytes());
    }
    let lines_sha256 = Sha256::digest(&bytes);
    let head = last_line.hash.clone();

    let chain_prefix = &chain_id.as_str()[..12.min(chain_id.as_str().len())];
    let file_name = format!("journal-w{writer}-{chain_prefix}-{first_seq:06}-{last}.jsonl");
    let manifest_name = format!(
        "{}.manifest.toml",
        file_name.strip_suffix(".jsonl").unwrap_or(&file_name)
    );

    Ok(ExportPlan {
        first_seq,
        last_seq: last,
        prev,
        head,
        lines_sha256,
        file_name,
        manifest_name,
        bytes,
        chain_id,
        writer,
        store_uid: store_uid.to_owned(),
        policy,
        tool_version: tool_version.to_owned(),
    })
}

/// One finding raised by [`verify_export`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Diagnostic code (`FB0800`, `FB0801` or `FB0802`).
    pub code: Code,
    /// Line the finding names (`0` for a whole-manifest finding).
    pub seq: u64,
    /// Plain explanation with no document text.
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.id(), self.message)
    }
}

/// The parsed fields of an export manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Manifest schema version.
    pub manifest_version: u32,
    /// Tool version that wrote the export.
    pub tool_version: String,
    /// Hash of line 1 of the journal.
    pub chain_id: Sha256,
    /// Journal writer index.
    pub writer: u32,
    /// Company store UUID, or the nil UUID.
    pub store_uid: String,
    /// First sequence number in the segment.
    pub first_seq: u64,
    /// Last sequence number in the segment.
    pub last_seq: u64,
    /// Chain hash before the segment.
    pub prev: Sha256,
    /// Chain hash of the last line in the segment.
    pub head: Sha256,
    /// SHA-256 of the exact segment bytes.
    pub lines_sha256: Sha256,
    /// Policy digest in force.
    pub policy: Sha256,
    /// File name of the exported journal segment.
    pub file: String,
}

impl Manifest {
    /// Parse and validate the manifest's keys and fields.
    ///
    /// # Errors
    ///
    /// Returns the findings that name every unknown or missing key and every malformed value.
    pub fn parse(text: &str) -> Result<Self, Vec<Finding>> {
        let table: toml::Table = match toml::from_str(text) {
            Ok(table) => table,
            Err(_) => {
                return Err(vec![whole_finding("the manifest is not valid TOML")]);
            }
        };

        let mut findings = Vec::new();
        for key in table.keys() {
            if !MANIFEST_KEYS.contains(&key.as_str()) {
                findings.push(whole_finding(&format!(
                    "the manifest has an unknown key `{key}`"
                )));
            }
        }
        for key in MANIFEST_KEYS {
            if !table.contains_key(key) {
                findings.push(whole_finding(&format!("the manifest is missing `{key}`")));
            }
        }
        if !findings.is_empty() {
            return Err(findings);
        }

        let manifest_version = req_u32(&table, "manifest_version", &mut findings);
        let writer = req_u32(&table, "writer", &mut findings);
        let tool_version = req_string(&table, "tool_version", &mut findings);
        let store_uid = req_string(&table, "store_uid", &mut findings);
        let file = req_string(&table, "file", &mut findings);
        let first_seq = req_u64(&table, "first_seq", &mut findings);
        let last_seq = req_u64(&table, "last_seq", &mut findings);
        let chain_id = req_sha(&table, "chain_id", &mut findings);
        let prev = req_sha(&table, "prev", &mut findings);
        let head = req_sha(&table, "head", &mut findings);
        let lines_sha256 = req_sha(&table, "lines_sha256", &mut findings);
        let policy = req_sha(&table, "policy", &mut findings);

        if let Some(version) = manifest_version
            && version != MANIFEST_VERSION
        {
            findings.push(whole_finding(&format!(
                "the manifest version is {version}, not {MANIFEST_VERSION}"
            )));
        }
        if let Some(name) = &file
            && (name.is_empty()
                || name == "."
                || name == ".."
                || name.contains(['/', '\\', ':'])
                || name.contains('\0'))
        {
            findings.push(whole_finding(
                "the manifest file is not a bare file name in the manifest's folder",
            ));
        }
        if let Some(uid) = &store_uid
            && !is_uuid(uid)
        {
            findings.push(whole_finding("the manifest store_uid is not a UUID"));
        }
        if let (Some(first), Some(_last)) = (first_seq, last_seq)
            && first == 0
        {
            findings.push(whole_finding("the manifest first_seq is 0"));
        }

        if !findings.is_empty() {
            return Err(findings);
        }

        Ok(Self {
            manifest_version: manifest_version.unwrap_or(MANIFEST_VERSION),
            tool_version: tool_version.unwrap_or_default(),
            chain_id: chain_id.unwrap_or_else(Sha256::zero),
            writer: writer.unwrap_or(0),
            store_uid: store_uid.unwrap_or_else(|| NIL_UUID.to_owned()),
            first_seq: first_seq.unwrap_or(0),
            last_seq: last_seq.unwrap_or(0),
            prev: prev.unwrap_or_else(Sha256::zero),
            head: head.unwrap_or_else(Sha256::zero),
            lines_sha256: lines_sha256.unwrap_or_else(Sha256::zero),
            policy: policy.unwrap_or_else(Sha256::zero),
            file: file.unwrap_or_default(),
        })
    }
}

fn whole_finding(message: &str) -> Finding {
    Finding {
        code: Code::Fb0802,
        seq: 0,
        message: message.to_owned(),
    }
}

fn req_string(table: &toml::Table, key: &str, findings: &mut Vec<Finding>) -> Option<String> {
    match table.get(key).and_then(toml::Value::as_str) {
        Some(value) => Some(value.to_owned()),
        None => {
            findings.push(whole_finding(&format!(
                "the manifest `{key}` is not a string"
            )));
            None
        }
    }
}

fn req_u64(table: &toml::Table, key: &str, findings: &mut Vec<Finding>) -> Option<u64> {
    match table.get(key).and_then(toml::Value::as_integer) {
        Some(value) => match u64::try_from(value) {
            Ok(value) => Some(value),
            Err(_) => {
                findings.push(whole_finding(&format!("the manifest `{key}` is negative")));
                None
            }
        },
        None => {
            findings.push(whole_finding(&format!(
                "the manifest `{key}` is not an integer"
            )));
            None
        }
    }
}

fn req_u32(table: &toml::Table, key: &str, findings: &mut Vec<Finding>) -> Option<u32> {
    let value = req_u64(table, key, findings)?;
    match u32::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => {
            findings.push(whole_finding(&format!(
                "the manifest `{key}` is out of range"
            )));
            None
        }
    }
}

fn req_sha(table: &toml::Table, key: &str, findings: &mut Vec<Finding>) -> Option<Sha256> {
    let value = req_string(table, key, findings)?;
    match Sha256::parse(&value) {
        Ok(sha) => Some(sha),
        Err(_) => {
            findings.push(whole_finding(&format!(
                "the manifest `{key}` is not a SHA-256"
            )));
            None
        }
    }
}

/// Returns `true` for a canonical lower-case `8-4-4-4-12` UUID.
#[must_use]
pub fn is_uuid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(idx, &b)| {
            if matches!(idx, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

/// Verify an exported copy without any state folder.
///
/// Checks every manifest key and field, the segment byte digest, the hash chain recomputed
/// from `prev`, the vocabulary of every line, the sequence range and head, and the optional
/// `expect_head` and `expect_seq` against the segment. Never panics on arbitrary input.
#[must_use]
pub fn verify_export(
    manifest_text: &str,
    lines_bytes: &[u8],
    expect_head: Option<&str>,
    expect_seq: Option<u64>,
) -> Vec<Finding> {
    let manifest = match Manifest::parse(manifest_text) {
        Ok(manifest) => manifest,
        Err(findings) => return findings,
    };

    let mut findings = Vec::new();
    let text = String::from_utf8_lossy(lines_bytes);
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let found = lines.len();
    let expected = usize::try_from(
        manifest
            .last_seq
            .saturating_sub(manifest.first_seq)
            .saturating_add(1),
    )
    .unwrap_or(0);

    let actual_digest = Sha256::digest(lines_bytes);
    if actual_digest != manifest.lines_sha256 {
        findings.push(Finding {
            code: Code::Fb0802,
            seq: 0,
            message: format!("the segment digest differs; the copy holds {found} lines"),
        });
    }

    if found != expected {
        findings.push(Finding {
            code: Code::Fb0802,
            seq: 0,
            message: format!("the manifest names {expected} lines but the copy holds {found}"),
        });
    }

    let mut prev_hex = manifest.prev.as_str().to_owned();
    let mut first_failure: Option<Finding> = None;
    for (idx, raw_line) in lines.iter().enumerate() {
        let seq = manifest.first_seq.saturating_add(idx as u64);
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let Ok((body, stored_hash)) = split_hash_suffix(line) else {
            if first_failure.is_none() {
                first_failure = Some(Finding {
                    code: Code::Fb0800,
                    seq,
                    message: format!("line {seq} is not a sealed journal line"),
                });
            }
            break;
        };
        let computed = compute_hash(&prev_hex, body);
        if computed != stored_hash {
            if first_failure.is_none() {
                first_failure = Some(Finding {
                    code: Code::Fb0800,
                    seq,
                    message: format!("line {seq} hash chain is broken"),
                });
            }
            break;
        }
        match Event::parse_line(line) {
            Ok((event, _)) => {
                if event.seq != seq && first_failure.is_none() {
                    first_failure = Some(Finding {
                        code: Code::Fb0802,
                        seq,
                        message: format!("line {seq} carries sequence number {}", event.seq),
                    });
                }
            }
            Err(err) => {
                findings.push(Finding {
                    code: Code::Fb0801,
                    seq,
                    message: format!("line {seq} invalid vocabulary or envelope: {err}"),
                });
            }
        }
        prev_hex = stored_hash.as_str().to_owned();
    }

    if let Some(finding) = first_failure {
        findings.push(finding);
    } else if let Some(last_line) = lines.last() {
        let tail_hash = split_hash_suffix(last_line.strip_suffix('\r').unwrap_or(last_line))
            .map(|(_, hash)| hash)
            .unwrap_or_else(|_| Sha256::zero());
        if tail_hash != manifest.head {
            findings.push(Finding {
                code: Code::Fb0802,
                seq: manifest.last_seq,
                message: "the manifest head is not the last line's hash".to_owned(),
            });
        }
    }

    if let Some(expected_head) = expect_head
        && expected_head != manifest.head.as_str()
    {
        findings.push(Finding {
            code: Code::Fb0802,
            seq: manifest.last_seq,
            message: "the segment head does not match --expect-head".to_owned(),
        });
    }
    if let Some(expected_seq) = expect_seq
        && expected_seq != manifest.last_seq
    {
        findings.push(Finding {
            code: Code::Fb0802,
            seq: manifest.last_seq,
            message: "the segment length does not match --expect-seq".to_owned(),
        });
    }

    findings
}

/// The 64-zero hash used before the first journal line.
#[must_use]
pub const fn zero_hash() -> &'static str {
    ZERO_HASH
}
