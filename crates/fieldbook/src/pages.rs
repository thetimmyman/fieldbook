//! The one binary Desk writer: `fieldbook status --page` (`AD-17`, `status-command`).
//!
//! **Responsibility.** Read the reports the desk pages show from the state folder under the
//! retained exclusive store lease, admit their labels to the State store, then write the
//! pages and their assets through the atomic helper under a `StateScope` token.
//!
//! **Rules.** Pages go only into `pages/` and `assets/` of the state folder. A failed write
//! leaves the old page. The writer never runs a command and never changes the Corpus.

#![expect(
    clippy::disallowed_methods,
    reason = "the desk writer reads the state reports and the audit journal it renders"
)]

use std::path::Path;

use fieldbook_core::audit::chain::RawJsonValue;
use fieldbook_core::audit::{LabelSet, Via};
use fieldbook_core::backup::BACKUP_TARGET_TEMPLATE;
use fieldbook_core::labels::{self, StoreId};
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::report::{AddReport, AuditShowReport, BackupList, ReportSource};
use fieldbook_render::desk::{self, activity, backup, import};

use crate::World;

/// The number of newest journal events the Activity page shows.
pub const PAGE_EVENTS: usize = desk::PAGE_EVENTS;

/// The prefix of every line the writer prints for a page it wrote.
pub const WROTE: &str = "wrote ";

/// What one `status --page` run wrote, and whether a source failed to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskWrite {
    /// One `wrote <path>` line for each page, in `PAGES` order.
    pub lines: Vec<String>,
    /// `true` when a source was unavailable: the diagnostic page was written and the command
    /// exits 3.
    pub source_failed: bool,
}

/// Reads the desk sources, admits them to State, and writes every page in `PAGES`.
///
/// Returns one `wrote <path>` line for each page, in `PAGES` order.
///
/// # Errors
///
/// Returns the diagnostic to print on standard error when a source is refused by State
/// admission (`FB0412`), when the lease or the journal cannot be acquired, or when a page
/// cannot be written.
pub fn write_desk_pages(
    world: &dyn World,
    policy: &EffectivePolicy,
    today: fieldbook_core::CivilDate,
) -> Result<DeskWrite, String> {
    let state_dir = world.state_dir().to_path_buf();

    let lease = world.write_lease().map_err(|err| format!("{err}\n"))?;
    let coherent_tree = world
        .read_tree_under(&lease)
        .map_err(|err| format!("{err}\n"))?;
    let (coherent_model, _) = fieldbook_core::check::load(
        &fieldbook_core::check::Host::new(coherent_tree, today),
        &fieldbook_core::check::Options::new(false),
    );

    // The reports are read before the journal is opened, so a state folder that holds no
    // journal still renders its empty Activity page rather than the `journal.open` line the
    // write itself appends.
    let mut import_source = read_add_report(&state_dir);
    let mut input_labels = None;
    if let ReportSource::Ready(report) = &import_source {
        input_labels =
            crate::io::backup_classify::report_input_labels(world, &coherent_model, report);
        if input_labels.is_none() {
            import_source = ReportSource::Unavailable {
                diagnostic: "Import provenance is unavailable".to_owned(),
            };
        }
    }
    let show_source = crate::audit::desk_show(&state_dir, PAGE_EVENTS);
    let verify_source = crate::audit::desk_verify(&state_dir);
    let backup_source = read_backup(world);
    let source_failed = matches!(import_source, ReportSource::Unavailable { .. })
        || matches!(show_source, ReportSource::Unavailable { .. })
        || matches!(verify_source, ReportSource::Unavailable { .. })
        || matches!(backup_source, ReportSource::Unavailable { .. });

    let rule = policy.store_rule(&StoreId::State);
    let ready_add = match &import_source {
        ReportSource::Ready(report) => Some(report),
        _ => None,
    };
    if crate::io::backup_classify::broken_desk_ledger(world, &coherent_model, ready_add) {
        return Err(format!(
            "error[{}]: status --page refused: a store label ledger (labels.csv) is unreadable\n",
            fieldbook_core::check::Code::Fb0412.id()
        ));
    }
    let mut carried = LabelSet::public();
    if let fieldbook_core::policy::PolicyState::Loaded(loaded) = policy.state() {
        let state_rule = loaded.store_rule(&StoreId::State);
        if !state_rule
            .default
            .contains(&fieldbook_core::labels::UNLABELLED)
        {
            carried = carried.union(&state_rule.default);
        }
    }
    if let ReportSource::Ready(report) = &import_source {
        carried = carried.union(input_labels.as_ref().unwrap_or(&report.labels));
    }
    if let ReportSource::Ready(list) = &backup_source {
        if let Some(target) = &list.target {
            carried = carried.union(&LabelSet::owner_supplied());
            let rules = policy.marking_rules();
            if let Some(mark_set) = fieldbook_core::labels::markings_in(&target.id, &rules) {
                carried = carried.union(&mark_set);
            }
        }
        if let Some(labels) = backup_snapshot_labels(list) {
            carried = carried.union(&labels);
        }
    }
    if let Some(labels) = read_last_backup_labels(&state_dir) {
        carried = carried.union(&labels);
    }
    if let ReportSource::Ready(report) = &show_source
        && let Some(labels) = event_labels(report)
    {
        carried = carried.union(&labels);
    }
    if let Err(refusal) = labels::admit(&carried, &rule) {
        return Err(format!(
            "error[{}]: status --page refused: the desk content carries a label the State store does not hold\n",
            refusal.code.id()
        ));
    }

    for path in desk::assets()
        .iter()
        .map(|(path, _)| *path)
        .chain(desk::PAGES.iter().map(|page| page.path))
    {
        let ctx = crate::io::root::git_context(world, &state_dir.join(path))
            .map_err(|err| format!("{err}\n"))?;
        if let Err(refusal) = labels::git_guard(&carried, ctx) {
            return Err(format!(
                "error[{}]: status --page refused by Git boundary\n",
                refusal.code.id()
            ));
        }
    }

    let journal =
        crate::io::state::open(world, &state_dir, Via::Cli).map_err(|err| format!("{err}\n"))?;
    let audited = journal
        .state_scope(&lease)
        .map_err(|err| format!("{err}\n"))?;

    for (path, bytes) in desk::assets() {
        let full = state_dir.join(path);
        world
            .write_file(&audited, &full, bytes.as_bytes())
            .map_err(|err| format!("{err}\n"))?;
    }

    let written = world.now().to_string();
    let render = |title: &str, body: String| desk::frame::frame(title, &written, &body);
    let pages: [(&str, String); 3] = [
        (
            import::PATH,
            render(import::TITLE, import::render(&import_source)),
        ),
        (
            activity::PATH,
            render(
                activity::TITLE,
                activity::render(&show_source, &verify_source),
            ),
        ),
        (
            backup::PATH,
            render(backup::TITLE, backup::render(&backup_source)),
        ),
    ];

    let mut lines = Vec::with_capacity(pages.len());
    for (path, html) in pages {
        let full = state_dir.join(path);
        world
            .write_file(&audited, &full, html.as_bytes())
            .map_err(|err| format!("{err}\n"))?;
        lines.push(format!("{WROTE}{path}"));
    }
    Ok(DeskWrite {
        lines,
        source_failed,
    })
}

fn read_add_report(state_dir: &Path) -> ReportSource<AddReport> {
    let path = state_dir.join("reports/last-add.json");
    match std::fs::read_to_string(&path) {
        Ok(text) => match crate::report::parse_saved_add_report(&text) {
            Ok(report) => ReportSource::Ready(report),
            Err(_) => ReportSource::Unavailable {
                diagnostic: "Import report is not readable".to_owned(),
            },
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ReportSource::Missing,
        Err(err) => ReportSource::Unavailable {
            diagnostic: format!("reports/last-add.json could not be read: {}", err.kind()),
        },
    }
}

fn read_last_backup_labels(state_dir: &Path) -> Option<LabelSet> {
    let text = std::fs::read_to_string(state_dir.join("reports/last-backup.json")).ok()?;
    let report: fieldbook_core::report::LastBackupReport = serde_json::from_str(&text).ok()?;
    if report.verification != "snapshot" || report.labels.is_empty() {
        return None;
    }
    Some(LabelSet::from_ids(report.labels))
}

fn labels_from_names<'a>(items: impl IntoIterator<Item = Option<&'a str>>) -> LabelSet {
    let mut ids = Vec::new();
    let mut count = 0usize;
    let mut invalid = false;
    for item in items {
        count += 1;
        match item.and_then(|id| fieldbook_core::audit::LabelId::parse(id).ok()) {
            Some(id) => ids.push(id),
            None => invalid = true,
        }
    }
    if count == 0 || invalid {
        LabelSet::from_ids(ids).union(&LabelSet::unlabelled())
    } else {
        LabelSet::from_ids(ids)
    }
}

fn backup_snapshot_labels(list: &BackupList) -> Option<LabelSet> {
    let mut out: Option<LabelSet> = None;
    for snapshot in &list.snapshots {
        let snap_set = labels_from_names(snapshot.labels.iter().map(|s| Some(s.as_str())));
        out = Some(match out {
            Some(prev) => prev.union(&snap_set),
            None => snap_set,
        });
    }
    out
}

fn event_labels(report: &AuditShowReport) -> Option<LabelSet> {
    let mut out: Option<LabelSet> = None;
    for event in &report.events {
        if event.get("kind").and_then(RawJsonValue::as_str) == Some("refuse") {
            continue;
        }
        if let Some(RawJsonValue::Array(items)) = event.get("labels") {
            let event_set = labels_from_names(items.iter().map(RawJsonValue::as_str));
            out = Some(match out {
                Some(prev) => prev.union(&event_set),
                None => event_set,
            });
        }
    }
    out
}

fn read_backup(world: &dyn World) -> ReportSource<BackupList> {
    let outcome = crate::backup::run_list(
        world,
        &crate::backup::BackupListArgs {
            target: None,
            in_dir: None,
            json: true,
        },
    );
    match serde_json::from_str::<BackupList>(&outcome.stdout) {
        Ok(list)
            if list
                .findings
                .iter()
                .any(|finding| finding.severity == "error") =>
        {
            ReportSource::Unavailable {
                diagnostic: list
                    .findings
                    .iter()
                    .map(|finding| finding.code.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            }
        }
        Ok(list) => ReportSource::Ready(list),
        Err(_) => ReportSource::Unavailable {
            diagnostic: "backup list did not return a readable report".to_owned(),
        },
    }
}

/// The two role lines `status` prints when `settings.toml` names no role, in order.
#[must_use]
pub const fn role_lines() -> [&'static str; 2] {
    [fieldbook_core::verify::ROLE_LINE, BACKUP_TARGET_TEMPLATE]
}

/// Returns whether `PAGES` still holds every desk page the writer renders.
#[must_use]
pub fn pages_are_complete() -> bool {
    desk::PAGES
        .iter()
        .all(|page| [import::PATH, activity::PATH, backup::PATH].contains(&page.path))
}
