//! Integration tests for `fieldbook new` (`new-verbs`), each on a temporary copy of
//! `tests/fixtures/corpus-mini`.

#![expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "integration tests prepare temporary fixtures and inspect written files and journal state"
)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fieldbook::World;
use fieldbook_core::CivilDate;
use fieldbook_core::check::{self, Host, Options};
use fieldbook_core::ids::AreaCode;
use fieldbook_core::new::{Fields, RowKind, plan_row, render_register};
use fieldbook_core::schema::table::{Table, read as read_csv, write as write_csv};
use serde_json::Value;

use support::faulty::{Fault, FaultKind, FaultyWorld};
use support::temp_copy;
use support::world::TestWorld;

const TODAY: &str = "2026-03-01";
const PAGE_PATH: &str = "corpus/pages/compliance/business-systems/cmp-synthetic-test.md";
const BANNER: &str = "PROPRIETARY AND CONFIDENTIAL";

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini")
}

fn svec(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

fn run(world: &dyn World, args: &[&str]) -> fieldbook::Outcome {
    fieldbook::run(&svec(args), world)
}

fn audit_show_json(world: &dyn World, extra: &[&str]) -> Value {
    let mut args = vec!["audit", "show", "--json"];
    args.extend_from_slice(extra);
    let out = run(world, &args);
    assert_eq!(out.code, 0, "audit show failed: {}", out.stderr);
    serde_json::from_str(&out.stdout).unwrap_or_else(|e| panic!("parse audit show json: {e}"))
}

fn events_of(world: &dyn World, kind: &str) -> Vec<Value> {
    audit_show_json(world, &["--kind", kind])["events"]
        .as_array()
        .expect("events array")
        .clone()
}

fn count(world: &dyn World, kind: &str) -> usize {
    events_of(world, kind).len()
}

fn table_at(root: &Path, rel: &str) -> Table {
    read_csv(&std::fs::read(root.join(rel)).expect("read register")).expect("parse register")
}

/// The cells of the row whose first cell is `id`, keyed by column name.
fn row(root: &Path, rel: &str, id: &str) -> BTreeMap<String, String> {
    let table = table_at(root, rel);
    let found = table
        .rows
        .iter()
        .find(|r| r.cells.first().is_some_and(|c| c == id))
        .unwrap_or_else(|| panic!("no row `{id}` in {rel}"));
    table
        .header
        .cells
        .iter()
        .cloned()
        .zip(found.cells.iter().cloned())
        .collect()
}

fn ids(root: &Path, rel: &str) -> Vec<String> {
    table_at(root, rel)
        .rows
        .iter()
        .filter_map(|r| r.cells.first().cloned())
        .collect()
}

/// Clone the first row of a register, edit its cells, and append the copy.
fn append_row(root: &Path, rel: &str, edit: impl FnOnce(&mut Vec<String>)) {
    let mut table = table_at(root, rel);
    let mut next = table.rows.first().expect("a first row").clone();
    edit(&mut next.cells);
    next.line += 100;
    table.rows.push(next);
    std::fs::write(root.join(rel), write_csv(&table)).expect("write register");
}

/// Put the banner in the last cell of the first row, so the file holds marked text.
fn mark_register(root: &Path, rel: &str) {
    let mut table = table_at(root, rel);
    let first = table.rows.first_mut().expect("a first row");
    *first.cells.last_mut().expect("a last cell") = format!("{BANNER} synthetic");
    std::fs::write(root.join(rel), write_csv(&table)).expect("write register");
}

fn read_bytes(root: &Path, rel: &str) -> Option<Vec<u8>> {
    std::fs::read(root.join(rel)).ok()
}

fn corpus_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let rel = path
                    .strip_prefix(base)
                    .expect("under base")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).expect("read file"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&root.join("corpus"), root, &mut out);
    out
}

const PAGE_ARGS: &[&str] = &[
    "new",
    "page",
    "--confirm-public",
    "cmp-synthetic-test",
    "--type",
    "concept",
    "--subarea",
    "business-systems",
];
const CLAIM_ARGS: &[&str] = &[
    "new",
    "claim",
    "--confirm-public",
    "--area",
    "cmp",
    "--source",
    "S-0002-v2",
    "--at",
    "p. 3",
    "--quote",
    "Synthetic quote, made up for tests.",
];
const QUESTION_ARGS: &[&str] = &[
    "new",
    "question",
    "--confirm-public",
    "--area",
    "cmp",
    "Synthetic question, made up for tests?",
];
const CONFLICT_ARGS: &[&str] = &["new", "conflict", "--confirm-public", "--area", "cmp"];
const TERM_ARGS: &[&str] = &["new", "term", "--confirm-public", "--area", "cmp"];

#[test]
fn new_page_from_template() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    let out = run(&world, PAGE_ARGS);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, format!("{PAGE_PATH}\n"));
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);

    let page = std::fs::read_to_string(tmp.path().join(PAGE_PATH)).expect("read page");
    let template = std::fs::read_to_string(tmp.path().join("corpus/templates/concept.md"))
        .expect("read template");
    let expected = template
        .replace("id = \"<area>-<kebab>\"", "id = \"cmp-synthetic-test\"")
        .replace("area = \"<area>\"", "area = \"cmp\"")
        .replace("subarea = \"<subarea>\"", "subarea = \"business-systems\"")
        .replace("as_of = \"<today>\"", &format!("as_of = {TODAY}"))
        .replace("<today>", TODAY);
    assert_eq!(page, expected);
    assert!(!page.contains("<today>"));
    // The placeholders no flag fills stay for the author.
    assert!(page.contains("title = \"<title>\""));

    let begin = events_of(&world, "write.begin");
    assert_eq!(begin.len(), 1);
    assert_eq!(begin[0]["verb"], "new page");
    assert_eq!(begin[0]["labels"], serde_json::json!(["public"]));
}

#[test]
fn new_claim_takes_next_id_in_its_area() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    let out = run(&world, CLAIM_ARGS);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "CL-CMP-0003\n");

    let est = run(
        &world,
        &[
            "new",
            "claim",
            "--confirm-public",
            "--area",
            "est",
            "--source",
            "S-0002-v2",
            "--at",
            "p. 4",
            "--quote",
            "Another synthetic quote.",
        ],
    );
    assert_eq!(est.code, 0, "stderr: {}", est.stderr);
    assert_eq!(est.stdout, "CL-EST-0004\n");

    let claim = row(tmp.path(), "corpus/registers/claims.csv", "CL-CMP-0003");
    assert_eq!(claim["area"], "cmp");
    assert_eq!(claim["kind"], "fact");
    assert_eq!(claim["source_id"], "S-0002");
    assert_eq!(claim["version"], "2");
    assert_eq!(claim["locator"], "p. 3");
    assert_eq!(claim["quote"], "Synthetic quote, made up for tests.");
    assert_eq!(claim["verification"], "unchecked");
    assert_eq!(claim["applicability"], "pending_company_confirmation");
    assert_eq!(claim["adoption"], "proposed");
    assert_eq!(claim["data_origin"], "unknown");
    assert_eq!(claim["consequential"], "false");
    assert_eq!(claim["drafted_by"], "codex");
    assert_eq!(claim["verified_on"], "");
    assert_eq!(claim["verified_by_role"], "");

    // Only the new rows were added; the others are as they were.
    let before = table_at(&fixture_root(), "corpus/registers/claims.csv");
    let after = table_at(tmp.path(), "corpus/registers/claims.csv");
    assert_eq!(after.rows.len(), before.rows.len() + 2);
    let old_rows: Vec<_> = after
        .rows
        .iter()
        .filter(|r| !["CL-CMP-0003", "CL-EST-0004"].contains(&r.cells[0].as_str()))
        .collect();
    assert_eq!(old_rows.len(), before.rows.len());
    for (a, b) in before.rows.iter().zip(old_rows) {
        assert_eq!(a.cells, b.cells);
    }
}

#[test]
fn next_id_counts_retired_rows() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    // A superseded claim, a withdrawn question and a resolved conflict still hold their ids.
    append_row(root, "corpus/registers/claims.csv", |cells| {
        cells[0] = "CL-CMP-0009".to_owned();
        cells[31] = "CL-CMP-0010".to_owned();
    });
    append_row(root, "corpus/registers/questions.csv", |cells| {
        cells[0] = "Q-005".to_owned();
        cells[9] = "withdrawn".to_owned();
    });
    append_row(root, "corpus/registers/conflicts.csv", |cells| {
        cells[0] = "XC-07".to_owned();
        cells[8] = "resolved".to_owned();
    });
    let world = TestWorld::new(root);

    assert_eq!(run(&world, CLAIM_ARGS).stdout, "CL-CMP-0010\n");
    assert_eq!(run(&world, QUESTION_ARGS).stdout, "Q-006\n");
    assert_eq!(run(&world, CONFLICT_ARGS).stdout, "XC-08\n");
}

#[test]
fn new_question_takes_text() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    let out = run(&world, QUESTION_ARGS);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "Q-003\n");

    let q = row(tmp.path(), "corpus/registers/questions.csv", "Q-003");
    assert_eq!(q["area"], "cmp");
    assert_eq!(q["question"], "Synthetic question, made up for tests?");
    // Both defaults.
    assert_eq!(q["day_one"], "false");
    assert_eq!(q["status"], "open");
    // Optional and company-answer cells stay empty.
    for column in [
        "owner_role",
        "blocks",
        "why_it_matters",
        "default_assumption",
        "work_can_continue",
        "answer",
        "answered_on",
        "adoption",
    ] {
        assert_eq!(q[column], "", "column `{column}` must be empty");
    }

    // Strict validation of the register reports nothing.
    let check = run(
        &world,
        &[
            "check",
            "--strict",
            "--today",
            TODAY,
            &tmp.path()
                .join("corpus/registers/questions.csv")
                .to_string_lossy(),
        ],
    );
    assert_eq!(
        check.code, 0,
        "stdout: {} stderr: {}",
        check.stdout, check.stderr
    );
    assert!(check.stderr.is_empty(), "stderr: {}", check.stderr);
}

#[test]
fn new_conflict_and_term() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    let conflict = run(&world, CONFLICT_ARGS);
    assert_eq!(conflict.code, 0, "stderr: {}", conflict.stderr);
    assert_eq!(conflict.stdout, "XC-02\n");
    let c = row(tmp.path(), "corpus/registers/conflicts.csv", "XC-02");
    assert_eq!(c["area"], "cmp");
    assert_eq!(c["status"], "unresolved");
    assert_eq!(c["resolution"], "");

    let term = run(&world, TERM_ARGS);
    assert_eq!(term.code, 0, "stderr: {}", term.stderr);
    assert_eq!(term.stdout, "cmp_term_1\n");
    let t = row(tmp.path(), "corpus/registers/glossary.csv", "cmp_term_1");
    assert_eq!(t["owner_area"], "cmp");
    assert_eq!(t["definition_status"], "general_practice");

    let named = run(
        &world,
        &[
            "new",
            "term",
            "--confirm-public",
            "--area",
            "cmp",
            "--id",
            "synthetic_term",
        ],
    );
    assert_eq!(named.stdout, "synthetic_term\n");
    assert_eq!(run(&world, TERM_ARGS).stdout, "cmp_term_2\n");

    // A taken or malformed term id is a usage error and writes nothing.
    let before = corpus_files(tmp.path());
    for bad in ["synthetic_term", "Not Snake"] {
        let out = run(
            &world,
            &[
                "new",
                "term",
                "--confirm-public",
                "--area",
                "cmp",
                "--id",
                bad,
            ],
        );
        assert_eq!(out.code, 2, "{bad}: {}", out.stderr);
    }
    assert_eq!(corpus_files(tmp.path()), before);
}

#[test]
fn missing_flag_without_terminal_is_usage_error() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    world.set_stdin_terminal(false);
    world.push_input_line("must not be read");
    let before = corpus_files(tmp.path());

    for args in [
        &["new", "question", "--confirm-public", "--area", "cmp"][..],
        &["new", "conflict", "--confirm-public"][..],
        &[
            "new",
            "claim",
            "--confirm-public",
            "--area",
            "cmp",
            "--source",
            "S-0002-v2",
            "--at",
            "p. 1",
        ][..],
        &[
            "new",
            "page",
            "--confirm-public",
            "cmp-synthetic-test",
            "--type",
            "concept",
        ][..],
    ] {
        let out = run(&world, args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.stderr);
        assert!(out.stderr.contains("missing required"), "{}", out.stderr);
        assert!(out.stdout.is_empty());
    }
    assert!(
        world.prompts().is_empty(),
        "nothing is prompted off a terminal"
    );
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn missing_flag_on_a_terminal_is_prompted() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    world.set_stdin_terminal(true);
    world.push_input_line("Prompted question?");

    let out = run(
        &world,
        &["new", "question", "--confirm-public", "--area", "cmp"],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "Q-003\n");
    // The prompt went through the writer, once, and not into the outcome.
    assert_eq!(world.prompts(), vec!["text: ".to_owned()]);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    let q = row(tmp.path(), "corpus/registers/questions.csv", "Q-003");
    assert_eq!(q["question"], "Prompted question?");

    // An empty answer is the same usage error as a missing flag.
    let before = corpus_files(tmp.path());
    world.push_input_line("   ");
    let empty = run(
        &world,
        &["new", "question", "--confirm-public", "--area", "cmp"],
    );
    assert_eq!(empty.code, 2, "{}", empty.stderr);
    assert!(
        empty.stderr.contains("missing required argument `<text>`"),
        "a positional is named as an argument: {}",
        empty.stderr
    );
    assert_eq!(corpus_files(tmp.path()), before);

    // The prompt is flushed before the line is read: input is withheld until it has been.
    {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        world.set_stdin_terminal(true);
        world.set_wait_for_input(true);

        std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                run(
                    &world,
                    &["new", "question", "--confirm-public", "--area", "cmp"],
                )
            });
            // Input is withheld until the prompt has been flushed.
            assert!(
                world.wait_for_prompts(1),
                "the prompt was not flushed before input was read"
            );
            assert_eq!(world.prompts(), vec!["text: ".to_owned()]);
            world.push_input_line("Answered after the flush?");
            let out = handle.join().expect("command thread");
            assert_eq!(out.code, 0, "stderr: {}", out.stderr);
            assert_eq!(out.stdout, "Q-003\n");
        });
        assert_eq!(world.prompts().len(), 1, "one prompt only");
    }
}

#[test]
fn a_failed_prompt_write_exits_3_and_writes_nothing() {
    let tmp = temp_copy(&fixture_root());
    let world = FaultyWorld::new(TestWorld::new(tmp.path()), Fault::BrokenPrompt);
    world.inner.set_stdin_terminal(true);
    world.inner.push_input_line("never read");
    let before = corpus_files(tmp.path());

    let out = run(
        &world,
        &["new", "question", "--confirm-public", "--area", "cmp"],
    );
    assert_eq!(out.code, 3, "stderr: {}", out.stderr);
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn prompt_holds_no_store_or_journal_lease() {
    let tmp = temp_copy(&fixture_root());
    let prompting = TestWorld::new(tmp.path());
    prompting.set_stdin_terminal(true);
    prompting.set_wait_for_input(true);
    let other = TestWorld::new(tmp.path());

    std::thread::scope(|scope| {
        let handle = scope.spawn(|| {
            run(
                &prompting,
                &["new", "question", "--confirm-public", "--area", "cmp"],
            )
        });
        assert!(prompting.wait_for_prompts(1), "the prompt never appeared");

        // Another writer completes while the first command waits for its answer.
        let done = run(
            &other,
            &[
                "new",
                "question",
                "--confirm-public",
                "--area",
                "cmp",
                "Written meanwhile?",
            ],
        );
        assert_eq!(done.code, 0, "the lease was held: {}", done.stderr);
        assert_eq!(done.stdout, "Q-003\n");

        prompting.push_input_line("Written after the wait?");
        let out = handle.join().expect("command thread");
        assert_eq!(out.code, 0, "stderr: {}", out.stderr);
        // Allocated from the tree as the other writer left it.
        assert_eq!(out.stdout, "Q-004\n");
    });
    assert_eq!(prompting.prompts().len(), 1, "one prompt only");
    let q3 = row(tmp.path(), "corpus/registers/questions.csv", "Q-003");
    let q4 = row(tmp.path(), "corpus/registers/questions.csv", "Q-004");
    assert_eq!(q3["question"], "Written meanwhile?");
    assert_eq!(q4["question"], "Written after the wait?");
}

#[test]
fn unknown_version_writes_nothing() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let before = corpus_files(tmp.path());

    for version in ["S-9999-v1", "S-0002-v9"] {
        let out = run(
            &world,
            &[
                "new",
                "claim",
                "--confirm-public",
                "--area",
                "cmp",
                "--source",
                version,
                "--at",
                "p. 1",
                "--quote",
                "synthetic",
            ],
        );
        assert_eq!(out.code, 2, "stderr: {}", out.stderr);
        assert!(out.stderr.contains(version), "stderr: {}", out.stderr);
        assert!(out.stdout.is_empty());
    }
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn a_line_break_in_a_value_is_refused_and_writes_nothing() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let before = corpus_files(tmp.path());

    for bad in [
        "First line\nsecond line?",
        "carriage\rreturn?",
        "tab\tseparated?",
    ] {
        let out = run(
            &world,
            &["new", "question", "--confirm-public", "--area", "cmp", bad],
        );
        assert_eq!(out.code, 2, "{bad:?}: {}", out.stderr);
        assert!(out.stderr.contains("`text`"), "{}", out.stderr);
        let out = run(
            &world,
            &[
                "new",
                "claim",
                "--confirm-public",
                "--area",
                "cmp",
                "--source",
                "S-0002-v2",
                "--at",
                "p. 1",
                "--quote",
                bad,
            ],
        );
        assert_eq!(out.code, 2, "{bad:?}: {}", out.stderr);
        assert!(out.stderr.contains("`quote`"), "{}", out.stderr);
    }
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);
    // The registers still read, and the next question still takes the next id.
    assert_eq!(run(&world, QUESTION_ARGS).stdout, "Q-003\n");
}

#[test]
fn an_unreadable_source_handling_cell_is_refused_as_unlabelled() {
    let tmp = temp_copy(&fixture_root());
    let rel = "corpus/registers/sources.csv";
    let mut table = table_at(tmp.path(), rel);
    let at = table
        .header
        .cells
        .iter()
        .position(|c| c == "handling")
        .expect("handling column");
    let target = table
        .rows
        .iter_mut()
        .find(|r| r.cells.first().is_some_and(|c| c == "S-0002") && r.cells[1] == "2")
        .expect("S-0002 v2");
    target.cells[at] = "not a label!".to_owned();
    std::fs::write(tmp.path().join(rel), write_csv(&table)).expect("write sources");

    let world = TestWorld::new(tmp.path());
    let before = corpus_files(tmp.path());
    let out = run(&world, CLAIM_ARGS);
    assert_refused(&world, &out, "FB0410", "unlabelled", 0);
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);
}

/// Rewrite the `concept` template's `handling` line to `replacement` (empty removes it).
fn set_template_handling(root: &Path, replacement: &str) {
    let path = root.join("corpus/templates/concept.md");
    let text = std::fs::read_to_string(&path).expect("read template");
    assert!(
        text.contains("handling = [\"public\"]\n"),
        "template handling line"
    );
    let edited = text.replacen("handling = [\"public\"]\n", replacement, 1);
    std::fs::write(&path, edited).expect("write template");
}

#[test]
fn a_multiline_template_handling_array_keeps_every_label() {
    let tmp = temp_copy(&fixture_root());
    set_template_handling(tmp.path(), "handling = [\"public\",\n  \"np-0007\"\n]\n");
    let world = TestWorld::new(tmp.path());
    let before = corpus_files(tmp.path());
    let out = run(&world, PAGE_ARGS);
    // The second label is not public: the Corpus refuses it, naming it, even with
    // `--confirm-public`.
    assert_refused(&world, &out, "FB0410", "unlabelled", 0);
    assert!(out.stderr.contains("np-0007"), "{}", out.stderr);
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn a_missing_or_malformed_template_handling_is_refused_as_unlabelled() {
    for replacement in [
        "",
        "handling = 5\n",
        "handling = [\"public\", 5]\n",
        "handling = []\n",
        "handling = [\"public\",\n",
        "handling = [\"Not A Label!\"]\n",
    ] {
        let tmp = temp_copy(&fixture_root());
        set_template_handling(tmp.path(), replacement);
        let world = TestWorld::new(tmp.path());
        let before = corpus_files(tmp.path());
        let out = run(&world, PAGE_ARGS);
        assert_refused(&world, &out, "FB0410", "unlabelled", 0);
        assert_eq!(corpus_files(tmp.path()), before, "{replacement:?}");
        assert_eq!(count(&world, "write.begin"), 0, "{replacement:?}");
    }
}

#[test]
fn unknown_area_subarea_type_and_a_taken_page_write_nothing() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let before = corpus_files(tmp.path());

    let cases: &[&[&str]] = &[
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "zzz",
            "Where?",
        ],
        &["new", "conflict", "--confirm-public", "--area", "zzz"],
        &[
            "new",
            "page",
            "--confirm-public",
            "cmp-x",
            "--type",
            "concept",
            "--subarea",
            "nowhere",
        ],
        &[
            "new",
            "page",
            "--confirm-public",
            "cmp-x",
            "--type",
            "poem",
            "--subarea",
            "business-systems",
        ],
        &[
            "new",
            "page",
            "--confirm-public",
            "cmp-system-review",
            "--type",
            "concept",
            "--subarea",
            "business-systems",
        ],
        &[
            "new",
            "claim",
            "--confirm-public",
            "--area",
            "cmp",
            "--source",
            "S-0002-v2",
            "--at",
            "nonsense",
            "--quote",
            "q",
        ],
    ];
    for args in cases {
        let out = run(&world, args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.stderr);
    }
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn templates_are_not_changed() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let templates = |root: &Path| -> BTreeMap<String, Vec<u8>> {
        corpus_files(root)
            .into_iter()
            .filter(|(path, _)| path.starts_with("corpus/templates/"))
            .collect()
    };
    let before = templates(tmp.path());
    assert!(!before.is_empty());

    for args in [
        PAGE_ARGS,
        CLAIM_ARGS,
        QUESTION_ARGS,
        CONFLICT_ARGS,
        TERM_ARGS,
    ] {
        assert_eq!(run(&world, args).code, 0);
    }
    assert_eq!(templates(tmp.path()), before);
    assert_eq!(templates(&fixture_root()), before);
}

/// Run `args` with the write to `fail_name` failing, leaving a pending write behind.
fn interrupt(root: &Path, args: &[&str], fail_name: &'static str) {
    let faulty = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: fail_name,
            kind: FaultKind::Io,
        },
    );
    let out = run(&faulty, args);
    assert_eq!(out.code, 3, "the run must be interrupted: {}", out.stderr);
    assert!(root.join(".fieldbook/pending.toml").exists());
}

/// The bytes a completed `args` run leaves in `rel`, from a run on a separate copy.
fn planned_bytes(args: &[&str], rel: &str) -> Vec<u8> {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    assert_eq!(run(&world, args).code, 0);
    read_bytes(tmp.path(), rel).expect("planned file")
}

#[test]
fn new_page_recovers_after_write_before_end() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let planned = planned_bytes(PAGE_ARGS, PAGE_PATH);
    interrupt(root, PAGE_ARGS, "cmp-synthetic-test.md");
    assert!(read_bytes(root, PAGE_PATH).is_none());

    // The crash came after the page was written and before write.end.
    std::fs::write(root.join(PAGE_PATH), &planned).expect("write planned page");
    let world = TestWorld::new(root);
    assert!(world.read_tree().is_err(), "pending generation is refused");

    // A changed request is refused while the original is pending.
    let changed = run(
        &world,
        &[
            "new",
            "page",
            "--confirm-public",
            "cmp-other",
            "--type",
            "concept",
            "--subarea",
            "business-systems",
        ],
    );
    assert_eq!(changed.code, 3, "{}", changed.stderr);
    // A third hash on disk is refused.
    let mut tampered = planned.clone();
    tampered.push(b'\n');
    std::fs::write(root.join(PAGE_PATH), &tampered).expect("tamper");
    assert_eq!(run(&world, PAGE_ARGS).code, 3);
    std::fs::write(root.join(PAGE_PATH), &planned).expect("restore");

    // The same request finishes the original transaction: no occupied-id error, and no content
    // write at all (a write to the page would fail here).
    let no_write = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: "cmp-synthetic-test.md",
            kind: FaultKind::Io,
        },
    );
    let recovered = run(&no_write, PAGE_ARGS);
    assert_eq!(recovered.code, 0, "{}", recovered.stderr);
    assert_eq!(recovered.stdout, format!("{PAGE_PATH}\n"));
    assert_eq!(
        read_bytes(root, PAGE_PATH).as_deref(),
        Some(planned.as_slice())
    );
    assert!(!root.join(".fieldbook/pending.toml").exists());
    assert!(world.read_tree().is_ok());
    assert_eq!(count(&world, "write.begin"), 1);
    assert_eq!(count(&world, "write.end"), 1);
    let page_files: Vec<String> = corpus_files(root)
        .into_keys()
        .filter(|p| p.contains("cmp-synthetic-test"))
        .collect();
    assert_eq!(page_files, vec![PAGE_PATH.to_owned()]);

    // Once complete, the id is occupied as usual.
    assert_eq!(run(&world, PAGE_ARGS).code, 2);
}

#[test]
fn new_page_recovers_when_the_write_never_happened() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    interrupt(root, PAGE_ARGS, "cmp-synthetic-test.md");
    let world = TestWorld::new(root);
    let recovered = run(&world, PAGE_ARGS);
    assert_eq!(recovered.code, 0, "{}", recovered.stderr);
    assert!(read_bytes(root, PAGE_PATH).is_some());
    assert_eq!(count(&world, "write.begin"), 1);
}

#[test]
fn new_row_recovery_keeps_the_original_id() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let rel = "corpus/registers/claims.csv";
    let planned = planned_bytes(CLAIM_ARGS, rel);
    interrupt(root, CLAIM_ARGS, "claims.csv");
    assert_eq!(
        read_bytes(root, rel),
        read_bytes(&fixture_root(), rel),
        "the register is as it was"
    );

    // The crash came after the register was written and before write.end. Allocating again
    // would now give CL-CMP-0004.
    std::fs::write(root.join(rel), &planned).expect("write planned register");
    let world = TestWorld::new(root);

    let changed = run(
        &world,
        &[
            "new",
            "claim",
            "--confirm-public",
            "--area",
            "cmp",
            "--source",
            "S-0002-v2",
            "--at",
            "p. 3",
            "--quote",
            "A different quote.",
        ],
    );
    assert_eq!(changed.code, 3, "{}", changed.stderr);
    // A third hash on disk (a register that still parses) is refused.
    let tampered = String::from_utf8(planned.clone())
        .expect("utf-8 register")
        .replacen("Synthetic", "Synthetik", 1);
    std::fs::write(root.join(rel), tampered).expect("tamper");
    assert_eq!(run(&world, CLAIM_ARGS).code, 3);
    std::fs::write(root.join(rel), &planned).expect("restore");

    // Finishing writes no content: a write to the register would fail here.
    let no_write = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: "claims.csv",
            kind: FaultKind::Io,
        },
    );
    let recovered = run(&no_write, CLAIM_ARGS);
    assert_eq!(recovered.code, 0, "{}", recovered.stderr);
    assert_eq!(recovered.stdout, "CL-CMP-0003\n");
    assert_eq!(read_bytes(root, rel).as_deref(), Some(planned.as_slice()));
    assert_eq!(
        ids(root, rel)
            .iter()
            .filter(|id| id.starts_with("CL-CMP-"))
            .count(),
        3
    );
    assert!(!root.join(".fieldbook/pending.toml").exists());
    assert!(world.read_tree().is_ok());
    assert_eq!(count(&world, "write.begin"), 1);
    assert_eq!(count(&world, "write.end"), 1);

    // The next claim takes the next id.
    assert_eq!(run(&world, CLAIM_ARGS).stdout, "CL-CMP-0004\n");
}

#[test]
fn new_row_recovery_before_the_write_keeps_the_original_id() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    interrupt(root, QUESTION_ARGS, "questions.csv");
    let world = TestWorld::new(root);
    let recovered = run(&world, QUESTION_ARGS);
    assert_eq!(recovered.code, 0, "{}", recovered.stderr);
    assert_eq!(recovered.stdout, "Q-003\n");
    assert_eq!(count(&world, "write.begin"), 1);
}

#[test]
fn new_claim_events_carry_the_affected_reference() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    assert_eq!(run(&world, CLAIM_ARGS).stdout, "CL-CMP-0003\n");

    let shown = audit_show_json(&world, &["--id", "CL-CMP-0003"]);
    let kinds: Vec<&str> = shown["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert_eq!(kinds, vec!["write.begin", "write.end"]);
}

#[test]
fn a_new_row_lands_in_sorted_position_without_an_fb0122() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let root = tmp.path();

    let term = run(&world, TERM_ARGS);
    assert_eq!(term.stdout, "cmp_term_1\n", "{}", term.stderr);
    assert_eq!(
        ids(root, "corpus/registers/glossary.csv"),
        [
            "billing_withhold",
            "cmp_term_1",
            "cost_baseline",
            "rate_window",
            "system_review"
        ]
    );

    // CMP claims sit before the EST rows already in the file.
    let claim = run(&world, CLAIM_ARGS);
    assert_eq!(claim.stdout, "CL-CMP-0003\n", "{}", claim.stderr);
    assert_eq!(
        ids(root, "corpus/registers/claims.csv"),
        [
            "CL-CMP-0001",
            "CL-CMP-0002",
            "CL-CMP-0003",
            "CL-EST-0001",
            "CL-EST-0002",
            "CL-EST-0003"
        ]
    );

    for rel in ["claims", "glossary"] {
        let bytes = read_bytes(root, &format!("corpus/registers/{rel}.csv")).expect("register");
        let normal = fieldbook_core::schema::table::normalize(
            if rel == "claims" {
                fieldbook_core::schema::Register::Claims
            } else {
                fieldbook_core::schema::Register::Glossary
            },
            &bytes,
        )
        .expect("normalize");
        assert_eq!(
            normal.as_bytes(),
            bytes.as_slice(),
            "{rel} is in normal form"
        );
    }
    let check = run(&world, &["check", "--strict", "--today", TODAY]);
    assert!(
        !check.stdout.contains("FB0122") && !check.stderr.contains("FB0122"),
        "stdout {} stderr {}",
        check.stdout,
        check.stderr
    );
}

#[test]
fn term_id_exhaustion_writes_nothing() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    append_row(root, "corpus/registers/glossary.csv", |cells| {
        cells[0] = "cmp_term_4294967295".to_owned();
    });
    let world = TestWorld::new(root);
    let before = corpus_files(root);

    let out = run(&world, TERM_ARGS);
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(
        out.stderr.contains("term id space exhausted"),
        "{}",
        out.stderr
    );
    assert_eq!(
        corpus_files(root),
        before,
        "the last row is not overwritten"
    );
    assert_eq!(count(&world, "write.begin"), 0);

    // A supplied id still works, and so does another area's series.
    let est = run(
        &world,
        &["new", "term", "--confirm-public", "--area", "est"],
    );
    assert_eq!(est.stdout, "est_term_1\n", "{}", est.stderr);
}

#[test]
fn id_exhaustion_writes_nothing() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    append_row(root, "corpus/registers/claims.csv", |cells| {
        cells[0] = "CL-CMP-99999".to_owned();
    });
    append_row(root, "corpus/registers/questions.csv", |cells| {
        cells[0] = "Q-999".to_owned();
    });
    append_row(root, "corpus/registers/conflicts.csv", |cells| {
        cells[0] = "XC-9999".to_owned();
    });
    let world = TestWorld::new(root);
    let before = corpus_files(root);

    let claim = run(&world, CLAIM_ARGS);
    assert_eq!(claim.code, 2, "{}", claim.stderr);
    assert!(
        claim.stderr.contains("claim id space exhausted"),
        "{}",
        claim.stderr
    );
    let question = run(&world, QUESTION_ARGS);
    assert_eq!(question.code, 2, "{}", question.stderr);
    assert!(
        question.stderr.contains("question id space exhausted"),
        "{}",
        question.stderr
    );
    assert_eq!(corpus_files(root), before);
    assert_eq!(count(&world, "write.begin"), 0);

    // The series are per area, and conflict ids have no four-digit cap.
    let est = run(
        &world,
        &[
            "new",
            "claim",
            "--confirm-public",
            "--area",
            "est",
            "--source",
            "S-0002-v2",
            "--at",
            "p. 3",
            "--quote",
            "synthetic",
        ],
    );
    assert_eq!(est.stdout, "CL-EST-0004\n");
    assert_eq!(run(&world, CONFLICT_ARGS).stdout, "XC-10000\n");
}

/// The five leaf commands, the file each writes and a way to put marked text in that file.
struct Leaf {
    name: &'static str,
    args: &'static [&'static str],
    target: &'static str,
    /// The basename of the write that is made to fail to interrupt the command.
    fail_name: &'static str,
}

const LEAVES: &[Leaf] = &[
    Leaf {
        name: "page",
        args: PAGE_ARGS,
        target: PAGE_PATH,
        fail_name: "cmp-synthetic-test.md",
    },
    Leaf {
        name: "claim",
        args: CLAIM_ARGS,
        target: "corpus/registers/claims.csv",
        fail_name: "claims.csv",
    },
    Leaf {
        name: "question",
        args: QUESTION_ARGS,
        target: "corpus/registers/questions.csv",
        fail_name: "questions.csv",
    },
    Leaf {
        name: "conflict",
        args: CONFLICT_ARGS,
        target: "corpus/registers/conflicts.csv",
        fail_name: "conflicts.csv",
    },
    Leaf {
        name: "term",
        args: TERM_ARGS,
        target: "corpus/registers/glossary.csv",
        fail_name: "glossary.csv",
    },
];

fn without_confirmation(args: &[&str]) -> Vec<String> {
    args.iter()
        .filter(|a| **a != "--confirm-public")
        .map(|a| (*a).to_owned())
        .collect()
}

/// Put marked text where the leaf will write: in the page template, or in its register.
fn mark_destination(root: &Path, leaf: &Leaf) {
    if leaf.name == "page" {
        let path = root.join("corpus/templates/concept.md");
        let text = std::fs::read_to_string(&path).expect("read template");
        let empty = fieldbook_core::strings::empty_part();
        let marked = text.replacen(empty, &format!("{BANNER} {empty}"), 1);
        std::fs::write(&path, marked).expect("write template");
    } else {
        mark_register(root, leaf.target);
    }
}

fn assert_refused(
    world: &dyn World,
    out: &fieldbook::Outcome,
    code: &str,
    reason: &str,
    refusals_before: usize,
) {
    assert_eq!(out.code, 1, "stdout {} stderr {}", out.stdout, out.stderr);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains(code), "stderr: {}", out.stderr);
    let refusals = events_of(world, "refuse");
    assert_eq!(
        refusals.len(),
        refusals_before + 1,
        "one refusal is recorded"
    );
    let last = refusals.last().expect("a refusal");
    assert_eq!(last["code"], code);
    assert_eq!(last["reason"], reason);
}

#[test]
fn new_admission_precedes_write_and_recovery() {
    for leaf in LEAVES {
        let name = leaf.name;

        // Public confirmation succeeds and labels the write public.
        {
            let tmp = temp_copy(&fixture_root());
            let world = TestWorld::new(tmp.path());
            let out = run(&world, leaf.args);
            assert_eq!(out.code, 0, "{name}: {}", out.stderr);
            let begin = events_of(&world, "write.begin");
            assert_eq!(begin.len(), 1, "{name}");
            assert_eq!(begin[0]["labels"], serde_json::json!(["public"]), "{name}");
        }

        // Missing confirmation: FB0410, reason unlabelled, nothing written, no write.begin.
        {
            let tmp = temp_copy(&fixture_root());
            let world = TestWorld::new(tmp.path());
            let before = corpus_files(tmp.path());
            let args = without_confirmation(leaf.args);
            let out = fieldbook::run(&args, &world);
            assert_refused(&world, &out, "FB0410", "unlabelled", 0);
            assert_eq!(corpus_files(tmp.path()), before, "{name}");
            assert_eq!(count(&world, "write.begin"), 0, "{name}");
        }

        // Marked content is refused even with the flag: FB0400, reason store.
        {
            let tmp = temp_copy(&fixture_root());
            mark_destination(tmp.path(), leaf);
            let world = TestWorld::new(tmp.path());
            let before = corpus_files(tmp.path());
            let out = run(&world, leaf.args);
            assert_refused(&world, &out, "FB0400", "store", 0);
            assert_eq!(corpus_files(tmp.path()), before, "{name}");
            assert_eq!(count(&world, "write.begin"), 0, "{name}");
        }

        // A company store refuses the typed act: FB0822, reason label.
        {
            let tmp = temp_copy(&fixture_root());
            let world = TestWorld::new(tmp.path());
            world.set_company_store(true);
            let before = corpus_files(tmp.path());
            let out = run(&world, leaf.args);
            assert_refused(&world, &out, "FB0822", "label", 0);
            assert_eq!(corpus_files(tmp.path()), before, "{name}");
            assert_eq!(count(&world, "write.begin"), 0, "{name}");
        }

        // The same checks run again before an interrupted original resumes.
        {
            let tmp = temp_copy(&fixture_root());
            let root = tmp.path();
            interrupt(root, leaf.args, leaf.fail_name);
            let on_disk = read_bytes(root, leaf.target);
            let world = TestWorld::new(root);
            assert_eq!(count(&world, "write.begin"), 1, "{name}");

            // Company store.
            world.set_company_store(true);
            let out = run(&world, leaf.args);
            assert_refused(&world, &out, "FB0822", "label", 0);
            world.set_company_store(false);
            assert_eq!(read_bytes(root, leaf.target), on_disk, "{name}");
            assert_eq!(count(&world, "write.begin"), 1, "{name}");
            assert!(root.join(".fieldbook/pending.toml").exists(), "{name}");

            // Marked content where the leaf will write: a register, or the page template.
            // A marked template also changes the request, so admission must come first.
            let template = root.join("corpus/templates/concept.md");
            let template_on_disk = std::fs::read(&template).expect("template");
            mark_destination(root, leaf);
            let marked = read_bytes(root, leaf.target);
            let out = run(&world, leaf.args);
            assert_refused(&world, &out, "FB0400", "store", 1);
            assert_eq!(read_bytes(root, leaf.target), marked, "{name}");
            assert_eq!(count(&world, "write.begin"), 1, "{name}");
            assert!(root.join(".fieldbook/pending.toml").exists(), "{name}");
            if name == "page" {
                std::fs::write(&template, &template_on_disk).expect("restore template");
            } else {
                std::fs::write(
                    root.join(leaf.target),
                    on_disk.as_ref().expect("register on disk"),
                )
                .expect("restore register");
            }

            // Missing confirmation changes the request, but admission refuses it first: FB0410
            // and a recorded refusal, not a pending-write mismatch.
            let before = read_bytes(root, leaf.target);
            let args = without_confirmation(leaf.args);
            let out = fieldbook::run(&args, &world);
            assert_refused(&world, &out, "FB0410", "unlabelled", 2);
            assert_eq!(read_bytes(root, leaf.target), before, "{name}");
            assert_eq!(count(&world, "write.begin"), 1, "{name}");
            assert!(root.join(".fieldbook/pending.toml").exists(), "{name}");

            // And the original still finishes once nothing is refused.
            let done = run(&world, leaf.args);
            assert_eq!(done.code, 0, "{name}: {}", done.stderr);
            assert_eq!(count(&world, "write.begin"), 1, "{name}");
            assert_eq!(count(&world, "write.end"), 1, "{name}");
        }
    }
}

/// Append a second `S-0002` version 2 row, with `handling` replaced, before or after the
/// original, so the version key repeats.
fn duplicate_source_row(root: &Path, handling: &str, before: bool) {
    let rel = "corpus/registers/sources.csv";
    let mut table = table_at(root, rel);
    let at = table
        .header
        .cells
        .iter()
        .position(|c| c == "handling")
        .expect("handling column");
    let found = table
        .rows
        .iter()
        .position(|r| r.cells.first().is_some_and(|c| c == "S-0002") && r.cells[1] == "2")
        .expect("S-0002 v2");
    let mut copy = table.rows[found].clone();
    copy.cells[at] = handling.to_owned();
    copy.line += 100;
    if before {
        table.rows.insert(found, copy);
    } else {
        table.rows.push(copy);
    }
    std::fs::write(root.join(rel), write_csv(&table)).expect("write sources");
}

#[test]
fn a_repeated_source_version_is_judged_on_every_row_in_either_order() {
    for before in [false, true] {
        let tmp = temp_copy(&fixture_root());
        duplicate_source_row(tmp.path(), "np-0007", before);
        let world = TestWorld::new(tmp.path());
        let files = corpus_files(tmp.path());
        let out = run(&world, CLAIM_ARGS);
        assert_refused(&world, &out, "FB0410", "unlabelled", 0);
        assert!(out.stderr.contains("np-0007"), "{}", out.stderr);
        assert_eq!(corpus_files(tmp.path()), files, "before={before}");
        assert_eq!(count(&world, "write.begin"), 0, "before={before}");
    }
}

#[test]
fn a_template_that_is_not_utf8_is_a_usage_error_and_writes_nothing() {
    let tmp = temp_copy(&fixture_root());
    let path = tmp.path().join("corpus/templates/concept.md");
    let text = std::fs::read(&path).expect("read template");
    let needle = b"Not est";
    let at = text
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("template placeholder text");
    let mut edited = text[..at].to_vec();
    edited.extend_from_slice(b"Not \xff est");
    edited.extend_from_slice(&text[at + needle.len()..]);
    std::fs::write(&path, edited).expect("write template");

    let world = TestWorld::new(tmp.path());
    let before = corpus_files(tmp.path());
    let out = run(&world, PAGE_ARGS);
    assert_eq!(out.code, 2, "stdout {} stderr {}", out.stdout, out.stderr);
    assert!(out.stderr.contains("UTF-8"), "{}", out.stderr);
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn a_register_saved_after_planning_is_not_overwritten() {
    let tmp = temp_copy(&fixture_root());
    let rel = "corpus/registers/questions.csv";
    let authored = "Q-777,cmp,Authored between planning and writing?,,,,,yes,false,open,,,\n";
    let world = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::EditAfterTreeRead {
            path: rel,
            append: authored,
        },
    );
    let out = run(&world, QUESTION_ARGS);
    assert_eq!(out.code, 3, "stdout {} stderr {}", out.stdout, out.stderr);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    assert!(out.stderr.contains(rel), "{}", out.stderr);
    assert!(out.stderr.contains("changed"), "{}", out.stderr);
    let saved = std::fs::read_to_string(tmp.path().join(rel)).expect("read register");
    assert!(saved.ends_with(authored), "the authored row survives");
    assert!(
        !saved.contains("Synthetic question"),
        "the new row is not written"
    );
    assert_eq!(count(&world, "write.begin"), 0);

    // Run again: nothing is pending, and the retry plans from the saved file.
    let again = TestWorld::new(tmp.path());
    let retry = run(&again, QUESTION_ARGS);
    assert_eq!(retry.code, 0, "stderr {}", retry.stderr);
    let saved = std::fs::read_to_string(tmp.path().join(rel)).expect("read register");
    assert!(saved.contains("Q-777"), "the authored row is kept");
    assert!(saved.contains("Synthetic question"), "the new row is added");
}

fn plant_claim_row(world: &TestWorld, root: &Path, at: &str, quote: &str) {
    let tree = world.read_tree().expect("read tree");
    let today = CivilDate::parse(TODAY).expect("valid today");
    let (model, _) = check::load(&Host::new(tree, today), &Options::new(false));
    let area = AreaCode::parse("cmp").expect("valid area");
    let fields = Fields::new()
        .with("area", "cmp")
        .with("source", "S-0001-v1")
        .with("at", at)
        .with("quote", quote)
        .with("drafted_by", "codex");
    let mut plan = plan_row(&model, RowKind::Claim, Some(&area), &fields).expect("plan_row");
    plan.cells[3] = "Synthetic statement.".to_owned();
    let bytes = render_register(&model, RowKind::Claim, &plan).expect("render_register");
    std::fs::write(root.join("corpus/registers/claims.csv"), bytes).expect("write claims.csv");
}

fn diagnostic_block(check_stdout: &str, code: &str) -> String {
    let lines: Vec<&str> = check_stdout.lines().collect();
    let pos = lines
        .iter()
        .position(|l| l.contains(&format!("[{code}]")))
        .unwrap_or_else(|| panic!("no diagnostic for {code} in:\n{check_stdout}"));
    format!("{}\n{}\n", lines[pos], lines[pos + 1])
}

#[test]
fn new_claim_refuses_unmatched_quote() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let before = corpus_files(tmp.path());

    let unmatched_args = &[
        "new",
        "claim",
        "--confirm-public",
        "--area",
        "cmp",
        "--source",
        "S-0001-v1",
        "--at",
        "sec. 1",
        "--quote",
        "Synthetic compliance reviews occur every 31 days.",
    ];
    let out = run(&world, unmatched_args);
    assert_eq!(out.code, 1, "stdout: {} stderr: {}", out.stdout, out.stderr);
    assert!(out.stdout.contains("FB0308"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("claims.csv"), "stdout: {}", out.stdout);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(corpus_files(tmp.path()), before);
    assert_eq!(count(&world, "write.begin"), 0);

    let matched_args = &[
        "new",
        "claim",
        "--confirm-public",
        "--area",
        "cmp",
        "--source",
        "S-0001-v1",
        "--at",
        "sec. 1",
        "--quote",
        "Synthetic compliance reviews occur every 30 days.",
    ];
    let out_ok = run(&world, matched_args);
    assert_eq!(
        out_ok.code, 0,
        "stdout: {} stderr: {}",
        out_ok.stdout, out_ok.stderr
    );
    assert_eq!(out_ok.stdout, "CL-CMP-0003\n");
    assert!(out_ok.stderr.is_empty(), "stderr: {}", out_ok.stderr);
    assert_eq!(count(&world, "write.begin"), 1);
    let claim = row(tmp.path(), "corpus/registers/claims.csv", "CL-CMP-0003");
    assert_eq!(
        claim["quote"],
        "Synthetic compliance reviews occur every 30 days."
    );
}

#[test]
fn new_claim_and_check_give_the_same_diagnostic() {
    // Case 1: a quote that does not match (FB0308)
    {
        let tmp1 = temp_copy(&fixture_root());
        let world1 = TestWorld::new(tmp1.path());
        let out1 = run(
            &world1,
            &[
                "new",
                "claim",
                "--confirm-public",
                "--area",
                "cmp",
                "--source",
                "S-0001-v1",
                "--at",
                "sec. 1",
                "--quote",
                "Synthetic compliance reviews occur every 31 days.",
            ],
        );
        assert_eq!(out1.code, 1);
        assert!(out1.stdout.contains("FB0308"));
        let d = out1.stdout;

        let tmp2 = temp_copy(&fixture_root());
        let world2 = TestWorld::new(tmp2.path());
        plant_claim_row(
            &world2,
            tmp2.path(),
            "sec. 1",
            "Synthetic compliance reviews occur every 31 days.",
        );
        let check_out = run(&world2, &["check"]);
        let block = diagnostic_block(&check_out.stdout, "FB0308");
        assert_eq!(block, d);
    }

    // Case 2: a locator that does not resolve (FB0304)
    // 2a: --at "p. 4" (page beyond page count)
    {
        let tmp1 = temp_copy(&fixture_root());
        let world1 = TestWorld::new(tmp1.path());
        let out1 = run(
            &world1,
            &[
                "new",
                "claim",
                "--confirm-public",
                "--area",
                "cmp",
                "--source",
                "S-0001-v1",
                "--at",
                "p. 4",
                "--quote",
                "Synthetic compliance reviews occur every 30 days.",
            ],
        );
        assert_eq!(out1.code, 1);
        assert!(out1.stdout.contains("FB0304"));
        let d = out1.stdout;

        let tmp2 = temp_copy(&fixture_root());
        let world2 = TestWorld::new(tmp2.path());
        plant_claim_row(
            &world2,
            tmp2.path(),
            "p. 4",
            "Synthetic compliance reviews occur every 30 days.",
        );
        let check_out = run(&world2, &["check"]);
        let block = diagnostic_block(&check_out.stdout, "FB0304");
        assert_eq!(block, d);
    }

    // 2b: --at "sec. 9" (label not in [locators])
    {
        let tmp1 = temp_copy(&fixture_root());
        let world1 = TestWorld::new(tmp1.path());
        let out1 = run(
            &world1,
            &[
                "new",
                "claim",
                "--confirm-public",
                "--area",
                "cmp",
                "--source",
                "S-0001-v1",
                "--at",
                "sec. 9",
                "--quote",
                "Synthetic compliance reviews occur every 30 days.",
            ],
        );
        assert_eq!(out1.code, 1);
        assert!(out1.stdout.contains("FB0304"));
        let d = out1.stdout;

        let tmp2 = temp_copy(&fixture_root());
        let world2 = TestWorld::new(tmp2.path());
        plant_claim_row(
            &world2,
            tmp2.path(),
            "sec. 9",
            "Synthetic compliance reviews occur every 30 days.",
        );
        let check_out = run(&world2, &["check"]);
        let block = diagnostic_block(&check_out.stdout, "FB0304");
        assert_eq!(block, d);
    }

    // Case 3: a quote that matches
    {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let out = run(
            &world,
            &[
                "new",
                "claim",
                "--confirm-public",
                "--area",
                "cmp",
                "--source",
                "S-0001-v1",
                "--at",
                "sec. 1",
                "--quote",
                "Synthetic compliance reviews occur every 30 days.",
            ],
        );
        assert_eq!(out.code, 0, "stdout: {} stderr: {}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "CL-CMP-0003\n");
        let check_out = run(&world, &["check"]);
        assert!(
            !check_out.stdout.contains("FB0304"),
            "stdout: {}",
            check_out.stdout
        );
        assert!(
            !check_out.stdout.contains("FB0308"),
            "stdout: {}",
            check_out.stdout
        );
    }
}
