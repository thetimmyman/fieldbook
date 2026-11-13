//! Integration tests for the restore verb and restore drill (AD-16, restore-and-drill).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "integration tests create temporary fixture directories and assert test conditions"
)]

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use fieldbook::Outcome;
use fieldbook::World;
use fieldbook::backup::{BackupOptions, run_backup};
use fieldbook::restore::{RestoreOptions, run_restore, set_after_copy_hook_for_test};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::Now;
use fieldbook_core::backup::{BackupToml, manifest_sha256};
use fieldbook_core::policy::{EffectivePolicy, FloorKind, OwnerSettings, PolicyState};

use support::world::TestWorld;

fn floor_a_effective() -> EffectivePolicy {
    EffectivePolicy::new(&PolicyState::Floor(FloorKind::A), &OwnerSettings::unset())
}

struct TestFixture {
    _tree_dir: tempfile::TempDir,
    _state_dir: tempfile::TempDir,
    target_dir: tempfile::TempDir,
    scratch: tempfile::TempDir,
    world: TestWorld,
}

impl TestFixture {
    fn new() -> Self {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let corpus_mini = repo.join("tests/fixtures/corpus-mini");
        let state_mini = repo.join("tests/fixtures/state-mini");

        let tree_dir = support::temp_copy(&corpus_mini);
        let state_dir = support::temp_copy(&state_mini);
        let target_dir = tempfile::tempdir().expect("target tempdir");
        let target_path = target_dir.path().to_path_buf();

        let settings_path = state_dir.path().join("settings.toml");
        let settings_text = format!(
            "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
            target_path.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(&settings_path, settings_text).expect("write settings.toml");

        let world = TestWorld::with_state(tree_dir.path(), state_dir.path());
        let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read corpus-mini tree");
        let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
            &tree,
            "1111111111111111111111111111111111111111",
        )
        .expect("catalogue from corpus-mini");
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
        Self {
            _tree_dir: tree_dir,
            _state_dir: state_dir,
            target_dir,
            scratch: tempfile::tempdir().expect("scratch tempdir"),
            world,
        }
    }

    /// A path outside the data root that does not exist yet and is cleaned up with the fixture.
    fn dest(&self, name: &str) -> PathBuf {
        self.scratch.path().join(name)
    }

    fn tree_root(&self) -> &Path {
        self.world.tree_root()
    }

    fn state_dir(&self) -> &Path {
        self.world.state_dir()
    }

    fn target_path(&self) -> &Path {
        self.target_dir.path()
    }

    fn backup(&self) {
        let outcome = run_backup(
            &self.world,
            &floor_a_effective(),
            &BackupOptions {
                target: Some("main".to_string()),
                to: None,
                dry_run: false,
            },
        );
        assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);
    }

    fn restore(&self, to: Option<PathBuf>, drill: bool) -> Outcome {
        run_restore(
            &self.world,
            &floor_a_effective(),
            &RestoreOptions {
                snapshot: 1,
                to,
                drill,
                target: None,
                from: None,
            },
        )
    }

    fn snap_dir(&self) -> PathBuf {
        self.target_path().join("fieldbook-backup-0001")
    }
}

fn rewrite_meta<F>(snap_dir: &Path, mutate: F)
where
    F: FnOnce(&mut BackupToml),
{
    let toml_path = snap_dir.join("backup.toml");
    let sums_path = snap_dir.join("SHA256SUMS");
    let toml_text = std::fs::read_to_string(&toml_path).expect("read backup.toml");
    let sums_text = std::fs::read_to_string(&sums_path).expect("read SHA256SUMS");
    let mut info = BackupToml::parse(&toml_text).expect("parse backup.toml");
    mutate(&mut info);
    let new_manifest = manifest_sha256(&info, &sums_text).expect("manifest_sha256");
    info.manifest_sha256 = new_manifest.as_str().to_string();
    let rewritten = toml::to_string_pretty(&info).expect("serialize backup.toml");
    std::fs::write(&toml_path, rewritten).expect("write backup.toml");
}

fn rewrite_meta_stale<F>(snap_dir: &Path, mutate: F)
where
    F: FnOnce(&mut BackupToml),
{
    let toml_path = snap_dir.join("backup.toml");
    let toml_text = std::fs::read_to_string(&toml_path).expect("read backup.toml");
    let mut info = BackupToml::parse(&toml_text).expect("parse backup.toml");
    mutate(&mut info);
    let rewritten = toml::to_string_pretty(&info).expect("serialize backup.toml");
    std::fs::write(&toml_path, rewritten).expect("write backup.toml");
}

fn journal_kinds(state_dir: &Path) -> Vec<String> {
    let audit_dir = state_dir.join("audit");
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&audit_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                files.push(p);
            }
        }
    }
    files.sort();
    let mut kinds = Vec::new();
    for f in files {
        let content = std::fs::read_to_string(&f).expect("read journal segment");
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let val: serde_json::Value = serde_json::from_str(trimmed).expect("parse journal line");
            if let Some(kind) = val.get("kind").and_then(|k| k.as_str())
                && kind != "journal.open"
            {
                kinds.push(kind.to_string());
            }
        }
    }
    kinds
}

fn drill_folder(state: &Path) -> PathBuf {
    state.join("drill")
}

fn assert_trees_equal(a: &Path, b: &Path) {
    fn collect_entries(
        base: &Path,
        dir: &Path,
        files: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
        dirs: &mut std::collections::BTreeSet<PathBuf>,
    ) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
        for entry in entries.flatten() {
            let p = entry.path();
            let rel = p.strip_prefix(base).expect("strip prefix").to_path_buf();
            if p.is_dir() {
                dirs.insert(rel.clone());
                collect_entries(base, &p, files, dirs);
            } else if p.is_file() {
                let bytes =
                    std::fs::read(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
                files.insert(rel, bytes);
            }
        }
    }

    let mut files_a = std::collections::BTreeMap::new();
    let mut dirs_a = std::collections::BTreeSet::new();
    collect_entries(a, a, &mut files_a, &mut dirs_a);

    let mut files_b = std::collections::BTreeMap::new();
    let mut dirs_b = std::collections::BTreeSet::new();
    collect_entries(b, b, &mut files_b, &mut dirs_b);

    assert_eq!(
        dirs_a,
        dirs_b,
        "directories differ between {} and {}",
        a.display(),
        b.display()
    );
    assert_eq!(
        files_a.keys().collect::<Vec<_>>(),
        files_b.keys().collect::<Vec<_>>(),
        "files differ between {} and {}",
        a.display(),
        b.display()
    );
    for (rel, content_a) in &files_a {
        let content_b = &files_b[rel];
        assert_eq!(
            content_a,
            content_b,
            "file {} differs between {} and {}",
            rel.display(),
            a.display(),
            b.display()
        );
    }
}

#[test]
fn restore_to_a_new_folder_matches_the_snapshot() {
    let fix = TestFixture::new();
    fix.backup();

    let corpus_before = support::temp_copy(&fix.tree_root().join("corpus"));
    let settings_before =
        std::fs::read_to_string(fix.state_dir().join("settings.toml")).expect("read settings");
    let kinds_before = journal_kinds(fix.state_dir());

    let dest = fix.dest("restored");
    assert!(!dest.exists());

    let outcome = fix.restore(Some(dest.clone()), false);
    assert_eq!(outcome.code, 0, "restore failed: {}", outcome.stderr);
    assert!(
        outcome.stdout.starts_with("restored snapshot 1 into"),
        "stdout: {}",
        outcome.stdout
    );

    assert_trees_equal(&dest.join("corpus"), &fix.tree_root().join("corpus"));
    assert_trees_equal(&dest.join("state"), &fix.snap_dir().join("data/state"));

    assert_trees_equal(&fix.tree_root().join("corpus"), corpus_before.path());
    assert_eq!(
        std::fs::read_to_string(fix.state_dir().join("settings.toml")).expect("read settings"),
        settings_before
    );
    let kinds_after = journal_kinds(fix.state_dir());
    assert_eq!(
        &kinds_after[kinds_before.len()..],
        &["restore.begin", "restore.end"]
    );
}

// control: ED-04
#[test]
fn restore_into_a_non_empty_folder_is_fb0815() {
    let fix = TestFixture::new();
    fix.backup();

    let dest = tempfile::tempdir().expect("dest");
    let test_file = dest.path().join("existing.txt");
    std::fs::write(&test_file, "untouched content").expect("write test file");

    let outcome = fix.restore(Some(dest.path().to_path_buf()), false);
    assert_eq!(outcome.code, 1);
    assert!(
        outcome.stderr.contains("FB0815"),
        "stderr: {}",
        outcome.stderr
    );

    assert_eq!(
        std::fs::read_to_string(&test_file).expect("read test file"),
        "untouched content"
    );
    let kinds_after = journal_kinds(fix.state_dir());
    assert!(!kinds_after.iter().any(|k| k == "restore.begin"));
}

#[test]
fn restore_over_a_live_root_is_fb0815() {
    let fix = TestFixture::new();
    fix.backup();

    let dest_is = fix.tree_root().join("corpus");
    let out_is = fix.restore(Some(dest_is), false);
    assert_eq!(out_is.code, 1);
    assert!(
        out_is.stderr.contains("FB0815"),
        "stderr: {}",
        out_is.stderr
    );
    assert!(
        out_is.stderr.contains("is a live root"),
        "stderr: {}",
        out_is.stderr
    );

    let dest_contains = fix.tree_root().to_path_buf();
    let out_contains = fix.restore(Some(dest_contains), false);
    assert_eq!(out_contains.code, 1);
    assert!(
        out_contains.stderr.contains("FB0815"),
        "stderr: {}",
        out_contains.stderr
    );
    assert!(
        out_contains.stderr.contains("contains a live root"),
        "stderr: {}",
        out_contains.stderr
    );

    let dest_inside = fix.tree_root().join("corpus/sub");
    let out_inside = fix.restore(Some(dest_inside.clone()), false);
    assert_eq!(out_inside.code, 1);
    assert!(
        out_inside.stderr.contains("FB0815"),
        "stderr: {}",
        out_inside.stderr
    );
    assert!(
        out_inside.stderr.contains("lies inside a live root"),
        "stderr: {}",
        out_inside.stderr
    );

    assert_ne!(out_is.stderr, out_contains.stderr);
    assert_ne!(out_contains.stderr, out_inside.stderr);
    assert_ne!(out_is.stderr, out_inside.stderr);
    assert!(!dest_inside.exists(), "nothing should be written");
}

#[test]
fn restore_of_a_bad_snapshot_names_the_verify_code() {
    // 1. Flip a byte in a data file -> FB0815 and FB0810 on first stderr line
    {
        let fix1 = TestFixture::new();
        fix1.backup();
        let target_data_file = fix1.snap_dir().join("data/corpus/corpus.toml");
        let mut content = std::fs::read(&target_data_file).expect("read file");
        content[0] ^= 0xFF;
        std::fs::write(&target_data_file, content).expect("write file");

        let dest1 = fix1.dest("restored");
        let outcome1 = fix1.restore(Some(dest1.clone()), false);
        assert_eq!(outcome1.code, 1);
        let first_line = outcome1.stderr.lines().next().unwrap_or("");
        assert!(
            first_line.contains("FB0815") && first_line.contains("FB0810"),
            "first line: {first_line}"
        );
        assert!(!dest1.exists());
    }

    // 2. Add an unlisted file under data/ -> FB0811
    {
        let fix2 = TestFixture::new();
        fix2.backup();
        let unlisted_file = fix2.snap_dir().join("data/unlisted.txt");
        std::fs::write(&unlisted_file, "unexpected").expect("write unlisted");

        let dest2 = fix2.dest("restored");
        let outcome2 = fix2.restore(Some(dest2.clone()), false);
        assert_eq!(outcome2.code, 1);
        assert!(
            outcome2.stderr.contains("FB0811"),
            "stderr: {}",
            outcome2.stderr
        );
        assert!(!dest2.exists());
    }

    // 3. rewrite_meta setting journal_seq to a seq not in the snapshot journal -> FB0802
    {
        let fix3 = TestFixture::new();
        fix3.backup();
        rewrite_meta(&fix3.snap_dir(), |info| {
            info.journal_seq = 999_999;
        });

        let dest3 = fix3.dest("restored");
        let outcome3 = fix3.restore(Some(dest3.clone()), false);
        assert_eq!(outcome3.code, 1);
        assert!(
            outcome3.stderr.contains("FB0802"),
            "stderr: {}",
            outcome3.stderr
        );
        assert!(!dest3.exists());
    }
}

#[test]
fn restore_from_a_folder_needs_no_settings() {
    let fix = TestFixture::new();
    fix.backup();
    let settings_path = fix.state_dir().join("settings.toml");
    std::fs::remove_file(&settings_path).expect("remove settings.toml");

    let dest = fix.dest("restored");
    let options = RestoreOptions {
        snapshot: 1,
        to: Some(dest.clone()),
        drill: false,
        target: None,
        from: Some(fix.target_path().to_path_buf()),
    };
    let outcome = run_restore(&fix.world, &floor_a_effective(), &options);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);

    assert_trees_equal(&dest, &fix.snap_dir().join("data"));
}

#[test]
fn restore_prints_rename_lines_for_sh_and_powershell() {
    let fix = TestFixture::new();
    fix.backup();

    let dest = fix.dest("restored");
    let outcome = fix.restore(Some(dest.clone()), false);
    assert_eq!(outcome.code, 0, "restore failed: {}", outcome.stderr);

    let tree_corpus = fix.tree_root().join("corpus").display().to_string();
    let dest_corpus = dest.join("corpus").display().to_string();

    let expected_sh_mv1 = format!("  mv '{tree_corpus}' '{tree_corpus}.old'");
    let expected_sh_mv2 = format!("  mv '{dest_corpus}' '{tree_corpus}'");
    let expected_pwsh1 =
        format!("  Move-Item -LiteralPath '{tree_corpus}' -Destination '{tree_corpus}.old'");
    let expected_pwsh2 =
        format!("  Move-Item -LiteralPath '{dest_corpus}' -Destination '{tree_corpus}'");

    assert!(
        outcome.stdout.contains("sh:\n"),
        "stdout: {}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains(&expected_sh_mv1),
        "stdout missing `{expected_sh_mv1}`:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains(&expected_sh_mv2),
        "stdout missing `{expected_sh_mv2}`:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains("PowerShell:\n"),
        "stdout: {}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains(&expected_pwsh1),
        "stdout missing `{expected_pwsh1}`:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains(&expected_pwsh2),
        "stdout missing `{expected_pwsh2}`:\n{}",
        outcome.stdout
    );
}

#[test]
fn drill_passes_on_the_fixture() {
    let fix = TestFixture::new();
    fix.backup();

    let outcome = fix.restore(None, true);
    assert_eq!(outcome.code, 0, "drill failed: {}", outcome.stderr);

    let ok_count = outcome
        .stdout
        .lines()
        .filter(|l| l.starts_with("ok:"))
        .count();
    assert_eq!(ok_count, 5, "stdout: {}", outcome.stdout);

    let last_line = outcome
        .stdout
        .lines()
        .rfind(|l| !l.is_empty())
        .unwrap_or("");
    assert!(
        last_line.starts_with("drill passed: check clean, journal ok, fingerprint ")
            && last_line.ends_with(" matches"),
        "last_line: {last_line}"
    );
    let fp = last_line
        .strip_prefix("drill passed: check clean, journal ok, fingerprint ")
        .and_then(|s| s.strip_suffix(" matches"))
        .unwrap_or("");
    assert_eq!(fp.len(), 12, "fingerprint length: {}", fp.len());
    assert!(
        fp.chars().all(|c| c.is_ascii_hexdigit()),
        "fingerprint not hex: {fp}"
    );

    let drill_dir = drill_folder(fix.state_dir());
    assert!(
        !drill_dir.exists(),
        "drill folder must not exist afterwards"
    );
}

#[test]
fn drill_fails_on_a_changed_byte() {
    let fix = TestFixture::new();
    fix.backup();
    let target_file = fix.snap_dir().join("data/corpus/corpus.toml");
    let mut bytes = std::fs::read(&target_file).expect("read");
    bytes[0] ^= 0xFF;
    std::fs::write(&target_file, bytes).expect("write");

    let outcome = fix.restore(None, true);
    assert_eq!(outcome.code, 1);
    assert!(
        outcome
            .stderr
            .lines()
            .any(|l| l.contains("FB0810") && l.contains("data/corpus/corpus.toml")),
        "stderr: {}",
        outcome.stderr
    );

    let drill_dir = drill_folder(fix.state_dir());
    assert!(!drill_dir.exists());

    let kinds_after = journal_kinds(fix.state_dir());
    assert!(!kinds_after.iter().any(|k| k == "restore.begin"));
}

#[test]
fn drill_fails_when_the_restored_journal_is_cut() {
    let fix = TestFixture::new();
    fix.backup();

    set_after_copy_hook_for_test(Some(Arc::new(|drill: &Path| {
        let audit_dir = drill.join("state/audit");
        if let Ok(entries) = std::fs::read_dir(&audit_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                    let content = std::fs::read_to_string(&p).expect("read jsonl");
                    let mut lines: Vec<&str> = content.lines().collect();
                    if lines.len() >= 2 {
                        lines.truncate(lines.len() - 2);
                    }
                    let mut new_content = lines.join("\n");
                    if !new_content.is_empty() {
                        new_content.push('\n');
                    }
                    std::fs::write(&p, new_content).expect("write jsonl");
                }
            }
        }
    })));

    let outcome = fix.restore(None, true);
    set_after_copy_hook_for_test(None);

    assert_eq!(outcome.code, 1);
    assert!(
        outcome.stderr.contains("FB0816"),
        "stderr: {}",
        outcome.stderr
    );
    assert!(
        outcome.stderr.contains("journal"),
        "stderr: {}",
        outcome.stderr
    );
    assert!(
        outcome.stderr.contains("FB0802"),
        "stderr: {}",
        outcome.stderr
    );

    let ok_lines: Vec<&str> = outcome
        .stdout
        .lines()
        .filter(|l| l.starts_with("ok:"))
        .collect();
    assert_eq!(ok_lines.len(), 3, "ok_lines: {:?}", ok_lines);
    assert!(ok_lines[0].starts_with("ok: snapshot 1 verified"));
    assert!(ok_lines[1].starts_with("ok: restored "));
    assert_eq!(ok_lines[2], "ok: check --strict clean");
    assert!(!outcome.stdout.contains("ok: journal ok"));

    let drill_dir = drill_folder(fix.state_dir());
    assert!(
        !drill_dir.exists(),
        "drill folder must not exist after failure"
    );
}

#[test]
fn drill_fails_on_a_wrong_fingerprint() {
    let fix = TestFixture::new();
    fix.backup();
    rewrite_meta(&fix.snap_dir(), |info| {
        info.fingerprint = "0".repeat(64);
    });

    let outcome = fix.restore(None, true);
    assert_eq!(outcome.code, 1);
    assert!(
        outcome.stderr.contains("FB0816"),
        "stderr: {}",
        outcome.stderr
    );
    assert!(
        outcome.stderr.contains("fingerprint"),
        "stderr: {}",
        outcome.stderr
    );

    assert!(outcome.stdout.contains("ok: check --strict clean"));
    assert!(outcome.stdout.contains("ok: journal ok"));

    let drill_dir = drill_folder(fix.state_dir());
    assert!(!drill_dir.exists());
}

#[test]
fn drill_refuses_stale_manifest_digest() {
    let fix = TestFixture::new();
    fix.backup();
    rewrite_meta_stale(&fix.snap_dir(), |info| {
        info.fingerprint = "0".repeat(64);
    });

    let outcome = fix.restore(None, true);
    assert_eq!(outcome.code, 1);
    assert!(
        outcome.stderr.contains("FB0811"),
        "stderr: {}",
        outcome.stderr
    );
    assert!(
        !outcome.stderr.contains("FB0816"),
        "stderr: {}",
        outcome.stderr
    );

    let kinds_after = journal_kinds(fix.state_dir());
    assert!(!kinds_after.iter().any(|k| k == "restore.begin"));

    let drill_dir = drill_folder(fix.state_dir());
    assert!(!drill_dir.exists());
}

#[test]
fn drill_uses_the_snapshot_date() {
    let fix = TestFixture::new();
    fix.backup();

    let check_args = vec![
        "check".to_string(),
        "--strict".to_string(),
        "--today".to_string(),
        "2099-01-01".to_string(),
    ];
    let check_outcome = fieldbook::run(&check_args, &fix.world);
    assert!(
        check_outcome.code != 0 || check_outcome.stdout.contains("FB0221"),
        "precondition: check --today 2099-01-01 must produce findings or non-zero exit: code={}, stdout={}",
        check_outcome.code,
        check_outcome.stdout
    );

    let far_date = CivilDate::from_ymd(2099, 1, 1).expect("date");
    let far_now = Now::new(far_date, 43_200).expect("now");
    fix.world.set_now(far_now);

    let outcome = fix.restore(None, true);
    assert_eq!(
        outcome.code, 0,
        "drill must use snapshot date and pass: stdout={}, stderr={}",
        outcome.stdout, outcome.stderr
    );
}

#[test]
fn drill_deletes_its_folder_on_every_path() {
    let drill_dir = |fix: &TestFixture| drill_folder(fix.state_dir());

    // 1. Successful drill with pre-created stale.txt
    {
        let fix = TestFixture::new();
        fix.backup();
        let drill_p = drill_dir(&fix);
        std::fs::create_dir_all(&drill_p).expect("create drill dir");
        std::fs::write(drill_p.join("stale.txt"), "stale").expect("write stale.txt");
        assert!(drill_p.join("stale.txt").exists());

        let out = fix.restore(None, true);
        assert_eq!(out.code, 0);
        assert!(
            !drill_p.exists(),
            "drill folder must be deleted after success"
        );
    }

    // 2. Wrong-fingerprint failure
    {
        let fix = TestFixture::new();
        fix.backup();
        rewrite_meta(&fix.snap_dir(), |info| {
            info.fingerprint = "0".repeat(64);
        });
        let out = fix.restore(None, true);
        assert_eq!(out.code, 1);
        assert!(
            !drill_dir(&fix).exists(),
            "drill folder must be deleted after fingerprint failure"
        );
    }

    // 3. Journal-cut failure
    {
        let fix = TestFixture::new();
        fix.backup();
        set_after_copy_hook_for_test(Some(Arc::new(|drill: &Path| {
            let p = drill.join("state/audit/journal.jsonl");
            if p.is_file() {
                let content = std::fs::read_to_string(&p).expect("read");
                let mut lines: Vec<&str> = content.lines().collect();
                if lines.len() >= 2 {
                    lines.truncate(lines.len() - 2);
                }
                let mut new_content = lines.join("\n");
                if !new_content.is_empty() {
                    new_content.push('\n');
                }
                std::fs::write(&p, new_content).expect("write");
            }
        })));
        let out = fix.restore(None, true);
        set_after_copy_hook_for_test(None);
        assert_eq!(out.code, 1);
        assert!(
            !drill_dir(&fix).exists(),
            "drill folder must be deleted after journal failure"
        );
    }

    // 4. Verification failure (byte flip)
    {
        let fix = TestFixture::new();
        fix.backup();
        let f = fix.snap_dir().join("data/corpus/corpus.toml");
        let mut b = std::fs::read(&f).expect("read");
        b[0] ^= 0xFF;
        std::fs::write(&f, b).expect("write");

        let out = fix.restore(None, true);
        assert_eq!(out.code, 1);
        assert!(
            !drill_dir(&fix).exists(),
            "drill folder must be absent after verification failure"
        );
    }
}

#[test]
fn restore_events_are_begin_then_end() {
    let fix = TestFixture::new();
    fix.backup();

    let toml_text =
        std::fs::read_to_string(fix.snap_dir().join("backup.toml")).expect("read backup.toml");
    let info = BackupToml::parse(&toml_text).expect("parse backup.toml");

    let backup_kinds = journal_kinds(fix.state_dir());

    let out_drill = fix.restore(None, true);
    assert_eq!(out_drill.code, 0);

    let dest = fix.dest("restored");
    let out_real = fix.restore(Some(dest), false);
    assert_eq!(out_real.code, 0);

    let all_kinds = journal_kinds(fix.state_dir());
    let new_kinds = &all_kinds[backup_kinds.len()..];
    assert_eq!(
        new_kinds,
        &[
            "restore.begin",
            "restore.end",
            "restore.begin",
            "restore.end"
        ]
    );
    assert!(!all_kinds.iter().any(|k| k == "restore.drill"));

    let mut audit_files = Vec::new();
    for entry in std::fs::read_dir(fix.state_dir().join("audit"))
        .expect("read audit dir")
        .flatten()
    {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            audit_files.push(p);
        }
    }
    audit_files.sort();

    let mut all_events = Vec::new();
    for f in audit_files {
        let content = std::fs::read_to_string(&f).expect("read segment");
        for line in content.lines() {
            let t = line.trim();
            if !t.is_empty() {
                let v: serde_json::Value = serde_json::from_str(t).expect("parse json");
                all_events.push(v);
            }
        }
    }

    let restore_events: Vec<&serde_json::Value> = all_events
        .iter()
        .filter(|v| {
            v.get("kind")
                .and_then(|k| k.as_str())
                .is_some_and(|k| k.starts_with("restore."))
        })
        .collect();

    assert_eq!(restore_events.len(), 4);

    // Event 0: drill restore.begin
    let ev0 = restore_events[0];
    assert_eq!(ev0["kind"], "restore.begin");
    assert_eq!(ev0["mode"], "drill");
    let ev0_labels: Vec<String> = ev0["labels"]
        .as_array()
        .expect("labels array")
        .iter()
        .map(|v| v.as_str().expect("str").to_string())
        .collect();
    assert_eq!(ev0_labels, info.labels);
    assert!(ev0.get("dest").is_none());
    assert!(ev0.get("policy").is_none());

    // Event 1: drill restore.end
    let ev1 = restore_events[1];
    assert_eq!(ev1["kind"], "restore.end");
    assert_eq!(ev1["mode"], "drill");
    assert_eq!(ev1["outcome"], "ok");

    // Event 2: real restore.begin
    let ev2 = restore_events[2];
    assert_eq!(ev2["kind"], "restore.begin");
    assert_eq!(ev2["mode"], "real");
    let ev2_labels: Vec<String> = ev2["labels"]
        .as_array()
        .expect("labels array")
        .iter()
        .map(|v| v.as_str().expect("str").to_string())
        .collect();
    assert_eq!(ev2_labels, info.labels);
    assert_eq!(ev2["dest"], "export");
    assert!(ev2["policy"].is_string());
    assert!(!ev2["policy"].as_str().unwrap().is_empty());

    // Event 3: real restore.end
    let ev3 = restore_events[3];
    assert_eq!(ev3["kind"], "restore.end");
    assert_eq!(ev3["mode"], "real");
    assert_eq!(ev3["outcome"], "ok");
}

#[test]
fn a_second_drill_replaces_the_first_folder_whole() {
    let fix = TestFixture::new();
    fix.backup();

    let drill_dir = drill_folder(fix.state_dir());
    std::fs::create_dir_all(&drill_dir).expect("create drill dir");
    let stale_path = drill_dir.join("stale.txt");
    std::fs::write(&stale_path, "leftover from earlier").expect("write stale");

    let seen = Arc::new(AtomicBool::new(false));
    let seen_clone = Arc::clone(&seen);
    let stale_check = stale_path.clone();

    set_after_copy_hook_for_test(Some(Arc::new(move |drill_path: &Path| {
        if drill_path.exists() && !stale_check.exists() {
            seen_clone.store(true, Ordering::SeqCst);
        }
    })));

    let outcome = fix.restore(None, true);
    set_after_copy_hook_for_test(None);

    assert_eq!(outcome.code, 0, "drill failed: {}", outcome.stderr);
    assert!(
        seen.load(Ordering::SeqCst),
        "after-copy hook should observe that drill folder exists and stale.txt is gone"
    );
}

#[test]
fn the_drill_writes_only_under_the_state_folder() {
    let fix = TestFixture::new();
    fix.backup();

    let before_len = fix.world.written().len();
    let outcome = fix.restore(None, true);
    assert_eq!(outcome.code, 0, "drill failed: {}", outcome.stderr);

    let written = fix.world.written();
    let restore_writes = &written[before_len..];
    assert!(
        !restore_writes.is_empty(),
        "expected restore writes during drill to be recorded"
    );

    let drill_prefix = drill_folder(fix.state_dir());
    for p in restore_writes {
        assert!(
            p.starts_with(&drill_prefix),
            "expected write {} to start with drill prefix {}",
            p.display(),
            drill_prefix.display()
        );
    }

    let audit_file = fix.state_dir().join("audit/journal.jsonl");
    let content = std::fs::read_to_string(&audit_file).expect("read journal");
    let last_begin = content
        .lines()
        .rfind(|l| l.contains("\"kind\":\"restore.begin\""))
        .expect("restore.begin exists");
    let val: serde_json::Value = serde_json::from_str(last_begin).expect("parse json");
    assert!(
        val.get("dest").is_none(),
        "drill restore.begin must have no dest: {last_begin}"
    );
}

#[test]
fn restore_starts_no_process_and_opens_no_socket() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("src/restore.rs");
    let text = std::fs::read_to_string(&path).expect("read restore.rs");
    let forbidden = [
        "std::process",
        "Command::new",
        "std::net",
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        "spawn(",
        "arboard",
        "clipboard",
    ];
    for token in &forbidden {
        assert!(
            !text.contains(token),
            "crates/fieldbook/src/restore.rs contains forbidden token `{token}`"
        );
    }
}

// ---------------------------------------------------------------------------
// Second half helpers and integration tests
// ---------------------------------------------------------------------------

use fieldbook::cli::{BackupArgs, BackupVerifyArgs, RestoreArgs};
use fieldbook::io::Disk;
use fieldbook::io::journal::{Journal, RestoreExportRequest};
use fieldbook_core::audit::{LabelSet, Via};
use fieldbook_core::backup::{PlannedFile, RestorePlan};
use fieldbook_core::egress::{
    Decision, Delivery, Dest, Operation, Shipped, Target, Transfer, check, hash_tree_manifest,
};
use fieldbook_core::manifest::SupportCatalogue;
use fieldbook_core::policy::Policy;

fn make_raw_copy(tree_root: &Path, state_dir: &Path) -> tempfile::TempDir {
    let raw = tempfile::tempdir().expect("raw tempdir");
    let corpus_dest = raw.path().join("corpus");
    let state_dest = raw.path().join("state");
    support::copy_dir_recursive(&tree_root.join("corpus"), &corpus_dest).expect("copy corpus");
    support::copy_dir_recursive(state_dir, &state_dest).expect("copy state");
    raw
}

fn all_journal_events(state_dir: &Path) -> Vec<serde_json::Value> {
    let audit_dir = state_dir.join("audit");
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&audit_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                files.push(p);
            }
        }
    }
    files.sort();
    let mut events = Vec::new();
    for f in files {
        let content = std::fs::read_to_string(&f).expect("read journal segment");
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let val: serde_json::Value = serde_json::from_str(trimmed).expect("parse journal line");
            events.push(val);
        }
    }
    events
}

struct GitFixture {
    _tmp: tempfile::TempDir,
    git_repo: PathBuf,
    state: PathBuf,
    target_dir: PathBuf,
    world: TestWorld,
}

impl GitFixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let git_repo = tmp.path().join("git-repo");
        std::fs::create_dir_all(git_repo.join(".git")).expect("create .git");
        std::fs::write(git_repo.join(".gitignore"), ".fieldbook\n").expect("write gitignore");

        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let corpus_mini = repo.join("tests/fixtures/corpus-mini");
        let state_mini = repo.join("tests/fixtures/state-mini");

        support::copy_dir_recursive(&corpus_mini, &git_repo).expect("copy corpus-mini");

        let state = git_repo.join(".fieldbook");
        support::copy_dir_recursive(&state_mini, &state).expect("copy state");

        let outside_target = tmp.path().join("outside-target");
        std::fs::create_dir_all(&outside_target).expect("create outside target");

        let settings_path = state.join("settings.toml");
        let settings_text = format!(
            "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
            outside_target.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(&settings_path, settings_text).expect("write settings");

        let world = TestWorld::with_state(&git_repo, &state);
        let tree = fieldbook::io::read_tree(world.tree_root()).expect("tree");
        if let Ok(cat) =
            SupportCatalogue::from_public_source(&tree, "1111111111111111111111111111111111111111")
        {
            let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
            world.set_support_catalogue(cat, cat_sha);
        }

        let eff = floor_a_effective();
        let outcome = run_backup(
            &world,
            &eff,
            &BackupOptions {
                target: Some("main".to_string()),
                to: None,
                dry_run: false,
            },
        );
        assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

        Self {
            _tmp: tmp,
            git_repo,
            state,
            target_dir: outside_target,
            world,
        }
    }

    fn snap_dir(&self) -> PathBuf {
        self.target_dir.join("fieldbook-backup-0001")
    }
}

// A. restore_with_an_unlabelled_snapshot_is_fb0901_and_exits_2
#[test]
fn restore_with_an_unlabelled_snapshot_is_fb0901_and_exits_2() {
    let fix = TestFixture::new();
    fix.backup();

    // Paired positive control: permitted case succeeds
    let dest_ok = fix.dest("restored_ok");
    let out_ok = fix.restore(Some(dest_ok.clone()), false);
    assert_eq!(out_ok.code, 0, "positive control failed: {}", out_ok.stderr);
    assert!(dest_ok.is_dir());

    // labels = ["unlabelled"]
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["unlabelled".to_string()];
    });
    let dest_unlabelled = fix.dest("restored");
    let events_before_u = all_journal_events(fix.state_dir());
    let out_u = fix.restore(Some(dest_unlabelled.clone()), false);
    assert_eq!(
        out_u.code, 2,
        "must exit 2 on unlabelled export refusal: {}",
        out_u.stderr
    );
    assert!(out_u.stderr.contains("FB0901"), "stderr: {}", out_u.stderr);
    assert!(
        !dest_unlabelled.exists(),
        "absent destination must stay absent"
    );

    let events_after_u = all_journal_events(fix.state_dir());
    let new_events_u = &events_after_u[events_before_u.len()..];
    assert!(
        !new_events_u.iter().any(|v| v["kind"] == "restore.begin"),
        "no restore.begin appended"
    );
    let refuses_u: Vec<_> = new_events_u
        .iter()
        .filter(|v| v["kind"] == "refuse")
        .collect();
    assert_eq!(refuses_u.len(), 1, "exactly one refuse event expected");
    assert_eq!(refuses_u[0]["reason"], "unlabelled");
    assert_eq!(refuses_u[0]["code"], "FB0901");

    // labels = ["marked"]
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["marked".to_string()];
    });
    let dest_marked = fix.dest("restored_marked");
    let events_before_m = all_journal_events(fix.state_dir());
    let out_m = fix.restore(Some(dest_marked.clone()), false);
    assert_eq!(
        out_m.code, 2,
        "must exit 2 on marked export refusal: {}",
        out_m.stderr
    );
    assert!(out_m.stderr.contains("FB0901"), "stderr: {}", out_m.stderr);
    assert!(!dest_marked.exists(), "absent destination must stay absent");

    let events_after_m = all_journal_events(fix.state_dir());
    let new_events_m = &events_after_m[events_before_m.len()..];
    assert!(
        !new_events_m.iter().any(|v| v["kind"] == "restore.begin"),
        "no restore.begin appended"
    );
    let refuses_m: Vec<_> = new_events_m
        .iter()
        .filter(|v| v["kind"] == "refuse")
        .collect();
    assert_eq!(refuses_m.len(), 1, "exactly one refuse event expected");
    assert_eq!(refuses_m[0]["reason"], "marked");
    assert_eq!(refuses_m[0]["code"], "FB0901");
}

// B. restore_outside_the_data_root_asks_for_an_export_token
#[test]
fn restore_outside_the_data_root_asks_for_an_export_token() {
    let fix = TestFixture::new();
    fix.backup();

    let snap_toml =
        std::fs::read_to_string(fix.snap_dir().join("backup.toml")).expect("read backup.toml");
    let snap_info = BackupToml::parse(&snap_toml).expect("parse backup.toml");

    // Drill restore.begin has neither dest nor policy
    let out_drill = fix.restore(None, true);
    assert_eq!(out_drill.code, 0, "drill failed: {}", out_drill.stderr);
    let events_drill = all_journal_events(fix.state_dir());
    let drill_begin = events_drill
        .iter()
        .find(|e| e["kind"] == "restore.begin" && e["mode"] == "drill")
        .expect("drill restore.begin");
    assert!(
        drill_begin.get("dest").is_none(),
        "drill restore.begin must have no dest"
    );
    assert!(
        drill_begin.get("policy").is_none(),
        "drill restore.begin must have no policy"
    );

    // Restore outside the data root asks for export token with dest: export, non-empty policy, snapshot labels
    let dest = fix.dest("restored");
    let out_to = fix.restore(Some(dest.clone()), false);
    assert_eq!(out_to.code, 0, "restore --to failed: {}", out_to.stderr);
    assert!(dest.is_dir());

    let events_to = all_journal_events(fix.state_dir());
    let export_begin = events_to
        .iter()
        .find(|e| e["kind"] == "restore.begin" && e["mode"] == "real")
        .expect("real restore.begin");
    assert_eq!(export_begin["dest"], "export");
    assert!(
        export_begin["policy"]
            .as_str()
            .is_some_and(|p| !p.is_empty()),
        "policy field must be non-empty"
    );
    let begin_labels: Vec<String> = export_begin["labels"]
        .as_array()
        .expect("labels array")
        .iter()
        .map(|v| v.as_str().expect("str").to_string())
        .collect();
    assert_eq!(begin_labels, snap_info.labels);
}

// C. Journal-level export binding tests
#[test]
fn restore_export_binding_is_exact() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let tmp_canonical = tmp.path().canonicalize().expect("canonicalize");
    let corpus_dir = tmp_canonical.join("corpus");
    let state_dir = tmp_canonical.join(".fieldbook");
    std::fs::create_dir_all(&corpus_dir).expect("create corpus dir");
    std::fs::create_dir_all(&state_dir).expect("create state dir");

    let disk = Disk::with_state(&corpus_dir, &state_dir);
    let mut journal = Journal::open(&disk, &state_dir, Via::Cli).expect("open journal");

    let export_dest = tmp_canonical.join("restored");
    let file1_path = export_dest.join("corpus/corpus.toml");
    let file2_path = export_dest.join("state/settings.toml");
    let file1_sha = "1111111111111111111111111111111111111111111111111111111111111111".to_string();
    let file2_sha = "2222222222222222222222222222222222222222222222222222222222222222".to_string();

    let planned_files = vec![
        PlannedFile {
            path: file1_path.clone(),
            sha256: file1_sha.clone(),
            bytes: 100,
        },
        PlannedFile {
            path: file2_path.clone(),
            sha256: file2_sha.clone(),
            bytes: 200,
        },
    ];
    let plan = RestorePlan {
        destination: export_dest.clone(),
        roots: Vec::new(),
    };
    let labels = LabelSet::public();
    let request = RestoreExportRequest {
        snapshot: 1,
        mode: "real",
        labels: &labels,
        plan: &plan,
        files: &planned_files,
        create_destination: true,
    };

    let (dir_sha, dir_bytes) = hash_tree_manifest(&[]);
    let transfer_dir = Transfer {
        target: Target::Path(export_dest.clone()),
        operation: Operation::CreateDir,
        sha256: dir_sha.clone(),
        bytes: dir_bytes,
    };
    let transfer_file1 = Transfer {
        target: Target::Path(file1_path),
        operation: Operation::Write,
        sha256: file1_sha,
        bytes: 100,
    };
    let transfer_file2 = Transfer {
        target: Target::Path(file2_path),
        operation: Operation::Write,
        sha256: file2_sha,
        bytes: 200,
    };
    let matching_transfers = vec![transfer_dir, transfer_file1, transfer_file2];

    let delivery = Delivery::new(matching_transfers.clone(), None);
    let dest = Dest::Export {
        path: export_dest.to_string_lossy().into_owned(),
    };
    let eff = floor_a_effective();
    let Decision::Allowed(exact_token) = check(
        &eff,
        &dest,
        Some(&labels),
        Some(&delivery),
        disk.now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed token for matching delivery");
    };

    // Positive control: exact token succeeds and appends restore.begin
    let audited = journal
        .begin_restore_export(&request, &exact_token)
        .expect("begin_restore_export with exact token should succeed");
    assert!(journal_kinds(&state_dir).contains(&"restore.begin".to_string()));
    journal
        .end_restore(audited, 1, "real", "ok", None)
        .expect("end_restore");

    // 1. Selection-only token (no delivery)
    let Decision::Allowed(selection_token) =
        check(&eff, &dest, Some(&labels), None, disk.now(), Shipped::Never)
    else {
        panic!("selection token");
    };
    let kinds_before_1 = journal_kinds(&state_dir);
    let res1 = journal.begin_restore_export(&request, &selection_token);
    assert!(res1.is_err(), "selection-only token must be refused");
    assert_eq!(journal_kinds(&state_dir), kinds_before_1);
    assert!(!export_dest.exists());

    // 2. Token for a different destination path
    let diff_dest_path = tmp_canonical.join("different_dest");
    let diff_dest = Dest::Export {
        path: diff_dest_path.to_string_lossy().into_owned(),
    };
    let diff_transfers = vec![
        Transfer {
            target: Target::Path(diff_dest_path.clone()),
            operation: Operation::CreateDir,
            sha256: dir_sha.clone(),
            bytes: dir_bytes,
        },
        Transfer {
            target: Target::Path(diff_dest_path.join("corpus/corpus.toml")),
            operation: Operation::Write,
            sha256: "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            bytes: 100,
        },
        Transfer {
            target: Target::Path(diff_dest_path.join("state/settings.toml")),
            operation: Operation::Write,
            sha256: "2222222222222222222222222222222222222222222222222222222222222222".to_string(),
            bytes: 200,
        },
    ];
    let Decision::Allowed(diff_path_token) = check(
        &eff,
        &diff_dest,
        Some(&labels),
        Some(&Delivery::new(diff_transfers, None)),
        disk.now(),
        Shipped::Never,
    ) else {
        panic!("diff path token");
    };
    let kinds_before_2 = journal_kinds(&state_dir);
    let res2 = journal.begin_restore_export(&request, &diff_path_token);
    assert!(
        res2.is_err(),
        "different destination path token must be refused"
    );
    assert_eq!(journal_kinds(&state_dir), kinds_before_2);
    assert!(!export_dest.exists());

    // 3. Token with a changed payload hash
    let mut bad_hash_transfers = matching_transfers.clone();
    bad_hash_transfers[1].sha256 =
        "3333333333333333333333333333333333333333333333333333333333333333".to_string();
    let Decision::Allowed(bad_hash_token) = check(
        &eff,
        &dest,
        Some(&labels),
        Some(&Delivery::new(bad_hash_transfers, None)),
        disk.now(),
        Shipped::Never,
    ) else {
        panic!("bad hash token");
    };
    let kinds_before_3 = journal_kinds(&state_dir);
    let res3 = journal.begin_restore_export(&request, &bad_hash_token);
    assert!(res3.is_err(), "changed payload hash token must be refused");
    assert_eq!(journal_kinds(&state_dir), kinds_before_3);
    assert!(!export_dest.exists());

    // 4. Token with one extra transfer
    let mut extra_transfers = matching_transfers.clone();
    extra_transfers.push(Transfer {
        target: Target::Path(export_dest.join("extra.txt")),
        operation: Operation::Write,
        sha256: "4444444444444444444444444444444444444444444444444444444444444444".to_string(),
        bytes: 50,
    });
    let Decision::Allowed(extra_token) = check(
        &eff,
        &dest,
        Some(&labels),
        Some(&Delivery::new(extra_transfers, None)),
        disk.now(),
        Shipped::Never,
    ) else {
        panic!("extra token");
    };
    let kinds_before_4 = journal_kinds(&state_dir);
    let res4 = journal.begin_restore_export(&request, &extra_token);
    assert!(res4.is_err(), "extra transfer token must be refused");
    assert_eq!(journal_kinds(&state_dir), kinds_before_4);
    assert!(!export_dest.exists());

    // 5. Token missing one file
    let mut missing_transfers = matching_transfers;
    missing_transfers.pop();
    let Decision::Allowed(missing_token) = check(
        &eff,
        &dest,
        Some(&labels),
        Some(&Delivery::new(missing_transfers, None)),
        disk.now(),
        Shipped::Never,
    ) else {
        panic!("missing token");
    };
    let kinds_before_5 = journal_kinds(&state_dir);
    let res5 = journal.begin_restore_export(&request, &missing_token);
    assert!(res5.is_err(), "missing file token must be refused");
    assert_eq!(journal_kinds(&state_dir), kinds_before_5);
    assert!(!export_dest.exists());
}

// D1. drill_refuses_company_labels_under_git_before_copy
#[test]
fn drill_refuses_company_labels_under_git_before_copy() {
    let fix = GitFixture::new();
    let eff = floor_a_effective();

    // Refusal: labels = ["np-0007"] under git
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["np-0007".to_string()];
    });
    let drill_dir = fix.state.join("drill");
    std::fs::create_dir_all(&drill_dir).expect("create drill dir");
    let canary_path = drill_dir.join("canary.txt");
    let canary_bytes = b"canary drill bytes before copy\n";
    std::fs::write(&canary_path, canary_bytes).expect("write canary");

    let events_before_1 = all_journal_events(&fix.state);
    let out_np = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: None,
            drill: true,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        out_np.code, 1,
        "drill must fail on company labels under git"
    );
    assert!(
        out_np.stderr.contains("FB0415"),
        "stderr: {}",
        out_np.stderr
    );

    let events_after_1 = all_journal_events(&fix.state);
    let new_events_1 = &events_after_1[events_before_1.len()..];
    assert!(
        !new_events_1.iter().any(|v| v["kind"] == "restore.begin"),
        "no restore.begin appended"
    );
    let refuses_1: Vec<_> = new_events_1
        .iter()
        .filter(|v| v["kind"] == "refuse")
        .collect();
    assert_eq!(refuses_1.len(), 1, "exactly one refuse event expected");
    assert_eq!(refuses_1[0]["reason"], "git");
    assert_eq!(refuses_1[0]["code"], "FB0415");

    assert_eq!(
        std::fs::read(&canary_path).expect("read canary"),
        canary_bytes,
        "canary must be byte-identical"
    );
    let entries_1: Vec<_> = std::fs::read_dir(&drill_dir)
        .expect("read drill dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(entries_1, vec![std::ffi::OsString::from("canary.txt")]);

    // Paired permitted: labels = ["public"] drill passes under git
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["public".to_string()];
    });
    let out_public = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: None,
            drill: true,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        out_public.code, 0,
        "public drill must pass under git: {}",
        out_public.stderr
    );
    assert!(!drill_dir.exists(), "drill folder deleted on success");

    // Paired permitted: labels = ["owner-supplied"] drill passes when State is the IGNORED home State
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["owner-supplied".to_string()];
    });
    let out_owner = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: None,
            drill: true,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        out_owner.code, 0,
        "owner-supplied drill in ignored home state must pass: {}",
        out_owner.stderr
    );
    assert!(!drill_dir.exists());

    // Refuses FB0415 when .gitignore does not ignore it (remove the file)
    std::fs::remove_file(fix.git_repo.join(".gitignore")).expect("remove .gitignore");
    std::fs::create_dir_all(&drill_dir).expect("create drill dir");
    std::fs::write(&canary_path, canary_bytes).expect("write canary");

    let events_before_unignored = all_journal_events(&fix.state);
    let out_unignored = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: None,
            drill: true,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        out_unignored.code, 1,
        "unignored state must refuse owner-supplied labels with exit 1"
    );
    assert!(
        out_unignored.stderr.contains("FB0415"),
        "stderr: {}",
        out_unignored.stderr
    );

    let events_after_unignored = all_journal_events(&fix.state);
    let new_events_un = &events_after_unignored[events_before_unignored.len()..];
    assert!(
        !new_events_un.iter().any(|v| v["kind"] == "restore.begin"),
        "no restore.begin appended"
    );
    let refuses_un: Vec<_> = new_events_un
        .iter()
        .filter(|v| v["kind"] == "refuse")
        .collect();
    assert_eq!(refuses_un.len(), 1, "exactly one refuse event expected");
    assert_eq!(refuses_un[0]["reason"], "git");
    assert_eq!(refuses_un[0]["code"], "FB0415");

    assert_eq!(
        std::fs::read(&canary_path).expect("read canary"),
        canary_bytes,
        "canary must be byte-identical"
    );
    let entries_un: Vec<_> = std::fs::read_dir(&drill_dir)
        .expect("read drill dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(entries_un, vec![std::ffi::OsString::from("canary.txt")]);
}

// D2. restore_to_refuses_company_labels_under_git_before_copy
#[test]
fn restore_to_refuses_company_labels_under_git_before_copy() {
    let fix = GitFixture::new();

    let tmp_prefix = fix
        ._tmp
        .path()
        .canonicalize()
        .expect("canon tmp")
        .to_string_lossy()
        .replace('\\', "/");
    let plain_outside = tempfile::tempdir().expect("plain_outside");
    let outside_prefix = plain_outside
        .path()
        .canonicalize()
        .expect("canon outside")
        .to_string_lossy()
        .replace('\\', "/");

    let policy_toml = format!(
        r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Supplier terms"
restricted = true

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'

[[destination]]
kind = "export"
allow = ["public", "owner-supplied", "np-0007"]
paths = ["/", "{tmp_prefix}", "{outside_prefix}"]
"#
    );
    let policy = Policy::parse(&policy_toml, None).expect("parse policy");
    let eff = EffectivePolicy::new(&PolicyState::Loaded(policy), &OwnerSettings::unset());

    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["np-0007".to_string()];
    });

    // (1) Absent folder under <tree>
    let dest_absent_1 = fix.git_repo.join("dest_absent_1");
    assert!(!dest_absent_1.exists());
    let ev_before_1 = all_journal_events(&fix.state);
    let out_1 = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_absent_1.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(out_1.code, 1);
    assert!(out_1.stderr.contains("FB0415"));
    assert!(!dest_absent_1.exists(), "absent folder must stay absent");
    let ev_after_1 = all_journal_events(&fix.state);
    let new_1 = &ev_after_1[ev_before_1.len()..];
    assert!(!new_1.iter().any(|v| v["kind"] == "restore.begin"));
    let refuses_1: Vec<_> = new_1.iter().filter(|v| v["kind"] == "refuse").collect();
    assert_eq!(refuses_1.len(), 1);
    assert_eq!(refuses_1[0]["reason"], "git");
    assert_eq!(refuses_1[0]["code"], "FB0415");

    // (2) Existing empty folder under <tree>
    let dest_empty_1 = fix.git_repo.join("dest_empty_1");
    std::fs::create_dir(&dest_empty_1).expect("create empty dir");
    let ev_before_2 = all_journal_events(&fix.state);
    let out_2 = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_empty_1.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(out_2.code, 1);
    assert!(out_2.stderr.contains("FB0415"));
    assert!(dest_empty_1.exists());
    assert!(
        std::fs::read_dir(&dest_empty_1)
            .expect("read empty")
            .next()
            .is_none(),
        "empty folder must stay empty"
    );
    let ev_after_2 = all_journal_events(&fix.state);
    let new_2 = &ev_after_2[ev_before_2.len()..];
    assert!(!new_2.iter().any(|v| v["kind"] == "restore.begin"));
    let refuses_2: Vec<_> = new_2.iter().filter(|v| v["kind"] == "refuse").collect();
    assert_eq!(refuses_2.len(), 1);
    assert_eq!(refuses_2[0]["reason"], "git");
    assert_eq!(refuses_2[0]["code"], "FB0415");

    // (3) Replace .git directory with a FILE (gitdir: x)
    std::fs::remove_dir_all(fix.git_repo.join(".git")).expect("remove .git dir");
    std::fs::write(fix.git_repo.join(".git"), "gitdir: /some/worktree\n").expect("write .git file");

    // (3a) Absent folder under <tree> when .git is a file
    let dest_absent_2 = fix.git_repo.join("dest_absent_2");
    assert!(!dest_absent_2.exists());
    let ev_before_3 = all_journal_events(&fix.state);
    let out_3 = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_absent_2.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(out_3.code, 1);
    assert!(out_3.stderr.contains("FB0415"));
    assert!(!dest_absent_2.exists(), "absent folder must stay absent");
    let ev_after_3 = all_journal_events(&fix.state);
    let new_3 = &ev_after_3[ev_before_3.len()..];
    assert!(!new_3.iter().any(|v| v["kind"] == "restore.begin"));
    let refuses_3: Vec<_> = new_3.iter().filter(|v| v["kind"] == "refuse").collect();
    assert_eq!(refuses_3.len(), 1);
    assert_eq!(refuses_3[0]["reason"], "git");
    assert_eq!(refuses_3[0]["code"], "FB0415");

    // (3b) Existing empty folder under <tree> when .git is a file
    let dest_empty_2 = fix.git_repo.join("dest_empty_2");
    std::fs::create_dir(&dest_empty_2).expect("create empty dir");
    let ev_before_4 = all_journal_events(&fix.state);
    let out_4 = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_empty_2.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(out_4.code, 1);
    assert!(out_4.stderr.contains("FB0415"));
    assert!(dest_empty_2.exists());
    assert!(
        std::fs::read_dir(&dest_empty_2)
            .expect("read empty")
            .next()
            .is_none(),
        "empty folder must stay empty"
    );
    let ev_after_4 = all_journal_events(&fix.state);
    let new_4 = &ev_after_4[ev_before_4.len()..];
    assert!(!new_4.iter().any(|v| v["kind"] == "restore.begin"));
    let refuses_4: Vec<_> = new_4.iter().filter(|v| v["kind"] == "refuse").collect();
    assert_eq!(refuses_4.len(), 1);
    assert_eq!(refuses_4[0]["reason"], "git");
    assert_eq!(refuses_4[0]["code"], "FB0415");

    // Paired permitted: public labels to a folder under git succeed
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["public".to_string()];
    });
    let dest_public = fix.git_repo.join("dest_public");
    let out_pub = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_public.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        out_pub.code, 0,
        "public restore to folder under git must succeed: {}",
        out_pub.stderr
    );
    assert!(dest_public.is_dir());

    // Paired permitted: np-0007 labels to a folder outside any git ancestry succeed under same policy
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["np-0007".to_string()];
    });
    let dest_outside = plain_outside.path().join("restored_np");
    let out_out = run_restore(
        &fix.world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_outside.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        out_out.code, 0,
        "np-0007 restore outside git must succeed: {}",
        out_out.stderr
    );
    assert!(dest_outside.is_dir());
}

// E1. drill_refuses_labels_not_held_by_state
#[test]
fn drill_refuses_labels_not_held_by_state() {
    let fix = TestFixture::new();
    fix.backup();

    // Floor policy has no store row for np-0007
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["np-0007".to_string()];
    });
    let drill_dir = drill_folder(fix.state_dir());
    std::fs::create_dir_all(&drill_dir).expect("create drill dir");
    let canary_path = drill_dir.join("canary.txt");
    let canary_bytes = b"canary state admission bytes\n";
    std::fs::write(&canary_path, canary_bytes).expect("write canary");

    let events_before = all_journal_events(fix.state_dir());
    let outcome = fix.restore(None, true);
    assert_eq!(
        outcome.code, 1,
        "drill must refuse labels not held by state"
    );
    assert!(
        outcome.stderr.contains("FB0412"),
        "stderr: {}",
        outcome.stderr
    );

    let events_after = all_journal_events(fix.state_dir());
    let new_events = &events_after[events_before.len()..];
    assert!(
        !new_events.iter().any(|v| v["kind"] == "restore.begin"),
        "no restore.begin appended"
    );
    let refuses: Vec<_> = new_events
        .iter()
        .filter(|v| v["kind"] == "refuse")
        .collect();
    assert_eq!(refuses.len(), 1, "exactly one refuse event expected");
    assert_eq!(refuses[0]["reason"], "store");
    assert_eq!(refuses[0]["code"], "FB0412");

    assert_eq!(
        std::fs::read(&canary_path).expect("read canary"),
        canary_bytes,
        "canary must be byte-identical"
    );
    let entries: Vec<_> = std::fs::read_dir(&drill_dir)
        .expect("read drill dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from("canary.txt")]);

    // Paired permitted case: drill passes with permitted public labels
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["public".to_string()];
    });
    let out_ok = fix.restore(None, true);
    assert_eq!(
        out_ok.code, 0,
        "drill with public labels must succeed: {}",
        out_ok.stderr
    );
    assert!(!drill_dir.exists(), "drill folder deleted on success");
}

// E2. drill_admits_permitted_state_labels
#[test]
fn drill_admits_permitted_state_labels() {
    let fix = TestFixture::new();
    fix.backup();

    // 1. labels = ["owner-supplied", "public"] under floor policy (sorted)
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["owner-supplied".to_string(), "public".to_string()];
    });
    let outcome1 = fix.restore(None, true);
    assert_eq!(
        outcome1.code, 0,
        "drill with public, owner-supplied must pass: {}",
        outcome1.stderr
    );

    // 2. labels = ["np-0007"] under Loaded policy whose state store holds np-0007
    let policy_toml = r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Supplier terms"
restricted = true

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'
"#;
    let policy = Policy::parse(policy_toml, None).expect("parse policy");
    let eff_loaded = EffectivePolicy::new(&PolicyState::Loaded(policy), &OwnerSettings::unset());

    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["np-0007".to_string()];
    });
    let outcome2 = run_restore(
        &fix.world,
        &eff_loaded,
        &RestoreOptions {
            snapshot: 1,
            to: None,
            drill: true,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        outcome2.code, 0,
        "drill with np-0007 must pass under admitting store: {}",
        outcome2.stderr
    );
}

// E3. restore_to_state_checks_admission
#[test]
fn restore_to_state_checks_admission() {
    let fix = TestFixture::new();
    fix.backup();

    let dest = fix.state_dir().join("drill/restored");

    // Refusal: labels = ["np-0007"] under floor policy
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["np-0007".to_string()];
    });
    let events_before_1 = all_journal_events(fix.state_dir());
    let out_refused = fix.restore(Some(dest.clone()), false);
    assert_eq!(
        out_refused.code, 1,
        "restore to state must refuse unadmitted label"
    );
    assert!(
        out_refused.stderr.contains("FB0412"),
        "stderr: {}",
        out_refused.stderr
    );
    assert!(!dest.exists(), "nothing must be created at destination");

    let events_after_1 = all_journal_events(fix.state_dir());
    let new_events_1 = &events_after_1[events_before_1.len()..];
    assert!(
        !new_events_1.iter().any(|v| v["kind"] == "restore.begin"),
        "no restore.begin appended"
    );
    let refuses_1: Vec<_> = new_events_1
        .iter()
        .filter(|v| v["kind"] == "refuse")
        .collect();
    assert_eq!(refuses_1.len(), 1, "exactly one refuse event expected");
    assert_eq!(refuses_1[0]["reason"], "store");
    assert_eq!(refuses_1[0]["code"], "FB0412");

    // Permitted: permitted labels succeed, folder exists, restore.begin has no dest
    rewrite_meta(&fix.snap_dir(), |info| {
        info.labels = vec!["owner-supplied".to_string(), "public".to_string()];
    });
    let events_before_2 = all_journal_events(fix.state_dir());
    let out_ok = fix.restore(Some(dest.clone()), false);
    assert_eq!(
        out_ok.code, 0,
        "restore to state must succeed: {}",
        out_ok.stderr
    );
    assert!(dest.is_dir(), "restored folder must exist");

    let events_after_2 = all_journal_events(fix.state_dir());
    let new_events_2 = &events_after_2[events_before_2.len()..];
    let begin_ev = new_events_2
        .iter()
        .find(|v| v["kind"] == "restore.begin")
        .expect("restore.begin present");
    assert!(
        begin_ev.get("dest").is_none(),
        "in-root state restore.begin must have no dest: {begin_ev:?}"
    );
}

// F. restored_support_keeps_exact_proof
#[test]
fn restored_support_keeps_exact_proof() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = support::temp_copy(&state_mini);
    let target_dir = tempfile::tempdir().expect("target tempdir");

    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_dir.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state_dir.path().join("settings.toml"), &settings_text).expect("write settings");

    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read tree");
    let cat =
        SupportCatalogue::from_public_source(&tree, "1111111111111111111111111111111111111111")
            .expect("from_public_source");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();
    let cat_file = tree_dir.path().join("corpus-support.toml");
    std::fs::write(&cat_file, &cat_bytes).expect("write cat file");

    let raw = make_raw_copy(tree_dir.path(), state_dir.path());
    let disk = Disk::with_state(tree_dir.path(), state_dir.path());
    let v_args = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: Some(cat_file),
        expect_support_sha256: Some(cat_sha.clone()),
    };
    let v_out = fieldbook::backup::run_verify(&disk, &v_args);
    assert_eq!(v_out.code, 0, "run_verify failed: {}", v_out.stderr);

    let b_args = BackupArgs {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
        command: None,
    };
    let b_out = fieldbook::backup::run(&b_args, None, &disk);
    assert_eq!(b_out.code, 0, "backup failed: {}", b_out.stderr);

    let restored_dir = tempfile::tempdir().expect("restored dir");
    let restored_path = restored_dir.path().join("restored");
    let r_args = RestoreArgs {
        snapshot: 1,
        to: Some(restored_path.clone()),
        drill: false,
        target: None,
        from: None,
    };
    let r_out = fieldbook::restore::run(&r_args, None, &disk);
    assert_eq!(r_out.code, 0, "restore failed: {}", r_out.stderr);

    let restored_corpus = restored_path.join("corpus");
    let restored_state = restored_path.join("state");

    // (1) Disk::with_state(<restored>/corpus, <restored>/state).support_facts() carries the same catalogue sha256
    let orig_facts = disk.support_facts();
    let orig_shas: Vec<String> = orig_facts
        .catalogues
        .iter()
        .map(|(_, s)| s.clone())
        .collect();
    assert!(
        orig_shas.contains(&cat_sha),
        "original state must hold verified cat_sha"
    );

    let restored_disk = Disk::with_state(&restored_corpus, &restored_state);
    let restored_facts = restored_disk.support_facts();
    let restored_shas: Vec<String> = restored_facts
        .catalogues
        .iter()
        .map(|(_, s)| s.clone())
        .collect();
    assert_eq!(
        orig_shas, restored_shas,
        "relocated state must retain exact support proof"
    );

    // (2) Forged state: fresh empty state plus ONLY copy of public-support/<sha>.toml yields EMPTY catalogues
    let forged_dir = tempfile::tempdir().expect("forged tempdir");
    let forged_state = forged_dir.path().join(".fieldbook");
    let forged_pub_support = forged_state.join("public-support");
    std::fs::create_dir_all(&forged_pub_support).expect("create forged public-support dir");
    std::fs::copy(
        restored_state
            .join("public-support")
            .join(format!("{cat_sha}.toml")),
        forged_pub_support.join(format!("{cat_sha}.toml")),
    )
    .expect("copy catalogue to forged state");

    let forged_disk = Disk::with_state(&restored_corpus, &forged_state);
    let forged_facts = forged_disk.support_facts();
    assert!(
        forged_facts.catalogues.is_empty(),
        "forged state without journal evidence must yield empty catalogues"
    );

    // (3) Restored copy whose public-support/<sha>.toml has one byte changed yields no catalogue for that sha
    let tampered_copy = support::temp_copy(&restored_path);
    let tampered_cat_file = tampered_copy
        .path()
        .join("state/public-support")
        .join(format!("{cat_sha}.toml"));
    let mut tampered_bytes = std::fs::read(&tampered_cat_file).expect("read cat file");
    tampered_bytes[0] ^= 0xFF;
    std::fs::write(&tampered_cat_file, tampered_bytes).expect("write tampered cat file");

    let tampered_corpus = tampered_copy.path().join("corpus");
    let tampered_state = tampered_copy.path().join("state");
    let tampered_disk = Disk::with_state(&tampered_corpus, &tampered_state);
    let tampered_facts = tampered_disk.support_facts();
    assert!(
        !tampered_facts.catalogues.iter().any(|(_, s)| s == &cat_sha),
        "tampered catalogue file must yield no catalogue for that sha"
    );

    // (4) Restored copy whose journal backup.verify evidence line is removed yields no catalogue for that sha
    let tampered_journal_copy = support::temp_copy(&restored_path);
    let audit_dir = tampered_journal_copy.path().join("state/audit");
    let mut found_journal = false;
    for entry in std::fs::read_dir(&audit_dir)
        .expect("read audit dir")
        .flatten()
    {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            let content = std::fs::read_to_string(&p).expect("read journal segment");
            let filtered: Vec<&str> = content
                .lines()
                .filter(|l| !l.contains("backup.verify"))
                .collect();
            let mut new_content = filtered.join("\n");
            if !new_content.is_empty() {
                new_content.push('\n');
            }
            std::fs::write(&p, new_content).expect("write modified journal");
            found_journal = true;
        }
    }
    assert!(found_journal, "must find and edit journal segment");

    let tampered_journal_corpus = tampered_journal_copy.path().join("corpus");
    let tampered_journal_state = tampered_journal_copy.path().join("state");
    let tampered_journal_disk = Disk::with_state(&tampered_journal_corpus, &tampered_journal_state);
    let tampered_journal_facts = tampered_journal_disk.support_facts();
    assert!(
        !tampered_journal_facts
            .catalogues
            .iter()
            .any(|(_, s)| s == &cat_sha),
        "journal without backup.verify evidence must yield no catalogue for that sha"
    );
}

// G. drill_time_is_measured
#[test]
#[ignore = "advisory: takes > 20s in the unoptimized test profile"]
fn drill_time_is_measured() {
    let fix = TestFixture::new();

    // Create 200 MB of synthetic public files (10 files of 20 MB each)
    let chunk_size = 20 * 1024 * 1024;
    let chunk = vec![0x42u8; chunk_size];
    let actual_sha = fieldbook_core::hash::sha256_hex(&chunk);
    let sources_dir = fix.tree_root().join("corpus/sources/public");
    let sources_csv = fix.tree_root().join("corpus/registers/sources.csv");
    let mut sources_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&sources_csv)
        .expect("open sources.csv");
    use std::io::Write as _;
    for i in 0..10 {
        let sid = format!("S-900{i}");
        let fname = format!("{sid}-v1.bin");
        std::fs::write(sources_dir.join(&fname), &chunk).expect("write 20MB file");
        writeln!(
            sources_file,
            "{sid},1,synthetic-perf,\"Large {i}\",PerfBoard,primary_law,primary_evidence,A,\"Synthetic\",synthetic_perf,reviewed,{fname},{actual_sha},2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,V2,public,Note,Purpose,,,,,"
        )
        .expect("write row to sources.csv");
    }
    drop(sources_file);

    let raw_sources = std::fs::read(&sources_csv).expect("read sources.csv");
    let norm_sources = fieldbook_core::schema::table::normalize(
        fieldbook_core::schema::Register::Sources,
        &raw_sources,
    )
    .expect("normalize sources.csv");
    std::fs::write(&sources_csv, norm_sources).expect("write normalized sources.csv");

    fix.backup();

    let start = std::time::Instant::now();
    let outcome = fix.restore(None, true);
    let elapsed = start.elapsed();
    assert_eq!(outcome.code, 0, "drill failed: {}", outcome.stderr);
    println!("drill of 200 MB took {elapsed:?}");
}

/// Adds a `SHA256SUMS` entry named `entry` whose bytes are `bytes`, stored where the entry
/// resolves inside the snapshot, and rewrites `backup.toml` so the snapshot still verifies.
fn plant_snapshot_entry(snap_dir: &Path, entry: &str, bytes: &[u8]) {
    let stored = snap_dir.join(entry);
    std::fs::write(&stored, bytes).expect("write planted file");
    let sums_path = snap_dir.join("SHA256SUMS");
    let sums_text = std::fs::read_to_string(&sums_path).expect("read SHA256SUMS");
    let planted = format!("{}  {entry}", fieldbook_core::hash::sha256_hex(bytes));
    let mut lines: Vec<&str> = sums_text.lines().collect();
    lines.push(&planted);
    lines.sort_by_key(|line| {
        line.split_once("  ")
            .map_or("", |(_, path)| path)
            .as_bytes()
    });
    let mut rewritten = lines.join("\n");
    rewritten.push('\n');
    std::fs::write(&sums_path, rewritten).expect("write SHA256SUMS");
    let added = bytes.len() as u64;
    rewrite_meta(snap_dir, |info| {
        info.files += 1;
        info.bytes += added;
    });
}

#[test]
fn drill_refuses_a_snapshot_path_that_leaves_the_drill_folder() {
    let fix = TestFixture::new();
    fix.backup();
    // `data/state/../../settings.toml` is the file `settings.toml` at the snapshot's top, and
    // joined to `<state>/drill` it is the live `<state>/settings.toml`.
    plant_snapshot_entry(
        &fix.snap_dir(),
        "data/state/../../settings.toml",
        b"role = \"planted\"\n",
    );
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_before = std::fs::read(&settings_path).expect("read settings");
    let kinds_before = journal_kinds(fix.state_dir());

    let outcome = fix.restore(None, true);

    assert_eq!(
        std::fs::read(&settings_path).expect("read settings"),
        settings_before,
        "the drill wrote over live state"
    );
    assert_eq!(outcome.code, 1, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stderr.contains("FB0811") && outcome.stderr.contains("settings.toml"),
        "stderr: {}",
        outcome.stderr
    );
    assert_eq!(journal_kinds(fix.state_dir()), kinds_before);
    assert!(!drill_folder(fix.state_dir()).exists());
}

#[test]
fn restore_to_refuses_a_snapshot_path_that_leaves_the_destination() {
    let fix = TestFixture::new();
    fix.backup();
    plant_snapshot_entry(&fix.snap_dir(), "data/state/../../escape.txt", b"planted\n");
    let kinds_before = journal_kinds(fix.state_dir());

    let dest = fix.dest("restored");
    let outcome = fix.restore(Some(dest.clone()), false);

    assert_eq!(outcome.code, 1, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stderr.contains("FB0815") && outcome.stderr.contains("FB0811"),
        "stderr: {}",
        outcome.stderr
    );
    assert!(!dest.exists(), "nothing may be created");
    assert!(!fix.dest("escape.txt").exists());
    assert_eq!(journal_kinds(fix.state_dir()), kinds_before);
}

#[test]
fn restore_rechecks_an_empty_destination_under_the_lease() {
    let fix = TestFixture::new();
    fix.backup();
    let dest = fix.dest("restored");
    std::fs::create_dir(&dest).expect("create empty destination");
    let kinds_before = journal_kinds(fix.state_dir());

    // Another restore fills the folder after this one saw it empty and before it holds the lease.
    let other = dest.join("corpus/corpus.toml");
    let planted = other.clone();
    fix.world.set_before_write_lease(move || {
        std::fs::create_dir_all(planted.parent().expect("parent")).expect("create corpus dir");
        std::fs::write(&planted, b"written by the other restore\n").expect("write other file");
    });

    let outcome = fix.restore(Some(dest.clone()), false);

    assert_eq!(outcome.code, 1, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stderr.contains("FB0815") && outcome.stderr.contains("not empty"),
        "stderr: {}",
        outcome.stderr
    );
    assert_eq!(
        std::fs::read(&other).expect("read other file"),
        b"written by the other restore\n",
        "the other restore's file was replaced"
    );
    assert!(!dest.join("state").exists(), "nothing may be written");
    assert_eq!(journal_kinds(fix.state_dir()), kinds_before);
}

#[test]
fn restore_inside_the_drill_folder_prints_rename_lines_that_run() {
    let fix = TestFixture::new();
    fix.backup();
    let state = fix.state_dir().to_path_buf();
    let dest = state.join("drill/restored");

    let outcome = fix.restore(Some(dest.clone()), false);
    assert_eq!(outcome.code, 0, "restore failed: {}", outcome.stderr);

    let state_str = state.display().to_string();
    let moved = Path::new(&format!("{state_str}.old"))
        .join("drill/restored/state")
        .display()
        .to_string();
    let stale = dest.join("state").display().to_string();
    let expected_sh = format!("  mv '{moved}' '{state_str}'");
    let expected_pwsh = format!("  Move-Item -LiteralPath '{moved}' -Destination '{state_str}'");
    assert!(
        outcome.stdout.contains(&expected_sh),
        "stdout missing `{expected_sh}`:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains(&expected_pwsh),
        "stdout missing `{expected_pwsh}`:\n{}",
        outcome.stdout
    );
    assert!(
        !outcome.stdout.contains(&format!("mv '{stale}'")),
        "a line reads the restored state from where it no longer is:\n{}",
        outcome.stdout
    );
}
