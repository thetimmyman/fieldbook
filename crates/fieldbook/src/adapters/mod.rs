//! Architectural port adapters and adapter registry (`AD-01`, `ADR 0004 §11`).
//!
//! **Responsibility.** Holds the adapter registry table ([`registry`]), and future model/pipeline
//! adapters and test doubles (`adapters/doubles.rs`).
//!
//! **One table instead of one per port.** Every adapter across all ports is registered in
//! [`registry::ADAPTERS`], describing its capabilities, crate, dependencies, and contract suite.

pub mod extract_io;
pub mod registry;
