//! Structural schema representation for model replies and MCP tools.

/// Structural schema constraining model replies and structured tool calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schema {
    /// Closed object schema with named fields.
    Object(Vec<Field>),
    /// Array schema with item schema and maximum length.
    Array {
        /// Element schema.
        items: Box<Schema>,
        /// Maximum item count.
        max: u32,
    },
    /// String schema with length bounds and optional enum variants.
    Str {
        /// Minimum Unicode scalar count.
        min_len: u32,
        /// Maximum Unicode scalar count.
        max_len: u32,
        /// Permitted string values (empty if unconstrained).
        one_of: Vec<String>,
    },
    /// Integer schema with inclusive minimum and maximum bounds.
    Int {
        /// Inclusive minimum.
        min: i64,
        /// Inclusive maximum.
        max: i64,
    },
    /// Boolean schema.
    Bool,
}

/// One named field in an object schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Schema of the field's value.
    pub schema: Schema,
    /// Whether this field is required.
    pub required: bool,
}

impl Field {
    /// Constructs a new [`Field`].
    #[must_use]
    pub fn new(name: impl Into<String>, schema: Schema, required: bool) -> Self {
        Self {
            name: name.into(),
            schema,
            required,
        }
    }

    /// Constructs a required [`Field`].
    #[must_use]
    pub fn required(name: impl Into<String>, schema: Schema) -> Self {
        Self::new(name, schema, true)
    }

    /// Constructs an optional [`Field`].
    #[must_use]
    pub fn optional(name: impl Into<String>, schema: Schema) -> Self {
        Self::new(name, schema, false)
    }
}

impl Schema {
    /// Constructs an object schema from fields.
    #[must_use]
    pub fn object(fields: Vec<Field>) -> Self {
        Self::Object(fields)
    }

    /// Constructs an array schema.
    #[must_use]
    pub fn array(items: Self, max: u32) -> Self {
        Self::Array {
            items: Box::new(items),
            max,
        }
    }

    /// Constructs a bounded string schema.
    #[must_use]
    pub fn string(min_len: u32, max_len: u32) -> Self {
        Self::Str {
            min_len,
            max_len,
            one_of: Vec::new(),
        }
    }

    /// Constructs an enum string schema with permitted values.
    #[must_use]
    pub fn string_enum(one_of: Vec<String>) -> Self {
        Self::Str {
            min_len: 0,
            max_len: u32::MAX,
            one_of,
        }
    }

    /// Constructs a bounded integer schema.
    #[must_use]
    pub const fn integer(min: i64, max: i64) -> Self {
        Self::Int { min, max }
    }

    /// Constructs a boolean schema.
    #[must_use]
    pub const fn boolean() -> Self {
        Self::Bool
    }
}
