//! Retained, dedicated version kernels for published compatibility bindings.
//!
//! **Responsibility.** Frozen copies of historical extractors and adapters that cannot
//! drift when current implementations evolve. The compatibility runner dispatches
//! historical vectors through these retained versions.
//!
//! **Rules.** Pure: no I/O.

pub mod pdftotext_layout_v1;
