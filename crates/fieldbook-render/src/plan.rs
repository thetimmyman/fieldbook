//! Relative-day window parser and `PlanTimeline` builder for `checklist` pages (`build-page-types`).
//!
//! **Responsibility.** Parse `###` group headings in `How it works` that match the exact
//! relative-day window grammar `Day N to Day M` (`1 <= N <= M`) and compute shared-axis
//! SVG bar coordinates (`viewBox="0 0 100 10"`) for `parts/plan_timeline.html`.

use std::ops::Range;

use fieldbook_core::check::PageEntry;
use fieldbook_core::strings;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// One relative-day window (`Day N to Day M`) on the shared `PlanTimeline` axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanWindowView {
    /// Exact window heading (`Day N to Day M`).
    pub label: String,
    /// Inclusive start day (`N >= 1`).
    pub start_day: u32,
    /// Inclusive end day (`M >= N`).
    pub end_day: u32,
    /// Horizontal start coordinate (`0.00` to `100.00`) on the shared SVG axis.
    pub x_attr: String,
    /// Bar width (`0.00` to `100.00`) on the shared SVG axis.
    pub width_attr: String,
}

/// Shared-axis `PlanTimeline` view model (`parts/plan_timeline.html`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanTimelineView {
    /// Maximum end day across all parsed windows (`0` when `windows` is empty).
    pub axis_max: u32,
    /// Accessible label for the inline SVG plot (`role="img"`).
    pub aria_label: String,
    /// Windows plotted in authored order on the shared `[1, axis_max]` axis.
    pub windows: Vec<PlanWindowView>,
}

/// Parse a `###` group heading against the exact `Day N to Day M` grammar (`1 <= N <= M`).
///
/// Any other form — including reversed bounds (`Day 30 to Day 1`), hyphen ranges (`Day 1-30`),
/// plural (`Days 1 to 30`), leading/trailing whitespace, or non-digits — returns `None`.
#[must_use]
pub fn parse_window_heading(heading: &str) -> Option<(u32, u32)> {
    let rest = heading.strip_prefix("Day ")?;
    let (n_str, m_str) = rest.split_once(" to Day ")?;
    if n_str.is_empty()
        || m_str.is_empty()
        || !n_str.bytes().all(|b| b.is_ascii_digit())
        || !m_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let start_day: u32 = n_str.parse().ok()?;
    let end_day: u32 = m_str.parse().ok()?;
    if start_day == 0 || start_day > end_day {
        return None;
    }
    Some((start_day, end_day))
}

/// Build a [`PlanTimelineView`] from the `###` group headings inside `How it works` on `page`.
#[must_use]
pub fn build_plan_timeline(page: &PageEntry) -> PlanTimelineView {
    let how_heading = strings::parts()[2];
    let Some(part_span) = page.parts().get(how_heading) else {
        return PlanTimelineView::default();
    };
    if part_span.event_range.is_empty() {
        return PlanTimelineView::default();
    }
    build_plan_timeline_from_range(&page.body, part_span.event_range.clone())
}

fn build_plan_timeline_from_range(body: &str, event_range: Range<usize>) -> PlanTimelineView {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);

    let mut raw_windows: Vec<(String, u32, u32)> = Vec::new();
    let mut in_h3 = false;
    let mut h3_text = String::new();

    for (idx, (event, _range)) in Parser::new_ext(body, opts).into_offset_iter().enumerate() {
        if idx >= event_range.end {
            break;
        }
        if idx < event_range.start {
            continue;
        }
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H3,
                ..
            }) => {
                in_h3 = true;
                h3_text.clear();
            }
            Event::End(TagEnd::Heading(HeadingLevel::H3)) if in_h3 => {
                in_h3 = false;
                if let Some((start_day, end_day)) = parse_window_heading(&h3_text) {
                    raw_windows.push((h3_text.clone(), start_day, end_day));
                }
            }
            Event::Text(cow) | Event::Code(cow) if in_h3 => {
                h3_text.push_str(&cow);
            }
            Event::SoftBreak | Event::HardBreak if in_h3 => {
                h3_text.push(' ');
            }
            _ => {}
        }
    }

    build_from_parsed_windows(&raw_windows)
}

/// Build a [`PlanTimelineView`] from already-parsed `(label, start_day, end_day)` tuples.
#[must_use]
pub fn build_from_parsed_windows(raw_windows: &[(String, u32, u32)]) -> PlanTimelineView {
    if raw_windows.is_empty() {
        return PlanTimelineView::default();
    }
    let axis_max = raw_windows
        .iter()
        .map(|(_, _, end)| *end)
        .max()
        .unwrap_or(1)
        .max(1);
    let axis_f = f64::from(axis_max);
    let mut windows = Vec::with_capacity(raw_windows.len());
    let mut labels = Vec::with_capacity(raw_windows.len());

    for (label, start_day, end_day) in raw_windows {
        let start_zero = f64::from(start_day.saturating_sub(1));
        let span_days = f64::from(end_day.saturating_sub(*start_day).saturating_add(1));
        let x = (start_zero / axis_f) * 100.0;
        let width = (span_days / axis_f) * 100.0;
        labels.push(label.as_str());
        windows.push(PlanWindowView {
            label: label.clone(),
            start_day: *start_day,
            end_day: *end_day,
            x_attr: format!("{x:.2}"),
            width_attr: format!("{width:.2}"),
        });
    }

    let aria_label = format!(
        "Plan timeline across {} days: {}",
        axis_max,
        labels.join(", ")
    );

    PlanTimelineView {
        axis_max,
        aria_label,
        windows,
    }
}
