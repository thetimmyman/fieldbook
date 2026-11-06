//! Implementation of `fieldbook search` (`docs/reference/search.md#the-command-line`).
//!
//! **Responsibility.** Read the Corpus through [`World::read_tree`] (one coherent generation under
//! the store lease), build the search index in memory, run the query with the CLI limit and print
//! the results: two lines per result, the full record of a single exact id, the abstention phrase
//! when nothing matches, or the `--json` report.
//!
//! **Rules.** Reads only: it writes no file and opens no socket. The ranking is `fieldbook-core`'s
//! `query`; this module only chooses the limit and formats the output.

use fieldbook_core::check::Tree;
use fieldbook_core::eval::corpus_index;
use fieldbook_core::report::SearchReport;
use fieldbook_core::search::record::record_lines;
use fieldbook_core::search::{DocEntry, Index, Results, is_exact_class, normalise, query};

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
        Err(err) => return io_failure(&format!("cannot build the search index: {err}")),
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
            doc.id,
            doc.kind,
            doc.title
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
            parts.push(format!("{label}{value}"));
        }
    }
    parts.join(" · ")
}
