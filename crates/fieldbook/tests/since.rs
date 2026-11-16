//! Integration tests for `fieldbook build --since` (`crates/fieldbook/src/since.rs`).

#![expect(
    clippy::disallowed_methods,
    reason = "integration tests prepare temporary fixtures and inspect built files"
)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fieldbook::build::run_with_info;
use fieldbook::cli::BuildArgs;
use fieldbook::since;
use fieldbook_core::CivilDate;
use fieldbook_core::edition::EditionInfo;
use fieldbook_render::meta::{BuildInfo, BuildProfile};

use support::temp_copy;
use support::world::TestWorld;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

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

fn released_info(ymd: &str) -> BuildInfo {
    let date = CivilDate::parse(ymd).unwrap_or_else(|e| panic!("{e}"));
    BuildInfo::new(
        Some(date),
        Some(COMMIT.to_owned()),
        BuildProfile::Silent,
        Some(1_772_323_200),
    )
}

fn set_changes_heading(root: &Path, heading_num: u32) {
    let changes_path = root.join("corpus/changes.md");
    let raw = std::fs::read_to_string(&changes_path).unwrap_or_else(|e| panic!("{e}"));
    let header_lines: String = raw.lines().take(3).collect::<Vec<_>>().join("\n");
    let mut body = format!("{header_lines}\n");
    for n in (1..=heading_num).rev() {
        body.push_str(&format!(
            "\n## edition-{n:03}\n\n- Added: synthetic note {n}.\n"
        ));
    }
    std::fs::write(&changes_path, body).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn since_reads_folder_and_file() {
    let tmp = temp_copy(&fixture_root());
    set_changes_heading(tmp.path(), 1);
    let world = TestWorld::new(tmp.path());

    // Build edition-001 first, then modify its edition.toml copy to omit one page so the diff is non-empty.
    let ed1_out = tmp.path().join("ed1");
    let out1 = run_with_info(
        &BuildArgs {
            out: ed1_out.clone(),
            watch: false,
            since: None,
        },
        &world,
        &released_info("2026-02-01"),
    );
    assert_eq!(out1.code, 0, "stderr: {}", out1.stderr);

    let ed1_dir = ed1_out.join("edition");
    let ed1_toml_path = ed1_dir.join("edition.toml");
    let mut ed1_parsed =
        EditionInfo::parse(&std::fs::read_to_string(&ed1_toml_path).unwrap()).unwrap();
    let added_page = ed1_parsed.ids.pages.remove(0);
    ed1_parsed.counts.pages = ed1_parsed.ids.pages.len();
    std::fs::write(&ed1_toml_path, ed1_parsed.to_toml()).unwrap();

    // Unit-level `since::read_since` accepts both folder and file and parses identically.
    let from_folder = since::read_since(&ed1_dir.to_string_lossy()).unwrap();
    let from_file = since::read_since(&ed1_toml_path.to_string_lossy()).unwrap();
    assert_eq!(from_folder, from_file);
    assert_eq!(from_folder.edition, "edition-001");

    // Advance Corpus to edition-002 and build twice: once with `--since <folder>`, once with `--since <file>`.
    set_changes_heading(tmp.path(), 2);
    let out_folder = tmp.path().join("ed2-from-folder");
    let res_folder = run_with_info(
        &BuildArgs {
            out: out_folder.clone(),
            watch: false,
            since: Some(ed1_dir.to_string_lossy().into_owned()),
        },
        &world,
        &released_info("2026-03-01"),
    );
    assert_eq!(res_folder.code, 0, "stderr: {}", res_folder.stderr);

    let out_file = tmp.path().join("ed2-from-file");
    let res_file = run_with_info(
        &BuildArgs {
            out: out_file.clone(),
            watch: false,
            since: Some(ed1_toml_path.to_string_lossy().into_owned()),
        },
        &world,
        &released_info("2026-03-01"),
    );
    assert_eq!(res_file.code, 0, "stderr: {}", res_file.stderr);

    let html_folder = std::fs::read_to_string(out_folder.join("edition/changes.html")).unwrap();
    let html_file = std::fs::read_to_string(out_file.join("edition/changes.html")).unwrap();
    assert_eq!(html_folder, html_file);
    assert!(html_folder.contains("Ids since edition-001"));
    assert!(html_folder.contains(&format!("href=\"p/{added_page}.html\">{added_page}</a>")));
}

#[test]
fn since_rejects_invalid_edition_toml() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    // Seed an existing build at `out/` so we verify a refused `--since` leaves it completely untouched.
    let seed = fieldbook::run(&["build".to_owned()], &world);
    assert_eq!(seed.code, 0, "stderr: {}", seed.stderr);

    let bad_dir = tempfile::tempdir().unwrap();
    let bad_toml = bad_dir.path().join("edition.toml");
    std::fs::write(&bad_toml, "edition = [\nnot valid toml\n").unwrap();

    let before = collect_tree_bytes(tmp.path());

    let outcome = fieldbook::run(
        &[
            "build".to_owned(),
            "--since".to_owned(),
            bad_toml.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(outcome.code, 2, "stderr: {}", outcome.stderr);
    assert!(outcome.stdout.is_empty());
    assert_eq!(
        outcome.stderr.lines().count(),
        1,
        "error must be a plain one-line message, got:\n{}",
        outcome.stderr
    );
    assert_eq!(
        collect_tree_bytes(tmp.path()),
        before,
        "invalid --since must leave target tree and journal completely unchanged"
    );

    // Also verify missing/unreadable `--since` exits with EXIT_IO (3) and leaves tree unchanged.
    let missing = bad_dir.path().join("does-not-exist/edition.toml");
    let io_outcome = fieldbook::run(
        &[
            "build".to_owned(),
            "--since".to_owned(),
            missing.to_string_lossy().into_owned(),
        ],
        &world,
    );
    assert_eq!(io_outcome.code, 3, "stderr: {}", io_outcome.stderr);
    assert_eq!(io_outcome.stderr.lines().count(), 1);
    assert_eq!(collect_tree_bytes(tmp.path()), before);
}

#[test]
fn since_rejects_newer_or_equal_edition() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    let since_tmp = tempfile::tempdir().unwrap();

    // Build edition-002 and edition-003 manifests into `since_tmp`.
    set_changes_heading(tmp.path(), 2);
    let ed2_out = since_tmp.path().join("ed2");
    assert_eq!(
        run_with_info(
            &BuildArgs {
                out: ed2_out.clone(),
                watch: false,
                since: None,
            },
            &world,
            &released_info("2026-02-15"),
        )
        .code,
        0
    );

    set_changes_heading(tmp.path(), 3);
    let ed3_out = since_tmp.path().join("ed3");
    assert_eq!(
        run_with_info(
            &BuildArgs {
                out: ed3_out.clone(),
                watch: false,
                since: None,
            },
            &world,
            &released_info("2026-02-20"),
        )
        .code,
        0
    );

    // Reset Corpus to edition-002 and seed `out` so we can verify `out` + journal stay untouched on refusal.
    set_changes_heading(tmp.path(), 2);
    let target_out = tmp.path().join("out");
    assert_eq!(
        run_with_info(
            &BuildArgs {
                out: target_out.clone(),
                watch: false,
                since: None,
            },
            &world,
            &released_info("2026-03-01"),
        )
        .code,
        0
    );
    let before = collect_tree_bytes(tmp.path());

    // Equal numbered edition (`edition-002` vs `edition-002`) -> exit 2, tree unchanged.
    let eq_outcome = run_with_info(
        &BuildArgs {
            out: target_out.clone(),
            watch: false,
            since: Some(
                ed2_out
                    .join("edition/edition.toml")
                    .to_string_lossy()
                    .into_owned(),
            ),
        },
        &world,
        &released_info("2026-03-01"),
    );
    assert_eq!(eq_outcome.code, 2, "stderr: {}", eq_outcome.stderr);
    assert_eq!(eq_outcome.stderr.lines().count(), 1);
    assert_eq!(
        collect_tree_bytes(tmp.path()),
        before,
        "equal edition --since must leave tree unchanged"
    );

    // Newer numbered edition (`edition-003` vs `edition-002`) -> exit 2, tree unchanged.
    let newer_outcome = run_with_info(
        &BuildArgs {
            out: target_out,
            watch: false,
            since: Some(
                ed3_out
                    .join("edition/edition.toml")
                    .to_string_lossy()
                    .into_owned(),
            ),
        },
        &world,
        &released_info("2026-03-01"),
    );
    assert_eq!(newer_outcome.code, 2, "stderr: {}", newer_outcome.stderr);
    assert_eq!(newer_outcome.stderr.lines().count(), 1);
    assert_eq!(
        collect_tree_bytes(tmp.path()),
        before,
        "newer edition --since must leave tree unchanged"
    );
}

#[test]
fn build_without_since_builds_changes_page() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    let outcome = fieldbook::run(&["build".to_owned()], &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);

    let changes_path = tmp.path().join("out/edition/changes.html");
    assert!(
        changes_path.is_file(),
        "changes.html must be built without --since"
    );
    let html = std::fs::read_to_string(changes_path).unwrap();
    assert!(html.contains("Ids since an earlier edition"));
    assert!(html.contains(
        "No earlier edition was named for this build, so the ids added and removed are not listed."
    ));
    assert!(!html.contains("<table"));
}
