//! Closed registry of intake proposal write targets (`pipeline-ports`).

/// Kind of store target written by an admitted proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetKind {
    /// A row and files in `sources.csv` / `corpus/sources/`.
    Source,
    /// Structured record batch target (added by `r2-records`).
    Record,
    /// Dataset target.
    Dataset,
}

/// Identifier key shape tracked for deduplication and correlation on a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyShape {
    /// 64-hex lowercase SHA-256 original file digest.
    Sha256,
    /// Source group identifier (`S-nnnn`).
    SourceId,
    /// Source version identifier (`S-nnnn-vN`).
    VersionId,
}

/// Static declaration of one proposal write target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// Stable lowercase identifier (`"sources"`, ...).
    pub id: &'static str,
    /// Architectural target category.
    pub kind: TargetKind,
    /// Key shapes tracked by this target.
    pub keys: &'static [KeyShape],
    /// Register column names that `check` requires to be filled for rows written here.
    pub required: &'static [&'static str],
}

/// Single authoritative table of every proposal intake target.
pub const TARGETS: &[Target] = &[Target {
    id: "sources",
    kind: TargetKind::Source,
    keys: &[KeyShape::Sha256, KeyShape::SourceId, KeyShape::VersionId],
    required: &[
        "source_id",
        "version",
        "title",
        "publisher",
        "source_kind",
        "source_role",
        "licence",
        "location",
        "sha256",
        "as_of",
        "retrieved",
        "lifecycle",
        "volatility",
        "handling",
    ],
}];
