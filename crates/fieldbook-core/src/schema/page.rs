//! Splitting and parsing Corpus page files (`+++` TOML front matter and Markdown body).
//!
//! **Responsibility.** Split a raw page into its TOML front matter and Markdown
//! body (`split`), parse the front matter into [`PageMeta`] with `deny_unknown_fields`
//! (`parse_meta`), classify every parse failure into [`MetaErrorKind`] with 1-based
//! page line and code-point column, and retain parser-recorded key positions
//! (`PageMeta::field_location`).

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use unicode_normalization::{UnicodeNormalization, is_nfc};

use crate::date::{CivilDate, Volatility};
use crate::ids::{AreaCode, ClaimId, ConflictId, PageId, QuestionId, TermId};
use crate::schema::corpus_toml::{byte_offset_to_line_col, is_slug};
use crate::schema::vocab::{PageStatus, PageType, Readiness};

/// Result of splitting a raw page file at its `+++` fences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// Raw TOML text between the opening and closing `+++` fences.
    pub front_matter: String,
    /// Markdown body following the blank line after the closing `+++` fence.
    pub body: String,
    /// 1-based line number in the page file where `body` starts.
    pub body_line: u32,
}

impl Split {
    /// Returns the raw TOML front-matter text.
    #[must_use]
    pub fn front_matter(&self) -> &str {
        &self.front_matter
    }

    /// Returns the Markdown body text.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns the 1-based line number where the body starts in the page file.
    #[must_use]
    pub const fn body_line(&self) -> u32 {
        self.body_line
    }
}

/// Kind of fence failure when splitting a page file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitErrorKind {
    /// The first line is not `+++`.
    MissingOpeningFence,
    /// No closing `+++` line was found.
    MissingClosingFence,
    /// The line immediately after the closing `+++` fence is missing or not blank.
    MissingBlankLine,
}

/// Error returned by [`split`] when a page file's fences are missing or malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitError {
    kind: SplitErrorKind,
    line: u32,
    col: u32,
}

impl SplitError {
    /// Returns the kind of fence error.
    #[must_use]
    pub const fn kind(&self) -> SplitErrorKind {
        self.kind
    }

    /// Returns the 1-based line number of the fault (always `>= 1`).
    #[must_use]
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// Returns the 1-based code-point column number of the fault (always `>= 1`).
    #[must_use]
    pub const fn col(&self) -> u32 {
        self.col
    }
}

impl fmt::Display for SplitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self.kind {
            SplitErrorKind::MissingOpeningFence => "missing opening +++ fence on line 1",
            SplitErrorKind::MissingClosingFence => "missing closing +++ fence",
            SplitErrorKind::MissingBlankLine => "missing blank line after closing +++ fence",
        };
        write!(f, "{}:{}: {msg}", self.line, self.col)
    }
}

impl std::error::Error for SplitError {}

/// Splits a page file into its TOML front matter, Markdown body, and 1-based
/// starting line of the body.
///
/// Requires line 1 to be `+++`, a closing line `+++`, and one blank line before
/// the body.
pub fn split(input: &str) -> Result<Split, SplitError> {
    let Some(after_open) = input.strip_prefix("+++\n") else {
        return Err(SplitError {
            kind: SplitErrorKind::MissingOpeningFence,
            line: 1,
            col: 1,
        });
    };

    // Search line by line in `after_open` (which starts on page line 2) for `+++`.
    let mut byte_pos = 0usize;
    let mut line_num = 2u32;
    let mut found_close: Option<(usize, usize, u32)> = None;

    for line in after_open.split('\n') {
        let line_len = line.len();
        if line == "+++" {
            found_close = Some((byte_pos, byte_pos + line_len, line_num));
            break;
        }
        byte_pos += line_len + 1;
        line_num = line_num.saturating_add(1);
    }

    let Some((close_start, close_end, close_line)) = found_close else {
        let total_lines = u32::try_from(input.bytes().filter(|&b| b == b'\n').count() + 1)
            .unwrap_or(u32::MAX)
            .max(1);
        return Err(SplitError {
            kind: SplitErrorKind::MissingClosingFence,
            line: total_lines,
            col: 1,
        });
    };

    let front_matter = if close_start == 0 {
        ""
    } else {
        &after_open[..close_start - 1]
    };

    let after_close = &after_open[close_end..];
    let Some(body) = after_close.strip_prefix("\n\n") else {
        let err_line = if after_close.starts_with('\n') {
            close_line.saturating_add(1)
        } else {
            close_line
        };
        return Err(SplitError {
            kind: SplitErrorKind::MissingBlankLine,
            line: err_line,
            col: 1,
        });
    };

    Ok(Split {
        front_matter: front_matter.to_owned(),
        body: body.to_owned(),
        body_line: close_line.saturating_add(2),
    })
}

/// Parsed TOML front matter of a v1 Corpus page.
///
/// Note-only fields (`origin`, `reviewed`, `question`, `context_fingerprint`) are
/// not part of v1 pages and are refused as unknown fields until `handoff-new-note`.
#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageMeta {
    /// Page identifier (`<area>-<kebab>`).
    #[serde(deserialize_with = "de_page_id")]
    pub id: PageId,
    /// Plain noun phrase of 1 to 120 characters, no markup characters or final period.
    #[serde(deserialize_with = "de_title")]
    pub title: String,
    /// Three-letter area code.
    #[serde(deserialize_with = "de_area_code")]
    pub area: AreaCode,
    /// Subarea folder slug.
    #[serde(deserialize_with = "de_subarea")]
    pub subarea: String,
    /// v1 page type.
    #[serde(rename = "type", deserialize_with = "de_page_type")]
    pub r#type: PageType,
    /// Curriculum stage (`1..=8`).
    #[serde(deserialize_with = "de_stage")]
    pub stage: u8,
    /// Page readiness.
    #[serde(deserialize_with = "de_readiness")]
    pub readiness: Readiness,
    /// Page maturity status (`draft`, `reviewed`, `stable`).
    #[serde(deserialize_with = "de_status")]
    pub status: PageStatus,
    /// Content as-of date (TOML local date).
    #[serde(deserialize_with = "de_as_of", serialize_with = "ser_date")]
    pub as_of: CivilDate,
    /// Handling label strings, kept as written for FB0410.
    #[serde(deserialize_with = "de_handling")]
    pub handling: Vec<String>,
    /// Glossary term ids used on the page.
    #[serde(default, deserialize_with = "de_terms")]
    pub terms: Vec<TermId>,
    /// Prerequisite page ids.
    #[serde(default, deserialize_with = "de_prereq")]
    pub prereq: Vec<PageId>,
    /// Open question ids (`Q-nnn`).
    #[serde(default, deserialize_with = "de_questions")]
    pub questions: Vec<QuestionId>,
    /// Conflict ids (`XC-nn`).
    #[serde(default, deserialize_with = "de_conflicts")]
    pub conflicts: Vec<ConflictId>,
    /// Related page ids owned by other areas.
    #[serde(default, deserialize_with = "de_links")]
    pub links: Vec<PageId>,
    /// Optional explicit volatility class (`V1`, `V2`, `V3`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_volatility"
    )]
    pub volatility: Option<Volatility>,
    /// Page ids this page replaces.
    #[serde(default, deserialize_with = "de_supersedes")]
    pub supersedes: Vec<PageId>,
    /// Page ids that replace this page.
    #[serde(default, deserialize_with = "de_superseded_by")]
    pub superseded_by: Vec<PageId>,
    /// Computed field (`claims`); present in the schema only so writing it can be reported as FB0109.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_claims"
    )]
    pub claims: Option<Vec<ClaimId>>,
    /// Computed field (`review_by`); present in the schema only so writing it can be reported as FB0109.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_review_by",
        serialize_with = "ser_opt_date"
    )]
    pub review_by: Option<CivilDate>,

    /// Parser-recorded 1-based `(line, col)` positions of keys written in the page file.
    #[serde(skip)]
    field_locations: BTreeMap<String, (u32, u32)>,
}

impl PartialEq for PageMeta {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.title == other.title
            && self.area == other.area
            && self.subarea == other.subarea
            && self.r#type == other.r#type
            && self.stage == other.stage
            && self.readiness == other.readiness
            && self.status == other.status
            && self.as_of == other.as_of
            && self.handling == other.handling
            && self.terms == other.terms
            && self.prereq == other.prereq
            && self.questions == other.questions
            && self.conflicts == other.conflicts
            && self.links == other.links
            && self.volatility == other.volatility
            && self.supersedes == other.supersedes
            && self.superseded_by == other.superseded_by
            && self.claims == other.claims
            && self.review_by == other.review_by
    }
}

impl PageMeta {
    /// Returns the start `(line, col)` of `key` as written in the page file,
    /// with 1-based page-file lines (including the opening `+++` fence) and
    /// 1-based code-point columns, or `None` if `key` was not written.
    #[must_use]
    pub fn field_location(&self, key: &str) -> Option<(u32, u32)> {
        self.field_locations.get(key).copied()
    }
}

/// Classification of a page front-matter parse error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaErrorKind {
    /// Invalid TOML syntax.
    Syntax,
    /// An unknown front-matter key (including note-only keys on non-note pages).
    UnknownField,
    /// A value of the wrong TOML type (e.g. `stage = "3"`).
    WrongType,
    /// A required front-matter key is missing (e.g. no `as_of`).
    MissingField,
    /// A malformed/impossible calendar date or a partial date where a full date is required.
    CalendarDate,
    /// A value of the right type that is outside its vocabulary or form.
    Value,
}

/// Error returned by [`parse_meta`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaError {
    kind: MetaErrorKind,
    field: Option<String>,
    message: String,
    line: u32,
    col: u32,
}

impl MetaError {
    /// Returns the error classification.
    #[must_use]
    pub const fn kind(&self) -> MetaErrorKind {
        self.kind
    }

    /// Returns the offending front-matter key name when known.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// Returns the diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the 1-based page-file line number (offset by the opening `+++` fence; always `>= 1`).
    #[must_use]
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// Returns the 1-based code-point column number (always `>= 1`).
    #[must_use]
    pub const fn col(&self) -> u32 {
        self.col
    }
}

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {:?}: {}",
            self.line, self.col, self.kind, self.message
        )
    }
}

impl std::error::Error for MetaError {}

const TAG_WRONG_TYPE: &str = "[WRONG_TYPE:";
const TAG_VALUE: &str = "[VALUE:";
const TAG_CALENDAR_DATE: &str = "[CALENDAR_DATE:";

fn tagged_err<E: de::Error>(tag: &str, field: &str, msg: impl fmt::Display) -> E {
    E::custom(format!("{tag}{field}]{msg}"))
}

fn parse_tagged_message(msg: &str) -> Option<(MetaErrorKind, String, String)> {
    for (prefix, kind) in [
        (TAG_WRONG_TYPE, MetaErrorKind::WrongType),
        (TAG_VALUE, MetaErrorKind::Value),
        (TAG_CALENDAR_DATE, MetaErrorKind::CalendarDate),
    ] {
        if let Some(rest) = msg.strip_prefix(prefix)
            && let Some((field, detail)) = rest.split_once(']')
        {
            return Some((kind, field.to_owned(), detail.to_owned()));
        }
    }
    None
}

fn is_valid_title(title: &str) -> bool {
    let count = title.chars().count();
    (1..=120).contains(&count)
        && !title.ends_with('.')
        && !title.contains(['<', '>', '*', '[', ']', '`'])
}

fn de_string_for_field<'de, D: Deserializer<'de>>(
    deserializer: D,
    field: &'static str,
) -> Result<String, D::Error> {
    struct StrVisitor(&'static str);
    impl<'de> Visitor<'de> for StrVisitor {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(v.to_owned())
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            Ok(v)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Err(tagged_err(TAG_WRONG_TYPE, self.0, "expected a string"))
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Err(tagged_err(TAG_WRONG_TYPE, self.0, "expected a string"))
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Err(tagged_err(TAG_WRONG_TYPE, self.0, "expected a string"))
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Err(tagged_err(TAG_WRONG_TYPE, self.0, "expected a string"))
        }
        fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err(tagged_err(TAG_WRONG_TYPE, self.0, "expected a string"))
        }
        fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err(tagged_err(TAG_WRONG_TYPE, self.0, "expected a string"))
        }
    }
    deserializer.deserialize_any(StrVisitor(field))
}

fn de_string_vec_for_field<'de, D: Deserializer<'de>>(
    deserializer: D,
    field: &'static str,
) -> Result<Vec<String>, D::Error> {
    struct VecStrVisitor(&'static str);
    impl<'de> Visitor<'de> for VecStrVisitor {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an array of strings")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(val) = seq.next_element::<toml::Value>()? {
                match val {
                    toml::Value::String(s) => out.push(s),
                    _ => {
                        return Err(tagged_err(
                            TAG_WRONG_TYPE,
                            self.0,
                            "expected an array of strings",
                        ));
                    }
                }
            }
            Ok(out)
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Err(tagged_err(
                TAG_WRONG_TYPE,
                self.0,
                "expected an array of strings",
            ))
        }
        fn visit_string<E: de::Error>(self, _: String) -> Result<Self::Value, E> {
            Err(tagged_err(
                TAG_WRONG_TYPE,
                self.0,
                "expected an array of strings",
            ))
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Err(tagged_err(
                TAG_WRONG_TYPE,
                self.0,
                "expected an array of strings",
            ))
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Err(tagged_err(
                TAG_WRONG_TYPE,
                self.0,
                "expected an array of strings",
            ))
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Err(tagged_err(
                TAG_WRONG_TYPE,
                self.0,
                "expected an array of strings",
            ))
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Err(tagged_err(
                TAG_WRONG_TYPE,
                self.0,
                "expected an array of strings",
            ))
        }
        fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err(tagged_err(
                TAG_WRONG_TYPE,
                self.0,
                "expected an array of strings",
            ))
        }
    }
    deserializer.deserialize_any(VecStrVisitor(field))
}

fn de_page_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PageId, D::Error> {
    let s = de_string_for_field(deserializer, "id")?;
    PageId::parse(&s).map_err(|e| tagged_err(TAG_VALUE, "id", e))
}

fn de_title<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let s = de_string_for_field(deserializer, "title")?;
    if !is_valid_title(&s) {
        return Err(tagged_err(
            TAG_VALUE,
            "title",
            "title must be 1..=120 chars without markup or final period",
        ));
    }
    Ok(s)
}

fn de_area_code<'de, D: Deserializer<'de>>(deserializer: D) -> Result<AreaCode, D::Error> {
    let s = de_string_for_field(deserializer, "area")?;
    AreaCode::parse(&s).map_err(|e| tagged_err(TAG_VALUE, "area", e))
}

fn de_subarea<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let s = de_string_for_field(deserializer, "subarea")?;
    if !is_slug(&s) {
        return Err(tagged_err(TAG_VALUE, "subarea", "invalid subarea slug"));
    }
    Ok(s)
}

fn de_page_type<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PageType, D::Error> {
    let s = de_string_for_field(deserializer, "type")?;
    PageType::parse(&s).map_err(|e| tagged_err(TAG_VALUE, "type", e))
}

fn de_stage<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    struct StageVisitor;
    impl<'de> Visitor<'de> for StageVisitor {
        type Value = u8;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an integer stage 1..=8")
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            if (1..=8).contains(&v) {
                Ok(u8::try_from(v).unwrap_or(1))
            } else {
                Err(tagged_err(TAG_VALUE, "stage", "stage must be 1 to 8"))
            }
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            if (1..=8).contains(&v) {
                Ok(u8::try_from(v).unwrap_or(1))
            } else {
                Err(tagged_err(TAG_VALUE, "stage", "stage must be 1 to 8"))
            }
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Err(tagged_err(TAG_WRONG_TYPE, "stage", "expected an integer"))
        }
        fn visit_string<E: de::Error>(self, _: String) -> Result<Self::Value, E> {
            Err(tagged_err(TAG_WRONG_TYPE, "stage", "expected an integer"))
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Err(tagged_err(TAG_WRONG_TYPE, "stage", "expected an integer"))
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Err(tagged_err(TAG_WRONG_TYPE, "stage", "expected an integer"))
        }
        fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err(tagged_err(TAG_WRONG_TYPE, "stage", "expected an integer"))
        }
        fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err(tagged_err(TAG_WRONG_TYPE, "stage", "expected an integer"))
        }
    }
    deserializer.deserialize_any(StageVisitor)
}

fn de_readiness<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Readiness, D::Error> {
    let s = de_string_for_field(deserializer, "readiness")?;
    Readiness::parse(&s).map_err(|e| tagged_err(TAG_VALUE, "readiness", e))
}

fn de_status<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PageStatus, D::Error> {
    let s = de_string_for_field(deserializer, "status")?;
    PageStatus::parse(&s).map_err(|e| tagged_err(TAG_VALUE, "status", e))
}

fn de_date_for_field<'de, D: Deserializer<'de>>(
    deserializer: D,
    field: &'static str,
) -> Result<CivilDate, D::Error> {
    let val = toml::Value::deserialize(deserializer)
        .map_err(|e| tagged_err(TAG_CALENDAR_DATE, field, e))?;
    match val {
        toml::Value::Datetime(dt) => {
            if dt.time.is_some() || dt.offset.is_some() {
                return Err(tagged_err(
                    TAG_CALENDAR_DATE,
                    field,
                    "expected a local calendar date YYYY-MM-DD without time or offset",
                ));
            }
            let Some(d) = dt.date else {
                return Err(tagged_err(
                    TAG_CALENDAR_DATE,
                    field,
                    "expected a local calendar date YYYY-MM-DD",
                ));
            };
            CivilDate::from_ymd(d.year, d.month, d.day)
                .map_err(|e| tagged_err(TAG_CALENDAR_DATE, field, e))
        }
        toml::Value::Integer(n) if (1000..=9999).contains(&n) => Err(tagged_err(
            TAG_CALENDAR_DATE,
            field,
            "partial year is not a full calendar date YYYY-MM-DD",
        )),
        toml::Value::String(s) => {
            if s.len() >= 4 && s.as_bytes()[..4].iter().all(u8::is_ascii_digit) {
                Err(tagged_err(
                    TAG_CALENDAR_DATE,
                    field,
                    "expected an unquoted full TOML local date YYYY-MM-DD",
                ))
            } else {
                Err(tagged_err(
                    TAG_WRONG_TYPE,
                    field,
                    "expected a TOML local date",
                ))
            }
        }
        _ => Err(tagged_err(
            TAG_WRONG_TYPE,
            field,
            "expected a TOML local date",
        )),
    }
}

fn de_as_of<'de, D: Deserializer<'de>>(deserializer: D) -> Result<CivilDate, D::Error> {
    de_date_for_field(deserializer, "as_of")
}

fn de_review_by<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<CivilDate>, D::Error> {
    de_date_for_field(deserializer, "review_by").map(Some)
}

fn ser_date<S: Serializer>(date: &CivilDate, serializer: S) -> Result<S::Ok, S::Error> {
    let dt = toml::value::Datetime {
        date: Some(toml::value::Date {
            year: date.year(),
            month: date.month(),
            day: date.day(),
        }),
        time: None,
        offset: None,
    };
    dt.serialize(serializer)
}

fn ser_opt_date<S: Serializer>(date: &Option<CivilDate>, serializer: S) -> Result<S::Ok, S::Error> {
    match date {
        Some(d) => ser_date(d, serializer),
        None => serializer.serialize_none(),
    }
}

fn de_handling<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    de_string_vec_for_field(deserializer, "handling")
}

fn de_terms<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<TermId>, D::Error> {
    let items = de_string_vec_for_field(deserializer, "terms")?;
    items
        .into_iter()
        .map(|s| TermId::parse(&s).map_err(|e| tagged_err(TAG_VALUE, "terms", e)))
        .collect()
}

fn de_id_list<'de, D: Deserializer<'de>, T>(
    deserializer: D,
    field: &'static str,
    parse_fn: fn(&str) -> Result<T, crate::ids::IdError>,
) -> Result<Vec<T>, D::Error> {
    let items = de_string_vec_for_field(deserializer, field)?;
    items
        .into_iter()
        .map(|s| parse_fn(&s).map_err(|e| tagged_err(TAG_VALUE, field, e)))
        .collect()
}

fn de_prereq<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PageId>, D::Error> {
    de_id_list(deserializer, "prereq", PageId::parse)
}

fn de_questions<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<QuestionId>, D::Error> {
    de_id_list(deserializer, "questions", QuestionId::parse)
}

fn de_conflicts<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<ConflictId>, D::Error> {
    de_id_list(deserializer, "conflicts", ConflictId::parse)
}

fn de_links<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PageId>, D::Error> {
    de_id_list(deserializer, "links", PageId::parse)
}

fn de_supersedes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PageId>, D::Error> {
    de_id_list(deserializer, "supersedes", PageId::parse)
}

fn de_superseded_by<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PageId>, D::Error> {
    de_id_list(deserializer, "superseded_by", PageId::parse)
}

fn de_claims<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<ClaimId>>, D::Error> {
    de_id_list(deserializer, "claims", ClaimId::parse).map(Some)
}

fn de_volatility<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Volatility>, D::Error> {
    let s = de_string_for_field(deserializer, "volatility")?;
    Volatility::parse(&s)
        .map(Some)
        .ok_or_else(|| tagged_err(TAG_VALUE, "volatility", "invalid volatility"))
}

#[derive(Deserialize)]
struct QuotedKeyHelper {
    v: String,
}

fn decode_quoted_toml_key(raw_quoted: &str, raw_inner: &str, quote: u8) -> String {
    if let Ok(parsed) = toml::from_str::<QuotedKeyHelper>(&format!("v = {raw_quoted}")) {
        return parsed.v;
    }
    if quote == b'"' {
        decode_toml_basic_key(raw_inner)
    } else {
        raw_inner.to_owned()
    }
}

fn decode_toml_basic_key(raw_inner: &str) -> String {
    let mut out = String::with_capacity(raw_inner.len());
    let mut chars = raw_inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(esc) = chars.next() else {
            out.push('\\');
            break;
        };
        match esc {
            'b' => out.push('\u{0008}'),
            't' => out.push('\t'),
            'n' => out.push('\n'),
            'f' => out.push('\u{000C}'),
            'r' => out.push('\r'),
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'u' | 'U' => {
                let count = if esc == 'u' { 4 } else { 8 };
                let mut hex = String::with_capacity(count);
                for _ in 0..count {
                    if let Some(hc) = chars.next() {
                        hex.push(hc);
                    }
                }
                if hex.len() == count
                    && let Ok(code) = u32::from_str_radix(&hex, 16)
                    && let Some(decoded) = char::from_u32(code)
                {
                    out.push(decoded);
                } else {
                    out.push('\\');
                    out.push(esc);
                    out.push_str(&hex);
                }
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

#[derive(Debug, Clone)]
struct ScannedKey {
    name: String,
    key_line: u32,
    key_col: u32,
    raw_value_trimmed: String,
}
/// page-file `(line, col)` of each key (offset by 1 line for the opening `+++` fence).
fn scan_top_level_keys(front_matter: &str, fence_line_offset: u32) -> Vec<ScannedKey> {
    let bytes = front_matter.as_bytes();
    let len = bytes.len();
    let mut idx = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut expecting_key = true;
    let mut out = Vec::new();

    while idx < len {
        let b = bytes[idx];

        // Handle whitespace
        if b == b' ' || b == b'\t' || b == b'\r' {
            idx += 1;
            continue;
        }
        if b == b'\n' {
            if bracket_depth == 0 && brace_depth == 0 {
                expecting_key = true;
            }
            idx += 1;
            continue;
        }
        // Handle comment
        if b == b'#' {
            while idx < len && bytes[idx] != b'\n' {
                idx += 1;
            }
            continue;
        }

        if expecting_key && bracket_depth == 0 && brace_depth == 0 {
            let key_start = idx;
            let key_name = if b == b'"' || b == b'\'' {
                let quote = b;
                idx += 1;
                let start_inner = idx;
                while idx < len && bytes[idx] != quote && bytes[idx] != b'\n' {
                    if quote == b'"' && bytes[idx] == b'\\' && idx + 1 < len {
                        idx += 2;
                    } else {
                        idx += 1;
                    }
                }
                let raw_inner = &front_matter[start_inner..idx];
                if idx < len && bytes[idx] == quote {
                    idx += 1;
                }
                let raw_quoted = &front_matter[key_start..idx];
                decode_quoted_toml_key(raw_quoted, raw_inner, quote)
            } else {
                while idx < len
                    && (bytes[idx].is_ascii_alphanumeric()
                        || bytes[idx] == b'_'
                        || bytes[idx] == b'-')
                {
                    idx += 1;
                }
                front_matter[key_start..idx].to_owned()
            };

            while idx < len && (bytes[idx] == b' ' || bytes[idx] == b'\t') {
                idx += 1;
            }
            if !key_name.is_empty() && idx < len && bytes[idx] == b'=' {
                idx += 1; // skip '='
                let val_start = idx;
                let mut line_end = val_start;
                let mut in_str: Option<u8> = None;
                while line_end < len && bytes[line_end] != b'\n' {
                    let ch = bytes[line_end];
                    if let Some(q) = in_str {
                        if q == b'"' && ch == b'\\' && line_end + 1 < len {
                            line_end += 2;
                            continue;
                        }
                        if ch == q {
                            in_str = None;
                        }
                    } else if ch == b'"' || ch == b'\'' {
                        in_str = Some(ch);
                    } else if ch == b'#' {
                        break;
                    }
                    line_end += 1;
                }
                let (raw_line, key_col) = byte_offset_to_line_col(front_matter, key_start);
                out.push(ScannedKey {
                    name: key_name,
                    key_line: raw_line.saturating_add(fence_line_offset),
                    key_col,
                    raw_value_trimmed: front_matter[val_start..line_end].trim().to_owned(),
                });
                expecting_key = false;
                continue;
            }
            expecting_key = false;
            continue;
        }

        // Advance through value tokens (strings, brackets, braces)
        if b == b'"' {
            if idx + 2 < len && bytes[idx + 1] == b'"' && bytes[idx + 2] == b'"' {
                idx += 3;
                while idx < len {
                    if bytes[idx] == b'\\' && idx + 1 < len {
                        idx += 2;
                    } else if idx + 2 < len
                        && bytes[idx] == b'"'
                        && bytes[idx + 1] == b'"'
                        && bytes[idx + 2] == b'"'
                    {
                        idx += 3;
                        break;
                    } else {
                        idx += 1;
                    }
                }
            } else {
                idx += 1;
                while idx < len && bytes[idx] != b'\n' {
                    if bytes[idx] == b'\\' && idx + 1 < len {
                        idx += 2;
                    } else if bytes[idx] == b'"' {
                        idx += 1;
                        break;
                    } else {
                        idx += 1;
                    }
                }
            }
        } else if b == b'\'' {
            if idx + 2 < len && bytes[idx + 1] == b'\'' && bytes[idx + 2] == b'\'' {
                idx += 3;
                while idx < len {
                    if idx + 2 < len
                        && bytes[idx] == b'\''
                        && bytes[idx + 1] == b'\''
                        && bytes[idx + 2] == b'\''
                    {
                        idx += 3;
                        break;
                    }
                    idx += 1;
                }
            } else {
                idx += 1;
                while idx < len && bytes[idx] != b'\n' && bytes[idx] != b'\'' {
                    idx += 1;
                }
                if idx < len && bytes[idx] == b'\'' {
                    idx += 1;
                }
            }
        } else if b == b'[' {
            bracket_depth += 1;
            idx += 1;
        } else if b == b']' {
            bracket_depth = bracket_depth.saturating_sub(1);
            idx += 1;
        } else if b == b'{' {
            brace_depth += 1;
            idx += 1;
        } else if b == b'}' {
            brace_depth = brace_depth.saturating_sub(1);
            idx += 1;
        } else {
            idx += 1;
        }
    }

    out
}

fn is_date_field(name: &str) -> bool {
    matches!(name, "as_of" | "review_by")
}

/// Checks whether a raw TOML value token on a date field (`as_of` or `review_by`)
/// is a malformed/impossible calendar date or partial date token that the TOML
/// lexer itself rejected before serde deserialization.
fn is_lex_rejected_date_token(raw_val: &str) -> bool {
    let Some(first) = raw_val.bytes().next() else {
        return false;
    };
    first.is_ascii_digit()
        && raw_val
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b':' | b'.' | b'+' | b' '))
}

fn extract_backticked_name(msg: &str) -> Option<String> {
    let start = msg.find('`')?;
    let after = &msg[start + 1..];
    let end = after.find('`')?;
    Some(after[..end].to_owned())
}

/// Parses TOML front matter into [`PageMeta`], classifying any failure into
/// [`MetaErrorKind`] and recording 1-based page-file line and code-point column
/// (offset by the opening `+++` fence).
///
/// Accepts either the inner front-matter slice returned by [`split`] or a full
/// fenced front-matter block (`+++\n...\n+++`).
pub fn parse_meta(input: &str) -> Result<PageMeta, MetaError> {
    let inner = if let Some(after_open) = input.strip_prefix("+++\n") {
        if let Some(trimmed) = after_open.strip_suffix("\n+++") {
            trimmed
        } else if let Some(trimmed) = after_open.strip_suffix("\n+++\n") {
            trimmed
        } else if after_open == "+++" {
            ""
        } else {
            input
        }
    } else {
        input
    };

    let fence_line_offset = 1u32;
    let scanned_keys = scan_top_level_keys(inner, fence_line_offset);

    match toml::from_str::<PageMeta>(inner) {
        Ok(mut meta) => {
            let mut locations = BTreeMap::new();
            for sk in scanned_keys {
                locations
                    .entry(sk.name)
                    .or_insert((sk.key_line, sk.key_col));
            }
            meta.field_locations = locations;
            Ok(meta)
        }
        Err(err) => {
            let raw_msg = err.message();
            let span_start = err.span().map_or(0, |r| r.start);
            let (raw_line, raw_col) = byte_offset_to_line_col(inner, span_start);
            let span_page_line = raw_line.saturating_add(fence_line_offset);

            if let Some((kind, field, detail)) = parse_tagged_message(raw_msg) {
                let (line, col) = scanned_keys
                    .iter()
                    .find(|sk| sk.name == field)
                    .map_or((span_page_line, raw_col), |sk| (sk.key_line, sk.key_col));
                return Err(MetaError {
                    kind,
                    field: Some(field),
                    message: detail,
                    line,
                    col,
                });
            }

            if raw_msg.starts_with("unknown field ") {
                let field = extract_backticked_name(raw_msg);
                let (line, col) = field
                    .as_deref()
                    .and_then(|f| scanned_keys.iter().find(|sk| sk.name == f))
                    .map_or((span_page_line, raw_col), |sk| (sk.key_line, sk.key_col));
                return Err(MetaError {
                    kind: MetaErrorKind::UnknownField,
                    field,
                    message: raw_msg.to_owned(),
                    line,
                    col,
                });
            }

            if raw_msg.starts_with("missing field ") {
                let field = extract_backticked_name(raw_msg);
                return Err(MetaError {
                    kind: MetaErrorKind::MissingField,
                    field,
                    message: raw_msg.to_owned(),
                    line: span_page_line,
                    col: raw_col,
                });
            }

            // Check if the TOML lexer failed on a date token (`as_of` or `review_by`),
            // such as `as_of = 2025-02-30` or `as_of = 2025-11`.
            if let Some(sk) = scanned_keys
                .iter()
                .find(|sk| sk.key_line == span_page_line && is_date_field(&sk.name))
                && is_lex_rejected_date_token(&sk.raw_value_trimmed)
            {
                return Err(MetaError {
                    kind: MetaErrorKind::CalendarDate,
                    field: Some(sk.name.clone()),
                    message: raw_msg.to_owned(),
                    line: sk.key_line,
                    col: sk.key_col,
                });
            }

            if raw_msg.contains("invalid type:") {
                let field = scanned_keys
                    .iter()
                    .find(|sk| sk.key_line == span_page_line)
                    .map(|sk| sk.name.clone());
                return Err(MetaError {
                    kind: MetaErrorKind::WrongType,
                    field,
                    message: raw_msg.to_owned(),
                    line: span_page_line,
                    col: raw_col,
                });
            }

            Err(MetaError {
                kind: MetaErrorKind::Syntax,
                field: None,
                message: raw_msg.to_owned(),
                line: span_page_line,
                col: raw_col,
            })
        }
    }
}

/// Error returned by [`normalize`] when a page file cannot be normalized without
/// rewriting its front matter or repairing invalid UTF-8 or front-matter syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFormatError {
    line: u32,
    col: u32,
    message: String,
}

impl PageFormatError {
    /// Returns the 1-based line number of the fault (always `>= 1`).
    #[must_use]
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// Returns the 1-based code-point column number of the fault (always `>= 1`).
    #[must_use]
    pub const fn col(&self) -> u32 {
        self.col
    }

    /// Returns the diagnostic message describing the fault.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PageFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for PageFormatError {}

/// Normalizes a Corpus page file (`bytes`) according to the File rules while leaving
/// front matter unreserialized:
///
/// - Strips a leading UTF-8 byte-order mark and converts `\r\n` / `\r` line endings
///   to `\n` across the whole file.
/// - Validates the `+++` fences and front matter, reporting any front-matter fault
///   other than a byte-order mark or line ending (including non-NFC text or trailing
///   whitespace inside front matter) without modifying the text.
/// - Normalizes the Markdown body to Unicode NFC, trims trailing spaces and tabs on
///   every body line, and ensures the file ends with a single `\n`.
///
/// # Errors
///
/// Returns [`PageFormatError`] if `bytes` is not valid UTF-8, the fences or front
/// matter fail validation, front matter contains non-NFC text or trailing whitespace,
/// or the page body is empty.
pub fn normalize(bytes: &[u8]) -> Result<String, PageFormatError> {
    let Ok(raw_text) = std::str::from_utf8(bytes) else {
        return Err(PageFormatError {
            line: 1,
            col: 1,
            message: "file is not valid UTF-8".to_owned(),
        });
    };

    let without_bom = raw_text.strip_prefix('\u{feff}').unwrap_or(raw_text);
    let lf_text = if without_bom.contains('\r') {
        without_bom.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        without_bom.to_owned()
    };

    let sp = split(&lf_text).map_err(|err| {
        let msg = match err.kind() {
            SplitErrorKind::MissingOpeningFence => "missing opening +++ fence on line 1",
            SplitErrorKind::MissingClosingFence => "missing closing +++ fence",
            SplitErrorKind::MissingBlankLine => "missing blank line after closing +++ fence",
        };
        PageFormatError {
            line: err.line(),
            col: err.col(),
            message: msg.to_owned(),
        }
    })?;

    if !sp.front_matter.is_empty() {
        for (idx, line_str) in sp.front_matter.split('\n').enumerate() {
            let line_no = u32::try_from(idx).unwrap_or(u32::MAX).saturating_add(2);
            if !is_nfc(line_str) {
                return Err(PageFormatError {
                    line: line_no,
                    col: 1,
                    message: "front matter line is not in Unicode NFC normal form".to_owned(),
                });
            }
            if line_str.ends_with(' ') || line_str.ends_with('\t') {
                let col = u32::try_from(line_str.chars().count()).unwrap_or(1).max(1);
                return Err(PageFormatError {
                    line: line_no,
                    col,
                    message: "front matter line has trailing whitespace".to_owned(),
                });
            }
        }
    }

    parse_meta(&sp.front_matter).map_err(|err| PageFormatError {
        line: err.line(),
        col: err.col(),
        message: err.message().to_owned(),
    })?;

    let nfc_body: String = sp.body.nfc().collect();
    let mut lines: Vec<&str> = nfc_body
        .split('\n')
        .map(|l| l.trim_end_matches([' ', '\t']))
        .collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Err(PageFormatError {
            line: sp.body_line(),
            col: 1,
            message: "page body is empty".to_owned(),
        });
    }

    let norm_body = format!("{}\n", lines.join("\n"));
    Ok(format!("+++\n{}\n+++\n\n{norm_body}", sp.front_matter))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC_PAGE: &str = "\
+++
id = \"cmp-withholding\"
title = \"Payment withholding for business-system deficiencies\"
area = \"cmp\"
subarea = \"business-systems\"
type = \"concept\"
stage = 3
readiness = \"PUBLIC-NOW\"
status = \"draft\"
as_of = 2025-11-01
handling = [\"public\"]
terms = [\"significant_deficiency\"]
volatility = \"V1\"
+++

## Purpose

Not established.
";

    #[test]
    fn synthetic_front_matter_parses() {
        let s = split(SYNTHETIC_PAGE).expect("synthetic page must split");
        assert_eq!(s.body_line(), 16);
        assert_eq!(s.body(), "## Purpose\n\nNot established.\n");
        let meta = parse_meta(&s.front_matter).expect("synthetic front matter must parse");
        assert_eq!(meta.id.as_str(), "cmp-withholding");
        assert_eq!(
            meta.title,
            "Payment withholding for business-system deficiencies"
        );
        assert_eq!(meta.area.as_str(), "cmp");
        assert_eq!(meta.subarea, "business-systems");
        assert_eq!(meta.r#type, PageType::Concept);
        assert_eq!(meta.stage, 3);
        assert_eq!(meta.readiness, Readiness::PublicNow);
        assert_eq!(meta.status, PageStatus::Draft);
        assert_eq!(meta.as_of.to_string(), "2025-11-01");
        assert_eq!(meta.handling, vec!["public".to_owned()]);
        assert_eq!(meta.terms.len(), 1);
        assert_eq!(meta.terms[0].as_str(), "significant_deficiency");
        assert_eq!(meta.volatility, Some(Volatility::V1));
    }

    #[test]
    fn split_reports_missing_opening_closing_and_blank_line() {
        let err_open = split("---\nid = \"cmp-withholding\"\n+++\n\nBody\n")
            .expect_err("first line of --- must fail");
        assert_eq!(err_open.kind(), SplitErrorKind::MissingOpeningFence);
        assert_eq!(err_open.line(), 1);

        let err_unbalanced =
            split("+++\nid = \"cmp-withholding\"\nBody\n").expect_err("unbalanced fence must fail");
        assert_eq!(err_unbalanced.kind(), SplitErrorKind::MissingClosingFence);

        let err_no_blank = split("+++\nid = \"cmp-withholding\"\n+++\nBody\n")
            .expect_err("missing blank line after +++ must fail");
        assert_eq!(err_no_blank.kind(), SplitErrorKind::MissingBlankLine);
        assert_eq!(err_no_blank.line(), 4);
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let s = split(SYNTHETIC_PAGE).expect("split");
        let bad = format!("{}\ntitel = \"typo\"", s.front_matter);
        let err = parse_meta(&bad).expect_err("unknown field must fail");
        assert_eq!(err.kind(), MetaErrorKind::UnknownField);
        assert_eq!(err.field(), Some("titel"));
        assert_eq!(err.line(), 14);
        assert_eq!(err.col(), 1);
    }

    #[test]
    fn note_only_field_is_an_unknown_field() {
        let s = split(SYNTHETIC_PAGE).expect("split");
        let bad = format!("{}\norigin = \"ai_drafted\"", s.front_matter);
        let err = parse_meta(&bad).expect_err("note-only field must fail on concept page");
        assert_eq!(err.kind(), MetaErrorKind::UnknownField);
        assert_eq!(err.field(), Some("origin"));
    }

    #[test]
    fn wrong_type_missing_field_and_vocabulary_errors_keep_their_kinds() {
        let s = split(SYNTHETIC_PAGE).expect("split");

        let wrong_type = s.front_matter.replace("stage = 3", "stage = \"3\"");
        let err = parse_meta(&wrong_type).expect_err("stage = \"3\" must fail");
        assert_eq!(err.kind(), MetaErrorKind::WrongType);
        assert_eq!(err.field(), Some("stage"));
        assert_eq!(err.line(), 7);

        let missing_as_of = s.front_matter.replace("as_of = 2025-11-01\n", "");
        let err = parse_meta(&missing_as_of).expect_err("missing as_of must fail");
        assert_eq!(err.kind(), MetaErrorKind::MissingField);
        assert_eq!(err.field(), Some("as_of"));

        let bad_status = s
            .front_matter
            .replace("status = \"draft\"", "status = \"final\"");
        let err = parse_meta(&bad_status).expect_err("status = \"final\" must fail");
        assert_eq!(err.kind(), MetaErrorKind::Value);
        assert_eq!(err.field(), Some("status"));
        assert_eq!(err.line(), 9);

        let bad_type = s
            .front_matter
            .replace("type = \"concept\"", "type = \"note\"");
        let err = parse_meta(&bad_type).expect_err("type = \"note\" must fail in v1");
        assert_eq!(err.kind(), MetaErrorKind::Value);
        assert_eq!(err.field(), Some("type"));
        assert_eq!(err.line(), 6);
    }

    #[test]
    fn calendar_date_errors_keep_their_kind() {
        let s = split(SYNTHETIC_PAGE).expect("split");

        // Impossible TOML local date
        let impossible = s
            .front_matter
            .replace("as_of = 2025-11-01", "as_of = 2025-02-30");
        let err = parse_meta(&impossible).expect_err("impossible date must fail");
        assert_eq!(err.kind(), MetaErrorKind::CalendarDate);
        assert_eq!(err.field(), Some("as_of"));
        assert_eq!(err.line(), 10);

        // Partial full-date field (unquoted YYYY-MM and quoted YYYY-MM)
        let partial_unquoted = s
            .front_matter
            .replace("as_of = 2025-11-01", "as_of = 2025-11");
        let err = parse_meta(&partial_unquoted).expect_err("partial date must fail");
        assert_eq!(err.kind(), MetaErrorKind::CalendarDate);
        assert_eq!(err.field(), Some("as_of"));
        assert_eq!(err.line(), 10);

        let partial_quoted = s
            .front_matter
            .replace("as_of = 2025-11-01", "as_of = \"2025-11\"");
        let err = parse_meta(&partial_quoted).expect_err("quoted partial date must fail");
        assert_eq!(err.kind(), MetaErrorKind::CalendarDate);
        assert_eq!(err.field(), Some("as_of"));
        assert_eq!(err.line(), 10);

        // Valid leap day succeeds
        let leap = s
            .front_matter
            .replace("as_of = 2025-11-01", "as_of = 2024-02-29");
        let parsed = parse_meta(&leap).expect("valid leap day must succeed");
        assert_eq!(parsed.as_of.to_string(), "2024-02-29");

        // Unrelated malformed array still has the syntax kind
        let bad_array = s
            .front_matter
            .replace("handling = [\"public\"]", "handling = [\"public\"");
        let err = parse_meta(&bad_array).expect_err("malformed array must fail");
        assert_eq!(err.kind(), MetaErrorKind::Syntax);
    }

    #[test]
    fn successful_meta_retains_field_locations() {
        let custom = "\
# Leading comment with multibyte — café and fake id = \"cmp-fake\"
  status = \"draft\"
title = \"Café — naïve withholding rule\"
id = \"cmp-withholding\"
area = \"cmp\"
subarea = \"business-systems\"
type = \"concept\"
stage = 3 # inline comment with — multibyte
readiness = \"PUBLIC-NOW\"
as_of = 2025-11-01
handling = [
  \"Bad Label!\"
]
  terms = [\"significant_deficiency\"]
review_by = 2026-01-01
claims = [\"CL-CMP-0001\"]
";
        let meta = parse_meta(custom).expect("custom front matter must parse");
        assert_eq!(meta.field_location("status"), Some((3, 3)));
        assert_eq!(meta.field_location("title"), Some((4, 1)));
        assert_eq!(meta.field_location("id"), Some((5, 1)));
        assert_eq!(meta.field_location("area"), Some((6, 1)));
        assert_eq!(meta.field_location("subarea"), Some((7, 1)));
        assert_eq!(meta.field_location("type"), Some((8, 1)));
        assert_eq!(meta.field_location("stage"), Some((9, 1)));
        assert_eq!(meta.field_location("readiness"), Some((10, 1)));
        assert_eq!(meta.field_location("as_of"), Some((11, 1)));
        assert_eq!(meta.field_location("handling"), Some((12, 1)));
        assert_eq!(meta.field_location("terms"), Some((15, 3)));
        assert_eq!(meta.field_location("review_by"), Some((16, 1)));
        assert_eq!(meta.field_location("claims"), Some((17, 1)));
        assert_eq!(meta.field_location("volatility"), None);
        assert_eq!(meta.field_location("prereq"), None);

        // Empty handling array also parses and retains its position for FB0410
        let empty_handling = custom.replace("[\n  \"Bad Label!\"\n]", "[]");
        let meta_empty = parse_meta(&empty_handling).expect("empty handling must parse");
        assert_eq!(meta_empty.handling, Vec::<String>::new());
        assert_eq!(meta_empty.field_location("handling"), Some((12, 1)));

        // Escaped quoted key decodes and retains its position
        let escaped_key = custom.replace("handling =", "\"handl\\u0069ng\" =");
        let meta_escaped = parse_meta(&escaped_key).expect("escaped quoted key must parse");
        assert_eq!(meta_escaped.field_location("handling"), Some((12, 1)));

        // Serialization skips field_locations and emits unquoted local dates that round-trip
        let serialized = toml::to_string(&meta).expect("PageMeta must serialize");
        assert!(!serialized.contains("field_locations"));
        assert!(serialized.contains("as_of = 2025-11-01"));
        assert!(serialized.contains("review_by = 2026-01-01"));
        let reparsed = parse_meta(&serialized).expect("serialized PageMeta must re-parse");
        assert_eq!(reparsed, meta);
    }

    #[test]
    fn normalize_fixes_body_and_preserves_front_matter_verbatim() {
        let page_with_comment =
            SYNTHETIC_PAGE.replacen("+++\n", "+++\n# synthetic comment kept verbatim\n", 1);
        let (fm_part, body_part) = page_with_comment
            .split_once("\n+++\n\n")
            .expect("split synthetic page");
        let dirty_body_only = format!(
            "\u{feff}{}\r\n+++\r\n\r\n{}\r\n\r\n",
            fm_part.replace('\n', "\r\n"),
            body_part
                .replace('\n', "  \t\r\n")
                .replace(crate::strings::empty_part(), "Cafe\u{0301} prose.")
        );
        let normalized = normalize(dirty_body_only.as_bytes()).expect("normalize valid page");
        assert!(normalized.contains("# synthetic comment kept verbatim\n"));
        assert!(normalized.contains("Caf\u{00e9} prose."));
        assert!(!normalized.starts_with('\u{feff}'));
        assert!(!normalized.contains('\r'));
        assert!(normalized.ends_with('\n') && !normalized.ends_with("\n\n"));

        // Front-matter faults other than BOM / CRLF are refused without rewriting.
        let bad_trailing_fm =
            page_with_comment.replacen("status = \"draft\"\n", "status = \"draft\" \n", 1);
        assert!(normalize(bad_trailing_fm.as_bytes()).is_err());

        let bad_nfd_fm = page_with_comment.replacen(
            "title = \"Payment withholding for business-system deficiencies\"",
            "title = \"Payment cafe\u{0301} withholding\"",
            1,
        );
        assert!(normalize(bad_nfd_fm.as_bytes()).is_err());
        assert!(normalize(b"\xff\xfe").is_err());
    }
}
