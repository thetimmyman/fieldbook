//! Implementation of `fieldbook search` (`docs/reference/search.md#the-command-line`).
//!
//! **Responsibility.** Read the Corpus through [`World::read_tree`] (one coherent generation under
//! the store lease), build the search index in memory, run the query with the CLI limit and print
//! the results: two lines per result, the full record of a single exact id, the abstention phrase
//! when nothing matches, or the `--json` report.
//!
//! **Rules.** Reads only: it writes no file and opens no socket. The ranking is `fieldbook-core`'s
//! `query`; this module only chooses the limit and formats the output. Every Corpus string it
//! prints goes through `single_line`, so Corpus text cannot forge a line or drive a terminal.

use std::fmt;

use fieldbook_core::check::Tree;
use fieldbook_core::eval::corpus_index;
use fieldbook_core::report::SearchReport;
use fieldbook_core::schema::{Register, page, table};
use fieldbook_core::search::{DocEntry, Index, Results, is_exact_class, normalise, query};
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::Value;

use crate::cli::{EXIT_FINDINGS, EXIT_IO, EXIT_OK, SearchArgs};
use crate::{Outcome, World};

/// How many results an ordinary query shows.
pub const DEFAULT_LIMIT: usize = 10;

/// The limit for `text`: [`DEFAULT_LIMIT`], or the number of units in the exact class when that is
/// larger, so that every version of a source id (or record id, or natural key) is listed.
#[must_use]
pub fn cli_limit(index: &Index, text: &str) -> usize {
    let folded = normalise(text);
    let folded = folded.trim();
    let exact = index
        .docs
        .iter()
        .filter(|doc| is_exact_class(doc, folded))
        .count();
    DEFAULT_LIMIT.max(exact)
}

fn io_failure(message: &str) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        code: EXIT_IO,
    }
}

/// Run `fieldbook search` against `world`.
#[must_use]
pub fn run(args: &SearchArgs, world: &dyn World) -> Outcome {
    let text = args.words.join(" ");
    let tree = match world.read_tree() {
        Ok(tree) => tree,
        Err(err) => return io_failure(&err.to_string()),
    };
    let index = match corpus_index(&tree) {
        Ok(index) => index,
        Err(err) => {
            return io_failure(&format!(
                "cannot build the search index: {err}\nrun `fieldbook check` to see which file and line to fix"
            ));
        }
    };
    let results = query(&index, &text, cli_limit(&index, &text));
    let code = if results.total == 0 {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    };

    let stdout = if args.json {
        render_json(&index, &text, &results)
    } else {
        render_plain(&tree, &index, &results)
    };
    Outcome {
        stdout,
        stderr: String::new(),
        code,
    }
}

/// The `--json` payload: pretty JSON with a trailing newline.
#[must_use]
pub fn render_json(index: &Index, text: &str, results: &Results) -> String {
    let report = SearchReport::from_results(index, text, results);
    let mut json = serde_json::to_string_pretty(&report).unwrap_or_default();
    json.push('\n');
    json
}

/// The plain output: the list, or the abstention phrase when there is nothing to list.
#[must_use]
pub fn render_plain(tree: &Tree, index: &Index, results: &Results) -> String {
    if results.hits.is_empty() {
        return format!("{}\n", fieldbook_core::strings::abstention());
    }

    let sole_exact = {
        let mut exact = results.hits.iter().filter(|hit| hit.exact);
        match (exact.next(), exact.next()) {
            (Some(only), None) => index.docs.get(only.unit).map(|doc| (only.unit, doc)),
            _ => None,
        }
    };
    let record = sole_exact.and_then(|(unit, doc)| record_lines(tree, doc).map(|l| (unit, l)));

    let mut out = String::new();
    for (pos, hit) in results.hits.iter().enumerate() {
        let Some(doc) = index.docs.get(hit.unit) else {
            continue;
        };
        if let Some((unit, lines)) = &record
            && *unit == hit.unit
        {
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        }
        out.push_str(&format!(
            "{}. {} ({}) {}\n",
            pos + 1,
            single_line(&doc.id),
            single_line(&doc.kind),
            single_line(&doc.title)
        ));
        let second = second_line(doc);
        if !second.is_empty() {
            out.push_str("   ");
            out.push_str(&second);
            out.push('\n');
        }
    }

    if results.total > results.hits.len() {
        out.push_str(&format!(
            "Showing {} of {}. fieldbook search <source id> lists every version.\n",
            results.hits.len(),
            results.total
        ));
    }
    out
}

/// The second line of a result: the non-empty status and dates, then the snippet, joined with
/// ` · `.
fn second_line(doc: &DocEntry) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (label, value) in [
        ("", &doc.status),
        ("as of ", &doc.as_of),
        ("effective ", &doc.effective_from),
        ("verified ", &doc.verified_on),
        ("", &doc.snippet),
    ] {
        if !value.is_empty() {
            parts.push(format!("{label}{}", single_line(value)));
        }
    }
    parts.join(" · ")
}

// --- The full record of a single exact result ------------------------------------------------

/// `text` on one line that is safe to print: a line break, tab, other control character
/// (C0, DEL, C1), line or paragraph separator, or bidirectional control becomes a visible escape
/// (`\\n`, `\\t`, `\\u{1b}`), so Corpus text can neither forge output lines nor drive a terminal.
#[must_use]
pub fn single_line(text: &str) -> String {
    use fmt::Write as _;

    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control()
                || matches!(
                    c,
                    '\u{061c}'
                        | '\u{200e}'
                        | '\u{200f}'
                        | '\u{2028}'
                        | '\u{2029}'
                        | '\u{202a}'..='\u{202e}'
                        | '\u{2066}'..='\u{2069}'
                ) =>
            {
                // Writing to a String cannot fail.
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

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
    let matches = |row: &table::Row| {
        let cell = |name: &str| {
            column(name)
                .and_then(|idx| row.cells.get(idx))
                .map_or("", String::as_str)
        };
        match register {
            // The index builds no unit from a quarantined version, so its row is never the record.
            Register::Sources => {
                cell("lifecycle") != "quarantined"
                    && format!("{}-v{}", cell("source_id"), cell("version")) == id
            }
            Register::Claims => cell("claim_id") == id,
            Register::Questions => cell("question_id") == id,
            Register::Conflicts => cell("conflict_id") == id,
            Register::Glossary => cell("term_id") == id,
        }
    };
    // A repeated key is a Corpus fault (FB0203); with no way to tell which row the unit was built
    // from, print the ordinary result instead of guessing.
    let mut candidates = parsed.rows.iter().filter(|row| matches(row));
    let row = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    Some(
        parsed
            .header
            .cells
            .iter()
            .zip(&row.cells)
            .filter(|(_, value)| !value.is_empty())
            .map(|(name, value)| format!("{}: {}", single_line(name), single_line(value)))
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
    let serialized = serde_json::to_string(&meta).ok()?;
    let FrontMatter(fields) = serde_json::from_str(&serialized).ok()?;

    let mut lines = vec![format!("path: {}", single_line(&path))];
    for (key, value) in fields {
        let rendered = match value {
            Value::Array(items) => items.iter().map(plain).collect::<Vec<_>>().join(" | "),
            other => plain(&other),
        };
        if !rendered.is_empty() {
            lines.push(format!("{}: {}", single_line(&key), single_line(&rendered)));
        }
    }
    Some(lines)
}

/// A scalar front-matter value as plain text. A date serializes as a one-field object holding its
/// text, which is unwrapped here.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(map) if map.len() == 1 => map.values().next().map_or_else(String::new, plain),
        other => other.to_string(),
    }
}

/// The top-level fields of a page's serialized front matter in schema order, each with its value,
/// read through the JSON parser rather than from a text's lines, so a value that spans lines
/// stays one value.
struct FrontMatter(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for FrontMatter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = FrontMatter;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<FrontMatter, A::Error> {
                let mut fields = Vec::new();
                while let Some(entry) = map.next_entry::<String, Value>()? {
                    fields.push(entry);
                }
                Ok(FrontMatter(fields))
            }
        }
        deserializer.deserialize_map(Fields)
    }
}
