//! Pure backup domain unit and integration tests (`fieldbook-core`).

use std::collections::BTreeMap;

use fieldbook_core::backup::{
    BACKUP_TARGET_TEMPLATE, BackupError, BackupFileKind, BackupToml, CopiedFile, PlaceFinding,
    SnapshotInfo, corpus_entries, manifest_sha256, path_budget, plan_backup, plan_retention,
    snapshot_labels, target_place, toml::encode_manifest_bytes,
};
use fieldbook_core::check::Code;
use fieldbook_core::labels::{
    ItemRef, LabelGraph, LabelId, LabelNode, LabelSet, Ledger, MARKED, OWNER_SUPPLIED, PUBLIC,
    StoreId, UNLABELLED, effective_item,
};
use fieldbook_core::labels::{PlaceFacts, RefuseReason};

fn sample_toml_text() -> &'static str {
    r#"backup_version = 1
tool_version = "0.1.0"
snapshot = 1
created_utc = "2026-01-01T09:00:00Z"
labels = ["public"]
roots = ["corpus", "state"]
files = 412
bytes = 9120344
fingerprint = "97f7c0c1aec4cc6e40449599b53079072984db7712470dbf1efb17d69de0c3d4"
journal_seq = 118
journal_head = "6e3b154116eefb2babc498c6dd381cf7129e7d8636e627e543ceeb84da80b906"
journal_state = "ok"
manifest_sha256 = "b33a8963009ad1464dc7ebe921c4c5123a9d4aef634394a6b867b5c66eb12d65"
"#
}

fn sample_sums() -> &'static str {
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  data/corpus/README.md\ncb8379ac2098aa165029e3938a51da0bcecfc008fd6795f401178647f96c5b34  data/state/settings.toml\n"
}

#[derive(Default)]
struct TestGraph {
    nodes: BTreeMap<ItemRef, LabelNode>,
    empty_ledger: Ledger,
}

impl TestGraph {
    fn insert_node(&mut self, item: ItemRef, own: LabelSet, inputs: Vec<ItemRef>) {
        self.nodes.insert(item, LabelNode::new(own, inputs));
    }
}

impl LabelGraph for TestGraph {
    fn node(&self, item: &ItemRef) -> Option<LabelNode> {
        self.nodes.get(item).cloned()
    }

    fn ledger(&self, _store: StoreId) -> &Ledger {
        &self.empty_ledger
    }
}

#[test]
fn backup_toml_denies_unknown_keys() {
    let valid = sample_toml_text();
    let parsed = BackupToml::parse(valid).expect("valid backup.toml should parse");
    assert_eq!(parsed.snapshot, 1);
    assert_eq!(parsed.backup_version, 1);
    assert_eq!(parsed.files, 412);

    let rendered = parsed.render();
    let re_parsed = BackupToml::parse(&rendered).expect("rendered backup.toml must re-parse");
    assert_eq!(parsed, re_parsed);

    let with_unknown = format!("{valid}forbidden_key = \"bad\"\n");
    let err = BackupToml::parse(&with_unknown).unwrap_err();
    assert!(matches!(err, BackupError::Toml(_)));
}

#[test]
fn retention_keeps_seven_and_four_weeks() {
    // 20 snapshots spanning across 10 calendar weeks (weeks 1 to 10, Monday-aligned).
    // Each week has 2 snapshots.
    let mut snapshots = Vec::new();
    let dates = [
        "2026-01-05T10:00:00Z", // W1
        "2026-01-06T10:00:00Z",
        "2026-01-12T10:00:00Z", // W2
        "2026-01-13T10:00:00Z",
        "2026-01-19T10:00:00Z", // W3
        "2026-01-20T10:00:00Z",
        "2026-01-26T10:00:00Z", // W4
        "2026-01-27T10:00:00Z",
        "2026-02-02T10:00:00Z", // W5
        "2026-02-03T10:00:00Z",
        "2026-02-09T10:00:00Z", // W6
        "2026-02-10T10:00:00Z",
        "2026-02-16T10:00:00Z", // W7
        "2026-02-17T10:00:00Z",
        "2026-02-23T10:00:00Z", // W8
        "2026-02-24T10:00:00Z",
        "2026-03-02T10:00:00Z", // W9
        "2026-03-03T10:00:00Z",
        "2026-03-09T10:00:00Z", // W10
        "2026-03-10T10:00:00Z",
    ];

    for (idx, date) in dates.iter().enumerate() {
        let num = u32::try_from(idx + 1).unwrap();
        snapshots.push(SnapshotInfo::new(num, *date, true));
    }

    // keep_last = 7: snapshots 14, 15, 16, 17, 18, 19, 20
    // keep_weekly = 4 (W10, W9, W8, W7): newest of each is 20, 18, 16, 14
    // Union: 14, 15, 16, 17, 18, 19, 20
    let kept = plan_retention(&snapshots, None, 7, 4);
    assert_eq!(kept, vec![14, 15, 16, 17, 18, 19, 20]);
}

#[test]
fn retention_never_removes_the_newest_verified() {
    let mut snapshots = Vec::new();
    for i in 1..=10 {
        snapshots.push(SnapshotInfo::new(
            i,
            format!("2026-01-{:02}T10:00:00Z", i),
            true,
        ));
    }

    // If keep_last = 2 and keep_weekly = 1, only 9 and 10 would be kept by count/week.
    // However, if snapshot 1 is the newest verified, it must never be removed.
    let kept = plan_retention(&snapshots, Some(1), 2, 1);
    assert!(kept.contains(&1));
    assert!(kept.contains(&9));
    assert!(kept.contains(&10));
}

#[test]
fn retention_keeps_two_if_two_exist() {
    let snapshots = vec![
        SnapshotInfo::new(1, "2026-01-01T10:00:00Z", true),
        SnapshotInfo::new(2, "2026-01-02T10:00:00Z", true),
    ];

    // Even if keep_last = 1 and keep_weekly = 0, at least 2 must be kept if 2 exist.
    let kept = plan_retention(&snapshots, None, 1, 0);
    assert_eq!(kept, vec![1, 2]);
}

#[test]
fn retention_ignores_foreign_folders() {
    let snapshots = vec![
        SnapshotInfo::new(1, "2026-01-01T10:00:00Z", true),
        SnapshotInfo::new(2, "2026-01-02T10:00:00Z", true),
        SnapshotInfo::new(99, "2026-01-03T10:00:00Z", false), // Missing backup.toml
    ];

    let kept = plan_retention(&snapshots, None, 2, 1);
    assert_eq!(kept, vec![1, 2]);
    assert!(!kept.contains(&99));
}

#[test]
fn synced_paths_come_from_the_place_test() {
    // A target whose path string looks like a cloud sync folder ("OneDrive/Backups")
    // but whose PlaceFacts has sync_root = false is treated as a local folder.
    let local_facts = PlaceFacts::local();
    let finding_fake = target_place(false, "folder", local_facts, false);
    assert_eq!(finding_fake, None);

    // Conversely, a path with PlaceFacts.sync_root = true triggers the synced path rule.
    let sync_facts = PlaceFacts {
        sync_root: true,
        ..PlaceFacts::local()
    };
    let finding_real = target_place(false, "folder", sync_facts, false);
    assert_eq!(finding_real, Some(PlaceFinding::Warning(Code::Fb0813)));
}

#[test]
fn path_budget_names_the_shortest_root() {
    // 260-character budget check.
    // Staged path is: target + "/fieldbook-backup-0001.partial/data/" (36 chars) + longest_rel
    // Suppose longest_rel is 50 chars.
    let longest_rel = "a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s/t/u/v/w/x/y/z.txt";
    // Target of length 200: 200 + 36 + 55 = 291 > 260.
    let long_target = format!("/mnt/storage/{}", "x".repeat(187));
    let err = path_budget(&long_target, longest_rel).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("exceeds 260-character budget"),
        "unexpected error message: {msg}"
    );
    assert!(
        msg.contains("shortest working root is `/`"),
        "error must name shortest working root: {msg}"
    );

    // Windows target naming C:\ as shortest root.
    let win_target = format!("C:\\backups\\{}", "y".repeat(190));
    let win_err = path_budget(&win_target, longest_rel).unwrap_err();
    assert!(
        win_err
            .to_string()
            .contains("shortest working root is `C:\\`")
    );

    // Short target passes.
    assert!(path_budget("/tmp/backup", "corpus/pages/index.md").is_ok());
}

#[test]
fn home_plain_folder_has_no_place_finding() {
    let facts = PlaceFacts::local();
    let outcome = target_place(false, "folder", facts, false);
    assert_eq!(outcome, None);
}

#[test]
fn home_it_location_has_no_place_finding() {
    let facts = PlaceFacts::local();
    let outcome = target_place(false, "it-location", facts, false);
    assert_eq!(outcome, None);

    // Network drive is also valid.
    let net_facts = PlaceFacts {
        network: true,
        ..PlaceFacts::local()
    };
    let net_outcome = target_place(false, "it-location", net_facts, false);
    assert_eq!(net_outcome, None);
}

#[test]
fn home_synced_path_warns_fb0813() {
    let sync_facts = PlaceFacts {
        sync_root: true,
        ..PlaceFacts::local()
    };

    // Undeclared kind gives warning FB0813.
    let warning = target_place(false, "folder", sync_facts, false);
    assert_eq!(warning, Some(PlaceFinding::Warning(Code::Fb0813)));

    // When target says kind = "synced", it is a one-line notice.
    let notice = target_place(false, "synced", sync_facts, false);
    assert_eq!(notice, Some(PlaceFinding::Notice));
}

#[test]
fn laptop_plain_folder_has_no_place_finding() {
    let facts = PlaceFacts::local();
    let outcome = target_place(true, "folder", facts, false);
    assert_eq!(outcome, None);
}

#[test]
fn laptop_it_location_has_no_place_finding() {
    let facts = PlaceFacts::local();
    let outcome = target_place(true, "it-location", facts, false);
    assert_eq!(outcome, None);
}

#[test]
fn laptop_synced_path_is_fb0415_unless_a_row_names_it() {
    let sync_facts = PlaceFacts {
        sync_root: true,
        ..PlaceFacts::local()
    };

    // Undeclared synced path on laptop is FB0415 refusal.
    let refused = target_place(true, "folder", sync_facts, false);
    assert!(matches!(
        refused,
        Some(PlaceFinding::Refusal(ref r)) if r.code == Code::Fb0415 && r.reason == RefuseReason::Place
    ));

    // kind = "synced" does NOT lift it on the laptop.
    let refused_synced_kind = target_place(true, "synced", sync_facts, false);
    assert!(matches!(
        refused_synced_kind,
        Some(PlaceFinding::Refusal(ref r)) if r.code == Code::Fb0415
    ));

    // Reparse point on laptop is also a synced path.
    let reparse_facts = PlaceFacts {
        reparse: true,
        ..PlaceFacts::local()
    };
    let refused_reparse = target_place(true, "folder", reparse_facts, false);
    assert!(matches!(
        refused_reparse,
        Some(PlaceFinding::Refusal(ref r)) if r.code == Code::Fb0415
    ));

    // If declared by a backup row's paths, there is no place finding.
    let declared = target_place(true, "folder", sync_facts, true);
    assert_eq!(declared, None);
}

#[test]
fn snapshot_labels_are_the_union() {
    let files = vec![
        CopiedFile::new(
            "corpus/pages/p1.md",
            BackupFileKind::Page,
            LabelSet::public(),
        ),
        CopiedFile::new(
            "state/audit/journal.jsonl",
            BackupFileKind::TextFreeControl,
            LabelSet::public(),
        ),
        CopiedFile::new(
            "corpus/sources/public/S-0001/doc.pdf",
            BackupFileKind::Original,
            LabelSet::from_ids([OWNER_SUPPLIED]),
        ),
    ];

    let labels = snapshot_labels(&files);
    assert_eq!(labels, LabelSet::from_ids([PUBLIC, OWNER_SUPPLIED]));
}

#[test]
fn derived_file_labels_follow_current_inputs() {
    let np = LabelId::parse("np-0007").unwrap();
    let restricted_source = ItemRef::new(StoreId::Overlay, "SO-0001-v1");
    let derived_page = ItemRef::new(StoreId::Overlay, "page-001");

    let mut graph = TestGraph::default();
    graph.insert_node(
        restricted_source.clone(),
        LabelSet::from_ids([np.clone()]),
        vec![],
    );
    graph.insert_node(
        derived_page.clone(),
        LabelSet::public(),
        vec![restricted_source],
    );

    let effective = effective_item(&graph, &derived_page);
    assert_eq!(effective, LabelSet::from_ids([np.clone()]));

    let files = vec![CopiedFile::new(
        "overlay/pages/page-001.md",
        BackupFileKind::Page,
        effective,
    )];
    let snap_labels = snapshot_labels(&files);
    assert!(snap_labels.contains(&np));
}

#[test]
fn control_files_add_no_label() {
    // Only TextFreeControl files => LabelSet::public().
    let only_controls = vec![
        CopiedFile::new(
            "state/audit/journal.jsonl",
            BackupFileKind::TextFreeControl,
            LabelSet::public(),
        ),
        CopiedFile::new(
            "state/lock",
            BackupFileKind::TextFreeControl,
            LabelSet::public(),
        ),
    ];
    assert_eq!(snapshot_labels(&only_controls), LabelSet::public());

    // Mixed: TextFreeControl contributes no labels to the non-control items.
    let mixed = vec![
        CopiedFile::new(
            "state/audit/journal.jsonl",
            BackupFileKind::TextFreeControl,
            LabelSet::public(),
        ),
        CopiedFile::new(
            "corpus/sources/public/S-0001/doc.pdf",
            BackupFileKind::Original,
            LabelSet::from_ids([OWNER_SUPPLIED]),
        ),
    ];
    assert_eq!(
        snapshot_labels(&mixed),
        LabelSet::from_ids([OWNER_SUPPLIED])
    );
}

#[test]
fn a_file_that_should_declare_labels_and_does_not_adds_unlabelled() {
    // Known kind with unlabelled / undeclared label set adds unlabelled.
    let unlabelled_page = vec![CopiedFile::new(
        "corpus/pages/missing.md",
        BackupFileKind::Page,
        LabelSet::unlabelled(),
    )];
    let labels = snapshot_labels(&unlabelled_page);
    assert_eq!(labels, LabelSet::unlabelled());

    // Stray file adds unlabelled even if it carried public.
    let stray = vec![
        CopiedFile::new(
            "corpus/pages/index.md",
            BackupFileKind::Page,
            LabelSet::public(),
        ),
        CopiedFile::new(
            "corpus/stray.tmp",
            BackupFileKind::Stray,
            LabelSet::public(),
        ),
    ];
    let stray_labels = snapshot_labels(&stray);
    assert!(stray_labels.contains(&UNLABELLED));
}

#[test]
fn snapshot_labels_keep_every_label_beside_unlabelled() {
    let np = LabelId::parse("np-0007").unwrap();
    let files = vec![
        CopiedFile::new(
            "corpus/pages/p1.md",
            BackupFileKind::Page,
            LabelSet::from_ids([UNLABELLED, MARKED]),
        ),
        CopiedFile::new(
            "corpus/sources/public/S-0001/doc.pdf",
            BackupFileKind::Original,
            LabelSet::from_ids([UNLABELLED, np.clone()]),
        ),
    ];
    let labels = snapshot_labels(&files);
    assert!(labels.contains(&UNLABELLED));
    assert!(labels.contains(&MARKED));
    assert!(labels.contains(&np));
}

#[test]
fn manifest_identity_vector() {
    let toml = BackupToml::parse(sample_toml_text()).unwrap();
    let sums = sample_sums();

    let bytes = encode_manifest_bytes(&toml, sums).expect("manifest should encode");

    // Independently computed with:
    // printf 'backup-manifest 1\nbackup_version 1\ntool_version 5:0.1.0\nsnapshot 1\ncreated_utc 2026-01-01T09:00:00Z\nlabels 1 6:public\nroots 2 6:corpus 5:state\nfiles 412\nbytes 9120344\nfingerprint 64:97f7c0c1aec4cc6e40449599b53079072984db7712470dbf1efb17d69de0c3d4\njournal_seq 118\njournal_head 64:6e3b154116eefb2babc498c6dd381cf7129e7d8636e627e543ceeb84da80b906\njournal_state 2:ok\nsums 179:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  data/corpus/README.md\ncb8379ac2098aa165029e3938a51da0bcecfc008fd6795f401178647f96c5b34  data/state/settings.toml\n\n' | sha256sum
    let expected_bytes = b"backup-manifest 1\nbackup_version 1\ntool_version 5:0.1.0\nsnapshot 1\ncreated_utc 2026-01-01T09:00:00Z\nlabels 1 6:public\nroots 2 6:corpus 5:state\nfiles 412\nbytes 9120344\nfingerprint 64:97f7c0c1aec4cc6e40449599b53079072984db7712470dbf1efb17d69de0c3d4\njournal_seq 118\njournal_head 64:6e3b154116eefb2babc498c6dd381cf7129e7d8636e627e543ceeb84da80b906\njournal_state 2:ok\nsums 179:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  data/corpus/README.md\ncb8379ac2098aa165029e3938a51da0bcecfc008fd6795f401178647f96c5b34  data/state/settings.toml\n\n";
    assert_eq!(bytes, expected_bytes);

    let digest = manifest_sha256(&toml, sums).expect("manifest sha256 should compute");
    assert_eq!(
        digest.as_str(),
        "b33a8963009ad1464dc7ebe921c4c5123a9d4aef634394a6b867b5c66eb12d65"
    );
}

#[test]
fn manifest_metadata_or_sums_change_refuses() {
    let base_toml = BackupToml::parse(sample_toml_text()).unwrap();
    let base_sums = sample_sums();
    let base_digest = manifest_sha256(&base_toml, base_sums).unwrap();

    // Changed labels modifies digest.
    let mut modified_labels = base_toml.clone();
    modified_labels.labels = vec!["np-0007".to_string(), "public".to_string()];
    let mod_digest = manifest_sha256(&modified_labels, base_sums).unwrap();
    assert_ne!(base_digest, mod_digest);

    // Unsorted labels refuses.
    let mut unsorted_labels = base_toml.clone();
    unsorted_labels.labels = vec!["public".to_string(), "np-0007".to_string()];
    assert!(matches!(
        manifest_sha256(&unsorted_labels, base_sums),
        Err(BackupError::UnsortedList {
            field: "labels",
            ..
        })
    ));

    // Changed roots modifies digest.
    let mut modified_roots = base_toml.clone();
    modified_roots.roots = vec!["corpus".to_string()];
    let mod_roots_digest = manifest_sha256(&modified_roots, base_sums).unwrap();
    assert_ne!(base_digest, mod_roots_digest);

    // Unsorted roots refuses.
    let mut unsorted_roots = base_toml.clone();
    unsorted_roots.roots = vec!["state".to_string(), "corpus".to_string()];
    assert!(matches!(
        manifest_sha256(&unsorted_roots, base_sums),
        Err(BackupError::UnsortedList { field: "roots", .. })
    ));

    // Changed checksum modifies digest.
    let mut changed_sums = base_sums.to_string();
    changed_sums.replace_range(0..1, "f");
    let mod_sums_digest = manifest_sha256(&base_toml, &changed_sums).unwrap();
    assert_ne!(base_digest, mod_sums_digest);

    // Noncanonical sums (unsorted paths) refuses.
    let unsorted_sums = "cb8379ac2098aa165029e3938a51da0bcecfc008fd6795f401178647f96c5b34  data/state/settings.toml\nba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  data/corpus/README.md\n";
    assert!(matches!(
        manifest_sha256(&base_toml, unsorted_sums),
        Err(BackupError::NonCanonicalSums(_))
    ));
}

#[test]
fn plan_backup_excludes_derived_and_applies_include() {
    let listing = [
        "corpus/pages/index.md",
        "corpus/sources/public/S-0001/doc.pdf",
        "state/audit/journal.jsonl",
        "state/pages/derived.md",      // derived folder: excluded
        "state/drill/snapshot-01.txt", // derived folder: excluded
        "state/site/index.html",       // derived folder: excluded
        "state/index/terms.bin",       // derived folder: excluded
    ];

    let plan = plan_backup(&listing, &[] as &[&str], 1);
    assert_eq!(plan.snapshot, 1);
    assert_eq!(plan.roots, vec!["corpus", "state"]);
    assert_eq!(
        plan.files,
        vec![
            "corpus/pages/index.md",
            "corpus/sources/public/S-0001/doc.pdf",
            "state/audit/journal.jsonl",
        ]
    );

    // include = ["audit"] copies only state/audit/
    let audit_plan = plan_backup(&listing, &["audit"], 2);
    assert_eq!(audit_plan.snapshot, 2);
    assert_eq!(audit_plan.roots, vec!["state"]);
    assert_eq!(audit_plan.files, vec!["state/audit/journal.jsonl"]);
}

#[test]
fn corpus_entries_extracts_pairs() {
    let sums = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  data/corpus/README.md\ncb8379ac2098aa165029e3938a51da0bcecfc008fd6795f401178647f96c5b34  data/state/settings.toml\n";
    let entries = corpus_entries(sums);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0],
        (
            "corpus/README.md".to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_string(),
        )
    );
}

#[test]
fn backup_target_template_is_defined() {
    assert!(BACKUP_TARGET_TEMPLATE.contains("[[backup.target]]"));
    assert!(BACKUP_TARGET_TEMPLATE.contains("kind = \"folder\""));
}
