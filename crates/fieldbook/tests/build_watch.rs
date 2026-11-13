//! Integration tests for `fieldbook build --watch` (`build-watch`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests inspect temporary directories and journal files"
)]

mod support;

use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fieldbook::build::{run_with_info, watch_with_info};
use fieldbook::build_watch_args;
use fieldbook::cli::BuildArgs;
use fieldbook::io::watch::{Poll, corpus_fingerprint};
use fieldbook::run;
use fieldbook_core::audit::{Event, Kind};
use fieldbook_render::meta::BuildInfo;

use support::temp_copy;
use support::world::TestWorld;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini")
}

fn collect_tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
        let mut paths: Vec<PathBuf> = entries
            .map(|res| res.unwrap_or_else(|e| panic!("dir entry: {e}")).path())
            .collect();
        paths.sort();
        for path in paths {
            let meta = std::fs::symlink_metadata(&path)
                .unwrap_or_else(|e| panic!("metadata {}: {e}", path.display()));
            if meta.is_dir() {
                walk(base, &path, out);
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or_else(|e| panic!("strip_prefix: {e}"))
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
                out.insert(rel, bytes);
            }
        }
    }

    let mut map = BTreeMap::new();
    if root.exists() {
        walk(root, root, &mut map);
    }
    map
}

fn read_journal_events(state_dir: &Path) -> Vec<Event> {
    let journal_path = state_dir.join("audit/journal.jsonl");
    if !journal_path.exists() {
        return Vec::new();
    }
    let raw = std::fs::read_to_string(&journal_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", journal_path.display()));
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            Event::parse_line(line)
                .unwrap_or_else(|e| panic!("parse: {e}"))
                .0
        })
        .collect()
}

const ANCHOR: &str = "totals sum the rounded extensions.";

/// Insert `text` after the anchor sentence of a page body (not in the Change note).
fn add_sentence(page: &Path, text: &str) {
    let content = std::fs::read_to_string(page).unwrap_or_else(|e| panic!("read page: {e}"));
    assert!(content.contains(ANCHOR), "anchor sentence is in the page");
    let edited = content.replacen(ANCHOR, &format!("{ANCHOR} {text}"), 1);
    std::fs::write(page, edited).unwrap_or_else(|e| panic!("write page: {e}"));
}

fn count_built_lines(bytes: &[u8]) -> usize {
    bytes
        .split(|&b| b == b'\n')
        .filter(|line| line.strip_suffix(b"\r").unwrap_or(line) == b"built")
        .count()
}

struct ScriptedPoll {
    root: PathBuf,
    steps: VecDeque<Box<dyn FnMut()>>,
}

impl ScriptedPoll {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            steps: VecDeque::new(),
        }
    }

    fn step<F>(&mut self, f: F)
    where
        F: FnMut() + 'static,
    {
        self.steps.push_back(Box::new(f));
    }

    fn idle(&mut self) {
        self.steps.push_back(Box::new(|| {}));
    }
}

impl Poll for ScriptedPoll {
    fn fingerprint(&mut self) -> Result<u64, fieldbook::WorldError> {
        corpus_fingerprint(&self.root)
    }

    fn pause(&mut self) -> bool {
        let Some(mut step) = self.steps.pop_front() else {
            return false;
        };
        step();
        true
    }
}

struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|e| panic!("lock: {e}"))
            .write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| panic!("lock: {e}"))
            .flush()
    }
}

struct EditDuringBuildWriter<F> {
    buf: Vec<u8>,
    on_first_write: Option<F>,
}

impl<F: FnOnce()> EditDuringBuildWriter<F> {
    fn new(on_first_write: F) -> Self {
        Self {
            buf: Vec::new(),
            on_first_write: Some(on_first_write),
        }
    }
}

impl<F: FnOnce()> Write for EditDuringBuildWriter<F> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(hook) = self.on_first_write.take() {
            hook();
        }
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn watch_builds_once_then_waits_while_nothing_changes() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let out_dir = tmp.path().join("out");
    let args = BuildArgs {
        out: out_dir,
        watch: true,
    };
    let info = BuildInfo::default();
    let mut poll = ScriptedPoll::new(tmp.path().to_path_buf());
    poll.idle();
    poll.idle();
    poll.idle();

    let mut out = Vec::new();
    let code = watch_with_info(&args, &world, &info, &mut poll, &mut out);
    assert_eq!(code, 0);

    assert_eq!(count_built_lines(&out), 1);
    let events = read_journal_events(&tmp.path().join(".fieldbook"));
    let write_begins = events
        .iter()
        .filter(|e| e.kind() == Kind::WriteBegin)
        .count();
    assert_eq!(write_begins, 1);
}

#[test]
fn watch_rebuilds_after_a_changed_page() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let out_dir = tmp.path().join("out");
    let args = BuildArgs {
        out: out_dir.clone(),
        watch: true,
    };
    let info = BuildInfo::default();
    let page_path = tmp
        .path()
        .join("corpus/pages/estimating/pricing/est-calc-01.md");
    let sentence = "Synthetic observation for build watch rebuild test.";

    let mut poll = ScriptedPoll::new(tmp.path().to_path_buf());
    let page_for_step = page_path;
    poll.step(move || {
        add_sentence(&page_for_step, sentence);
    });
    poll.idle();

    let mut out = Vec::new();
    let code = watch_with_info(&args, &world, &info, &mut poll, &mut out);
    assert_eq!(code, 0);

    assert_eq!(count_built_lines(&out), 2);
    let events = read_journal_events(&tmp.path().join(".fieldbook"));
    let write_begins = events
        .iter()
        .filter(|e| e.kind() == Kind::WriteBegin)
        .count();
    assert_eq!(write_begins, 2);

    let edition_files = collect_tree_bytes(&out_dir.join("edition"));
    assert!(
        !edition_files.is_empty(),
        "edition output must not be empty"
    );
    let found = edition_files
        .values()
        .any(|b| String::from_utf8_lossy(b).contains(sentence));
    assert!(
        found,
        "built edition HTML must contain the appended sentence"
    );
}

#[test]
fn watch_waits_for_a_change_to_settle() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let out_dir = tmp.path().join("out");
    let args = BuildArgs {
        out: out_dir,
        watch: true,
    };
    let info = BuildInfo::default();
    let page_path = tmp
        .path()
        .join("corpus/pages/estimating/pricing/est-calc-01.md");

    let mut poll = ScriptedPoll::new(tmp.path().to_path_buf());
    let p1 = page_path.clone();
    poll.step(move || {
        add_sentence(&p1, "First edit in flight.");
    });
    let p2 = page_path;
    poll.step(move || {
        add_sentence(&p2, "Second edit while settling.");
    });
    poll.idle();

    let mut out = Vec::new();
    let code = watch_with_info(&args, &world, &info, &mut poll, &mut out);
    assert_eq!(code, 0);

    assert_eq!(
        count_built_lines(&out),
        2,
        "rapid edits must settle into one rebuild (two builds total)"
    );
    let events = read_journal_events(&tmp.path().join(".fieldbook"));
    let write_begins = events
        .iter()
        .filter(|e| e.kind() == Kind::WriteBegin)
        .count();
    assert_eq!(write_begins, 2);
}

#[test]
fn watch_rebuild_equals_a_plain_build() {
    let tmp_watch = temp_copy(&fixture_root());
    let world_watch = TestWorld::new(tmp_watch.path());
    let out_watch = tmp_watch.path().join("out");
    let args_watch = BuildArgs {
        out: out_watch.clone(),
        watch: true,
    };
    let info = BuildInfo::default();
    let sentence = "Byte equality sentence between watch rebuild and plain build.";

    let page_rel = "corpus/pages/estimating/pricing/est-calc-01.md";
    let mut poll = ScriptedPoll::new(tmp_watch.path().to_path_buf());
    let page_watch = tmp_watch.path().join(page_rel);
    poll.step(move || {
        add_sentence(&page_watch, sentence);
    });
    poll.idle();

    let mut out = Vec::new();
    let code = watch_with_info(&args_watch, &world_watch, &info, &mut poll, &mut out);
    assert_eq!(code, 0);
    assert_eq!(count_built_lines(&out), 2);

    // Second directory for plain run_with_info of the identical modified tree
    let tmp_plain = temp_copy(&fixture_root());
    let world_plain = TestWorld::new(tmp_plain.path());
    let out_plain = tmp_plain.path().join("out");
    let args_plain = BuildArgs {
        out: out_plain.clone(),
        watch: false,
    };
    let page_plain = tmp_plain.path().join(page_rel);
    add_sentence(&page_plain, sentence);

    let plain_outcome = run_with_info(&args_plain, &world_plain, &info);
    assert_eq!(plain_outcome.code, 0);

    let watch_edition = collect_tree_bytes(&out_watch.join("edition"));
    let plain_edition = collect_tree_bytes(&out_plain.join("edition"));
    assert!(!watch_edition.is_empty(), "edition files must not be empty");
    assert_eq!(watch_edition, plain_edition);
}

#[test]
fn a_failed_build_keeps_the_old_edition_and_the_watch_goes_on() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let out_dir = tmp.path().join("out");
    let args = BuildArgs {
        out: out_dir.clone(),
        watch: true,
    };
    let info = BuildInfo::default();
    let page_path = tmp
        .path()
        .join("corpus/pages/estimating/pricing/est-calc-01.md");
    let original_bytes = std::fs::read(&page_path).unwrap_or_else(|e| panic!("{e}"));
    let original_str = String::from_utf8(original_bytes.clone()).unwrap_or_else(|e| panic!("{e}"));
    assert!(original_str.contains("type = \"calc\""));
    let broken_str = original_str.replace("type = \"calc\"", "type = \"unknown_page_type\"");

    let shared_out = Arc::new(Mutex::new(Vec::new()));
    let mut writer = SharedWriter(Arc::clone(&shared_out));

    let edition_dir = out_dir.join("edition");
    let initial_edition = Arc::new(Mutex::new(BTreeMap::new()));
    let initial_edition_capture = Arc::clone(&initial_edition);
    let initial_edition_check = Arc::clone(&initial_edition);

    let mut poll = ScriptedPoll::new(tmp.path().to_path_buf());
    let p_corrupt = page_path.clone();
    let broken_to_write = broken_str.into_bytes();
    let ed_cap = edition_dir.clone();
    poll.step(move || {
        let initial = collect_tree_bytes(&ed_cap);
        assert!(!initial.is_empty(), "initial edition must exist");
        *initial_edition_capture
            .lock()
            .unwrap_or_else(|e| panic!("{e}")) = initial;
        std::fs::write(&p_corrupt, &broken_to_write).unwrap_or_else(|e| panic!("{e}"));
    });
    poll.idle();
    poll.idle();

    let p_restore = page_path;
    let orig_to_write = original_bytes;
    let ed_chk = edition_dir;
    let out_for_check = Arc::clone(&shared_out);
    poll.step(move || {
        let out_text =
            String::from_utf8_lossy(&out_for_check.lock().unwrap_or_else(|e| panic!("{e}")))
                .into_owned();
        assert!(
            out_text.contains("build failed (exit 1)"),
            "expected exit 1, got: {out_text}"
        );
        let current_edition = collect_tree_bytes(&ed_chk);
        let initial = initial_edition_check
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .clone();
        assert_eq!(
            current_edition, initial,
            "failed build must keep the old edition bytes unchanged"
        );
        std::fs::write(&p_restore, &orig_to_write).unwrap_or_else(|e| panic!("{e}"));
    });
    poll.idle();
    poll.idle();

    let code = watch_with_info(&args, &world, &info, &mut poll, &mut writer);
    assert_eq!(code, 0);

    let final_out = shared_out.lock().unwrap_or_else(|e| panic!("{e}")).clone();
    let final_text = String::from_utf8_lossy(&final_out);
    assert!(final_text.contains("build failed (exit 1)"));
    assert_eq!(
        count_built_lines(&final_out),
        2,
        "expected two 'built' lines (initial and after restore)"
    );
}

#[test]
fn an_edit_during_a_build_is_built_next() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let out_dir = tmp.path().join("out");
    let args = BuildArgs {
        out: out_dir.clone(),
        watch: true,
    };
    let info = BuildInfo::default();
    let page_path = tmp
        .path()
        .join("corpus/pages/estimating/pricing/est-calc-01.md");
    let sentence = "Sentence appended during initial build output write.";

    let page_clone = page_path;
    let mut writer = EditDuringBuildWriter::new(move || {
        add_sentence(&page_clone, sentence);
    });

    let mut poll = ScriptedPoll::new(tmp.path().to_path_buf());
    poll.idle();
    poll.idle();

    let code = watch_with_info(&args, &world, &info, &mut poll, &mut writer);
    assert_eq!(code, 0);

    assert_eq!(
        count_built_lines(&writer.buf),
        2,
        "edit during build must be built on next iteration"
    );
    let edition_files = collect_tree_bytes(&out_dir.join("edition"));
    let found = edition_files
        .values()
        .any(|b| String::from_utf8_lossy(b).contains(sentence));
    assert!(
        found,
        "second edition must contain the sentence edited during the first build"
    );
}

#[test]
fn out_is_not_watched() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let out_dir = tmp.path().join("out");
    let args = BuildArgs {
        out: out_dir.clone(),
        watch: true,
    };
    let info = BuildInfo::default();

    let mut poll = ScriptedPoll::new(tmp.path().to_path_buf());
    let out_clone = out_dir;
    poll.step(move || {
        let stray = out_clone.join("edition/stray.txt");
        std::fs::write(&stray, "stray file created in out dir").unwrap_or_else(|e| panic!("{e}"));
    });
    poll.idle();
    poll.idle();

    let mut out = Vec::new();
    let code = watch_with_info(&args, &world, &info, &mut poll, &mut out);
    assert_eq!(code, 0);

    assert_eq!(
        count_built_lines(&out),
        1,
        "modifications inside out DIR must not trigger a rebuild"
    );
    let events = read_journal_events(&tmp.path().join(".fieldbook"));
    let write_begins = events
        .iter()
        .filter(|e| e.kind() == Kind::WriteBegin)
        .count();
    assert_eq!(write_begins, 1);
}

#[test]
fn a_fingerprint_error_ends_the_watch_with_exit_3() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let out_dir = tmp.path().join("out");
    let args = BuildArgs {
        out: out_dir,
        watch: true,
    };
    let info = BuildInfo::default();

    let mut poll = ScriptedPoll::new(tmp.path().to_path_buf());
    let corpus_dir = tmp.path().join("corpus");
    poll.step(move || {
        std::fs::remove_dir_all(&corpus_dir).unwrap_or_else(|e| panic!("{e}"));
    });

    let mut out = Vec::new();
    let code = watch_with_info(&args, &world, &info, &mut poll, &mut out);
    assert_eq!(
        code, 3,
        "a fingerprint WorldError must terminate watch with exit 3 (EXIT_IO)"
    );
}

#[test]
fn the_watch_journals_under_the_state_flag() {
    let tmp = temp_copy(&fixture_root());
    let state_dir = tmp.path().join("state-elsewhere");
    let world = TestWorld::with_state(tmp.path(), &state_dir);
    let out_dir = tmp.path().join("out");
    let args = BuildArgs {
        out: out_dir,
        watch: true,
    };
    let info = BuildInfo::default();

    let mut poll = ScriptedPoll::new(tmp.path().to_path_buf());
    poll.idle();

    let mut out = Vec::new();
    let code = watch_with_info(&args, &world, &info, &mut poll, &mut out);
    assert_eq!(code, 0);

    assert_eq!(count_built_lines(&out), 1);
    let state_events = read_journal_events(&state_dir);
    assert!(
        !state_events.is_empty(),
        "journal events must be recorded in overridden state directory"
    );
    let default_state = tmp.path().join(".fieldbook");
    assert!(
        !default_state.exists(),
        "default .fieldbook state directory must not be created"
    );
}

#[test]
fn build_watch_args_parses_state_and_watch() {
    let watch_with_state = vec![
        "--state".to_string(),
        "S".to_string(),
        "build".to_string(),
        "--watch".to_string(),
        "--out".to_string(),
        "D".to_string(),
    ];
    let parsed = build_watch_args(&watch_with_state);
    assert!(parsed.is_some());
    let (state, args) = parsed.unwrap();
    assert_eq!(state, Some(PathBuf::from("S")));
    assert!(args.watch);
    assert_eq!(args.out, PathBuf::from("D"));

    let plain_build = vec!["build".to_string()];
    assert_eq!(build_watch_args(&plain_build), None);

    let watch_no_state = vec![
        "build".to_string(),
        "--watch".to_string(),
        "--out".to_string(),
        "out-dir".to_string(),
    ];
    let parsed_no_state = build_watch_args(&watch_no_state);
    assert!(parsed_no_state.is_some());
    let (state_none, args_watch) = parsed_no_state.unwrap();
    assert_eq!(state_none, None);
    assert!(args_watch.watch);
    assert_eq!(args_watch.out, PathBuf::from("out-dir"));
}

#[test]
fn watch_through_run_is_refused() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let outcome = run(&["build".into(), "--watch".into()], &world);
    assert_eq!(outcome.code, 2);
    assert!(
        outcome
            .stderr
            .contains("error: --watch needs a terminal; run it from the program"),
        "stderr was: {}",
        outcome.stderr
    );
    let events = read_journal_events(&tmp.path().join(".fieldbook"));
    assert!(
        events.is_empty(),
        "refused run(&['build', '--watch']) must write no journal events"
    );
}

#[test]
fn fingerprint_ignores_git_and_reads_no_content() {
    let tmp = temp_copy(&fixture_root());
    let fp_initial = corpus_fingerprint(tmp.path()).unwrap();

    // 1. Changes inside corpus/.git/ are ignored.
    let git_dir = tmp.path().join("corpus/.git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("x"), b"git internal content").unwrap();
    let fp_git = corpus_fingerprint(tmp.path()).unwrap();
    assert_eq!(
        fp_initial, fp_git,
        "corpus_fingerprint must ignore files inside corpus/.git/"
    );

    // 2. Same length with a new modified time gives a different fingerprint.
    let page_path = tmp
        .path()
        .join("corpus/pages/estimating/pricing/est-calc-01.md");
    let existing_bytes = std::fs::read(&page_path).unwrap();
    std::fs::write(&page_path, &existing_bytes).unwrap();
    let file = std::fs::File::options()
        .write(true)
        .open(&page_path)
        .unwrap();
    let new_mtime =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000);
    file.set_modified(new_mtime).unwrap();
    drop(file);

    let fp_mtime = corpus_fingerprint(tmp.path()).unwrap();
    assert_ne!(
        fp_initial, fp_mtime,
        "corpus_fingerprint must change when mtime changes even if length is identical"
    );

    // 3. Unreadable file on Unix (mode 000): symlink_metadata succeeds without opening/reading.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&page_path).unwrap().permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&page_path, perms).unwrap();

        let fp_res = corpus_fingerprint(tmp.path());

        let mut restored = std::fs::metadata(&page_path).unwrap().permissions();
        restored.set_mode(0o644);
        let _ = std::fs::set_permissions(&page_path, restored);

        assert!(
            fp_res.is_ok(),
            "corpus_fingerprint must read metadata only, not content, succeeding on mode 000 files"
        );
    }
}
