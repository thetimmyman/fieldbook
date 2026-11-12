//! Integration tests for `fieldbook build` (`build-v0`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests spawn the fieldbook binary and inspect temporary directories and journal files"
)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use fieldbook::build::{
    BuildPlanError, plan_edition, plan_edition_with_site, run_with_info, write_edition_outputs,
};
use fieldbook::cli::BuildArgs;
use fieldbook::io::journal::set_write_pause_hook;
use fieldbook::io::{dirs, state};
use fieldbook::{Error, World};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{DestKind, Event, EventPayload, Kind, LabelId, LabelSet, Via, Word};
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Code, Host, Options, Severity, load};
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
use fieldbook_core::labels::PUBLIC;
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::strings;
use fieldbook_render::csp::CSP_STATIC;
use fieldbook_render::lint;
use fieldbook_render::meta::{BuildInfo, BuildProfile};
use fieldbook_render::site::{self, Site};
use fieldbook_render::source;

use support::temp_copy;
use support::world::TestWorld;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini")
}

fn run_bin(cwd: &Path, envs: &[(&str, &str)], args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fieldbook"));
    cmd.current_dir(cwd).args(args);
    for &(k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn fieldbook binary: {e}"))
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
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
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
                .unwrap_or_else(|e| panic!("parse journal line: {e}"))
                .0
        })
        .collect()
}

mod build {
    use super::*;

    #[test]
    fn refuses_a_corpus_with_errors() {
        let tmp = temp_copy(&fixture_root());
        std::fs::write(
            tmp.path().join("corpus/corpus.toml"),
            "schema_version = 999\n",
        )
        .unwrap_or_else(|e| panic!("corrupt corpus.toml: {e}"));

        let out_dir = tmp.path().join("out-err");
        let out_str = out_dir.to_string_lossy().into_owned();
        let output = run_bin(tmp.path(), &[], &["build", "--out", &out_str]);

        assert_eq!(
            output.status.code(),
            Some(1),
            "expected exit 1 on corpus errors, stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(Code::Fb0600.id()),
            "stderr must contain FB0600, got:\n{stderr}"
        );
        assert!(
            !out_dir.exists(),
            "nothing may be written when check reports errors"
        );
        assert!(
            read_journal_events(&tmp.path().join(".fieldbook")).is_empty(),
            "refused check must append no journal event"
        );
    }

    #[test]
    fn builds_the_fixture() {
        let tmp = temp_copy(&fixture_root());
        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());

        let edition = tmp.path().join("out/edition");
        assert!(edition.join("index.html").is_file());
        assert!(edition.join("assets/strings.js").is_file());
        assert!(edition.join("assets/app.css").is_file());
        assert!(edition.join("assets/app.js").is_file());
        assert!(tmp.path().join("out/.fieldbook-out").is_file());

        let index_html = std::fs::read_to_string(edition.join("index.html"))
            .unwrap_or_else(|e| panic!("read index.html: {e}"));
        assert!(index_html.contains(CSP_STATIC));
        assert!(index_html.contains("<main id=\"fb:main\" tabindex=\"-1\">"));
        assert!(
            index_html.contains("id=\"areas\""),
            "the home page has its area index"
        );
        assert!(index_html.contains("<nav aria-label=\"Sections\">"));
        assert!(index_html.contains("<details class=\"fold nav-more\">"));
        assert!(!index_html.contains("class=\"not-available\""));
    }

    #[test]
    fn build_out_is_the_parent_folder() {
        let tmp = temp_copy(&fixture_root());
        let parent = tmp.path().join("custom-out");
        let parent_str = parent.to_string_lossy().into_owned();
        let output = run_bin(tmp.path(), &[], &["build", "--out", &parent_str]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            parent.join("edition/index.html").is_file(),
            "$T/edition/index.html must exist"
        );
        assert!(
            !parent.join("index.html").exists(),
            "$T/index.html must not exist directly in parent folder"
        );
    }

    #[test]
    fn a_build_appends_one_write_begin_and_one_write_end() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let state_dir = world.state_dir().to_path_buf();
        let saw_begin_before_write = Arc::new(AtomicBool::new(false));
        let hook_flag = Arc::clone(&saw_begin_before_write);
        let hook_state_dir = state_dir.clone();

        set_write_pause_hook(Some(Arc::new(move |_path| {
            let events = read_journal_events(&hook_state_dir);
            let write_begins = events
                .iter()
                .filter(|e| e.kind() == Kind::WriteBegin)
                .count();
            let write_ends = events.iter().filter(|e| e.kind() == Kind::WriteEnd).count();
            if write_begins == 1 && write_ends == 0 {
                hook_flag.store(true, Ordering::SeqCst);
            }
        })));

        let args = BuildArgs {
            out: PathBuf::from("out"),
        };
        let outcome = run_with_info(&args, &world, &BuildInfo::default());
        set_write_pause_hook(None);

        assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
        assert!(
            saw_begin_before_write.load(Ordering::SeqCst),
            "Journal::begin_export must append write.begin before io::out::replace writes files"
        );

        let events = read_journal_events(&state_dir);
        let write_begins: Vec<&Event> = events
            .iter()
            .filter(|e| e.kind() == Kind::WriteBegin)
            .collect();
        let write_ends: Vec<&Event> = events
            .iter()
            .filter(|e| e.kind() == Kind::WriteEnd)
            .collect();
        assert_eq!(
            write_begins.len(),
            1,
            "expected exactly one write.begin, got {events:?}"
        );
        assert_eq!(
            write_ends.len(),
            1,
            "expected exactly one write.end, got {events:?}"
        );
        assert!(
            write_begins[0].seq < write_ends[0].seq,
            "write.begin must precede write.end"
        );

        match &write_begins[0].payload {
            EventPayload::WriteBegin {
                verb, labels, dest, ..
            } => {
                assert_eq!(verb.as_str(), "build");
                assert_eq!(labels, &LabelSet::public());
                assert_eq!(*dest, Some(DestKind::Edition));
            }
            other => panic!("expected WriteBegin payload, got {other:?}"),
        }
        match &write_ends[0].payload {
            EventPayload::WriteEnd { outcome, dest, .. } => {
                assert_eq!(outcome.as_str(), "ok");
                assert_eq!(*dest, Some(DestKind::Edition));
            }
            other => panic!("expected WriteEnd payload, got {other:?}"),
        }
    }

    // control: DT-01
    #[test]
    fn two_builds_are_identical() {
        let tmp = temp_copy(&fixture_root());
        let out_a = tmp.path().join("out-a");
        let out_b = tmp.path().join("out-b");
        let out_a_str = out_a.to_string_lossy().into_owned();
        let out_b_str = out_b.to_string_lossy().into_owned();

        let first = run_bin(
            tmp.path(),
            &[("TZ", "UTC"), ("SOURCE_DATE_EPOCH", "1772323200")],
            &["build", "--out", &out_a_str],
        );
        assert_eq!(
            first.status.code(),
            Some(0),
            "first build failed:\n{}",
            String::from_utf8_lossy(&first.stderr)
        );

        let subdir = tmp.path().join("corpus/pages");
        let second = run_bin(
            &subdir,
            &[
                ("TZ", "Pacific/Auckland"),
                ("SOURCE_DATE_EPOCH", "1772323200"),
            ],
            &["build", "--out", &out_b_str],
        );
        assert_eq!(
            second.status.code(),
            Some(0),
            "second build failed:\n{}",
            String::from_utf8_lossy(&second.stderr)
        );

        let files_a = collect_tree_bytes(&out_a.join("edition"));
        let files_b = collect_tree_bytes(&out_b.join("edition"));
        assert!(!files_a.is_empty(), "edition output must not be empty");

        let mut all_paths: Vec<String> = files_a.keys().chain(files_b.keys()).cloned().collect();
        all_paths.sort();
        all_paths.dedup();

        for rel in all_paths {
            let bytes_a = files_a.get(&rel);
            let bytes_b = files_b.get(&rel);
            if bytes_a != bytes_b {
                panic!("{}: two builds differed at `{rel}`", Code::Fb0606.id());
            }
        }
    }

    #[test]
    fn a_rebuild_drops_a_page_the_corpus_no_longer_has() {
        let tmp = temp_copy(&fixture_root());
        let out_t = tmp.path().join("out-rebuild");
        let out_t_str = out_t.to_string_lossy().into_owned();

        let first = run_bin(tmp.path(), &[], &["build", "--out", &out_t_str]);
        assert_eq!(first.status.code(), Some(0));

        let gone = out_t.join("edition/p/gone.html");
        std::fs::create_dir_all(gone.parent().unwrap_or_else(|| Path::new(".")))
            .unwrap_or_else(|e| panic!("mkdir p: {e}"));
        std::fs::write(&gone, b"<html>gone</html>\n")
            .unwrap_or_else(|e| panic!("write gone.html: {e}"));
        assert!(gone.is_file());

        let second = run_bin(tmp.path(), &[], &["build", "--out", &out_t_str]);
        assert_eq!(second.status.code(), Some(0));
        assert!(
            !gone.exists(),
            "rebuild must remove planted edition/p/gone.html"
        );

        let out_fresh = tmp.path().join("out-fresh");
        let out_fresh_str = out_fresh.to_string_lossy().into_owned();
        let fresh = run_bin(tmp.path(), &[], &["build", "--out", &out_fresh_str]);
        assert_eq!(fresh.status.code(), Some(0));

        assert_eq!(
            collect_tree_bytes(&out_t.join("edition")),
            collect_tree_bytes(&out_fresh.join("edition")),
            "rebuilt edition folder must equal a fresh build"
        );
    }

    #[test]
    fn a_foreign_edition_folder_is_not_deleted() {
        let tmp = temp_copy(&fixture_root());
        let foreign_out = tmp.path().join("foreign-out");
        let keep_file = foreign_out.join("edition/keep.txt");
        std::fs::create_dir_all(
            keep_file
                .parent()
                .unwrap_or_else(|| panic!("keep.txt parent")),
        )
        .unwrap_or_else(|e| panic!("create foreign edition dir: {e}"));
        std::fs::write(&keep_file, b"keep me\n").unwrap_or_else(|e| panic!("write keep.txt: {e}"));

        let foreign_str = foreign_out.to_string_lossy().into_owned();
        let output = run_bin(tmp.path(), &[], &["build", "--out", &foreign_str]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "foreign root must exit 2, stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(
                "it was not made by fieldbook build; remove it yourself or choose another --out"
            ),
            "unexpected stderr: {stderr}"
        );
        assert_eq!(
            std::fs::read(&keep_file).unwrap_or_else(|e| panic!("read keep.txt: {e}")),
            b"keep me\n",
            "keep.txt in foreign edition folder must remain untouched"
        );
        assert!(
            read_journal_events(&tmp.path().join(".fieldbook")).is_empty(),
            "foreign root refusal must append no journal event"
        );
    }

    #[test]
    fn a_second_build_removes_a_stale_file() {
        let tmp = temp_copy(&fixture_root());
        let out_dir = tmp.path().join("out-stale");
        let out_str = out_dir.to_string_lossy().into_owned();

        let first = run_bin(tmp.path(), &[], &["build", "--out", &out_str]);
        assert_eq!(first.status.code(), Some(0));

        let stale = out_dir.join("edition/stale.txt");
        std::fs::write(&stale, b"stale\n").unwrap_or_else(|e| panic!("write stale.txt: {e}"));
        assert!(stale.is_file());

        let second = run_bin(tmp.path(), &[], &["build", "--out", &out_str]);
        assert_eq!(second.status.code(), Some(0));
        assert!(!stale.exists(), "second build must remove stale.txt");
        assert!(out_dir.join("edition/index.html").is_file());
    }

    #[test]
    fn build_binds_every_output() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let out_dir = tmp.path().join("out-bound");

        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());

        let plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &out_dir,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("plan_edition: {e:?}"))
        .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

        let mut journal =
            state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
        let audited = journal
            .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
            .unwrap_or_else(|e| panic!("{e}"));
        write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
        journal
            .end(audited, Word::new_static("ok"))
            .unwrap_or_else(|e| panic!("{e}"));

        // Verify marker and every planned file exist on disk with exact bound bytes.
        assert_eq!(
            std::fs::read(&plan.marker_path).unwrap_or_else(|e| panic!("{e}")),
            dirs::marker_bytes()
        );
        for (rel, expected) in plan.site.files() {
            let disk = std::fs::read(plan.edition_dir.join(rel)).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(&disk, expected, "output `{rel}` bytes must match plan");
        }

        // Verify that every single output file and the marker is checked at the final sink
        // against the delivery-bound Allowed token.
        let file_keys: Vec<String> = plan.site.files().keys().cloned().collect();
        for target_rel in &file_keys {
            let probe_out = tmp
                .path()
                .join(format!("probe-{}", target_rel.replace(['/', '.'], "-")));
            let mut probe_plan = plan_edition(
                &view,
                &BuildInfo::default(),
                &probe_out,
                &eff,
                &LabelSet::public(),
                now,
                shipped,
            )
            .unwrap_or_else(|e| panic!("{e:?}"))
            .unwrap_or_else(|d| panic!("{d:?}"));

            let probe_audited = journal
                .begin_export(
                    "build",
                    &probe_plan.write_set,
                    &probe_plan.allowed,
                    &probe_plan.out_dir,
                )
                .unwrap_or_else(|e| panic!("{e}"));

            let mut mutated_site = Site::new();
            for (rel, bytes) in probe_plan.site.files() {
                if rel == target_rel {
                    let mut altered = bytes.clone();
                    altered.push(b'\n');
                    mutated_site.insert(rel, altered);
                } else {
                    mutated_site.insert(rel, bytes.clone());
                }
            }
            probe_plan.site = mutated_site;

            let res = write_edition_outputs(&probe_plan, &probe_audited);
            let _ = journal.end(probe_audited, Word::new_static("failed"));
            assert!(
                matches!(res, Err(Error::OutOfScope(_))),
                "final sink must refuse altered bytes for `{target_rel}`, got {res:?}"
            );
        }

        // Also probe the marker bytes at the final sink.
        let marker_probe_out = tmp.path().join("probe-marker");
        let mut marker_plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &marker_probe_out,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));
        let marker_audited = journal
            .begin_export(
                "build",
                &marker_plan.write_set,
                &marker_plan.allowed,
                &marker_plan.out_dir,
            )
            .unwrap_or_else(|e| panic!("{e}"));
        marker_plan.marker_bytes.push(b'!');
        let marker_res = write_edition_outputs(&marker_plan, &marker_audited);
        let _ = journal.end(marker_audited, Word::new_static("failed"));
        assert!(
            matches!(marker_res, Err(Error::OutOfScope(_))),
            "final sink must refuse altered marker bytes, got {marker_res:?}"
        );
    }

    #[test]
    fn changed_build_bytes_or_target_are_refused() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());
        let mut journal =
            state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));

        // 1. Byte mutation case at the final sink seam (after valid Allowed + Audited).
        let byte_out = tmp.path().join("sink-byte-case");
        let mut byte_plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &byte_out,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        let byte_audited = journal
            .begin_export(
                "build",
                &byte_plan.write_set,
                &byte_plan.allowed,
                &byte_plan.out_dir,
            )
            .unwrap_or_else(|e| panic!("{e}"));

        let mut altered_site = Site::new();
        for (rel, bytes) in byte_plan.site.files() {
            if rel == "index.html" {
                let mut mutated = bytes.clone();
                mutated.extend_from_slice(b"<!-- mutated after approval -->");
                altered_site.insert(rel, mutated);
            } else {
                altered_site.insert(rel, bytes.clone());
            }
        }
        byte_plan.site = altered_site;

        let byte_res = write_edition_outputs(&byte_plan, &byte_audited);
        let _ = journal.end(byte_audited, Word::new_static("failed"));
        assert!(
            matches!(byte_res, Err(Error::OutOfScope(_))),
            "altered build bytes after approval must be refused at the final sink, got {byte_res:?}"
        );
        assert!(
            !byte_plan.edition_dir.join("index.html").exists(),
            "refused byte-altered write must not create index.html"
        );

        // 2. Target path mutation case at the final sink seam (after valid Allowed + Audited).
        let path_out = tmp.path().join("sink-path-case");
        let mut path_plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &path_out,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        let path_audited = journal
            .begin_export(
                "build",
                &path_plan.write_set,
                &path_plan.allowed,
                &path_plan.out_dir,
            )
            .unwrap_or_else(|e| panic!("{e}"));

        let mut redirected_site = Site::new();
        for (rel, bytes) in path_plan.site.files() {
            if rel == "index.html" {
                redirected_site.insert("unapproved.html", bytes.clone());
            } else {
                redirected_site.insert(rel, bytes.clone());
            }
        }
        path_plan.site = redirected_site;

        let path_res = write_edition_outputs(&path_plan, &path_audited);
        let _ = journal.end(path_audited, Word::new_static("failed"));
        assert!(
            matches!(path_res, Err(Error::OutOfScope(_))),
            "altered target path after approval must be refused at the final sink, got {path_res:?}"
        );
        assert!(
            !path_plan.edition_dir.join("unapproved.html").exists(),
            "refused path-altered write must not create unapproved.html"
        );
    }

    #[test]
    fn public_shell_delivery_succeeds() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());

        // Unauthorized delivery (non-public label set) refuses before creating the destination.
        let unauth_out = tmp.path().join("unauth-out");
        let np = LabelId::parse("np-0007").unwrap_or_else(|e| panic!("{e}"));
        let non_public = LabelSet::from_ids([PUBLIC, np]);
        let unauth_res = plan_edition(
            &view,
            &BuildInfo::default(),
            &unauth_out,
            &eff,
            &non_public,
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert!(
            unauth_res.is_err(),
            "non-public label set must be denied for Dest::Edition"
        );
        assert!(
            !unauth_out.exists(),
            "unauthorized delivery must refuse before creating the destination folder"
        );

        // Authorized public shell delivery succeeds and writes the shell, marker, and assets.
        let ok_out = tmp.path().join("public-out");
        let outcome = run_with_info(
            &BuildArgs {
                out: ok_out.clone(),
            },
            &world,
            &BuildInfo::default(),
        );
        assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);

        assert_eq!(
            std::fs::read(ok_out.join(dirs::MARKER_FILE)).unwrap_or_else(|e| panic!("{e}")),
            dirs::marker_bytes()
        );
        let index = std::fs::read_to_string(ok_out.join("edition/index.html"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(index.contains(CSP_STATIC));
        assert!(index.contains("href=\"assets/app.css\""));
        assert!(index.contains("src=\"assets/strings.js\""));
        assert!(index.contains(strings::unreleased_build()));
        assert_eq!(
            std::fs::read_to_string(ok_out.join("edition/assets/strings.js"))
                .unwrap_or_else(|e| panic!("{e}")),
            strings::js()
        );
        assert!(ok_out.join("edition/assets/app.css").is_file());
    }

    #[test]
    fn released_build_without_heading_preserves_output() {
        let tmp = temp_copy(&fixture_root());
        let out_dir = tmp.path().join("out-preserve");

        // Replace `corpus/changes.md` with a valid file that has no `## edition-NNN` heading.
        std::fs::write(
            tmp.path().join("corpus/changes.md"),
            "# Changes\n\nNo edition heading yet.\n",
        )
        .unwrap_or_else(|e| panic!("write changes.md: {e}"));

        let world = TestWorld::new(tmp.path());

        // 1. Incomplete release metadata (date without commit) still yields the unreleased shell
        //    and populates existing owned output in `out_dir`.
        let built_date =
            CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("valid date: {e}"));
        let incomplete_info = BuildInfo {
            built: Some(built_date),
            commit: None,
            workbook_epoch: 1_772_323_200,
            profile: BuildProfile::Silent,
        };
        let initial = run_with_info(
            &BuildArgs {
                out: out_dir.clone(),
            },
            &world,
            &incomplete_info,
        );
        assert_eq!(
            initial.code, 0,
            "incomplete release metadata must still succeed with unreleased shell: {}",
            initial.stderr
        );
        let initial_index = std::fs::read_to_string(out_dir.join("edition/index.html"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(initial_index.contains(strings::unreleased_build()));

        // Plant a known byte snapshot in the owned `out_dir/edition/index.html` so we can verify
        // that a subsequent failed released build preserves every existing output byte.
        let preserved_marker = b"<!-- owned output before failed released build -->\n";
        std::fs::write(out_dir.join("edition/index.html"), preserved_marker)
            .unwrap_or_else(|e| panic!("{e}"));
        let before_files = collect_tree_bytes(&out_dir);
        let events_before = read_journal_events(world.state_dir());

        // 2. Now run with valid release date AND valid commit, while `changes.md` lacks an
        //    edition heading. Must return exit 1, preserve every output byte, and emit no write.begin.
        let released_info = BuildInfo {
            built: Some(built_date),
            commit: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
            workbook_epoch: 1_772_323_200,
            profile: BuildProfile::Silent,
        };
        let failed = run_with_info(
            &BuildArgs {
                out: out_dir.clone(),
            },
            &world,
            &released_info,
        );
        assert_eq!(
            failed.code, 1,
            "released build without edition heading must return exit 1, stderr: {}",
            failed.stderr
        );

        let after_files = collect_tree_bytes(&out_dir);
        assert_eq!(
            after_files, before_files,
            "failed released build must preserve every existing output byte"
        );

        let events_after = read_journal_events(world.state_dir());
        let write_begins_before = events_before
            .iter()
            .filter(|e| e.kind() == Kind::WriteBegin)
            .count();
        let write_begins_after = events_after
            .iter()
            .filter(|e| e.kind() == Kind::WriteBegin)
            .count();
        assert_eq!(
            write_begins_after, write_begins_before,
            "failed released build must emit no write.begin"
        );
    }

    #[test]
    fn the_fixture_site_is_lint_clean() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let sel = match check(
            &eff,
            &Dest::Edition,
            None,
            None,
            world.now(),
            Shipped::Never,
        ) {
            Decision::Allowed(a) => a,
            Decision::Denied(d) => panic!("denied: {d:?}"),
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());
        let site = site::render(&view, &BuildInfo::default()).unwrap_or_else(|e| panic!("{e}"));
        let diags = lint::lint(&site, &source::original_names(&view));
        assert!(
            diags.is_empty(),
            "fixture site must be lint-clean, got:\n{}",
            fieldbook_core::check::Report::from_parts(diags, Vec::new()).render_text()
        );
    }

    #[test]
    fn a_lint_finding_writes_nothing() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let sel = match check(
            &eff,
            &Dest::Edition,
            None,
            None,
            world.now(),
            Shipped::Never,
        ) {
            Decision::Allowed(a) => a,
            Decision::Denied(d) => panic!("denied: {d:?}"),
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());
        let ctx =
            site::render_context(&view, &BuildInfo::default()).unwrap_or_else(|e| panic!("{e}"));
        let mut site = site::render_with_context(&view, &ctx, BuildProfile::Silent);
        site.insert(
            "bad.html",
            format!(
                "<!doctype html><html><head><meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\"></head><body><a href=\"missing.html\">Dead</a></body></html>"
            )
            .into_bytes(),
        );

        let out_dir = tmp.path().join("out-lint-fail");
        let res = plan_edition_with_site(
            &view,
            ctx,
            site,
            &out_dir,
            &eff,
            &LabelSet::public(),
            world.now(),
            Shipped::Never,
        );
        match res {
            Err(BuildPlanError::Lint(diags)) => {
                assert_eq!(diags.len(), 1);
                assert_eq!(diags[0].code, Code::Fb0603);
            }
            other => panic!("expected BuildPlanError::Lint, got {other:?}"),
        }
        assert!(
            !out_dir.exists(),
            "a lint finding must write no file to out_dir"
        );
        let events = read_journal_events(world.state_dir());
        assert!(
            events.iter().all(|e| e.kind() != Kind::WriteBegin),
            "a lint finding must append no write.begin"
        );
    }

    #[test]
    fn navigation_noninterference_in_edition_shell() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree_base = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model_base, report_base) = load(
            &Host::new(tree_base.clone(), world.today()),
            &Options::new(false),
        );
        assert!(report_base.is_ok());

        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view_base = LabelledView::new(&model_base, &sel, &ReadableStores::corpus());
        let out_base = tmp.path().join("out-nav-base");
        let plan_base = plan_edition(
            &view_base,
            &BuildInfo::default(),
            &out_base,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        // Add a forbidden (`np-0007`) page to an in-memory copy of the tree and confirm neither
        // shell delivery, available links, nor navigation counts change.
        let mut tree_mixed = tree_base;
        tree_mixed.insert_bytes(
            "corpus/pages/estimating/pricing/est-forbidden-canary.md",
            b"+++\nid = \"est-forbidden-canary\"\ntitle = \"Forbidden pricing canary\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSecret.\n\n## The rule\n\nSecret.\n\n## How it works\n\nSecret.\n\n## Worked example\n\nSecret.\n\n## Never confuse\n\nSecret.\n\n## Company calibration\n\nSecret.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-03-01: Secret.\n"
                .to_vec(),
        );
        let (model_mixed, _report_mixed) =
            load(&Host::new(tree_mixed, world.today()), &Options::new(false));
        let view_mixed = LabelledView::new(&model_mixed, &sel, &ReadableStores::corpus());
        let out_mixed = tmp.path().join("out-nav-mixed");
        let plan_mixed = plan_edition(
            &view_mixed,
            &BuildInfo::default(),
            &out_mixed,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        assert_eq!(plan_mixed.site.files(), plan_base.site.files());
        let nav_base = fieldbook_render::nav::build(&view_base, &site::planned_paths(&view_base));
        let nav_mixed =
            fieldbook_render::nav::build(&view_mixed, &site::planned_paths(&view_mixed));
        assert_eq!(nav_mixed, nav_base);
    }

    #[test]
    fn build_from_view_omits_hidden_canary() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let mut tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-forbidden-canary.md",
            b"+++\nid = \"est-forbidden-canary\"\ntitle = \"Forbidden pricing canary\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nForbidden pricing canary body text.\n\n## The rule\n\nSecret rule.\n\n## How it works\n\nSecret steps.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nSecret example.\n\n## Never confuse\n\nSecret distinction.\n\n## Company calibration\n\nSecret calibration.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Added forbidden pricing canary.\n"
                .to_vec(),
        );

        let (model, _report) = load(&Host::new(tree, world.today()), &Options::new(false));
        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());

        let out_dir = tmp.path().join("out-canary");
        let plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &out_dir,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("plan_edition: {e:?}"))
        .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

        let mut journal =
            state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
        let audited = journal
            .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
            .unwrap_or_else(|e| panic!("{e}"));
        write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
        journal
            .end(audited, Word::new_static("ok"))
            .unwrap_or_else(|e| panic!("{e}"));

        let edition = out_dir.join("edition");
        let concept_path = edition.join("p/cmp-system-review.html");
        let steps_path = edition.join("p/cmp-review-steps.html");
        assert!(
            concept_path.is_file(),
            "permitted page p/cmp-system-review.html must be written"
        );
        assert!(
            steps_path.is_file(),
            "permitted page p/cmp-review-steps.html must be written"
        );
        let concept_html = std::fs::read_to_string(&concept_path).unwrap_or_else(|e| panic!("{e}"));
        let steps_html = std::fs::read_to_string(&steps_path).unwrap_or_else(|e| panic!("{e}"));

        // Permitted page title, body, and emitted inter-page PageLink must reach the site.
        assert!(
            concept_html.contains("Synthetic business-system review and withholding overview"),
            "permitted page title must appear in p/cmp-system-review.html"
        );
        assert!(
            concept_html.contains(
                "Synthetic reviewers check control operation and record open items before billing runs."
            ),
            "permitted page body must appear in p/cmp-system-review.html"
        );
        assert!(
            steps_html.contains("class=\"plink\" href=\"cmp-system-review.html\""),
            "permitted inter-page PageLink must appear in p/cmp-review-steps.html, got:\n{steps_html}"
        );

        // Forbidden page file, title, body, and link must be absent across the entire edition.
        assert!(
            !edition.join("p/est-forbidden-canary.html").exists(),
            "forbidden page p/est-forbidden-canary.html must not exist"
        );
        for (rel, bytes) in collect_tree_bytes(&edition) {
            let text = String::from_utf8_lossy(&bytes);
            assert!(
                !text.contains("Forbidden pricing canary"),
                "forbidden page title must not appear in `{rel}`"
            );
            assert!(
                !text.contains("Forbidden pricing canary body text."),
                "forbidden page body must not appear in `{rel}`"
            );
            assert!(
                !text.contains("est-forbidden-canary.html"),
                "forbidden page link must not appear in `{rel}`"
            );
        }
    }

    #[test]
    fn build_registers_counts_use_the_permitted_view() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree_base = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model_base, report_base) = load(
            &Host::new(tree_base.clone(), world.today()),
            &Options::new(false),
        );
        assert!(report_base.is_ok());

        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view_base = LabelledView::new(&model_base, &sel, &ReadableStores::corpus());

        // The base with the kept-row cells that name the canaries: the permitted view keeps those
        // cells verbatim, so this tree, not the plain base, is what the mixed tree must equal.
        // Its references dangle (FB0207), and nothing else may be reported.
        let mut tree_cells = tree_base.clone();
        support::name_canaries_from_kept_rows(&mut tree_cells);
        let (model_cells, report_cells) =
            load(&Host::new(tree_cells, world.today()), &Options::new(false));
        support::assert_only_dangling_references(&report_cells);
        let view_cells = LabelledView::new(&model_cells, &sel, &ReadableStores::corpus());
        assert_eq!(view_cells.left_out(), 0);

        // Add forbidden (`np-0007`) register rows (source S-0099 and its dependent claim, conflict
        // and glossary term), the kept-row cells that name them, and a page to a mixed tree.
        let mut tree_mixed = tree_base;
        support::add_canary_register_rows(&mut tree_mixed);
        tree_mixed.insert_bytes(
            "corpus/pages/estimating/pricing/est-forbidden-canary.md",
            b"+++\nid = \"est-forbidden-canary\"\ntitle = \"Forbidden pricing canary\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSecret.\n\n## The rule\n\nSecret.\n\n## How it works\n\nSecret.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nSecret.\n\n## Never confuse\n\nSecret.\n\n## Company calibration\n\nSecret.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Secret.\n"
                .to_vec(),
        );

        // The left-out source version's extracted text: the view must leave its folder out.
        let canary_text: &[u8] =
            b"Forbidden extracted text canary. Synthetic padding so that the page is not thin. Forbidden quote.\n";
        tree_mixed.insert_bytes(
            "corpus/sources/text/S-0099-v1/p0001.txt",
            canary_text.to_vec(),
        );
        tree_mixed.insert_bytes(
            "corpus/sources/text/S-0099-v1/extraction.toml",
            format!(
                "tool = \"plain\"\ntool_version = \"1\"\nmethod = \"plain\"\npage_count = 1\nthin_pages = []\noriginal_sha256 = \"{}\"\n\n[pages]\np0001 = \"{}\"\n[locators]\n\"9\" = 1\n",
                "9".repeat(64),
                fieldbook_core::hash::sha256_hex(canary_text)
            )
            .into_bytes(),
        );

        // The loader reports the repository-Corpus handling errors of the canary source and page
        // (FB0410) and the canary claim left on the version that the kept `S-0001` supersedes
        // (FB0222), and nothing else, but still builds the model the permitted view must filter.
        let (model_mixed, report_mixed) =
            load(&Host::new(tree_mixed, world.today()), &Options::new(false));
        support::assert_report_codes(
            &report_mixed,
            &[
                ("FB0410", Severity::Error),
                ("FB0410", Severity::Error),
                ("FB0222", Severity::Warning),
            ],
        );
        assert!(
            model_mixed
                .tree()
                .get_bytes("corpus/sources/text/S-0099-v1/p0001.txt")
                .is_some(),
            "the model holds the text folder the view must leave out"
        );
        let view_mixed = LabelledView::new(&model_mixed, &sel, &ReadableStores::corpus());
        assert!(view_mixed.left_out() >= 5);

        let out_base = tmp.path().join("out-reg-base");
        let plan_base = plan_edition(
            &view_base,
            &BuildInfo::default(),
            &out_base,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        let plan_cells = plan_edition(
            &view_cells,
            &BuildInfo::default(),
            &tmp.path().join("out-reg-cells"),
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        let out_mixed = tmp.path().join("out-reg-mixed");
        let plan_mixed = plan_edition(
            &view_mixed,
            &BuildInfo::default(),
            &out_mixed,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        // All 5 register pages and ids.html are built, and sidebar/header counts and row contents
        // are identical between base and mixed because both read strictly from LabelledView.
        for req_page in [
            "claims.html",
            "sources.html",
            "questions.html",
            "conflicts.html",
            "glossary.html",
            "ids.html",
        ] {
            assert!(
                plan_base.site.files().contains_key(req_page),
                "`{req_page}` must be built in edition site"
            );
        }
        for (page, permitted_rows) in [
            ("claims.html", "5 rows"),
            ("sources.html", "4 rows"),
            ("questions.html", "2 rows"),
            ("conflicts.html", "1 row"),
            ("glossary.html", "4 rows"),
        ] {
            let html = String::from_utf8_lossy(&plan_mixed.site.files()[page]).into_owned();
            assert!(
                html.contains(&format!(
                    "<span class=\"chip mono\">{permitted_rows}</span>"
                )),
                "`{page}` must show its permitted count `{permitted_rows}`, never the unrestricted model count"
            );
        }
        for (rel, bytes) in plan_mixed.site.files() {
            let s = String::from_utf8_lossy(bytes);
            for canary in [
                "S-0099",
                "CL-CMP-0099",
                "XC-99",
                "forbidden_term_canary",
                "Forbidden source canary",
                "Forbidden claim canary statement.",
                "Forbidden conflict canary topic",
                "Forbidden term canary",
            ] {
                assert!(
                    !s.contains(canary),
                    "forbidden canary `{canary}` must not appear in {rel}"
                );
            }
        }
        assert_eq!(
            plan_mixed.site.files(),
            plan_cells.site.files(),
            "forbidden register rows and pages must not change any site byte, edition.toml and its fingerprint included"
        );
        // The cells base differs from the plain base only by the kept-row cells that name
        // left-out records; every page but the manifest must match the plain base, as on main.
        support::assert_plain_site_equals_cells_site(
            plan_base.site.files(),
            plan_cells.site.files(),
        );
        assert_eq!(
            plan_mixed.originals, plan_cells.originals,
            "forbidden register rows must not change the originals"
        );

        // The real build handler selects and views the records itself. Its delivered register
        // pages carry the permitted counts and equal the planned bytes, so a view bypass inside
        // the handler (counting the unrestricted model) changes them and fails here. The canary
        // tree stays at plan level because the loader refuses it (FB0410) before the handler runs.
        let handler_out = tmp.path().join("out-reg-handler");
        let outcome = run_with_info(
            &BuildArgs {
                out: handler_out.clone(),
            },
            &world,
            &BuildInfo::default(),
        );
        assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
        for (page, permitted_rows) in [
            ("claims.html", "5 rows"),
            ("sources.html", "4 rows"),
            ("questions.html", "2 rows"),
            ("conflicts.html", "1 row"),
            ("glossary.html", "4 rows"),
        ] {
            let delivered = std::fs::read(handler_out.join("edition").join(page))
                .unwrap_or_else(|e| panic!("handler must deliver `{page}`: {e}"));
            assert!(
                String::from_utf8_lossy(&delivered).contains(&format!(
                    "<span class=\"chip mono\">{permitted_rows}</span>"
                )),
                "the delivered `{page}` must show `{permitted_rows}`"
            );
            assert_eq!(
                &delivered,
                &plan_base.site.files()[page],
                "the delivered `{page}` equals the planned page"
            );
        }
        let delivered_ids = std::fs::read(handler_out.join("edition/ids.html"))
            .unwrap_or_else(|e| panic!("handler must deliver ids.html: {e}"));
        assert_eq!(
            &delivered_ids,
            &plan_base.site.files()["ids.html"],
            "the delivered ids.html equals the planned page"
        );

        // Every claims part page is delivered, and equals the planned page.
        let part_pages: Vec<&String> = plan_base
            .site
            .files()
            .keys()
            .filter(|k| k.starts_with("claims/"))
            .collect();
        assert!(
            part_pages.iter().any(|k| k.as_str() == "claims/CMP-1.html"),
            "claims part pages must be built, got {part_pages:?}"
        );
        for page in part_pages {
            let delivered = std::fs::read(handler_out.join("edition").join(page))
                .unwrap_or_else(|e| panic!("handler must deliver `{page}`: {e}"));
            assert_eq!(
                &delivered,
                &plan_base.site.files()[page.as_str()],
                "the delivered `{page}` equals the planned page"
            );
        }

        // Verify FB0603 checks every row anchor linked from pages (e.g. claims/CMP-1.html#CL-CMP-0001).
        let names = source::original_names(&view_base);
        let diags = lint::lint(&plan_base.site, &names);
        assert!(diags.is_empty(), "edition site must pass FB0603: {diags:?}");
        let mut broken_site = plan_base.site.clone();
        let broken_claims =
            String::from_utf8_lossy(broken_site.files().get("claims/CMP-1.html").unwrap())
                .replace("id=\"CL-CMP-0001\"", "id=\"CL-CMP-MISSING\"");
        let mut rebuilt = Site::new();
        for (k, v) in broken_site.files() {
            if k == "claims/CMP-1.html" {
                rebuilt.insert(k, broken_claims.as_bytes().to_vec());
            } else {
                rebuilt.insert(k, v.clone());
            }
        }
        broken_site = rebuilt;
        let broken_diags = lint::lint(&broken_site, &names);
        assert!(
            broken_diags
                .iter()
                .any(|d| d.code == Code::Fb0603 && d.message.contains("CL-CMP-0001")),
            "removing row anchor id=\"CL-CMP-0001\" from claims/CMP-1.html must trigger FB0603 from pages linking to it, got: {broken_diags:?}"
        );
    }

    const PDF_BYTES: &[u8] = b"%PDF-1.4\nsynthetic original, made up for tests\n";
    const HTML_BYTES: &[u8] =
        b"<html><body>synthetic html original, made up for tests</body></html>\n";

    /// Appends one source-version row to the copy's sources register and stores `file` (its
    /// name and bytes) under `corpus/sources/public/`; with no file the row is metadata-only.
    fn add_version(
        root: &Path,
        id: &str,
        version: u32,
        file: Option<(&str, &[u8])>,
        lifecycle: &str,
        note: &str,
    ) {
        let path = root.join("corpus/registers/sources.csv");
        let csv = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
        let header = csv
            .lines()
            .next()
            .unwrap_or_else(|| panic!("header"))
            .to_owned();
        let (location, sha256) = match file {
            Some((name, bytes)) => {
                std::fs::write(root.join("corpus/sources/public").join(name), bytes)
                    .unwrap_or_else(|e| panic!("{e}"));
                (
                    name.to_owned(),
                    fieldbook_core::audit::Sha256::digest(bytes)
                        .as_str()
                        .to_owned(),
                )
            }
            None => (
                "metadata-only: made up for tests".to_owned(),
                "7".repeat(64),
            ),
        };
        let title = format!("Synthetic Source {id} v{version}, made up for tests");
        let source_id = format!("S-{id}");
        let version = version.to_string();
        let row = support::register_row(
            &header,
            &[
                ("source_id", &source_id),
                ("version", &version),
                ("tag", &format!("synthetic-{id}")),
                ("title", &title),
                ("publisher", "Synthetic Board"),
                ("source_kind", "primary_law"),
                ("source_role", "primary_evidence"),
                ("authority_tier", "A"),
                ("licence", "Synthetic, no restrictions"),
                ("location", &location),
                ("sha256", &sha256),
                ("as_of", "2026-02-01"),
                ("retrieved", "2026-02-01"),
                ("lifecycle", lifecycle),
                ("volatility", "V2"),
                ("handling", "public"),
                ("note", note),
                ("purpose", "Synthetic purpose for tests"),
            ],
        );
        std::fs::write(&path, support::splice_register_row(&csv, &row, None))
            .unwrap_or_else(|e| panic!("{e}"));
    }

    fn stdout_of(output: &Output) -> String {
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn stderr_of(output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    #[test]
    fn originals_are_copied_byte_for_byte_under_shipped_names() {
        let tmp = temp_copy(&fixture_root());
        add_version(
            tmp.path(),
            "0004",
            1,
            Some(("S-0004-v1.html", HTML_BYTES)),
            "active",
            "",
        );
        add_version(
            tmp.path(),
            "0005",
            1,
            Some(("S-0005-v1.pdf", PDF_BYTES)),
            "active",
            "",
        );
        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            stderr_of(&output)
        );

        let delivered = collect_tree_bytes(&tmp.path().join("out/originals"));
        let stored = collect_tree_bytes(&tmp.path().join("corpus/sources/public"));
        let expected: BTreeMap<String, Vec<u8>> = [
            ("S-0001-v1.txt", stored["S-0001-v1.txt"].clone()),
            ("S-0002-v2.txt", stored["S-0002-v2.txt"].clone()),
            ("S-0004-v1.html.txt", HTML_BYTES.to_vec()),
            ("S-0005-v1.pdf", PDF_BYTES.to_vec()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        assert_eq!(
            delivered, expected,
            "every stored original ships once, byte for byte, a .html one as .html.txt"
        );
        assert!(
            !tmp.path().join("out/originals/S-0004-v1.html").exists(),
            "a renderable original never ships under its own name"
        );
        let page = std::fs::read_to_string(tmp.path().join("out/edition/s/S-0004-v1.html"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(page.contains("href=\"../../originals/S-0004-v1.html.txt\""));
    }

    #[test]
    fn a_marked_text_original_does_not_ship() {
        let marked: &[u8] =
            b"<html><body>CONTROLLED UNCLASSIFIED INFORMATION\nSECRET//NOFORN\nsynthetic body</body></html>\n";
        let tmp = temp_copy(&fixture_root());
        add_version(
            tmp.path(),
            "0004",
            1,
            Some(("S-0004-v1.html", marked)),
            "active",
            "",
        );
        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_ne!(
            output.status.code(),
            Some(0),
            "a marking banner in an original must stop the edition:\n{}",
            stderr_of(&output)
        );
        assert!(
            !tmp.path().join("out/originals/S-0004-v1.html.txt").exists(),
            "the marked original must not be written"
        );
        assert!(!tmp.path().join("out/edition").exists());
    }

    /// The repository Corpus records the address an original came from in `location` (the
    /// source-adding steps of corpus-format.md), while the stored file is named for the version.
    /// The labelled view keeps an original by its version id, so such a row still ships one.
    #[test]
    fn an_original_ships_when_location_is_the_address_it_came_from() {
        let tmp = temp_copy(&fixture_root());
        add_version(
            tmp.path(),
            "0005",
            1,
            Some(("S-0005-v1.pdf", PDF_BYTES)),
            "active",
            "",
        );
        let sources = tmp.path().join("corpus/registers/sources.csv");
        let csv = std::fs::read_to_string(&sources).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(
            &sources,
            csv.replace(",S-0005-v1.pdf,", ",https://example.invalid/guide.pdf,"),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            stderr_of(&output)
        );
        let delivered = collect_tree_bytes(&tmp.path().join("out/originals"));
        assert_eq!(
            delivered.get("S-0005-v1.pdf").map(Vec::as_slice),
            Some(PDF_BYTES),
            "the stored original ships whatever the location cell says"
        );
        let page = std::fs::read_to_string(tmp.path().join("out/edition/s/S-0005-v1.html"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(page.contains("href=\"../../originals/S-0005-v1.pdf\""));
    }

    #[test]
    fn originals_are_in_the_write_set() {
        let tmp = temp_copy(&fixture_root());
        add_version(
            tmp.path(),
            "0004",
            1,
            Some(("S-0004-v1.html", HTML_BYTES)),
            "active",
            "",
        );
        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            stderr_of(&output)
        );

        let edition_files = collect_tree_bytes(&tmp.path().join("out/edition")).len() as u64;
        let original_files = collect_tree_bytes(&tmp.path().join("out/originals")).len() as u64;
        assert_eq!(original_files, 3);
        let events = read_journal_events(&tmp.path().join(".fieldbook"));
        let begins: Vec<&Event> = events
            .iter()
            .filter(|e| e.kind() == Kind::WriteBegin)
            .collect();
        assert_eq!(begins.len(), 1, "one write.begin: {events:?}");
        match &begins[0].payload {
            EventPayload::WriteBegin { files, .. } => assert_eq!(
                files.value(),
                edition_files + original_files + 1,
                "write.begin counts the files of edition/ and originals/ together with the marker"
            ),
            other => panic!("expected WriteBegin, got {other:?}"),
        }
    }

    #[test]
    fn a_held_version_prints_held_and_ships_nothing() {
        let tmp = temp_copy(&fixture_root());
        add_version(
            tmp.path(),
            "0004",
            1,
            Some(("S-0004-v1.html", HTML_BYTES)),
            "active",
            "",
        );
        add_version(
            tmp.path(),
            "0005",
            1,
            Some(("S-0005-v1.pdf", PDF_BYTES)),
            "quarantined",
            "Held for test.",
        );
        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            stderr_of(&output)
        );
        assert_eq!(stdout_of(&output), "held S-0005-v1\n");

        assert!(!tmp.path().join("out/edition/s/S-0005-v1.html").exists());
        assert!(tmp.path().join("out/edition/s/S-0004-v1.html").is_file());
        let originals = collect_tree_bytes(&tmp.path().join("out/originals"));
        assert!(
            originals.keys().all(|k| !k.starts_with("S-0005-v1")),
            "a held original must not be copied: {:?}",
            originals.keys().collect::<Vec<_>>()
        );
        assert!(originals.contains_key("S-0004-v1.html.txt"));
        for (rel, bytes) in collect_tree_bytes(&tmp.path().join("out/edition")) {
            assert!(
                !String::from_utf8_lossy(&bytes).contains("S-0005-v1.html"),
                "{rel} must not link the held version"
            );
        }
    }

    #[test]
    fn a_foreign_originals_folder_is_not_deleted() {
        let tmp = temp_copy(&fixture_root());
        let foreign_out = tmp.path().join("foreign-out");
        let keep_file = foreign_out.join("originals/keep.txt");
        std::fs::create_dir_all(
            keep_file
                .parent()
                .unwrap_or_else(|| panic!("keep.txt parent")),
        )
        .unwrap_or_else(|e| panic!("create foreign originals dir: {e}"));
        std::fs::write(&keep_file, b"keep me\n").unwrap_or_else(|e| panic!("write keep.txt: {e}"));

        let foreign_str = foreign_out.to_string_lossy().into_owned();
        let output = run_bin(tmp.path(), &[], &["build", "--out", &foreign_str]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "foreign root must exit 2, stderr:\n{}",
            stderr_of(&output)
        );
        assert!(
            stderr_of(&output).contains(
                "it was not made by fieldbook build; remove it yourself or choose another --out"
            ),
            "unexpected stderr: {}",
            stderr_of(&output)
        );
        assert_eq!(
            std::fs::read(&keep_file).unwrap_or_else(|e| panic!("read keep.txt: {e}")),
            b"keep me\n",
            "keep.txt in the foreign originals folder must remain untouched"
        );
        assert!(
            !foreign_out.join("edition").exists(),
            "nothing may be written beside a foreign originals folder"
        );
        assert!(
            read_journal_events(&tmp.path().join(".fieldbook")).is_empty(),
            "foreign root refusal must append no journal event"
        );
    }

    /// Adds the claim `CL-CMP-0050`, on the PDF source `S-0004-v1` at `pp. 20-21`, and a page
    /// whose only citation is that claim.
    fn add_claim_only_page(root: &Path) {
        let claims_path = root.join("corpus/registers/claims.csv");
        let csv = std::fs::read_to_string(&claims_path).unwrap_or_else(|e| panic!("{e}"));
        let header = csv
            .lines()
            .next()
            .unwrap_or_else(|| panic!("header"))
            .to_owned();
        let row = support::register_row(
            &header,
            &[
                ("claim_id", "CL-CMP-0050"),
                ("area", "cmp"),
                ("kind", "fact"),
                (
                    "statement",
                    "Synthetic claim routing statement, made up for tests.",
                ),
                ("as_of", "2026-02-01"),
                ("source_id", "S-0004"),
                ("version", "1"),
                ("locator_kind", "pdf_page_box"),
                ("locator", "pp. 20-21"),
                ("quote", "Synthetic quote."),
                ("consequential", "false"),
                ("drafted_by", "codex"),
                ("verification", "unchecked"),
                ("applicability", "general_reference"),
                ("adoption", "proposed"),
                ("data_origin", "observed"),
                ("volatility", "V2"),
                ("note", "Synthetic claim for the routing test."),
            ],
        );
        std::fs::write(
            &claims_path,
            support::splice_register_row(&csv, &row, Some("CL-EST-0001")),
        )
        .unwrap_or_else(|e| panic!("{e}"));

        let steps = root.join("corpus/pages/compliance/business-systems/cmp-review-steps.md");
        let text = std::fs::read_to_string(&steps).unwrap_or_else(|e| panic!("{e}"));
        let page = text
            .replace("id = \"cmp-review-steps\"", "id = \"cmp-claim-routing\"")
            .replace(
                "title = \"Synthetic procedure for running a business-system review\"",
                "title = \"Synthetic claim routing page\"",
            )
            .replace("[@CL-CMP-0001]", "[@CL-CMP-0050]");
        std::fs::write(
            root.join("corpus/pages/compliance/business-systems/cmp-claim-routing.md"),
            page,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn pdf_citation_links_to_shipped_original_page() {
        let tmp = temp_copy(&fixture_root());
        add_version(
            tmp.path(),
            "0004",
            1,
            Some(("S-0004-v1.pdf", PDF_BYTES)),
            "active",
            "",
        );
        add_version(
            tmp.path(),
            "0005",
            1,
            None,
            "quarantined",
            "Held <for> review & test.",
        );
        let steps_path = tmp
            .path()
            .join("corpus/pages/compliance/business-systems/cmp-review-steps.md");
        let steps = std::fs::read_to_string(&steps_path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(
            &steps_path,
            steps.replace(
                "## How it works\n",
                "## How it works\n\nThe PDF says so [@S-0004-v1, p. 14] and over a range [@S-0004-v1, pp. 30-31]. The metadata-only source says so [@S-0001-v1, sec. 1]. The held source says so [@S-0005-v1, sec. 1].\n\n",
            ),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        add_claim_only_page(tmp.path());

        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            stderr_of(&output)
        );

        let delivered =
            std::fs::read_to_string(tmp.path().join("out/edition/p/cmp-review-steps.html"))
                .unwrap_or_else(|e| panic!("{e}"));
        // A single page: the delivered relative href, with the locator text beside the link.
        assert!(
            delivered.contains("href=\"../../originals/S-0004-v1.pdf#page=14\""),
            "single-page citation must link to the shipped PDF at its page:\n{delivered}"
        );
        assert!(delivered.contains("<span class=\"loc\">p. 14</span>"));
        // A range: the first page, never the last.
        assert!(
            delivered.contains("href=\"../../originals/S-0004-v1.pdf#page=30\""),
            "a pp. citation must link to the first page of the range:\n{delivered}"
        );
        assert!(delivered.contains("<span class=\"loc\">pp. 30-31</span>"));
        assert!(!delivered.contains("#page=31"));
        // A metadata-only version keeps its source page and its lock.
        assert!(
            delivered.contains("href=\"../s/S-0001-v1.html\""),
            "a metadata-only citation keeps its s/ href:\n{delivered}"
        );
        assert!(!delivered.contains("originals/S-0001"));
        // A held version is plain text with its escaped reason, and has no href of either kind.
        assert!(
            delivered.contains("Held: Held &lt;for&gt; review &amp; test."),
            "a held citation shows its escaped reason:\n{delivered}"
        );
        assert!(!delivered.contains("S-0005-v1.html") && !delivered.contains("originals/S-0005"));
        assert!(!delivered.contains("sources.html#S-0005"));

        // A page whose only citation is a claim: the claim's own source version and locator
        // route its Sources row.
        let claim_only =
            std::fs::read_to_string(tmp.path().join("out/edition/p/cmp-claim-routing.html"))
                .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !claim_only.contains("[@S-"),
            "the claim-only page cites no source directly"
        );
        assert!(
            claim_only.contains("href=\"../../originals/S-0004-v1.pdf#page=20\""),
            "the claim's own locator must route its Sources row:\n{claim_only}"
        );
        assert!(claim_only.contains("<span class=\"loc\">pp. 20-21</span>"));
        assert!(!claim_only.contains("#page=21"));
    }

    /// A [`World`] whose Corpus tree also holds an overlay label ledger, as a tree with a company
    /// store does. The ledger keeps an item out of the corpus-only edition view while the row
    /// itself stays public, so `check` has nothing to refuse and the build runs its real path.
    struct LedgerWorld {
        inner: TestWorld,
        ledger: &'static str,
    }

    impl LedgerWorld {
        fn with_ledger(
            &self,
            mut tree: fieldbook_core::check::Tree,
        ) -> fieldbook_core::check::Tree {
            tree.insert_bytes("overlay/labels.csv", self.ledger.as_bytes());
            tree
        }
    }

    impl World for LedgerWorld {
        fn read_tree(&self) -> Result<fieldbook_core::check::Tree, fieldbook::WorldError> {
            self.inner.read_tree().map(|tree| self.with_ledger(tree))
        }

        fn write_lease(&self) -> Result<fieldbook::StoreLease, fieldbook::WorldError> {
            self.inner.write_lease()
        }

        fn read_tree_under(
            &self,
            lease: &fieldbook::StoreLease,
        ) -> Result<fieldbook_core::check::Tree, fieldbook::WorldError> {
            self.inner
                .read_tree_under(lease)
                .map(|tree| self.with_ledger(tree))
        }

        fn write_file(
            &self,
            audited: &fieldbook::Audited,
            path: &Path,
            bytes: &[u8],
        ) -> Result<fieldbook_core::audit::Sha256, fieldbook::WorldError> {
            self.inner.write_file(audited, path, bytes)
        }

        fn write_outside(
            &self,
            allowed: &fieldbook_core::egress::Allowed,
            audited: &fieldbook::Audited,
            path: &Path,
            bytes: &[u8],
        ) -> Result<fieldbook_core::audit::Sha256, fieldbook::WorldError> {
            self.inner.write_outside(allowed, audited, path, bytes)
        }

        fn has_company_store(&self) -> bool {
            self.inner.has_company_store()
        }

        fn today(&self) -> CivilDate {
            self.inner.today()
        }

        fn now(&self) -> fieldbook_core::audit::Now {
            self.inner.now()
        }

        fn principal(&self) -> fieldbook_core::audit::Principal {
            self.inner.principal()
        }

        fn cwd(&self) -> &Path {
            self.inner.cwd()
        }

        fn tree_root(&self) -> &Path {
            self.inner.tree_root()
        }

        fn state_dir(&self) -> &Path {
            self.inner.state_dir()
        }

        fn stdin_is_terminal(&self) -> bool {
            self.inner.stdin_is_terminal()
        }

        fn read_line(&self) -> String {
            self.inner.read_line()
        }

        fn prompt(&self, flag: &str) -> Result<(), fieldbook::WorldError> {
            self.inner.prompt(flag)
        }

        fn place(&self, path: &Path) -> fieldbook_core::labels::PlaceFacts {
            self.inner.place(path)
        }

        fn support_facts(&self) -> fieldbook::SupportFacts {
            self.inner.support_facts()
        }

        fn reset_drill(&self, audited: &fieldbook::Audited) -> Result<(), fieldbook::WorldError> {
            self.inner.reset_drill(audited)
        }

        fn remove_drill(&self, audited: &fieldbook::Audited) -> Result<(), fieldbook::WorldError> {
            self.inner.remove_drill(audited)
        }
    }

    /// The real `build` path: a valid preflight (the row of `S-0098-v1` is public and active, and
    /// its stored original matches its hash), and the version kept out of the edition view by the
    /// overlay ledger, which labels it `np-0007` in a store the edition does not read. Nothing
    /// but the permitted view stands between that version and the outputs, and it is not held, so
    /// the hold rule cannot be what keeps it out.
    fn assert_the_build_path_ships_only_the_permitted_view() {
        let permitted_html: &[u8] = b"<p>Permitted original canary, made up for tests.</p>\n";
        let forbidden_html: &[u8] = b"<p>Forbidden original canary, made up for tests.</p>\n";
        let tmp = temp_copy(&fixture_root());
        add_version(
            tmp.path(),
            "0004",
            1,
            Some(("S-0004-v1.html", permitted_html)),
            "active",
            "Permitted source page canary.",
        );
        add_version(
            tmp.path(),
            "0098",
            1,
            Some(("S-0098-v1.html", forbidden_html)),
            "active",
            "Forbidden source page canary.",
        );
        // A second stored file of a permitted version, with the same bytes: `check` accepts it, and
        // the view keeps only the file the row's `location` names.
        std::fs::write(
            tmp.path().join("corpus/sources/public/S-0004-v1.bin"),
            permitted_html,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        // The regression set refuses to index a tree that holds a ledger (FB0126), the guard in
        // front of this view on a repository Corpus; a Corpus with no set has no such pass.
        std::fs::remove_file(tmp.path().join("corpus/eval/regression.toml"))
            .unwrap_or_else(|e| panic!("{e}"));
        let world = LedgerWorld {
            inner: TestWorld::new(tmp.path()),
            ledger: "seq,item_id,kind,labels,origin\n1,S-0098-v1,add,np-0007,person\n",
        };

        // The preflight is valid, on the tree the build itself reads.
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok(), "the preflight must pass: {report:?}");
        assert!(
            model
                .sources()
                .iter()
                .any(|s| s.value.source_id.as_str() == "S-0098"),
            "the model holds the row the view must leave out"
        );

        let out_dir = tmp.path().join("out-build-path");
        let outcome = run_with_info(
            &BuildArgs {
                out: out_dir.clone(),
            },
            &world,
            &BuildInfo::default(),
        );
        assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
        assert!(
            !outcome.stdout.contains("S-0098"),
            "the version is not held, so no hold line names it: {}",
            outcome.stdout
        );

        let edition = out_dir.join("edition");
        let originals = out_dir.join("originals");

        // Permitted presence stays required.
        let permitted_page = std::fs::read_to_string(edition.join("s/S-0004-v1.html"))
            .unwrap_or_else(|e| panic!("the permitted source page must be written: {e}"));
        assert!(permitted_page.contains("Permitted source page canary"));
        assert!(permitted_page.contains("href=\"../../originals/S-0004-v1.html.txt\""));
        assert_eq!(
            std::fs::read(originals.join("S-0004-v1.html.txt"))
                .unwrap_or_else(|e| panic!("the permitted original must be written: {e}")),
            permitted_html
        );

        // Forbidden: bypassing source-page filtering writes the page, and bypassing original
        // selection copies the file.
        assert!(
            !edition.join("s/S-0098-v1.html").exists(),
            "a version outside the permitted view must have no source page"
        );
        assert!(
            !originals.join("S-0098-v1.html.txt").exists()
                && !originals.join("S-0098-v1.html").exists(),
            "a version outside the permitted view must have no shipped original"
        );
        assert_eq!(
            collect_tree_bytes(&originals)
                .into_keys()
                .collect::<Vec<_>>(),
            ["S-0001-v1.txt", "S-0002-v2.txt", "S-0004-v1.html.txt"],
            "originals/ holds exactly the originals the permitted view selects"
        );
        assert_no_forbidden_text(
            &edition,
            &originals,
            &[
                "Forbidden source page canary",
                "Forbidden original canary",
                "S-0098",
            ],
        );
    }

    fn assert_no_forbidden_text(edition: &Path, originals: &Path, needles: &[&str]) {
        for (folder, root) in [("edition", edition), ("originals", originals)] {
            for (rel, bytes) in collect_tree_bytes(root) {
                assert!(
                    !rel.contains("S-0098"),
                    "a forbidden file must not be shipped: {folder}/{rel}"
                );
                let text = String::from_utf8_lossy(&bytes);
                for needle in needles {
                    assert!(
                        !text.contains(needle),
                        "forbidden `{needle}` must not appear in {folder}/{rel}"
                    );
                }
            }
        }
    }

    /// The home and area counts a delivered edition must show for `corpus-mini`.
    fn assert_permitted_home_and_areas(edition: &Path) {
        let read = |rel: &str| {
            std::fs::read_to_string(edition.join(rel))
                .unwrap_or_else(|e| panic!("the handler must deliver {rel}: {e}"))
        };
        let index = read("index.html");
        for shown in [
            "5 pages",
            "4 pages",
            "1 of 2 supported",
            "0 of 3 supported",
            "Q-001",
            "XC-01",
            "href=\"a/cmp.html\"",
            "href=\"a/est.html\"",
            "p/est-rate-window-brief.html",
            "id=\"areas\"",
        ] {
            assert!(index.contains(shown), "index.html must show `{shown}`");
        }
        let cmp = read("a/cmp.html");
        assert!(cmp.contains(
            "aria-label=\"1 of 2 supported: 1 supported, 1 unchecked, 0 contradicted, 0 stale, 0 unresolved\""
        ));
        assert!(cmp.contains("<span class=\"count\">5</span>"), "cmp pages");
        assert!(cmp.contains("<tr id=\"cmp-system-review\""));
        assert!(cmp.contains("Q-001"));
        let est = read("a/est.html");
        assert!(est.contains(
            "aria-label=\"0 of 3 supported: 0 supported, 3 unchecked, 0 contradicted, 0 stale, 0 unresolved\""
        ));
        assert!(est.contains("<span class=\"count\">4</span>"), "est pages");
        assert!(est.contains("<tr id=\"est-rate-window-brief\""));
        assert!(
            est.contains("Nothing upstream has moved."),
            "no canary change"
        );
    }

    /// Records whose own handling is public but which name a record the ledger leaves out, so the
    /// view must leave them out through the dependency alone: one brief per page reference (a
    /// term, a question, a conflict, a cited claim and a linked page), a claim whose `conflict_id`
    /// is the left-out conflict, a claim superseded by the left-out claim, a term labelled
    /// `np-0007` by the ledger that cites only kept records, and three conflicts whose `evidence`
    /// names the left-out claim or the left-out source version `S-0098-v1`, or whose
    /// `question_id` is the left-out question. Every id starts `est-dep-`, `CL-CMP-009`,
    /// `XC-9`, `S-0098` or `forbidden_`, and every text starts `Forbidden` or `Dependent`.
    fn plant_public_records_that_name_left_out_ones(root: &Path) {
        let read = |rel: &str| {
            std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
        };
        let write = |rel: &str, text: String| {
            std::fs::write(root.join(rel), text).unwrap_or_else(|e| panic!("{rel}: {e}"));
        };
        let claims = read("corpus/registers/claims.csv");
        let header: Vec<&str> = claims
            .lines()
            .next()
            .unwrap_or_else(|| panic!("claims header"))
            .split(',')
            .collect();
        // CL-CMP-0002 has no quoted cell, so a plain split keeps its columns.
        let template: Vec<&str> = claims
            .lines()
            .find(|l| l.starts_with("CL-CMP-0002,"))
            .unwrap_or_else(|| panic!("fixture claim"))
            .split(',')
            .collect();
        assert_eq!(template.len(), header.len());
        let claim_row = |cells: &[(&str, &str)]| {
            header
                .iter()
                .zip(&template)
                .map(|(col, cell)| {
                    cells
                        .iter()
                        .find(|(name, _)| name == col)
                        .map_or(*cell, |(_, v)| *v)
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let superseded = claim_row(&[
            ("claim_id", "CL-CMP-0096"),
            ("statement", "Dependent superseded claim canary statement."),
            ("applicability", "not_applicable"),
            (
                "adoption",
                fieldbook_core::schema::Adoption::Superseded.as_str(),
            ),
            ("superseded_by", "CL-CMP-0098"),
            ("question_id", ""),
        ]);
        let in_conflict = claim_row(&[
            ("claim_id", "CL-CMP-0097"),
            ("statement", "Dependent conflict claim canary statement."),
            ("conflict_id", "XC-98"),
            ("question_id", ""),
        ]);
        let left_out_claim = claims
            .lines()
            .find(|l| l.starts_with("CL-CMP-0098,"))
            .unwrap_or_else(|| panic!("the left-out claim is planted first"));
        let supersedes = header
            .iter()
            .position(|c| *c == "supersedes")
            .unwrap_or_else(|| panic!("supersedes column"));
        let mut cells: Vec<&str> = left_out_claim.split(',').collect();
        cells[supersedes] = "CL-CMP-0096";
        let claims = claims.replace(
            &format!("\n{left_out_claim}\n"),
            &format!("\n{superseded}\n{in_conflict}\n{}\n", cells.join(",")),
        );
        assert!(
            claims.contains("\nCL-CMP-0096,"),
            "the dependent claims are planted"
        );
        write("corpus/registers/claims.csv", claims);
        // The conflict lists the claim that names it, as the format requires.
        let conflicts = read("corpus/registers/conflicts.csv");
        let left_out_conflict = conflicts
            .lines()
            .find(|l| l.starts_with("XC-98,"))
            .unwrap_or_else(|| panic!("the left-out conflict is planted first"));
        write(
            "corpus/registers/conflicts.csv",
            conflicts.replace(
                left_out_conflict,
                &left_out_conflict.replace(
                    ",CL-CMP-0001|CL-CMP-0002,",
                    ",CL-CMP-0001|CL-CMP-0002|CL-CMP-0097,",
                ),
            ),
        );
        // A source version the ledger leaves out (`S-0098-v1`), so a conflict can name it as
        // evidence. It is metadata-only, so it ships no original.
        let sources = read("corpus/registers/sources.csv");
        let sources_header = sources
            .lines()
            .next()
            .unwrap_or_else(|| panic!("sources header"));
        let left_out_version = support::register_row(
            sources_header,
            &[
                ("source_id", "S-0098"),
                ("version", "1"),
                ("tag", "forbidden-dep-source"),
                (
                    "title",
                    "Forbidden dependent source canary, made up for tests",
                ),
                ("publisher", "Synthetic Standards Board"),
                ("source_kind", "trade_press"),
                ("source_role", "accepted_interpretation"),
                ("authority_tier", "D"),
                ("licence", "Synthetic, no restrictions"),
                ("location", "metadata-only: made up for tests"),
                ("sha256", &"8".repeat(64)),
                ("published", "2026-01-25"),
                ("as_of", "2026-02-01"),
                ("retrieved", "2026-02-01"),
                ("lifecycle", "active"),
                ("volatility", "V2"),
                ("handling", "public"),
                ("note", "Forbidden dependent source note."),
                ("purpose", "Forbidden dependent source purpose"),
            ],
        );
        write(
            "corpus/registers/sources.csv",
            support::splice_register_row(&sources, &left_out_version, None),
        );
        // Public conflicts that name a left-out record outside `claim_ids`: one resolved on the
        // left-out claim, one resolved on the left-out source version, and one whose open question
        // is the left-out question. Each sorts before the left-out conflict `XC-98`.
        let conflicts = read("corpus/registers/conflicts.csv");
        let dependent_conflicts = [
            "XC-95,est,Dependent evidence claim conflict canary,Dependent first claim position|Dependent second claim position,Dependent claim resolution.,Dependent claim rationale.,CL-CMP-0098,2026-02-01,resolved,CL-CMP-0001|CL-CMP-0002,",
            "XC-96,est,Dependent evidence version conflict canary,Dependent first version position|Dependent second version position,Dependent version resolution.,Dependent version rationale.,S-0098-v1,2026-02-01,resolved,CL-CMP-0001|CL-CMP-0002,",
            "XC-97,est,Dependent question conflict canary,Dependent first question position|Dependent second question position,,,,,unresolved,CL-CMP-0001|CL-CMP-0002,Q-098",
        ]
        .join("\n");
        assert!(
            conflicts.contains("\nXC-98,"),
            "the left-out conflict is planted first"
        );
        write(
            "corpus/registers/conflicts.csv",
            support::splice_register_row(&conflicts, &dependent_conflicts, Some("XC-98,")),
        );
        let glossary = read("corpus/registers/glossary.csv");
        write(
            "corpus/registers/glossary.csv",
            glossary.replace(
                "\nrate_window,",
                "\nforbidden_window,forbidden window,Forbidden glossary canary definition.,Forbidden glossary canary boundary.,est,CL-EST-0003,public_reference,,\nrate_window,",
            ),
        );
        let brief = read("corpus/pages/estimating/pricing/est-rate-window-brief.md");
        let page = |slug: &str, from: &str, to: &str| {
            let text = brief
                .replace(
                    "id = \"est-rate-window-brief\"",
                    &format!("id = \"est-dep-{slug}\""),
                )
                .replace(
                    "title = \"Synthetic decision brief on forward pricing rate review window\"",
                    &format!("title = \"Dependent {slug} canary\""),
                );
            assert!(text.contains(from), "the brief holds `{from}`");
            write(
                &format!("corpus/pages/estimating/pricing/est-dep-{slug}.md"),
                text.replace(from, to),
            );
        };
        page(
            "term",
            "terms = [\"rate_window\", \"cost_baseline\"]",
            "terms = [\"rate_window\", \"cost_baseline\", \"forbidden_window\"]",
        );
        page(
            "question",
            "questions = [\"Q-002\"]",
            "questions = [\"Q-002\", \"Q-098\"]",
        );
        page(
            "conflict",
            "questions = [\"Q-002\"]",
            "questions = [\"Q-002\"]\nconflicts = [\"XC-98\"]",
        );
        page(
            "cite",
            "3 percent per year [@CL-EST-0001].",
            "3 percent per year [@CL-EST-0001] [@CL-CMP-0098].",
        );
        page(
            "link",
            "links = [\"cmp-system-review\"]",
            "links = [\"cmp-system-review\", \"est-forbidden-brief\"]",
        );
    }

    /// Plants a disallowed claim, question, conflict, brief and term in `root`, and the public
    /// records that name them, runs the real build handler, and returns the delivered edition
    /// folder.
    fn deliver_with_disallowed_records(root: &Path) -> PathBuf {
        let read = |rel: &str| {
            std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
        };
        let write = |rel: &str, text: String| {
            std::fs::write(root.join(rel), text).unwrap_or_else(|e| panic!("{rel}: {e}"));
        };
        // A disallowed record of each kind the home and area pages count: a claim, an
        // open conflict, a day-one question and a brief, in an area with permitted records.
        let claims = read("corpus/registers/claims.csv");
        let claim = claims
            .lines()
            .find(|l| l.starts_with("CL-CMP-0002,"))
            .unwrap_or_else(|| panic!("fixture claim"))
            .replace("CL-CMP-0002", "CL-CMP-0098")
            .replace(
                "Synthetic withholding threshold is 5 percent of billed amounts.",
                "Forbidden claim canary statement.",
            );
        // The register stays in normal form: rows in id order.
        let next_row = claims
            .find("CL-EST-0001,")
            .unwrap_or_else(|| panic!("fixture claim"));
        let (before, after) = claims.split_at(next_row);
        write(
            "corpus/registers/claims.csv",
            format!("{before}{claim}\n{after}"),
        );
        let questions = read("corpus/registers/questions.csv");
        write(
            "corpus/registers/questions.csv",
            format!("{questions}Q-098,est,Forbidden day-one canary question?,,,,,,true,open,,,\n"),
        );
        let conflicts = read("corpus/registers/conflicts.csv");
        let conflict = conflicts
            .lines()
            .nth(1)
            .unwrap_or_else(|| panic!("fixture conflict"))
            .replace("XC-01,cmp,", "XC-98,est,")
            .replace(
                "Synthetic review interval versus withholding check timing",
                "Forbidden conflict canary topic",
            );
        write(
            "corpus/registers/conflicts.csv",
            format!("{conflicts}{conflict}\n"),
        );
        let brief_path = "corpus/pages/estimating/pricing/est-rate-window-brief.md";
        write(
            "corpus/pages/estimating/pricing/est-forbidden-brief.md",
            read(brief_path)
                .replace(
                    "id = \"est-rate-window-brief\"",
                    "id = \"est-forbidden-brief\"",
                )
                .replace(
                    "title = \"Synthetic decision brief on forward pricing rate review window\"",
                    "title = \"Forbidden brief canary\"",
                ),
        );
        plant_public_records_that_name_left_out_ones(root);
        // The regression set refuses to index a tree that holds a ledger (FB0126).
        std::fs::remove_file(root.join("corpus/eval/regression.toml"))
            .unwrap_or_else(|e| panic!("{e}"));
        let world = LedgerWorld {
            inner: TestWorld::new(root),
            ledger: "seq,item_id,kind,labels,origin\n1,CL-CMP-0098,add,np-0007,person\n2,Q-098,add,np-0007,person\n3,XC-98,add,np-0007,person\n4,est-forbidden-brief,add,np-0007,person\n5,forbidden_window,add,np-0007,person\n6,S-0098-v1,add,np-0007,person\n",
        };

        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok(), "the preflight must pass: {report:?}");
        assert!(
            model
                .claims()
                .iter()
                .any(|c| c.value.claim_id.as_str() == "CL-CMP-0098")
                && model
                    .pages()
                    .iter()
                    .any(|p| p.id.as_str() == "est-forbidden-brief"),
            "the model holds the records the view must leave out"
        );
        for page in [
            "est-dep-term",
            "est-dep-question",
            "est-dep-conflict",
            "est-dep-cite",
            "est-dep-link",
        ] {
            assert!(
                model.pages().iter().any(|p| p.id.as_str() == page),
                "the model holds the dependent page `{page}`"
            );
        }
        for claim in ["CL-CMP-0096", "CL-CMP-0097"] {
            assert!(
                model
                    .claims()
                    .iter()
                    .any(|c| c.value.claim_id.as_str() == claim),
                "the model holds the dependent claim `{claim}`"
            );
        }
        for conflict in ["XC-95", "XC-96", "XC-97"] {
            assert!(
                model
                    .conflicts()
                    .iter()
                    .any(|x| x.value.conflict_id.as_str() == conflict),
                "the model holds the dependent conflict `{conflict}`"
            );
        }
        assert!(
            model
                .sources()
                .iter()
                .any(|s| s.value.source_id.as_str() == "S-0098"),
            "the model holds the left-out source version"
        );
        assert!(
            model
                .glossary()
                .iter()
                .any(|t| t.value.term_id.as_str() == "forbidden_window"),
            "the model holds the left-out term"
        );

        deliver(&world, &root.join("out-home-areas")).join("edition")
    }

    /// Runs the real build handler of `world` into `out_dir` and returns `out_dir`.
    fn deliver(world: &LedgerWorld, out_dir: &Path) -> PathBuf {
        let outcome = run_with_info(
            &BuildArgs {
                out: out_dir.to_path_buf(),
            },
            world,
            &BuildInfo::default(),
        );
        assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
        out_dir.to_path_buf()
    }

    /// Records the overlay ledger leaves out, a brief whose own handling is public among them,
    /// change no delivered byte: every edition file, `edition.toml` and its fingerprint included,
    /// and every original equal those of the same Corpus without the records.
    #[test]
    fn build_ledger_left_out_records_change_no_delivered_byte() {
        let tmp_base = temp_copy(&fixture_root());
        // The regression set refuses to index a tree that holds a ledger (FB0126).
        std::fs::remove_file(tmp_base.path().join("corpus/eval/regression.toml"))
            .unwrap_or_else(|e| panic!("{e}"));
        let base_world = LedgerWorld {
            inner: TestWorld::new(tmp_base.path()),
            ledger: "seq,item_id,kind,labels,origin\n",
        };
        let tree = base_world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (_model, report) = load(&Host::new(tree, base_world.today()), &Options::new(false));
        assert!(report.is_ok(), "the base preflight must pass: {report:?}");
        let base_out = deliver(&base_world, &tmp_base.path().join("out-ledger"));

        let tmp_mixed = temp_copy(&fixture_root());
        let mixed_edition = deliver_with_disallowed_records(tmp_mixed.path());
        assert!(
            tmp_mixed
                .path()
                .join("corpus/pages/estimating/pricing/est-forbidden-brief.md")
                .is_file(),
            "the left-out brief is in the Corpus the build reads"
        );
        let mixed_out = mixed_edition
            .parent()
            .unwrap_or_else(|| panic!("the edition folder has a parent"));

        let base_edition = collect_tree_bytes(&base_out.join("edition"));
        assert!(base_edition.contains_key("edition.toml"));
        assert_eq!(
            collect_tree_bytes(&mixed_edition),
            base_edition,
            "left-out records must change no edition byte, edition.toml and its fingerprint included"
        );
        assert_eq!(
            collect_tree_bytes(&mixed_out.join("originals")),
            collect_tree_bytes(&base_out.join("originals")),
            "left-out records must change no shipped original"
        );
    }

    #[test]
    fn build_about_counts_use_the_permitted_view() {
        let tmp = temp_copy(&fixture_root());
        let edition = deliver_with_disallowed_records(tmp.path());
        let about = std::fs::read_to_string(edition.join("about.html"))
            .unwrap_or_else(|e| panic!("the handler must deliver about.html: {e}"));
        for shown in [
            "<dt>Pages</dt><dd class=\"mono\">9</dd>",
            "<dt>Claims</dt><dd class=\"mono\">5</dd>",
            "<dt>Pages draft</dt><dd class=\"mono\">9</dd>",
            "<dt>Pages stable</dt><dd class=\"mono\">0</dd>",
            "<dt>Originals</dt><dd class=\"mono\">2</dd>",
            "<dt>Claims unchecked</dt><dd class=\"mono\">4</dd>",
            "This edition holds public only.",
            "silent build",
            "CL-CMP-0002",
        ] {
            assert!(about.contains(shown), "about.html must show `{shown}`");
        }
        let toml = std::fs::read_to_string(edition.join("edition.toml"))
            .unwrap_or_else(|e| panic!("the handler must deliver edition.toml: {e}"));
        let parsed = fieldbook_core::edition::EditionInfo::parse(&toml)
            .unwrap_or_else(|e| panic!("edition.toml must parse: {e}"));
        assert_eq!(parsed.counts.pages, 9);
        assert_eq!(parsed.counts.claims, 5);
        assert_eq!(parsed.counts.originals, 2);
        assert!(!parsed.ids.claims.iter().any(|id| id == "CL-CMP-0098"));
        assert!(
            !parsed
                .ids
                .pages
                .iter()
                .any(|id| id == "est-forbidden-brief")
        );
        for needle in [
            "CL-CMP-0098",
            "Forbidden claim canary statement.",
            "Q-098",
            "XC-98",
            "est-forbidden-brief",
        ] {
            assert!(!about.contains(needle), "about.html holds `{needle}`");
            assert!(!toml.contains(needle), "edition.toml holds `{needle}`");
        }
    }

    #[test]
    fn build_home_and_areas_counts_use_the_permitted_view() {
        let tmp = temp_copy(&fixture_root());
        let edition = deliver_with_disallowed_records(tmp.path());
        assert_permitted_home_and_areas(&edition);
        for (rel, bytes) in collect_tree_bytes(&edition) {
            let text = String::from_utf8_lossy(&bytes);
            for needle in [
                "CL-CMP-0098",
                "Forbidden claim canary statement.",
                "Q-098",
                "Forbidden day-one canary question?",
                "XC-98",
                "Forbidden conflict canary topic",
                "est-forbidden-brief",
                "Forbidden brief canary",
                "est-dep-",
                "Dependent ",
                "CL-CMP-0096",
                "CL-CMP-0097",
                "forbidden_window",
                "Forbidden glossary canary",
                "XC-95",
                "XC-96",
                "XC-97",
                "S-0098",
                "Forbidden dependent source",
            ] {
                assert!(
                    !text.contains(needle),
                    "forbidden `{needle}` must not appear in edition/{rel}"
                );
            }
        }
    }

    #[test]
    fn build_source_pages_and_originals_use_the_permitted_view() {
        assert_the_build_path_ships_only_the_permitted_view();

        // The same outcome at plan level, where the forbidden label is on the row itself (the
        // loader reports it, so the build never reaches this view) and the tree keeps the bytes.
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let mut tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));

        // The tree keeps the bytes of every original, as an in-memory tree does.
        let stored = std::fs::read(tmp.path().join("corpus/sources/public/S-0002-v2.txt"))
            .unwrap_or_else(|e| panic!("{e}"));
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", stored.clone());

        // A permitted source with an original, and a source with an original and a source row
        // whose handling carries the label `np-0007` that the edition selection leaves out.
        let permitted_html: &[u8] = b"<p>Permitted original canary, made up for tests.</p>\n";
        let forbidden_html: &[u8] = b"<p>Forbidden original canary, made up for tests.</p>\n";
        let sources = String::from_utf8(
            tree.get_bytes("corpus/registers/sources.csv")
                .unwrap_or_else(|| panic!("sources register"))
                .to_vec(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let header = sources
            .lines()
            .next()
            .unwrap_or_else(|| panic!("header"))
            .to_owned();
        let row = |id: &str, title: &str, bytes: &[u8], handling: &str| {
            let location = format!("S-{id}-v1.html");
            let sha256 = fieldbook_core::audit::Sha256::digest(bytes)
                .as_str()
                .to_owned();
            support::register_row(
                &header,
                &[
                    ("source_id", &format!("S-{id}")),
                    ("version", "1"),
                    ("tag", &format!("synthetic-{id}")),
                    ("title", title),
                    ("publisher", "Synthetic Board"),
                    ("source_kind", "primary_law"),
                    ("source_role", "primary_evidence"),
                    ("authority_tier", "A"),
                    ("licence", "Synthetic, no restrictions"),
                    ("location", &location),
                    ("sha256", &sha256),
                    ("as_of", "2026-02-01"),
                    ("retrieved", "2026-02-01"),
                    ("lifecycle", "active"),
                    ("volatility", "V2"),
                    ("handling", handling),
                    ("note", "Synthetic source."),
                    ("purpose", "Synthetic purpose for tests"),
                ],
            )
        };
        let mixed_sources = format!(
            "{}\n{}\n{}\n",
            sources.trim_end_matches('\n'),
            row(
                "0004",
                "Permitted source page canary",
                permitted_html,
                "public"
            ),
            row(
                "0098",
                "Forbidden source page canary",
                forbidden_html,
                "public|np-0007"
            ),
        );
        tree.insert_bytes("corpus/registers/sources.csv", mixed_sources.into_bytes());
        tree.insert_bytes("corpus/sources/public/S-0004-v1.html", permitted_html);
        tree.insert_bytes("corpus/sources/public/S-0098-v1.html", forbidden_html);
        // A stored file no source row names has no page and ships nothing.
        let orphan_html: &[u8] = b"<p>Orphan original canary, made up for tests.</p>\n";
        tree.insert_bytes("corpus/sources/public/S-0097-v1.html", orphan_html);

        // The loader reports the handling error of the forbidden row (FB0410) but still builds
        // the model, which is what the permitted view must filter.
        let (model, _report) = load(&Host::new(tree, world.today()), &Options::new(false));
        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());
        assert!(view.left_out() >= 1);

        let out_dir = tmp.path().join("out-permitted");
        let plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &out_dir,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("plan_edition: {e:?}"))
        .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

        let mut journal =
            state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
        let audited = journal
            .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
            .unwrap_or_else(|e| panic!("{e}"));
        write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
        journal
            .end(audited, Word::new_static("ok"))
            .unwrap_or_else(|e| panic!("{e}"));

        let edition = out_dir.join("edition");
        let originals = out_dir.join("originals");

        // Permitted: the source page and its text, and the original byte for byte.
        let permitted_page = std::fs::read_to_string(edition.join("s/S-0004-v1.html"))
            .unwrap_or_else(|e| panic!("the permitted source page must be written: {e}"));
        assert!(permitted_page.contains("Permitted source page canary"));
        assert!(permitted_page.contains("href=\"../../originals/S-0004-v1.html.txt\""));
        assert_eq!(
            std::fs::read(originals.join("S-0004-v1.html.txt"))
                .unwrap_or_else(|e| panic!("the permitted original must be written: {e}")),
            permitted_html
        );
        assert_eq!(
            std::fs::read(originals.join("S-0002-v2.txt")).unwrap_or_else(|e| panic!("{e}")),
            stored
        );

        // Forbidden: no page, no original, no id or text in either folder.
        assert!(!edition.join("s/S-0098-v1.html").exists());
        assert!(!originals.join("S-0098-v1.html.txt").exists());
        assert!(!originals.join("S-0098-v1.html").exists());
        assert!(!edition.join("s/S-0097-v1.html").exists());
        assert!(!originals.join("S-0097-v1.html.txt").exists());
        assert_no_forbidden_text(
            &edition,
            &originals,
            &[
                "Forbidden source page canary",
                "Forbidden original canary",
                "Orphan original canary",
                "S-0098",
                "S-0097",
            ],
        );
    }

    /// The delivered `assets/search-index.js` of `edition`, parsed: the JSON between
    /// `window.FB_INDEX = ` and `;`, and the raw text.
    fn delivered_search_index(edition: &Path) -> (serde_json::Value, String) {
        let text = std::fs::read_to_string(edition.join("assets/search-index.js"))
            .unwrap_or_else(|e| panic!("the handler must deliver assets/search-index.js: {e}"));
        let json = text
            .strip_prefix("window.FB_INDEX = ")
            .and_then(|s| s.strip_suffix(';'))
            .unwrap_or_else(|| panic!("the index must be `window.FB_INDEX = <json>;`"));
        let value = serde_json::from_str(json).unwrap_or_else(|e| panic!("index JSON: {e}"));
        (value, text)
    }

    /// The unit ids of the delivered index, in unit-number order.
    fn index_doc_ids(index: &serde_json::Value) -> Vec<String> {
        index["docs"]
            .as_array()
            .unwrap_or_else(|| panic!("docs"))
            .iter()
            .map(|d| d[0].as_str().unwrap_or_else(|| panic!("doc id")).to_owned())
            .collect()
    }

    /// The ids of the units that `term`'s postings name, resolved through the delivered unit
    /// table, or `None` when the index has no such term.
    fn posting_ids(index: &serde_json::Value, term: &str) -> Option<Vec<String>> {
        let ids = index_doc_ids(index);
        let terms = index["terms"].as_array().unwrap_or_else(|| panic!("terms"));
        let at = terms.iter().position(|t| t == term)?;
        let post = index["post"][at]
            .as_array()
            .unwrap_or_else(|| panic!("post"));
        Some(
            post.chunks(2)
                .map(|pair| {
                    let unit = pair[0].as_u64().unwrap_or_else(|| panic!("unit number"));
                    ids[usize::try_from(unit).unwrap_or(usize::MAX)].clone()
                })
                .collect(),
        )
    }

    /// The delivered index holds the permitted page and section units, their snippets and
    /// postings, and no record the view leaves out, no held version and no unit of another kind.
    #[test]
    fn build_search_index_uses_the_permitted_view() {
        let tmp = temp_copy(&fixture_root());
        add_version(
            tmp.path(),
            "0005",
            1,
            Some(("S-0005-v1.pdf", PDF_BYTES)),
            "quarantined",
            "Held for test heldsearchcanary.",
        );
        let edition = deliver_with_disallowed_records(tmp.path());
        let (index, text) = delivered_search_index(&edition);
        let ids = index_doc_ids(&index);

        // Permitted presence stays required: page leads, sections, a clause card, a snippet and
        // the postings of a permitted word.
        for id in [
            "cmp-system-review",
            "cmp-system-review#purpose",
            "cmp-system-review#the-rule",
            "est-rate-window-brief",
            "est-rate-window-brief#recommendation",
            "cmp-clause-9900-101",
        ] {
            assert!(
                ids.iter().any(|d| d == id),
                "permitted unit {id} must be indexed"
            );
        }
        let docs = index["docs"].as_array().unwrap_or_else(|| panic!("docs"));
        let lead = docs
            .iter()
            .find(|d| d[0] == "cmp-system-review")
            .unwrap_or_else(|| panic!("lead"));
        assert_eq!(
            lead[4],
            "Explain how a synthetic system review interacts with a synthetic billing withhold.",
            "the permitted lead snippet"
        );
        let clause = docs
            .iter()
            .find(|d| d[0] == "cmp-clause-9900-101")
            .unwrap_or_else(|| panic!("clause"));
        assert_eq!(clause[1], "clause");
        let reviewers =
            posting_ids(&index, "reviewers").unwrap_or_else(|| panic!("a permitted term"));
        assert!(
            reviewers
                .iter()
                .any(|id| id == "cmp-system-review#how-it-works"),
            "the postings of `reviewers` must name the permitted section: {reviewers:?}"
        );

        // Forbidden: no record the view leaves out, no held version, no unit of another kind.
        for id in &ids {
            for forbidden in [
                "est-forbidden-brief",
                "est-dep-",
                "CL-CMP-009",
                "Q-098",
                "XC-9",
                "S-0098",
                "S-0005",
                "forbidden_window",
            ] {
                assert!(!id.starts_with(forbidden), "unit {id} must not be indexed");
            }
            for other_kind in [
                "CL-CMP-0001",
                "S-0002-v2",
                "Q-001",
                "XC-01",
                "system_review",
            ] {
                assert_ne!(id, other_kind, "a {other_kind} unit must not be indexed");
            }
        }
        for doc in docs {
            assert!(doc[1] == "page" || doc[1] == "clause", "{doc}");
        }
        for needle in [
            "Forbidden",
            "Dependent ",
            "est-forbidden-brief",
            "est-dep-",
            "CL-CMP-0098",
            "Q-098",
            "XC-98",
            "S-0098",
            "S-0005",
            "heldsearchcanary",
            "np-0007",
        ] {
            assert!(!text.contains(needle), "the index holds `{needle}`");
        }
        // The unique words of the left-out and held records are not terms.
        for term in [
            "forbidden",
            "dependent",
            "canary",
            "heldsearchcanary",
            "s-0005-v1",
        ] {
            assert_eq!(
                posting_ids(&index, term),
                None,
                "`{term}` must not be a term of the index"
            );
        }
    }

    /// The `id` attributes of the `<h2>` and `<h3>` headings of `html`.
    fn heading_ids(html: &str) -> Vec<String> {
        let mut out = Vec::new();
        for tag in ["<h2 ", "<h3 "] {
            for chunk in html.split(tag).skip(1) {
                let head = chunk.split('>').next().unwrap_or_default();
                if let Some(rest) = head.split("id=\"").nth(1) {
                    out.push(rest.split('"').next().unwrap_or_default().to_owned());
                }
            }
        }
        out
    }

    /// Every section fragment of the delivered index equals a heading id of the delivered page,
    /// with a repeated heading and a non-ASCII heading on the page; and a two-sentence Purpose
    /// gives the lead its first paragraph while summaries use its first sentence.
    #[test]
    fn search_section_links_resolve_to_rendered_headings() {
        let tmp = temp_copy(&fixture_root());
        let path = tmp
            .path()
            .join("corpus/pages/compliance/business-systems/cmp-system-review.md");
        let page = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
        let page = page.replace(
            "## Purpose\n\nExplain how a synthetic system review interacts with a synthetic billing withhold.\n",
            "## Purpose\n\nExplain how a synthetic system review interacts with a synthetic billing withhold. A second synthetic sentence names the review cycle.\n\n### The rule\n\nA synthetic subheading that repeats a part heading.\n\n### Résumé übersicht\n\nA synthetic subheading with non-ASCII letters.\n",
        );
        assert!(
            page.contains("### Résumé übersicht"),
            "the fixture edit applies"
        );
        std::fs::write(&path, page).unwrap_or_else(|e| panic!("{e}"));

        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            stderr_of(&output)
        );
        let edition = tmp.path().join("out/edition");
        let (index, _text) = delivered_search_index(&edition);
        let ids = index_doc_ids(&index);

        let mut sections = 0usize;
        for id in &ids {
            let Some((page_id, fragment)) = id.split_once('#') else {
                continue;
            };
            sections += 1;
            let html = std::fs::read_to_string(edition.join(format!("p/{page_id}.html")))
                .unwrap_or_else(|e| panic!("p/{page_id}.html: {e}"));
            let headings = heading_ids(&html);
            assert!(
                headings.iter().any(|h| h == fragment),
                "section {id} must link to a rendered heading of p/{page_id}.html: {headings:?}"
            );
        }
        assert!(sections > 0, "the index holds section units");

        // The repeated heading moves the part's anchor on, in the index and on the page alike.
        let review = std::fs::read_to_string(edition.join("p/cmp-system-review.html"))
            .unwrap_or_else(|e| panic!("{e}"));
        let review_headings = heading_ids(&review);
        assert!(
            ids.iter().any(|id| id == "cmp-system-review#the-rule-2"),
            "{ids:?}"
        );
        assert!(
            !ids.iter().any(|id| id == "cmp-system-review#the-rule"),
            "{ids:?}"
        );
        for anchor in ["the-rule", "the-rule-2", "résumé-übersicht"] {
            assert!(
                review_headings.iter().any(|h| h == anchor),
                "p/cmp-system-review.html must render the heading id `{anchor}`: {review_headings:?}"
            );
        }

        // The lead snippet is Purpose's first paragraph, both sentences, under 160 code points.
        let docs = index["docs"].as_array().unwrap_or_else(|| panic!("docs"));
        let lead = docs
            .iter()
            .find(|d| d[0] == "cmp-system-review")
            .unwrap_or_else(|| panic!("lead"));
        assert_eq!(
            lead[4],
            "Explain how a synthetic system review interacts with a synthetic billing withhold. A second synthetic sentence names the review cycle."
        );
        // A summary uses the first sentence only.
        let area =
            std::fs::read_to_string(edition.join("a/cmp.html")).unwrap_or_else(|e| panic!("{e}"));
        assert!(area.contains(
            "Explain how a synthetic system review interacts with a synthetic billing withhold."
        ));
        assert!(!area.contains("A second synthetic sentence"));
    }
}
