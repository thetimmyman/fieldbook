//! `audit_doc`: render `docs/reference/audit.md` from `fieldbook_core::audit::Kind`
//! (`audit-journal`, AD-16, AD-22).

use std::fmt::Write as _;

use fieldbook_core::audit::Kind;

const ENVELOPE_FIELDS: &[(&str, &str, &str)] = &[
    ("seq", "Count", "1-based event sequence number"),
    ("time", "Now", "UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`)"),
    ("kind", "Kind", "Event kind discriminator"),
    (
        "actor",
        "Principal",
        "Operating-system or local principal (`os:<DOMAIN>\\<user>` or `local`)",
    ),
    ("role", "Role", "Declared role (`owner`, `lead`, `agent`)"),
    (
        "store",
        "StoreId",
        "Store identifier (`corpus`, `overlay`, `working`, `state`)",
    ),
    ("via", "Via", "Door (`cli`, `chat`, `mcp`)"),
    (
        "hash",
        "Sha256",
        "Chain hash `SHA-256(prev_hex || 0x0A || body)`",
    ),
];

/// Render the generated body between `<!-- generated:audit -->` and `<!-- /generated:audit -->`
/// in `docs/reference/audit.md`.
///
/// # Errors
///
/// Returns an error if formatting fails.
pub fn render_audit_section(_current_body: &str) -> Result<String, String> {
    let mut out = String::new();
    out.push_str(
        "Generated from `fieldbook_core::audit::Kind` and [audit-contract.md](audit-contract.md) by `cargo xtask docs`. Do not edit by hand.\n\n",
    );

    out.push_str("## Envelope\n\n");
    out.push_str("Every event line carries these envelope keys once:\n\n");
    out.push_str("| Field | Type | Meaning |\n|---|---|---|\n");
    for &(field, ty, meaning) in ENVELOPE_FIELDS {
        let _ = writeln!(out, "| `{field}` | `{ty}` | {meaning} |");
    }

    out.push_str("\n## Event kinds\n\n");
    out.push_str("| Kind | Fields (`name: Type`) | Emitted by | Egress |\n|---|---|---|---|\n");
    for kind in Kind::ALL {
        let fields = kind.fields();
        let fields_cell = if fields.is_empty() {
            "-".to_owned()
        } else {
            fields
                .iter()
                .map(|f| format!("`{}: {}`", f.name, f.ty))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let egress_mark = if kind.is_egress() { "egress" } else { "-" };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            kind.as_str(),
            fields_cell,
            kind.emitted_by(),
            egress_mark
        );
    }

    Ok(out)
}
