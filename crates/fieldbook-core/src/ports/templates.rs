//! Compiled-in prompt templates for AI ports.

use std::fmt;

use super::request::{PartKind, Purpose};
use super::schema::Schema;

/// Definition of a compiled-in model prompt template.
#[derive(Debug, Clone, Copy)]
pub struct TemplateDef {
    /// Unique template identifier (e.g. `ask@1`).
    pub id: &'static str,
    /// Intended AI purpose.
    pub purpose: Purpose,
    /// Permitted part kinds in fixed order.
    pub parts: &'static [PartKind],
    /// Optional output schema generator.
    pub schema: Option<fn() -> Schema>,
    /// System instruction text.
    pub text: &'static str,
}

/// Table of all compiled-in prompt templates.
pub const TEMPLATES: &[TemplateDef] = &[TemplateDef {
    id: "ask@1",
    purpose: Purpose::Ask,
    parts: &[
        PartKind::Rules,
        PartKind::History,
        PartKind::Context,
        PartKind::Question,
    ],
    schema: None,
    text: include_str!("templates/ask@1.txt"),
}];

/// Compiled-in template handle ensuring only known templates are referenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Template(&'static str);

/// Error returned when looking up an unrecognized template identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTemplate(pub String);

impl fmt::Display for UnknownTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown template identifier: {}", self.0)
    }
}

impl std::error::Error for UnknownTemplate {}

impl Template {
    /// Resolves a template by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownTemplate`] if `id` is not present in [`TEMPLATES`].
    pub fn new(id: &str) -> Result<Self, UnknownTemplate> {
        for def in TEMPLATES {
            if def.id == id {
                return Ok(Self(def.id));
            }
        }
        Err(UnknownTemplate(id.to_owned()))
    }

    /// Returns the template identifier.
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.0
    }

    /// Returns the static template definition.
    #[must_use]
    pub fn def(self) -> &'static TemplateDef {
        for def in TEMPLATES {
            if def.id == self.0 {
                return def;
            }
        }
        &TEMPLATES[0]
    }

    /// Returns the system prompt text for this template.
    #[must_use]
    pub fn text(self) -> &'static str {
        self.def().text
    }

    /// Returns an iterator over all compiled-in template definitions.
    pub fn all() -> impl Iterator<Item = &'static TemplateDef> {
        TEMPLATES.iter()
    }
}
