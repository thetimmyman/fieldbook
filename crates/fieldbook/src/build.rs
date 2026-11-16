//! `fieldbook build [--out DIR]`: static site and edition builder (`build-v0`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use fieldbook_core::audit::{
    DestKind, Kind, LabelSet, Now, RecordRef, RefuseAbout, Sha256, StoreId, Via, Word, WriteFile,
    WriteSet,
};
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{self, Code, Diagnostic, Host, Model, Options, Report};
use fieldbook_core::edition::EditionInfo;
use fieldbook_core::egress::{
    self, Allowed, Decision, Delivery, Denial, Dest, LabelledView, Shipped, Transfer,
};
use fieldbook_core::labels::{self, ItemRef, MarkingRules};
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_render::lint::{self, OriginalNames};
use fieldbook_render::meta::{BuildInfo, MetadataError, RenderContext};
use fieldbook_render::site::{self, Site};
use fieldbook_render::source;

use crate::cli::{BuildArgs, EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE};
use crate::io::dirs::{self, BuildRoot};
use crate::io::{Audited, Error, out, state, tree};
use crate::since::{self, SinceError};
use crate::{Outcome, World};

/// Planned and delivery-bound edition output ready for `Journal::begin_export` and [`write_edition_outputs`].
#[derive(Debug)]
pub struct BuildPlan {
    /// Canonical or resolved parent output directory (`DIR`).
    pub out_dir: PathBuf,
    /// Resolved `DIR/edition` directory.
    pub edition_dir: PathBuf,
    /// Resolved `DIR/.fieldbook-out` marker file path.
    pub marker_path: PathBuf,
    /// Exact bytes of `.fieldbook-out`.
    pub marker_bytes: Vec<u8>,
    /// Validated [`RenderContext`] for this edition build.
    pub context: RenderContext,
    /// Rendered site files (`index.html`, `assets/strings.js`, `assets/app.css`).
    pub site: Site,
    /// Provenance input [`ItemRef`]s contributing to this output (`labels::effective_item`).
    pub input_items: Vec<ItemRef>,
    /// Resolved `DIR/originals` directory.
    pub originals_dir: PathBuf,
    /// The shipped originals, by shipped name (`S-0042-v2.html.txt`), byte for byte.
    pub originals: BTreeMap<String, Vec<u8>>,
    /// Journal [`WriteSet`] covering `.fieldbook-out`, every file under `edition/` and every file
    /// under `originals/`.
    pub write_set: WriteSet,
    /// Outbound [`Delivery`] covering optional directory removal, `.fieldbook-out`, all site
    /// files and all shipped originals.
    pub delivery: Delivery,
    /// Delivery-bound [`Allowed`] token issued by `egress::check`.
    pub allowed: Allowed,
}

/// Error returned while planning an edition build before `Journal::begin_export`.
#[derive(Debug)]
pub enum BuildPlanError {
    /// Edition metadata validation failed (`MetadataError`).
    Metadata(MetadataError),
    /// Site lint findings (`FB0603`, `FB0604`, `FB0605`).
    Lint(Vec<Diagnostic>),
    /// Filesystem or scope check failed (`io::Error`).
    Io(Error),
}

/// Collect the sorted provenance input [`ItemRef`]s and journal [`RecordRef`]s from `view`.
#[must_use]
pub fn collect_view_inputs(view: &LabelledView) -> (Vec<ItemRef>, Vec<RecordRef>) {
    let mut item_refs = BTreeSet::new();
    let mut record_refs = BTreeSet::new();

    for src in view.sources() {
        let ver_id = format!("{}-v{}", src.value.source_id, src.value.version);
        item_refs.insert(ItemRef::new(StoreId::Corpus, &ver_id));
        if let Ok(rr) = RecordRef::parse(&format!("corpus/{}", src.value.source_id)) {
            record_refs.insert(rr);
        }
    }
    for page in view.pages() {
        item_refs.insert(ItemRef::new(StoreId::Corpus, page.id.as_str()));
        if let Ok(rr) = RecordRef::parse(&format!("corpus/{}", page.id)) {
            record_refs.insert(rr);
        }
    }
    if record_refs.is_empty()
        && let Ok(fallback) = RecordRef::parse("corpus/S-0001")
    {
        record_refs.insert(fallback);
    }

    (
        item_refs.into_iter().collect(),
        record_refs.into_iter().collect(),
    )
}

/// The shipped originals the tree of `view` holds as bytes, by shipped name.
///
/// A tree read from disk keeps only the hash of an original (`Entry::Original`), so `build`
/// reads those bytes itself; this covers the in-memory trees of tests.
fn in_memory_originals(view: &LabelledView) -> BTreeMap<String, Vec<u8>> {
    source::shipped_originals(view)
        .iter()
        .filter_map(|o| {
            o.entry
                .as_bytes()
                .map(|bytes| (o.shipped_name(), bytes.to_vec()))
        })
        .collect()
}

/// The pure hash comparison of one original: its bytes must hash to the `sha256` of its row.
///
/// # Errors
///
/// Returns an [`Outcome`] with exit code 3 when the hashes differ.
pub fn check_original_hash(
    version_id: &str,
    expected_sha256: &str,
    bytes: &[u8],
) -> Result<(), Outcome> {
    let actual = Sha256::digest(bytes);
    if actual.as_str().eq_ignore_ascii_case(expected_sha256) {
        return Ok(());
    }
    Err(Outcome {
        stdout: String::new(),
        stderr: format!(
            "error: the original of {version_id} does not match the hash in its row; nothing was written\n"
        ),
        code: EXIT_IO,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "shares the 7 plan_edition arguments plus pre-rendered Site, the originals and their names"
)]
fn build_plan_from_site(
    view: &LabelledView,
    context: RenderContext,
    site: Site,
    originals: BTreeMap<String, Vec<u8>>,
    names: &OriginalNames,
    out_dir: &Path,
    eff: &EffectivePolicy,
    labels: &LabelSet,
    now: Now,
    shipped: Shipped,
) -> Result<Result<BuildPlan, Denial>, BuildPlanError> {
    let lint_diags = lint::lint(&site, names);
    if !lint_diags.is_empty() {
        return Err(BuildPlanError::Lint(lint_diags));
    }

    dirs::check_build_roots(out_dir, &[BuildRoot::Edition, BuildRoot::Originals])
        .map_err(BuildPlanError::Io)?;

    let resolved_out_dir =
        crate::io::root::resolve_outside_path(out_dir).map_err(BuildPlanError::Io)?;
    let edition_dir = resolved_out_dir.join(BuildRoot::Edition.as_str());
    let originals_dir = resolved_out_dir.join(BuildRoot::Originals.as_str());
    let marker_path = resolved_out_dir.join(dirs::MARKER_FILE);
    let marker_bytes = dirs::marker_bytes();

    let mut entries = Vec::new();
    if let Some(remove_tx) = dirs::tree_remove_transfer(&edition_dir).map_err(BuildPlanError::Io)? {
        entries.push(remove_tx);
    }
    if let Some(remove_tx) =
        dirs::tree_remove_transfer(&originals_dir).map_err(BuildPlanError::Io)?
    {
        entries.push(remove_tx);
    }
    entries.push(Transfer::write_path(marker_path.clone(), &marker_bytes));

    let mut write_files = Vec::with_capacity(site.files().len() + originals.len() + 1);
    write_files.push(WriteFile::new(
        dirs::MARKER_FILE,
        Sha256::digest(&marker_bytes).as_str().to_owned(),
    ));

    let rules = MarkingRules::default();
    let mut markings: Option<LabelSet> = None;
    let mut note_markings = |bytes: &[u8]| {
        if let Ok(text) = std::str::from_utf8(bytes)
            && let Some(mset) = labels::markings_in(text, &rules)
        {
            markings = Some(match markings.take() {
                Some(prev) => prev.union(&mset),
                None => mset,
            });
        }
    };
    for (rel, bytes) in site.files() {
        let target_path = edition_dir.join(rel);
        entries.push(Transfer::write_path(target_path, bytes));
        write_files.push(WriteFile::new(
            format!("{}/{rel}", BuildRoot::Edition.as_str()),
            Sha256::digest(bytes).as_str().to_owned(),
        ));
        note_markings(bytes);
    }
    // A text original is scanned like a site file, so a marking banner in it reaches the
    // egress check. A PDF is not text here; extraction scans it.
    for (name, bytes) in &originals {
        entries.push(Transfer::write_path(originals_dir.join(name), bytes));
        write_files.push(WriteFile::new(
            format!("{}/{name}", BuildRoot::Originals.as_str()),
            Sha256::digest(bytes).as_str().to_owned(),
        ));
        note_markings(bytes);
    }
    let delivery = Delivery { entries, markings };
    let (input_items, affected) = collect_view_inputs(view);
    let write_set = WriteSet {
        labels: labels.clone(),
        files: write_files,
        affected,
    };

    match egress::check(
        eff,
        &Dest::Edition,
        Some(labels),
        Some(&delivery),
        now,
        shipped,
    ) {
        Decision::Allowed(allowed) => Ok(Ok(BuildPlan {
            out_dir: resolved_out_dir,
            edition_dir,
            marker_path,
            marker_bytes,
            context,
            site,
            originals_dir,
            originals,
            input_items,
            write_set,
            delivery,
            allowed,
        })),
        Decision::Denied(denial) => Ok(Err(denial)),
    }
}

/// Validate `RenderContext`, render [`Site`], lint the rendered site (`FB0603`..=`FB0605`),
/// repeat the root check, construct the [`Delivery`] and [`WriteSet`], and obtain the final
/// delivery-bound [`Allowed`] token.
///
/// The shipped originals are those the tree of `view` holds as bytes; a tree read from disk
/// keeps only their hashes, and [`plan_edition_with_originals`] takes their bytes from the caller.
/// `lint` is given the name of every shipped original either way.
///
/// # Errors
///
/// Returns [`BuildPlanError::Metadata`] if `RenderContext` validation fails,
/// [`BuildPlanError::Lint`] if `fieldbook_render::lint::lint` reports findings or workbook
/// rendering fails, or [`BuildPlanError::Io`] if `dirs::check_build_roots` or path resolution fails.
pub fn plan_edition(
    view: &LabelledView,
    info: &BuildInfo,
    out_dir: &Path,
    eff: &EffectivePolicy,
    labels: &LabelSet,
    now: Now,
    shipped: Shipped,
) -> Result<Result<BuildPlan, Denial>, BuildPlanError> {
    let context = site::render_context(view, info).map_err(BuildPlanError::Metadata)?;
    let site = site::render_with_info(view, &context, info).map_err(|e| {
        BuildPlanError::Lint(vec![Diagnostic::new(
            Code::Fb0605,
            false,
            fieldbook_render::xlsx::OUTPUT_PATH.to_owned(),
            1,
            1,
            e.to_string(),
        )])
    })?;
    build_plan_from_site(
        view,
        context,
        site,
        in_memory_originals(view),
        &source::original_names(view),
        out_dir,
        eff,
        labels,
        now,
        shipped,
    )
}

/// Like [`plan_edition`], with the bytes of every shipped original read by the caller, by
/// shipped name (`S-0042-v2.html.txt`). Every original goes into the [`WriteSet`] and the
/// [`Delivery`] with its hash, and the names are the [`OriginalNames`] given to `lint`.
///
/// # Errors
///
/// Returns [`BuildPlanError`] as [`plan_edition`] does.
#[expect(
    clippy::too_many_arguments,
    reason = "shares the 7 plan_edition arguments plus the originals"
)]
pub fn plan_edition_with_originals(
    view: &LabelledView,
    info: &BuildInfo,
    out_dir: &Path,
    eff: &EffectivePolicy,
    labels: &LabelSet,
    now: Now,
    shipped: Shipped,
    originals: BTreeMap<String, Vec<u8>>,
) -> Result<Result<BuildPlan, Denial>, BuildPlanError> {
    plan_edition_with_previous(
        view, info, out_dir, eff, labels, now, shipped, originals, None,
    )
}

/// Like [`plan_edition_with_originals`], rendering `changes.html` against the earlier
/// [`EditionInfo`] that `build --since` named, when there is one.
#[expect(
    clippy::too_many_arguments,
    reason = "shares the 8 plan_edition_with_originals arguments plus the previous edition"
)]
pub fn plan_edition_with_previous(
    view: &LabelledView,
    info: &BuildInfo,
    out_dir: &Path,
    eff: &EffectivePolicy,
    labels: &LabelSet,
    now: Now,
    shipped: Shipped,
    originals: BTreeMap<String, Vec<u8>>,
    previous: Option<&EditionInfo>,
) -> Result<Result<BuildPlan, Denial>, BuildPlanError> {
    let context = site::render_context(view, info).map_err(BuildPlanError::Metadata)?;
    let site = site::render_with_previous(view, &context, info, previous).map_err(|e| {
        BuildPlanError::Lint(vec![Diagnostic::new(
            Code::Fb0605,
            false,
            fieldbook_render::xlsx::OUTPUT_PATH.to_owned(),
            1,
            1,
            e.to_string(),
        )])
    })?;
    let names = OriginalNames(originals.keys().cloned().collect());
    build_plan_from_site(
        view, context, site, originals, &names, out_dir, eff, labels, now, shipped,
    )
}

/// Like [`plan_edition`], using an already-rendered [`Site`] (e.g. for testing lint failures in the build pipeline).
///
/// # Errors
///
/// Returns [`BuildPlanError`] on lint findings or root/path resolution errors.
#[expect(
    clippy::too_many_arguments,
    reason = "shares the 7 plan_edition arguments plus pre-rendered Site"
)]
pub fn plan_edition_with_site(
    view: &LabelledView,
    context: RenderContext,
    site: Site,
    out_dir: &Path,
    eff: &EffectivePolicy,
    labels: &LabelSet,
    now: Now,
    shipped: Shipped,
) -> Result<Result<BuildPlan, Denial>, BuildPlanError> {
    build_plan_from_site(
        view,
        context,
        site,
        in_memory_originals(view),
        &source::original_names(view),
        out_dir,
        eff,
        labels,
        now,
        shipped,
    )
}

/// Execute the final outward writes for `plan` under `audited`: replace `DIR/edition` and
/// `DIR/originals` via [`out::replace`] and write `DIR/.fieldbook-out` via
/// [`crate::io::atomic::write_outside`].
///
/// # Errors
///
/// Returns [`Error`] if any delivery-binding check, marker check, directory removal, or atomic
/// file write fails.
pub fn write_edition_outputs(plan: &BuildPlan, audited: &Audited) -> Result<(), Error> {
    // Check every file against the delivery before any folder is touched, so a refused byte or
    // path in either folder leaves both as they were.
    for (rel, bytes) in plan.site.files() {
        let target = plan.edition_dir.join(rel);
        let _ = crate::io::atomic::check_outside_write_binding(&plan.allowed, &target, bytes)?;
    }
    for (rel, bytes) in &plan.originals {
        let target = plan.originals_dir.join(rel);
        let _ = crate::io::atomic::check_outside_write_binding(&plan.allowed, &target, bytes)?;
    }
    out::replace(&plan.edition_dir, plan.site.files(), &plan.allowed, audited)?;
    out::replace(&plan.originals_dir, &plan.originals, &plan.allowed, audited)?;
    let _ = crate::io::atomic::write_outside(
        &plan.allowed,
        &plan.marker_path,
        &plan.marker_bytes,
        audited,
    )?;
    Ok(())
}

fn format_fb0600_failure(report: &Report) -> Outcome {
    let mut out = report.render_text();
    let diag = Diagnostic::new(
        Code::Fb0600,
        false,
        "corpus/corpus.toml",
        1,
        1,
        format!(
            "refusing to build: Corpus has {} check error(s)",
            report.errors()
        ),
    );
    let mut extra = Report::new();
    extra.push(diag);
    out.push_str(&extra.render_text());
    Outcome {
        stdout: String::new(),
        stderr: out,
        code: EXIT_FINDINGS,
    }
}

fn map_root_error(err: Error) -> Outcome {
    match err {
        Error::OutOfScope(root_path) => Outcome {
            stdout: String::new(),
            stderr: format!("{}\n", dirs::foreign_root_message(&root_path)),
            code: EXIT_USAGE,
        },
        other => Outcome {
            stdout: String::new(),
            stderr: format!("{other}\n"),
            code: EXIT_IO,
        },
    }
}

fn resolve_out_dir(world: &dyn World, arg_out: &Path) -> PathBuf {
    if arg_out.is_absolute() {
        arg_out.to_path_buf()
    } else if arg_out == Path::new("out") {
        world.tree_root().join(arg_out)
    } else {
        world.cwd().join(arg_out)
    }
}

/// The newest `## edition-NNN` heading in the snapshot's `corpus/changes.md`, which names the
/// edition a released build is writing.
fn current_edition_heading(snapshot: &check::Tree) -> Option<String> {
    snapshot
        .get_bytes("corpus/changes.md")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(fieldbook_core::edition::latest_edition_heading)
        .map(str::to_owned)
}

/// Map a refused `--since` value to the build's outcome: one plain line on stderr, no stdout,
/// and no write to the tree.
fn since_outcome(err: &SinceError) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: err.exit_code(),
    }
}

fn load_checked_snapshot(
    tree: fieldbook_core::check::Tree,
    today: fieldbook_core::CivilDate,
) -> Result<Model, Outcome> {
    let host = Host::new(tree, today);
    let options = Options::new(false);
    let (model, report) = check::load(&host, &options);
    if !report.is_ok() {
        return Err(format_fb0600_failure(&report));
    }
    Ok(model)
}

/// Run `fieldbook build` with an explicit [`BuildInfo`].
#[must_use]
pub fn run_with_info(args: &BuildArgs, world: &dyn World, info: &BuildInfo) -> Outcome {
    let out_dir = resolve_out_dir(world, &args.out);
    let today = world.today();

    // 0. Read `--since` before any write: a refused value must leave the tree and the journal
    //    untouched, so it is read and checked here, before the journal opens.
    let previous = match args.since.as_deref().map(since::read_since).transpose() {
        Ok(p) => p,
        Err(err) => return since_outcome(&err),
    };

    // 1. Read coherent snapshot under shared store lease and run non-strict check.
    let initial_tree = match world.read_tree() {
        Ok(t) => t,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };
    let initial_gen = initial_tree.generation_hash();
    let mut current_heading = current_edition_heading(&initial_tree);
    let mut model = match load_checked_snapshot(initial_tree, today) {
        Ok(m) => m,
        Err(outcome) => return outcome,
    };

    // 1a. Refuse an out-of-order `--since` against the read-only snapshot, before the write
    //     lease: taking the lease creates the state folder and its lock files, and a refused
    //     `--since` writes nothing. The check repeats under the lease below, in case the
    //     Corpus changed in between. Only a released build is numbered: an unreleased one
    //     is `unreleased` whatever `changes.md` starts with, so its heading orders nothing.
    if info.is_released()
        && let Some(previous) = &previous
        && let Err(err) = since::check_order(previous, current_heading.as_deref())
    {
        return since_outcome(&err);
    }

    // 2. Check target build roots before opening the journal or taking the exclusive lease.
    if let Err(err) = dirs::check_build_roots(&out_dir, &[BuildRoot::Edition, BuildRoot::Originals])
    {
        return map_root_error(err);
    }

    // 3. Take exclusive store lease and confirm snapshot generation is unchanged; if changed,
    //    reload and re-check under the lease before opening the journal.
    let lease = match world.write_lease() {
        Ok(l) => l,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };
    let lease_tree = match world.read_tree_under(&lease) {
        Ok(t) => t,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };
    if lease_tree.generation_hash() != initial_gen {
        current_heading = current_edition_heading(&lease_tree);
        model = match load_checked_snapshot(lease_tree, today) {
            Ok(m) => m,
            Err(outcome) => return outcome,
        };
    }
    // Recheck the order under the lease: the snapshot read under it may name a newer edition.
    if info.is_released()
        && let Some(previous) = &previous
        && let Err(err) = since::check_order(previous, current_heading.as_deref())
    {
        return since_outcome(&err);
    }

    // 4. Open the audit journal and capture `now`, `shipped`, and `EffectivePolicy::floor(false)`.
    let mut journal = match state::open(world, world.state_dir(), Via::Cli) {
        Ok(j) => j,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
    };
    let now = world.now();
    let shipped = egress::shipped(journal.last_time_of(Kind::AuditExport), now);
    let eff = EffectivePolicy::floor(false);

    // 5. Select the edition view (`Dest::Edition`).
    let selection = match egress::check(&eff, &Dest::Edition, None, None, now, shipped) {
        Decision::Allowed(sel) => sel,
        Decision::Denied(denial) => {
            let about = RefuseAbout {
                labels: (!denial.labels.is_empty())
                    .then(|| LabelSet::from_ids(denial.labels.clone())),
                dest: Some(DestKind::Edition),
                ..RefuseAbout::default()
            };
            if let Err(j_err) =
                journal.refuse(Some("build"), denial.code, denial.reason.to_word(), &about)
            {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{j_err}\n"),
                    code: j_err.exit_code(),
                };
            }
            return Outcome {
                stdout: String::new(),
                stderr: format!(
                    "error[{}]: edition build refused ({})\n",
                    denial.code, denial.reason
                ),
                code: EXIT_USAGE,
            };
        }
    };

    let view = LabelledView::new(&model, &selection, &ReadableStores::corpus());

    // 6. Read the shipped originals, hash each against its row, then plan and render the
    //    edition, validating RenderContext and binding Delivery.
    let originals = match read_shipped_originals(world, &view) {
        Ok(o) => o,
        Err(outcome) => return outcome,
    };
    let plan = match plan_edition_with_previous(
        &view,
        info,
        &out_dir,
        &eff,
        &LabelSet::public(),
        now,
        shipped,
        originals,
        previous.as_ref(),
    ) {
        Ok(Ok(plan)) => plan,
        Ok(Err(denial)) => {
            let about = RefuseAbout {
                labels: (!denial.labels.is_empty())
                    .then(|| LabelSet::from_ids(denial.labels.clone())),
                dest: Some(DestKind::Edition),
                ..RefuseAbout::default()
            };
            if let Err(j_err) =
                journal.refuse(Some("build"), denial.code, denial.reason.to_word(), &about)
            {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{j_err}\n"),
                    code: j_err.exit_code(),
                };
            }
            return Outcome {
                stdout: String::new(),
                stderr: format!(
                    "error[{}]: edition build refused ({})\n",
                    denial.code, denial.reason
                ),
                code: EXIT_USAGE,
            };
        }
        Err(BuildPlanError::Metadata(meta_err)) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("error: {meta_err}\n"),
                code: EXIT_FINDINGS,
            };
        }
        Err(BuildPlanError::Lint(diags)) => {
            let report = Report::from_parts(diags, Vec::new());
            return Outcome {
                stdout: String::new(),
                stderr: report.render_text(),
                code: EXIT_FINDINGS,
            };
        }
        Err(BuildPlanError::Io(io_err)) => return map_root_error(io_err),
    };

    // 7. Append `write.begin`, replace `DIR/edition` and write `.fieldbook-out`, and append `write.end`.
    let audited = match journal.begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
    {
        Ok(a) => a,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
    };

    if let Err(err) = write_edition_outputs(&plan, &audited) {
        let _ = journal.end(audited, Word::new_static("failed"));
        return map_root_error(err);
    }

    if let Err(err) = journal.end(audited, Word::new_static("ok")) {
        return Outcome {
            stdout: String::new(),
            stderr: format!("{err}\n"),
            code: err.exit_code(),
        };
    }

    drop(lease);
    let mut stdout = String::new();
    for vid in source::held_versions(&view) {
        stdout.push_str("held ");
        stdout.push_str(vid.as_str());
        stdout.push('\n');
    }
    Outcome {
        stdout,
        stderr: String::new(),
        code: EXIT_OK,
    }
}

/// Reads the bytes of every shipped original of `view` (from the tree when it keeps them,
/// otherwise from `corpus/sources/public/` under the tree root) and checks each against the
/// `sha256` of its row. The edition's originals are the public originals of the Corpus; no
/// other folder is read.
fn read_shipped_originals(
    world: &dyn World,
    view: &LabelledView,
) -> Result<BTreeMap<String, Vec<u8>>, Outcome> {
    let mut originals = BTreeMap::new();
    for original in source::shipped_originals(view) {
        let bytes =
            match original.entry.as_bytes() {
                Some(bytes) => bytes.to_vec(),
                None => tree::read_file(&world.tree_root().join(original.stored_path())).map_err(
                    |err| Outcome {
                        stdout: String::new(),
                        stderr: format!("{err}\n"),
                        code: EXIT_IO,
                    },
                )?,
            };
        check_original_hash(&original.version_id(), original.row_sha256(), &bytes)?;
        originals.insert(original.shipped_name(), bytes);
    }
    Ok(originals)
}

/// Run `fieldbook build` reading `SOURCE_DATE_EPOCH` and compile-time `FIELDBOOK_GIT_SHA`
/// from the process boundary.
#[must_use]
pub fn run(args: &BuildArgs, world: &dyn World) -> Outcome {
    let info = crate::io::build_info_from_env();
    run_with_info(args, world, &info)
}

fn print_fingerprint_error(err: &crate::io::WorldError) {
    let mut err_stream = std::io::stderr().lock();
    let _ = writeln!(err_stream, "{err}");
    let _ = err_stream.flush();
}

fn build_once(args: &BuildArgs, world: &dyn World, info: &BuildInfo, out: &mut dyn Write) -> bool {
    let outcome = run_with_info(args, world, info);
    if out.write_all(outcome.stdout.as_bytes()).is_err() || out.flush().is_err() {
        return false;
    }
    let mut err_stream = std::io::stderr().lock();
    let _ = err_stream.write_all(outcome.stderr.as_bytes());
    let _ = err_stream.flush();

    let summary = if outcome.code == 0 {
        "built\n".to_string()
    } else {
        format!("build failed (exit {})\n", outcome.code)
    };
    if out.write_all(summary.as_bytes()).is_err() || out.flush().is_err() {
        return false;
    }
    true
}

/// Rebuild the edition when the Corpus changes, polling through `poll`.
#[must_use]
pub fn watch_with_info(
    args: &BuildArgs,
    world: &dyn World,
    info: &BuildInfo,
    poll: &mut dyn crate::io::watch::Poll,
    out: &mut dyn Write,
) -> u8 {
    let mut last = match poll.fingerprint() {
        Ok(fp) => fp,
        Err(err) => {
            print_fingerprint_error(&err);
            return EXIT_IO;
        }
    };

    if !build_once(args, world, info, out) {
        return EXIT_IO;
    }

    loop {
        if !poll.pause() {
            return EXIT_OK;
        }

        let mut cur = match poll.fingerprint() {
            Ok(fp) => fp,
            Err(err) => {
                print_fingerprint_error(&err);
                return EXIT_IO;
            }
        };

        if cur == last {
            continue;
        }

        // Settle: wait until two consecutive polls agree.
        loop {
            if !poll.pause() {
                return EXIT_OK;
            }
            let again = match poll.fingerprint() {
                Ok(fp) => fp,
                Err(err) => {
                    print_fingerprint_error(&err);
                    return EXIT_IO;
                }
            };
            if again == cur {
                break;
            }
            cur = again;
        }

        last = cur;
        if !build_once(args, world, info, out) {
            return EXIT_IO;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BYTES: &[u8] = b"synthetic original bytes, made up for tests\n";

    #[test]
    fn original_hash_mismatch_is_an_error() {
        let row_hash = Sha256::digest(BYTES).as_str().to_owned();
        assert!(check_original_hash("S-0001-v1", &row_hash, BYTES).is_ok());

        let other = "0".repeat(64);
        let err = check_original_hash("S-0001-v1", &other, BYTES)
            .err()
            .unwrap_or_else(|| unreachable!("a different hash must be refused"));
        assert_eq!(err.code, EXIT_IO);
        assert!(err.stderr.contains("S-0001-v1"));
        assert!(err.stdout.is_empty());

        assert!(check_original_hash("S-0001-v1", &row_hash, b"changed bytes").is_err());
    }
}
