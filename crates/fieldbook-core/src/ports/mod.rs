//! The pure ports module: model request shapes, templates, schemas, and provenance.

use std::fmt;

use serde::{Deserialize, Serialize};

pub mod chat;
pub mod index;
pub mod provenance;
pub mod request;
pub mod schema;
pub mod templates;

pub use chat::{AdapterStatus, ConnState, KeyState, LastCall};
pub use provenance::{
    Fb0917, HandoffId, HandoffIdError, HandoffTicket, ProvenanceRef, verify_provenance,
};
pub use request::{Limits, ModelRequest, Part, PartKind, Purpose, RequestError};
pub use schema::{Field, Schema};
pub use templates::{TEMPLATES, Template, TemplateDef, UnknownTemplate};

/// Closed enumeration of architectural ports (`AD-01`, `ADR 0004 §11`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Port {
    /// Model invocation port.
    Model,
    /// Automated decision port.
    Decide,
    /// Search and retrieval index port.
    Index,
    /// Edition and report output port.
    Output,
    /// Document text extractor port.
    Extractor,
    /// Content classifier port.
    Classifier,
    /// Content correlation port.
    Correlator,
    /// Store admission gate port.
    Admit,
    /// Snapshot backup target port.
    Backup,
    /// External data pull port.
    Pull,
    /// Invariant register port.
    Register,
    /// Authorisation and visibility port.
    Authz,
    /// Data storage root port.
    Store,
}

impl Port {
    /// All thirteen architectural ports in canonical order.
    pub const ALL: [Self; 13] = [
        Self::Model,
        Self::Decide,
        Self::Index,
        Self::Output,
        Self::Extractor,
        Self::Classifier,
        Self::Correlator,
        Self::Admit,
        Self::Backup,
        Self::Pull,
        Self::Register,
        Self::Authz,
        Self::Store,
    ];

    /// Returns the canonical lowercase port identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Decide => "decide",
            Self::Index => "index",
            Self::Output => "output",
            Self::Extractor => "extractor",
            Self::Classifier => "classifier",
            Self::Correlator => "correlator",
            Self::Admit => "admit",
            Self::Backup => "backup",
            Self::Pull => "pull",
            Self::Register => "register",
            Self::Authz => "authz",
            Self::Store => "store",
        }
    }
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Capabilities declared by an adapter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    /// Whether the adapter requires network access.
    pub network: bool,
    /// Whether the adapter supports structured output schemas.
    pub structured: bool,
    /// Whether the adapter provides text embeddings.
    pub embeddings: bool,
    /// Whether the adapter supports streaming responses.
    pub streaming: bool,
    /// Maximum prompt characters supported by the adapter.
    pub max_prompt_chars: u32,
}

impl Caps {
    /// Constructs a new [`Caps`].
    #[must_use]
    pub const fn new(
        network: bool,
        structured: bool,
        embeddings: bool,
        streaming: bool,
        max_prompt_chars: u32,
    ) -> Self {
        Self {
            network,
            structured,
            embeddings,
            streaming,
            max_prompt_chars,
        }
    }
}
