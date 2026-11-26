//! Implementation of `fieldbook audit verify` and `fieldbook audit show`.

use std::collections::BTreeMap;
use std::path::Path;

use fieldbook_core::audit::chain::{self, RawEventObject, parse_json_object};
use fieldbook_core::audit::export::{Finding, Manifest, verify_export};
use fieldbook_core::audit::{
    Event, EventPayload, Id, Kind, LabelId, Now, RecordRef, Sha256, Via, ZERO_HASH,
};
use fieldbook_core::report::{
    AuditExport, AuditHead, AuditShowReport, AuditVerifyExport, AuditVerifyFinding,
    AuditVerifyReport, REPORT_VERSION,
};

use crate::cli::{
    AuditArgs, AuditExportArgs, AuditShowArgs, AuditSubcommand, AuditVerifyArgs, EXIT_FINDINGS,
    EXIT_IO, EXIT_OK, EXIT_USAGE,
};
use crate::io::journal::{Journal, JournalError, list_segments, read_segment_bytes};
use crate::policy::PolicyCtx;
use crate::{Outcome, World};

/// Dispatch `fieldbook audit` subcommands.
#[must_use]
pub fn run(args: &AuditArgs, state_flag: Option<&Path>, world: &dyn World) -> Outcome {
    match &args.command {
        AuditSubcommand::Verify(verify_args) => run_verify(verify_args, state_flag, world),
        AuditSubcommand::Show(show_args) => run_show(show_args, state_flag, world),
        AuditSubcommand::Export(export_args) => run_export(export_args, state_flag, world),
    }
}

/// Format an [`AuditVerifyReport`] as pretty JSON with a trailing newline.
#[must_use]
pub fn render_verify_json(report: &AuditVerifyReport) -> String {
    let mut json = serde_json::to_string_pretty(report).unwrap_or_default();
    json.push('\n');
    json
}

/// Format an [`AuditShowReport`] as pretty JSON with a trailing newline.
#[must_use]
pub fn render_show_json(report: &AuditShowReport) -> String {
    let mut json = serde_json::to_string_pretty(report).unwrap_or_default();
    json.push('\n');
    json
}

fn resolve_state_dir(state_flag: Option<&Path>, world: &dyn World) -> std::path::PathBuf {
    match state_flag {
        Some(flag) => crate::io::state::resolve(world.tree_root(), world.cwd(), Some(flag)),
        None => world.state_dir().to_path_buf(),
    }
}

/// Reads the `audit show` report the Activity page shows, with no filters.
///
/// `Missing` means the journal holds no events yet; an unreadable journal is `Unavailable`.
#[must_use]
pub fn desk_show(
    state_dir: &Path,
    last: usize,
) -> fieldbook_core::report::ReportSource<AuditShowReport> {
    use fieldbook_core::report::ReportSource;
    let audit_dir = state_dir.join("audit");
    if !audit_dir.exists() {
        return ReportSource::Missing;
    }
    let segments = match list_segments(&audit_dir) {
        Ok(segments) => segments,
        Err(err) => {
            return ReportSource::Unavailable {
                diagnostic: format!("audit/journal.jsonl could not be listed: {err}"),
            };
        }
    };
    let mut total_count = 0usize;
    let mut head_seq = 0u64;
    let mut head_hash = ZERO_HASH.to_owned();
    let mut matching_rows: Vec<RawEventObject> = Vec::new();
    for (_, seg_path) in segments {
        let bytes = match read_segment_bytes(&seg_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                return ReportSource::Unavailable {
                    diagnostic: format!("audit/journal.jsonl could not be read: {err}"),
                };
            }
        };
        let content = String::from_utf8_lossy(&bytes);
        for raw_line in content.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let Ok((_, stored_hash)) = chain::split_hash_suffix(line) else {
                continue;
            };
            let Ok(raw_obj) = parse_json_object(line) else {
                continue;
            };
            let parsed_event = Event::parse_line(line).ok().map(|(event, _)| event);
            total_count += 1;
            head_seq = parsed_event
                .as_ref()
                .map_or(total_count as u64, |event| event.seq);
            head_hash = stored_hash.as_str().to_owned();
            matching_rows.push(raw_obj);
        }
    }
    if total_count == 0 {
        return ReportSource::Missing;
    }
    let skip = matching_rows.len().saturating_sub(last);
    let events: Vec<RawEventObject> = matching_rows.into_iter().skip(skip).collect();
    ReportSource::Ready(AuditShowReport {
        report_version: REPORT_VERSION,
        verb: "audit show",
        head: AuditHead {
            seq: head_seq,
            hash: head_hash,
        },
        count: total_count,
        shown: events.len(),
        events,
        exit: EXIT_OK,
    })
}

/// Reads the `audit verify` report the Activity page shows.
#[must_use]
pub fn desk_verify(state_dir: &Path) -> fieldbook_core::report::ReportSource<AuditVerifyReport> {
    use fieldbook_core::report::ReportSource;
    let audit_dir = state_dir.join("audit");
    if !audit_dir.exists() {
        return ReportSource::Missing;
    }
    let inspection = match inspect_journal(state_dir) {
        Ok(inspection) => inspection,
        Err(err) => {
            return ReportSource::Unavailable {
                diagnostic: format!("audit/journal.jsonl could not be verified: {err}"),
            };
        }
    };
    let exit = if inspection.has_error {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    };
    ReportSource::Ready(AuditVerifyReport {
        report_version: REPORT_VERSION,
        verb: "audit verify",
        state: inspection.state,
        events: inspection.events,
        head: AuditHead {
            seq: inspection.head_seq,
            hash: inspection.head_hash,
        },
        findings: inspection
            .findings
            .iter()
            .map(|finding| AuditVerifyFinding {
                code: finding.code,
                severity: finding.severity,
                seq: finding.seq,
            })
            .collect(),
        unfinished: inspection.unfinished,
        exit,
    })
}

#[derive(Debug, Clone)]
struct FindingDetail {
    code: &'static str,
    severity: &'static str,
    seq: u64,
    message: String,
}

#[derive(Debug, Clone)]
struct VerifyInspection {
    state: &'static str,
    events: usize,
    head_seq: u64,
    head_hash: String,
    findings: Vec<FindingDetail>,
    unfinished: Vec<u64>,
    has_error: bool,
    broken_chain_repair: Option<(u64, u64, Sha256)>,
    seq_hashes: BTreeMap<u64, String>,
}

fn inspect_journal(state_dir: &Path) -> Result<VerifyInspection, JournalError> {
    let audit_dir = state_dir.join("audit");
    let segments = list_segments(&audit_dir)?;

    let mut prev_hex = ZERO_HASH.to_owned();
    let mut expected_seq = 1u64;
    let mut head_seq = 0u64;
    let mut head_hash = ZERO_HASH.to_owned();
    let mut events_count = 0usize;
    let mut findings: Vec<FindingDetail> = Vec::new();
    let mut seq_hashes: BTreeMap<u64, String> = BTreeMap::new();

    let mut open_write_begin: Option<u64> = None;
    let mut open_ai_begin: BTreeMap<u64, u64> = BTreeMap::new();
    let mut unfinished: Vec<u64> = Vec::new();
    let mut prev_at: Option<Now> = None;

    let mut has_broken = false;
    let mut has_torn = false;
    let mut chain_error_reported = false;
    let mut broken_chain_repair: Option<(u64, u64, Sha256)> = None;
    let mut last_good_seq = 0u64;
    let mut last_good_hash = Sha256::parse(ZERO_HASH).unwrap_or_else(|_| unreachable!());

    let num_segments = segments.len();
    for (seg_idx, (_, seg_path)) in segments.into_iter().enumerate() {
        let is_last_seg = seg_idx + 1 == num_segments;
        let bytes = read_segment_bytes(&seg_path)?;
        if bytes.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let raw_lines: Vec<&str> = text.split('\n').collect();
        let line_count = if raw_lines.last() == Some(&"") {
            raw_lines.len().saturating_sub(1)
        } else {
            raw_lines.len()
        };

        for (line_idx, &raw_line) in raw_lines.iter().take(line_count).enumerate() {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let is_last_line = is_last_seg && (line_idx + 1 == line_count);
            let current_seq = expected_seq;

            let split_res = chain::split_hash_suffix(line);
            let Ok((_, stored_hash)) = split_res else {
                if is_last_line && !line.trim_end().ends_with('}') {
                    has_torn = true;
                    findings.push(FindingDetail {
                        code: "FB0804",
                        severity: "warning",
                        seq: current_seq,
                        message: format!("line {current_seq} has a torn tail"),
                    });
                } else if !chain_error_reported {
                    has_broken = true;
                    chain_error_reported = true;
                    if broken_chain_repair.is_none() {
                        broken_chain_repair =
                            Some((current_seq, last_good_seq, last_good_hash.clone()));
                    }
                    findings.push(FindingDetail {
                        code: "FB0800",
                        severity: "error",
                        seq: current_seq,
                        message: format!("line {current_seq} hash chain is broken"),
                    });
                }
                break;
            };

            let parse_res = Event::parse_line(line);
            let chain_res = chain::verify_line(&prev_hex, line);

            match (parse_res, chain_res) {
                (Ok((event, _)), Ok(_)) => {
                    if event.seq != expected_seq {
                        if !chain_error_reported {
                            has_broken = true;
                            chain_error_reported = true;
                            if broken_chain_repair.is_none() {
                                broken_chain_repair =
                                    Some((current_seq, last_good_seq, last_good_hash.clone()));
                            }
                            findings.push(FindingDetail {
                                code: "FB0800",
                                severity: "error",
                                seq: current_seq,
                                message: format!("line {current_seq} hash chain is broken"),
                            });
                        }
                    } else if broken_chain_repair.is_none() {
                        last_good_seq = event.seq;
                        last_good_hash = stored_hash.clone();
                    }

                    observe_event(
                        &event,
                        &mut prev_at,
                        &mut open_write_begin,
                        &mut open_ai_begin,
                        &mut unfinished,
                        &mut has_torn,
                        &mut findings,
                    );

                    events_count += 1;
                    head_seq = event.seq;
                    head_hash = stored_hash.as_str().to_owned();
                    seq_hashes.insert(event.seq, head_hash.clone());
                    prev_hex = head_hash.clone();
                    expected_seq = current_seq.saturating_add(1);
                }
                (Ok((event, _)), Err(_)) => {
                    if !chain_error_reported {
                        has_broken = true;
                        chain_error_reported = true;
                        if broken_chain_repair.is_none() {
                            broken_chain_repair =
                                Some((current_seq, last_good_seq, last_good_hash.clone()));
                        }
                        findings.push(FindingDetail {
                            code: "FB0800",
                            severity: "error",
                            seq: current_seq,
                            message: format!("line {current_seq} hash chain is broken"),
                        });
                    }

                    observe_event(
                        &event,
                        &mut prev_at,
                        &mut open_write_begin,
                        &mut open_ai_begin,
                        &mut unfinished,
                        &mut has_torn,
                        &mut findings,
                    );

                    events_count += 1;
                    head_seq = event.seq;
                    head_hash = stored_hash.as_str().to_owned();
                    seq_hashes.insert(event.seq, head_hash.clone());
                    prev_hex = head_hash.clone();
                    expected_seq = current_seq.saturating_add(1);
                }
                (Err(err), _) => {
                    if parse_json_object(line).is_ok() {
                        has_broken = true;
                        if broken_chain_repair.is_none() {
                            broken_chain_repair =
                                Some((current_seq, last_good_seq, last_good_hash.clone()));
                        }
                        findings.push(FindingDetail {
                            code: "FB0801",
                            severity: "error",
                            seq: current_seq,
                            message: format!(
                                "line {current_seq} invalid vocabulary or envelope: {err}"
                            ),
                        });
                    } else if !chain_error_reported {
                        has_broken = true;
                        chain_error_reported = true;
                        if broken_chain_repair.is_none() {
                            broken_chain_repair =
                                Some((current_seq, last_good_seq, last_good_hash.clone()));
                        }
                        findings.push(FindingDetail {
                            code: "FB0800",
                            severity: "error",
                            seq: current_seq,
                            message: format!("line {current_seq} hash chain is broken: {err}"),
                        });
                    }
                    events_count += 1;
                    head_seq = current_seq;
                    head_hash = stored_hash.as_str().to_owned();
                    seq_hashes.insert(current_seq, head_hash.clone());
                    prev_hex = head_hash.clone();
                    expected_seq = current_seq.saturating_add(1);
                }
            }
        }
    }

    if let Some(unclosed_seq) = open_write_begin {
        unfinished.push(unclosed_seq);
        findings.push(FindingDetail {
            code: "FB0803",
            severity: "warning",
            seq: unclosed_seq,
            message: format!("line {unclosed_seq} write.begin has no matching write.end"),
        });
    }

    for (_, ai_seq) in open_ai_begin {
        findings.push(FindingDetail {
            code: "FB0803",
            severity: "warning",
            seq: ai_seq,
            message: format!("line {ai_seq} ai.begin has no matching ai.call"),
        });
    }

    let state = if has_broken {
        fieldbook_core::strings::cite_statuses()[0]
    } else if has_torn {
        "torn"
    } else if !unfinished.is_empty() || findings.iter().any(|f| f.code == "FB0803") {
        "unfinished"
    } else {
        "ok"
    };

    Ok(VerifyInspection {
        state,
        events: events_count,
        head_seq,
        head_hash,
        findings,
        unfinished,
        has_error: has_broken,
        broken_chain_repair,
        seq_hashes,
    })
}

fn observe_event(
    event: &Event,
    prev_at: &mut Option<Now>,
    open_write_begin: &mut Option<u64>,
    open_ai_begin: &mut BTreeMap<u64, u64>,
    unfinished: &mut Vec<u64>,
    has_torn: &mut bool,
    findings: &mut Vec<FindingDetail>,
) {
    if let Some(prev) = *prev_at
        && event.time < prev
    {
        findings.push(FindingDetail {
            code: "FB0805",
            severity: "warning",
            seq: event.seq,
            message: format!("line {} timestamp goes backwards", event.seq),
        });
    }
    *prev_at = Some(event.time);

    match &event.payload {
        EventPayload::WriteBegin { .. } => {
            if let Some(prev_begin) = open_write_begin.replace(event.seq) {
                unfinished.push(prev_begin);
                findings.push(FindingDetail {
                    code: "FB0803",
                    severity: "warning",
                    seq: prev_begin,
                    message: format!("line {prev_begin} write.begin has no matching write.end"),
                });
            }
        }
        EventPayload::WriteEnd { .. } => {
            *open_write_begin = None;
        }
        EventPayload::AiBegin { tx, .. } => {
            open_ai_begin.insert(tx.value(), event.seq);
        }
        EventPayload::AiCall { tx, .. } => {
            open_ai_begin.remove(&tx.value());
        }
        EventPayload::JournalRecover { reason, .. } if reason.as_str() == "torn_tail" => {
            *has_torn = true;
            findings.push(FindingDetail {
                code: "FB0804",
                severity: "warning",
                seq: event.seq,
                message: format!("line {} recovered a torn tail", event.seq),
            });
        }
        _ => {}
    }
}

/// Format an [`AuditExport`] report as pretty JSON with a trailing newline.
#[must_use]
pub fn render_export_json(report: &AuditExport) -> String {
    let mut json = serde_json::to_string_pretty(report).unwrap_or_default();
    json.push('\n');
    json
}

/// Format an [`AuditVerifyExport`] report as pretty JSON with a trailing newline.
#[must_use]
pub fn render_verify_export_json(report: &AuditVerifyExport) -> String {
    let mut json = serde_json::to_string_pretty(report).unwrap_or_default();
    json.push('\n');
    json
}

fn run_export(args: &AuditExportArgs, state_flag: Option<&Path>, world: &dyn World) -> Outcome {
    let state_dir = resolve_state_dir(state_flag, world);
    let policy: PolicyCtx = match crate::policy::active_policy() {
        Some(ctx) => ctx,
        None => match crate::policy::resolve(world, &state_dir, world.tree_root(), false) {
            Ok(ctx) => ctx,
            Err(refusal) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{refusal}\n"),
                    code: EXIT_USAGE,
                };
            }
        },
    };
    let mut journal = match crate::io::state::open(world, &state_dir, Via::Cli) {
        Ok(journal) => journal,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };
    let layout = crate::io::audit_export::Layout::of(world);
    match crate::io::audit_export::export(
        world,
        &layout,
        &policy,
        &mut journal,
        args.to.as_deref(),
        args.since,
    ) {
        Ok(report) => {
            let stdout = if args.json {
                render_export_json(&AuditExport {
                    report_version: REPORT_VERSION,
                    verb: "audit export",
                    written: report.written,
                    events: report.events,
                    first_seq: report.first_seq,
                    last_seq: report.last_seq,
                    head: report.head.as_str().to_owned(),
                    file: report.file.clone(),
                    manifest: report.manifest.clone(),
                    exit: EXIT_OK,
                })
            } else if report.written {
                let short = &report.head.as_str()[..12.min(report.head.as_str().len())];
                format!(
                    "audit export: {} events ({} to {}), head {short}, written to {}\n",
                    report.events, report.first_seq, report.last_seq, report.file
                )
            } else {
                format!(
                    "audit export: nothing new; the journal ends at {}\n",
                    report.journal_end
                )
            };
            Outcome {
                stdout,
                stderr: String::new(),
                code: EXIT_OK,
            }
        }
        Err(refusal) => Outcome {
            stdout: String::new(),
            stderr: format!("error[{}]: {}\n", refusal.code.id(), refusal.message),
            code: EXIT_FINDINGS,
        },
    }
}

fn run_verify_export(args: &AuditVerifyArgs, manifest_path: &Path, world: &dyn World) -> Outcome {
    let manifest_name = manifest_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("manifest.toml")
        .to_owned();
    let bytes = match world.read_file(manifest_path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("error: {} cannot be read\n", manifest_path.display()),
                code: EXIT_IO,
            };
        }
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };
    let manifest_text = String::from_utf8_lossy(&bytes).into_owned();
    let parsed = Manifest::parse(&manifest_text);

    let (first_seq, last_seq, head, segment_bytes) = match &parsed {
        Ok(manifest) => {
            let folder = manifest_path.parent().unwrap_or_else(|| Path::new("."));
            let segment_path = folder.join(&manifest.file);
            match world.read_file(&segment_path) {
                Ok(Some(bytes)) => (
                    manifest.first_seq,
                    manifest.last_seq,
                    manifest.head.clone(),
                    bytes,
                ),
                Ok(None) => {
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("error: {} cannot be read\n", segment_path.display()),
                        code: EXIT_IO,
                    };
                }
                Err(err) => {
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("{err}\n"),
                        code: EXIT_IO,
                    };
                }
            }
        }
        Err(_) => (0, 0, Sha256::zero(), Vec::new()),
    };

    let findings = match parsed {
        Ok(_) => verify_export(
            &manifest_text,
            &segment_bytes,
            args.expect_head.as_deref(),
            args.expect_seq,
        ),
        Err(findings) => findings,
    };
    let exit = if findings.is_empty() {
        EXIT_OK
    } else {
        EXIT_FINDINGS
    };

    if args.json {
        let events =
            usize::try_from(last_seq.saturating_sub(first_seq).saturating_add(1)).unwrap_or(0);
        let report = AuditVerifyExport {
            report_version: REPORT_VERSION,
            verb: "audit verify",
            manifest: manifest_name,
            state: if findings.is_empty() {
                "ok"
            } else {
                fieldbook_core::strings::cite_statuses()[0]
            },
            events: if first_seq == 0 { 0 } else { events },
            first_seq,
            last_seq,
            head: head.as_str().to_owned(),
            findings: findings.iter().map(report_finding).collect(),
            exit,
        };
        return Outcome {
            stdout: render_verify_export_json(&report),
            stderr: String::new(),
            code: exit,
        };
    }

    let mut stdout = String::new();
    if findings.is_empty() {
        let events =
            usize::try_from(last_seq.saturating_sub(first_seq).saturating_add(1)).unwrap_or(0);
        let short = &head.as_str()[..12.min(head.as_str().len())];
        stdout.push_str(&format!(
            "export ok: {events} events ({first_seq} to {last_seq}), head {short}\n"
        ));
    } else {
        for finding in &findings {
            let line = finding.seq.max(1);
            stdout.push_str(&format!(
                "export:{line}:1: error[{}]: {}\n",
                finding.code.id(),
                finding.message
            ));
        }
    }

    Outcome {
        stdout,
        stderr: String::new(),
        code: exit,
    }
}

fn report_finding(finding: &Finding) -> AuditVerifyFinding {
    AuditVerifyFinding {
        code: finding.code.id(),
        severity: "error",
        seq: finding.seq,
    }
}

fn run_verify(args: &AuditVerifyArgs, state_flag: Option<&Path>, world: &dyn World) -> Outcome {
    if let Some(manifest) = &args.export {
        return run_verify_export(args, manifest, world);
    }
    if args.anchor_seq.is_some() != args.anchor_head.is_some() {
        return Outcome {
            stdout: String::new(),
            stderr: "error: --anchor-seq and --anchor-head must be provided together\n".to_owned(),
            code: EXIT_USAGE,
        };
    }
    if let Some(anchor_seq) = args.anchor_seq {
        if anchor_seq < 1 {
            return Outcome {
                stdout: String::new(),
                stderr: "error: --anchor-seq must be at least 1\n".to_owned(),
                code: EXIT_USAGE,
            };
        }
        if args.expect_seq.is_some() || args.expect_head.is_some() || args.repair {
            return Outcome {
                stdout: String::new(),
                stderr: "error: --anchor-seq/--anchor-head cannot be combined with --expect-seq, --expect-head, or --repair\n".to_owned(),
                code: EXIT_USAGE,
            };
        }
    }
    if let Some(anchor_head) = &args.anchor_head
        && Sha256::parse(anchor_head).is_err()
    {
        return Outcome {
            stdout: String::new(),
            stderr: "error: --anchor-head must be 64 lowercase hexadecimal characters\n".to_owned(),
            code: EXIT_USAGE,
        };
    }
    if let Some(expect_head) = &args.expect_head
        && Sha256::parse(expect_head).is_err()
    {
        return Outcome {
            stdout: String::new(),
            stderr: "error: --expect-head must be 64 lowercase hexadecimal characters\n".to_owned(),
            code: EXIT_USAGE,
        };
    }

    let state_dir = resolve_state_dir(state_flag, world);
    let mut inspection = match inspect_journal(&state_dir) {
        Ok(ins) => ins,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };

    if args.repair
        && let Some((first_bad_seq, last_good_seq, last_good_hash)) =
            inspection.broken_chain_repair.clone()
    {
        let lease = match crate::io::journal::acquire_write_lease(world.tree_root(), &state_dir) {
            Ok(l) => l,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        let reread = match inspect_journal(&state_dir) {
            Ok(ins) => ins,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        let (bad_seq, good_seq, good_hash) = reread.broken_chain_repair.clone().unwrap_or((
            first_bad_seq,
            last_good_seq,
            last_good_hash,
        ));

        let broken_dir = match Journal::open_for_repair(
            world,
            &state_dir,
            Via::Cli,
            &lease,
            bad_seq,
            good_seq,
            good_hash,
        ) {
            Ok(dir) => dir,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        let folder_name = broken_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("journal.broken");

        let after_repair = inspect_journal(&state_dir).unwrap_or(reread);
        let exit = EXIT_FINDINGS;
        if args.json {
            let report = AuditVerifyReport {
                report_version: REPORT_VERSION,
                verb: "audit verify",
                state: "torn",
                events: after_repair.events,
                head: AuditHead {
                    seq: after_repair.head_seq,
                    hash: after_repair.head_hash,
                },
                findings: vec![AuditVerifyFinding {
                    code: "FB0804",
                    severity: "warning",
                    seq: bad_seq,
                }],
                unfinished: Vec::new(),
                exit,
            };
            return Outcome {
                stdout: render_verify_json(&report),
                stderr: String::new(),
                code: exit,
            };
        }
        return Outcome {
            stdout: format!(
                "audit/journal.jsonl:{bad_seq}:1: warning[FB0804]: broken journal moved aside to {folder_name}; started new chain\n"
            ),
            stderr: String::new(),
            code: exit,
        };
    }

    let mut mismatch_head_or_seq = false;
    if let Some(exp_seq) = args.expect_seq
        && inspection.head_seq != exp_seq
    {
        mismatch_head_or_seq = true;
    }
    if let Some(exp_head) = &args.expect_head
        && &inspection.head_hash != exp_head
    {
        mismatch_head_or_seq = true;
    }
    if let (Some(anchor_seq), Some(anchor_head)) = (args.anchor_seq, &args.anchor_head) {
        match inspection.seq_hashes.get(&anchor_seq) {
            Some(actual_hash) if actual_hash == anchor_head => {}
            _ => {
                mismatch_head_or_seq = true;
            }
        }
    }

    if mismatch_head_or_seq {
        let report_seq = args
            .anchor_seq
            .map(|s| s.min(inspection.head_seq))
            .unwrap_or(inspection.head_seq);
        inspection.state = fieldbook_core::strings::cite_statuses()[0];
        inspection.has_error = true;
        inspection.findings.push(FindingDetail {
            code: "FB0802",
            severity: "error",
            seq: report_seq,
            message: format!(
                "journal head (seq {}, hash {}) does not match expected head or length",
                inspection.head_seq, inspection.head_hash
            ),
        });
    }

    let exit = if inspection.has_error {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    };

    if args.json {
        let report = AuditVerifyReport {
            report_version: REPORT_VERSION,
            verb: "audit verify",
            state: inspection.state,
            events: inspection.events,
            head: AuditHead {
                seq: inspection.head_seq,
                hash: inspection.head_hash,
            },
            findings: inspection
                .findings
                .iter()
                .map(|f| AuditVerifyFinding {
                    code: f.code,
                    severity: f.severity,
                    seq: f.seq,
                })
                .collect(),
            unfinished: inspection.unfinished,
            exit,
        };
        return Outcome {
            stdout: render_verify_json(&report),
            stderr: String::new(),
            code: exit,
        };
    }

    let mut stdout = String::new();
    if inspection.findings.is_empty() {
        let short_head = &inspection.head_hash[..12.min(inspection.head_hash.len())];
        stdout.push_str(&format!(
            "journal ok: {} events, head {short_head}\n",
            inspection.events
        ));
    } else {
        for f in &inspection.findings {
            let line_no = f.seq.max(1);
            stdout.push_str(&format!(
                "audit/journal.jsonl:{line_no}:1: {}[{}]: {}\n",
                f.severity, f.code, f.message
            ));
        }
    }

    Outcome {
        stdout,
        stderr: String::new(),
        code: exit,
    }
}

fn validate_id_filter(input: &str) -> Result<String, String> {
    if input.contains('/') {
        RecordRef::parse(input)
            .map(|r| r.to_string())
            .map_err(|e| e.to_string())
    } else {
        Id::parse(input)
            .map(|id| id.as_str().to_owned())
            .map_err(|e| e.to_string())
    }
}

fn run_show(args: &AuditShowArgs, state_flag: Option<&Path>, world: &dyn World) -> Outcome {
    let kind_filter = match &args.kind {
        Some(k) => match Kind::parse(k) {
            Ok(parsed) => Some(parsed),
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("error: {err}\n"),
                    code: EXIT_USAGE,
                };
            }
        },
        None => None,
    };

    let id_filter = match &args.id {
        Some(id_str) => match validate_id_filter(id_str) {
            Ok(parsed) => Some(parsed),
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("error: {err}\n"),
                    code: EXIT_USAGE,
                };
            }
        },
        None => None,
    };

    let label_filter = match &args.label {
        Some(label_str) => match LabelId::parse(label_str) {
            Ok(parsed) => Some(parsed),
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("error: {err}\n"),
                    code: EXIT_USAGE,
                };
            }
        },
        None => None,
    };

    let state_dir = resolve_state_dir(state_flag, world);
    let audit_dir = state_dir.join("audit");
    let segments = match list_segments(&audit_dir) {
        Ok(segs) => segs,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };

    let mut total_count = 0usize;
    let mut head_seq = 0u64;
    let mut head_hash = ZERO_HASH.to_owned();
    let mut matching_rows: Vec<(String, RawEventObject)> = Vec::new();

    for (_, seg_path) in segments {
        let bytes = match read_segment_bytes(&seg_path) {
            Ok(b) => b,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        let content = String::from_utf8_lossy(&bytes);

        for raw_line in content.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let Ok((_, stored_hash)) = chain::split_hash_suffix(line) else {
                continue;
            };
            let Ok(raw_obj) = parse_json_object(line) else {
                continue;
            };
            let parsed_event = Event::parse_line(line).ok().map(|(ev, _)| ev);

            total_count += 1;
            if let Some(ev) = &parsed_event {
                head_seq = ev.seq;
            } else {
                head_seq = total_count as u64;
            }
            head_hash = stored_hash.as_str().to_owned();

            if let Some(kf) = kind_filter {
                let matches = parsed_event.as_ref().is_some_and(|ev| ev.kind() == kf);
                if !matches {
                    continue;
                }
            }
            if let Some(idf) = &id_filter {
                let matches = parsed_event.as_ref().is_some_and(|ev| ev.touches_id(idf));
                if !matches {
                    continue;
                }
            }
            if let Some(lf) = &label_filter {
                let matches = parsed_event.as_ref().is_some_and(|ev| ev.touches_label(lf));
                if !matches {
                    continue;
                }
            }

            matching_rows.push((line.to_owned(), raw_obj));
        }
    }

    let selected: Vec<(String, RawEventObject)> = if args.head {
        Vec::new()
    } else {
        let limit = args.last;
        let skip = matching_rows.len().saturating_sub(limit);
        matching_rows.into_iter().skip(skip).collect()
    };

    if args.json {
        let events: Vec<RawEventObject> = selected.into_iter().map(|(_, obj)| obj).collect();
        let report = AuditShowReport {
            report_version: REPORT_VERSION,
            verb: "audit show",
            head: AuditHead {
                seq: head_seq,
                hash: head_hash,
            },
            count: total_count,
            shown: events.len(),
            events,
            exit: EXIT_OK,
        };
        return Outcome {
            stdout: render_show_json(&report),
            stderr: String::new(),
            code: EXIT_OK,
        };
    }

    let mut stdout = String::new();
    if args.head {
        stdout.push_str(&format!("{head_seq} {head_hash}\n"));
    } else {
        for (line, _) in selected {
            stdout.push_str(&line);
            stdout.push('\n');
        }
    }

    Outcome {
        stdout,
        stderr: String::new(),
        code: EXIT_OK,
    }
}
