//! Pure planner and row serializer for `fieldbook add` (`add-new-and-edition`).
//!
//! **Responsibility.** Plan a new source or source version addition against [`Model`],
//! validate completed source cells (`CompletedSourceCells`), and render canonical
//! 28-column `sources.csv` rows and replacement register payloads through the existing
//! CSV writer (`crate::schema::table::write`).
//!
//! **Rules.** Pure: no filesystem, clock, or environment access.

use std::collections::BTreeMap;
use std::fmt;

use crate::audit::{Sha256, WriteFile};
use crate::check::Model;
use crate::check::register_cells::source_kind_allows_tier;
use crate::date::{CivilDate, PartialDate, Volatility};
use crate::ids::{
    FAMILIES, IdError, IdErrorKind, MAX_SOURCE_NUMBER, SourceId, VersionId, next_source_id,
};
use crate::schema::Register;
use crate::schema::rows::{MAX_TAG_CHARS, MAX_TEXT_CHARS};
use crate::schema::table::{Row, Table, write as write_csv};
use crate::schema::vocab::{AuthorityTier, Lifecycle, SourceKind};

/// The ten `sources.csv` columns that may be populated through [`CompletedSourceCells`].
pub const SUGGESTIBLE_FIELDS: [&str; 10] = [
    "title",
    "publisher",
    "source_kind",
    "authority_tier",
    "licence",
    "published",
    "effective_from",
    "volatility",
    "tag",
    "purpose",
];

/// The eighteen reserved `sources.csv` columns that [`CompletedSourceCells::parse`] rejects.
pub const RESERVED_SOURCE_FIELDS: [&str; 18] = [
    "source_id",
    "version",
    "source_role",
    "source_family",
    "family_basis",
    "location",
    "sha256",
    "as_of",
    "retrieved",
    "lifecycle",
    "supersedes",
    "handling",
    "note",
    "origin_system",
    "origin_stream",
    "origin_key",
    "doc_kind",
    "series",
];

/// Base 28-column `sources.csv` row constructed by `add` before optional completed cells are applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    /// Column 1: `source_id` (`S-nnnn`).
    pub source_id: SourceId,
    /// Column 2: `version` (`1, 2, ...`).
    pub version: u32,
    /// Column 3: `tag`.
    pub tag: String,
    /// Column 4: `title`.
    pub title: String,
    /// Column 5: `publisher`.
    pub publisher: String,
    /// Column 6: `source_kind`.
    pub source_kind: String,
    /// Column 7: `source_role`.
    pub source_role: String,
    /// Column 8: `authority_tier`.
    pub authority_tier: String,
    /// Column 9: `licence`.
    pub licence: String,
    /// Column 10: `source_family`.
    pub source_family: String,
    /// Column 11: `family_basis`.
    pub family_basis: String,
    /// Column 12: `location`.
    pub location: String,
    /// Column 13: `sha256`.
    pub sha256: String,
    /// Column 14: `published`.
    pub published: String,
    /// Column 15: `effective_from`.
    pub effective_from: String,
    /// Column 16: `as_of`.
    pub as_of: String,
    /// Column 17: `retrieved`.
    pub retrieved: CivilDate,
    /// Column 18: `lifecycle`.
    pub lifecycle: Lifecycle,
    /// Column 19: `supersedes`.
    pub supersedes: String,
    /// Column 20: `volatility`.
    pub volatility: String,
    /// Column 21: `handling`.
    pub handling: String,
    /// Column 22: `note`.
    pub note: String,
    /// Column 23: `purpose`.
    pub purpose: String,
    /// Column 24: `origin_system`.
    pub origin_system: String,
    /// Column 25: `origin_stream`.
    pub origin_stream: String,
    /// Column 26: `origin_key`.
    pub origin_key: String,
    /// Column 27: `doc_kind`.
    pub doc_kind: String,
    /// Column 28: `series`.
    pub series: String,
}

impl SourceRow {
    /// Constructs a base `SourceRow` for `add` with `lifecycle = active`, `handling = public`,
    /// and all non-initial columns empty.
    #[must_use]
    pub fn new(
        source_id: SourceId,
        version: u32,
        location: impl Into<String>,
        sha256: impl Into<String>,
        retrieved: CivilDate,
    ) -> Self {
        Self {
            source_id,
            version,
            tag: String::new(),
            title: String::new(),
            publisher: String::new(),
            source_kind: String::new(),
            source_role: String::new(),
            authority_tier: String::new(),
            licence: String::new(),
            source_family: String::new(),
            family_basis: String::new(),
            location: location.into(),
            sha256: sha256.into(),
            published: String::new(),
            effective_from: String::new(),
            as_of: String::new(),
            retrieved,
            lifecycle: Lifecycle::Active,
            supersedes: String::new(),
            volatility: String::new(),
            handling: "public".to_owned(),
            note: String::new(),
            purpose: String::new(),
            origin_system: String::new(),
            origin_stream: String::new(),
            origin_key: String::new(),
            doc_kind: String::new(),
            series: String::new(),
        }
    }

    /// Formats this row into its 28 CSV cell strings in column order.
    #[must_use]
    pub fn to_cells(&self) -> Vec<String> {
        vec![
            self.source_id.to_string(),
            self.version.to_string(),
            self.tag.clone(),
            self.title.clone(),
            self.publisher.clone(),
            self.source_kind.clone(),
            self.source_role.clone(),
            self.authority_tier.clone(),
            self.licence.clone(),
            self.source_family.clone(),
            self.family_basis.clone(),
            self.location.clone(),
            self.sha256.clone(),
            self.published.clone(),
            self.effective_from.clone(),
            self.as_of.clone(),
            self.retrieved.to_string(),
            self.lifecycle.as_str().to_owned(),
            self.supersedes.clone(),
            self.volatility.clone(),
            self.handling.clone(),
            self.note.clone(),
            self.purpose.clone(),
            self.origin_system.clone(),
            self.origin_stream.clone(),
            self.origin_key.clone(),
            self.doc_kind.clone(),
            self.series.clone(),
        ]
    }
}

/// Validated map of the ten suggestible `sources.csv` cells (`title`, `publisher`, `source_kind`,
/// `authority_tier`, `licence`, `published`, `effective_from`, `volatility`, `tag`, `purpose`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletedSourceCells {
    cells: BTreeMap<String, String>,
}

impl CompletedSourceCells {
    /// Validates `raw` against the `sources.csv` cell rules for the ten suggestible columns,
    /// rejecting reserved columns and unknown field names.
    ///
    /// # Errors
    ///
    /// Returns [`AddError`] if any key is reserved or unknown, or if any cell value fails its
    /// column validation rule.
    pub fn parse(raw: &BTreeMap<String, String>) -> Result<Self, AddError> {
        let mut parsed_kind: Option<SourceKind> = None;
        let mut parsed_tier: Option<Option<AuthorityTier>> = None;

        for (key, value) in raw {
            let key_str = key.as_str();
            if RESERVED_SOURCE_FIELDS.contains(&key_str) {
                return Err(AddError::ReservedField(key.clone()));
            }
            if !SUGGESTIBLE_FIELDS.contains(&key_str) {
                return Err(AddError::UnknownField(key.clone()));
            }
            if value.contains(['\n', '\r']) {
                return Err(AddError::InvalidCell {
                    column: suggestible_static_name(key_str),
                    value: value.clone(),
                    reason: "line break in cell",
                });
            }
            if value.contains('|') {
                return Err(AddError::InvalidCell {
                    column: suggestible_static_name(key_str),
                    value: value.clone(),
                    reason: "pipe character in non-list cell",
                });
            }

            match key_str {
                "title" | "publisher" | "licence" => {
                    let col = suggestible_static_name(key_str);
                    if value.is_empty() {
                        return Err(AddError::InvalidCell {
                            column: col,
                            value: value.clone(),
                            reason: "required cell is empty",
                        });
                    }
                    if value.chars().count() > MAX_TEXT_CHARS {
                        return Err(AddError::InvalidCell {
                            column: col,
                            value: value.clone(),
                            reason: "exceeds maximum length",
                        });
                    }
                }
                "tag" => {
                    if value.chars().count() > MAX_TAG_CHARS {
                        return Err(AddError::InvalidCell {
                            column: "tag",
                            value: value.clone(),
                            reason: "exceeds maximum length",
                        });
                    }
                }
                "purpose" => {
                    if value.chars().count() > MAX_TEXT_CHARS {
                        return Err(AddError::InvalidCell {
                            column: "purpose",
                            value: value.clone(),
                            reason: "exceeds maximum length",
                        });
                    }
                }
                "source_kind" => {
                    let sk = SourceKind::parse(value).map_err(|_| AddError::InvalidCell {
                        column: "source_kind",
                        value: value.clone(),
                        reason: "outside source_kind vocabulary",
                    })?;
                    if sk.is_overlay() {
                        return Err(AddError::InvalidCell {
                            column: "source_kind",
                            value: value.clone(),
                            reason: "company-store-only source_kind is not permitted in Corpus",
                        });
                    }
                    parsed_kind = Some(sk);
                }
                "authority_tier" => {
                    if value.is_empty() {
                        parsed_tier = Some(None);
                    } else {
                        let tier =
                            AuthorityTier::parse(value).map_err(|_| AddError::InvalidCell {
                                column: "authority_tier",
                                value: value.clone(),
                                reason: "outside authority_tier vocabulary",
                            })?;
                        parsed_tier = Some(Some(tier));
                    }
                }
                "published" => {
                    if !value.is_empty() {
                        PartialDate::parse(value).map_err(|_| AddError::InvalidCell {
                            column: "published",
                            value: value.clone(),
                            reason: "invalid partial date",
                        })?;
                    }
                }
                "effective_from" => {
                    if !value.is_empty() {
                        PartialDate::parse(value).map_err(|_| AddError::InvalidCell {
                            column: "effective_from",
                            value: value.clone(),
                            reason: "invalid partial date",
                        })?;
                    }
                }
                "volatility" => {
                    if Volatility::parse(value).is_none() {
                        return Err(AddError::InvalidCell {
                            column: "volatility",
                            value: value.clone(),
                            reason: "outside volatility vocabulary",
                        });
                    }
                }
                _ => unreachable!(),
            }
        }

        if let (Some(sk), Some(tier_opt)) = (parsed_kind, parsed_tier) {
            if sk == SourceKind::SyntheticFixture {
                if let Some(tier) = tier_opt {
                    return Err(AddError::InvalidCell {
                        column: "authority_tier",
                        value: tier.as_str().to_owned(),
                        reason: "authority_tier must be empty for synthetic_fixture",
                    });
                }
            } else if let Some(tier) = tier_opt {
                if !source_kind_allows_tier(sk, tier) {
                    return Err(AddError::InvalidCell {
                        column: "authority_tier",
                        value: tier.as_str().to_owned(),
                        reason: "authority_tier is not allowed for source_kind",
                    });
                }
            } else {
                return Err(AddError::InvalidCell {
                    column: "authority_tier",
                    value: String::new(),
                    reason: "authority_tier is required for this source_kind",
                });
            }
        }

        Ok(Self { cells: raw.clone() })
    }

    /// Returns the validated value for `column`, if present.
    #[must_use]
    pub fn get(&self, column: &str) -> Option<&str> {
        self.cells.get(column).map(String::as_str)
    }

    /// Returns the underlying validated cell map.
    #[must_use]
    pub const fn as_map(&self) -> &BTreeMap<String, String> {
        &self.cells
    }
}

fn suggestible_static_name(key: &str) -> &'static str {
    for &candidate in &SUGGESTIBLE_FIELDS {
        if candidate == key {
            return candidate;
        }
    }
    "unknown"
}

fn apply_completed_cells(row: &mut SourceRow, completed: Option<&CompletedSourceCells>) {
    let Some(completed) = completed else {
        return;
    };
    if let Some(v) = completed.get("title") {
        row.title = v.to_owned();
    }
    if let Some(v) = completed.get("publisher") {
        row.publisher = v.to_owned();
    }
    if let Some(v) = completed.get("source_kind") {
        row.source_kind = v.to_owned();
    }
    if let Some(v) = completed.get("authority_tier") {
        row.authority_tier = v.to_owned();
    }
    if let Some(v) = completed.get("licence") {
        row.licence = v.to_owned();
    }
    if let Some(v) = completed.get("published") {
        row.published = v.to_owned();
    }
    if let Some(v) = completed.get("effective_from") {
        row.effective_from = v.to_owned();
    }
    if let Some(v) = completed.get("volatility") {
        row.volatility = v.to_owned();
    }
    if let Some(v) = completed.get("tag") {
        row.tag = v.to_owned();
    }
    if let Some(v) = completed.get("purpose") {
        row.purpose = v.to_owned();
    }
}

/// Applies only the ten suggestible cells from `completed` onto `base` (preserving all reserved
/// defaults) and serializes the single 28-column CSV row (including its trailing `\n`) through
/// the canonical CSV writer (`crate::schema::table::write`).
///
/// # Errors
///
/// Returns [`AddError`] if `base.sha256` is not a valid 64-character lowercase hex digest or
/// `base.location` is empty.
pub fn render_source_row(
    base: &SourceRow,
    completed: Option<&CompletedSourceCells>,
) -> Result<Vec<u8>, AddError> {
    if base.location.is_empty() {
        return Err(AddError::EmptyFileName);
    }
    if !is_valid_sha256_hex(&base.sha256) {
        return Err(AddError::InvalidSha256(base.sha256.clone()));
    }
    let mut row = base.clone();
    apply_completed_cells(&mut row, completed);

    let header_cells: Vec<String> = Register::Sources
        .columns()
        .iter()
        .map(|c| c.name.to_owned())
        .collect();
    let table = Table {
        header: Row {
            line: 1,
            cols: vec![1; header_cells.len()],
            cells: header_cells,
        },
        rows: vec![Row {
            line: 2,
            cols: vec![1; 28],
            cells: row.to_cells(),
        }],
    };
    let full = write_csv(&table);
    let row_line = full
        .split_once('\n')
        .map_or(full.as_str(), |(_, rest)| rest);
    Ok(row_line.as_bytes().to_vec())
}

/// Options controlling `plan_add`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddOptions {
    /// Existing [`SourceId`] when adding a new version (`--source <source-id>`).
    pub source: Option<SourceId>,
    /// Record the row with `location = "metadata-only: <file>"` without copying the original.
    pub metadata_only: bool,
}

/// Identifiers allocated for a planned source addition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddIds {
    /// The allocated or existing [`SourceId`].
    pub source_id: SourceId,
    /// The allocated [`VersionId`] (`<source-id>-v<N>`).
    pub version_id: VersionId,
}

/// Complete pure write plan for a new source or version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPlan {
    /// Allocated source and version identifiers.
    pub ids: AddIds,
    /// Ordered files to write (original first unless `--metadata-only`, `sources.csv` last).
    pub files: Vec<WriteFile>,
    /// Complete replacement bytes for `corpus/registers/sources.csv`.
    pub sources_csv: Vec<u8>,
}

/// Outcome of [`plan_add`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddOutcome {
    /// A row in `sources.csv` already carries the input file's `sha256`.
    Present(VersionId),
    /// The file is not yet in `sources.csv`; carries the complete [`AddPlan`].
    Planned(AddPlan),
}

/// Errors returned by [`plan_add`], [`CompletedSourceCells::parse`], and [`render_source_row`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddError {
    /// Identifier capacity exhausted (e.g. `S-9999` reached).
    Exhausted(IdError),
    /// `--source` named a [`SourceId`] that does not exist in `sources.csv`.
    UnknownSource(SourceId),
    /// The supplied file name is empty.
    EmptyFileName,
    /// The supplied SHA-256 string is not 64 lowercase hexadecimal characters.
    InvalidSha256(String),
    /// `corpus/registers/sources.csv` is missing or failed table parsing in [`Model`].
    MissingSourcesRegister,
    /// A reserved `sources.csv` column was supplied to [`CompletedSourceCells::parse`].
    ReservedField(String),
    /// An unknown column name was supplied to [`CompletedSourceCells::parse`].
    UnknownField(String),
    /// A cell in [`CompletedSourceCells::parse`] violated its column rule.
    InvalidCell {
        /// Offending column name.
        column: &'static str,
        /// Rejected value.
        value: String,
        /// Human-readable reason.
        reason: &'static str,
    },
}

impl AddError {
    /// Returns `true` when this error represents identifier space exhaustion.
    #[must_use]
    pub const fn is_exhausted(&self) -> bool {
        matches!(self, Self::Exhausted(_))
    }
}

impl fmt::Display for AddError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted(err) => write!(f, "{} id space exhausted", err.form()),
            Self::UnknownSource(id) => write!(f, "unknown source `{id}` in sources.csv"),
            Self::EmptyFileName => f.write_str("file name must not be empty"),
            Self::InvalidSha256(s) => write!(f, "invalid sha256 digest `{s}`"),
            Self::MissingSourcesRegister => {
                f.write_str("corpus/registers/sources.csv is missing or unreadable")
            }
            Self::ReservedField(col) => {
                write!(f, "reserved source column `{col}` cannot be overridden")
            }
            Self::UnknownField(col) => write!(f, "unknown source column `{col}`"),
            Self::InvalidCell {
                column,
                value,
                reason,
            } => write!(f, "invalid `{column}` value `{value}`: {reason}"),
        }
    }
}

impl std::error::Error for AddError {}

fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Returns `true` when `source_id` appears in column 1 (`source_id`) of `sources.csv` in `model`.
#[must_use]
pub fn source_exists(model: &Model, source_id: &SourceId) -> bool {
    model
        .register_rows(Register::Sources)
        .iter()
        .any(|row| row.cells.first().map(String::as_str) == Some(source_id.as_str()))
}

/// Computes the stored original file name under `corpus/sources/public/`:
/// `<version-id>.<lowercase-ext>` when `file_name` has a 1..=5 character ASCII alphanumeric
/// extension, or `<version-id>.bin` otherwise so the stored filename always satisfies `FB0123`.
#[must_use]
pub fn stored_original_name(version_id: &VersionId, file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    if let Some((stem, ext)) = base.rsplit_once('.')
        && !stem.is_empty()
        && (1..=5).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        format!("{version_id}.{}", ext.to_ascii_lowercase())
    } else {
        format!("{version_id}.bin")
    }
}

/// Plans an addition of `file_name` with content digest `sha256` into `model`.
///
/// - Returns `Ok(AddOutcome::Present(version_id))` when `sha256` is already present in
///   `sources.csv`.
/// - Otherwise allocates the next version of `options.source` (or the next free `SourceId` with
///   version `1`), builds the new row with `handling = "public"`, serializes the complete
///   replacement `sources.csv` through `schema::table::write`, and returns `Ok(AddOutcome::Planned)`.
///
/// # Errors
///
/// Returns [`AddError`] if `file_name` is empty, `sha256` is malformed, `sources.csv` is absent,
/// `options.source` does not exist in `sources.csv`, or the `SourceId` sequence is exhausted.
pub fn plan_add(
    model: &Model,
    file_name: &str,
    sha256: &str,
    options: &AddOptions,
    completed: Option<&CompletedSourceCells>,
    today: CivilDate,
) -> Result<AddOutcome, AddError> {
    if file_name.is_empty() {
        return Err(AddError::EmptyFileName);
    }
    if !is_valid_sha256_hex(sha256) {
        return Err(AddError::InvalidSha256(sha256.to_owned()));
    }
    let table = model
        .register(Register::Sources)
        .ok_or(AddError::MissingSourcesRegister)?;

    for row in &table.rows {
        let row_sha = row.cells.get(12).map_or("", String::as_str);
        if row_sha == sha256 {
            let src_raw = row.cells.first().map_or("", String::as_str);
            let ver_raw = row.cells.get(1).map_or("", String::as_str);
            let version_id = VersionId::parse(&format!("{src_raw}-v{ver_raw}")).map_err(|_| {
                AddError::InvalidCell {
                    column: "version",
                    value: format!("{src_raw}-v{ver_raw}"),
                    reason: "existing row has invalid version id",
                }
            })?;
            return Ok(AddOutcome::Present(version_id));
        }
    }

    let (source_id, version_num) = if let Some(ref target_source) = options.source {
        let mut max_ver = 0u32;
        for row in &table.rows {
            if row.cells.first().map(String::as_str) == Some(target_source.as_str())
                && let Some(ver_str) = row.cells.get(1)
                && let Ok(v) = ver_str.parse::<u32>()
                && v > max_ver
            {
                max_ver = v;
            }
        }
        if max_ver == 0 {
            return Err(AddError::UnknownSource(target_source.clone()));
        }
        let next_ver = max_ver.checked_add(1).ok_or_else(|| {
            AddError::Exhausted(IdError::overflow(
                "source_version",
                format!("{target_source}-v{max_ver}"),
            ))
        })?;
        (target_source.clone(), next_ver)
    } else {
        let existing_ids = table
            .rows
            .iter()
            .filter_map(|r| r.cells.first().map(String::as_str))
            .filter(|s| !s.is_empty());
        let next_src = match next_source_id(existing_ids) {
            Ok(id) => id,
            Err(err) if err.is_exhausted() => return Err(AddError::Exhausted(err)),
            Err(_) => {
                // Fall back to scanning valid SourceIds if a malformed cell exists in the table.
                let mut max_num = 0u32;
                let mut max_raw = String::new();
                for r in &table.rows {
                    if let Some(cell) = r.cells.first()
                        && let Ok(parsed) = SourceId::parse(cell)
                        && parsed.number() > max_num
                    {
                        max_num = parsed.number();
                        max_raw = parsed.to_string();
                    }
                }
                if max_num >= MAX_SOURCE_NUMBER {
                    let form = FAMILIES
                        .iter()
                        .find(|f| f.name == "source")
                        .map_or("source", |f| f.name);
                    return Err(AddError::Exhausted(IdError {
                        form,
                        input: max_raw,
                        kind: IdErrorKind::Exhausted,
                    }));
                }
                let next_num = max_num + 1;
                SourceId::parse(&format!("S-{next_num:04}")).map_err(AddError::Exhausted)?
            }
        };
        (next_src, 1)
    };

    let version_id =
        VersionId::parse(&format!("{source_id}-v{version_num}")).map_err(AddError::Exhausted)?;

    let plan = build_plan_from_table(
        table, file_name, sha256, options, completed, source_id, version_id, today,
    )?;
    Ok(AddOutcome::Planned(plan))
}

/// Reconstructs the deterministic [`AddPlan`] for a pending recovery using the stored
/// `(allocated_source, allocated_version, planned_on)` from `OperationIntent`.
///
/// If `sources.csv` in `model` already ends with the exact row written by this transaction
/// (when a crash occurred after writing `sources.csv` and before `write.end` completed),
/// strips that trailing row before re-rendering so the resulting `sources_csv` bytes and
/// `WriteFile` hashes match the original transaction plan.
///
/// # Errors
///
/// Returns [`AddError`] if `sources.csv` is missing or the row cannot be rendered.
#[expect(
    clippy::too_many_arguments,
    reason = "recovery replanning binds the full original request and stored intent allocations"
)]
pub fn plan_add_with_allocation(
    model: &Model,
    file_name: &str,
    sha256: &str,
    options: &AddOptions,
    completed: Option<&CompletedSourceCells>,
    allocated_source: &SourceId,
    allocated_version: &VersionId,
    planned_on: CivilDate,
) -> Result<AddPlan, AddError> {
    if file_name.is_empty() {
        return Err(AddError::EmptyFileName);
    }
    if !is_valid_sha256_hex(sha256) {
        return Err(AddError::InvalidSha256(sha256.to_owned()));
    }
    let table = model
        .register(Register::Sources)
        .ok_or(AddError::MissingSourcesRegister)?;

    let location = if options.metadata_only {
        format!("metadata-only: {file_name}")
    } else {
        file_name.to_owned()
    };
    let mut expected_row = SourceRow::new(
        allocated_source.clone(),
        allocated_version.version(),
        location,
        sha256,
        planned_on,
    );
    apply_completed_cells(&mut expected_row, completed);
    let expected_cells = expected_row.to_cells();

    let mut base_table = table.clone();
    if base_table
        .rows
        .last()
        .is_some_and(|last| last.cells == expected_cells)
    {
        base_table.rows.pop();
    }

    build_plan_from_table(
        &base_table,
        file_name,
        sha256,
        options,
        completed,
        allocated_source.clone(),
        allocated_version.clone(),
        planned_on,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "internal helper assembling AddPlan from validated components"
)]
fn build_plan_from_table(
    table: &Table,
    file_name: &str,
    sha256: &str,
    options: &AddOptions,
    completed: Option<&CompletedSourceCells>,
    source_id: SourceId,
    version_id: VersionId,
    today: CivilDate,
) -> Result<AddPlan, AddError> {
    let location = if options.metadata_only {
        format!("metadata-only: {file_name}")
    } else {
        file_name.to_owned()
    };

    let base_row = SourceRow::new(
        source_id.clone(),
        version_id.version(),
        location,
        sha256,
        today,
    );
    let _rendered_row = render_source_row(&base_row, completed)?;

    let mut updated_row = base_row;
    apply_completed_cells(&mut updated_row, completed);

    let mut new_table = table.clone();
    let next_line = new_table
        .rows
        .last()
        .map_or(2, |r| r.line.saturating_add(1));
    new_table.rows.push(Row {
        line: next_line,
        cols: vec![1; 28],
        cells: updated_row.to_cells(),
    });

    let sources_csv = write_csv(&new_table).into_bytes();
    let sources_csv_sha = Sha256::digest(&sources_csv).as_str().to_owned();

    let mut files = Vec::new();
    if !options.metadata_only {
        let stored_name = stored_original_name(&version_id, file_name);
        files.push(WriteFile::new(
            format!("corpus/sources/public/{stored_name}"),
            sha256.to_owned(),
        ));
    }
    files.push(WriteFile::new(
        "corpus/registers/sources.csv",
        sources_csv_sha,
    ));

    Ok(AddPlan {
        ids: AddIds {
            source_id,
            version_id,
        },
        files,
        sources_csv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_cells_parse_and_preserve_reserved_defaults() {
        let src_id = SourceId::parse("S-0003").unwrap_or_else(|e| panic!("{e}"));
        let today = CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("{e}"));
        let sha = "a".repeat(64);
        let base = SourceRow::new(src_id, 1, "note.txt", &sha, today);

        let direct = render_source_row(&base, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            String::from_utf8(direct.clone()).unwrap_or_else(|e| panic!("{e}")),
            format!("S-0003,1,,,,,,,,,,note.txt,{sha},,,,2026-03-01,active,,,public,,,,,,,\n")
        );

        let empty_completed =
            CompletedSourceCells::parse(&BTreeMap::new()).unwrap_or_else(|e| panic!("{e}"));
        let with_empty =
            render_source_row(&base, Some(&empty_completed)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(direct, with_empty);

        let mut map = BTreeMap::new();
        map.insert("title".to_owned(), "Acquisition Guide".to_owned());
        map.insert("publisher".to_owned(), "DoD".to_owned());
        map.insert("source_kind".to_owned(), "standard_or_guidance".to_owned());
        map.insert("authority_tier".to_owned(), "A".to_owned());
        map.insert("licence".to_owned(), "public-domain".to_owned());
        map.insert("published".to_owned(), "2025-10".to_owned());
        map.insert("effective_from".to_owned(), "2025-11-01".to_owned());
        map.insert("volatility".to_owned(), "V1".to_owned());
        map.insert("tag".to_owned(), "acq-guide".to_owned());
        map.insert("purpose".to_owned(), "reference".to_owned());

        let completed = CompletedSourceCells::parse(&map).unwrap_or_else(|e| panic!("{e}"));
        let rendered = render_source_row(&base, Some(&completed)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            String::from_utf8(rendered).unwrap_or_else(|e| panic!("{e}")),
            format!(
                "S-0003,1,acq-guide,Acquisition Guide,DoD,standard_or_guidance,,A,public-domain,,,note.txt,{sha},2025-10,2025-11-01,,2026-03-01,active,,V1,public,,reference,,,,,\n"
            )
        );
    }

    #[test]
    fn completed_cells_reject_reserved_and_invalid_fields() {
        for &reserved in &RESERVED_SOURCE_FIELDS {
            let mut map = BTreeMap::new();
            map.insert(reserved.to_owned(), "override".to_owned());
            let err = CompletedSourceCells::parse(&map).unwrap_err();
            assert_eq!(err, AddError::ReservedField(reserved.to_owned()));
        }

        let mut unknown = BTreeMap::new();
        unknown.insert("not_a_column".to_owned(), "val".to_owned());
        assert_eq!(
            CompletedSourceCells::parse(&unknown).unwrap_err(),
            AddError::UnknownField("not_a_column".to_owned())
        );

        let mut bad_vocab = BTreeMap::new();
        bad_vocab.insert("source_kind".to_owned(), "vendor_quote".to_owned());
        assert!(matches!(
            CompletedSourceCells::parse(&bad_vocab).unwrap_err(),
            AddError::InvalidCell {
                column: "source_kind",
                ..
            }
        ));
    }
}
