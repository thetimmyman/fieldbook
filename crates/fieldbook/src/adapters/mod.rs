//! Architectural port adapters and adapter registry (`AD-01`, `ADR 0004 §11`).
//!
//! **Responsibility.** Holds the adapter registry table ([`registry`]), and future model/pipeline
//! adapters and test doubles (`adapters/doubles.rs`).
//!
//! **One table instead of one per port.** Every adapter across all ports is registered in
//! [`registry::ADAPTERS`], describing its capabilities, crate, dependencies, and contract suite.

pub mod doubles;
pub mod extract_io;
pub mod gate;
pub mod model;
pub mod registry;
pub mod router;
pub mod schema_json;

use self::model::{Deps, ModelAdapter};

/// Constructor of one model adapter from the shared run [`Deps`].
///
/// A factory receives the run dependencies for construction only; an operation's banners and
/// policy hash arrive through `PreparationContext`. The table starts empty, and each adapter
/// card adds one row.
pub type FactoryFn = fn(&Deps) -> Box<dyn ModelAdapter + Send + Sync>;

/// One registered model adapter factory, keyed by its adapter identifier (`<name>@<n>`).
pub struct Factory {
    /// Adapter identifier (`<name>@<n>`).
    pub id: &'static str,
    /// Constructor of the adapter.
    pub build: FactoryFn,
}

/// Model adapter factories. Each adapter card adds one row; nothing is built here yet.
pub const FACTORIES: &[Factory] = &[];

/// Looks up a factory by adapter identifier.
#[must_use]
pub fn factory_for<'a>(id: &'a str, factories: &'a [Factory]) -> Option<&'a Factory> {
    factories.iter().find(|f| f.id == id)
}
