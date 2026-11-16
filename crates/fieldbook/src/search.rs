//! Implementation of `fieldbook search` (`docs/reference/search.md#the-command-line`).
//!
//! **Responsibility.** Read the Corpus through [`World::read_tree`] (one coherent generation under
//! the store lease), build the search index in memory, run the query and print the results as the
//! page view, or as the flat unit list with `--units`: two lines per result, the full record of a
//! single exact id, the abstention phrase when nothing matches, or the `--json` report.
//!
//! After the output is made, a question that is not a bare id is recorded in the question log of
//! the state folder ([`crate::asked::log_question`]): an `ask` event, then one row of
//! `asked/asked.csv`.
//!
//! **Rules.** It writes nothing to the Corpus and opens no socket. Recording fails open: a log
//! that cannot be written adds one `question not recorded` line to stderr and changes neither the
//! output nor the exit code. The ranking is `fieldbook-core`'s `query` and the grouping its
//! `page_view`; this module only chooses the limits and formats the output. Every Corpus string
//! it prints goes through `single_line`, so Corpus text cannot forge a line or drive a terminal.

use std::fmt;

use fieldbook_core::check::Tree;
use fieldbook_core::eval::corpus_index;
use fieldbook_core::report::SearchReport;
use fieldbook_core::schema::{Register, page, table};
use fieldbook_core::search::{
    DocEntry, Hit, Index, PageRow, PageView, Results, ShownRows, ViewLimits, is_exact_class,
    normalise, page_view, query,
};
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::Value;

use crate::asked::{Cited, Hits, TOP_IDS, Via as AskedVia};
use crate::cli::{EXIT_FINDINGS, EXIT_IO, EXIT_OK, SearchArgs};
use crate::io::state::read_asked_settings;
use crate::{Outcome, World};
use fieldbook_core::audit::Via as JournalVia;
use fieldbook_core::labels::{ItemRef, LabelSet, StoreId};
use fieldbook_core::policy::EffectivePolicy;

/// How many results an ordinary query shows in the unit list.
pub const DEFAULT_LIMIT: usize = 10;

/// The unit-list limit for `text`: [`DEFAULT_LIMIT`], or the number of units in the exact class when that is
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
    let limit = if args.units {
        cli_limit(&index, &text)
    } else {
        index.docs.len()
    };
    let results = query(&index, &text, limit);
    let code = if results.total == 0 {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    };

    let (stdout, top_ids) = match (args.units, args.json) {
        (true, json) => {
            let out = if json {
                render_json(&index, &text, &results)
            } else {
                render_plain(&tree, &index, &results)
            };
            (out, unit_top_ids(&index, &results))
        }
        (false, json) => {
            let view = page_view(&index, &results);
            let out = if json {
                render_view_json(&index, &text, &view)
            } else {
                render_view_plain(&tree, &index, &view)
            };
            (out, view_top_ids(&index, &view))
        }
    };
    let hits = Hits {
        count: results.total as u64,
        top_ids,
        fingerprint: crate::asked::fingerprint(&tree),
    };
    drop(tree);
    let stderr = record(world, &text, &hits)
        .map(|warning| format!("{warning}\n"))
        .unwrap_or_default();
    Outcome {
        stdout,
        stderr,
        code,
    }
}

/// The ids of the first [`TOP_IDS`] units of the unit list, in rank order.
fn unit_top_ids(index: &Index, results: &Results) -> Vec<String> {
    results
        .hits
        .iter()
        .filter_map(|hit| index.docs.get(hit.unit).map(|doc| doc.id.clone()))
        .take(TOP_IDS)
        .collect()
}

/// The ids of the first [`TOP_IDS`] rows of the page view, in the order printed: exact rows, page
/// rows, then other rows.
fn view_top_ids(index: &Index, view: &PageView<'_>) -> Vec<String> {
    let shown = view.shown(ViewLimits::CLI);
    let exact = shown
        .exact
        .iter()
        .filter_map(|hit| index.docs.get(hit.unit).map(|doc| doc.id.clone()));
    let pages = shown.pages.iter().map(|row| row.page.to_owned());
    let other = shown
        .other
        .iter()
        .filter_map(|hit| index.docs.get(hit.unit).map(|doc| doc.id.clone()));
    exact.chain(pages).chain(other).take(TOP_IDS).collect()
}

/// Records the question `text` with its result `hits`; the warning line when it was not recorded.
/// A bare id, and a question whose own labels the State store cannot keep, open no state.
fn record(world: &dyn World, text: &str, hits: &Hits) -> Option<String> {
    let policy = EffectivePolicy::floor(false);
    let labels = LabelSet::owner_supplied();
    match crate::asked::precheck(world, &policy, &labels, text)? {
        Ok(()) => {}
        Err(warning) => return Some(warning),
    }
    let settings = match read_asked_settings(world.state_dir()) {
        Ok(settings) => settings,
        Err(err) => return Some(crate::asked::warning(err)),
    };
    let journal = match crate::io::state::open(world, world.state_dir(), JournalVia::Cli) {
        Ok(journal) => journal,
        Err(err) => return Some(crate::asked::warning(err)),
    };
    let inputs: Vec<ItemRef> = hits
        .top_ids
        .iter()
        .map(|id| ItemRef {
            store: StoreId::Corpus,
            id: id.clone(),
        })
        .collect();
    crate::asked::log_question(
        world,
        &journal,
        &settings,
        &policy,
        AskedVia::Search,
        &labels,
        &inputs,
        text,
        hits,
        Cited::NotChecked,
    )
    .warning
}

/// The `--json` payload of the unit list: pretty JSON with a trailing newline.
#[must_use]
pub fn render_json(index: &Index, text: &str, results: &Results) -> String {
    report_json(&SearchReport::from_results(index, text, results))
}

/// The `--json` payload of the page view: the rows the plain output lists.
#[must_use]
pub fn render_view_json(index: &Index, text: &str, view: &PageView<'_>) -> String {
    let shown = view.shown(ViewLimits::CLI);
    report_json(&SearchReport::from_page_view(
        index, text, view.total, &shown,
    ))
}

fn report_json(report: &SearchReport) -> String {
    let mut json = serde_json::to_string_pretty(report).unwrap_or_default();
    json.push('\n');
    json
}

/// The plain output of the page view: exact rows, page rows, then other rows after a heading, or
/// the abstention phrase when there is nothing to list. Ranks run across all three lists.
#[must_use]
pub fn render_view_plain(tree: &Tree, index: &Index, view: &PageView<'_>) -> String {
    if view.total == 0 {
        return format!("{}\n", fieldbook_core::strings::abstention());
    }
    let shown = view.shown(ViewLimits::CLI);
    let record = match shown.exact {
        [only] => index
            .docs
            .get(only.unit)
            .and_then(|doc| record_lines(tree, doc)),
        _ => None,
    };

    let mut out = String::new();
    let mut rank = 0;
    if let Some(lines) = &record {
        rank += 1;
        push_lines(&mut out, lines);
    } else {
        for hit in shown.exact {
            rank += 1;
            push_unit(&mut out, rank, index, hit);
        }
    }
    for row in shown.pages {
        rank += 1;
        push_page(&mut out, rank, row);
    }
    if !shown.other.is_empty() && rank > 0 {
        out.push_str("Also matched:\n");
    }
    for hit in shown.other {
        rank += 1;
        push_unit(&mut out, rank, index, hit);
    }

    if view.is_cut_by(&shown) {
        out.push_str(&cut_line(view, &shown));
        out.push('\n');
    }
    out
}

/// `shown of total noun`, with the noun in the singular when the total is one.
fn counted(shown: usize, total: usize, noun: &str) -> String {
    let plural = if total == 1 { "" } else { "s" };
    format!("{shown} of {total} {noun}{plural}")
}

/// What a cut left: the shown count out of each list that has rows, then the guidance on source ids
/// when a source or record version is among the other rows.
fn cut_line(view: &PageView<'_>, shown: &ShownRows<'_, '_>) -> String {
    let other_noun = if view.exact.is_empty() && view.pages.is_empty() {
        "result"
    } else {
        "other result"
    };
    let mut counts = Vec::new();
    if !view.pages.is_empty() {
        counts.push(counted(shown.pages.len(), view.pages.len(), "page"));
    }
    if !view.other.is_empty() {
        counts.push(counted(shown.other.len(), view.other.len(), other_noun));
    }
    let mut line = format!("Showing {}.", counts.join(" and "));
    if view
        .other
        .iter()
        .any(|hit| matches!(hit.kind.as_str(), "source" | "record"))
    {
        line.push_str(" fieldbook search <source id> lists every version.");
    }
    line
}

fn push_lines(out: &mut String, lines: &[String]) {
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

fn push_row(out: &mut String, rank: usize, id: &str, doc: &DocEntry, second: &str) {
    out.push_str(&format!(
        "{rank}. {} ({}) {}\n",
        single_line(id),
        single_line(&doc.kind),
        single_line(&doc.title)
    ));
    if !second.is_empty() {
        out.push_str("   ");
        out.push_str(second);
        out.push('\n');
    }
}

fn push_unit(out: &mut String, rank: usize, index: &Index, hit: &Hit) {
    if let Some(doc) = index.docs.get(hit.unit) {
        push_row(out, rank, &doc.id, doc, &second_line(doc));
    }
}

fn push_page(out: &mut String, rank: usize, row: &PageRow<'_>) {
    let mut parts = status_parts(row.display);
    if row.best_is_section() {
        let heading = &row.best_doc.title;
        parts.push(if heading.is_empty() {
            format!("section {}", single_line(&row.best_doc.id))
        } else {
            format!(
                "section {} ({})",
                single_line(&row.best_doc.id),
                single_line(heading)
            )
        });
    }
    if !row.best_doc.snippet.is_empty() {
        parts.push(single_line(&row.best_doc.snippet));
    }
    push_row(out, rank, row.page, row.display, &parts.join(" · "));
}

/// The plain output of the unit list, or the abstention phrase when there is nothing to list.
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
            push_lines(&mut out, lines);
            continue;
        }
        push_row(&mut out, pos + 1, &doc.id, doc, &second_line(doc));
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

/// The non-empty status and dates of `doc`, each with its label.
fn status_parts(doc: &DocEntry) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for (label, value) in [
        ("", &doc.status),
        ("as of ", &doc.as_of),
        ("effective ", &doc.effective_from),
        ("verified ", &doc.verified_on),
    ] {
        if !value.is_empty() {
            parts.push(format!("{label}{}", single_line(value)));
        }
    }
    parts
}

/// The second line of a result: the non-empty status and dates, then the snippet, joined with
/// ` · `.
fn second_line(doc: &DocEntry) -> String {
    let mut parts = status_parts(doc);
    if !doc.snippet.is_empty() {
        parts.push(single_line(&doc.snippet));
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
