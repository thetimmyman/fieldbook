//! Curriculum `StageTrack` builder and renderer (`parts/stage_track.html`, `build-page-types`).
//!
//! **Responsibility.** Build the eight curriculum stage columns (`1..=8`) from the `stage`
//! front-matter field of every page in [`LabelledView`], linking each stage's pages without
//! any reading progress or done marks.

use askama::Template;
use fieldbook_core::egress::LabelledView;

use crate::inline::first_sentence_of_purpose;
use crate::links::{escape_html, render_page_link};
use crate::site::PlannedPaths;

/// Number of curriculum stages (`1..=8`).
pub const STAGE_COUNT: u8 = 8;

/// One linked page entry inside a curriculum [`StageColumn`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagePageItem {
    /// Page identifier (`<area>-<slug>`).
    pub page_id: String,
    /// Page title.
    pub title: String,
    /// Rendered `PageLink` HTML (or plain text if unplanned).
    pub link_html: String,
}

/// One column (`Stage 1` through `Stage 8`) of the `StageTrack` component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageColumn {
    /// Stage number (`1..=8`).
    pub stage: u8,
    /// `true` when this column matches the current stage.
    pub is_current: bool,
    /// Pages assigned to `stage` in deterministic `page.id` order.
    pub pages: Vec<StagePageItem>,
}

#[derive(Template)]
#[template(path = "parts/stage_track.html")]
struct StageTrackTemplate {
    stage_columns: Vec<StageColumn>,
}

/// Build the eight [`StageColumn`] entries (`1..=8`) from `view.pages()`.
#[must_use]
pub fn build_stage_columns(
    from_path: &str,
    current_stage: Option<u8>,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> Vec<StageColumn> {
    let mut columns: Vec<StageColumn> = (1..=STAGE_COUNT)
        .map(|s| StageColumn {
            stage: s,
            is_current: current_stage == Some(s),
            pages: Vec::new(),
        })
        .collect();

    for page in view.pages() {
        let stage = page.meta.stage;
        if (1..=STAGE_COUNT).contains(&stage) {
            let idx = usize::from(stage - 1);
            let purpose_first = first_sentence_of_purpose(page);
            let link_html = render_page_link(
                from_path,
                page.id.as_str(),
                None,
                &escape_html(&page.meta.title),
                Some(&page.meta.title),
                Some(&purpose_first),
                planned,
            );
            columns[idx].pages.push(StagePageItem {
                page_id: page.id.to_string(),
                title: page.meta.title.clone(),
                link_html,
            });
        }
    }

    columns
}

/// Render `parts/stage_track.html` for `view` relative to `from_path`.
#[must_use]
pub fn render_stage_track(
    from_path: &str,
    current_stage: Option<u8>,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> String {
    let stage_columns = build_stage_columns(from_path, current_stage, view, planned);
    let tpl = StageTrackTemplate { stage_columns };
    tpl.render()
        .unwrap_or_else(|e| unreachable!("stage_track.html is infallible: {e}"))
}
