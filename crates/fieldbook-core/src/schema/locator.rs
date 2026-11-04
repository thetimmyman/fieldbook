//! Locator kinds (`locator_kind` in `claims.csv`) and the extension-locator grammar (`ext <kind> <payload>`).
//!
//! **Responsibility.** Own the append-only registry of extension locator names,
//! [`LocatorKind`] (built-in kinds plus `ext:<kind>`), [`Locator`] (built-in or
//! lossless decoded extension `{ kind, payload }`), [`ExtensionResolution`], and
//! the canonical percent-encoding/decoding rules of `docs/reference/corpus-format.md`.
//!
//! **Rules.** Pure: no I/O. Preserves unknown extension kinds and decoded UTF-8
//! payloads byte-for-byte without normalization.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::schema::rows::MAX_TEXT_CHARS;
use crate::schema::vocab::{BuiltinLocatorKind, VocabError};

/// Maximum length in ASCII bytes for an `extension-kind` identifier (`[a-z][a-z0-9_]*`).
pub const MAX_EXTENSION_KIND_LEN: usize = 64;

/// Entry in the append-only registry of known extension locator kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionKindEntry {
    /// Canonical `extension-kind` name (`[a-z][a-z0-9_]*`).
    pub kind: &'static str,
    /// Short summary of what the extension locator names.
    pub summary: &'static str,
}

/// Append-only registry of extension locator kinds and their meanings.
///
/// An extension kind need not appear here to parse or round-trip structurally;
/// unknown kinds resolve as [`ExtensionResolution::Unsupported`] until a resolver lands.
pub const EXTENSION_REGISTRY: &[ExtensionKindEntry] = &[ExtensionKindEntry {
    kind: "structured_path",
    summary: "Structured document or data path (e.g., JSON/XML pointer)",
}];

/// Returns the registry metadata if `kind` is a registered extension locator kind.
#[must_use]
pub fn lookup_extension_kind(kind: &str) -> Option<&'static ExtensionKindEntry> {
    EXTENSION_REGISTRY.iter().find(|entry| entry.kind == kind)
}

/// Returns `true` when `kind` matches the extension-kind grammar
/// `^[a-z][a-z0-9_]*$` (1 to 64 ASCII bytes) and is not one of the six built-in
/// `locator_kind` names.
#[must_use]
pub fn is_valid_extension_kind(kind: &str) -> bool {
    let bytes = kind.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    if bytes.len() > MAX_EXTENSION_KIND_LEN || !first.is_ascii_lowercase() {
        return false;
    }
    if !rest
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
    {
        return false;
    }
    BuiltinLocatorKind::parse(kind).is_err()
}

/// Returns `true` when byte `b` is unreserved in an `extension-payload`
/// (stays literal without `%HH` escaping): ASCII letters, digits, `-`, `.`, `_`, `~`.
#[must_use]
pub const fn is_unreserved_payload_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

/// Encodes a non-empty UTF-8 `payload` into canonical `extension-payload` form:
/// unreserved ASCII bytes stay literal; every other byte is `%HH` with uppercase hex.
#[must_use]
pub fn encode_extension_payload(payload: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(payload.len());
    for &b in payload.as_bytes() {
        if is_unreserved_payload_byte(b) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
    }
    out
}

fn decode_upper_hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodes a canonical `extension-payload` into its exact UTF-8 string.
///
/// Refuses empty payloads, truncated or lowercase escapes, percent-encoded unreserved
/// bytes, literal bytes outside the unreserved set, and invalid UTF-8, and verifies
/// that re-encoding yields the exact input bytes.
pub fn decode_extension_payload(encoded: &str) -> Result<String, LocatorError> {
    if encoded.is_empty() {
        return Err(LocatorError::EmptyExtensionPayload);
    }
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        let b = bytes[idx];
        if b == b'%' {
            let Some(&hi_byte) = bytes.get(idx + 1) else {
                return Err(LocatorError::MalformedExtensionEscape);
            };
            let Some(&lo_byte) = bytes.get(idx + 2) else {
                return Err(LocatorError::MalformedExtensionEscape);
            };
            let Some(hi) = decode_upper_hex_nibble(hi_byte) else {
                return Err(LocatorError::MalformedExtensionEscape);
            };
            let Some(lo) = decode_upper_hex_nibble(lo_byte) else {
                return Err(LocatorError::MalformedExtensionEscape);
            };
            let val = (hi << 4) | lo;
            if is_unreserved_payload_byte(val) {
                return Err(LocatorError::EncodedUnreservedByte);
            }
            decoded.push(val);
            idx += 3;
        } else if is_unreserved_payload_byte(b) {
            decoded.push(b);
            idx += 1;
        } else {
            return Err(LocatorError::UnescapedPayloadByte);
        }
    }

    let utf8 = String::from_utf8(decoded).map_err(|_| LocatorError::InvalidExtensionUtf8)?;
    if encode_extension_payload(&utf8) != encoded {
        return Err(LocatorError::NonCanonicalExtension);
    }
    Ok(utf8)
}

/// Value of the `locator_kind` column in `claims.csv`: either one of the six closed
/// built-in kinds or an extension kind `ext:<kind>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LocatorKind {
    /// `pdf_page_box`
    PdfPageBox,
    /// `text_section`
    TextSection,
    /// `sheet_cells`
    SheetCells,
    /// `record_key`
    RecordKey,
    /// `no_fine_locator`
    NoFineLocator,
    /// `record_field` (company store only)
    RecordField,
    /// `ext:<kind>`
    Extension(String),
}

impl LocatorKind {
    /// The six built-in locator kinds in reference table order.
    pub const BUILTIN_ALL: [BuiltinLocatorKind; 6] = BuiltinLocatorKind::ALL;

    /// Converts a [`BuiltinLocatorKind`] into a [`LocatorKind`].
    #[must_use]
    pub const fn from_builtin(builtin: BuiltinLocatorKind) -> Self {
        match builtin {
            BuiltinLocatorKind::PdfPageBox => Self::PdfPageBox,
            BuiltinLocatorKind::TextSection => Self::TextSection,
            BuiltinLocatorKind::SheetCells => Self::SheetCells,
            BuiltinLocatorKind::RecordKey => Self::RecordKey,
            BuiltinLocatorKind::NoFineLocator => Self::NoFineLocator,
            BuiltinLocatorKind::RecordField => Self::RecordField,
        }
    }

    /// Returns `true` when this locator kind is reachable only in a company store (`record_field`).
    #[must_use]
    pub const fn is_overlay(&self) -> bool {
        matches!(self, Self::RecordField)
    }

    /// Returns the extension kind name when `self` is [`LocatorKind::Extension`].
    #[must_use]
    pub fn extension_kind(&self) -> Option<&str> {
        match self {
            Self::Extension(k) => Some(k.as_str()),
            _ => None,
        }
    }

    /// Parses a `locator_kind` cell value (`pdf_page_box`, ..., `record_field`, or `ext:<kind>`).
    pub fn parse(input: &str) -> Result<Self, VocabError> {
        if let Ok(builtin) = BuiltinLocatorKind::parse(input) {
            return Ok(Self::from_builtin(builtin));
        }
        if let Some(ext_kind) = input.strip_prefix("ext:")
            && is_valid_extension_kind(ext_kind)
        {
            return Ok(Self::Extension(ext_kind.to_owned()));
        }
        Err(VocabError::new("locator_kind", input))
    }
}

impl fmt::Display for LocatorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PdfPageBox => f.write_str("pdf_page_box"),
            Self::TextSection => f.write_str("text_section"),
            Self::SheetCells => f.write_str("sheet_cells"),
            Self::RecordKey => f.write_str("record_key"),
            Self::NoFineLocator => f.write_str("no_fine_locator"),
            Self::RecordField => f.write_str("record_field"),
            Self::Extension(kind) => write!(f, "ext:{kind}"),
        }
    }
}

impl FromStr for LocatorKind {
    type Err = VocabError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for LocatorKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for LocatorKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Error returned when parsing a [`Locator`] fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocatorError {
    /// The locator text is empty.
    Empty,
    /// The full locator text exceeds 1,000 characters.
    TooLong,
    /// The locator contains `;` or `]` or a line break.
    ForbiddenDelimiter,
    /// The locator starts with a known built-in prefix (`p.`, `pp.`, `sec.`, `§`, `para.`, `sheet`, `rec`, `field`) whose body is malformed.
    MalformedBuiltin,
    /// The locator does not start with any recognized prefix.
    UnknownPrefix,
    /// The `ext <kind> <payload>` locator has an invalid `<kind>`.
    InvalidExtensionKind,
    /// The `ext <kind> <payload>` locator has an empty payload.
    EmptyExtensionPayload,
    /// The `ext <kind> <payload>` locator has a malformed `%HH` escape.
    MalformedExtensionEscape,
    /// The `ext <kind> <payload>` locator percent-encodes an unreserved byte.
    EncodedUnreservedByte,
    /// The `ext <kind> <payload>` locator contains an unescaped non-unreserved byte.
    UnescapedPayloadByte,
    /// The decoded `ext <kind> <payload>` bytes are not valid UTF-8.
    InvalidExtensionUtf8,
    /// Re-encoding the decoded payload does not match the input text.
    NonCanonicalExtension,
}

impl fmt::Display for LocatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for LocatorError {}

/// Resolver outcome for an extension locator until a concrete resolver is registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionResolution {
    /// The extension kind has no active resolver; preserved as an unavailable fact,
    /// never a verified locator or quote match.
    Unsupported {
        /// Canonical extension kind (`[a-z][a-z0-9_]*`).
        kind: String,
        /// Lossless decoded UTF-8 payload.
        payload: String,
    },
}

/// Parsed locator value (`p. 14`, `pp. 14-15`, `sec. ...`, `sheet ...`, `rec ...`, `field ...`, or `ext <kind> <payload>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    /// Single PDF page (`p. <digits>`).
    Page(u32),
    /// PDF page range (`pp. <start>-<end>`).
    PageRange {
        /// First page (1-based).
        start: u32,
        /// Last page (inclusive, `>= start`).
        end: u32,
    },
    /// Section, section-sign, or paragraph locator (`sec. <label>`, `§ <label>`, `para. <label>`), stored with its full canonical text.
    Section(String),
    /// Spreadsheet locator (`sheet <name>!<range>`), stored with its full canonical text.
    Sheet(String),
    /// Record-row locator (`rec <key>`), stored with its full canonical text.
    Record(String),
    /// Company-store record field (`field <column>`), stored with its column name.
    Field(String),
    /// Extension locator (`ext <kind> <payload>`) with lossless decoded UTF-8 `payload`.
    Extension {
        /// Extension kind (`[a-z][a-z0-9_]*`).
        kind: String,
        /// Decoded UTF-8 payload (re-encoded by `Display` / [`Locator::to_canonical_string`]).
        payload: String,
    },
}

fn is_positive_digits(s: &str) -> Option<u32> {
    if s.is_empty() || s.starts_with('0') || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u32>().ok().filter(|&n| n >= 1)
}

fn is_valid_field_column(s: &str) -> bool {
    let bytes = s.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && rest
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn is_valid_sheet_name_escapes(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let Some(esc) = bytes.get(i..i + 3) else {
                return false;
            };
            if !matches!(esc, b"%25" | b"%3B" | b"%5D") {
                return false;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    true
}

fn is_valid_cell_coord(s: &str) -> bool {
    let bytes = s.as_bytes();
    let col_len = bytes.iter().take_while(|b| b.is_ascii_uppercase()).count();
    if !(1..=3).contains(&col_len) {
        return false;
    }
    let col_str = &s[..col_len];
    let row_str = &s[col_len..];
    // Column must be A..=XFD
    let col_num = col_str
        .bytes()
        .fold(0u32, |acc, b| acc * 26 + u32::from(b - b'A' + 1));
    if col_num == 0 || col_num > 16_384 {
        return false;
    }
    let Some(row_num) = is_positive_digits(row_str) else {
        return false;
    };
    row_num <= 1_048_576
}

fn is_valid_sheet_locator_body(body: &str) -> bool {
    let Some((name, range)) = body.rsplit_once('!') else {
        return false;
    };
    if !is_valid_sheet_name_escapes(name) || range.is_empty() {
        return false;
    }
    if let Some((start, end)) = range.split_once(':') {
        start != end && is_valid_cell_coord(start) && is_valid_cell_coord(end)
    } else {
        is_valid_cell_coord(range)
    }
}

impl Locator {
    /// Constructs a new [`Locator::Extension`] after validating `kind`, non-empty `payload`,
    /// and the 1,000-character limit on the encoded canonical form.
    pub fn new_extension(
        kind: impl Into<String>,
        payload: impl Into<String>,
    ) -> Result<Self, LocatorError> {
        let kind = kind.into();
        let payload = payload.into();
        if !is_valid_extension_kind(&kind) {
            return Err(LocatorError::InvalidExtensionKind);
        }
        if payload.is_empty() {
            return Err(LocatorError::EmptyExtensionPayload);
        }
        let loc = Self::Extension { kind, payload };
        if loc.to_canonical_string().chars().count() > MAX_TEXT_CHARS {
            return Err(LocatorError::TooLong);
        }
        Ok(loc)
    }

    /// Parses an `ext <kind> <payload>` locator directly.
    pub fn parse_extension(input: &str) -> Result<Self, LocatorError> {
        if input.is_empty() {
            return Err(LocatorError::Empty);
        }
        if input.chars().count() > MAX_TEXT_CHARS {
            return Err(LocatorError::TooLong);
        }
        let Some(rest) = input.strip_prefix("ext ") else {
            return Err(LocatorError::UnknownPrefix);
        };
        let Some((kind, encoded_payload)) = rest.split_once(' ') else {
            return Err(LocatorError::InvalidExtensionKind);
        };
        if !is_valid_extension_kind(kind) {
            return Err(LocatorError::InvalidExtensionKind);
        }
        let payload = decode_extension_payload(encoded_payload)?;
        Ok(Self::Extension {
            kind: kind.to_owned(),
            payload,
        })
    }

    /// Parses any canonical locator (`p. ...`, `pp. ...`, `sec. ...`, `§ ...`, `para. ...`,
    /// `sheet ...`, `rec ...`, `field ...`, or `ext <kind> <payload>`).
    ///
    /// Known built-in prefixes never fall back to extension parsing when malformed.
    pub fn parse(input: &str) -> Result<Self, LocatorError> {
        if input.is_empty() {
            return Err(LocatorError::Empty);
        }
        if input.chars().count() > MAX_TEXT_CHARS {
            return Err(LocatorError::TooLong);
        }
        if input.contains([';', ']', '\r', '\n']) {
            return Err(LocatorError::ForbiddenDelimiter);
        }

        if input.starts_with("ext ") || input == "ext" {
            return Self::parse_extension(input);
        }

        if let Some(rest) = input.strip_prefix("p. ") {
            let page = is_positive_digits(rest).ok_or(LocatorError::MalformedBuiltin)?;
            return Ok(Self::Page(page));
        }
        if input.starts_with("p.") {
            return Err(LocatorError::MalformedBuiltin);
        }

        if let Some(rest) = input.strip_prefix("pp. ") {
            let (start_s, end_s) = rest.split_once('-').ok_or(LocatorError::MalformedBuiltin)?;
            let start = is_positive_digits(start_s).ok_or(LocatorError::MalformedBuiltin)?;
            let end = is_positive_digits(end_s).ok_or(LocatorError::MalformedBuiltin)?;
            if end < start {
                return Err(LocatorError::MalformedBuiltin);
            }
            return Ok(Self::PageRange { start, end });
        }
        if input.starts_with("pp.") {
            return Err(LocatorError::MalformedBuiltin);
        }

        for prefix in ["sec. ", "§ ", "para. "] {
            if let Some(label) = input.strip_prefix(prefix) {
                if label.trim().is_empty() {
                    return Err(LocatorError::MalformedBuiltin);
                }
                return Ok(Self::Section(input.to_owned()));
            }
        }
        if input.starts_with("sec.") || input.starts_with('§') || input.starts_with("para.") {
            return Err(LocatorError::MalformedBuiltin);
        }

        if let Some(body) = input.strip_prefix("sheet ") {
            if !is_valid_sheet_locator_body(body) {
                return Err(LocatorError::MalformedBuiltin);
            }
            return Ok(Self::Sheet(input.to_owned()));
        }
        if input.starts_with("sheet") {
            return Err(LocatorError::MalformedBuiltin);
        }

        if let Some(key) = input.strip_prefix("rec ") {
            if key.trim().is_empty() {
                return Err(LocatorError::MalformedBuiltin);
            }
            return Ok(Self::Record(input.to_owned()));
        }
        if input.starts_with("rec") {
            return Err(LocatorError::MalformedBuiltin);
        }

        if let Some(col) = input.strip_prefix("field ") {
            if !is_valid_field_column(col) {
                return Err(LocatorError::MalformedBuiltin);
            }
            return Ok(Self::Field(col.to_owned()));
        }
        if input.starts_with("field") {
            return Err(LocatorError::MalformedBuiltin);
        }

        Err(LocatorError::UnknownPrefix)
    }

    /// Returns the [`LocatorKind`] corresponding to this locator.
    #[must_use]
    pub fn kind(&self) -> LocatorKind {
        match self {
            Self::Page(_) | Self::PageRange { .. } => LocatorKind::PdfPageBox,
            Self::Section(_) => LocatorKind::TextSection,
            Self::Sheet(_) => LocatorKind::SheetCells,
            Self::Record(_) => LocatorKind::RecordKey,
            Self::Field(_) => LocatorKind::RecordField,
            Self::Extension { kind, .. } => LocatorKind::Extension(kind.clone()),
        }
    }

    /// Formats this locator into its canonical string representation.
    #[must_use]
    pub fn to_canonical_string(&self) -> String {
        match self {
            Self::Page(p) => format!("p. {p}"),
            Self::PageRange { start, end } => format!("pp. {start}-{end}"),
            Self::Section(s) | Self::Sheet(s) | Self::Record(s) => s.clone(),
            Self::Field(col) => format!("field {col}"),
            Self::Extension { kind, payload } => {
                format!("ext {kind} {}", encode_extension_payload(payload))
            }
        }
    }

    /// Resolves an extension locator if `self` is [`Locator::Extension`].
    #[must_use]
    pub fn resolve_extension(&self) -> Option<ExtensionResolution> {
        match self {
            Self::Extension { kind, payload } => Some(ExtensionResolution::Unsupported {
                kind: kind.clone(),
                payload: payload.clone(),
            }),
            _ => None,
        }
    }
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_canonical_string())
    }
}

impl FromStr for Locator {
    type Err = LocatorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_escaping_and_refusals_follow_grammar() {
        // UTF-8, `%`, `;`, `]` and newline round-trip losslessly
        let raw_payload = "sec/αβ%25;]\nline2";
        let loc = Locator::new_extension("structured_path", raw_payload)
            .expect("valid extension locator");
        let encoded = loc.to_canonical_string();
        assert_eq!(
            encoded,
            "ext structured_path sec%2F%CE%B1%CE%B2%2525%3B%5D%0Aline2"
        );
        let reparsed = Locator::parse(&encoded).expect("canonical extension must parse");
        assert_eq!(reparsed, loc);
        assert_eq!(reparsed.to_canonical_string(), encoded);
        assert_eq!(
            reparsed.resolve_extension(),
            Some(ExtensionResolution::Unsupported {
                kind: "structured_path".to_owned(),
                payload: raw_payload.to_owned(),
            })
        );

        // Unknown future extension kind is accepted structurally and resolves to Unsupported
        let future = Locator::parse("ext future_xml_node %2Froot%2Fitem%5B1%5D")
            .expect("unknown extension kind must parse structurally");
        assert_eq!(
            future.kind(),
            LocatorKind::Extension("future_xml_node".to_owned())
        );
        assert_eq!(
            future.to_canonical_string(),
            "ext future_xml_node %2Froot%2Fitem%5B1%5D"
        );

        // Refusal of malformed escapes, lowercase hex, encoded unreserved bytes, empty payload
        for bad in [
            "ext structured_path ",
            "ext structured_path",
            "ext structured_path %",
            "ext structured_path %2",
            "ext structured_path %2f",
            "ext structured_path %41",
            "ext structured_path %FF",
            "ext structured_path a b",
            "ext pdf_page_box %2F1",
            "ext BadKind %2F1",
        ] {
            assert!(
                Locator::parse(bad).is_err(),
                "expected malformed extension {bad:?} to be refused"
            );
        }

        // Refusal of malformed built-in syntax (must not fall through to extension)
        for bad_builtin in [
            "p.14",
            "p. 0",
            "p. abc",
            "pp. 15-14",
            "sec.",
            "sec.   ",
            "§15.404-1",
            "sheet NoBang",
            "sheet Rates!A1:A1",
            "rec ",
            "field BadCol",
        ] {
            assert!(
                Locator::parse(bad_builtin).is_err(),
                "expected malformed built-in {bad_builtin:?} to be refused"
            );
        }
    }
}
