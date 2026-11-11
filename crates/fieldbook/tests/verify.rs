//! Integration tests for `fieldbook verify` (`verify-status-fmt`), each on a temporary copy of
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
use fieldbook_core::audit::Role;
use fieldbook_core::check::{Host, Options, load};
use fieldbook_core::schema::table::{Table, read as read_csv, write as write_csv};
use fieldbook_core::verify::Refusal;
use serde_json::Value;

use fieldbook::io::journal::set_fail_after_end_append;
use support::faulty::{Fault, FaultKind, FaultyWorld};
use support::temp_copy;
use support::world::TestWorld;

const TODAY: &str = "2026-03-01";

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

fn world_with_role(root: &Path, role: Option<&str>) -> TestWorld {
    let state_dir = root.join(".fieldbook");
    std::fs::create_dir_all(&state_dir).expect("create .fieldbook");
    let content = match role {
        Some(r) => format!("role = \"{r}\"\n"),
        None => String::new(),
    };
    std::fs::write(state_dir.join("settings.toml"), content).expect("write settings.toml");
    let world = TestWorld::new(root);
    world.set_stdin_terminal(true);
    world
}

fn answer(world: &TestWorld, line: &str) {
    world.push_input_line(line);
}

fn verify_args<'a>(claim: &'a str, on: &'a str) -> Vec<&'a str> {
    vec!["verify", claim, "--on", on]
}

fn claims_bytes(root: &Path) -> Option<Vec<u8>> {
    read_bytes(root, "corpus/registers/claims.csv")
}

fn edit_cell(root: &Path, rel: &str, key: &str, column: &str, value: &str) {
    let mut table = table_at(root, rel);
    let col_idx = table
        .header
        .cells
        .iter()
        .position(|c| c == column)
        .unwrap_or_else(|| panic!("column `{column}` not in {rel}"));
    let target_row = table
        .rows
        .iter_mut()
        .find(|r| r.cells.first().is_some_and(|c| c == key))
        .unwrap_or_else(|| panic!("no row `{key}` in {rel}"));
    if col_idx < target_row.cells.len() {
        target_row.cells[col_idx] = value.to_owned();
    } else {
        target_row.cells.resize(col_idx + 1, String::new());
        target_row.cells[col_idx] = value.to_owned();
    }
    std::fs::write(root.join(rel), write_csv(&table)).expect("write register");
}

#[test]
fn yes_writes_exactly_the_three_cells() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let before_table = table_at(root, "corpus/registers/claims.csv");

    let world = world_with_role(root, Some("owner"));
    answer(&world, "yes");

    let out = run(&world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.lines().count(), 1);
    assert!(out.stdout.contains("CL-CMP-0002"));
    assert!(out.stdout.contains(TODAY));
    assert!(
        out.stderr.is_empty(),
        "expected empty stderr, got: {}",
        out.stderr
    );

    let prompts = world.prompts();
    assert_eq!(prompts.len(), 1, "prompt must be flushed exactly once");
    let p = &prompts[0];
    assert!(p.contains("CL-CMP-0002"));
    assert!(p.contains("Synthetic withholding threshold is 5 percent of billed amounts."));
    assert!(p.contains("sec. 2"));
    assert!(p.contains("Synthetic withholding threshold is 5 percent."));
    assert!(p.contains("S-0001-v1"));
    let trimmed_prompt = p.trim_end();
    assert!(
        trimmed_prompt.ends_with("Type yes") || trimmed_prompt.ends_with("Type yes:"),
        "prompt must end with 'Type yes', got: {trimmed_prompt:?}"
    );

    let after_table = table_at(root, "corpus/registers/claims.csv");
    assert_eq!(before_table.rows.len(), after_table.rows.len());
    let header = &before_table.header.cells;
    for (b_row, a_row) in before_table.rows.iter().zip(&after_table.rows) {
        let id = b_row.cells.first().expect("id cell");
        for (idx, col_name) in header.iter().enumerate() {
            let b_val = &b_row.cells[idx];
            let a_val = &a_row.cells[idx];
            if id == "CL-CMP-0002" && col_name == "verification" {
                assert_eq!(a_val, "primary_source_supported");
                assert_ne!(b_val, a_val);
            } else if id == "CL-CMP-0002" && col_name == "verified_on" {
                assert_eq!(a_val, TODAY);
                assert_ne!(b_val, a_val);
            } else if id == "CL-CMP-0002" && col_name == "verified_by_role" {
                assert_eq!(a_val, "owner");
                assert_ne!(b_val, a_val);
            } else {
                assert_eq!(
                    b_val, a_val,
                    "cell mismatch for row {id} in column {col_name}: before={b_val:?}, after={a_val:?}"
                );
            }
        }
    }
}

#[test]
fn no_writes_nothing() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let before_bytes = claims_bytes(root);

    let world = world_with_role(root, Some("owner"));
    answer(&world, "no");

    let out = run(&world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out.code, 1);
    assert_eq!(claims_bytes(root), before_bytes);
    assert_eq!(count(&world, "write.begin"), 0);

    // Empty line (no input) behaves the same.
    let tmp2 = temp_copy(&fixture_root());
    let root2 = tmp2.path();
    let before_bytes2 = claims_bytes(root2);

    let world2 = world_with_role(root2, Some("owner"));
    answer(&world2, "");

    let out2 = run(&world2, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out2.code, 1);
    assert_eq!(claims_bytes(root2), before_bytes2);
    assert_eq!(count(&world2, "write.begin"), 0);
}

#[test]
fn stdin_not_a_terminal_exits_2_and_writes_nothing() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let before = corpus_files(root);

    let world = TestWorld::new(root);
    let out = run(&world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out.code, 2);
    assert!(
        out.stderr
            .contains("verify needs a terminal; it is for the owner at home")
    );
    assert!(out.stdout.is_empty());
    assert_eq!(corpus_files(root), before);
    assert!(!root.join(".fieldbook").exists());
}

#[test]
fn verify_without_a_role_prints_the_line_and_exits_2() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let before = corpus_files(root);

    let world = world_with_role(root, None);
    let out = run(&world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out.code, 2);
    assert!(
        out.stderr
            .contains("verify needs a role: add the line role = \"owner\"")
    );
    assert!(out.stderr.contains("or `role = \"lead\"`"));
    assert!(out.stdout.is_empty());
    assert_eq!(corpus_files(root), before);
    assert!(world.prompts().is_empty());
    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn an_unknown_id_exits_1() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let before = corpus_files(root);

    let world = world_with_role(root, Some("owner"));
    let out = run(&world, &verify_args("CL-CMP-9999", TODAY));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("no claim"));
    assert_eq!(corpus_files(root), before);
    assert!(world.prompts().is_empty());
}

#[test]
fn a_tier_c_source_is_refused() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    edit_cell(
        root,
        "corpus/registers/sources.csv",
        "S-0001",
        "authority_tier",
        "C",
    );
    let before = corpus_files(root);

    let world = world_with_role(root, Some("owner"));
    let out = run(&world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("FB0209"));
    assert!(out.stderr.contains("cannot supply primary support"));
    assert!(world.prompts().is_empty());
    assert_eq!(corpus_files(root), before);
}

#[test]
fn a_consequential_claim_needs_the_owner() {
    // 1. Role lead on consequential CL-CMP-0002: exit 1.
    let tmp1 = temp_copy(&fixture_root());
    let root1 = tmp1.path();
    let before_claims1 = claims_bytes(root1);
    let world1 = world_with_role(root1, Some("lead"));
    let out1 = run(&world1, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out1.code, 1);
    assert!(out1.stderr.contains("FB0209"));
    assert!(out1.stderr.contains("owner"));
    assert!(world1.prompts().is_empty());
    assert_eq!(claims_bytes(root1), before_claims1);

    // 2. Fresh copy with role owner: exit 0.
    let tmp2 = temp_copy(&fixture_root());
    let root2 = tmp2.path();
    let world2 = world_with_role(root2, Some("owner"));
    answer(&world2, "yes");
    let out2 = run(&world2, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out2.code, 0, "stderr: {}", out2.stderr);

    // 3. Fresh copy, non-consequential, role lead: exit 0, verified_by_role = "lead".
    let tmp3 = temp_copy(&fixture_root());
    let root3 = tmp3.path();
    edit_cell(
        root3,
        "corpus/registers/claims.csv",
        "CL-CMP-0002",
        "consequential",
        "false",
    );
    let world3 = world_with_role(root3, Some("lead"));
    answer(&world3, "yes");
    let out3 = run(&world3, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out3.code, 0, "stderr: {}", out3.stderr);
    let r3 = row(root3, "corpus/registers/claims.csv", "CL-CMP-0002");
    assert_eq!(r3["verified_by_role"], "lead");

    // 4. Fresh copy, non-consequential, role owner: exit 0, verified_by_role = "owner".
    let tmp4 = temp_copy(&fixture_root());
    let root4 = tmp4.path();
    edit_cell(
        root4,
        "corpus/registers/claims.csv",
        "CL-CMP-0002",
        "consequential",
        "false",
    );
    let world4 = world_with_role(root4, Some("owner"));
    answer(&world4, "yes");
    let out4 = run(&world4, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out4.code, 0, "stderr: {}", out4.stderr);
    let r4 = row(root4, "corpus/registers/claims.csv", "CL-CMP-0002");
    assert_eq!(r4["verified_by_role"], "owner");
}

#[test]
fn an_impossible_date_exits_2() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let before = corpus_files(root);

    let world = world_with_role(root, Some("owner"));
    answer(&world, "yes");

    let out = run(&world, &verify_args("CL-CMP-0002", "2026-02-30"));
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty());
    assert_eq!(corpus_files(root), before);
    assert!(world.prompts().is_empty());
    assert_eq!(count(&world, "write.begin"), 0);
    assert_eq!(count(&world, "write.end"), 0);
}

#[test]
fn verify_records_role_in_the_journal() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    edit_cell(
        root,
        "corpus/registers/claims.csv",
        "CL-CMP-0002",
        "consequential",
        "false",
    );

    let world = world_with_role(root, Some("lead"));
    answer(&world, "yes");

    let out = run(&world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let begins = events_of(&world, "write.begin");
    let ends = events_of(&world, "write.end");
    assert_eq!(begins.len(), 1);
    assert_eq!(ends.len(), 1);

    assert_eq!(begins[0]["verb"], "verify");
    assert_eq!(begins[0]["role"], "lead");
    assert_eq!(begins[0]["labels"], serde_json::json!(["public"]));

    assert_eq!(ends[0]["verb"], "verify");
    assert_eq!(ends[0]["role"], "lead");
    assert_eq!(ends[0]["labels"], serde_json::json!(["public"]));

    let verify_audit = run(&world, &["audit", "verify"]);
    assert_eq!(
        verify_audit.code, 0,
        "audit verify stderr: {}",
        verify_audit.stderr
    );
    assert!(verify_audit.stdout.contains("journal ok"));

    let by_id = audit_show_json(&world, &["--id", "CL-CMP-0002"]);
    let kinds: Vec<&str> = by_id["events"]
        .as_array()
        .expect("events array")
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert_eq!(kinds, vec!["write.begin", "write.end"]);
}

#[test]
fn check_strict_is_clean_after_yes() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let world = world_with_role(root, Some("owner"));

    let check_args = ["check", "--strict", "--today", TODAY];
    let before_check = run(&world, &check_args);
    assert_eq!(
        before_check.code, 0,
        "initial check stderr: {}",
        before_check.stderr
    );
    let before_stdout = before_check.stdout;

    answer(&world, "yes");
    let verify_out = run(&world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(verify_out.code, 0, "verify stderr: {}", verify_out.stderr);

    let after_check = run(&world, &check_args);
    assert_eq!(
        after_check.code, 0,
        "subsequent check stderr: {}",
        after_check.stderr
    );
    assert_eq!(after_check.stdout, before_stdout);
}

#[test]
fn each_instrument_needs_its_numbering_cell_and_permits_the_other_empty() {
    let cases = [
        ("far_codified", "citation_codified"),
        ("dfars_codified", "citation_codified"),
        ("cas", "citation_codified"),
        ("federal_register_notice", "citation_codified"),
        ("statute", "citation_codified"),
        ("rfo_model_text", "citation_rfo"),
        ("class_deviation", "citation_rfo"),
        ("proposed_rule", "citation_rfo"),
    ];

    for (inst, req_col) in cases {
        // Step 1: missing required numbering cell refuses before confirmation or write.
        let tmp = temp_copy(&fixture_root());
        let root = tmp.path();
        edit_cell(
            root,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "instrument",
            inst,
        );
        edit_cell(
            root,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "effective_from",
            "2026-01-15",
        );
        edit_cell(
            root,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "text_status",
            "final",
        );
        edit_cell(
            root,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "citation_codified",
            "",
        );
        edit_cell(
            root,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "citation_rfo",
            "",
        );
        let before_bytes = claims_bytes(root);

        let world = world_with_role(root, Some("owner"));
        answer(&world, "yes");

        let out = run(&world, &verify_args("CL-CMP-0002", TODAY));
        assert_eq!(
            out.code, 1,
            "instrument {inst} without {req_col} should exit 1"
        );
        assert!(
            out.stderr.contains("FB0209"),
            "missing FB0209 for {inst}: {}",
            out.stderr
        );
        assert!(
            out.stderr.contains(req_col),
            "missing {req_col} in error for {inst}: {}",
            out.stderr
        );
        assert!(world.prompts().is_empty());
        assert_eq!(count(&world, "write.begin"), 0);
        assert_eq!(claims_bytes(root), before_bytes);

        // Step 2: setting required numbering cell (other remains empty) verifies cleanly.
        let tmp2 = temp_copy(&fixture_root());
        let root2 = tmp2.path();
        edit_cell(
            root2,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "instrument",
            inst,
        );
        edit_cell(
            root2,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "effective_from",
            "2026-01-15",
        );
        edit_cell(
            root2,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "text_status",
            "final",
        );
        edit_cell(
            root2,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "citation_codified",
            "",
        );
        edit_cell(
            root2,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "citation_rfo",
            "",
        );
        edit_cell(
            root2,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            req_col,
            "Synthetic cite 1",
        );

        let world2 = world_with_role(root2, Some("owner"));
        answer(&world2, "yes");

        let out2 = run(&world2, &verify_args("CL-CMP-0002", TODAY));
        assert_eq!(
            out2.code, 0,
            "instrument {inst} with {req_col} failed: {}",
            out2.stderr
        );
    }

    // Non-regulatory claim (no instrument) with both citation cells empty exits 0:
    // this baseline fixture CL-CMP-0002 case is verified in `yes_writes_exactly_the_three_cells`.
}

#[test]
fn verify_refuses_unsupported_extension() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();

    edit_cell(
        root,
        "corpus/registers/claims.csv",
        "CL-CMP-0002",
        "locator_kind",
        "ext:synthetic_kind",
    );
    edit_cell(
        root,
        "corpus/registers/claims.csv",
        "CL-CMP-0002",
        "locator",
        "ext synthetic_kind synthetic_payload",
    );

    let world = world_with_role(root, Some("owner"));
    let tree = world.read_tree().expect("read tree");
    let on = CivilDate::parse(TODAY).expect("valid today");
    let (model, _report) = load(&Host::new(tree, on), &Options::new(false));
    assert!(
        model
            .claims()
            .iter()
            .any(|c| c.value.claim_id.to_string() == "CL-CMP-0002"),
        "CL-CMP-0002 must load into Model"
    );

    // Refusal::Rule naming `an available fine locator`
    let plan_res = fieldbook_core::verify::plan(&model, "CL-CMP-0002", on, Role::Owner);
    match plan_res {
        Err(Refusal::Rule(problems)) => {
            let combined = problems
                .iter()
                .map(|p| p.message.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                combined.contains("an available fine locator"),
                "expected 'an available fine locator' in rule refusal, got: {combined}"
            );
        }
        other => panic!("expected Err(Refusal::Rule(..)), got {other:?}"),
    }

    // `shown` preserves kind and payload in `Shown`
    let shown = fieldbook_core::verify::shown(&model, "CL-CMP-0002", on).expect("shown succeeds");
    assert!(
        shown.locator_kind.contains("synthetic_kind"),
        "locator_kind should contain 'synthetic_kind', got: {}",
        shown.locator_kind
    );
    assert!(
        shown.locator.contains("synthetic_payload"),
        "locator should contain payload text, got: {}",
        shown.locator
    );
}

/// Run `fieldbook verify` with the write to `fail_name` failing, leaving a pending write behind.
fn interrupt_verify(root: &Path, fail_name: &'static str) {
    let world = world_with_role(root, Some("owner"));
    answer(&world, "yes");
    let faulty = FaultyWorld::new(
        world,
        Fault::FailOnName {
            name: fail_name,
            kind: FaultKind::Io,
        },
    );
    let out = run(&faulty, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out.code, 3, "the run must be interrupted: {}", out.stderr);
    assert!(
        root.join(".fieldbook/pending.toml").exists(),
        "pending.toml must exist"
    );
}

/// The bytes a completed verify leaves in `claims.csv`, from a run on a separate temp copy.
fn planned_claims(claim: &str, on: &str) -> Vec<u8> {
    let tmp = temp_copy(&fixture_root());
    let world = world_with_role(tmp.path(), Some("owner"));
    answer(&world, "yes");
    let out = run(&world, &verify_args(claim, on));
    assert_eq!(out.code, 0, "planned verify failed: {}", out.stderr);
    claims_bytes(tmp.path()).expect("planned claims.csv")
}

#[test]
fn verify_prompt_holds_no_lease() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let world = world_with_role(root, Some("owner"));
    world.set_wait_for_input(true);

    let verify_out = std::thread::scope(|s| {
        let handle = s.spawn(|| run(&world, &verify_args("CL-CMP-0002", TODAY)));

        assert!(
            world.wait_for_prompts(1),
            "prompt must be flushed while waiting for input"
        );

        let other_writer = run(
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
                "Another writer",
            ],
        );
        assert_eq!(
            other_writer.code, 0,
            "another writer must exit 0: {}",
            other_writer.stderr
        );

        answer(&world, "yes");
        handle.join().expect("join verify thread")
    });

    assert_eq!(verify_out.code, 0, "verify stderr: {}", verify_out.stderr);

    let other_claim = row(root, "corpus/registers/claims.csv", "CL-CMP-0003");
    assert_eq!(other_claim["quote"], "Another writer");

    let verified_claim = row(root, "corpus/registers/claims.csv", "CL-CMP-0002");
    assert_eq!(verified_claim["verification"], "primary_source_supported");
    assert_eq!(verified_claim["verified_on"], TODAY);
    assert_eq!(verified_claim["verified_by_role"], "owner");

    assert_eq!(count(&world, "write.begin"), 2);
    assert_eq!(count(&world, "write.end"), 2);
}

#[test]
fn changed_evidence_after_prompt_is_refused() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let world = world_with_role(root, Some("owner"));
    world.set_wait_for_input(true);

    let verify_out = std::thread::scope(|s| {
        let handle = s.spawn(|| run(&world, &verify_args("CL-CMP-0002", TODAY)));

        assert!(
            world.wait_for_prompts(1),
            "prompt must be flushed while waiting for input"
        );

        edit_cell(
            root,
            "corpus/registers/claims.csv",
            "CL-CMP-0002",
            "quote",
            "Evidence changed while prompt waited.",
        );

        answer(&world, "yes");
        handle.join().expect("join verify thread")
    });

    assert_eq!(verify_out.code, 1);
    assert_eq!(verify_out.stderr, "claim changed; run verify again\n");
    assert!(verify_out.stdout.is_empty(), "stdout must be empty");

    let claim = row(root, "corpus/registers/claims.csv", "CL-CMP-0002");
    assert_eq!(claim["verification"], "unchecked");
    assert_eq!(claim["verified_on"], "");
    assert_eq!(claim["verified_by_role"], "");

    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn changed_role_after_prompt_is_refused() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let world = world_with_role(root, Some("owner"));
    world.set_wait_for_input(true);

    let verify_out = std::thread::scope(|s| {
        let handle = s.spawn(|| run(&world, &verify_args("CL-CMP-0002", TODAY)));

        assert!(
            world.wait_for_prompts(1),
            "prompt must be flushed while waiting for input"
        );

        std::fs::write(root.join(".fieldbook/settings.toml"), "role = \"lead\"\n")
            .expect("rewrite settings.toml");

        answer(&world, "yes");
        handle.join().expect("join verify thread")
    });

    assert_eq!(verify_out.code, 1);
    assert_eq!(verify_out.stderr, "claim changed; run verify again\n");
    assert!(verify_out.stdout.is_empty(), "stdout must be empty");

    let claim = row(root, "corpus/registers/claims.csv", "CL-CMP-0002");
    assert_eq!(claim["verification"], "unchecked");
    assert_eq!(claim["verified_on"], "");
    assert_eq!(claim["verified_by_role"], "");

    assert_eq!(count(&world, "write.begin"), 0);
}

#[test]
fn verify_recovers_when_the_write_never_happened() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let planned = planned_claims("CL-CMP-0002", TODAY);
    let before_claims = claims_bytes(root).expect("initial claims");

    interrupt_verify(root, "claims.csv");
    assert_eq!(
        claims_bytes(root).as_deref(),
        Some(before_claims.as_slice()),
        "claims.csv must be unchanged after interrupted write"
    );

    let world = world_with_role(root, Some("owner"));
    assert!(
        world.read_tree().is_err(),
        "pending generation must refuse ordinary read_tree"
    );

    answer(&world, "yes");
    let recovered = run(&world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(recovered.code, 0, "rerun stderr: {}", recovered.stderr);
    assert_eq!(world.prompts().len(), 1, "rerun must prompt again");
    assert_eq!(claims_bytes(root).as_deref(), Some(planned.as_slice()));
    assert!(
        !root.join(".fieldbook/pending.toml").exists(),
        "pending.toml must be removed"
    );
    assert!(world.read_tree().is_ok(), "read_tree must succeed");
    assert_eq!(count(&world, "write.begin"), 1);
    assert_eq!(count(&world, "write.end"), 1);

    let audit_out = run(&world, &["audit", "verify"]);
    assert_eq!(
        audit_out.code, 0,
        "audit verify stderr: {}",
        audit_out.stderr
    );
}

#[test]
fn verify_recovers_after_the_csv_write() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let planned = planned_claims("CL-CMP-0002", TODAY);

    interrupt_verify(root, "claims.csv");

    std::fs::write(root.join("corpus/registers/claims.csv"), &planned)
        .expect("write planned claims");

    let world = world_with_role(root, Some("owner"));
    let faulty = FaultyWorld::new(
        world,
        Fault::FailOnName {
            name: "claims.csv",
            kind: FaultKind::Io,
        },
    );

    let out = run(&faulty, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out.code, 0, "rerun stderr: {}", out.stderr);
    assert!(faulty.inner.prompts().is_empty(), "prompts must be empty");
    assert!(
        out.stdout.contains("CL-CMP-0002") && out.stdout.contains(TODAY),
        "the claim line must be printed, got: {}",
        out.stdout
    );
    assert_eq!(count(&faulty, "write.begin"), 1);
    assert_eq!(count(&faulty, "write.end"), 1);
    assert!(
        !root.join(".fieldbook/pending.toml").exists(),
        "pending.toml must be removed"
    );
    assert_eq!(claims_bytes(root).as_deref(), Some(planned.as_slice()));
    assert!(faulty.read_tree().is_ok(), "read_tree must succeed");

    let audit_out = run(&faulty, &["audit", "verify"]);
    assert_eq!(
        audit_out.code, 0,
        "audit verify stderr: {}",
        audit_out.stderr
    );
}

#[test]
fn verify_recovers_during_completion() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();

    let world = world_with_role(root, Some("owner"));
    answer(&world, "yes");

    // Injected fault in journal.end after write.end is appended and before pending.toml is cleared.
    set_fail_after_end_append(true);
    let out = run(&world, &verify_args("CL-CMP-0002", TODAY));
    set_fail_after_end_append(false);

    assert_eq!(out.code, 3, "expected failure exit code 3: {}", out.stderr);
    assert!(
        root.join(".fieldbook/pending.toml").exists(),
        "pending.toml must remain in place after completion failure"
    );

    let rerun_world = world_with_role(root, Some("owner"));
    let rerun_out = run(&rerun_world, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(rerun_out.code, 0, "rerun stderr: {}", rerun_out.stderr);
    assert!(
        rerun_world.prompts().is_empty(),
        "clean rerun needs no prompt/answer"
    );

    assert_eq!(count(&rerun_world, "write.begin"), 1);
    assert_eq!(count(&rerun_world, "write.end"), 1);

    let r = row(root, "corpus/registers/claims.csv", "CL-CMP-0002");
    assert_eq!(r["verification"], "primary_source_supported");
    assert_eq!(r["verified_on"], TODAY);
    assert_eq!(r["verified_by_role"], "owner");

    let table = table_at(root, "corpus/registers/claims.csv");
    let match_count = table
        .rows
        .iter()
        .filter(|row| row.cells.first().is_some_and(|c| c == "CL-CMP-0002"))
        .count();
    assert_eq!(match_count, 1, "exactly one row for CL-CMP-0002");

    assert!(
        !root.join(".fieldbook/pending.toml").exists(),
        "pending.toml must be removed"
    );
    assert!(rerun_world.read_tree().is_ok(), "read_tree must succeed");

    let audit_out = run(&rerun_world, &["audit", "verify"]);
    assert_eq!(
        audit_out.code, 0,
        "audit verify stderr: {}",
        audit_out.stderr
    );
}

#[test]
fn a_changed_request_is_refused_while_a_verification_is_pending() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let planned = planned_claims("CL-CMP-0002", TODAY);
    let before_claims = claims_bytes(root).expect("initial claims");

    interrupt_verify(root, "claims.csv");
    assert!(root.join(".fieldbook/pending.toml").exists());

    // 1. Rerun with a different date: exit 1, claims unchanged, pending.toml still there.
    let world_date = world_with_role(root, Some("owner"));
    answer(&world_date, "yes");
    let out_date = run(&world_date, &verify_args("CL-CMP-0002", "2026-02-28"));
    assert_eq!(out_date.code, 1);
    assert!(
        out_date.stderr.contains("pending with a different request"),
        "stderr: {}",
        out_date.stderr
    );
    assert_eq!(
        claims_bytes(root).as_deref(),
        Some(before_claims.as_slice())
    );
    assert!(root.join(".fieldbook/pending.toml").exists());

    // 2. Rerun with role changed to lead: exit 1, nothing written, pending.toml still there.
    std::fs::write(root.join(".fieldbook/settings.toml"), "role = \"lead\"\n")
        .expect("write settings.toml");
    let world_lead = world_with_role(root, Some("lead"));
    answer(&world_lead, "yes");
    let out_lead = run(&world_lead, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out_lead.code, 1);
    assert_eq!(
        claims_bytes(root).as_deref(),
        Some(before_claims.as_slice())
    );
    assert!(root.join(".fieldbook/pending.toml").exists());

    // 3. With role agent (empty role): exit 2 and role message.
    let world_agent = world_with_role(root, None);
    let out_agent = run(&world_agent, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(out_agent.code, 2);
    assert!(
        out_agent
            .stderr
            .contains("verify needs a role: add the line role = \"owner\""),
        "stderr: {}",
        out_agent.stderr
    );
    assert_eq!(
        claims_bytes(root).as_deref(),
        Some(before_claims.as_slice())
    );
    assert!(root.join(".fieldbook/pending.toml").exists());

    // 4. A third hash on disk after writing planned bytes is refused.
    let world_third = world_with_role(root, Some("owner"));
    let mut tampered = planned.clone();
    tampered.push(b'\n');
    std::fs::write(root.join("corpus/registers/claims.csv"), &tampered).expect("write tampered");
    answer(&world_third, "yes");
    let out_third = run(&world_third, &verify_args("CL-CMP-0002", TODAY));
    assert_ne!(out_third.code, 0, "third hash must be refused");
    assert!(root.join(".fieldbook/pending.toml").exists());

    // 5. Restore and the same command completes.
    std::fs::write(root.join("corpus/registers/claims.csv"), &planned).expect("restore planned");
    let world_restore = world_with_role(root, Some("owner"));
    answer(&world_restore, "yes");
    let out_restore = run(&world_restore, &verify_args("CL-CMP-0002", TODAY));
    assert_eq!(
        out_restore.code, 0,
        "restore completed stderr: {}",
        out_restore.stderr
    );
    assert!(!root.join(".fieldbook/pending.toml").exists());
    assert_eq!(claims_bytes(root).as_deref(), Some(planned.as_slice()));
    assert_eq!(count(&world_restore, "write.begin"), 1);
    assert_eq!(count(&world_restore, "write.end"), 1);

    let audit_out = run(&world_restore, &["audit", "verify"]);
    assert_eq!(
        audit_out.code, 0,
        "audit verify stderr: {}",
        audit_out.stderr
    );
}

#[test]
fn verify_bypassing_recovery_fails() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let before_corpus = corpus_files(root);

    interrupt_verify(root, "claims.csv");

    let world = world_with_role(root, Some("owner"));
    assert!(
        world.read_tree().is_err(),
        "read_tree must fail while verification is pending"
    );

    let other_writer = run(
        &world,
        &[
            "new",
            "question",
            "--confirm-public",
            "--area",
            "cmp",
            "Another?",
        ],
    );
    assert_ne!(
        other_writer.code, 0,
        "a different writer must not succeed while pending"
    );
    assert_eq!(
        corpus_files(root),
        before_corpus,
        "Corpus must remain untouched"
    );

    let check_out = run(&world, &["check", "--today", TODAY]);
    assert_ne!(
        check_out.code, 0,
        "ordinary check must report error or refuse to read while pending"
    );

    let audit_out = run(&world, &["audit", "verify"]);
    assert_eq!(
        audit_out.code, 0,
        "audit verify stderr: {}",
        audit_out.stderr
    );
}
