//! Tests for the workbook copy of the registers (`data/registers.xlsx`) and CSV copies (`xlsx-determinism`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read fixtures, inspect files, read env vars, start child process"
)]
#![expect(
    clippy::disallowed_types,
    reason = "child process command and temporary files"
)]

use std::collections::{BTreeMap, BTreeSet};
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
use fieldbook_core::schema::{ColumnKind, Register};
use fieldbook_render::csv_copy;
use fieldbook_render::site::{self, planned_paths};
use fieldbook_render::xlsx;

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

fn build_workbook_from_fixture(root: &Path, epoch: i64) -> Vec<u8> {
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
    xlsx::render(&view, epoch).unwrap()
}

#[test]
fn core_xml_has_the_fixed_time() {
    fixture_view_with(|view| {
        let bytes = xlsx::render(view, 1_772_366_400).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut core_file = archive.by_name("docProps/core.xml").unwrap();
        let mut core_xml = String::new();
        core_file.read_to_string(&mut core_xml).unwrap();
        assert!(
            core_xml.contains(
                "<dcterms:created xsi:type=\"dcterms:W3CDTF\">2026-03-01T12:00:00Z</dcterms:created>"
            ),
            "docProps/core.xml must contain exact created element, got: {core_xml}"
        );
    });
}

#[test]
fn workbook_without_epoch_has_fixed_creation_time() {
    fixture_view_with(|view| {
        let site1 = site::render(view, &BuildInfo::default()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let site2 = site::render(view, &BuildInfo::default()).unwrap();

        let bytes1 = site1.files().get(xlsx::OUTPUT_PATH).unwrap();
        let bytes2 = site2.files().get(xlsx::OUTPUT_PATH).unwrap();

        assert_eq!(
            bytes1, bytes2,
            "workbooks built at different times without epoch must be byte identical"
        );

        let mut archive = ZipArchive::new(Cursor::new(bytes1)).unwrap();
        let mut core_file = archive.by_name("docProps/core.xml").unwrap();
        let mut core_xml = String::new();
        core_file.read_to_string(&mut core_xml).unwrap();

        assert!(
            core_xml.contains("2000-01-01T00:00:00Z"),
            "docProps/core.xml must contain fallback epoch 2000-01-01T00:00:00Z, got: {core_xml}"
        );
    });
}

#[test]
fn no_formula_macro_external_link_or_hyperlink() {
    fixture_view_with(|view| {
        let bytes = xlsx::render(view, 1_772_366_400).unwrap();
        let viols = violations(&bytes);
        assert!(
            viols.is_empty(),
            "rendered fixture workbook must have no violations, found: {viols:?}"
        );
    });
}

#[test]
fn every_cell_is_a_string_equal_to_its_register_cell() {
    fixture_view_with(|view| {
        let bytes = xlsx::render(view, 1_772_366_400).unwrap();
        let sheets = read_workbook(&bytes);
        let sheets_by_name: BTreeMap<String, &SheetData> =
            sheets.iter().map(|s| (s.name.clone(), s)).collect();

        let mut total_data_rows = 0;
        let mut checked_version_digit = false;

        for &reg in &Register::ALL {
            let name = xlsx::sheet_name(reg);
            let sheet = sheets_by_name
                .get(name)
                .unwrap_or_else(|| panic!("sheet `{name}` must exist"));

            let reg_rows = view.register_rows(reg);
            total_data_rows += reg_rows.len();
            let columns = reg.columns();

            let mut expected_cell_coords = BTreeSet::new();

            for (c_idx, _col) in columns.iter().enumerate() {
                let coord = format!("{}1", col_to_letters(c_idx));
                expected_cell_coords.insert(coord);
            }

            for (r_idx, row) in reg_rows.iter().enumerate() {
                let sheet_row = r_idx + 2;
                for (c_idx, col) in columns.iter().enumerate() {
                    let expected_val = row.get(c_idx).map_or("", String::as_str);
                    let coord = format!("{}{sheet_row}", col_to_letters(c_idx));

                    if !expected_val.is_empty() {
                        expected_cell_coords.insert(coord.clone());
                    }

                    let cell = sheet.cells.get(&coord);
                    match cell {
                        Some(Cell::Text(val)) => {
                            assert_eq!(
                                val, expected_val,
                                "cell text mismatch at {}!{coord}",
                                sheet.name
                            );
                            if reg == Register::Sources && col.name == "version" {
                                assert!(
                                    !val.is_empty() && val.chars().all(|ch| ch.is_ascii_digit()),
                                    "sources!{coord} version must be a digit string, got: {val}"
                                );
                                checked_version_digit = true;
                            }
                        }
                        Some(Cell::NotText(what)) => {
                            panic!(
                                "expected text cell at {}!{coord}, got non-text: {what}",
                                sheet.name
                            );
                        }
                        None => {
                            assert!(
                                expected_val.is_empty(),
                                "missing cell at {}!{coord}, expected: `{expected_val}`",
                                sheet.name
                            );
                        }
                    }
                }
            }

            for coord in sheet.cells.keys() {
                assert!(
                    expected_cell_coords.contains(coord),
                    "extra unexpected cell found at {}!{coord}",
                    sheet.name
                );
            }
        }

        assert!(
            total_data_rows > 0,
            "at least one register must have data rows"
        );
        assert!(
            checked_version_digit,
            "sources version digit cell must be verified"
        );
    });
}

#[test]
fn header_frozen_with_autofilter() {
    fixture_view_with(|view| {
        let bytes = xlsx::render(view, 1_772_366_400).unwrap();
        let sheets = read_workbook(&bytes);
        let sheets_by_name: BTreeMap<String, &SheetData> =
            sheets.iter().map(|s| (s.name.clone(), s)).collect();

        for &reg in &Register::ALL {
            let name = xlsx::sheet_name(reg);
            let sheet = sheets_by_name
                .get(name)
                .unwrap_or_else(|| panic!("sheet `{name}` must exist"));

            let pane_pos = sheet
                .xml
                .find("<pane ")
                .unwrap_or_else(|| panic!("sheet `{name}` must contain <pane element"));
            let pane_end = sheet.xml[pane_pos..]
                .find('>')
                .unwrap_or_else(|| panic!("malformed <pane tag in sheet `{name}`"));
            let pane_tag = &sheet.xml[pane_pos..pane_pos + pane_end + 1];

            assert!(
                pane_tag.contains("ySplit=\"1\""),
                "sheet `{name}` pane must have ySplit=\"1\", got: {pane_tag}"
            );
            assert!(
                pane_tag.contains("state=\"frozen\""),
                "sheet `{name}` pane must have state=\"frozen\", got: {pane_tag}"
            );
            assert!(
                !pane_tag.contains("xSplit="),
                "sheet `{name}` pane must not contain xSplit, got: {pane_tag}"
            );

            let data_rows = view.register_rows(reg);
            let last_row = data_rows.len().max(1) + 1;
            let last_col_letters = col_to_letters(reg.columns().len().saturating_sub(1));
            let expected_ref = format!("A1:{last_col_letters}{last_row}");
            let expected_autofilter = format!("<autoFilter ref=\"{expected_ref}\"");

            assert!(
                sheet.xml.contains(&expected_autofilter),
                "sheet `{name}` must contain `{expected_autofilter}`, xml: {}",
                sheet.xml
            );
        }
    });
}

#[test]
fn every_header_equals_register_columns() {
    fixture_view_with(|view| {
        let bytes = xlsx::render(view, 1_772_366_400).unwrap();
        let sheets = read_workbook(&bytes);
        let sheets_by_name: BTreeMap<String, &SheetData> =
            sheets.iter().map(|s| (s.name.clone(), s)).collect();

        for &reg in &Register::ALL {
            let name = xlsx::sheet_name(reg);
            let sheet = sheets_by_name
                .get(name)
                .unwrap_or_else(|| panic!("sheet `{name}` must exist"));
            let columns = reg.columns();

            for (c_idx, col) in columns.iter().enumerate() {
                let coord = format!("{}1", col_to_letters(c_idx));
                let cell = sheet
                    .cells
                    .get(&coord)
                    .unwrap_or_else(|| panic!("missing header cell at {}!{coord}", sheet.name));
                match cell {
                    Cell::Text(val) => {
                        assert_eq!(
                            val, col.name,
                            "header name mismatch at {}!{coord}: expected `{}`, got `{val}`",
                            sheet.name, col.name
                        );
                    }
                    Cell::NotText(what) => {
                        panic!("header cell at {}!{coord} is not text: {what}", sheet.name);
                    }
                }
            }
        }
    });
}

// control: XL-01
#[test]
fn readback_flags_a_formula_cell() {
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

// control: DT-01
#[test]
fn two_builds_same_bytes() {
    let fixture_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
    let info = BuildInfo::new(None, None, BuildProfile::Silent, Some(1_772_366_400));
    let parent_bytes = build_workbook_from_fixture(&fixture_dir, info.workbook_epoch);

    assert!(
        parent_bytes.len() > 1000,
        "build must not be trivially empty"
    );
    let archive = ZipArchive::new(Cursor::new(&parent_bytes)).unwrap();
    assert!(!archive.is_empty(), "parent zip must have entries");

    let out_dir = std::env::temp_dir().join(format!("fb-xlsx-test-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).unwrap();

    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new(exe)
        .args(["--exact", "build_in_child", "--ignored", "--nocapture"])
        .env("TZ", "Pacific/Auckland")
        .env("FB_XLSX_FIXTURE", &fixture_dir)
        .env("FB_XLSX_OUT", &out_dir)
        .current_dir(&out_dir)
        .status()
        .unwrap();

    assert!(status.success(), "child process must exit successfully");
    let child_file = out_dir.join("registers.xlsx");
    assert!(
        child_file.exists(),
        "child output file registers.xlsx must exist"
    );
    let child_bytes = std::fs::read(&child_file).unwrap();
    let _ = std::fs::remove_dir_all(&out_dir);

    assert_eq!(
        parent_bytes, child_bytes,
        "parent and child builds must produce identical bytes"
    );
}

#[test]
#[ignore = "started by two_builds_same_bytes in a child process"]
fn build_in_child() {
    let Ok(fixture_str) = std::env::var("FB_XLSX_FIXTURE") else {
        return;
    };
    let Ok(out_str) = std::env::var("FB_XLSX_OUT") else {
        return;
    };
    let fixture_dir = Path::new(&fixture_str);
    let out_dir = Path::new(&out_str);
    let bytes = build_workbook_from_fixture(fixture_dir, 1_772_366_400);
    std::fs::write(out_dir.join("registers.xlsx"), bytes).unwrap();
}

// control: SC-06
#[test]
fn csv_copy_has_bom_crlf_and_guard() {
    fixture_view_with(|view| {
        let site = site::render(view, &BuildInfo::default()).unwrap();
        assert!(
            !site.files().contains_key("data/claims.csv"),
            "site must not contain unparted data/claims.csv"
        );

        let check_csv_bytes = |label: &str, reg: Register, csv_bytes: &[u8]| {
            // 1. Starts with UTF-8 BOM EF BB BF
            assert!(
                csv_bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
                "CSV for {label} must start with UTF-8 BOM"
            );

            // 2. Every \n is preceded by \r (CRLF)
            for (i, &b) in csv_bytes.iter().enumerate() {
                if b == b'\n' {
                    assert!(
                        i > 0 && csv_bytes[i - 1] == b'\r',
                        "Every LF must be preceded by CR in CSV for {label} at index {i}"
                    );
                }
            }

            // 3. First line (after BOM) equals column names joined by ','
            let csv_text = std::str::from_utf8(&csv_bytes[3..]).unwrap();
            let first_line = csv_text
                .split("\r\n")
                .next()
                .unwrap_or_else(|| panic!("first line exists for {label}"));
            let expected_header = reg
                .columns()
                .iter()
                .map(|col| col.name)
                .collect::<Vec<_>>()
                .join(",");
            assert_eq!(
                first_line, expected_header,
                "header line mismatch in CSV for {label}"
            );
        };

        for &reg in &Register::ALL {
            if reg == Register::Claims {
                continue;
            }
            let csv_bytes = csv_copy::render(view, reg);
            let out_path = csv_copy::output_path(reg);
            check_csv_bytes(&out_path, reg, &csv_bytes);

            // 4. Site's data/<register>.csv equals csv_copy::render
            let site_csv = site
                .files()
                .get(&out_path)
                .unwrap_or_else(|| panic!("site must contain {out_path}"));
            assert_eq!(
                site_csv, &csv_bytes,
                "site file {out_path} must match csv_copy::render exactly"
            );
        }

        let part_csvs = csv_copy::render_claims_parts(view);
        assert!(
            !part_csvs.is_empty(),
            "claims part CSVs must be non-empty on fixture"
        );
        for (out_path, csv_bytes) in part_csvs {
            check_csv_bytes(&out_path, Register::Claims, &csv_bytes);
            let site_csv = site
                .files()
                .get(&out_path)
                .unwrap_or_else(|| panic!("site must contain {out_path}"));
            assert_eq!(
                site_csv, &csv_bytes,
                "site file {out_path} must match csv_copy::render_claims_parts exactly"
            );
        }
    });

    // Synthetic cell check with leading '=' on both single-file register and claims part CSV
    fixture_view_with_tree(
        |tree| {
            let sources_bytes = tree
                .get_bytes("corpus/registers/sources.csv")
                .expect("sources.csv in fixture tree");
            let sources_str = std::str::from_utf8(sources_bytes).unwrap();
            let modified = sources_str.replace(
                "Synthetic fixture source for automated tests.",
                "=synthetic-formula",
            );
            tree.insert_bytes("corpus/registers/sources.csv", modified.into_bytes());

            let claims_bytes = tree
                .get_bytes("corpus/registers/claims.csv")
                .expect("claims.csv in fixture tree");
            let claims_str = std::str::from_utf8(claims_bytes).unwrap();
            let modified_claims = claims_str.replace(
                "Synthetic owner-verified fact for fixture tests.",
                "=synthetic-claim-formula",
            );
            tree.insert_bytes("corpus/registers/claims.csv", modified_claims.into_bytes());
        },
        |view| {
            // Note: `load` reports diagnostic FB0403 for the synthetic '=' formula cell,
            // but the row still reaches the model and the view.
            let sources_rows = view.register_rows(Register::Sources);
            assert!(
                !sources_rows.is_empty(),
                "sources rows must reach the view despite FB0403"
            );
            let rendered = csv_copy::render(view, Register::Sources);
            let s = std::str::from_utf8(&rendered).unwrap();

            // Cell starting with '=' comes out as '=...
            assert!(
                s.contains("'=synthetic-formula"),
                "guarded cell '=synthetic-formula must appear in CSV, got: {s}"
            );
            // Unguarded text does not appear at the start of a cell (after ',' or after '\n')
            assert!(
                !s.contains(",=synthetic-formula") && !s.contains("\n=synthetic-formula"),
                "unguarded =synthetic-formula must not appear at cell start in CSV"
            );

            let claim_parts = csv_copy::render_claims_parts(view);
            let cmp_part = claim_parts
                .iter()
                .find(|(p, _)| p == "data/claims-CMP-1.csv")
                .expect("data/claims-CMP-1.csv present");
            let cmp_s = std::str::from_utf8(&cmp_part.1).unwrap();
            assert!(
                cmp_s.contains("'=synthetic-claim-formula"),
                "guarded cell '=synthetic-claim-formula must appear in claims part CSV"
            );
            assert!(
                !cmp_s.contains(",=synthetic-claim-formula")
                    && !cmp_s.contains("\n=synthetic-claim-formula"),
                "unguarded =synthetic-claim-formula must not appear at cell start in claims part CSV"
            );
        },
    );

    // Decimal cell like "-5" through guard_cell is unchanged
    assert_eq!(
        csv_copy::guard_cell(ColumnKind::Decimal, "-5"),
        "-5",
        "Decimal cell -5 must not be guarded"
    );
}

#[test]
fn the_six_data_files_are_planned_and_built() {
    fixture_view_with(|view| {
        let planned = planned_paths(view);
        assert!(
            planned.contains(xlsx::OUTPUT_PATH),
            "planned_paths must contain {}",
            xlsx::OUTPUT_PATH
        );
        assert!(
            !planned.contains("data/claims.csv"),
            "planned_paths must not contain data/claims.csv"
        );
        for &reg in &Register::ALL {
            if reg == Register::Claims {
                continue;
            }
            let p = csv_copy::output_path(reg);
            assert!(planned.contains(&p), "planned_paths must contain {p}");
        }
        let part_paths: Vec<String> = fieldbook_render::register::claims_parts(view)
            .into_iter()
            .map(|p| csv_copy::claims_part_output_path(&p.area_code, p.part))
            .collect();
        assert_eq!(
            part_paths,
            vec!["data/claims-CMP-1.csv", "data/claims-EST-1.csv"]
        );
        for p in &part_paths {
            assert!(planned.contains(p), "planned_paths must contain {p}");
        }

        let site = site::render(view, &BuildInfo::default()).unwrap();
        let data_files: Vec<&str> = site
            .files()
            .keys()
            .filter(|path| path.starts_with("data/"))
            .map(String::as_str)
            .collect();

        let mut expected_data_files = vec![
            xlsx::OUTPUT_PATH,
            "data/sources.csv",
            "data/questions.csv",
            "data/conflicts.csv",
            "data/glossary.csv",
            "data/claims-CMP-1.csv",
            "data/claims-EST-1.csv",
        ];
        expected_data_files.sort_unstable();

        let mut actual_data_files = data_files;
        actual_data_files.sort_unstable();

        assert_eq!(
            actual_data_files, expected_data_files,
            "site files under data/ must be the workbook, four register copies, and claims part CSVs"
        );
    });
}

#[test]
fn over_limit_list_cell_is_a_clean_error() {
    fixture_view_with_tree(
        |tree| {
            let bytes = tree
                .get_bytes("corpus/registers/glossary.csv")
                .expect("glossary.csv in fixture tree");
            let s = std::str::from_utf8(bytes).expect("glossary.csv is utf-8");
            assert!(
                s.contains(",withholding holdback,"),
                "expected substring in glossary.csv"
            );
            let items: Vec<String> = (0..40)
                .map(|i| format!("{i:02}{}", "x".repeat(900)))
                .collect();
            let replacement = format!(",{},", items.join("|"));
            let new_text = s.replace(",withholding holdback,", &replacement);
            tree.insert_bytes("corpus/registers/glossary.csv", new_text.into_bytes());
        },
        |view| {
            let err = xlsx::render(view, 1_772_366_400).unwrap_err();
            assert_eq!(err.register, "glossary");
            assert_eq!(err.row, "billing_withhold");
            assert_eq!(err.column, "aliases");
            let msg = err.to_string();
            assert!(msg.contains("glossary"), "expected 'glossary' in: {msg}");
            assert!(
                msg.contains("billing_withhold"),
                "expected 'billing_withhold' in: {msg}"
            );
            assert!(msg.contains("aliases"), "expected 'aliases' in: {msg}");
        },
    );
}

#[test]
fn over_limit_list_cell_stops_the_site_render() {
    fixture_view_with_tree(
        |tree| {
            let bytes = tree
                .get_bytes("corpus/registers/glossary.csv")
                .expect("glossary.csv in fixture tree");
            let s = std::str::from_utf8(bytes).expect("glossary.csv is utf-8");
            assert!(
                s.contains(",withholding holdback,"),
                "expected substring in glossary.csv"
            );
            let items: Vec<String> = (0..40)
                .map(|i| format!("{i:02}{}", "x".repeat(900)))
                .collect();
            let replacement = format!(",{},", items.join("|"));
            let new_text = s.replace(",withholding holdback,", &replacement);
            tree.insert_bytes("corpus/registers/glossary.csv", new_text.into_bytes());
        },
        |view| {
            let err = site::render(view, &BuildInfo::default()).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("billing_withhold"),
                "expected 'billing_withhold' in: {msg}"
            );
            assert!(msg.contains("aliases"), "expected 'aliases' in: {msg}");
        },
    );
}
