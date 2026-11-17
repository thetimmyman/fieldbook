//! Integration tests for per-part claims CSV copies (`claims-csv-parts`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

use fieldbook_core::audit::Now;
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Host, Model, Options, Tree, load};
use fieldbook_core::date::CivilDate;
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::schema::Register;
use fieldbook_render::csv_copy;
use fieldbook_render::meta::BuildInfo;
use fieldbook_render::register;
use fieldbook_render::site::{self, planned_paths};

const CORPUS_TOML: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
const ASSISTANT_MD: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
const CHANGES_MD: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/changes.md");
const SOURCES_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
const CLAIMS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
const QUESTIONS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
const CONFLICTS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
const GLOSSARY_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
const S_0002_V2_TXT: &[u8] =
    include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0002-v2.txt");
const S_0001_V1_TXT: &[u8] =
    include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt");

fn make_claim_row(id: &str) -> String {
    let first_row = CLAIMS_CSV.lines().nth(1).unwrap();
    let mut cols: Vec<String> = first_row.split(',').map(str::to_owned).collect();
    let area_lower = id
        .strip_prefix("CL-")
        .and_then(|rest| rest.split('-').next())
        .unwrap()
        .to_ascii_lowercase();
    cols[0] = id.to_owned();
    cols[1] = area_lower;
    cols.join(",")
}

fn tree_with_claims_csv(claims_csv: &str) -> Tree {
    let mut tree = Tree::new();
    tree.insert_dir("corpus");
    tree.insert_dir("corpus/eval");
    tree.insert_dir("corpus/registers");
    tree.insert_dir("corpus/sources");
    tree.insert_dir("corpus/sources/public");
    tree.insert_dir("corpus/pages");

    tree.insert_bytes("corpus/corpus.toml", CORPUS_TOML.as_bytes());
    tree.insert_bytes("corpus/ASSISTANT.md", ASSISTANT_MD.as_bytes());
    tree.insert_bytes("corpus/changes.md", CHANGES_MD.as_bytes());
    tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");
    tree.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
    tree.insert_bytes("corpus/registers/claims.csv", claims_csv.as_bytes());
    tree.insert_bytes("corpus/registers/questions.csv", QUESTIONS_CSV.as_bytes());
    tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS_CSV.as_bytes());
    tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY_CSV.as_bytes());
    tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
    tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", S_0001_V1_TXT);
    tree
}

fn load_view(tree: Tree) -> (Model, LabelledView) {
    let today = CivilDate::parse("2026-03-01").unwrap();
    let (model, report) = load(&Host::new(tree, today), &Options::new(false));
    assert!(
        report.is_ok(),
        "fixture tree must load cleanly:\n{}",
        report.render_text()
    );
    let eff = EffectivePolicy::floor(false);
    let now = Now::new(today, 0).unwrap();
    let Decision::Allowed(allowed) = check(&eff, &Dest::Edition, None, None, now, Shipped::Never)
    else {
        panic!("edition selection must succeed");
    };
    let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
    (model, view)
}

/// Builds the fixture claims register augmented with synthetic CMP part-1/part-2/part-3 rows
/// before the fixture's EST rows so canonical `(area, number)` order holds.
fn multi_part_claims_csv() -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut src_lines = CLAIMS_CSV.lines();
    let header = src_lines.next().unwrap();
    lines.push(header.to_owned());

    let mut inserted_extra_cmp = false;
    for line in src_lines {
        if !inserted_extra_cmp && line.starts_with("CL-EST-") {
            for id in ["CL-CMP-0500", "CL-CMP-0501", "CL-CMP-1000", "CL-CMP-1001"] {
                lines.push(make_claim_row(id));
            }
            inserted_extra_cmp = true;
        }
        lines.push(line.to_owned());
    }
    lines.join("\n") + "\n"
}

fn parse_csv_data_rows(bytes: &[u8], label: &str) -> Vec<Vec<String>> {
    assert!(
        bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "{label} must start with UTF-8 BOM"
    );
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            assert!(
                i > 0 && bytes[i - 1] == b'\r',
                "every LF in {label} must be preceded by CR (byte {i})"
            );
        }
    }
    let body = std::str::from_utf8(&bytes[3..]).unwrap();
    assert!(body.ends_with("\r\n"), "{label} must end with CRLF");

    let expected_header = Register::Claims
        .columns()
        .iter()
        .map(|col| col.name)
        .collect::<Vec<_>>()
        .join(",");
    let first_line = body.split("\r\n").next().unwrap();
    assert_eq!(
        first_line, expected_header,
        "header mismatch in claims part CSV {label}"
    );

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(&bytes[3..]);
    let headers = rdr
        .headers()
        .unwrap()
        .iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let expected_cols: Vec<String> = Register::Claims
        .columns()
        .iter()
        .map(|c| c.name.to_owned())
        .collect();
    assert_eq!(headers, expected_cols, "parsed header columns for {label}");

    rdr.records()
        .map(|rec| rec.unwrap().iter().map(str::to_owned).collect())
        .collect()
}

#[test]
fn claims_part_csvs_equal_the_part_pages() {
    let (_model, view) = load_view(tree_with_claims_csv(&multi_part_claims_csv()));
    let site = site::render(&view, &BuildInfo::default()).unwrap();
    let planned = planned_paths(&view);

    let expected: Vec<String> = register::claims_parts(&view)
        .into_iter()
        .map(|part| format!("data/claims-{}-{}.csv", part.area_code, part.part))
        .collect();
    assert_eq!(
        expected,
        vec![
            "data/claims-CMP-1.csv",
            "data/claims-CMP-2.csv",
            "data/claims-CMP-3.csv",
            "data/claims-EST-1.csv",
        ]
    );

    let actual: Vec<String> = site
        .files()
        .keys()
        .filter(|path| path.starts_with("data/claims-") && path.ends_with(".csv"))
        .cloned()
        .collect();
    assert_eq!(
        actual, expected,
        "built site data/claims-*.csv keys must map one-to-one to register::claims_parts"
    );

    let planned_part_csvs: Vec<String> = planned
        .iter()
        .filter(|path| path.starts_with("data/claims-") && path.ends_with(".csv"))
        .map(str::to_owned)
        .collect();
    assert_eq!(planned_part_csvs, expected);
}

#[test]
fn claims_part_csvs_hold_the_register_rows_in_order() {
    let (_model, view) = load_view(tree_with_claims_csv(&multi_part_claims_csv()));
    let site = site::render(&view, &BuildInfo::default()).unwrap();

    let parts = register::claims_parts(&view);
    assert!(!parts.is_empty());

    let mut joined_rows: Vec<Vec<String>> = Vec::new();
    for part in &parts {
        let rel = format!("data/claims-{}-{}.csv", part.area_code, part.part);
        let bytes = site
            .files()
            .get(&rel)
            .unwrap_or_else(|| panic!("missing claims part CSV `{rel}` in site"));
        let part_rows = parse_csv_data_rows(bytes, &rel);
        assert_eq!(
            part_rows.len(),
            part.count,
            "row count in `{rel}` must match ClaimsPart::count"
        );
        joined_rows.extend(part_rows);
    }

    let expected_rows = csv_copy::rows(&view, Register::Claims);
    assert_eq!(
        joined_rows, expected_rows,
        "joined claims part CSV rows in claims_parts order must equal csv_copy::rows"
    );
}

#[test]
fn a_part_splits_at_500() {
    let header = CLAIMS_CSV.lines().next().unwrap();
    let claims_csv = format!(
        "{header}\n{}\n{}\n{}\n",
        make_claim_row("CL-CMP-0001"),
        make_claim_row("CL-CMP-0500"),
        make_claim_row("CL-CMP-0501"),
    );
    let mut tree = tree_with_claims_csv(&claims_csv);
    let conflicts_header = format!("{}\n", CONFLICTS_CSV.lines().next().unwrap());
    let glossary_header = format!("{}\n", GLOSSARY_CSV.lines().next().unwrap());
    tree.insert_bytes(
        "corpus/registers/conflicts.csv",
        conflicts_header.as_bytes(),
    );
    tree.insert_bytes("corpus/registers/glossary.csv", glossary_header.as_bytes());

    let (_model, view) = load_view(tree);
    let site = site::render(&view, &BuildInfo::default()).unwrap();

    let part1_bytes = site
        .files()
        .get("data/claims-CMP-1.csv")
        .expect("data/claims-CMP-1.csv must exist");
    let part2_bytes = site
        .files()
        .get("data/claims-CMP-2.csv")
        .expect("data/claims-CMP-2.csv must exist");

    let part1_ids: Vec<String> = parse_csv_data_rows(part1_bytes, "data/claims-CMP-1.csv")
        .into_iter()
        .map(|r| r[0].clone())
        .collect();
    let part2_ids: Vec<String> = parse_csv_data_rows(part2_bytes, "data/claims-CMP-2.csv")
        .into_iter()
        .map(|r| r[0].clone())
        .collect();

    assert_eq!(part1_ids, vec!["CL-CMP-0001", "CL-CMP-0500"]);
    assert_eq!(part2_ids, vec!["CL-CMP-0501"]);
}

#[test]
fn no_unparted_claims_csv() {
    let (_model, view) = load_view(tree_with_claims_csv(CLAIMS_CSV));
    let planned = planned_paths(&view);
    assert!(
        !planned.contains("data/claims.csv"),
        "planned_paths must not hold data/claims.csv"
    );

    let site = site::render(&view, &BuildInfo::default()).unwrap();
    assert!(
        !site.files().contains_key("data/claims.csv"),
        "built site must not contain data/claims.csv"
    );
}

#[test]
fn claims_part_pages_link_their_csv() {
    let (_model, view) = load_view(tree_with_claims_csv(&multi_part_claims_csv()));
    let planned = planned_paths(&view);
    let site = site::render(&view, &BuildInfo::default()).unwrap();

    for part in register::claims_parts(&view) {
        let csv_file = format!("claims-{}-{}.csv", part.area_code, part.part);
        let csv_rel = format!("data/{csv_file}");
        assert!(
            planned.contains(&csv_rel),
            "part CSV `{csv_rel}` must be in planned_paths"
        );

        let page_bytes = site
            .files()
            .get(&part.route)
            .unwrap_or_else(|| panic!("missing part page `{}`", part.route));
        let page_html = std::str::from_utf8(page_bytes).unwrap();
        let expected_line = format!(
            "<p class=\"muted data-links\">Copy for Excel: <a href=\"../data/{csv_file}\">{csv_file}</a> \u{b7} <a href=\"../data/registers.xlsx\">registers.xlsx</a></p>"
        );
        assert!(
            page_html.contains(&expected_line),
            "part page `{}` must carry `{expected_line}`",
            part.route
        );
        assert!(
            !page_html.contains("data/claims.csv"),
            "part page `{}` must not link data/claims.csv",
            part.route
        );
    }

    let index_bytes = site
        .files()
        .get("claims.html")
        .expect("claims.html in site files");
    let index_html = std::str::from_utf8(index_bytes).unwrap();
    assert!(
        index_html.contains(
            "<p class=\"muted data-links\">Copy for Excel: <a href=\"data/registers.xlsx\">registers.xlsx</a></p>"
        ),
        "claims index must keep its workbook link and drop its CSV link"
    );
    assert!(
        !index_html.contains("data/claims.csv") && !index_html.contains("data/claims-"),
        "claims index must carry neither data/claims.csv nor per-part CSV links"
    );
}
