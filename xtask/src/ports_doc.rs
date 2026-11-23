//! `ports_doc`: render `docs/reference/ports.md` from the adapter registry and the compiled-in
//! template table (`ai-kernel`, AD-01, AD-22).

use std::fmt::Write as _;

use fieldbook::adapters::registry::{ADAPTERS, Row};
use fieldbook_core::ports::Port;
use fieldbook_core::ports::templates::TEMPLATES;

use crate::cards::Finding;

/// The heading that the generator writes for a registry row, `<port> <id>`.
fn heading_text(row: &Row) -> String {
    format!("{} {}", row.port.as_str(), row.id)
}

fn keys_cell(row: &Row) -> String {
    if row.settings_keys.is_empty() {
        "-".to_owned()
    } else {
        row.settings_keys
            .iter()
            .map(|k| format!("`{k}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn render_rows(rows: &[Row]) -> String {
    let mut out = String::new();
    out.push_str(
        "Generated from `fieldbook::adapters::registry::ADAPTERS` and `fieldbook_core::ports::templates::TEMPLATES` by `cargo xtask docs`. Do not edit by hand.\n",
    );
    for port in Port::ALL {
        let port_rows: Vec<&Row> = rows.iter().filter(|r| r.port == port).collect();
        if port_rows.is_empty() {
            continue;
        }
        let _ = write!(out, "\n## {}\n\n", port.as_str());
        out.push_str(
            "| ID | Default | State | Build | Crate | Needs | Sees | Settings keys | Fixtures | Suite |\n|---|---|---|---|---|---|---|---|---|---|\n",
        );
        for r in &port_rows {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} | `{}` | {} | {} | {} | `{}` | `{}` |",
                r.id,
                if r.default { "yes" } else { "no" },
                r.state.as_str(),
                r.build.as_str(),
                r.krate,
                r.needs.as_str(),
                r.sees.as_str(),
                keys_cell(r),
                r.fixtures,
                r.suite,
            );
        }
        for r in &port_rows {
            let _ = write!(out, "\n### {}\n\n", heading_text(r));
            let _ = writeln!(
                out,
                "Contract test `{}`; model calls: {}.",
                r.contract_name(),
                if r.calls_model { "yes" } else { "no" },
            );
        }
    }
    out.push_str("\n## Templates\n\n");
    out.push_str("| ID | Purpose | Parts | Schema |\n|---|---|---|---|\n");
    for t in TEMPLATES {
        let parts = t
            .parts
            .iter()
            .map(|p| format!("{p:?}").to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            t.id,
            format!("{:?}", t.purpose).to_ascii_lowercase(),
            parts,
            if t.schema.is_some() { "yes" } else { "none" },
        );
    }
    out
}

/// Render the body between `<!-- generated:ports -->` and `<!-- /generated:ports -->`.
///
/// # Errors
///
/// Never fails today; the signature is that of a registered generator.
pub fn render_ports_section(_current_body: &str) -> Result<String, String> {
    Ok(render_rows(ADAPTERS))
}

/// The backticked first cell of a table row, `| `id` | ...`.
fn first_cell_id(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("| `")?;
    let (id, _) = rest.split_once('`')?;
    Some(id)
}

fn check_ids_in(text: &str, rows: &[Row]) -> Vec<(usize, String)> {
    let mut bad = Vec::new();
    let mut section = "";
    for (i, line) in text.lines().enumerate() {
        if let Some(name) = line.strip_prefix("## ") {
            section = name.trim();
            continue;
        }
        if let Some(rest) = line.strip_prefix("### ") {
            let rest = rest.trim();
            if !rows.iter().any(|r| heading_text(r) == rest) {
                bad.push((i + 1, rest.to_owned()));
            }
            continue;
        }
        let Some(id) = first_cell_id(line) else {
            continue;
        };
        let known = if section == "Templates" {
            TEMPLATES.iter().any(|t| t.id == id)
        } else if Port::ALL.iter().any(|p| p.as_str() == section) {
            rows.iter()
                .any(|r| r.port.as_str() == section && r.id == id)
        } else {
            continue;
        };
        if !known {
            bad.push((i + 1, format!("{section} {id}")));
        }
    }
    bad
}

/// The shared registry-ID checker: every `### <port> <id>` heading of `ports.md`, every ID cell
/// of its generated tables and every template id names a row of the registry.
#[must_use]
pub fn check_registry_ids(file: &str, text: &str) -> Vec<Finding> {
    check_ids_in(text, ADAPTERS)
        .into_iter()
        .map(|(line, heading)| {
            Finding::new(
                file,
                line,
                format!("`{heading}` names no row of the registry"),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_doc_lists_registered_rows_and_the_ask_template() {
        let body = render_ports_section("").expect("render");
        assert_eq!(ADAPTERS.len(), 10);
        for r in ADAPTERS {
            assert!(
                body.contains(&format!("### {} {}\n", r.port.as_str(), r.id)),
                "heading for {} {}",
                r.port.as_str(),
                r.id
            );
            assert!(body.contains(&format!("| `{}` |", r.id)));
        }
        assert!(body.contains("## Templates"));
        assert!(body.contains("| `ask@1` | ask |"));
    }

    #[test]
    fn ports_doc_is_deterministic() {
        assert_eq!(
            render_ports_section("x").expect("a"),
            render_ports_section("y").expect("b")
        );
    }

    #[test]
    fn ports_doc_headings_pass_the_registry_id_check() {
        let body = render_ports_section("").expect("render");
        assert!(check_registry_ids("docs/reference/ports.md", &body).is_empty());
    }

    #[test]
    fn registry_id_check_names_an_unknown_heading() {
        let found = check_registry_ids("docs/reference/ports.md", "x\n### extractor nope@9\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
        assert!(found[0].message.contains("extractor nope@9"));
    }

    #[test]
    fn registry_id_check_names_an_unknown_table_row() {
        let body = render_ports_section("").expect("render");
        let bad = body.replacen("| `plain@1` |", "| `plain@9` |", 1);
        let found = check_registry_ids("docs/reference/ports.md", &bad);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("plain@9"));
        let bad_tpl = body.replacen("| `ask@1` |", "| `ask@9` |", 1);
        let found = check_registry_ids("docs/reference/ports.md", &bad_tpl);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("ask@9"));
    }
}
