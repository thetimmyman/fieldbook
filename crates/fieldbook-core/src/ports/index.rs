//! Search index port types and trait (`AD-01`, `pipeline-ports`).
//!
//! **Responsibility.** Define the data model (`Scope`, `Query`, `Tier`, `Hit`, `Hits`,
//! `IndexError`) and [`IndexPort`] trait separating search callers (such as [`crate::pipeline::Correlator`])
//! from concrete index implementations.
//!
//! **Rules.** Pure types and trait only; no implementation or I/O here.

use std::fmt;

use crate::check::Code;
use crate::search::units::lookup_unit_kind;

/// One indexed unit reference returned in a [`Hit`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitRef {
    /// Unit identifier in byte order (e.g. `S-0001-v1`, `Q-001`, `cmp-page#section`).
    pub id: String,
    /// Unit kind from [`crate::search::units::UNIT_KINDS`] (`page`, `clause`, `source`,
    /// `claim`, `question`, `conflict`, `term`, `record`, `text`).
    pub kind: String,
}

impl UnitRef {
    /// Construct a [`UnitRef`] with `id` and `kind`.
    #[must_use]
    pub fn new(id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
        }
    }
}

/// Filter selecting which [`crate::search::units::UNIT_KINDS`] a query matches.
///
/// An empty list matches every unit kind.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scope {
    kinds: Vec<String>,
}

impl Scope {
    /// Construct a [`Scope`] that matches every unit kind.
    #[must_use]
    pub const fn all() -> Self {
        Self { kinds: Vec::new() }
    }

    /// Construct a [`Scope`] restricted to `kinds`, validating every entry against
    /// [`crate::search::units::UNIT_KINDS`].
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] when any kind is not registered in [`crate::search::units::UNIT_KINDS`].
    pub fn new<I, S>(kinds: I) -> Result<Self, IndexError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut validated = Vec::new();
        for item in kinds {
            let kind = item.into();
            if lookup_unit_kind(&kind).is_none() {
                return Err(IndexError::new(
                    Code::Fb0930,
                    format!("unknown document unit kind `{kind}`"),
                ));
            }
            if !validated.contains(&kind) {
                validated.push(kind);
            }
        }
        Ok(Self { kinds: validated })
    }

    /// Returns `true` when the scope is unrestricted (empty).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// Returns the restricted unit kinds in declaration order.
    #[must_use]
    pub fn kinds(&self) -> &[String] {
        &self.kinds
    }

    /// Returns `true` when `kind` is included by this [`Scope`].
    #[must_use]
    pub fn allows(&self, kind: &str) -> bool {
        self.kinds.is_empty() || self.kinds.iter().any(|k| k == kind)
    }
}

/// Input parameters for an [`IndexPort::query`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    /// Raw query string passed to the index.
    pub text: String,
    /// Unit kind filter; empty means all kinds.
    pub scope: Scope,
    /// Maximum number of hits to return.
    pub limit: usize,
}

impl Query {
    /// Construct a [`Query`].
    #[must_use]
    pub fn new(text: impl Into<String>, scope: Scope, limit: usize) -> Self {
        Self {
            text: text.into(),
            scope,
            limit,
        }
    }
}

/// Match class tier of an index [`Hit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// Exact identifier or key match.
    Exact,
    /// Ranked term/BM25 match.
    Ranked,
}

/// One hit returned by [`IndexPort::query`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Matched unit identifier and kind.
    pub unit: UnitRef,
    /// Summed integer score / impact.
    pub score: u32,
    /// Match tier (`Exact` or `Ranked`).
    pub tier: Tier,
}

/// Ranked hit list returned by [`IndexPort::query`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hits {
    /// Ordered hits truncated to `Query::limit`.
    pub hits: Vec<Hit>,
    /// Total number of matching units before truncation.
    pub total: usize,
}

/// Typed error returned by an [`IndexPort`] implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexError {
    /// Diagnostic code associated with the failure.
    pub code: Code,
    /// Human-readable error cause.
    pub message: String,
}

impl IndexError {
    /// Construct an [`IndexError`].
    #[must_use]
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.id(), self.message)
    }
}

impl std::error::Error for IndexError {}

/// Search index port shared by correlators and retrieval callers.
pub trait IndexPort {
    /// Adapter registry identifier (e.g. `bm25f@1`).
    fn id(&self) -> &'static str;

    /// Execute `q` against this index.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] when the index backend cannot serve `q`.
    fn query(&self, q: &Query) -> Result<Hits, IndexError>;
}
