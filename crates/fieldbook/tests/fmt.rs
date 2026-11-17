//! Integration tests for `fmt` on temporary copies of `tests/fixtures/corpus-mini`.

#![expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "integration tests prepare temporary fixture copies and inspect written files and journal state"
)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fieldbook::World;
use fieldbook::io::journal::set_fail_after_end_append;
use fieldbook_core::CivilDate;
use fieldbook_core::check::{Code, Host, Options, load};
use fieldbook_core::schema::{Register, page, table};
use serde_json::Value;

use support::faulty::{Fault, FaultKind, FaultyWorld};
use support::temp_copy;
use support::world::TestWorld;

const TODAY: &str = "2026-03-01";
const PAGE_STEPS: &str = "corpus/pages/compliance/business-systems/cmp-review-steps.md";
const PAGE_CONCEPT: &str = "corpus/pages/compliance/business-systems/cmp-system-review.md";
const REG_CLAIMS: &str = "corpus/registers/claims.csv";
const REG_GLOSSARY: &str = "corpus/registers/glossary.csv";

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini")
}

fn svec(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

fn run(world: &dyn World, args: &[&str]) -> fieldbook::Outcome {
    fieldbook::run(&svec(args), world)
}

fn to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

fn events_of(world: &dyn World, kind: &str) -> Vec<Value> {
    let out = run(world, &["audit", "show", "--json", "--kind", kind]);
    assert_eq!(out.code, 0, "audit show stderr: {}", out.stderr);
    let parsed: Value =
        serde_json::from_str(&out.stdout).unwrap_or_else(|e| panic!("audit show json: {e}"));
    parsed["events"].as_array().expect("events array").clone()
}

fn raw_journal_bytes(root: &Path) -> String {
    let journal_dir = root.join(".fieldbook/audit");
    let mut combined = String::new();
    let mut entries: Vec<_> = std::fs::read_dir(&journal_dir)
        .expect("read journal dir")
        .map(|e| e.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            let text = std::fs::read_to_string(&path).expect("read journal segment");
            combined.push_str(&text);
        }
    }
    combined
}

#[test]
fn a_normal_copy_passes_check() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    let out = run(&world, &["fmt", "--check"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
    assert!(
        out.stdout.starts_with("fmt: ") && out.stdout.ends_with(" files in normal form\n"),
        "unexpected stdout: {:?}",
        out.stdout
    );
    assert!(
        !tmp.path().join(".fieldbook").exists(),
        "fmt --check must not create state directory"
    );
}

#[test]
fn a_crlf_register_is_fixed() {
    let tmp = temp_copy(&fixture_root());
    let glossary_path = tmp.path().join(REG_GLOSSARY);
    let original = std::fs::read_to_string(&glossary_path).expect("read glossary.csv");
    let crlf = to_crlf(&original);
    std::fs::write(&glossary_path, crlf.as_bytes()).expect("write CRLF glossary.csv");

    let world = TestWorld::new(tmp.path());
    let check_out = run(&world, &["fmt", "--check"]);
    assert_eq!(check_out.code, 1);
    assert_eq!(check_out.stdout, format!("would format {REG_GLOSSARY}\n"));
    assert!(check_out.stderr.is_empty());
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("read after --check"),
        crlf,
        "fmt --check must not modify files"
    );

    let fmt_out = run(&world, &["fmt"]);
    assert_eq!(fmt_out.code, 0, "stderr: {}", fmt_out.stderr);
    assert_eq!(fmt_out.stdout, format!("formatted {REG_GLOSSARY}\n"));
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("read after fmt"),
        original
    );

    let after_check = run(&world, &["fmt", "--check"]);
    assert_eq!(after_check.code, 0, "stderr: {}", after_check.stderr);
}

#[test]
fn a_page_with_a_byte_order_mark_is_fixed() {
    let tmp = temp_copy(&fixture_root());
    let page_path = tmp.path().join(PAGE_STEPS);
    let original = std::fs::read_to_string(&page_path).expect("read fixture page");

    // Include a front-matter comment and custom key order to prove front matter is never
    // parsed and reserialized, plus BOM + CRLF on the whole file and decomposed NFD in body.
    let commented = original.replacen("+++\n", "+++\n# synthetic comment preserved verbatim\n", 1);
    let with_nfd_body = commented.replace("checklist", "checklist cafe\u{0301}");
    let expected = commented.replace("checklist", "checklist caf\u{00e9}");
    let dirty = format!("\u{feff}{}", to_crlf(&with_nfd_body));
    std::fs::write(&page_path, dirty.as_bytes()).expect("write dirty page");

    let world = TestWorld::new(tmp.path());
    let check_out = run(&world, &["fmt", "--check"]);
    assert_eq!(check_out.code, 1);
    assert_eq!(check_out.stdout, format!("would format {PAGE_STEPS}\n"));

    let fmt_out = run(&world, &["fmt"]);
    assert_eq!(fmt_out.code, 0, "stderr: {}", fmt_out.stderr);
    assert_eq!(fmt_out.stdout, format!("formatted {PAGE_STEPS}\n"));

    let actual = std::fs::read_to_string(&page_path).expect("read formatted page");
    assert_eq!(actual, expected);
    assert!(
        actual.contains("# synthetic comment preserved verbatim\n"),
        "front-matter comment must be preserved verbatim"
    );

    let after_check = run(&world, &["fmt", "--check"]);
    assert_eq!(after_check.code, 0, "stderr: {}", after_check.stderr);
}

#[test]
fn trailing_spaces_are_fixed() {
    let tmp = temp_copy(&fixture_root());
    let steps_path = tmp.path().join(PAGE_STEPS);
    let concept_path = tmp.path().join(PAGE_CONCEPT);

    let orig_steps = std::fs::read_to_string(&steps_path).expect("read steps page");
    let orig_concept = std::fs::read_to_string(&concept_path).expect("read concept page");

    let dirty_steps = format!(
        "{}\n\n",
        orig_steps.replace("## Purpose\n", "## Purpose   \t\n")
    );
    let dirty_concept = orig_concept
        .trim_end_matches('\n')
        .replace("## Definition\n", "## Definition  \n");

    std::fs::write(&steps_path, dirty_steps.as_bytes()).expect("write dirty steps");
    std::fs::write(&concept_path, dirty_concept.as_bytes()).expect("write dirty concept");

    let world = TestWorld::new(tmp.path());
    let check_out = run(&world, &["fmt", "--check"]);
    assert_eq!(check_out.code, 1);
    assert_eq!(
        check_out.stdout,
        format!("would format {PAGE_STEPS}\nwould format {PAGE_CONCEPT}\n")
    );

    let fmt_out = run(&world, &["fmt"]);
    assert_eq!(fmt_out.code, 0, "stderr: {}", fmt_out.stderr);
    assert_eq!(
        fmt_out.stdout,
        format!("formatted {PAGE_STEPS}\nformatted {PAGE_CONCEPT}\n")
    );
    assert_eq!(
        std::fs::read_to_string(&steps_path).expect("read fixed steps"),
        orig_steps
    );
    assert_eq!(
        std::fs::read_to_string(&concept_path).expect("read fixed concept"),
        orig_concept
    );

    let after_check = run(&world, &["fmt", "--check"]);
    assert_eq!(after_check.code, 0, "stderr: {}", after_check.stderr);
}

#[test]
fn unparsable_register_and_front_matter_faults_are_reported_not_fixed() {
    let tmp = temp_copy(&fixture_root());
    let glossary_path = tmp.path().join(REG_GLOSSARY);
    let bad_csv = "term_id,label\n\"unclosed_quote,value\n";
    std::fs::write(&glossary_path, bad_csv.as_bytes()).expect("write broken csv");

    let page_path = tmp.path().join(PAGE_STEPS);
    let orig_page = std::fs::read_to_string(&page_path).expect("read page");
    let bad_fm_page = format!(
        "\u{feff}{}",
        orig_page.replacen("status = \"draft\"\n", "status = \"draft\"   \n", 1)
    );
    std::fs::write(&page_path, bad_fm_page.as_bytes()).expect("write page with fm trailing space");

    let world = TestWorld::new(tmp.path());
    let fmt_out = run(&world, &["fmt"]);
    assert_eq!(fmt_out.code, 1);
    assert!(fmt_out.stdout.is_empty(), "stdout: {}", fmt_out.stdout);
    assert!(
        fmt_out.stderr.contains(REG_GLOSSARY),
        "stderr must report broken register: {}",
        fmt_out.stderr
    );
    assert!(
        fmt_out.stderr.contains(PAGE_STEPS),
        "stderr must report front-matter fault: {}",
        fmt_out.stderr
    );
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("glossary untouched"),
        bad_csv
    );
    assert_eq!(
        std::fs::read_to_string(&page_path).expect("page untouched"),
        bad_fm_page
    );

    // Also verify non-NFC in front matter is reported, not rewritten.
    let nfd_fm_page = orig_page.replacen(
        "title = \"Synthetic procedure for running a business-system review\"",
        "title = \"Synthetic procedure for cafe\u{0301} review\"",
        1,
    );
    std::fs::write(&page_path, nfd_fm_page.as_bytes()).expect("write nfd fm page");
    let check_out = run(&world, &["fmt", "--check"]);
    assert_eq!(check_out.code, 1);
    assert!(
        check_out.stderr.contains(PAGE_STEPS),
        "stderr must report non-NFC front matter: {}",
        check_out.stderr
    );
}

#[test]
fn fmt_is_idempotent_and_journaled() {
    let tmp = temp_copy(&fixture_root());
    let glossary_path = tmp.path().join(REG_GLOSSARY);
    let original = std::fs::read_to_string(&glossary_path).expect("read glossary.csv");
    std::fs::write(&glossary_path, to_crlf(&original).as_bytes()).expect("write CRLF glossary.csv");

    let world = TestWorld::new(tmp.path());

    // First run formats 1 file and appends one write.begin + one write.end.
    let first = run(&world, &["fmt"]);
    assert_eq!(first.code, 0, "first stderr: {}", first.stderr);
    assert_eq!(first.stdout, format!("formatted {REG_GLOSSARY}\n"));

    let begins = events_of(&world, "write.begin");
    let ends = events_of(&world, "write.end");
    assert_eq!(begins.len(), 1, "expected 1 write.begin, got {begins:?}");
    assert_eq!(ends.len(), 1, "expected 1 write.end, got {ends:?}");

    assert_eq!(begins[0]["verb"], "fmt");
    assert_eq!(begins[0]["files"], 1);
    assert_eq!(begins[0]["affected"], serde_json::json!([]));

    assert_eq!(ends[0]["verb"], "fmt");
    assert_eq!(ends[0]["outcome"], "ok");
    assert_eq!(ends[0]["affected"], serde_json::json!([]));

    let raw_segments = raw_journal_bytes(tmp.path());
    assert!(
        !raw_segments.contains("glossary.csv") && !raw_segments.contains("corpus/registers"),
        "journal segments must hold no filenames: {raw_segments}"
    );

    let audit_out = run(&world, &["audit", "verify"]);
    assert_eq!(
        audit_out.code, 0,
        "audit verify stderr: {}",
        audit_out.stderr
    );
    assert!(
        audit_out.stdout.contains("journal ok"),
        "expected `journal ok`, got: {}",
        audit_out.stdout
    );

    let normalized_expected =
        table::normalize(Register::Glossary, original.as_bytes()).expect("normalize glossary");
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("read glossary after fmt"),
        normalized_expected
    );

    // Second run changes nothing and appends zero bytes to the journal, `journal.open` included.
    let journal_before = raw_journal_bytes(tmp.path());
    let second = run(&world, &["fmt"]);
    assert_eq!(second.code, 0, "second stderr: {}", second.stderr);
    assert!(
        second.stdout.starts_with("fmt: ") && second.stdout.ends_with(" files in normal form\n"),
        "unexpected second stdout: {}",
        second.stdout
    );
    assert_eq!(
        raw_journal_bytes(tmp.path()),
        journal_before,
        "a no-op `fmt` must append zero journal bytes"
    );
    assert_eq!(events_of(&world, "write.begin").len(), 1);
    assert_eq!(events_of(&world, "write.end").len(), 1);
}

#[test]
fn fmt_changes_form_never_a_value() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let today = CivilDate::parse(TODAY).expect("valid TODAY");

    // Perturb every register (BOM, CRLF, reversed rows) and every page (BOM, CRLF, trailing spaces, extra blank lines).
    let before_tree = fieldbook::io::read_tree(root).expect("read clean fixture tree");
    let mut target_count = 0usize;
    for (rel, _) in before_tree.iter() {
        let disk_path = root.join(rel);
        if rel.starts_with("corpus/registers/") && rel.ends_with(".csv") {
            let text = std::fs::read_to_string(&disk_path).expect("read register");
            let mut parsed = table::read(text.as_bytes()).expect("parse register");
            parsed.rows.reverse();
            let rewritten = table::write(&parsed);
            let dirty = format!("\u{feff}{}", to_crlf(&rewritten));
            std::fs::write(&disk_path, dirty.as_bytes()).expect("write dirty register");
            target_count += 1;
        } else if rel.starts_with("corpus/pages/") && rel.ends_with(".md") {
            let text = std::fs::read_to_string(&disk_path).expect("read page");
            let with_spaces = text
                .lines()
                .enumerate()
                .map(|(idx, line)| {
                    // Only add trailing spaces after front-matter closing fence (line > 20).
                    if idx > 20 && !line.is_empty() {
                        format!("{line}   ")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\r\n");
            let dirty = format!("\u{feff}{with_spaces}\r\n\r\n");
            std::fs::write(&disk_path, dirty.as_bytes()).expect("write dirty page");
            target_count += 1;
        }
    }
    assert!(target_count >= 7, "expected registers + pages in fixture");

    let dirty_tree = fieldbook::io::read_tree(root).expect("read dirty tree");
    let (dirty_model, _) = load(&Host::new(dirty_tree.clone(), today), &Options::new(false));

    let world = TestWorld::new(root);
    let fmt_out = run(&world, &["fmt"]);
    assert_eq!(fmt_out.code, 0, "fmt stderr: {}", fmt_out.stderr);
    assert_eq!(fmt_out.stdout.lines().count(), target_count);

    let after_tree = fieldbook::io::read_tree(root).expect("read formatted tree");
    let (after_model, _) = load(&Host::new(after_tree.clone(), today), &Options::new(false));

    // Compare parsed typed model rows (sorted by key) before and after fmt.
    let dirty_sources: BTreeMap<_, _> = dirty_model
        .sources()
        .iter()
        .map(|r| {
            (
                (r.value.source_id.clone(), r.value.version),
                r.value.clone(),
            )
        })
        .collect();
    let after_sources: BTreeMap<_, _> = after_model
        .sources()
        .iter()
        .map(|r| {
            (
                (r.value.source_id.clone(), r.value.version),
                r.value.clone(),
            )
        })
        .collect();
    assert_eq!(dirty_sources, after_sources);

    let dirty_claims: BTreeMap<_, _> = dirty_model
        .claims()
        .iter()
        .map(|r| (r.value.claim_id.clone(), r.value.clone()))
        .collect();
    let after_claims: BTreeMap<_, _> = after_model
        .claims()
        .iter()
        .map(|r| (r.value.claim_id.clone(), r.value.clone()))
        .collect();
    assert_eq!(dirty_claims, after_claims);

    let dirty_questions: BTreeMap<_, _> = dirty_model
        .questions()
        .iter()
        .map(|r| (r.value.question_id.clone(), r.value.clone()))
        .collect();
    let after_questions: BTreeMap<_, _> = after_model
        .questions()
        .iter()
        .map(|r| (r.value.question_id.clone(), r.value.clone()))
        .collect();
    assert_eq!(dirty_questions, after_questions);

    let dirty_conflicts: BTreeMap<_, _> = dirty_model
        .conflicts()
        .iter()
        .map(|r| (r.value.conflict_id.clone(), r.value.clone()))
        .collect();
    let after_conflicts: BTreeMap<_, _> = after_model
        .conflicts()
        .iter()
        .map(|r| (r.value.conflict_id.clone(), r.value.clone()))
        .collect();
    assert_eq!(dirty_conflicts, after_conflicts);

    let dirty_glossary: BTreeMap<_, _> = dirty_model
        .glossary()
        .iter()
        .map(|r| (r.value.term_id.clone(), r.value.clone()))
        .collect();
    let after_glossary: BTreeMap<_, _> = after_model
        .glossary()
        .iter()
        .map(|r| (r.value.term_id.clone(), r.value.clone()))
        .collect();
    assert_eq!(dirty_glossary, after_glossary);

    // Compare parsed PageMeta and per-line trimmed body content for every page.
    for (rel, _) in before_tree.iter() {
        if rel.starts_with("corpus/pages/") && rel.ends_with(".md") {
            let dirty_raw = String::from_utf8(
                dirty_tree
                    .get_bytes(rel)
                    .expect("dirty page bytes")
                    .to_vec(),
            )
            .expect("utf8");
            let dirty_clean_lf = dirty_raw
                .strip_prefix('\u{feff}')
                .unwrap_or(&dirty_raw)
                .replace("\r\n", "\n");
            let sp_before = page::split(&dirty_clean_lf).expect("split before");
            let meta_before = page::parse_meta(&sp_before.front_matter).expect("meta before");

            let after_raw = String::from_utf8(
                after_tree
                    .get_bytes(rel)
                    .expect("after page bytes")
                    .to_vec(),
            )
            .expect("utf8");
            let sp_after = page::split(&after_raw).expect("split after");
            let meta_after = page::parse_meta(&sp_after.front_matter).expect("meta after");

            assert_eq!(meta_before, meta_after, "PageMeta changed for {rel}");
            let norm_before_lines: Vec<&str> = sp_before
                .body
                .trim_end_matches('\n')
                .lines()
                .map(|l| l.trim_end_matches([' ', '\t']))
                .collect();
            let norm_after_lines: Vec<&str> =
                sp_after.body.trim_end_matches('\n').lines().collect();
            assert_eq!(
                norm_before_lines, norm_after_lines,
                "body values changed for {rel}"
            );
        }
    }
}

#[test]
fn check_strict_is_clean_after_fmt() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();

    // Step 11: supply claims.csv with UTF-8 BOM + CRLF AND a trailing-space `note` cell!
    let claims_path = root.join(REG_CLAIMS);
    let orig_claims = std::fs::read_to_string(&claims_path).expect("read claims.csv");
    let trailing_note = "Synthetic unchecked consequential fact for fixture tests. ";
    let updated_claims = orig_claims.replace(
        "Synthetic unchecked consequential fact for fixture tests.",
        trailing_note,
    );
    assert_ne!(orig_claims, updated_claims, "must replace note cell");
    let dirty_claims = format!("\u{feff}{}", to_crlf(&updated_claims));
    std::fs::write(&claims_path, dirty_claims.as_bytes()).expect("write BOM+CRLF claims.csv");

    // Also dirty a page with BOM, CRLF, and trailing body whitespace.
    let page_path = root.join(PAGE_STEPS);
    let orig_page = std::fs::read_to_string(&page_path).expect("read page");
    let dirty_page = format!(
        "\u{feff}{}\r\n",
        to_crlf(&orig_page.replace("## Purpose\n", "## Purpose   \n"))
    );
    std::fs::write(&page_path, dirty_page.as_bytes()).expect("write dirty page");

    let world = TestWorld::new(root);

    // Before fmt, strict check reports FB0122.
    let before_check = run(&world, &["check", "--strict", "--today", TODAY]);
    assert_eq!(before_check.code, 1);
    assert!(
        before_check.stdout.contains(Code::Fb0122.id()),
        "expected FB0122 before fmt, got: {}",
        before_check.stdout
    );

    // Format everything.
    let fmt_out = run(&world, &["fmt"]);
    assert_eq!(fmt_out.code, 0, "fmt stderr: {}", fmt_out.stderr);

    // Verify the exact trailing-space `note` cell is preserved in claims.csv!
    let formatted_claims_bytes = std::fs::read(&claims_path).expect("read formatted claims.csv");
    let parsed_claims = table::read(&formatted_claims_bytes).expect("parse formatted claims.csv");
    let note_col = parsed_claims
        .header
        .cells
        .iter()
        .position(|c| c == "note")
        .expect("note column");
    let row_cmp_0002 = parsed_claims
        .rows
        .iter()
        .find(|r| r.cells.first().map(String::as_str) == Some("CL-CMP-0002"))
        .expect("CL-CMP-0002 row");
    assert_eq!(
        row_cmp_0002.cells[note_col], trailing_note,
        "trailing space in register note cell is data and must be preserved"
    );

    // `fmt --check` succeeds.
    let fmt_check = run(&world, &["fmt", "--check"]);
    assert_eq!(
        fmt_check.code, 0,
        "fmt --check stderr: {}",
        fmt_check.stderr
    );

    // `check --strict` emits no FB0122 and exits 0.
    let after_check = run(&world, &["check", "--strict", "--today", TODAY]);
    assert_eq!(
        after_check.code, 0,
        "strict check stdout: {}\nstderr: {}",
        after_check.stdout, after_check.stderr
    );
    assert!(
        !after_check.stdout.contains(Code::Fb0122.id()),
        "unexpected FB0122 after fmt: {}",
        after_check.stdout
    );
}

#[test]
fn fmt_preserves_unknown_extension_locator() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();

    let ext_locator = "ext structured_path %2Froot%2Fitem%20one";
    let ext_citation = format!("[@S-0001-v1, {ext_locator}]");

    // Put canonical unknown extension kind/payload with percent escapes into claims.csv + BOM/CRLF.
    let claims_path = root.join(REG_CLAIMS);
    let orig_claims = std::fs::read_to_string(&claims_path).expect("read claims.csv");
    let ext_claims = orig_claims.replace(
        "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,text_section,sec. 2,",
        &format!(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,ext:structured_path,{ext_locator},"
        ),
    );
    assert_ne!(orig_claims, ext_claims);
    std::fs::write(
        &claims_path,
        format!("\u{feff}{}", to_crlf(&ext_claims)).as_bytes(),
    )
    .expect("write dirty ext claims.csv");

    // Put the same extension locator citation into a page body + BOM/CRLF/trailing spaces.
    let page_path = root.join(PAGE_CONCEPT);
    let orig_page = std::fs::read_to_string(&page_path).expect("read concept page");
    let target_sentence =
        "Synthetic reviewers check control operation and record open items before billing runs.";
    let cited_sentence = format!("{target_sentence} {ext_citation}");
    let ext_page = orig_page.replace(target_sentence, &format!("{cited_sentence}   "));
    assert_ne!(orig_page, ext_page);
    std::fs::write(
        &page_path,
        format!("\u{feff}{}", to_crlf(&ext_page)).as_bytes(),
    )
    .expect("write dirty ext page");

    let world = TestWorld::new(root);
    let fmt_out = run(&world, &["fmt"]);
    assert_eq!(fmt_out.code, 0, "fmt stderr: {}", fmt_out.stderr);
    assert_eq!(
        fmt_out.stdout,
        format!("formatted {PAGE_CONCEPT}\nformatted {REG_CLAIMS}\n")
    );

    // Re-parse register and page and assert byte-identical locator cell and citation text.
    let today = CivilDate::parse(TODAY).expect("valid TODAY");
    let tree = fieldbook::io::read_tree(root).expect("read formatted tree");
    let (model, _) = load(&Host::new(tree, today), &Options::new(false));
    let claim = model
        .claims()
        .iter()
        .find(|c| c.value.claim_id.as_str() == "CL-CMP-0002")
        .expect("CL-CMP-0002 present");
    assert_eq!(claim.value.locator.as_deref(), Some(ext_locator));

    let parsed_table =
        table::read(&std::fs::read(&claims_path).expect("read claims bytes")).expect("parse csv");
    let loc_col = parsed_table
        .header
        .cells
        .iter()
        .position(|c| c == "locator")
        .expect("locator col");
    let raw_row = parsed_table
        .rows
        .iter()
        .find(|r| r.cells.first().map(String::as_str) == Some("CL-CMP-0002"))
        .expect("CL-CMP-0002 csv row");
    assert_eq!(raw_row.cells[loc_col].as_bytes(), ext_locator.as_bytes());

    let formatted_page = std::fs::read_to_string(&page_path).expect("read formatted page");
    let split_page = page::split(&formatted_page).expect("split formatted page");
    assert!(
        split_page.body.contains(&cited_sentence),
        "citation text must be preserved byte-identically in body:\n{}",
        split_page.body
    );
}

#[test]
fn fmt_recovers_interrupted_between_two_writes() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();

    let page_path = root.join(PAGE_STEPS);
    let glossary_path = root.join(REG_GLOSSARY);
    let orig_page = std::fs::read_to_string(&page_path).expect("read page");
    let orig_glossary = std::fs::read_to_string(&glossary_path).expect("read glossary");
    let dirty_glossary = to_crlf(&orig_glossary);

    std::fs::write(&page_path, format!("\u{feff}{orig_page}").as_bytes()).expect("dirty page");
    std::fs::write(&glossary_path, dirty_glossary.as_bytes()).expect("dirty glossary");

    // Interrupt on the second file (`glossary.csv`) after `cmp-review-steps.md` is already written.
    let faulty_second = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: "glossary.csv",
            kind: FaultKind::Io,
        },
    );
    let interrupted = run(&faulty_second, &["fmt"]);
    assert_eq!(
        interrupted.code, 3,
        "expected I/O failure: {}",
        interrupted.stderr
    );
    assert!(root.join(".fieldbook/pending.toml").exists());
    assert_eq!(
        std::fs::read_to_string(&page_path).expect("first file already written"),
        orig_page
    );
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("second file still before"),
        dirty_glossary
    );

    // `fmt --check` stays read-only and refuses pending generations.
    let check_world = TestWorld::new(root);
    let check_pending = run(&check_world, &["fmt", "--check"]);
    assert_eq!(
        check_pending.code, 3,
        "expected pending refusal: {}",
        check_pending.stderr
    );
    assert!(check_pending.stdout.is_empty());
    assert!(root.join(".fieldbook/pending.toml").exists());
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("glossary still untouched"),
        dirty_glossary
    );

    // Resume with a fault armed on the ALREADY-WRITTEN first file (`cmp-review-steps.md`)
    // to prove recovery writes ONLY unfinished files while still validating the full set.
    let faulty_first = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: "cmp-review-steps.md",
            kind: FaultKind::Io,
        },
    );
    let resumed = run(&faulty_first, &["fmt"]);
    assert_eq!(resumed.code, 0, "resume stderr: {}", resumed.stderr);
    assert_eq!(
        resumed.stdout,
        format!("formatted {PAGE_STEPS}\nformatted {REG_GLOSSARY}\n")
    );
    assert!(!root.join(".fieldbook/pending.toml").exists());
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("glossary now normalized"),
        orig_glossary
    );
    assert_eq!(events_of(&faulty_first, "write.begin").len(), 1);
    assert_eq!(events_of(&faulty_first, "write.end").len(), 1);

    let audit_out = run(&faulty_first, &["audit", "verify"]);
    assert_eq!(
        audit_out.code, 0,
        "audit verify stderr: {}",
        audit_out.stderr
    );
    assert!(audit_out.stdout.contains("journal ok"));
}

#[test]
fn fmt_recovers_after_last_write_before_journal_end() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let glossary_path = root.join(REG_GLOSSARY);
    let orig_glossary = std::fs::read_to_string(&glossary_path).expect("read glossary");
    std::fs::write(&glossary_path, to_crlf(&orig_glossary).as_bytes()).expect("dirty glossary");

    let faulty = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: "glossary.csv",
            kind: FaultKind::Io,
        },
    );
    assert_eq!(run(&faulty, &["fmt"]).code, 3);
    assert!(root.join(".fieldbook/pending.toml").exists());

    // Simulate crash right after atomic rename of last file, before write.end.
    std::fs::write(&glossary_path, orig_glossary.as_bytes()).expect("place after bytes");

    // Keep FailOnName armed on glossary.csv: recovery must NOT rewrite it!
    // The tree is already in normal form, so this run writes no target; the pending intent alone
    // must open the journal and close the interrupted write.
    let journal_before_resume = raw_journal_bytes(root);
    let resumed = run(&faulty, &["fmt"]);
    assert_eq!(resumed.code, 0, "resume stderr: {}", resumed.stderr);
    assert_ne!(
        raw_journal_bytes(root),
        journal_before_resume,
        "recovering a pending intent must still write the closing journal events"
    );
    assert_eq!(resumed.stdout, format!("formatted {REG_GLOSSARY}\n"));
    assert!(!root.join(".fieldbook/pending.toml").exists());
    assert_eq!(events_of(&faulty, "write.begin").len(), 1);
    assert_eq!(events_of(&faulty, "write.end").len(), 1);
    let audit_out = run(&faulty, &["audit", "verify"]);
    assert_eq!(audit_out.code, 0);
    assert!(audit_out.stdout.contains("journal ok"));
}

#[test]
fn fmt_recovery_still_reports_a_fault_outside_the_pending_write() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let glossary_path = root.join(REG_GLOSSARY);
    let orig_glossary = std::fs::read_to_string(&glossary_path).expect("read glossary");
    std::fs::write(&glossary_path, to_crlf(&orig_glossary).as_bytes()).expect("dirty glossary");

    // A page whose front matter cannot be normalized: reported, never part of the write set.
    let page_path = root.join(PAGE_STEPS);
    let orig_page = std::fs::read_to_string(&page_path).expect("read page");
    let bad_page = orig_page.replacen("status = \"draft\"\n", "status = \"draft\"   \n", 1);
    assert_ne!(orig_page, bad_page);
    std::fs::write(&page_path, bad_page.as_bytes()).expect("write faulty page");

    let faulty = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: "glossary.csv",
            kind: FaultKind::Io,
        },
    );
    assert_eq!(run(&faulty, &["fmt"]).code, 3);
    assert!(root.join(".fieldbook/pending.toml").exists());

    let world = TestWorld::new(root);
    let resumed = run(&world, &["fmt"]);
    assert_eq!(resumed.stdout, format!("formatted {REG_GLOSSARY}\n"));
    assert!(
        resumed.stderr.contains(PAGE_STEPS),
        "recovery must keep the page fault: {}",
        resumed.stderr
    );
    assert_eq!(resumed.code, 1, "a fault is a finding even after recovery");
    assert!(!root.join(".fieldbook/pending.toml").exists());
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("glossary normalized"),
        orig_glossary
    );
    assert_eq!(
        std::fs::read_to_string(&page_path).expect("page untouched"),
        bad_page
    );
}

#[test]
fn fmt_recovers_after_journal_end_before_cleanup() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let glossary_path = root.join(REG_GLOSSARY);
    let orig_glossary = std::fs::read_to_string(&glossary_path).expect("read glossary");
    std::fs::write(&glossary_path, to_crlf(&orig_glossary).as_bytes()).expect("dirty glossary");

    let world = TestWorld::new(root);
    set_fail_after_end_append(true);
    let failed_cleanup = run(&world, &["fmt"]);
    set_fail_after_end_append(false);

    assert_eq!(failed_cleanup.code, 3);
    assert!(root.join(".fieldbook/pending.toml").exists());

    let resumed = run(&world, &["fmt"]);
    assert_eq!(resumed.code, 0, "resume stderr: {}", resumed.stderr);
    assert_eq!(resumed.stdout, format!("formatted {REG_GLOSSARY}\n"));
    assert!(!root.join(".fieldbook/pending.toml").exists());
    assert_eq!(events_of(&world, "write.begin").len(), 1);
    assert_eq!(events_of(&world, "write.end").len(), 1);
    let audit_out = run(&world, &["audit", "verify"]);
    assert_eq!(audit_out.code, 0);
    assert!(audit_out.stdout.contains("journal ok"));
}

#[test]
fn fmt_refuses_recovery_on_third_hash() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let glossary_path = root.join(REG_GLOSSARY);
    let orig_glossary = std::fs::read_to_string(&glossary_path).expect("read glossary");
    std::fs::write(&glossary_path, to_crlf(&orig_glossary).as_bytes()).expect("dirty glossary");

    let faulty = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: "glossary.csv",
            kind: FaultKind::Io,
        },
    );
    assert_eq!(run(&faulty, &["fmt"]).code, 3);
    assert!(root.join(".fieldbook/pending.toml").exists());

    // Write valid CSV with a third hash matching neither before_sha256 nor after_sha256.
    let third_bytes = format!("{orig_glossary}z_extra,extra,Def.,Rule.,cmp,,general_practice,,\n");
    std::fs::write(&glossary_path, third_bytes.as_bytes()).expect("write third-hash bytes");

    let world = TestWorld::new(root);
    let refused = run(&world, &["fmt"]);
    assert_ne!(refused.code, 0, "third hash must refuse recovery");
    assert!(root.join(".fieldbook/pending.toml").exists());
    assert_eq!(
        std::fs::read_to_string(&glossary_path).expect("third-hash file untouched"),
        third_bytes
    );
}

#[test]
fn readers_accept_a_hand_edit_after_a_journaled_write() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let glossary_path = root.join(REG_GLOSSARY);
    let orig_glossary = std::fs::read_to_string(&glossary_path).expect("read glossary");
    std::fs::write(&glossary_path, to_crlf(&orig_glossary).as_bytes()).expect("dirty glossary");

    let world = TestWorld::new(root);
    assert_eq!(run(&world, &["fmt"]).code, 0);
    assert!(
        root.join(".fieldbook/generation").is_file(),
        "the journaled write leaves a generation marker"
    );

    // A hand edit after the journaled write: one line that needs formatting.
    let page_path = root.join(PAGE_STEPS);
    let mut page = std::fs::read_to_string(&page_path).expect("read page");
    page.push_str("A sentence added by hand.   \n");
    std::fs::write(&page_path, page.as_bytes()).expect("hand edit");

    let checked = run(&world, &["fmt", "--check"]);
    assert_eq!(checked.stdout, format!("would format {PAGE_STEPS}\n"));
    assert_eq!(checked.code, 1, "stderr: {}", checked.stderr);

    for args in [
        &["check", "--today", "2026-01-15"][..],
        &["status"][..],
        &["search", "review"][..],
    ] {
        let out = run(&world, args);
        assert_ne!(out.code, 3, "{args:?} refused a hand edit: {}", out.stderr);
        assert!(
            !out.stderr.contains("generation"),
            "{args:?} named the generation marker: {}",
            out.stderr
        );
    }

    // The writing command still formats the hand edit, and the tree is normal afterwards.
    let fixed = run(&world, &["fmt"]);
    assert_eq!(fixed.stdout, format!("formatted {PAGE_STEPS}\n"));
    assert_eq!(run(&world, &["fmt", "--check"]).code, 0);
}

#[test]
fn readers_still_refuse_a_pending_write_after_a_hand_edit() {
    let tmp = temp_copy(&fixture_root());
    let root = tmp.path();
    let glossary_path = root.join(REG_GLOSSARY);
    let orig_glossary = std::fs::read_to_string(&glossary_path).expect("read glossary");
    std::fs::write(&glossary_path, to_crlf(&orig_glossary).as_bytes()).expect("dirty glossary");

    let faulty = FaultyWorld::new(
        TestWorld::new(root),
        Fault::FailOnName {
            name: "glossary.csv",
            kind: FaultKind::Io,
        },
    );
    assert_eq!(run(&faulty, &["fmt"]).code, 3);
    assert!(root.join(".fieldbook/pending.toml").exists());

    let world = TestWorld::new(root);
    for args in [
        &["fmt", "--check"][..],
        &["check", "--today", "2026-01-15"][..],
        &["status"][..],
        &["search", "review"][..],
    ] {
        let out = run(&world, args);
        assert_eq!(out.code, 3, "{args:?} read an interrupted write");
        assert!(
            out.stderr.contains("pending"),
            "{args:?} must name the pending write: {}",
            out.stderr
        );
    }
}
