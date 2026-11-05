//! Shared test support for `fieldbook` integration tests (`audit-journal`, `add-new-and-edition`).

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "shared test support modules are selectively used across test binaries"
)]

use std::path::Path;

pub mod faulty;
pub mod state;
pub mod world;

/// Recursively copy `fixture` into a new [`tempfile::TempDir`].
#[must_use]
pub fn temp_copy(fixture: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temporary fixture directory");
    copy_dir_recursive(fixture, dir.path()).expect("copy fixture directory into tempdir");
    dir
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
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

/// Add well-formed canary rows, one per register, whose source carries the label `np-0007`
/// that the edition selection leaves out: the source `S-0099`, a claim on it, a conflict over that
/// claim and a term citing it. Every canary text starts with `Forbidden` or is an id the canary
/// owns, so a test can search every delivered file for it.
pub fn add_canary_register_rows(tree: &mut fieldbook_core::check::Tree) {
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
}
