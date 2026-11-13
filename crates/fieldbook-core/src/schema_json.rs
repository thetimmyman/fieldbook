//! Structural schema JSON emission and validation.
//!
//! **Responsibility.** Converts [`Schema`] definitions to and from JSON Schema documents
//! using a pure, closed representation ([`JsonValue`]) without external JSON dependencies,
//! and validates [`JsonValue`] payloads against schemas in deterministic depth-first order.
//!
//! **Rules.** Pure module: no filesystem, clock, network, or environment access.
//! No `serde_json` dependency in crate `fieldbook-core`.

use std::collections::BTreeSet;
use std::fmt;

use crate::ports::schema::{Field, Schema};

/// Pure JSON value representation without third-party dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    /// Null literal.
    Null,
    /// Boolean literal.
    Bool(bool),
    /// Signed 64-bit integer.
    Integer(i64),
    /// UTF-8 string.
    String(String),
    /// Ordered array of JSON values.
    Array(Vec<JsonValue>),
    /// Ordered map of key-value pairs.
    Object(Vec<(String, JsonValue)>),
}

/// Error raised when an object contains a duplicate key.
///
/// Diagnostics omit input values to ensure data-boundary guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateKey {
    /// Byte length of the duplicate key name.
    pub key_len: usize,
}

impl DuplicateKey {
    /// Constructs a new [`DuplicateKey`] error with the key length.
    #[must_use]
    pub const fn new(key_len: usize) -> Self {
        Self { key_len }
    }
}

impl fmt::Display for DuplicateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "duplicate object key (length {})", self.key_len)
    }
}

impl std::error::Error for DuplicateKey {}

/// Error converting a number to [`JsonValue::Integer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberError {
    /// Number is NaN or infinity.
    NotFinite,
    /// Number has a non-zero fractional component.
    Fractional,
    /// Number exceeds signed 64-bit integer range.
    OutOfRange,
}

impl fmt::Display for NumberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite => write!(f, "number is not finite"),
            Self::Fractional => write!(f, "number has fractional component"),
            Self::OutOfRange => write!(f, "number is out of 64-bit signed integer range"),
        }
    }
}

impl std::error::Error for NumberError {}

impl JsonValue {
    /// Constructs a [`JsonValue::Object`] after verifying no duplicate keys exist.
    ///
    /// # Errors
    /// Returns [`DuplicateKey`] on the first duplicate key found.
    pub fn object(entries: Vec<(String, JsonValue)>) -> Result<Self, DuplicateKey> {
        let mut seen = BTreeSet::new();
        for (key, _) in &entries {
            if !seen.insert(key.as_str()) {
                return Err(DuplicateKey::new(key.len()));
            }
        }
        Ok(Self::Object(entries))
    }
}

/// Converts a 64-bit float to [`JsonValue::Integer`].
///
/// # Errors
/// Refuses non-finite numbers, numbers with fractional components, and
/// numbers outside the signed 64-bit integer range.
#[expect(
    clippy::cast_possible_truncation,
    reason = "range checks ensure v fits within i64 bounds"
)]
pub fn integer_from_f64(v: f64) -> Result<JsonValue, NumberError> {
    if !v.is_finite() {
        return Err(NumberError::NotFinite);
    }
    if v.fract() != 0.0 {
        return Err(NumberError::Fractional);
    }
    const MIN_F64: f64 = -9_223_372_036_854_775_808.0;
    const MAX_EXCLUSIVE_F64: f64 = 9_223_372_036_854_775_808.0;
    if v < MIN_F64 || v >= MAX_EXCLUSIVE_F64 {
        return Err(NumberError::OutOfRange);
    }
    Ok(JsonValue::Integer(v as i64))
}

/// Converts an unsigned 64-bit integer to [`JsonValue::Integer`].
///
/// # Errors
/// Returns [`NumberError::OutOfRange`] if `v > i64::MAX`.
pub fn integer_from_u64(v: u64) -> Result<JsonValue, NumberError> {
    i64::try_from(v)
        .map(JsonValue::Integer)
        .map_err(|_| NumberError::OutOfRange)
}

/// Canonical keywords supported in Fieldbook schema definitions.
pub const SCHEMA_KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "enum",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "items",
    "maxItems",
];

/// Emits a [`Schema`] as a [`JsonValue`] JSON Schema document.
///
/// Keys are emitted in fixed order: `type`, `properties`, `required`,
/// `additionalProperties`, `enum`, `minLength`, `maxLength`, `minimum`,
/// `maximum`, `items`, `maxItems` (only those applicable to the variant).
#[must_use]
pub fn to_json_schema(schema: &Schema) -> JsonValue {
    match schema {
        Schema::Object(fields) => {
            let properties = fields
                .iter()
                .map(|f| (f.name.clone(), to_json_schema(&f.schema)))
                .collect();
            let required = fields
                .iter()
                .filter(|f| f.required)
                .map(|f| JsonValue::String(f.name.clone()))
                .collect();
            JsonValue::Object(vec![
                ("type".to_string(), JsonValue::String("object".to_string())),
                ("properties".to_string(), JsonValue::Object(properties)),
                ("required".to_string(), JsonValue::Array(required)),
                ("additionalProperties".to_string(), JsonValue::Bool(false)),
            ])
        }
        Schema::Str {
            min_len,
            max_len,
            one_of,
        } => {
            if !one_of.is_empty() {
                let variants = one_of
                    .iter()
                    .map(|s| JsonValue::String(s.clone()))
                    .collect();
                let mut entries = vec![
                    ("type".to_string(), JsonValue::String("string".to_string())),
                    ("enum".to_string(), JsonValue::Array(variants)),
                ];
                if *min_len != 0 {
                    entries.push((
                        "minLength".to_string(),
                        JsonValue::Integer(i64::from(*min_len)),
                    ));
                }
                if *max_len != u32::MAX {
                    entries.push((
                        "maxLength".to_string(),
                        JsonValue::Integer(i64::from(*max_len)),
                    ));
                }
                JsonValue::Object(entries)
            } else {
                JsonValue::Object(vec![
                    ("type".to_string(), JsonValue::String("string".to_string())),
                    (
                        "minLength".to_string(),
                        JsonValue::Integer(i64::from(*min_len)),
                    ),
                    (
                        "maxLength".to_string(),
                        JsonValue::Integer(i64::from(*max_len)),
                    ),
                ])
            }
        }
        Schema::Int { min, max } => JsonValue::Object(vec![
            ("type".to_string(), JsonValue::String("integer".to_string())),
            ("minimum".to_string(), JsonValue::Integer(*min)),
            ("maximum".to_string(), JsonValue::Integer(*max)),
        ]),
        Schema::Bool => JsonValue::Object(vec![(
            "type".to_string(),
            JsonValue::String("boolean".to_string()),
        )]),
        Schema::Array { items, max } => JsonValue::Object(vec![
            ("type".to_string(), JsonValue::String("array".to_string())),
            ("items".to_string(), to_json_schema(items)),
            ("maxItems".to_string(), JsonValue::Integer(i64::from(*max))),
        ]),
    }
}

/// Error parsing a [`JsonValue`] schema document into [`Schema`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaDocError {
    /// Schema document must be a JSON object.
    NotAnObject,
    /// Schema document contains duplicate keys.
    DuplicateKey,
    /// Keyword is not in [`SCHEMA_KEYWORDS`].
    UnknownKeyword(String),
    /// Schema missing `type` keyword.
    MissingType,
    /// Schema `type` is not a string or is unknown.
    UnknownType(String),
    /// Keyword is not valid on the given schema type.
    InvalidKeywordForType {
        /// Keyword name.
        keyword: String,
        /// Schema type name.
        schema_type: &'static str,
    },
    /// Object schema missing `additionalProperties`.
    MissingAdditionalProperties,
    /// Object schema `additionalProperties` must be literal `false`.
    InvalidAdditionalProperties,
    /// Bound keyword is missing.
    MissingBound(&'static str),
    /// Bound keyword must be an integer.
    NonIntegerBound(&'static str),
    /// Bound keyword integer value out of permitted range.
    BoundOutOfRange(&'static str),
    /// Lower bound exceeds upper bound.
    InvertedBounds(&'static str),
    /// Array schema missing `items` keyword.
    MissingItems,
    /// Array schema `items` must be a schema object.
    InvalidItems,
    /// Object schema `properties` must be an object.
    InvalidProperties,
    /// Object schema `required` must be an array of strings.
    InvalidRequired,
    /// String schema `enum` must be an array of strings.
    InvalidEnum,
}

impl fmt::Display for SchemaDocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnObject => write!(f, "schema document must be a JSON object"),
            Self::DuplicateKey => write!(f, "schema document contains duplicate keys"),
            Self::UnknownKeyword(kw) => write!(f, "unsupported keyword '{kw}'"),
            Self::MissingType => write!(f, "missing 'type' keyword"),
            Self::UnknownType(t) => write!(f, "unknown schema type '{t}'"),
            Self::InvalidKeywordForType {
                keyword,
                schema_type,
            } => {
                write!(
                    f,
                    "keyword '{keyword}' is not valid on type '{schema_type}'"
                )
            }
            Self::MissingAdditionalProperties => {
                write!(
                    f,
                    "missing 'additionalProperties' keyword on object schema (must be false)"
                )
            }
            Self::InvalidAdditionalProperties => {
                write!(
                    f,
                    "'additionalProperties' on object schema must be literal false"
                )
            }
            Self::MissingBound(b) => write!(f, "missing bound '{b}'"),
            Self::NonIntegerBound(b) => write!(f, "bound '{b}' must be an integer"),
            Self::BoundOutOfRange(b) => write!(f, "bound '{b}' is out of range"),
            Self::InvertedBounds(b) => write!(f, "inverted bounds at '{b}'"),
            Self::MissingItems => write!(f, "missing 'items' keyword on array schema"),
            Self::InvalidItems => write!(f, "'items' on array schema must be a schema object"),
            Self::InvalidProperties => write!(f, "'properties' on object schema must be an object"),
            Self::InvalidRequired => {
                write!(f, "'required' on object schema must be an array of strings")
            }
            Self::InvalidEnum => write!(f, "'enum' on string schema must be an array of strings"),
        }
    }
}

impl std::error::Error for SchemaDocError {}

fn read_length_bound(
    entries: &[(String, JsonValue)],
    key: &'static str,
    default: Option<u32>,
) -> Result<u32, SchemaDocError> {
    if let Some((_, val)) = entries.iter().find(|(k, _)| k == key) {
        let JsonValue::Integer(n) = val else {
            return Err(SchemaDocError::NonIntegerBound(key));
        };
        if *n < 0 || *n > i64::from(u32::MAX) {
            return Err(SchemaDocError::BoundOutOfRange(key));
        }
        u32::try_from(*n).map_err(|_| SchemaDocError::BoundOutOfRange(key))
    } else {
        default.ok_or(SchemaDocError::MissingBound(key))
    }
}

/// Parses a [`JsonValue`] schema document into a typed [`Schema`].
///
/// # Errors
/// Refuses unknown keywords, missing required bounds, invalid property formats,
/// and type mismatches.
pub fn from_json_schema(value: &JsonValue) -> Result<Schema, SchemaDocError> {
    let JsonValue::Object(entries) = value else {
        return Err(SchemaDocError::NotAnObject);
    };

    let mut seen = BTreeSet::new();
    for (k, _) in entries {
        if !seen.insert(k.as_str()) {
            return Err(SchemaDocError::DuplicateKey);
        }
    }

    for (k, _) in entries {
        if !SCHEMA_KEYWORDS.contains(&k.as_str()) {
            return Err(SchemaDocError::UnknownKeyword(k.clone()));
        }
    }

    let Some((_, type_val)) = entries.iter().find(|(k, _)| k == "type") else {
        return Err(SchemaDocError::MissingType);
    };
    let JsonValue::String(type_str) = type_val else {
        return Err(SchemaDocError::UnknownType(String::new()));
    };

    match type_str.as_str() {
        "boolean" => {
            for (k, _) in entries {
                if k != "type" {
                    return Err(SchemaDocError::InvalidKeywordForType {
                        keyword: k.clone(),
                        schema_type: "boolean",
                    });
                }
            }
            Ok(Schema::Bool)
        }
        "integer" => {
            for (k, _) in entries {
                if k != "type" && k != "minimum" && k != "maximum" {
                    return Err(SchemaDocError::InvalidKeywordForType {
                        keyword: k.clone(),
                        schema_type: "integer",
                    });
                }
            }
            let Some((_, min_val)) = entries.iter().find(|(k, _)| k == "minimum") else {
                return Err(SchemaDocError::MissingBound("minimum"));
            };
            let JsonValue::Integer(min) = min_val else {
                return Err(SchemaDocError::NonIntegerBound("minimum"));
            };

            let Some((_, max_val)) = entries.iter().find(|(k, _)| k == "maximum") else {
                return Err(SchemaDocError::MissingBound("maximum"));
            };
            let JsonValue::Integer(max) = max_val else {
                return Err(SchemaDocError::NonIntegerBound("maximum"));
            };

            if *min > *max {
                return Err(SchemaDocError::InvertedBounds("minimum"));
            }
            Ok(Schema::Int {
                min: *min,
                max: *max,
            })
        }
        "string" => {
            for (k, _) in entries {
                if k != "type" && k != "enum" && k != "minLength" && k != "maxLength" {
                    return Err(SchemaDocError::InvalidKeywordForType {
                        keyword: k.clone(),
                        schema_type: "string",
                    });
                }
            }
            if let Some((_, enum_val)) = entries.iter().find(|(k, _)| k == "enum") {
                let JsonValue::Array(variants) = enum_val else {
                    return Err(SchemaDocError::InvalidEnum);
                };
                let mut one_of = Vec::with_capacity(variants.len());
                for variant in variants {
                    let JsonValue::String(s) = variant else {
                        return Err(SchemaDocError::InvalidEnum);
                    };
                    one_of.push(s.clone());
                }
                if one_of.is_empty() {
                    return Err(SchemaDocError::InvalidEnum);
                }
                let min_len = read_length_bound(entries, "minLength", Some(0))?;
                let max_len = read_length_bound(entries, "maxLength", Some(u32::MAX))?;
                if min_len > max_len {
                    return Err(SchemaDocError::InvertedBounds("minLength"));
                }
                Ok(Schema::Str {
                    min_len,
                    max_len,
                    one_of,
                })
            } else {
                let min_len = read_length_bound(entries, "minLength", Some(0))?;
                let max_len = read_length_bound(entries, "maxLength", None)?;
                if min_len > max_len {
                    return Err(SchemaDocError::InvertedBounds("minLength"));
                }
                Ok(Schema::string(min_len, max_len))
            }
        }
        "array" => {
            for (k, _) in entries {
                if k != "type" && k != "items" && k != "maxItems" {
                    return Err(SchemaDocError::InvalidKeywordForType {
                        keyword: k.clone(),
                        schema_type: "array",
                    });
                }
            }
            let Some((_, items_val)) = entries.iter().find(|(k, _)| k == "items") else {
                return Err(SchemaDocError::MissingItems);
            };
            let items_schema = match from_json_schema(items_val) {
                Ok(s) => s,
                Err(SchemaDocError::NotAnObject) => return Err(SchemaDocError::InvalidItems),
                Err(other) => return Err(other),
            };

            let Some((_, max_val)) = entries.iter().find(|(k, _)| k == "maxItems") else {
                return Err(SchemaDocError::MissingBound("maxItems"));
            };
            let JsonValue::Integer(n) = max_val else {
                return Err(SchemaDocError::NonIntegerBound("maxItems"));
            };
            if *n < 0 || *n > i64::from(u32::MAX) {
                return Err(SchemaDocError::BoundOutOfRange("maxItems"));
            }
            let max = u32::try_from(*n).map_err(|_| SchemaDocError::BoundOutOfRange("maxItems"))?;
            Ok(Schema::array(items_schema, max))
        }
        "object" => {
            for (k, _) in entries {
                if k != "type"
                    && k != "properties"
                    && k != "required"
                    && k != "additionalProperties"
                {
                    return Err(SchemaDocError::InvalidKeywordForType {
                        keyword: k.clone(),
                        schema_type: "object",
                    });
                }
            }
            let Some((_, add_val)) = entries.iter().find(|(k, _)| k == "additionalProperties")
            else {
                return Err(SchemaDocError::MissingAdditionalProperties);
            };
            if add_val != &JsonValue::Bool(false) {
                return Err(SchemaDocError::InvalidAdditionalProperties);
            }

            let mut required_names = BTreeSet::new();
            if let Some((_, req_val)) = entries.iter().find(|(k, _)| k == "required") {
                let JsonValue::Array(req_arr) = req_val else {
                    return Err(SchemaDocError::InvalidRequired);
                };
                for item in req_arr {
                    let JsonValue::String(name) = item else {
                        return Err(SchemaDocError::InvalidRequired);
                    };
                    required_names.insert(name.clone());
                }
            }

            let prop_entries =
                if let Some((_, prop_val)) = entries.iter().find(|(k, _)| k == "properties") {
                    let JsonValue::Object(p) = prop_val else {
                        return Err(SchemaDocError::InvalidProperties);
                    };
                    p
                } else {
                    return Err(SchemaDocError::InvalidProperties);
                };

            for req in &required_names {
                if !prop_entries.iter().any(|(name, _)| name == req) {
                    return Err(SchemaDocError::InvalidRequired);
                }
            }

            let mut fields = Vec::with_capacity(prop_entries.len());
            for (prop_name, prop_val) in prop_entries {
                let prop_schema = from_json_schema(prop_val)?;
                let is_req = required_names.contains(prop_name);
                fields.push(Field::new(prop_name.clone(), prop_schema, is_req));
            }
            Ok(Schema::Object(fields))
        }
        _ => Err(SchemaDocError::UnknownType(type_str.clone())),
    }
}

/// Closed failure codes for schema validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailureCode {
    /// Payload has wrong JSON type for schema node.
    Type,
    /// Required object field is missing.
    Required,
    /// Object contains unknown field (additionalProperties is false).
    Additional,
    /// String payload is not one of permitted enum variants.
    Enum,
    /// String payload length in Unicode scalar values is less than minLength.
    MinLength,
    /// String payload length in Unicode scalar values exceeds maxLength.
    MaxLength,
    /// Integer payload value is less than minimum.
    Minimum,
    /// Integer payload value exceeds maximum.
    Maximum,
    /// Array payload item count exceeds maxItems.
    MaxItems,
    /// Object payload contains duplicate keys.
    Duplicate,
}

impl FailureCode {
    /// String identifier of the failure code.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Required => "required",
            Self::Additional => "additional",
            Self::Enum => "enum",
            Self::MinLength => "min_length",
            Self::MaxLength => "max_length",
            Self::Minimum => "minimum",
            Self::Maximum => "maximum",
            Self::MaxItems => "max_items",
            Self::Duplicate => "duplicate",
        }
    }
}

impl fmt::Display for FailureCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Validation failure describing the diagnostic code and schema node index.
///
/// Diagnostics omit input values to ensure data-boundary guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    /// Machine-readable failure code.
    pub code: FailureCode,
    /// Pre-order index of the schema node that failed.
    pub node: usize,
}

impl Failure {
    /// Constructs a new [`Failure`].
    #[must_use]
    pub const fn new(code: FailureCode, node: usize) -> Self {
        Self { code, node }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "schema validation failure '{}' at node {}",
            self.code.as_str(),
            self.node
        )
    }
}

impl std::error::Error for Failure {}

fn schema_node_count(schema: &Schema) -> usize {
    match schema {
        Schema::Bool | Schema::Int { .. } | Schema::Str { .. } => 1,
        Schema::Array { items, .. } => 1 + schema_node_count(items),
        Schema::Object(fields) => {
            1 + fields
                .iter()
                .map(|f| schema_node_count(&f.schema))
                .sum::<usize>()
        }
    }
}

fn validate_inner(schema: &Schema, value: &JsonValue, node: usize) -> Result<(), Failure> {
    match schema {
        Schema::Bool => {
            if !matches!(value, JsonValue::Bool(_)) {
                return Err(Failure::new(FailureCode::Type, node));
            }
            Ok(())
        }
        Schema::Int { min, max } => {
            let JsonValue::Integer(n) = value else {
                return Err(Failure::new(FailureCode::Type, node));
            };
            if *n < *min {
                return Err(Failure::new(FailureCode::Minimum, node));
            }
            if *n > *max {
                return Err(Failure::new(FailureCode::Maximum, node));
            }
            Ok(())
        }
        Schema::Str {
            min_len,
            max_len,
            one_of,
        } => {
            let JsonValue::String(s) = value else {
                return Err(Failure::new(FailureCode::Type, node));
            };
            let count = s.chars().count();
            if count < *min_len as usize {
                return Err(Failure::new(FailureCode::MinLength, node));
            }
            if count > *max_len as usize {
                return Err(Failure::new(FailureCode::MaxLength, node));
            }
            if !one_of.is_empty() && !one_of.iter().any(|allowed| allowed == s) {
                return Err(Failure::new(FailureCode::Enum, node));
            }
            Ok(())
        }
        Schema::Array { items, max } => {
            let JsonValue::Array(elements) = value else {
                return Err(Failure::new(FailureCode::Type, node));
            };
            if elements.len() > *max as usize {
                return Err(Failure::new(FailureCode::MaxItems, node));
            }
            let item_node = node + 1;
            for elem in elements {
                validate_inner(items, elem, item_node)?;
            }
            Ok(())
        }
        Schema::Object(fields) => {
            let JsonValue::Object(entries) = value else {
                return Err(Failure::new(FailureCode::Type, node));
            };
            let mut seen = BTreeSet::new();
            for (k, _) in entries {
                if !seen.insert(k.as_str()) {
                    return Err(Failure::new(FailureCode::Duplicate, node));
                }
            }

            let mut child_node = node + 1;
            for field in fields {
                let this_child_node = child_node;
                child_node += schema_node_count(&field.schema);

                match entries.iter().find(|(k, _)| k == &field.name) {
                    Some((_, val)) => {
                        validate_inner(&field.schema, val, this_child_node)?;
                    }
                    None => {
                        if field.required {
                            return Err(Failure::new(FailureCode::Required, node));
                        }
                    }
                }
            }

            for (k, _) in entries {
                if !fields.iter().any(|f| &f.name == k) {
                    return Err(Failure::new(FailureCode::Additional, node));
                }
            }
            Ok(())
        }
    }
}

/// Validates a [`JsonValue`] payload against a [`Schema`].
///
/// Validation runs in deterministic depth-first pre-order. Returns [`Ok(())`] on success,
/// or the first [`Failure`] encountered.
///
/// Diagnostics omit all payload values to maintain strict isolation boundaries.
///
/// # Errors
/// Returns [`Failure`] on the first type mismatch, missing required property,
/// additional unknown property, out-of-bounds length or value, or duplicate key.
pub fn validate(schema: &Schema, value: &JsonValue) -> Result<(), Failure> {
    validate_inner(schema, value, 0)
}
