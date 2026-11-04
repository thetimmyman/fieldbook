//! Deterministic fuzz/mutation tests ensuring the CSV reader and normalizer never panic.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::schema::register::Register;
use fieldbook_core::schema::table;
use rng::Rng;

const EXCEL_SAVED_SAMPLE: &[u8] =
    "\u{feff}question_id,area,question\r\n\"Q-010\",\"cmp\",\"Plain cell\"\r\nQ-002,cmp,\"Needs, comma and \"\"quote\"\" in Cafe\u{0301}\"\r\n"
        .as_bytes();

fn check_csv_bytes(bytes: &[u8]) {
    match table::read(bytes) {
        Ok(parsed) => {
            assert!(parsed.header.line >= 1);
            for &c in &parsed.header.cols {
                assert!(c >= 1);
            }
            for row in &parsed.rows {
                assert!(row.line >= 1);
                for &c in &row.cols {
                    assert!(c >= 1);
                }
            }

            for reg in Register::ALL {
                let _ = table::is_normal(reg, bytes);
                let Ok(norm) = table::normalize(reg, bytes) else {
                    panic!("an Ok table from read must normalize without error");
                };
                assert!(
                    table::is_normal(reg, norm.as_bytes()),
                    "normalized text must satisfy is_normal"
                );
                let Ok(reparsed) = table::read(norm.as_bytes()) else {
                    panic!("normalized text must read back without error");
                };
                let rewritten = table::write(&reparsed);
                assert_eq!(
                    rewritten, norm,
                    "normalized text must read back and write to itself"
                );
                let Ok(reparsed_again) = table::read(rewritten.as_bytes()) else {
                    panic!("rewritten text must read back without error");
                };
                assert_eq!(reparsed_again, reparsed);
            }
        }
        Err(err) => {
            assert!(err.line() >= 1);
            assert!(err.col() >= 1);
            for reg in Register::ALL {
                assert!(!table::is_normal(reg, bytes));
            }
        }
    }
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let mut rng = Rng::new(1);
    for _ in 0..200 {
        let bytes = rng.bytes(128);
        check_csv_bytes(&bytes);
    }

    check_csv_bytes(EXCEL_SAVED_SAMPLE);

    for i in 0..EXCEL_SAVED_SAMPLE.len() {
        let mut flipped = EXCEL_SAVED_SAMPLE.to_vec();
        flipped[i] ^= 0x01;
        check_csv_bytes(&flipped);

        let mut flipped_high = EXCEL_SAVED_SAMPLE.to_vec();
        flipped_high[i] ^= 0x80;
        check_csv_bytes(&flipped_high);

        let mut deleted = EXCEL_SAVED_SAMPLE.to_vec();
        deleted.remove(i);
        check_csv_bytes(&deleted);
    }
}
