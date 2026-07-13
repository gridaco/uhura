//! `step_u` — one external event, one deterministic step (design §7):
//! acceptance (stale-scope → occluded → ineligible; stale `view_rev`
//! accepted), multi-handler guard selection, TRANSACTIONAL handler bodies
//! (§4.2: staged writes/pending/counters/commands; a not-ready projection
//! read aborts atomically — guard position reads as false), structural
//! statements applied at dispatch end, `rev + 1` always, `eval_view` once
//! from the final state. No internal queue ⇒ termination by construction.

use std::collections::BTreeMap;

use uhura_base::{Diagnostic, FileId, Ident, Span, Value, codes};
use uhura_port::envelope::{CommandEnvelope, OutcomeResult, ProviderMsg};

use crate::decode::decode_value;
use crate::eval::{EmitEnv, EvalError, Frame, Stop, eval_view};
use crate::event::Event;
use crate::ir::{self, DefIr, ProgramIr};
use crate::state::{
    Counters, NavEntry, PendingCommand, Projections, SurfaceState, UiState, initial_state,
    map_key_string,
};
use crate::trace::{DispatchRecord, Disposition, DropReason, GuardNote, GuardResult, StepTrace};
use crate::view::{Descriptor, Node, Snapshot};

pub struct StepResult {
    pub u: UiState,
    pub v: Snapshot,
    pub c: Vec<CommandEnvelope>,
    pub i: Vec<IntentEnvelope>,
    pub g: Vec<Diagnostic>,
    pub t: StepTrace,
}

impl StepResult {
    /// The frozen step-result envelope `dispatch` returns across the wasm
    /// boundary (§12.3, plan micro-decision #14). Every key is always
    /// present: `c` command envelopes (provider wire form) for the shell
    /// to forward, `i` intents, `g` runtime diagnostics, `t` the canonical
    /// trace record, `v` the full `uhura-view/0` snapshot. `U` stays
    /// inside the machine — hashes travel in `t`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "c": self
                .c
                .iter()
                .map(|c| ProviderMsg::Command(c.clone()).to_json())
                .collect::<Vec<_>>(),
            "i": self.i.iter().map(IntentEnvelope::to_json).collect::<Vec<_>>(),
            "g": self.g.iter().map(diagnostic_json).collect::<Vec<_>>(),
            "t": self.t.to_json(),
            "v": self.v.to_json(),
        })
    }
}

/// Host intents (§7.4): the machine owns `nav`; the host's history moves
/// only via these. The spike shell executes them as no-ops — the contract
/// stays visible in `T`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntentEnvelope {
    HistoryPush {
        route: Ident,
        params: BTreeMap<Ident, Value>,
    },
    HistoryBack,
    /// Emitted when the topmost surface dismisses and its opening trigger
    /// node is known (§4.2).
    FocusRestore {
        key_path: String,
    },
}

impl IntentEnvelope {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            IntentEnvelope::HistoryPush { route, params } => serde_json::json!({
                "intent": "history-push",
                "route": route.to_string(),
                "params": params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_json()))
                    .collect::<serde_json::Map<_, _>>(),
            }),
            IntentEnvelope::HistoryBack => serde_json::json!({ "intent": "history-back" }),
            IntentEnvelope::FocusRestore { key_path } => serde_json::json!({
                "intent": "focus-restore",
                "key-path": key_path,
            }),
        }
    }
}

/// The reserved structural dismiss event (micro-decision #36).
const DISMISS: &str = "dismiss";

pub fn step_u(
    p: &ProgramIr,
    u: UiState,
    x: &Projections,
    e: Event,
) -> Result<StepResult, EvalError> {
    let event_json = e.to_json();
    let mut machine = Machine {
        p,
        x,
        u,
        c: Vec::new(),
        i: Vec::new(),
        g: Vec::new(),
        structural: Vec::new(),
        pre_view: None,
    };

    let disposition = match &e {
        Event::Init { route, params } => {
            machine.init(route, params)?;
            Disposition::Delivery
        }
        Event::Projection { .. } | Event::ProjectionFailed { .. } => {
            // The harness already applied the updates to X (§7.2); the
            // step exists to recompute V — overlay rebase is free.
            Disposition::Delivery
        }
        Event::Outcome {
            correlation,
            result,
            ..
        } => machine.dispatch_outcome(correlation, result)?,
        Event::Ui {
            descriptor, data, ..
        } => machine.dispatch_ui(descriptor, data.as_ref())?,
    };

    machine.u.rev += 1;
    let v = eval_view(p, &machine.u, x)?;
    let t = StepTrace {
        event: event_json,
        applies: Vec::new(),
        disposition,
        structural: machine.structural,
        c: machine
            .c
            .iter()
            .map(|c| ProviderCommand(c).to_json())
            .collect(),
        i: machine.i.iter().map(IntentEnvelope::to_json).collect(),
        g: machine.g.iter().map(diagnostic_json).collect(),
        u_hash: machine.u.u_hash(),
        v_hash: v.v_hash(),
        v: None,
    };
    Ok(StepResult {
        u: machine.u,
        v,
        c: machine.c,
        i: machine.i,
        g: machine.g,
        t,
    })
}

/// Runtime diagnostics trace as spanless records — they are minted by the
/// machine, not a source pass (§7.5).
fn diagnostic_json(d: &Diagnostic) -> serde_json::Value {
    serde_json::json!({
        "code": d.code,
        "rule": d.rule,
        "message": d.message,
    })
}

/// Command envelopes trace in their wire form (`kind: "command"`).
struct ProviderCommand<'a>(&'a CommandEnvelope);

impl ProviderCommand<'_> {
    fn to_json(&self) -> serde_json::Value {
        uhura_port::envelope::ProviderMsg::Command(self.0.clone()).to_json()
    }
}

// ── the per-step machine ────────────────────────────────────────────────────

struct Machine<'a> {
    p: &'a ProgramIr,
    x: &'a Projections,
    u: UiState,
    c: Vec<CommandEnvelope>,
    i: Vec<IntentEnvelope>,
    /// Runtime diagnostics (`G` — §7.1).
    g: Vec<Diagnostic>,
    structural: Vec<serde_json::Value>,
    /// The view the input event was emitted against, for the focus-restore
    /// node search — captured lazily at commit time but BEFORE the commit
    /// mutates state (only an `open-surface` commit fills it).
    pre_view: Option<Snapshot>,
}

/// Where a scope string lands in the current state.
enum ScopeTarget {
    /// Index into `nav` (top = interactive).
    Page(usize),
    /// Index into `surfaces`.
    Surface(usize),
}

impl<'a> Machine<'a> {
    // ── init ────────────────────────────────────────────────────────────

    fn init(&mut self, route: &Ident, params: &BTreeMap<Ident, Value>) -> Result<(), EvalError> {
        let def = self
            .p
            .pages
            .get(route)
            .ok_or_else(|| EvalError(format!("init: no page for route `{route}`")))?;
        let serial = self.u.counters.mint_page();
        self.u.nav = vec![NavEntry {
            serial,
            route: route.clone(),
            params: params.clone(),
            state: initial_state(def),
        }];
        self.u.surfaces.clear();
        self.structural.push(serde_json::json!({
            "op": "init",
            "route": route.to_string(),
            "serial": serial,
        }));
        Ok(())
    }

    // ── scope resolution ────────────────────────────────────────────────

    fn parse_scope(scope: &str) -> Option<(&str, u64)> {
        let (kind, serial) = scope.split_once(':')?;
        Some((kind, serial.parse().ok()?))
    }

    fn find_scope(&self, scope: &str) -> Option<ScopeTarget> {
        let (kind, serial) = Self::parse_scope(scope)?;
        match kind {
            "page" => self
                .u
                .nav
                .iter()
                .position(|e| e.serial == serial)
                .map(ScopeTarget::Page),
            "surface" => self
                .u
                .surfaces
                .iter()
                .position(|s| s.serial == serial)
                .map(ScopeTarget::Surface),
            _ => None,
        }
    }

    /// §7.2 Ui acceptance, first two rules: scope alive → else stale-scope
    /// (a below-top page is not the interactive layer); top-surface
    /// modality → else occluded. The Err IS the drop disposition — size is
    /// irrelevant on the once-per-step drop path.
    #[allow(clippy::result_large_err)]
    fn accept_ui_scope(&self, scope: &str) -> Result<ScopeTarget, Disposition> {
        let dropped = |reason, detail: String| Disposition::Dropped { reason, detail };
        let target = self
            .find_scope(scope)
            .ok_or_else(|| dropped(DropReason::StaleScope, format!("scope `{scope}` is gone")))?;
        match target {
            ScopeTarget::Page(idx) => {
                if idx + 1 != self.u.nav.len() {
                    return Err(dropped(
                        DropReason::StaleScope,
                        format!("`{scope}` is below the top of the nav stack"),
                    ));
                }
                if !self.u.surfaces.is_empty() {
                    return Err(dropped(
                        DropReason::Occluded,
                        "a surface is above the page".to_string(),
                    ));
                }
                Ok(target)
            }
            ScopeTarget::Surface(idx) => {
                if idx + 1 != self.u.surfaces.len() {
                    return Err(dropped(
                        DropReason::Occluded,
                        format!("`{scope}` is not the top surface"),
                    ));
                }
                Ok(target)
            }
        }
    }

    fn scope_def(&self, target: &ScopeTarget) -> Result<(&'a DefIr, Ident), EvalError> {
        match target {
            ScopeTarget::Page(idx) => {
                let entry = &self.u.nav[*idx];
                let def = self
                    .p
                    .pages
                    .get(&entry.route)
                    .ok_or_else(|| EvalError(format!("no page for route `{}`", entry.route)))?;
                Ok((def, entry.route.clone()))
            }
            ScopeTarget::Surface(idx) => {
                let s = &self.u.surfaces[*idx];
                let def = self
                    .p
                    .surfaces
                    .get(&s.definition)
                    .ok_or_else(|| EvalError(format!("no surface `{}`", s.definition)))?;
                Ok((def, s.definition.clone()))
            }
        }
    }

    // ── ui dispatch ─────────────────────────────────────────────────────

    fn dispatch_ui(
        &mut self,
        descriptor: &Descriptor,
        data: Option<&Value>,
    ) -> Result<Disposition, EvalError> {
        // The reserved dismiss is structural: no authored handler (§8.1).
        if descriptor.emit.as_str() == DISMISS && descriptor.scope.starts_with("surface:") {
            return match self.accept_ui_scope(&descriptor.scope) {
                Err(drop) => Ok(drop),
                Ok(ScopeTarget::Surface(_)) => {
                    let (_, serial) =
                        Self::parse_scope(&descriptor.scope).expect("accepted scope parses");
                    self.dismiss_surface(serial);
                    Ok(Disposition::Reserved {
                        scope: descriptor.scope.clone(),
                    })
                }
                Ok(ScopeTarget::Page(_)) => unreachable!("surface scope resolved to a page"),
            };
        }

        let target = match self.accept_ui_scope(&descriptor.scope) {
            Ok(target) => target,
            Err(drop) => return Ok(drop),
        };
        let (def, definition) = self.scope_def(&target)?;

        // Third acceptance rule: the emit must be declared with a
        // matching payload — fields (payload ∪ carried data) must cover
        // the signature exactly, each decoding against its type.
        let Some(sig) = def.events.get(&descriptor.emit) else {
            return Ok(Disposition::Dropped {
                reason: DropReason::Ineligible,
                detail: format!("`{definition}` declares no event `{}`", descriptor.emit),
            });
        };
        let bindings = match bind_payload(sig, &descriptor.payload, data) {
            Ok(bindings) => bindings,
            Err(detail) => {
                return Ok(Disposition::Dropped {
                    reason: DropReason::Ineligible,
                    detail,
                });
            }
        };

        let record = self.dispatch(
            &target,
            def,
            definition,
            descriptor.scope.clone(),
            descriptor.emit.to_string(),
            |h| matches!(&h.on, ir::EventKeyIr::Semantic { event } if *event == descriptor.emit),
            &BindSpec::Named(&bindings),
            Some(descriptor),
        )?;
        Ok(Disposition::Dispatched(record))
    }

    // ── outcome dispatch ────────────────────────────────────────────────

    fn dispatch_outcome(
        &mut self,
        correlation: &str,
        result: &OutcomeResult,
    ) -> Result<Disposition, EvalError> {
        let tag = correlation
            .strip_prefix("c-")
            .and_then(|n| n.parse::<u64>().ok());
        let Some(pending) = tag.and_then(|t| self.u.pending.remove(&t)) else {
            return Ok(Disposition::Dropped {
                reason: DropReason::UnknownCorrelation,
                detail: format!("no pending command `{correlation}`"),
            });
        };
        let tag = tag.expect("pending entry implies a parsed tag");

        // Outcomes reach any mounted origin — below-top pages keep state
        // and must settle their overlays. Only a DEAD origin drops
        // (pending stays removed; truth already lives in X — §7.2).
        let Some(target) = self.find_scope(&pending.origin) else {
            return Ok(Disposition::Dropped {
                reason: DropReason::StaleOutcome,
                detail: format!("origin `{}` is unmounted", pending.origin),
            });
        };
        let (def, definition) = self.scope_def(&target)?;

        let (which, refusal) = match result {
            OutcomeResult::Ok => (ir::OutcomeKindIr::Ok, None),
            OutcomeResult::Refused { refusal } => {
                (ir::OutcomeKindIr::Err, Some(refusal.to_string()))
            }
            // `unavailable` routes to `.err` (§4.2); the refusal binding
            // reads the reserved name.
            OutcomeResult::Unavailable { .. } => {
                (ir::OutcomeKindIr::Err, Some("unavailable".to_string()))
            }
        };
        let mut positional = vec![Value::Tag(tag), pending.payload.clone()];
        if let Some(refusal) = &refusal {
            positional.push(Value::Text(refusal.clone()));
        }
        let on_label = format!(
            "{}.{}",
            pending.command,
            match which {
                ir::OutcomeKindIr::Ok => "ok",
                ir::OutcomeKindIr::Err => "err",
            }
        );
        let record = self.dispatch(
            &target,
            def,
            definition,
            pending.origin.clone(),
            on_label,
            |h| {
                matches!(&h.on, ir::EventKeyIr::Outcome { command, which: w }
                    if *command == pending.command && *w == which)
            },
            &BindSpec::Positional(&positional),
            None,
        )?;
        Ok(Disposition::Dispatched(record))
    }

    // ── guard selection + the transaction ───────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn dispatch(
        &mut self,
        target: &ScopeTarget,
        def: &DefIr,
        definition: Ident,
        scope: String,
        on_label: String,
        matches_handler: impl Fn(&ir::HandlerIr) -> bool,
        bind: &BindSpec<'_>,
        trigger: Option<&Descriptor>,
    ) -> Result<DispatchRecord, EvalError> {
        let (entry_state, props, params) = match target {
            ScopeTarget::Page(idx) => {
                let entry = &self.u.nav[*idx];
                (entry.state.clone(), BTreeMap::new(), entry.params.clone())
            }
            ScopeTarget::Surface(idx) => {
                let s = &self.u.surfaces[*idx];
                (s.state.clone(), s.props.clone(), BTreeMap::new())
            }
        };

        let mut record = DispatchRecord {
            scope: scope.clone(),
            definition,
            on: on_label,
            guards: Vec::new(),
            selected: None,
            writes: Vec::new(),
            aborted: None,
        };

        // Source order, first satisfied guard wins; a guard-position
        // not-ready read means the guard is false (§4.2).
        type Selected<'h> = (usize, &'h ir::HandlerIr, Vec<(Ident, Value)>);
        let mut selected: Option<Selected<'_>> = None;
        for (index, handler) in def.handlers.iter().enumerate() {
            if !matches_handler(handler) {
                continue;
            }
            let bindings = bind.bind(&handler.params);
            let result = match &handler.guard {
                None => GuardResult::Satisfied,
                Some(guard) => {
                    let frame = Frame {
                        program: self.p,
                        x: self.x,
                        scope: scope.clone(),
                        state: &entry_state,
                        props: props.clone(),
                        params: params.clone(),
                        bindings: bindings.clone(),
                        emits: EmitEnv::Machine,
                    };
                    match frame.eval_expr(guard) {
                        Ok(Value::Bool(true)) => GuardResult::Satisfied,
                        Ok(Value::Bool(false)) => GuardResult::Unsatisfied,
                        Ok(other) => {
                            return Err(EvalError(format!("guard was {other:?}")));
                        }
                        Err(Stop::NotReady(_)) => GuardResult::NotReady,
                        Err(Stop::Internal(msg)) => return Err(EvalError(msg)),
                    }
                }
            };
            record.guards.push(GuardNote {
                handler: index,
                result,
            });
            if result == GuardResult::Satisfied {
                selected = Some((index, handler, bindings));
                break;
            }
        }

        let Some((index, handler, bindings)) = selected else {
            return Ok(record); // dropped: no handler — traced by the record
        };
        record.selected = Some(index);

        // The transaction: staged state / pending / counters / commands /
        // structural ops. Nothing touches `self.u` until commit.
        match self.run_txn(
            def,
            &scope,
            &entry_state,
            &props,
            &params,
            handler,
            bindings,
        )? {
            TxnOutcome::Aborted => {
                record.aborted = Some("projection-not-ready".to_string());
                Ok(record)
            }
            TxnOutcome::Committed(txn) => {
                record.writes = txn.writes;
                self.g.extend(txn.warnings);
                // The focus-restore search needs the view the event was
                // emitted against — capture it BEFORE the commit mutates
                // anything (micro-decision #42: pre-step, not post-commit;
                // a handler write can change the trigger's own payload).
                if trigger.is_some()
                    && self.pre_view.is_none()
                    && txn.ops.iter().any(|op| matches!(op, StagedOp::Open { .. }))
                {
                    self.pre_view = Some(eval_view(self.p, &self.u, self.x)?);
                }
                match target {
                    ScopeTarget::Page(idx) => self.u.nav[*idx].state = txn.state,
                    ScopeTarget::Surface(idx) => self.u.surfaces[*idx].state = txn.state,
                }
                self.u.counters = txn.counters;
                self.u.pending.extend(txn.pending_add);
                self.c.extend(txn.commands);
                for op in txn.ops {
                    self.apply_op(op, &scope, trigger)?;
                }
                Ok(record)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_txn(
        &self,
        _def: &DefIr,
        scope: &str,
        entry_state: &BTreeMap<Ident, Value>,
        props: &BTreeMap<Ident, Value>,
        params: &BTreeMap<Ident, Value>,
        handler: &ir::HandlerIr,
        bindings: Vec<(Ident, Value)>,
    ) -> Result<TxnOutcome, EvalError> {
        let mut txn = Txn {
            state: entry_state.clone(),
            counters: self.u.counters,
            pending_add: Vec::new(),
            commands: Vec::new(),
            ops: Vec::new(),
            writes: Vec::new(),
            warnings: Vec::new(),
        };
        let mut bindings = bindings;

        for stmt in &handler.body {
            // Writes are visible sequentially WITHIN the handler: each
            // statement evaluates against the staged state (§4.2).
            let eval = |e: &ir::ExprIr, txn: &Txn, bindings: &[(Ident, Value)]| {
                eval_staged(
                    self.p, self.x, scope, &txn.state, props, params, bindings, e,
                )
            };
            let eval_args = |args: &[ir::ArgIr],
                             txn: &Txn,
                             bindings: &[(Ident, Value)]|
             -> Result<BTreeMap<Ident, Value>, Stop> {
                let mut out = BTreeMap::new();
                for arg in args {
                    out.insert(arg.name.clone(), eval(&arg.value, txn, bindings)?);
                }
                Ok(out)
            };

            let step: Result<(), Stop> = (|| {
                match stmt {
                    ir::StmtIr::Set { field, key, value } => {
                        let key_value = match key {
                            Some(k) => Some(eval(k, &txn, &bindings)?),
                            None => None,
                        };
                        let value = eval(value, &txn, &bindings)?;
                        match key_value {
                            None => {
                                txn.writes.push(serde_json::json!({
                                    "field": field.to_string(),
                                    "value": value.to_json(),
                                }));
                                txn.state.insert(field.clone(), value);
                            }
                            Some(key_value) => {
                                let key_str = map_key_string(&key_value).ok_or_else(|| {
                                    Stop::Internal(format!("non-identity map key {key_value:?}"))
                                })?;
                                let key_ident = Ident::new(&key_str)
                                    .map_err(|e| Stop::Internal(format!("map key: {e}")))?;
                                let Some(Value::Record(map)) = txn.state.get_mut(field) else {
                                    return Err(Stop::Internal(format!("`{field}` is not a map")));
                                };
                                txn.writes.push(serde_json::json!({
                                    "field": field.to_string(),
                                    "key": key_str,
                                    "value": value.to_json(),
                                }));
                                // `= none` removes the entry (§4.2).
                                if value == Value::None {
                                    map.remove(&key_ident);
                                } else {
                                    map.insert(key_ident, value);
                                }
                            }
                        }
                    }
                    ir::StmtIr::Send {
                        port,
                        command,
                        args,
                        bind,
                    } => {
                        let payload = Value::Record(eval_args(args, &txn, &bindings)?);
                        // Duplicate identical in-flight send → warning
                        // (§4.2: suppression is the author's guard's job;
                        // the machine only diagnoses).
                        let duplicate = self
                            .u
                            .pending
                            .values()
                            .chain(txn.pending_add.iter().map(|(_, p)| p))
                            .any(|p| {
                                p.port == *port && p.command == *command && p.payload == payload
                            });
                        if duplicate {
                            txn.warnings.push(Diagnostic::warning(
                                codes::DUPLICATE_IN_FLIGHT.0,
                                codes::DUPLICATE_IN_FLIGHT.1,
                                format!("`{command}` is already in flight with this payload"),
                                Span::new(FileId(0), 0, 0),
                            ));
                        }
                        let tag = txn.counters.mint_tag();
                        txn.commands.push(CommandEnvelope {
                            port: port.clone(),
                            command: command.clone(),
                            correlation: format!("c-{tag}"),
                            payload: payload.to_json(),
                        });
                        txn.pending_add.push((
                            tag,
                            PendingCommand {
                                port: port.clone(),
                                command: command.clone(),
                                payload,
                                origin: scope.to_string(),
                            },
                        ));
                        if let Some(bind) = bind {
                            bindings.push((bind.clone(), Value::Tag(tag)));
                        }
                    }
                    ir::StmtIr::OpenSurface { surface, args } => {
                        let props = eval_args(args, &txn, &bindings)?;
                        txn.ops.push(StagedOp::Open {
                            surface: surface.clone(),
                            props,
                        });
                    }
                    ir::StmtIr::Dismiss => txn.ops.push(StagedOp::Dismiss),
                    ir::StmtIr::Navigate { route, args } => {
                        let params = eval_args(args, &txn, &bindings)?;
                        txn.ops.push(StagedOp::Navigate {
                            route: route.clone(),
                            params,
                        });
                    }
                    ir::StmtIr::NavigateBack => txn.ops.push(StagedOp::Back),
                }
                Ok(())
            })();

            match step {
                Ok(()) => {}
                // Body-position not-ready: atomic abort — no writes, no
                // commands, counters rolled back (§4.2).
                Err(Stop::NotReady(_)) => return Ok(TxnOutcome::Aborted),
                Err(Stop::Internal(msg)) => return Err(EvalError(msg)),
            }
        }
        Ok(TxnOutcome::Committed(txn))
    }

    // ── structural ops (applied at dispatch end, §4.2) ──────────────────

    fn apply_op(
        &mut self,
        op: StagedOp,
        origin: &str,
        trigger: Option<&Descriptor>,
    ) -> Result<(), EvalError> {
        match op {
            StagedOp::Open { surface, props } => {
                // Idempotent per (definition, canonical context).
                if let Some(existing) = self
                    .u
                    .surfaces
                    .iter()
                    .find(|s| s.definition == surface && s.props == props)
                {
                    self.structural.push(serde_json::json!({
                        "op": "already-open",
                        "surface": format!("{}:{}", existing.definition, existing.serial),
                    }));
                    return Ok(());
                }
                let def = self
                    .p
                    .surfaces
                    .get(&surface)
                    .ok_or_else(|| EvalError(format!("no surface `{surface}`")))?;
                let restore_focus = match trigger {
                    Some(descriptor) => self.trigger_key_path(descriptor)?,
                    None => None,
                };
                let serial = self.u.counters.mint_surface();
                self.structural.push(serde_json::json!({
                    "op": "open-surface",
                    "surface": format!("{surface}:{serial}"),
                    "opener": origin,
                }));
                self.u.surfaces.push(SurfaceState {
                    serial,
                    definition: surface,
                    props,
                    state: initial_state(def),
                    opener: origin.to_string(),
                    restore_focus,
                });
                self.sweep_orphans();
            }
            StagedOp::Dismiss => {
                let (_, serial) = Machine::parse_scope(origin)
                    .filter(|(kind, _)| *kind == "surface")
                    .ok_or_else(|| {
                        EvalError(format!("`dismiss` from non-surface scope `{origin}`"))
                    })?;
                self.dismiss_surface(serial);
            }
            StagedOp::Navigate { route, params } => {
                let def = self
                    .p
                    .pages
                    .get(&route)
                    .ok_or_else(|| EvalError(format!("no page for route `{route}`")))?;
                let serial = self.u.counters.mint_page();
                self.structural.push(serde_json::json!({
                    "op": "navigate",
                    "route": route.to_string(),
                    "serial": serial,
                }));
                self.u.nav.push(NavEntry {
                    serial,
                    route: route.clone(),
                    params: params.clone(),
                    state: initial_state(def),
                });
                self.i.push(IntentEnvelope::HistoryPush { route, params });
            }
            StagedOp::Back => {
                if self.u.nav.len() < 2 {
                    self.structural
                        .push(serde_json::json!({ "op": "nav-underflow" }));
                    return Ok(());
                }
                let popped = self.u.nav.pop().expect("len checked");
                self.sweep_orphans();
                self.structural.push(serde_json::json!({
                    "op": "back",
                    "popped": popped.route.to_string(),
                    "to": self.u.nav.last().map(|e| e.route.to_string()),
                }));
                self.i.push(IntentEnvelope::HistoryBack);
            }
        }
        Ok(())
    }

    /// Pops a surface instance; FocusRestore intent when it was topmost
    /// (§4.2). Surfaces it opened force-close with it (the sweep).
    fn dismiss_surface(&mut self, serial: u64) {
        let Some(index) = self.u.surfaces.iter().position(|s| s.serial == serial) else {
            return; // scope-alive acceptance makes this unreachable
        };
        let was_top = index + 1 == self.u.surfaces.len();
        let removed = self.u.surfaces.remove(index);
        self.structural.push(serde_json::json!({
            "op": "dismiss",
            "surface": format!("{}:{}", removed.definition, removed.serial),
            "top": was_top,
        }));
        if was_top && let Some(key_path) = &removed.restore_focus {
            self.i.push(IntentEnvelope::FocusRestore {
                key_path: key_path.clone(),
            });
        }
        self.sweep_orphans();
    }

    fn scope_mounted(&self, scope: &str) -> bool {
        match Self::parse_scope(scope) {
            Some(("page", serial)) => self.u.nav.iter().any(|e| e.serial == serial),
            Some(("surface", serial)) => self.u.surfaces.iter().any(|s| s.serial == serial),
            _ => false,
        }
    }

    /// Re-establishes the surface-stack invariant after every structural
    /// op: a mounted surface's opener scope is alive. This is the §7.4
    /// force-close cascade — and it also catches a surface opened AFTER
    /// its opener died within the same dispatch (dismiss-then-open).
    fn sweep_orphans(&mut self) {
        loop {
            let Some(index) = self
                .u
                .surfaces
                .iter()
                .position(|s| !self.scope_mounted(&s.opener))
            else {
                return;
            };
            let removed = self.u.surfaces.remove(index);
            self.structural.push(serde_json::json!({
                "op": "force-close",
                "surface": format!("{}:{}", removed.definition, removed.serial),
            }));
        }
    }

    // ── focus-restore node search ───────────────────────────────────────

    /// Finds the key-path of the node that carried the dispatched
    /// descriptor, searching the PRE-commit view (micro-decision #42:
    /// descriptors match by (emit, scope, payload) — their semantic
    /// identity; the commit's own writes must not move the target).
    fn trigger_key_path(&mut self, descriptor: &Descriptor) -> Result<Option<String>, EvalError> {
        let Some(snapshot) = self.pre_view.as_ref() else {
            return Ok(None); // no Ui trigger captured a view
        };
        let (kind, serial) = match Self::parse_scope(&descriptor.scope) {
            Some(parsed) => parsed,
            None => return Ok(None),
        };
        let root = match kind {
            "page" => &snapshot.page.root,
            "surface" => {
                let suffix = format!(":{serial}");
                match snapshot.surfaces.iter().find(|s| s.key.ends_with(&suffix)) {
                    Some(s) => &s.root,
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        Ok(find_descriptor(root, descriptor, &descriptor.scope))
    }
}

fn find_descriptor(node: &Node, descriptor: &Descriptor, prefix: &str) -> Option<String> {
    let path = format!("{prefix}/{}", node.key);
    let matches = node.on.iter().any(|d| {
        d.emit == descriptor.emit && d.scope == descriptor.scope && d.payload == descriptor.payload
    });
    if matches {
        return Some(path);
    }
    node.children
        .iter()
        .find_map(|child| find_descriptor(child, descriptor, &path))
}

// ── binding & payload typing ────────────────────────────────────────────────

enum BindSpec<'a> {
    /// Semantic events bind by name from the decoded payload.
    Named(&'a BTreeMap<Ident, Value>),
    /// Outcome handlers bind positionally: `(tag, cmd[, refusal])` —
    /// fixed shapes, author-chosen names (§4.2).
    Positional(&'a [Value]),
}

impl BindSpec<'_> {
    fn bind(&self, params: &[Ident]) -> Vec<(Ident, Value)> {
        match self {
            BindSpec::Named(values) => params
                .iter()
                .map(|p| (p.clone(), values.get(p).cloned().unwrap_or(Value::None)))
                .collect(),
            BindSpec::Positional(values) => params
                .iter()
                .zip(values.iter())
                .map(|(p, v)| (p.clone(), v.clone()))
                .collect(),
        }
    }
}

/// The eligibility check + payload typing: (payload ∪ carried data)
/// fields must equal the declared signature exactly, each value decoding
/// against its declared type (§7.2).
fn bind_payload(
    sig: &[ir::EventParamIr],
    payload: &serde_json::Value,
    data: Option<&Value>,
) -> Result<BTreeMap<Ident, Value>, String> {
    let empty = serde_json::Map::new();
    let payload = match payload {
        serde_json::Value::Object(map) => map,
        serde_json::Value::Null => &empty,
        other => return Err(format!("payload must be an object, got {other}")),
    };
    let empty_data = BTreeMap::new();
    let data = match data {
        None => &empty_data,
        Some(Value::Record(fields)) => fields,
        Some(other) => return Err(format!("carried data must be a record, got {other:?}")),
    };

    let mut out = BTreeMap::new();
    for param in sig {
        if let Some(json) = payload.get(param.name.as_str()) {
            let value = decode_value(json, &param.ty)
                .map_err(|e| format!("payload field `{}`: {e}", param.name))?;
            out.insert(param.name.clone(), value);
        } else if let Some(value) = data.get(&param.name) {
            out.insert(param.name.clone(), value.clone());
        } else if matches!(param.ty, ir::TyIr::Option(_)) {
            out.insert(param.name.clone(), Value::None);
        } else {
            return Err(format!("payload is missing field `{}`", param.name));
        }
    }
    for key in payload.keys() {
        if !sig.iter().any(|p| p.name.as_str() == key) {
            return Err(format!("payload field `{key}` is not in the signature"));
        }
    }
    for key in data.keys() {
        if !sig.iter().any(|p| p.name == *key) {
            return Err(format!("carried field `{key}` is not in the signature"));
        }
    }
    Ok(out)
}

/// Expression evaluation against staged transaction state — the shared
/// core of both dispatchers.
#[allow(clippy::too_many_arguments)]
fn eval_staged(
    p: &ProgramIr,
    x: &Projections,
    scope: &str,
    state: &BTreeMap<Ident, Value>,
    props: &BTreeMap<Ident, Value>,
    params: &BTreeMap<Ident, Value>,
    bindings: &[(Ident, Value)],
    e: &ir::ExprIr,
) -> Result<Value, Stop> {
    let frame = Frame {
        program: p,
        x,
        scope: scope.to_string(),
        state,
        props: props.clone(),
        params: params.clone(),
        bindings: bindings.to_vec(),
        emits: EmitEnv::Machine,
    };
    frame.eval_expr(e)
}

// ── the transaction value ───────────────────────────────────────────────────

struct Txn {
    state: BTreeMap<Ident, Value>,
    counters: Counters,
    pending_add: Vec<(u64, PendingCommand)>,
    commands: Vec<CommandEnvelope>,
    ops: Vec<StagedOp>,
    writes: Vec<serde_json::Value>,
    /// Runtime diagnostics (`G`) minted during the transaction.
    warnings: Vec<Diagnostic>,
}

enum TxnOutcome {
    Committed(Txn),
    Aborted,
}

enum StagedOp {
    Open {
        surface: Ident,
        props: BTreeMap<Ident, Value>,
    },
    Dismiss,
    Navigate {
        route: Ident,
        params: BTreeMap<Ident, Value>,
    },
    Back,
}

// ── fragment replay (derived surface/component examples, §6.2) ──────────────

/// A machine-less dispatch context for standalone fragment previews: the
/// same guard selection and transaction semantics as `step_u`, minus
/// structure — a structural statement in a fragment replay has nowhere to
/// act and surfaces as an error the checker turns into "pin this state
/// instead".
#[derive(Clone, Debug, Default)]
pub struct FragmentMachine {
    pub state: BTreeMap<Ident, Value>,
    pub pending: BTreeMap<u64, PendingCommand>,
    pub counters: Counters,
}

/// What one fragment dispatch did (the caller owns trace presentation).
#[derive(Clone, Debug)]
pub enum FragmentNote {
    Committed { commands: Vec<CommandEnvelope> },
    NoHandler,
    NotReady,
}

pub const FRAGMENT_SCOPE: &str = "fragment:0";

#[derive(Clone, Debug)]
pub enum FragmentError {
    /// The handler ran a structural statement — not replayable standalone.
    Structural(&'static str),
    Invariant(String),
}

impl std::fmt::Display for FragmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FragmentError::Structural(stmt) => {
                write!(f, "`{stmt}` needs a mounted machine")
            }
            FragmentError::Invariant(msg) => write!(f, "{msg}"),
        }
    }
}

impl FragmentMachine {
    pub fn from_state(state: BTreeMap<Ident, Value>) -> FragmentMachine {
        FragmentMachine {
            state,
            pending: BTreeMap::new(),
            counters: Counters::default(),
        }
    }

    /// Dispatches a semantic event with an already-typed payload.
    pub fn dispatch_semantic(
        &mut self,
        p: &ProgramIr,
        def: &DefIr,
        props: &BTreeMap<Ident, Value>,
        x: &Projections,
        event: &Ident,
        payload: &BTreeMap<Ident, Value>,
    ) -> Result<FragmentNote, FragmentError> {
        self.dispatch(
            p,
            def,
            props,
            x,
            |h| matches!(&h.on, ir::EventKeyIr::Semantic { event: e } if e == event),
            &BindSpec::Named(payload),
        )
    }

    /// Settles a pending command (the oldest matching one is the CALLER's
    /// choice — it holds the correlation rule, §6.2).
    pub fn dispatch_outcome(
        &mut self,
        p: &ProgramIr,
        def: &DefIr,
        props: &BTreeMap<Ident, Value>,
        x: &Projections,
        tag: u64,
        result: &OutcomeResult,
    ) -> Result<FragmentNote, FragmentError> {
        let Some(pending) = self.pending.remove(&tag) else {
            return Err(FragmentError::Invariant(format!(
                "no pending command t-{tag}"
            )));
        };
        let (which, refusal) = match result {
            OutcomeResult::Ok => (ir::OutcomeKindIr::Ok, None),
            OutcomeResult::Refused { refusal } => {
                (ir::OutcomeKindIr::Err, Some(refusal.to_string()))
            }
            OutcomeResult::Unavailable { .. } => {
                (ir::OutcomeKindIr::Err, Some("unavailable".to_string()))
            }
        };
        let mut positional = vec![Value::Tag(tag), pending.payload.clone()];
        if let Some(refusal) = &refusal {
            positional.push(Value::Text(refusal.clone()));
        }
        self.dispatch(
            p,
            def,
            props,
            x,
            |h| {
                matches!(&h.on, ir::EventKeyIr::Outcome { command, which: w }
                    if *command == pending.command && *w == which)
            },
            &BindSpec::Positional(&positional),
        )
    }

    fn dispatch(
        &mut self,
        p: &ProgramIr,
        def: &DefIr,
        props: &BTreeMap<Ident, Value>,
        x: &Projections,
        matches_handler: impl Fn(&ir::HandlerIr) -> bool,
        bind: &BindSpec<'_>,
    ) -> Result<FragmentNote, FragmentError> {
        let params = BTreeMap::new();
        let mut selected = None;
        for handler in def.handlers.iter().filter(|h| matches_handler(h)) {
            let bindings = bind.bind(&handler.params);
            let satisfied = match &handler.guard {
                None => true,
                Some(guard) => {
                    let frame = Frame {
                        program: p,
                        x,
                        scope: FRAGMENT_SCOPE.to_string(),
                        state: &self.state,
                        props: props.clone(),
                        params: params.clone(),
                        bindings: bindings.clone(),
                        emits: EmitEnv::Machine,
                    };
                    match frame.eval_expr(guard) {
                        Ok(Value::Bool(b)) => b,
                        Ok(other) => {
                            return Err(FragmentError::Invariant(format!("guard was {other:?}")));
                        }
                        Err(Stop::NotReady(_)) => false,
                        Err(Stop::Internal(msg)) => return Err(FragmentError::Invariant(msg)),
                    }
                }
            };
            if satisfied {
                selected = Some((handler, bindings));
                break;
            }
        }
        let Some((handler, mut bindings)) = selected else {
            return Ok(FragmentNote::NoHandler);
        };

        // The same staged transaction as `step_u`, structural ops refused.
        let mut state = self.state.clone();
        let mut counters = self.counters;
        let mut pending_add = Vec::new();
        let mut commands = Vec::new();
        for stmt in &handler.body {
            let eval =
                |e: &ir::ExprIr, state: &BTreeMap<Ident, Value>, bindings: &[(Ident, Value)]| {
                    eval_staged(p, x, FRAGMENT_SCOPE, state, props, &params, bindings, e)
                };
            let result: Result<(), Stop> = (|| match stmt {
                ir::StmtIr::Set { field, key, value } => {
                    let key_value = match key {
                        Some(k) => Some(eval(k, &state, &bindings)?),
                        None => None,
                    };
                    let value = eval(value, &state, &bindings)?;
                    match key_value {
                        None => {
                            state.insert(field.clone(), value);
                        }
                        Some(key_value) => {
                            let key_str = map_key_string(&key_value).ok_or_else(|| {
                                Stop::Internal("non-identity map key".to_string())
                            })?;
                            let key_ident = Ident::new(&key_str)
                                .map_err(|e| Stop::Internal(format!("map key: {e}")))?;
                            let Some(Value::Record(map)) = state.get_mut(field) else {
                                return Err(Stop::Internal(format!("`{field}` is not a map")));
                            };
                            if value == Value::None {
                                map.remove(&key_ident);
                            } else {
                                map.insert(key_ident, value);
                            }
                        }
                    }
                    Ok(())
                }
                ir::StmtIr::Send {
                    port,
                    command,
                    args,
                    bind,
                } => {
                    let mut payload = BTreeMap::new();
                    for arg in args {
                        payload.insert(arg.name.clone(), eval(&arg.value, &state, &bindings)?);
                    }
                    let payload = Value::Record(payload);
                    let tag = counters.mint_tag();
                    commands.push(CommandEnvelope {
                        port: port.clone(),
                        command: command.clone(),
                        correlation: format!("c-{tag}"),
                        payload: payload.to_json(),
                    });
                    pending_add.push((
                        tag,
                        PendingCommand {
                            port: port.clone(),
                            command: command.clone(),
                            payload,
                            origin: FRAGMENT_SCOPE.to_string(),
                        },
                    ));
                    if let Some(bind) = bind {
                        bindings.push((bind.clone(), Value::Tag(tag)));
                    }
                    Ok(())
                }
                ir::StmtIr::OpenSurface { .. } => Err(Stop::Internal("open-surface".to_string())),
                ir::StmtIr::Dismiss => Err(Stop::Internal("dismiss".to_string())),
                ir::StmtIr::Navigate { .. } => Err(Stop::Internal("navigate".to_string())),
                ir::StmtIr::NavigateBack => Err(Stop::Internal("navigate back".to_string())),
            })();
            match result {
                Ok(()) => {}
                Err(Stop::NotReady(_)) => return Ok(FragmentNote::NotReady),
                Err(Stop::Internal(msg))
                    if matches!(
                        stmt,
                        ir::StmtIr::OpenSurface { .. }
                            | ir::StmtIr::Dismiss
                            | ir::StmtIr::Navigate { .. }
                            | ir::StmtIr::NavigateBack
                    ) =>
                {
                    let stmt_name: &'static str = match stmt {
                        ir::StmtIr::OpenSurface { .. } => "open-surface",
                        ir::StmtIr::Dismiss => "dismiss",
                        ir::StmtIr::Navigate { .. } => "navigate",
                        ir::StmtIr::NavigateBack => "navigate back",
                        _ => unreachable!(),
                    };
                    let _ = msg;
                    return Err(FragmentError::Structural(stmt_name));
                }
                Err(Stop::Internal(msg)) => return Err(FragmentError::Invariant(msg)),
            }
        }

        self.state = state;
        self.counters = counters;
        self.pending.extend(pending_add);
        Ok(FragmentNote::Committed { commands })
    }
}
