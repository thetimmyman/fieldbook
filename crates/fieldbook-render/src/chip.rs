//! `CommandChip`: the one quoting function behind every command the site shows (`build-about`).
//!
//! **Rules.** A chip is one row of `strings::chips`, exactly as written, with `fieldbook` first.
//! Only an id, a lowercase area code or a page type may fill a placeholder; a text placeholder
//! stays for the owner to type, so text from a page, a register or an answer never enters a command.

use std::fmt;

use askama::Template;
use fieldbook_core::ids::{AreaCode, ClaimId, SourceId, VersionId};
use fieldbook_core::schema::PageType;
use fieldbook_core::strings::{self, ReleaseFrom};

use crate::source::render_unavailable;

/// The chip key (the verb map's intent) of the command that records a claim's check.
pub const KEY_VERIFY: &str = "Record that the owner verified a claim";

/// Whether the `verify` command exists. Until it does, a page shows its landing line, not a chip a
/// reader could copy and fail to run. `verify-status-fmt` flips this one value, and a test in
/// `crates/fieldbook/tests/chips.rs` fails while it disagrees with the command line.
pub const VERIFY_LANDED: bool = true;

/// The thing the `verify` landing line names.
pub const VERIFY_THING: &str = "The verify command";

/// The release the `verify` landing line names.
pub const VERIFY_UNTIL: &str = "Day 30";

const ICON_COPY: &str = r#"<svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><rect x="5.5" y="5.5" width="8" height="8" rx="1"/><path d="M10.5 5.5V3.5a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2"/></svg>"#;

/// A command built from the verb map, with its label and tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipCommand {
    /// The command text, with its allowed placeholders filled.
    pub command: String,
    /// The chip's intent, which names its group.
    pub label: &'static str,
    /// `tag_at_home`, or `tag_r2_laptop` for a chip that exists only from R2.
    pub tag: &'static str,
}

/// Why [`quote`] refused to build a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChipError {
    /// The key is not an intent of `strings::chips`.
    UnknownChip(String),
    /// The chip has no placeholder of that name.
    UnknownPlaceholder(String),
    /// A value was offered for a placeholder that stays for the owner to type.
    TextPlaceholder(String),
    /// The value is not the id, lowercase area code or page type the placeholder takes.
    InvalidValue {
        /// The placeholder's name.
        placeholder: String,
        /// The refused value.
        value: String,
    },
}

impl fmt::Display for ChipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownChip(key) => write!(f, "no chip with the key `{key}`"),
            Self::UnknownPlaceholder(name) => write!(f, "the chip has no placeholder `{name}`"),
            Self::TextPlaceholder(name) => write!(f, "`{name}` stays for the owner to type"),
            Self::InvalidValue { placeholder, value } => {
                write!(f, "`{value}` cannot fill the placeholder `{placeholder}`")
            }
        }
    }
}

impl std::error::Error for ChipError {}

fn is_proposal_id(value: &str) -> bool {
    value
        .strip_prefix("P-")
        .is_some_and(|n| n.len() == 4 && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `Some(valid)` for a placeholder a value may fill; `None` for a text placeholder.
fn accepts(placeholder: &str, value: &str) -> Option<bool> {
    Some(match placeholder {
        "claim-id" => ClaimId::parse(value).is_ok(),
        "source-id" => SourceId::parse(value).is_ok(),
        "version-id" => VersionId::parse(value).is_ok(),
        "proposal-id" => is_proposal_id(value),
        "a" | "area" => AreaCode::parse(value).is_ok(),
        "type" => PageType::parse(value).is_ok(),
        _ => return None,
    })
}

/// Builds the command of the chip `key` (`strings::chips`, by intent), filling `values`
/// as `(placeholder, value)` pairs. A placeholder with no value stays as written.
///
/// # Errors
///
/// Returns [`ChipError`] for an unknown key or placeholder, for any value offered to a text
/// placeholder, and for a value that is not an id, a lowercase area code or a page type.
pub fn quote(key: &str, values: &[(&str, &str)]) -> Result<ChipCommand, ChipError> {
    let row = strings::chips()
        .iter()
        .find(|row| row.intent == key)
        .ok_or_else(|| ChipError::UnknownChip(key.to_owned()))?;
    let mut command = row.chip.to_owned();
    for &(placeholder, value) in values {
        let slot = format!("<{placeholder}>");
        if !row.chip.contains(&slot) {
            return Err(ChipError::UnknownPlaceholder(placeholder.to_owned()));
        }
        match accepts(placeholder, value) {
            None => return Err(ChipError::TextPlaceholder(placeholder.to_owned())),
            Some(false) => {
                return Err(ChipError::InvalidValue {
                    placeholder: placeholder.to_owned(),
                    value: value.to_owned(),
                });
            }
            Some(true) => command = command.replace(&slot, value),
        }
    }
    let tag = match row.from {
        ReleaseFrom::R2 => strings::tag_r2_laptop(),
        ReleaseFrom::Day1 | ReleaseFrom::Day30 | ReleaseFrom::Day90 => strings::tag_at_home(),
    };
    Ok(ChipCommand {
        command,
        label: row.intent,
        tag,
    })
}

#[derive(Template)]
#[template(path = "parts/chip.html")]
struct ChipTemplate<'a> {
    label: &'a str,
    tag: Option<&'a str>,
    text: &'a str,
    copy_kind: &'a str,
    copy_label: &'a str,
    icon: &'a str,
}

fn render_part(label: &str, tag: Option<&str>, text: &str, copy_kind: &str, noun: &str) -> String {
    let copy_label = format!("Copy {noun}: {text}");
    ChipTemplate {
        label,
        tag,
        text,
        copy_kind,
        copy_label: &copy_label,
        icon: ICON_COPY,
    }
    .render()
    .unwrap_or_else(|e| unreachable!("chip.html is infallible: {e}"))
}

/// Renders a command: `role="group"`, its tag, the command in mono and a Copy button.
#[must_use]
pub fn render(chip: &ChipCommand) -> String {
    render_part(
        chip.label,
        Some(chip.tag),
        &chip.command,
        "command",
        "command",
    )
}

/// Renders the chip, or the `not_available` line for `thing` and `until` where the object it
/// acts on does not exist in the edition.
#[must_use]
pub fn render_or_landing(chip: Option<&ChipCommand>, thing: &str, until: &str) -> String {
    chip.map_or_else(|| render_unavailable(thing, until), render)
}

/// Renders an external address as text with a Copy button, in the same part with no command tag.
#[must_use]
pub fn render_address(label: &str, address: &str) -> String {
    render_part(label, None, address, "address", "address")
}
