//! `StatusChip` renderer for the vocabularies mapped in `HANDOFF.md` §7 (`build-registers`).
//!
//! **Responsibility.** Render `StatusChip` HTML (`<span class="chip ...">`) strictly for the
//! vocabularies mapped in `HANDOFF.md` §7 (`verification`, `applicability`, `adoption`
//! and `handling`). Unmapped enum columns in register tables render their
//! escaped canonical text directly and must never pass through a colour or icon guesser.

use fieldbook_core::schema::{Adoption, Applicability, Verification};

use crate::links::escape_html;

/// 12×12 stroke check icon (`HANDOFF.md` §7).
pub const ICON_CHECK: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M3.5 8.5 6.5 11.5 12.5 4.5\"/></svg>";

/// 12×12 stroke cross icon (`HANDOFF.md` §7).
pub const ICON_X: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M4.5 4.5 11.5 11.5M11.5 4.5 4.5 11.5\"/></svg>";

/// 12×12 stroke clock icon (`HANDOFF.md` §7).
pub const ICON_CLOCK: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><circle cx=\"8\" cy=\"8\" r=\"5.5\"/><path d=\"M8 5v3.5l2.5 1.5\"/></svg>";

/// 12×12 stroke question-mark icon (`HANDOFF.md` §7).
pub const ICON_QUESTION: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><circle cx=\"8\" cy=\"8\" r=\"5.5\"/><path d=\"M6.4 6.3a1.7 1.7 0 1 1 2.5 1.5c-.6.3-.9.7-.9 1.4\"/><circle cx=\"8\" cy=\"11.4\" r=\"0.6\"/></svg>";

/// 12×12 stroke dashed-circle icon (`HANDOFF.md` §7).
pub const ICON_CIRCLE_DASHED: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><circle cx=\"8\" cy=\"8\" r=\"5.5\" stroke-dasharray=\"2 2\"/></svg>";

/// 12×12 stroke alert triangle icon (`HANDOFF.md` §7).
pub const ICON_ALERT: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M8 2.5 14 13H2L8 2.5Z\"/><path d=\"M8 6.5v3\"/><circle cx=\"8\" cy=\"11.2\" r=\"0.6\"/></svg>";

/// Return `(optional_variant_class, display_label, icon_svg)` for [`Verification`] as
/// specified in `HANDOFF.md` §7 (`Status display rules`).
#[must_use]
pub const fn verification_chip_spec(
    ver: Verification,
) -> (Option<&'static str>, &'static str, &'static str) {
    match ver {
        Verification::PrimarySourceSupported => (Some("ok"), "Supported", ICON_CHECK),
        Verification::Contradicted => (Some("bad"), "Contradicted", ICON_X),
        Verification::Stale => (Some("warn"), "Stale", ICON_CLOCK),
        Verification::Unresolved => (Some("info"), "Unresolved", ICON_QUESTION),
        Verification::Unchecked => (None, "Unchecked", ICON_CIRCLE_DASHED),
    }
}

/// Render a [`Verification`] `StatusChip` (`HANDOFF.md` §7).
///
/// Always includes both the icon and text label, and stores the raw vocabulary string in
/// `title` (`corpus-format.md#status-vocabularies-and-promotion-rules`).
#[must_use]
pub fn render_verification_chip(ver: Verification) -> String {
    let (variant, label, icon) = verification_chip_spec(ver);
    let class_attr = match variant {
        Some(v) => format!("chip {v}"),
        None => "chip".to_owned(),
    };
    let raw = escape_html(ver.as_str());
    let text = escape_html(label);
    format!("<span class=\"{class_attr}\" title=\"{raw}\">{icon}<span>{text}</span></span>")
}

/// Return the display label for [`Applicability`] (`HANDOFF.md` §7).
#[must_use]
pub const fn applicability_label(app: Applicability) -> &'static str {
    match app {
        Applicability::GeneralReference => "General reference",
        Applicability::PrototypeAssumption => "Prototype assumption",
        Applicability::PendingCompanyConfirmation => "Pending company",
        Applicability::ConfirmedApplicable => "Confirmed applicable",
        Applicability::NotApplicable => "Not applicable",
    }
}

/// Render a neutral [`Applicability`] `StatusChip` (`HANDOFF.md` §7).
#[must_use]
pub fn render_applicability_chip(app: Applicability) -> String {
    let raw = escape_html(app.as_str());
    let label = escape_html(applicability_label(app));
    format!("<span class=\"chip\" title=\"{raw}\">{label}</span>")
}

/// Return the display label for [`Adoption`] (`HANDOFF.md` §7).
#[must_use]
pub const fn adoption_label(adopt: Adoption) -> &'static str {
    match adopt {
        Adoption::Proposed => "Proposed",
        Adoption::PrototypeReviewed => "Prototype reviewed",
        Adoption::PendingCompanyApproval => "Pending approval",
        Adoption::CompanyApproved => "Company approved",
        Adoption::Rejected => "Rejected",
        Adoption::Superseded => "Superseded",
    }
}

/// Render a neutral [`Adoption`] `StatusChip` (`HANDOFF.md` §7).
#[must_use]
pub fn render_adoption_chip(adopt: Adoption) -> String {
    let raw = escape_html(adopt.as_str());
    let label = escape_html(adoption_label(adopt));
    format!("<span class=\"chip\" title=\"{raw}\">{label}</span>")
}

/// Render one neutral `<span class="chip">` per handling label id (`HANDOFF.md` §7).
#[must_use]
pub fn render_handling_chips(handling: &[String]) -> String {
    handling
        .iter()
        .map(|label| format!("<span class=\"chip\">{}</span>", escape_html(label)))
        .collect::<Vec<_>>()
        .join(" ")
}
