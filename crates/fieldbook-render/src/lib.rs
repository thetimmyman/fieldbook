//! Fieldbook HTML, print, and site renderer.

/// Architectural layer name (`layers` test).
pub const LAYER: &str = "render";

pub mod csp;
pub mod frame;
pub mod inline;
pub mod links;
pub mod lint;
pub mod meta;
pub mod nav;
pub mod page;
pub mod plan;
pub mod site;
pub mod sources_block;
pub mod stages;
pub mod types;
