//! The Corpus logic: pure functions over values and bytes.
//!
//! **Responsibility.** The serde types that are the schema, the check rules, ids,
//! the citation lexer, scans, the search tokenizer and ranker, the frozen strings,
//! and the two assist functions (`context_pack` and `cite_check`). These arrive with
//! the cards that follow `workspace-skeleton`.
//!
//! **Allowed dependencies.** `fieldbook-calc` only, among workspace crates. External
//! crates join through their cards, each at its pin in `docs/reference/dependencies.md`.
//!
//! **Rules.** No files, environment, clock, process or sockets: the `fieldbook` binary
//! does all I/O and passes bytes in (workspace `clippy.toml`).

/// This crate's name in the chain calc, core, render, fieldbook.
pub const LAYER: &str = "core";

#[cfg(test)]
mod tests {
    use super::LAYER;

    #[test]
    fn layer_matches_the_package_name() {
        assert_eq!(format!("fieldbook-{LAYER}"), env!("CARGO_PKG_NAME"));
    }
}
