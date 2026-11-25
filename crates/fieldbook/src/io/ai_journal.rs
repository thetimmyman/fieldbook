//! The gate's journal writer (`model-gate` step 3).
//!
//! Each event opens the journal, appends one line and drops the handle, so the journal lock is
//! held for one append and never across a model call. The chain stays whole because the next open
//! reads the last hash.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fieldbook_core::audit::kind::{AI_ADAPTERS, AI_PURPOSES, AI_TEMPLATES};
use fieldbook_core::audit::{Count, DestKind, Event, EventPayload, LabelSet, Sha256, Via, Word};
use fieldbook_core::check::Code;

use crate::World;
use crate::adapters::model::{AiJournal, BeginRecord, CallEvent, ErrorKind, ModelError};
use crate::io::journal::{self, Journal};

/// A journal-backed gate writer over one state folder.
pub struct AiJournalWriter {
    world: Arc<dyn World + Send + Sync>,
    state_dir: PathBuf,
}

impl AiJournalWriter {
    /// Constructs a writer over `state_dir`.
    #[must_use]
    pub fn new(world: Arc<dyn World + Send + Sync>, state_dir: impl Into<PathBuf>) -> Self {
        Self {
            world,
            state_dir: state_dir.into(),
        }
    }

    fn open(&self) -> Result<Journal<'_>, ModelError> {
        Journal::open(self.world.as_ref(), &self.state_dir, Via::Cli).map_err(|_| begin_error())
    }
}

const fn begin_error() -> ModelError {
    ModelError::new(Code::Fb0903, ErrorKind::Availability)
}

const fn finish_error() -> ModelError {
    ModelError::new(Code::Fb0904, ErrorKind::Availability)
}

/// Reads the last sequence number of a journal under `state_dir`.
fn last_seq(state_dir: &Path) -> Option<u64> {
    let audit_dir = state_dir.join("audit");
    let segments = journal::list_segments(&audit_dir).ok()?;
    let mut last = None;
    for (_, path) in segments {
        let bytes = journal::read_segment_bytes(&path).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines().rev() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok((event, _)) = Event::parse_line(line) {
                last = Some(event.seq);
                break;
            }
        }
    }
    last
}

impl AiJournal for AiJournalWriter {
    fn begin(&self, record: BeginRecord) -> Result<u64, ModelError> {
        let journal = self.open()?;
        let next = last_seq(&self.state_dir).unwrap_or(0) + 1;
        let payload = match record {
            BeginRecord::Model(event) => EventPayload::AiBegin {
                tx: Count(next),
                purpose: Word::from_closed(event.purpose.as_str(), AI_PURPOSES)
                    .map_err(|_| begin_error())?,
                dest: DestKind::Model,
                labels: event.labels,
                policy: event.policy,
                adapter: Word::from_closed(&event.adapter, AI_ADAPTERS)
                    .map_err(|_| begin_error())?,
                endpoint: event.endpoint,
                model: event.model,
                template: Word::from_closed(&event.template, AI_TEMPLATES)
                    .map_err(|_| begin_error())?,
                prompt_sha256: event.prompt_sha256,
                context_ids: event.context_ids,
            },
            BeginRecord::Handoff(event) => EventPayload::AiPack {
                prompt_sha256: event.prompt_sha256,
                context_ids: event.context_ids,
                dest: DestKind::Handoff,
                labels: event.labels,
                policy: event.policy,
            },
        };
        journal.append_payload(payload).map_err(|_| begin_error())?;
        last_seq(&self.state_dir).ok_or_else(begin_error)
    }

    fn finish(&self, tx: u64, event: CallEvent) -> Result<(), ModelError> {
        let journal = self.open()?;
        journal
            .append_payload(EventPayload::AiCall {
                tx: Count(tx),
                outcome: Word::new_static(event.outcome),
                code: event.code,
                attempts: Count(event.attempts),
                output_sha256: event.output_sha256,
                tokens_in: Count(event.tokens_in),
                tokens_out: Count(event.tokens_out),
            })
            .map(|_| ())
            .map_err(|_| finish_error())
    }

    fn refuse(
        &self,
        dest: Option<DestKind>,
        labels: &LabelSet,
        code: Code,
        reason: Word,
    ) -> Result<(), ModelError> {
        let journal = self.open()?;
        journal
            .append_payload(EventPayload::Refuse {
                code,
                reason,
                sha256: None,
                name_sha256: None,
                labels: Some(labels.clone()),
                dest,
                verb: None,
                system: None,
                stream: None,
            })
            .map(|_| ())
            .map_err(|_| begin_error())
    }

    fn paste(
        &self,
        pack_seq: Option<u64>,
        output_sha256: Sha256,
        labels: &LabelSet,
    ) -> Result<(), ModelError> {
        let journal = self.open()?;
        journal
            .append_payload(EventPayload::AiPaste {
                pack_seq: pack_seq.map(Count),
                output_sha256,
                labels: labels.clone(),
            })
            .map(|_| ())
            .map_err(|_| begin_error())
    }

    fn audited(&self) -> bool {
        !self.state_dir.join("audit").is_file()
    }
}
