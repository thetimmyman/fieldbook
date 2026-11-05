//! Outbound destination kinds (`Dest`, `DEST_KINDS`, `DestSpec`, `RowSource`, `RowBind`,
//! `docs/reference/policy.md#destination-kinds`).

use crate::audit::DestKind;

/// Whether a destination kind's rule comes from a `[[destination]]` policy row or is fixed in code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowSource {
    /// Configured by a `[[destination]]` row in `policy.toml` (or the floor table).
    PolicyRow,
    /// Fixed in code (`audit`, `git`, `edition`).
    Fixed,
}

/// What binds a destination row to a specific target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowBind {
    /// Bound by endpoint `name` and `url == base_url` (`model`).
    NameAndUrl,
    /// Bound by optional endpoint `name` (`handoff`).
    OptionalName,
    /// Bound by optional `clients` SHA-256 hash list (`mcp`).
    ClientList,
    /// Bound by canonical `paths` prefix (`export`, `backup`).
    PathPrefix,
    /// Bound by the destination kind alone (`bridge`, `print`, `clipboard`, and fixed rows).
    Nothing,
}

/// Metadata for one destination kind in [`DEST_KINDS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestSpec {
    /// The destination kind.
    pub kind: DestKind,
    /// Whether the rule comes from a policy row or is fixed in code.
    pub source: RowSource,
    /// Whether this kind is advisory (`handoff`, `print`, `clipboard`).
    pub advisory: bool,
    /// `Some(true)` if the left-out record count is shown (`handoff`, `export`, `print`),
    /// `Some(false)` if the fixed line without a count is shown (`model`, `mcp`, `bridge`,
    /// `clipboard`, `edition`), or `None` (`n/a` in the reference table: `backup`, `audit`, `git`).
    pub shows_left_out_count: Option<bool>,
    /// What binds the row.
    pub bind: RowBind,
    /// Token-taking API that sends to this destination.
    pub token_api: &'static str,
}

impl DestSpec {
    /// Returns `true` when `shows_left_out_count == Some(true)`.
    #[must_use]
    pub const fn shows_count(&self) -> bool {
        matches!(self.shows_left_out_count, Some(true))
    }
}

/// The single authoritative table of destination kinds (`docs/reference/policy.md#destination-kinds`).
pub const DEST_KINDS: &[DestSpec] = &[
    DestSpec {
        kind: DestKind::Model,
        source: RowSource::PolicyRow,
        advisory: false,
        shows_left_out_count: Some(false),
        bind: RowBind::NameAndUrl,
        token_api: "ai::model::send",
    },
    DestSpec {
        kind: DestKind::Handoff,
        source: RowSource::PolicyRow,
        advisory: true,
        shows_left_out_count: Some(true),
        bind: RowBind::OptionalName,
        token_api: "render::pack",
    },
    DestSpec {
        kind: DestKind::Mcp,
        source: RowSource::PolicyRow,
        advisory: false,
        shows_left_out_count: Some(false),
        bind: RowBind::ClientList,
        token_api: "mcp::reply",
    },
    DestSpec {
        kind: DestKind::Bridge,
        source: RowSource::PolicyRow,
        advisory: false,
        shows_left_out_count: Some(false),
        bind: RowBind::Nothing,
        token_api: "chat::serve",
    },
    DestSpec {
        kind: DestKind::Export,
        source: RowSource::PolicyRow,
        advisory: false,
        shows_left_out_count: Some(true),
        bind: RowBind::PathPrefix,
        token_api: "io::atomic::write_outside",
    },
    DestSpec {
        kind: DestKind::Backup,
        source: RowSource::PolicyRow,
        advisory: false,
        shows_left_out_count: None,
        bind: RowBind::PathPrefix,
        token_api: "io::atomic::write_outside",
    },
    DestSpec {
        kind: DestKind::Print,
        source: RowSource::PolicyRow,
        advisory: true,
        shows_left_out_count: Some(true),
        bind: RowBind::Nothing,
        token_api: "render::print",
    },
    DestSpec {
        kind: DestKind::Clipboard,
        source: RowSource::PolicyRow,
        advisory: true,
        shows_left_out_count: Some(false),
        bind: RowBind::Nothing,
        token_api: "render::clipboard",
    },
    DestSpec {
        kind: DestKind::Audit,
        source: RowSource::Fixed,
        advisory: false,
        shows_left_out_count: None,
        bind: RowBind::Nothing,
        token_api: "io::atomic::write_outside",
    },
    DestSpec {
        kind: DestKind::Git,
        source: RowSource::Fixed,
        advisory: false,
        shows_left_out_count: None,
        bind: RowBind::Nothing,
        token_api: "labels::git_guard",
    },
    DestSpec {
        kind: DestKind::Edition,
        source: RowSource::Fixed,
        advisory: false,
        shows_left_out_count: Some(false),
        bind: RowBind::Nothing,
        token_api: "edition::build",
    },
];

/// Concrete destination value passed to [`crate::egress::check`], [`crate::egress::covers`],
/// and [`crate::policy::EffectivePolicy::allow_set`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dest {
    /// An HTTPS or loopback model endpoint named in `settings.toml`.
    Model {
        /// Endpoint name in `settings.toml`.
        name: String,
        /// Endpoint `base_url` in `settings.toml`.
        base_url: String,
    },
    /// A context pack file the owner carries to an approved assistant.
    Handoff,
    /// Results returned over `fieldbook mcp`.
    Mcp {
        /// Lowercase 64-hex SHA-256 of the client's self-declared name.
        client: String,
    },
    /// The local chat Bridge listener on the loopback address.
    Bridge,
    /// Any file written outside the data root (`export`).
    Export {
        /// Canonical target path.
        path: String,
    },
    /// A backup snapshot target (`backup`).
    Backup {
        /// Canonical target path.
        path: String,
        /// Whether the target has `company_storage = true` in `settings.toml` (`false` for one-off `--to`).
        company_storage: bool,
    },
    /// The print view of a page.
    Print,
    /// Copy controls in rendered pages.
    Clipboard,
    /// The audit journal export sink (`audit export`).
    Audit,
    /// A source repository work tree (`git`).
    Git,
    /// The public edition that ships (`edition`).
    Edition,
}

impl Dest {
    /// Returns the [`DestKind`] of this destination.
    #[must_use]
    pub const fn kind(&self) -> DestKind {
        match self {
            Self::Model { .. } => DestKind::Model,
            Self::Handoff => DestKind::Handoff,
            Self::Mcp { .. } => DestKind::Mcp,
            Self::Bridge => DestKind::Bridge,
            Self::Export { .. } => DestKind::Export,
            Self::Backup { .. } => DestKind::Backup,
            Self::Print => DestKind::Print,
            Self::Clipboard => DestKind::Clipboard,
            Self::Audit => DestKind::Audit,
            Self::Git => DestKind::Git,
            Self::Edition => DestKind::Edition,
        }
    }

    /// Looks up the [`DestSpec`] in [`DEST_KINDS`] for this destination.
    #[must_use]
    pub const fn spec(&self) -> &'static DestSpec {
        let idx = match self {
            Self::Model { .. } => 0,
            Self::Handoff => 1,
            Self::Mcp { .. } => 2,
            Self::Bridge => 3,
            Self::Export { .. } => 4,
            Self::Backup { .. } => 5,
            Self::Print => 6,
            Self::Clipboard => 7,
            Self::Audit => 8,
            Self::Git => 9,
            Self::Edition => 10,
        };
        &DEST_KINDS[idx]
    }

    /// Returns `true` when this destination kind shows the count of omitted records
    /// (`handoff`, `export`, and `print`).
    #[must_use]
    pub fn shows_left_out_count(&self) -> bool {
        self.spec().shows_count()
    }
}
