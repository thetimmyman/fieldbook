//! Execution contexts for deterministic and AI verbs (`ai-kernel`, AD-01).
//!
//! [`Ctx`] is what every deterministic verb receives; [`AiCtx`] is what the AI
//! verbs receive. The model type parameter is filled in by a later card.
//!
//! No prompt or model wait retains a write token or store lease (the [`Journal`]
//! here is idle between operations).

use fieldbook_core::check::Tree;

use crate::World;
use crate::io::journal::Journal;

/// Execution context supplied to every deterministic verb.
///
/// Holds references to the [`World`], an idle [`Journal`] handle between
/// operations, and the in-memory Corpus [`Tree`].
pub struct Ctx<'a> {
    /// World environment seam for filesystem, clock, and process boundaries.
    pub io: &'a dyn World,
    /// Idle journal handle between operations (holds no lease).
    pub journal: Journal<'a>,
    /// In-memory Corpus tree.
    pub corpus: &'a Tree,
}

impl<'a> Ctx<'a> {
    /// Construct a new deterministic verb execution context.
    #[must_use]
    pub const fn new(io: &'a dyn World, journal: Journal<'a>, corpus: &'a Tree) -> Self {
        Self {
            io,
            journal,
            corpus,
        }
    }
}

/// Execution context supplied to AI verbs.
///
/// Wraps a deterministic [`Ctx`] and a reference to the model port implementation.
pub struct AiCtx<'a, M: ?Sized> {
    /// Underlying deterministic context.
    pub ctx: Ctx<'a>,
    /// Model port adapter reference.
    pub model: &'a M,
}

impl<'a, M: ?Sized> AiCtx<'a, M> {
    /// Construct a new AI verb execution context.
    #[must_use]
    pub const fn new(ctx: Ctx<'a>, model: &'a M) -> Self {
        Self { ctx, model }
    }
}
