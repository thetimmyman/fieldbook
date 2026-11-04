//! Guard that `tests/support/rng.rs` is the only SplitMix64 generator in the workspace.

#![expect(
    clippy::disallowed_methods,
    reason = "repository guard test walks source files on disk"
)]

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("xtask lives one directory below the repository root"))
        .to_path_buf()
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read directory {}: {err}", dir.display()));
    let mut paths: Vec<PathBuf> = entries
        .map(|entry| {
            entry
                .unwrap_or_else(|err| panic!("failed to read entry in {}: {err}", dir.display()))
                .path()
        })
        .collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn one_rng_exists() {
    let root = repo_root();
    let needle = format!("{}{}", "9e3779b9", "7f4a7c15");

    let mut files = Vec::new();
    for folder in ["crates", "xtask"] {
        collect_rs_files(&root.join(folder), &mut files);
    }

    let mut offenders = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        let normalized: String = text
            .chars()
            .filter(|&c| c != '_')
            .flat_map(char::to_lowercase)
            .collect();
        if normalized.contains(&needle) {
            let rel = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            offenders.push(rel);
        }
    }

    assert!(
        offenders.is_empty(),
        "SplitMix64 constant found under crates/ or xtask/ (use tests/support/rng.rs instead):\n  {}",
        offenders.join("\n  ")
    );
}
