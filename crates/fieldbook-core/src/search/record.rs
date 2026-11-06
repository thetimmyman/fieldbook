//! The full record of a single exact result (`docs/reference/search.md#the-command-line`).
//!
//! **Responsibility.** For the one unit in the exact class, produce the `column: value` lines of
//! its register row in column order (empty values skipped), or for a page `path:` followed by the
//! front-matter fields in schema order.
//!
//! **Rules.** Pure: no I/O. A unit with no record of its own (a page section) yields `None` and is
//! printed as an ordinary result.

use crate::check::Tree;
use crate::schema::{Register, page, table};

use super::index::DocEntry;

fn register_of(kind: &str) -> Option<Register> {
    match kind {
        "source" => Some(Register::Sources),
        "claim" => Some(Register::Claims),
        "question" => Some(Register::Questions),
        "conflict" => Some(Register::Conflicts),
        "term" => Some(Register::Glossary),
        _ => None,
    }
}

/// The `column: value` lines of the single exact unit: its register row in column order, or for a
/// page `path:` and the front-matter fields in schema order. `None` for a unit that has no record
/// of its own (a section), which is then printed as an ordinary result.
#[must_use]
pub fn record_lines(tree: &Tree, doc: &DocEntry) -> Option<Vec<String>> {
    if let Some(register) = register_of(&doc.kind) {
        return register_row_lines(tree, register, &doc.id);
    }
    if matches!(doc.kind.as_str(), "page" | "clause") && !doc.id.contains('#') {
        return page_lines(tree, &doc.id);
    }
    None
}

fn register_row_lines(tree: &Tree, register: Register, id: &str) -> Option<Vec<String>> {
    let path = format!("corpus/registers/{}", register.file_name());
    let parsed = table::read(tree.get_bytes(&path)?).ok()?;
    let column = |name: &str| parsed.header.cells.iter().position(|cell| cell == name);
    let row = parsed.rows.iter().find(|row| {
        let cell = |name: &str| {
            column(name)
                .and_then(|idx| row.cells.get(idx))
                .map_or("", String::as_str)
        };
        match register {
            Register::Sources => format!("{}-v{}", cell("source_id"), cell("version")) == id,
            Register::Claims => cell("claim_id") == id,
            Register::Questions => cell("question_id") == id,
            Register::Conflicts => cell("conflict_id") == id,
            Register::Glossary => cell("term_id") == id,
        }
    })?;
    Some(
        parsed
            .header
            .cells
            .iter()
            .zip(&row.cells)
            .filter(|(_, value)| !value.is_empty())
            .map(|(name, value)| format!("{name}: {value}"))
            .collect(),
    )
}

fn page_lines(tree: &Tree, id: &str) -> Option<Vec<String>> {
    let suffix = format!("/{id}.md");
    let (path, text) = tree
        .paths()
        .filter(|p| p.ends_with(&suffix))
        .find_map(|p| {
            let text = std::str::from_utf8(tree.get_bytes(p)?).ok()?;
            Some((p.to_owned(), text))
        })?;
    let split = page::split(text).ok()?;
    let meta = page::parse_meta(&split.front_matter).ok()?;
    let serialized = toml::to_string(&meta).ok()?;

    let mut lines = vec![format!("path: {path}")];
    for line in serialized.lines() {
        let Some((key, raw)) = line.split_once(" = ") else {
            continue;
        };
        let Ok(parsed) = toml::from_str::<toml::Table>(&format!("v = {raw}")) else {
            continue;
        };
        let Some(value) = parsed.get("v") else {
            continue;
        };
        let rendered = match value {
            toml::Value::String(text) => text.clone(),
            toml::Value::Array(items) => items
                .iter()
                .map(|item| match item {
                    toml::Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" | "),
            other => other.to_string(),
        };
        if !rendered.is_empty() {
            lines.push(format!("{key}: {rendered}"));
        }
    }
    Some(lines)
}
