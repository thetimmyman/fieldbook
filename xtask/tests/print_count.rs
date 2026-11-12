//! Integration tests for `cargo xtask print-count`.

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration test starts processes and reads filesystem"
)]

#[path = "../../tests/support/rng.rs"]
mod rng;

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("xtask lives one directory below the repository root"))
        .to_path_buf()
}

#[test]
fn one_page_pdf_passes() {
    let pdf_path = repo_root().join("xtask/tests/fixtures/synthetic-one-page.pdf");
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("print-count")
        .arg(&pdf_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run xtask: {e}"));
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("pages: 1"),
        "expected `pages: 1` in stdout, got:\n{stdout}"
    );
}

#[test]
fn two_page_pdf_is_fb0607() {
    let pdf_path = repo_root().join("xtask/tests/fixtures/synthetic-two-pages.pdf");
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("print-count")
        .arg(&pdf_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run xtask: {e}"));
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("pages: 2"),
        "expected `pages: 2` in stdout, got:\n{stdout}"
    );
    assert!(
        stdout.contains("FB0607"),
        "expected `FB0607` in stdout, got:\n{stdout}"
    );
}

#[test]
fn not_a_pdf_exits_3() {
    let temp_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let file_path = temp_dir.path().join("not-a-pdf.txt");
    std::fs::write(&file_path, b"hello").unwrap_or_else(|e| panic!("write: {e}"));
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("print-count")
        .arg(&file_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run xtask: {e}"));
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    let temp_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let mut rng = rng::Rng::new(1);
    let mut paths = Vec::new();

    for file_idx in 0..200 {
        let len = usize::try_from(rng.next_u64() % 129).unwrap_or_default();
        let data = rng.bytes(len);
        let path = temp_dir.path().join(format!("seeded-{file_idx}.bin"));
        std::fs::write(&path, data).unwrap_or_else(|e| panic!("write: {e}"));
        paths.push(path);
    }

    let one_page_path = repo_root().join("xtask/tests/fixtures/synthetic-one-page.pdf");
    let original = std::fs::read(&one_page_path).unwrap_or_else(|e| panic!("read fixture: {e}"));

    for idx in (0..original.len()).step_by(8) {
        let mut flipped = original.clone();
        if let Some(b) = flipped.get_mut(idx) {
            *b ^= 0xFF;
        }
        let flipped_path = temp_dir.path().join(format!("mutated-flipped-{idx}.pdf"));
        std::fs::write(&flipped_path, flipped).unwrap_or_else(|e| panic!("write: {e}"));
        paths.push(flipped_path);

        let mut deleted = original.clone();
        if idx < deleted.len() {
            deleted.remove(idx);
        }
        let deleted_path = temp_dir.path().join(format!("mutated-deleted-{idx}.pdf"));
        std::fs::write(&deleted_path, deleted).unwrap_or_else(|e| panic!("write: {e}"));
        paths.push(deleted_path);
    }

    for path in &paths {
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("print-count")
            .arg(path)
            .output()
            .unwrap_or_else(|e| panic!("failed to run xtask on {}: {e}", path.display()));
        let code = output.status.code();
        assert!(
            matches!(code, Some(0) | Some(1) | Some(3)),
            "path {} exited with unexpected status {:?}",
            path.display(),
            code
        );
    }
}
