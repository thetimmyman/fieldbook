//! Page-type templates and completeness checks.
//!
//! **Responsibility.** Map every [`PageType`] variant to its compiled-in
//! `corpus/templates/<type>.md` template via an exhaustive `match`, and verify
//! that every variant has a valid template and a row in the Page types table of
//! `docs/reference/corpus-format.md`.

use crate::schema::vocab::PageType;

/// Returns the raw template file (`corpus/templates/<type>.md`) for `ty`.
///
/// The `match` is exhaustive so that adding a [`PageType`] variant fails to
/// compile until its template is wired in.
#[must_use]
pub const fn template(ty: PageType) -> &'static str {
    match ty {
        PageType::Concept => include_str!("../../../../corpus/templates/concept.md"),
        PageType::Procedure => include_str!("../../../../corpus/templates/procedure.md"),
        PageType::Reference => include_str!("../../../../corpus/templates/reference.md"),
        PageType::Case => include_str!("../../../../corpus/templates/case.md"),
        PageType::ClauseCard => include_str!("../../../../corpus/templates/clause-card.md"),
        PageType::Calc => include_str!("../../../../corpus/templates/calc.md"),
        PageType::Checklist => include_str!("../../../../corpus/templates/checklist.md"),
        PageType::Template => include_str!("../../../../corpus/templates/template.md"),
        PageType::Brief => include_str!("../../../../corpus/templates/brief.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::page::{MetaErrorKind, parse_meta, split};

    const CORPUS_FORMAT_MD: &str = include_str!("../../../../docs/reference/corpus-format.md");

    fn parse_page_types_table(doc: &str) -> Vec<String> {
        let mut in_table = false;
        let mut rows = Vec::new();
        for line in doc.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("| Type | Parts | Use |") {
                in_table = true;
                continue;
            }
            if in_table {
                if trimmed.starts_with("|---") {
                    continue;
                }
                if !trimmed.starts_with('|') {
                    break;
                }
                let first_cell = trimmed
                    .trim_matches('|')
                    .split('|')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('`');
                if !first_cell.is_empty() {
                    rows.push(first_cell.to_owned());
                }
            }
        }
        rows
    }

    fn fill_template_placeholders(front_matter: &str) -> String {
        front_matter
            .replace("\"<area>-<kebab>\"", "\"cmp-sample\"")
            .replace("\"<title>\"", "\"Sample title\"")
            .replace("\"<area>\"", "\"cmp\"")
            .replace("\"<subarea>\"", "\"business-systems\"")
            .replace("\"<1 to 8>\"", "1")
            .replace(
                "\"<PUBLIC-NOW, SYNTHETIC-NOW, MAP-ONBOARDING, POLICY-GATED or CONTRACT-SPECIFIC>\"",
                "\"PUBLIC-NOW\"",
            )
            .replace("\"<today>\"", "2025-11-01")
    }

    #[test]
    fn every_type_is_complete() {
        let table_types = parse_page_types_table(CORPUS_FORMAT_MD);
        assert!(
            !table_types.is_empty(),
            "Page types table not found in corpus-format.md"
        );

        for ty in PageType::ALL {
            let raw = template(ty);
            let s = split(raw).unwrap_or_else(|e| {
                panic!("template for page type {} failed split: {e}", ty.as_str())
            });

            // Raw template front matter must raise no syntax or unknown-field error.
            if let Err(err) = parse_meta(&s.front_matter) {
                assert!(
                    !matches!(
                        err.kind(),
                        MetaErrorKind::Syntax | MetaErrorKind::UnknownField
                    ),
                    "template for page type {} raised {:?}: {}",
                    ty.as_str(),
                    err.kind(),
                    err.message()
                );
            }

            // Its `type` line must name `ty`.
            let expected_type_line = format!("type = \"{}\"", ty.as_str());
            assert!(
                s.front_matter
                    .lines()
                    .any(|l| l.trim() == expected_type_line),
                "template for page type {} does not contain `{expected_type_line}`",
                ty.as_str()
            );

            // With placeholders filled, `parse_meta` must succeed with unknown fields refused
            // and produce `meta.r#type == ty`.
            let filled = fill_template_placeholders(&s.front_matter);
            let meta = parse_meta(&filled).unwrap_or_else(|e| {
                panic!(
                    "filled template for page type {} failed parse_meta: {e}",
                    ty.as_str()
                )
            });
            assert_eq!(meta.r#type, ty);

            // The Page types table in corpus-format.md must have a row for `ty`.
            assert!(
                table_types.iter().any(|row| row == ty.as_str()),
                "page type {} has no row in the Page types table of corpus-format.md",
                ty.as_str()
            );
        }

        // And the table must have no row for a type outside `PageType::ALL`.
        for row in &table_types {
            assert!(
                PageType::ALL.iter().any(|ty| ty.as_str() == row),
                "Page types table row {row:?} is not in PageType::ALL"
            );
        }
    }
}
