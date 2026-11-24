//! Central adapter registry for all architectural ports (`AD-01`, `ADR 0004 §11`).
//!
//! **Responsibility.** Holds [`ADAPTERS`], the single authoritative table of all built and planned
//! port adapters. Each row declares an adapter's port, id, crate, requirements, data visibility,
//! fixtures, contract suite, and documentation anchor.

pub use fieldbook_core::ports::{Caps, Port};

/// Implementation lifecycle state of an adapter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum State {
    /// Adapter is implemented and tested.
    Built,
    /// Adapter is planned for a future card.
    Later,
}

impl State {
    /// Returns the lowercase string identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Built => "built",
            Self::Later => "later",
        }
    }
}

/// Build profile required for an adapter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Build {
    /// Adapter is included in default silent offline builds.
    Silent,
    /// Adapter requires the optional AI build.
    Ai,
}

impl Build {
    /// Returns the lowercase string identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Silent => "silent",
            Self::Ai => "ai",
        }
    }
}

/// External system resources required by an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Needs {
    /// Adapter requires no external resources (pure compute / memory).
    None,
    /// Adapter requires local filesystem access.
    Files,
    /// Adapter spawns an external host process.
    Process,
    /// Adapter requires outbound network access.
    Net,
}

impl Needs {
    /// Returns the lowercase string identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Files => "files",
            Self::Process => "process",
            Self::Net => "net",
        }
    }
}

/// Data visibility and label sensitivity permitted to an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sees {
    /// Adapter only sees text or data labelled `public`.
    Public,
    /// Adapter may receive labelled views or originals within permitted policy scope.
    Labelled,
}

impl Sees {
    /// Returns the lowercase string identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Labelled => "labelled",
        }
    }
}

/// One registered port adapter entry in [`ADAPTERS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The architectural port this adapter serves.
    pub port: Port,
    /// The unique adapter identifier (`<name>@<n>`).
    pub id: &'static str,
    /// Whether this adapter is the default implementation for the port.
    pub default: bool,
    /// Lifecycle implementation state.
    pub state: State,
    /// Build profile requirement.
    pub build: Build,
    /// The crate holding the implementation.
    pub krate: &'static str,
    /// External system capabilities required.
    pub needs: Needs,
    /// Data sensitivity scope visible to the adapter.
    pub sees: Sees,
    /// Whether the adapter invokes a model.
    pub calls_model: bool,
    /// Settings keys read by this adapter.
    pub settings_keys: &'static [&'static str],
    /// Repository-relative path to the fixture directory.
    pub fixtures: &'static str,
    /// Name of the contract test suite verifying this adapter.
    pub suite: &'static str,
    /// Heading anchor in `docs/reference/ports.md`.
    pub doc: &'static str,
}

impl Row {
    /// Returns the runtime capabilities declared for this adapter row.
    #[must_use]
    pub const fn caps(&self) -> Caps {
        Caps {
            network: matches!(self.needs, Needs::Net),
            structured: false,
            embeddings: false,
            streaming: false,
            max_prompt_chars: 0,
        }
    }

    /// Returns the canonical contract test name for this row.
    #[must_use]
    pub fn contract_name(&self) -> String {
        contract_name(self.port, self.id)
    }
}

/// Returns the canonical contract test name for a port and adapter id.
///
/// Formatted as `contract_<port>_<id>` where every character in `id` outside `[a-z0-9]` is replaced by `_`.
#[must_use]
pub fn contract_name(port: Port, id: &str) -> String {
    let sanitized: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("contract_{}_{sanitized}", port.as_str())
}

/// The single static registry of architectural port adapters.
pub const ADAPTERS: &[Row] = &[
    Row {
        port: Port::Extractor,
        id: "plain@1",
        default: true,
        state: State::Built,
        build: Build::Silent,
        krate: "fieldbook-core",
        needs: Needs::None,
        sees: Sees::Labelled,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/extractor/plain@1/",
        suite: "extractor_contract",
        doc: "extractor-plain1",
    },
    Row {
        port: Port::Extractor,
        id: "html-strip@1",
        default: false,
        state: State::Built,
        build: Build::Silent,
        krate: "fieldbook-core",
        needs: Needs::None,
        sees: Sees::Labelled,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/extractor/html-strip@1/",
        suite: "extractor_contract",
        doc: "extractor-html-strip1",
    },
    Row {
        port: Port::Extractor,
        id: "pdftotext-layout@1",
        default: false,
        state: State::Built,
        build: Build::Silent,
        krate: "xtask",
        needs: Needs::Process,
        sees: Sees::Public,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/extractor/pdftotext-layout@1/",
        suite: "extractor_contract",
        doc: "extractor-pdftotext-layout1",
    },
    Row {
        port: Port::Extractor,
        id: "staged@1",
        default: false,
        state: State::Built,
        build: Build::Silent,
        krate: env!("CARGO_PKG_NAME"),
        needs: Needs::Files,
        sees: Sees::Public,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/extractor/staged@1/",
        suite: "extractor_contract",
        doc: "extractor-staged1",
    },
    Row {
        port: Port::Admit,
        id: "corpus-add@1",
        default: true,
        state: State::Built,
        build: Build::Silent,
        krate: "fieldbook-core",
        needs: Needs::None,
        sees: Sees::Public,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/admit/corpus-add@1/",
        suite: "admit_suite",
        doc: "admit-corpus-add1",
    },
    Row {
        port: Port::Classifier,
        id: "rules@1",
        default: true,
        state: State::Built,
        build: Build::Silent,
        krate: "fieldbook-core",
        needs: Needs::None,
        sees: Sees::Labelled,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/classifier/rules@1/",
        suite: "classifier_suite",
        doc: "classifier-rules1",
    },
    Row {
        port: Port::Backup,
        id: "folder@1",
        default: true,
        state: State::Built,
        build: Build::Silent,
        krate: env!("CARGO_PKG_NAME"),
        needs: Needs::Files,
        sees: Sees::Labelled,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/backup/folder@1/",
        suite: "backup_target_contract",
        doc: "backup-folder1",
    },
    Row {
        port: Port::Store,
        id: "folder@1",
        default: true,
        state: State::Built,
        build: Build::Silent,
        krate: env!("CARGO_PKG_NAME"),
        needs: Needs::Files,
        sees: Sees::Labelled,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/store/folder@1/",
        suite: "store_suite",
        doc: "store-folder1",
    },
    Row {
        port: Port::Authz,
        id: "policy-readers@1",
        default: true,
        state: State::Built,
        build: Build::Silent,
        krate: "fieldbook-core",
        needs: Needs::None,
        sees: Sees::Public,
        calls_model: false,
        settings_keys: &[],
        fixtures: "tests/fixtures/ports/authz/policy-readers@1/",
        suite: "authz_suite",
        doc: "authz-policy-readers1",
    },
];
