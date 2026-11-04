//! The `check-page-links` pass: `FB0110` to `FB0113`, `FB0115` and `FB0136`.
//!
//! Validates page front-matter references (`prereq`, `links`, `supersedes`, `superseded_by`,
//! `terms`, `questions`, `conflicts`), prerequisite stage ordering and cycles, page supersession
//! reciprocity and loops, and relative links in page bodies.

use std::collections::{BTreeMap, BTreeSet};

use crate::ids::PageId;
use crate::schema::PageMeta;

use super::markdown;
use super::{Code, Diagnostic, Model, Options, PageEntry, Report};

/// Resolution of a link target in a page body relative to `page_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinkResolution {
    /// The link has a URI scheme or climbs out of the Corpus pages tree (`FB0135` in `check-page-body`).
    OutsideCorpus,
    /// The link resolves to a path inside the Corpus (`FB0136` if absent from [`super::Tree`]).
    InsideCorpus(String),
}

/// Run the `check-page-links` pass over `model` and append diagnostics to `report`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    let pages = model.pages();
    if pages.is_empty() {
        return;
    }

    let mut page_by_id: BTreeMap<PageId, &PageEntry> = BTreeMap::new();
    for page in pages {
        page_by_id.entry(page.id.clone()).or_insert(page);
    }

    check_front_matter_refs(model, &page_by_id, options.strict, report);
    check_prereq_stages_and_cycles(model, &page_by_id, options.strict, report);
    check_page_supersession(model, &page_by_id, options.strict, report);
    check_body_links(model, options.strict, report);
}

fn field_loc(meta: &PageMeta, key: &str) -> (usize, usize) {
    meta.field_location(key)
        .and_then(|(l, c)| Some((usize::try_from(l).ok()?, usize::try_from(c).ok()?)))
        .unwrap_or((1, 1))
}

fn check_front_matter_refs(
    model: &Model,
    _page_by_id: &BTreeMap<PageId, &PageEntry>,
    strict: bool,
    report: &mut Report,
) {
    let index = model.index();
    for page in model.pages() {
        let meta = &page.meta;

        // FB0110: prereq, links, supersedes, superseded_by
        let (prereq_line, prereq_col) = field_loc(meta, "prereq");
        for id in &meta.prereq {
            if !index.contains_page(id) {
                report.push(Diagnostic::new(
                    Code::Fb0110,
                    strict,
                    &page.path,
                    prereq_line,
                    prereq_col,
                    format!("`prereq` references unknown page `{id}`"),
                ));
            }
        }

        let (links_line, links_col) = field_loc(meta, "links");
        for id in &meta.links {
            if !index.contains_page(id) {
                report.push(Diagnostic::new(
                    Code::Fb0110,
                    strict,
                    &page.path,
                    links_line,
                    links_col,
                    format!("`links` references unknown page `{id}`"),
                ));
            }
        }

        let (sup_line, sup_col) = field_loc(meta, "supersedes");
        for id in &meta.supersedes {
            if !index.contains_page(id) {
                report.push(Diagnostic::new(
                    Code::Fb0110,
                    strict,
                    &page.path,
                    sup_line,
                    sup_col,
                    format!("`supersedes` references unknown page `{id}`"),
                ));
            }
        }

        let (sup_by_line, sup_by_col) = field_loc(meta, "superseded_by");
        for id in &meta.superseded_by {
            if !index.contains_page(id) {
                report.push(Diagnostic::new(
                    Code::Fb0110,
                    strict,
                    &page.path,
                    sup_by_line,
                    sup_by_col,
                    format!("`superseded_by` references unknown page `{id}`"),
                ));
            }
        }

        // FB0112: terms
        let (terms_line, terms_col) = field_loc(meta, "terms");
        for term in &meta.terms {
            if !index.contains_term(term) {
                report.push(Diagnostic::new(
                    Code::Fb0112,
                    strict,
                    &page.path,
                    terms_line,
                    terms_col,
                    format!("`terms` references unknown glossary term `{term}`"),
                ));
            }
        }

        // FB0113: questions and conflicts
        let (q_line, q_col) = field_loc(meta, "questions");
        for qid in &meta.questions {
            if !index.contains_question(qid) {
                report.push(Diagnostic::new(
                    Code::Fb0113,
                    strict,
                    &page.path,
                    q_line,
                    q_col,
                    format!("`questions` references unknown question `{qid}`"),
                ));
            }
        }

        let (c_line, c_col) = field_loc(meta, "conflicts");
        for xid in &meta.conflicts {
            if !index.contains_conflict(xid) {
                report.push(Diagnostic::new(
                    Code::Fb0113,
                    strict,
                    &page.path,
                    c_line,
                    c_col,
                    format!("`conflicts` references unknown conflict `{xid}`"),
                ));
            }
        }
    }
}

fn check_prereq_stages_and_cycles(
    model: &Model,
    page_by_id: &BTreeMap<PageId, &PageEntry>,
    strict: bool,
    report: &mut Report,
) {
    let mut graph: BTreeMap<PageId, Vec<PageId>> = BTreeMap::new();

    for page in model.pages() {
        let (line, col) = field_loc(&page.meta, "prereq");
        let mut edges = Vec::new();
        for prereq_id in &page.meta.prereq {
            if prereq_id == &page.id {
                report.push(Diagnostic::new(
                    Code::Fb0111,
                    strict,
                    &page.path,
                    line,
                    col,
                    format!("`prereq` cycle: page `{}` lists itself", page.id),
                ));
                continue;
            }
            let Some(prereq_page) = page_by_id.get(prereq_id) else {
                continue;
            };
            if prereq_page.meta.stage > page.meta.stage {
                report.push(Diagnostic::new(
                    Code::Fb0111,
                    strict,
                    &page.path,
                    line,
                    col,
                    format!(
                        "page `{}` (stage {}) lists prerequisite `{prereq_id}` with later stage {}",
                        page.id, page.meta.stage, prereq_page.meta.stage
                    ),
                ));
            }
            edges.push(prereq_id.clone());
        }
        if !edges.is_empty() {
            edges.sort();
            edges.dedup();
            graph.entry(page.id.clone()).or_default().extend(edges);
        }
    }

    for edges in graph.values_mut() {
        edges.sort();
        edges.dedup();
    }

    for cycle in find_cycles(&graph) {
        if let Some(first_id) = cycle.iter().next()
            && let Some(first_page) = page_by_id.get(first_id)
        {
            let (line, col) = field_loc(&first_page.meta, "prereq");
            let names: Vec<String> = cycle.iter().map(ToString::to_string).collect();
            report.push(Diagnostic::new(
                Code::Fb0111,
                strict,
                &first_page.path,
                line,
                col,
                format!("`prereq` cycle: {}", names.join(" -> ")),
            ));
        }
    }
}

fn check_page_supersession(
    model: &Model,
    page_by_id: &BTreeMap<PageId, &PageEntry>,
    strict: bool,
    report: &mut Report,
) {
    for page in model.pages() {
        if page.meta.supersedes.contains(&page.id) {
            let (line, col) = field_loc(&page.meta, "supersedes");
            report.push(Diagnostic::new(
                Code::Fb0115,
                strict,
                &page.path,
                line,
                col,
                format!("page `{}` cannot supersede itself", page.id),
            ));
        }
        if page.meta.superseded_by.contains(&page.id) {
            let (line, col) = field_loc(&page.meta, "superseded_by");
            report.push(Diagnostic::new(
                Code::Fb0115,
                strict,
                &page.path,
                line,
                col,
                format!("page `{}` cannot be superseded by itself", page.id),
            ));
        }
    }

    let mut graph: BTreeMap<PageId, Vec<PageId>> = BTreeMap::new();

    for (id, page) in page_by_id {
        for old_id in &page.meta.supersedes {
            if old_id == id {
                continue;
            }
            let Some(old_page) = page_by_id.get(old_id) else {
                continue;
            };
            if !old_page.meta.superseded_by.contains(id) {
                let (line, col) = field_loc(&page.meta, "supersedes");
                report.push(Diagnostic::new(
                    Code::Fb0115,
                    strict,
                    &page.path,
                    line,
                    col,
                    format!(
                        "page `{id}` lists `{old_id}` in `supersedes`, but `{old_id}` does not list `{id}` in `superseded_by`"
                    ),
                ));
            }
            graph.entry(old_id.clone()).or_default().push(id.clone());
        }

        for new_id in &page.meta.superseded_by {
            if new_id == id {
                continue;
            }
            let Some(new_page) = page_by_id.get(new_id) else {
                continue;
            };
            if !new_page.meta.supersedes.contains(id) {
                let (line, col) = field_loc(&page.meta, "superseded_by");
                report.push(Diagnostic::new(
                    Code::Fb0115,
                    strict,
                    &page.path,
                    line,
                    col,
                    format!(
                        "page `{id}` lists `{new_id}` in `superseded_by`, but `{new_id}` does not list `{id}` in `supersedes`"
                    ),
                ));
            }
            graph.entry(id.clone()).or_default().push(new_id.clone());
        }
    }

    for edges in graph.values_mut() {
        edges.sort();
        edges.dedup();
    }

    for cycle in find_cycles(&graph) {
        if let Some(first_id) = cycle.iter().next()
            && let Some(first_page) = page_by_id.get(first_id)
        {
            let key = if !first_page.meta.superseded_by.is_empty() {
                "superseded_by"
            } else {
                "supersedes"
            };
            let (line, col) = field_loc(&first_page.meta, key);
            let names: Vec<String> = cycle.iter().map(ToString::to_string).collect();
            report.push(Diagnostic::new(
                Code::Fb0115,
                strict,
                &first_page.path,
                line,
                col,
                format!("page supersession loop: {}", names.join(" -> ")),
            ));
        }
    }
}

fn check_body_links(model: &Model, strict: bool, report: &mut Report) {
    let tree = model.tree();
    for page in model.pages() {
        let body_start_line = usize::try_from(page.body_line).unwrap_or(1);
        for link in markdown::body_links(&page.body, body_start_line) {
            if let LinkResolution::InsideCorpus(resolved) =
                resolve_page_link(&page.path, &link.target)
                && tree.get(&resolved).is_none()
            {
                report.push(Diagnostic::new(
                    Code::Fb0136,
                    strict,
                    &page.path,
                    link.line,
                    link.col,
                    format!(
                        "relative link `{}` resolves to missing file `{resolved}`",
                        link.target
                    ),
                ));
            }
        }
    }
}

/// Resolve a link target written in `page_path` into [`LinkResolution`].
#[must_use]
pub(crate) fn resolve_page_link(page_path: &str, target: &str) -> LinkResolution {
    if has_scheme(target) {
        return LinkResolution::OutsideCorpus;
    }
    let path_part = target.split_once('#').map_or(target, |(p, _)| p);
    if path_part.is_empty() {
        return LinkResolution::InsideCorpus(page_path.to_owned());
    }
    if path_part.starts_with('/') {
        return LinkResolution::OutsideCorpus;
    }

    let page_dir = page_path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut segments: Vec<&str> = page_dir.split('/').filter(|s| !s.is_empty()).collect();

    for seg in path_part.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop();
                if segments.first() != Some(&"corpus") {
                    return LinkResolution::OutsideCorpus;
                }
            }
            other => segments.push(other),
        }
    }

    if segments.first() != Some(&"corpus") {
        return LinkResolution::OutsideCorpus;
    }

    LinkResolution::InsideCorpus(segments.join("/"))
}

/// Returns `true` when `target` begins with a URI scheme (`http:`, `https:`, `mailto:`, etc.) or
/// protocol-relative `//`.
#[must_use]
pub(crate) fn has_scheme(target: &str) -> bool {
    if target.starts_with("//") {
        return true;
    }
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    if scheme.is_empty() || scheme.contains(['/', '#', '?']) {
        return false;
    }
    let mut bytes = scheme.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// Find strongly connected loop components (size >= 2) in `graph` using Tarjan's SCC algorithm,
/// returning each component's node set in `BTreeSet` key order, sorted by minimum key.
fn find_cycles<K: Ord + Clone>(graph: &BTreeMap<K, Vec<K>>) -> Vec<BTreeSet<K>> {
    struct Tarjan<'a, K> {
        graph: &'a BTreeMap<K, Vec<K>>,
        index: usize,
        indices: BTreeMap<K, usize>,
        lowlink: BTreeMap<K, usize>,
        stack: Vec<K>,
        on_stack: BTreeSet<K>,
        sccs: Vec<BTreeSet<K>>,
    }

    fn strongconnect<K: Ord + Clone>(t: &mut Tarjan<'_, K>, v: &K) {
        let v_idx = t.index;
        t.index += 1;
        t.indices.insert(v.clone(), v_idx);
        t.lowlink.insert(v.clone(), v_idx);
        t.stack.push(v.clone());
        t.on_stack.insert(v.clone());

        if let Some(neighbors) = t.graph.get(v) {
            for w in neighbors {
                if !t.indices.contains_key(w) {
                    strongconnect(t, w);
                    let w_low = t.lowlink[w];
                    let v_low = t.lowlink[v];
                    if w_low < v_low {
                        t.lowlink.insert(v.clone(), w_low);
                    }
                } else if t.on_stack.contains(w) {
                    let w_idx = t.indices[w];
                    let v_low = t.lowlink[v];
                    if w_idx < v_low {
                        t.lowlink.insert(v.clone(), w_idx);
                    }
                }
            }
        }

        if t.lowlink[v] == t.indices[v] {
            let mut scc = BTreeSet::new();
            while let Some(w) = t.stack.pop() {
                t.on_stack.remove(&w);
                let done = &w == v;
                scc.insert(w);
                if done {
                    break;
                }
            }
            if scc.len() >= 2 {
                t.sccs.push(scc);
            }
        }
    }

    let mut t = Tarjan {
        graph,
        index: 0,
        indices: BTreeMap::new(),
        lowlink: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        sccs: Vec::new(),
    };

    for v in graph.keys() {
        if !t.indices.contains_key(v) {
            strongconnect(&mut t, v);
        }
    }

    t.sccs.sort_by(|a, b| a.iter().next().cmp(&b.iter().next()));
    t.sccs
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Tree, run};
    use crate::date::CivilDate;

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const MINI_ASSISTANT_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const MINI_CHANGES_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/changes.md");
    const MINI_SOURCES_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const MINI_CLAIMS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const MINI_QUESTIONS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const MINI_CONFLICTS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const MINI_GLOSSARY_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");

    const PAGE_CMP_REVIEW_STEPS: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );
    const PAGE_CMP_SYSTEM_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const PAGE_EST_RATE_WINDOW_BRIEF: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );

    const PATH_CMP_REVIEW_STEPS: &str =
        "corpus/pages/compliance/business-systems/cmp-review-steps.md";
    const PATH_CMP_SYSTEM_REVIEW: &str =
        "corpus/pages/compliance/business-systems/cmp-system-review.md";
    const PATH_EST_RATE_WINDOW_BRIEF: &str =
        "corpus/pages/estimating/pricing/est-rate-window-brief.md";

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_dir("corpus/pages/estimating");
        tree.insert_dir("corpus/pages/estimating/pricing");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", MINI_SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", MINI_CLAIMS_CSV.as_bytes());
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes(),
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, PAGE_CMP_REVIEW_STEPS.as_bytes());
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, PAGE_CMP_SYSTEM_REVIEW.as_bytes());
        tree.insert_bytes(
            PATH_EST_RATE_WINDOW_BRIEF,
            PAGE_EST_RATE_WINDOW_BRIEF.as_bytes(),
        );
        tree
    }

    #[test]
    fn fb0110_unknown_page_ids_in_prereq_links_and_supersession() {
        let mut tree = mini_tree();
        // Add comment before `prereq` to verify parser-recorded field_location is used.
        let bad = PAGE_CMP_REVIEW_STEPS
            .replace(
                "+++\nid = \"cmp-review-steps\"",
                "+++\n# comment with prereq = [\"cmp-comment\"]\nid = \"cmp-review-steps\"",
            )
            .replace(
                "prereq = [\"cmp-system-review\"]",
                "prereq = [\"cmp-nope\"]\nlinks = [\"est-nope\"]\nsupersedes = [\"cmp-old-nope\"]\nsuperseded_by = [\"cmp-new-nope\"]",
            );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 4, "{:?}", report.diagnostics);
        assert!(report.diagnostics.iter().all(|d| d.code == Code::Fb0110));
        assert_eq!(report.diagnostics[0].line, 14);
        assert!(report.diagnostics[0].message.contains("cmp-nope"));
        assert_eq!(report.diagnostics[1].line, 15);
        assert!(report.diagnostics[1].message.contains("est-nope"));
        assert_eq!(report.diagnostics[2].line, 16);
        assert!(report.diagnostics[2].message.contains("cmp-old-nope"));
        assert_eq!(report.diagnostics[3].line, 17);
        assert!(report.diagnostics[3].message.contains("cmp-new-nope"));
    }

    #[test]
    fn fb0111_prereq_cycle_of_three_pages_and_later_stage() {
        // 1. Cycle of three pages: cmp-review-steps -> cmp-system-review -> est-rate-window-brief -> cmp-review-steps
        // Set all three to stage 3 so only the cycle fires, and verify it is reported once at the
        // page whose id sorts first (`cmp-review-steps`) with all three ids in sorted order.
        let mut tree = mini_tree();
        let p_steps = PAGE_CMP_REVIEW_STEPS; // already has prereq = ["cmp-system-review"], stage = 3
        let p_sys = PAGE_CMP_SYSTEM_REVIEW
            .replace("stage = 2", "stage = 3")
            .replace(
                "volatility = \"V2\"\n+++",
                "prereq = [\"est-rate-window-brief\"]\nvolatility = \"V2\"\n+++",
            );
        let p_est = PAGE_EST_RATE_WINDOW_BRIEF
            .replace("stage = 2", "stage = 3")
            .replace(
                "links = [\"cmp-system-review\"]",
                "prereq = [\"cmp-review-steps\"]\nlinks = [\"cmp-system-review\"]",
            );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, p_steps.as_bytes());
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, p_sys.as_bytes());
        tree.insert_bytes(PATH_EST_RATE_WINDOW_BRIEF, p_est.as_bytes());

        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0111);
        assert_eq!(d.path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(d.line, 13);
        assert_eq!(
            d.message,
            "`prereq` cycle: cmp-review-steps -> cmp-system-review -> est-rate-window-brief"
        );

        // 2. Prerequisite with a later stage: cmp-review-steps at stage 1 with prereq cmp-system-review (stage 2).
        let mut tree_stage = mini_tree();
        let bad_stage = PAGE_CMP_REVIEW_STEPS.replace("stage = 3", "stage = 1");
        tree_stage.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_stage.as_bytes());
        let report_stage = run(&Host::new(tree_stage, sample_today()), &Options::new(false));
        assert_eq!(
            report_stage.diagnostics.len(),
            1,
            "{:?}",
            report_stage.diagnostics
        );
        assert_eq!(report_stage.diagnostics[0].code, Code::Fb0111);
        assert_eq!(report_stage.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report_stage.diagnostics[0].line, 13);
        assert!(
            report_stage.diagnostics[0]
                .message
                .contains("cmp-system-review")
        );
    }

    #[test]
    fn fb0112_unknown_terms() {
        let mut tree = mini_tree();
        let bad = PAGE_CMP_REVIEW_STEPS.replace(
            "terms = [\"system_review\"]",
            "terms = [\"no_such_term\", \"system_review\"]",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0112);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 12);
        assert!(report.diagnostics[0].message.contains("no_such_term"));
    }

    #[test]
    fn fb0113_unknown_questions_and_conflicts() {
        let mut tree = mini_tree();
        let bad = PAGE_CMP_REVIEW_STEPS.replace(
            "questions = [\"Q-001\"]",
            "questions = [\"Q-999\"]\nconflicts = [\"XC-99\"]",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 2, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0113);
        assert_eq!(report.diagnostics[0].line, 14);
        assert!(report.diagnostics[0].message.contains("Q-999"));
        assert_eq!(report.diagnostics[1].code, Code::Fb0113);
        assert_eq!(report.diagnostics[1].line, 15);
        assert!(report.diagnostics[1].message.contains("XC-99"));
    }

    #[test]
    fn fb0115_page_supersession_reciprocity_self_and_loop() {
        // 1. Missing reciprocity in `superseded_by`: A lists B in `superseded_by`, B does not list A in `supersedes`.
        let mut tree = mini_tree();
        let bad_recip = PAGE_CMP_REVIEW_STEPS.replace(
            "volatility = \"V2\"\n+++",
            "volatility = \"V2\"\nsuperseded_by = [\"cmp-system-review\"]\n+++",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_recip.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0115);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 16);

        // 2. Self-supersession: A lists A in `supersedes`.
        let mut tree_self = mini_tree();
        let bad_self = PAGE_CMP_REVIEW_STEPS.replace(
            "volatility = \"V2\"\n+++",
            "volatility = \"V2\"\nsupersedes = [\"cmp-review-steps\"]\n+++",
        );
        tree_self.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_self.as_bytes());
        let report_self = run(&Host::new(tree_self, sample_today()), &Options::new(false));
        assert_eq!(
            report_self.diagnostics.len(),
            1,
            "{:?}",
            report_self.diagnostics
        );
        assert_eq!(report_self.diagnostics[0].code, Code::Fb0115);
        assert_eq!(report_self.diagnostics[0].line, 16);

        // 3. Reciprocal loop of two pages: A and B both supersede each other.
        let mut tree_loop = mini_tree();
        let p_a = PAGE_CMP_REVIEW_STEPS.replace(
            "volatility = \"V2\"\n+++",
            "volatility = \"V2\"\nsupersedes = [\"cmp-system-review\"]\nsuperseded_by = [\"cmp-system-review\"]\n+++",
        );
        let p_b = PAGE_CMP_SYSTEM_REVIEW.replace(
            "volatility = \"V2\"\n+++",
            "volatility = \"V2\"\nsupersedes = [\"cmp-review-steps\"]\nsuperseded_by = [\"cmp-review-steps\"]\n+++",
        );
        tree_loop.insert_bytes(PATH_CMP_REVIEW_STEPS, p_a.as_bytes());
        tree_loop.insert_bytes(PATH_CMP_SYSTEM_REVIEW, p_b.as_bytes());
        let report_loop = run(&Host::new(tree_loop, sample_today()), &Options::new(false));
        assert_eq!(
            report_loop.diagnostics.len(),
            1,
            "{:?}",
            report_loop.diagnostics
        );
        assert_eq!(report_loop.diagnostics[0].code, Code::Fb0115);
        assert_eq!(report_loop.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(
            report_loop.diagnostics[0].message,
            "page supersession loop: cmp-review-steps -> cmp-system-review"
        );
    }

    #[test]
    fn fragment_only_link_targets_current_page() {
        let mut tree = mini_tree();
        let with_frag = PAGE_CMP_REVIEW_STEPS.replace(
            "[review concept](cmp-system-review.md)",
            "[purpose section](#purpose) and [review concept](cmp-system-review.md#purpose)",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, with_frag.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report.diagnostics.is_empty(),
            "expected `#purpose` and `cmp-system-review.md#purpose` to pass, got {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn fb0136_missing_link_with_fragment_and_outside_corpus_links_ignored() {
        // 1. `missing.md#purpose` raises FB0136.
        let mut tree = mini_tree();
        let with_missing = PAGE_CMP_REVIEW_STEPS.replace(
            "[review concept](cmp-system-review.md)",
            "[missing page](missing.md#purpose)",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, with_missing.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0136);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 29);
        assert!(report.diagnostics[0].message.contains("missing.md#purpose"));

        // 2. A link to another folder inside `corpus/` (e.g. `../../../registers/sources.csv`)
        // resolves when the target exists and raises FB0136 when it does not.
        let mut tree_register = mini_tree();
        let with_register = PAGE_CMP_REVIEW_STEPS.replace(
            "[review concept](cmp-system-review.md)",
            "[sources](../../../registers/sources.csv) and [missing](../../../registers/missing.csv)",
        );
        tree_register.insert_bytes(PATH_CMP_REVIEW_STEPS, with_register.as_bytes());
        let report_register = run(
            &Host::new(tree_register, sample_today()),
            &Options::new(false),
        );
        assert_eq!(
            report_register.diagnostics.len(),
            1,
            "{:?}",
            report_register.diagnostics
        );
        assert_eq!(report_register.diagnostics[0].code, Code::Fb0136);
        assert!(
            report_register.diagnostics[0]
                .message
                .contains("../../../registers/missing.csv")
        );

        // 3. A link that leaves `corpus/` or has a scheme raises nothing in `check-page-links`
        // (owned by FB0135 in `check-page-body`), and `[@CL-CMP-0001][@CL-CMP-0001]` is not a link.
        let mut tree_outside = mini_tree();
        let with_outside = PAGE_CMP_REVIEW_STEPS.replace(
            "[review concept](cmp-system-review.md)",
            "[climb4](../../../../outside.md) and [ext](https://example.invalid/x) and [@CL-CMP-0001][@CL-CMP-0001]",
        );
        tree_outside.insert_bytes(PATH_CMP_REVIEW_STEPS, with_outside.as_bytes());
        let report_outside = run(
            &Host::new(tree_outside, sample_today()),
            &Options::new(false),
        );
        assert!(
            report_outside
                .diagnostics
                .iter()
                .all(|d| d.code != Code::Fb0136),
            "expected outside-corpus links and adjacent citations to raise no FB0136 here, got {:?}",
            report_outside.diagnostics
        );
    }
}
