//! Tests for the static workbook templates (`templates/<name>.xlsx`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read fixtures, inspect files, read env vars, start child process"
)]
#![expect(
    clippy::disallowed_types,
    reason = "child process command and temporary files"
)]

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

use fieldbook_core::audit::Now;
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Host, Options, Tree, load};
use fieldbook_core::date::CivilDate;
use fieldbook_core::edition::{BuildInfo, BuildProfile};
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::schema::table;
use fieldbook_render::site;
use fieldbook_render::templates_xlsx::{self, Column, TEMPLATES, TemplateError, TemplateSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    Text(String),
    NotText(String),
}

pub struct SheetData {
    pub name: String,
    pub cells: BTreeMap<String, Cell>,
    pub xml: String,
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

fn col_to_letters(mut col: usize) -> String {
    let mut s = String::new();
    loop {
        let rem = col % 26;
        let b = u8::try_from(rem).unwrap_or(0);
        s.push((b'A' + b) as char);
        if col < 26 {
            break;
        }
        col = (col / 26) - 1;
    }
    s.chars().rev().collect()
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
    let mut strings = Vec::new();
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
            let raw_text = &si_content[text_start..text_start + t_close];
            text.push_str(&unescape_xml(raw_text));
            t_search = text_start + t_close + 4;
        }
        strings.push(text);
    }
    strings
}

fn parse_sheet_cells(xml: &str, shared_strings: &[String]) -> BTreeMap<String, Cell> {
    let mut cells = BTreeMap::new();
    let mut search_from = 0;
    while let Some(c_rel) = xml[search_from..].find("<c ") {
        let c_start = search_from + c_rel;
        let Some(open_tag_end_rel) = xml[c_start..].find('>') else {
            break;
        };
        let open_tag_end = c_start + open_tag_end_rel;
        let open_tag = &xml[c_start..=open_tag_end];

        let cell_xml = if open_tag.ends_with("/>") {
            search_from = open_tag_end + 1;
            open_tag
        } else {
            let Some(close_rel) = xml[open_tag_end..].find("</c>") else {
                break;
            };
            let close_end = open_tag_end + close_rel + 4;
            let full = &xml[c_start..close_end];
            search_from = close_end;
            full
        };

        let Some(cell_ref) = extract_attr(open_tag, "r") else {
            continue;
        };

        let cell = if cell_xml.contains("<f>") || cell_xml.contains("<f ") {
            Cell::NotText("formula".to_owned())
        } else {
            let t_attr = extract_attr(open_tag, "t");
            match t_attr {
                Some("s") => {
                    if let Some(v_start) = cell_xml.find("<v>") {
                        let after_v = &cell_xml[v_start + 3..];
                        if let Some(v_end) = after_v.find("</v>") {
                            let idx_str = &after_v[..v_end];
                            if let Ok(idx) = idx_str.parse::<usize>() {
                                if let Some(val) = shared_strings.get(idx) {
                                    Cell::Text(val.clone())
                                } else {
                                    Cell::NotText(format!("missing shared string index {idx}"))
                                }
                            } else {
                                Cell::NotText(format!("invalid shared string index {idx_str}"))
                            }
                        } else {
                            Cell::NotText("missing closing </v>".to_owned())
                        }
                    } else {
                        Cell::Text(String::new())
                    }
                }
                Some("inlineStr") => Cell::NotText("inline string, not a string".to_owned()),
                Some("b") => Cell::NotText("boolean, not a string".to_owned()),
                Some("d") => Cell::NotText("date, not a string".to_owned()),
                Some("e") => Cell::NotText("error, not a string".to_owned()),
                Some("str") => Cell::NotText("formula string, not a string".to_owned()),
                Some("n") | None => Cell::NotText("number, not a string".to_owned()),
                Some(other) => Cell::NotText(format!("{other}, not a string")),
            }
        };

        cells.insert(cell_ref.to_owned(), cell);
    }
    cells
}

pub fn read_workbook(bytes: &[u8]) -> Vec<SheetData> {
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
            .unwrap_or_else(|_| panic!("{sheet_path} exists"));
        let mut xml = String::new();
        f.read_to_string(&mut xml).unwrap();
        let cells = parse_sheet_cells(&xml, &shared_strings);
        sheets.push(SheetData { name, cells, xml });
    }

    sheets
}

pub fn violations(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = Cursor::new(bytes);
    let mut archive = match ZipArchive::new(&mut cursor) {
        Ok(a) => a,
        Err(e) => return vec![format!("archive: invalid zip: {e}")],
    };

    for i in 0..archive.len() {
        let Ok(file) = archive.by_index(i) else {
            continue;
        };
        let Ok(name) = file.name() else {
            continue;
        };
        let name_str = name.as_ref();
        if name_str == "xl/vbaProject.bin" {
            out.push("xl/vbaProject.bin: VBA macro project found".to_owned());
        }
        if name_str.starts_with("xl/externalLinks/") {
            out.push(format!("{name_str}: external link entry found"));
        }
    }

    for i in 0..archive.len() {
        let (name_str, is_rels) = {
            let Ok(file) = archive.by_index(i) else {
                continue;
            };
            let Ok(name) = file.name() else {
                continue;
            };
            (name.to_string(), name.ends_with(".rels"))
        };
        if !is_rels {
            continue;
        }
        if let Ok(mut file) = archive.by_name(&name_str) {
            let mut content = String::new();
            if file.read_to_string(&mut content).is_ok()
                && (content.contains(
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink",
                ) || content.contains("/relationships/hyperlink"))
            {
                out.push(format!("{name_str}: hyperlink relationship found"));
            }
        }
    }

    let sheets = read_workbook(bytes);
    for sheet in &sheets {
        if sheet.xml.contains("<hyperlink") {
            out.push(format!("{}: hyperlink element found in sheet", sheet.name));
        }
        if sheet.xml.contains("<f>") || sheet.xml.contains("<f ") {
            out.push(format!("{}: sheet contains formula element", sheet.name));
        }
        for (cell_ref, cell) in &sheet.cells {
            if let Cell::NotText(desc) = cell {
                out.push(format!("{}!{cell_ref}: {desc}", sheet.name));
            }
        }
    }

    out
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

fn fixture_view_with_tree<T>(
    edit_tree: impl FnOnce(&mut Tree),
    f: impl FnOnce(&LabelledView) -> T,
) -> T {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
    let mut tree = Tree::new();
    insert_all(&mut tree, &root.join("corpus"), "corpus");
    edit_tree(&mut tree);
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

fn fixture_view_with<T>(f: impl FnOnce(&LabelledView) -> T) -> T {
    fixture_view_with_tree(|_| {}, f)
}

fn template_csv(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/templates/workbooks")
        .join(format!("{name}.csv"));
    std::fs::read(path).unwrap()
}

fn zip_entry_text(bytes: &[u8], name: &str) -> String {
    let mut cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(&mut cursor).unwrap();
    let mut file = archive.by_name(name).unwrap();
    let mut s = String::new();
    file.read_to_string(&mut s).unwrap();
    s
}

fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
    let mut cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(&mut cursor).unwrap();
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i).unwrap();
        names.push(file.name().unwrap().to_string());
    }
    names
}

fn insert_all_skipping(tree: &mut Tree, dir: &Path, rel: &str, skip: &impl Fn(&str) -> bool) {
    tree.insert_dir(rel);
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let child = format!("{rel}/{name}");
        if entry.file_type().unwrap().is_dir() {
            insert_all_skipping(tree, &entry.path(), &child, skip);
        } else if !skip(&child) {
            tree.insert_bytes(child, std::fs::read(entry.path()).unwrap());
        }
    }
}

fn fixture_view_skipping<T>(skip: impl Fn(&str) -> bool, f: impl FnOnce(&LabelledView) -> T) -> T {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
    let mut tree = Tree::new();
    insert_all_skipping(&mut tree, &root.join("corpus"), "corpus", &skip);
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

fn date_shape_matches(text: &str) -> Vec<String> {
    let mut matches = Vec::new();
    let bytes = text.as_bytes();
    if bytes.len() >= 10 {
        for i in 0..=(bytes.len() - 10) {
            let slice = &bytes[i..i + 10];
            if slice[0..4].iter().all(u8::is_ascii_digit)
                && slice[4] == b'-'
                && slice[5..7].iter().all(u8::is_ascii_digit)
                && slice[7] == b'-'
                && slice[8..10].iter().all(u8::is_ascii_digit)
            {
                matches.push(std::str::from_utf8(slice).unwrap().to_owned());
            }
        }
    }
    matches
}

#[test]
fn every_template_header_equals_its_csv() {
    for spec in TEMPLATES {
        let spec: &TemplateSpec = spec;
        let csv = template_csv(spec.name);
        let table = table::read(&csv).unwrap();
        let expected_headers: Vec<&str> = spec.columns.iter().map(|c| c.header).collect();
        let csv_headers: Vec<&str> = table.header.iter().map(String::as_str).collect();
        assert_eq!(
            csv_headers, expected_headers,
            "CSV headers mismatch for {}",
            spec.name
        );

        let bytes = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();
        let sheets = read_workbook(&bytes);
        let data_sheet = sheets
            .iter()
            .find(|s| s.name == spec.sheet)
            .unwrap_or_else(|| panic!("data sheet `{}` missing in {}", spec.sheet, spec.name));

        for (c_idx, col) in spec.columns.iter().enumerate() {
            let col: &Column = col;
            let coord = format!("{}1", col_to_letters(c_idx));
            let cell = data_sheet
                .cells
                .get(&coord)
                .unwrap_or_else(|| panic!("missing header cell at {coord} in {}", spec.sheet));
            match cell {
                Cell::Text(val) => {
                    assert_eq!(
                        val, col.header,
                        "header mismatch at {coord} in {}",
                        spec.sheet
                    );
                }
                Cell::NotText(what) => {
                    panic!(
                        "header cell at {coord} in {} is not text: {what}",
                        spec.sheet
                    );
                }
            }
        }

        let row1_count = data_sheet
            .cells
            .keys()
            .filter(|k| k.trim_start_matches(|c: char| c.is_ascii_alphabetic()) == "1")
            .count();
        assert_eq!(
            row1_count,
            spec.columns.len(),
            "exact header cell count mismatch for {}",
            spec.sheet
        );
    }
}

#[test]
fn each_template_has_a_read_me_sheet_and_one_table() {
    for spec in TEMPLATES {
        let csv = template_csv(spec.name);
        let table = table::read(&csv).unwrap();
        assert_eq!(
            table.rows.len(),
            3,
            "expected 3 example rows for {}",
            spec.name
        );

        let bytes = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();
        let sheets = read_workbook(&bytes);
        let sheet_names: Vec<&str> = sheets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            sheet_names,
            vec!["Read me", spec.sheet],
            "sheet names mismatch for {}",
            spec.name
        );

        let entry_names = zip_entry_names(&bytes);
        let table_entries: Vec<&str> = entry_names
            .iter()
            .filter(|n| n.starts_with("xl/tables/table") && n.ends_with(".xml"))
            .map(String::as_str)
            .collect();
        assert_eq!(
            table_entries.len(),
            1,
            "expected exactly one xl/tables/table*.xml entry in {}",
            spec.name
        );

        let table_xml = zip_entry_text(&bytes, table_entries[0]);
        let expected_last_col = col_to_letters(spec.columns.len().saturating_sub(1));
        let expected_last_row = 1 + table.rows.len();
        let expected_ref = format!("A1:{expected_last_col}{expected_last_row}");
        let actual_ref = extract_attr(&table_xml, "ref");
        assert_eq!(
            actual_ref,
            Some(expected_ref.as_str()),
            "table ref mismatch in {}",
            spec.name
        );

        for totals_marker in [
            "totalsRowCount",
            "totalsRowLabel",
            "totalsRowFunction",
            "totalsRowFormula",
        ] {
            assert!(
                !table_xml.contains(totals_marker),
                "table XML must not have a total row ({totals_marker}) in {}",
                spec.name
            );
        }
        assert!(
            table_xml.contains("totalsRowShown=\"0\""),
            "table XML must say the total row is not shown in {}",
            spec.name
        );
        assert!(
            !table_xml.contains("<calculatedColumnFormula"),
            "table XML must not have calculatedColumnFormula in {}",
            spec.name
        );

        let data_sheet = sheets
            .iter()
            .find(|s| s.name == spec.sheet)
            .unwrap_or_else(|| panic!("data sheet `{}` missing in {}", spec.sheet, spec.name));

        for (r_idx, row) in table.rows.iter().enumerate() {
            let row_num = r_idx + 2;
            for (c_idx, expected_val) in row.iter().enumerate() {
                let coord = format!("{}{row_num}", col_to_letters(c_idx));
                let cell = data_sheet.cells.get(&coord);
                if expected_val.is_empty() {
                    assert!(
                        cell.is_none(),
                        "expected empty cell at {coord} in {}, got: {cell:?}",
                        spec.name
                    );
                } else {
                    assert_eq!(
                        cell,
                        Some(&Cell::Text(expected_val.clone())),
                        "cell mismatch at {coord} in {}",
                        spec.name
                    );
                }
            }
        }
    }
}

// control: XL-01
#[test]
fn no_formula_macro_external_link_or_hyperlink() {
    for spec in TEMPLATES {
        let csv = template_csv(spec.name);
        let bytes = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();

        let viols = violations(&bytes);
        assert!(
            viols.is_empty(),
            "rendered template {} must have no violations, found: {viols:?}",
            spec.name
        );

        let sheets = read_workbook(&bytes);
        let data_sheet = sheets
            .iter()
            .find(|s| s.name == spec.sheet)
            .unwrap_or_else(|| panic!("data sheet `{}` missing in {}", spec.sheet, spec.name));

        let formula1_count = data_sheet.xml.matches("<formula1>").count();
        let closed_col_count = spec.columns.iter().filter(|c| c.list.is_some()).count();
        assert_eq!(
            formula1_count, closed_col_count,
            "<formula1> count mismatch for {}",
            spec.name
        );
    }

    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("claims").unwrap();
    ws.write_string(0, 0, "header").unwrap();
    ws.write_formula(1, 2, "=1+1").unwrap();
    ws.write_string(1, 0, "normal").unwrap();
    let bytes = wb.save_to_buffer().unwrap();

    let viols = violations(&bytes);
    assert!(
        !viols.is_empty(),
        "violations must be non-empty when a formula cell is present"
    );
    assert!(
        viols
            .iter()
            .any(|msg| msg.contains("claims!C2") && msg.contains("formula")),
        "expected violation containing `claims!C2` and `formula`, got: {viols:?}"
    );
}

#[test]
fn closed_columns_have_the_listed_values() {
    for spec in TEMPLATES {
        let csv = template_csv(spec.name);
        let table = table::read(&csv).unwrap();
        let bytes = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();
        let sheets = read_workbook(&bytes);
        let data_sheet = sheets
            .iter()
            .find(|s| s.name == spec.sheet)
            .unwrap_or_else(|| panic!("data sheet `{}` missing in {}", spec.sheet, spec.name));

        let mut validations: Vec<(String, String)> = Vec::new();
        let mut search_from = 0;
        while let Some(dv_rel) = data_sheet.xml[search_from..].find("<dataValidation ") {
            let dv_start = search_from + dv_rel;
            let Some(dv_end_rel) = data_sheet.xml[dv_start..].find("</dataValidation>") else {
                break;
            };
            let dv_end = dv_start + dv_end_rel + "</dataValidation>".len();
            let dv_xml = &data_sheet.xml[dv_start..dv_end];
            search_from = dv_end;

            let sqref = extract_attr(dv_xml, "sqref").unwrap();
            let f1_start = dv_xml.find("<formula1>").unwrap() + "<formula1>".len();
            let f1_end = dv_xml.find("</formula1>").unwrap();
            let formula1 = &dv_xml[f1_start..f1_end];
            validations.push((sqref.to_owned(), formula1.to_owned()));
        }

        let expected_end_row = 1 + table.rows.len() + 200;

        for (c_idx, col) in spec.columns.iter().enumerate() {
            let col_let = col_to_letters(c_idx);
            let expected_sqref = format!("{col_let}2:{col_let}{expected_end_row}");

            if let Some(list) = col.list {
                let matching = validations
                    .iter()
                    .find(|(sqref, _)| sqref == &expected_sqref);
                assert!(
                    matching.is_some(),
                    "missing dataValidation for column `{}` with sqref `{expected_sqref}` in {}",
                    col.header,
                    spec.name
                );
                let (_, formula1_raw) = matching.unwrap();
                let unescaped = unescape_xml(formula1_raw);
                let stripped = unescaped
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(&unescaped);
                assert_eq!(
                    stripped,
                    list.join(","),
                    "validation list mismatch for column `{}` in {}",
                    col.header,
                    spec.name
                );
            } else {
                let has_validation = validations
                    .iter()
                    .any(|(sqref, _)| sqref.starts_with(&format!("{col_let}2:")));
                assert!(
                    !has_validation,
                    "column `{}` without list should have no validation, found in {}",
                    col.header, spec.name
                );
            }
        }

        for row in &table.rows {
            for (c_idx, col) in spec.columns.iter().enumerate() {
                if let Some(list) = col.list {
                    let val = &row[c_idx];
                    assert!(
                        list.contains(&val.as_str()),
                        "example row value `{val}` for column `{}` not in list {list:?} in {}",
                        col.header,
                        spec.name
                    );
                }
            }
        }
    }
}

#[test]
fn read_me_names_the_bridge() {
    for spec in TEMPLATES {
        let csv = template_csv(spec.name);
        let bytes = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();
        let sheets = read_workbook(&bytes);
        let readme = sheets
            .iter()
            .find(|s| s.name == "Read me")
            .unwrap_or_else(|| panic!("Read me sheet missing in {}", spec.name));

        let mut all_text = String::new();
        for cell in readme.cells.values() {
            if let Cell::Text(t) = cell {
                all_text.push_str(t);
                all_text.push('\n');
            }
        }

        assert!(
            all_text.contains("Save as CSV UTF-8"),
            "Read me in {} must contain 'Save as CSV UTF-8'",
            spec.name
        );
        assert!(
            all_text.contains("drop folder"),
            "Read me in {} must contain 'drop folder'",
            spec.name
        );
        assert!(
            all_text.contains("recalculate"),
            "Read me in {} must contain 'recalculate'",
            spec.name
        );
        assert!(
            all_text.contains("Synthetic example rows: replace them."),
            "Read me in {} must contain 'Synthetic example rows: replace them.'",
            spec.name
        );

        for col in spec.columns {
            if let Some(list) = col.list {
                let expected_line = format!("{}: {}", col.header, list.join(", "));
                assert!(
                    all_text.contains(&expected_line),
                    "Read me in {} must contain closed list line `{expected_line}`",
                    spec.name
                );
            }
        }
    }
}

#[test]
fn plan_template_has_no_date_column_and_no_calendar_date() {
    let spec_plan = TEMPLATES.iter().find(|s| s.name == "90-day-plan").unwrap();
    for col in spec_plan.columns {
        let lower = col.header.to_ascii_lowercase();
        assert_ne!(lower, "date", "plan header cannot be 'date'");
        assert_ne!(lower, "start", "plan header cannot be 'start'");
        assert!(!lower.contains("date"), "plan header cannot contain 'date'");
    }

    let plan_csv = template_csv("90-day-plan");
    let plan_table = table::read(&plan_csv).unwrap();
    for h in plan_table.header.iter() {
        let lower = h.to_ascii_lowercase();
        assert_ne!(lower, "date", "plan CSV header cannot be 'date'");
        assert_ne!(lower, "start", "plan CSV header cannot be 'start'");
        assert!(
            !lower.contains("date"),
            "plan CSV header cannot contain 'date'"
        );
    }

    let plan_bytes = templates_xlsx::render("90-day-plan", &plan_csv, 1_772_366_400).unwrap();
    let plan_sheets = read_workbook(&plan_bytes);
    for sheet in &plan_sheets {
        for (coord, cell) in &sheet.cells {
            if let Cell::Text(val) = cell {
                let dates = date_shape_matches(val);
                assert!(
                    dates.is_empty(),
                    "plan sheet `{}` cell {coord} contains date pattern: {dates:?}",
                    sheet.name
                );
            }
        }
    }

    for name in ["discovery-log", "assumption-register"] {
        let csv = template_csv(name);
        let bytes = templates_xlsx::render(name, &csv, 1_772_366_400).unwrap();
        let sheets = read_workbook(&bytes);
        let mut found_dates = Vec::new();
        for sheet in &sheets {
            for cell in sheet.cells.values() {
                if let Cell::Text(val) = cell {
                    found_dates.extend(date_shape_matches(val));
                }
            }
        }
        assert!(
            !found_dates.is_empty(),
            "{name} must contain at least one synthetic date"
        );
        for d in &found_dates {
            assert_eq!(
                d, "2000-01-01",
                "unexpected calendar date `{d}` in {name}, only synthetic date 2000-01-01 allowed"
            );
        }
    }
}

#[test]
fn core_xml_has_the_fixed_time() {
    for spec in TEMPLATES {
        let csv = template_csv(spec.name);
        let bytes = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();
        let core_xml = zip_entry_text(&bytes, "docProps/core.xml");
        assert!(
            core_xml.contains(
                "<dcterms:created xsi:type=\"dcterms:W3CDTF\">2026-03-01T12:00:00Z</dcterms:created>"
            ),
            "docProps/core.xml must contain exact created element for {}, got: {core_xml}",
            spec.name
        );
    }

    fixture_view_with(|view| {
        let site = site::render(view, &BuildInfo::default()).unwrap();
        for spec in TEMPLATES {
            let path = templates_xlsx::output_path(spec.name);
            let bytes = site
                .files()
                .get(&path)
                .unwrap_or_else(|| panic!("site file `{path}` must exist"));
            let core_xml = zip_entry_text(bytes, "docProps/core.xml");
            assert!(
                core_xml.contains("2000-01-01T00:00:00Z"),
                "docProps/core.xml for {path} must contain 2000-01-01T00:00:00Z, got: {core_xml}"
            );
        }
    });
}

// control: DT-01
#[test]
fn two_builds_same_bytes() {
    let mut parent_renders = BTreeMap::new();
    for spec in TEMPLATES {
        let csv = template_csv(spec.name);
        let bytes1 = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();
        let bytes2 = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();
        assert_eq!(
            bytes1, bytes2,
            "two parent renders in the same process must be identical for {}",
            spec.name
        );
        assert!(
            bytes1.len() > 1000,
            "template build must not be trivially empty for {}",
            spec.name
        );
        let archive = ZipArchive::new(Cursor::new(&bytes1)).unwrap();
        assert!(
            !archive.is_empty(),
            "parent zip must have entries for {}",
            spec.name
        );
        parent_renders.insert(spec.name, bytes1);
    }

    let out_dir = std::env::temp_dir().join(format!("fb-templates-test-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).unwrap();

    // A build that read the clock would differ across a second boundary.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new(exe)
        .args([
            "--exact",
            "build_templates_in_child",
            "--ignored",
            "--nocapture",
        ])
        .env("TZ", "Pacific/Auckland")
        .env("FB_TEMPLATES_OUT", &out_dir)
        .current_dir(&out_dir)
        .status()
        .unwrap();

    assert!(status.success(), "child process must exit successfully");

    let mut child_renders = BTreeMap::new();
    for spec in TEMPLATES {
        let child_file = out_dir.join(format!("{}.xlsx", spec.name));
        assert!(
            child_file.exists(),
            "child output file {}.xlsx must exist",
            spec.name
        );
        let child_bytes = std::fs::read(&child_file).unwrap();
        child_renders.insert(spec.name, child_bytes);
    }

    let _ = std::fs::remove_dir_all(&out_dir);

    for spec in TEMPLATES {
        let parent_bytes = parent_renders.get(spec.name).unwrap();
        let child_bytes = child_renders.get(spec.name).unwrap();
        assert_eq!(
            parent_bytes, child_bytes,
            "parent and child builds must produce identical bytes for {}",
            spec.name
        );
    }
}

#[test]
#[ignore = "started by two_builds_same_bytes in a child process"]
fn build_templates_in_child() {
    let Ok(out_str) = std::env::var("FB_TEMPLATES_OUT") else {
        return;
    };
    let out_dir = Path::new(&out_str);
    for spec in TEMPLATES {
        let csv = template_csv(spec.name);
        let bytes = templates_xlsx::render(spec.name, &csv, 1_772_366_400).unwrap();
        std::fs::write(out_dir.join(format!("{}.xlsx", spec.name)), bytes).unwrap();
    }
}

#[test]
fn a_wrong_header_is_a_template_error() {
    let csv = template_csv("assumption-register");
    // The shipped CSV is the baseline: it renders, so only the edit below can break the header.
    templates_xlsx::render("assumption-register", &csv, 1_772_366_400).unwrap();
    let csv_str = std::str::from_utf8(&csv).unwrap();
    let modified = csv_str.replace("evidence_grade", "evidence_grade2");
    assert_ne!(modified, csv_str, "the edit must change the header");
    let err = templates_xlsx::render("assumption-register", modified.as_bytes(), 1_772_366_400)
        .unwrap_err();
    let TemplateError::Header {
        template,
        column,
        detail: _,
    } = &err
    else {
        panic!("expected TemplateError::Header, got {err:?}");
    };
    assert_eq!(template, "assumption-register");
    assert_eq!(column, "evidence_grade");
    let display = err.to_string();
    assert!(
        display.contains("assumption-register"),
        "Display text must contain template name: {display}"
    );
    assert!(
        display.contains("evidence_grade"),
        "Display text must contain column name: {display}"
    );

    let dl_csv = template_csv("discovery-log");
    let dl_str = std::str::from_utf8(&dl_csv).unwrap();
    let missing_lines: Vec<&str> = dl_str
        .lines()
        .map(|line| line.rsplit_once(',').unwrap().0)
        .collect();
    let missing_csv = missing_lines.join("\n");
    let err_missing =
        templates_xlsx::render("discovery-log", missing_csv.as_bytes(), 1_772_366_400).unwrap_err();
    assert!(
        matches!(err_missing, TemplateError::Header { .. }),
        "expected TemplateError::Header for missing column, got: {err_missing:?}"
    );

    let extra_lines: Vec<String> = dl_str
        .lines()
        .map(|line| format!("{line},extra_col"))
        .collect();
    let extra_csv = extra_lines.join("\n");
    let err_extra =
        templates_xlsx::render("discovery-log", extra_csv.as_bytes(), 1_772_366_400).unwrap_err();
    assert!(
        matches!(err_extra, TemplateError::Header { .. }),
        "expected TemplateError::Header for extra column, got: {err_extra:?}"
    );

    let err_unknown = templates_xlsx::render("no-such", b"a,b\n1,2", 1_772_366_400).unwrap_err();
    assert_eq!(
        err_unknown,
        TemplateError::UnknownTemplate {
            name: "no-such".to_owned()
        }
    );

    let unbalanced = b"col1,col2\n\"unclosed,1";
    let err_csv = templates_xlsx::render("discovery-log", unbalanced, 1_772_366_400).unwrap_err();
    assert!(
        matches!(err_csv, TemplateError::Csv { .. }),
        "expected TemplateError::Csv for unbalanced quote, got: {err_csv:?}"
    );
}

#[test]
fn a_missing_template_csv_renders_no_file() {
    fixture_view_skipping(
        |p| p == "corpus/templates/workbooks/discovery-log.csv",
        |view| {
            let info = BuildInfo::default();
            let ctx = site::render_context(view, &info).unwrap();
            let site = site::render_with_info(view, &ctx, &info).unwrap();

            assert!(
                !site.files().contains_key("templates/discovery-log.xlsx"),
                "site must not hold discovery-log.xlsx when source CSV is absent"
            );
            assert!(
                site.files()
                    .contains_key("templates/assumption-register.xlsx"),
                "site must hold assumption-register.xlsx"
            );
            assert!(
                site.files().contains_key("templates/90-day-plan.xlsx"),
                "site must hold 90-day-plan.xlsx"
            );

            let site_planned = site::planned_paths(view);
            assert!(
                !site_planned.contains("templates/discovery-log.xlsx"),
                "site planned paths must not contain discovery-log.xlsx"
            );
            assert!(
                site_planned.contains("templates/assumption-register.xlsx"),
                "site planned paths must contain assumption-register.xlsx"
            );
            assert!(
                site_planned.contains("templates/90-day-plan.xlsx"),
                "site planned paths must contain 90-day-plan.xlsx"
            );

            let t_planned = templates_xlsx::planned_paths(view);
            assert_eq!(
                t_planned,
                vec![
                    "templates/assumption-register.xlsx",
                    "templates/90-day-plan.xlsx",
                ]
            );
            let mut t_planned_sorted = t_planned;
            t_planned_sorted.sort();
            assert_eq!(
                t_planned_sorted,
                vec![
                    "templates/90-day-plan.xlsx",
                    "templates/assumption-register.xlsx",
                ]
            );
        },
    );

    fixture_view_skipping(
        |p| p.starts_with("corpus/templates/workbooks/"),
        |view| {
            let info = BuildInfo::default();
            let ctx = site::render_context(view, &info).unwrap();
            let site = site::render_with_info(view, &ctx, &info).unwrap();

            assert!(
                !site.files().keys().any(|k| k.starts_with("templates/")),
                "site must hold no templates/ files when all template CSVs are absent"
            );
            assert!(
                templates_xlsx::planned_paths(view).is_empty(),
                "templates_xlsx::planned_paths must be empty"
            );
        },
    );
}

#[test]
fn a_malformed_template_csv_names_its_path() {
    let path = "corpus/templates/workbooks/90-day-plan.csv";
    fixture_view_with_tree(
        |tree| {
            tree.insert_bytes(
                path,
                b"row_id,window,wrong_col,duty,proof_test,owner_role,status,drafted_by\n1,Day 1 to Day 30,item,duty,proof,owner,open,ai\n".to_vec(),
            );
        },
        |view| {
            let err = templates_xlsx::render_all(view, 1_772_366_400).unwrap_err();
            assert_eq!(err.register, path);
            assert_eq!(err.row, "");
            assert_eq!(err.column, "");
            let msg = err.to_string();
            assert!(
                msg.contains(path),
                "WorkbookError to_string() must contain `{path}`, got: {msg}"
            );

            let info = BuildInfo::new(None, None, BuildProfile::Silent, Some(1_772_366_400));
            let ctx = site::render_context(view, &info).unwrap();
            let site_err = site::render_with_info(view, &ctx, &info).unwrap_err();
            assert_eq!(site_err.register, path);
            assert_eq!(site_err.row, "");
            assert_eq!(site_err.column, "");
            assert_eq!(site_err.to_string(), msg);
            assert_eq!(site_err, err);
        },
    );
}

#[test]
fn a_rendered_site_holds_the_three_templates() {
    fixture_view_with(|view| {
        let site = site::render(view, &BuildInfo::default()).unwrap();
        let template_files: Vec<&str> = site
            .files()
            .keys()
            .filter(|p| p.starts_with("templates/"))
            .map(String::as_str)
            .collect();

        assert_eq!(
            template_files,
            vec![
                "templates/90-day-plan.xlsx",
                "templates/assumption-register.xlsx",
                "templates/discovery-log.xlsx",
            ]
        );

        let planned = site::planned_paths(view);
        let planned_keys: Vec<&str> = planned.iter().collect();
        let site_keys: Vec<&str> = site.files().keys().map(String::as_str).collect();
        assert_eq!(
            planned_keys, site_keys,
            "planned_paths must match site keys exactly"
        );
    });
}
