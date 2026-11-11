//! Pins the deterministic workbook bytes by verifying identical output across runs
//! with a fixed creation datetime property.

use std::io::{Cursor, Read};
use std::thread;
use std::time::Duration;

use rust_xlsxwriter::{DocProperties, ExcelDateTime, Workbook, XlsxError};
use zip::ZipArchive;

fn build_workbook() -> Result<Vec<u8>, XlsxError> {
    let date = ExcelDateTime::from_ymd(2026, 3, 15)?.and_hms(0, 0, 0)?;
    let properties = DocProperties::new().set_creation_datetime(&date);

    let mut workbook = Workbook::new();
    workbook.set_properties(&properties);

    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Register")?;

    worksheet.write_string(0, 0, "id")?;
    worksheet.write_string(0, 1, "title")?;
    worksheet.write_string(0, 2, "amount")?;

    worksheet.write_string(1, 0, "rec-1")?;
    worksheet.write_string(1, 1, "Synthetic record 1")?;
    worksheet.write_number(1, 2, 42.5)?;

    worksheet.write_string(2, 0, "rec-2")?;
    worksheet.write_string(2, 1, "Synthetic record 2")?;
    worksheet.write_number(2, 2, 84.0)?;

    workbook.save_to_buffer()
}

#[test]
fn a_workbook_with_a_fixed_creation_time_is_byte_identical() {
    let first = build_workbook().expect("first workbook build succeeds");
    thread::sleep(Duration::from_millis(1100));
    let second = build_workbook().expect("second workbook build succeeds");

    assert_eq!(
        first, second,
        "workbooks built at different times must be byte identical"
    );

    let cursor = Cursor::new(&first);
    let mut archive = ZipArchive::new(cursor).expect("zip archive opens");
    let mut core_file = archive
        .by_name("docProps/core.xml")
        .expect("docProps/core.xml entry exists");

    let mut core_xml = String::new();
    core_file
        .read_to_string(&mut core_xml)
        .expect("docProps/core.xml reads to string");

    assert!(
        core_xml.contains("2026-03-15T00:00:00Z"),
        "docProps/core.xml must contain fixed creation time 2026-03-15T00:00:00Z, got: {core_xml}"
    );
}
