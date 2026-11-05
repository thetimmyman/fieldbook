//! Pure audit journal chain, vocabulary, envelope, and event payload tests (`audit-journal`).

#![expect(clippy::expect_used, reason = "integration tests use expect")]

use fieldbook_core::CivilDate;
use fieldbook_core::audit::chain::{compute_hash, reseal_line, seal_body, verify_line};
use fieldbook_core::audit::{
    Actor, AskedVia, BackupVerified, CorpusPath, Count, DestKind, Event, EventEnvelope,
    EventPayload, FileHash, Id, Kind, LabelAllow, LabelId, LabelSet, NIL_UUID, Now,
    OperationIntent, OperationKey, PendingFile, PendingWrite, Principal, RecordRef, Role, Sha256,
    StoreId, StoreUid, StreamKey, Verb, Via, Word, WriteFile, WriteSet, ZERO_HASH,
    completed_after_hash, compute_file_hashes, normalize_affected, request_sha256, sample_payload,
    source_root_sha256,
};
use fieldbook_core::check::Code;

const SAMPLES_JSONL: &str = include_str!("audit-samples.jsonl");
const KIND_RS_SOURCE: &str = include_str!("../src/audit/kind.rs");

fn sample_time() -> Now {
    Now::new(
        CivilDate::parse("2026-03-01").expect("valid date"),
        12 * 3600,
    )
    .expect("valid time")
}

fn sample_envelope() -> EventEnvelope {
    EventEnvelope::from_actor(
        &Actor::new(Principal::local(), Role::Lead, Via::Cli),
        StoreId::Corpus,
    )
}

fn build_expected_samples_jsonl() -> String {
    let time = sample_time();
    let env = sample_envelope();
    let mut prev = ZERO_HASH.to_owned();
    let mut out = String::new();
    for (idx, &kind) in Kind::ALL.iter().enumerate() {
        let seq = (idx as u64) + 1;
        let payload = sample_payload(kind, seq);
        let (line, hash) = payload.format_line(seq, time, &env, &prev);
        out.push_str(&line);
        out.push('\n');
        prev = hash.as_str().to_owned();
    }
    out
}

fn reseal_only(line: &str) -> String {
    reseal_line(ZERO_HASH, line).expect("reseal").0
}

#[test]
fn chain_hash_matches_the_golden_vector() {
    let body = r#"{"seq":1,"time":"2026-03-01T12:00:00Z","kind":"journal.open","actor":"local","role":"lead","store":"corpus","via":"cli","writer":0,"event_schema":1,"tool_version":"0.1.0","stores":[]"#;
    // Independent vector: SHA-256(64 zeros || 0x0A || body)
    let expected = "6a54e91fe2e366f8fc1a8a564783ffa5c483418e96bed517d3db8ac0f706171b";
    let (sealed, hash) = seal_body(ZERO_HASH, body);
    assert_eq!(hash.as_str(), expected);
    let verified = verify_line(ZERO_HASH, &sealed).expect("sealed line must verify");
    assert_eq!(verified.body, body);
    assert_eq!(verified.hash.as_str(), expected);
}

#[test]
fn verify_does_not_reserialise() {
    // Include extra whitespace inside the JSON body; verify_line hashes the raw body slice
    // directly up to the fixed 75-byte suffix without re-serialising.
    let raw_body =
        "{  \"seq\": 1,  \"time\": \"2026-03-01T12:00:00Z\",  \"kind\": \"journal.open\"  ";
    let (sealed, hash) = seal_body(ZERO_HASH, raw_body);
    let verified = verify_line(ZERO_HASH, &sealed).expect("verify_line must not re-serialise");
    assert_eq!(verified.body, raw_body);
    assert_eq!(verified.hash, hash);
    assert_eq!(compute_hash(ZERO_HASH, raw_body), hash);
}

// control: AL-01
#[test]
fn vocabulary_rejects_a_free_text_value() {
    assert!(Id::parse("What is the withholding cap for a business system?").is_err());
    assert!(RecordRef::parse("corpus/free text sentence").is_err());
    assert!(CorpusPath::parse("/etc/passwd").is_err());
    assert!(CorpusPath::parse("../secret.txt").is_err());
    assert!(Verb::parse(" ask a question with Caps!").is_err());
    assert!(StreamKey::parse("free text stream").is_err());
    assert!(Word::from_closed("free-text-note", &["ok", "failed"]).is_err());

    let valid_line = SAMPLES_JSONL.lines().next().expect("first sample line");
    let with_note = valid_line.replace(
        "\"stores\":[]",
        "\"stores\":[],\"note\":\"This is a free-text sentence that must be rejected.\"",
    );
    let resealed = reseal_only(&with_note);
    assert!(
        Event::parse_line(&resealed).is_err(),
        "unknown/free-text key must be rejected as FB0801"
    );

    let bad_role = valid_line.replace("\"role\":\"lead\"", "\"role\":\"free text role\"");
    let resealed_role = reseal_only(&bad_role);
    assert!(Event::parse_line(&resealed_role).is_err());
}

#[test]
fn envelope_keys_are_required() {
    let valid_line = SAMPLES_JSONL.lines().next().expect("first sample line");
    assert!(Event::parse_line(valid_line).is_ok());

    for needle in [
        "\"actor\":\"local\",",
        "\"role\":\"lead\",",
        "\"store\":\"corpus\",",
        "\"via\":\"cli\",",
    ] {
        let missing = valid_line.replace(needle, "");
        assert_ne!(missing, valid_line, "needle {needle} must be present");
        let resealed = reseal_only(&missing);
        assert!(
            Event::parse_line(&resealed).is_err(),
            "dropping envelope key {needle} must fail Event::parse_line"
        );
    }
}

#[test]
fn egress_kinds_carry_dest_labels_policy() {
    assert_eq!(
        Kind::EGRESS,
        [
            Kind::AiBegin,
            Kind::AiPack,
            Kind::McpCall,
            Kind::BackupBegin
        ]
    );
    for kind in Kind::EGRESS {
        assert!(kind.is_egress());
        let field_names: Vec<&str> = kind.fields().iter().map(|f| f.name).collect();
        assert!(
            field_names.contains(&"dest"),
            "{kind} must declare `dest` in fields()"
        );
        assert!(
            field_names.contains(&"labels"),
            "{kind} must declare `labels` in fields()"
        );
        assert!(
            field_names.contains(&"policy"),
            "{kind} must declare `policy` in fields()"
        );

        let payload = sample_payload(kind, 1);
        let (line, _) = payload.format_line(1, sample_time(), &sample_envelope(), ZERO_HASH);
        let (parsed, obj) = Event::parse_line(&line).expect("egress sample must parse");
        assert_eq!(parsed.kind(), kind);
        assert!(obj.get("dest").is_some());
        assert!(obj.get("labels").is_some());
        assert!(obj.get("policy").is_some());
    }
}

#[test]
fn dest_without_labels_and_policy_is_refused() {
    let payload = sample_payload(Kind::AiBegin, 1);
    let (line, _) = payload.format_line(1, sample_time(), &sample_envelope(), ZERO_HASH);
    assert!(Event::parse_line(&line).is_ok());

    let policy_hex = Sha256::digest(b"fieldbook-floor-policy-1");
    let needle = format!(",\"policy\":\"{policy_hex}\"");
    let without_policy = line.replace(&needle, "");
    assert_ne!(without_policy, line);
    let resealed = reseal_only(&without_policy);
    assert!(
        Event::parse_line(&resealed).is_err(),
        "ai.begin with dest and without policy must be refused"
    );

    let without_labels = line.replace(",\"labels\":[\"public\"]", "");
    assert_ne!(without_labels, line);
    let resealed_labels = reseal_only(&without_labels);
    assert!(
        Event::parse_line(&resealed_labels).is_err(),
        "ai.begin with dest and without labels must be refused"
    );
}

#[test]
fn refuse_with_dest_and_without_policy_parses() {
    let time = sample_time();
    let env = sample_envelope();

    let with_labels = EventPayload::Refuse {
        verb: Some(Verb::parse("sync").unwrap()),
        code: Code::Fb0901,
        reason: Word::new_static("label"),
        sha256: None,
        name_sha256: None,
        labels: Some(LabelSet::unlabelled()),
        dest: Some(DestKind::Model),
        system: Some(StreamKey::parse("finance-exports").unwrap()),
        stream: Some(StreamKey::parse("commitments").unwrap()),
    };
    let (line1, _) = with_labels.format_line(1, time, &env, ZERO_HASH);
    let (parsed1, obj1) =
        Event::parse_line(&line1).expect("refuse with dest and labels must parse");
    assert_eq!(parsed1.payload, with_labels);
    assert!(obj1.get("policy").is_none());

    let without_labels = EventPayload::Refuse {
        verb: Some(Verb::parse("sync").unwrap()),
        code: Code::Fb0918,
        reason: Word::new_static("no-row"),
        sha256: None,
        name_sha256: None,
        labels: None,
        dest: Some(DestKind::Model),
        system: None,
        stream: None,
    };
    let (line2, _) = without_labels.format_line(2, time, &env, ZERO_HASH);
    let (parsed2, obj2) =
        Event::parse_line(&line2).expect("refuse with dest and no labels must parse");
    assert_eq!(parsed2.payload, without_labels);
    assert!(obj2.get("labels").is_none());
    assert!(obj2.get("policy").is_none());

    // Refuse with policy must be rejected.
    let with_policy = line1.replace(
        ",\"dest\":\"model\"",
        ",\"dest\":\"model\",\"policy\":\"0000000000000000000000000000000000000000000000000000000000000000\"",
    );
    let resealed = reseal_only(&with_policy);
    assert!(Event::parse_line(&resealed).is_err());
}

#[test]
fn refuse_with_verb_system_and_stream_parses() {
    let payload = EventPayload::Refuse {
        verb: Some(Verb::parse("sync").unwrap()),
        code: Code::Fb0807,
        reason: Word::new_static("root"),
        sha256: None,
        name_sha256: None,
        labels: Some(LabelSet::unlabelled()),
        dest: None,
        system: Some(StreamKey::parse("finance-exports").unwrap()),
        stream: Some(StreamKey::parse("commitments").unwrap()),
    };
    let (line, _) = payload.format_line(1, sample_time(), &sample_envelope(), ZERO_HASH);
    let (parsed, _) = Event::parse_line(&line).expect("refuse with verb/system/stream must parse");
    assert_eq!(parsed.payload, payload);
    match parsed.payload {
        EventPayload::Refuse {
            verb,
            system,
            stream,
            ..
        } => {
            assert_eq!(verb.as_ref().map(Verb::as_str), Some("sync"));
            assert_eq!(
                system.as_ref().map(StreamKey::as_str),
                Some("finance-exports")
            );
            assert_eq!(stream.as_ref().map(StreamKey::as_str), Some("commitments"));
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn a_verb_with_a_capital_a_digit_or_33_bytes_is_fb0801() {
    assert!(Verb::parse("Sync").is_err(), "capitalized verb must fail");
    assert!(Verb::parse("sync2").is_err(), "verb with digit must fail");
    let thirty_three = "a".repeat(33);
    assert_eq!(thirty_three.len(), 33);
    assert!(
        Verb::parse(&thirty_three).is_err(),
        "33-byte verb must fail"
    );

    assert_eq!(Verb::parse("sync").unwrap().as_str(), "sync");
    assert_eq!(
        Verb::parse("queue accept").unwrap().as_str(),
        "queue accept"
    );

    // Also verify through Event::parse_line (which raises FB0801 in audit verify).
    let base = sample_payload(Kind::Refuse, 1)
        .format_line(1, sample_time(), &sample_envelope(), ZERO_HASH)
        .0;
    for bad in ["Sync", "sync2", &thirty_three] {
        let mutated = base.replace("\"verb\":\"sync\"", &format!("\"verb\":\"{bad}\""));
        let resealed = reseal_only(&mutated);
        assert!(
            Event::parse_line(&resealed).is_err(),
            "Event::parse_line must reject invalid verb `{bad}`"
        );
    }
}

#[test]
fn every_kind_has_sample_and_no_free_text() {
    assert_eq!(Kind::ALL.len(), 32, "Kind::ALL must list all 32 kinds");

    // Verify no event field in kind.rs or Kind::fields() has type `String`.
    assert!(
        !KIND_RS_SOURCE.contains(": String") && !KIND_RS_SOURCE.contains("<String>"),
        "kind.rs must not contain any String field on an event kind"
    );
    for kind in Kind::ALL {
        for field in kind.fields() {
            assert!(
                !field.ty.contains("String"),
                "kind {kind} field {} must not use String type (found {})",
                field.name,
                field.ty
            );
        }
    }

    let expected_samples = build_expected_samples_jsonl();
    assert_eq!(
        SAMPLES_JSONL, expected_samples,
        "audit-samples.jsonl is out of date with sample_payload()"
    );

    let lines: Vec<&str> = SAMPLES_JSONL.lines().collect();
    assert_eq!(
        lines.len(),
        Kind::ALL.len(),
        "audit-samples.jsonl must hold exactly one sample line per kind in Kind::ALL"
    );

    let mut prev = ZERO_HASH.to_owned();
    for (idx, (&kind, &line)) in Kind::ALL.iter().zip(lines.iter()).enumerate() {
        let expected_seq = (idx as u64) + 1;
        let verified = verify_line(&prev, line)
            .unwrap_or_else(|e| panic!("sample line {expected_seq} ({kind}) failed chain: {e}"));
        let (parsed, obj) = Event::parse_line(line)
            .unwrap_or_else(|e| panic!("sample line {expected_seq} ({kind}) failed parse: {e}"));
        assert_eq!(parsed.seq, expected_seq);
        assert_eq!(parsed.kind(), kind);
        assert_eq!(parsed.hash, verified.hash);

        let (reformatted, rehashed) =
            parsed
                .payload
                .format_line(parsed.seq, parsed.time, &parsed.envelope, &prev);
        assert_eq!(reformatted, line, "round-trip mismatch for {kind}");
        assert_eq!(rehashed, verified.hash);

        if kind == Kind::Refuse {
            assert_eq!(
                obj.get("dest").and_then(|v| v.as_str()),
                Some("model"),
                "refuse sample must carry a dest"
            );
            assert_eq!(
                obj.get("verb").and_then(|v| v.as_str()),
                Some("sync"),
                "refuse sample must carry verb=sync"
            );
            assert_eq!(
                obj.get("system").and_then(|v| v.as_str()),
                Some("finance-exports"),
                "refuse sample must carry system=finance-exports"
            );
            assert_eq!(
                obj.get("stream").and_then(|v| v.as_str()),
                Some("commitments"),
                "refuse sample must carry stream=commitments"
            );
        }

        prev = verified.hash.as_str().to_owned();
    }
}

#[test]
fn ask_keeps_envelope_and_payload_via() {
    let payload = sample_payload(Kind::Ask, 13);
    let env = sample_envelope();
    assert_eq!(env.via, Via::Cli);
    let (line, _) = payload.format_line(13, sample_time(), &env, ZERO_HASH);
    assert!(line.contains("\"via\":\"cli\""));
    assert!(line.contains("\"asked_via\":\"search\""));

    let (parsed, _) = Event::parse_line(&line).expect("ask sample must parse");
    assert_eq!(parsed.envelope.via, Via::Cli);
    match parsed.payload {
        EventPayload::Ask { asked_via, .. } => assert_eq!(asked_via, AskedVia::Search),
        other => panic!("expected Ask payload, got {other:?}"),
    }

    // Replacing `asked_via` with `via` creates a duplicate `via` key and must fail.
    let duplicate_via = line.replace("\"asked_via\":\"search\"", "\"via\":\"search\"");
    let resealed = reseal_only(&duplicate_via);
    assert!(
        Event::parse_line(&resealed).is_err(),
        "replacing asked_via with via must fail"
    );
}

#[test]
fn backup_verify_and_end_round_trip_and_reject_mixed_or_duplicate() {
    let time = sample_time();
    let env = sample_envelope();

    // 1. backup.verify snapshot
    let snap_payload = sample_payload(Kind::BackupVerify, 20);
    let (snap_line, _) = snap_payload.format_line(20, time, &env, ZERO_HASH);
    assert!(snap_line.contains("\"kind\":\"backup.verify\""));
    assert!(snap_line.contains("\"verification\":\"snapshot\""));
    let (parsed_snap, _) = Event::parse_line(&snap_line).expect("snapshot verify must parse");
    assert_eq!(parsed_snap.payload, snap_payload);

    // 2. backup.verify raw (without and with public_support_sha256)
    let raw_payload = EventPayload::BackupVerify {
        outcome: Word::new_static("ok"),
        verified: BackupVerified::Raw {
            path_sha256: Sha256::digest(b"raw-copy-path"),
            public_support_sha256: Some(Sha256::digest(b"support-catalogue")),
        },
    };
    let (raw_line, _) = raw_payload.format_line(21, time, &env, ZERO_HASH);
    assert!(raw_line.contains("\"verification\":\"raw\""));
    let (parsed_raw, _) = Event::parse_line(&raw_line).expect("raw verify must parse");
    assert_eq!(parsed_raw.payload, raw_payload);

    // Failed raw verify with public_support_sha256 is forbidden.
    let failed_raw_with_support = raw_line.replace("\"outcome\":\"ok\"", "\"outcome\":\"failed\"");
    let resealed_failed = reseal_only(&failed_raw_with_support);
    assert!(Event::parse_line(&resealed_failed).is_err());

    // Mixed snapshot + path_sha256 must fail.
    let mixed = snap_line.replace(
        "\"snapshot\":1",
        &format!(
            "\"snapshot\":1,\"path_sha256\":\"{}\"",
            Sha256::digest(b"raw-copy-path")
        ),
    );
    let resealed_mixed = reseal_only(&mixed);
    assert!(Event::parse_line(&resealed_mixed).is_err());

    // 3. backup.end round-trip and duplicate kind rejection
    let end_payload = sample_payload(Kind::BackupEnd, 19);
    let (end_line, _) = end_payload.format_line(19, time, &env, ZERO_HASH);
    assert!(end_line.contains("\"kind\":\"backup.end\""));
    assert!(end_line.contains("\"target_kind\":\"folder\""));
    let (parsed_end, _) = Event::parse_line(&end_line).expect("backup.end must parse");
    assert_eq!(parsed_end.payload, end_payload);

    let repeated_kind = end_line.replace("\"target_kind\":\"folder\"", "\"kind\":\"folder\"");
    let resealed_dup = reseal_only(&repeated_kind);
    assert!(
        Event::parse_line(&resealed_dup).is_err(),
        "repeated kind key must be rejected"
    );
}

#[test]
fn sync_begin_root_sha256_optional_and_rejects_paths() {
    let time = sample_time();
    let env = sample_envelope();

    let with_root = sample_payload(Kind::SyncBegin, 31);
    let (line_with, _) = with_root.format_line(31, time, &env, ZERO_HASH);
    let (parsed_with, _) = Event::parse_line(&line_with).expect("sync.begin with root_sha256");
    assert_eq!(parsed_with.payload, with_root);

    let without_root = EventPayload::SyncBegin {
        system: StreamKey::parse("finance-exports").unwrap(),
        stream: StreamKey::parse("commitments").unwrap(),
        adapter: Word::new_static("file-drop@1"),
        root_sha256: None,
        policy: Sha256::digest(b"sample-policy"),
    };
    let (line_without, _) = without_root.format_line(31, time, &env, ZERO_HASH);
    assert!(!line_without.contains("root_sha256"));
    let (parsed_without, _) =
        Event::parse_line(&line_without).expect("sync.begin without root_sha256");
    assert_eq!(parsed_without.payload, without_root);

    // Reject a raw path in root_sha256 or an unknown `root` field.
    let path_in_root_sha = line_without.replace(
        "\"adapter\":\"file-drop@1\"",
        "\"adapter\":\"file-drop@1\",\"root_sha256\":\"/mnt/drop/finance\"",
    );
    let resealed_path = reseal_only(&path_in_root_sha);
    assert!(Event::parse_line(&resealed_path).is_err());

    let unknown_root_key = line_without.replace(
        "\"adapter\":\"file-drop@1\"",
        "\"adapter\":\"file-drop@1\",\"root\":\"/mnt/drop/finance\"",
    );
    let resealed_unknown = reseal_only(&unknown_root_key);
    assert!(Event::parse_line(&resealed_unknown).is_err());

    // Domain separation between Unix and Windows source_root_sha256.
    let unix_hash = source_root_sha256(false, b"/srv/drop");
    let win_hash = source_root_sha256(true, b"/srv/drop");
    assert_ne!(unix_hash, win_hash);
}

#[test]
fn mcp_session_client_sha256_optional_and_rejects_malformed() {
    let time = sample_time();
    let env = sample_envelope();

    // Omitted client_sha256 fixture case
    let omitted = EventPayload::McpSession {
        protocol: Word::new_static("2025-11-25"),
        client_sha256: None,
        allow: LabelAllow::from_ids([LabelId::parse("public").unwrap()]),
    };
    let (line_omitted, _) = omitted.format_line(14, time, &env, ZERO_HASH);
    assert!(!line_omitted.contains("client_sha256"));
    let (parsed_omitted, _) =
        Event::parse_line(&line_omitted).expect("mcp.session without client_sha256 must parse");
    assert_eq!(parsed_omitted.payload, omitted);

    // Reject empty or malformed client_sha256
    for bad in ["", "claude-desktop", "0123456789abcdef"] {
        let bad_line = line_omitted.replace(
            "\"protocol\":\"2025-11-25\"",
            &format!("\"protocol\":\"2025-11-25\",\"client_sha256\":\"{bad}\""),
        );
        let resealed = reseal_only(&bad_line);
        assert!(
            Event::parse_line(&resealed).is_err(),
            "malformed client_sha256 `{bad}` must be rejected"
        );
    }
}

#[test]
fn file_hashes_and_completed_write_evidence() {
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![
            WriteFile::new(
                "registers/sources.csv",
                Sha256::digest(b"after-sources").as_str(),
            ),
            WriteFile::new("README.md", Sha256::digest(b"after-readme").as_str()),
        ],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap()],
    };
    let hashes = compute_file_hashes(&ws.files).expect("valid file hashes");
    assert_eq!(hashes.len(), 2);
    assert!(hashes[0].path_sha256 < hashes[1].path_sha256);

    let time = sample_time();
    let env = sample_envelope();
    let begin_payload = EventPayload::WriteBegin {
        verb: Verb::parse("add source").unwrap(),
        labels: ws.labels.clone(),
        files: Count(2),
        file_hashes: Some(hashes),
        affected: normalize_affected(&ws.affected),
        dest: None,
        path_sha256: None,
        policy: None,
    };
    let (begin_line, h1) = begin_payload.format_line(1, time, &env, ZERO_HASH);
    let (begin_event, _) = Event::parse_line(&begin_line).expect("parse begin");

    // Unfinished write.begin alone provides no completed-write evidence.
    assert_eq!(
        completed_after_hash(
            std::slice::from_ref(&begin_event),
            StoreId::Corpus,
            "README.md"
        ),
        None
    );

    // Matching ok write.end supplies completed-write evidence.
    let end_ok_payload = EventPayload::WriteEnd {
        verb: Verb::parse("add source").unwrap(),
        labels: ws.labels.clone(),
        affected: normalize_affected(&ws.affected),
        outcome: Word::new_static("ok"),
        dest: None,
        path_sha256: None,
        policy: None,
    };
    let (end_ok_line, _) = end_ok_payload.format_line(2, time, &env, h1.as_str());
    let (end_ok_event, _) = Event::parse_line(&end_ok_line).expect("parse ok end");
    let events_ok = [begin_event.clone(), end_ok_event];
    assert_eq!(
        completed_after_hash(&events_ok, StoreId::Corpus, "README.md"),
        Some(Sha256::digest(b"after-readme"))
    );
    assert_eq!(
        completed_after_hash(&events_ok, StoreId::Overlay, "README.md"),
        None,
        "foreign store must not match"
    );

    // Changing the matching write.end to failed removes provenance.
    let end_failed_payload = EventPayload::WriteEnd {
        verb: Verb::parse("add source").unwrap(),
        labels: ws.labels,
        affected: normalize_affected(&ws.affected),
        outcome: Word::new_static("failed"),
        dest: None,
        path_sha256: None,
        policy: None,
    };
    let (end_failed_line, _) = end_failed_payload.format_line(2, time, &env, h1.as_str());
    let (end_failed_event, _) = Event::parse_line(&end_failed_line).expect("parse failed end");
    let events_failed = [begin_event, end_failed_event];
    assert_eq!(
        completed_after_hash(&events_failed, StoreId::Corpus, "README.md"),
        None
    );

    // File count mismatch with file_hashes length must be rejected by Event::parse_line.
    let bad_count_line = begin_line.replace("\"files\":2", "\"files\":1");
    let resealed_bad = reseal_only(&bad_count_line);
    assert!(Event::parse_line(&resealed_bad).is_err());

    // Duplicate path_sha256 in file_hashes must be rejected.
    let dup_fh = FileHash {
        path_sha256: Sha256::digest(b"README.md"),
        sha256: Sha256::digest(b"after-readme"),
    };
    let dup_payload = EventPayload::WriteBegin {
        verb: Verb::parse("add source").unwrap(),
        labels: LabelSet::public(),
        files: Count(2),
        file_hashes: Some(vec![dup_fh.clone(), dup_fh]),
        affected: Vec::new(),
        dest: None,
        path_sha256: None,
        policy: None,
    };
    let (dup_line, _) = dup_payload.format_line(1, time, &env, ZERO_HASH);
    assert!(Event::parse_line(&dup_line).is_err());
}

#[test]
fn store_uid_and_operation_intent_round_trip() {
    let uid = StoreUid::parse("0195ef40-7c00-7000-8000-000000000001").expect("valid UUID");
    assert_eq!(StoreUid::nil().as_str(), NIL_UUID);

    // Malformed UUIDs are rejected.
    for bad in [
        "",
        "0195ef40-7c00-7000-8000-00000000000G",
        "0195EF40-7C00-7000-8000-000000000001",
        "0195ef407c0070008000000000000001",
    ] {
        assert!(StoreUid::parse(bad).is_err(), "UUID `{bad}` must fail");
    }

    // StoreUid is never valid as Id or RecordRef.
    assert!(Id::parse(uid.as_str()).is_err());
    assert!(RecordRef::parse(&format!("overlay/{uid}")).is_err());

    let req_hash =
        request_sha256(&[("store", b"overlay"), ("verb", b"store init")]).expect("request hash");
    assert!(
        request_sha256(&[("dup", b"1"), ("dup", b"2")]).is_err(),
        "duplicate request field names must be rejected"
    );

    let intent = OperationIntent {
        schema: 1,
        key: OperationKey::new(
            Verb::parse("store init").unwrap(),
            Sha256::digest(b"overlay"),
        ),
        request_sha256: req_hash,
        allocated: vec![RecordRef::parse("overlay/SO-100001").unwrap()],
        allocated_store_uid: Some(uid.clone()),
        planned_on: CivilDate::parse("2026-03-01").unwrap(),
    };
    let pending = PendingWrite {
        schema: 1,
        begin_seq: 4,
        source_generation: Sha256::digest(b"gen-0"),
        labels: LabelSet::unlabelled(),
        affected: vec![RecordRef::parse("overlay/SO-100001").unwrap()],
        intent,
        files: vec![PendingFile {
            path: CorpusPath::parse("store.toml").unwrap(),
            before_sha256: None,
            after_sha256: Sha256::digest(b"store-toml-bytes"),
        }],
    };
    let toml_text = pending.to_toml();
    let parsed = PendingWrite::parse_toml(&toml_text).expect("PendingWrite round-trip");
    assert_eq!(parsed, pending);
    assert_eq!(parsed.intent.allocated_store_uid, Some(uid));
}
