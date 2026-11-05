//! Shared test support for `fieldbook` integration tests (`audit-journal`).

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "shared test support modules are selectively used across test binaries"
)]

pub mod state;
pub mod world;
