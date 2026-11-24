//! Pure policy display formatter (`show`, `PolicyShowText`, `policy_line`)
//! (`policy-load`, `ADR 0004 §3`, `docs/reference/policy.md#commands`).

use std::fmt::{self, Write as _};

use crate::audit::DestKind;
use crate::date::CivilDate;
use crate::egress::{DEST_KINDS, Dest, RowSource};
use crate::labels::StoreId;
use crate::report::{
    PolicyShow, PolicyShowCut, PolicyShowDestination, PolicyShowEffective, PolicyShowLabel,
    PolicyShowRecorded, PolicyShowStore, REPORT_VERSION,
};

use super::merge::{CutBy, EffectivePolicy};
use super::parse::{Policy, PolicyDestination};
use super::place::Source;
use super::seen::{Resolved, Seen};
use super::state::PolicyState;

/// Formatted text and structured JSON report for `fieldbook policy show`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyShowText {
    /// Human-readable terminal text output.
    pub text: String,
    /// Structured `--json` report payload.
    pub report: PolicyShow,
}

impl PolicyShowText {
    /// Returns the formatted terminal output string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for PolicyShowText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Formats the first line that every AI verb prints:
/// `policy: <floor|loaded|closed> <first eight hex digits of the hash>`.
#[must_use]
pub fn policy_line_from(state: &PolicyState, hash: &str) -> String {
    let state_word = match state {
        PolicyState::Floor(_) => "floor",
        PolicyState::Loaded(_) => "loaded",
        PolicyState::Closed(_) => "closed",
    };
    let prefix = &hash[..hash.len().min(8)];
    format!("policy: {state_word} {prefix}")
}

/// Formats `policy: <floor|loaded|closed> <first eight hex digits of the hash>` from a [`Resolved`].
#[must_use]
pub fn policy_line(resolved: &Resolved) -> String {
    let hash = resolved.observation_hash();
    policy_line_from(&resolved.state, hash.as_str())
}

/// Builds the [`PolicyShowText`] (both terminal text and [`PolicyShow`] JSON report) for the
/// current [`Resolved`] policy, [`EffectivePolicy`], optional recorded [`Seen`], [`Source`], and
/// `today` date.
#[must_use]
pub fn show(
    resolved: &Resolved,
    effective: &EffectivePolicy,
    seen: Option<&Seen>,
    source: &Source,
) -> PolicyShowText {
    show_at(
        resolved,
        effective,
        seen,
        source,
        CivilDate::from_ymd(2026, 3, 1).unwrap_or(CivilDate::MIN),
    )
}

/// Like [`show`], evaluating destination `expires` dates against `today`.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "renders the complete policy show text and JSON report in one deterministic pass"
)]
pub fn show_at(
    resolved: &Resolved,
    effective: &EffectivePolicy,
    seen: Option<&Seen>,
    source: &Source,
    today: CivilDate,
) -> PolicyShowText {
    let mut out = String::new();

    let (state_word, exit_code) = match &resolved.state {
        PolicyState::Floor(_) => {
            let _ = writeln!(out, "policy: none (floor)");
            ("floor", 0u8)
        }
        PolicyState::Loaded(_) => {
            let _ = writeln!(out, "policy: loaded");
            ("loaded", 0u8)
        }
        PolicyState::Closed(_) => {
            let code = resolved.closed_code().map_or("FB0413", |c| c.id());
            let why = resolved
                .closed_why_text()
                .unwrap_or_else(|| "invalid policy file".to_owned());
            let _ = writeln!(out, "policy: closed ({code}): {why}");
            ("closed", 1u8)
        }
    };

    let path_opt = source.path().map(|p| p.to_string_lossy().into_owned());
    match &path_opt {
        Some(p) => {
            let _ = writeln!(out, "path: {p}");
        }
        None => {
            let _ = writeln!(out, "path: none");
        }
    }

    let hash_str = match &resolved.state {
        PolicyState::Floor(_) | PolicyState::Loaded(_) => effective.hash().to_owned(),
        PolicyState::Closed(_) => resolved.observation_hash().as_str().to_owned(),
    };
    let _ = writeln!(out, "hash: {hash_str}");

    let writer_check_str = match &resolved.observation.writers {
        Ok(()) => "ok".to_owned(),
        Err(why) => why.as_str().to_owned(),
    };
    let _ = writeln!(out, "writer check: {writer_check_str}");

    let revision = match &resolved.state {
        PolicyState::Loaded(p) => {
            let _ = writeln!(out, "revision: {}", p.revision());
            p.revision()
        }
        PolicyState::Floor(_) | PolicyState::Closed(_) => {
            let _ = writeln!(out, "revision: none");
            0
        }
    };

    let recorded_report = match seen {
        Some(s) => {
            let h = s.hash.as_str();
            let short = &h[..h.len().min(12)];
            let _ = writeln!(out, "recorded: revision {} ({short})", s.revision);
            Some(PolicyShowRecorded {
                revision: s.revision,
                hash: short.to_owned(),
            })
        }
        None => {
            let _ = writeln!(out, "recorded: none");
            None
        }
    };

    let active_policy: Option<Policy> = resolved.state.policy();

    // Labels section
    let mut report_labels = Vec::new();
    if let Some(policy) = &active_policy {
        if policy.labels().is_empty() {
            let _ = writeln!(
                out,
                "labels: builtin (public, owner-supplied, unlabelled, marked)"
            );
        } else {
            for lbl in policy.labels() {
                match &lbl.banner {
                    Some(banner) => {
                        let _ = writeln!(
                            out,
                            "label {}: title={:?} banner={:?} restricted={}",
                            lbl.id.as_str(),
                            lbl.title,
                            banner,
                            lbl.restricted
                        );
                    }
                    None => {
                        let _ = writeln!(
                            out,
                            "label {}: title={:?} restricted={}",
                            lbl.id.as_str(),
                            lbl.title,
                            lbl.restricted
                        );
                    }
                }
                report_labels.push(PolicyShowLabel {
                    id: lbl.id.as_str().to_owned(),
                    title: lbl.title.clone(),
                    banner: lbl.banner.clone(),
                    restricted: lbl.restricted,
                });
            }
        }
    } else {
        let _ = writeln!(out, "labels: none (closed)");
    }

    // Stores section
    let mut report_stores = Vec::new();
    if let Some(policy) = &active_policy {
        for store_id in StoreId::ALL {
            let rule = policy.store_rule(&store_id);
            let holds: Vec<String> = rule
                .holds
                .ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect();
            let default: Vec<String> = rule
                .default
                .ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect();
            let readers_disp = rule.readers.as_deref().unwrap_or("none");
            let changers_disp = rule.changers.as_deref().unwrap_or("none");
            let _ = writeln!(
                out,
                "store {}: holds=[{}] default=[{}] readers={} changers={} hold={}",
                store_id.as_str(),
                holds.join(", "),
                default.join(", "),
                readers_disp,
                changers_disp,
                rule.hold
            );
            report_stores.push(PolicyShowStore {
                id: store_id.as_str().to_owned(),
                holds,
                default,
                readers: rule.readers.clone(),
                changers: rule.changers.clone(),
                hold: rule.hold,
            });
        }
    }

    // Rules section: [[marking]] and [[source]]
    if let Some(policy) = &active_policy {
        for mk in policy.marking_rules().rows() {
            let labels: Vec<&str> = mk.labels.ids().iter().map(|id| id.as_str()).collect();
            let _ = writeln!(
                out,
                "marking: match={:?} labels=[{}]",
                mk.matched,
                labels.join(", ")
            );
        }
        for src in &policy.sources {
            let def: Vec<&str> = src.default.ids().iter().map(|id| id.as_str()).collect();
            let _ = writeln!(
                out,
                "source {}: root={:?} default=[{}]",
                src.id,
                src.root,
                def.join(", ")
            );
            for r in &src.rules {
                let lbls: Vec<&str> = r.labels.ids().iter().map(|id| id.as_str()).collect();
                let _ = writeln!(
                    out,
                    "  source.rule: prefix={:?} labels=[{}]",
                    r.prefix,
                    lbls.join(", ")
                );
            }
            for m in &src.maps {
                let lbls: Vec<&str> = m.labels.ids().iter().map(|id| id.as_str()).collect();
                let _ = writeln!(
                    out,
                    "  source.map: native={:?} labels=[{}]",
                    m.native,
                    lbls.join(", ")
                );
            }
        }
    }

    // Destinations section
    let mut report_destinations = Vec::new();
    let mut report_effective = Vec::new();

    if let Some(policy) = &active_policy {
        for row in policy.destinations() {
            let is_advisory = DEST_KINDS
                .iter()
                .find(|s| s.kind == row.kind)
                .is_some_and(|s| s.advisory);
            let allow_vec: Vec<String> = row
                .allow
                .ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect();
            let mut line = format!("destination {}", row.kind.as_str());
            if is_advisory {
                line.push_str(" (advisory)");
            }
            if let Some(name) = &row.name {
                let _ = write!(line, " name={name}");
            }
            if let Some(url) = &row.url {
                let _ = write!(line, " url={url}");
            }
            let _ = write!(line, ": allow=[{}]", allow_vec.join(", "));
            let expired_opt = if let Some(exp) = row.expires {
                let is_expired = today > exp;
                let status = if is_expired { "expired" } else { "active" };
                let _ = write!(line, " expires={exp} ({status})");
                Some(is_expired)
            } else {
                None
            };
            if let Some(days) = row.keep_calls_days_max {
                let _ = write!(line, " keep_calls_days_max={days}");
            }
            if let Some(reference) = &row.reference {
                let _ = write!(line, " ref={reference}");
            }
            if let Some(clients) = &row.clients {
                let _ = write!(line, " clients=[{}]", clients.join(", "));
            }
            if let Some(paths) = &row.paths {
                let _ = write!(line, " paths=[{}]", paths.join(", "));
            }
            if row.company_storage_only {
                line.push_str(" company_storage_only=true");
            }
            let _ = writeln!(out, "{line}");

            report_destinations.push(PolicyShowDestination {
                kind: row.kind.as_str().to_owned(),
                name: row.name.clone(),
                url: row.url.clone(),
                allow: allow_vec,
                expires: row.expires.map(|d| d.to_string()),
                expired: expired_opt,
                keep_calls_days_max: row.keep_calls_days_max,
                reference: row.reference.clone(),
                clients: row.clients.clone(),
                paths: row.paths.clone(),
                built_in: false,
                advisory: is_advisory,
            });

            // Effective row explanation
            let sample_dest = dest_for_row(row);
            let eff_allow: Vec<String> = effective
                .allow_set(&sample_dest)
                .ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect();
            let cuts = effective.explain(&sample_dest);
            let mut cut_parts = Vec::new();
            let mut report_cuts = Vec::new();
            for cut in cuts {
                let side = match cut.by {
                    CutBy::Policy => "policy",
                    CutBy::Owner => "owner",
                };
                cut_parts.push(format!("{} (cut by {side})", cut.label.as_str()));
                report_cuts.push(PolicyShowCut {
                    label: cut.label.as_str().to_owned(),
                    by: side.to_owned(),
                });
            }
            let mut eff_line = format!("effective {}", row.kind.as_str());
            if let Some(name) = &row.name {
                let _ = write!(eff_line, " {name}");
            }
            let _ = write!(eff_line, ": allow=[{}]", eff_allow.join(", "));
            if !cut_parts.is_empty() {
                let _ = write!(eff_line, " cuts=[{}]", cut_parts.join(", "));
            }
            let _ = writeln!(out, "{eff_line}");

            report_effective.push(PolicyShowEffective {
                kind: row.kind.as_str().to_owned(),
                name: row.name.clone(),
                allow: eff_allow,
                cuts: report_cuts,
            });
        }
    }

    // Fixed rows marked "built in" (`audit`, `git`, `edition`)
    for spec in DEST_KINDS.iter().filter(|s| s.source == RowSource::Fixed) {
        let kind_str = spec.kind.as_str();
        let _ = writeln!(out, "destination {kind_str} (built in): allow=[public]");
        report_destinations.push(PolicyShowDestination {
            kind: kind_str.to_owned(),
            name: None,
            url: None,
            allow: vec!["public".to_owned()],
            expires: None,
            expired: None,
            keep_calls_days_max: None,
            reference: None,
            clients: None,
            paths: None,
            built_in: true,
            advisory: false,
        });
    }

    // [audit] table
    let audit = effective.audit();
    let sink_disp = audit
        .sink
        .as_deref()
        .or_else(|| {
            seen.and_then(|s| s.audit_sink.as_ref())
                .and_then(|p| p.to_str())
        })
        .unwrap_or("none");
    let hours_disp = audit
        .max_unshipped_hours
        .map_or_else(|| "none".to_owned(), |h| h.to_string());
    let req_disp = if audit.require_fresh_export {
        "on"
    } else {
        "off"
    };
    let _ = writeln!(
        out,
        "audit: sink={sink_disp} max_unshipped_hours={hours_disp}"
    );
    let _ = writeln!(out, "export requirement: {req_disp}");

    let report = PolicyShow {
        report_version: REPORT_VERSION,
        verb: "policy show".to_owned(),
        state: state_word.to_owned(),
        path: path_opt,
        hash: hash_str,
        writer_check: writer_check_str,
        revision,
        recorded: recorded_report,
        labels: report_labels,
        stores: report_stores,
        destinations: report_destinations,
        effective: report_effective,
        exit: exit_code,
    };

    PolicyShowText { text: out, report }
}

fn dest_for_row(row: &PolicyDestination) -> Dest {
    match row.kind {
        DestKind::Model => Dest::Model {
            name: row.name.clone().unwrap_or_default(),
            base_url: row.url.clone().unwrap_or_default(),
        },
        DestKind::Handoff => Dest::Handoff,
        DestKind::Mcp => Dest::Mcp {
            client: row
                .clients
                .as_ref()
                .and_then(|c| c.first())
                .cloned()
                .unwrap_or_default(),
        },
        DestKind::Bridge => Dest::Bridge,
        DestKind::Export => Dest::Export {
            path: row
                .paths
                .as_ref()
                .and_then(|p| p.first())
                .cloned()
                .unwrap_or_default(),
        },
        DestKind::Backup => Dest::Backup {
            path: row
                .paths
                .as_ref()
                .and_then(|p| p.first())
                .cloned()
                .unwrap_or_default(),
            company_storage: row.company_storage_only,
        },
        DestKind::Print => Dest::Print,
        DestKind::Clipboard => Dest::Clipboard,
        DestKind::Audit => Dest::Audit,
        DestKind::Git => Dest::Git,
        DestKind::Edition => Dest::Edition,
    }
}
