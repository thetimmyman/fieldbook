//! Inline SVG written by the build (`build-home-and-areas`; `build-charts` extends this file).
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
