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
            world,
        }
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

    let dest = tempfile::tempdir().expect("dest").path().join("restored");
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

        let dest1 = tempfile::tempdir().expect("dest").path().join("restored");
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

        let dest2 = tempfile::tempdir().expect("dest").path().join("restored");
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

        let dest3 = tempfile::tempdir().expect("dest").path().join("restored");
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

    let dest = tempfile::tempdir().expect("dest").path().join("restored");
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

    let dest = tempfile::tempdir().expect("dest").path().join("restored");
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

    let dest = tempfile::tempdir().expect("dest").path().join("restored");
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
