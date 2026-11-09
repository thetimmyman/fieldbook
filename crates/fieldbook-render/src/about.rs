//! About page (`about.html`, `build-about`): what this build is and what it holds.
//!
//! **Rules.** Every value comes from the permitted view, the [`RenderContext`] or the compiled
//! [`BuildProfile`]. The page never probes a setting, an adapter or a connection, and the AI
//! position is text with no button and no link.

use askama::Template;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::{ClaimRow, PageStatus, Register, Verification};
use fieldbook_core::strings;

use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::home;
use crate::links::{escape_html, href_if_planned, render_claim_chip};
use crate::meta::{BuildProfile, RenderContext};
use crate::nav::{Navigation, SectionId};
use crate::register;
use crate::site::PlannedPaths;
use crate::status_chip::verification_chip_spec;

/// Site-relative output path of the About page.
pub const OUTPUT_PATH: &str = "about.html";

/// Page title of `about.html`.
pub const PAGE_TITLE: &str = "About this edition";

/// The Short form of the canonical network-posture sentence (`docs/cards/README.md`).
pub const NETWORK_POSTURE_SHORT: &str = "The silent build opens no socket, starts no process, contains no model and makes no outbound connection. The AI build adds an HTTPS client for configured endpoints and, built with `bridge`, one loopback listener while `fieldbook chat` runs (IT-27, IT-32).";

/// What a public edition says about the builds in it.
const ONLY_BUILD: &str = "The silent build is the only build in this edition.";

/// The AI position of a silent build (AD-24): what the AI build would add, as text only.
const AI_POSITION_SILENT: &str = "The AI build is not in this edition. The Later list (AD-24) names what it would add: read-only search, context and citation checks for an approved AI host, decision adapters, a local model reached on loopback, and semantic search through the Model port.";

/// The AI position of an AI build (AD-24): what is still Later, as text only.
const AI_POSITION_AI: &str = "This is the AI build. The Later list (AD-24) names what it does not add yet: read-only search, context and citation checks for an approved AI host, decision adapters, a local model reached on loopback, and semantic search through the Model port.";

/// The empty line of a list with no items.
const EMPTY_LIST: &str = "None recorded";

const STALE_TITLE: &str = "Stale items";
const UNREVIEWED_TITLE: &str = "Unreviewed consequential claims";

/// Most rows a claim list writes. The panel's count is the true total and the claims register
/// holds the rest, so the page stays the same size as the Corpus grows.
pub const CLAIM_LIST_SHOWN: usize = 10;

struct Fact {
    label: String,
    value: String,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate<'a> {
    title: &'a str,
    build: Vec<Fact>,
    handling: &'a str,
    profile_name: &'a str,
    posture_html: String,
    only_build: Option<&'a str>,
    ai_position: &'a str,
    records: Vec<Fact>,
    pages_by_status: Vec<Fact>,
    claims_by_verification: Vec<Fact>,
    stale_html: String,
    unreviewed_html: String,
}

fn or_none(value: &str) -> String {
    if value.is_empty() {
        EMPTY_LIST.to_owned()
    } else {
        value.to_owned()
    }
}

/// The posture sentence as HTML: its Markdown code spans become `<code>`, so the constant can
/// quote the canonical sentence exactly and the reader sees no backticks.
fn posture_html(sentence: &str) -> String {
    sentence
        .split('`')
        .enumerate()
        .map(|(index, part)| {
            let part = escape_html(part);
            if index % 2 == 1 {
                format!("<code>{part}</code>")
            } else {
                part
            }
        })
        .collect()
}

/// A panel with the true count of the claims `keep` selects, their first [`CLAIM_LIST_SHOWN`]
/// rows, and a line that sends the reader to the claims register for the rest.
fn claim_panel(
    title: &str,
    view: &LabelledView,
    planned: &PlannedPaths,
    keep: impl Fn(&ClaimRow) -> bool,
) -> String {
    let kept: Vec<&ClaimRow> = view
        .claims()
        .iter()
        .map(|located| &located.value)
        .filter(|claim| keep(claim))
        .collect();
    if kept.is_empty() {
        return home::panel("h2", title, None, &[], EMPTY_LIST);
    }
    let rows: String = kept
        .iter()
        .take(CLAIM_LIST_SHOWN)
        .map(|claim| {
            format!(
                "<li class=\"tu-item\"><span>{} {}</span></li>",
                render_claim_chip(OUTPUT_PATH, &claim.claim_id, planned),
                escape_html(&claim.statement)
            )
        })
        .collect();
    let more = kept.len().saturating_sub(CLAIM_LIST_SHOWN);
    let rest = if more == 0 {
        String::new()
    } else {
        let line = format!("{more} more in the claims register");
        let line = match href_if_planned(
            OUTPUT_PATH,
            register::output_path(Register::Claims),
            planned,
        ) {
            Some(href) => format!("<a href=\"{}\">{line}</a>", escape_html(&href)),
            None => line,
        };
        format!("<p class=\"muted\">{line}</p>")
    };
    let id = format!("tu-{}", title.to_ascii_lowercase().replace(' ', "-"));
    format!(
        "<section class=\"panel\" aria-labelledby=\"{id}\"><div class=\"panel-head\"><h2 class=\"panel-title\" id=\"{id}\">{title}</h2><span class=\"panel-sub mono\">{}</span></div><ul class=\"tu-list\">{rows}</ul>{rest}</section>",
        kept.len()
    )
}

/// Renders the `<main>` content of `about.html`.
#[must_use]
pub fn render_main(
    view: &LabelledView,
    ctx: &RenderContext,
    profile: BuildProfile,
    planned: &PlannedPaths,
) -> String {
    let edition = ctx.edition_info();
    let handling = match ctx {
        RenderContext::Public(_) => "This edition holds public only.",
    };
    let (profile_name, only_build, ai_position) = match profile {
        BuildProfile::Silent => ("silent build", Some(ONLY_BUILD), AI_POSITION_SILENT),
        BuildProfile::Ai => ("AI build", None, AI_POSITION_AI),
    };
    let build = vec![
        Fact {
            label: "Tool version".to_owned(),
            value: edition.tool_version.clone(),
        },
        Fact {
            label: "Edition".to_owned(),
            value: if edition.built.is_empty() {
                strings::unreleased_build().to_owned()
            } else {
                edition.edition.clone()
            },
        },
        Fact {
            label: "Built".to_owned(),
            value: or_none(&edition.built),
        },
        Fact {
            label: "Commit".to_owned(),
            value: or_none(&edition.commit),
        },
    ];
    let counts = edition.counts;
    let fact = |label: &str, n: usize| Fact {
        label: label.to_owned(),
        value: n.to_string(),
    };
    let records = vec![
        fact("Pages", counts.pages),
        fact("Source versions", counts.source_versions),
        fact("Originals", counts.originals),
        fact("Claims", counts.claims),
        fact("Questions", counts.questions),
        fact("Conflicts", counts.conflicts),
        fact("Glossary terms", counts.terms),
    ];
    let pages_by_status = PageStatus::ALL
        .into_iter()
        .map(|status| {
            let n = view
                .pages()
                .iter()
                .filter(|page| page.meta.status == status)
                .count();
            fact(status.as_str(), n)
        })
        .collect();
    let claims_by_verification = Verification::ALL
        .into_iter()
        .map(|verification| {
            let n = view
                .claims()
                .iter()
                .filter(|claim| claim.value.verification == verification)
                .count();
            let (_, word, _) = verification_chip_spec(verification);
            fact(&word.to_ascii_lowercase(), n)
        })
        .collect();
    AboutTemplate {
        title: PAGE_TITLE,
        build,
        handling,
        profile_name,
        posture_html: posture_html(NETWORK_POSTURE_SHORT),
        only_build,
        ai_position,
        records,
        pages_by_status,
        claims_by_verification,
        stale_html: claim_panel(STALE_TITLE, view, planned, |claim| {
            claim.verification == Verification::Stale
        }),
        unreviewed_html: claim_panel(UNREVIEWED_TITLE, view, planned, |claim| {
            claim.consequential && claim.verification == Verification::Unchecked
        }),
    }
    .render()
    .unwrap_or_else(|e| unreachable!("about.html is infallible: {e}"))
}

/// Renders the complete `about.html` document inside the global frame.
#[must_use]
pub fn render_page(
    view: &LabelledView,
    ctx: &RenderContext,
    profile: BuildProfile,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_main(view, ctx, profile, planned);
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let today_crumb = match SectionId::Today.first_path() {
        Some(first) => Crumb::linked("Today", first, planned),
        None => Crumb::current("Today"),
    };
    let crumbs = [today_crumb, Crumb::current(PAGE_TITLE)];
    render_frame_page(&FrameInput {
        output_path: OUTPUT_PATH,
        title: PAGE_TITLE,
        page_area: None,
        edition: ctx.edition_info(),
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}
