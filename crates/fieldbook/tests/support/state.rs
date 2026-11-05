//! Builder and fault-planter for `tests/fixtures/state-mini` and
//! `tests/fixtures/state-known-bad/<CODE>` (`audit-journal`).

use std::path::Path;

use fieldbook::io::journal::{Journal, cleanup_fixture_locks, write_sync_file};
use fieldbook_core::audit::{
    AskedVia, BackupVerified, Count, DestKind, EventPayload, FileHash, Id, LabelId, LabelSet,
    RecordRef, SemVer, SettingsGroup, Sha256, Verb, Via, Word, ZERO_HASH,
    parse_settings_role_and_baseline, seal_body, split_hash_suffix,
};
use fieldbook_core::check::Code;

use super::world::TestWorld;

/// Contents of `tests/fixtures/state-mini/settings.toml`.
pub const STATE_MINI_SETTINGS_TOML: &str = "role = \"lead\"\n";

/// Faults planted into `state-mini` to form `tests/fixtures/state-known-bad/<CODE>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plant {
    /// `FB0800`: one byte changed in line 5 without resealing.
    BrokenChain,
    /// `FB0801`: unknown `"note":"free text"` key added to line 6 and resealed.
    UnknownKey,
    /// `FB0803`: journal truncated after line 10 (`write.begin` with no `write.end`).
    UnclosedBegin,
    /// `FB0804`: last line (line 12) cut in half.
    TornTail,
    /// `FB0805`: line 8 timestamp moved backwards and chain resealed.
    TimeBackwards,
}

impl Plant {
    /// All five directory-backed known-bad state faults and their check codes.
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::BrokenChain, "FB0800"),
        (Self::UnknownKey, "FB0801"),
        (Self::UnclosedBegin, "FB0803"),
        (Self::TornTail, "FB0804"),
        (Self::TimeBackwards, "FB0805"),
    ];
}

fn sha(byte: u8) -> Sha256 {
    let hex = format!("{byte:02x}").repeat(32);
    Sha256::parse(&hex).expect("valid synthetic sha256")
}

/// The 12 typed event payloads that make up `tests/fixtures/state-mini/audit/journal.jsonl`.
#[must_use]
pub fn state_mini_events() -> Vec<EventPayload> {
    let (_, baseline) = parse_settings_role_and_baseline(Some(STATE_MINI_SETTINGS_TOML.as_bytes()))
        .expect("valid state-mini settings");
    let pub_labels = LabelSet::public();
    let np_labels = LabelSet::from_ids([LabelId::parse("np-0007").expect("valid company label")]);

    let s1 = RecordRef::parse("corpus/S-0001").expect("valid ref");
    let s1_v1 = RecordRef::parse("corpus/S-0001-v1").expect("valid ref");
    let c1 = RecordRef::parse("corpus/CL-CMP-0001").expect("valid ref");
    let page = RecordRef::parse("corpus/fnd-overview").expect("valid ref");

    vec![
        // 1: journal.open
        EventPayload::JournalOpen {
            writer: Count::new(0),
            event_schema: Count::new(1),
            tool_version: SemVer::parse(env!("CARGO_PKG_VERSION")).expect("valid semver"),
            stores: Vec::new(),
        },
        // 2: settings.change
        EventPayload::SettingsChange {
            names: vec![SettingsGroup::Role],
            baseline,
        },
        // 3: write.begin (add)
        EventPayload::WriteBegin {
            verb: Verb::parse("add").expect("valid verb"),
            labels: pub_labels.clone(),
            files: Count::new(2),
            file_hashes: Some(vec![
                FileHash {
                    path_sha256: sha(0x11),
                    sha256: sha(0x12),
                },
                FileHash {
                    path_sha256: sha(0x13),
                    sha256: sha(0x14),
                },
            ]),
            affected: vec![s1.clone(), s1_v1.clone()],
            dest: None,
            path_sha256: None,
            policy: None,
        },
        // 4: write.end (add)
        EventPayload::WriteEnd {
            verb: Verb::parse("add").expect("valid verb"),
            labels: pub_labels.clone(),
            affected: vec![s1, s1_v1],
            outcome: Word::new_static("ok"),
            dest: None,
            path_sha256: None,
            policy: None,
        },
        // 5: write.begin (new claim)
        EventPayload::WriteBegin {
            verb: Verb::parse("new claim").expect("valid verb"),
            labels: pub_labels.clone(),
            files: Count::new(1),
            file_hashes: Some(vec![FileHash {
                path_sha256: sha(0x21),
                sha256: sha(0x22),
            }]),
            affected: vec![c1.clone()],
            dest: None,
            path_sha256: None,
            policy: None,
        },
        // 6: write.end (new claim)
        EventPayload::WriteEnd {
            verb: Verb::parse("new claim").expect("valid verb"),
            labels: pub_labels.clone(),
            affected: vec![c1],
            outcome: Word::new_static("ok"),
            dest: None,
            path_sha256: None,
            policy: None,
        },
        // 7: ask
        EventPayload::Ask {
            asked: Some(Id::parse("A-0001").expect("valid asked id")),
            asked_via: AskedVia::Search,
            question_sha256: sha(0x31),
            result_count: Count::new(3),
            outcome: Word::new_static("answered"),
            labels: pub_labels.clone(),
        },
        // 8: refuse
        EventPayload::Refuse {
            code: Code::Fb0410,
            sha256: None,
            name_sha256: None,
            labels: Some(np_labels),
            dest: None,
            reason: Word::new_static("label"),
            verb: Some(Verb::parse("new page").expect("valid verb")),
            system: None,
            stream: None,
        },
        // 9: backup.begin
        EventPayload::BackupBegin {
            labels: pub_labels.clone(),
            dest: DestKind::Backup,
            policy: sha(0x41),
            snapshot: Count::new(1),
        },
        // 10: write.begin (new page)
        EventPayload::WriteBegin {
            verb: Verb::parse("new page").expect("valid verb"),
            labels: pub_labels.clone(),
            files: Count::new(1),
            file_hashes: Some(vec![FileHash {
                path_sha256: sha(0x51),
                sha256: sha(0x52),
            }]),
            affected: vec![page.clone()],
            dest: None,
            path_sha256: None,
            policy: None,
        },
        // 11: write.end (new page)
        EventPayload::WriteEnd {
            verb: Verb::parse("new page").expect("valid verb"),
            labels: pub_labels,
            affected: vec![page],
            outcome: Word::new_static("ok"),
            dest: None,
            path_sha256: None,
            policy: None,
        },
        // 12: backup.verify
        EventPayload::BackupVerify {
            outcome: Word::new_static("ok"),
            verified: BackupVerified::Snapshot {
                target_sha256: sha(0x61),
                snapshot: Count::new(1),
                manifest_sha256: sha(0x62),
            },
        },
    ]
}

/// Build a state folder at `dir` by writing `settings.toml` (`role = "lead"`) and running the
/// real [`Journal`] writer over `events`.
pub fn build_state(dir: &Path, events: &[EventPayload]) {
    write_sync_file(
        &dir.join("settings.toml"),
        STATE_MINI_SETTINGS_TOML.as_bytes(),
    )
    .expect("write settings.toml");

    let root = dir.parent().unwrap_or(dir);
    let world = TestWorld::with_state(root, dir);
    let journal = Journal::open(&world, dir, Via::Cli).expect("Journal::open in build_state");

    let mut skip = 0usize;
    if matches!(events.first(), Some(EventPayload::JournalOpen { .. })) {
        skip = 1;
        if matches!(events.get(1), Some(EventPayload::SettingsChange { .. })) {
            skip = 2;
        }
    }

    for payload in events.iter().skip(skip) {
        journal
            .append_payload(payload.clone())
            .expect("append_payload in build_state");
    }

    drop(journal);
    cleanup_fixture_locks(dir).expect("cleanup_fixture_locks");
}

fn reseal_lines(lines: &[String]) -> String {
    let mut prev_hex = ZERO_HASH.to_owned();
    let mut out = String::new();
    for line in lines {
        let (body, _) = split_hash_suffix(line).expect("split_hash_suffix in reseal_lines");
        let (sealed, hash) = seal_body(&prev_hex, body);
        out.push_str(&sealed);
        out.push('\n');
        prev_hex = hash.as_str().to_owned();
    }
    out
}

/// Build `state-mini` in `dir` and apply `plant` to `audit/journal.jsonl`.
pub fn plant(dir: &Path, plant: Plant) {
    build_state(dir, &state_mini_events());
    let journal_path = dir.join("audit/journal.jsonl");
    let raw = std::fs::read_to_string(&journal_path).expect("read built journal.jsonl");
    let mut lines: Vec<String> = raw.lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 12, "state-mini must have 12 lines");

    let updated = match plant {
        Plant::BrokenChain => {
            assert!(
                lines[4].contains("\"corpus/CL-CMP-0001\""),
                "line 5 must contain corpus/CL-CMP-0001"
            );
            lines[4] = lines[4].replacen("\"corpus/CL-CMP-0001\"", "\"corpus/CL-CMP-0002\"", 1);
            let mut out = lines.join("\n");
            out.push('\n');
            out
        }
        Plant::UnknownKey => {
            let (body, hash) = split_hash_suffix(&lines[6]).expect("line 7 suffix");
            lines[6] = format!("{body},\"note\":\"free text\",\"hash\":\"{hash}\"}}");
            reseal_lines(&lines)
        }
        Plant::UnclosedBegin => {
            lines.truncate(10);
            let mut out = lines.join("\n");
            out.push('\n');
            out
        }
        Plant::TornTail => {
            let last = lines.pop().expect("line 12");
            let half = &last[..last.len() / 2];
            let mut out = lines.join("\n");
            out.push('\n');
            out.push_str(half);
            out
        }
        Plant::TimeBackwards => {
            assert!(
                lines[7].contains("\"time\":\"2026-03-01T12:00:00Z\""),
                "line 8 must carry default timestamp"
            );
            lines[7] = lines[7].replacen(
                "\"time\":\"2026-03-01T12:00:00Z\"",
                "\"time\":\"2026-03-01T11:59:00Z\"",
                1,
            );
            reseal_lines(&lines)
        }
    };

    write_sync_file(&journal_path, updated.as_bytes()).expect("write planted journal.jsonl");
    let settings = dir.join("settings.toml");
    if settings.exists() {
        let _ = std::fs::remove_file(settings);
    }
}
