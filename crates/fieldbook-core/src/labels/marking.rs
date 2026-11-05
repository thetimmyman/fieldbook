//! Marking rules (`MarkingRules`), public acts (`PublicAct`), label contexts (`LabelContext`),
//! and pure text labellers (`markings_in`, `for_text`)
//! (`docs/decisions/0004-company-boundary.md#2-labels`, `FB0822`).

use std::path::Path;

use crate::check::Code;
use crate::scan::marking as banner_scan;

use super::id::{LabelId, MARKED, PUBLIC};
use super::set::LabelSet;
use super::store::{Refusal, RefuseReason, StoreRule};

/// One `[[marking]]` rule row mapping literal text (3..=80 characters) to a [`LabelSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkingRow {
    /// Literal text matched in any ASCII/Unicode case with runs of whitespace collapsed.
    pub matched: String,
    /// Labels assigned when `matched` is found in the text.
    pub labels: LabelSet,
}

impl MarkingRow {
    /// Creates a new [`MarkingRow`].
    #[must_use]
    pub fn new(matched: impl Into<String>, labels: LabelSet) -> Self {
        Self {
            matched: matched.into(),
            labels,
        }
    }
}

/// The marking rules used by [`markings_in`] and [`for_text`].
///
/// Built-in banner shapes from [`crate::scan::marking`] are always scanned; a shape covered by no
/// configured [`MarkingRow`] maps to `marked`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkingRules {
    rows: Vec<MarkingRow>,
}

impl MarkingRules {
    /// Returns the built-in marking rules (all banner shapes of `scan::marking` map to `marked`).
    #[must_use]
    pub const fn builtin() -> Self {
        Self { rows: Vec::new() }
    }

    /// Returns marking rules with the given `[[marking]]` rows.
    #[must_use]
    pub fn with_rows<I>(rows: I) -> Self
    where
        I: IntoIterator<Item = MarkingRow>,
    {
        Self {
            rows: rows.into_iter().collect(),
        }
    }

    /// Returns the configured `[[marking]]` rows.
    #[must_use]
    pub fn rows(&self) -> &[MarkingRow] {
        &self.rows
    }
}

/// Closed list of acts by which text may become `public` (`ADR 0004 §2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicAct {
    kind: PublicActKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublicActKind {
    Edition,
    SourceRow,
    Typed,
}

impl PublicAct {
    /// The edition build act.
    #[must_use]
    pub const fn edition() -> Self {
        Self {
            kind: PublicActKind::Edition,
        }
    }

    /// A policy `[[source]]` row act for a stream whose `folder` lies inside `root` on whole
    /// canonical path components. Returns `None` when `folder` is outside `root` or either path is
    /// relative or contains `.` or `..` components.
    #[must_use]
    pub fn source_row<P: AsRef<Path>, Q: AsRef<Path>>(folder: P, root: Q) -> Option<Self> {
        let folder_comps = canonical_components(folder.as_ref())?;
        let root_comps = canonical_components(root.as_ref())?;
        if folder_comps.len() < root_comps.len() {
            return None;
        }
        if folder_comps[..root_comps.len()] == root_comps[..] {
            Some(Self {
                kind: PublicActKind::SourceRow,
            })
        } else {
            None
        }
    }

    /// A person's typed confirmation (`--confirm-public` for home `add` and `new`).
    ///
    /// Valid only when `has_company_store` is `false` and `confirmed` is `true`; otherwise returns
    /// a [`Refusal`] with `FB0822`.
    pub fn typed(has_company_store: bool, confirmed: bool) -> Result<Self, Refusal> {
        let _ = Self {
            kind: PublicActKind::Typed,
        };
        if has_company_store || !confirmed {
            return Err(Refusal {
                code: Code::Fb0822,
                reason: RefuseReason::Label,
                labels: LabelSet::public(),
            });
        }
        Ok(Self {
            kind: PublicActKind::Typed,
        })
    }
}

/// Parses a path into normalized whole components, accepting both POSIX (`/`) and Windows drive/UNC
/// (`C:\...`, `\\server\share\...`) absolute paths, and rejecting relative paths or `.` / `..`.
fn canonical_components(path: &Path) -> Option<Vec<String>> {
    let raw = path.to_str()?;
    if raw.is_empty() {
        return None;
    }

    let bytes = raw.as_bytes();
    let (prefix, rest, windows_path) = if let Some(trimmed) = raw.strip_prefix("\\\\") {
        let mut parts = trimmed.split(['\\', '/']);
        let server = parts.next().filter(|s| !s.is_empty())?;
        let share = parts.next().filter(|s| !s.is_empty())?;
        if server == "." || server == ".." || share == "." || share == ".." {
            return None;
        }
        let consumed = 2 + server.len() + 1 + share.len();
        let tail = raw.get(consumed..).unwrap_or("");
        (
            vec![format!(
                "\\\\{}\\{}",
                server.to_ascii_lowercase(),
                share.to_ascii_lowercase()
            )],
            tail,
            true,
        )
    } else if let Some(stripped) = raw.strip_prefix('/') {
        (vec!["/".to_owned()], stripped, false)
    } else if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        let drive = (bytes[0].to_ascii_lowercase() as char).to_string() + ":";
        (vec![drive], &raw[3..], true)
    } else {
        return None;
    };

    let mut comps = prefix;
    if windows_path {
        for seg in rest.split(['/', '\\']) {
            if seg.is_empty() {
                continue;
            }
            if seg == "." || seg == ".." {
                return None;
            }
            comps.push(seg.to_ascii_lowercase());
        }
    } else {
        for seg in rest.split('/') {
            if seg.is_empty() {
                continue;
            }
            if seg == "." || seg == ".." {
                return None;
            }
            comps.push(seg.to_owned());
        }
    }
    Some(comps)
}

/// Context carrying the default labels of the place text came from and whether a [`PublicAct`]
/// authorized `public`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelContext {
    default_ids: Vec<LabelId>,
    public_act: Option<PublicAct>,
}

impl LabelContext {
    /// Context with no default labels and no [`PublicAct`].
    #[must_use]
    pub const fn none() -> Self {
        Self {
            default_ids: Vec::new(),
            public_act: None,
        }
    }

    /// Context using the `default` of `rule` (with no [`PublicAct`]).
    #[must_use]
    pub fn store(rule: &StoreRule) -> Self {
        Self {
            default_ids: rule.default.ids().to_vec(),
            public_act: None,
        }
    }

    /// Context carrying a validated [`PublicAct`].
    #[must_use]
    pub fn act(act: PublicAct) -> Self {
        let _ = act.kind;
        Self {
            default_ids: vec![PUBLIC],
            public_act: Some(act),
        }
    }

    /// Returns `true` when this context carries a valid [`PublicAct`].
    #[must_use]
    pub const fn has_public_act(&self) -> bool {
        self.public_act.is_some()
    }
}

impl Default for LabelContext {
    fn default() -> Self {
        Self::none()
    }
}

/// Collapses runs of whitespace to a single ASCII space and lowercases characters for
/// case-insensitive, whitespace-normalized matching.
fn normalize_ws_lower(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_ws = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() {
                in_ws = true;
            }
        } else {
            if in_ws {
                out.push(' ');
                in_ws = false;
            }
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        }
    }
    out
}

/// Normalized lowercase text alongside the start and end byte offsets in the original input
/// for each byte of `norm`.
struct NormalizedText {
    norm: String,
    orig_start: Vec<usize>,
    orig_end: Vec<usize>,
}

impl NormalizedText {
    fn from_input(input: &str) -> Self {
        let mut norm = String::with_capacity(input.len());
        let mut orig_start = Vec::with_capacity(input.len());
        let mut orig_end = Vec::with_capacity(input.len());
        let mut ws_span: Option<(usize, usize)> = None;

        for (idx, ch) in input.char_indices() {
            let ch_end = idx + ch.len_utf8();
            if ch.is_whitespace() {
                if !norm.is_empty() {
                    ws_span = Some(match ws_span {
                        Some((s, _)) => (s, ch_end),
                        None => (idx, ch_end),
                    });
                }
            } else {
                if let Some((ws_s, ws_e)) = ws_span.take() {
                    norm.push(' ');
                    orig_start.push(ws_s);
                    orig_end.push(ws_e);
                }
                let mut buf = [0u8; 4];
                for lower in ch.to_lowercase() {
                    let encoded = lower.encode_utf8(&mut buf);
                    norm.push_str(encoded);
                    for _ in 0..encoded.len() {
                        orig_start.push(idx);
                        orig_end.push(ch_end);
                    }
                }
            }
        }

        Self {
            norm,
            orig_start,
            orig_end,
        }
    }

    /// Returns all original-input byte spans `(start, end)` where `needle_norm` matches `self.norm`.
    fn match_spans(&self, needle_norm: &str) -> Vec<(usize, usize)> {
        if needle_norm.is_empty() || needle_norm.len() > self.norm.len() {
            return Vec::new();
        }
        let mut spans = Vec::new();
        let mut search_from = 0usize;
        while let Some(rel) = self.norm[search_from..].find(needle_norm) {
            let m_start = search_from + rel;
            let m_end = m_start + needle_norm.len();
            let orig_s = self.orig_start[m_start];
            let orig_e = self.orig_end[m_end - 1];
            spans.push((orig_s, orig_e));
            let next_step = self.norm[m_start..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            search_from = m_start + next_step;
        }
        spans
    }
}

/// Pure scan of `text` against `rules` and the built-in banner shapes of [`crate::scan::marking`].
///
/// Unions the labels of every [`MarkingRow`] that matches, plus `marked` for any built-in shape
/// occurrence that no matching row span covers. Returns `None` when `text` holds no marking.
#[must_use]
pub fn markings_in(text: &str, rules: &MarkingRules) -> Option<LabelSet> {
    let norm_text = NormalizedText::from_input(text);
    let mut found_ids: Vec<LabelId> = Vec::new();
    let mut matched_spans: Vec<(usize, usize)> = Vec::new();

    for row in &rules.rows {
        let char_count = row.matched.chars().count();
        if !(3..=80).contains(&char_count) {
            continue;
        }
        let norm_row = normalize_ws_lower(&row.matched);
        if norm_row.is_empty() {
            continue;
        }
        let spans = norm_text.match_spans(&norm_row);
        if !spans.is_empty() {
            found_ids.extend(row.labels.ids().iter().cloned());
            matched_spans.extend(spans);
        }
    }

    let builtin_findings = banner_scan::scan(text);
    for finding in &builtin_findings {
        let covered = matched_spans
            .iter()
            .any(|&(r_start, r_end)| r_start <= finding.span.start && finding.span.end <= r_end);
        if !covered {
            found_ids.push(MARKED);
        }
    }

    if found_ids.is_empty() {
        None
    } else {
        Some(LabelSet::from_ids(found_ids))
    }
}

/// Pure text labeller (`labels::for_text`).
///
/// Combines [`markings_in`] with `context`'s default labels:
/// - When nothing is found, returns `unlabelled`.
/// - When the normalized result is `public`-only and `context` carries no valid [`PublicAct`],
///   returns `unlabelled`, even if a store default or global marking row supplied `public`.
#[must_use]
pub fn for_text(text: &str, rules: &MarkingRules, context: &LabelContext) -> LabelSet {
    let mut ids: Vec<LabelId> = context.default_ids.clone();
    if let Some(marked_set) = markings_in(text, rules) {
        ids.extend(marked_set.ids().iter().cloned());
    }
    let set = LabelSet::from_ids(ids);
    if set.is_public() && !context.has_public_act() {
        return LabelSet::unlabelled();
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::id::UNLABELLED;
    use crate::labels::set::LabelAllow;
    use crate::labels::store::StoreId;

    fn id(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
    }

    fn synthetic_banner_text(index: usize) -> String {
        let pat = banner_scan::PATTERNS[index].text;
        format!("Header line: {pat} — synthetic test body.")
    }

    #[test]
    fn plain_text_with_no_act_is_unlabelled() {
        let set = for_text(
            "Plain synthetic paragraph.",
            &MarkingRules::builtin(),
            &LabelContext::none(),
        );
        assert_eq!(set, LabelSet::unlabelled());
    }

    #[test]
    fn plain_text_with_an_edition_act_is_public() {
        let set = for_text(
            "Plain synthetic paragraph.",
            &MarkingRules::builtin(),
            &LabelContext::act(PublicAct::edition()),
        );
        assert_eq!(set, LabelSet::public());
    }

    #[test]
    fn a_banner_without_a_rule_is_marked() {
        let text = synthetic_banner_text(0);
        let set = for_text(
            &text,
            &MarkingRules::builtin(),
            &LabelContext::act(PublicAct::edition()),
        );
        assert_eq!(set, LabelSet::marked());
    }

    #[test]
    fn a_rule_maps_a_marking_to_its_labels() {
        let banner = banner_scan::PATTERNS[13].text; // from scan's own list
        let text = format!("Notice: {banner} synthetic figure");
        let np = id("np-0007");
        let rules =
            MarkingRules::with_rows([MarkingRow::new(banner, LabelSet::from_ids([np.clone()]))]);
        let set = for_text(&text, &rules, &LabelContext::none());
        assert_eq!(set, LabelSet::from_ids([np]));
    }

    #[test]
    fn a_rule_row_is_matched_in_any_case_and_across_whitespace() {
        let banner = banner_scan::PATTERNS[14].text; // two-word pattern from scan's list
        let split_lower = banner.to_ascii_lowercase().replace(' ', "   \n\t ");
        let text = format!("Header: {split_lower} footer");
        let bid = id("bid-0003");
        let rules =
            MarkingRules::with_rows([MarkingRow::new(banner, LabelSet::from_ids([bid.clone()]))]);
        assert_eq!(
            markings_in(&text, &rules),
            Some(LabelSet::from_ids([bid.clone()]))
        );
        assert_eq!(
            for_text(&text, &rules, &LabelContext::none()),
            LabelSet::from_ids([bid])
        );
    }

    #[test]
    fn no_default_and_no_marking_is_unlabelled() {
        let set = for_text(
            "Ordinary text without banners.",
            &MarkingRules::builtin(),
            &LabelContext::none(),
        );
        assert_eq!(set, LabelSet::unlabelled());
    }

    #[test]
    fn a_marking_beside_the_default_keeps_both() {
        let banner = banner_scan::PATTERNS[13].text;
        let text = format!("Synthetic {banner} text");
        let np = id("np-0007");
        let bid = id("bid-0003");
        let rules =
            MarkingRules::with_rows([MarkingRow::new(banner, LabelSet::from_ids([bid.clone()]))]);
        let store = StoreRule {
            store: StoreId::Overlay,
            holds: LabelAllow::from_ids([PUBLIC, UNLABELLED, np.clone(), bid.clone()]),
            default: LabelSet::from_ids([np.clone()]),
            readers: None,
            changers: None,
            hold: false,
        };
        let set = for_text(&text, &rules, &LabelContext::store(&store));
        assert_eq!(set, LabelSet::from_ids([bid, np]));
    }

    #[test]
    fn markings_in_is_none_for_plain_text() {
        assert_eq!(
            markings_in("Plain synthetic text.", &MarkingRules::builtin()),
            None
        );
    }

    #[test]
    fn same_text_same_labels() {
        let text = synthetic_banner_text(2);
        let rules = MarkingRules::builtin();
        let ctx = LabelContext::none();
        let first = for_text(&text, &rules, &ctx);
        let second = for_text(&text, &rules, &ctx);
        assert_eq!(first, second);
    }

    #[test]
    fn a_source_folder_outside_its_root_gives_no_act() {
        assert!(
            PublicAct::source_row("/srv/company/quotes/stream", "/srv/company/quotes").is_some()
        );
        assert!(
            PublicAct::source_row("/srv/company/quotes-other", "/srv/company/quotes").is_none()
        );
        assert!(PublicAct::source_row("/srv/other/quotes", "/srv/company/quotes").is_none());
        assert!(PublicAct::source_row("relative/path", "/srv/company/quotes").is_none());
        assert!(
            PublicAct::source_row("/srv/company/quotes/../other", "/srv/company/quotes").is_none()
        );

        let valid_act =
            PublicAct::source_row("/srv/company/public/stream", "/srv/company/public").unwrap();
        assert_eq!(
            for_text(
                "Plain stream text",
                &MarkingRules::builtin(),
                &LabelContext::act(valid_act)
            ),
            LabelSet::public()
        );
    }

    #[test]
    fn a_typed_public_is_refused_when_a_company_store_exists() {
        let err = PublicAct::typed(true, true).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
        assert_eq!(err.reason, RefuseReason::Label);
    }

    #[test]
    fn a_typed_public_needs_the_confirmation() {
        let err = PublicAct::typed(false, false).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
        assert_eq!(err.reason, RefuseReason::Label);
    }

    #[test]
    fn a_confirmed_typed_public_gives_public_with_no_company_store() {
        let act = PublicAct::typed(false, true).unwrap();
        let set = for_text(
            "Synthetic public source text",
            &MarkingRules::builtin(),
            &LabelContext::act(act),
        );
        assert_eq!(set, LabelSet::public());
    }

    #[test]
    fn store_default_cannot_grant_public() {
        let store_with_public_default = StoreRule {
            store: StoreId::Working,
            holds: LabelAllow::from_ids([PUBLIC, UNLABELLED]),
            default: LabelSet::public(),
            readers: None,
            changers: None,
            hold: false,
        };
        let set = for_text(
            "Plain note in working store",
            &MarkingRules::builtin(),
            &LabelContext::store(&store_with_public_default),
        );
        assert_eq!(set, LabelSet::unlabelled());
    }

    #[test]
    fn marking_rule_cannot_grant_public() {
        let banner = banner_scan::PATTERNS[13].text;
        let rules = MarkingRules::with_rows([
            MarkingRow::new("approved public release", LabelSet::public()),
            MarkingRow::new(banner, LabelSet::public()),
        ]);
        assert_eq!(
            for_text(
                "This document says approved public release.",
                &rules,
                &LabelContext::none()
            ),
            LabelSet::unlabelled()
        );
        assert_eq!(
            for_text(
                &format!("Header {banner} text"),
                &rules,
                &LabelContext::none()
            ),
            LabelSet::unlabelled()
        );

        // Positive cases: restrictive labels beside public are preserved, and real PublicActs work.
        let np = id("np-0007");
        let restrictive_rules = MarkingRules::with_rows([MarkingRow::new(
            banner,
            LabelSet::from_ids([PUBLIC, np.clone()]),
        )]);
        assert_eq!(
            for_text(
                &format!("Header {banner} text"),
                &restrictive_rules,
                &LabelContext::none()
            ),
            LabelSet::from_ids([np])
        );
    }

    #[test]
    fn posix_backslash_in_segment_is_not_a_path_separator() {
        assert!(
            PublicAct::source_row(Path::new(r"/srv/public\private"), Path::new("/srv/public"))
                .is_none()
        );
    }

    #[test]
    fn uncovered_second_banner_occurrence_still_adds_marked() {
        let banner = banner_scan::PATTERNS[9].text; // "EXPORT CONTROLLED"
        let aa = id("aa-0001");
        let rules = MarkingRules::with_rows([MarkingRow::new(
            format!("{banner} ALPHA"),
            LabelSet::from_ids([aa.clone()]),
        )]);
        let text = format!("{banner} ALPHA section one; standalone {banner} section two");
        let found = markings_in(&text, &rules).expect("marking expected");
        assert_eq!(found, LabelSet::from_ids([aa, MARKED]));
    }
}
