//! Changes page (`changes.html`): edition notes from `corpus/changes.md` and optional id diff
//! against an earlier `edition.toml`.
//!
//! **Rules.** Pure: reads no files, environment, or clock. Added ids come from the permitted view
//! and link when their target is planned. Removed ids come from the supplied `EditionInfo` value,
//! are filtered to the public non-company id form of each group (any other removed id is counted in
//! [`IdDiff::hidden`] and never printed), and always render as plain text.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use askama::Template;
use fieldbook_core::edition::{EditionIds, EditionInfo};
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::{ClaimId, ConflictId, EditionId, PageId, QuestionId, TermId, VersionId};

use crate::frame::{
    Crumb, FrameInput, PRODUCT_TITLE, StoreContext, render_page as render_frame_page,
};
use crate::links::{
    claim_href, conflict_href, escape_html, page_href, question_href, source_version_href,
    term_href,
};
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId};
use crate::site::PlannedPaths;

/// Site-relative output path of the Changes page.
pub const OUTPUT_PATH: &str = "changes.html";

/// Page title of `changes.html`.
pub const PAGE_TITLE: &str = "Changes";

/// Number of newest edition-note blocks shown before folding the rest.
pub const NOTES_SHOWN: usize = 3;

/// Number of id chips shown per Added or Removed list before folding the rest.
pub const GROUP_IDS_SHOWN: usize = 20;

/// Sorted added and listed-removed ids for one `edition.toml` identifier group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupDiff {
    /// Added ids in natural id order.
    pub added: Vec<String>,
    /// Public-form removed ids in natural id order.
    pub removed: Vec<String>,
}

impl GroupDiff {
    /// Returns `true` when neither added nor listed-removed ids are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Identifier set differences across the six `edition.toml` groups, plus the count of removed ids
/// excluded because they lack the public non-company id form of their group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdDiff {
    /// Page id diff (`pages`).
    pub pages: GroupDiff,
    /// Source version id diff (`source_versions`).
    pub source_versions: GroupDiff,
    /// Claim id diff (`claims`).
    pub claims: GroupDiff,
    /// Question id diff (`questions`).
    pub questions: GroupDiff,
    /// Conflict id diff (`conflicts`).
    pub conflicts: GroupDiff,
    /// Glossary term id diff (`terms`).
    pub terms: GroupDiff,
    /// Count of removed ids omitted because they are not in the public id form of their group.
    pub hidden: usize,
}

impl IdDiff {
    /// Returns `true` when no group has added or listed-removed ids and `hidden == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
            && self.source_versions.is_empty()
            && self.claims.is_empty()
            && self.questions.is_empty()
            && self.conflicts.is_empty()
            && self.terms.is_empty()
            && self.hidden == 0
    }
}

#[derive(Clone, Copy)]
enum GroupKind {
    Pages,
    SourceVersions,
    Claims,
    Questions,
    Conflicts,
    Terms,
}

impl GroupKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Pages => "Pages",
            Self::SourceVersions => "Source versions",
            Self::Claims => "Claims",
            Self::Questions => "Questions",
            Self::Conflicts => "Conflicts",
            Self::Terms => "Terms",
        }
    }

    fn is_public_removed_id(self, id: &str) -> bool {
        match self {
            Self::Pages => PageId::parse(id).is_ok_and(|pid| !pid.as_str().starts_with('x')),
            Self::SourceVersions => VersionId::parse(id).is_ok(),
            Self::Claims => {
                ClaimId::parse(id).is_ok_and(|cid| !cid.area().as_str().starts_with('x'))
            }
            Self::Questions => QuestionId::parse(id).is_ok(),
            Self::Conflicts => ConflictId::parse(id).is_ok(),
            Self::Terms => TermId::parse(id).is_ok_and(|tid| !tid.as_str().starts_with("x_")),
        }
    }

    fn cmp_natural(self, a: &str, b: &str) -> Ordering {
        match self {
            Self::Pages | Self::Questions | Self::Terms => a.cmp(b),
            Self::SourceVersions => match (VersionId::parse(a), VersionId::parse(b)) {
                (Ok(va), Ok(vb)) => va
                    .source()
                    .number()
                    .cmp(&vb.source().number())
                    .then_with(|| va.version().cmp(&vb.version()))
                    .then_with(|| a.cmp(b)),
                _ => a.cmp(b),
            },
            Self::Claims => match (ClaimId::parse(a), ClaimId::parse(b)) {
                (Ok(ca), Ok(cb)) => ca.cmp(&cb).then_with(|| a.cmp(b)),
                _ => a.cmp(b),
            },
            Self::Conflicts => match (ConflictId::parse(a), ConflictId::parse(b)) {
                (Ok(xa), Ok(xb)) => xa.number().cmp(&xb.number()).then_with(|| a.cmp(b)),
                _ => a.cmp(b),
            },
        }
    }
}

fn diff_group(kind: GroupKind, previous: &[String], current: &[String]) -> (GroupDiff, usize) {
    let prev_set: BTreeSet<&str> = previous.iter().map(String::as_str).collect();
    let curr_set: BTreeSet<&str> = current.iter().map(String::as_str).collect();

    let mut added: Vec<String> = curr_set
        .difference(&prev_set)
        .map(|&s| s.to_owned())
        .collect();
    added.sort_by(|a, b| kind.cmp_natural(a, b));

    let mut removed = Vec::new();
    let mut hidden = 0usize;
    for &id in prev_set.difference(&curr_set) {
        if kind.is_public_removed_id(id) {
            removed.push(id.to_owned());
        } else {
            hidden += 1;
        }
    }
    removed.sort_by(|a, b| kind.cmp_natural(a, b));

    (GroupDiff { added, removed }, hidden)
}

/// Computes the pure identifier diff between `previous` and `current` across all six groups.
#[must_use]
pub fn diff_ids(previous: &EditionIds, current: &EditionIds) -> IdDiff {
    let (pages, h_pages) = diff_group(GroupKind::Pages, &previous.pages, &current.pages);
    let (source_versions, h_versions) = diff_group(
        GroupKind::SourceVersions,
        &previous.source_versions,
        &current.source_versions,
    );
    let (claims, h_claims) = diff_group(GroupKind::Claims, &previous.claims, &current.claims);
    let (questions, h_questions) = diff_group(
        GroupKind::Questions,
        &previous.questions,
        &current.questions,
    );
    let (conflicts, h_conflicts) = diff_group(
        GroupKind::Conflicts,
        &previous.conflicts,
        &current.conflicts,
    );
    let (terms, h_terms) = diff_group(GroupKind::Terms, &previous.terms, &current.terms);

    IdDiff {
        pages,
        source_versions,
        claims,
        questions,
        conflicts,
        terms,
        hidden: h_pages + h_versions + h_claims + h_questions + h_conflicts + h_terms,
    }
}

struct NoteLine {
    label: &'static str,
    text: String,
}

struct NoteBlock {
    number: u32,
    edition: String,
    lines: Vec<NoteLine>,
}

struct RawNoteSection {
    number: u32,
    edition: String,
    added: Vec<String>,
    changed: Vec<String>,
    verified: Vec<String>,
}

impl RawNoteSection {
    fn into_block(self) -> NoteBlock {
        let mut lines = Vec::new();
        for (label, entries) in [
            ("Added", self.added),
            ("Changed", self.changed),
            ("Verified", self.verified),
        ] {
            if !entries.is_empty() {
                lines.push(NoteLine {
                    label,
                    text: entries.join(" "),
                });
            }
        }
        NoteBlock {
            number: self.number,
            edition: self.edition,
            lines,
        }
    }
}

fn parse_edition_notes(changes_md: &str) -> Vec<NoteBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<RawNoteSection> = None;

    for raw_line in changes_md.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            let token = rest.trim();
            if let Ok(ed) = EditionId::parse(token) {
                if let Some(sec) = current.take() {
                    blocks.push(sec.into_block());
                }
                current = Some(RawNoteSection {
                    number: ed.number(),
                    edition: ed.as_str().to_owned(),
                    added: Vec::new(),
                    changed: Vec::new(),
                    verified: Vec::new(),
                });
                continue;
            }
        }
        let Some(sec) = current.as_mut() else {
            continue;
        };
        if let Some(body) = line.strip_prefix("- Added:") {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                sec.added.push(trimmed.to_owned());
            }
        } else if let Some(body) = line.strip_prefix("- Changed:") {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                sec.changed.push(trimmed.to_owned());
            }
        } else if let Some(body) = line.strip_prefix("- Verified:") {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                sec.verified.push(trimmed.to_owned());
            }
        }
    }
    if let Some(sec) = current {
        blocks.push(sec.into_block());
    }
    blocks.sort_by_key(|a| std::cmp::Reverse(a.number));
    blocks
}

fn render_plain_chip(id: &str) -> String {
    format!("<span class=\"cite neutral\">{}</span>", escape_html(id))
}

fn render_added_chip(
    kind: GroupKind,
    id: &str,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> String {
    let href = match kind {
        GroupKind::Pages => PageId::parse(id)
            .ok()
            .and_then(|pid| page_href(OUTPUT_PATH, pid.as_str(), None, planned)),
        GroupKind::SourceVersions => VersionId::parse(id).ok().and_then(|vid| {
            let src = view
                .sources()
                .iter()
                .find(|s| s.value.source_id == vid.source() && s.value.version == vid.version())
                .map(|s| &s.value);
            source_version_href(OUTPUT_PATH, &vid, src, planned)
        }),
        GroupKind::Claims => ClaimId::parse(id)
            .ok()
            .and_then(|cid| claim_href(OUTPUT_PATH, &cid, planned)),
        GroupKind::Questions => QuestionId::parse(id)
            .ok()
            .and_then(|qid| question_href(OUTPUT_PATH, &qid, planned)),
        GroupKind::Conflicts => ConflictId::parse(id)
            .ok()
            .and_then(|cid| conflict_href(OUTPUT_PATH, &cid, planned)),
        GroupKind::Terms => TermId::parse(id)
            .ok()
            .and_then(|tid| term_href(OUTPUT_PATH, &tid, planned)),
    };
    match href {
        Some(href) => format!(
            "<a class=\"cite neutral\" href=\"{}\">{}</a>",
            escape_html(&href),
            escape_html(id)
        ),
        None => render_plain_chip(id),
    }
}

struct FoldedChipsView {
    count: usize,
    chips_html: String,
}

struct SublistView {
    shown_html: String,
    folded: Option<FoldedChipsView>,
}

fn build_sublist(chips: &[String]) -> Option<SublistView> {
    if chips.is_empty() {
        return None;
    }
    let split_at = chips.len().min(GROUP_IDS_SHOWN);
    let shown_html = chips[..split_at].join(" ");
    let folded = (chips.len() > GROUP_IDS_SHOWN).then(|| FoldedChipsView {
        count: chips.len() - GROUP_IDS_SHOWN,
        chips_html: chips[split_at..].join(" "),
    });
    Some(SublistView { shown_html, folded })
}

struct SummaryRowView {
    name: &'static str,
    added: usize,
    removed: usize,
}

struct GroupBlockView {
    name: &'static str,
    added: Option<SublistView>,
    removed: Option<SublistView>,
}

struct DiffPanelView {
    caption: String,
    rows: Vec<SummaryRowView>,
    hidden_line: Option<String>,
    groups: Vec<GroupBlockView>,
}

enum IdsPanelView {
    NotCompared,
    Unchanged(String),
    Diff(DiffPanelView),
}

#[derive(Template)]
#[template(path = "changes.html")]
struct ChangesTemplate<'a> {
    title: &'a str,
    notes_shown: Vec<NoteBlock>,
    notes_folded: Vec<NoteBlock>,
    ids_heading: String,
    ids_panel: IdsPanelView,
}

fn previous_edition_label(prev: &EditionInfo) -> &str {
    if prev.edition == "unreleased" || prev.number == 0 {
        "an unreleased build"
    } else {
        prev.edition.as_str()
    }
}

/// Renders the `<main>` content of `changes.html`.
#[must_use]
pub fn render_main(
    view: &LabelledView,
    ctx: &RenderContext,
    planned: &PlannedPaths,
    previous: Option<&EditionInfo>,
) -> String {
    let notes_all = view
        .tree()
        .get_bytes("corpus/changes.md")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map_or_else(Vec::new, parse_edition_notes);
    let mut notes_shown = notes_all;
    let notes_folded = if notes_shown.len() > NOTES_SHOWN {
        notes_shown.split_off(NOTES_SHOWN)
    } else {
        Vec::new()
    };

    let (ids_heading, ids_panel) = match previous {
        None => (
            "Ids since an earlier edition".to_owned(),
            IdsPanelView::NotCompared,
        ),
        Some(prev) => {
            let label = previous_edition_label(prev);
            let heading = format!("Ids since {label}");
            let diff = diff_ids(&prev.ids, &ctx.edition_info().ids);
            if diff.is_empty() {
                (
                    heading,
                    IdsPanelView::Unchanged(format!("No ids were added or removed since {label}.")),
                )
            } else {
                let pairs: [(GroupKind, &GroupDiff); 6] = [
                    (GroupKind::Pages, &diff.pages),
                    (GroupKind::SourceVersions, &diff.source_versions),
                    (GroupKind::Claims, &diff.claims),
                    (GroupKind::Questions, &diff.questions),
                    (GroupKind::Conflicts, &diff.conflicts),
                    (GroupKind::Terms, &diff.terms),
                ];
                let rows = pairs
                    .iter()
                    .map(|&(kind, g)| SummaryRowView {
                        name: kind.name(),
                        added: g.added.len(),
                        removed: g.removed.len(),
                    })
                    .collect();
                let hidden_line = match diff.hidden {
                    0 => None,
                    1 => Some(
                        "1 removed id is not listed. It is not in the public id form.".to_owned(),
                    ),
                    n => Some(format!(
                        "{n} removed ids are not listed. They are not in the public id form."
                    )),
                };
                let groups = pairs
                    .iter()
                    .filter(|(_, g)| !g.is_empty())
                    .map(|&(kind, g)| {
                        let added_chips: Vec<String> = g
                            .added
                            .iter()
                            .map(|id| render_added_chip(kind, id, view, planned))
                            .collect();
                        let removed_chips: Vec<String> =
                            g.removed.iter().map(|id| render_plain_chip(id)).collect();
                        GroupBlockView {
                            name: kind.name(),
                            added: build_sublist(&added_chips),
                            removed: build_sublist(&removed_chips),
                        }
                    })
                    .collect();
                (
                    heading,
                    IdsPanelView::Diff(DiffPanelView {
                        caption: format!("Ids added and removed since {label}"),
                        rows,
                        hidden_line,
                        groups,
                    }),
                )
            }
        }
    };

    ChangesTemplate {
        title: PAGE_TITLE,
        notes_shown,
        notes_folded,
        ids_heading,
        ids_panel,
    }
    .render()
    .unwrap_or_else(|e| unreachable!("changes.html is infallible: {e}"))
}

/// Renders the complete `changes.html` document inside the global frame.
#[must_use]
pub fn render_page(
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
    previous: Option<&EditionInfo>,
) -> String {
    let main_html = render_main(view, ctx, planned, previous);
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let today_crumb = match SectionId::Today.first_path() {
        Some(first) => Crumb::linked("Today", first, planned),
        None => Crumb::current("Today"),
    };
    let crumbs = [today_crumb, Crumb::current(PAGE_TITLE)];
    let doc_title = format!("{PAGE_TITLE} · {PRODUCT_TITLE}");
    render_frame_page(&FrameInput {
        output_path: OUTPUT_PATH,
        title: &doc_title,
        page_area: None,
        edition: ctx.edition_info(),
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}
