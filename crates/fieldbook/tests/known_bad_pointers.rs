//! Verify that `in_process` pointers in `tests/known-bad/*.toml` resolve to existing tests.

#![expect(
    clippy::disallowed_methods,
    reason = "pointer check scans tests/known-bad and crates/*/tests on disk"
)]

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MaybeInProcess {
    in_process: Option<String>,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn validate_in_process_dir(root: &Path, known_bad_dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(known_bad_dir)
        .map_err(|e| format!("failed to read {}: {e}", known_bad_dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
        .collect();
    paths.sort();

    for path in paths {
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if matches!(file_name, "cases.toml" | "pending.toml" | "elsewhere.toml") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let parsed: MaybeInProcess = toml::from_str(&text)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        let Some(pointer) = parsed.in_process else {
            continue;
        };
        let parts: Vec<&str> = pointer.split('/').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(format!(
                "{}: invalid in_process pointer `{pointer}` (expected `<crate>/<test file>/<test name>`)",
                path.display()
            ));
        }
        let (krate, test_file, test_name) = (parts[0], parts[1], parts[2]);
        let target = root
            .join("crates")
            .join(krate)
            .join("tests")
            .join(format!("{test_file}.rs"));
        let Ok(source) = std::fs::read_to_string(&target) else {
            return Err(format!(
                "{}: in_process `{pointer}` names missing test file {}",
                path.display(),
                target.display()
            ));
        };
        let needle = format!("fn {test_name}");
        if !source.contains(&needle) {
            return Err(format!(
                "{}: in_process `{pointer}` test `{test_name}` not found in {}",
                path.display(),
                target.display()
            ));
        }
    }
    Ok(())
}

mod known_bad_pointers {
    use super::{repo_root, validate_in_process_dir};

    #[test]
    fn in_process_names_an_existing_test() {
        let root = repo_root();
        let known_bad_dir = root.join("tests/known-bad");
        if let Err(msg) = validate_in_process_dir(&root, &known_bad_dir) {
            panic!("{msg}");
        }

        let tmp = tempfile::tempdir().expect("tempdir");
        let valid_file = tmp.path().join("FB0998.toml");
        std::fs::write(
            &valid_file,
            "code = \"FB0998\"\nin_process = \"fieldbook/cli/version_prints_name_version_and_identity\"\n",
        )
        .expect("write valid in_process");
        assert!(validate_in_process_dir(&root, tmp.path()).is_ok());

        let bad_file = tmp.path().join("FB0999.toml");
        std::fs::write(
            &bad_file,
            "code = \"FB0999\"\nin_process = \"fieldbook/none/missing\"\n",
        )
        .expect("write bad in_process");
        let err = validate_in_process_dir(&root, tmp.path()).unwrap_err();
        assert!(err.contains("FB0999.toml"), "{err}");
    }
}
