//! Re-exports of shared core build and edition metadata (`fieldbook_core::edition`).

pub use fieldbook_core::edition::{
    BuildInfo, BuildProfile, DEFAULT_WORKBOOK_EPOCH, EditionInfo, MetadataError, RenderContext,
};
use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::vocab::{DraftedBy, Verification};

/// Build-time claim counts for the unreleased build status banner (never constants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildStatus {
    /// M: every claim row of the register the build renders.
    pub total: usize,
    /// A: rows whose `drafted_by` is `codex` or `ai`.
    pub ai_drafted: usize,
    /// U: rows whose `verification` is `unchecked`.
    pub unchecked: usize,
    /// S: rows whose `verification` is `primary_source_supported`.
    pub supported: usize,
}

impl BuildStatus {
    /// Computes claim counts in a single deterministic pass over `view.claims()`.
    #[must_use]
    pub fn from_view(view: &LabelledView) -> Self {
        let mut total = 0;
        let mut ai_drafted = 0;
        let mut unchecked = 0;
        let mut supported = 0;

        for located in view.claims() {
            let claim = &located.value;
            total += 1;
            if matches!(claim.drafted_by, DraftedBy::Codex | DraftedBy::Ai) {
                ai_drafted += 1;
            }
            match claim.verification {
                Verification::Unchecked => unchecked += 1,
                Verification::PrimarySourceSupported => supported += 1,
                Verification::Contradicted | Verification::Stale | Verification::Unresolved => {}
            }
        }

        Self {
            total,
            ai_drafted,
            unchecked,
            supported,
        }
    }

    /// Formats the AI-drafted and unchecked counts sentence, or "No claims recorded." if empty.
    #[must_use]
    pub fn count_sentence(&self) -> String {
        if self.total == 0 {
            return "No claims recorded.".to_owned();
        }

        let a = format_comma_separated(self.ai_drafted);
        let m = format_comma_separated(self.total);
        let u = format_comma_separated(self.unchecked);

        let noun = if self.total == 1 { "claim" } else { "claims" };
        let verb_a = if self.ai_drafted == 1 { "is" } else { "are" };
        let verb_u = if self.unchecked == 1 { "is" } else { "are" };

        format!("{a} of {m} {noun} {verb_a} AI-drafted; {u} of {m} {verb_u} unchecked.")
    }

    /// Formats the supported verification count sentence, or `None` if no claims are checked.
    #[must_use]
    pub fn supported_sentence(&self) -> Option<String> {
        if self.total == 0 || self.unchecked >= self.total {
            return None;
        }

        let s = format_comma_separated(self.supported);
        let m = format_comma_separated(self.total);
        let has = if self.supported == 1 { "has" } else { "have" };

        Some(format!(
            "{s} of {m} {has} been checked and found supported."
        ))
    }
}

/// The line of an empty Changes to review panel, from the `total` claims in scope and the
/// `unchecked` among them. It says what was observed: "Nothing upstream has moved." only when
/// every claim in scope has been checked, so a panel over unchecked claims never reports a result
/// nobody observed. A claim is checked when its `verification` is any value but `unchecked`.
#[must_use]
pub(crate) fn no_changes_line(total: usize, unchecked: usize) -> String {
    let checked = total.saturating_sub(unchecked);
    if total == 0 {
        "No claims here yet.".to_owned()
    } else if checked == 0 && total == 1 {
        "Not established: the claim here has not been checked yet.".to_owned()
    } else if checked == 0 {
        format!(
            "Not established: none of the {} claims here has been checked yet.",
            format_comma_separated(total)
        )
    } else if unchecked == 0 {
        "Nothing upstream has moved.".to_owned()
    } else {
        let moved = if checked == 1 {
            "The one checked claim here has not moved".to_owned()
        } else {
            format!(
                "None of the {} checked claims here has moved",
                format_comma_separated(checked)
            )
        };
        let left = if unchecked == 1 {
            "1 is".to_owned()
        } else {
            format!("{} are", format_comma_separated(unchecked))
        };
        format!("{moved}; {left} unchecked.")
    }
}

fn format_comma_separated(n: usize) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len <= 3 {
        return s;
    }
    let mut out = String::with_capacity(len + (len - 1) / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(char::from(b));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_comma_separated_formats_numbers() {
        assert_eq!(format_comma_separated(0), "0");
        assert_eq!(format_comma_separated(999), "999");
        assert_eq!(format_comma_separated(1_000), "1,000");
        assert_eq!(format_comma_separated(28_857), "28,857");
        assert_eq!(format_comma_separated(1_000_000), "1,000,000");
    }

    #[test]
    fn count_sentence_empty_register() {
        let status = BuildStatus::default();
        assert_eq!(status.count_sentence(), "No claims recorded.");
        assert_eq!(status.supported_sentence(), None);
    }

    #[test]
    fn count_sentence_examples() {
        let s1 = BuildStatus {
            total: 4,
            ai_drafted: 3,
            unchecked: 4,
            supported: 0,
        };
        assert_eq!(
            s1.count_sentence(),
            "3 of 4 claims are AI-drafted; 4 of 4 are unchecked."
        );

        let s2 = BuildStatus {
            total: 1,
            ai_drafted: 1,
            unchecked: 1,
            supported: 0,
        };
        assert_eq!(
            s2.count_sentence(),
            "1 of 1 claim is AI-drafted; 1 of 1 is unchecked."
        );

        let s3 = BuildStatus {
            total: 40,
            ai_drafted: 0,
            unchecked: 40,
            supported: 0,
        };
        assert_eq!(
            s3.count_sentence(),
            "0 of 40 claims are AI-drafted; 40 of 40 are unchecked."
        );

        let s4 = BuildStatus {
            total: 28_857,
            ai_drafted: 1_234,
            unchecked: 5_678,
            supported: 10_000,
        };
        assert_eq!(
            s4.count_sentence(),
            "1,234 of 28,857 claims are AI-drafted; 5,678 of 28,857 are unchecked."
        );
        assert_eq!(
            s4.supported_sentence().as_deref(),
            Some("10,000 of 28,857 have been checked and found supported.")
        );
    }

    #[test]
    fn supported_sentence_presence_and_grammar() {
        let s_one = BuildStatus {
            total: 3,
            ai_drafted: 0,
            unchecked: 2,
            supported: 1,
        };
        assert_eq!(
            s_one.supported_sentence().as_deref(),
            Some("1 of 3 has been checked and found supported.")
        );

        let s_many = BuildStatus {
            total: 3,
            ai_drafted: 0,
            unchecked: 0,
            supported: 2,
        };
        assert_eq!(
            s_many.supported_sentence().as_deref(),
            Some("2 of 3 have been checked and found supported.")
        );

        let s_all_unchecked = BuildStatus {
            total: 3,
            ai_drafted: 0,
            unchecked: 3,
            supported: 0,
        };
        assert_eq!(s_all_unchecked.supported_sentence(), None);

        let s_unchecked_exceeds_total = BuildStatus {
            total: 3,
            ai_drafted: 0,
            unchecked: 4,
            supported: 0,
        };
        assert_eq!(s_unchecked_exceeds_total.supported_sentence(), None);
    }
}
