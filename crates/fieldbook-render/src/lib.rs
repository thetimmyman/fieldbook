//! Rendering: the static site, workbooks, the search index and CSV copies.
//!
//! **Responsibility.** Turn checked Corpus values into bytes: HTML from compile-checked
//! templates, `registers.xlsx`, `search-index.js` and the Excel-safe CSV copies. Output
//! is deterministic: the same input gives the same bytes. These arrive with the cards
//! that follow `workspace-skeleton`.
//!
//! **Allowed dependencies.** `fieldbook-core` and `fieldbook-calc`, among workspace
//! crates. External crates join through their cards, each at its pin in
//! `docs/reference/dependencies.md`.
//!
//! **Rules.** No files, environment, clock, process or sockets: the `fieldbook` binary
//! does all I/O (workspace `clippy.toml`).

/// This crate's name in the chain calc, core, render, fieldbook.
pub const LAYER: &str = "render";

#[cfg(test)]
mod tests {
    use super::LAYER;

    #[test]
    fn layer_matches_the_package_name() {
        assert_eq!(format!("fieldbook-{LAYER}"), env!("CARGO_PKG_NAME"));
    }
}
