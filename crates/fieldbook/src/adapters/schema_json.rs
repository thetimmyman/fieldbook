//! The edge JSON parser and reply extractor (`model-gate` steps 2 and 16).
//!
//! The shared schema conversion, validation and strict-input rules are pure and live in
//! `fieldbook_core::schema_json`. This file only re-exports them and owns `extract_json`, the one
//! place in the program that finds the JSON in a reply, plus the duplicate-key-aware parser that
//! both model-reply validation and the later MCP argument path call.

pub use fieldbook_core::schema_json::{
    Failure, FailureCode, JsonValue, SchemaDocError, from_json_schema, to_json_schema, validate,
};

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// Why a reply did not hold one findable JSON value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractError {
    /// The reply is empty or whitespace only.
    Empty,
    /// The first non-blank character is not `{`, `[` or a code fence.
    ProseBefore,
    /// One JSON value parsed and more text follows.
    TextAfter,
    /// The reply holds more than one code fence.
    TwoFences,
    /// A code fence was opened and never closed.
    FenceNotClosed,
    /// The text is not exactly one JSON value.
    NotJson,
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "reply is empty",
            Self::ProseBefore => "reply begins with prose before any JSON",
            Self::TextAfter => "text follows the JSON value",
            Self::TwoFences => "reply holds more than one code fence",
            Self::FenceNotClosed => "code fence is not closed",
            Self::NotJson => "reply is not one JSON value",
        })
    }
}

impl std::error::Error for ExtractError {}

/// Returns the single JSON value of `reply`.
///
/// It accepts bare JSON (surrounding whitespace allowed) or JSON inside exactly one code fence
/// (three backticks, optionally followed by the word `json`, and three backticks to close it),
/// with only whitespace outside it.
///
/// # Errors
///
/// Returns [`ExtractError`] naming the shape when there is not exactly one JSON value.
pub fn extract_json(reply: &str) -> Result<&str, ExtractError> {
    let text = reply.trim();
    if text.is_empty() {
        return Err(ExtractError::Empty);
    }

    if let Some(rest) = text.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        let rest = rest.trim_start_matches([' ', '\t', '\r', '\n']);
        let Some(end) = rest.find("```") else {
            return Err(ExtractError::FenceNotClosed);
        };
        let content = rest[..end].trim();
        let after = &rest[end + 3..];
        if after.contains("```") {
            return Err(ExtractError::TwoFences);
        }
        if !after.trim().is_empty() {
            return Err(ExtractError::TextAfter);
        }
        if one_json_value(content).is_none() {
            return Err(ExtractError::NotJson);
        }
        return Ok(content);
    }

    let Some(first) = text.chars().next() else {
        return Err(ExtractError::Empty);
    };
    if !matches!(first, '{' | '[') {
        return Err(ExtractError::ProseBefore);
    }

    match one_json_value(text) {
        Some(value) => Ok(value),
        None => {
            if json_prefix_parses(text) {
                Err(ExtractError::TextAfter)
            } else {
                Err(ExtractError::NotJson)
            }
        }
    }
}

/// Returns the byte offset just past the first complete JSON value that starts at `s[0]`.
fn json_value_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let first = *bytes.first()?;
    if !matches!(first, b'{' | b'[') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns `s` when it is exactly one JSON value and only whitespace follows.
fn one_json_value(s: &str) -> Option<&str> {
    let end = json_value_end(s)?;
    let value = &s[..end];
    serde_json::from_str::<serde_json::Value>(value).ok()?;
    if !s[end..].trim().is_empty() {
        return None;
    }
    Some(value)
}

/// Returns `true` when one JSON value parses at the start of `s`, whatever follows.
fn json_prefix_parses(s: &str) -> bool {
    json_value_end(s)
        .is_some_and(|end| serde_json::from_str::<serde_json::Value>(&s[..end]).is_ok())
}

/// Error of the strict edge parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The JSON is malformed.
    Json(String),
    /// Text follows the single JSON value.
    Trailing,
    /// An object repeats a key.
    DuplicateKey(usize),
    /// A number is fractional, out of range or not finite.
    Number(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(message) => write!(f, "invalid JSON: {message}"),
            Self::Trailing => f.write_str("trailing input after the JSON value"),
            Self::DuplicateKey(len) => write!(f, "duplicate JSON object key (length {len})"),
            Self::Number(reason) => write!(f, "invalid JSON number: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

struct StrictValue(JsonValue);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = StrictValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(JsonValue::Bool(v)))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
        Ok(StrictValue(JsonValue::Integer(v)))
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
        let value = i64::try_from(v).map_err(|_| E::custom("integer is out of range"))?;
        Ok(StrictValue(JsonValue::Integer(value)))
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
        if !v.is_finite() {
            return Err(E::custom("number is not finite"));
        }
        const MIN: f64 = -9_223_372_036_854_775_808.0;
        const MAX: f64 = 9_223_372_036_854_775_808.0;
        if v < MIN || v >= MAX {
            return Err(E::custom("number is out of range"));
        }
        // An integer round trip tests for a fraction without the `trunc` import that `fract`
        // adds on Windows, which the binary scan does not list.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the range check ensures v fits in i64"
        )]
        let whole = v as i64;
        #[expect(
            clippy::cast_precision_loss,
            clippy::float_cmp,
            reason = "the comparison is exact for every value that came from an f64"
        )]
        if whole as f64 != v {
            return Err(E::custom("number has a fractional component"));
        }
        Ok(StrictValue(JsonValue::Integer(whole)))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
        Ok(StrictValue(JsonValue::String(v.to_owned())))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
        Ok(StrictValue(JsonValue::String(v)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(JsonValue::Null))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(JsonValue::Null))
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        StrictValue::deserialize(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<StrictValue>()? {
            items.push(item.0);
        }
        Ok(StrictValue(JsonValue::Array(items)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        while let Some((key, value)) = map.next_entry::<String, StrictValue>()? {
            if !seen.insert(key.clone()) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate object key (length {})",
                    key.len()
                )));
            }
            entries.push((key, value.0));
        }
        Ok(StrictValue(JsonValue::Object(entries)))
    }
}

/// Parses exactly one JSON value, refusing duplicate keys, fractional or out-of-range integers,
/// non-finite numbers and trailing input.
///
/// # Errors
///
/// Returns [`ParseError`] naming the first refused shape.
pub fn parse_json_value(text: &str) -> Result<JsonValue, ParseError> {
    let mut de = serde_json::Deserializer::from_str(text);
    let value = StrictValue::deserialize(&mut de).map_err(|err| {
        let message = err.to_string();
        if message.contains("duplicate object key") {
            let len = message
                .rsplit_once(' ')
                .and_then(|(_, tail)| tail.strip_suffix(')'))
                .and_then(|tail| tail.parse::<usize>().ok())
                .unwrap_or(0);
            ParseError::DuplicateKey(len)
        } else if message.contains("fractional") {
            ParseError::Number("fractional component")
        } else if message.contains("out of range") {
            ParseError::Number("out of range")
        } else if message.contains("not finite") {
            ParseError::Number("not finite")
        } else {
            ParseError::Json(message)
        }
    })?;
    de.end().map_err(|_| ParseError::Trailing)?;
    Ok(value.0)
}

#[cfg(test)]
mod tests {
    use super::{ExtractError, ParseError, extract_json, parse_json_value};

    #[test]
    fn extract_json_reads_these_shapes() {
        let fenced = include_str!("../../../../tests/fixtures/ports/model/reply-fenced.json");
        let rows: &[(&str, Result<&str, ExtractError>)] = &[
            ("{\"answer\":1}", Ok("{\"answer\":1}")),
            (fenced, Ok("{\"answer\":1}")),
            ("```\n{\"answer\":1}\n```", Ok("{\"answer\":1}")),
            (
                "Here is the reply:\n{\"answer\":1}",
                Err(ExtractError::ProseBefore),
            ),
            (
                "```json\n{\"answer\":1}\n```\n```json\n{\"answer\":2}\n```",
                Err(ExtractError::TwoFences),
            ),
            ("{\"answer\":1} trailing", Err(ExtractError::TextAfter)),
            ("   ", Err(ExtractError::Empty)),
            ("```json\n{\"answer\":1}", Err(ExtractError::FenceNotClosed)),
        ];
        for (reply, expected) in rows {
            assert_eq!(extract_json(reply), *expected, "reply: {reply:?}");
        }
    }

    #[test]
    fn duplicate_json_keys_are_refused_before_conversion() {
        assert!(matches!(
            parse_json_value("{\"a\":1,\"a\":2}"),
            Err(ParseError::DuplicateKey(_))
        ));
        assert!(matches!(
            parse_json_value("{\"outer\":{\"x\":1,\"x\":2}}"),
            Err(ParseError::DuplicateKey(_))
        ));
        assert!(matches!(
            parse_json_value("1.5"),
            Err(ParseError::Number("fractional component"))
        ));
        assert!(matches!(
            parse_json_value("{\"a\":1} {}",),
            Err(ParseError::Trailing)
        ));
        assert_eq!(
            parse_json_value("{\"a\":1}"),
            Ok(fieldbook_core::schema_json::JsonValue::Object(vec![(
                "a".to_owned(),
                fieldbook_core::schema_json::JsonValue::Integer(1)
            )]))
        );
    }
}
