//! Saved call records and Handoff packs (`model-gate` step 3).
//!
//! A call record is written only when the `state` store holds every label of the request and the
//! effective retention is on. A write that fails is a warning, never a failure of the call.
//! `resume` restores the request facts from the saved ticket, not from the caller's input.

#![expect(
    clippy::disallowed_methods,
    reason = "the call store reads and removes its own files under the state root"
)]

use std::path::{Path, PathBuf};

use fieldbook_core::audit::{Count, EventPayload, Sha256, Via, Word};
use fieldbook_core::labels::{ItemRef, LabelId, LabelSet, StoreId, UNLABELLED};
use fieldbook_core::ports::{HandoffId, HandoffTicket, Template};
use fieldbook_core::schema_json::SchemaToml;

use crate::World;
use crate::adapters::model::{CallRecord, CallStore, StoredTicket, TicketMeta};
use crate::io::Disk;
use crate::io::atomic;
use crate::io::journal::Journal;

/// A call store over the state folder of a [`Disk`].
#[derive(Debug, Clone)]
pub struct DiskCallStore {
    disk: Disk,
}

impl DiskCallStore {
    /// Constructs a store over `disk`.
    #[must_use]
    pub const fn new(disk: Disk) -> Self {
        Self { disk }
    }

    fn state_dir(&self) -> &Path {
        self.disk.state_dir()
    }

    fn write_state_file(&self, rel: &str, bytes: &[u8]) -> Result<(), String> {
        let lease = self
            .disk
            .write_lease()
            .map_err(|err| format!("warning: {err}"))?;
        let journal = Journal::open(&self.disk, self.state_dir(), Via::Cli)
            .map_err(|err| format!("warning: {err}"))?;
        let audited = journal
            .state_scope(&lease)
            .map_err(|err| format!("warning: {err}"))?;
        atomic::write_in_root(&self.disk.roots().state, Path::new(rel), bytes, &audited)
            .map(|_| ())
            .map_err(|err| format!("warning: {err}"))
    }

    fn call_records(&self) -> Vec<(PathBuf, CallRecordFile)> {
        let dir = self.state_dir().join("ai/calls");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut records = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(record) = serde_json::from_str::<CallRecordFile>(&text) {
                records.push((path, record));
            }
        }
        records
    }
}

impl CallStore for DiskCallStore {
    fn put_call(&self, record: &CallRecord) -> Result<(), String> {
        let rule = fieldbook_core::labels::StoreRule::floor(StoreId::State, false);
        if fieldbook_core::labels::admit(&record.labels, &rule).is_err() {
            return Err(
                "warning: the call record was not written (the state store cannot hold the request's labels)"
                    .to_owned(),
            );
        }
        if record.keep_days == 0 {
            return Err("warning: the call record was not written (retention is off)".to_owned());
        }

        self.purge(record);

        let next = self
            .call_records()
            .iter()
            .filter_map(|(_, file)| file.seq)
            .max()
            .unwrap_or(0)
            + 1;
        let file = CallRecordFile::from_record(next, record);
        let text = serde_json::to_string_pretty(&file)
            .map_err(|_| "warning: the call record could not be serialized".to_owned())?;
        self.write_state_file(&format!("ai/calls/C-{next:04}.json"), text.as_bytes())
    }

    fn put_pack(&self, pack: &[u8], meta: &TicketMeta) -> Result<HandoffTicket, String> {
        let id = next_handoff_id(self.state_dir())
            .ok_or_else(|| "warning: the handoff folder could not be read".to_owned())?;
        let ticket = HandoffTicket::new(id, Sha256::digest(pack));
        let name = id.format();
        self.write_state_file(&format!("handoff/{name}.txt"), pack)?;
        let file = TicketFile {
            ticket_version: 1,
            id: name.clone(),
            pack_seq: meta.pack_seq,
            template: meta.template.clone(),
            used: false,
            labels: meta
                .labels
                .ids()
                .iter()
                .map(LabelId::as_str)
                .map(str::to_owned)
                .collect(),
            context_ids: meta.context_ids.clone(),
            inputs: meta
                .inputs
                .iter()
                .map(|item| InputFile {
                    store: item.store.as_str().to_owned(),
                    id: item.id.clone(),
                })
                .collect(),
            response_schema: meta.response_schema.clone().map(SchemaToml),
        };
        let text = toml::to_string(&file)
            .map_err(|_| "warning: the ticket could not be serialized".to_owned())?;
        self.write_state_file(&format!("handoff/{name}.toml"), text.as_bytes())?;
        Ok(ticket)
    }

    fn get_ticket(&self, id: HandoffId) -> Result<Option<StoredTicket>, String> {
        let path = self
            .state_dir()
            .join(format!("handoff/{}.toml", id.format()));
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(format!("warning: {err}")),
        };
        let file: TicketFile =
            toml::from_str(&text).map_err(|_| "warning: malformed ticket metadata".to_owned())?;
        if file.ticket_version != 1
            || file.id != id.format()
            || file.pack_seq == 0
            || Template::new(&file.template).is_err()
            || file.context_ids.iter().any(String::is_empty)
        {
            return Err("warning: malformed ticket metadata".to_owned());
        }
        let response_schema = file.response_schema.map(|s| s.0);
        let labels = parse_labels(&file.labels)
            .ok_or_else(|| "warning: malformed ticket labels".to_owned())?;
        let inputs = file
            .inputs
            .iter()
            .map(|input| {
                if input.id.is_empty() {
                    return Err("warning: malformed ticket input".to_owned());
                }
                StoreId::from_root_name(&input.store)
                    .map(|store| ItemRef::new(store, input.id.clone()))
                    .ok_or_else(|| "warning: malformed ticket input".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(StoredTicket {
            id,
            meta: TicketMeta {
                response_schema,
                pack_seq: file.pack_seq,
                template: file.template,
                labels,
                context_ids: file.context_ids,
                inputs,
            },
            used: file.used,
        }))
    }

    fn mark_used(&self, id: HandoffId) -> Result<(), String> {
        let path = self
            .state_dir()
            .join(format!("handoff/{}.toml", id.format()));
        let text = std::fs::read_to_string(&path).map_err(|err| format!("warning: {err}"))?;
        let mut file: TicketFile =
            toml::from_str(&text).map_err(|_| "warning: malformed ticket metadata".to_owned())?;
        file.used = true;
        let text = toml::to_string(&file)
            .map_err(|_| "warning: the ticket could not be serialized".to_owned())?;
        self.write_state_file(&format!("handoff/{}.toml", id.format()), text.as_bytes())
    }
}

impl DiskCallStore {
    fn purge(&self, record: &CallRecord) {
        let today = days_from_civil(record.now.date);
        let mut removed = 0u64;
        for (path, file) in self.call_records() {
            let Some(written) = file.written_on.as_deref() else {
                continue;
            };
            let Ok(date) = fieldbook_core::CivilDate::parse(written) else {
                continue;
            };
            let age = today.saturating_sub(days_from_civil(date));
            if age > u64::from(file.keep_days.unwrap_or(record.keep_days))
                && std::fs::remove_file(&path).is_ok()
            {
                removed += 1;
            }
        }
        if removed > 0
            && let Ok(lease) = self.disk.write_lease()
            && let Ok(journal) = Journal::open(&self.disk, self.state_dir(), Via::Cli)
        {
            let _ = journal.append_payload(EventPayload::Purge {
                what: Word::new_static("calls"),
                count: Count(removed),
            });
            drop(lease);
        }
    }
}

fn next_handoff_id(state_dir: &Path) -> Option<HandoffId> {
    let dir = state_dir.join("handoff");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => Some(entries),
        // No folder yet: the first ticket. Any other failure must not reuse id 1.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return None,
    };
    let max = entries
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter_map(|name| name.strip_suffix(".toml").map(str::to_owned))
        .filter_map(|stem| HandoffId::parse(&stem).ok())
        .map(HandoffId::value)
        .max()
        .unwrap_or(0);
    HandoffId::new(max + 1)
}

fn parse_labels(labels: &[String]) -> Option<LabelSet> {
    if labels.is_empty() {
        return None;
    }
    let ids = labels
        .iter()
        .map(|id| LabelId::parse(id).ok())
        .collect::<Option<Vec<_>>>()?;
    let set = LabelSet::from_ids(ids);
    (!set.contains(&UNLABELLED)).then_some(set)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CallRecordFile {
    seq: Option<u64>,
    adapter: String,
    endpoint: String,
    model: String,
    template: String,
    purpose: String,
    labels: Vec<String>,
    inputs: Vec<InputFile>,
    policy: String,
    prompt_sha256: String,
    output_sha256: Option<String>,
    system: String,
    prompt: String,
    reply: String,
    tokens_in: u64,
    tokens_out: u64,
    attempts: u64,
    outcome: String,
    keep_days: Option<u32>,
    written_on: Option<String>,
}

impl CallRecordFile {
    fn from_record(seq: u64, record: &CallRecord) -> Self {
        Self {
            seq: Some(seq),
            adapter: record.adapter.clone(),
            endpoint: record.endpoint.clone(),
            model: record.model.clone(),
            template: record.template.clone(),
            purpose: record.purpose.clone(),
            labels: record
                .labels
                .ids()
                .iter()
                .map(LabelId::as_str)
                .map(str::to_owned)
                .collect(),
            inputs: record
                .inputs
                .iter()
                .map(|item| InputFile {
                    store: item.store.as_str().to_owned(),
                    id: item.id.clone(),
                })
                .collect(),
            policy: record.policy.as_str().to_owned(),
            prompt_sha256: record.prompt_sha256.as_str().to_owned(),
            output_sha256: record
                .output_sha256
                .as_ref()
                .map(|sha| sha.as_str().to_owned()),
            system: record.system.clone(),
            prompt: record.prompt.clone(),
            reply: record.reply.clone(),
            tokens_in: record.tokens_in,
            tokens_out: record.tokens_out,
            attempts: record.attempts,
            outcome: record.outcome.clone(),
            keep_days: Some(record.keep_days),
            written_on: Some(record.now.date.to_string()),
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InputFile {
    store: String,
    id: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TicketFile {
    ticket_version: u32,
    id: String,
    pack_seq: u64,
    template: String,
    used: bool,
    labels: Vec<String>,
    context_ids: Vec<String>,
    inputs: Vec<InputFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    response_schema: Option<SchemaToml>,
}

const fn days_from_civil(date: fieldbook_core::CivilDate) -> u64 {
    let y = date.year() as i64 - if date.month() <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = date.month() as i64;
    let d = date.day() as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468).cast_unsigned()
}
