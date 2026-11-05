//! Fieldbook HTML, print, and site renderer.

/// Architectural layer name (`layers` test).
pub const LAYER: &str = "render";

pub mod csp;
pub mod frame;
pub mod lint;
pub mod meta;
pub mod nav;
pub mod site;
