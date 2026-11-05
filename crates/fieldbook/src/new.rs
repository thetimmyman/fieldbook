//! `fieldbook new`: start a page from its template, or append a row in entry state to
//! `claims.csv`, `questions.csv`, `conflicts.csv` or `glossary.csv` (`new-verbs`, AD-17).
//!
//! **Flow.** Missing flags are collected first (prompted for on a terminal, a usage error
//! otherwise), so nothing is held while a person types. Then one exclusive store lease covers
//! the journal, the tree read, the plan, admission and the write. A pending intent for the same
//! leaf command is resolved before any existence check or allocation: its original id and
//! planning date are replanned, admitted again and finished, never allocated anew.

use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    Id, OperationIntent, OperationKey, PendingOperation, RecordRef, RefuseAbout, Role, Sha256,
    StoreId, Verb, Via, Word, WriteFile, WriteSet, normalize_affected, request_sha256,
};
use fieldbook_core::check::{self, Entry, Host, Model};
use fieldbook_core::ids::{AreaCode, PageId};
use fieldbook_core::labels::{self, LabelSet, Refusal, StoreRule};
use fieldbook_core::new::{
    Fields, NewError, RowKind, RowPlan, plan_page, plan_row, render_register, replan_page,
    replan_row, source_handling, template_handling, template_path,
};
use fieldbook_core::policy::{FloorKind, Policy};
use fieldbook_core::schema::vocab::{DraftedBy, PageType};

use crate::add::{add_refuse_reason, label_context};
use crate::cli::{
    EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE, NewArgs, NewClaimArgs, NewConflictArgs,
    NewPageArgs, NewQuestionArgs, NewSubcommand, NewTermArgs,
};
use crate::io::{Journal, JournalError, Recovery};
use crate::{Outcome, StoreLease, World, WorldError};

/// The leaf command being run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leaf {
    Page,
    Claim,
    Question,
    Conflict,
    Term,
}

impl Leaf {
    /// The journal verb of this leaf command.
    const fn verb(self) -> &'static str {
        match self {
            Self::Page => "new page",
            Self::Claim => "new claim",
            Self::Question => "new question",
            Self::Conflict => "new conflict",
            Self::Term => "new term",
        }
    }

    const fn kind(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Claim => "claim",
            Self::Question => "question",
            Self::Conflict => "conflict",
            Self::Term => "term",
        }
    }

    const fn row_kind(self) -> Option<RowKind> {
        match self {
            Self::Page => None,
            Self::Claim => Some(RowKind::Claim),
            Self::Question => Some(RowKind::Question),
            Self::Conflict => Some(RowKind::Conflict),
            Self::Term => Some(RowKind::Term),
        }
    }
}

/// Every input of one `new` command, complete before the lease is taken.
struct Job {
    leaf: Leaf,
    /// The page id, or the term id when one was supplied; empty otherwise.
    supplied_id: String,
    area: AreaCode,
    page_type: Option<PageType>,
    subarea: String,
    fields: Fields,
    confirm_public: bool,
}

/// Run `fieldbook new` under the active floor policy for `world`.
#[must_use]
pub fn run(args: &NewArgs, world: &dyn World) -> Outcome {
    let policy = Policy::floor(FloorKind::for_machine(world.has_company_store()));
    run_with_policy(args, world, &policy)
}

/// Run `fieldbook new` with an explicit [`Policy`].
#[must_use]
pub fn run_with_policy(args: &NewArgs, world: &dyn World, policy: &Policy) -> Outcome {
    let job = match &args.command {
        NewSubcommand::Page(a) => page_job(a, world),
        NewSubcommand::Claim(a) => claim_job(a, world),
        NewSubcommand::Question(a) => question_job(a, world),
        NewSubcommand::Conflict(a) => conflict_job(a, world),
        NewSubcommand::Term(a) => term_job(a, world),
    };
    match job {
        Ok(job) => execute(world, policy, &job),
        Err(out) => out,
    }
}

/// A flag's value: the given one, or one line prompted for on a terminal.
///
/// The prompt is written to standard error and flushed before the line is read, and nothing
/// is held while it waits. An empty answer is the same usage error as a missing flag.
fn resolve_flag(world: &dyn World, current: Option<&str>, flag: &str) -> Result<String, Outcome> {
    resolve_value(world, current, flag, &format!("flag `--{flag}`"))
}

/// A positional argument's value, with the same prompt and error as [`resolve_flag`] but named
/// as an argument, since it has no flag.
fn resolve_positional(
    world: &dyn World,
    current: Option<&str>,
    name: &str,
) -> Result<String, Outcome> {
    resolve_value(world, current, name, &format!("argument `<{name}>`"))
}

fn resolve_value(
    world: &dyn World,
    current: Option<&str>,
    prompt: &str,
    described: &str,
) -> Result<String, Outcome> {
    if let Some(value) = current.map(str::trim).filter(|v| !v.is_empty()) {
        return Ok(value.to_owned());
    }
    if world.stdin_is_terminal() {
        world.prompt(prompt).map_err(io_err)?;
        let line = world.read_line();
        let line = line.trim();
        if !line.is_empty() {
            return Ok(line.to_owned());
        }
    }
    Err(usage_err(format!("missing required {described}")))
}

fn parse_area(raw: &str) -> Result<AreaCode, Outcome> {
    AreaCode::parse(raw).map_err(|_| new_err_outcome(&NewError::UnknownArea(raw.to_owned())))
}

fn page_job(args: &NewPageArgs, world: &dyn World) -> Result<Job, Outcome> {
    let id = resolve_positional(world, args.id.as_deref(), "id")?;
    let ty = resolve_flag(world, args.ty.as_deref(), "type")?;
    let subarea = resolve_flag(world, args.subarea.as_deref(), "subarea")?;
    let page_id = PageId::parse(&id).map_err(usage_err)?;
    let page_type =
        PageType::parse(&ty).map_err(|_| new_err_outcome(&NewError::UnknownPageType(ty)))?;
    Ok(Job {
        leaf: Leaf::Page,
        supplied_id: id,
        area: page_id.area(),
        page_type: Some(page_type),
        subarea,
        fields: Fields::new(),
        confirm_public: args.confirm_public,
    })
}

fn claim_job(args: &NewClaimArgs, world: &dyn World) -> Result<Job, Outcome> {
    let area = resolve_flag(world, args.area.as_deref(), "area")?;
    let source = resolve_flag(world, args.source.as_deref(), "source")?;
    let at = resolve_flag(world, args.at.as_deref(), "at")?;
    let quote = resolve_flag(world, args.quote.as_deref(), "quote")?;
    let drafted_by = drafted_by(world)?;
    let fields = Fields::new()
        .with("area", &area)
        .with("source", source)
        .with("at", at)
        .with("quote", quote)
        .with("drafted_by", drafted_by.as_str());
    row_job(Leaf::Claim, &area, "", fields, args.confirm_public)
}

fn question_job(args: &NewQuestionArgs, world: &dyn World) -> Result<Job, Outcome> {
    let area = resolve_flag(world, args.area.as_deref(), "area")?;
    let text = resolve_positional(world, args.text.as_deref(), "text")?;
    let fields = Fields::new().with("area", &area).with("text", text);
    row_job(Leaf::Question, &area, "", fields, args.confirm_public)
}

fn conflict_job(args: &NewConflictArgs, world: &dyn World) -> Result<Job, Outcome> {
    let area = resolve_flag(world, args.area.as_deref(), "area")?;
    let fields = Fields::new().with("area", &area);
    row_job(Leaf::Conflict, &area, "", fields, args.confirm_public)
}

fn term_job(args: &NewTermArgs, world: &dyn World) -> Result<Job, Outcome> {
    let area = resolve_flag(world, args.area.as_deref(), "area")?;
    let id = args.id.as_deref().map(str::trim).unwrap_or_default();
    let mut fields = Fields::new().with("area", &area);
    if !id.is_empty() {
        fields.insert("id", id);
    }
    row_job(Leaf::Term, &area, id, fields, args.confirm_public)
}

fn row_job(
    leaf: Leaf,
    area: &str,
    supplied_id: &str,
    fields: Fields,
    confirm_public: bool,
) -> Result<Job, Outcome> {
    Ok(Job {
        leaf,
        supplied_id: supplied_id.to_owned(),
        area: parse_area(area)?,
        page_type: None,
        subarea: String::new(),
        fields,
        confirm_public,
    })
}

/// Who drafts a new claim: the state's role (`owner`, `lead`; an agent is `codex`).
fn drafted_by(world: &dyn World) -> Result<DraftedBy, Outcome> {
    let (role, _) =
        crate::io::state::read_role_and_baseline(world.state_dir()).map_err(|e| journal_err(&e))?;
    Ok(match role {
        Role::Owner => DraftedBy::Owner,
        Role::Lead => DraftedBy::Lead,
        Role::Agent => DraftedBy::Codex,
    })
}

/// The operation key of a leaf command: its verb and the destination store.
fn operation_key(leaf: Leaf) -> OperationKey {
    let subject = request_sha256(&[
        ("verb", leaf.verb().as_bytes()),
        ("store", StoreId::Corpus.as_str().as_bytes()),
    ])
    .unwrap_or_else(|_| unreachable!());
    let verb = Verb::parse(leaf.verb()).unwrap_or_else(|_| unreachable!());
    OperationKey::new(verb, subject)
}

/// The normalised request: kind, supplied id, area, type, subarea, template hash, every final
/// field value and the public confirmation.
fn request_hash(job: &Job, template_sha256: &str) -> Sha256 {
    let ty = job.page_type.map_or("", PageType::as_str);
    let confirm: &[u8] = if job.confirm_public { b"1" } else { b"0" };
    let names: Vec<String> = job
        .fields
        .iter()
        .map(|(k, _)| format!("field:{k}"))
        .collect();
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("kind", job.leaf.kind().as_bytes()),
        ("supplied_id", job.supplied_id.as_bytes()),
        ("area", job.area.as_str().as_bytes()),
        ("type", ty.as_bytes()),
        ("subarea", job.subarea.as_bytes()),
        ("template_sha256", template_sha256.as_bytes()),
        ("confirm_public", confirm),
    ];
    for (name, (_, value)) in names.iter().zip(job.fields.iter()) {
        entries.push((name.as_str(), value.as_bytes()));
    }
    request_sha256(&entries).unwrap_or_else(|_| unreachable!())
}

/// What the plan will write.
struct Planned {
    /// Repository-relative path, forward slashes.
    path: String,
    bytes: Vec<u8>,
    /// The id or path printed on success.
    printed: String,
    /// The id the operation allocated.
    id: String,
    /// The final planned content, classified with the supplied values.
    text: String,
    /// Input labels from the template or the source row.
    input_labels: LabelSet,
    /// The digest of the destination as the planning snapshot read it (`None`: it was absent).
    seen: Option<Sha256>,
}

fn execute(world: &dyn World, policy: &Policy, job: &Job) -> Outcome {
    let lease = match world.write_lease() {
        Ok(lease) => lease,
        Err(err) => return io_err(err),
    };
    let mut journal = match crate::io::state::open(world, world.state_dir(), Via::Cli) {
        Ok(journal) => journal,
        Err(err) => return journal_err(&err),
    };
    let tree = match world.read_tree_under(&lease) {
        Ok(tree) => tree,
        Err(err) => return io_err(err),
    };
    // One clock read serves the model, the plan and the recorded intent, so a run that crosses
    // midnight cannot replan to different bytes when it resumes.
    let today = world.today();
    let (model, _) = check::load(&Host::new(tree, today), &check::Options::new(false));

    let template = match template_text(&model, job) {
        Ok(template) => template,
        Err(out) => return out,
    };
    let template_sha = template.as_ref().map_or_else(String::new, |t| {
        Sha256::digest(t.as_bytes()).as_str().to_owned()
    });

    // Admission of the supplied values and the template comes first, so a request that would
    // be refused is refused whether or not an interrupted original is waiting.
    let screen = screening(job, template.as_deref());
    if let Err(out) = admit(world, policy, &journal, job, &screen) {
        return out;
    }

    let key = operation_key(job.leaf);
    let request = request_hash(job, &template_sha);

    let pending = match pending_for(&journal, &lease, &key, &request) {
        Ok(pending) => pending,
        Err(out) => return out,
    };
    let (planned_on, allocated) = match &pending {
        Some(op) => (op.intent.planned_on, op.intent.allocated.first().cloned()),
        None => (today, None),
    };
    let planned = match plan(
        &model,
        job,
        template.as_deref(),
        allocated.as_ref().map(|r| r.id().as_str()),
        planned_on,
    ) {
        Ok(planned) => Planned {
            seen: seen_digest(&model, &planned.path),
            ..planned
        },
        Err(out) => return out,
    };

    let labels = match admit(world, policy, &journal, job, &planned) {
        Ok(labels) => labels,
        Err(out) => return out,
    };

    let Ok(id) = Id::parse(&planned.id) else {
        return usage_err(format!("`{}` is not an id", planned.id));
    };
    let affected = normalize_affected(&[RecordRef::new(StoreId::Corpus, id)]);
    let writes = WriteSet {
        labels,
        files: vec![WriteFile::new(
            planned.path.clone(),
            Sha256::digest(&planned.bytes).as_str(),
        )],
        affected: affected.clone(),
    };
    let intent = OperationIntent {
        schema: 1,
        key,
        request_sha256: request,
        allocated: affected,
        allocated_store_uid: None,
        planned_on,
    };
    if let Err(out) = commit(
        world,
        &mut journal,
        &lease,
        &intent,
        &writes,
        pending.is_some(),
        &planned,
    ) {
        return out;
    }
    Outcome {
        stdout: format!("{}\n", planned.printed),
        stderr: String::new(),
        code: EXIT_OK,
    }
}

/// The digest of `path` as the planning snapshot read it; `None` when it was absent.
fn seen_digest(model: &Model, path: &str) -> Option<Sha256> {
    match model.tree().get(path) {
        Some(Entry::Bytes(bytes)) => Some(Sha256::digest(bytes)),
        Some(Entry::Original { sha256, .. }) => Sha256::parse(sha256).ok(),
        None => None,
    }
}

/// What admission can classify before anything is planned: the supplied values with the
/// template (a page) or the destination register (a row).
fn screening(job: &Job, template: Option<&str>) -> Planned {
    let text = template.unwrap_or_default().to_owned();
    let path = match job.leaf.row_kind() {
        Some(kind) => fieldbook_core::new::register_path(kind.register())
            .to_string_lossy()
            .replace('\\', "/"),
        None => job
            .page_type
            .map_or_else(String::new, fieldbook_core::new::template_path),
    };
    Planned {
        path,
        bytes: text.clone().into_bytes(),
        printed: String::new(),
        id: String::new(),
        input_labels: template.map_or_else(LabelSet::public, template_labels),
        seen: None,
        text,
    }
}

/// The template text of a `new page`, read from the tree; `None` for a row command.
fn template_text(model: &Model, job: &Job) -> Result<Option<String>, Outcome> {
    let Some(ty) = job.page_type else {
        return Ok(None);
    };
    match model.tree().get(&template_path(ty)) {
        // A template that is not UTF-8 is refused, never rewritten with replacement characters.
        Some(Entry::Bytes(bytes)) => String::from_utf8(bytes.clone()).map(Some).map_err(|_| {
            usage_err(format!(
                "template `{}` is not valid UTF-8",
                fieldbook_core::new::template_path(ty)
            ))
        }),
        _ => Err(new_err_outcome(&NewError::MissingTemplate(ty))),
    }
}

fn pending_for(
    journal: &Journal<'_>,
    lease: &StoreLease,
    key: &OperationKey,
    request: &Sha256,
) -> Result<Option<PendingOperation>, Outcome> {
    let Some(found) = journal.pending_key(lease).map_err(|e| journal_err(&e))? else {
        return Ok(None);
    };
    if &found.key != key {
        return Ok(None);
    }
    journal
        .pending_intent(lease, key, request.clone())
        .map_err(|e| journal_err(&e))
}

/// Plan the write: a fresh plan, or a replan of the id and date an interrupted run recorded.
fn plan(
    model: &Model,
    job: &Job,
    template: Option<&str>,
    recorded_id: Option<&str>,
    planned_on: CivilDate,
) -> Result<Planned, Outcome> {
    let Some(kind) = job.leaf.row_kind() else {
        return plan_page_write(
            model,
            job,
            template.unwrap_or_default(),
            planned_on,
            recorded_id,
        );
    };
    let result = match recorded_id {
        Some(id) => replan_row(model, kind, Some(&job.area), &job.fields, id, planned_on),
        None => plan_row(model, kind, Some(&job.area), &job.fields),
    };
    let row = result.map_err(|e| new_err_outcome(&e))?;
    let bytes = render_register(model, kind, &row).map_err(|e| new_err_outcome(&e))?;
    let mut input_labels = LabelSet::public();
    if kind == RowKind::Claim
        && let Some(version) = job.fields.get("source")
        && let Ok(version) = fieldbook_core::ids::VersionId::parse(version)
    {
        // A missing or unreadable `handling` cell fails closed: admission refuses `unlabelled`.
        input_labels = source_handling(model, &version)
            .and_then(|handling| LabelSet::parse_cell(&handling).ok())
            .unwrap_or_else(LabelSet::unlabelled);
    }
    Ok(row_planned(&row, bytes, input_labels))
}

fn row_planned(row: &RowPlan, bytes: Vec<u8>, input_labels: LabelSet) -> Planned {
    Planned {
        path: row.path.to_string_lossy().replace('\\', "/"),
        printed: row.id.clone(),
        id: row.id.clone(),
        text: String::from_utf8_lossy(&bytes).into_owned(),
        bytes,
        input_labels,
        seen: None,
    }
}

fn plan_page_write(
    model: &Model,
    job: &Job,
    template: &str,
    planned_on: CivilDate,
    recorded_id: Option<&str>,
) -> Result<Planned, Outcome> {
    let (Some(ty), Ok(id)) = (job.page_type, PageId::parse(&job.supplied_id)) else {
        return Err(usage_err("a page needs an id and a type"));
    };
    let page = if recorded_id.is_some() {
        replan_page(model, &id, ty, &job.subarea, template, planned_on)
    } else {
        plan_page(model, &id, ty, &job.subarea, template)
    }
    .map_err(|e| new_err_outcome(&e))?;
    let path = page.path.to_string_lossy().replace('\\', "/");
    Ok(Planned {
        printed: path.clone(),
        path,
        id: job.supplied_id.clone(),
        text: format!("{template}\n{}", page.text),
        bytes: page.text.into_bytes(),
        input_labels: template_labels(template),
        seen: None,
    })
}

/// The labels a template's `handling` front matter names. A template whose `handling` cannot
/// be read completely fails closed as `unlabelled`, which admission refuses.
fn template_labels(template: &str) -> LabelSet {
    template_handling(template).unwrap_or_else(LabelSet::unlabelled)
}

/// Classify every supplied value and the planned content, union the input labels and admit the
/// result to the Corpus. A refusal is recorded once and changes nothing.
fn admit(
    world: &dyn World,
    policy: &Policy,
    journal: &Journal<'_>,
    job: &Job,
    planned: &Planned,
) -> Result<LabelSet, Outcome> {
    let path_sha = Sha256::digest(planned.path.as_bytes());
    let file_sha = Sha256::digest(&planned.bytes);
    let refused = |refusal: &Refusal| -> Outcome {
        let about = RefuseAbout {
            sha256: Some(file_sha.clone()),
            name_sha256: Some(path_sha.clone()),
            labels: Some(refusal.labels.clone()),
            dest: None,
            system: None,
            stream: None,
        };
        let reason = add_refuse_reason(refusal.code);
        if let Err(err) = journal.refuse(Some(job.leaf.verb()), refusal.code, reason, &about) {
            return journal_err(&err);
        }
        Outcome {
            stdout: String::new(),
            stderr: refusal_diag(&planned.path, refusal),
            code: EXIT_FINDINGS,
        }
    };

    let context = label_context(world, job.confirm_public).map_err(|r| refused(&r))?;
    let mut text = String::new();
    text.push_str(&job.supplied_id);
    text.push('\n');
    text.push_str(&job.subarea);
    text.push('\n');
    for (_, value) in job.fields.iter() {
        text.push_str(value);
        text.push('\n');
    }
    text.push_str(&planned.text);
    let found = labels::for_text(&text, &policy.marking_rules(), &context);
    let labels = found.union(&planned.input_labels);

    labels::admit(&labels, &StoreRule::corpus()).map_err(|r| refused(&r))?;
    let git = crate::io::root::git_context(world, world.tree_root()).map_err(io_err)?;
    labels::git_guard(&labels, git).map_err(|r| refused(&r))?;
    Ok(labels)
}

fn refusal_diag(path: &str, refusal: &Refusal) -> String {
    format!(
        "{path}:1:1: error[{}]: store `{}` refused labels `{}` ({})\n",
        refusal.code.id(),
        StoreId::Corpus,
        refusal.labels.to_cell(),
        add_refuse_reason(refusal.code),
    )
}

/// Begin and finish the write, or finish the interrupted one with the same plan.
fn commit(
    world: &dyn World,
    journal: &mut Journal<'_>,
    lease: &StoreLease,
    intent: &OperationIntent,
    writes: &WriteSet,
    recovering: bool,
    planned: &Planned,
) -> Result<(), Outcome> {
    let target = world.tree_root().join(&planned.path);
    if recovering {
        let recovery = journal
            .recover(lease, &intent.key, intent.request_sha256.clone(), writes)
            .map_err(|e| journal_err(&e))?;
        match recovery {
            Recovery::Completed { .. } => return Ok(()),
            Recovery::Resume { audited, .. } => {
                world
                    .write_file(&audited, &target, &planned.bytes)
                    .map_err(io_err)?;
                return journal
                    .end(audited, Word::new_static("ok"))
                    .map_err(|e| journal_err(&e));
            }
            Recovery::None => {}
        }
    }
    // The plan read the file under the lease; an editor save since then is not ours to replace.
    let audited = journal
        .begin_expecting(
            lease,
            intent,
            writes,
            &[(planned.path.as_str(), planned.seen.clone())],
        )
        .map_err(|e| journal_err(&e))?;
    world
        .write_file(&audited, &target, &planned.bytes)
        .map_err(io_err)?;
    journal
        .end(audited, Word::new_static("ok"))
        .map_err(|e| journal_err(&e))
}

fn new_err_outcome(err: &NewError) -> Outcome {
    usage_err(err)
}

fn usage_err(err: impl std::fmt::Display) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: EXIT_USAGE,
    }
}

fn journal_err(err: &JournalError) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: err.exit_code(),
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Result::map_err passes the owned WorldError"
)]
fn io_err(err: WorldError) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: EXIT_IO,
    }
}
