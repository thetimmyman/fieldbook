//! Filtered view over [`Model`] for a single destination (`LabelledView::new`,
//! `LabelledView::restrict`, `left_out`, `left_out_line`, `docs/reference/policy.md#the-decision`).

use std::collections::{BTreeMap, BTreeSet};

use crate::authz::ReadableStores;
use crate::check::{Index, Located, Model, PageEntry, Tree};
use crate::cite::{Citation, KeyKind};
use crate::date::CivilDate;
use crate::ids::{ClaimId, ConflictId, PageId, QuestionId, VersionId};
use crate::labels::{LabelAllow, LabelId, LabelSet, Ledger, StoreId, effective};
use crate::schema::rows::RefId;
use crate::schema::table::{Row, Table};
use crate::schema::{
    ClaimRow, ConflictRow, CorpusToml, GlossaryRow, Lifecycle, QuestionRow, Register, SourceRow,
};

use super::decision::Allowed;
use super::dest::Dest;

const fn register_index(register: Register) -> usize {
    match register {
        Register::Sources => 0,
        Register::Claims => 1,
        Register::Questions => 2,
        Register::Conflicts => 3,
        Register::Glossary => 4,
    }
}

/// Filtered, read-only view of [`Model`] constructed from an [`Allowed`] capability token and
/// the actor's [`ReadableStores`].
///
/// - Retains a page, source version, or register record only if every label it carries is in
///   `allowed.permitted()` (and any subsequent [`Self::restrict`] narrowing) and its store is in
///   `readable`, and retains dependent rows only when every referenced item is also retained.
/// - Records outside the allowed set are left out whole, never redacted.
/// - Exposes no accessor to the unrestricted [`Model`] or [`Tree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelledView {
    permitted: LabelAllow,
    readable: ReadableStores,
    today: CivilDate,
    corpus: Option<CorpusToml>,
    tree: Tree,
    registers: [Option<Table>; 5],
    sources: Vec<Located<SourceRow>>,
    claims: Vec<Located<ClaimRow>>,
    questions: Vec<Located<QuestionRow>>,
    conflicts: Vec<Located<ConflictRow>>,
    glossary: Vec<Located<GlossaryRow>>,
    pages: Vec<PageEntry>,
    index: Index,
    page_citations: BTreeMap<PageId, Vec<Citation>>,
    left_out: usize,
    raw_items: RawModelItems,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawModelItems {
    today: CivilDate,
    corpus: Option<CorpusToml>,
    input_tree: Tree,
    registers: [Option<Table>; 5],
    sources: Vec<Located<SourceRow>>,
    claims: Vec<Located<ClaimRow>>,
    questions: Vec<Located<QuestionRow>>,
    conflicts: Vec<Located<ConflictRow>>,
    glossary: Vec<Located<GlossaryRow>>,
    pages: Vec<PageEntry>,
    page_citations: BTreeMap<PageId, Vec<Citation>>,
    ledgers: BTreeMap<StoreId, Ledger>,
    record_stores: BTreeMap<String, StoreId>,
    record_labels: BTreeMap<String, LabelSet>,
}

impl LabelledView {
    /// Builds a filtered [`LabelledView`] from `model`, `allowed`, and `readable`.
    #[must_use]
    pub fn new(model: &Model, allowed: &Allowed, readable: &ReadableStores) -> Self {
        let raw = RawModelItems::extract(model);
        Self::build_from_raw(raw, allowed.permitted().clone(), readable.clone())
    }

    /// Returns a narrower [`LabelledView`] whose permitted label set is intersected with `extra`.
    /// Never feeds permissions through [`LabelSet`] normalization.
    #[must_use]
    pub fn restrict(&self, extra: &LabelAllow) -> Self {
        let narrowed = self.permitted.intersection(extra);
        Self::build_from_raw(self.raw_items.clone(), narrowed, self.readable.clone())
    }

    /// Returns the number of primary records (pages, source versions, claims, questions, conflicts,
    /// glossary terms, and store records) omitted by policy or store readability.
    #[must_use]
    pub const fn left_out(&self) -> usize {
        self.left_out
    }

    /// Formats the omission notice for `dest`:
    /// - `"Left out (policy): N records"` when `dest.shows_left_out_count()` is `true`.
    /// - `"Some records were omitted."` (with no count) for all other destination kinds.
    #[must_use]
    pub fn left_out_line(&self, dest: &Dest) -> String {
        if dest.shows_left_out_count() {
            format!("Left out (policy): {} records", self.left_out)
        } else {
            "Some records were omitted.".to_owned()
        }
    }

    /// Returns the permitted [`LabelAllow`] governing this view.
    #[must_use]
    pub const fn permitted(&self) -> &LabelAllow {
        &self.permitted
    }

    /// Returns the filtered [`Tree`] containing only permitted files, their ancestor directories,
    /// and structural directories of selected readable stores (including an empty originals root).
    #[must_use]
    pub const fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Returns the calendar date from the underlying model.
    #[must_use]
    pub const fn today(&self) -> CivilDate {
        self.today
    }

    /// Returns the validated [`CorpusToml`] when `corpus` is readable and `public` is permitted.
    #[must_use]
    pub const fn corpus(&self) -> Option<&CorpusToml> {
        self.corpus.as_ref()
    }

    /// Returns the filtered [`Table`] for `register`.
    #[must_use]
    pub const fn register(&self, register: Register) -> Option<&Table> {
        self.registers[register_index(register)].as_ref()
    }

    /// Returns the filtered [`Row`] slice for `register`.
    #[must_use]
    pub fn register_rows(&self, register: Register) -> &[Row] {
        match &self.registers[register_index(register)] {
            Some(t) => &t.rows,
            None => &[],
        }
    }

    /// Returns the kept [`SourceRow`] entries.
    #[must_use]
    pub fn sources(&self) -> &[Located<SourceRow>] {
        &self.sources
    }

    /// Returns the kept [`ClaimRow`] entries.
    #[must_use]
    pub fn claims(&self) -> &[Located<ClaimRow>] {
        &self.claims
    }

    /// Returns the kept [`QuestionRow`] entries.
    #[must_use]
    pub fn questions(&self) -> &[Located<QuestionRow>] {
        &self.questions
    }

    /// Returns the kept [`ConflictRow`] entries.
    #[must_use]
    pub fn conflicts(&self) -> &[Located<ConflictRow>] {
        &self.conflicts
    }

    /// Returns the kept [`GlossaryRow`] entries.
    #[must_use]
    pub fn glossary(&self) -> &[Located<GlossaryRow>] {
        &self.glossary
    }

    /// Returns the kept [`PageEntry`] entries.
    #[must_use]
    pub fn pages(&self) -> &[PageEntry] {
        &self.pages
    }

    /// Returns the filtered identifier [`Index`].
    #[must_use]
    pub const fn index(&self) -> &Index {
        &self.index
    }

    /// Returns the resolved [`Citation`] slice for `page` (empty if `page` was omitted).
    #[must_use]
    pub fn citations(&self, page: &PageId) -> &[Citation] {
        match self.page_citations.get(page) {
            Some(c) => c.as_slice(),
            None => &[],
        }
    }

    /// Renders a deterministic text summary of the kept records and files in this view
    /// (used by the label-leak matrix).
    #[must_use]
    pub fn render_summary(&self, dest: &Dest) -> String {
        let mut out = String::new();
        for page in &self.pages {
            out.push_str(&format!(
                "page:{}:{}:{}\n",
                page.id, page.meta.title, page.body
            ));
        }
        for s in &self.sources {
            out.push_str(&format!(
                "source:{}-v{}:{}:{}\n",
                s.value.source_id,
                s.value.version,
                s.value.title,
                s.value.note.as_deref().unwrap_or("")
            ));
        }
        for c in &self.claims {
            out.push_str(&format!(
                "claim:{}:{}:{}\n",
                c.value.claim_id,
                c.value.statement,
                c.value.quote.as_deref().unwrap_or("")
            ));
        }
        for q in &self.questions {
            out.push_str(&format!(
                "question:{}:{}:{}\n",
                q.value.question_id,
                q.value.question,
                q.value.answer.as_deref().unwrap_or("")
            ));
        }
        for x in &self.conflicts {
            out.push_str(&format!(
                "conflict:{}:{}:{}\n",
                x.value.conflict_id,
                x.value.topic,
                x.value.positions.join("|")
            ));
        }
        for g in &self.glossary {
            out.push_str(&format!(
                "term:{}:{}:{}\n",
                g.value.term_id, g.value.label, g.value.definition
            ));
        }
        for (path, entry) in self.tree.iter() {
            if let Some(bytes) = entry.as_bytes()
                && !path.starts_with("corpus/registers/")
                && path != "corpus/corpus.toml"
                && path != "corpus/ASSISTANT.md"
                && path != "corpus/changes.md"
                && let Ok(text) = std::str::from_utf8(bytes)
            {
                out.push_str(&format!("file:{path}:{text}\n"));
            }
        }
        out.push_str(&self.left_out_line(dest));
        out.push('\n');
        out
    }

    fn build_from_raw(raw: RawModelItems, permitted: LabelAllow, readable: ReadableStores) -> Self {
        let mut left_out = 0usize;

        // 1. Filter sources (primary records).
        let mut kept_sources = Vec::new();
        let mut kept_source_lines = BTreeSet::new();
        let mut kept_source_ids = BTreeSet::new();
        let mut kept_version_ids = BTreeSet::new();
        let mut kept_original_versions = BTreeSet::new();

        for loc in &raw.sources {
            let vid_str = format!("{}-v{}", loc.value.source_id, loc.value.version);
            let store = raw
                .item_store(&vid_str)
                .or_else(|| raw.item_store(loc.value.source_id.as_str()))
                .unwrap_or(StoreId::Corpus);
            let labels = raw.resolve_explicit_labels(
                &vid_str,
                &parse_handling_vec(&loc.value.handling),
                store,
            );
            if readable.contains(store) && permitted.permits(&labels) {
                kept_source_lines.insert(loc.line);
                kept_source_ids.insert(loc.value.source_id.clone());
                if let Ok(vid) = VersionId::parse(&vid_str) {
                    // A held version ships its row only, and a metadata-only one has no file.
                    if loc.value.lifecycle != Lifecycle::Quarantined
                        && !loc.value.location.starts_with("metadata-only: ")
                    {
                        kept_original_versions.insert(vid.clone());
                    }
                    kept_version_ids.insert(vid);
                }
                kept_sources.push(loc.clone());
            } else {
                left_out += 1;
            }
        }

        // 2. Filter questions (primary records).
        let mut kept_questions = Vec::new();
        let mut kept_question_lines = BTreeSet::new();
        let mut kept_question_ids = BTreeSet::new();
        for loc in &raw.questions {
            let qid_str = loc.value.question_id.as_str();
            let store = raw.item_store(qid_str).unwrap_or(StoreId::Corpus);
            let labels = raw.resolve_labels(qid_str, &LabelSet::public(), store);
            if readable.contains(store) && permitted.permits(&labels) {
                kept_question_lines.insert(loc.line);
                kept_question_ids.insert(loc.value.question_id.clone());
                kept_questions.push(loc.clone());
            } else {
                left_out += 1;
            }
        }

        // 3. Filter claims (depend on source version if `source_id`/`version` is present, and question).
        // First pass without `conflict` check, then filter conflicts, then drop claims pointing at dropped conflicts.
        let mut candidate_claims = Vec::new();
        for loc in &raw.claims {
            let cid_str = loc.value.claim_id.as_str();
            let store = raw.item_store(cid_str).unwrap_or(StoreId::Corpus);
            let labels = raw.resolve_labels(cid_str, &LabelSet::public(), store);
            if !readable.contains(store) || !permitted.permits(&labels) {
                left_out += 1;
                continue;
            }
            let source_ok = match (&loc.value.source_id, loc.value.version) {
                (Some(sid), Some(ver)) => VersionId::parse(&format!("{sid}-v{ver}"))
                    .is_ok_and(|vid| kept_version_ids.contains(&vid)),
                (Some(sid), None) => kept_source_ids.contains(sid),
                (None, _) => true,
            };
            let question_ok = loc
                .value
                .question_id
                .as_ref()
                .is_none_or(|qid| kept_question_ids.contains(qid));
            if source_ok && question_ok {
                candidate_claims.push(loc.clone());
            } else {
                left_out += 1;
            }
        }

        let candidate_claim_ids: BTreeSet<ClaimId> = candidate_claims
            .iter()
            .map(|c| c.value.claim_id.clone())
            .collect();

        // 4. Filter conflicts (point at `claim_ids`, `evidence`, `question_id`).
        let mut kept_conflicts = Vec::new();
        let mut kept_conflict_lines = BTreeSet::new();
        let mut kept_conflict_ids = BTreeSet::new();
        for loc in &raw.conflicts {
            let xid_str = loc.value.conflict_id.as_str();
            let store = raw.item_store(xid_str).unwrap_or(StoreId::Corpus);
            let labels = raw.resolve_labels(xid_str, &LabelSet::public(), store);
            if !readable.contains(store) || !permitted.permits(&labels) {
                left_out += 1;
                continue;
            }
            let claims_ok = loc
                .value
                .claim_ids
                .iter()
                .all(|cid| candidate_claim_ids.contains(cid));
            let evidence_ok = loc.value.evidence.iter().all(|r| match r {
                RefId::Claim(cid) => candidate_claim_ids.contains(cid),
                RefId::SourceVersion(vid) => kept_version_ids.contains(vid),
            });
            let question_ok = loc
                .value
                .question_id
                .as_ref()
                .is_none_or(|qid| kept_question_ids.contains(qid));
            if claims_ok && evidence_ok && question_ok {
                kept_conflict_lines.insert(loc.line);
                kept_conflict_ids.insert(loc.value.conflict_id.clone());
                kept_conflicts.push(loc.clone());
            } else {
                left_out += 1;
            }
        }

        // Finalize claims (dropping any claim pointing at a dropped conflict or superseded_by a dropped claim).
        let mut kept_claims = Vec::new();
        let mut kept_claim_lines = BTreeSet::new();
        let mut kept_claim_ids = BTreeSet::new();
        for loc in candidate_claims {
            let conflict_ok = loc
                .value
                .conflict_id
                .as_ref()
                .is_none_or(|xid| kept_conflict_ids.contains(xid));
            let sup_ok = loc
                .value
                .superseded_by
                .iter()
                .all(|cid| candidate_claim_ids.contains(cid));
            if conflict_ok && sup_ok {
                kept_claim_lines.insert(loc.line);
                kept_claim_ids.insert(loc.value.claim_id.clone());
                kept_claims.push(loc);
            } else {
                left_out += 1;
            }
        }

        // 5. Filter glossary terms (point at `source_ref` and `superseded_by`).
        let mut kept_glossary = Vec::new();
        let mut kept_glossary_lines = BTreeSet::new();
        let mut kept_term_ids = BTreeSet::new();
        for loc in &raw.glossary {
            let tid_str = loc.value.term_id.as_str();
            let store = raw.item_store(tid_str).unwrap_or(StoreId::Corpus);
            let labels = raw.resolve_labels(tid_str, &LabelSet::public(), store);
            if !readable.contains(store) || !permitted.permits(&labels) {
                left_out += 1;
                continue;
            }
            let refs_ok = loc.value.source_ref.iter().all(|r| match r {
                RefId::Claim(cid) => kept_claim_ids.contains(cid),
                RefId::SourceVersion(vid) => kept_version_ids.contains(vid),
            });
            if refs_ok {
                kept_glossary_lines.insert(loc.line);
                kept_term_ids.insert(loc.value.term_id.clone());
                kept_glossary.push(loc.clone());
            } else {
                left_out += 1;
            }
        }

        // 6. Filter pages (keep if store is readable, own labels permitted, and every cited/linked item is kept).
        let mut candidate_pages = Vec::new();
        for page in &raw.pages {
            let pid_str = page.id.as_str();
            let store = raw
                .item_store(pid_str)
                .or_else(|| store_from_path(&page.path))
                .unwrap_or(StoreId::Corpus);
            let own_labels = parse_handling_vec(&page.meta.handling);
            let labels = raw.resolve_explicit_labels(pid_str, &own_labels, store);
            if !readable.contains(store) || !permitted.permits(&labels) {
                left_out += 1;
                continue;
            }
            let terms_ok = page.meta.terms.iter().all(|t| kept_term_ids.contains(t));
            let questions_ok = page
                .meta
                .questions
                .iter()
                .all(|q| kept_question_ids.contains(q));
            let conflicts_ok = page
                .meta
                .conflicts
                .iter()
                .all(|x| kept_conflict_ids.contains(x));
            let cites = raw
                .page_citations
                .get(&page.id)
                .map_or(&[][..], Vec::as_slice);
            let cites_ok = cites.iter().all(|c| match c.key_kind {
                KeyKind::Version => {
                    VersionId::parse(&c.key).is_ok_and(|vid| kept_version_ids.contains(&vid))
                }
                KeyKind::Claim => {
                    ClaimId::parse(&c.key).is_ok_and(|cid| kept_claim_ids.contains(&cid))
                }
                KeyKind::Question => {
                    QuestionId::parse(&c.key).is_ok_and(|qid| kept_question_ids.contains(&qid))
                }
                KeyKind::Conflict => {
                    ConflictId::parse(&c.key).is_ok_and(|xid| kept_conflict_ids.contains(&xid))
                }
                KeyKind::Note => true,
            });
            if terms_ok && questions_ok && conflicts_ok && cites_ok {
                candidate_pages.push(page.clone());
            } else {
                left_out += 1;
            }
        }

        let candidate_page_ids: BTreeSet<PageId> =
            candidate_pages.iter().map(|p| p.id.clone()).collect();
        let mut kept_pages = Vec::new();
        let mut kept_page_paths = BTreeSet::new();
        for page in candidate_pages {
            let links_ok = page
                .meta
                .prereq
                .iter()
                .chain(page.meta.links.iter())
                .chain(page.meta.supersedes.iter())
                .chain(page.meta.superseded_by.iter())
                .all(|pid| candidate_page_ids.contains(pid));
            if links_ok {
                kept_page_paths.insert(page.path.clone());
                kept_pages.push(page);
            } else {
                left_out += 1;
            }
        }

        // 7. Filter arbitrary store files / extra records in `input_tree`.
        let mut kept_extra_files = BTreeSet::new();
        for (path, _) in raw.input_tree.iter() {
            if path == "corpus/corpus.toml"
                || path == "corpus/ASSISTANT.md"
                || path == "corpus/changes.md"
                || path.starts_with("corpus/registers/")
                || path.starts_with("corpus/pages/")
                || path.starts_with("corpus/sources/public/")
                || path.ends_with("/labels.csv")
            {
                continue;
            }
            if let Some(after_text) = path.strip_prefix("corpus/sources/text/") {
                let vid_ok = after_text
                    .split('/')
                    .next()
                    .and_then(|seg| VersionId::parse(seg).ok())
                    .is_some_and(|vid| kept_version_ids.contains(&vid));
                if vid_ok {
                    kept_extra_files.insert(path.to_owned());
                } else {
                    left_out += 1;
                }
                continue;
            }
            let store = raw
                .item_store(path)
                .or_else(|| store_from_path(path))
                .unwrap_or(StoreId::Corpus);
            let base_labels = raw
                .record_labels
                .get(path)
                .cloned()
                .unwrap_or_else(LabelSet::public);
            let labels = raw.resolve_labels(path, &base_labels, store);
            if readable.contains(store) && permitted.permits(&labels) {
                kept_extra_files.insert(path.to_owned());
            } else {
                left_out += 1;
            }
        }

        // 8. Build filtered register tables.
        let filtered_registers = [
            filter_table(raw.registers[0].as_ref(), &kept_source_lines),
            filter_table(raw.registers[1].as_ref(), &kept_claim_lines),
            filter_table(raw.registers[2].as_ref(), &kept_question_lines),
            filter_table(raw.registers[3].as_ref(), &kept_conflict_lines),
            filter_table(raw.registers[4].as_ref(), &kept_glossary_lines),
        ];

        // 9. Build filtered Tree:
        //    - Retain known structural directories of selected readable stores (including an empty
        //      originals root `corpus/sources/public`), if present in the input tree.
        //    - Retain permitted files and their ancestor directories (`insert_dir` on parent).
        //    - Never retain arbitrary directories emptied by filtering.
        let mut filtered_tree = Tree::new();
        let public_permitted = permitted.permits(&LabelSet::public());

        for store in readable.stores() {
            let root_name = store.as_str();
            if raw.input_tree.contains_dir(root_name) {
                filtered_tree.insert_dir(root_name);
            }
            if *store == StoreId::Corpus {
                for structural in [
                    "corpus/pages",
                    "corpus/registers",
                    "corpus/sources",
                    "corpus/sources/public",
                    "corpus/sources/text",
                    "corpus/cases",
                ] {
                    if raw.input_tree.contains_dir(structural) {
                        filtered_tree.insert_dir(structural);
                    }
                }
            } else {
                for sub in ["pages", "registers", "sources", "records", "notes"] {
                    let dir = format!("{root_name}/{sub}");
                    if raw.input_tree.contains_dir(&dir) {
                        filtered_tree.insert_dir(&dir);
                    }
                }
            }
        }

        if readable.contains(StoreId::Corpus) && public_permitted {
            for ctrl in [
                "corpus/corpus.toml",
                "corpus/ASSISTANT.md",
                "corpus/changes.md",
            ] {
                if let Some(entry) = raw.input_tree.get(ctrl) {
                    insert_entry_with_ancestors(&mut filtered_tree, ctrl, entry.clone());
                }
            }
            for reg in Register::ALL {
                let r_idx = register_index(reg);
                if let Some(tbl) = &filtered_registers[r_idx] {
                    let path = format!("corpus/registers/{}", reg.file_name());
                    if raw.input_tree.get(&path).is_some() {
                        let csv_bytes = crate::schema::table::write(tbl).into_bytes();
                        insert_entry_with_ancestors(
                            &mut filtered_tree,
                            &path,
                            crate::check::Entry::Bytes(csv_bytes),
                        );
                    }
                }
            }
        }

        for page_path in &kept_page_paths {
            if let Some(entry) = raw.input_tree.get(page_path) {
                insert_entry_with_ancestors(&mut filtered_tree, page_path, entry.clone());
            }
        }

        // An original is `corpus/sources/public/<version-id>.<ext>` and goes with its version.
        // `location` says where it came from and never selects a file.
        for (path, entry) in raw.input_tree.iter() {
            let is_kept = path
                .strip_prefix("corpus/sources/public/")
                .filter(|name| !name.contains('/'))
                .and_then(|name| name.split_once('.'))
                .and_then(|(stem, _)| VersionId::parse(stem).ok())
                .is_some_and(|vid| kept_original_versions.contains(&vid));
            if is_kept {
                insert_entry_with_ancestors(&mut filtered_tree, path, entry.clone());
            }
        }

        for extra_path in &kept_extra_files {
            if let Some(entry) = raw.input_tree.get(extra_path) {
                insert_entry_with_ancestors(&mut filtered_tree, extra_path, entry.clone());
            }
        }

        let corpus = if readable.contains(StoreId::Corpus) && public_permitted {
            raw.corpus.clone()
        } else {
            None
        };

        let index = Index::build(
            corpus.as_ref(),
            &kept_sources,
            &kept_claims,
            &kept_questions,
            &kept_conflicts,
            &kept_glossary,
            &kept_pages,
        );

        let mut page_citations = BTreeMap::new();
        for page in &kept_pages {
            if let Some(c) = raw.page_citations.get(&page.id) {
                page_citations.insert(page.id.clone(), c.clone());
            }
        }

        Self {
            permitted,
            readable,
            today: raw.today,
            corpus,
            tree: filtered_tree,
            registers: filtered_registers,
            sources: kept_sources,
            claims: kept_claims,
            questions: kept_questions,
            conflicts: kept_conflicts,
            glossary: kept_glossary,
            pages: kept_pages,
            index,
            page_citations,
            left_out,
            raw_items: raw,
        }
    }
}

impl RawModelItems {
    fn extract(model: &Model) -> Self {
        let registers = [
            model.register(Register::Sources).cloned(),
            model.register(Register::Claims).cloned(),
            model.register(Register::Questions).cloned(),
            model.register(Register::Conflicts).cloned(),
            model.register(Register::Glossary).cloned(),
        ];
        let pages = model.pages().to_vec();
        let mut page_citations = BTreeMap::new();
        for p in &pages {
            page_citations.insert(p.id.clone(), model.citations(&p.id).to_vec());
        }

        let mut ledgers = BTreeMap::new();
        for store in [StoreId::Overlay, StoreId::Working, StoreId::State] {
            let ledger_path = format!("{}/labels.csv", store.as_str());
            if let Some(bytes) = model.tree().get_bytes(&ledger_path) {
                let text = std::str::from_utf8(bytes).unwrap_or("");
                let ledger = Ledger::parse(text).unwrap_or_else(Ledger::broken);
                ledgers.insert(store, ledger);
            }
        }

        let mut record_stores = BTreeMap::new();
        let mut record_labels = BTreeMap::new();
        for (&store, ledger) in &ledgers {
            for entry in ledger.entries() {
                record_stores.insert(entry.item_id.clone(), store);
            }
        }
        for (path, entry) in model.tree().iter() {
            if let Some(store) = store_from_path(path) {
                record_stores.insert(path.to_owned(), store);
                if let Some(stem) = path.rsplit('/').next().and_then(|f| f.split('.').next())
                    && !stem.is_empty()
                {
                    if store != StoreId::Corpus {
                        record_stores.insert(stem.to_owned(), store);
                    } else {
                        record_stores.entry(stem.to_owned()).or_insert(store);
                    }
                }
            }
            let is_source_or_page_or_reg = path.contains("/sources/")
                || path.contains("/pages/")
                || path.contains("/registers/")
                || path.ends_with("/labels.csv")
                || path == "corpus/corpus.toml"
                || path == "corpus/ASSISTANT.md"
                || path == "corpus/changes.md";
            if !is_source_or_page_or_reg
                && let Some(bytes) = entry.as_bytes()
                && let Ok(text) = std::str::from_utf8(bytes)
                && let Some(labels) = extract_inline_handling(text)
            {
                record_labels.insert(path.to_owned(), labels.clone());
                if let Some(stem) = path.rsplit('/').next().and_then(|f| f.split('.').next())
                    && !stem.is_empty()
                {
                    if store_from_path(path) != Some(StoreId::Corpus) {
                        record_labels.insert(stem.to_owned(), labels);
                    } else {
                        record_labels.entry(stem.to_owned()).or_insert(labels);
                    }
                }
            }
        }

        Self {
            today: model.today(),
            corpus: model.corpus().cloned(),
            input_tree: model.tree().clone(),
            registers,
            sources: model.sources().to_vec(),
            claims: model.claims().to_vec(),
            questions: model.questions().to_vec(),
            conflicts: model.conflicts().to_vec(),
            glossary: model.glossary().to_vec(),
            pages,
            page_citations,
            ledgers,
            record_stores,
            record_labels,
        }
    }

    fn item_store(&self, id_or_path: &str) -> Option<StoreId> {
        self.record_stores.get(id_or_path).copied()
    }

    fn resolve_explicit_labels(&self, id: &str, initial: &LabelSet, store: StoreId) -> LabelSet {
        if let Some(ledger) = self.ledgers.get(&store) {
            effective(initial, id, ledger)
        } else {
            initial.clone()
        }
    }

    fn resolve_labels(&self, id: &str, initial: &LabelSet, store: StoreId) -> LabelSet {
        let base = self
            .record_labels
            .get(id)
            .cloned()
            .unwrap_or_else(|| initial.clone());
        if let Some(ledger) = self.ledgers.get(&store) {
            effective(&base, id, ledger)
        } else {
            base
        }
    }
}

fn store_from_path(path: &str) -> Option<StoreId> {
    let first = path.split('/').next()?;
    StoreId::from_root_name(first)
}

fn parse_handling_vec(items: &[String]) -> LabelSet {
    if items.is_empty() {
        return LabelSet::unlabelled();
    }
    let mut ids = Vec::with_capacity(items.len());
    for s in items {
        let Ok(id) = LabelId::parse(s) else {
            return LabelSet::unlabelled();
        };
        ids.push(id);
    }
    LabelSet::from_ids(ids)
}

fn extract_inline_handling(text: &str) -> Option<LabelSet> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("handling") {
            let rest = rest.trim_start();
            if let Some(val) = rest.strip_prefix('=') {
                let val = val.trim();
                if let Some(inner) = val.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                    return LabelSet::parse_cell(inner).ok();
                }
                if let Some(arr) = val.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    let mut ids = Vec::new();
                    for item in arr.split(',') {
                        let clean = item.trim().trim_matches('"');
                        if !clean.is_empty()
                            && let Ok(id) = LabelId::parse(clean)
                        {
                            ids.push(id);
                        }
                    }
                    if !ids.is_empty() {
                        return Some(LabelSet::from_ids(ids));
                    }
                }
            }
        }
    }
    None
}

fn filter_table(table: Option<&Table>, kept_lines: &BTreeSet<u32>) -> Option<Table> {
    let tbl = table?;
    let rows = tbl
        .rows
        .iter()
        .filter(|r| kept_lines.contains(&r.line))
        .cloned()
        .collect();
    Some(Table {
        header: tbl.header.clone(),
        rows,
    })
}

fn insert_entry_with_ancestors(tree: &mut Tree, path: &str, entry: crate::check::Entry) {
    if let Some((parent, _)) = path.rsplit_once('/')
        && !parent.is_empty()
    {
        tree.insert_dir(parent);
    }
    tree.insert(path.to_owned(), entry);
}
