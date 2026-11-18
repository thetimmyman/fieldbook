//! Retained, dedicated per-version adapter kernels for published compatibility bindings.
//!
//! **Responsibility.** Retain frozen naming, parsing, sorting, digest, and validation
//! kernels for published adapters (`folder@1` backup and `folder@1` store).
//!
//! **Rules.** Pure: no I/O. Any change to behaviour requires a new version file (e.g. `v2`).

pub mod backup_folder_v1;
pub mod staged_v1;
pub mod store_folder_v1;
