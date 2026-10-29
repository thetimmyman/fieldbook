//! Bounded catalog and workflow reference facts. Policy belongs to the consumer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub struct Source {
    pub path: PathBuf,
    pub line: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ReadError {
    pub source: Source,
    pub message: &'static str,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub source: Source,
    pub table: String,
    pub identity: String,
    pub cells: Vec<String>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ActionReference {
    pub source: Source,
    pub reference: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct References {
    pub rows: Vec<Row>,
    pub workflows: Vec<ActionReference>,
}

fn source(path: &str, line: usize) -> Source {
    Source {
        path: path.into(),
        line,
    }
}
fn error(path: &str, line: usize, message: &'static str) -> ReadError {
    ReadError {
        source: source(path, line),
        message,
    }
}
fn text(root: &Path, path: &str) -> Result<String, ReadError> {
    std::fs::read_to_string(root.join(path)).map_err(|_| error(path, 1, "unreadable input"))
}
fn cells(line: &str) -> Option<Vec<String>> {
    if line.contains("\\|") || line.contains("<!--") {
        return None;
    }
    Some(
        line.trim()
            .strip_prefix('|')?
            .strip_suffix('|')?
            .split('|')
            .map(|s| s.trim().to_owned())
            .collect(),
    )
}
fn identity(first: &str, tool: bool) -> Option<&str> {
    if let Some(tail) = first.strip_prefix('`') {
        tail.split_once('`')
            .filter(|(s, _)| !s.is_empty())
            .map(|(s, _)| s)
    } else if tool {
        first.split(" (").next().filter(|s| !s.is_empty())
    } else {
        None
    }
}
fn catalog(root: &Path) -> Result<Vec<Row>, ReadError> {
    const PATH: &str = "docs/reference/dependencies.md";
    let content = text(root, PATH)?;
    let lines: Vec<_> = content.lines().collect();
    let headers = [
        (
            "Pins",
            "Crate|Version (released)|Features|Used by|Licence|MSRV",
        ),
        (
            "Test-only and dev-only crates",
            "Crate|Version (released)|Features|Where|Licence|MSRV|Why|Maintainer health",
        ),
        ("Developer tools", "Tool|Version (released)|Joins|Note"),
        ("CI actions", "Action|Version (released)|Commit SHA|Note"),
    ];
    let (mut top, mut sub, mut i) = ("", "", 0);
    let mut fence = None;
    let (mut seen, mut names, mut rows) = (BTreeSet::new(), BTreeSet::new(), Vec::new());
    while i < lines.len() {
        let line = lines[i].trim();
        if lines[i].starts_with("    ") || lines[i].starts_with('\t') {
            i += 1;
            continue;
        }
        let marker = line.chars().next().filter(|c| matches!(c, '`' | '~'));
        let count = marker.map_or(0, |c| line.chars().take_while(|x| *x == c).count());
        if let Some((c, length)) = fence {
            if marker == Some(c) && count >= length && line[count..].trim().is_empty() {
                fence = None;
            }
            i += 1;
            continue;
        }
        if count >= 3 {
            fence = marker.map(|c| (c, count));
            i += 1;
            continue;
        }
        if line.starts_with('<') || line.contains("<!--") {
            return Err(error(PATH, i + 1, "unsupported HTML context"));
        }
        if let Some(s) = line.strip_prefix("## ") {
            top = s;
            sub = "";
        }
        if let Some(s) = line.strip_prefix("### ") {
            sub = s;
        }
        let target = headers.iter().enumerate().find(|(n, (name, _))| {
            if *n == 0 {
                top == "Shipped runtime crates" && sub == "Pins"
            } else {
                top == *name && sub.is_empty()
            }
        });
        if let Some((n, (name, header))) = target.filter(|_| line.starts_with('|')) {
            let fail = |line, message| error(PATH, line, message);
            if !seen.insert(n) {
                return Err(fail(i + 1, "duplicate table"));
            }
            let columns = cells(line).ok_or_else(|| fail(i + 1, "malformed table"))?;
            if columns.join("|") != *header {
                return Err(fail(i + 1, "unsupported table header"));
            }
            i += 1;
            let sep = lines
                .get(i)
                .and_then(|s| cells(s))
                .ok_or_else(|| fail(i + 1, "missing table separator"))?;
            if sep.len() != columns.len()
                || sep.iter().any(|s| {
                    let s = s.trim_matches(':');
                    s.len() < 3 || !s.bytes().all(|c| c == b'-')
                })
            {
                return Err(fail(i + 1, "malformed table separator"));
            }
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() && !lines[i].starts_with('#') {
                if lines[i].starts_with("    ") || lines[i].starts_with('\t') {
                    return Err(fail(i + 1, "indented table row"));
                }
                let cells = cells(lines[i]).ok_or_else(|| fail(i + 1, "malformed table row"))?;
                if cells.len() != columns.len()
                    || cells
                        .iter()
                        .take(if n < 2 { columns.len() } else { 3 })
                        .any(String::is_empty)
                {
                    return Err(fail(i + 1, "incomplete table row"));
                }
                let id =
                    identity(&cells[0], n == 2).ok_or_else(|| fail(i + 1, "malformed identity"))?;
                if !names.insert((if n < 2 { 0 } else { n }, id.to_owned())) {
                    return Err(fail(i + 1, "duplicate identity"));
                }
                rows.push(Row {
                    source: source(PATH, i + 1),
                    table: (*name).into(),
                    identity: id.into(),
                    cells,
                });
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    if seen.len() != headers.len() {
        return Err(error(PATH, 1, "missing required table"));
    }
    Ok(rows)
}

struct Node {
    indent: usize,
    key: String,
    line: usize,
}
fn plain(value: &str) -> &str {
    let mut space = true;
    for (i, c) in value.char_indices() {
        if c == '#' && space {
            return value[..i].trim();
        }
        space = c.is_whitespace();
    }
    value.trim()
}
fn scalar(value: &str) -> Option<String> {
    let value = value.trim();
    let Some(quote @ (b'\'' | b'"')) = value.as_bytes().first().copied() else {
        return Some(plain(value).into());
    };
    let (bytes, mut i) = (value.as_bytes(), 1);
    while i < bytes.len() {
        if bytes[i] == quote {
            if quote == b'\'' && bytes.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            let tail = &value[i + 1..];
            if !(tail.trim().is_empty()
                || tail.starts_with(char::is_whitespace) && tail.trim_start().starts_with('#'))
            {
                return None;
            }
            return if quote == b'\'' {
                Some(value[1..i].replace("''", "'"))
            } else {
                serde_json::from_str(&value[..=i]).ok()
            };
        }
        i += if quote == b'"' && bytes[i] == b'\\' {
            2
        } else {
            1
        };
    }
    None
}
fn uses_like(line: &str) -> bool {
    line.split(|c: char| c.is_whitespace() || "{}[],".contains(c))
        .any(|s| s.trim_end_matches(':').trim_matches(['\'', '"']) == "uses")
}
fn data_key(key: &str) -> bool {
    matches!(
        key,
        "env" | "with" | "strategy" | "matrix" | "inputs" | "outputs" | "on" | "defaults"
    )
}
fn workflow(root: &Path, path: &str) -> Result<Vec<ActionReference>, ReadError> {
    let (mut nodes, mut seen, mut refs) = (Vec::<Node>::new(), BTreeSet::new(), Vec::new());
    let mut block = None;
    for (i, raw) in text(root, path)?.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - trimmed.len();
        if block.is_some_and(|level| indent > level) {
            continue;
        }
        block = None;
        let dash = trimmed.starts_with("- ");
        if trimmed == "-"
            || trimmed.starts_with("-\t")
            || (dash
                && (plain(&trimmed[2..]).is_empty()
                    || trimmed[2..].starts_with(char::is_whitespace)
                    || trimmed[2..].starts_with(['&', '*', '!', '|', '>'])))
        {
            return Err(error(path, i + 1, "unsupported YAML syntax"));
        }
        if raw[..indent].contains('\t')
            || (dash
                && nodes
                    .last()
                    .is_some_and(|n| n.key == "steps" && indent <= n.indent))
        {
            return Err(error(path, i + 1, "unsupported YAML syntax"));
        }
        if dash {
            while nodes.last().is_some_and(|n| n.indent >= indent) {
                nodes.pop();
            }
            nodes.push(Node {
                indent,
                key: "*".into(),
                line: i + 1,
            });
        }
        let level = indent + if dash { 2 } else { 0 };
        while nodes.last().is_some_and(|n| n.indent >= level) {
            nodes.pop();
        }
        let line = if dash { &trimmed[2..] } else { trimmed };
        if line.starts_with(['\'', '"']) {
            scalar(line).ok_or_else(|| error(path, i + 1, "unsupported quoted syntax"))?;
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            if dash || uses_like(line) {
                return Err(error(path, i + 1, "unsupported YAML syntax"));
            }
            continue;
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(error(path, i + 1, "unsupported YAML syntax"));
        }
        let quoted = value.trim_start().starts_with(['\'', '"']);
        let value = scalar(value).ok_or_else(|| error(path, i + 1, "unsupported quoted syntax"))?;
        let parent: Vec<_> = nodes.iter().map(|n| n.line).collect();
        if !seen.insert((parent, key.to_owned())) {
            return Err(error(path, i + 1, "duplicate mapping key"));
        }
        let keys: Vec<_> = nodes.iter().map(|n| n.key.as_str()).collect();
        let action = matches!(keys.as_slice(), ["jobs", _] | ["jobs", _, "steps", "*"]);
        let job = keys.as_slice() == ["jobs"];
        // A job id is a name, even when it is spelled like a data key.
        let data =
            (!job && data_key(key)) || keys.iter().enumerate().any(|(n, s)| n != 1 && data_key(s));
        if !quoted && value.starts_with(['&', '*', '!']) {
            return Err(error(path, i + 1, "unsupported YAML syntax"));
        }
        if !data && (job || matches!(key, "jobs" | "steps")) && (quoted || !value.is_empty()) {
            return Err(error(path, i + 1, "unsupported YAML syntax"));
        }
        if !quoted && value.starts_with(['|', '>']) {
            if key == "uses" || !matches!(value.as_str(), "|" | "|-" | "|+" | ">" | ">-" | ">+") {
                return Err(error(path, i + 1, "unsupported block scalar"));
            }
            block = Some(level);
        }
        if key == "uses" && action {
            if value.is_empty() {
                return Err(error(path, i + 1, "invalid uses scalar"));
            }
            if value.contains("${{")
                || (!quoted && (value.contains(['{', '}', '[', ']']) || value.contains(": ")))
            {
                return Err(error(path, i + 1, "unsupported uses scalar"));
            }
            refs.push(ActionReference {
                source: source(path, i + 1),
                reference: value,
            });
        } else if key == "uses" && !data && !job {
            return Err(error(path, i + 1, "unsupported uses context"));
        }
        nodes.push(Node {
            indent: level,
            key: key.into(),
            line: i + 1,
        });
    }
    Ok(refs)
}
fn workflows(root: &Path) -> Result<Vec<ActionReference>, ReadError> {
    let dir = ".github/workflows";
    let entries =
        std::fs::read_dir(root.join(dir)).map_err(|_| error(dir, 1, "unreadable directory"))?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error(dir, 1, "failed directory entry"))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let mut refs = Vec::new();
    for entry in entries {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| error(dir, 1, "unsupported filename"))?;
        let path = format!("{dir}/{name}");
        if !entry
            .file_type()
            .map_err(|_| error(&path, 1, "unreadable entry"))?
            .is_file()
            || !(name.ends_with(".yml") || name.ends_with(".yaml"))
        {
            return Err(error(&path, 1, "unsupported workflow entry"));
        }
        refs.extend(workflow(root, &path)?);
    }
    Ok(refs)
}
pub(crate) fn read_references(root: &Path) -> Result<References, ReadError> {
    Ok(References {
        rows: catalog(root)?,
        workflows: workflows(root)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture(case: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/deps-references")
            .join(case)
    }
    fn observed(r: &References) -> Value {
        json!({"rows": r.rows.iter().map(|r| json!([r.source.path, r.source.line, r.table, r.identity, r.cells])).collect::<Vec<_>>(),
            "workflows": r.workflows.iter().map(|r| json!([r.source.path, r.source.line, r.reference])).collect::<Vec<_>>()})
    }
    #[test]
    fn deps_references_complete_facts_from_supplied_root() {
        for case in [
            "complete",
            "contexts",
            "code-context",
            "policy-faults",
            "review-contexts",
        ] {
            let root = fixture(case);
            let expected: Value =
                serde_json::from_str(&std::fs::read_to_string(root.join("expected.json")).unwrap())
                    .unwrap();
            assert_eq!(
                observed(&read_references(&root).unwrap()),
                expected,
                "{case}"
            );
        }
    }
    #[test]
    fn deps_references_refuses_incomplete_and_ambiguous_inputs() {
        let cases: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(fixture("errors.json")).unwrap())
                .unwrap();
        for case in cases {
            let name = case[0].as_str().unwrap();
            let got = read_references(&fixture(name)).unwrap_err();
            assert_eq!(
                json!([got.source.path, got.source.line, got.message]),
                Value::Array(case.as_array().unwrap()[1..].to_vec()),
                "{name}"
            );
        }
    }
    #[test]
    fn deps_references_reads_current_sources() {
        read_references(&Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap();
    }
    #[test]
    fn deps_references_bytes_and_single_byte_mutations_do_not_panic() -> std::io::Result<()> {
        let root = std::env::temp_dir().join(format!("reference-bytes-{}", std::process::id()));
        let paths = [
            "docs/reference/dependencies.md",
            ".github/workflows/check.yml",
        ];
        for path in paths {
            let dest = root.join(path);
            std::fs::create_dir_all(dest.parent().unwrap())?;
            std::fs::copy(fixture("complete").join(path), dest)?;
        }
        let bytes = std::fs::read(fixture("bytes.bin"))?;
        assert_eq!(bytes.len(), 6400);
        for path in paths {
            let original = std::fs::read(root.join(path))?;
            for sample in bytes.as_chunks::<32>().0 {
                std::fs::write(root.join(path), sample)?;
                let _: Result<References, ReadError> = read_references(&root);
            }
            for i in 0..original.len() {
                let mut changed = original.clone();
                changed[i] ^= 1;
                std::fs::write(root.join(path), changed)?;
                let _: Result<References, ReadError> = read_references(&root);
            }
            std::fs::write(root.join(path), original)?;
        }
        std::fs::remove_dir_all(root)
    }
}
