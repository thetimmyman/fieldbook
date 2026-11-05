//! Append-only, hash-chained audit journal types, vocabulary,settings baseline,
//! and operation intents (`audit-journal`, AD-16, ADR 0004 §12).

pub mod chain;
pub mod kind;
pub mod vocab;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Sha256Hasher};

use crate::date::CivilDate;

pub use self::chain::{ChainError, VerifiedLine, seal_body, split_hash_suffix, verify_line};
pub use self::kind::{
    BackupVerified, Event, EventEnvelope, EventPayload, FieldSpec, Kind, RefuseAbout,
    sample_payload,
};
pub use self::vocab::{
    Actor, AskedVia, CorpusPath, Count, DestKind, EndpointName, Id, LabelAllow, LabelId, LabelSet,
    ModelId, NIL_UUID, Principal, RecordRef, Role, SemVer, Sha256, StoreId, StoreUid, StreamKey,
    Verb, Via, VocabError, Word, ZERO_HASH,
};

/// Domain separator for normalized write-request hashes (`OperationIntent.request_sha256`).
pub const WRITE_REQUEST_DOMAIN: &[u8] = b"fieldbook-write-request-1";

/// Domain separator for canonical settings group digests (`SettingsBaseline`).
pub const SETTINGS_GROUP_DOMAIN: &[u8] = b"fieldbook-settings-group-1";

/// Domain separator for canonical source-root fingerprints (`sync.begin.root_sha256`).
pub const SOURCE_ROOT_DOMAIN: &[u8] = b"fieldbook-source-root-1";

/// UTC timestamp (`date` and `seconds` within the UTC day, `0..86_400`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Now {
    /// Civil date in UTC.
    pub date: CivilDate,
    /// Seconds since `00:00:00Z` (`0..86_400`).
    pub seconds: u32,
}

impl Now {
    /// Construct a `Now` after validating `seconds < 86_400`.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidTimestamp`] if `seconds >= 86_400`.
    pub fn new(date: CivilDate, seconds: u32) -> Result<Self, VocabError> {
        if seconds >= 86_400 {
            return Err(VocabError::InvalidTimestamp(format!("{date}+{seconds}s")));
        }
        Ok(Self { date, seconds })
    }

    /// Convert Unix epoch seconds to a UTC `Now`.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidTimestamp`] if the resulting date lies outside the
    /// supported `CivilDate` range (`1900..=2199`).
    pub fn from_unix_secs(epoch_secs: u64) -> Result<Self, VocabError> {
        let days = (epoch_secs / 86_400).cast_signed();
        let seconds = (epoch_secs % 86_400) as u32;
        let (year, month, day) = civil_from_days(days);
        let date_str = format!("{year:04}-{month:02}-{day:02}");
        let date =
            CivilDate::parse(&date_str).map_err(|_| VocabError::InvalidTimestamp(date_str))?;
        Self::new(date, seconds)
    }

    /// Parse a UTC timestamp formatted as `YYYY-MM-DDTHH:MM:SSZ`.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidTimestamp`] if `text` is not a valid `YYYY-MM-DDTHH:MM:SSZ`
    /// UTC timestamp.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        let bytes = text.as_bytes();
        if bytes.len() != 20
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || bytes[16] != b':'
            || bytes[19] != b'Z'
        {
            return Err(VocabError::InvalidTimestamp(text.to_owned()));
        }
        let date = CivilDate::parse(&text[..10])
            .map_err(|_| VocabError::InvalidTimestamp(text.to_owned()))?;
        let hh = parse_two_digits(&bytes[11..13])
            .ok_or_else(|| VocabError::InvalidTimestamp(text.to_owned()))?;
        let mm = parse_two_digits(&bytes[14..16])
            .ok_or_else(|| VocabError::InvalidTimestamp(text.to_owned()))?;
        let ss = parse_two_digits(&bytes[17..19])
            .ok_or_else(|| VocabError::InvalidTimestamp(text.to_owned()))?;
        if hh > 23 || mm > 59 || ss > 59 {
            return Err(VocabError::InvalidTimestamp(text.to_owned()));
        }
        let seconds = hh * 3600 + mm * 60 + ss;
        Self::new(date, seconds)
    }
}

fn parse_two_digits(bytes: &[u8]) -> Option<u32> {
    if bytes.len() == 2 && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit() {
        Some(u32::from(bytes[0] - b'0') * 10 + u32::from(bytes[1] - b'0'))
    } else {
        None
    }
}

/// Howard Hinnant's civil-from-days algorithm (days since 1970-01-01).
#[expect(
    clippy::cast_possible_truncation,
    reason = "month (1..=12) and day (1..=31) fit in u32"
)]
const fn civil_from_days(days_since_1970: i64) -> (i64, u32, u32) {
    let z = days_since_1970 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097).cast_unsigned();
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe.cast_signed() + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

impl fmt::Display for Now {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hh = self.seconds / 3600;
        let mm = (self.seconds % 3600) / 60;
        let ss = self.seconds % 60;
        write!(f, "{}T{hh:02}:{mm:02}:{ss:02}Z", self.date)
    }
}

impl Serialize for Now {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// One planned file in a [`WriteSet`]: its canonical root-relative slash path and the
/// SHA-256 hex digest of the bytes to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteFile {
    /// Canonical root-relative slash path.
    pub path: String,
    /// Lowercase 64-hex SHA-256 of the planned after-bytes.
    pub sha256: String,
}

impl WriteFile {
    /// Construct a `WriteFile`.
    #[must_use]
    pub fn new(path: impl Into<String>, sha256: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            sha256: sha256.into(),
        }
    }
}

/// The complete planned write batch handed to `Journal::begin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteSet {
    /// Union of labels of the write set.
    pub labels: LabelSet,
    /// Files to be written, with their planned after-byte hashes.
    pub files: Vec<WriteFile>,
    /// Store-qualified record references affected by this write batch.
    pub affected: Vec<RecordRef>,
}

/// Typed path/after-content digest pair recorded in `write.begin.file_hashes` for in-root writes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileHash {
    /// SHA-256 of the canonical root-relative slash path.
    pub path_sha256: Sha256,
    /// SHA-256 of the planned file bytes.
    pub sha256: Sha256,
}

/// Compute sorted, unique [`FileHash`] entries from a [`WriteSet`]'s files.
///
/// # Errors
///
/// Returns [`VocabError`] if any file path is not a canonical relative slash path, any
/// `sha256` is not a valid 64-hex digest, or two files share the same canonical path.
pub fn compute_file_hashes(files: &[WriteFile]) -> Result<Vec<FileHash>, VocabError> {
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        let canon = CorpusPath::parse(&file.path)?;
        let sha = Sha256::parse(&file.sha256)
            .map_err(|_| VocabError::InvalidSha256(file.sha256.clone()))?;
        let path_sha256 = Sha256::digest(canon.as_str().as_bytes());
        out.push(FileHash {
            path_sha256,
            sha256: sha,
        });
    }
    out.sort();
    for pair in out.windows(2) {
        if pair[0].path_sha256 == pair[1].path_sha256 {
            return Err(VocabError::InvalidCorpusPath(
                "duplicate file path in write set".to_owned(),
            ));
        }
    }
    Ok(out)
}

/// Normalize and validate `affected` record references (sorted, distinct).
#[must_use]
pub fn normalize_affected(affected: &[RecordRef]) -> Vec<RecordRef> {
    let mut set = BTreeSet::new();
    for item in affected {
        set.insert(item.clone());
    }
    set.into_iter().collect()
}

/// Return the latest completed after-content hash for `path` in `store` from verified
/// journal events.
///
/// Only a `write.begin` in `store` paired with a matching `write.end` whose `outcome == "ok"`
/// in the same `store` supplies completed-write evidence; open, failed, or foreign-store
/// events never do.
#[must_use]
pub fn completed_after_hash(events: &[Event], store: StoreId, path: &str) -> Option<Sha256> {
    let Ok(canon) = CorpusPath::parse(path) else {
        return None;
    };
    let target_path_sha = Sha256::digest(canon.as_str().as_bytes());
    let mut open_begin: Option<(&Verb, &[RecordRef], &[FileHash])> = None;
    let mut latest = None;

    for event in events {
        if event.envelope.store != store {
            continue;
        }
        match &event.payload {
            EventPayload::WriteBegin {
                verb,
                file_hashes: Some(hashes),
                affected,
                dest: None,
                ..
            } => {
                open_begin = Some((verb, affected.as_slice(), hashes.as_slice()));
            }
            EventPayload::WriteEnd {
                verb,
                affected,
                outcome,
                dest: None,
                ..
            } => {
                if let Some((begin_verb, begin_affected, hashes)) = open_begin.take()
                    && begin_verb == verb
                    && begin_affected == affected.as_slice()
                    && outcome.as_str() == "ok"
                {
                    for entry in hashes {
                        if entry.path_sha256 == target_path_sha {
                            latest = Some(entry.sha256.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    latest
}

/// Hash a canonical source-root path according to the source-root fingerprint contract
/// (`fieldbook-source-root-1`, platform byte `0` for Unix or `1` for Windows, `u64` big-endian
/// byte length, and canonical native path bytes).
#[must_use]
pub fn source_root_sha256(windows_platform: bool, canonical_native_bytes: &[u8]) -> Sha256 {
    let mut hasher = Sha256Hasher::new();
    hasher.update(SOURCE_ROOT_DOMAIN);
    hasher.update([u8::from(windows_platform)]);
    hasher.update((canonical_native_bytes.len() as u64).to_be_bytes());
    hasher.update(canonical_native_bytes);
    let digest = hasher.finalize();
    let hex_str = crate::hex::lower(&digest);
    Sha256::parse(&hex_str).unwrap_or_else(|_| unreachable!())
}

/// Errors raised when constructing or parsing an [`OperationIntent`] or [`PendingWrite`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentError {
    /// Duplicate or empty field name in `request_sha256`.
    DuplicateRequestField(String),
    /// Unsupported schema version.
    UnsupportedSchema(u32),
    /// Malformed TOML or invalid field value in `pending.toml`.
    InvalidPending(String),
}

impl fmt::Display for IntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRequestField(name) => {
                write!(f, "duplicate or empty request field `{name}`")
            }
            Self::UnsupportedSchema(v) => write!(f, "unsupported intent schema `{v}`"),
            Self::InvalidPending(msg) => write!(f, "invalid pending write descriptor: {msg}"),
        }
    }
}

impl std::error::Error for IntentError {}

/// Compute the normalized write-request hash over `fieldbook-write-request-1` and sorted
/// length-prefixed `(name, value)` pairs.
///
/// # Errors
///
/// Returns [`IntentError::DuplicateRequestField`] if any field name is empty or repeated.
pub fn request_sha256(fields: &[(&str, &[u8])]) -> Result<Sha256, IntentError> {
    let mut sorted: Vec<(&str, &[u8])> = fields.to_vec();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    for &(name, _) in &sorted {
        if name.is_empty() {
            return Err(IntentError::DuplicateRequestField(String::new()));
        }
    }
    for pair in sorted.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(IntentError::DuplicateRequestField(pair[0].0.to_owned()));
        }
    }

    let mut hasher = Sha256Hasher::new();
    hasher.update(WRITE_REQUEST_DOMAIN);
    for (name, value) in sorted {
        hasher.update((name.len() as u64).to_be_bytes());
        hasher.update(name.as_bytes());
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    let digest = hasher.finalize();
    let hex_str = crate::hex::lower(&digest);
    Ok(Sha256::parse(&hex_str).unwrap_or_else(|_| unreachable!()))
}

/// Typed operation identity (`verb` and `subject_sha256`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationKey {
    /// Validated verb word(s).
    pub verb: Verb,
    /// SHA-256 identifying the operation's target subject.
    pub subject_sha256: Sha256,
}

impl OperationKey {
    /// Construct an `OperationKey`.
    #[must_use]
    pub const fn new(verb: Verb, subject_sha256: Sha256) -> Self {
        Self {
            verb,
            subject_sha256,
        }
    }
}

/// Persisted operation intent for crash recovery (`schema = 1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationIntent {
    /// Intent schema version (`1`).
    pub schema: u32,
    /// Operation identity.
    pub key: OperationKey,
    /// Normalized request digest from [`request_sha256`].
    pub request_sha256: Sha256,
    /// Sorted, distinct record references allocated by this operation.
    pub allocated: Vec<RecordRef>,
    /// Allocated store UUID for `init --overlay`, absent for ordinary record writes.
    pub allocated_store_uid: Option<StoreUid>,
    /// Civil date on which the operation was planned.
    pub planned_on: CivilDate,
}

impl OperationIntent {
    /// Validate invariants of this `OperationIntent`.
    ///
    /// # Errors
    ///
    /// Returns [`IntentError`] if `schema != 1` or `allocated` contains duplicates or is unsorted.
    pub fn validate(&self) -> Result<(), IntentError> {
        if self.schema != 1 {
            return Err(IntentError::UnsupportedSchema(self.schema));
        }
        for pair in self.allocated.windows(2) {
            if pair[0] >= pair[1] {
                return Err(IntentError::InvalidPending(
                    "allocated record references must be sorted and distinct".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// Read-only discovery result returned by `Journal::pending_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingKey {
    /// Operation identity of the pending write.
    pub key: OperationKey,
    /// Normalized request digest of the pending write.
    pub request_sha256: Sha256,
}

/// Read-only pending operation details returned by `Journal::pending_intent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOperation {
    /// Stored operation intent (including original allocations and planning date).
    pub intent: OperationIntent,
    /// Stored complete write set (labels, files with after hashes, and affected records).
    pub writes: WriteSet,
}

/// One planned file entry in `state/pending.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFile {
    /// Canonical root-relative slash path.
    pub path: CorpusPath,
    /// SHA-256 of the file before the write (`None` when the file did not exist before).
    pub before_sha256: Option<Sha256>,
    /// SHA-256 of the planned after-bytes.
    pub after_sha256: Sha256,
}

/// Typed pending-write descriptor persisted at `state/pending.toml` before `write.begin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingWrite {
    /// Descriptor schema version (`1`).
    pub schema: u32,
    /// Planned sequence number of the `write.begin` event.
    pub begin_seq: u64,
    /// Committed store generation hash before the write started.
    pub source_generation: Sha256,
    /// Labels of the write set.
    pub labels: LabelSet,
    /// Sorted, distinct affected record references.
    pub affected: Vec<RecordRef>,
    /// Operation intent bound to this write.
    pub intent: OperationIntent,
    /// Ordered root-relative files with before and after hashes.
    pub files: Vec<PendingFile>,
}

impl PendingWrite {
    /// Validate invariants of this `PendingWrite`.
    ///
    /// # Errors
    ///
    /// Returns [`IntentError`] if schema, allocations, affected references, or file entries
    /// violate the pending-write contract.
    pub fn validate(&self) -> Result<(), IntentError> {
        if self.schema != 1 {
            return Err(IntentError::UnsupportedSchema(self.schema));
        }
        if self.begin_seq == 0 {
            return Err(IntentError::InvalidPending(
                "begin_seq must be >= 1".to_owned(),
            ));
        }
        self.intent.validate()?;
        for pair in self.affected.windows(2) {
            if pair[0] >= pair[1] {
                return Err(IntentError::InvalidPending(
                    "affected record references must be sorted and distinct".to_owned(),
                ));
            }
        }
        if self.files.is_empty() {
            return Err(IntentError::InvalidPending(
                "pending write must list at least one file".to_owned(),
            ));
        }
        let mut seen_paths = BTreeSet::new();
        for file in &self.files {
            if !seen_paths.insert(file.path.as_str()) {
                return Err(IntentError::InvalidPending(format!(
                    "duplicate file path `{}` in pending.toml",
                    file.path
                )));
            }
        }
        Ok(())
    }

    /// Reconstruct the [`WriteSet`] stored in this descriptor.
    #[must_use]
    pub fn to_write_set(&self) -> WriteSet {
        WriteSet {
            labels: self.labels.clone(),
            files: self
                .files
                .iter()
                .map(|f| WriteFile {
                    path: f.path.as_str().to_owned(),
                    sha256: f.after_sha256.as_str().to_owned(),
                })
                .collect(),
            affected: self.affected.clone(),
        }
    }

    /// Render this `PendingWrite` to canonical TOML text.
    #[must_use]
    pub fn to_toml(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(out, "schema = {}", self.schema);
        let _ = writeln!(out, "begin_seq = {}", self.begin_seq);
        let _ = writeln!(out, "source_generation = \"{}\"", self.source_generation);
        let labels_joined: Vec<String> = self
            .labels
            .ids()
            .iter()
            .map(|l| format!("\"{}\"", l.as_str()))
            .collect();
        let _ = writeln!(out, "labels = [{}]", labels_joined.join(", "));
        let affected_joined: Vec<String> =
            self.affected.iter().map(|r| format!("\"{r}\"")).collect();
        let _ = writeln!(out, "affected = [{}]", affected_joined.join(", "));
        out.push('\n');
        out.push_str("[intent]\n");
        let _ = writeln!(out, "schema = {}", self.intent.schema);
        let _ = writeln!(out, "verb = \"{}\"", self.intent.key.verb);
        let _ = writeln!(
            out,
            "subject_sha256 = \"{}\"",
            self.intent.key.subject_sha256
        );
        let _ = writeln!(out, "request_sha256 = \"{}\"", self.intent.request_sha256);
        let alloc_joined: Vec<String> = self
            .intent
            .allocated
            .iter()
            .map(|r| format!("\"{r}\""))
            .collect();
        let _ = writeln!(out, "allocated = [{}]", alloc_joined.join(", "));
        if let Some(uid) = &self.intent.allocated_store_uid {
            let _ = writeln!(out, "allocated_store_uid = \"{uid}\"");
        }
        let _ = writeln!(out, "planned_on = \"{}\"", self.intent.planned_on);

        for file in &self.files {
            out.push('\n');
            out.push_str("[[file]]\n");
            let _ = writeln!(out, "path = \"{}\"", file.path);
            if let Some(before) = &file.before_sha256 {
                let _ = writeln!(out, "before_sha256 = \"{before}\"");
            }
            let _ = writeln!(out, "after_sha256 = \"{}\"", file.after_sha256);
        }
        out
    }

    /// Parse and validate a `PendingWrite` from TOML text.
    ///
    /// # Errors
    ///
    /// Returns [`IntentError`] if `text` is invalid TOML, contains unknown keys, or violates
    /// any pending-write invariant.
    pub fn parse_toml(text: &str) -> Result<Self, IntentError> {
        let table: toml::Table =
            toml::from_str(text).map_err(|e| IntentError::InvalidPending(e.to_string()))?;
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "schema"
                    | "begin_seq"
                    | "source_generation"
                    | "labels"
                    | "affected"
                    | "intent"
                    | "file"
            ) {
                return Err(IntentError::InvalidPending(format!("unknown key `{key}`")));
            }
        }
        let schema = toml_u32(&table, "schema")?;
        if schema != 1 {
            return Err(IntentError::UnsupportedSchema(schema));
        }
        let begin_seq = toml_u64(&table, "begin_seq")?;
        let source_generation = toml_sha256(&table, "source_generation")?;

        let labels_arr = table
            .get("labels")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| IntentError::InvalidPending("missing `labels` array".to_owned()))?;
        let mut label_ids = Vec::with_capacity(labels_arr.len());
        for item in labels_arr {
            let s = item
                .as_str()
                .ok_or_else(|| IntentError::InvalidPending("label must be a string".to_owned()))?;
            let id = LabelId::parse(s)
                .map_err(|_| IntentError::InvalidPending(format!("invalid label `{s}`")))?;
            label_ids.push(id);
        }
        if label_ids.is_empty() {
            return Err(IntentError::InvalidPending(
                "labels must not be empty".to_owned(),
            ));
        }
        let labels = LabelSet::from_ids(label_ids.clone());
        if labels.ids() != label_ids.as_slice() {
            return Err(IntentError::InvalidPending(
                "labels must be normalized, sorted and distinct".to_owned(),
            ));
        }

        let affected_arr = table
            .get("affected")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| IntentError::InvalidPending("missing `affected` array".to_owned()))?;
        let mut affected = Vec::with_capacity(affected_arr.len());
        for item in affected_arr {
            let s = item.as_str().ok_or_else(|| {
                IntentError::InvalidPending("affected entry must be a string".to_owned())
            })?;
            affected
                .push(RecordRef::parse(s).map_err(|e| IntentError::InvalidPending(e.to_string()))?);
        }

        let intent_tbl = table
            .get("intent")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| IntentError::InvalidPending("missing `[intent]` table".to_owned()))?;
        for key in intent_tbl.keys() {
            if !matches!(
                key.as_str(),
                "schema"
                    | "verb"
                    | "subject_sha256"
                    | "request_sha256"
                    | "allocated"
                    | "allocated_store_uid"
                    | "planned_on"
            ) {
                return Err(IntentError::InvalidPending(format!(
                    "unknown intent key `{key}`"
                )));
            }
        }
        let intent_schema = toml_u32(intent_tbl, "schema")?;
        if intent_schema != 1 {
            return Err(IntentError::UnsupportedSchema(intent_schema));
        }
        let verb_raw = toml_str(intent_tbl, "verb")?;
        let verb = Verb::parse(verb_raw).map_err(|e| IntentError::InvalidPending(e.to_string()))?;
        let subject_sha256 = toml_sha256(intent_tbl, "subject_sha256")?;
        let req_sha256 = toml_sha256(intent_tbl, "request_sha256")?;

        let alloc_arr = intent_tbl
            .get("allocated")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| IntentError::InvalidPending("missing `allocated` array".to_owned()))?;
        let mut allocated = Vec::with_capacity(alloc_arr.len());
        for item in alloc_arr {
            let s = item.as_str().ok_or_else(|| {
                IntentError::InvalidPending("allocated entry must be a string".to_owned())
            })?;
            allocated
                .push(RecordRef::parse(s).map_err(|e| IntentError::InvalidPending(e.to_string()))?);
        }

        let allocated_store_uid = match intent_tbl.get("allocated_store_uid") {
            Some(v) => {
                let s = v.as_str().ok_or_else(|| {
                    IntentError::InvalidPending("allocated_store_uid must be a string".to_owned())
                })?;
                Some(StoreUid::parse(s).map_err(|e| IntentError::InvalidPending(e.to_string()))?)
            }
            None => None,
        };

        let planned_raw = toml_str(intent_tbl, "planned_on")?;
        let planned_on = CivilDate::parse(planned_raw)
            .map_err(|_| IntentError::InvalidPending(format!("invalid date `{planned_raw}`")))?;

        let files_arr = table
            .get("file")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| IntentError::InvalidPending("missing `[[file]]` array".to_owned()))?;
        let mut files = Vec::with_capacity(files_arr.len());
        for item in files_arr {
            let file_tbl = item.as_table().ok_or_else(|| {
                IntentError::InvalidPending("file entry must be a table".to_owned())
            })?;
            for key in file_tbl.keys() {
                if !matches!(key.as_str(), "path" | "before_sha256" | "after_sha256") {
                    return Err(IntentError::InvalidPending(format!(
                        "unknown file key `{key}`"
                    )));
                }
            }
            let path_raw = toml_str(file_tbl, "path")?;
            let path = CorpusPath::parse(path_raw)
                .map_err(|e| IntentError::InvalidPending(e.to_string()))?;
            let before_sha256 =
                match file_tbl.get("before_sha256") {
                    Some(v) => {
                        let s = v.as_str().ok_or_else(|| {
                            IntentError::InvalidPending("before_sha256 must be a string".to_owned())
                        })?;
                        Some(Sha256::parse(s).map_err(|_| {
                            IntentError::InvalidPending(format!("bad sha256 `{s}`"))
                        })?)
                    }
                    None => None,
                };
            let after_sha256 = toml_sha256(file_tbl, "after_sha256")?;
            files.push(PendingFile {
                path,
                before_sha256,
                after_sha256,
            });
        }

        let pw = Self {
            schema,
            begin_seq,
            source_generation,
            labels,
            affected,
            intent: OperationIntent {
                schema: intent_schema,
                key: OperationKey::new(verb, subject_sha256),
                request_sha256: req_sha256,
                allocated,
                allocated_store_uid,
                planned_on,
            },
            files,
        };
        pw.validate()?;
        Ok(pw)
    }
}

fn toml_str<'a>(table: &'a toml::Table, key: &str) -> Result<&'a str, IntentError> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| IntentError::InvalidPending(format!("missing or non-string `{key}`")))
}

fn toml_u64(table: &toml::Table, key: &str) -> Result<u64, IntentError> {
    let val = table
        .get(key)
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| IntentError::InvalidPending(format!("missing or non-integer `{key}`")))?;
    u64::try_from(val).map_err(|_| IntentError::InvalidPending(format!("negative `{key}`")))
}

fn toml_u32(table: &toml::Table, key: &str) -> Result<u32, IntentError> {
    let val = toml_u64(table, key)?;
    u32::try_from(val).map_err(|_| IntentError::InvalidPending(format!("out-of-range `{key}`")))
}

fn toml_sha256(table: &toml::Table, key: &str) -> Result<Sha256, IntentError> {
    let raw = toml_str(table, key)?;
    Sha256::parse(raw).map_err(|_| IntentError::InvalidPending(format!("invalid `{key}`")))
}

/// Closed list of settings groups tracked by `settings.change` (`audit-contract.md#settings-baseline`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsGroup {
    /// `asked`
    Asked,
    /// `backup`
    Backup,
    /// `chat`
    Chat,
    /// `mcp.allow`
    McpAllow,
    /// `model.endpoint.allow`
    ModelEndpointAllow,
    /// `model.endpoint.base_url`
    ModelEndpointBaseUrl,
    /// `model.handoff.allow`
    ModelHandoffAllow,
    /// `model.order`
    ModelOrder,
    /// `other`
    Other,
    /// `role`
    Role,
}

impl SettingsGroup {
    /// All 10 settings groups in canonical string-sorted order.
    pub const ALL: [Self; 10] = [
        Self::Asked,
        Self::Backup,
        Self::Chat,
        Self::McpAllow,
        Self::ModelEndpointAllow,
        Self::ModelEndpointBaseUrl,
        Self::ModelHandoffAllow,
        Self::ModelOrder,
        Self::Other,
        Self::Role,
    ];

    /// Parse a settings group name.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidWord`] if `text` is not one of the 10 closed group names.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        match text {
            "role" => Ok(Self::Role),
            "backup" => Ok(Self::Backup),
            "asked" => Ok(Self::Asked),
            "model.order" => Ok(Self::ModelOrder),
            "model.endpoint.allow" => Ok(Self::ModelEndpointAllow),
            "model.endpoint.base_url" => Ok(Self::ModelEndpointBaseUrl),
            "model.handoff.allow" => Ok(Self::ModelHandoffAllow),
            "mcp.allow" => Ok(Self::McpAllow),
            "chat" => Ok(Self::Chat),
            "other" => Ok(Self::Other),
            _ => Err(VocabError::InvalidWord(text.to_owned())),
        }
    }

    /// Canonical group name string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::Backup => "backup",
            Self::Asked => "asked",
            Self::ModelOrder => "model.order",
            Self::ModelEndpointAllow => "model.endpoint.allow",
            Self::ModelEndpointBaseUrl => "model.endpoint.base_url",
            Self::ModelHandoffAllow => "model.handoff.allow",
            Self::McpAllow => "mcp.allow",
            Self::Chat => "chat",
            Self::Other => "other",
        }
    }
}

impl Ord for SettingsGroup {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for SettingsGroup {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for SettingsGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for SettingsGroup {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Typed settings baseline recorded in `settings.change` (`schema = 1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingsBaseline {
    /// Baseline schema version (`1`).
    pub schema: u32,
    /// Whether `settings.toml` was present on disk.
    pub present: bool,
    /// SHA-256 of the raw `settings.toml` bytes when `present` is `true`, otherwise `None`.
    pub file_sha256: Option<Sha256>,
    /// Optional canonical group digest for each of the 10 [`SettingsGroup`]s.
    pub groups: BTreeMap<SettingsGroup, Option<Sha256>>,
}

/// Errors raised when reading `settings.toml` for the journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    /// `settings.toml` is not valid UTF-8 or TOML.
    InvalidToml(String),
    /// The `role` value in `settings.toml` is not a valid `Role` string.
    InvalidRole(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToml(msg) => write!(f, "invalid settings.toml: {msg}"),
            Self::InvalidRole(msg) => write!(f, "invalid role in settings.toml: {msg}"),
        }
    }
}

impl std::error::Error for SettingsError {}

/// Parse the `role` key from `settings.toml` bytes (`Role::Agent` when absent) and compute
/// the [`SettingsBaseline`] in a single pass.
///
/// # Errors
///
/// Returns [`SettingsError`] if `bytes` is not valid UTF-8/TOML or if `role` is present
/// with an invalid value.
pub fn parse_settings_role_and_baseline(
    bytes: Option<&[u8]>,
) -> Result<(Role, SettingsBaseline), SettingsError> {
    let Some(raw) = bytes else {
        return Ok((Role::Agent, SettingsBaseline::absent()));
    };
    let text = std::str::from_utf8(raw).map_err(|e| SettingsError::InvalidToml(e.to_string()))?;
    let table: toml::Table =
        toml::from_str(text).map_err(|e| SettingsError::InvalidToml(e.to_string()))?;

    let role = match table.get("role") {
        None => Role::Agent,
        Some(val) => {
            let s = val
                .as_str()
                .ok_or_else(|| SettingsError::InvalidRole("role must be a string".to_owned()))?;
            Role::parse(s).map_err(|_| SettingsError::InvalidRole(s.to_owned()))?
        }
    };

    let baseline = SettingsBaseline::from_parsed(raw, table);
    Ok((role, baseline))
}

impl SettingsBaseline {
    /// Construct the baseline for an absent `settings.toml` file.
    #[must_use]
    pub fn absent() -> Self {
        let mut groups = BTreeMap::new();
        for group in SettingsGroup::ALL {
            groups.insert(group, None);
        }
        Self {
            schema: 1,
            present: false,
            file_sha256: None,
            groups,
        }
    }

    /// Compute the baseline from raw `settings.toml` bytes (`None` when the file is absent).
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::InvalidToml`] if `bytes` is `Some` and is not valid UTF-8 or TOML.
    pub fn from_bytes(bytes: Option<&[u8]>) -> Result<Self, SettingsError> {
        let Some(raw) = bytes else {
            return Ok(Self::absent());
        };
        let text =
            std::str::from_utf8(raw).map_err(|e| SettingsError::InvalidToml(e.to_string()))?;
        let table: toml::Table =
            toml::from_str(text).map_err(|e| SettingsError::InvalidToml(e.to_string()))?;
        Ok(Self::from_parsed(raw, table))
    }

    fn from_parsed(raw: &[u8], mut residual: toml::Table) -> Self {
        let mut groups = BTreeMap::new();
        for group in SettingsGroup::ALL {
            groups.insert(group, None);
        }

        if let Some(val) = residual.remove("role") {
            groups.insert(SettingsGroup::Role, Some(hash_settings_value(&val)));
        }
        if let Some(val) = residual.remove("backup") {
            groups.insert(SettingsGroup::Backup, Some(hash_settings_value(&val)));
        }
        if let Some(val) = residual.remove("asked") {
            groups.insert(SettingsGroup::Asked, Some(hash_settings_value(&val)));
        }
        if let Some(val) = residual.remove("chat") {
            groups.insert(SettingsGroup::Chat, Some(hash_settings_value(&val)));
        }

        if let Some(toml::Value::Table(mcp_tbl)) = residual.get_mut("mcp")
            && let Some(allow_val) = mcp_tbl.remove("allow")
        {
            groups.insert(
                SettingsGroup::McpAllow,
                Some(hash_settings_value(&allow_val)),
            );
            if mcp_tbl.is_empty() {
                residual.remove("mcp");
            }
        }

        if let Some(toml::Value::Table(model_tbl)) = residual.get_mut("model") {
            let mut extracted_from_model = false;
            if let Some(order_val) = model_tbl.remove("order") {
                groups.insert(
                    SettingsGroup::ModelOrder,
                    Some(hash_settings_value(&order_val)),
                );
                extracted_from_model = true;
            }
            if let Some(toml::Value::Table(handoff_tbl)) = model_tbl.get_mut("handoff")
                && let Some(allow_val) = handoff_tbl.remove("allow")
            {
                groups.insert(
                    SettingsGroup::ModelHandoffAllow,
                    Some(hash_settings_value(&allow_val)),
                );
                extracted_from_model = true;
                if handoff_tbl.is_empty() {
                    model_tbl.remove("handoff");
                }
            }
            if let Some(toml::Value::Table(endpoint_tbl)) = model_tbl.get_mut("endpoint") {
                let mut ep_allow_map = toml::Table::new();
                let mut ep_url_map = toml::Table::new();
                let mut empty_eps = Vec::new();
                for (ep_name, ep_val) in endpoint_tbl.iter_mut() {
                    if let toml::Value::Table(ep_inner) = ep_val {
                        let mut extracted_ep = false;
                        if let Some(allow_v) = ep_inner.remove("allow") {
                            ep_allow_map.insert(ep_name.clone(), allow_v);
                            extracted_ep = true;
                        }
                        if let Some(url_v) = ep_inner.remove("base_url") {
                            ep_url_map.insert(ep_name.clone(), url_v);
                            extracted_ep = true;
                        }
                        if extracted_ep && ep_inner.is_empty() {
                            empty_eps.push(ep_name.clone());
                        }
                    }
                }
                for ep_name in empty_eps {
                    endpoint_tbl.remove(&ep_name);
                }
                if !ep_allow_map.is_empty() {
                    groups.insert(
                        SettingsGroup::ModelEndpointAllow,
                        Some(hash_settings_value(&toml::Value::Table(ep_allow_map))),
                    );
                    extracted_from_model = true;
                }
                if !ep_url_map.is_empty() {
                    groups.insert(
                        SettingsGroup::ModelEndpointBaseUrl,
                        Some(hash_settings_value(&toml::Value::Table(ep_url_map))),
                    );
                    extracted_from_model = true;
                }
                if extracted_from_model && endpoint_tbl.is_empty() {
                    model_tbl.remove("endpoint");
                }
            }
            if extracted_from_model && model_tbl.is_empty() {
                residual.remove("model");
            }
        }

        if !residual.is_empty() {
            groups.insert(
                SettingsGroup::Other,
                Some(hash_settings_value(&toml::Value::Table(residual))),
            );
        }

        Self {
            schema: 1,
            present: true,
            file_sha256: Some(Sha256::digest(raw)),
            groups,
        }
    }

    /// Validate that this baseline satisfies the `schema = 1` contract.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError`] if `schema != 1`, `groups` does not contain all 10 groups,
    /// or `present`/`file_sha256`/`groups` are inconsistent.
    pub fn validate(&self) -> Result<(), VocabError> {
        if self.schema != 1 || self.groups.len() != SettingsGroup::ALL.len() {
            return Err(VocabError::InvalidWord(
                "invalid settings baseline schema or group map".to_owned(),
            ));
        }
        for group in SettingsGroup::ALL {
            if !self.groups.contains_key(&group) {
                return Err(VocabError::InvalidWord(format!(
                    "missing settings group `{group}`"
                )));
            }
        }
        if !self.present {
            if self.file_sha256.is_some() || self.groups.values().any(Option::is_some) {
                return Err(VocabError::InvalidWord(
                    "absent settings baseline cannot have hashes".to_owned(),
                ));
            }
        } else if self.file_sha256.is_none() {
            return Err(VocabError::InvalidWord(
                "present settings baseline requires file_sha256".to_owned(),
            ));
        }
        Ok(())
    }

    /// Compare `self` with `previous` (`None` if no `settings.change` has been recorded yet).
    ///
    /// Returns `None` when `previous == Some(self)` (unchanged reopen emits no event).
    /// Returns `Some(changed_groups)` when an initial or updated `settings.change` must be
    /// appended; formatting-only changes return `Some(vec![])`.
    #[must_use]
    pub fn diff(&self, previous: Option<&Self>) -> Option<Vec<SettingsGroup>> {
        let Some(prev) = previous else {
            let mut names = Vec::new();
            for group in SettingsGroup::ALL {
                if self.groups.get(&group).and_then(Option::as_ref).is_some() {
                    names.push(group);
                }
            }
            return Some(names);
        };
        if self == prev {
            return None;
        }
        let mut names = Vec::new();
        for group in SettingsGroup::ALL {
            if self.groups.get(&group) != prev.groups.get(&group) {
                names.push(group);
            }
        }
        Some(names)
    }
}

fn hash_settings_value(value: &toml::Value) -> Sha256 {
    let mut hasher = Sha256Hasher::new();
    hasher.update(SETTINGS_GROUP_DOMAIN);
    encode_canonical_toml_value(&mut hasher, value);
    let digest = hasher.finalize();
    let hex_str = crate::hex::lower(&digest);
    Sha256::parse(&hex_str).unwrap_or_else(|_| unreachable!())
}

fn encode_canonical_toml_value(hasher: &mut Sha256Hasher, value: &toml::Value) {
    match value {
        toml::Value::String(s) => {
            hasher.update([0x01]);
            hasher.update((s.len() as u64).to_be_bytes());
            hasher.update(s.as_bytes());
        }
        toml::Value::Integer(i) => {
            hasher.update([0x02]);
            let dec = i.to_string();
            hasher.update((dec.len() as u64).to_be_bytes());
            hasher.update(dec.as_bytes());
        }
        toml::Value::Float(f) => {
            hasher.update([0x03]);
            let bits = if f.is_nan() {
                0x7ff8_0000_0000_0000_u64
            } else if *f == 0.0 {
                0u64
            } else {
                f.to_bits()
            };
            hasher.update(bits.to_be_bytes());
        }
        toml::Value::Boolean(b) => {
            hasher.update([0x04, u8::from(*b)]);
        }
        toml::Value::Datetime(dt) => {
            hasher.update([0x05]);
            let canonical = dt.to_string();
            hasher.update((canonical.len() as u64).to_be_bytes());
            hasher.update(canonical.as_bytes());
        }
        toml::Value::Array(arr) => {
            hasher.update([0x06]);
            hasher.update((arr.len() as u64).to_be_bytes());
            for item in arr {
                encode_canonical_toml_value(hasher, item);
            }
        }
        toml::Value::Table(tbl) => {
            hasher.update([0x07]);
            let mut entries: Vec<(&String, &toml::Value)> = tbl.iter().collect();
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            hasher.update((entries.len() as u64).to_be_bytes());
            for (k, v) in entries {
                hasher.update((k.len() as u64).to_be_bytes());
                hasher.update(k.as_bytes());
                encode_canonical_toml_value(hasher, v);
            }
        }
    }
}
