//! The schema versions this tool reads, and the tool version each one needs.
//!
//! **Responsibility.** One table: the highest schema version [`SCHEMA_VERSION_MAX`] and,
//! for each version, the first tool version that supports it ([`min_tool_version`]). The
//! `min_tool_version` field of `edition.toml` comes from here, and `check` and `build`
//! refuse a Corpus whose `schema_version` is newer than the tool supports (FB0100).
//!
//! **Rules.** Pure: no I/O, no clock, no environment. A new schema version adds a row;
//! it never edits one (the format gate, AD-21).

/// The highest `schema_version` this tool reads.
pub const SCHEMA_VERSION_MAX: u32 = 1;

/// The first tool version that supports each schema version, oldest first.
///
/// Schema 1 is the frozen v1 specification; `0.1.0` is the first tool that reads it.
const FIRST_TOOL: [(u32, &str); 1] = [(1, "0.1.0")];

/// The oldest tool version that supports `schema`, or `None` for a version this table
/// does not know.
#[must_use]
pub fn min_tool_version(schema: u32) -> Option<&'static str> {
    FIRST_TOOL
        .iter()
        .find(|(version, _)| *version == schema)
        .map(|(_, tool)| *tool)
}

#[cfg(test)]
mod tests {
    use super::{SCHEMA_VERSION_MAX, min_tool_version};

    #[test]
    fn the_maximum_is_the_newest_known_version() {
        assert_eq!(SCHEMA_VERSION_MAX, 1);
        assert_eq!(min_tool_version(SCHEMA_VERSION_MAX), Some("0.1.0"));
    }

    #[test]
    fn schema_one_needs_tool_0_1_0() {
        assert_eq!(min_tool_version(1), Some("0.1.0"));
    }

    #[test]
    fn an_unknown_schema_has_no_tool() {
        assert_eq!(min_tool_version(0), None);
        assert_eq!(min_tool_version(2), None);
        assert_eq!(min_tool_version(SCHEMA_VERSION_MAX + 1), None);
    }
}
