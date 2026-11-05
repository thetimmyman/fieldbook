//! Pure egress decision (`egress::check`, `egress::covers`, `Shipped`, `shipped`,
//! `Decision`, `Allowed`, `Denial`, `Reason`, `docs/reference/policy.md#the-decision`).

use std::fmt;

use crate::audit::{DestKind, Now, Word};
use crate::check::Code;
use crate::date::CivilDate;
use crate::hash::sha256_hex;
use crate::labels::{LabelAllow, LabelId, LabelSet, MARKED, PUBLIC, UNLABELLED};
use crate::policy::{EffectivePolicy, FloorKind, PolicyState};

use super::delivery::{Delivery, Operation, Target, Transfer};
use super::dest::{Dest, RowSource};

/// Age of the last `audit.export` event passed to [`check`] and [`covers`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shipped {
    /// No `audit.export` event has ever been recorded.
    Never,
    /// Whole hours elapsed since the newest `audit.export` event, rounded down.
    Hours(u64),
}

/// Computes the [`Shipped`] age from `last_export` (`journal.last_time_of(Kind::AuditExport)`)
/// and the current timestamp `now`, rounding down to whole hours.
#[must_use]
pub fn shipped(last_export: Option<Now>, now: Now) -> Shipped {
    let Some(last) = last_export else {
        return Shipped::Never;
    };
    let last_secs = now_to_epoch_seconds(last);
    let now_secs = now_to_epoch_seconds(now);
    if now_secs <= last_secs {
        return Shipped::Hours(0);
    }
    Shipped::Hours(((now_secs - last_secs) / 3600).cast_unsigned())
}

fn now_to_epoch_seconds(t: Now) -> i64 {
    days_from_civil(t.date) * 86_400 + i64::from(t.seconds)
}

const fn days_from_civil(date: CivilDate) -> i64 {
    let y = date.year() as i64 - if date.month() <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = date.month() as i64;
    let d = date.day() as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Closed reason word recorded in the journal `refuse` event for an egress denial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// Policy state is `Closed` (`"closed"`).
    Closed,
    /// Request carries `marked` (`"marked"`).
    Marked,
    /// Request carries `unlabelled` (`"unlabelled"`).
    Unlabelled,
    /// Request carries a label outside the defined table or outside the destination's narrowed `allow` (`"label"`).
    Label,
    /// No destination row matches (`"no-row"`).
    NoRow,
    /// Matching destination row has expired (`"expired"`).
    Expired,
    /// Model URL differs from the policy row's `url` (`"url"`).
    Url,
    /// MCP client hash is not in the policy row's `clients` list (`"client"`).
    Client,
    /// Outbound text contains a marking outside the request's labels, or delivery lacks labels (`"mismatch"`).
    Mismatch,
    /// Non-public model call with a missing or stale `audit.export` (`"shipped"`).
    Shipped,
}

impl Reason {
    /// Returns the closed vocabulary word for the journal `refuse` event.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Marked => "marked",
            Self::Unlabelled => "unlabelled",
            Self::Label => "label",
            Self::NoRow => "no-row",
            Self::Expired => "expired",
            Self::Url => "url",
            Self::Client => "client",
            Self::Mismatch => "mismatch",
            Self::Shipped => "shipped",
        }
    }

    /// Converts this [`Reason`] into a typed audit [`Word`].
    #[must_use]
    pub const fn to_word(self) -> Word {
        Word::new_static(self.as_str())
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A denial returned by [`check`].
///
/// Holds only the diagnostic [`Code`], the closed [`Reason`] word, and the request's opaque
/// [`LabelId`] values—never a human label title or document text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    /// Diagnostic code (`FB0413`, `FB0901`, `FB0902`, `FB0918`, or `FB0925`).
    pub code: Code,
    /// Closed journal `refuse` reason word.
    pub reason: Reason,
    /// Opaque label ids carried by the request.
    pub labels: Vec<LabelId>,
}

impl fmt::Display for Denial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.labels.is_empty() {
            write!(f, "{}: denied ({})", self.code.id(), self.reason.as_str())
        } else {
            let joined = self
                .labels
                .iter()
                .map(LabelId::as_str)
                .collect::<Vec<_>>()
                .join("|");
            write!(
                f,
                "{}: denied ({}) for labels `{joined}`",
                self.code.id(),
                self.reason.as_str()
            )
        }
    }
}

impl std::error::Error for Denial {}

/// Unforgeable, non-cloneable capability token produced exclusively by [`check`].
///
/// - Has private fields, no public constructor, and no `Clone` implementation.
/// - Selection (`labels = None, delivery = None`) and preflight (`labels = Some(..), delivery = None`)
///   tokens have `delivery = None` and are refused at every sink (`permits_transfer` returns `false`).
/// - Delivery-bound tokens (`labels = Some(..), delivery = Some(..)`) bind exact targets, operations,
///   SHA-256 digests, and byte lengths.
///
/// ```compile_fail
/// // allowed_cannot_be_built_by_hand
/// use fieldbook_core::audit::DestKind;
/// use fieldbook_core::egress::Allowed;
/// use fieldbook_core::labels::LabelAllow;
///
/// let _token = Allowed {
///     kind: DestKind::Handoff,
///     labels: None,
///     permitted: LabelAllow::empty(),
///     policy_hash: String::new(),
///     delivery: None,
/// };
/// ```
///
/// ```compile_fail
/// // allowed_cannot_be_cloned
/// use fieldbook_core::egress::Allowed;
///
/// fn duplicate(token: &Allowed) -> Allowed {
///     token.clone()
/// }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct Allowed {
    kind: DestKind,
    labels: Option<LabelSet>,
    permitted: LabelAllow,
    policy_hash: String,
    delivery: Option<Vec<Transfer>>,
}

impl Allowed {
    /// Returns the [`DestKind`] this token was issued for.
    #[must_use]
    pub const fn kind(&self) -> DestKind {
        self.kind
    }

    /// Returns the request's data [`LabelSet`] when `check` was called with `Some(labels)`.
    #[must_use]
    pub const fn labels(&self) -> Option<&LabelSet> {
        self.labels.as_ref()
    }

    /// Returns the destination's narrowed permission list ([`LabelAllow`]) for [`super::LabelledView`] selection.
    #[must_use]
    pub const fn permitted(&self) -> &LabelAllow {
        &self.permitted
    }

    /// Returns the 64-character lowercase hex SHA-256 of the policy in force when this token was issued.
    #[must_use]
    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }

    /// Returns `true` when this token was issued with `Some(delivery)` and carries sink authority.
    #[must_use]
    pub const fn has_delivery(&self) -> bool {
        self.delivery.is_some()
    }

    /// Returns the bound [`Transfer`] slice when this token is delivery-bound, or `None` for
    /// selection/preflight tokens.
    #[must_use]
    pub fn transfers(&self) -> Option<&[Transfer]> {
        self.delivery.as_deref()
    }

    /// Verifies that this token is delivery-bound and contains a [`Transfer`] matching `target`,
    /// `operation`, and the exact SHA-256 digest and byte length of `bytes`.
    ///
    /// Selection and preflight tokens (`delivery == None`) always return `false`.
    #[must_use]
    pub fn permits_transfer(&self, target: &Target, operation: &Operation, bytes: &[u8]) -> bool {
        let Some(entries) = &self.delivery else {
            return false;
        };
        let actual_sha = sha256_hex(bytes);
        let actual_len = bytes.len() as u64;
        entries.iter().any(|t| {
            &t.target == target
                && &t.operation == operation
                && t.sha256 == actual_sha
                && t.bytes == actual_len
        })
    }

    /// Verifies a directory or tree operation (`CreateDir`, `RemoveTree`, `RenameTree`) against
    /// a bound manifest digest and byte length.
    #[must_use]
    pub fn permits_manifest(
        &self,
        target: &Target,
        operation: &Operation,
        manifest_sha256: &str,
        total_bytes: u64,
    ) -> bool {
        let Some(entries) = &self.delivery else {
            return false;
        };
        entries.iter().any(|t| {
            &t.target == target
                && &t.operation == operation
                && t.sha256 == manifest_sha256
                && t.bytes == total_bytes
        })
    }
}

/// Outcome of [`check`]: either [`Decision::Allowed`] or [`Decision::Denied`].
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// The request is permitted.
    Allowed(Allowed),
    /// The request is denied.
    Denied(Denial),
}

impl Decision {
    /// Returns `true` when this decision is [`Decision::Allowed`].
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed(_))
    }

    /// Returns `true` when this decision is [`Decision::Denied`].
    #[must_use]
    pub const fn is_denied(&self) -> bool {
        matches!(self, Self::Denied(_))
    }

    /// Consumes `self`, returning `Some(Allowed)` if allowed.
    #[must_use]
    pub fn ok(self) -> Option<Allowed> {
        match self {
            Self::Allowed(a) => Some(a),
            Self::Denied(_) => None,
        }
    }

    /// Consumes `self`, returning `Some(Denial)` if denied.
    #[must_use]
    pub fn err(self) -> Option<Denial> {
        match self {
            Self::Allowed(_) => None,
            Self::Denied(d) => Some(d),
        }
    }
}

/// Decides whether `labels` and `delivery` may go to `dest` under `policy` at `now` with export
/// age `shipped` (`docs/reference/policy.md#the-decision`, `ADR 0004 §5`).
///
/// Decision order:
/// 1. `Closed` state -> deny (`FB0413`, reason `closed`), except `Dest::Audit`.
/// 2. Any label is `marked` (`FB0901`, `marked`) or outside the policy's label table (`FB0901`, `label`),
///    or `unlabelled` (`FB0901`, `unlabelled`) unless `state == Floor(FloorKind::B)` and `dest` is
///    `Dest::Backup { company_storage: true, .. }`.
/// 3. Destination availability: no row (`FB0918`, `no-row`), expired row (`FB0918`, `expired`),
///    model URL mismatch (`FB0918`, `url`), unlisted MCP client (`FB0918`, `client`), or path outside
///    every prefix (`FB0918`, `no-row`).
/// 4. When `labels` is `Some`: every label in `labels` must be in the narrowed `allow` set (`FB0901`, `label`).
/// 5. When `delivery` is `Some`: `labels` must also be `Some` (otherwise `FB0902`, `mismatch`), every
///    `Transfer` in `delivery` must be valid and non-empty, and any `delivery.markings` must be a
///    subset of `labels` (`FB0902`, `mismatch`).
/// 6. For `Dest::Model` with a non-public label while `[audit] require_fresh_export` is `true`:
///    `shipped` older than `max_unshipped_hours` or `Shipped::Never` -> deny (`FB0925`, `shipped`).
/// 7. Return `Decision::Allowed(Allowed)`.
#[must_use]
pub fn check(
    policy: &EffectivePolicy,
    dest: &Dest,
    labels: Option<&LabelSet>,
    delivery: Option<&Delivery>,
    now: Now,
    shipped: Shipped,
) -> Decision {
    let req_ids: Vec<LabelId> = labels.map_or_else(Vec::new, |s| s.ids().to_vec());

    // A delivery without data labels is FB0902 (`mismatch`).
    if labels.is_none() && delivery.is_some() {
        return Decision::Denied(Denial {
            code: Code::Fb0902,
            reason: Reason::Mismatch,
            labels: req_ids,
        });
    }

    // Step 1: Closed state denies every destination except `Dest::Audit` (FB0413).
    if policy.state().is_closed() && !matches!(dest, Dest::Audit) {
        return Decision::Denied(Denial {
            code: Code::Fb0413,
            reason: Reason::Closed,
            labels: req_ids,
        });
    }

    // Audit sink exception: journal lines only, permitted even when Closed.
    if matches!(dest, Dest::Audit) {
        if let Some(set) = labels {
            if set.contains(&MARKED) {
                return Decision::Denied(Denial {
                    code: Code::Fb0901,
                    reason: Reason::Marked,
                    labels: req_ids,
                });
            }
            if set.contains(&UNLABELLED) {
                return Decision::Denied(Denial {
                    code: Code::Fb0901,
                    reason: Reason::Unlabelled,
                    labels: req_ids,
                });
            }
            if !set.is_public() {
                return Decision::Denied(Denial {
                    code: Code::Fb0901,
                    reason: Reason::Label,
                    labels: req_ids,
                });
            }
        }
        let bound = match validate_delivery_binding(dest, labels, delivery, &req_ids) {
            Ok(b) => b,
            Err(denial) => return Decision::Denied(denial),
        };
        return Decision::Allowed(Allowed {
            kind: DestKind::Audit,
            labels: labels.cloned(),
            permitted: LabelAllow::from_ids([PUBLIC]),
            policy_hash: policy.hash().to_owned(),
            delivery: bound,
        });
    }

    // Step 2: `marked`, unknown label, or `unlabelled` (except Floor B backup with company_storage = true).
    if let Some(set) = labels {
        if set.contains(&MARKED) {
            return Decision::Denied(Denial {
                code: Code::Fb0901,
                reason: Reason::Marked,
                labels: req_ids,
            });
        }
        let table = policy.label_table();
        for id in set.ids() {
            if !table.is_known(id) {
                return Decision::Denied(Denial {
                    code: Code::Fb0901,
                    reason: Reason::Label,
                    labels: req_ids,
                });
            }
        }
        if set.contains(&UNLABELLED) {
            let floor_b_backup_ok = matches!(policy.state(), PolicyState::Floor(FloorKind::B))
                && matches!(
                    dest,
                    Dest::Backup {
                        company_storage: true,
                        ..
                    }
                );
            if !floor_b_backup_ok {
                return Decision::Denied(Denial {
                    code: Code::Fb0901,
                    reason: Reason::Unlabelled,
                    labels: req_ids,
                });
            }
        }
    }

    // Step 3: Destination row existence, expiry, URL, client hash, and path prefix (FB0918).
    let permitted = match check_row_and_narrow(policy, dest, now, &req_ids) {
        Ok(allow) => allow,
        Err(denial) => return Decision::Denied(denial),
    };

    // Step 4: Every label in `labels` must be in the narrowed `permitted` set (FB0901).
    if let Some(set) = labels
        && !permitted.permits(set)
    {
        return Decision::Denied(Denial {
            code: Code::Fb0901,
            reason: Reason::Label,
            labels: req_ids,
        });
    }

    // Step 5: Outbound markings subset check and delivery binding validation (FB0902).
    let bound = match validate_delivery_binding(dest, labels, delivery, &req_ids) {
        Ok(b) => b,
        Err(denial) => return Decision::Denied(denial),
    };

    // Step 6: Model call with a non-public label requires a fresh audit export when `require_fresh_export` is true (FB0925).
    if matches!(dest, Dest::Model { .. })
        && let Some(set) = labels
        && !set.is_public()
    {
        let audit = policy.audit();
        if audit.require_fresh_export {
            let Some(max_hours) = audit.max_unshipped_hours else {
                return Decision::Denied(Denial {
                    code: Code::Fb0925,
                    reason: Reason::Shipped,
                    labels: req_ids,
                });
            };
            match shipped {
                Shipped::Never => {
                    return Decision::Denied(Denial {
                        code: Code::Fb0925,
                        reason: Reason::Shipped,
                        labels: req_ids,
                    });
                }
                Shipped::Hours(h) if h > max_hours => {
                    return Decision::Denied(Denial {
                        code: Code::Fb0925,
                        reason: Reason::Shipped,
                        labels: req_ids,
                    });
                }
                Shipped::Hours(_) => {}
            }
        }
    }

    // Step 7: Allowed.
    Decision::Allowed(Allowed {
        kind: dest.kind(),
        labels: labels.cloned(),
        permitted,
        policy_hash: policy.hash().to_owned(),
        delivery: bound,
    })
}

/// Pure boolean helper for status display: calls [`check`] with `Some(labels)` and `None` delivery
/// and returns `true` iff the decision is [`Decision::Allowed`].
#[must_use]
pub fn covers(
    policy: &EffectivePolicy,
    dest: &Dest,
    labels: &LabelSet,
    now: Now,
    shipped: Shipped,
) -> bool {
    check(policy, dest, Some(labels), None, now, shipped).is_allowed()
}

fn check_row_and_narrow(
    policy: &EffectivePolicy,
    dest: &Dest,
    now: Now,
    req_ids: &[LabelId],
) -> Result<LabelAllow, Denial> {
    if dest.spec().source == RowSource::Fixed {
        return Ok(LabelAllow::from_ids([PUBLIC]));
    }

    let rows = policy.matching_rows(dest);
    if rows.is_empty() {
        return Err(Denial {
            code: Code::Fb0918,
            reason: Reason::NoRow,
            labels: req_ids.to_vec(),
        });
    }

    let active_rows: Vec<_> = rows
        .into_iter()
        .filter(|r| r.expires.is_none_or(|exp| now.date <= exp))
        .collect();
    if active_rows.is_empty() {
        return Err(Denial {
            code: Code::Fb0918,
            reason: Reason::Expired,
            labels: req_ids.to_vec(),
        });
    }

    match dest {
        Dest::Model { name, base_url } => {
            let row = active_rows[0];
            if matches!(policy.state(), PolicyState::Loaded(_))
                && row.url.as_deref() != Some(base_url.as_str())
            {
                return Err(Denial {
                    code: Code::Fb0918,
                    reason: Reason::Url,
                    labels: req_ids.to_vec(),
                });
            }
            let narrowed = if let Some(owner_allow) = policy.owner().endpoints.get(name) {
                row.allow.intersection(owner_allow)
            } else if matches!(policy.state(), PolicyState::Floor(_))
                && policy.owner().endpoints.is_empty()
            {
                row.allow.clone()
            } else {
                LabelAllow::empty()
            };
            Ok(narrowed)
        }
        Dest::Mcp { client } => {
            let row = active_rows[0];
            if let Some(clients) = &row.clients
                && !clients.iter().any(|c| c == client)
            {
                return Err(Denial {
                    code: Code::Fb0918,
                    reason: Reason::Client,
                    labels: req_ids.to_vec(),
                });
            }
            let narrowed = match &policy.owner().mcp {
                Some(owner_allow) => row.allow.intersection(owner_allow),
                None => row.allow.clone(),
            };
            Ok(narrowed)
        }
        Dest::Handoff => {
            let row = active_rows[0];
            let narrowed = match &policy.owner().handoff {
                Some(owner_allow) => row.allow.intersection(owner_allow),
                None => row.allow.clone(),
            };
            Ok(narrowed)
        }
        Dest::Export { .. } | Dest::Backup { .. } => {
            let mut unioned = LabelAllow::empty();
            for row in active_rows {
                unioned = unioned.union(&row.allow);
            }
            Ok(unioned)
        }
        Dest::Bridge | Dest::Print | Dest::Clipboard => Ok(active_rows[0].allow.clone()),
        Dest::Audit | Dest::Git | Dest::Edition => Ok(LabelAllow::from_ids([PUBLIC])),
    }
}

fn validate_delivery_binding(
    dest: &Dest,
    labels: Option<&LabelSet>,
    delivery: Option<&Delivery>,
    req_ids: &[LabelId],
) -> Result<Option<Vec<Transfer>>, Denial> {
    let Some(deliv) = delivery else {
        return Ok(None);
    };
    let Some(set) = labels else {
        return Err(Denial {
            code: Code::Fb0902,
            reason: Reason::Mismatch,
            labels: req_ids.to_vec(),
        });
    };

    if let Some(marked_set) = &deliv.markings
        && !marked_set.is_subset_of(set)
    {
        return Err(Denial {
            code: Code::Fb0902,
            reason: Reason::Mismatch,
            labels: req_ids.to_vec(),
        });
    }

    if deliv.entries.is_empty() || !deliv.entries.iter().all(Transfer::is_valid) {
        return Err(Denial {
            code: Code::Fb0902,
            reason: Reason::Mismatch,
            labels: req_ids.to_vec(),
        });
    }

    // Verify each transfer target matches the destination kind/target.
    for entry in &deliv.entries {
        let target_ok = match (dest, &entry.target, &entry.operation) {
            (Dest::Model { base_url, .. }, Target::Endpoint(ep), Operation::Post) => {
                ep == base_url || ep.starts_with(base_url)
            }
            (
                Dest::Export { .. } | Dest::Backup { .. } | Dest::Audit,
                Target::Path(_),
                Operation::Write
                | Operation::CreateDir
                | Operation::RemoveTree
                | Operation::RenameTree { .. },
            )
            | (
                Dest::Handoff
                | Dest::Mcp { .. }
                | Dest::Bridge
                | Dest::Print
                | Dest::Clipboard
                | Dest::Edition,
                Target::Reply(_),
                Operation::Reply,
            ) => true,
            _ => false,
        };
        if !target_ok {
            return Err(Denial {
                code: Code::Fb0902,
                reason: Reason::Mismatch,
                labels: req_ids.to_vec(),
            });
        }
    }

    Ok(Some(deliv.entries.clone()))
}
