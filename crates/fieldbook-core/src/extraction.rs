//! Text extraction from source originals and locator recognition.
//!
//! **Responsibility.** The pure extraction routines ([`Plain`], [`HtmlStrip`]), the
//! [`supplied_text`] helper, selector dispatch ([`select`]), page numbering helpers,
//! locator recognition from section headings ([`recognise_locators`]), and anchor
//! generation ([`section_anchor`]).
//!
//! **Rules.** Pure: no I/O. Input original size is capped at [`MAX_ORIGINAL_BYTES`].

pub mod methods;

use std::collections::BTreeMap;
use std::fmt;

use unicode_normalization::UnicodeNormalization;

pub use methods::{ExtractionMethod, METHODS, Method};

/// Maximum original file size in bytes (32 MiB).
pub const MAX_ORIGINAL_BYTES: u64 = 33_554_432;

/// Maximum pages in one extraction: page files are named pNNNN.txt, so four digits.
pub const MAX_PAGES: usize = 9_999;

/// Character threshold below which a trimmed page is considered thin.
pub const THIN_PAGE_CHARS: usize = 40;

/// File extensions supported by pure built-in extractors via [`select`].
pub const SELECT_TYPES: &[&str] = &["txt", "md", "markdown", "html", "htm", "xhtml", "svg"];

/// Tool name recorded for supplied text.
pub const SUPPLIED_TOOL: &str = "supplied";

/// Tool version recorded for supplied text.
pub const SUPPLIED_TOOL_VERSION: &str = "1";

/// Error arising during text extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    /// The input bytes do not contain valid UTF-8 text.
    NotText,
    /// The input bytes exceed the size limit.
    TooLarge {
        /// Actual size in bytes.
        bytes: u64,
    },
    /// An external extraction tool is missing from the environment.
    ToolMissing(String),
    /// An external extraction tool exited with a non-zero status.
    ToolFailed {
        /// Process exit status code.
        status: i32,
    },
    /// Required extraction data (such as a staging folder) was not found.
    Missing(&'static str),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotText => write!(f, "not valid UTF-8 text"),
            Self::TooLarge { bytes } => {
                write!(
                    f,
                    "original is {bytes} bytes, over the {MAX_ORIGINAL_BYTES}-byte limit"
                )
            }
            Self::ToolMissing(name) => write!(f, "{name} is not installed"),
            Self::ToolFailed { status } => write!(f, "the tool exited with status {status}"),
            Self::Missing(what) => write!(f, "missing {what}"),
        }
    }
}

impl std::error::Error for ExtractError {}

/// The extracted text of a single page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageText {
    /// 1-based page number.
    pub number: u32,
    /// Extracted page text.
    pub text: String,
}

/// The result of extracting text from an original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extraction {
    /// The canonical extraction method name.
    pub method: &'static str,
    /// Extracted pages in sequential order.
    pub pages: Vec<PageText>,
}

/// Common trait for text extractors.
pub trait Extractor {
    /// The unique adapter identifier (e.g. `plain@1`).
    fn id(&self) -> &'static str;
    /// Whether this extractor accepts a file with lowercased extension `ext` and header bytes `head`.
    fn accepts(&self, ext: &str, head: &[u8]) -> bool;
    /// Extract text pages from original bytes.
    fn extract(&self, bytes: &[u8], ext: &str) -> Result<Extraction, ExtractError>;
}

/// Plain-text extractor for UTF-8 text formats.
#[derive(Debug, Clone, Copy, Default)]
pub struct Plain;

impl Extractor for Plain {
    fn id(&self) -> &'static str {
        methods::PLAIN_ID
    }

    fn accepts(&self, ext: &str, _head: &[u8]) -> bool {
        matches!(ext.to_ascii_lowercase().as_str(), "txt" | "md" | "markdown")
    }

    fn extract(&self, bytes: &[u8], _ext: &str) -> Result<Extraction, ExtractError> {
        crate::versions::plain_v1::extract(bytes)
    }
}

/// HTML text extractor stripping markup and normalizing whitespace.
#[derive(Debug, Clone, Copy, Default)]
pub struct HtmlStrip;

impl Extractor for HtmlStrip {
    fn id(&self) -> &'static str {
        methods::HTML_STRIP_ID
    }

    fn accepts(&self, ext: &str, _head: &[u8]) -> bool {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "html" | "htm" | "xhtml" | "svg"
        )
    }

    fn extract(&self, bytes: &[u8], _ext: &str) -> Result<Extraction, ExtractError> {
        crate::versions::html_strip_v1::extract(bytes)
    }
}

static PLAIN: Plain = Plain;
static HTML_STRIP: HtmlStrip = HtmlStrip;

/// Select an extractor by file extension and header bytes.
///
/// The extension is compared in lowercase without a leading dot.
pub fn select(ext: &str, head: &[u8]) -> Option<&'static dyn Extractor> {
    let extractors: [&'static dyn Extractor; 2] = [&PLAIN, &HTML_STRIP];
    extractors.into_iter().find(|e| e.accepts(ext, head))
}

/// Formats a page number into its canonical stem (`p0001`).
pub fn page_stem(number: u32) -> String {
    format!("p{number:04}")
}

/// Formats a page number into its canonical file name (`p0001.txt`).
pub fn page_file_name(number: u32) -> String {
    format!("p{number:04}.txt")
}

/// The refusal for an extraction with more pages than [`MAX_PAGES`], or `None` when it fits.
///
/// Every writer of an extraction asks this one helper, so the page files `pNNNN.txt`
/// keep four digits whether the text came from a tool or was supplied by hand. The
/// message is the whole refusal, and `subject` is the version id or the original's
/// file name that the caller names the run by.
#[must_use]
pub fn page_limit_refusal(subject: &str, pages: usize) -> Option<String> {
    if pages > MAX_PAGES {
        Some(format!(
            "{subject}: {pages} pages, over the {MAX_PAGES}-page limit; nothing written"
        ))
    } else {
        None
    }
}

/// Returns the numbers of thin pages in ascending order.
pub fn thin_page_numbers(pages: &[PageText]) -> Vec<u32> {
    let mut thin = Vec::new();
    for page in pages {
        if page.text.trim().chars().count() < THIN_PAGE_CHARS {
            thin.push(page.number);
        }
    }
    thin.sort_unstable();
    thin
}

/// Compute a URL-safe anchor slug for a section label.
pub fn section_anchor(label: &str) -> String {
    let mut out = String::from("sec-");
    let mut in_hyphen = false;
    let mut body = String::new();
    for ch in label.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            if in_hyphen && !body.is_empty() {
                body.push('-');
            }
            body.push(lower);
            in_hyphen = false;
        } else {
            in_hyphen = true;
        }
    }
    out.push_str(&body);
    out
}

/// Section page span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocatorPages {
    /// The 1-based page where the locator first begins.
    pub first: u32,
    /// The 1-based page where the locator ends.
    pub last: u32,
}

/// Section labels that the page text announces.
///
/// A line that opens with `§`, `section `, `sec. ` or `sec ` names the label that
/// follows. Only a label that looks like a section number is proposed: a sentence
/// that happens to begin with one of those words proposes nothing. The lead adds
/// any other label by hand.
pub fn recognise_locators(pages: &[PageText]) -> BTreeMap<String, LocatorPages> {
    let mut locators = BTreeMap::new();
    for page in pages {
        for line in page.text.lines() {
            let trimmed = line.trim();
            let remainder = match_section_prefix(trimmed);
            let Some(rest) = remainder else { continue };
            let Some(token) = rest.split_whitespace().next() else {
                continue;
            };
            let label = token.trim_end_matches(['.', ':', ',']);
            if label.is_empty() || label.chars().count() > 40 {
                continue;
            }
            let Some(first_ch) = label.chars().next() else {
                continue;
            };
            if !first_ch.is_ascii_alphanumeric() && first_ch != '(' {
                continue;
            }
            if !label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '(' | ')' | '.' | '-'))
            {
                continue;
            }
            if !looks_like_section_label(label) {
                continue;
            }
            let norm_label: String = label.nfc().collect();
            locators.entry(norm_label).or_insert(LocatorPages {
                first: page.number,
                last: page.number,
            });
        }
    }
    locators
}

/// Whether `label` looks like a section identifier rather than a word of prose.
///
/// A digit anywhere is enough (`3`, `4.2`, `15.404-1`). Without one the label must be
/// a parenthesised sub-item (`(a)`, `(iii)`, `(e)(1)`), an upper-case Roman numeral (`IV`)
/// or a single upper-case letter (`A`). Every other word is prose: `Section is …` proposes
/// nothing, and a heading word such as `Agencies` or `Affected` is not a locator.
fn looks_like_section_label(label: &str) -> bool {
    if label.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    if is_parenthesised_sub_item(label) {
        return true;
    }
    if is_roman_numeral(label) {
        return true;
    }
    let mut chars = label.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.next().is_none()
}

/// Whether `label` is one or more parenthesised sub-items: one or two letters or digits
/// (`(a)`, `(ii)`, `(A)`, `(e)(1)`) or a single-case Roman numeral built from `I`, `V`, `X`
/// and `L` (`(iii)`, `(viii)`, `(a)(viii)`). A parenthesised word such as `(Agencies)` or
/// `(mix)` is not.
fn is_parenthesised_sub_item(label: &str) -> bool {
    let bytes = label.as_bytes();
    let mut index = 0;
    let mut groups: usize = 0;
    while index < bytes.len() {
        if bytes.get(index) != Some(&b'(') {
            return false;
        }
        index = index.saturating_add(1);
        let start = index;
        while bytes.get(index).is_some_and(|b| b.is_ascii_alphanumeric()) {
            index = index.saturating_add(1);
        }
        let Some(inner) = label.get(start..index) else {
            return false;
        };
        if inner.is_empty()
            || bytes.get(index) != Some(&b')')
            || (inner.len() > 2 && !is_sub_item_roman(inner))
        {
            return false;
        }
        index = index.saturating_add(1);
        groups = groups.saturating_add(1);
    }
    groups > 0
}

/// Tens and ones prefixes for [`is_sub_item_roman`], using `I`, `V`, `X` and `L` only.
const SUB_ITEM_TENS: [&str; 8] = ["LXXX", "LXX", "LX", "L", "XL", "XXX", "XX", "X"];
const SUB_ITEM_ONES: [&str; 9] = ["IX", "VIII", "VII", "VI", "V", "IV", "III", "II", "I"];

/// Whether `item` is a single-case Roman numeral built from `I`, `V`, `X` and `L`:
/// `(iii)`, `(viii)`, `(xxiv)`, `(VIII)`. Sub-items never use `C`, `D` or `M`, so words
/// made of those letters (`civil`, `vivid`, `did`, `mix`) are refused.
fn is_sub_item_roman(item: &str) -> bool {
    let single_case = item.bytes().all(|b| b.is_ascii_lowercase())
        || item.bytes().all(|b| b.is_ascii_uppercase());
    if !single_case || item.is_empty() {
        return false;
    }
    let mut rest = item;
    for tens in SUB_ITEM_TENS {
        if let Some(after) = rest
            .get(..tens.len())
            .filter(|p| p.eq_ignore_ascii_case(tens))
            .and_then(|_| rest.get(tens.len()..))
        {
            rest = after;
            break;
        }
    }
    for ones in SUB_ITEM_ONES {
        if let Some(after) = rest
            .get(..ones.len())
            .filter(|p| p.eq_ignore_ascii_case(ones))
            .and_then(|_| rest.get(ones.len()..))
        {
            rest = after;
            break;
        }
    }
    rest.is_empty()
}

/// Roman numeral symbols, in descending value, for [`is_roman_numeral`].
const ROMAN_SYMBOLS: [(&str, u32); 13] = [
    ("M", 1000),
    ("CM", 900),
    ("D", 500),
    ("CD", 400),
    ("C", 100),
    ("XC", 90),
    ("L", 50),
    ("XL", 40),
    ("X", 10),
    ("IX", 9),
    ("V", 5),
    ("IV", 4),
    ("I", 1),
];

/// Whether `label` is an upper-case Roman numeral: `I`, `IV`, `XII`. A lower-case or
/// mixed label is not one, so the prose word `mix` proposes nothing.
fn is_roman_numeral(label: &str) -> bool {
    let mut rest = label;
    let mut total = 0_u32;
    for (symbol, value) in ROMAN_SYMBOLS {
        while let Some(after) = rest.strip_prefix(symbol) {
            total = total.saturating_add(value);
            rest = after;
        }
    }
    rest.is_empty() && total > 0
}

fn match_section_prefix(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix('§') {
        return Some(rest);
    }
    if line
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("section "))
    {
        return line.get(8..);
    }
    if line
        .get(..5)
        .is_some_and(|p| p.eq_ignore_ascii_case("sec. "))
    {
        return line.get(5..);
    }
    if line
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("sec "))
    {
        return line.get(4..);
    }
    None
}

/// Extract text pages from pre-supplied text content.
pub fn supplied_text(bytes: &[u8]) -> Result<Extraction, ExtractError> {
    let len = u64::try_from(bytes.len()).map_err(|_| ExtractError::TooLarge { bytes: u64::MAX })?;
    if len > MAX_ORIGINAL_BYTES {
        return Err(ExtractError::TooLarge { bytes: len });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ExtractError::NotText)?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);

    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
    }

    let mut pages = Vec::new();
    let mut current_buffer = String::new();

    for segment in normalized.split_inclusive('\n') {
        let line_content = segment.strip_suffix('\n').unwrap_or(segment);
        if line_content == "[page break]" {
            let number = u32::try_from(pages.len() + 1).unwrap_or(u32::MAX);
            pages.push(PageText {
                number,
                text: std::mem::take(&mut current_buffer),
            });
        } else {
            current_buffer.push_str(segment);
        }
    }

    let number = u32::try_from(pages.len() + 1).unwrap_or(u32::MAX);
    pages.push(PageText {
        number,
        text: current_buffer,
    });

    Ok(Extraction {
        method: methods::SUPPLIED,
        pages,
    })
}

/// Typed representation of an `extraction.toml` file.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionToml {
    /// Extraction tool name.
    pub tool: String,
    /// Extraction tool version string.
    pub tool_version: String,
    /// Extraction method descriptor.
    pub method: ExtractionMethod,
    /// Total count of extracted pages.
    pub page_count: u32,
    /// Page numbers of thin pages in strictly ascending order.
    pub thin_pages: Vec<u32>,
    /// SHA-256 hex digest of the original file.
    pub original_sha256: String,
    /// Extracted page stems and their SHA-256 hex digests.
    pub pages: BTreeMap<String, String>,
    /// Section locator label mappings.
    pub locators: BTreeMap<String, LocatorPages>,
    /// Method-specific metadata table for unsupported/future methods.
    pub method_data: Option<toml::Table>,
    /// Additional root-level keys retained for unsupported/future methods.
    pub extra: BTreeMap<String, toml::Value>,
}

/// Error encountered while parsing an `extraction.toml` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionParseError {
    /// 1-based line number of the error.
    pub line: usize,
    /// 1-based column number of the error.
    pub col: usize,
    /// Diagnostic error message.
    pub message: String,
}

impl fmt::Display for ExtractionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for ExtractionParseError {}

/// Classification of why an evidence basis is considered weak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeakEvidence {
    /// The source version is marked as metadata-only.
    MetadataOnly,
    /// The extraction contains thin pages with little or no text.
    ThinPages(Vec<u32>),
    /// The page text was supplied by hand rather than extracted from an original.
    Supplied,
}

impl fmt::Display for WeakEvidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MetadataOnly => write!(f, "the version is metadata-only"),
            Self::ThinPages(pages) => {
                let list = pages
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "pages {list} have little or no text")
            }
            Self::Supplied => write!(
                f,
                "the text was supplied by hand and is not checked against the original"
            ),
        }
    }
}

/// Evaluates whether a source version or extraction provides weak evidence.
pub fn weak_evidence(location: &str, record: Option<&ExtractionToml>) -> Option<WeakEvidence> {
    if location.starts_with("metadata-only: ") {
        return Some(WeakEvidence::MetadataOnly);
    }
    let rec = record?;
    if rec.method.name() == methods::SUPPLIED {
        return Some(WeakEvidence::Supplied);
    }
    if !rec.thin_pages.is_empty() {
        return Some(WeakEvidence::ThinPages(rec.thin_pages.clone()));
    }
    None
}

/// Build an [`ExtractionToml`] record from extracted page content and metadata.
pub fn build_record(
    extraction: &Extraction,
    tool: &str,
    tool_version: &str,
    original_sha256: &str,
    kept_locators: &BTreeMap<String, LocatorPages>,
) -> ExtractionToml {
    let method = ExtractionMethod::parse(extraction.method);
    let page_count = u32::try_from(extraction.pages.len()).unwrap_or(u32::MAX);
    let thin_pages = thin_page_numbers(&extraction.pages);

    let mut pages = BTreeMap::new();
    for page in &extraction.pages {
        let stem = page_stem(page.number);
        let hash = crate::hash::sha256_hex(page.text.as_bytes());
        pages.insert(stem, hash);
    }

    let mut locators = recognise_locators(&extraction.pages);
    for (label, loc) in kept_locators {
        locators.insert(label.clone(), *loc);
    }

    ExtractionToml {
        tool: tool.to_owned(),
        tool_version: tool_version.to_owned(),
        method,
        page_count,
        thin_pages,
        original_sha256: original_sha256.to_owned(),
        pages,
        locators,
        method_data: None,
        extra: BTreeMap::new(),
    }
}

impl ExtractionToml {
    /// Parse an `extraction.toml` document into a typed [`ExtractionToml`] struct.
    pub fn parse(text: &str) -> Result<Self, ExtractionParseError> {
        let table: toml::Table = toml::from_str(text).map_err(|err| {
            let offset = err.span().map_or(0, |r| r.start);
            let (line, col) = byte_offset_to_line_col(text, offset);
            ExtractionParseError {
                line,
                col,
                message: err.message().to_owned(),
            }
        })?;

        let method_str = match table.get("method") {
            Some(toml::Value::String(s)) if !s.is_empty() => s.as_str(),
            Some(toml::Value::String(_)) => {
                return Err(ExtractionParseError {
                    line: find_key_line(text, "method"),
                    col: 1,
                    message: "method must not be empty".to_owned(),
                });
            }
            Some(_) => {
                return Err(ExtractionParseError {
                    line: find_key_line(text, "method"),
                    col: 1,
                    message: "method must be a string".to_owned(),
                });
            }
            None => {
                return Err(ExtractionParseError {
                    line: 1,
                    col: 1,
                    message: "missing required key `method`".to_owned(),
                });
            }
        };

        let method = ExtractionMethod::parse(method_str);

        let mut extra = BTreeMap::new();
        let mut method_data = None;

        match &method {
            ExtractionMethod::Known(m) => {
                if table.contains_key("method_data") {
                    return Err(ExtractionParseError {
                        line: find_key_line(text, "method_data"),
                        col: 1,
                        message: format!("method `{}` has no method_data keys", m.name),
                    });
                }
                for key in table.keys() {
                    if !m.keys.contains(&key.as_str()) {
                        return Err(ExtractionParseError {
                            line: find_key_line(text, key),
                            col: 1,
                            message: format!("unknown key `{key}` for method `{}`", m.name),
                        });
                    }
                }
            }
            ExtractionMethod::Unsupported(_) => {
                if let Some(md_val) = table.get("method_data") {
                    let md_tbl = md_val.as_table().ok_or_else(|| ExtractionParseError {
                        line: find_key_line(text, "method_data"),
                        col: 1,
                        message: "method_data must be a table".to_owned(),
                    })?;
                    method_data = Some(md_tbl.clone());
                }
                for (key, val) in &table {
                    if !methods::COMMON_KEYS.contains(&key.as_str()) && key != "method_data" {
                        extra.insert(key.clone(), val.clone());
                    }
                }
            }
        }

        let tool = match table.get("tool") {
            Some(toml::Value::String(s)) if !s.is_empty() => s.clone(),
            Some(toml::Value::String(_)) => {
                return Err(ExtractionParseError {
                    line: find_key_line(text, "tool"),
                    col: 1,
                    message: "tool must not be empty".to_owned(),
                });
            }
            Some(_) => {
                return Err(ExtractionParseError {
                    line: find_key_line(text, "tool"),
                    col: 1,
                    message: "tool must be a string".to_owned(),
                });
            }
            None => {
                return Err(ExtractionParseError {
                    line: 1,
                    col: 1,
                    message: "missing required key `tool`".to_owned(),
                });
            }
        };

        let tool_version = match table.get("tool_version") {
            Some(toml::Value::String(s)) if !s.is_empty() => s.clone(),
            Some(toml::Value::String(_)) => {
                return Err(ExtractionParseError {
                    line: find_key_line(text, "tool_version"),
                    col: 1,
                    message: "tool_version must not be empty".to_owned(),
                });
            }
            Some(_) => {
                return Err(ExtractionParseError {
                    line: find_key_line(text, "tool_version"),
                    col: 1,
                    message: "tool_version must be a string".to_owned(),
                });
            }
            None => {
                return Err(ExtractionParseError {
                    line: 1,
                    col: 1,
                    message: "missing required key `tool_version`".to_owned(),
                });
            }
        };

        let page_count = match table.get("page_count") {
            Some(toml::Value::Integer(n)) => {
                if *n >= 0 && *n <= i64::from(u32::MAX) {
                    u32::try_from(*n).map_err(|_| ExtractionParseError {
                        line: find_key_line(text, "page_count"),
                        col: 1,
                        message: "page_count exceeds u32::MAX".to_owned(),
                    })?
                } else {
                    return Err(ExtractionParseError {
                        line: find_key_line(text, "page_count"),
                        col: 1,
                        message: "page_count must be between 0 and u32::MAX".to_owned(),
                    });
                }
            }
            Some(_) => {
                return Err(ExtractionParseError {
                    line: find_key_line(text, "page_count"),
                    col: 1,
                    message: "page_count must be an integer".to_owned(),
                });
            }
            None => {
                return Err(ExtractionParseError {
                    line: 1,
                    col: 1,
                    message: "missing required key `page_count`".to_owned(),
                });
            }
        };

        let thin_pages_line = find_key_line(text, "thin_pages");
        let thin_pages = match table.get("thin_pages") {
            Some(toml::Value::Array(arr)) => {
                let mut list = Vec::with_capacity(arr.len());
                let mut prev: Option<u32> = None;
                for item in arr {
                    let val = match item {
                        toml::Value::Integer(n) if *n > 0 && *n <= i64::from(u32::MAX) => {
                            u32::try_from(*n).map_err(|_| ExtractionParseError {
                                line: thin_pages_line,
                                col: 1,
                                message: "thin_pages element exceeds u32::MAX".to_owned(),
                            })?
                        }
                        toml::Value::Integer(_) => {
                            return Err(ExtractionParseError {
                                line: thin_pages_line,
                                col: 1,
                                message: "thin_pages elements must be positive integers (>= 1)"
                                    .to_owned(),
                            });
                        }
                        toml::Value::String(s) => {
                            return Err(ExtractionParseError {
                                line: thin_pages_line,
                                col: 1,
                                message: format!(
                                    "thin_pages contains string {s:?}; page numbers must be integers"
                                ),
                            });
                        }
                        _ => {
                            return Err(ExtractionParseError {
                                line: thin_pages_line,
                                col: 1,
                                message: "thin_pages elements must be integers".to_owned(),
                            });
                        }
                    };
                    if let Some(p) = prev
                        && val <= p
                    {
                        return Err(ExtractionParseError {
                            line: thin_pages_line,
                            col: 1,
                            message: format!(
                                "thin_pages must be strictly ascending, found {val} after {p}"
                            ),
                        });
                    }
                    prev = Some(val);
                    list.push(val);
                }
                list
            }
            Some(_) => {
                return Err(ExtractionParseError {
                    line: thin_pages_line,
                    col: 1,
                    message: "thin_pages must be an array of positive integers".to_owned(),
                });
            }
            None => {
                return Err(ExtractionParseError {
                    line: 1,
                    col: 1,
                    message: "missing required key `thin_pages`".to_owned(),
                });
            }
        };

        let sha_line = find_key_line(text, "original_sha256");
        let original_sha256 = match table.get("original_sha256") {
            Some(toml::Value::String(s)) => {
                if is_valid_sha256_hex(s) {
                    s.clone()
                } else {
                    return Err(ExtractionParseError {
                        line: sha_line,
                        col: 1,
                        message: "original_sha256 must be exactly 64 lowercase hex characters"
                            .to_owned(),
                    });
                }
            }
            Some(_) => {
                return Err(ExtractionParseError {
                    line: sha_line,
                    col: 1,
                    message: "original_sha256 must be a string".to_owned(),
                });
            }
            None => {
                return Err(ExtractionParseError {
                    line: 1,
                    col: 1,
                    message: "missing required key `original_sha256`".to_owned(),
                });
            }
        };

        let pages_line = find_key_line(text, "pages");
        let pages_tbl = match table.get("pages") {
            Some(toml::Value::Table(tbl)) => tbl,
            Some(_) => {
                return Err(ExtractionParseError {
                    line: pages_line,
                    col: 1,
                    message: "pages must be a table".to_owned(),
                });
            }
            None => {
                return Err(ExtractionParseError {
                    line: 1,
                    col: 1,
                    message: "missing required table [pages]".to_owned(),
                });
            }
        };

        let mut pages = BTreeMap::new();
        for (stem, val) in pages_tbl {
            let stem_line = find_key_line(text, stem);
            if !is_valid_page_stem(stem) {
                return Err(ExtractionParseError {
                    line: stem_line,
                    col: 1,
                    message: format!(
                        "invalid page stem `{stem}`: must be 'p' followed by at least 4 ASCII digits"
                    ),
                });
            }
            match val {
                toml::Value::String(s) if is_valid_sha256_hex(s) => {
                    pages.insert(stem.clone(), s.clone());
                }
                _ => {
                    return Err(ExtractionParseError {
                        line: stem_line,
                        col: 1,
                        message: format!(
                            "page `{stem}` hash must be exactly 64 lowercase hex characters"
                        ),
                    });
                }
            }
        }

        let mut locators = BTreeMap::new();
        if let Some(loc_val) = table.get("locators") {
            let loc_tbl = loc_val.as_table().ok_or_else(|| ExtractionParseError {
                line: find_key_line(text, "locators"),
                col: 1,
                message: "locators must be a table".to_owned(),
            })?;

            for (label, val) in loc_tbl {
                let label_line = find_key_line(text, label);
                if label.is_empty() {
                    return Err(ExtractionParseError {
                        line: label_line,
                        col: 1,
                        message: "locator label must not be empty".to_owned(),
                    });
                }
                let pages = match val {
                    toml::Value::Integer(n) if *n >= 1 && *n <= i64::from(u32::MAX) => {
                        let page = u32::try_from(*n).map_err(|_| ExtractionParseError {
                            line: label_line,
                            col: 1,
                            message: "locator page exceeds u32::MAX".to_owned(),
                        })?;
                        LocatorPages {
                            first: page,
                            last: page,
                        }
                    }
                    toml::Value::Array(arr) if arr.len() == 2 => {
                        let first_int = arr.first().and_then(toml::Value::as_integer);
                        let last_int = arr.get(1).and_then(toml::Value::as_integer);
                        match (first_int, last_int) {
                            (Some(f), Some(l)) if f >= 1 && f <= l && l <= i64::from(u32::MAX) => {
                                let first = u32::try_from(f).map_err(|_| ExtractionParseError {
                                    line: label_line,
                                    col: 1,
                                    message: "locator first page exceeds u32::MAX".to_owned(),
                                })?;
                                let last = u32::try_from(l).map_err(|_| ExtractionParseError {
                                    line: label_line,
                                    col: 1,
                                    message: "locator last page exceeds u32::MAX".to_owned(),
                                })?;
                                LocatorPages { first, last }
                            }
                            _ => {
                                return Err(ExtractionParseError {
                                    line: label_line,
                                    col: 1,
                                    message: format!(
                                        "locator `{label}` array must be [first, last] with 1 <= first <= last"
                                    ),
                                });
                            }
                        }
                    }
                    _ => {
                        return Err(ExtractionParseError {
                            line: label_line,
                            col: 1,
                            message: format!(
                                "locator `{label}` must be an integer >= 1 or [first, last] array"
                            ),
                        });
                    }
                };
                locators.insert(label.clone(), pages);
            }
        }

        Ok(Self {
            tool,
            tool_version,
            method,
            page_count,
            thin_pages,
            original_sha256,
            pages,
            locators,
            method_data,
            extra,
        })
    }

    /// Whether the `[locators]` table obeys the `FB0212` locator rules that
    /// `check::extraction` enforces: every locator names pages within
    /// `1..=page_count`, and no two labels share a section anchor.
    #[must_use]
    pub fn locators_are_valid(&self) -> bool {
        let in_bounds = self
            .locators
            .values()
            .all(|loc| loc.first != 0 && loc.last <= self.page_count);
        let mut anchors = std::collections::BTreeSet::new();
        in_bounds
            && self
                .locators
                .keys()
                .all(|label| anchors.insert(section_anchor(label)))
    }

    /// Render this record as canonical `extraction.toml` text.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "tool = {}\n",
            toml::Value::String(self.tool.clone())
        ));
        out.push_str(&format!(
            "tool_version = {}\n",
            toml::Value::String(self.tool_version.clone())
        ));
        out.push_str(&format!(
            "method = {}\n",
            toml::Value::String(self.method.name().to_owned())
        ));
        out.push_str(&format!("page_count = {}\n", self.page_count));
        let thin_str = self
            .thin_pages
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("thin_pages = [{thin_str}]\n"));
        out.push_str(&format!(
            "original_sha256 = {}\n",
            toml::Value::String(self.original_sha256.clone())
        ));

        for (k, v) in &self.extra {
            let key_str = format_toml_key(k);
            out.push_str(&format!("{key_str} = {v}\n"));
        }

        out.push_str("[pages]\n");
        for (stem, hash) in &self.pages {
            out.push_str(&format!("{stem} = {}\n", toml::Value::String(hash.clone())));
        }

        if !self.locators.is_empty() {
            out.push_str("[locators]\n");
            for (label, loc) in &self.locators {
                let label_str = toml::Value::String(label.clone()).to_string();
                if loc.first == loc.last {
                    out.push_str(&format!("{label_str} = {}\n", loc.first));
                } else {
                    out.push_str(&format!("{label_str} = [{}, {}]\n", loc.first, loc.last));
                }
            }
        }

        if let Some(t) = &self.method_data {
            let mut wrap = toml::Table::new();
            wrap.insert("method_data".to_string(), toml::Value::Table(t.clone()));
            if let Ok(rendered) = toml::to_string(&wrap) {
                out.push_str(&rendered);
            }
        }

        while out.ends_with("\n\n") {
            out.pop();
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }

        out
    }
}

fn format_toml_key(k: &str) -> String {
    let is_bare = !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if is_bare {
        k.to_owned()
    } else {
        toml::Value::String(k.to_owned()).to_string()
    }
}

fn is_valid_page_stem(stem: &str) -> bool {
    if !stem.starts_with('p') || stem.len() < 5 {
        return false;
    }
    stem.get(1..)
        .is_some_and(|rest| rest.bytes().all(|b| b.is_ascii_digit()))
}

fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn byte_offset_to_line_col(text: &str, byte_offset: usize) -> (usize, usize) {
    let clamped = byte_offset.min(text.len());
    let mut end = clamped;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = text.get(..end).unwrap_or("");
    let mut line = 1usize;
    let mut line_start_byte = 0usize;
    for (idx, b) in prefix.bytes().enumerate() {
        if b == b'\n' {
            line = line.saturating_add(1);
            line_start_byte = idx + 1;
        }
    }
    let col = prefix
        .get(line_start_byte..)
        .map_or(1, |s| s.chars().count() + 1);
    (line.max(1), col.max(1))
}

fn find_key_line(text: &str, key: &str) -> usize {
    let quoted_double = format!("\"{key}\"");
    let quoted_single = format!("'{key}'");
    let table_header = format!("[{key}]");

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if trimmed == table_header {
            return idx + 1;
        }
        if let Some(rest) = trimmed.strip_prefix('[')
            && let Some(inner) = rest.strip_suffix(']')
        {
            let inner_trimmed = inner.trim();
            if inner_trimmed == key
                || inner_trimmed == quoted_double
                || inner_trimmed == quoted_single
            {
                return idx + 1;
            }
        }
        for prefix in [key, quoted_double.as_str(), quoted_single.as_str()] {
            if let Some(rest) = trimmed.strip_prefix(prefix)
                && rest.trim_start().starts_with('=')
            {
                return idx + 1;
            }
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_anchor_follows_the_rule() {
        assert_eq!(section_anchor("(e)(1)"), "sec-e-1");
        assert_eq!(section_anchor("15.404-1"), "sec-15-404-1");
        assert_eq!(section_anchor("3"), "sec-3");
        assert_eq!(section_anchor("A.B"), "sec-a-b");
        assert_eq!(section_anchor("()"), "sec-");
        assert_eq!(section_anchor("§ 4.2"), "sec-4-2");
        assert_eq!(section_anchor("Café 1"), "sec-caf-1");
        assert_eq!(section_anchor("§§§"), "sec-");
    }

    #[test]
    fn recognise_locators_finds_section_headings() {
        let pages = vec![
            PageText {
                number: 1,
                text: "section 3: Synthetic rate review\n§ 4.2\nThe section 3 text\n".to_owned(),
            },
            PageText {
                number: 2,
                text: "Sec. 5:\nsection 3: Duplicate on page 2\n".to_owned(),
            },
        ];
        let locators = recognise_locators(&pages);
        assert_eq!(locators.get("3"), Some(&LocatorPages { first: 1, last: 1 }));
        assert_eq!(
            locators.get("4.2"),
            Some(&LocatorPages { first: 1, last: 1 })
        );
        assert_eq!(locators.get("5"), Some(&LocatorPages { first: 2, last: 2 }));
        assert!(!locators.contains_key("The"));
        assert_eq!(locators.len(), 3);
    }

    #[test]
    fn recognise_locators_ignores_prose_after_a_section_word() {
        for prefix in ["section ", "§ ", "Sec. ", "sec "] {
            for word in [
                "is", "using", "would", "do", "applies", "a", "Agencies", "Affected",
            ] {
                let pages = vec![PageText {
                    number: 1,
                    text: format!("{prefix}{word} the rest of the sentence\n"),
                }];
                assert!(
                    recognise_locators(&pages).is_empty(),
                    "{prefix}{word} must propose no locator"
                );
            }
        }
    }

    #[test]
    fn recognise_locators_keeps_real_labels() {
        let pages = vec![
            PageText {
                number: 2,
                text: "§ 4.2 Review\nsection IV\nSec. A\n".to_owned(),
            },
            PageText {
                number: 3,
                text: "section (a)(i)\nsection 15.404-1\n".to_owned(),
            },
        ];
        let locators = recognise_locators(&pages);
        for (label, page) in [
            ("4.2", 2),
            ("IV", 2),
            ("A", 2),
            ("(a)(i)", 3),
            ("15.404-1", 3),
        ] {
            assert_eq!(
                locators.get(label),
                Some(&LocatorPages {
                    first: page,
                    last: page
                }),
                "{label}"
            );
        }
        assert_eq!(locators.len(), 5);
    }

    #[test]
    fn a_label_must_look_like_a_section_number() {
        for good in [
            "3",
            "4.2",
            "15.404-1",
            "(a)",
            "(ii)",
            "(iii)",
            "(viii)",
            "(xii)",
            "(xxiv)",
            "(A)",
            "(III)",
            "(VIII)",
            "(e)(1)",
            "(a)(viii)",
            "IV",
            "XII",
            "A",
            "B2",
        ] {
            assert!(looks_like_section_label(good), "{good} is a section label");
        }
        for bad in [
            "is",
            "using",
            "would",
            "do",
            "applies",
            "a",
            "Agencies",
            "Affected",
            "IVX",
            "(Agencies)",
            "(civil)",
            "(vivid)",
            "(did)",
            "(mix)",
            "(Mix)",
            "(iiii)",
            "( )",
            "Section",
        ] {
            assert!(!looks_like_section_label(bad), "{bad} is prose");
        }
    }

    #[test]
    fn recognise_locators_keeps_longer_roman_sub_items_and_rejects_parenthesised_words() {
        let pages = vec![PageText {
            number: 4,
            text: concat!(
                "section (iii) Synthetic sub-item\n",
                "section (a)(viii) Nested sub-item\n",
                "section (xxiv): Twenty-fourth item\n",
                "section (VIII) Upper-case Roman sub-item\n",
                "section (Agencies) Affected\n",
                "section (civil) penalty\n",
                "section (vivid) example\n",
                "section (did) apply\n",
                "section (mix) of rates\n",
                "section (Mix) of rates\n",
            )
            .to_owned(),
        }];
        let locators = recognise_locators(&pages);
        for label in ["(iii)", "(a)(viii)", "(xxiv)", "(VIII)"] {
            assert_eq!(
                locators.get(label),
                Some(&LocatorPages { first: 4, last: 4 }),
                "{label} must be recognised"
            );
        }
        for rejected in [
            "(Agencies)",
            "(civil)",
            "(vivid)",
            "(did)",
            "(mix)",
            "(Mix)",
        ] {
            assert!(
                !locators.contains_key(rejected),
                "{rejected} must be rejected"
            );
        }
        assert_eq!(locators.len(), 4);
    }

    #[test]
    fn page_limit_refusal_holds_the_four_digit_cap() {
        assert_eq!(MAX_PAGES, 9_999);
        assert_eq!(page_limit_refusal("S-0001-v1", 0), None);
        assert_eq!(page_limit_refusal("S-0001-v1", 1), None);
        assert_eq!(page_limit_refusal("S-0001-v1", MAX_PAGES), None);
        let refusal = page_limit_refusal("S-0001-v1", MAX_PAGES + 1).expect("refused");
        assert_eq!(
            refusal,
            "S-0001-v1: 10000 pages, over the 9999-page limit; nothing written"
        );
    }

    #[test]
    fn thin_pages_are_under_forty_characters() {
        let pages = vec![
            PageText {
                number: 1,
                text: "a".repeat(39),
            },
            PageText {
                number: 2,
                text: "a".repeat(40),
            },
            PageText {
                number: 3,
                text: "   \t\n   ".to_owned(),
            },
            PageText {
                number: 4,
                text: format!("   {}   ", "b".repeat(39)),
            },
            PageText {
                number: 5,
                text: format!("   {}   ", "b".repeat(40)),
            },
        ];
        let thin = thin_page_numbers(&pages);
        assert_eq!(thin, vec![1, 3, 4]);
    }

    #[test]
    fn plain_copies_text_unchanged() {
        let original = "Hello synthetic world\nLine 2";
        let ext = Plain
            .extract(original.as_bytes(), "txt")
            .expect("plain extraction");
        assert_eq!(ext.method, "plain");
        assert_eq!(ext.pages.len(), 1);
        assert_eq!(ext.pages[0].number, 1);
        assert_eq!(ext.pages[0].text, original);
    }

    #[test]
    fn plain_refuses_invalid_utf8() {
        let bad = [0xff, 0xfe, 0xfd];
        let err = Plain.extract(&bad, "txt").expect_err("should refuse");
        assert_eq!(err, ExtractError::NotText);
    }

    #[test]
    fn plain_empty_input_is_one_thin_page() {
        let ext = Plain.extract(b"", "txt").expect("empty is valid");
        assert_eq!(ext.method, "plain");
        assert_eq!(ext.pages.len(), 1);
        assert_eq!(ext.pages[0].number, 1);
        assert_eq!(ext.pages[0].text, "");
        assert_eq!(thin_page_numbers(&ext.pages), vec![1]);
    }

    #[test]
    fn size_limit_is_checked_first() {
        let oversize_len = usize::try_from(MAX_ORIGINAL_BYTES + 1).expect("fits usize");
        let oversize_bad_utf8 = vec![0xff; oversize_len];
        let err = Plain
            .extract(&oversize_bad_utf8, "txt")
            .expect_err("must be too large");
        assert_eq!(
            err,
            ExtractError::TooLarge {
                bytes: MAX_ORIGINAL_BYTES + 1
            }
        );

        let exact_len = usize::try_from(MAX_ORIGINAL_BYTES).expect("fits usize");
        let exact_ascii = vec![b'a'; exact_len];
        let ext = Plain
            .extract(&exact_ascii, "txt")
            .expect("exact size should succeed");
        assert_eq!(ext.pages.len(), 1);
        assert_eq!(ext.pages[0].text.len(), exact_len);
    }

    #[test]
    fn html_with_no_text_is_one_empty_page() {
        let html = b"<html><head><style>x</style></head><body><!-- comment --></body></html>";
        let ext = HtmlStrip.extract(html, "html").expect("html extraction");
        assert_eq!(ext.method, "html-strip");
        assert_eq!(ext.pages.len(), 1);
        assert_eq!(ext.pages[0].text, "");
    }

    #[test]
    fn html_never_needs_a_closing_delimiter() {
        let cases = [
            "<p>a<",
            "<p>a<b",
            "<!-- x",
            "<script>x",
            "<p title=\"x",
            "a &amp",
            "&#99999999999;",
        ];
        for input in cases {
            let res = HtmlStrip.extract(input.as_bytes(), "html");
            assert!(res.is_ok(), "failed on input: {input:?}");
        }
    }

    #[test]
    fn html_entities_and_blocks() {
        let html = "<p>Synthetic &amp; &lt; &gt; &quot; &apos; foo&nbsp;bar</p>\
                    <p>Decimal: &#169; Hex: &#x20AC; Invalid: &#99999999999;</p>";
        let ext = HtmlStrip.extract(html.as_bytes(), "html").expect("extract");
        assert_eq!(
            ext.pages[0].text,
            "Synthetic & < > \" ' foo bar\n\nDecimal: © Hex: € Invalid: &#99999999999;\n"
        );
    }

    #[test]
    fn html_strip_keeps_text_after_a_self_closing_script() {
        let doc = "<html><head><script src=\"a.js\"/></head><body>\
                   <p>This synthetic paragraph contains well over forty characters for testing extraction.</p>\
                   </body></html>";
        let ext = HtmlStrip
            .extract(doc.as_bytes(), "xhtml")
            .expect("extract xhtml");
        let para =
            "This synthetic paragraph contains well over forty characters for testing extraction.";
        assert!(ext.pages[0].text.contains(para));
        assert!(ext.pages[0].text.trim().chars().count() >= THIN_PAGE_CHARS);
        assert!(thin_page_numbers(&ext.pages).is_empty());
    }

    #[test]
    fn html_strip_a_self_closing_script_does_not_swallow_a_later_inline_script() {
        let doc = "<html><head><script src=\"a.js\"/></head><body>\
                   <p>First synthetic paragraph.</p>\
                   <p>Second synthetic paragraph.</p>\
                   <script>var x = 1;</script>\
                   <p>Third synthetic paragraph.</p>\
                   </body></html>";
        let ext = HtmlStrip
            .extract(doc.as_bytes(), "html")
            .expect("extract html");
        assert!(ext.pages[0].text.contains("First synthetic paragraph."));
        assert!(ext.pages[0].text.contains("Third synthetic paragraph."));
        assert!(!ext.pages[0].text.contains("var x"));
    }

    #[test]
    fn html_strip_keeps_svg_text_after_a_self_closing_style() {
        let doc = "<svg><style/><text>Synthetic SVG text content</text></svg>";
        let ext = HtmlStrip
            .extract(doc.as_bytes(), "svg")
            .expect("extract svg");
        assert!(ext.pages[0].text.contains("Synthetic SVG text content"));
    }

    #[test]
    fn supplied_text_normalizes_and_splits() {
        let raw =
            "\u{FEFF}Page 1 line 1\r\nPage 1 line 2\r[page break]\n[page break]\nPage 3 line 1\n";
        let ext = supplied_text(raw.as_bytes()).expect("supplied");
        assert_eq!(ext.method, "supplied");
        assert_eq!(ext.pages.len(), 3);
        assert_eq!(ext.pages[0].number, 1);
        assert_eq!(ext.pages[0].text, "Page 1 line 1\nPage 1 line 2\n");
        assert_eq!(ext.pages[1].number, 2);
        assert_eq!(ext.pages[1].text, "");
        assert_eq!(ext.pages[2].number, 3);
        assert_eq!(ext.pages[2].text, "Page 3 line 1\n");

        // [page break] alone is two empty pages
        let two_empty = supplied_text(b"[page break]").expect("alone");
        assert_eq!(two_empty.pages.len(), 2);
        assert_eq!(two_empty.pages[0].text, "");
        assert_eq!(two_empty.pages[1].text, "");

        // "  [page break]" with spaces is NOT a separator
        let not_sep = supplied_text(b"  [page break]\n").expect("not sep");
        assert_eq!(not_sep.pages.len(), 1);
        assert_eq!(not_sep.pages[0].text, "  [page break]\n");
    }

    #[test]
    fn supplied_text_refuses_non_utf8_and_oversize() {
        let bad = [0xff, 0xfe];
        assert_eq!(supplied_text(&bad), Err(ExtractError::NotText));

        let oversize_len = usize::try_from(MAX_ORIGINAL_BYTES + 1).expect("fits usize");
        let oversize = vec![b'x'; oversize_len];
        assert_eq!(
            supplied_text(&oversize),
            Err(ExtractError::TooLarge {
                bytes: MAX_ORIGINAL_BYTES + 1
            })
        );
    }

    #[test]
    fn select_matches_select_types() {
        for &ext in SELECT_TYPES {
            assert!(
                select(ext, &[]).is_some(),
                "select should return Some for {ext}"
            );
        }
        for &ext in &["pdf", "csv", "docx", "", "exe"] {
            assert!(
                select(ext, &[]).is_none(),
                "select should return None for {ext}"
            );
        }
    }

    #[test]
    fn html_strip_matches_the_fixture() {
        let sample_html = include_str!("../../../tests/fixtures/extract/html-strip/sample.html");
        let sample_expected =
            include_str!("../../../tests/fixtures/extract/html-strip/sample.expected.txt");
        let ext = HtmlStrip
            .extract(sample_html.as_bytes(), "html")
            .expect("extract sample.html");
        assert_eq!(ext.pages[0].text, sample_expected);
    }

    #[test]
    fn extraction_toml_round_trips() {
        let expected = "\
tool = \"pdftotext\"
tool_version = \"26.10.0\"
method = \"pdftotext-layout\"
page_count = 3
thin_pages = [3]
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
p0002 = \"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789\"
[locators]
\"(e)(1)\" = 2
\"15.404-1\" = [2, 3]
";
        // Provide input with locators and pages out of order to verify canonical sorting
        let input = "\
tool = \"pdftotext\"
tool_version = \"26.10.0\"
method = \"pdftotext-layout\"
page_count = 3
thin_pages = [3]
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[pages]
p0002 = \"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789\"
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[locators]
\"15.404-1\" = [2, 3]
\"(e)(1)\" = 2
";
        let parsed = ExtractionToml::parse(input).expect("parse input");
        let rendered = parsed.render();
        assert_eq!(rendered, expected);
        let reparsed = ExtractionToml::parse(&rendered).expect("reparse rendered");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn thin_pages_is_always_written() {
        let input = "\
tool = \"pdftotext\"
tool_version = \"26.10.0\"
method = \"pdftotext-layout\"
page_count = 1
thin_pages = []
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
";
        let parsed = ExtractionToml::parse(input).expect("parse record with empty thin_pages");
        assert!(parsed.thin_pages.is_empty());
        let rendered = parsed.render();
        assert!(rendered.contains("thin_pages = []\n"));
    }

    #[test]
    fn thin_pages_uses_page_numbers() {
        let template = |thin_part: &str| {
            format!(
                "tool = \"plain\"\ntool_version = \"1\"\nmethod = \"plain\"\npage_count = 3\nthin_pages = {thin_part}\noriginal_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n[pages]\np0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n"
            )
        };

        // Accepts [] and [1, 3]
        let ok_empty = ExtractionToml::parse(&template("[]")).expect("accept []");
        assert_eq!(ok_empty.thin_pages, Vec::<u32>::new());

        let ok_asc = ExtractionToml::parse(&template("[1, 3]")).expect("accept [1, 3]");
        assert_eq!(ok_asc.thin_pages, vec![1, 3]);

        // Refuses ["p0001"], [0], [3, 1], [2, 2], [1.5] with a typed error whose line is the thin_pages line (line 5)
        for bad in &["[\"p0001\"]", "[0]", "[3, 1]", "[2, 2]", "[1.5]"] {
            let err = ExtractionToml::parse(&template(bad)).expect_err("should refuse");
            assert_eq!(err.line, 5, "expected line 5 for bad thin_pages: {bad}");
        }
    }

    #[test]
    fn thin_page_stems_are_refused() {
        let input = "\
tool = \"plain\"
tool_version = \"1\"
method = \"plain\"
page_count = 1
thin_pages = [\"p0001\"]
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
";
        let err = ExtractionToml::parse(input).expect_err("refuse stem string");
        assert_eq!(err.line, 5);
        assert!(
            err.message.contains("thin_pages"),
            "error message should mention key 'thin_pages', got: {}",
            err.message
        );
    }

    #[test]
    fn parse_reports_positions() {
        // A syntax error on line 4 reports line 4 and col >= 1
        let syntax_err_toml = "\
tool = \"pdftotext\"
tool_version = \"1.0\"
method = \"plain\"
bad syntax here !!!
page_count = 1
";
        let err = ExtractionToml::parse(syntax_err_toml).expect_err("syntax error");
        assert_eq!(err.line, 4);
        assert!(err.col >= 1);

        // A missing key reports line 1 col 1
        let missing_key_toml = "\
tool_version = \"1.0\"
method = \"plain\"
page_count = 1
thin_pages = []
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
";
        let err = ExtractionToml::parse(missing_key_toml).expect_err("missing tool");
        assert_eq!(err.line, 1);
        assert_eq!(err.col, 1);

        // A bad sha reports the original_sha256 line
        let bad_sha_toml = "\
tool = \"plain\"
tool_version = \"1.0\"
method = \"plain\"
page_count = 1
thin_pages = []
original_sha256 = \"not_a_valid_sha\"
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
";
        let err = ExtractionToml::parse(bad_sha_toml).expect_err("bad sha");
        assert_eq!(err.line, 6);
        assert_eq!(err.col, 1);

        // An unknown key for a known method is reported at its line
        let unknown_key_toml = "\
tool = \"plain\"
tool_version = \"1.0\"
method = \"plain\"
page_count = 1
unknown_setting = 123
thin_pages = []
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
";
        let err = ExtractionToml::parse(unknown_key_toml).expect_err("unknown key");
        assert_eq!(err.line, 5);
        assert_eq!(err.col, 1);

        // A bad stem in [pages] is reported
        let bad_stem_toml = "\
tool = \"plain\"
tool_version = \"1.0\"
method = \"plain\"
page_count = 1
thin_pages = []
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[pages]
bad_stem = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
";
        let err = ExtractionToml::parse(bad_stem_toml).expect_err("bad stem");
        assert_eq!(err.line, 8);
        assert_eq!(err.col, 1);
    }

    #[test]
    fn known_methods_reject_unknown_keys_and_method_data() {
        let toml_unknown_key = "\
tool = \"plain\"
tool_version = \"1.0\"
method = \"plain\"
page_count = 1
thin_pages = []
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
extra_key = 42
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
";
        let err_key = ExtractionToml::parse(toml_unknown_key).expect_err("reject unknown key");
        assert_eq!(err_key.line, 7);
        assert_eq!(err_key.col, 1);
        assert!(
            err_key
                .message
                .contains("unknown key `extra_key` for method `plain`")
        );

        let toml_method_data = "\
tool = \"plain\"
tool_version = \"1.0\"
method = \"plain\"
page_count = 1
thin_pages = []
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[method_data]
foo = \"bar\"
";
        let err_md = ExtractionToml::parse(toml_method_data).expect_err("reject method_data");
        assert_eq!(err_md.line, 9);
        assert_eq!(err_md.col, 1);
        assert!(
            err_md
                .message
                .contains("method `plain` has no method_data keys")
        );
    }

    #[test]
    fn unsupported_methods_keep_their_metadata() {
        let input = "\
tool = \"future-tool\"
tool_version = \"2.0\"
method = \"future-method\"
page_count = 1
thin_pages = []
original_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
unit = \"window\"
window_rows = 50
[pages]
p0001 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"
[method_data]
nested_field = \"nested_val\"
";
        let rec = ExtractionToml::parse(input).expect("parse future method ok");
        assert!(!rec.method.is_supported());
        assert_eq!(
            rec.extra.get("unit"),
            Some(&toml::Value::String("window".to_string()))
        );
        assert_eq!(
            rec.extra.get("window_rows"),
            Some(&toml::Value::Integer(50))
        );
        assert!(rec.method_data.is_some());

        let rendered = rec.render();
        let reparsed = ExtractionToml::parse(&rendered).expect("reparse future method ok");
        assert_eq!(rec, reparsed);

        // weak_evidence with no thin pages is None BUT test also asserts ExtractionMethod::is_supported() is false
        let weak = weak_evidence("public/S-0001-v1.txt", Some(&rec));
        assert_eq!(weak, None);
        assert!(!rec.method.is_supported());
    }

    #[test]
    fn appended_method_and_locator_keep_old_rows() {
        let synthetic = Method {
            name: "synthetic-method",
            types: &["syn"],
            locator: "synthetic_loc",
            keys: &["synthetic_key"],
            extractor: None,
            fixture: "tests/fixtures/synthetic",
        };

        // synthetic-method is not in METHODS
        assert!(!METHODS.iter().any(|m| m.name == synthetic.name));

        // A record naming it is Unsupported
        let parsed_m = ExtractionMethod::parse(synthetic.name);
        assert_eq!(
            parsed_m,
            ExtractionMethod::Unsupported("synthetic-method".to_string())
        );

        // METHODS[0..4] names are still pdftotext-layout, html-strip, plain, supplied in that order
        assert!(METHODS.len() >= 4);
        assert_eq!(METHODS[0].name, "pdftotext-layout");
        assert_eq!(METHODS[1].name, "html-strip");
        assert_eq!(METHODS[2].name, "plain");
        assert_eq!(METHODS[3].name, "supplied");
    }

    #[test]
    fn build_record_hashes_pages_and_keeps_locators() {
        let pages = vec![
            PageText {
                number: 1,
                text: "Section 3: Text on page 1\nSection 4: Another heading\nThis page is sufficiently long so it is not thin at all.\n".to_owned(),
            },
            PageText {
                number: 2,
                text: "Section 3: Duplicate on page 2\n".to_owned(),
            },
        ];
        let extraction = Extraction {
            method: methods::PLAIN,
            pages,
        };

        let mut kept = BTreeMap::new();
        // Hand label for "3" overrides recognised on page 1, pointing to page 2
        kept.insert("3".to_string(), LocatorPages { first: 2, last: 2 });
        // Other hand label kept
        kept.insert("hand-label".to_string(), LocatorPages { first: 1, last: 2 });

        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let record = build_record(&extraction, "test-tool", "1.0", sha, &kept);

        assert_eq!(record.tool, "test-tool");
        assert_eq!(record.tool_version, "1.0");
        assert_eq!(record.method.name(), "plain");
        assert_eq!(record.page_count, 2);
        assert_eq!(record.original_sha256, sha);

        // Pages hashed with crate::hash::sha256_hex
        assert_eq!(
            record.pages.get("p0001"),
            Some(&crate::hash::sha256_hex(
                extraction.pages[0].text.as_bytes()
            ))
        );
        assert_eq!(
            record.pages.get("p0002"),
            Some(&crate::hash::sha256_hex(
                extraction.pages[1].text.as_bytes()
            ))
        );

        // Thin pages computed
        assert_eq!(record.thin_pages, vec![2]);

        // Recognised locators added
        assert_eq!(
            record.locators.get("4"),
            Some(&LocatorPages { first: 1, last: 1 })
        );

        // Hand label passed in kept_locators is kept even when text announces same label on another page
        assert_eq!(
            record.locators.get("3"),
            Some(&LocatorPages { first: 2, last: 2 })
        );

        // Other hand labels kept
        assert_eq!(
            record.locators.get("hand-label"),
            Some(&LocatorPages { first: 1, last: 2 })
        );
    }

    #[test]
    fn build_record_is_deterministic() {
        let pages = vec![
            PageText {
                number: 1,
                text: "Section 1: Deterministic check\nPlenty of text to make it not thin.\n"
                    .to_owned(),
            },
            PageText {
                number: 2,
                text: "Short\n".to_owned(),
            },
        ];
        let extraction = Extraction {
            method: methods::PLAIN,
            pages,
        };
        let mut kept = BTreeMap::new();
        kept.insert("custom".to_string(), LocatorPages { first: 1, last: 2 });
        let sha = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

        let r1 = build_record(&extraction, "tool-det", "0.1", sha, &kept);
        let r2 = build_record(&extraction, "tool-det", "0.1", sha, &kept);

        assert_eq!(r1, r2);
        assert_eq!(r1.render(), r2.render());
    }

    #[test]
    fn weak_evidence_reasons() {
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let pages_map = BTreeMap::from([("p0001".to_string(), sha.to_string())]);

        let supplied_rec = ExtractionToml {
            tool: "supplied".to_string(),
            tool_version: "1".to_string(),
            method: ExtractionMethod::parse("supplied"),
            page_count: 1,
            thin_pages: vec![1],
            original_sha256: sha.to_string(),
            pages: pages_map.clone(),
            locators: BTreeMap::new(),
            method_data: None,
            extra: BTreeMap::new(),
        };

        // 1. metadata-only location wins even over supplied method and thin pages
        let weak_meta = weak_evidence("metadata-only: paper source", Some(&supplied_rec));
        assert_eq!(weak_meta, Some(WeakEvidence::MetadataOnly));
        assert_eq!(
            WeakEvidence::MetadataOnly.to_string(),
            "the version is metadata-only"
        );

        // 2. supplied method
        let weak_sup = weak_evidence("public/S-0001-v1.txt", Some(&supplied_rec));
        assert_eq!(weak_sup, Some(WeakEvidence::Supplied));
        assert_eq!(
            WeakEvidence::Supplied.to_string(),
            "the text was supplied by hand and is not checked against the original"
        );

        // 3. thin pages listed
        let thin_rec = ExtractionToml {
            tool: "plain".to_string(),
            tool_version: "1.0".to_string(),
            method: ExtractionMethod::parse("plain"),
            page_count: 2,
            thin_pages: vec![2],
            original_sha256: sha.to_string(),
            pages: pages_map,
            locators: BTreeMap::new(),
            method_data: None,
            extra: BTreeMap::new(),
        };
        let weak_thin = weak_evidence("public/S-0001-v1.txt", Some(&thin_rec));
        assert_eq!(weak_thin, Some(WeakEvidence::ThinPages(vec![2])));
        assert_eq!(
            WeakEvidence::ThinPages(vec![2]).to_string(),
            "pages 2 have little or no text"
        );
        assert_eq!(
            WeakEvidence::ThinPages(vec![2, 5]).to_string(),
            "pages 2, 5 have little or no text"
        );

        // 4. ordinary version without a record is None
        assert_eq!(weak_evidence("public/S-0001-v1.txt", None), None);

        // 5. a metadata-only location is reported even with no record
        assert_eq!(
            weak_evidence("metadata-only: missing file", None),
            Some(WeakEvidence::MetadataOnly)
        );
    }
}
