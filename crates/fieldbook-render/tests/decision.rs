//! Tests for the synthetic decision package workbook (`templates/decision-package.xlsx`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read fixtures and inspect zip archives"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::Path;
use std::time::Duration;

use fieldbook_calc::{
    Dec, Money, Reason, RoundingPolicy, Stage, Value, add, div_at_stage, mul, percent_of,
    round_money_at_stage, sub, sub_dec,
};
use fieldbook_core::audit::Now;
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Host, Options, Tree, load};
use fieldbook_core::date::CivilDate;
use fieldbook_core::edition::{BuildInfo, BuildProfile, EditionCounts, EditionIds, EditionInfo};
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::strings;
use fieldbook_render::decision::{
    self, ASSUMPTION_DIRECT_MATERIAL, ASSUMPTION_ESCALATION_PCT, ASSUMPTION_GA_PCT,
    ASSUMPTION_LABOUR_HOURS, ASSUMPTION_LABOUR_RATE, ASSUMPTION_OVERHEAD_PCT,
    ASSUMPTION_PROFIT_PCT, LINE_BURDENED_COST, LINE_CHECK, LINE_GA, LINE_LABOUR, LINE_MATERIAL,
    LINE_OVERHEAD, LINE_PROFIT, LINE_PROFIT_ON_COST, LINE_TOTAL_COST_INPUT, LINE_UNIT_PRICE,
    OUTPUT_PATH, PREVIEW, PREVIEW_LABEL, SHEET_ASSUMPTIONS, SHEET_COST_BUILD, SHEET_README,
    SHEET_SUMMARY, Version, cost_build, decision_workbook, save_workbook, synthetic_example,
};
use fieldbook_render::home;
use fieldbook_render::meta::DEFAULT_WORKBOOK_EPOCH;
use fieldbook_render::site::{self, PlannedPaths};
use zip::ZipArchive;

const ALLOWED_FUNCTIONS: &[&str] = &["INDEX", "MATCH", "ROUND", "SUM", "IF"];

const FORBIDDEN_FUNCTIONS: &[&str] = &[
    // Volatile functions
    "NOW",
    "TODAY",
    "RAND",
    "RANDBETWEEN",
    "OFFSET",
    "INDIRECT",
    "INFO",
    "CELL",
    // Dynamic-array / Excel 365 functions
    "FILTER",
    "SORT",
    "UNIQUE",
    "SEQUENCE",
    "LET",
    "LAMBDA",
    "XLOOKUP",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum CellValue {
    String(String),
    Number(String),
    Formula {
        formula: String,
        cached: Option<String>,
        cell_type: Option<String>,
    },
}

struct SheetData {
    name: String,
    cells: BTreeMap<String, CellValue>,
    xml: String,
}

fn unescape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            while let Some(&next_c) = chars.peek() {
                if next_c == ';' {
                    chars.next();
                    break;
                }
                entity.push(chars.next().unwrap());
                if entity.len() > 10 {
                    break;
                }
            }
            match entity.as_str() {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ => {
                    out.push('&');
                    out.push_str(&entity);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn extract_attr<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let pattern = format!("{attr}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn extract_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open_prefix = format!("<{tag}");
    let open_pos = xml.find(&open_prefix)?;
    let after_open = &xml[open_pos..];
    let open_end = after_open.find('>')?;
    let open_tag = &after_open[..=open_end];
    if open_tag.ends_with("/>") {
        return Some(String::new());
    }
    let body_start = open_pos + open_end + 1;
    let close_tag = format!("</{tag}>");
    let close_rel = xml[body_start..].find(&close_tag)?;
    Some(unescape_xml(&xml[body_start..body_start + close_rel]))
}

fn parse_workbook_sheet_names(workbook_xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut search_from = 0;
    while let Some(start) = workbook_xml[search_from..].find("<sheet ") {
        let tag_start = search_from + start;
        let Some(tag_len) = workbook_xml[tag_start..].find('>') else {
            break;
        };
        let tag = &workbook_xml[tag_start..tag_start + tag_len + 1];
        if let Some(name) = extract_attr(tag, "name") {
            names.push(unescape_xml(name));
        }
        search_from = tag_start + tag_len + 1;
    }
    names
}

fn parse_shared_strings(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(si_rel) = xml[search_from..].find("<si") {
        let start = search_from + si_rel;
        let Some(si_end) = xml[start..].find("</si>") else {
            break;
        };
        let si_content = &xml[start..start + si_end + 5];
        search_from = start + si_end + 5;

        let mut text = String::new();
        let mut t_search = 0;
        while let Some(t_open_rel) = si_content[t_search..].find("<t") {
            let t_open = t_search + t_open_rel;
            let Some(t_open_end) = si_content[t_open..].find('>') else {
                break;
            };
            let text_start = t_open + t_open_end + 1;
            let Some(t_close) = si_content[text_start..].find("</t>") else {
                break;
            };
            let raw = &si_content[text_start..text_start + t_close];
            text.push_str(&unescape_xml(raw));
            t_search = text_start + t_close + 4;
        }
        out.push(text);
    }
    out
}

fn parse_sheet_cells(xml: &str, shared_strings: &[String]) -> BTreeMap<String, CellValue> {
    let mut cells = BTreeMap::new();
    let mut search_from = 0;
    while let Some(c_rel) = xml[search_from..].find("<c ") {
        let c_start = search_from + c_rel;
        let Some(open_end_rel) = xml[c_start..].find('>') else {
            break;
        };
        let open_end = c_start + open_end_rel;
        let open_tag = &xml[c_start..=open_end];

        let cell_xml = if open_tag.ends_with("/>") {
            search_from = open_end + 1;
            open_tag
        } else {
            let Some(close_rel) = xml[open_end..].find("</c>") else {
                break;
            };
            let close_end = open_end + close_rel + 4;
            let full = &xml[c_start..close_end];
            search_from = close_end;
            full
        };

        let Some(cell_ref) = extract_attr(open_tag, "r") else {
            continue;
        };
        let t_attr = extract_attr(open_tag, "t").map(str::to_owned);

        let value = if let Some(formula) = extract_tag_text(cell_xml, "f") {
            let cached = extract_tag_text(cell_xml, "v");
            CellValue::Formula {
                formula,
                cached,
                cell_type: t_attr,
            }
        } else if t_attr.as_deref() == Some("s") {
            let text = extract_tag_text(cell_xml, "v")
                .and_then(|idx_str| idx_str.parse::<usize>().ok())
                .and_then(|idx| shared_strings.get(idx).cloned())
                .unwrap_or_default();
            CellValue::String(text)
        } else if t_attr.as_deref() == Some("inlineStr") {
            let text = extract_tag_text(cell_xml, "t").unwrap_or_default();
            CellValue::String(text)
        } else {
            let num = extract_tag_text(cell_xml, "v").unwrap_or_default();
            CellValue::Number(num)
        };

        cells.insert(cell_ref.to_owned(), value);
    }
    cells
}

fn read_workbook(bytes: &[u8]) -> Vec<SheetData> {
    let mut cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(&mut cursor).unwrap();

    let workbook_xml = {
        let mut f = archive.by_name("xl/workbook.xml").unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        s
    };
    let sheet_names = parse_workbook_sheet_names(&workbook_xml);

    let shared_strings = if let Ok(mut f) = archive.by_name("xl/sharedStrings.xml") {
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        parse_shared_strings(&s)
    } else {
        Vec::new()
    };

    let mut sheets = Vec::new();
    for (i, name) in sheet_names.into_iter().enumerate() {
        let sheet_path = format!("xl/worksheets/sheet{}.xml", i + 1);
        let mut f = archive
            .by_name(&sheet_path)
            .unwrap_or_else(|_| panic!("{sheet_path} must exist"));
        let mut xml = String::new();
        f.read_to_string(&mut xml).unwrap();
        let cells = parse_sheet_cells(&xml, &shared_strings);
        sheets.push(SheetData { name, cells, xml });
    }

    sheets
}

fn read_zip_part(bytes: &[u8], name: &str) -> String {
    let mut cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(&mut cursor).unwrap();
    let mut f = archive
        .by_name(name)
        .unwrap_or_else(|e| panic!("missing zip part `{name}`: {e}"));
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    s
}

fn zip_entries(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(&mut cursor).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        out.insert(name, data);
    }
    out
}

/// Extracts uppercase Excel function names (`IDENT(`) outside quoted strings.
fn extract_formula_functions(formula: &str) -> Vec<String> {
    let bytes = formula.as_bytes();
    let mut out = Vec::new();
    let mut in_quotes = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            if in_quotes && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                i += 2;
                continue;
            }
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes && (b.is_ascii_alphabetic() || b == b'_') {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'(' {
                let ident = &formula[start..i];
                let clean = ident.strip_prefix("_xlfn.").unwrap_or(ident);
                out.push(clean.to_ascii_uppercase());
            }
            continue;
        }
        i += 1;
    }
    out
}

/// Extracts the argument lists of every `MATCH(...)` call outside quoted strings.
fn extract_match_calls(formula: &str) -> Vec<Vec<String>> {
    let bytes = formula.as_bytes();
    let mut calls = Vec::new();
    let mut in_quotes = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            if in_quotes && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                i += 2;
                continue;
            }
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes && (b.is_ascii_alphabetic() || b == b'_') {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            if i < bytes.len()
                && bytes[i] == b'('
                && formula[start..i].eq_ignore_ascii_case("MATCH")
            {
                let mut depth = 0usize;
                let mut q = false;
                let mut arg_start = i + 1;
                let mut args = Vec::new();
                let mut j = i + 1;
                while j < bytes.len() {
                    let ch = bytes[j];
                    if ch == b'"' {
                        if q && j + 1 < bytes.len() && bytes[j + 1] == b'"' {
                            j += 2;
                            continue;
                        }
                        q = !q;
                    } else if !q {
                        if ch == b'(' {
                            depth += 1;
                        } else if ch == b')' {
                            if depth == 0 {
                                args.push(formula[arg_start..j].trim().to_owned());
                                break;
                            }
                            depth -= 1;
                        } else if ch == b',' && depth == 0 {
                            args.push(formula[arg_start..j].trim().to_owned());
                            arg_start = j + 1;
                        }
                    }
                    j += 1;
                }
                calls.push(args);
            }
            continue;
        }
        i += 1;
    }
    calls
}

/// Checks XL-02: no macro, external link, hyperlink, DDE, volatile function or dynamic array.
fn xl02_violations(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let entries = zip_entries(bytes);

    for (name, data) in &entries {
        if name == "xl/vbaProject.bin" || name.ends_with("vbaProject.bin") {
            out.push(format!("{name}: VBA macro project found"));
        }
        if name.starts_with("xl/externalLinks/") {
            out.push(format!("{name}: external link entry found"));
        }
        if let Ok(text) = std::str::from_utf8(data) {
            if name.ends_with(".rels") && text.contains("/relationships/hyperlink") {
                out.push(format!("{name}: hyperlink relationship found"));
            }
            if text.contains("ddeLink") {
                out.push(format!("{name}: DDE link found"));
            }
        }
    }

    let sheets = read_workbook(bytes);
    for sheet in &sheets {
        let mut search_from = 0;
        while let Some(rel) = sheet.xml[search_from..].find("<hyperlink ") {
            let start = search_from + rel;
            let end = sheet.xml[start..]
                .find('>')
                .map_or(sheet.xml.len(), |e| start + e + 1);
            let tag = &sheet.xml[start..end];
            let cell_ref = extract_attr(tag, "ref").unwrap_or("?");
            out.push(format!(
                "{}!{cell_ref}: hyperlink element found",
                sheet.name
            ));
            search_from = end;
        }

        let mut c_from = 0;
        while let Some(rel) = sheet.xml[c_from..].find("<c ") {
            let start = c_from + rel;
            let end = sheet.xml[start..]
                .find('>')
                .map_or(sheet.xml.len(), |e| start + e + 1);
            let tag = &sheet.xml[start..end];
            if tag.contains(" cm=\"") {
                let cell_ref = extract_attr(tag, "r").unwrap_or("?");
                out.push(format!(
                    "{}!{cell_ref}: dynamic-array cm attribute found",
                    sheet.name
                ));
            }
            c_from = end;
        }

        for (cell_ref, val) in &sheet.cells {
            if let CellValue::Formula { formula, .. } = val {
                for func in extract_formula_functions(formula) {
                    if FORBIDDEN_FUNCTIONS.contains(&func.as_str()) {
                        out.push(format!(
                            "{}!{cell_ref}: forbidden function `{func}` in formula `{formula}`",
                            sheet.name
                        ));
                    }
                }
            }
        }
    }

    out
}

fn released_edition() -> EditionInfo {
    EditionInfo {
        edition: "edition-007".to_owned(),
        number: 7,
        built: "2026-03-01".to_owned(),
        tool_version: "0.1.0".to_owned(),
        min_tool_version: "0.1.0".to_owned(),
        schema_version: 1,
        commit: "1".repeat(40),
        fingerprint: "a".repeat(64),
        counts: EditionCounts::default(),
        ids: EditionIds::default(),
    }
}

fn unreleased_edition() -> EditionInfo {
    EditionInfo {
        edition: "unreleased".to_owned(),
        number: 0,
        built: String::new(),
        tool_version: "0.0.0".to_owned(),
        min_tool_version: "0.1.0".to_owned(),
        schema_version: 1,
        commit: String::new(),
        fingerprint: "b".repeat(64),
        counts: EditionCounts::default(),
        ids: EditionIds::default(),
    }
}

fn sheet_by_name<'a>(sheets: &'a [SheetData], name: &str) -> &'a SheetData {
    sheets
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("missing sheet `{name}`"))
}

fn cell_text(sheet: &SheetData, cell_ref: &str) -> String {
    match sheet.cells.get(cell_ref) {
        Some(CellValue::String(s)) => s.clone(),
        other => panic!(
            "expected string at {}!{cell_ref}, got {other:?}",
            sheet.name
        ),
    }
}

fn cell_formula_cached(sheet: &SheetData, cell_ref: &str) -> (String, String) {
    match sheet.cells.get(cell_ref) {
        Some(CellValue::Formula {
            formula,
            cached: Some(v),
            ..
        }) => (formula.clone(), v.clone()),
        other => panic!(
            "expected formula with cached value at {}!{cell_ref}, got {other:?}",
            sheet.name
        ),
    }
}

fn insert_all(tree: &mut Tree, dir: &Path, rel: &str) {
    tree.insert_dir(rel);
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let child = format!("{rel}/{name}");
        if entry.file_type().unwrap().is_dir() {
            insert_all(tree, &entry.path(), &child);
        } else {
            tree.insert_bytes(child, std::fs::read(entry.path()).unwrap());
        }
    }
}

fn fixture_view_with<T>(f: impl FnOnce(&LabelledView) -> T) -> T {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
    let mut tree = Tree::new();
    insert_all(&mut tree, &root.join("corpus"), "corpus");
    let today = CivilDate::parse("2026-03-01").unwrap();
    let (model, _report) = load(&Host::new(tree, today), &Options::new(false));
    let eff = EffectivePolicy::floor(false);
    let now = Now::new(today, 0).unwrap();
    let Decision::Allowed(allowed) = check(&eff, &Dest::Edition, None, None, now, Shipped::Never)
    else {
        panic!("edition selection must succeed");
    };
    let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
    f(&view)
}

#[test]
fn core_xml_has_the_fixed_time() {
    let bytes = decision::render(&released_edition(), 1_772_366_400).unwrap();
    let core = read_zip_part(&bytes, "docProps/core.xml");
    assert!(
        core.contains("2026-03-01T12:00:00Z"),
        "docProps/core.xml must contain supplied epoch instant 2026-03-01T12:00:00Z, got: {core}"
    );
}

#[test]
fn workbook_without_epoch_has_fixed_creation_time() {
    let bytes = decision::render(&unreleased_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let core = read_zip_part(&bytes, "docProps/core.xml");
    assert!(
        core.contains("2000-01-01T00:00:00Z"),
        "docProps/core.xml must contain 2000-01-01T00:00:00Z, got: {core}"
    );

    let fallback_bytes = decision::render(&unreleased_edition(), i64::MIN).unwrap();
    let fallback_core = read_zip_part(&fallback_bytes, "docProps/core.xml");
    assert!(
        fallback_core.contains("2000-01-01T00:00:00Z"),
        "invalid epoch must fall back to 2000-01-01T00:00:00Z, got: {fallback_core}"
    );
}

#[test]
fn full_calc_on_load_is_set() {
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let wb_xml = read_zip_part(&bytes, "xl/workbook.xml");
    assert!(
        wb_xml.contains("fullCalcOnLoad=\"1\""),
        "xl/workbook.xml must set fullCalcOnLoad=\"1\", got: {wb_xml}"
    );
}

// control: XL-02
#[test]
fn no_macro_external_link_hyperlink_dde_volatile_or_dynamic_array() {
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let viols = xl02_violations(&bytes);
    assert!(
        viols.is_empty(),
        "decision package workbook failed XL-02 checks:\n{}",
        viols.join("\n")
    );
}

// control: XL-02
#[test]
fn readback_flags_a_web_hyperlink() {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(SHEET_SUMMARY).unwrap();
    ws.write_url(1, 2, "https://example.invalid").unwrap();
    let bytes = wb.save_to_buffer().unwrap();

    let viols = xl02_violations(&bytes);
    assert!(
        !viols.is_empty(),
        "workbook with web hyperlink must be flagged by xl02_violations"
    );
    assert!(
        viols.iter().any(|v| v.contains("Summary!C2")),
        "hyperlink violation must name sheet and cell `Summary!C2`, got: {viols:?}"
    );
}

#[test]
fn formulas_use_only_the_allowed_functions_and_exact_match() {
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let sheets = read_workbook(&bytes);

    let mut formula_count = 0usize;
    let mut match_count = 0usize;
    let mut index_count = 0usize;

    for sheet in &sheets {
        for (cell_ref, val) in &sheet.cells {
            if let CellValue::Formula { formula, .. } = val {
                formula_count += 1;
                let funcs = extract_formula_functions(formula);
                for func in &funcs {
                    assert!(
                        ALLOWED_FUNCTIONS.contains(&func.as_str()),
                        "{}!{cell_ref}: disallowed function `{func}` in `{formula}`",
                        sheet.name
                    );
                    if func == "INDEX" {
                        index_count += 1;
                    }
                    if func == "MATCH" {
                        match_count += 1;
                    }
                }
                for args in extract_match_calls(formula) {
                    assert_eq!(
                        args.len(),
                        3,
                        "{}!{cell_ref}: MATCH must take 3 arguments in `{formula}`, got {args:?}",
                        sheet.name
                    );
                    assert_eq!(
                        args[2], "0",
                        "{}!{cell_ref}: MATCH must use exact match type 0 in `{formula}`, got `{}`",
                        sheet.name, args[2]
                    );
                }
            }
        }
    }

    assert!(formula_count > 0, "workbook must contain formula cells");
    assert!(index_count > 0, "workbook must use INDEX lookups");
    assert!(match_count > 0, "workbook must use exact MATCH lookups");
}

#[test]
fn every_formula_cell_has_a_cached_value() {
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let sheets = read_workbook(&bytes);

    let mut formula_count = 0usize;
    for sheet in &sheets {
        for (cell_ref, val) in &sheet.cells {
            if let CellValue::Formula {
                formula, cached, ..
            } = val
            {
                formula_count += 1;
                let non_empty = cached.as_deref().is_some_and(|v| !v.is_empty());
                assert!(
                    non_empty,
                    "{}!{cell_ref}: formula `{formula}` has missing or empty cached value `{cached:?}`",
                    sheet.name
                );
            }
        }
    }
    assert!(formula_count > 0, "expected formula cells in workbook");
}

#[test]
fn cached_values_equal_an_independent_recomputation() {
    let input = synthetic_example();
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let sheets = read_workbook(&bytes);
    let cost_sheet = sheet_by_name(&sheets, SHEET_COST_BUILD);
    let summary_sheet = sheet_by_name(&sheets, SHEET_SUMMARY);

    let cents = RoundingPolicy::new(2).unwrap();
    let find_val = |id: &str, v: Version| -> Dec {
        let a = input.assumptions.iter().find(|a| a.id == id).unwrap();
        let raw = match v {
            Version::V1 => &a.v1,
            Version::V2 => &a.v2,
        };
        Dec::parse(raw).unwrap()
    };

    let recompute = |v: Version| -> Vec<Dec> {
        let mat = Money::usd(find_val(ASSUMPTION_DIRECT_MATERIAL, v));
        let esc = find_val(ASSUMPTION_ESCALATION_PCT, v);
        let hrs = find_val(ASSUMPTION_LABOUR_HOURS, v);
        let rate = Money::usd(find_val(ASSUMPTION_LABOUR_RATE, v));
        let oh_pct = find_val(ASSUMPTION_OVERHEAD_PCT, v);
        let ga_pct = find_val(ASSUMPTION_GA_PCT, v);
        let prof_pct = find_val(ASSUMPTION_PROFIT_PCT, v);

        let mat_esc = round_money_at_stage(
            add(
                Value::Known(mat),
                percent_of(Value::Known(mat), Value::Known(esc)).unwrap(),
            )
            .unwrap(),
            Stage::Extension,
            &cents,
        )
        .unwrap();
        let labour = round_money_at_stage(
            mul(Value::Known(rate), Value::Known(hrs)).unwrap(),
            Stage::Extension,
            &cents,
        )
        .unwrap();
        let overhead_unrounded = percent_of(labour, Value::Known(oh_pct)).unwrap();
        let overhead = round_money_at_stage(overhead_unrounded, Stage::Extension, &cents).unwrap();
        // Verify the example actually has a sub-cent overhead before rounding.
        assert_ne!(
            overhead_unrounded.known().unwrap().amount,
            overhead.known().unwrap().amount,
            "example overhead must be sub-cent before rounding"
        );
        let tci = round_money_at_stage(
            add(add(mat_esc, labour).unwrap(), overhead).unwrap(),
            Stage::Total,
            &cents,
        )
        .unwrap();
        let ga = round_money_at_stage(
            percent_of(tci, Value::Known(ga_pct)).unwrap(),
            Stage::Extension,
            &cents,
        )
        .unwrap();
        let burdened = round_money_at_stage(add(tci, ga).unwrap(), Stage::Total, &cents).unwrap();
        let profit = round_money_at_stage(
            percent_of(burdened, Value::Known(prof_pct)).unwrap(),
            Stage::Extension,
            &cents,
        )
        .unwrap();
        let price =
            round_money_at_stage(add(burdened, profit).unwrap(), Stage::Total, &cents).unwrap();
        let poc = div_at_stage(
            mul(profit, Value::Known(Dec::ONE_HUNDRED)).unwrap(),
            Value::Known(burdened.known().unwrap().amount),
            Stage::Total,
            &cents,
        )
        .unwrap();

        vec![
            mat_esc.known().unwrap().amount,
            labour.known().unwrap().amount,
            overhead.known().unwrap().amount,
            tci.known().unwrap().amount,
            ga.known().unwrap().amount,
            burdened.known().unwrap().amount,
            profit.known().unwrap().amount,
            price.known().unwrap().amount,
            poc.known().unwrap().amount,
        ]
    };

    let expected_v1 = recompute(Version::V1);
    let expected_v2 = recompute(Version::V2);

    let expected_labels = [
        LINE_MATERIAL,
        LINE_LABOUR,
        LINE_OVERHEAD,
        LINE_TOTAL_COST_INPUT,
        LINE_GA,
        LINE_BURDENED_COST,
        LINE_PROFIT,
        LINE_UNIT_PRICE,
        LINE_PROFIT_ON_COST,
    ];

    for (idx, label) in expected_labels.iter().enumerate() {
        let row = idx + 2;
        assert_eq!(
            cell_text(cost_sheet, &format!("A{row}")),
            *label,
            "{SHEET_COST_BUILD}!A{row} label mismatch"
        );

        let (_, cached_v1) = cell_formula_cached(cost_sheet, &format!("B{row}"));
        assert_eq!(
            cached_v1,
            expected_v1[idx].to_string(),
            "{SHEET_COST_BUILD}!B{row} ({label} v1) cached value mismatch"
        );

        let (_, cached_v2) = cell_formula_cached(cost_sheet, &format!("C{row}"));
        assert_eq!(
            cached_v2,
            expected_v2[idx].to_string(),
            "{SHEET_COST_BUILD}!C{row} ({label} v2) cached value mismatch"
        );

        let expected_diff = if idx == 8 {
            sub_dec(
                Value::Known(expected_v2[idx]),
                Value::Known(expected_v1[idx]),
            )
            .unwrap()
            .known()
            .unwrap()
        } else {
            sub(
                Value::Known(Money::usd(expected_v2[idx])),
                Value::Known(Money::usd(expected_v1[idx])),
            )
            .unwrap()
            .known()
            .unwrap()
            .amount
        };
        let (_, cached_diff) = cell_formula_cached(cost_sheet, &format!("D{row}"));
        assert_eq!(
            cached_diff,
            expected_diff.to_string(),
            "{SHEET_COST_BUILD}!D{row} ({label} change) cached value mismatch"
        );
    }

    // Verify Summary key figure cached values match the independent recomputation.
    let expected_price_diff = sub(
        Value::Known(Money::usd(expected_v2[7])),
        Value::Known(Money::usd(expected_v1[7])),
    )
    .unwrap()
    .known()
    .unwrap()
    .amount;
    let summary_checks = [
        ("B12", expected_v1[7].to_string()),
        ("B13", expected_v2[7].to_string()),
        ("B14", expected_price_diff.to_string()),
        ("B15", expected_v2[5].to_string()),
        ("B16", expected_v2[8].to_string()),
        ("B17", "OK".to_owned()),
    ];
    for (cell_ref, expected) in summary_checks {
        let (_, cached) = cell_formula_cached(summary_sheet, cell_ref);
        assert_eq!(
            cached, expected,
            "{SHEET_SUMMARY}!{cell_ref} cached value mismatch"
        );
    }
}

#[test]
fn self_check_cell_is_ok() {
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let sheets = read_workbook(&bytes);
    let cost_sheet = sheet_by_name(&sheets, SHEET_COST_BUILD);
    let summary_sheet = sheet_by_name(&sheets, SHEET_SUMMARY);

    assert_eq!(cell_text(cost_sheet, "A11"), LINE_CHECK);
    for col in ["B", "C", "D"] {
        let cell_ref = format!("{col}11");
        let (formula, cached) = cell_formula_cached(cost_sheet, &cell_ref);
        assert_eq!(
            cached, "OK",
            "{SHEET_COST_BUILD}!{cell_ref} self-check must cache OK, formula={formula}"
        );
    }
    let (_, summary_check) = cell_formula_cached(summary_sheet, "B17");
    assert_eq!(summary_check, "OK");
}

#[test]
fn zero_denominator_reads_n_a_and_no_iferror() {
    let mut input = synthetic_example();
    for a in &mut input.assumptions {
        if matches!(
            a.id.as_str(),
            ASSUMPTION_DIRECT_MATERIAL | ASSUMPTION_LABOUR_HOURS | ASSUMPTION_LABOUR_RATE
        ) {
            a.v1 = "0".to_owned();
            a.v2 = "0".to_owned();
        }
    }

    let lines = cost_build(&input, Version::V1).unwrap();
    assert_eq!(
        lines[8].value,
        Value::Unknown(Reason::ZeroDenominator),
        "profit on cost must be Unknown(ZeroDenominator) when burdened cost is zero"
    );

    let mut wb = decision_workbook(&input, &released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let bytes = save_workbook(&mut wb).unwrap();
    let sheets = read_workbook(&bytes);
    let cost_sheet = sheet_by_name(&sheets, SHEET_COST_BUILD);
    let summary_sheet = sheet_by_name(&sheets, SHEET_SUMMARY);

    for cell_ref in ["B10", "C10", "D10"] {
        let (formula, cached) = cell_formula_cached(cost_sheet, cell_ref);
        assert_eq!(
            cached, "n/a: zero denominator",
            "{SHEET_COST_BUILD}!{cell_ref} must show n/a: zero denominator, formula={formula}"
        );
        assert!(
            formula.contains("IF("),
            "{SHEET_COST_BUILD}!{cell_ref} must use an explicit IF zero test: {formula}"
        );
    }
    let (_, summary_poc) = cell_formula_cached(summary_sheet, "B16");
    assert_eq!(summary_poc, "n/a: zero denominator");

    for sheet in &sheets {
        for (cell_ref, val) in &sheet.cells {
            if let CellValue::Formula { formula, .. } = val {
                assert!(
                    !formula.to_ascii_uppercase().contains("IFERROR"),
                    "{}!{cell_ref}: IFERROR masking is forbidden: `{formula}`",
                    sheet.name
                );
            }
        }
    }
}

#[test]
fn summary_rows_use_the_frozen_part_headings_and_review_state() {
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let sheets = read_workbook(&bytes);
    let summary = sheet_by_name(&sheets, SHEET_SUMMARY);

    for (idx, expected_part) in strings::BRIEF_PARTS.iter().enumerate() {
        let row = idx + 3;
        assert_eq!(
            cell_text(summary, &format!("A{row}")),
            *expected_part,
            "{SHEET_SUMMARY}!A{row} must match strings::BRIEF_PARTS[{idx}]"
        );
        assert!(
            !cell_text(summary, &format!("B{row}")).is_empty(),
            "{SHEET_SUMMARY}!B{row} must not be empty"
        );
    }
    assert_eq!(cell_text(summary, "A9"), "Review state");
    assert!(!cell_text(summary, "B9").is_empty());

    for row in 12..=17 {
        let label = cell_text(summary, &format!("A{row}"));
        assert!(
            label.contains('(') && label.contains(')'),
            "{SHEET_SUMMARY}!A{row} key figure label must carry its basis: `{label}`"
        );
        let (formula, cached) = cell_formula_cached(summary, &format!("B{row}"));
        assert!(
            formula.contains("INDEX(") && formula.contains("MATCH("),
            "{SHEET_SUMMARY}!B{row} must look up Cost build via INDEX/MATCH: `{formula}`"
        );
        assert!(!cached.is_empty());
        let origin = cell_text(summary, &format!("C{row}"));
        assert!(
            strings::ORIGIN_LABELS.contains(&origin.as_str()),
            "{SHEET_SUMMARY}!C{row} origin `{origin}` must be in strings::ORIGIN_LABELS"
        );
    }
}

#[test]
fn header_block_holds_edition_tool_version_and_as_of() {
    let released = released_edition();
    let released_bytes = decision::render(&released, DEFAULT_WORKBOOK_EPOCH).unwrap();
    let released_sheets = read_workbook(&released_bytes);
    let readme_rel = sheet_by_name(&released_sheets, SHEET_README);
    assert_eq!(cell_text(readme_rel, "A3"), "Edition");
    assert_eq!(cell_text(readme_rel, "B3"), "edition-007");
    assert_eq!(cell_text(readme_rel, "A4"), "Tool version");
    assert_eq!(cell_text(readme_rel, "B4"), "0.1.0");
    assert_eq!(cell_text(readme_rel, "A5"), "As-of date");
    assert_eq!(cell_text(readme_rel, "B5"), "2026-03-01");

    let unreleased = unreleased_edition();
    let unreleased_bytes = decision::render(&unreleased, DEFAULT_WORKBOOK_EPOCH).unwrap();
    let unreleased_sheets = read_workbook(&unreleased_bytes);
    let readme_unrel = sheet_by_name(&unreleased_sheets, SHEET_README);
    assert_eq!(cell_text(readme_unrel, "B3"), "unreleased");
    assert_eq!(cell_text(readme_unrel, "B4"), "0.0.0");
    assert_eq!(cell_text(readme_unrel, "B5"), strings::unreleased_build());
}

#[test]
fn preview_label_is_on_read_me_and_summary() {
    const {
        assert!(PREVIEW, "PREVIEW constant must remain true until S2 passes");
    }
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let sheets = read_workbook(&bytes);
    let readme = sheet_by_name(&sheets, SHEET_README);
    let summary = sheet_by_name(&sheets, SHEET_SUMMARY);

    let readme_texts: Vec<&str> = readme
        .cells
        .values()
        .filter_map(|v| match v {
            CellValue::String(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    let summary_texts: Vec<&str> = summary
        .cells
        .values()
        .filter_map(|v| match v {
            CellValue::String(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        readme_texts.contains(&PREVIEW_LABEL),
        "Read-me sheet must carry preview label `{PREVIEW_LABEL}`"
    );
    assert!(
        summary_texts.contains(&PREVIEW_LABEL),
        "Summary sheet must carry preview label `{PREVIEW_LABEL}`"
    );
    assert!(
        readme_texts
            .iter()
            .any(|t| t.to_ascii_lowercase().contains("synthetic")),
        "Read-me sheet must mark the example synthetic"
    );
    assert!(
        summary_texts
            .iter()
            .any(|t| t.to_ascii_lowercase().contains("synthetic")),
        "Summary sheet must mark the example synthetic"
    );
    assert!(
        readme_texts
            .iter()
            .any(|t| t.contains("Excel asks to save after opening")),
        "Read-me sheet must explain the Excel save prompt"
    );
}

#[test]
fn every_cost_line_names_an_assumption_that_exists() {
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let sheets = read_workbook(&bytes);
    let cost_sheet = sheet_by_name(&sheets, SHEET_COST_BUILD);
    let assumptions_sheet = sheet_by_name(&sheets, SHEET_ASSUMPTIONS);

    let mut assumption_ids = BTreeSet::new();
    for row in 2..=8 {
        assumption_ids.insert(cell_text(assumptions_sheet, &format!("A{row}")));
    }
    assert_eq!(assumption_ids.len(), 7);

    for row in 2..=10 {
        let line_label = cell_text(cost_sheet, &format!("A{row}"));
        let ids_cell = cell_text(cost_sheet, &format!("E{row}"));
        assert!(
            !ids_cell.is_empty(),
            "{SHEET_COST_BUILD}!E{row} ({line_label}) must name at least one assumption id"
        );
        for raw_id in ids_cell.split(',') {
            let id = raw_id.trim();
            assert!(
                assumption_ids.contains(id),
                "{SHEET_COST_BUILD}!E{row} ({line_label}) names unknown assumption `{id}`"
            );
        }
        // Also verify every MATCH("SYN-...", ...) lookup in B/C targets an existing assumption id.
        for col in ["B", "C"] {
            let (formula, _) = cell_formula_cached(cost_sheet, &format!("{col}{row}"));
            for args in extract_match_calls(&formula) {
                let target = args[0].trim_matches('"');
                assert!(
                    assumption_ids.contains(target),
                    "{SHEET_COST_BUILD}!{col}{row} MATCH target `{target}` not found in Assumptions"
                );
            }
        }
    }
}

#[test]
fn assumptions_have_the_trace_columns() {
    let bytes = decision::render(&released_edition(), DEFAULT_WORKBOOK_EPOCH).unwrap();
    let sheets = read_workbook(&bytes);
    let assumptions = sheet_by_name(&sheets, SHEET_ASSUMPTIONS);

    let expected_headers = [
        ("A1", "id"),
        ("B1", "text"),
        ("C1", "unit"),
        ("D1", "v1"),
        ("E1", "v2"),
        ("F1", "owner role"),
        ("G1", "claim id"),
        ("H1", "source version"),
        ("I1", "locator"),
        ("J1", "origin"),
        ("K1", "status"),
    ];
    for (cell_ref, expected) in expected_headers {
        assert_eq!(
            cell_text(assumptions, cell_ref),
            expected,
            "{SHEET_ASSUMPTIONS}!{cell_ref} header mismatch"
        );
    }

    assert!(
        assumptions.xml.contains("ySplit=\"1\"") && assumptions.xml.contains("state=\"frozen\""),
        "Assumptions header row must be frozen"
    );
    assert!(
        assumptions.xml.contains("<autoFilter ref=\"A1:K8\""),
        "Assumptions must have autofilter A1:K8"
    );

    let mut has_not_established = false;
    for row in 2..=8 {
        assert!(
            matches!(
                assumptions.cells.get(&format!("D{row}")),
                Some(CellValue::Number(_))
            ),
            "{SHEET_ASSUMPTIONS}!D{row} must be numeric"
        );
        assert!(
            matches!(
                assumptions.cells.get(&format!("E{row}")),
                Some(CellValue::Number(_))
            ),
            "{SHEET_ASSUMPTIONS}!E{row} must be numeric"
        );
        let claim_id = cell_text(assumptions, &format!("G{row}"));
        let source_ver = cell_text(assumptions, &format!("H{row}"));
        let locator = cell_text(assumptions, &format!("I{row}"));
        if claim_id == "Not established"
            && source_ver == "Not established"
            && locator == "Not established"
        {
            has_not_established = true;
        }
        let origin = cell_text(assumptions, &format!("J{row}"));
        assert!(
            strings::ORIGIN_LABELS.contains(&origin.as_str()),
            "{SHEET_ASSUMPTIONS}!J{row} origin `{origin}` must be in strings::ORIGIN_LABELS"
        );
    }
    assert!(
        has_not_established,
        "synthetic example must include at least one assumption with `Not established` trace fields"
    );
}

#[test]
fn decision_workbook_then_save_workbook_equals_render() {
    let edition = released_edition();
    let epoch = 1_772_366_400;

    let mut wb = decision_workbook(&synthetic_example(), &edition, epoch).unwrap();
    let two_step = save_workbook(&mut wb).unwrap();
    let rendered = decision::render(&edition, epoch).unwrap();
    assert_eq!(two_step, rendered);

    // Decide-port seam: appending a fifth sheet between decision_workbook and save_workbook
    // places it last and leaves the four existing sheets untouched.
    let mut wb_plus = decision_workbook(&synthetic_example(), &edition, epoch).unwrap();
    let extra = wb_plus.add_worksheet();
    extra.set_name("AI proposal").unwrap();
    extra
        .write_string(0, 0, "AI proposal, not accepted")
        .unwrap();
    let plus_bytes = save_workbook(&mut wb_plus).unwrap();

    let base_sheets = read_workbook(&rendered);
    let plus_sheets = read_workbook(&plus_bytes);
    let plus_names: Vec<&str> = plus_sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        plus_names,
        [
            SHEET_README,
            SHEET_SUMMARY,
            SHEET_COST_BUILD,
            SHEET_ASSUMPTIONS,
            "AI proposal"
        ]
    );
    for idx in 0..4 {
        assert_eq!(
            plus_sheets[idx].cells, base_sheets[idx].cells,
            "sheet `{}` must be unchanged when a sheet is added after decision_workbook",
            base_sheets[idx].name
        );
    }
}

// control: DT-01
#[test]
fn two_builds_same_bytes() {
    let edition = released_edition();
    let epoch1 = 1_772_366_400;
    let first = decision::render(&edition, epoch1).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let second = decision::render(&edition, epoch1).unwrap();
    assert_eq!(
        first, second,
        "two builds a second apart with the same epoch must produce identical bytes"
    );

    let epoch2 = epoch1 + 86_400;
    let other_epoch = decision::render(&edition, epoch2).unwrap();
    assert_ne!(
        first, other_epoch,
        "different epochs must produce different workbook bytes"
    );

    let entries1 = zip_entries(&first);
    let entries2 = zip_entries(&other_epoch);
    let keys1: Vec<&str> = entries1.keys().map(String::as_str).collect();
    let keys2: Vec<&str> = entries2.keys().map(String::as_str).collect();
    assert_eq!(keys1, keys2, "zip entry list must not depend on epoch");

    let differing: Vec<&str> = entries1
        .iter()
        .filter(|(k, v)| entries2.get(*k) != Some(*v))
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(
        differing,
        ["docProps/core.xml"],
        "only docProps/core.xml may differ between two epochs"
    );
}

#[test]
fn site_plans_and_builds_the_template() {
    fixture_view_with(|view| {
        let planned = site::planned_paths(view);
        assert!(
            planned.contains(OUTPUT_PATH),
            "planned_paths must contain `{OUTPUT_PATH}`"
        );
        let rendered = site::render(
            view,
            &BuildInfo::new(None, None, BuildProfile::Silent, None),
        )
        .unwrap();
        assert!(
            rendered.files().contains_key(OUTPUT_PATH),
            "site::render must build `{OUTPUT_PATH}`"
        );
        let site_keys: Vec<&str> = rendered.files().keys().map(String::as_str).collect();
        let planned_keys: Vec<&str> = planned.iter().collect();
        assert_eq!(site_keys, planned_keys);
    });
}

#[test]
fn home_links_the_decision_package() {
    fixture_view_with(|view| {
        let rendered = site::render(view, &BuildInfo::default()).unwrap();
        let index_html = std::str::from_utf8(rendered.files().get("index.html").unwrap()).unwrap();
        let templates_pos = index_html
            .find("id=\"templates\"")
            .expect("index.html must contain templates panel");
        let templates_panel = &index_html[templates_pos..];
        let templates_end = templates_panel.find("</section>").unwrap();
        let templates_panel = &templates_panel[..templates_end];

        assert!(
            templates_panel.contains(&format!(
                "<a href=\"{OUTPUT_PATH}\">Decision package (preview)</a>"
            )),
            "Templates panel must link `{OUTPUT_PATH}` as Decision package (preview): {templates_panel}"
        );
        assert!(
            templates_panel.to_ascii_lowercase().contains("synthetic"),
            "Decision package row must say it is a synthetic example: {templates_panel}"
        );

        let unplanned = home::render_main(view, &PlannedPaths::default());
        assert!(
            !unplanned.contains(&format!("href=\"{OUTPUT_PATH}\"")),
            "unplanned home page must not link `{OUTPUT_PATH}`"
        );
        assert!(
            unplanned.contains(&format!("{OUTPUT_PATH} is not available until")),
            "unplanned home page must show the frozen not_available line for `{OUTPUT_PATH}`"
        );
    });
}
