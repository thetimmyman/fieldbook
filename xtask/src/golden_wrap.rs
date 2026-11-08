//! Golden vectors as classic scripts for the browser harness (`search-js`,
//! `tests/golden/README.md#how-the-vectors-are-loaded`).
//!
//! A page opened with `file://` cannot fetch JSON, so every golden file under `tests/golden/`
//! except the report goldens in `tests/golden/json/` becomes `out/harness/golden/<path>.js`, which
//! assigns the file's JSON, unchanged, to `window.FB_GOLDEN["<path>"]`. A file with `units` (a
//! ranked file or the page-view file) also gets `<path>.index.js`: the index that
//! `fieldbook_core::search` builds from its `units` and `glossary`, in the form of
//! `search-index.js`, assigned to `window.FB_GOLDEN_INDEX["<path>"]`. The wrappers are generated
//! and never committed.

use std::fs;
use std::path::{Path, PathBuf};

use fieldbook_core::search::{Glossary, GoldenUnit, Index};
use fieldbook_render::search_index;
use serde::Deserialize;

/// Where the wrappers go, relative to the repository root.
pub const OUT_DIR: &str = "out/harness/golden";

/// Where the golden files are, relative to the repository root.
pub const GOLDEN_DIR: &str = "tests/golden";

/// The report goldens, which a Rust test compares and no page reads.
const LEFT_OUT: &str = "json/";

/// The global that holds the golden files.
const GOLDEN_GLOBAL: &str = "window.FB_GOLDEN";

/// The global that holds the indexes built from ranked files.
const INDEX_GLOBAL: &str = "window.FB_GOLDEN_INDEX";

/// One generated script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrapper {
    /// The golden file's path below `tests/golden/`, `/`-separated: the key in the global.
    pub key: String,
    /// The script's path below [`OUT_DIR`].
    pub path: String,
    /// The script.
    pub text: String,
}

/// The parts of a golden file an index is built from.
#[derive(Deserialize)]
struct Units {
    units: Vec<GoldenUnit>,
    #[serde(default)]
    glossary: Glossary,
}

/// The script that assigns `raw`, unchanged, to `window.FB_GOLDEN[key]`.
#[must_use]
pub fn wrap(key: &str, raw: &str) -> String {
    assign(GOLDEN_GLOBAL, key, raw)
}

/// For a golden file with `units`, the script that assigns the index Rust builds from them to
/// `window.FB_GOLDEN_INDEX[key]`; `None` for a file without `units`.
///
/// # Errors
///
/// Returns a message naming `key` when the file is not JSON or its `units` do not parse.
pub fn wrap_index(key: &str, raw: &str) -> Result<Option<String>, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("{key}: not JSON: {e}"))?;
    if value.get("units").is_none() {
        return Ok(None);
    }
    let file: Units =
        serde_json::from_value(value).map_err(|e| format!("{key}: units do not parse: {e}"))?;
    let script = search_index::write(&Index::from_golden(&file.units, &file.glossary));
    let payload = script
        .strip_prefix("window.FB_INDEX = ")
        .and_then(|rest| rest.strip_suffix(';'))
        .ok_or_else(|| format!("{key}: search_index::write changed its form"))?;
    Ok(Some(assign(INDEX_GLOBAL, key, payload)))
}

fn assign(global: &str, key: &str, value: &str) -> String {
    let key_literal = serde_json::to_string(key).unwrap_or_else(|_| format!("{key:?}"));
    format!("{global} = {global} || {{}};\n{global}[{key_literal}] = {value};\n")
}

/// Every wrapper for the golden files under `golden_root`, in path order.
///
/// # Errors
///
/// Returns a message naming the file that cannot be read or built.
pub fn wrappers(golden_root: &Path) -> Result<Vec<Wrapper>, String> {
    let mut files = Vec::new();
    collect(golden_root, golden_root, &mut files)?;
    files.sort();
    let mut out = Vec::new();
    for (key, path) in files {
        if key.starts_with(LEFT_OUT) {
            continue;
        }
        let raw = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(text) = wrap_index(&key, &raw)? {
            out.push(Wrapper {
                key: key.clone(),
                path: format!("{key}.index.js"),
                text,
            });
        }
        out.push(Wrapper {
            path: format!("{key}.js"),
            text: wrap(&key, &raw),
            key,
        });
    }
    Ok(out)
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let key = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push((key, path));
        }
    }
    Ok(())
}

/// Writes every wrapper under `<root>/out/harness/golden/`, replacing what is there, and returns
/// them.
///
/// # Errors
///
/// Returns a message naming the file that cannot be read, built or written.
pub fn write(root: &Path) -> Result<Vec<Wrapper>, String> {
    let all = wrappers(&root.join(GOLDEN_DIR))?;
    let out = root.join(OUT_DIR);
    if out.exists() {
        fs::remove_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    for wrapper in &all {
        let path = out.join(&wrapper.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&path, &wrapper.text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(all)
}

/// `cargo xtask golden-wrap`: writes the wrappers and prints how many.
///
/// # Errors
///
/// Returns a message naming the file that cannot be read, built or written.
pub fn command(root: &Path) -> Result<bool, String> {
    let all = write(root)?;
    println!("golden-wrap: {} scripts in {OUT_DIR}", all.len());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value a wrapper assigns, read back from its text.
    fn assigned<'a>(global: &str, key: &str, text: &'a str) -> Option<&'a str> {
        let key_literal = serde_json::to_string(key).ok()?;
        let head = format!("{global} = {global} || {{}};\n{global}[{key_literal}] = ");
        text.strip_prefix(&head)?.strip_suffix(";\n")
    }

    fn golden_root() -> PathBuf {
        crate::proc::repo_root().join(GOLDEN_DIR)
    }

    #[test]
    fn golden_wrap() {
        let root = golden_root();
        let all = wrappers(&root).unwrap_or_else(|e| panic!("{e}"));
        let mut failures = Vec::new();
        let mut ranked = 0;
        let mut plain = 0;
        for wrapper in &all {
            let raw = fs::read_to_string(root.join(&wrapper.key))
                .unwrap_or_else(|e| panic!("{}: {e}", wrapper.key));
            if wrapper.key.starts_with(LEFT_OUT) {
                failures.push(format!("{}: report goldens are not wrapped", wrapper.key));
            }
            if wrapper.path.ends_with(".index.js") {
                ranked += 1;
                let Some(payload) = assigned(INDEX_GLOBAL, &wrapper.key, &wrapper.text) else {
                    failures.push(format!("{}: index wrapper has another form", wrapper.path));
                    continue;
                };
                let file: Units =
                    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{}: {e}", wrapper.key));
                let expected = Index::from_golden(&file.units, &file.glossary);
                let got: Index = serde_json::from_str(payload)
                    .unwrap_or_else(|e| panic!("{}: {e}", wrapper.path));
                let version = serde_json::from_str::<serde_json::Value>(payload)
                    .ok()
                    .and_then(|v| v.get("v").and_then(serde_json::Value::as_u64));
                if got != expected || version != Some(1) {
                    failures.push(format!(
                        "{}: the index differs from fieldbook_core::search's",
                        wrapper.path
                    ));
                }
            } else {
                plain += 1;
                if assigned(GOLDEN_GLOBAL, &wrapper.key, &wrapper.text) != Some(raw.as_str()) {
                    failures.push(format!(
                        "{}: the wrapper does not hold {} unchanged",
                        wrapper.path, wrapper.key
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "golden_wrap failures:\n{}",
            failures.join("\n")
        );

        // Every golden file outside `json/` has a wrapper, and every file with `units` an index.
        let mut files = Vec::new();
        collect(&root, &root, &mut files).unwrap_or_else(|e| panic!("{e}"));
        let wrapped: Vec<&str> = files
            .iter()
            .map(|(key, _)| key.as_str())
            .filter(|key| !key.starts_with(LEFT_OUT))
            .collect();
        assert_eq!(plain, wrapped.len(), "one wrapper per golden file");
        assert!(
            ranked >= 10,
            "every ranked file and page-view.json has an index, got {ranked}"
        );
        assert!(
            all.iter()
                .any(|w| w.path == "search/seed-ranked.json.index.js"),
            "the seed ranked file has an index"
        );
    }

    #[test]
    fn a_file_without_units_has_no_index_and_bad_json_is_named() {
        assert_eq!(
            wrap_index("search/x.json", r#"{"kind":"tokenize","cases":[]}"#),
            Ok(None)
        );
        let err = wrap_index("search/broken.json", "{").unwrap_err();
        assert!(err.contains("search/broken.json"), "{err}");
    }
}
