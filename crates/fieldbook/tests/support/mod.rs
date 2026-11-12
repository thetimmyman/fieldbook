//! Shared test support for `fieldbook` integration tests (`audit-journal`, `add-new-and-edition`).

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "shared test support modules are selectively used across test binaries"
)]

use std::path::Path;

pub mod backup_suite;
pub mod faulty;
pub mod state;
pub mod world;

use fieldbook_core::check::Severity;

/// Recursively copy `fixture` into a new [`tempfile::TempDir`].
#[must_use]
pub fn temp_copy(fixture: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temporary fixture directory");
    copy_dir_recursive(fixture, dir.path()).expect("copy fixture directory into tempdir");
    dir
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

/// One register row in the header's column order. Columns not named in `cells` are empty; a cell
/// with a comma or a double quote is quoted as `fmt` writes it.
#[must_use]
pub fn register_row(header: &str, cells: &[(&str, &str)]) -> String {
    header
        .split(',')
        .map(|col| {
            let value = cells
                .iter()
                .find(|(name, _)| *name == col)
                .map_or("", |(_, v)| *v);
            if value.contains([',', '"']) {
                format!("\"{}\"", value.replace('"', "\"\""))
            } else {
                value.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Add `row` to a register's CSV text: before the first line that starts with `before`, or at the
/// end when `before` is `None`, so the register stays in its sort order.
#[must_use]
pub fn splice_register_row(csv: &str, row: &str, before: Option<&str>) -> String {
    match before.and_then(|prefix| csv.find(&format!("\n{prefix}"))) {
        Some(at) => format!("{}\n{row}{}", &csv[..at], &csv[at..]),
        None => format!("{}{row}\n", csv.trim_end_matches('\n').to_owned() + "\n"),
    }
}

/// Set one cell of the register row whose first field is `key`, leaving every other cell as it
/// is. Fields are split and rejoined with quotes honoured, so a quoted comma stays in its cell.
pub fn set_register_cell(
    tree: &mut fieldbook_core::check::Tree,
    register: &str,
    key: &str,
    column: &str,
    value: &str,
) {
    let path = format!("corpus/registers/{register}.csv");
    let csv = String::from_utf8(
        tree.get_bytes(&path)
            .expect("fixture register present")
            .to_vec(),
    )
    .expect("fixture register is UTF-8");
    let header: Vec<String> = split_csv_line(csv.lines().next().expect("register header"));
    let col = header
        .iter()
        .position(|c| c == column)
        .expect("column in header");
    let mut hit = false;
    let lines: Vec<String> = csv
        .lines()
        .map(|line| {
            let mut cells = split_csv_line(line);
            if cells.first().map(String::as_str) == Some(key) {
                cells[col] = value.to_owned();
                hit = true;
                return cells
                    .iter()
                    .map(|v| {
                        if v.contains([',', '"']) {
                            format!("\"{}\"", v.replace('"', "\"\""))
                        } else {
                            v.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
            }
            line.to_owned()
        })
        .collect();
    assert!(hit, "row `{key}` present in `{register}`");
    tree.insert_bytes(path, (lines.join("\n") + "\n").into_bytes());
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut cur)),
            (other, _) => cur.push(other),
        }
    }
    out.push(cur);
    out
}

/// Add the canary rows and the kept-row cells that name them: [`add_left_out_canary_rows`] then
/// [`name_canaries_from_kept_rows`].
pub fn add_canary_register_rows(tree: &mut fieldbook_core::check::Tree) {
    add_left_out_canary_rows(tree);
    name_canaries_from_kept_rows(tree);
}

/// Add well-formed canary rows, one per register, whose source carries the label `np-0007`
/// that the edition selection leaves out: the source `S-0099`, a claim on it, a conflict over that
/// claim and a term citing it. Every canary text starts with `Forbidden` or is an id the canary
/// owns, so a test can search every delivered file for it. The claim canary is superseded by the
/// kept claim `CL-EST-0002`, the reciprocal of [`name_canaries_from_kept_rows`].
pub fn add_left_out_canary_rows(tree: &mut fieldbook_core::check::Tree) {
    let read = |tree: &fieldbook_core::check::Tree, name: &str| {
        let bytes = tree
            .get_bytes(&format!("corpus/registers/{name}.csv"))
            .expect("fixture register present");
        String::from_utf8(bytes.to_vec()).expect("fixture register is UTF-8")
    };
    let mut edit = |name: &str, cells: &[(&str, &str)], before: Option<&str>| {
        let csv = read(tree, name);
        let header = csv.lines().next().expect("register header").to_owned();
        let row = register_row(&header, cells);
        let out = splice_register_row(&csv, &row, before);
        tree.insert_bytes(format!("corpus/registers/{name}.csv"), out.into_bytes());
    };
    edit(
        "sources",
        &[
            ("source_id", "S-0099"),
            ("version", "1"),
            ("tag", "s-forbidden"),
            ("title", "Forbidden source canary, made up for tests"),
            ("publisher", "Synthetic Standards Board"),
            ("source_kind", "primary_law"),
            ("source_role", "primary_evidence"),
            ("authority_tier", "A"),
            ("licence", "Synthetic, no restrictions"),
            ("location", "metadata-only: made up for tests"),
            ("sha256", &"9".repeat(64)),
            ("as_of", "2026-02-01"),
            ("retrieved", "2026-02-01"),
            ("lifecycle", "active"),
            ("volatility", "V2"),
            ("handling", "public|np-0007"),
            ("note", "Forbidden source note."),
        ],
        None,
    );
    edit(
        "claims",
        &[
            ("claim_id", "CL-CMP-0099"),
            ("area", "cmp"),
            ("kind", "fact"),
            ("statement", "Forbidden claim canary statement."),
            ("as_of", "2026-02-01"),
            ("source_id", "S-0099"),
            ("version", "1"),
            ("locator_kind", "text_section"),
            ("locator", "sec. 9"),
            ("quote", "Forbidden quote."),
            ("consequential", "false"),
            ("drafted_by", "codex"),
            ("verification", "unchecked"),
            ("applicability", "general_reference"),
            ("adoption", "proposed"),
            ("data_origin", "observed"),
            ("volatility", "V2"),
            ("note", "Forbidden claim note."),
        ],
        Some("CL-EST-0001"),
    );
    edit(
        "conflicts",
        &[
            ("conflict_id", "XC-99"),
            ("area", "cmp"),
            ("topic", "Forbidden conflict canary topic"),
            ("positions", "First secret position|Second secret position"),
            ("status", "unresolved"),
            ("claim_ids", "CL-CMP-0001|CL-CMP-0099"),
            ("question_id", "Q-001"),
        ],
        None,
    );
    edit(
        "glossary",
        &[
            ("term_id", "forbidden_term_canary"),
            ("label", "Forbidden term canary"),
            ("definition", "Secret definition."),
            ("boundary_rule", "Secret boundary."),
            ("owner_area", "cmp"),
            ("source_ref", "CL-CMP-0099"),
            ("definition_status", "public_reference"),
            ("aliases", "secret_alias"),
        ],
        Some("rate_window"),
    );
    set_register_cell(
        tree,
        "claims",
        "CL-CMP-0099",
        "superseded_by",
        "CL-EST-0002",
    );
    set_register_cell(
        tree,
        "claims",
        "CL-CMP-0099",
        "applicability",
        "not_applicable",
    );
    set_register_cell(
        tree,
        "claims",
        "CL-CMP-0099",
        "adoption",
        fieldbook_core::schema::Adoption::Superseded.as_str(),
    );
}

/// Write the three cells of kept rows that name a canary of [`add_left_out_canary_rows`]:
/// `CL-EST-0002.supersedes`, `S-0001.supersedes` and `rate_window.superseded_by`. The permitted
/// view keeps these rows verbatim, so the pages must not print or link the omitted ids. On a tree
/// without the canary rows the references dangle (FB0207), and the view and every output must
/// equal those of the tree with them.
pub fn name_canaries_from_kept_rows(tree: &mut fieldbook_core::check::Tree) {
    set_register_cell(tree, "claims", "CL-EST-0002", "supersedes", "CL-CMP-0099");
    set_register_cell(tree, "sources", "S-0001", "supersedes", "S-0099-v1");
    set_register_cell(
        tree,
        "glossary",
        "rate_window",
        "superseded_by",
        "forbidden_term_canary",
    );
}

/// Assert that the site built from the plain tree equals the site built from the same tree with
/// the cells of [`name_canaries_from_kept_rows`], file for file, except `edition.toml`, whose
/// fingerprint covers the changed register bytes. A kept row's dangling reference to a record the
/// edition leaves out must render exactly as no reference, so no page shows that something was
/// left out.
pub fn assert_plain_site_equals_cells_site(
    plain: &std::collections::BTreeMap<String, Vec<u8>>,
    cells: &std::collections::BTreeMap<String, Vec<u8>>,
) {
    let plain_names: Vec<&String> = plain.keys().collect();
    let cells_names: Vec<&String> = cells.keys().collect();
    assert_eq!(
        plain_names, cells_names,
        "the kept-row cells that name left-out records must not add or drop a site file"
    );
    for (rel, bytes) in plain {
        if rel == "edition.toml" {
            // The fingerprint covers the registers, so only that line may differ.
            let lines = |b: &[u8]| -> Vec<String> {
                String::from_utf8_lossy(b)
                    .lines()
                    .filter(|l| !l.starts_with("fingerprint"))
                    .map(str::to_owned)
                    .collect()
            };
            assert_eq!(
                lines(bytes),
                cells.get(rel).map(|b| lines(b)).unwrap_or_default(),
                "a kept row's reference to a left-out record must change no line of edition.toml \
                 but its fingerprint"
            );
            continue;
        }
        assert!(
            cells.get(rel) == Some(bytes),
            "a kept row's reference to a left-out record must change no byte of {rel}: the plain \
             base and the base with those cells must render it identically"
        );
    }
}

/// Assert that `report` holds exactly the three dangling references (FB0207) that the cells of
/// [`name_canaries_from_kept_rows`] make on a tree without the canary rows, and nothing else, of
/// any severity.
pub fn assert_only_dangling_references(report: &fieldbook_core::check::Report) {
    assert_report_codes(
        report,
        &[
            ("FB0207", Severity::Error),
            ("FB0207", Severity::Error),
            ("FB0207", Severity::Error),
        ],
    );
}

/// Assert that `report` holds exactly the findings `expected`, as (code, severity) pairs in any
/// order, so no other finding of any severity is tolerated.
pub fn assert_report_codes(report: &fieldbook_core::check::Report, expected: &[(&str, Severity)]) {
    let mut found: Vec<(String, bool)> = report
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.severity == Severity::Error))
        .collect();
    let mut want: Vec<(String, bool)> = expected
        .iter()
        .map(|(code, severity)| ((*code).to_owned(), *severity == Severity::Error))
        .collect();
    found.sort();
    want.sort();
    let rendered: Vec<String> = report.diagnostics.iter().map(|d| d.render_text()).collect();
    assert_eq!(found, want, "unexpected findings:\n{}", rendered.join("\n"));
}
