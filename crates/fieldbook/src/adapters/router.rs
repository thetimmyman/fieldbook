//! `Router`: route selection over `[model] order` (`model-gate` step 6).
//!
//! The router has no label comparison of its own. It asks the shared egress decision which
//! candidate covers the request, preserves a terminal denial before reducing that decision to a
//! coverage boolean, and only then runs the chosen gate.

use fieldbook_core::audit::ModelId;
use fieldbook_core::check::Code;
use fieldbook_core::egress::{self, Decision, Dest};
use fieldbook_core::labels::LabelSet;
use fieldbook_core::ports::ModelRequest;
use fieldbook_core::ports::chat::SendPreview;

/// The route name of the copy-paste Handoff adapter.
pub const HANDOFF_ROUTE: &str = "handoff";

use crate::adapters::doubles::HandoffAdapter;
use crate::adapters::gate::denial_is_terminal;
use crate::adapters::model::{
    Deps, ErrorKind, ModelAnswer, ModelError, ModelOutcome, ModelPort, Plan, Route, RouteInfo,
};
use crate::adapters::registry::{Port, Row, State};
use crate::adapters::{Factory, factory_for};
use crate::io::net::DEFAULT_MAX_REPLY_BYTES;
use crate::settings::Settings;

/// One router over the configured endpoints and the Handoff route.
pub struct Router {
    entries: Vec<crate::adapters::gate::Guarded>,
    order: Vec<String>,
    deps: Deps,
}

impl Router {
    /// Builds one gate per configured endpoint and for handoff.
    ///
    /// # Errors
    ///
    /// Returns `FB0900` when a configured endpoint names no built adapter or no factory.
    pub fn from_settings(
        settings: &Settings,
        rows: &[Row],
        factories: &[Factory],
        deps: Deps,
    ) -> Result<Self, ModelError> {
        let mut entries = Vec::new();
        for (name, endpoint) in &settings.model.endpoints {
            let row = rows
                .iter()
                .find(|row| row.port == Port::Model && row.id == endpoint.adapter)
                .ok_or_else(|| ModelError::new(Code::Fb0900, ErrorKind::Policy))?;
            if row.state != State::Built {
                return Err(ModelError::new(Code::Fb0900, ErrorKind::Policy));
            }
            let factory = factory_for(&endpoint.adapter, factories)
                .ok_or_else(|| ModelError::new(Code::Fb0900, ErrorKind::Policy))?;
            let adapter = (factory.build)(&deps);
            let route = Route {
                name: name.clone(),
                base_url: endpoint.base_url.clone(),
                model: endpoint.model.clone(),
                timeout_s: endpoint.timeout_s,
                max_reply_bytes: DEFAULT_MAX_REPLY_BYTES,
                key_env: endpoint.key_env.clone(),
            };
            entries.push(crate::adapters::gate::Guarded::new(
                adapter,
                route,
                deps.clone(),
            ));
        }
        entries.push(crate::adapters::gate::Guarded::new(
            Box::new(HandoffAdapter),
            handoff_route(),
            deps.clone(),
        ));
        let order = if settings.model.order.is_empty() {
            vec![HANDOFF_ROUTE.to_owned()]
        } else {
            settings.model.order.clone()
        };
        Ok(Self {
            entries,
            order,
            deps,
        })
    }

    /// Returns every route in configured order, handoff last.
    #[must_use]
    pub fn routes(&self) -> &[crate::adapters::gate::Guarded] {
        &self.entries
    }

    fn entry(&self, name: &str) -> Option<&crate::adapters::gate::Guarded> {
        self.entries
            .iter()
            .find(|entry| entry.route().name.as_str() == name)
    }

    fn handoff(&self) -> Option<&crate::adapters::gate::Guarded> {
        self.entry(HANDOFF_ROUTE)
    }

    fn choose<'a>(
        &'a self,
        via: Option<&str>,
        labels: &LabelSet,
    ) -> Result<&'a crate::adapters::gate::Guarded, ModelError> {
        let snapshot = (self.deps.policy_provider)();
        let candidates: Vec<String> = match via {
            Some(name) => vec![name.to_owned()],
            None if self.order.is_empty() => vec![HANDOFF_ROUTE.to_owned()],
            None => self.order.clone(),
        };

        if via.is_some() {
            let name = candidates
                .first()
                .map(String::as_str)
                .unwrap_or(HANDOFF_ROUTE);
            let guarded = self
                .entry(name)
                .ok_or_else(|| ModelError::new(Code::Fb0918, ErrorKind::Policy))?;
            let dest = guarded.destination();
            return match egress::check(
                &snapshot.policy,
                &dest,
                Some(labels),
                None,
                snapshot.now,
                snapshot.shipped,
            ) {
                Decision::Allowed(_) => Ok(guarded),
                Decision::Denied(denial) => {
                    let _ = self.deps.journal.refuse(
                        Some(dest.kind()),
                        labels,
                        denial.code,
                        denial.reason.to_word(),
                    );
                    Err(ModelError::new(denial.code, ErrorKind::Policy))
                }
            };
        }

        for name in &candidates {
            let Some(guarded) = self.entry(name) else {
                continue;
            };
            let dest = guarded.destination();
            match egress::check(
                &snapshot.policy,
                &dest,
                Some(labels),
                None,
                snapshot.now,
                snapshot.shipped,
            ) {
                Decision::Allowed(_) => return Ok(guarded),
                Decision::Denied(denial) if denial_is_terminal(denial.code) => {
                    let _ = self.deps.journal.refuse(
                        Some(dest.kind()),
                        labels,
                        denial.code,
                        denial.reason.to_word(),
                    );
                    return Err(ModelError::new(denial.code, ErrorKind::Policy));
                }
                Decision::Denied(_) => {}
            }
        }

        let _ = self.deps.journal.refuse(
            None,
            labels,
            Code::Fb0901,
            fieldbook_core::audit::Word::new_static("label"),
        );
        Err(ModelError::new(Code::Fb0901, ErrorKind::Policy))
    }

    fn fallback_pack(
        &self,
        error: ModelError,
        req: &ModelRequest,
        stop: &dyn Fn() -> bool,
    ) -> ModelError {
        if error.kind != ErrorKind::Availability || error.ticket.is_some() {
            return error;
        }
        if !self.order.iter().any(|name| name == HANDOFF_ROUTE) {
            return error;
        }
        let Some(handoff) = self.handoff() else {
            return error;
        };
        let snapshot = (self.deps.policy_provider)();
        let decision = egress::check(
            &snapshot.policy,
            &Dest::Handoff,
            Some(&req.labels()),
            None,
            snapshot.now,
            snapshot.shipped,
        );
        if !decision.is_allowed() {
            return error;
        }
        match handoff.run_inner(req, stop) {
            Ok(ModelOutcome::Deferred(ticket)) => error.with_ticket(ticket),
            _ => error,
        }
    }
}

impl ModelPort for Router {
    fn plan(&self, via: Option<&str>, base: &LabelSet) -> Result<Plan, ModelError> {
        let guarded = self.choose(via, base)?;
        Ok(Plan {
            name: guarded.route().name.clone(),
            dest: guarded.destination(),
        })
    }

    fn run(
        &self,
        req: &ModelRequest,
        via: Option<&str>,
        stop: &dyn Fn() -> bool,
    ) -> Result<ModelOutcome, ModelError> {
        let guarded = self.choose(via, &req.labels())?;
        match guarded.run_inner(req, stop) {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                if via.is_none() {
                    Err(self.fallback_pack(error, req, stop))
                } else {
                    Err(error)
                }
            }
        }
    }

    fn resume(
        &self,
        ticket: &str,
        pasted: &str,
        label: Option<&str>,
    ) -> Result<ModelAnswer, ModelError> {
        let handoff = self
            .handoff()
            .ok_or_else(|| ModelError::new(Code::Fb0916, ErrorKind::Invalid))?;
        handoff.resume_inner(ticket, pasted, label)
    }

    fn describe(&self) -> Vec<RouteInfo> {
        self.entries
            .iter()
            .map(crate::adapters::gate::Guarded::describe_route)
            .collect()
    }

    fn preview(&self, req: &ModelRequest, via: Option<&str>) -> SendPreview {
        match self.choose(via, &req.labels()) {
            Ok(guarded) => guarded.preview_inner(req),
            Err(error) => SendPreview {
                labels: req.labels().ids().to_vec(),
                verdict: Some(error.code.id().to_owned()),
                reason: None,
                items: req.input_refs().len(),
                characters: req.system().chars().count() + req.prompt().chars().count(),
                ..SendPreview::default()
            },
        }
    }
}

/// The Handoff route value.
#[must_use]
pub fn handoff_route() -> Route {
    Route {
        name: fieldbook_core::audit::EndpointName::parse(HANDOFF_ROUTE)
            .unwrap_or_else(|_| unreachable!()),
        base_url: String::new(),
        model: ModelId::parse("handoff").unwrap_or_else(|_| unreachable!()),
        timeout_s: 0,
        max_reply_bytes: DEFAULT_MAX_REPLY_BYTES,
        key_env: None,
    }
}
