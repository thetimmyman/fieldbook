//! Typed vocabulary of the audit journal (`audit-journal`, AD-16, ADR 0004 §12).
//!
//! Every event field in the journal belongs to one of the closed or shape-validated
//! types defined here. No event holds free text, a prompt, a question, file content,
//! or an outside path.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{
    AdrId, CaseId, ClaimId, ConflictId, EditionId, PageId, QuestionId, ReleaseId, SourceId, TermId,
    VersionId,
};
pub use crate::labels::{LabelAllow, LabelId, LabelSet, StoreId};

/// The 64-zero initial hash at the head of a fresh journal chain.
pub const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The nil UUID (`00000000-0000-0000-0000-000000000000`).
pub const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// Errors returned when parsing a typed audit vocabulary value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VocabError {
    /// Invalid identifier shape.
    InvalidId(String),
    /// Invalid store-qualified record reference (`<store>/<id>`).
    InvalidRecordRef(String),
    /// Invalid canonical relative corpus path.
    InvalidCorpusPath(String),
    /// Invalid SHA-256 hex digest.
    InvalidSha256(String),
    /// Invalid role word.
    InvalidRole(String),
    /// Value is not in the closed word list for the field.
    InvalidWord(String),
    /// Invalid verb shape (`[a-z]+( [a-z]+)*`, at most 32 bytes).
    InvalidVerb(String),
    /// Invalid stream or system key (`[a-z][a-z0-9-]{0,31}`).
    InvalidStreamKey(String),
    /// Invalid endpoint name (`[a-z0-9-]{1,32}`).
    InvalidEndpointName(String),
    /// Invalid model identifier.
    InvalidModelId(String),
    /// Invalid principal (`local` or `os:<DOMAIN>\<user>`).
    InvalidPrincipal(String),
    /// Invalid envelope `via` word (`cli`, `chat`, `mcp`).
    InvalidVia(String),
    /// Invalid `asked_via` word (`search`, `context`, `ask`, `chat`).
    InvalidAskedVia(String),
    /// Invalid canonical lowercase UUID (`8-4-4-4-12`).
    InvalidStoreUid(String),
    /// Invalid label or label set.
    InvalidLabels(String),
    /// Invalid destination kind.
    InvalidDest(String),
    /// Invalid tool version (`N.N.N`).
    InvalidVersion(String),
    /// Invalid UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
    InvalidTimestamp(String),
}

impl fmt::Display for VocabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(s) => write!(f, "not an identifier: {s}"),
            Self::InvalidRecordRef(s) => write!(f, "not a record reference: {s}"),
            Self::InvalidCorpusPath(s) => write!(f, "not a corpus path: {s}"),
            Self::InvalidSha256(s) => write!(f, "not a sha256 digest: {s}"),
            Self::InvalidRole(s) => write!(f, "not a role word: {s}"),
            Self::InvalidWord(s) => write!(f, "not a closed vocabulary word: {s}"),
            Self::InvalidVerb(s) => write!(f, "not a verb word: {s}"),
            Self::InvalidStreamKey(s) => write!(f, "not a stream key: {s}"),
            Self::InvalidEndpointName(s) => write!(f, "not an endpoint name: {s}"),
            Self::InvalidModelId(s) => write!(f, "not a model identifier: {s}"),
            Self::InvalidPrincipal(s) => write!(f, "not a principal: {s}"),
            Self::InvalidVia(s) => write!(f, "not a via word: {s}"),
            Self::InvalidAskedVia(s) => write!(f, "not an asked_via word: {s}"),
            Self::InvalidStoreUid(s) => write!(f, "not a store UUID: {s}"),
            Self::InvalidLabels(s) => write!(f, "not a valid label list: {s}"),
            Self::InvalidDest(s) => write!(f, "not a destination kind: {s}"),
            Self::InvalidVersion(s) => write!(f, "not a tool version: {s}"),
            Self::InvalidTimestamp(s) => write!(f, "not a UTC timestamp: {s}"),
        }
    }
}

impl std::error::Error for VocabError {}

/// An identifier of any registered public, state, or company family.
///
/// Canonical UUIDs (`StoreUid`) are never valid `Id` values.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// Parse and validate an identifier string.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidId`] if `text` does not match a known identifier shape
    /// or is a canonical UUID.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        if StoreUid::parse(text).is_ok() {
            return Err(VocabError::InvalidId(text.to_owned()));
        }
        if is_public_id(text) || is_state_id(text) || is_company_id(text) {
            Ok(Self(text.to_owned()))
        } else {
            Err(VocabError::InvalidId(text.to_owned()))
        }
    }

    /// Return the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_public_id(text: &str) -> bool {
    PageId::parse(text).is_ok()
        || SourceId::parse(text).is_ok()
        || VersionId::parse(text).is_ok()
        || ClaimId::parse(text).is_ok()
        || QuestionId::parse(text).is_ok()
        || ConflictId::parse(text).is_ok()
        || TermId::parse(text).is_ok()
        || CaseId::parse(text).is_ok()
        || AdrId::parse(text).is_ok()
        || EditionId::parse(text).is_ok()
        || ReleaseId::parse(text).is_ok()
}

fn is_state_id(text: &str) -> bool {
    let Some((prefix, digits)) = text.split_once('-') else {
        return false;
    };
    if !matches!(prefix, "A" | "P" | "H" | "D") {
        return false;
    }
    digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()) && digits != "0000"
}

fn is_company_number(digits: &str) -> bool {
    if !(6..=10).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let split_at = digits.len() - 6;
    let writer_part = &digits[..split_at];
    let seq_part = &digits[split_at..];
    if writer_part.starts_with('0') {
        return false;
    }
    seq_part != "000000"
}

fn is_company_id(text: &str) -> bool {
    if let Some(rest) = text.strip_prefix("x_") {
        return (1..=46).contains(&rest.len())
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    }
    if text.len() >= 5 && text.as_bytes()[0] == b'x' && text.as_bytes()[3] == b'-' {
        let area = &text[..3];
        let slug = &text[4..];
        if area.bytes().all(|b| b.is_ascii_lowercase())
            && (1..=60).contains(&slug.len())
            && !slug.starts_with('-')
            && !slug.ends_with('-')
            && !slug.contains("--")
            && slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return true;
        }
    }
    if let Some(rest) = text.strip_prefix("CL-X") {
        if rest.len() < 9 || rest.as_bytes()[2] != b'-' {
            return false;
        }
        let area_tail = &rest[..2];
        let num = &rest[3..];
        return area_tail.bytes().all(|b| b.is_ascii_uppercase()) && is_company_number(num);
    }
    for prefix in ["SO-", "QO-", "XO-", "CM-"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            if (prefix == "SO-" || prefix == "CM-")
                && let Some((num, ver)) = rest.split_once("-v")
            {
                let ver_ok = !ver.is_empty()
                    && !ver.starts_with('0')
                    && ver.len() <= 4
                    && ver.bytes().all(|b| b.is_ascii_digit());
                return is_company_number(num) && ver_ok;
            }
            return is_company_number(rest);
        }
    }
    false
}

/// A store-qualified record reference, written `<store>/<id>` (for example `overlay/SO-000042`).
///
/// Canonical UUIDs (`StoreUid`) are never encoded as `RecordRef`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecordRef {
    store: StoreId,
    id: Id,
}

impl RecordRef {
    /// Construct a `RecordRef` from a validated store and identifier.
    #[must_use]
    pub const fn new(store: StoreId, id: Id) -> Self {
        Self { store, id }
    }

    /// Parse `<store>/<id>`.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidRecordRef`] if `text` is not `<store>/<id>` with a valid
    /// store and identifier.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        let Some((store_raw, id_raw)) = text.split_once('/') else {
            return Err(VocabError::InvalidRecordRef(text.to_owned()));
        };
        let Some(store) = StoreId::from_root_name(store_raw) else {
            return Err(VocabError::InvalidRecordRef(text.to_owned()));
        };
        if StoreUid::parse(id_raw).is_ok() {
            return Err(VocabError::InvalidRecordRef(text.to_owned()));
        }
        let id = Id::parse(id_raw).map_err(|_| VocabError::InvalidRecordRef(text.to_owned()))?;
        Ok(Self { store, id })
    }

    /// Return the store component.
    #[must_use]
    pub const fn store(&self) -> StoreId {
        self.store
    }

    /// Return the identifier component.
    #[must_use]
    pub const fn id(&self) -> &Id {
        &self.id
    }
}

impl fmt::Display for RecordRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.store.as_str(), self.id.as_str())
    }
}

impl Serialize for RecordRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Validated canonical lowercase UUID (`8-4-4-4-12` hex digits) for store identity and recovery.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StoreUid(String);

impl StoreUid {
    /// Return the nil UUID (`00000000-0000-0000-0000-000000000000`).
    #[must_use]
    pub fn nil() -> Self {
        Self(NIL_UUID.to_owned())
    }

    /// Parse a canonical lowercase UUID (`8-4-4-4-12`).
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidStoreUid`] if `text` is not 36 bytes in `8-4-4-4-12`
    /// lowercase hexadecimal format.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        let bytes = text.as_bytes();
        if bytes.len() != 36 {
            return Err(VocabError::InvalidStoreUid(text.to_owned()));
        }
        for (idx, &b) in bytes.iter().enumerate() {
            if matches!(idx, 8 | 13 | 18 | 23) {
                if b != b'-' {
                    return Err(VocabError::InvalidStoreUid(text.to_owned()));
                }
            } else if !(b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                return Err(VocabError::InvalidStoreUid(text.to_owned()));
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the UUID string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StoreUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Canonical relative slash path inside a store or Corpus root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CorpusPath(String);

impl CorpusPath {
    /// Parse a canonical relative slash path.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidCorpusPath`] if `text` is empty, starts or ends with `/`,
    /// contains `\`, `.` or `..` segments, empty segments, or characters outside `[A-Za-z0-9._/-]`.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        if text.is_empty()
            || text.len() > 256
            || text.starts_with('/')
            || text.ends_with('/')
            || text.contains('\\')
        {
            return Err(VocabError::InvalidCorpusPath(text.to_owned()));
        }
        for segment in text.split('/') {
            if segment.is_empty() || segment == "." || segment == ".." {
                return Err(VocabError::InvalidCorpusPath(text.to_owned()));
            }
            if !segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            {
                return Err(VocabError::InvalidCorpusPath(text.to_owned()));
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the path string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CorpusPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validated 64-character lowercase hexadecimal SHA-256 digest (`[0-9a-f]{64}`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sha256(String);

impl Sha256 {
    /// Return the 64-zero initial hash.
    #[must_use]
    pub fn zero() -> Self {
        Self(ZERO_HASH.to_owned())
    }

    /// Compute the SHA-256 digest of `bytes` and return it as a validated [`Sha256`].
    #[must_use]
    pub fn digest(bytes: &[u8]) -> Self {
        Self(crate::hash::sha256_hex(bytes))
    }

    /// Parse a 64-character lowercase hexadecimal SHA-256 digest.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidSha256`] if `text` is not 64 lowercase hex characters.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        if text.len() != 64
            || !text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(VocabError::InvalidSha256(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the 64-character hex digest slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Semantic tool version (`N.N.N`, no `v` prefix).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SemVer(String);

impl SemVer {
    /// Parse a `N.N.N` semantic version string.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidVersion`] if `text` is not three dot-separated unsigned
    /// decimal numbers without leading zeros on multi-digit components.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        let mut parts = text.split('.');
        let (Some(major), Some(minor), Some(patch), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(VocabError::InvalidVersion(text.to_owned()));
        };
        for part in [major, minor, patch] {
            if part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || part.parse::<u32>().is_err()
            {
                return Err(VocabError::InvalidVersion(text.to_owned()));
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the version string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Outbound destination kind recorded in egress and refuse events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DestKind {
    /// Approved HTTP model endpoint (`"model"`).
    Model,
    /// Browser Handoff copy-paste flow (`"handoff"`).
    Handoff,
    /// Model Context Protocol tool client (`"mcp"`).
    Mcp,
    /// Local browser chat Bridge (`"bridge"`).
    Bridge,
    /// File or directory export (`"export"`).
    Export,
    /// Backup snapshot or raw target (`"backup"`).
    Backup,
    /// Print / PDF sheet (`"print"`).
    Print,
    /// System clipboard (`"clipboard"`).
    Clipboard,
    /// Audit export sink (`"audit"`).
    Audit,
    /// Git work-tree (`"git"`).
    Git,
    /// Public edition bundle (`"edition"`).
    Edition,
}

impl DestKind {
    /// All eleven destination kinds in canonical order.
    pub const ALL: [Self; 11] = [
        Self::Model,
        Self::Handoff,
        Self::Mcp,
        Self::Bridge,
        Self::Export,
        Self::Backup,
        Self::Print,
        Self::Clipboard,
        Self::Audit,
        Self::Git,
        Self::Edition,
    ];

    /// Parse a destination kind word.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "model" => Some(Self::Model),
            "handoff" => Some(Self::Handoff),
            "mcp" => Some(Self::Mcp),
            "bridge" => Some(Self::Bridge),
            "export" => Some(Self::Export),
            "backup" => Some(Self::Backup),
            "print" => Some(Self::Print),
            "clipboard" => Some(Self::Clipboard),
            "audit" => Some(Self::Audit),
            "git" => Some(Self::Git),
            "edition" => Some(Self::Edition),
            _ => None,
        }
    }

    /// Canonical lowercase destination kind word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Handoff => "handoff",
            Self::Mcp => "mcp",
            Self::Bridge => "bridge",
            Self::Export => "export",
            Self::Backup => "backup",
            Self::Print => "print",
            Self::Clipboard => "clipboard",
            Self::Audit => "audit",
            Self::Git => "git",
            Self::Edition => "edition",
        }
    }
}

impl fmt::Display for DestKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unsigned integer count in an audit event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Count(pub u64);

impl Count {
    /// Construct a `Count`.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the underlying integer value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for Count {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<u32> for Count {
    fn from(value: u32) -> Self {
        Self(u64::from(value))
    }
}

impl From<u16> for Count {
    fn from(value: u16) -> Self {
        Self(u64::from(value))
    }
}

impl From<usize> for Count {
    fn from(value: usize) -> Self {
        Self(value as u64)
    }
}

impl fmt::Display for Count {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Actor role declared in `settings.toml` (`owner`, `lead`, or `agent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The repository or machine owner.
    Owner,
    /// The lead curator.
    Lead,
    /// An automated agent (default when `role` is unset).
    Agent,
}

impl Role {
    /// Parse a role word.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidRole`] if `text` is not `owner`, `lead`, or `agent`.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        match text {
            "owner" => Ok(Self::Owner),
            "lead" => Ok(Self::Lead),
            "agent" => Ok(Self::Agent),
            _ => Err(VocabError::InvalidRole(text.to_owned())),
        }
    }

    /// Canonical lowercase role word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Lead => "lead",
            Self::Agent => "agent",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A value drawn from a field-specific closed word list.
///
/// Holds `&'static str` so no runtime free text can ever be wrapped in a `Word`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Word(&'static str);

impl Word {
    /// Construct a `Word` from a static literal in a closed list.
    #[must_use]
    pub const fn new_static(word: &'static str) -> Self {
        Self(word)
    }

    /// Validate `text` against `allowed` and return the matching static word.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidWord`] if `text` is not in `allowed`.
    pub fn from_closed(text: &str, allowed: &'static [&'static str]) -> Result<Self, VocabError> {
        for &candidate in allowed {
            if candidate == text {
                return Ok(Self(candidate));
            }
        }
        Err(VocabError::InvalidWord(text.to_owned()))
    }

    /// Return the static word slice.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Serialize for Word {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

/// Closed word lists per event field (`audit-contract.md#kinds-and-fields`).
pub const RECOVER_REASONS: &[&str] = &["torn_tail", "broken_chain"];
/// Closed outcome words for `write.end`, `backup.end`, `backup.verify`, and `restore.end`.
pub const OK_FAILED_OUTCOMES: &[&str] = &["ok", "failed"];
/// Closed reason words for `refuse`.
pub const REFUSE_REASONS: &[&str] = &[
    "label",
    "unlabelled",
    "marked",
    "no-row",
    "expired",
    "url",
    "client",
    "closed",
    "mismatch",
    "store",
    "git",
    "authz",
    "shipped",
    "root",
    "place",
    "adopt",
];
/// Closed purpose words for `ai.begin`.
pub const AI_PURPOSES: &[&str] = &[
    "ask",
    "classify",
    "correlate",
    "draft",
    "decide",
    "embed",
    "suggest",
];
/// Closed adapter identifiers for `ai.begin`.
pub const AI_ADAPTERS: &[&str] = &["openai-compatible@1", "handoff@1", "scripted@1"];
/// Closed template identifiers for `ai.begin`.
pub const AI_TEMPLATES: &[&str] = &[
    "ask@1",
    "classify@1",
    "correlate@1",
    "draft@1",
    "decide@1",
    "suggest@1",
];
/// Closed outcome words for `ai.call`.
pub const AI_CALL_OUTCOMES: &[&str] = &["ok", "failed", "invalid", "truncated", "cancelled"];
/// Closed outcome words for `ask`.
pub const ASK_OUTCOMES: &[&str] = &["answered", "no_result", "cite_failed", "flagged"];
/// Closed protocol revisions for `mcp.session`.
pub const MCP_PROTOCOLS: &[&str] = &["2025-11-25"];
/// Closed tool names for `mcp.call`.
pub const MCP_TOOLS: &[&str] = &[
    "search",
    "get_unit",
    "context",
    "cite_check",
    "open_questions",
];
/// Closed outcome words for `mcp.call`.
pub const MCP_OUTCOMES: &[&str] = &["ok", "invalid", "failed"];
/// Closed phase words for `chat.session`.
pub const CHAT_SESSION_PHASES: &[&str] = &["start", "stop", "idle"];
/// Closed phase words for `chat.pack`.
pub const CHAT_PACK_PHASES: &[&str] = &["save", "delete"];
/// Closed target kinds for `backup.end`.
pub const BACKUP_TARGET_KINDS: &[&str] = &["folder", "it-location", "synced"];
/// Closed verification discriminators for `backup.verify`.
pub const BACKUP_VERIFICATIONS: &[&str] = &["snapshot", "raw"];
/// Closed prune reasons for `backup.prune`.
pub const BACKUP_PRUNE_REASONS: &[&str] = &["retention"];
/// Closed restore modes for `restore.begin` and `restore.end`.
pub const RESTORE_MODES: &[&str] = &["drill", "real"];
/// Closed purge targets for `purge`.
pub const PURGE_TARGETS: &[&str] = &["asked"];
/// Closed policy resolution outcomes for `policy.change`.
pub const POLICY_OUTCOMES: &[&str] = &["ok", "failed", "loaded", "absent", "invalid"];
/// Closed label-change origins for `label.change`.
pub const LABEL_ORIGINS: &[&str] = &["person", "reapply"];
/// Closed adoption modes for `store.adopted`.
pub const ADOPT_MODES: &[&str] = &["retired", "writer"];
/// Closed pull adapter identifiers for `sync.begin`.
pub const SYNC_ADAPTERS: &[&str] = &["file-drop@1"];
/// Closed sync scopes for `sync.end`.
pub const SYNC_SCOPES: &[&str] = &["full", "incremental"];

/// Command verb words (`[a-z]+( [a-z]+)*`, at most 32 bytes).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Verb(String);

impl Verb {
    /// Parse a command verb string.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidVerb`] if `text` is empty, longer than 32 bytes, or
    /// does not match `[a-z]+( [a-z]+)*`.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        if text.is_empty() || text.len() > 32 {
            return Err(VocabError::InvalidVerb(text.to_owned()));
        }
        for part in text.split(' ') {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_lowercase()) {
                return Err(VocabError::InvalidVerb(text.to_owned()));
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the verb string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Sync system or stream identifier (`[a-z][a-z0-9-]{0,31}`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StreamKey(String);

impl StreamKey {
    /// Parse a stream or system key.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidStreamKey`] if `text` does not match `[a-z][a-z0-9-]{0,31}`.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() > 32 || !bytes[0].is_ascii_lowercase() {
            return Err(VocabError::InvalidStreamKey(text.to_owned()));
        }
        if !bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(VocabError::InvalidStreamKey(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the key string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StreamKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Configured model endpoint name (`[a-z0-9-]{1,32}`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EndpointName(String);

impl EndpointName {
    /// Parse an endpoint name.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidEndpointName`] if `text` does not match `[a-z0-9-]{1,32}`.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        let bytes = text.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 32
            || !bytes
                .iter()
                .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(VocabError::InvalidEndpointName(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the endpoint name string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EndpointName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Model identifier token (`[A-Za-z0-9._:@/-]{1,64}`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(String);

impl ModelId {
    /// Parse a model identifier.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidModelId`] if `text` is empty, longer than 64 bytes, or
    /// contains whitespace or disallowed characters.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        let bytes = text.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 64
            || !bytes.iter().all(|&b| {
                b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'@' | b'/' | b'-')
            })
        {
            return Err(VocabError::InvalidModelId(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the model identifier string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Operating-system principal (`os:<DOMAIN>\<user>`) or `local` for in-memory test worlds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Principal(String);

impl Principal {
    /// Construct the `local` test principal.
    #[must_use]
    pub fn local() -> Self {
        Self("local".to_owned())
    }

    /// Construct an operating-system principal `os:<domain>\<user>`.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidPrincipal`] if `domain` or `user` is empty or contains
    /// backslashes or control characters.
    pub fn os(domain: &str, user: &str) -> Result<Self, VocabError> {
        Self::parse(&format!("os:{domain}\\{user}"))
    }

    /// Parse a principal string (`local` or `os:<DOMAIN>\<user>`).
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidPrincipal`] if `text` is neither `local` nor a valid
    /// `os:<DOMAIN>\<user>` string.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        if text == "local" {
            return Ok(Self::local());
        }
        let Some(rest) = text.strip_prefix("os:") else {
            return Err(VocabError::InvalidPrincipal(text.to_owned()));
        };
        let Some((domain, user)) = rest.split_once('\\') else {
            return Err(VocabError::InvalidPrincipal(text.to_owned()));
        };
        if domain.is_empty()
            || user.is_empty()
            || text.len() > 128
            || domain.contains('\\')
            || user.contains('\\')
            || domain.chars().any(char::is_control)
            || user.chars().any(char::is_control)
        {
            return Err(VocabError::InvalidPrincipal(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    /// Return the principal string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Principal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Envelope entry door (`cli`, `chat`, or `mcp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    /// Command-line invocation.
    Cli,
    /// Local browser chat bridge.
    Chat,
    /// Model Context Protocol server.
    Mcp,
}

impl Via {
    /// Parse an envelope `via` word.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidVia`] if `text` is not `cli`, `chat`, or `mcp`.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        match text {
            "cli" => Ok(Self::Cli),
            "chat" => Ok(Self::Chat),
            "mcp" => Ok(Self::Mcp),
            _ => Err(VocabError::InvalidVia(text.to_owned())),
        }
    }

    /// Canonical lowercase `via` word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Chat => "chat",
            Self::Mcp => "mcp",
        }
    }
}

impl fmt::Display for Via {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Question origin recorded in `ask` event payloads (`search`, `context`, `ask`, or `chat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AskedVia {
    /// `fieldbook search`.
    Search,
    /// `fieldbook context`.
    Context,
    /// `fieldbook ask`.
    Ask,
    /// Browser chat session.
    Chat,
}

impl AskedVia {
    /// Parse an `asked_via` word.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidAskedVia`] if `text` is not `search`, `context`, `ask`, or `chat`.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        match text {
            "search" => Ok(Self::Search),
            "context" => Ok(Self::Context),
            "ask" => Ok(Self::Ask),
            "chat" => Ok(Self::Chat),
            _ => Err(VocabError::InvalidAskedVia(text.to_owned())),
        }
    }

    /// Canonical lowercase `asked_via` word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Context => "context",
            Self::Ask => "ask",
            Self::Chat => "chat",
        }
    }
}

impl fmt::Display for AskedVia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The actor recorded in the envelope of every journal event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// Operating-system principal (`os:<DOMAIN>\<user>` or `local`).
    pub principal: Principal,
    /// Declared role from `settings.toml` (`agent` when unset).
    pub role: Role,
    /// Door by which the run entered (`cli`, `chat`, or `mcp`).
    pub via: Via,
}

impl Actor {
    /// Construct an `Actor`.
    #[must_use]
    pub const fn new(principal: Principal, role: Role, via: Via) -> Self {
        Self {
            principal,
            role,
            via,
        }
    }
}
