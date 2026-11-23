//! Implementation of `fieldbook ai status [--json]` (`ai-config-and-keys` steps 6 & 11).
//!
//! Deterministic, read-only status reporting over `settings.toml`, the active policy,
//! the adapter registry, environment key state, and the audit journal. Opens no file for
//! writing and makes no network connection.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use fieldbook_core::audit::{Event, EventPayload};
use fieldbook_core::egress::{self, Decision, Dest, Shipped};
use fieldbook_core::labels::{LabelAllow, LabelId};
use fieldbook_core::policy::{ClosedWhy, Cut, CutBy, EffectivePolicy, PolicyState};
use fieldbook_core::ports::{AdapterStatus, ConnState, KeyState, LastCall, Port};
use fieldbook_core::report::{AiStatus, REPORT_VERSION};

use crate::adapters::registry::{ADAPTERS, Needs, Row, State};
use crate::cli::{AiArgs, AiStatusArgs, AiSubcommand, EXIT_IO, EXIT_OK, EXIT_USAGE};
use crate::io::ai_settings::read_settings_text;
use crate::io::journal::{list_segments, read_segment_bytes};
use crate::io::keys::{EnvKeys, KeyError, KeySource};
use crate::policy::PolicyCtx;
use crate::settings::{EndpointSettings, Settings};
use crate::{Outcome, World};

/// Dispatch `fieldbook ai` subcommands.
#[must_use]
pub fn run(args: &AiArgs, ctx: &PolicyCtx, state_dir: &Path, world: &dyn World) -> Outcome {
    match &args.command {
        AiSubcommand::Status(status_args) => run_status(
            status_args,
            ctx,
            state_dir,
            world,
            ADAPTERS,
            &EnvKeys,
            false,
        ),
    }
}

struct AdapterLineDetail {
    status: AdapterStatus,
    key_display: String,
    policy_allow: Option<LabelAllow>,
    owner_allow: Option<LabelAllow>,
    cuts: Vec<Cut>,
}

/// Runs `fieldbook ai status [--json]` with caller-supplied registry `rows`, [`KeySource`], and
/// network-feature availability flag (`false` in the silent build).
#[must_use]
pub fn run_status(
    args: &AiStatusArgs,
    ctx: &PolicyCtx,
    state_dir: &Path,
    world: &dyn World,
    rows: &[Row],
    keys: &dyn KeySource,
    has_net: bool,
) -> Outcome {
    let raw_text = match read_settings_text(state_dir) {
        Ok(opt) => opt,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };

    let (settings, has_settings_file) = match raw_text {
        None => (Settings::default(), false),
        Some(text) => match Settings::parse(&text, rows) {
            Ok(parsed) => (parsed, true),
            Err(errors) => {
                let mut stderr = String::new();
                for err in errors {
                    let _ = writeln!(stderr, "{err}");
                }
                return Outcome {
                    stdout: String::new(),
                    stderr,
                    code: EXIT_USAGE,
                };
            }
        },
    };

    let eff = EffectivePolicy::new(&ctx.state, &settings.owner());
    let now = world.now();
    let last_calls = read_last_calls(state_dir);

    let mut details: Vec<AdapterLineDetail> = Vec::new();
    let mut emitted_model_rows: Vec<&'static str> = Vec::new();

    // 1. Model port: handoff@1 (only when present in `rows`).
    if let Some(handoff_row) = rows
        .iter()
        .find(|r| r.port == Port::Model && r.id == "handoff@1")
    {
        emitted_model_rows.push(handoff_row.id);
        let detail = build_handoff_status(
            handoff_row,
            &ctx.state,
            &eff,
            settings.model.handoff.allow.as_ref(),
            now,
            last_calls.get("handoff").cloned(),
        );
        details.push(detail);
    }

    // 2. Model port: configured endpoints `[model.endpoint.<name>]`.
    for (ep_name, ep_cfg) in &settings.model.endpoints {
        let row = rows
            .iter()
            .find(|r| r.port == Port::Model && r.id == ep_cfg.adapter);
        if let Some(row) = row {
            emitted_model_rows.push(row.id);
            let detail = build_endpoint_status(
                ep_name.as_str(),
                ep_cfg,
                row,
                &ctx.state,
                &eff,
                now,
                keys,
                has_net,
                last_calls.get(ep_name.as_str()).cloned(),
            );
            details.push(detail);
        }
    }

    // 3. Model port: any `State::Later` Model rows not already emitted.
    for row in rows {
        if row.port != Port::Model || row.state != State::Later {
            continue;
        }
        if emitted_model_rows.contains(&row.id) {
            continue;
        }
        let state = if ctx.state.is_closed() {
            ConnState::Blocked {
                code: "FB0413".to_owned(),
            }
        } else {
            ConnState::Off
        };
        details.push(AdapterLineDetail {
            status: AdapterStatus {
                port: Port::Model.as_str().to_owned(),
                adapter: row.id.to_owned(),
                endpoint: None,
                state,
                allow: Vec::new(),
                key: KeyState::NotNeeded,
                caps: row.caps(),
                last: None,
            },
            key_display: "not-needed".to_owned(),
            policy_allow: None,
            owner_allow: None,
            cuts: Vec::new(),
        });
    }

    // 4. Other ports: default adapter per port (in canonical Port::ALL order).
    for port in Port::ALL {
        if port == Port::Model {
            continue;
        }
        let Some(row) = rows
            .iter()
            .find(|r| r.port == port && r.default)
            .or_else(|| rows.iter().find(|r| r.port == port))
        else {
            continue;
        };
        let state = if ctx.state.is_closed() {
            ConnState::Blocked {
                code: "FB0413".to_owned(),
            }
        } else if row.state == State::Later {
            ConnState::Off
        } else {
            ConnState::Ready
        };
        details.push(AdapterLineDetail {
            status: AdapterStatus {
                port: port.as_str().to_owned(),
                adapter: row.id.to_owned(),
                endpoint: None,
                state,
                allow: Vec::new(),
                key: KeyState::NotNeeded,
                caps: row.caps(),
                last: None,
            },
            key_display: "not-needed".to_owned(),
            policy_allow: None,
            owner_allow: None,
            cuts: Vec::new(),
        });
    }

    if args.json {
        let policy_str = match &ctx.state {
            PolicyState::Floor(_) => "floor",
            PolicyState::Loaded(_) => "loaded",
            PolicyState::Closed(_) => "closed",
        };
        let report = AiStatus {
            report_version: REPORT_VERSION,
            policy: policy_str.to_owned(),
            adapters: details.into_iter().map(|d| d.status).collect(),
            exit: EXIT_OK,
        };
        let mut json = serde_json::to_string_pretty(&report).unwrap_or_default();
        json.push('\n');
        return Outcome {
            stdout: json,
            stderr: String::new(),
            code: EXIT_OK,
        };
    }

    let mut stdout = String::new();
    if has_settings_file {
        let _ = writeln!(stdout, "settings: ok");
    } else {
        let _ = writeln!(stdout, "settings: default (no settings.toml)");
    }

    match &ctx.state {
        PolicyState::Floor(_) => {
            let _ = writeln!(stdout, "policy: none (floor)");
        }
        PolicyState::Loaded(p) => {
            let _ = writeln!(stdout, "policy: revision {}", p.revision());
        }
        PolicyState::Closed(why) => {
            let reason = match why {
                ClosedWhy::Absent => "absent",
                ClosedWhy::Invalid => "invalid",
                ClosedWhy::LowerRevision => "lower revision",
                ClosedWhy::Writable => "writable",
            };
            let _ = writeln!(stdout, "policy: closed ({reason})");
        }
    }

    let _ = writeln!(stdout, "order: {}", settings.model.order.join(", "));

    let mut limits_line = format!(
        "Limits: retries {}, max_calls_per_run {}, max_out_tokens {}",
        settings.model.retries, settings.model.max_calls_per_run, settings.model.max_out_tokens
    );
    for (ep_name, ep_cfg) in &settings.model.endpoints {
        let _ = write!(
            limits_line,
            ", timeout_s {} {}",
            ep_name.as_str(),
            ep_cfg.timeout_s
        );
    }
    let _ = writeln!(stdout, "{limits_line}");

    let mut warnings = collect_undefined_label_warnings(&settings, &eff);

    for detail in &details {
        let is_model = detail.status.port == Port::Model.as_str();
        let state_str = format_conn_state(&detail.status.state);
        let allow_str = if is_model {
            format_label_slice(&detail.status.allow)
        } else {
            "-".to_owned()
        };
        let last_str = detail.status.last.as_ref().map_or_else(
            || "none".to_owned(),
            |lc| format!("{} {} ({})", lc.outcome, lc.at, lc.attempts),
        );
        if let Some(ep) = &detail.status.endpoint {
            let _ = writeln!(
                stdout,
                "{} {} ({}) {} {} {} {}",
                detail.status.port,
                detail.status.adapter,
                ep,
                state_str,
                allow_str,
                detail.key_display,
                last_str
            );
        } else {
            let _ = writeln!(
                stdout,
                "{} {} {} {} {} {}",
                detail.status.port,
                detail.status.adapter,
                state_str,
                allow_str,
                detail.key_display,
                last_str
            );
        }

        if is_model && !ctx.state.is_closed() {
            if let Some(p_allow) = &detail.policy_allow {
                let owner_str = detail
                    .owner_allow
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |a| format_label_slice(a.ids()));
                let _ = writeln!(
                    stdout,
                    "  allow: policy {} owner {}",
                    format_label_slice(p_allow.ids()),
                    owner_str
                );
            }
            for cut in &detail.cuts {
                let side = match cut.by {
                    CutBy::Policy => "policy",
                    CutBy::Owner => "owner",
                };
                let _ = writeln!(stdout, "  cut {}: by {side}", cut.label.as_str());
            }
        }
    }

    for warn in warnings.drain(..) {
        let _ = writeln!(stdout, "{warn}");
    }

    Outcome {
        stdout,
        stderr: String::new(),
        code: EXIT_OK,
    }
}

fn build_handoff_status(
    row: &Row,
    policy_state: &PolicyState,
    eff: &EffectivePolicy,
    owner_allow: Option<&LabelAllow>,
    now: fieldbook_core::audit::Now,
    last: Option<LastCall>,
) -> AdapterLineDetail {
    let dest = Dest::Handoff;
    let policy_allow = policy_state.policy().map(|p| {
        p.destinations()
            .iter()
            .find(|d| d.kind == fieldbook_core::audit::DestKind::Handoff)
            .map_or_else(LabelAllow::empty, |r| r.allow.clone())
    });

    let (state, allow, cuts) = if policy_state.is_closed() {
        (
            ConnState::Blocked {
                code: "FB0413".to_owned(),
            },
            Vec::new(),
            Vec::new(),
        )
    } else if row.state == State::Later {
        (ConnState::Off, Vec::new(), Vec::new())
    } else {
        match egress::check(eff, &dest, None, None, now, Shipped::Never) {
            Decision::Denied(denial) => (
                ConnState::Blocked {
                    code: denial.code.id().to_owned(),
                },
                Vec::new(),
                eff.explain(&dest),
            ),
            Decision::Allowed(allowed) => {
                let effective = allowed.permitted().ids().to_vec();
                let st = if effective.is_empty() {
                    ConnState::Blocked {
                        code: "FB0901".to_owned(),
                    }
                } else {
                    ConnState::Handoff
                };
                (st, effective, eff.explain(&dest))
            }
        }
    };

    AdapterLineDetail {
        status: AdapterStatus {
            port: Port::Model.as_str().to_owned(),
            adapter: row.id.to_owned(),
            endpoint: Some("handoff".to_owned()),
            state,
            allow,
            key: KeyState::NotNeeded,
            caps: row.caps(),
            last,
        },
        key_display: "not-needed".to_owned(),
        policy_allow,
        owner_allow: owner_allow.cloned(),
        cuts,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "status assembly takes explicit pure inputs without global state"
)]
fn build_endpoint_status(
    ep_name: &str,
    ep_cfg: &EndpointSettings,
    row: &Row,
    policy_state: &PolicyState,
    eff: &EffectivePolicy,
    now: fieldbook_core::audit::Now,
    keys: &dyn KeySource,
    has_net: bool,
    last: Option<LastCall>,
) -> AdapterLineDetail {
    let dest = Dest::Model {
        name: ep_name.to_owned(),
        base_url: ep_cfg.base_url.clone(),
    };

    let policy_allow = policy_state.policy().map(|p| {
        let matching = match policy_state {
            PolicyState::Floor(_) => p
                .destinations()
                .iter()
                .find(|d| d.kind == fieldbook_core::audit::DestKind::Model),
            PolicyState::Loaded(_) => p.destinations().iter().find(|d| {
                d.kind == fieldbook_core::audit::DestKind::Model
                    && d.name.as_deref() == Some(ep_name)
            }),
            PolicyState::Closed(_) => None,
        };
        matching.map_or_else(LabelAllow::empty, |r| r.allow.clone())
    });

    let (key_state, key_display) = match &ep_cfg.key_env {
        Some(var_name) => match keys.key(var_name) {
            Ok(key) => (
                KeyState::Set,
                format!("{var_name}: set ({} characters)", key.len()),
            ),
            Err(KeyError::Unset | KeyError::Empty) => {
                (KeyState::Missing, format!("{var_name}: missing"))
            }
        },
        None => (KeyState::NotNeeded, "not-needed".to_owned()),
    };

    let (state, allow, cuts) = if policy_state.is_closed() {
        (
            ConnState::Blocked {
                code: "FB0413".to_owned(),
            },
            Vec::new(),
            Vec::new(),
        )
    } else if row.state == State::Later {
        (ConnState::Off, Vec::new(), Vec::new())
    } else {
        match egress::check(eff, &dest, None, None, now, Shipped::Never) {
            Decision::Denied(denial) => (
                ConnState::Blocked {
                    code: denial.code.id().to_owned(),
                },
                Vec::new(),
                eff.explain(&dest),
            ),
            Decision::Allowed(allowed) => {
                // An endpoint with no `allow` key accepts nothing even on Floor A.
                let effective = if ep_cfg.allow.is_none() {
                    Vec::new()
                } else {
                    allowed.permitted().ids().to_vec()
                };
                let cuts =
                    if ep_cfg.allow.is_none() && matches!(policy_state, PolicyState::Floor(_)) {
                        let p_allow = policy_allow.clone().unwrap_or_else(LabelAllow::empty);
                        let mut c = Vec::new();
                        for id in [
                            fieldbook_core::labels::OWNER_SUPPLIED,
                            fieldbook_core::labels::PUBLIC,
                        ] {
                            let by = if p_allow.contains(&id) {
                                CutBy::Owner
                            } else {
                                CutBy::Policy
                            };
                            c.push(Cut { label: id, by });
                        }
                        c
                    } else {
                        eff.explain(&dest)
                    };

                let st = if effective.is_empty() {
                    ConnState::Blocked {
                        code: "FB0901".to_owned(),
                    }
                } else if row.needs == Needs::Net && !has_net {
                    ConnState::Unavailable {
                        code: "FB0912".to_owned(),
                    }
                } else {
                    ConnState::Ready
                };
                (st, effective, cuts)
            }
        }
    };

    AdapterLineDetail {
        status: AdapterStatus {
            port: Port::Model.as_str().to_owned(),
            adapter: row.id.to_owned(),
            endpoint: Some(ep_name.to_owned()),
            state,
            allow,
            key: key_state,
            caps: row.caps(),
            last,
        },
        key_display,
        policy_allow,
        owner_allow: ep_cfg.allow.clone(),
        cuts,
    }
}

fn collect_undefined_label_warnings(settings: &Settings, eff: &EffectivePolicy) -> Vec<String> {
    let table = eff.label_table();
    let mut out = Vec::new();

    if let Some(allow) = &settings.model.handoff.allow {
        for id in allow.ids() {
            if !table.is_known(id) {
                out.push(format!(
                    "warning[FB0411]: label `{}` in `[model.handoff] allow` is not defined by the active policy",
                    id.as_str()
                ));
            }
        }
    }

    for (ep_name, ep_cfg) in &settings.model.endpoints {
        if let Some(allow) = &ep_cfg.allow {
            for id in allow.ids() {
                if !table.is_known(id) {
                    out.push(format!(
                        "warning[FB0411]: label `{}` in `[model.endpoint.{}] allow` is not defined by the active policy",
                        id.as_str(),
                        ep_name.as_str()
                    ));
                }
            }
        }
    }

    if let Some(allow) = &settings.mcp.allow {
        for id in allow.ids() {
            if !table.is_known(id) {
                out.push(format!(
                    "warning[FB0411]: label `{}` in `[mcp] allow` is not defined by the active policy",
                    id.as_str()
                ));
            }
        }
    }

    out
}

fn format_conn_state(state: &ConnState) -> String {
    match state {
        ConnState::Off => "Off".to_owned(),
        ConnState::Handoff => "Handoff".to_owned(),
        ConnState::Ready => "Ready".to_owned(),
        ConnState::Busy => "Busy".to_owned(),
        ConnState::Unavailable { code } => format!("Unavailable({code})"),
        ConnState::Blocked { code } => format!("Blocked({code})"),
    }
}

fn format_label_slice(labels: &[LabelId]) -> String {
    if labels.is_empty() {
        "[]".to_owned()
    } else {
        let joined = labels
            .iter()
            .map(LabelId::as_str)
            .collect::<Vec<_>>()
            .join(",");
        format!("[{joined}]")
    }
}

/// Reads all audit journal segments under `<state_dir>/audit` (failing open as `audit show` does)
/// and joins each completed `ai.call` by `tx` to its `ai.begin` endpoint, taking the greatest
/// completion sequence (`event.seq`) per endpoint (`ai-config-and-keys` step 11).
#[must_use]
pub fn read_last_calls(state_dir: &Path) -> BTreeMap<String, LastCall> {
    let audit_dir = state_dir.join("audit");
    let Ok(segments) = list_segments(&audit_dir) else {
        return BTreeMap::new();
    };

    let mut events = Vec::new();
    for (_, seg_path) in segments {
        let Ok(bytes) = read_segment_bytes(&seg_path) else {
            continue;
        };
        let content = String::from_utf8_lossy(&bytes);
        for raw_line in content.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            if let Ok((event, _)) = Event::parse_line(line) {
                events.push(event);
            }
        }
    }

    let mut begins: BTreeMap<u64, String> = BTreeMap::new();
    for ev in &events {
        if let EventPayload::AiBegin { tx, endpoint, .. } = &ev.payload {
            begins.insert(tx.value(), endpoint.as_str().to_owned());
        }
    }

    let mut best: BTreeMap<String, (u64, LastCall)> = BTreeMap::new();
    for ev in &events {
        if let EventPayload::AiCall {
            tx,
            outcome,
            attempts,
            ..
        } = &ev.payload
            && let Some(endpoint) = begins.get(&tx.value())
        {
            let replace = best
                .get(endpoint)
                .is_none_or(|(prev_seq, _)| ev.seq > *prev_seq);
            if replace {
                best.insert(
                    endpoint.clone(),
                    (
                        ev.seq,
                        LastCall {
                            outcome: outcome.as_str().to_owned(),
                            at: ev.time.to_string(),
                            attempts: attempts.value(),
                        },
                    ),
                );
            }
        }
    }

    best.into_iter().map(|(ep, (_, lc))| (ep, lc)).collect()
}
