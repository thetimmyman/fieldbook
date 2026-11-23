//! Pure unit and architectural tests for `policy/{place,writers,seen,show}.rs` (`policy-load`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "tests read repository fixture files and scan crate source files for single-entry-point invariants"
)]

use std::path::{Path, PathBuf};

use fieldbook_core::CivilDate;
use fieldbook_core::audit::Sha256;
use fieldbook_core::check::Code;
use fieldbook_core::labels::{LabelAllow, LabelId, PUBLIC};
use fieldbook_core::policy::{
    ClosedWhy, EffectivePolicy, FileFact, FileObservation, Flags, FloorKind, LaptopFolders,
    MAX_POLICY_BYTES, OVER_LIMIT_PREFIX_BYTES, ObservationDigest, OwnerSettings, PlaceWhy,
    Placement, Policy, PolicyState, Seen, Source, Writer, WriterFacts, WriterWhy, flags_allowed,
    place, policy_line, resolve, show, show_at, writer_check,
};

const BASE_FIXTURE: &str = include_str!("../../../tests/fixtures/policy/policy.toml");
const LOWER_FIXTURE: &str = include_str!("../../../tests/fixtures/policy/policy-lower.toml");
const BAD_FIXTURE: &str = include_str!("../../../tests/fixtures/policy/policy-bad.toml");
const NEXT_FIXTURE: &str = include_str!("../../../tests/fixtures/policy/policy-next.toml");

// ---------------------------------------------------------------------------
// Step 1: policy/place.rs
// ---------------------------------------------------------------------------

#[test]
fn a_policy_in_the_data_root_is_refused() {
    let p = Placement {
        in_data_root: true,
        in_corpus: false,
        under_git: false,
        fixture_ok: true,
    };
    assert_eq!(place(&p), Err(PlaceWhy::DataRoot));
}

#[test]
fn a_policy_in_the_corpus_is_refused() {
    let p = Placement {
        in_data_root: false,
        in_corpus: true,
        under_git: false,
        fixture_ok: true,
    };
    assert_eq!(place(&p), Err(PlaceWhy::Corpus));
}

#[test]
fn a_policy_under_git_is_refused() {
    let p = Placement {
        in_data_root: false,
        in_corpus: false,
        under_git: true,
        fixture_ok: false,
    };
    assert_eq!(place(&p), Err(PlaceWhy::GitWorkTree));
}

#[test]
fn a_test_build_fixture_under_git_is_allowed() {
    let p = Placement {
        in_data_root: false,
        in_corpus: false,
        under_git: true,
        fixture_ok: true,
    };
    assert_eq!(place(&p), Ok(()));
}

#[test]
fn a_policy_beside_the_data_root_is_allowed() {
    let p = Placement {
        in_data_root: false,
        in_corpus: false,
        under_git: false,
        fixture_ok: false,
    };
    assert_eq!(place(&p), Ok(()));
}

#[test]
fn on_the_laptop_a_state_flag_elsewhere_is_fb0419() {
    let fixed = LaptopFolders {
        state: PathBuf::from("/fixed/state"),
        overlay: PathBuf::from("/fixed/overlay"),
        working: PathBuf::from("/fixed/working"),
    };
    let flags = Flags {
        state: Some(PathBuf::from("/elsewhere/state")),
        overlay: None,
        working: None,
    };
    let err = flags_allowed(true, &fixed, &flags).unwrap_err();
    assert_eq!(err.code, Code::Fb0419);
}

#[test]
fn on_the_laptop_the_fixed_folders_are_allowed() {
    let fixed = LaptopFolders {
        state: PathBuf::from("/fixed/state"),
        overlay: PathBuf::from("/fixed/overlay"),
        working: PathBuf::from("/fixed/working"),
    };
    let flags = Flags {
        state: Some(PathBuf::from("/fixed/state")),
        overlay: Some(PathBuf::from("/fixed/overlay")),
        working: Some(PathBuf::from("/fixed/working")),
    };
    assert_eq!(flags_allowed(true, &fixed, &flags), Ok(()));
    assert_eq!(flags_allowed(true, &fixed, &Flags::default()), Ok(()));
}

#[test]
fn at_home_any_state_flag_is_allowed() {
    let fixed = LaptopFolders {
        state: PathBuf::from("/fixed/state"),
        overlay: PathBuf::from("/fixed/overlay"),
        working: PathBuf::from("/fixed/working"),
    };
    let flags = Flags {
        state: Some(PathBuf::from("/custom/state")),
        overlay: Some(PathBuf::from("/custom/overlay")),
        working: Some(PathBuf::from("/custom/working")),
    };
    assert_eq!(flags_allowed(false, &fixed, &flags), Ok(()));
}

// ---------------------------------------------------------------------------
// Step 2: policy/writers.rs
// ---------------------------------------------------------------------------

#[test]
fn administrators_and_system_alone_pass() {
    let facts = WriterFacts {
        file: vec![Writer::Administrators, Writer::System],
        folder: vec![Writer::Administrators, Writer::System],
    };
    assert_eq!(writer_check(&facts), Ok(()));
}

#[test]
fn a_user_who_can_write_the_file_fails() {
    let facts = WriterFacts {
        file: vec![Writer::Administrators, Writer::Other],
        folder: vec![Writer::Administrators, Writer::System],
    };
    let err = writer_check(&facts).unwrap_err();
    assert_eq!(err.code(), Code::Fb0418);
    assert_eq!(err, WriterWhy::FileOther);
}

#[test]
fn a_user_who_can_write_the_folder_fails() {
    let facts = WriterFacts {
        file: vec![Writer::Administrators],
        folder: vec![Writer::Administrators, Writer::Other],
    };
    let err = writer_check(&facts).unwrap_err();
    assert_eq!(err.code(), Code::Fb0418);
    assert_eq!(err, WriterWhy::FolderOther);
}

#[test]
fn an_unknown_entry_fails() {
    let facts = WriterFacts {
        file: vec![Writer::Unknown],
        folder: vec![Writer::Administrators],
    };
    let err = writer_check(&facts).unwrap_err();
    assert_eq!(err.code(), Code::Fb0418);
    assert_eq!(err, WriterWhy::FileUnknown);
}

#[test]
fn the_refusal_names_the_path_and_no_account() {
    let p = Path::new("/etc/fieldbook/policy.toml");
    let msg = WriterWhy::FileOther.format_refusal(p);
    assert!(msg.contains("FB0418"), "{msg}");
    assert!(msg.contains("/etc/fieldbook/policy.toml"), "{msg}");
    assert!(!msg.contains("os:"), "{msg}");
    assert!(!msg.contains("EXAMPLE\\"), "{msg}");
}

// ---------------------------------------------------------------------------
// Step 3: policy/seen.rs
// ---------------------------------------------------------------------------

#[test]
fn a_missing_file_with_no_record_is_the_floor() {
    let resolved = resolve(FileObservation::absent(), None, false);
    assert_eq!(resolved.state, PolicyState::Floor(FloorKind::A));
    assert_eq!(resolved.outcome, None);
    assert_eq!(resolved.seen, None);
    assert_eq!(resolved.note, None);
}

#[test]
fn the_floor_is_b_when_a_company_store_exists() {
    let resolved = resolve(FileObservation::absent(), None, true);
    assert_eq!(resolved.state, PolicyState::Floor(FloorKind::B));
    assert_eq!(resolved.outcome, None);
    assert_eq!(resolved.seen, None);
}

#[test]
fn a_missing_file_with_a_record_is_closed_absent() {
    let prev = Seen {
        revision: 3,
        hash: Sha256::digest(BASE_FIXTURE.as_bytes()),
        audit_sink: Some(PathBuf::from("/srv/company/audit")),
    };
    let resolved = resolve(FileObservation::absent(), Some(&prev), false);
    assert_eq!(resolved.state, PolicyState::Closed(ClosedWhy::Absent));
    assert_eq!(resolved.outcome, Some("absent"));
    assert_eq!(resolved.seen.as_ref(), Some(&prev));
}

#[test]
fn a_valid_file_is_loaded() {
    let obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    assert!(matches!(resolved.state, PolicyState::Loaded(_)));
    assert_eq!(resolved.outcome, Some("loaded"));
    let seen = resolved.seen.expect("seen recorded");
    assert_eq!(seen.revision, 3);
    assert_eq!(seen.hash, Sha256::digest(BASE_FIXTURE.as_bytes()));
}

#[test]
fn an_invalid_file_is_closed_invalid() {
    let obs = FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    assert_eq!(resolved.state, PolicyState::Closed(ClosedWhy::Invalid));
    assert_eq!(resolved.outcome, Some("invalid"));
    assert_eq!(resolved.note, None);
    assert!(resolved.finding.is_some());
}

#[test]
fn a_writable_file_is_closed_writable() {
    let obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Err(WriterWhy::FileOther));
    let resolved = resolve(obs, None, false);
    assert_eq!(resolved.state, PolicyState::Closed(ClosedWhy::Writable));
    assert_eq!(resolved.outcome, Some("invalid"));
    assert_eq!(resolved.closed_code(), Some(Code::Fb0418));
}

#[test]
fn a_lower_revision_is_closed() {
    let prev = Seen {
        revision: 3,
        hash: Sha256::digest(BASE_FIXTURE.as_bytes()),
        audit_sink: Some(PathBuf::from("/srv/company/audit")),
    };
    let obs = FileObservation::from_bytes(LOWER_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, Some(&prev), false);
    assert_eq!(
        resolved.state,
        PolicyState::Closed(ClosedWhy::LowerRevision)
    );
    assert_eq!(resolved.outcome, Some("invalid"));
    assert_eq!(resolved.seen.as_ref(), Some(&prev));
}

#[test]
fn an_equal_revision_with_new_bytes_is_loaded() {
    let prev = Seen {
        revision: 3,
        hash: Sha256::digest(b"previous bytes"),
        audit_sink: None,
    };
    let obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, Some(&prev), false);
    assert!(matches!(resolved.state, PolicyState::Loaded(_)));
    assert_eq!(resolved.outcome, Some("loaded"));
    assert_eq!(
        resolved.seen.unwrap().hash,
        Sha256::digest(BASE_FIXTURE.as_bytes())
    );
}

#[test]
fn a_closed_run_keeps_the_old_record() {
    let prev = Seen {
        revision: 3,
        hash: Sha256::digest(BASE_FIXTURE.as_bytes()),
        audit_sink: Some(PathBuf::from("/srv/company/audit")),
    };
    let obs = FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, Some(&prev), false);
    assert!(resolved.state.is_closed());
    assert_eq!(resolved.seen.as_ref(), Some(&prev));
}

#[test]
fn the_sink_is_recorded_from_the_loaded_policy() {
    let obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    let seen = resolved.seen.expect("seen");
    assert_eq!(seen.audit_sink, Some(PathBuf::from("/srv/company/audit")));
}

#[test]
fn a_policy_with_no_audit_table_records_no_sink() {
    let minimal = "policy_schema = 1\nrevision = 1\n[[destination]]\nkind = \"clipboard\"\nallow = [\"public\"]\n";
    let obs = FileObservation::from_bytes(minimal.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    assert!(matches!(resolved.state, PolicyState::Loaded(_)));
    let seen = resolved.seen.expect("seen");
    assert_eq!(seen.audit_sink, None);
}

#[test]
fn seen_round_trips() {
    let seen = Seen {
        revision: 4,
        hash: Sha256::digest(NEXT_FIXTURE.as_bytes()),
        audit_sink: Some(PathBuf::from("/srv/company/audit")),
    };
    let text = seen.to_text();
    assert!(text.starts_with("seen_schema = 1\n"), "{text}");
    assert_eq!(Seen::parse(&text), Some(seen));

    let no_sink = Seen {
        revision: 1,
        hash: Sha256::digest(b"x"),
        audit_sink: None,
    };
    assert_eq!(Seen::parse(&no_sink.to_text()), Some(no_sink));
}

#[test]
fn an_unreadable_record_is_none() {
    assert_eq!(Seen::parse("not valid toml = ["), None);
    assert_eq!(
        Seen::parse(
            "seen_schema = 2\nrevision = 1\nhash = \"0000000000000000000000000000000000000000000000000000000000000000\"\n"
        ),
        None
    );
    assert_eq!(
        Seen::parse(
            "seen_schema = 1\nrevision = 0\nhash = \"0000000000000000000000000000000000000000000000000000000000000000\"\n"
        ),
        None
    );
    assert_eq!(
        Seen::parse(
            "seen_schema = 1\nrevision = 1\nhash = \"0000000000000000000000000000000000000000000000000000000000000000\"\nextra = 1\n"
        ),
        None
    );
}

#[test]
fn a_file_saved_by_windows_tools_loads() {
    let crlf = BASE_FIXTURE.replace("\r\n", "\n").replace('\n', "\r\n");
    let mut win_bytes = vec![0xEF, 0xBB, 0xBF];
    win_bytes.extend_from_slice(crlf.as_bytes());
    let expected_sha = Sha256::digest(&win_bytes);

    let obs = FileObservation::from_bytes(&win_bytes, Ok(()));
    let resolved = resolve(obs, None, false);
    let PolicyState::Loaded(policy) = &resolved.state else {
        panic!("expected Loaded, got {:?}", resolved.state);
    };
    assert_eq!(policy.hash(), expected_sha.as_str());
    assert_eq!(resolved.seen.unwrap().hash, expected_sha);
}

#[test]
fn a_utf16_file_is_unreadable_and_says_so() {
    let mut utf16le = vec![0xFF, 0xFE];
    for ch in BASE_FIXTURE.encode_utf16() {
        utf16le.extend_from_slice(&ch.to_le_bytes());
    }
    let fact = FileFact::from_bytes(&utf16le);
    assert_eq!(fact, FileFact::NotUtf8 { utf16: true });

    let obs = FileObservation::from_bytes(&utf16le, Ok(()));
    let resolved = resolve(obs, None, false);
    assert_eq!(resolved.state, PolicyState::Closed(ClosedWhy::Invalid));
    assert_eq!(resolved.outcome, Some("invalid"));
    let note = resolved.note.expect("note");
    assert!(note.contains("UTF-16"), "{note}");
    assert!(note.contains("UTF-8"), "{note}");
}

#[test]
fn a_file_over_one_mib_is_too_large() {
    let exact = vec![b'#'; MAX_POLICY_BYTES];
    assert!(matches!(FileFact::from_bytes(&exact), FileFact::Text(_)));

    let over = vec![b'#'; MAX_POLICY_BYTES + 1];
    assert_eq!(FileFact::from_bytes(&over), FileFact::TooLarge);
}

// ---------------------------------------------------------------------------
// Step 4: policy/show.rs
// ---------------------------------------------------------------------------

#[test]
fn show_floor_prints_none_and_the_floor_table() {
    let resolved = resolve(FileObservation::absent(), None, false);
    let eff = EffectivePolicy::new(&resolved.state, &OwnerSettings::unset());
    let shown = show(&resolved, &eff, None, &Source::None);
    assert!(
        shown.text.starts_with("policy: none (floor)\n"),
        "{}",
        shown.text
    );
    assert!(shown.text.contains("destination model"), "{}", shown.text);
    assert!(shown.text.contains("destination backup"), "{}", shown.text);
    assert_eq!(shown.report.state, "floor");
    assert_eq!(shown.report.exit, 0);
}

#[test]
fn show_floor_b_prints_the_narrow_backup_route() {
    let resolved = resolve(FileObservation::absent(), None, true);
    let eff = EffectivePolicy::new(&resolved.state, &OwnerSettings::unset());
    let shown = show(&resolved, &eff, None, &Source::None);
    assert!(
        shown.text.starts_with("policy: none (floor)\n"),
        "{}",
        shown.text
    );
    assert!(
        shown.text.contains("company_storage_only=true"),
        "{}",
        shown.text
    );
    assert!(
        shown.text.contains("destination model: allow=[]"),
        "{}",
        shown.text
    );
    assert!(
        shown.text.contains("destination export: allow=[]"),
        "{}",
        shown.text
    );
}

#[test]
fn show_loaded_prints_every_row() {
    let obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    let eff = EffectivePolicy::new(&resolved.state, &OwnerSettings::unset());
    let seen = resolved.seen.clone();
    let src = Source::Machine(PathBuf::from("/etc/fieldbook/policy.toml"));
    let shown = show(&resolved, &eff, seen.as_ref(), &src);

    assert!(shown.text.starts_with("policy: loaded\n"), "{}", shown.text);
    assert!(shown.text.contains("revision: 3\n"), "{}", shown.text);
    assert!(shown.text.contains("np-0007"), "{}", shown.text);
    assert!(
        shown.text.contains("Supplier terms under an NDA (fixture)"),
        "{}",
        shown.text
    );
    assert!(
        shown
            .text
            .contains("NP-0007: restricted to the named counterparty"),
        "{}",
        shown.text
    );
    assert!(
        shown.text.contains("source finance-exports"),
        "{}",
        shown.text
    );
    assert!(
        shown.text.contains("marking: match=\"NP-0007 NDA\""),
        "{}",
        shown.text
    );
    assert!(shown.text.contains("ref=CHG-0001"), "{}", shown.text);
    assert_eq!(shown.report.state, "loaded");
    assert_eq!(shown.report.exit, 0);
}

#[test]
fn show_closed_prints_the_code_and_the_reason() {
    let obs = FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    let eff = EffectivePolicy::new(&resolved.state, &OwnerSettings::unset());
    let shown = show(&resolved, &eff, None, &Source::None);
    assert!(
        shown.text.starts_with("policy: closed (FB0413):"),
        "{}",
        shown.text
    );
    assert_eq!(shown.report.state, "closed");
    assert_eq!(shown.report.exit, 1);

    let writable = resolve(
        FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Err(WriterWhy::FileOther)),
        None,
        false,
    );
    let eff_w = EffectivePolicy::new(&writable.state, &OwnerSettings::unset());
    let shown_w = show(&writable, &eff_w, None, &Source::None);
    assert!(
        shown_w.text.starts_with("policy: closed (FB0418):"),
        "{}",
        shown_w.text
    );
}

#[test]
fn show_prints_the_writer_check() {
    let ok_obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(()));
    let ok_res = resolve(ok_obs, None, false);
    let ok_eff = EffectivePolicy::new(&ok_res.state, &OwnerSettings::unset());
    let ok_shown = show(&ok_res, &ok_eff, None, &Source::None);
    assert!(
        ok_shown.text.contains("writer check: ok\n"),
        "{}",
        ok_shown.text
    );

    let bad_obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Err(WriterWhy::FileOther));
    let bad_res = resolve(bad_obs, None, false);
    let bad_eff = EffectivePolicy::new(&bad_res.state, &OwnerSettings::unset());
    let bad_shown = show(&bad_res, &bad_eff, None, &Source::None);
    assert!(
        bad_shown
            .text
            .contains("writer check: the policy file is writable by a non-administrator\n"),
        "{}",
        bad_shown.text
    );
}

#[test]
fn show_prints_the_export_requirement_on_off_and_by_default() {
    let with_false = format!("{BASE_FIXTURE}\nrequire_fresh_export = false\n");
    let with_true = format!("{BASE_FIXTURE}\nrequire_fresh_export = true\n");

    for (toml_text, expected) in [
        (with_false.as_str(), "export requirement: off"),
        (with_true.as_str(), "export requirement: on"),
        (BASE_FIXTURE, "export requirement: on"),
    ] {
        let obs = FileObservation::from_bytes(toml_text.as_bytes(), Ok(()));
        let res = resolve(obs, None, false);
        let eff = EffectivePolicy::new(&res.state, &OwnerSettings::unset());
        let shown = show(&res, &eff, None, &Source::None);
        assert!(
            shown.text.contains(expected),
            "expected `{expected}` in:\n{}",
            shown.text
        );
    }
}

#[test]
fn show_marks_the_fixed_rows_built_in() {
    let resolved = resolve(FileObservation::absent(), None, false);
    let eff = EffectivePolicy::new(&resolved.state, &OwnerSettings::unset());
    let shown = show(&resolved, &eff, None, &Source::None);
    for kind in ["audit", "git", "edition"] {
        let needle = format!("destination {kind} (built in):");
        assert!(
            shown.text.contains(&needle),
            "missing `{needle}` in:\n{}",
            shown.text
        );
    }
}

#[test]
fn show_marks_the_advisory_kinds() {
    let obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    let eff = EffectivePolicy::new(&resolved.state, &OwnerSettings::unset());
    let shown = show(&resolved, &eff, None, &Source::None);
    for kind in ["handoff", "print", "clipboard"] {
        let needle = format!("destination {kind} (advisory)");
        assert!(
            shown.text.contains(&needle),
            "missing `{needle}` in:\n{}",
            shown.text
        );
    }
}

#[test]
fn show_names_the_side_that_cut_a_label() {
    let obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    let mut owner = OwnerSettings::unset();
    owner.handoff = Some(LabelAllow::from_ids([PUBLIC]));
    let eff = EffectivePolicy::new(&resolved.state, &owner);
    let shown = show(&resolved, &eff, None, &Source::None);
    assert!(
        shown.text.contains("bid-0003 (cut by owner)"),
        "{}",
        shown.text
    );
    assert!(
        shown.text.contains("ctl-0002 (cut by policy)"),
        "{}",
        shown.text
    );
}

#[test]
fn policy_line_has_the_state_and_eight_digits() {
    let floor_res = resolve(FileObservation::absent(), None, false);
    let floor_line = policy_line(&floor_res);
    let floor_hash = floor_res.observation_hash();
    assert_eq!(
        floor_line,
        format!("policy: floor {}", &floor_hash.as_str()[..8])
    );

    let loaded_res = resolve(
        FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(())),
        None,
        false,
    );
    let loaded_hash = loaded_res.observation_hash();
    assert_eq!(
        policy_line(&loaded_res),
        format!("policy: loaded {}", &loaded_hash.as_str()[..8])
    );

    let closed_res = resolve(
        FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(())),
        None,
        false,
    );
    let closed_hash = closed_res.observation_hash();
    assert_eq!(
        policy_line(&closed_res),
        format!("policy: closed {}", &closed_hash.as_str()[..8])
    );
}

#[test]
fn show_is_deterministic() {
    let obs = FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(()));
    let resolved = resolve(obs, None, false);
    let eff = EffectivePolicy::new(&resolved.state, &OwnerSettings::unset());
    let seen = resolved.seen.clone();
    let src = Source::Machine(PathBuf::from("/etc/fieldbook/policy.toml"));
    let today = CivilDate::from_ymd(2026, 3, 1).unwrap();
    let first = show_at(&resolved, &eff, seen.as_ref(), &src, today);
    let second = show_at(&resolved, &eff, seen.as_ref(), &src, today);
    assert_eq!(first, second);
}

// ---------------------------------------------------------------------------
// Step 8 & 15: Fixtures, Observations, and Single-Read / Single-Constructor Scans
// ---------------------------------------------------------------------------

#[test]
fn the_three_valid_fixtures_parse() {
    let p_base = Policy::parse(BASE_FIXTURE, None).expect("policy.toml");
    assert_eq!(p_base.revision(), 3);
    let p_lower = Policy::parse(LOWER_FIXTURE, None).expect("policy-lower.toml");
    assert_eq!(p_lower.revision(), 2);
    let p_next = Policy::parse(NEXT_FIXTURE, None).expect("policy-next.toml");
    assert_eq!(p_next.revision(), 4);
}

#[test]
fn the_bad_fixture_fails_with_fb0413() {
    let err = Policy::parse(BAD_FIXTURE, None).unwrap_err();
    assert_eq!(err.code(), Code::Fb0413);
}

#[test]
fn the_lower_fixture_is_closed_after_the_base_fixture() {
    let base_res = resolve(
        FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(())),
        None,
        false,
    );
    let seen = base_res.seen.expect("base seen");
    let lower_res = resolve(
        FileObservation::from_bytes(LOWER_FIXTURE.as_bytes(), Ok(())),
        Some(&seen),
        false,
    );
    assert_eq!(
        lower_res.state,
        PolicyState::Closed(ClosedWhy::LowerRevision)
    );
}

#[test]
fn the_next_fixture_is_loaded_after_the_base_fixture() {
    let base_res = resolve(
        FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(())),
        None,
        false,
    );
    let seen = base_res.seen.expect("base seen");
    let next_res = resolve(
        FileObservation::from_bytes(NEXT_FIXTURE.as_bytes(), Ok(())),
        Some(&seen),
        false,
    );
    assert!(matches!(next_res.state, PolicyState::Loaded(_)));
    assert_eq!(next_res.seen.unwrap().revision, 4);
}

#[test]
fn fixture_expiry_dates_are_far_off() {
    let min_date = CivilDate::from_ymd(2090, 1, 1).unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/policy");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.file_name().and_then(|s| s.to_str()) == Some("policy.toml") {
            // `policy.toml` is the `core-policy` base fixture whose date is pinned in `egress.rs` and `policy.rs`.
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap();
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("expires") {
                let rest = rest.trim_start();
                if let Some(val) = rest.strip_prefix('=') {
                    let date_str = val.trim().trim_matches('"');
                    let date = CivilDate::parse(date_str).unwrap();
                    assert!(
                        date >= min_date,
                        "{} has expires {date} before 2090-01-01",
                        path.display()
                    );
                }
            }
        }
    }
}

#[test]
fn closed_observations_keep_distinct_invalid_bytes() {
    let obs_a = FileObservation::from_bytes(b"invalid = [1", Ok(()));
    let obs_b = FileObservation::from_bytes(b"invalid = [2", Ok(()));
    let res_a = resolve(obs_a, None, false);
    let res_b = resolve(obs_b, None, false);
    assert!(res_a.state.is_closed());
    assert!(res_b.state.is_closed());
    assert_ne!(res_a.observation_hash(), res_b.observation_hash());
}

#[test]
fn identical_closed_observation_is_not_repeated() {
    let obs_1 = FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(()));
    let obs_2 = FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(()));
    let res_1 = resolve(obs_1, None, false);
    let res_2 = resolve(obs_2, None, false);
    assert_eq!(res_1.observation_hash(), res_2.observation_hash());
}

#[test]
fn oversized_observation_hashes_only_the_bounded_prefix() {
    let mut bytes_a = vec![b'a'; OVER_LIMIT_PREFIX_BYTES + 128];
    let mut bytes_b = bytes_a.clone();
    // Mutate a byte past the 1,048,577-byte bounded prefix.
    bytes_b[OVER_LIMIT_PREFIX_BYTES + 10] = b'z';
    bytes_a[0] = b'#';
    bytes_b[0] = b'#';

    let obs_a = FileObservation::from_bytes(&bytes_a, Ok(()));
    let obs_b = FileObservation::from_bytes(&bytes_b, Ok(()));
    let expected_prefix_sha = Sha256::digest(&bytes_a[..OVER_LIMIT_PREFIX_BYTES]);
    assert_eq!(
        obs_a.digest,
        ObservationDigest::Prefix {
            bytes: OVER_LIMIT_PREFIX_BYTES as u64,
            sha256: expected_prefix_sha.clone(),
        }
    );
    assert_eq!(obs_a.digest, obs_b.digest);

    let res_a = resolve(obs_a, None, false);
    let res_b = resolve(obs_b, None, false);
    assert_eq!(res_a.observation_hash(), res_b.observation_hash());

    let whole_sha = Sha256::digest(&bytes_a);
    assert_ne!(expected_prefix_sha, whole_sha);
}

#[test]
fn restoring_valid_bytes_observes_loaded_again() {
    let res_1 = resolve(
        FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(())),
        None,
        false,
    );
    let seen_1 = res_1.seen.clone().unwrap();
    let res_closed = resolve(
        FileObservation::from_bytes(BAD_FIXTURE.as_bytes(), Ok(())),
        Some(&seen_1),
        false,
    );
    assert!(res_closed.state.is_closed());
    assert_eq!(res_closed.seen.as_ref(), Some(&seen_1));

    let res_restored = resolve(
        FileObservation::from_bytes(BASE_FIXTURE.as_bytes(), Ok(())),
        res_closed.seen.as_ref(),
        false,
    );
    assert!(matches!(res_restored.state, PolicyState::Loaded(_)));
    assert_eq!(res_restored.observation_hash(), res_1.observation_hash());
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn strip_line_comments(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[test]
fn policy_is_read_in_one_place() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    collect_rs_files(&crates_dir, &mut files);

    for file in files {
        let rel = file
            .strip_prefix(&crates_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel.contains("/tests/")
            || rel.ends_with("/io/policy.rs")
            || rel.ends_with("/src/policy.rs")
            || rel.ends_with("/src/cli.rs")
            || rel.ends_with("/src/layout.rs")
        {
            continue;
        }
        let raw = std::fs::read_to_string(&file).unwrap();
        let code = strip_line_comments(&raw);
        assert!(
            !code.contains("\"policy.toml\"")
                && !code.contains("\\policy.toml")
                && !code.contains("/policy.toml")
                && !code.contains("PolicySource::read"),
            "unexpected policy.toml literal or PolicySource::read in {rel}"
        );
    }
}

#[test]
fn policy_ctx_is_built_in_one_place() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    collect_rs_files(&crates_dir, &mut files);

    for file in files {
        let rel = file
            .strip_prefix(&crates_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel.contains("/tests/") || rel.ends_with("/src/policy.rs") {
            continue;
        }
        let raw = std::fs::read_to_string(&file).unwrap();
        let code = strip_line_comments(&raw);
        assert!(
            !code.contains("PolicyCtx::loaded") && !code.contains("PolicyCtx::closed"),
            "PolicyCtx constructor called outside policy.rs and tests/ in {rel}"
        );
    }
}

#[test]
fn label_id_helper_used() {
    let id = LabelId::parse("np-0007").unwrap();
    assert_eq!(id.as_str(), "np-0007");
}
