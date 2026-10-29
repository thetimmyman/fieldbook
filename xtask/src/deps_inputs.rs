//! Located configuration facts, aggregated with the existing reference producer.

use crate::deps_references::{self, ReadError, References, Source};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct Fact {
    pub section: String,
    pub key: String,
    pub value: Value,
    pub source: Source,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Replay {
    pub fields: Vec<Fact>,
    pub adjustments: Vec<Vec<Fact>>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Inputs {
    pub references: References,
    pub workspace: Vec<Fact>,
    pub tools: Vec<Fact>,
    pub replay: Option<Replay>,
}

fn error(path: &str, line: usize, message: &'static str) -> ReadError {
    ReadError {
        source: Source {
            path: path.into(),
            line,
        },
        message,
    }
}
fn one_of(value: &str, words: &str) -> bool {
    words.split_whitespace().any(|s| s == value)
}
fn bare(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
// Split only the bounded one-line forms, retaining quoted delimiters and comments.
fn parts(text: &str, separator: char) -> Option<Vec<&str>> {
    if text.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
        return None;
    }
    let (mut quote, mut escaped, mut depth, mut start) = (None, false, 0_i32, 0);
    let mut out = Vec::new();
    for (i, c) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if q == '"' && c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
        } else if c.is_whitespace() && c != ' ' && c != '\t' {
            return None;
        } else if c == '\'' || c == '"' {
            if text[i..].starts_with("\"\"\"") || text[i..].starts_with("'''") {
                return None;
            }
            quote = Some(c);
        } else if c == separator && depth == 0 {
            out.push(text[start..i].trim());
            if separator == '#' {
                return Some(out);
            }
            start = i + 1;
        } else if c == '[' || c == '{' {
            depth += 1;
        } else if c == ']' || c == '}' {
            depth -= 1;
            if depth < 0 {
                return None;
            }
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    out.push(text[start..].trim());
    Some(out)
}
fn string(text: &str) -> Option<String> {
    if text.starts_with('"') {
        let (bytes, mut i) = (text.as_bytes(), 0);
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                match bytes.get(i + 1)? {
                    b'u' => {
                        let _ =
                            char::from_u32(u32::from_str_radix(text.get(i + 2..i + 6)?, 16).ok()?)?;
                        i += 6;
                    }
                    b'"' | b'\\' | b'b' | b't' | b'n' | b'f' | b'r' => i += 2,
                    _ => return None,
                }
            } else {
                i += 1;
            }
        }
        serde_json::from_str(&text.replace('\t', "\\t")).ok()
    } else {
        text.strip_prefix('\'')?
            .strip_suffix('\'')
            .filter(|s| !s.contains('\''))
            .map(str::to_owned)
    }
}
fn atom(text: &str) -> Option<Value> {
    if let Some(s) = string(text) {
        return Some(Value::String(s));
    }
    match text {
        "true" => Some(Value::Bool(true)),
        "false" => Some(Value::Bool(false)),
        "1" => Some(Value::from(1)),
        _ => {
            let inner = text.strip_prefix('[')?.strip_suffix(']')?;
            let mut items = parts(inner, ',')?;
            if items.last() == Some(&"") {
                items.pop();
            }
            items
                .into_iter()
                .map(|s| string(s).map(Value::String))
                .collect::<Option<Vec<_>>>()
                .map(Value::Array)
        }
    }
}
fn declaration(text: &str) -> Option<Value> {
    if let Some(s) = string(text) {
        return Some(Value::String(s));
    }
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    let mut items = parts(inner, ',')?;
    if items.last() == Some(&"") {
        items.pop();
    }
    let mut fields = serde_json::Map::new();
    for item in items {
        let (key, value) = item.split_once('=')?;
        let (key, value) = (key.trim(), atom(value.trim())?);
        let valid = match key {
            "version" | "package" | "path" | "registry" | "git" | "branch" | "tag" | "rev" => {
                value.is_string()
            }
            "features" => value.is_array(),
            "default-features" | "optional" | "workspace" => value.is_boolean(),
            _ => false,
        };
        if !valid || fields.insert(key.into(), value).is_some() {
            return None;
        }
    }
    Some(Value::Object(fields))
}
struct Block {
    name: String,
    line: usize,
    fields: Vec<Fact>,
}
fn document(root: &Path, path: &str) -> Result<Vec<Block>, ReadError> {
    let text =
        std::fs::read_to_string(root.join(path)).map_err(|_| error(path, 1, "unreadable input"))?;
    let (mut blocks, mut seen) = (Vec::<Block>::new(), BTreeSet::new());
    for (i, raw) in text.split_inclusive('\n').enumerate() {
        let raw = raw
            .strip_suffix('\n')
            .map_or(raw, |line| line.strip_suffix('\r').unwrap_or(line));
        let tokens = parts(raw, '#').ok_or_else(|| error(path, i + 1, "unsupported syntax"))?;
        let line = tokens[0].trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            let adjustment = path == ".config/tools.toml" && line == "[[lock.adjustment]]";
            let name = if adjustment {
                "lock.adjustment"
            } else {
                line.strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .filter(|s| !s.contains(['[', ']']))
                    .ok_or_else(|| error(path, i + 1, "unsupported section"))?
                    .trim()
            };
            let known = match path {
                "Cargo.toml" => {
                    one_of(
                        name,
                        "workspace workspace.dependencies workspace.package workspace.lints package lints features dependencies dev-dependencies build-dependencies",
                    ) || [
                        "workspace.lints.",
                        "package.",
                        "lints.",
                        "profile.",
                        "target.",
                    ]
                    .iter()
                    .any(|s| name.starts_with(s))
                }
                "rust-toolchain.toml" => name == "toolchain",
                _ => one_of(name, "cargo gitleaks poppler fuzz lock lock.adjustment"),
            };
            if !known
                || (!adjustment && (name == "lock.adjustment" || !seen.insert(name.to_owned())))
            {
                return Err(error(path, i + 1, "unknown or duplicate section"));
            }
            blocks.push(Block {
                name: name.into(),
                line: i + 1,
                fields: Vec::new(),
            });
            continue;
        }
        if path == "Cargo.toml"
            && blocks.is_empty()
            && line
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == "cargo-features")
        {
            continue;
        }
        let b = blocks
            .last_mut()
            .ok_or_else(|| error(path, i + 1, "field outside section"))?;
        if path == "Cargo.toml" && !one_of(&b.name, "workspace workspace.dependencies") {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| error(path, i + 1, "expected key = value"))?;
        let key = key.trim();
        if !bare(key) || b.fields.iter().any(|f| f.key == key) {
            return Err(error(path, i + 1, "invalid or duplicate key"));
        }
        let value = if b.name == "workspace.dependencies" {
            declaration(value.trim())
        } else {
            atom(value.trim())
        }
        .ok_or_else(|| error(path, i + 1, "unsupported value"))?;
        b.fields.push(Fact {
            section: b.name.clone(),
            key: key.into(),
            value,
            source: Source {
                path: path.into(),
                line: i + 1,
            },
        });
    }
    Ok(blocks)
}
fn required(b: &Block, path: &str, keys: &str) -> Result<(), ReadError> {
    if keys
        .split_whitespace()
        .any(|k| !b.fields.iter().any(|f| f.key == k))
    {
        return Err(error(path, b.line, "missing required field"));
    }
    Ok(())
}
pub(crate) fn read(root: &Path) -> Result<Inputs, ReadError> {
    let mut inputs = Inputs {
        references: deps_references::read_references(root)?,
        workspace: Vec::new(),
        tools: Vec::new(),
        replay: None,
    };
    let mut replay = Replay {
        fields: Vec::new(),
        adjustments: Vec::new(),
    };
    for path in ["Cargo.toml", ".config/tools.toml", "rust-toolchain.toml"] {
        let blocks = document(root, path)?;
        let needed = match path {
            "Cargo.toml" => "",
            "rust-toolchain.toml" => "toolchain",
            _ => "cargo gitleaks poppler lock",
        };
        if needed
            .split_whitespace()
            .any(|s| !blocks.iter().any(|b| b.name == s))
        {
            return Err(error(path, 1, "missing required section"));
        }
        let recorded = blocks.iter().any(|b| {
            b.name == "lock.adjustment"
                || (b.name == "lock" && b.fields.iter().any(|f| f.key != "nightly"))
        });
        for b in blocks {
            let keys = match b.name.as_str() {
                "cargo" | "workspace" | "workspace.dependencies" => "",
                "gitleaks" => "version linux-x64-sha256",
                "poppler" => "version",
                "fuzz" => "toolchain",
                "toolchain" => "channel profile",
                "lock" if recorded => {
                    "replay_schema mode resolver cutoff inputs_sha256 snapshot snapshot_sha256"
                }
                "lock" => "nightly",
                "lock.adjustment"
                    if b.fields
                        .iter()
                        .any(|f| f.key == "reason" && f.value == "security") =>
                {
                    "package version reason advisory approval_commit approval approval_sha256"
                }
                "lock.adjustment" => "package version reason",
                _ => continue,
            };
            for f in &b.fields {
                let valid = match (b.name.as_str(), f.key.as_str()) {
                    ("workspace", "resolver") | ("lock", "nightly") => f.value.is_string(),
                    ("workspace", "members" | "exclude" | "default-members")
                    | ("toolchain", "components" | "targets") => f.value.is_array(),
                    ("workspace", _) => false,
                    ("cargo", _) => f.value.as_str().is_some_and(|s| !s.is_empty()),
                    ("workspace.dependencies", _) => f.value.is_string() || f.value.is_object(),
                    ("lock", "replay_schema") => f.value == 1,
                    ("lock", "mode") => f
                        .value
                        .as_str()
                        .is_some_and(|s| one_of(s, "publish-time stable-fallback")),
                    ("lock.adjustment", "reason") => f
                        .value
                        .as_str()
                        .is_some_and(|s| one_of(s, "cooldown security")),
                    _ => one_of(&f.key, keys) && f.value.is_string(),
                };
                if !valid {
                    return Err(error(path, f.source.line, "unknown field or invalid type"));
                }
            }
            required(&b, path, keys)?;
            if b.name == "lock.adjustment" {
                replay.adjustments.push(b.fields);
            } else if b.name == "lock" {
                for f in b.fields {
                    if f.key == "nightly" {
                        inputs.tools.push(f);
                    } else {
                        replay.fields.push(f);
                    }
                }
            } else if path == "Cargo.toml" {
                inputs.workspace.extend(b.fields);
            } else {
                inputs.tools.extend(b.fields);
            }
        }
    }
    if !replay.fields.is_empty() {
        inputs.replay = Some(replay);
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn fixture(case: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/deps-inputs")
            .join(case)
    }
    struct FixtureRoot(PathBuf);
    impl Drop for FixtureRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let mut name = entry.file_name();
            if name == "Cargo.toml.input" {
                name = "Cargo.toml".into();
            }
            let dest = to.join(name);
            let kind = entry.file_type()?;
            if kind.is_dir() {
                fs::create_dir(&dest)?;
                copy_tree(&entry.path(), &dest)?;
            } else if kind.is_file() {
                fs::copy(entry.path(), dest)?;
            } else {
                return Err(std::io::Error::other("unsupported fixture entry"));
            }
        }
        Ok(())
    }
    fn materialize(case: &str) -> std::io::Result<FixtureRoot> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "deps-inputs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        let root = FixtureRoot(path);
        copy_tree(&fixture(case), &root.0)?;
        Ok(root)
    }
    fn facts(items: &[Fact]) -> Value {
        items
            .iter()
            .map(|f| json!([f.source.path, f.source.line, f.section, f.key, f.value]))
            .collect()
    }
    fn observed(i: &Inputs) -> Value {
        json!({"workspace":facts(&i.workspace), "tools":facts(&i.tools),
            "replay":i.replay.as_ref().map(|r| json!({"fields":facts(&r.fields),"adjustments":r.adjustments.iter().map(|a| facts(a)).collect::<Vec<_>>()})),
            "references":{"rows":i.references.rows.iter().map(|r| json!([r.source.path,r.source.line,r.table,r.identity,r.cells])).collect::<Vec<_>>(),
                "workflows":i.references.workflows.iter().map(|r| json!([r.source.path,r.source.line,r.reference])).collect::<Vec<_>>()}})
    }
    #[test]
    fn deps_inputs_complete_facts_from_explicit_root() {
        for case in [
            "baseline",
            "crlf",
            "empty",
            "empty-workspace",
            "package-only",
            "dependencies-only",
            "workspace-shapes",
            "publish-replay",
            "fallback-replay",
            "escaping",
        ] {
            let root = materialize(case).unwrap();
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(root.0.join("expected.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(observed(&read(&root.0).unwrap()), expected, "{case}");
        }
    }
    #[test]
    fn deps_inputs_refuses_invalid_configuration_and_propagates_reference_errors() {
        let cases: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(fixture("errors.json")).unwrap())
                .unwrap();
        for case in cases {
            let name = case[0].as_str().unwrap();
            let root = materialize(name).unwrap();
            let result = read(&root.0);
            assert!(result.is_err(), "{name}: {result:?}");
            let got = result.unwrap_err();
            assert_eq!(
                json!([got.source.path, got.source.line, got.message]),
                Value::Array(case.as_array().unwrap()[1..].to_vec()),
                "{name}"
            );
        }
    }
    #[test]
    fn deps_inputs_reads_current_configuration() {
        read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap();
    }
    #[test]
    fn deps_inputs_bytes_and_single_byte_mutations_do_not_panic() -> std::io::Result<()> {
        let sandbox = materialize("baseline")?;
        let root = &sandbox.0;
        let paths = ["Cargo.toml", ".config/tools.toml", "rust-toolchain.toml"];
        let bytes = std::fs::read(fixture("bytes.bin"))?;
        assert_eq!(bytes.len(), 6400);
        for path in paths {
            let original = std::fs::read(root.join(path))?;
            for sample in bytes.as_chunks::<32>().0 {
                std::fs::write(root.join(path), sample)?;
                let _: Result<Inputs, ReadError> = read(root);
            }
            for i in 0..original.len() {
                let mut changed = original.clone();
                changed[i] ^= 1;
                std::fs::write(root.join(path), changed)?;
                let _: Result<Inputs, ReadError> = read(root);
            }
            std::fs::write(root.join(path), original)?;
        }
        Ok(())
    }
}
