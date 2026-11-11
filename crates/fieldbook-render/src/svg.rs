//! Inline SVG written by the build (`build-home-and-areas` and `build-charts`).
//!
//! **Rules.** Geometry and classes only: no text inside the SVG and no `style` attribute, so the
//! AD-12 CSP holds and the bar reads with scripts blocked. Colour comes from `assets/app.css`;
//! labels, values and the legend are HTML beside the plot.

use fieldbook_core::schema::Verification;

/// Hundredths of a viewBox unit across the 100-unit track, so widths sum exactly.
const TRACK_HUNDREDTHS: usize = 10_000;

/// The least width of a state that has any claim, so a lone contradicted claim among thousands
/// still shows past the segment's edge stroke. The label and the legend carry the exact counts.
const MIN_SEGMENT_HUNDREDTHS: usize = 200;

/// How many claims hold each of the five verification states.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoverageCounts {
    /// `primary_source_supported`: the numerator of "N of M supported".
    pub supported: usize,
    /// `unchecked`.
    pub unchecked: usize,
    /// `contradicted`.
    pub contradicted: usize,
    /// `stale`.
    pub stale: usize,
    /// `unresolved`.
    pub unresolved: usize,
}

impl CoverageCounts {
    /// Counts one claim in its verification state.
    pub const fn add(&mut self, verification: Verification) {
        match verification {
            Verification::PrimarySourceSupported => self.supported += 1,
            Verification::Unchecked => self.unchecked += 1,
            Verification::Contradicted => self.contradicted += 1,
            Verification::Stale => self.stale += 1,
            Verification::Unresolved => self.unresolved += 1,
        }
    }

    /// The denominator: all five counts.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.supported + self.unchecked + self.contradicted + self.stale + self.unresolved
    }

    /// `"N of M supported"`, or `"No claims"` when there are none.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.total() == 0 {
            "No claims".to_owned()
        } else {
            format!("{} of {} supported", self.supported, self.total())
        }
    }

    /// The accessible label of the bar: the summary and all five counts.
    #[must_use]
    pub fn label(&self) -> String {
        if self.total() == 0 {
            return self.summary();
        }
        format!(
            "{}: {} supported, {} unchecked, {} contradicted, {} stale, {} unresolved",
            self.summary(),
            self.supported,
            self.unchecked,
            self.contradicted,
            self.stale,
            self.unresolved
        )
    }

    /// The five states in bar order: `(count, segment class, legend word)`.
    const fn segments(&self) -> [(usize, &'static str, &'static str); 5] {
        [
            (self.supported, "s-ok", "Supported"),
            (self.unchecked, "s-un", "Unchecked"),
            (self.contradicted, "s-bad", "Contradicted"),
            (self.stale, "s-stale", "Stale"),
            (self.unresolved, "s-unres", "Unresolved"),
        ]
    }
}

fn units(hundredths: usize) -> String {
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// Writes one `CoverageBar`: a bar 8 units tall in a 10-unit track, one segment per nonzero state.
/// Zero claims leave the track empty. `role="img"` and the `aria-label` carry every count.
#[must_use]
pub fn coverage_bar(counts: &CoverageCounts) -> String {
    let mut out = format!(
        "<svg class=\"cover\" viewBox=\"0 0 100 10\" preserveAspectRatio=\"none\" focusable=\"false\" role=\"img\" aria-label=\"{}\"><rect class=\"track\" x=\"0\" y=\"1\" width=\"100\" height=\"8\"/>",
        counts.label()
    );
    let total = counts.total();
    if total > 0 {
        let drawn = counts.segments().iter().filter(|s| s.0 > 0).count();
        let share = TRACK_HUNDREDTHS - drawn * MIN_SEGMENT_HUNDREDTHS;
        let mut running = 0;
        let mut floors = 0;
        let mut at = 0;
        for (count, class, _) in counts.segments() {
            if count == 0 {
                continue;
            }
            running += count;
            floors += MIN_SEGMENT_HUNDREDTHS;
            // Each edge rounds from the running count, so the widths always meet the track's end.
            let edge = floors + (2 * running * share + total) / (2 * total);
            out.push_str(&format!(
                "<rect class=\"seg {class}\" x=\"{}\" y=\"1\" width=\"{}\" height=\"8\"/>",
                units(at),
                units(edge - at)
            ));
            at = edge;
        }
    }
    out.push_str("</svg>");
    out
}

/// Writes the HTML legend of the bar, with each state's count when `counts` is given.
#[must_use]
pub fn coverage_legend(counts: Option<&CoverageCounts>) -> String {
    let shown = counts.copied().unwrap_or_default();
    let mut out = String::from("<span class=\"legend\">");
    for (count, class, word) in shown.segments() {
        out.push_str(&format!("<span><i class=\"{class}\"></i>{word}"));
        if counts.is_some() {
            out.push_str(&format!(" {count}"));
        }
        out.push_str("</span>");
    }
    out.push_str("</span>");
    out
}

/// Writes one `BarList` track: a horizontal bar in a 10-unit tall track.
///
/// The track is `aria-hidden` because the row's text carries the value, and the hover `<title>`
/// sits on the bar.
#[must_use]
pub fn bar_track(start: usize, end: usize, total: bool, tip: &str) -> String {
    let mut start = start.min(TRACK_HUNDREDTHS);
    let end = end.min(TRACK_HUNDREDTHS);
    if end <= start {
        return String::from(
            "<svg class=\"track\" viewBox=\"0 0 100 10\" preserveAspectRatio=\"none\" focusable=\"false\" aria-hidden=\"true\"></svg>",
        );
    }
    let w = end.saturating_sub(start).max(80);
    if start.saturating_add(w) > TRACK_HUNDREDTHS {
        start = TRACK_HUNDREDTHS.saturating_sub(w);
    }
    let class = if total { "bar1 tot" } else { "bar1" };
    let tip_esc = crate::links::escape_html(tip);
    format!(
        "<svg class=\"track\" viewBox=\"0 0 100 10\" preserveAspectRatio=\"none\" focusable=\"false\" aria-hidden=\"true\"><rect class=\"{class}\" x=\"{}\" y=\"1\" width=\"{}\" height=\"8\"><title>{tip_esc}</title></rect></svg>",
        units(start),
        units(w)
    )
}

/// Writes one `LineChart` plot: an SVG line chart in a 100x100 viewBox.
///
/// No area fill is drawn; the 2px stroke and the 8px dot with a 12px surface ring come from the
/// stylesheet with non-scaling strokes. Axis labels are HTML beside the plot.
#[must_use]
pub fn line_plot(points: &[(String, i64)], lo: i64, hi: i64, label: &str) -> String {
    let esc_label = crate::links::escape_html(label);
    let mut out = format!(
        "<svg class=\"plot\" viewBox=\"0 0 100 100\" preserveAspectRatio=\"none\" focusable=\"false\" role=\"img\" aria-label=\"{esc_label}\"><line class=\"grid\" x1=\"0\" x2=\"100\" y1=\"0\" y2=\"0\"/><line class=\"grid\" x1=\"0\" x2=\"100\" y1=\"50\" y2=\"50\"/><line class=\"grid\" x1=\"0\" x2=\"100\" y1=\"100\" y2=\"100\"/>"
    );
    if points.len() < 2 || hi <= lo {
        out.push_str("</svg>");
        return out;
    }

    let n = points.len();
    let denom = n - 1;
    let hi_128 = i128::from(hi);
    let lo_128 = i128::from(lo);
    let span = hi_128 - lo_128;

    let mut coords = Vec::with_capacity(n);
    for (i, (_, v)) in points.iter().enumerate() {
        let x = i.saturating_mul(TRACK_HUNDREDTHS) / denom;
        let v_128 = i128::from(*v);
        let dividend = (hi_128 - v_128).saturating_mul(10_000);
        let unclamped = dividend / span;
        let clamped = unclamped.clamp(0, 10_000);
        let y = usize::try_from(clamped).unwrap_or_default();
        coords.push((x, y));
    }

    for (i, (pt_label, val)) in points.iter().enumerate() {
        let start_x = if i == 0 {
            0
        } else {
            (coords[i - 1].0 + coords[i].0) / 2
        };
        let end_x = if i == n - 1 {
            TRACK_HUNDREDTHS
        } else {
            (coords[i].0 + coords[i + 1].0) / 2
        };
        let width = end_x.saturating_sub(start_x);
        let esc_pt_label = crate::links::escape_html(pt_label);
        out.push_str(&format!(
            "<rect class=\"hit\" x=\"{}\" y=\"0\" width=\"{}\" height=\"100\"><title>{esc_pt_label}: {val}</title></rect>",
            units(start_x),
            units(width)
        ));
    }

    out.push_str("<polyline class=\"line\" points=\"");
    for (i, &(x, y)) in coords.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&units(x));
        out.push(',');
        out.push_str(&units(y));
    }
    out.push_str("\"/>");

    if let Some(&(last_x, last_y)) = coords.last() {
        let x_last = units(last_x);
        let y_last = units(last_y);
        out.push_str(&format!(
            "<line class=\"ring\" x1=\"{x_last}\" y1=\"{y_last}\" x2=\"{x_last}\" y2=\"{y_last}\"/><line class=\"dot\" x1=\"{x_last}\" y1=\"{y_last}\" x2=\"{x_last}\" y2=\"{y_last}\"/>"
        ));
    }

    out.push_str("</svg>");
    out
}

#[cfg(test)]
mod tests {
    use super::{bar_track, line_plot};

    #[test]
    fn bar_track_total() {
        let svg = bar_track(0, 5000, true, "Total cost");
        assert!(svg.contains("bar1 tot"));
        assert!(svg.contains("aria-hidden=\"true\""));
        assert!(!svg.contains("<text"));
    }

    #[test]
    fn line_plot_three_points() {
        let points = vec![
            ("Jan".to_owned(), 100),
            ("Feb".to_owned(), 150),
            ("Mar".to_owned(), 120),
        ];
        let svg = line_plot(&points, 50, 200, "Trend");
        assert_eq!(svg.matches("<polyline").count(), 1);
        assert!(svg.contains("role=\"img\""));
        assert!(!svg.contains("<text"));
    }
}
