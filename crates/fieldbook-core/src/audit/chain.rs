//! Hash-chain computation, suffix splitting, and strict JSON line parsing (`audit-journal`, AD-16).
//!
//! Each journal line has the form `<body(n)>,"hash":"<64 hex>"}`, where `body(n)` starts with
//! `{"seq":...` and omits the closing brace. The chain hash is:
//! `H(n) = SHA-256(prev_hex || 0x0A || body(n))` with the first `prev_hex` equal to 64 zeros.
//! Verification cuts the fixed 75-byte suffix `,"hash":"<64 hex>"}` and hashes `body(n)`
//! directly without re-serialising.

use std::fmt;

use serde::Serialize;
use serde::ser::{SerializeMap, SerializeSeq};
use sha2::{Digest, Sha256 as Sha256Hasher};

use super::vocab::{Sha256, ZERO_HASH};

/// Fixed byte length of `,"hash":"<64 hex>"}`.
pub const HASH_SUFFIX_LEN: usize = 75;

const HASH_PREFIX: &str = ",\"hash\":\"";
const HASH_SUFFIX_END: &str = "\"}";

/// Errors raised when checking the hash chain or JSON structure of a journal line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainError {
    /// Line does not end with a valid 75-byte `,"hash":"<64 hex>"}` suffix or is not a JSON object.
    MalformedLine(String),
    /// Stored hash does not equal `SHA-256(prev_hex || 0x0A || body)` (FB0800).
    HashMismatch {
        /// Recomputed hash.
        expected: Sha256,
        /// Hash recorded on the line.
        actual: Sha256,
    },
    /// Sequence number is not sequential (`expected`, `actual`) (FB0800).
    SequenceMismatch {
        /// Expected sequence number.
        expected: u64,
        /// Sequence number recorded on the line.
        actual: u64,
    },
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedLine(msg) => write!(f, "malformed journal line: {msg}"),
            Self::HashMismatch { expected, actual } => {
                write!(f, "hash mismatch: expected {expected}, found {actual}")
            }
            Self::SequenceMismatch { expected, actual } => {
                write!(f, "sequence mismatch: expected {expected}, found {actual}")
            }
        }
    }
}

impl std::error::Error for ChainError {}

/// A line whose 75-byte hash suffix has been split and verified against `prev_hex`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedLine<'a> {
    /// The prefix `body(n)` up to (and excluding) `,"hash":"<64 hex>"}`.
    pub body: &'a str,
    /// The verified 64-hex line hash.
    pub hash: Sha256,
}

/// Cut the fixed 75-byte suffix `,"hash":"<64 hex>"}` from `line` without re-serialising.
///
/// # Errors
///
/// Returns [`ChainError::MalformedLine`] if `line` contains a newline, does not start with `{`,
/// is shorter than 76 bytes, or does not end with `,"hash":"<64 lowercase hex>"}`.
pub fn split_hash_suffix(line: &str) -> Result<(&str, Sha256), ChainError> {
    if line.contains('\n') || line.contains('\r') {
        return Err(ChainError::MalformedLine(
            "embedded line break in journal line".to_owned(),
        ));
    }
    if !line.starts_with('{') || line.len() <= HASH_SUFFIX_LEN {
        return Err(ChainError::MalformedLine(
            "line is too short or does not start with `{`".to_owned(),
        ));
    }
    let split_at = line.len() - HASH_SUFFIX_LEN;
    if !line.is_char_boundary(split_at) {
        return Err(ChainError::MalformedLine(
            "suffix boundary is not a UTF-8 char boundary".to_owned(),
        ));
    }
    let body = &line[..split_at];
    let suffix = &line[split_at..];
    if !suffix.starts_with(HASH_PREFIX) || !suffix.ends_with(HASH_SUFFIX_END) {
        return Err(ChainError::MalformedLine(
            "missing `,\"hash\":\"<64 hex>\"}` suffix".to_owned(),
        ));
    }
    let hex_part = &suffix[HASH_PREFIX.len()..HASH_SUFFIX_LEN - HASH_SUFFIX_END.len()];
    let hash = Sha256::parse(hex_part)
        .map_err(|_| ChainError::MalformedLine("invalid 64-hex hash in suffix".to_owned()))?;
    Ok((body, hash))
}

/// Compute `H(n) = SHA-256(prev_hex || 0x0A || body)`.
#[must_use]
pub fn compute_hash(prev_hex: &str, body: &str) -> Sha256 {
    let mut hasher = Sha256Hasher::new();
    hasher.update(prev_hex.as_bytes());
    hasher.update(b"\n");
    hasher.update(body.as_bytes());
    let digest = hasher.finalize();
    let hex_str = crate::hex::lower(&digest);
    Sha256::parse(&hex_str).unwrap_or_else(|_| unreachable!())
}

/// Append `,"hash":"<64 hex>"}` to `body` (which begins with `{` and has no closing `}`)
/// using `prev_hex` (`ZERO_HASH` for event 1).
#[must_use]
pub fn seal_body(prev_hex: &str, body: &str) -> (String, Sha256) {
    let hash = compute_hash(prev_hex, body);
    let line = format!("{body},\"hash\":\"{hash}\"}}");
    (line, hash)
}

/// Reseal a full journal line (`{"seq":...,"hash":"<64 hex>"}`) with `prev_hex`.
///
/// # Errors
///
/// Returns [`ChainError::MalformedLine`] if `line` does not have a valid 75-byte hash suffix.
pub fn reseal_line(prev_hex: &str, line: &str) -> Result<(String, Sha256), ChainError> {
    let (body, _) = split_hash_suffix(line)?;
    Ok(seal_body(prev_hex, body))
}

/// Verify a single journal line against `prev_hex` (`ZERO_HASH` for the first line)
/// by cutting the fixed 75-byte suffix and hashing `body` without re-serialising.
///
/// # Errors
///
/// Returns [`ChainError`] if the suffix is malformed or the computed hash differs from
/// the stored hash.
pub fn verify_line<'a>(prev_hex: &str, line: &'a str) -> Result<VerifiedLine<'a>, ChainError> {
    let (body, stored_hash) = split_hash_suffix(line)?;
    let expected = compute_hash(prev_hex, body);
    if expected != stored_hash {
        return Err(ChainError::HashMismatch {
            expected,
            actual: stored_hash,
        });
    }
    Ok(VerifiedLine {
        body,
        hash: stored_hash,
    })
}

/// Return the initial `prev_hex` for sequence 1 (`ZERO_HASH`).
#[must_use]
pub const fn initial_prev_hash() -> &'static str {
    ZERO_HASH
}

/// Parsed JSON value preserving exact key order and rejecting duplicate keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawJsonValue {
    /// JSON `null`.
    Null,
    /// JSON boolean (`true` or `false`).
    Bool(bool),
    /// JSON unsigned integer.
    Uint(u64),
    /// JSON string.
    Str(String),
    /// JSON array.
    Array(Vec<RawJsonValue>),
    /// JSON object preserving exact key order.
    Object(Vec<(String, RawJsonValue)>),
}

impl RawJsonValue {
    /// Return the string slice if this value is `RawJsonValue::Str`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl Serialize for RawJsonValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_none(),
            Self::Bool(b) => serializer.serialize_bool(*b),
            Self::Uint(n) => serializer.serialize_u64(*n),
            Self::Str(s) => serializer.serialize_str(s),
            Self::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Self::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (k, v) in entries {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

/// Parsed JSON object of a journal event line, preserving every key in stored order for
/// `audit show --json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEventObject(pub Vec<(String, RawJsonValue)>);

impl RawEventObject {
    /// Look up a field value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&RawJsonValue> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

impl Serialize for RawEventObject {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

/// Parse a compact JSON object line, rejecting duplicate keys at every nesting level,
/// trailing characters, negative or fractional numbers, and unescaped control characters.
///
/// # Errors
///
/// Returns an error string if `line` is not a valid JSON object or contains duplicate keys.
pub fn parse_json_object(line: &str) -> Result<RawEventObject, String> {
    let mut parser = JsonParser::new(line.as_bytes());
    let val = parser.parse_value(0)?;
    if parser.pos != parser.input.len() {
        return Err("trailing characters after JSON object".to_owned());
    }
    match val {
        RawJsonValue::Object(entries) => Ok(RawEventObject(entries)),
        _ => Err("expected JSON object at root".to_owned()),
    }
}

/// Escape and append a JSON string literal (`"..."`) to `out`.
pub fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct JsonParser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn next_byte(&mut self) -> Result<u8, String> {
        let Some(&b) = self.input.get(self.pos) else {
            return Err("unexpected end of JSON input".to_owned());
        };
        self.pos += 1;
        Ok(b)
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if matches!(b, b' ' | b'\t') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<RawJsonValue, String> {
        if depth > 8 {
            return Err("JSON nesting depth exceeded".to_owned());
        }
        self.skip_ws();
        let b = self
            .peek()
            .ok_or_else(|| "unexpected end of JSON".to_owned())?;
        let val = match b {
            b'"' => RawJsonValue::Str(self.parse_string()?),
            b'{' => RawJsonValue::Object(self.parse_object(depth + 1)?),
            b'[' => RawJsonValue::Array(self.parse_array(depth + 1)?),
            b't' => {
                self.expect_bytes(b"true")?;
                RawJsonValue::Bool(true)
            }
            b'f' => {
                self.expect_bytes(b"false")?;
                RawJsonValue::Bool(false)
            }
            b'n' => {
                self.expect_bytes(b"null")?;
                RawJsonValue::Null
            }
            b'0'..=b'9' => RawJsonValue::Uint(self.parse_uint()?),
            _ => return Err(format!("unexpected byte `0x{b:02x}` in JSON")),
        };
        self.skip_ws();
        Ok(val)
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> Result<(), String> {
        if self.input.get(self.pos..self.pos + expected.len()) == Some(expected) {
            self.pos += expected.len();
            Ok(())
        } else {
            Err("unexpected literal in JSON".to_owned())
        }
    }

    fn parse_uint(&mut self) -> Result<u64, String> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        let slice = &self.input[start..self.pos];
        if slice.is_empty() || (slice.len() > 1 && slice[0] == b'0') {
            return Err("invalid integer literal".to_owned());
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err("fractional or exponent numbers are not permitted".to_owned());
        }
        let text = std::str::from_utf8(slice).map_err(|_| "invalid UTF-8 in number".to_owned())?;
        text.parse::<u64>()
            .map_err(|_| "integer overflow in JSON".to_owned())
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.next_byte()? != b'"' {
            return Err("expected `\"`".to_owned());
        }
        let mut out = String::new();
        loop {
            let b = self.next_byte()?;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.next_byte()?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex_bytes = self
                                .input
                                .get(self.pos..self.pos + 4)
                                .ok_or_else(|| "truncated \\u escape".to_owned())?;
                            self.pos += 4;
                            let hex_str = std::str::from_utf8(hex_bytes)
                                .map_err(|_| "invalid \\u escape".to_owned())?;
                            let code = u32::from_str_radix(hex_str, 16)
                                .map_err(|_| "invalid hex in \\u escape".to_owned())?;
                            let ch = char::from_u32(code).ok_or_else(|| {
                                "surrogate or invalid scalar in \\u escape".to_owned()
                            })?;
                            out.push(ch);
                        }
                        _ => return Err("invalid escape sequence in JSON string".to_owned()),
                    }
                }
                0x00..=0x1f => return Err("unescaped control character in JSON string".to_owned()),
                _ => {
                    self.pos -= 1;
                    let rest = std::str::from_utf8(&self.input[self.pos..])
                        .map_err(|_| "invalid UTF-8 in JSON string".to_owned())?;
                    let ch = rest
                        .chars()
                        .next()
                        .ok_or_else(|| "unexpected end of string".to_owned())?;
                    self.pos += ch.len_utf8();
                    out.push(ch);
                }
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<Vec<RawJsonValue>, String> {
        if self.next_byte()? != b'[' {
            return Err("expected `[`".to_owned());
        }
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(items);
        }
        loop {
            items.push(self.parse_value(depth)?);
            self.skip_ws();
            match self.next_byte()? {
                b',' => {
                    self.skip_ws();
                }
                b']' => return Ok(items),
                _ => return Err("expected `,` or `]` in JSON array".to_owned()),
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<Vec<(String, RawJsonValue)>, String> {
        if self.next_byte()? != b'{' {
            return Err("expected `{`".to_owned());
        }
        self.skip_ws();
        let mut entries: Vec<(String, RawJsonValue)> = Vec::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(entries);
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            if entries.iter().any(|(existing, _)| existing == &key) {
                return Err(format!("duplicate JSON key `{key}`"));
            }
            self.skip_ws();
            if self.next_byte()? != b':' {
                return Err("expected `:` after JSON key".to_owned());
            }
            let val = self.parse_value(depth)?;
            entries.push((key, val));
            self.skip_ws();
            match self.next_byte()? {
                b',' => {
                    self.skip_ws();
                }
                b'}' => return Ok(entries),
                _ => return Err("expected `,` or `}` in JSON object".to_owned()),
            }
        }
    }
}
