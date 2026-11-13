//! Pure restore planning and rename help tests (`fieldbook-core`).

use std::path::{Path, PathBuf};

use fieldbook_core::backup::{
    BackupToml, DRILL_STEPS, DestinationFacts, PlannedRoot, RestorePlan, RestoreRefusalKind,
    RestoreShell, plan_restore, rename_help, snapshot_entry_tail,
};
use fieldbook_core::check::Code;

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

#[test]
fn destination_rules() {
    assert_eq!(
        DRILL_STEPS,
        ["snapshot", "restore", "check", "journal", "fingerprint"]
    );

    let toml = BackupToml::parse(sample_toml_text()).expect("sample toml should parse");
    let live_roots = [PathBuf::from("/live/corpus"), PathBuf::from("/live/state")];

    // missing destination ok and roots are dest/corpus, dest/state
    let dest = Path::new("/restore/dest");
    let missing_facts = DestinationFacts {
        exists: false,
        empty: false,
    };
    let plan = plan_restore(&toml, dest, missing_facts, &live_roots)
        .expect("missing destination should succeed");
    assert_eq!(plan.destination, dest);
    assert_eq!(
        plan.roots,
        vec![
            PlannedRoot {
                name: "corpus".to_string(),
                path: dest.join("corpus"),
            },
            PlannedRoot {
                name: "state".to_string(),
                path: dest.join("state"),
            },
        ]
    );

    // existing empty ok
    let empty_facts = DestinationFacts {
        exists: true,
        empty: true,
    };
    let plan_empty = plan_restore(&toml, dest, empty_facts, &live_roots)
        .expect("existing empty destination should succeed");
    assert_eq!(plan_empty, plan);

    // existing non-empty -> NotEmpty with code Fb0815
    let nonempty_facts = DestinationFacts {
        exists: true,
        empty: false,
    };
    let err_not_empty = plan_restore(&toml, dest, nonempty_facts, &live_roots).unwrap_err();
    assert_eq!(err_not_empty.kind, RestoreRefusalKind::NotEmpty);
    assert_eq!(err_not_empty.code(), Code::Fb0815);

    // destination equal to a live root -> IsLiveRoot
    let live_root_dest = Path::new("/live/corpus");
    let err_is_live = plan_restore(&toml, live_root_dest, missing_facts, &live_roots).unwrap_err();
    assert_eq!(err_is_live.kind, RestoreRefusalKind::IsLiveRoot);
    assert_eq!(err_is_live.code(), Code::Fb0815);

    // destination that is a parent of a live root -> ContainsLiveRoot
    let parent_dest = Path::new("/live");
    let err_contains_live =
        plan_restore(&toml, parent_dest, missing_facts, &live_roots).unwrap_err();
    assert_eq!(err_contains_live.kind, RestoreRefusalKind::ContainsLiveRoot);
    assert_eq!(err_contains_live.code(), Code::Fb0815);

    // destination inside a live root -> InsideLiveRoot
    let inside_dest = Path::new("/live/corpus/nested");
    let err_inside_live = plan_restore(&toml, inside_dest, missing_facts, &live_roots).unwrap_err();
    assert_eq!(err_inside_live.kind, RestoreRefusalKind::InsideLiveRoot);
    assert_eq!(err_inside_live.code(), Code::Fb0815);

    // live-root check beats non-empty
    let err_beats_nonempty =
        plan_restore(&toml, live_root_dest, nonempty_facts, &live_roots).unwrap_err();
    assert_eq!(err_beats_nonempty.kind, RestoreRefusalKind::IsLiveRoot);

    // each detail string differs
    let details = [
        &err_not_empty.detail,
        &err_is_live.detail,
        &err_contains_live.detail,
        &err_inside_live.detail,
    ];
    for i in 0..details.len() {
        for j in (i + 1)..details.len() {
            assert_ne!(
                details[i], details[j],
                "detail strings at {i} and {j} must differ: {:?}",
                details[i]
            );
        }
    }
}

#[test]
fn rename_help_is_stable() {
    let plan = RestorePlan {
        destination: PathBuf::from("/restore/dest"),
        roots: vec![
            PlannedRoot {
                name: "corpus".to_string(),
                path: PathBuf::from("/restore/dest/corpus"),
            },
            PlannedRoot {
                name: "state".to_string(),
                path: PathBuf::from("/restore/dest/state"),
            },
        ],
    };

    let live = vec![
        ("corpus".to_string(), PathBuf::from("/var/data/it's corpus")),
        ("state".to_string(), PathBuf::from("/var/data/state")),
    ];

    let sh_lines = rename_help(&plan, &live, RestoreShell::Sh);
    assert_eq!(
        sh_lines,
        vec![
            "mv '/var/data/it'\\''s corpus' '/var/data/it'\\''s corpus.old'",
            "mv '/restore/dest/corpus' '/var/data/it'\\''s corpus'",
            "mv '/var/data/state' '/var/data/state.old'",
            "mv '/restore/dest/state' '/var/data/state'",
        ]
    );

    let pwsh_lines = rename_help(&plan, &live, RestoreShell::PowerShell);
    assert_eq!(
        pwsh_lines,
        vec![
            "Move-Item -LiteralPath '/var/data/it''s corpus' -Destination '/var/data/it''s corpus.old'",
            "Move-Item -LiteralPath '/restore/dest/corpus' -Destination '/var/data/it''s corpus'",
            "Move-Item -LiteralPath '/var/data/state' -Destination '/var/data/state.old'",
            "Move-Item -LiteralPath '/restore/dest/state' -Destination '/var/data/state'",
        ]
    );

    // and a root with no live entry yields no lines
    let live_partial = vec![("corpus".to_string(), PathBuf::from("/var/data/it's corpus"))];
    let sh_partial = rename_help(&plan, &live_partial, RestoreShell::Sh);
    assert_eq!(
        sh_partial,
        vec![
            "mv '/var/data/it'\\''s corpus' '/var/data/it'\\''s corpus.old'",
            "mv '/restore/dest/corpus' '/var/data/it'\\''s corpus'",
        ]
    );

    let pwsh_partial = rename_help(&plan, &live_partial, RestoreShell::PowerShell);
    assert_eq!(
        pwsh_partial,
        vec![
            "Move-Item -LiteralPath '/var/data/it''s corpus' -Destination '/var/data/it''s corpus.old'",
            "Move-Item -LiteralPath '/restore/dest/corpus' -Destination '/var/data/it''s corpus'",
        ]
    );

    let empty_live: Vec<(String, PathBuf)> = Vec::new();
    assert!(rename_help(&plan, &empty_live, RestoreShell::Sh).is_empty());
    assert!(rename_help(&plan, &empty_live, RestoreShell::PowerShell).is_empty());
}

#[test]
fn rename_help_follows_a_restore_moved_with_its_live_root() {
    // `--to <state>/drill/restored` is allowed, and moving the live state folder aside takes the
    // restored copy with it: the second state line must read it from under `<state>.old`.
    let plan = RestorePlan {
        destination: PathBuf::from("/home/.fieldbook/drill/restored"),
        roots: vec![
            PlannedRoot {
                name: "corpus".to_string(),
                path: PathBuf::from("/home/.fieldbook/drill/restored/corpus"),
            },
            PlannedRoot {
                name: "state".to_string(),
                path: PathBuf::from("/home/.fieldbook/drill/restored/state"),
            },
        ],
    };
    let live = vec![
        ("corpus".to_string(), PathBuf::from("/home/corpus")),
        ("state".to_string(), PathBuf::from("/home/.fieldbook")),
    ];

    assert_eq!(
        rename_help(&plan, &live, RestoreShell::Sh),
        vec![
            "mv '/home/corpus' '/home/corpus.old'",
            "mv '/home/.fieldbook/drill/restored/corpus' '/home/corpus'",
            "mv '/home/.fieldbook' '/home/.fieldbook.old'",
            "mv '/home/.fieldbook.old/drill/restored/state' '/home/.fieldbook'",
        ]
    );
    assert_eq!(
        rename_help(&plan, &live, RestoreShell::PowerShell),
        vec![
            "Move-Item -LiteralPath '/home/corpus' -Destination '/home/corpus.old'",
            "Move-Item -LiteralPath '/home/.fieldbook/drill/restored/corpus' -Destination '/home/corpus'",
            "Move-Item -LiteralPath '/home/.fieldbook' -Destination '/home/.fieldbook.old'",
            "Move-Item -LiteralPath '/home/.fieldbook.old/drill/restored/state' -Destination '/home/.fieldbook'",
        ]
    );
}

#[test]
fn snapshot_entry_tail_takes_plain_paths_under_a_root_only() {
    let roots = vec!["corpus".to_string(), "state".to_string()];

    assert_eq!(
        snapshot_entry_tail("data/corpus/pages/a.md", &roots),
        Ok("corpus/pages/a.md")
    );
    assert_eq!(
        snapshot_entry_tail("data/state/settings.toml", &roots),
        Ok("state/settings.toml")
    );

    for unsafe_entry in [
        "data/state/../../settings.toml",
        "data/corpus/../state/settings.toml",
        "data/../escape.txt",
        "data/corpus/./a.md",
        "data/corpus//a.md",
        "data//etc/passwd",
        "data/corpus/a\\..\\..\\b",
        "data/corpus",
        "data/corpus/",
        "data/other/a.md",
        "backup.toml",
        "/data/corpus/a.md",
        "data/corpus/..",
    ] {
        assert!(
            snapshot_entry_tail(unsafe_entry, &roots).is_err(),
            "`{unsafe_entry}` must be refused"
        );
    }
}
