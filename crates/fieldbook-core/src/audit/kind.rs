//! The 32 audit event kinds, their typed payloads, envelope rules, and JSON parser/writer
//! (`audit-journal`, `docs/reference/audit-contract.md`).

use std::collections::BTreeMap;
use std::fmt;

use crate::check::Code;

use super::chain::{RawEventObject, RawJsonValue, parse_json_object, push_json_str, seal_body};
pub use super::vocab::{
    ADOPT_MODES, AI_ADAPTERS, AI_CALL_OUTCOMES, AI_PURPOSES, AI_TEMPLATES, ASK_OUTCOMES, Actor,
    AskedVia, BACKUP_PRUNE_REASONS, BACKUP_TARGET_KINDS, BACKUP_VERIFICATIONS, CHAT_PACK_PHASES,
    CHAT_SESSION_PHASES, Count, DestKind, EndpointName, Id, LABEL_ORIGINS, LabelAllow, LabelId,
    LabelSet, MCP_OUTCOMES, MCP_PROTOCOLS, MCP_TOOLS, ModelId, OK_FAILED_OUTCOMES, POLICY_OUTCOMES,
    PURGE_TARGETS, Principal, RECOVER_REASONS, REFUSE_REASONS, RESTORE_MODES, RecordRef, Role,
    SYNC_ADAPTERS, SYNC_SCOPES, SemVer, Sha256, StoreId, StoreUid, StreamKey, Verb, Via,
    VocabError, Word,
};
use super::{FileHash, Now, SettingsBaseline, SettingsGroup};

/// One field specification for documentation generation (`audit.md`) and contract tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    /// Field key in the JSON line.
    pub name: &'static str,
    /// Typed vocabulary name.
    pub ty: &'static str,
}

/// The 32 audit event kinds (`audit-contract.md#kinds-and-fields`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// `journal.open`
    JournalOpen,
    /// `journal.recover`
    JournalRecover,
    /// `write.begin`
    WriteBegin,
    /// `write.end`
    WriteEnd,
    /// `refuse`
    Refuse,
    /// `queue.propose`
    QueuePropose,
    /// `queue.accept`
    QueueAccept,
    /// `queue.reject`
    QueueReject,
    /// `ai.begin`
    AiBegin,
    /// `ai.call`
    AiCall,
    /// `ai.pack`
    AiPack,
    /// `ai.paste`
    AiPaste,
    /// `ask`
    Ask,
    /// `mcp.session`
    McpSession,
    /// `mcp.call`
    McpCall,
    /// `chat.session`
    ChatSession,
    /// `chat.pack`
    ChatPack,
    /// `backup.begin`
    BackupBegin,
    /// `backup.end`
    BackupEnd,
    /// `backup.verify`
    BackupVerify,
    /// `backup.prune`
    BackupPrune,
    /// `restore.begin`
    RestoreBegin,
    /// `restore.end`
    RestoreEnd,
    /// `settings.change`
    SettingsChange,
    /// `purge`
    Purge,
    /// `policy.change`
    PolicyChange,
    /// `label.change`
    LabelChange,
    /// `audit.export`
    AuditExport,
    /// `store.missing`
    StoreMissing,
    /// `store.adopted`
    StoreAdopted,
    /// `sync.begin`
    SyncBegin,
    /// `sync.end`
    SyncEnd,
}

impl Kind {
    /// All 32 event kinds in contract order.
    pub const ALL: [Self; 32] = [
        Self::JournalOpen,
        Self::JournalRecover,
        Self::WriteBegin,
        Self::WriteEnd,
        Self::Refuse,
        Self::QueuePropose,
        Self::QueueAccept,
        Self::QueueReject,
        Self::AiBegin,
        Self::AiCall,
        Self::AiPack,
        Self::AiPaste,
        Self::Ask,
        Self::McpSession,
        Self::McpCall,
        Self::ChatSession,
        Self::ChatPack,
        Self::BackupBegin,
        Self::BackupEnd,
        Self::BackupVerify,
        Self::BackupPrune,
        Self::RestoreBegin,
        Self::RestoreEnd,
        Self::SettingsChange,
        Self::Purge,
        Self::PolicyChange,
        Self::LabelChange,
        Self::AuditExport,
        Self::StoreMissing,
        Self::StoreAdopted,
        Self::SyncBegin,
        Self::SyncEnd,
    ];

    /// The four designated egress kinds that always carry `dest`, `labels`, and `policy`.
    pub const EGRESS: [Self; 4] = [
        Self::AiBegin,
        Self::AiPack,
        Self::McpCall,
        Self::BackupBegin,
    ];

    /// Parse an event kind string.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError::InvalidWord`] if `text` is not one of the 32 event kinds.
    pub fn parse(text: &str) -> Result<Self, VocabError> {
        for kind in Self::ALL {
            if kind.as_str() == text {
                return Ok(kind);
            }
        }
        Err(VocabError::InvalidWord(IdOrWord::format_err(text)))
    }

    /// Canonical wire name of the kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JournalOpen => "journal.open",
            Self::JournalRecover => "journal.recover",
            Self::WriteBegin => "write.begin",
            Self::WriteEnd => "write.end",
            Self::Refuse => "refuse",
            Self::QueuePropose => "queue.propose",
            Self::QueueAccept => "queue.accept",
            Self::QueueReject => "queue.reject",
            Self::AiBegin => "ai.begin",
            Self::AiCall => "ai.call",
            Self::AiPack => "ai.pack",
            Self::AiPaste => "ai.paste",
            Self::Ask => "ask",
            Self::McpSession => "mcp.session",
            Self::McpCall => "mcp.call",
            Self::ChatSession => "chat.session",
            Self::ChatPack => "chat.pack",
            Self::BackupBegin => "backup.begin",
            Self::BackupEnd => "backup.end",
            Self::BackupVerify => "backup.verify",
            Self::BackupPrune => "backup.prune",
            Self::RestoreBegin => "restore.begin",
            Self::RestoreEnd => "restore.end",
            Self::SettingsChange => "settings.change",
            Self::Purge => "purge",
            Self::PolicyChange => "policy.change",
            Self::LabelChange => "label.change",
            Self::AuditExport => "audit.export",
            Self::StoreMissing => "store.missing",
            Self::StoreAdopted => "store.adopted",
            Self::SyncBegin => "sync.begin",
            Self::SyncEnd => "sync.end",
        }
    }

    /// Whether this kind is in [`Self::EGRESS`].
    #[must_use]
    pub fn is_egress(self) -> bool {
        Self::EGRESS.contains(&self)
    }

    /// Verb or component that emits this kind (used by `xtask/src/audit_doc.rs`).
    #[must_use]
    pub const fn emitted_by(self) -> &'static str {
        match self {
            Self::JournalOpen | Self::SettingsChange | Self::PolicyChange | Self::StoreMissing => {
                "Journal::open"
            }
            Self::JournalRecover => "Journal::open, audit verify --repair",
            Self::WriteBegin | Self::WriteEnd => {
                "add, new, fmt, verify, build, queue accept, decide, migrate, clean, audit export"
            }
            Self::Refuse => "any gated verb",
            Self::QueuePropose => "add --propose, sync",
            Self::QueueAccept => "queue accept",
            Self::QueueReject => "queue reject",
            Self::AiBegin | Self::AiCall => "ask, decide, ai test, chat, queue ingest",
            Self::AiPack => "ask, context, decide, queue ingest",
            Self::AiPaste => {
                "ask --resume, new note --ai-drafted, decide --resume, queue ingest --from"
            }
            Self::Ask => "search, context, ask, chat",
            Self::McpSession | Self::McpCall => "mcp",
            Self::ChatSession | Self::ChatPack => "chat",
            Self::BackupBegin | Self::BackupEnd | Self::BackupPrune => "backup",
            Self::BackupVerify => "backup, backup verify",
            Self::RestoreBegin | Self::RestoreEnd => "restore",
            Self::Purge => "search, ask, decide, chat",
            Self::LabelChange => "label add, label reapply",
            Self::AuditExport => "audit export",
            Self::StoreAdopted => "init --overlay --adopt",
            Self::SyncBegin | Self::SyncEnd => "sync",
        }
    }

    /// Typed payload field specifications for this kind (excluding the common envelope).
    #[must_use]
    pub const fn fields(self) -> &'static [FieldSpec] {
        match self {
            Self::JournalOpen => &[
                FieldSpec {
                    name: "writer",
                    ty: "Count",
                },
                FieldSpec {
                    name: "event_schema",
                    ty: "Count",
                },
                FieldSpec {
                    name: "tool_version",
                    ty: "SemVer",
                },
                FieldSpec {
                    name: "stores",
                    ty: "Vec<StoreUid>",
                },
            ],
            Self::JournalRecover => &[
                FieldSpec {
                    name: "reason",
                    ty: "Word",
                },
                FieldSpec {
                    name: "last_seq",
                    ty: "Count",
                },
                FieldSpec {
                    name: "last_hash",
                    ty: "Sha256",
                },
            ],
            Self::WriteBegin => &[
                FieldSpec {
                    name: "verb",
                    ty: "Verb",
                },
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
                FieldSpec {
                    name: "files",
                    ty: "Count",
                },
                FieldSpec {
                    name: "file_hashes",
                    ty: "Option<Vec<FileHash>>",
                },
                FieldSpec {
                    name: "affected",
                    ty: "Vec<RecordRef>",
                },
                FieldSpec {
                    name: "dest",
                    ty: "Option<DestKind>",
                },
                FieldSpec {
                    name: "path_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Option<Sha256>",
                },
            ],
            Self::WriteEnd => &[
                FieldSpec {
                    name: "verb",
                    ty: "Verb",
                },
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
                FieldSpec {
                    name: "affected",
                    ty: "Vec<RecordRef>",
                },
                FieldSpec {
                    name: "outcome",
                    ty: "Word",
                },
                FieldSpec {
                    name: "dest",
                    ty: "Option<DestKind>",
                },
                FieldSpec {
                    name: "path_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Option<Sha256>",
                },
            ],
            Self::Refuse => &[
                FieldSpec {
                    name: "code",
                    ty: "Code",
                },
                FieldSpec {
                    name: "reason",
                    ty: "Word",
                },
                FieldSpec {
                    name: "sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "name_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "labels",
                    ty: "Option<LabelSet>",
                },
                FieldSpec {
                    name: "dest",
                    ty: "Option<DestKind>",
                },
                FieldSpec {
                    name: "verb",
                    ty: "Option<Verb>",
                },
                FieldSpec {
                    name: "system",
                    ty: "Option<StreamKey>",
                },
                FieldSpec {
                    name: "stream",
                    ty: "Option<StreamKey>",
                },
            ],
            Self::QueuePropose | Self::QueueAccept | Self::QueueReject => &[FieldSpec {
                name: "labels",
                ty: "LabelSet",
            }],
            Self::AiBegin => &[
                FieldSpec {
                    name: "tx",
                    ty: "Count",
                },
                FieldSpec {
                    name: "purpose",
                    ty: "Word",
                },
                FieldSpec {
                    name: "dest",
                    ty: "DestKind",
                },
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "adapter",
                    ty: "Word",
                },
                FieldSpec {
                    name: "endpoint",
                    ty: "EndpointName",
                },
                FieldSpec {
                    name: "model",
                    ty: "ModelId",
                },
                FieldSpec {
                    name: "template",
                    ty: "Word",
                },
                FieldSpec {
                    name: "prompt_sha256",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "context_ids",
                    ty: "Vec<RecordRef>",
                },
            ],
            Self::AiCall => &[
                FieldSpec {
                    name: "tx",
                    ty: "Count",
                },
                FieldSpec {
                    name: "outcome",
                    ty: "Word",
                },
                FieldSpec {
                    name: "code",
                    ty: "Option<Code>",
                },
                FieldSpec {
                    name: "attempts",
                    ty: "Count",
                },
                FieldSpec {
                    name: "output_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "tokens_in",
                    ty: "Count",
                },
                FieldSpec {
                    name: "tokens_out",
                    ty: "Count",
                },
            ],
            Self::AiPack => &[
                FieldSpec {
                    name: "prompt_sha256",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "context_ids",
                    ty: "Vec<RecordRef>",
                },
                FieldSpec {
                    name: "dest",
                    ty: "DestKind",
                },
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Sha256",
                },
            ],
            Self::AiPaste => &[
                FieldSpec {
                    name: "pack_seq",
                    ty: "Option<Count>",
                },
                FieldSpec {
                    name: "output_sha256",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
            ],
            Self::Ask => &[
                FieldSpec {
                    name: "asked",
                    ty: "Option<Id>",
                },
                FieldSpec {
                    name: "asked_via",
                    ty: "AskedVia",
                },
                FieldSpec {
                    name: "question_sha256",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "result_count",
                    ty: "Count",
                },
                FieldSpec {
                    name: "outcome",
                    ty: "Word",
                },
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
            ],
            Self::McpSession => &[
                FieldSpec {
                    name: "protocol",
                    ty: "Word",
                },
                FieldSpec {
                    name: "client_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "allow",
                    ty: "LabelAllow",
                },
            ],
            Self::McpCall => &[
                FieldSpec {
                    name: "tool",
                    ty: "Word",
                },
                FieldSpec {
                    name: "args_sha256",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "result_ids",
                    ty: "Vec<RecordRef>",
                },
                FieldSpec {
                    name: "dest",
                    ty: "DestKind",
                },
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "outcome",
                    ty: "Word",
                },
            ],
            Self::ChatSession => &[
                FieldSpec {
                    name: "phase",
                    ty: "Word",
                },
                FieldSpec {
                    name: "calls",
                    ty: "Count",
                },
            ],
            Self::ChatPack => &[
                FieldSpec {
                    name: "phase",
                    ty: "Word",
                },
                FieldSpec {
                    name: "name_sha256",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "count",
                    ty: "Count",
                },
            ],
            Self::BackupBegin => &[
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
                FieldSpec {
                    name: "dest",
                    ty: "DestKind",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "snapshot",
                    ty: "Count",
                },
            ],
            Self::BackupEnd => &[
                FieldSpec {
                    name: "outcome",
                    ty: "Word",
                },
                FieldSpec {
                    name: "files",
                    ty: "Count",
                },
                FieldSpec {
                    name: "bytes",
                    ty: "Count",
                },
                FieldSpec {
                    name: "manifest_sha256",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "journal_seq",
                    ty: "Count",
                },
                FieldSpec {
                    name: "journal_head",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "fingerprint",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "target_kind",
                    ty: "Word",
                },
            ],
            Self::BackupVerify => &[
                FieldSpec {
                    name: "verification",
                    ty: "Word",
                },
                FieldSpec {
                    name: "outcome",
                    ty: "Word",
                },
                FieldSpec {
                    name: "target_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "snapshot",
                    ty: "Option<Count>",
                },
                FieldSpec {
                    name: "manifest_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "path_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "public_support_sha256",
                    ty: "Option<Sha256>",
                },
            ],
            Self::BackupPrune => &[
                FieldSpec {
                    name: "snapshots",
                    ty: "Vec<Count>",
                },
                FieldSpec {
                    name: "reason",
                    ty: "Word",
                },
            ],
            Self::RestoreBegin => &[
                FieldSpec {
                    name: "snapshot",
                    ty: "Count",
                },
                FieldSpec {
                    name: "mode",
                    ty: "Word",
                },
                FieldSpec {
                    name: "labels",
                    ty: "LabelSet",
                },
                FieldSpec {
                    name: "dest",
                    ty: "Option<DestKind>",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Option<Sha256>",
                },
            ],
            Self::RestoreEnd => &[
                FieldSpec {
                    name: "snapshot",
                    ty: "Count",
                },
                FieldSpec {
                    name: "mode",
                    ty: "Word",
                },
                FieldSpec {
                    name: "outcome",
                    ty: "Word",
                },
                FieldSpec {
                    name: "code",
                    ty: "Option<Code>",
                },
            ],
            Self::SettingsChange => &[
                FieldSpec {
                    name: "names",
                    ty: "Vec<SettingsGroup>",
                },
                FieldSpec {
                    name: "baseline",
                    ty: "SettingsBaseline",
                },
            ],
            Self::Purge => &[
                FieldSpec {
                    name: "what",
                    ty: "Word",
                },
                FieldSpec {
                    name: "count",
                    ty: "Count",
                },
            ],
            Self::PolicyChange => &[
                FieldSpec {
                    name: "policy",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "prev",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "revision",
                    ty: "Count",
                },
                FieldSpec {
                    name: "outcome",
                    ty: "Word",
                },
                FieldSpec {
                    name: "labels_n",
                    ty: "Count",
                },
                FieldSpec {
                    name: "rows_n",
                    ty: "Count",
                },
            ],
            Self::LabelChange => &[
                FieldSpec {
                    name: "ids",
                    ty: "Vec<RecordRef>",
                },
                FieldSpec {
                    name: "added",
                    ty: "LabelAllow",
                },
                FieldSpec {
                    name: "removed",
                    ty: "LabelAllow",
                },
                FieldSpec {
                    name: "origin",
                    ty: "Word",
                },
            ],
            Self::AuditExport => &[
                FieldSpec {
                    name: "from_seq",
                    ty: "Count",
                },
                FieldSpec {
                    name: "to_seq",
                    ty: "Count",
                },
                FieldSpec {
                    name: "head",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Sha256",
                },
                FieldSpec {
                    name: "store_uid",
                    ty: "StoreUid",
                },
            ],
            Self::StoreMissing => &[FieldSpec {
                name: "store_uid",
                ty: "StoreUid",
            }],
            Self::StoreAdopted => &[
                FieldSpec {
                    name: "store_uid",
                    ty: "StoreUid",
                },
                FieldSpec {
                    name: "mode",
                    ty: "Word",
                },
                FieldSpec {
                    name: "writer",
                    ty: "Count",
                },
            ],
            Self::SyncBegin => &[
                FieldSpec {
                    name: "system",
                    ty: "StreamKey",
                },
                FieldSpec {
                    name: "stream",
                    ty: "StreamKey",
                },
                FieldSpec {
                    name: "adapter",
                    ty: "Word",
                },
                FieldSpec {
                    name: "root_sha256",
                    ty: "Option<Sha256>",
                },
                FieldSpec {
                    name: "policy",
                    ty: "Sha256",
                },
            ],
            Self::SyncEnd => &[
                FieldSpec {
                    name: "system",
                    ty: "StreamKey",
                },
                FieldSpec {
                    name: "stream",
                    ty: "StreamKey",
                },
                FieldSpec {
                    name: "scope",
                    ty: "Word",
                },
                FieldSpec {
                    name: "seen",
                    ty: "Count",
                },
                FieldSpec {
                    name: "proposed",
                    ty: "Count",
                },
                FieldSpec {
                    name: "present",
                    ty: "Count",
                },
                FieldSpec {
                    name: "skipped",
                    ty: "Count",
                },
                FieldSpec {
                    name: "refused",
                    ty: "Count",
                },
            ],
        }
    }
}

struct IdOrWord;
impl IdOrWord {
    fn format_err(text: &str) -> std::string::String {
        text.to_owned()
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Optional subject context passed to `Journal::refuse`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefuseAbout {
    /// Content SHA-256 of the refused file when known.
    pub sha256: Option<Sha256>,
    /// SHA-256 of the candidate's file name when known.
    pub name_sha256: Option<Sha256>,
    /// Labels of the refused item or request when known.
    pub labels: Option<LabelSet>,
    /// Destination at which the refusal occurred when applicable.
    pub dest: Option<DestKind>,
    /// Sync system key when a stream is the subject.
    pub system: Option<StreamKey>,
    /// Sync stream key when a stream is the subject.
    pub stream: Option<StreamKey>,
}

/// Variant-specific fields for `backup.verify` (`snapshot` or `raw`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupVerified {
    /// Numbered snapshot verification (`verification = "snapshot"`).
    Snapshot {
        /// SHA-256 of the canonical target path.
        target_sha256: Sha256,
        /// Snapshot number (`>= 1`).
        snapshot: Count,
        /// SHA-256 of the snapshot's `SHA256SUMS` manifest.
        manifest_sha256: Sha256,
    },
    /// Raw copy verification (`verification = "raw"`).
    Raw {
        /// SHA-256 of the raw copy path.
        path_sha256: Sha256,
        /// Optional verified public support catalogue digest (permitted only when `outcome == "ok"`).
        public_support_sha256: Option<Sha256>,
    },
}

/// The common envelope present on every journal event (`actor`, `role`, `store`, `via`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEnvelope {
    /// Operating-system principal (`os:<DOMAIN>\<user>` or `local`).
    pub actor: Principal,
    /// Declared role (`owner`, `lead`, or `agent`).
    pub role: Role,
    /// Store the event acts on (`corpus`, `overlay`, `working`, or `state`).
    pub store: StoreId,
    /// Entry door (`cli`, `chat`, or `mcp`).
    pub via: Via,
}

impl EventEnvelope {
    /// Build an `EventEnvelope` from an [`Actor`] and [`StoreId`].
    #[must_use]
    pub fn from_actor(actor: &Actor, store: StoreId) -> Self {
        Self {
            actor: actor.principal.clone(),
            role: actor.role,
            store,
            via: actor.via,
        }
    }
}

/// Typed payload of a journal event, one variant per [`Kind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventPayload {
    /// `journal.open`
    JournalOpen {
        /// Store writer index (`0` with no company store).
        writer: Count,
        /// Event schema version (`1`).
        event_schema: Count,
        /// Tool version (`N.N.N`).
        tool_version: SemVer,
        /// UUIDs of every company store opened in this run.
        stores: Vec<StoreUid>,
    },
    /// `journal.recover`
    JournalRecover {
        /// Recovery reason (`torn_tail` or `broken_chain`).
        reason: Word,
        /// Sequence number of the last valid line before recovery (`0` when none).
        last_seq: Count,
        /// Hash of the last valid line before recovery (64 zeros when none).
        last_hash: Sha256,
    },
    /// `write.begin`
    WriteBegin {
        /// Command verb.
        verb: Verb,
        /// Union of labels of the write set.
        labels: LabelSet,
        /// Number of files in the write set.
        files: Count,
        /// Sorted path/after hashes for in-root writes (`None` for exports).
        file_hashes: Option<Vec<FileHash>>,
        /// Sorted, distinct record references affected by the write.
        affected: Vec<RecordRef>,
        /// Export destination (`Some` only for exports).
        dest: Option<DestKind>,
        /// Export target path hash (`Some` only for exports).
        path_sha256: Option<Sha256>,
        /// Policy digest in force (`Some` only for exports).
        policy: Option<Sha256>,
    },
    /// `write.end`
    WriteEnd {
        /// Command verb.
        verb: Verb,
        /// Union of labels of the write set.
        labels: LabelSet,
        /// Sorted, distinct record references affected by the write.
        affected: Vec<RecordRef>,
        /// Outcome (`ok` or `failed`).
        outcome: Word,
        /// Export destination (`Some` only for exports).
        dest: Option<DestKind>,
        /// Export target path hash (`Some` only for exports).
        path_sha256: Option<Sha256>,
        /// Policy digest in force (`Some` only for exports).
        policy: Option<Sha256>,
    },
    /// `refuse`
    Refuse {
        /// Diagnostic code raised by the refusal.
        code: Code,
        /// Closed refusal reason word.
        reason: Word,
        /// Content SHA-256 when a file is the subject.
        sha256: Option<Sha256>,
        /// File-name SHA-256 when a file is the subject.
        name_sha256: Option<Sha256>,
        /// Labels when known.
        labels: Option<LabelSet>,
        /// Destination when refused at an egress or export gate.
        dest: Option<DestKind>,
        /// Verb that raised the refusal when applicable.
        verb: Option<Verb>,
        /// Sync system key when a stream is the subject.
        system: Option<StreamKey>,
        /// Sync stream key when a stream is the subject.
        stream: Option<StreamKey>,
    },
    /// `queue.propose`
    QueuePropose {
        /// Proposal labels.
        labels: LabelSet,
    },
    /// `queue.accept`
    QueueAccept {
        /// Proposal labels.
        labels: LabelSet,
    },
    /// `queue.reject`
    QueueReject {
        /// Proposal labels.
        labels: LabelSet,
    },
    /// `ai.begin`
    AiBegin {
        /// Transaction sequence number (`equals seq`).
        tx: Count,
        /// Purpose word.
        purpose: Word,
        /// Destination (`model`).
        dest: DestKind,
        /// Request labels.
        labels: LabelSet,
        /// Policy digest in force.
        policy: Sha256,
        /// Adapter identifier.
        adapter: Word,
        /// Endpoint name.
        endpoint: EndpointName,
        /// Model identifier.
        model: ModelId,
        /// Template identifier.
        template: Word,
        /// SHA-256 of the rendered prompt.
        prompt_sha256: Sha256,
        /// Store-qualified context references.
        context_ids: Vec<RecordRef>,
    },
    /// `ai.call`
    AiCall {
        /// Transaction sequence number of the matching `ai.begin`.
        tx: Count,
        /// Call outcome (`ok`, `failed`, `invalid`, `truncated`, `cancelled`).
        outcome: Word,
        /// Diagnostic code when the call did not succeed cleanly.
        code: Option<Code>,
        /// Number of transport attempts made.
        attempts: Count,
        /// SHA-256 of the model reply when received.
        output_sha256: Option<Sha256>,
        /// Input token count.
        tokens_in: Count,
        /// Output token count.
        tokens_out: Count,
    },
    /// `ai.pack`
    AiPack {
        /// SHA-256 of the rendered prompt pack.
        prompt_sha256: Sha256,
        /// Store-qualified context references.
        context_ids: Vec<RecordRef>,
        /// Destination (`handoff`).
        dest: DestKind,
        /// Pack labels.
        labels: LabelSet,
        /// Policy digest in force.
        policy: Sha256,
    },
    /// `ai.paste`
    AiPaste {
        /// Sequence number of the matching `ai.pack` (`None` for `new note --ai-drafted`).
        pack_seq: Option<Count>,
        /// SHA-256 of the pasted output bytes.
        output_sha256: Sha256,
        /// Labels of the pasted content.
        labels: LabelSet,
    },
    /// `ask`
    Ask {
        /// Asked row identifier (`A-nnnn`, omitted in `off` mode).
        asked: Option<Id>,
        /// Door by which the question was asked (`search`, `context`, `ask`, `chat`).
        asked_via: AskedVia,
        /// SHA-256 of the question text.
        question_sha256: Sha256,
        /// Number of hits returned.
        result_count: Count,
        /// Question outcome (`answered`, `no_result`, `cite_failed`, `flagged`).
        outcome: Word,
        /// Resolved labels of the question and inputs.
        labels: LabelSet,
    },
    /// `mcp.session`
    McpSession {
        /// Negotiated protocol version (`2025-11-25`).
        protocol: Word,
        /// SHA-256 of `clientInfo.name` when supplied.
        client_sha256: Option<Sha256>,
        /// Effective allowed labels for this session.
        allow: LabelAllow,
    },
    /// `mcp.call`
    McpCall {
        /// Tool name.
        tool: Word,
        /// SHA-256 of the canonicalized tool arguments.
        args_sha256: Sha256,
        /// Store-qualified record references in the tool result.
        result_ids: Vec<RecordRef>,
        /// Destination (`mcp`).
        dest: DestKind,
        /// Union of labels of the rendered result.
        labels: LabelSet,
        /// Policy digest in force.
        policy: Sha256,
        /// Tool call outcome (`ok`, `invalid`, `failed`).
        outcome: Word,
    },
    /// `chat.session`
    ChatSession {
        /// Session phase (`start`, `stop`, `idle`).
        phase: Word,
        /// Number of calls made in the session.
        calls: Count,
    },
    /// `chat.pack`
    ChatPack {
        /// Pack action (`save` or `delete`).
        phase: Word,
        /// SHA-256 of the pack name.
        name_sha256: Sha256,
        /// Item count in the pack.
        count: Count,
    },
    /// `backup.begin`
    BackupBegin {
        /// Snapshot label union.
        labels: LabelSet,
        /// Destination (`backup`).
        dest: DestKind,
        /// Policy digest in force.
        policy: Sha256,
        /// Snapshot number (`>= 1`).
        snapshot: Count,
    },
    /// `backup.end`
    BackupEnd {
        /// Outcome (`ok` or `failed`).
        outcome: Word,
        /// Number of files copied.
        files: Count,
        /// Total bytes copied.
        bytes: Count,
        /// SHA-256 of `SHA256SUMS`.
        manifest_sha256: Sha256,
        /// Journal sequence number recorded in the snapshot.
        journal_seq: Count,
        /// Journal head hash recorded in the snapshot.
        journal_head: Sha256,
        /// Content fingerprint of the backed-up store.
        fingerprint: Sha256,
        /// Target kind (`folder`, `it-location`, `synced`).
        target_kind: Word,
    },
    /// `backup.verify`
    BackupVerify {
        /// Outcome (`ok` or `failed`).
        outcome: Word,
        /// Snapshot or raw verification payload.
        verified: BackupVerified,
    },
    /// `backup.prune`
    BackupPrune {
        /// Removed snapshot numbers.
        snapshots: Vec<Count>,
        /// Prune reason (`retention`).
        reason: Word,
    },
    /// `restore.begin`
    RestoreBegin {
        /// Snapshot number being restored.
        snapshot: Count,
        /// Restore mode (`drill` or `real`).
        mode: Word,
        /// Labels of the snapshot.
        labels: LabelSet,
        /// Destination when restoring outside the data root (`export`).
        dest: Option<DestKind>,
        /// Policy digest when restoring outside the data root.
        policy: Option<Sha256>,
    },
    /// `restore.end`
    RestoreEnd {
        /// Snapshot number restored.
        snapshot: Count,
        /// Restore mode (`drill` or `real`).
        mode: Word,
        /// Outcome (`ok` or `failed`).
        outcome: Word,
        /// Diagnostic code on failure.
        code: Option<Code>,
    },
    /// `settings.change`
    SettingsChange {
        /// Sorted, distinct settings groups whose digests changed.
        names: Vec<SettingsGroup>,
        /// Complete typed settings baseline.
        baseline: SettingsBaseline,
    },
    /// `purge`
    Purge {
        /// What was purged (`asked` or `calls`).
        what: Word,
        /// Number of expired rows or files removed.
        count: Count,
    },
    /// `policy.change`
    PolicyChange {
        /// Current policy or observation digest.
        policy: Sha256,
        /// Previous policy or observation digest (64 zeros when none).
        prev: Sha256,
        /// Policy revision number.
        revision: Count,
        /// Resolution outcome (`ok`, `failed`, `loaded`, `absent`, `invalid`).
        outcome: Word,
        /// Number of defined labels.
        labels_n: Count,
        /// Number of destination rows.
        rows_n: Count,
    },
    /// `label.change`
    LabelChange {
        /// Store-qualified record references whose labels changed.
        ids: Vec<RecordRef>,
        /// Labels added.
        added: LabelAllow,
        /// Labels removed.
        removed: LabelAllow,
        /// Origin (`person` or `reapply`).
        origin: Word,
    },
    /// `audit.export`
    AuditExport {
        /// First exported sequence number.
        from_seq: Count,
        /// Last exported sequence number.
        to_seq: Count,
        /// Head hash at `to_seq`.
        head: Sha256,
        /// Policy digest in force.
        policy: Sha256,
        /// Store UUID (nil UUID when no company store exists).
        store_uid: StoreUid,
    },
    /// `store.missing`
    StoreMissing {
        /// UUID of the missing company store.
        store_uid: StoreUid,
    },
    /// `store.adopted`
    StoreAdopted {
        /// UUID of the adopted company store.
        store_uid: StoreUid,
        /// Adoption mode (`retired` or `writer`).
        mode: Word,
        /// Resulting writer index.
        writer: Count,
    },
    /// `sync.begin`
    SyncBegin {
        /// Sync system key.
        system: StreamKey,
        /// Sync stream key.
        stream: StreamKey,
        /// Pull adapter identifier (`file-drop@1`).
        adapter: Word,
        /// Optional canonical source-root fingerprint.
        root_sha256: Option<Sha256>,
        /// Policy digest in force.
        policy: Sha256,
    },
    /// `sync.end`
    SyncEnd {
        /// Sync system key.
        system: StreamKey,
        /// Sync stream key.
        stream: StreamKey,
        /// Sync scope (`full` or `incremental`).
        scope: Word,
        /// Files seen.
        seen: Count,
        /// Proposals created.
        proposed: Count,
        /// Files already present.
        present: Count,
        /// Files skipped.
        skipped: Count,
        /// Files refused.
        refused: Count,
    },
}

impl EventPayload {
    /// Return the [`Kind`] discriminator of this payload.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::JournalOpen { .. } => Kind::JournalOpen,
            Self::JournalRecover { .. } => Kind::JournalRecover,
            Self::WriteBegin { .. } => Kind::WriteBegin,
            Self::WriteEnd { .. } => Kind::WriteEnd,
            Self::Refuse { .. } => Kind::Refuse,
            Self::QueuePropose { .. } => Kind::QueuePropose,
            Self::QueueAccept { .. } => Kind::QueueAccept,
            Self::QueueReject { .. } => Kind::QueueReject,
            Self::AiBegin { .. } => Kind::AiBegin,
            Self::AiCall { .. } => Kind::AiCall,
            Self::AiPack { .. } => Kind::AiPack,
            Self::AiPaste { .. } => Kind::AiPaste,
            Self::Ask { .. } => Kind::Ask,
            Self::McpSession { .. } => Kind::McpSession,
            Self::McpCall { .. } => Kind::McpCall,
            Self::ChatSession { .. } => Kind::ChatSession,
            Self::ChatPack { .. } => Kind::ChatPack,
            Self::BackupBegin { .. } => Kind::BackupBegin,
            Self::BackupEnd { .. } => Kind::BackupEnd,
            Self::BackupVerify { .. } => Kind::BackupVerify,
            Self::BackupPrune { .. } => Kind::BackupPrune,
            Self::RestoreBegin { .. } => Kind::RestoreBegin,
            Self::RestoreEnd { .. } => Kind::RestoreEnd,
            Self::SettingsChange { .. } => Kind::SettingsChange,
            Self::Purge { .. } => Kind::Purge,
            Self::PolicyChange { .. } => Kind::PolicyChange,
            Self::LabelChange { .. } => Kind::LabelChange,
            Self::AuditExport { .. } => Kind::AuditExport,
            Self::StoreMissing { .. } => Kind::StoreMissing,
            Self::StoreAdopted { .. } => Kind::StoreAdopted,
            Self::SyncBegin { .. } => Kind::SyncBegin,
            Self::SyncEnd { .. } => Kind::SyncEnd,
        }
    }

    /// Render the JSON body prefix `{"seq":...` (up to and excluding `,"hash":"<64 hex>"}`).
    #[must_use]
    pub fn render_body(&self, seq: u64, time: Now, env: &EventEnvelope) -> std::string::String {
        use std::fmt::Write as _;
        let mut out = std::string::String::with_capacity(256);
        let _ = write!(
            out,
            "{{\"seq\":{seq},\"time\":\"{time}\",\"kind\":\"{}\"",
            self.kind()
        );
        out.push_str(",\"actor\":");
        push_json_str(&mut out, env.actor.as_str());
        let _ = write!(
            out,
            ",\"role\":\"{}\",\"store\":\"{}\",\"via\":\"{}\"",
            env.role,
            env.store.as_str(),
            env.via
        );

        match self {
            Self::JournalOpen {
                writer,
                event_schema,
                tool_version,
                stores,
            } => {
                let _ = write!(
                    out,
                    ",\"writer\":{writer},\"event_schema\":{event_schema},\"tool_version\":\"{tool_version}\",\"stores\":["
                );
                for (i, uid) in stores.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{uid}\"");
                }
                out.push(']');
            }
            Self::JournalRecover {
                reason,
                last_seq,
                last_hash,
            } => {
                let _ = write!(
                    out,
                    ",\"reason\":\"{reason}\",\"last_seq\":{last_seq},\"last_hash\":\"{last_hash}\""
                );
            }
            Self::WriteBegin {
                verb,
                labels,
                files,
                file_hashes,
                affected,
                dest,
                path_sha256,
                policy,
            } => {
                let _ = write!(out, ",\"verb\":\"{verb}\",\"labels\":");
                push_label_set(&mut out, labels);
                let _ = write!(out, ",\"files\":{files}");
                if let Some(hashes) = file_hashes {
                    out.push_str(",\"file_hashes\":[");
                    for (i, fh) in hashes.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        let _ = write!(
                            out,
                            "{{\"path_sha256\":\"{}\",\"sha256\":\"{}\"}}",
                            fh.path_sha256, fh.sha256
                        );
                    }
                    out.push(']');
                }
                out.push_str(",\"affected\":");
                push_record_refs(&mut out, affected);
                if let Some(d) = dest {
                    let _ = write!(out, ",\"dest\":\"{}\"", d.as_str());
                }
                if let Some(p) = path_sha256 {
                    let _ = write!(out, ",\"path_sha256\":\"{p}\"");
                }
                if let Some(pol) = policy {
                    let _ = write!(out, ",\"policy\":\"{pol}\"");
                }
            }
            Self::WriteEnd {
                verb,
                labels,
                affected,
                outcome,
                dest,
                path_sha256,
                policy,
            } => {
                let _ = write!(out, ",\"verb\":\"{verb}\",\"labels\":");
                push_label_set(&mut out, labels);
                out.push_str(",\"affected\":");
                push_record_refs(&mut out, affected);
                let _ = write!(out, ",\"outcome\":\"{outcome}\"");
                if let Some(d) = dest {
                    let _ = write!(out, ",\"dest\":\"{}\"", d.as_str());
                }
                if let Some(p) = path_sha256 {
                    let _ = write!(out, ",\"path_sha256\":\"{p}\"");
                }
                if let Some(pol) = policy {
                    let _ = write!(out, ",\"policy\":\"{pol}\"");
                }
            }
            Self::Refuse {
                code,
                reason,
                sha256,
                name_sha256,
                labels,
                dest,
                verb,
                system,
                stream,
            } => {
                let _ = write!(out, ",\"code\":\"{}\",\"reason\":\"{reason}\"", code.id());
                if let Some(s) = sha256 {
                    let _ = write!(out, ",\"sha256\":\"{s}\"");
                }
                if let Some(n) = name_sha256 {
                    let _ = write!(out, ",\"name_sha256\":\"{n}\"");
                }
                if let Some(l) = labels {
                    out.push_str(",\"labels\":");
                    push_label_set(&mut out, l);
                }
                if let Some(d) = dest {
                    let _ = write!(out, ",\"dest\":\"{}\"", d.as_str());
                }
                if let Some(v) = verb {
                    let _ = write!(out, ",\"verb\":\"{v}\"");
                }
                if let Some(sys) = system {
                    let _ = write!(out, ",\"system\":\"{sys}\"");
                }
                if let Some(str_key) = stream {
                    let _ = write!(out, ",\"stream\":\"{str_key}\"");
                }
            }
            Self::QueuePropose { labels }
            | Self::QueueAccept { labels }
            | Self::QueueReject { labels } => {
                out.push_str(",\"labels\":");
                push_label_set(&mut out, labels);
            }
            Self::AiBegin {
                tx,
                purpose,
                dest,
                labels,
                policy,
                adapter,
                endpoint,
                model,
                template,
                prompt_sha256,
                context_ids,
            } => {
                let _ = write!(
                    out,
                    ",\"tx\":{tx},\"purpose\":\"{purpose}\",\"dest\":\"{}\",\"labels\":",
                    dest.as_str()
                );
                push_label_set(&mut out, labels);
                let _ = write!(
                    out,
                    ",\"policy\":\"{policy}\",\"adapter\":\"{adapter}\",\"endpoint\":\"{endpoint}\",\"model\":\"{model}\",\"template\":\"{template}\",\"prompt_sha256\":\"{prompt_sha256}\",\"context_ids\":"
                );
                push_record_refs(&mut out, context_ids);
            }
            Self::AiCall {
                tx,
                outcome,
                code,
                attempts,
                output_sha256,
                tokens_in,
                tokens_out,
            } => {
                let _ = write!(out, ",\"tx\":{tx},\"outcome\":\"{outcome}\"");
                if let Some(c) = code {
                    let _ = write!(out, ",\"code\":\"{}\"", c.id());
                }
                let _ = write!(out, ",\"attempts\":{attempts}");
                if let Some(out_sha) = output_sha256 {
                    let _ = write!(out, ",\"output_sha256\":\"{out_sha}\"");
                }
                let _ = write!(
                    out,
                    ",\"tokens_in\":{tokens_in},\"tokens_out\":{tokens_out}"
                );
            }
            Self::AiPack {
                prompt_sha256,
                context_ids,
                dest,
                labels,
                policy,
            } => {
                let _ = write!(
                    out,
                    ",\"prompt_sha256\":\"{prompt_sha256}\",\"context_ids\":"
                );
                push_record_refs(&mut out, context_ids);
                let _ = write!(out, ",\"dest\":\"{}\",\"labels\":", dest.as_str());
                push_label_set(&mut out, labels);
                let _ = write!(out, ",\"policy\":\"{policy}\"");
            }
            Self::AiPaste {
                pack_seq,
                output_sha256,
                labels,
            } => {
                if let Some(ps) = pack_seq {
                    let _ = write!(out, ",\"pack_seq\":{ps}");
                }
                let _ = write!(out, ",\"output_sha256\":\"{output_sha256}\",\"labels\":");
                push_label_set(&mut out, labels);
            }
            Self::Ask {
                asked,
                asked_via,
                question_sha256,
                result_count,
                outcome,
                labels,
            } => {
                if let Some(id) = asked {
                    let _ = write!(out, ",\"asked\":\"{id}\"");
                }
                let _ = write!(
                    out,
                    ",\"asked_via\":\"{asked_via}\",\"question_sha256\":\"{question_sha256}\",\"result_count\":{result_count},\"outcome\":\"{outcome}\",\"labels\":"
                );
                push_label_set(&mut out, labels);
            }
            Self::McpSession {
                protocol,
                client_sha256,
                allow,
            } => {
                let _ = write!(out, ",\"protocol\":\"{protocol}\"");
                if let Some(cs) = client_sha256 {
                    let _ = write!(out, ",\"client_sha256\":\"{cs}\"");
                }
                out.push_str(",\"allow\":");
                push_label_allow(&mut out, allow);
            }
            Self::McpCall {
                tool,
                args_sha256,
                result_ids,
                dest,
                labels,
                policy,
                outcome,
            } => {
                let _ = write!(
                    out,
                    ",\"tool\":\"{tool}\",\"args_sha256\":\"{args_sha256}\",\"result_ids\":"
                );
                push_record_refs(&mut out, result_ids);
                let _ = write!(out, ",\"dest\":\"{}\",\"labels\":", dest.as_str());
                push_label_set(&mut out, labels);
                let _ = write!(out, ",\"policy\":\"{policy}\",\"outcome\":\"{outcome}\"");
            }
            Self::ChatSession { phase, calls } => {
                let _ = write!(out, ",\"phase\":\"{phase}\",\"calls\":{calls}");
            }
            Self::ChatPack {
                phase,
                name_sha256,
                count,
            } => {
                let _ = write!(
                    out,
                    ",\"phase\":\"{phase}\",\"name_sha256\":\"{name_sha256}\",\"count\":{count}"
                );
            }
            Self::BackupBegin {
                labels,
                dest,
                policy,
                snapshot,
            } => {
                out.push_str(",\"labels\":");
                push_label_set(&mut out, labels);
                let _ = write!(
                    out,
                    ",\"dest\":\"{}\",\"policy\":\"{policy}\",\"snapshot\":{snapshot}",
                    dest.as_str()
                );
            }
            Self::BackupEnd {
                outcome,
                files,
                bytes,
                manifest_sha256,
                journal_seq,
                journal_head,
                fingerprint,
                target_kind,
            } => {
                let _ = write!(
                    out,
                    ",\"outcome\":\"{outcome}\",\"files\":{files},\"bytes\":{bytes},\"manifest_sha256\":\"{manifest_sha256}\",\"journal_seq\":{journal_seq},\"journal_head\":\"{journal_head}\",\"fingerprint\":\"{fingerprint}\",\"target_kind\":\"{target_kind}\""
                );
            }
            Self::BackupVerify { outcome, verified } => match verified {
                BackupVerified::Snapshot {
                    target_sha256,
                    snapshot,
                    manifest_sha256,
                } => {
                    let _ = write!(
                        out,
                        ",\"verification\":\"snapshot\",\"outcome\":\"{outcome}\",\"target_sha256\":\"{target_sha256}\",\"snapshot\":{snapshot},\"manifest_sha256\":\"{manifest_sha256}\""
                    );
                }
                BackupVerified::Raw {
                    path_sha256,
                    public_support_sha256,
                } => {
                    let _ = write!(
                        out,
                        ",\"verification\":\"raw\",\"outcome\":\"{outcome}\",\"path_sha256\":\"{path_sha256}\""
                    );
                    if let Some(sup) = public_support_sha256 {
                        let _ = write!(out, ",\"public_support_sha256\":\"{sup}\"");
                    }
                }
            },
            Self::BackupPrune { snapshots, reason } => {
                out.push_str(",\"snapshots\":[");
                for (i, s) in snapshots.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "{s}");
                }
                let _ = write!(out, "],\"reason\":\"{reason}\"");
            }
            Self::RestoreBegin {
                snapshot,
                mode,
                labels,
                dest,
                policy,
            } => {
                let _ = write!(
                    out,
                    ",\"snapshot\":{snapshot},\"mode\":\"{mode}\",\"labels\":"
                );
                push_label_set(&mut out, labels);
                if let Some(d) = dest {
                    let _ = write!(out, ",\"dest\":\"{}\"", d.as_str());
                }
                if let Some(pol) = policy {
                    let _ = write!(out, ",\"policy\":\"{pol}\"");
                }
            }
            Self::RestoreEnd {
                snapshot,
                mode,
                outcome,
                code,
            } => {
                let _ = write!(
                    out,
                    ",\"snapshot\":{snapshot},\"mode\":\"{mode}\",\"outcome\":\"{outcome}\""
                );
                if let Some(c) = code {
                    let _ = write!(out, ",\"code\":\"{}\"", c.id());
                }
            }
            Self::SettingsChange { names, baseline } => {
                out.push_str(",\"names\":[");
                for (i, g) in names.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{g}\"");
                }
                let _ = write!(
                    out,
                    "],\"baseline\":{{\"schema\":{},\"present\":{},\"file_sha256\":",
                    baseline.schema, baseline.present
                );
                match &baseline.file_sha256 {
                    Some(h) => {
                        let _ = write!(out, "\"{h}\"");
                    }
                    None => out.push_str("null"),
                }
                out.push_str(",\"groups\":{");
                for (i, group) in SettingsGroup::ALL.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{group}\":");
                    match baseline.groups.get(group).and_then(Option::as_ref) {
                        Some(h) => {
                            let _ = write!(out, "\"{h}\"");
                        }
                        None => out.push_str("null"),
                    }
                }
                out.push_str("}}");
            }
            Self::Purge { what, count } => {
                let _ = write!(out, ",\"what\":\"{what}\",\"count\":{count}");
            }
            Self::PolicyChange {
                policy,
                prev,
                revision,
                outcome,
                labels_n,
                rows_n,
            } => {
                let _ = write!(
                    out,
                    ",\"policy\":\"{policy}\",\"prev\":\"{prev}\",\"revision\":{revision},\"outcome\":\"{outcome}\",\"labels_n\":{labels_n},\"rows_n\":{rows_n}"
                );
            }
            Self::LabelChange {
                ids,
                added,
                removed,
                origin,
            } => {
                out.push_str(",\"ids\":");
                push_record_refs(&mut out, ids);
                out.push_str(",\"added\":");
                push_label_allow(&mut out, added);
                out.push_str(",\"removed\":");
                push_label_allow(&mut out, removed);
                let _ = write!(out, ",\"origin\":\"{origin}\"");
            }
            Self::AuditExport {
                from_seq,
                to_seq,
                head,
                policy,
                store_uid,
            } => {
                let _ = write!(
                    out,
                    ",\"from_seq\":{from_seq},\"to_seq\":{to_seq},\"head\":\"{head}\",\"policy\":\"{policy}\",\"store_uid\":\"{store_uid}\""
                );
            }
            Self::StoreMissing { store_uid } => {
                let _ = write!(out, ",\"store_uid\":\"{store_uid}\"");
            }
            Self::StoreAdopted {
                store_uid,
                mode,
                writer,
            } => {
                let _ = write!(
                    out,
                    ",\"store_uid\":\"{store_uid}\",\"mode\":\"{mode}\",\"writer\":{writer}"
                );
            }
            Self::SyncBegin {
                system,
                stream,
                adapter,
                root_sha256,
                policy,
            } => {
                let _ = write!(
                    out,
                    ",\"system\":\"{system}\",\"stream\":\"{stream}\",\"adapter\":\"{adapter}\""
                );
                if let Some(root) = root_sha256 {
                    let _ = write!(out, ",\"root_sha256\":\"{root}\"");
                }
                let _ = write!(out, ",\"policy\":\"{policy}\"");
            }
            Self::SyncEnd {
                system,
                stream,
                scope,
                seen,
                proposed,
                present,
                skipped,
                refused,
            } => {
                let _ = write!(
                    out,
                    ",\"system\":\"{system}\",\"stream\":\"{stream}\",\"scope\":\"{scope}\",\"seen\":{seen},\"proposed\":{proposed},\"present\":{present},\"skipped\":{skipped},\"refused\":{refused}"
                );
            }
        }
        out
    }

    /// Seal and format a complete JSONL event line (`{"seq":...,"hash":"<64 hex>"}`).
    #[must_use]
    pub fn format_line(
        &self,
        seq: u64,
        time: Now,
        env: &EventEnvelope,
        prev_hex: &str,
    ) -> (std::string::String, Sha256) {
        let body = self.render_body(seq, time, env);
        seal_body(prev_hex, &body)
    }
}

fn push_label_set(out: &mut std::string::String, labels: &LabelSet) {
    use std::fmt::Write as _;
    out.push('[');
    for (i, label) in labels.ids().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\"", label.as_str());
    }
    out.push(']');
}

fn push_label_allow(out: &mut std::string::String, allow: &LabelAllow) {
    use std::fmt::Write as _;
    out.push('[');
    for (i, label) in allow.ids().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\"", label.as_str());
    }
    out.push(']');
}

fn push_record_refs(out: &mut std::string::String, refs: &[RecordRef]) {
    use std::fmt::Write as _;
    out.push('[');
    for (i, r) in refs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{r}\"");
    }
    out.push(']');
}

/// A parsed and vocabulary-validated journal event line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// 1-based sequence number.
    pub seq: u64,
    /// Event timestamp in UTC.
    pub time: Now,
    /// Common event envelope.
    pub envelope: EventEnvelope,
    /// Kind-specific typed payload.
    pub payload: EventPayload,
    /// Line hash stored in the `hash` suffix.
    pub hash: Sha256,
}

impl Event {
    /// Return the [`Kind`] of this event.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.payload.kind()
    }

    /// Parse and validate a journal line's vocabulary and envelope (FB0801).
    ///
    /// Also returns the [`RawEventObject`] preserving exact stored key order for `audit show --json`.
    ///
    /// # Errors
    ///
    /// Returns [`VocabError`] if the line is not valid JSON, has duplicate or unknown keys,
    /// lacks a required envelope key, violates destination/label/policy rules, or holds any
    /// malformed or free-text value.
    pub fn parse_line(line: &str) -> Result<(Self, RawEventObject), VocabError> {
        let obj = parse_json_object(line).map_err(VocabError::InvalidWord)?;
        let seq = req_u64(&obj, "seq")?;
        if seq == 0 {
            return Err(VocabError::InvalidWord("seq must be >= 1".to_owned()));
        }
        let time = Now::parse(req_str(&obj, "time")?)?;
        let kind = Kind::parse(req_str(&obj, "kind")?)?;
        let actor = Principal::parse(req_str(&obj, "actor")?)?;
        let role = Role::parse(req_str(&obj, "role")?)?;
        let store_raw = req_str(&obj, "store")?;
        let store = StoreId::from_root_name(store_raw)
            .ok_or_else(|| VocabError::InvalidWord(store_raw.to_owned()))?;
        let via = Via::parse(req_str(&obj, "via")?)?;
        let hash = Sha256::parse(req_str(&obj, "hash")?)
            .map_err(|_| VocabError::InvalidSha256("hash".to_owned()))?;

        // Check destination / labels / policy cross-cutting contract rules before kind dispatch.
        if kind == Kind::Refuse && obj.get("policy").is_some() {
            return Err(VocabError::InvalidWord(
                "refuse must not carry policy".to_owned(),
            ));
        }
        if kind != Kind::Refuse
            && obj.get("dest").is_some()
            && (obj.get("labels").is_none() || obj.get("policy").is_none())
        {
            return Err(VocabError::InvalidWord(
                "dest on a non-refuse event requires labels and policy".to_owned(),
            ));
        }
        if kind.is_egress()
            && (obj.get("dest").is_none()
                || obj.get("labels").is_none()
                || obj.get("policy").is_none())
        {
            return Err(VocabError::InvalidWord(
                "egress event requires dest, labels, and policy".to_owned(),
            ));
        }

        let payload = parse_payload(kind, seq, &obj)?;
        let event = Self {
            seq,
            time,
            envelope: EventEnvelope {
                actor,
                role,
                store,
                via,
            },
            payload,
            hash,
        };
        Ok((event, obj))
    }

    /// Return `true` when this event touches `query_id` (matched as either bare `<id>` or
    /// `<store>/<id>`).
    #[must_use]
    pub fn touches_id(&self, query_id: &str) -> bool {
        let matches_ref = |r: &RecordRef| {
            r.id().as_str() == query_id
                || format!("{}/{}", r.store().as_str(), r.id().as_str()) == query_id
        };
        match &self.payload {
            EventPayload::WriteBegin { affected, .. } | EventPayload::WriteEnd { affected, .. } => {
                affected.iter().any(matches_ref)
            }
            EventPayload::AiBegin { context_ids, .. }
            | EventPayload::AiPack { context_ids, .. } => context_ids.iter().any(matches_ref),
            EventPayload::McpCall { result_ids, .. } => result_ids.iter().any(matches_ref),
            EventPayload::LabelChange { ids, .. } => ids.iter().any(matches_ref),
            EventPayload::Ask {
                asked: Some(id), ..
            } => {
                id.as_str() == query_id
                    || format!("{}/{}", self.envelope.store.as_str(), id.as_str()) == query_id
            }
            _ => false,
        }
    }

    /// Return `true` when this event carries `label` in its label set or permission list.
    #[must_use]
    pub fn touches_label(&self, label: &LabelId) -> bool {
        match &self.payload {
            EventPayload::WriteBegin { labels, .. }
            | EventPayload::WriteEnd { labels, .. }
            | EventPayload::QueuePropose { labels }
            | EventPayload::QueueAccept { labels }
            | EventPayload::QueueReject { labels }
            | EventPayload::AiBegin { labels, .. }
            | EventPayload::AiPack { labels, .. }
            | EventPayload::AiPaste { labels, .. }
            | EventPayload::Ask { labels, .. }
            | EventPayload::McpCall { labels, .. }
            | EventPayload::BackupBegin { labels, .. }
            | EventPayload::RestoreBegin { labels, .. }
            | EventPayload::Refuse {
                labels: Some(labels),
                ..
            } => labels.contains(label),
            EventPayload::McpSession { allow, .. } => allow.contains(label),
            EventPayload::LabelChange { added, removed, .. } => {
                added.contains(label) || removed.contains(label)
            }
            _ => false,
        }
    }
}

fn check_allowed_keys(obj: &RawEventObject, allowed: &[&str]) -> Result<(), VocabError> {
    const ENVELOPE_KEYS: &[&str] = &[
        "seq", "time", "kind", "actor", "role", "store", "via", "hash",
    ];
    for (k, _) in &obj.0 {
        let key = k.as_str();
        if !ENVELOPE_KEYS.contains(&key) && !allowed.contains(&key) {
            return Err(VocabError::InvalidWord(format!("unknown key `{key}`")));
        }
    }
    Ok(())
}

fn parse_payload(kind: Kind, seq: u64, obj: &RawEventObject) -> Result<EventPayload, VocabError> {
    match kind {
        Kind::JournalOpen => {
            check_allowed_keys(obj, &["writer", "event_schema", "tool_version", "stores"])?;
            let writer_u64 = req_u64(obj, "writer")?;
            if writer_u64 > 9999 {
                return Err(VocabError::InvalidWord("writer out of range".to_owned()));
            }
            let event_schema = req_u64(obj, "event_schema")?;
            if event_schema != 1 {
                return Err(VocabError::InvalidWord(
                    "unsupported event_schema".to_owned(),
                ));
            }
            let tv_raw = req_str(obj, "tool_version")?;
            let tool_version =
                SemVer::parse(tv_raw).map_err(|_| VocabError::InvalidVersion(tv_raw.to_owned()))?;
            let stores = req_store_uids(obj, "stores")?;
            Ok(EventPayload::JournalOpen {
                writer: Count(writer_u64),
                event_schema: Count(event_schema),
                tool_version,
                stores,
            })
        }
        Kind::JournalRecover => {
            check_allowed_keys(obj, &["reason", "last_seq", "last_hash"])?;
            let reason = Word::from_closed(req_str(obj, "reason")?, RECOVER_REASONS)?;
            let last_seq = Count(req_u64(obj, "last_seq")?);
            let last_hash = req_sha256(obj, "last_hash")?;
            Ok(EventPayload::JournalRecover {
                reason,
                last_seq,
                last_hash,
            })
        }
        Kind::WriteBegin => {
            check_allowed_keys(
                obj,
                &[
                    "verb",
                    "labels",
                    "files",
                    "file_hashes",
                    "affected",
                    "dest",
                    "path_sha256",
                    "policy",
                ],
            )?;
            let verb = Verb::parse(req_str(obj, "verb")?)?;
            let labels = req_label_set(obj, "labels")?;
            let files = Count(req_u64(obj, "files")?);
            let file_hashes = opt_file_hashes(obj, "file_hashes")?;
            let affected = req_sorted_record_refs(obj, "affected")?;
            let dest = opt_dest(obj, "dest")?;
            let path_sha256 = opt_sha256(obj, "path_sha256")?;
            let policy = opt_sha256(obj, "policy")?;

            let is_export = dest.is_some() || path_sha256.is_some() || policy.is_some();
            if is_export {
                if dest.is_none()
                    || path_sha256.is_none()
                    || policy.is_none()
                    || file_hashes.is_some()
                {
                    return Err(VocabError::InvalidWord(
                        "export write.begin requires dest, path_sha256, policy and forbids file_hashes"
                            .to_owned(),
                    ));
                }
            } else {
                let Some(hashes) = &file_hashes else {
                    return Err(VocabError::InvalidWord(
                        "in-root write.begin requires file_hashes".to_owned(),
                    ));
                };
                if hashes.len() as u64 != files.0 {
                    return Err(VocabError::InvalidWord(
                        "file_hashes count must equal files".to_owned(),
                    ));
                }
            }
            Ok(EventPayload::WriteBegin {
                verb,
                labels,
                files,
                file_hashes,
                affected,
                dest,
                path_sha256,
                policy,
            })
        }
        Kind::WriteEnd => {
            check_allowed_keys(
                obj,
                &[
                    "verb",
                    "labels",
                    "affected",
                    "outcome",
                    "dest",
                    "path_sha256",
                    "policy",
                ],
            )?;
            let verb = Verb::parse(req_str(obj, "verb")?)?;
            let labels = req_label_set(obj, "labels")?;
            let affected = req_sorted_record_refs(obj, "affected")?;
            let outcome = Word::from_closed(req_str(obj, "outcome")?, OK_FAILED_OUTCOMES)?;
            let dest = opt_dest(obj, "dest")?;
            let path_sha256 = opt_sha256(obj, "path_sha256")?;
            let policy = opt_sha256(obj, "policy")?;
            if (dest.is_some() || path_sha256.is_some() || policy.is_some())
                && (dest.is_none() || path_sha256.is_none() || policy.is_none())
            {
                return Err(VocabError::InvalidWord(
                    "export write.end requires dest, path_sha256, and policy together".to_owned(),
                ));
            }
            Ok(EventPayload::WriteEnd {
                verb,
                labels,
                affected,
                outcome,
                dest,
                path_sha256,
                policy,
            })
        }
        Kind::Refuse => {
            check_allowed_keys(
                obj,
                &[
                    "code",
                    "reason",
                    "sha256",
                    "name_sha256",
                    "labels",
                    "dest",
                    "verb",
                    "system",
                    "stream",
                ],
            )?;
            let code = req_code(obj, "code")?;
            let reason = Word::from_closed(req_str(obj, "reason")?, REFUSE_REASONS)?;
            let sha256 = opt_sha256(obj, "sha256")?;
            let name_sha256 = opt_sha256(obj, "name_sha256")?;
            let labels = opt_label_set(obj, "labels")?;
            let dest = opt_dest(obj, "dest")?;
            let verb = match obj.get("verb") {
                Some(RawJsonValue::Str(s)) => Some(Verb::parse(s)?),
                Some(_) => return Err(VocabError::InvalidVerb("verb".to_owned())),
                None => None,
            };
            let system = match obj.get("system") {
                Some(RawJsonValue::Str(s)) => Some(StreamKey::parse(s)?),
                Some(_) => return Err(VocabError::InvalidStreamKey("system".to_owned())),
                None => None,
            };
            let stream = match obj.get("stream") {
                Some(RawJsonValue::Str(s)) => Some(StreamKey::parse(s)?),
                Some(_) => return Err(VocabError::InvalidStreamKey("stream".to_owned())),
                None => None,
            };
            Ok(EventPayload::Refuse {
                code,
                reason,
                sha256,
                name_sha256,
                labels,
                dest,
                verb,
                system,
                stream,
            })
        }
        Kind::QueuePropose => {
            check_allowed_keys(obj, &["labels"])?;
            let labels = req_label_set(obj, "labels")?;
            Ok(EventPayload::QueuePropose { labels })
        }
        Kind::QueueAccept => {
            check_allowed_keys(obj, &["labels"])?;
            let labels = req_label_set(obj, "labels")?;
            Ok(EventPayload::QueueAccept { labels })
        }
        Kind::QueueReject => {
            check_allowed_keys(obj, &["labels"])?;
            let labels = req_label_set(obj, "labels")?;
            Ok(EventPayload::QueueReject { labels })
        }
        Kind::AiBegin => {
            check_allowed_keys(
                obj,
                &[
                    "tx",
                    "purpose",
                    "dest",
                    "labels",
                    "policy",
                    "adapter",
                    "endpoint",
                    "model",
                    "template",
                    "prompt_sha256",
                    "context_ids",
                ],
            )?;
            let tx = req_u64(obj, "tx")?;
            if tx == 0 || tx != seq {
                return Err(VocabError::InvalidWord(
                    "ai.begin tx must equal seq".to_owned(),
                ));
            }
            let purpose = Word::from_closed(req_str(obj, "purpose")?, AI_PURPOSES)?;
            let dest = req_dest(obj, "dest")?;
            let labels = req_label_set(obj, "labels")?;
            let policy = req_sha256(obj, "policy")?;
            let adapter = Word::from_closed(req_str(obj, "adapter")?, AI_ADAPTERS)?;
            let endpoint = EndpointName::parse(req_str(obj, "endpoint")?)?;
            let model = ModelId::parse(req_str(obj, "model")?)?;
            let template = Word::from_closed(req_str(obj, "template")?, AI_TEMPLATES)?;
            let prompt_sha256 = req_sha256(obj, "prompt_sha256")?;
            let context_ids = req_record_refs(obj, "context_ids")?;
            Ok(EventPayload::AiBegin {
                tx: Count(tx),
                purpose,
                dest,
                labels,
                policy,
                adapter,
                endpoint,
                model,
                template,
                prompt_sha256,
                context_ids,
            })
        }
        Kind::AiCall => {
            check_allowed_keys(
                obj,
                &[
                    "tx",
                    "outcome",
                    "code",
                    "attempts",
                    "output_sha256",
                    "tokens_in",
                    "tokens_out",
                ],
            )?;
            let tx = req_u64(obj, "tx")?;
            if tx == 0 {
                return Err(VocabError::InvalidWord(
                    "ai.call tx must be >= 1".to_owned(),
                ));
            }
            let outcome = Word::from_closed(req_str(obj, "outcome")?, AI_CALL_OUTCOMES)?;
            let code = opt_code(obj, "code")?;
            let attempts = Count(req_u64(obj, "attempts")?);
            let output_sha256 = opt_sha256(obj, "output_sha256")?;
            let tokens_in = Count(req_u64(obj, "tokens_in")?);
            let tokens_out = Count(req_u64(obj, "tokens_out")?);
            Ok(EventPayload::AiCall {
                tx: Count(tx),
                outcome,
                code,
                attempts,
                output_sha256,
                tokens_in,
                tokens_out,
            })
        }
        Kind::AiPack => {
            check_allowed_keys(
                obj,
                &["prompt_sha256", "context_ids", "dest", "labels", "policy"],
            )?;
            let prompt_sha256 = req_sha256(obj, "prompt_sha256")?;
            let context_ids = req_record_refs(obj, "context_ids")?;
            let dest = req_dest(obj, "dest")?;
            let labels = req_label_set(obj, "labels")?;
            let policy = req_sha256(obj, "policy")?;
            Ok(EventPayload::AiPack {
                prompt_sha256,
                context_ids,
                dest,
                labels,
                policy,
            })
        }
        Kind::AiPaste => {
            check_allowed_keys(obj, &["pack_seq", "output_sha256", "labels"])?;
            let pack_seq = match obj.get("pack_seq") {
                Some(RawJsonValue::Uint(n)) if *n > 0 => Some(Count(*n)),
                Some(_) => return Err(VocabError::InvalidWord("invalid pack_seq".to_owned())),
                None => None,
            };
            let output_sha256 = req_sha256(obj, "output_sha256")?;
            let labels = req_label_set(obj, "labels")?;
            Ok(EventPayload::AiPaste {
                pack_seq,
                output_sha256,
                labels,
            })
        }
        Kind::Ask => {
            check_allowed_keys(
                obj,
                &[
                    "asked",
                    "asked_via",
                    "question_sha256",
                    "result_count",
                    "outcome",
                    "labels",
                ],
            )?;
            let asked = match obj.get("asked") {
                Some(RawJsonValue::Str(s)) => Some(Id::parse(s)?),
                Some(_) => return Err(VocabError::InvalidId("asked".to_owned())),
                None => None,
            };
            let asked_via = AskedVia::parse(req_str(obj, "asked_via")?)?;
            let question_sha256 = req_sha256(obj, "question_sha256")?;
            let result_count = Count(req_u64(obj, "result_count")?);
            let outcome = Word::from_closed(req_str(obj, "outcome")?, ASK_OUTCOMES)?;
            let labels = req_label_set(obj, "labels")?;
            Ok(EventPayload::Ask {
                asked,
                asked_via,
                question_sha256,
                result_count,
                outcome,
                labels,
            })
        }
        Kind::McpSession => {
            check_allowed_keys(obj, &["protocol", "client_sha256", "allow"])?;
            let protocol = Word::from_closed(req_str(obj, "protocol")?, MCP_PROTOCOLS)?;
            let client_sha256 = opt_sha256(obj, "client_sha256")?;
            let allow = req_label_allow(obj, "allow")?;
            Ok(EventPayload::McpSession {
                protocol,
                client_sha256,
                allow,
            })
        }
        Kind::McpCall => {
            check_allowed_keys(
                obj,
                &[
                    "tool",
                    "args_sha256",
                    "result_ids",
                    "dest",
                    "labels",
                    "policy",
                    "outcome",
                ],
            )?;
            let tool = Word::from_closed(req_str(obj, "tool")?, MCP_TOOLS)?;
            let args_sha256 = req_sha256(obj, "args_sha256")?;
            let result_ids = req_record_refs(obj, "result_ids")?;
            let dest = req_dest(obj, "dest")?;
            let labels = req_label_set(obj, "labels")?;
            let policy = req_sha256(obj, "policy")?;
            let outcome = Word::from_closed(req_str(obj, "outcome")?, MCP_OUTCOMES)?;
            Ok(EventPayload::McpCall {
                tool,
                args_sha256,
                result_ids,
                dest,
                labels,
                policy,
                outcome,
            })
        }
        Kind::ChatSession => {
            check_allowed_keys(obj, &["phase", "calls"])?;
            let phase = Word::from_closed(req_str(obj, "phase")?, CHAT_SESSION_PHASES)?;
            let calls = Count(req_u64(obj, "calls")?);
            Ok(EventPayload::ChatSession { phase, calls })
        }
        Kind::ChatPack => {
            check_allowed_keys(obj, &["phase", "name_sha256", "count"])?;
            let phase = Word::from_closed(req_str(obj, "phase")?, CHAT_PACK_PHASES)?;
            let name_sha256 = req_sha256(obj, "name_sha256")?;
            let count = Count(req_u64(obj, "count")?);
            Ok(EventPayload::ChatPack {
                phase,
                name_sha256,
                count,
            })
        }
        Kind::BackupBegin => {
            check_allowed_keys(obj, &["labels", "dest", "policy", "snapshot"])?;
            let labels = req_label_set(obj, "labels")?;
            let dest = req_dest(obj, "dest")?;
            let policy = req_sha256(obj, "policy")?;
            let snapshot = req_u64(obj, "snapshot")?;
            if snapshot == 0 {
                return Err(VocabError::InvalidWord("snapshot must be >= 1".to_owned()));
            }
            Ok(EventPayload::BackupBegin {
                labels,
                dest,
                policy,
                snapshot: Count(snapshot),
            })
        }
        Kind::BackupEnd => {
            check_allowed_keys(
                obj,
                &[
                    "outcome",
                    "files",
                    "bytes",
                    "manifest_sha256",
                    "journal_seq",
                    "journal_head",
                    "fingerprint",
                    "target_kind",
                ],
            )?;
            let outcome = Word::from_closed(req_str(obj, "outcome")?, OK_FAILED_OUTCOMES)?;
            let files = Count(req_u64(obj, "files")?);
            let bytes = Count(req_u64(obj, "bytes")?);
            let manifest_sha256 = req_sha256(obj, "manifest_sha256")?;
            let journal_seq = Count(req_u64(obj, "journal_seq")?);
            let journal_head = req_sha256(obj, "journal_head")?;
            let fingerprint = req_sha256(obj, "fingerprint")?;
            let target_kind = Word::from_closed(req_str(obj, "target_kind")?, BACKUP_TARGET_KINDS)?;
            Ok(EventPayload::BackupEnd {
                outcome,
                files,
                bytes,
                manifest_sha256,
                journal_seq,
                journal_head,
                fingerprint,
                target_kind,
            })
        }
        Kind::BackupVerify => {
            let verification =
                Word::from_closed(req_str(obj, "verification")?, BACKUP_VERIFICATIONS)?;
            let outcome = Word::from_closed(req_str(obj, "outcome")?, OK_FAILED_OUTCOMES)?;
            let verified = if verification.as_str() == "snapshot" {
                check_allowed_keys(
                    obj,
                    &[
                        "verification",
                        "outcome",
                        "target_sha256",
                        "snapshot",
                        "manifest_sha256",
                    ],
                )?;
                let target_sha256 = req_sha256(obj, "target_sha256")?;
                let snapshot = req_u64(obj, "snapshot")?;
                if snapshot == 0 {
                    return Err(VocabError::InvalidWord("snapshot must be >= 1".to_owned()));
                }
                let manifest_sha256 = req_sha256(obj, "manifest_sha256")?;
                BackupVerified::Snapshot {
                    target_sha256,
                    snapshot: Count(snapshot),
                    manifest_sha256,
                }
            } else {
                check_allowed_keys(
                    obj,
                    &[
                        "verification",
                        "outcome",
                        "path_sha256",
                        "public_support_sha256",
                    ],
                )?;
                let path_sha256 = req_sha256(obj, "path_sha256")?;
                let public_support_sha256 = opt_sha256(obj, "public_support_sha256")?;
                if public_support_sha256.is_some() && outcome.as_str() != "ok" {
                    return Err(VocabError::InvalidWord(
                        "public_support_sha256 is permitted only with raw outcome = ok".to_owned(),
                    ));
                }
                BackupVerified::Raw {
                    path_sha256,
                    public_support_sha256,
                }
            };
            Ok(EventPayload::BackupVerify { outcome, verified })
        }
        Kind::BackupPrune => {
            check_allowed_keys(obj, &["snapshots", "reason"])?;
            let Some(RawJsonValue::Array(arr)) = obj.get("snapshots") else {
                return Err(VocabError::InvalidWord("missing snapshots".to_owned()));
            };
            let mut snapshots = Vec::with_capacity(arr.len());
            for item in arr {
                let RawJsonValue::Uint(n) = item else {
                    return Err(VocabError::InvalidWord(
                        "snapshot must be an integer".to_owned(),
                    ));
                };
                if *n == 0 {
                    return Err(VocabError::InvalidWord("snapshot must be >= 1".to_owned()));
                }
                snapshots.push(Count(*n));
            }
            let reason = Word::from_closed(req_str(obj, "reason")?, BACKUP_PRUNE_REASONS)?;
            Ok(EventPayload::BackupPrune { snapshots, reason })
        }
        Kind::RestoreBegin => {
            check_allowed_keys(obj, &["snapshot", "mode", "labels", "dest", "policy"])?;
            let snapshot = req_u64(obj, "snapshot")?;
            if snapshot == 0 {
                return Err(VocabError::InvalidWord("snapshot must be >= 1".to_owned()));
            }
            let mode = Word::from_closed(req_str(obj, "mode")?, RESTORE_MODES)?;
            let labels = req_label_set(obj, "labels")?;
            let dest = opt_dest(obj, "dest")?;
            let policy = opt_sha256(obj, "policy")?;
            if dest.is_some() != policy.is_some() {
                return Err(VocabError::InvalidWord(
                    "restore.begin requires dest and policy together".to_owned(),
                ));
            }
            Ok(EventPayload::RestoreBegin {
                snapshot: Count(snapshot),
                mode,
                labels,
                dest,
                policy,
            })
        }
        Kind::RestoreEnd => {
            check_allowed_keys(obj, &["snapshot", "mode", "outcome", "code"])?;
            let snapshot = req_u64(obj, "snapshot")?;
            if snapshot == 0 {
                return Err(VocabError::InvalidWord("snapshot must be >= 1".to_owned()));
            }
            let mode = Word::from_closed(req_str(obj, "mode")?, RESTORE_MODES)?;
            let outcome = Word::from_closed(req_str(obj, "outcome")?, OK_FAILED_OUTCOMES)?;
            let code = opt_code(obj, "code")?;
            Ok(EventPayload::RestoreEnd {
                snapshot: Count(snapshot),
                mode,
                outcome,
                code,
            })
        }
        Kind::SettingsChange => {
            check_allowed_keys(obj, &["names", "baseline"])?;
            let Some(RawJsonValue::Array(names_arr)) = obj.get("names") else {
                return Err(VocabError::InvalidWord("missing names array".to_owned()));
            };
            let mut names = Vec::with_capacity(names_arr.len());
            for item in names_arr {
                let RawJsonValue::Str(s) = item else {
                    return Err(VocabError::InvalidWord(
                        "settings group name must be a string".to_owned(),
                    ));
                };
                names.push(SettingsGroup::parse(s)?);
            }
            for pair in names.windows(2) {
                if pair[0] >= pair[1] {
                    return Err(VocabError::InvalidWord(
                        "settings.change names must be sorted and distinct".to_owned(),
                    ));
                }
            }
            let baseline = parse_settings_baseline(obj)?;
            Ok(EventPayload::SettingsChange { names, baseline })
        }
        Kind::Purge => {
            check_allowed_keys(obj, &["what", "count"])?;
            let what = Word::from_closed(req_str(obj, "what")?, PURGE_TARGETS)?;
            let count = Count(req_u64(obj, "count")?);
            Ok(EventPayload::Purge { what, count })
        }
        Kind::PolicyChange => {
            check_allowed_keys(
                obj,
                &[
                    "policy", "prev", "revision", "outcome", "labels_n", "rows_n",
                ],
            )?;
            let policy = req_sha256(obj, "policy")?;
            let prev = req_sha256(obj, "prev")?;
            let revision = Count(req_u64(obj, "revision")?);
            let outcome = Word::from_closed(req_str(obj, "outcome")?, POLICY_OUTCOMES)?;
            let labels_n = Count(req_u64(obj, "labels_n")?);
            let rows_n = Count(req_u64(obj, "rows_n")?);
            Ok(EventPayload::PolicyChange {
                policy,
                prev,
                revision,
                outcome,
                labels_n,
                rows_n,
            })
        }
        Kind::LabelChange => {
            check_allowed_keys(obj, &["ids", "added", "removed", "origin"])?;
            let ids = req_record_refs(obj, "ids")?;
            if ids.is_empty() {
                return Err(VocabError::InvalidWord(
                    "label.change ids must not be empty".to_owned(),
                ));
            }
            let added = req_label_allow(obj, "added")?;
            let removed = req_label_allow(obj, "removed")?;
            let origin = Word::from_closed(req_str(obj, "origin")?, LABEL_ORIGINS)?;
            Ok(EventPayload::LabelChange {
                ids,
                added,
                removed,
                origin,
            })
        }
        Kind::AuditExport => {
            check_allowed_keys(obj, &["from_seq", "to_seq", "head", "policy", "store_uid"])?;
            let from_seq = req_u64(obj, "from_seq")?;
            let to_seq = req_u64(obj, "to_seq")?;
            if from_seq == 0 || to_seq < from_seq {
                return Err(VocabError::InvalidWord(
                    "invalid audit.export sequence range".to_owned(),
                ));
            }
            let head = req_sha256(obj, "head")?;
            let policy = req_sha256(obj, "policy")?;
            let store_uid = StoreUid::parse(req_str(obj, "store_uid")?)?;
            Ok(EventPayload::AuditExport {
                from_seq: Count(from_seq),
                to_seq: Count(to_seq),
                head,
                policy,
                store_uid,
            })
        }
        Kind::StoreMissing => {
            check_allowed_keys(obj, &["store_uid"])?;
            let store_uid = StoreUid::parse(req_str(obj, "store_uid")?)?;
            Ok(EventPayload::StoreMissing { store_uid })
        }
        Kind::StoreAdopted => {
            check_allowed_keys(obj, &["store_uid", "mode", "writer"])?;
            let store_uid = StoreUid::parse(req_str(obj, "store_uid")?)?;
            let mode = Word::from_closed(req_str(obj, "mode")?, ADOPT_MODES)?;
            let writer = req_u64(obj, "writer")?;
            if writer > 9999 {
                return Err(VocabError::InvalidWord("writer out of range".to_owned()));
            }
            Ok(EventPayload::StoreAdopted {
                store_uid,
                mode,
                writer: Count(writer),
            })
        }
        Kind::SyncBegin => {
            check_allowed_keys(
                obj,
                &["system", "stream", "adapter", "root_sha256", "policy"],
            )?;
            let system = StreamKey::parse(req_str(obj, "system")?)?;
            let stream = StreamKey::parse(req_str(obj, "stream")?)?;
            let adapter = Word::from_closed(req_str(obj, "adapter")?, SYNC_ADAPTERS)?;
            let root_sha256 = opt_sha256(obj, "root_sha256")?;
            let policy = req_sha256(obj, "policy")?;
            Ok(EventPayload::SyncBegin {
                system,
                stream,
                adapter,
                root_sha256,
                policy,
            })
        }
        Kind::SyncEnd => {
            check_allowed_keys(
                obj,
                &[
                    "system", "stream", "scope", "seen", "proposed", "present", "skipped",
                    "refused",
                ],
            )?;
            let system = StreamKey::parse(req_str(obj, "system")?)?;
            let stream = StreamKey::parse(req_str(obj, "stream")?)?;
            let scope = Word::from_closed(req_str(obj, "scope")?, SYNC_SCOPES)?;
            let seen = Count(req_u64(obj, "seen")?);
            let proposed = Count(req_u64(obj, "proposed")?);
            let present = Count(req_u64(obj, "present")?);
            let skipped = Count(req_u64(obj, "skipped")?);
            let refused = Count(req_u64(obj, "refused")?);
            Ok(EventPayload::SyncEnd {
                system,
                stream,
                scope,
                seen,
                proposed,
                present,
                skipped,
                refused,
            })
        }
    }
}

fn req_str<'a>(obj: &'a RawEventObject, key: &str) -> Result<&'a str, VocabError> {
    match obj.get(key) {
        Some(RawJsonValue::Str(s)) => Ok(s.as_str()),
        _ => Err(VocabError::InvalidWord(format!(
            "missing or non-string `{key}`"
        ))),
    }
}

fn req_u64(obj: &RawEventObject, key: &str) -> Result<u64, VocabError> {
    match obj.get(key) {
        Some(RawJsonValue::Uint(n)) => Ok(*n),
        _ => Err(VocabError::InvalidWord(format!(
            "missing or non-integer `{key}`"
        ))),
    }
}

fn req_sha256(obj: &RawEventObject, key: &str) -> Result<Sha256, VocabError> {
    let s = req_str(obj, key)?;
    Sha256::parse(s).map_err(|_| VocabError::InvalidSha256(s.to_owned()))
}

fn opt_sha256(obj: &RawEventObject, key: &str) -> Result<Option<Sha256>, VocabError> {
    match obj.get(key) {
        None => Ok(None),
        Some(RawJsonValue::Str(s)) => Sha256::parse(s)
            .map(Some)
            .map_err(|_| VocabError::InvalidSha256(s.clone())),
        Some(_) => Err(VocabError::InvalidSha256(key.to_owned())),
    }
}

fn req_dest(obj: &RawEventObject, key: &str) -> Result<DestKind, VocabError> {
    let s = req_str(obj, key)?;
    DestKind::parse(s).ok_or_else(|| VocabError::InvalidDest(s.to_owned()))
}

fn opt_dest(obj: &RawEventObject, key: &str) -> Result<Option<DestKind>, VocabError> {
    match obj.get(key) {
        None => Ok(None),
        Some(RawJsonValue::Str(s)) => DestKind::parse(s)
            .map(Some)
            .ok_or_else(|| VocabError::InvalidDest(s.clone())),
        Some(_) => Err(VocabError::InvalidDest(key.to_owned())),
    }
}

fn req_code(obj: &RawEventObject, key: &str) -> Result<Code, VocabError> {
    let s = req_str(obj, key)?;
    Code::parse(s).map_err(|_| VocabError::InvalidId(s.to_owned()))
}

fn opt_code(obj: &RawEventObject, key: &str) -> Result<Option<Code>, VocabError> {
    match obj.get(key) {
        None => Ok(None),
        Some(RawJsonValue::Str(s)) => Code::parse(s)
            .map(Some)
            .map_err(|_| VocabError::InvalidId(s.clone())),
        Some(_) => Err(VocabError::InvalidId(key.to_owned())),
    }
}

fn req_label_set(obj: &RawEventObject, key: &str) -> Result<LabelSet, VocabError> {
    let Some(RawJsonValue::Array(arr)) = obj.get(key) else {
        return Err(VocabError::InvalidLabels(key.to_owned()));
    };
    if arr.is_empty() {
        return Err(VocabError::InvalidLabels(
            "labels must not be empty".to_owned(),
        ));
    }
    let mut ids = Vec::with_capacity(arr.len());
    for item in arr {
        let RawJsonValue::Str(s) = item else {
            return Err(VocabError::InvalidLabels(key.to_owned()));
        };
        let id = LabelId::parse(s).map_err(|_| VocabError::InvalidLabels(s.clone()))?;
        ids.push(id);
    }
    for pair in ids.windows(2) {
        if pair[0] >= pair[1] {
            return Err(VocabError::InvalidLabels(
                "labels must be sorted and distinct".to_owned(),
            ));
        }
    }
    let set = LabelSet::from_ids(ids.clone());
    if set.ids() != ids.as_slice() {
        return Err(VocabError::InvalidLabels(
            "labels must be normalized".to_owned(),
        ));
    }
    Ok(set)
}

fn opt_label_set(obj: &RawEventObject, key: &str) -> Result<Option<LabelSet>, VocabError> {
    if obj.get(key).is_none() {
        Ok(None)
    } else {
        req_label_set(obj, key).map(Some)
    }
}

fn req_label_allow(obj: &RawEventObject, key: &str) -> Result<LabelAllow, VocabError> {
    let Some(RawJsonValue::Array(arr)) = obj.get(key) else {
        return Err(VocabError::InvalidLabels(key.to_owned()));
    };
    let mut ids = Vec::with_capacity(arr.len());
    for item in arr {
        let RawJsonValue::Str(s) = item else {
            return Err(VocabError::InvalidLabels(key.to_owned()));
        };
        let id = LabelId::parse(s).map_err(|_| VocabError::InvalidLabels(s.clone()))?;
        ids.push(id);
    }
    for pair in ids.windows(2) {
        if pair[0] >= pair[1] {
            return Err(VocabError::InvalidLabels(
                "allow labels must be sorted and distinct".to_owned(),
            ));
        }
    }
    Ok(LabelAllow::from_ids(ids))
}

fn req_record_refs(obj: &RawEventObject, key: &str) -> Result<Vec<RecordRef>, VocabError> {
    let Some(RawJsonValue::Array(arr)) = obj.get(key) else {
        return Err(VocabError::InvalidRecordRef(key.to_owned()));
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let RawJsonValue::Str(s) = item else {
            return Err(VocabError::InvalidRecordRef(key.to_owned()));
        };
        out.push(RecordRef::parse(s)?);
    }
    Ok(out)
}

fn req_sorted_record_refs(obj: &RawEventObject, key: &str) -> Result<Vec<RecordRef>, VocabError> {
    let refs = req_record_refs(obj, key)?;
    for pair in refs.windows(2) {
        if pair[0] >= pair[1] {
            return Err(VocabError::InvalidRecordRef(
                "affected record references must be sorted and distinct".to_owned(),
            ));
        }
    }
    Ok(refs)
}

fn req_store_uids(obj: &RawEventObject, key: &str) -> Result<Vec<StoreUid>, VocabError> {
    let Some(RawJsonValue::Array(arr)) = obj.get(key) else {
        return Err(VocabError::InvalidStoreUid(key.to_owned()));
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let RawJsonValue::Str(s) = item else {
            return Err(VocabError::InvalidStoreUid(key.to_owned()));
        };
        out.push(StoreUid::parse(s)?);
    }
    Ok(out)
}

fn opt_file_hashes(obj: &RawEventObject, key: &str) -> Result<Option<Vec<FileHash>>, VocabError> {
    let Some(val) = obj.get(key) else {
        return Ok(None);
    };
    let RawJsonValue::Array(arr) = val else {
        return Err(VocabError::InvalidWord(
            "file_hashes must be an array".to_owned(),
        ));
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let RawJsonValue::Object(entries) = item else {
            return Err(VocabError::InvalidWord(
                "file_hashes entry must be an object".to_owned(),
            ));
        };
        if entries.len() != 2 {
            return Err(VocabError::InvalidWord(
                "file_hashes entry must have path_sha256 and sha256".to_owned(),
            ));
        }
        let inner = RawEventObject(entries.clone());
        let path_sha256 = req_sha256(&inner, "path_sha256")?;
        let sha256 = req_sha256(&inner, "sha256")?;
        out.push(FileHash {
            path_sha256,
            sha256,
        });
    }
    for pair in out.windows(2) {
        if pair[0].path_sha256 >= pair[1].path_sha256 {
            return Err(VocabError::InvalidWord(
                "file_hashes must be sorted and have unique path_sha256".to_owned(),
            ));
        }
    }
    Ok(Some(out))
}

fn parse_settings_baseline(obj: &RawEventObject) -> Result<SettingsBaseline, VocabError> {
    let Some(RawJsonValue::Object(entries)) = obj.get("baseline") else {
        return Err(VocabError::InvalidWord(
            "missing baseline object".to_owned(),
        ));
    };
    let base_obj = RawEventObject(entries.clone());
    for (k, _) in &base_obj.0 {
        if !matches!(k.as_str(), "schema" | "present" | "file_sha256" | "groups") {
            return Err(VocabError::InvalidWord(format!(
                "unknown baseline key `{k}`"
            )));
        }
    }
    let schema_u64 = req_u64(&base_obj, "schema")?;
    let schema = u32::try_from(schema_u64)
        .map_err(|_| VocabError::InvalidWord("invalid baseline schema".to_owned()))?;
    let Some(RawJsonValue::Bool(present)) = base_obj.get("present") else {
        return Err(VocabError::InvalidWord(
            "missing baseline.present".to_owned(),
        ));
    };
    let file_sha256 = match base_obj.get("file_sha256") {
        Some(RawJsonValue::Null) => None,
        Some(RawJsonValue::Str(s)) => {
            Some(Sha256::parse(s).map_err(|_| VocabError::InvalidSha256(s.clone()))?)
        }
        _ => {
            return Err(VocabError::InvalidWord(
                "invalid baseline.file_sha256".to_owned(),
            ));
        }
    };
    let Some(RawJsonValue::Object(group_entries)) = base_obj.get("groups") else {
        return Err(VocabError::InvalidWord(
            "missing baseline.groups".to_owned(),
        ));
    };
    let mut groups = BTreeMap::new();
    for (g_key, g_val) in group_entries {
        let group = SettingsGroup::parse(g_key)?;
        let digest = match g_val {
            RawJsonValue::Null => None,
            RawJsonValue::Str(s) => {
                Some(Sha256::parse(s).map_err(|_| VocabError::InvalidSha256(s.clone()))?)
            }
            _ => {
                return Err(VocabError::InvalidWord(
                    "invalid group digest value".to_owned(),
                ));
            }
        };
        groups.insert(group, digest);
    }
    let baseline = SettingsBaseline {
        schema,
        present: *present,
        file_sha256,
        groups,
    };
    baseline.validate()?;
    Ok(baseline)
}

/// Construct the canonical synthetic [`EventPayload`] for `kind` at `seq`
/// (`crates/fieldbook-core/tests/audit-samples.jsonl`).
#[must_use]
pub fn sample_payload(kind: Kind, seq: u64) -> EventPayload {
    let floor_policy = Sha256::digest(b"fieldbook-floor-policy-1");
    match kind {
        Kind::JournalOpen => EventPayload::JournalOpen {
            writer: Count(0),
            event_schema: Count(1),
            tool_version: SemVer::parse("0.1.0").unwrap_or_else(|_| unreachable!()),
            stores: Vec::new(),
        },
        Kind::JournalRecover => EventPayload::JournalRecover {
            reason: Word::new_static("torn_tail"),
            last_seq: Count(seq.saturating_sub(1)),
            last_hash: Sha256::digest(b"sample-last-hash"),
        },
        Kind::WriteBegin => EventPayload::WriteBegin {
            verb: Verb::parse("add source").unwrap_or_else(|_| unreachable!()),
            labels: LabelSet::public(),
            files: Count(1),
            file_hashes: Some(vec![FileHash {
                path_sha256: Sha256::digest(b"registers/sources.csv"),
                sha256: Sha256::digest(b"sample-sources-csv"),
            }]),
            affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|_| unreachable!())],
            dest: None,
            path_sha256: None,
            policy: None,
        },
        Kind::WriteEnd => EventPayload::WriteEnd {
            verb: Verb::parse("add source").unwrap_or_else(|_| unreachable!()),
            labels: LabelSet::public(),
            affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|_| unreachable!())],
            outcome: Word::new_static("ok"),
            dest: None,
            path_sha256: None,
            policy: None,
        },
        Kind::Refuse => EventPayload::Refuse {
            verb: Some(Verb::parse("sync").unwrap_or_else(|_| unreachable!())),
            code: Code::Fb0901,
            reason: Word::new_static("label"),
            sha256: None,
            name_sha256: None,
            labels: Some(LabelSet::unlabelled()),
            dest: Some(DestKind::Model),
            system: Some(StreamKey::parse("finance-exports").unwrap_or_else(|_| unreachable!())),
            stream: Some(StreamKey::parse("commitments").unwrap_or_else(|_| unreachable!())),
        },
        Kind::QueuePropose => EventPayload::QueuePropose {
            labels: LabelSet::unlabelled(),
        },
        Kind::QueueAccept => EventPayload::QueueAccept {
            labels: LabelSet::public(),
        },
        Kind::QueueReject => EventPayload::QueueReject {
            labels: LabelSet::unlabelled(),
        },
        Kind::AiBegin => EventPayload::AiBegin {
            tx: Count(seq),
            purpose: Word::new_static("ask"),
            dest: DestKind::Model,
            labels: LabelSet::public(),
            policy: floor_policy,
            adapter: Word::new_static("openai-compatible@1"),
            endpoint: EndpointName::parse("local-vllm").unwrap_or_else(|_| unreachable!()),
            model: ModelId::parse("qwen2.5-14b-instruct").unwrap_or_else(|_| unreachable!()),
            template: Word::new_static("ask@1"),
            prompt_sha256: Sha256::digest(b"sample-prompt"),
            context_ids: vec![
                RecordRef::parse("corpus/CL-CMP-0001").unwrap_or_else(|_| unreachable!()),
            ],
        },
        Kind::AiCall => EventPayload::AiCall {
            tx: Count(seq.saturating_sub(1).max(1)),
            outcome: Word::new_static("ok"),
            code: None,
            attempts: Count(1),
            output_sha256: Some(Sha256::digest(b"sample-output")),
            tokens_in: Count(120),
            tokens_out: Count(45),
        },
        Kind::AiPack => EventPayload::AiPack {
            prompt_sha256: Sha256::digest(b"sample-pack-prompt"),
            context_ids: vec![
                RecordRef::parse("corpus/CL-CMP-0001").unwrap_or_else(|_| unreachable!()),
            ],
            dest: DestKind::Handoff,
            labels: LabelSet::public(),
            policy: floor_policy,
        },
        Kind::AiPaste => EventPayload::AiPaste {
            pack_seq: Some(Count(seq.saturating_sub(1).max(1))),
            labels: LabelSet::public(),
            output_sha256: Sha256::digest(b"sample-paste-output"),
        },
        Kind::Ask => EventPayload::Ask {
            asked: Some(Id::parse("A-0001").unwrap_or_else(|_| unreachable!())),
            asked_via: AskedVia::Search,
            question_sha256: Sha256::digest(b"sample-question"),
            result_count: Count(3),
            outcome: Word::new_static("answered"),
            labels: LabelSet::public(),
        },
        Kind::McpSession => EventPayload::McpSession {
            protocol: Word::new_static("2025-11-25"),
            client_sha256: Some(Sha256::digest(b"claude-desktop")),
            allow: LabelAllow::from_ids([
                LabelId::parse("public").unwrap_or_else(|_| unreachable!())
            ]),
        },
        Kind::McpCall => EventPayload::McpCall {
            tool: Word::new_static("search"),
            args_sha256: Sha256::digest(b"sample-args"),
            result_ids: vec![
                RecordRef::parse("corpus/CL-CMP-0001").unwrap_or_else(|_| unreachable!()),
            ],
            dest: DestKind::Mcp,
            labels: LabelSet::public(),
            policy: floor_policy,
            outcome: Word::new_static("ok"),
        },
        Kind::ChatSession => EventPayload::ChatSession {
            phase: Word::new_static("start"),
            calls: Count(0),
        },
        Kind::ChatPack => EventPayload::ChatPack {
            phase: Word::new_static("save"),
            name_sha256: Sha256::digest(b"sample-pack-name"),
            count: Count(2),
        },
        Kind::BackupBegin => EventPayload::BackupBegin {
            labels: LabelSet::public(),
            dest: DestKind::Backup,
            policy: floor_policy,
            snapshot: Count(1),
        },
        Kind::BackupEnd => EventPayload::BackupEnd {
            outcome: Word::new_static("ok"),
            files: Count(10),
            bytes: Count(4096),
            manifest_sha256: Sha256::digest(b"sample-manifest"),
            journal_seq: Count(seq.saturating_sub(1).max(1)),
            journal_head: Sha256::digest(b"sample-head"),
            fingerprint: Sha256::digest(b"sample-fingerprint"),
            target_kind: Word::new_static("folder"),
        },
        Kind::BackupVerify => EventPayload::BackupVerify {
            outcome: Word::new_static("ok"),
            verified: BackupVerified::Snapshot {
                target_sha256: Sha256::digest(b"sample-target"),
                snapshot: Count(1),
                manifest_sha256: Sha256::digest(b"sample-manifest"),
            },
        },
        Kind::BackupPrune => EventPayload::BackupPrune {
            snapshots: vec![Count(1)],
            reason: Word::new_static("retention"),
        },
        Kind::RestoreBegin => EventPayload::RestoreBegin {
            snapshot: Count(1),
            mode: Word::new_static("drill"),
            labels: LabelSet::public(),
            dest: None,
            policy: None,
        },
        Kind::RestoreEnd => EventPayload::RestoreEnd {
            snapshot: Count(1),
            mode: Word::new_static("drill"),
            outcome: Word::new_static("ok"),
            code: None,
        },
        Kind::SettingsChange => {
            let (_, baseline) = super::parse_settings_role_and_baseline(Some(b"role = \"lead\"\n"))
                .unwrap_or_else(|_| unreachable!());
            EventPayload::SettingsChange {
                names: vec![SettingsGroup::Role],
                baseline,
            }
        }
        Kind::Purge => EventPayload::Purge {
            what: Word::new_static("asked"),
            count: Count(4),
        },
        Kind::PolicyChange => EventPayload::PolicyChange {
            policy: Sha256::digest(b"sample-policy"),
            prev: floor_policy,
            revision: Count(1),
            outcome: Word::new_static("loaded"),
            labels_n: Count(3),
            rows_n: Count(8),
        },
        Kind::LabelChange => EventPayload::LabelChange {
            ids: vec![RecordRef::parse("overlay/SO-100001").unwrap_or_else(|_| unreachable!())],
            added: LabelAllow::from_ids([
                LabelId::parse("np-0007").unwrap_or_else(|_| unreachable!())
            ]),
            removed: LabelAllow::from_ids([
                LabelId::parse("unlabelled").unwrap_or_else(|_| unreachable!())
            ]),
            origin: Word::new_static("person"),
        },
        Kind::AuditExport => EventPayload::AuditExport {
            from_seq: Count(1),
            to_seq: Count(seq.saturating_sub(1).max(1)),
            head: Sha256::digest(b"sample-export-head"),
            policy: Sha256::digest(b"sample-policy"),
            store_uid: StoreUid::nil(),
        },
        Kind::StoreMissing => EventPayload::StoreMissing {
            store_uid: StoreUid::parse("0195ef40-7c00-7000-8000-000000000001")
                .unwrap_or_else(|_| unreachable!()),
        },
        Kind::StoreAdopted => EventPayload::StoreAdopted {
            store_uid: StoreUid::parse("0195ef40-7c00-7000-8000-000000000001")
                .unwrap_or_else(|_| unreachable!()),
            mode: Word::new_static("writer"),
            writer: Count(2),
        },
        Kind::SyncBegin => EventPayload::SyncBegin {
            system: StreamKey::parse("finance-exports").unwrap_or_else(|_| unreachable!()),
            stream: StreamKey::parse("commitments").unwrap_or_else(|_| unreachable!()),
            adapter: Word::new_static("file-drop@1"),
            root_sha256: Some(Sha256::digest(b"sample-sync-root")),
            policy: Sha256::digest(b"sample-policy"),
        },
        Kind::SyncEnd => EventPayload::SyncEnd {
            system: StreamKey::parse("finance-exports").unwrap_or_else(|_| unreachable!()),
            stream: StreamKey::parse("commitments").unwrap_or_else(|_| unreachable!()),
            scope: Word::new_static("incremental"),
            seen: Count(10),
            proposed: Count(2),
            present: Count(8),
            skipped: Count(0),
            refused: Count(0),
        },
    }
}
