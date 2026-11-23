//! Fuzz-style no-panic test for `Settings::parse` (`ai-config-and-keys` step 9).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    reason = "parser fuzz test reads the fixture settings file"
)]

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::path::PathBuf;

use fieldbook::adapters::registry::{ADAPTERS, Build, Needs, Port, Row, Sees, State};
use fieldbook::settings::Settings;
use fieldbook_core::check::Code;

fn test_rows() -> Vec<Row> {
    let mut rows = ADAPTERS.to_vec();
    rows.push(Row {
        port: Port::Model,
        id: "openai-compatible@1",
        default: false,
        state: State::Built,
        build: Build::Ai,
        krate: env!("CARGO_PKG_NAME"),
        needs: Needs::Net,
        sees: Sees::Labelled,
        calls_model: true,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/model/openai-compatible@1/",
        suite: "model_suite",
        doc: "model-openai-compatible1",
    });
    rows
}

fn assert_result_valid(input: &str, rows: &[Row]) {
    if let Err(errors) = Settings::parse(input, rows) {
        assert!(!errors.is_empty());
        for err in errors {
            assert_eq!(err.code, Code::Fb0900);
            assert!(
                err.line >= 1,
                "every SettingsError must have a 1-based line number: {err:?}"
            );
        }
    }
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let rows = test_rows();
    let mut prng = rng::Rng::new(1);

    // 200 random byte strings of length 0..=128.
    for idx in 0..200 {
        let len = match idx {
            0 => 0,
            1 => 128,
            _ => usize::try_from(prng.next_u64() % 129).expect("len fits usize"),
        };
        let bytes = prng.bytes(len);
        let text = String::from_utf8_lossy(&bytes);
        assert_result_valid(&text, &rows);
    }

    // Explicit edge cases including unterminated table headers.
    for edge in [
        "[",
        "[[",
        "[model",
        "[model.endpoint.work",
        "[[backup.target",
        "[\"\n",
        "key = \"unterminated",
    ] {
        assert_result_valid(edge, &rows);
    }

    // Mutate every byte of `tests/fixtures/state-ai/settings.toml` (once flipped, once deleted).
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/state-ai/settings.toml");
    let orig = std::fs::read(&path).expect("read state-ai/settings.toml");
    assert!(!orig.is_empty());

    for pos in 0..orig.len() {
        let mut flipped = orig.clone();
        flipped[pos] ^= 0x01;
        let text_flipped = String::from_utf8_lossy(&flipped);
        assert_result_valid(&text_flipped, &rows);

        let mut deleted = orig.clone();
        deleted.remove(pos);
        let text_deleted = String::from_utf8_lossy(&deleted);
        assert_result_valid(&text_deleted, &rows);
    }
}
