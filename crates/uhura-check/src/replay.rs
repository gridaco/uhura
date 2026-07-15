//! Derived-example replay (design §6.2): a left fold of the PUBLIC machine
//! — `step_u` + `apply_updates` for pages, the same transactional
//! dispatcher via `FragmentMachine` for standalone surfaces — over the
//! example's timeline. Replay is a build/check phase: each derived example
//! resolves to a frozen `(route, U, X, surface stack)` snapshot;
//! Editor-model construction executes zero transitions.
//!
//! This is what makes derived examples SELF-VERIFYING: change a guard and
//! the timeline's event drops (`no-handler`), which is a check error here.
//!
//! Rules (§6.2): an injected outcome settles the OLDEST unsettled matching
//! command in the replay prefix — no unsettled match is "pin this state
//! instead"; leaving the subject route at any step is an error; failures
//! attribute to the first failing step (the caller handles ancestor
//! blocking).

use std::collections::BTreeMap;

use uhura_base::{Ident, Span, Value};
use uhura_core::event::{Event, apply_failure, apply_updates};
use uhura_core::state::{Projections, UiState, initial_state};
use uhura_core::step::{FragmentError, FragmentMachine, FragmentNote, step_u};
use uhura_core::trace::{DispatchRecord, Disposition, GuardResult, StepTrace};
use uhura_core::view::{Descriptor, DescriptorKind};
use uhura_port::envelope::{CommandEnvelope, OutcomeResult, ProjectionUpdate};
use uhura_syntax::ast;

use crate::fixture::{FixtureData, decode_against_ty};
use crate::preview::PreviewPayload;
use crate::resolve::{DefEnv, Resolved, SubjectKind};

pub struct ReplayOutcome {
    pub payload: PreviewPayload,
    /// Unsettled commands at the end of the timeline — the caption's
    /// "N command(s) in flight" (§6.2).
    pub in_flight: usize,
    /// One runtime-backed record per authored timeline event, including
    /// event payload, handler/guard selection, and committed effects.
    pub steps: Vec<ReplayStep>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayStepKind {
    Semantic,
    Outcome,
    Projection,
}

impl ReplayStepKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayStepKind::Semantic => "semantic",
            ReplayStepKind::Outcome => "outcome",
            ReplayStepKind::Projection => "projection",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayGuard {
    pub handler: usize,
    pub result: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayDispatch {
    pub scope: String,
    pub definition: String,
    pub on: String,
    pub guards: Vec<ReplayGuard>,
    pub selected: Option<usize>,
    pub aborted: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayEffects {
    pub writes: Vec<serde_json::Value>,
    pub commands: Vec<serde_json::Value>,
    pub intents: Vec<serde_json::Value>,
    pub structural: Vec<serde_json::Value>,
    pub projections: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayStep {
    pub label: String,
    pub kind: ReplayStepKind,
    pub payload: serde_json::Value,
    pub dispatch: Option<ReplayDispatch>,
    pub effects: ReplayEffects,
}

impl ReplayStep {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "label": self.label,
            "kind": self.kind.as_str(),
            "payload": self.payload,
            "dispatch": self.dispatch.as_ref().map(|dispatch| serde_json::json!({
                "scope": dispatch.scope,
                "definition": dispatch.definition,
                "on": dispatch.on,
                "guards": dispatch.guards.iter().map(|guard| serde_json::json!({
                    "handler": guard.handler,
                    "result": guard.result,
                })).collect::<Vec<_>>(),
                "selected": dispatch.selected,
                "aborted": dispatch.aborted,
            })),
            "effects": {
                "writes": self.effects.writes,
                "commands": self.effects.commands,
                "intents": self.effects.intents,
                "structural": self.effects.structural,
                "projections": self.effects.projections,
            },
        })
    }
}

pub(crate) fn projection_step_label(port: &str, projection: &str) -> String {
    format!("projection {port}.{projection}")
}

pub(crate) fn outcome_step_label(command: &str, which: &ast::OutcomeKind) -> String {
    format!(
        "{command}.{}",
        match which {
            ast::OutcomeKind::Ok => "ok",
            ast::OutcomeKind::Err => "err",
        }
    )
}

fn replay_dispatch(record: &DispatchRecord) -> ReplayDispatch {
    ReplayDispatch {
        scope: record.scope.clone(),
        definition: record.definition.to_string(),
        on: record.on.clone(),
        guards: record
            .guards
            .iter()
            .map(|guard| ReplayGuard {
                handler: guard.handler,
                result: match guard.result {
                    GuardResult::Satisfied => "satisfied",
                    GuardResult::Unsatisfied => "unsatisfied",
                    GuardResult::NotReady => "not-ready",
                },
            })
            .collect(),
        selected: record.selected,
        aborted: record.aborted.clone(),
    }
}

fn replay_step_from_trace(
    label: String,
    kind: ReplayStepKind,
    payload: serde_json::Value,
    trace: &StepTrace,
    projections: Vec<serde_json::Value>,
) -> ReplayStep {
    let (dispatch, writes) = match &trace.disposition {
        Disposition::Dispatched(record) => (Some(replay_dispatch(record)), record.writes.clone()),
        _ => (None, Vec::new()),
    };
    ReplayStep {
        label,
        kind,
        payload,
        dispatch,
        effects: ReplayEffects {
            writes,
            commands: trace.c.clone(),
            intents: trace.i.clone(),
            structural: trace.structural.clone(),
            projections,
        },
    }
}

fn command_json(command: &CommandEnvelope) -> serde_json::Value {
    serde_json::json!({
        "kind": "command",
        "port": command.port.to_string(),
        "command": command.command.to_string(),
        "correlation": command.correlation,
        "payload": command.payload,
    })
}

fn replay_step_from_fragment(
    label: String,
    kind: ReplayStepKind,
    payload: serde_json::Value,
    dispatch: &DispatchRecord,
    commands: &[CommandEnvelope],
) -> ReplayStep {
    ReplayStep {
        label,
        kind,
        payload,
        dispatch: Some(replay_dispatch(dispatch)),
        effects: ReplayEffects {
            writes: dispatch.writes.clone(),
            commands: commands.iter().map(command_json).collect(),
            ..ReplayEffects::default()
        },
    }
}

/// Attributes to the first failing step (§6.2).
pub struct ReplayError {
    pub span: Span,
    pub message: String,
}

fn fail<T>(span: Span, message: impl Into<String>) -> Result<T, ReplayError> {
    Err(ReplayError {
        span,
        message: message.into(),
    })
}

pub struct ReplayInput<'a> {
    pub x: Projections,
    pub params: BTreeMap<Ident, Value>,
    pub props: BTreeMap<Ident, Value>,
    pub state_pins: BTreeMap<Ident, Value>,
    pub events: &'a [&'a ast::ExampleEvent],
    /// The whole example, for errors with no better anchor.
    pub span: Span,
}

pub fn replay(
    program: &uhura_core::ir::ProgramIr,
    resolved: &Resolved,
    env: &DefEnv,
    fixture: &FixtureData,
    input: ReplayInput<'_>,
) -> Result<ReplayOutcome, ReplayError> {
    match &env.kind {
        SubjectKind::Page { route } => replay_page(program, resolved, fixture, route, input),
        SubjectKind::Surface { name, .. } => {
            replay_fragment(program, resolved, env, fixture, name, true, input)
        }
        SubjectKind::Component { name } => {
            replay_fragment(program, resolved, env, fixture, name, false, input)
        }
    }
}

// ── page timelines: the real machine, boot to finish ───────────────────────

fn replay_page(
    program: &uhura_core::ir::ProgramIr,
    resolved: &Resolved,
    fixture: &FixtureData,
    route: &Ident,
    input: ReplayInput<'_>,
) -> Result<ReplayOutcome, ReplayError> {
    let mut x = input.x;
    let mut revisions = RevisionCounter::default();
    let mut steps = Vec::new();

    let boot = step_u(
        program,
        UiState::boot(),
        &x,
        Event::Init {
            route: route.clone(),
            params: input.params,
        },
    );
    let mut u = match boot {
        Ok(result) => result.u,
        Err(e) => return fail(input.span, e.to_string()),
    };
    // Pins apply before the fold (§6.2 evaluation order); state pins land
    // on the mounted page.
    if let Some(entry) = u.nav.last_mut() {
        entry.state.extend(input.state_pins);
    }

    for event in input.events {
        match event {
            ast::ExampleEvent::Semantic { name, args, span } => {
                let Ok(emit) = Ident::new(name) else {
                    return fail(*span, format!("`{name}` is not a legal event name"));
                };
                let mut payload = serde_json::Map::new();
                for arg in args {
                    let json = static_arg(&arg.value, fixture, *span)?;
                    payload.insert(arg.name.clone(), json);
                }
                let replay_payload = serde_json::Value::Object(payload.clone());
                let scope = match u.nav.last() {
                    Some(entry) => format!("page:{}", entry.serial),
                    None => return fail(*span, "the machine has no mounted page"),
                };
                let descriptor = Descriptor {
                    kind: DescriptorKind::Input,
                    event: emit.clone(),
                    emit,
                    scope,
                    payload: serde_json::Value::Object(payload),
                    carries: BTreeMap::new(),
                };
                let view_rev = u.rev;
                let result = step_u(
                    program,
                    u,
                    &x,
                    Event::Ui {
                        descriptor,
                        data: None,
                        view_rev,
                    },
                );
                u = match result {
                    Ok(result) => {
                        inspect_disposition(name, &result.t.disposition, *span)?;
                        steps.push(replay_step_from_trace(
                            name.clone(),
                            ReplayStepKind::Semantic,
                            replay_payload,
                            &result.t,
                            Vec::new(),
                        ));
                        result.u
                    }
                    Err(e) => return fail(*span, e.to_string()),
                };
                check_on_subject(&u, route, *span)?;
            }
            ast::ExampleEvent::Projection(pin) => {
                if let Some(reason) = failed_delivery(pin, fixture)? {
                    let (port, projection, key) = pin_target(pin, resolved, fixture)?;
                    if let Err(e) =
                        apply_failure(program, &mut x, &port, &projection, key.as_ref(), &reason)
                    {
                        return fail(pin.span, e);
                    }
                    let projection_effect = serde_json::json!({
                        "port": port.to_string(),
                        "projection": projection.to_string(),
                        "key": key,
                        "failed": reason,
                    });
                    u = match step_u(
                        program,
                        u,
                        &x,
                        Event::ProjectionFailed {
                            port: port.clone(),
                            projection: projection.clone(),
                            key: key.clone(),
                            reason: reason.clone(),
                        },
                    ) {
                        Ok(result) => {
                            steps.push(replay_step_from_trace(
                                projection_step_label(&pin.port, &pin.projection),
                                ReplayStepKind::Projection,
                                projection_effect.clone(),
                                &result.t,
                                vec![projection_effect],
                            ));
                            result.u
                        }
                        Err(e) => return fail(pin.span, e.to_string()),
                    };
                    continue;
                }
                let update = pin_update(pin, resolved, fixture, &mut revisions, &x)?;
                if let Err(e) = apply_updates(program, &mut x, std::slice::from_ref(&update)) {
                    return fail(pin.span, e);
                }
                let projection_effect = update.to_json();
                u = match step_u(
                    program,
                    u,
                    &x,
                    Event::Projection {
                        updates: vec![update],
                    },
                ) {
                    Ok(result) => {
                        steps.push(replay_step_from_trace(
                            projection_step_label(&pin.port, &pin.projection),
                            ReplayStepKind::Projection,
                            projection_effect.clone(),
                            &result.t,
                            vec![projection_effect],
                        ));
                        result.u
                    }
                    Err(e) => return fail(pin.span, e.to_string()),
                };
            }
            ast::ExampleEvent::Outcome {
                command,
                which,
                args,
                span,
            } => {
                let Ok(command) = Ident::new(command) else {
                    return fail(*span, format!("`{command}` is not a legal command name"));
                };
                // The OLDEST unsettled matching command (§6.2).
                let tag = u
                    .pending
                    .iter()
                    .find(|(_, p)| p.command == command)
                    .map(|(t, _)| *t);
                let Some(tag) = tag else {
                    return fail(
                        *span,
                        format!(
                            "no unsettled `{command}` command in the replay prefix — \
                             pin this state instead (§6.2)"
                        ),
                    );
                };
                let result_kind = outcome_result(which, args, *span)?;
                let outcome_name = outcome_step_label(command.as_str(), which);
                let outcome_label = format!("{command} outcome");
                let replay_payload = result_kind.to_json();
                let result = step_u(
                    program,
                    u,
                    &x,
                    Event::Outcome {
                        correlation: format!("c-{tag}"),
                        result: result_kind,
                        updates: vec![],
                    },
                );
                u = match result {
                    Ok(result) => {
                        inspect_disposition(&outcome_label, &result.t.disposition, *span)?;
                        steps.push(replay_step_from_trace(
                            outcome_name,
                            ReplayStepKind::Outcome,
                            replay_payload,
                            &result.t,
                            Vec::new(),
                        ));
                        result.u
                    }
                    Err(e) => return fail(*span, e.to_string()),
                };
                // Outcome handlers can navigate too (§6.2's rule is
                // per-step, not per-event-kind).
                check_on_subject(&u, route, *span)?;
            }
        }
    }

    let in_flight = u.pending.len();
    Ok(ReplayOutcome {
        payload: PreviewPayload::Page {
            route: route.clone(),
            u,
            x,
        },
        in_flight,
        steps,
    })
}

/// Leaving the subject route at any step is a check error (§6.2).
fn check_on_subject(u: &UiState, route: &Ident, span: Span) -> Result<(), ReplayError> {
    let on_subject = u.nav.len() == 1 && u.nav.last().is_some_and(|e| e.route == *route);
    if on_subject {
        Ok(())
    } else {
        fail(
            span,
            format!(
                "this step leaves the subject route `{route}` — an example \
                 previews one subject (§6.2)"
            ),
        )
    }
}

/// A timeline step that dropped or aborted is an authoring error — the
/// example claims a transition the machine refuses.
fn inspect_disposition(
    label: &str,
    disposition: &Disposition,
    span: Span,
) -> Result<(), ReplayError> {
    match disposition {
        Disposition::Dropped { reason, detail } => fail(
            span,
            format!(
                "the machine drops `{label}` here ({}): {detail}",
                reason.as_str()
            ),
        ),
        Disposition::Dispatched(record) if record.aborted.is_some() => fail(
            span,
            format!(
                "`{label}` aborts on an undelivered projection — deliver or pin it \
                 before this step (§4.2)"
            ),
        ),
        Disposition::Dispatched(record) if record.selected.is_none() => fail(
            span,
            format!(
                "no handler's guard accepts `{label}` at this point in the timeline — \
                 the machine drops it"
            ),
        ),
        _ => Ok(()),
    }
}

// ── fragment timelines: surfaces (and components) standalone ───────────────

fn replay_fragment(
    program: &uhura_core::ir::ProgramIr,
    resolved: &Resolved,
    env: &DefEnv,
    fixture: &FixtureData,
    name: &Ident,
    surface: bool,
    input: ReplayInput<'_>,
) -> Result<ReplayOutcome, ReplayError> {
    let def = if surface {
        program.surfaces.get(name)
    } else {
        program.components.get(name)
    };
    let Some(def) = def else {
        return fail(input.span, format!("no definition `{name}`"));
    };

    let mut x = input.x;
    let mut revisions = RevisionCounter::default();
    let mut state = initial_state(def);
    state.extend(input.state_pins);
    let mut machine = FragmentMachine::from_state(name.clone(), state);
    let mut steps = Vec::new();

    for event in input.events {
        match event {
            ast::ExampleEvent::Semantic {
                name: event_name,
                args,
                span,
            } => {
                let Ok(event_ident) = Ident::new(event_name) else {
                    return fail(*span, format!("`{event_name}` is not a legal event name"));
                };
                let Some(sig) = env.events.get(&event_ident) else {
                    return fail(*span, format!("`{name}` declares no event `{event_name}`"));
                };
                // Timeline args are static; each decodes against the
                // signature's declared type (L8 at use).
                let mut payload = BTreeMap::new();
                for arg in args {
                    let Some((_, ty)) = sig.iter().find(|(p, _)| p.as_str() == arg.name) else {
                        return fail(
                            *span,
                            format!("`{event_name}` has no parameter `{}`", arg.name),
                        );
                    };
                    let json = static_arg(&arg.value, fixture, *span)?;
                    match decode_against_ty(&json, ty) {
                        Ok(value) => {
                            payload.insert(Ident::new(&arg.name).expect("checked"), value);
                        }
                        Err(e) => return fail(*span, format!("`{}`: {e}", arg.name)),
                    }
                }
                for (param, ty) in sig {
                    if !payload.contains_key(param) && !matches!(ty, crate::types::Ty::Option(_)) {
                        return fail(*span, format!("`{event_name}` needs `{param}` here"));
                    }
                }
                let replay_payload = serde_json::Value::Object(
                    payload
                        .iter()
                        .map(|(key, value)| (key.to_string(), value.to_json()))
                        .collect(),
                );
                let note = machine.dispatch_semantic(
                    program,
                    def,
                    &input.props,
                    &x,
                    &event_ident,
                    &payload,
                );
                let (dispatch, commands) = accept_fragment_note(event_name, note, *span)?;
                steps.push(replay_step_from_fragment(
                    event_name.clone(),
                    ReplayStepKind::Semantic,
                    replay_payload,
                    &dispatch,
                    &commands,
                ));
            }
            ast::ExampleEvent::Projection(pin) => {
                if let Some(reason) = failed_delivery(pin, fixture)? {
                    let (port, projection, key) = pin_target(pin, resolved, fixture)?;
                    if let Err(e) =
                        apply_failure(program, &mut x, &port, &projection, key.as_ref(), &reason)
                    {
                        return fail(pin.span, e);
                    }
                    let effect = serde_json::json!({
                        "port": port.to_string(),
                        "projection": projection.to_string(),
                        "key": key,
                        "failed": reason,
                    });
                    steps.push(ReplayStep {
                        label: projection_step_label(&pin.port, &pin.projection),
                        kind: ReplayStepKind::Projection,
                        payload: effect.clone(),
                        dispatch: None,
                        effects: ReplayEffects {
                            projections: vec![effect],
                            ..ReplayEffects::default()
                        },
                    });
                    continue;
                }
                let update = pin_update(pin, resolved, fixture, &mut revisions, &x)?;
                if let Err(e) = apply_updates(program, &mut x, std::slice::from_ref(&update)) {
                    return fail(pin.span, e);
                }
                let effect = update.to_json();
                steps.push(ReplayStep {
                    label: projection_step_label(&pin.port, &pin.projection),
                    kind: ReplayStepKind::Projection,
                    payload: effect.clone(),
                    dispatch: None,
                    effects: ReplayEffects {
                        projections: vec![effect],
                        ..ReplayEffects::default()
                    },
                });
            }
            ast::ExampleEvent::Outcome {
                command,
                which,
                args,
                span,
            } => {
                let Ok(command_ident) = Ident::new(command) else {
                    return fail(*span, format!("`{command}` is not a legal command name"));
                };
                let tag = machine
                    .pending
                    .iter()
                    .find(|(_, p)| p.command == command_ident)
                    .map(|(t, _)| *t);
                let Some(tag) = tag else {
                    return fail(
                        *span,
                        format!(
                            "no unsettled `{command}` command in the replay prefix — \
                             pin this state instead (§6.2)"
                        ),
                    );
                };
                let result = outcome_result(which, args, *span)?;
                let replay_payload = result.to_json();
                let outcome_name = outcome_step_label(command, which);
                let note = machine.dispatch_outcome(program, def, &input.props, &x, tag, &result);
                let (dispatch, commands) =
                    accept_fragment_note(&format!("{command} outcome"), note, *span)?;
                steps.push(replay_step_from_fragment(
                    outcome_name,
                    ReplayStepKind::Outcome,
                    replay_payload,
                    &dispatch,
                    &commands,
                ));
            }
        }
    }

    let in_flight = machine.pending.len();
    Ok(ReplayOutcome {
        payload: PreviewPayload::Fragment {
            surface,
            name: name.clone(),
            props: input.props,
            state: machine.state,
            x,
        },
        in_flight,
        steps,
    })
}

fn accept_fragment_note(
    label: &str,
    note: Result<FragmentNote, FragmentError>,
    span: Span,
) -> Result<(DispatchRecord, Vec<CommandEnvelope>), ReplayError> {
    match note {
        Ok(FragmentNote::Committed { dispatch, commands }) => Ok((dispatch, commands)),
        Ok(FragmentNote::NoHandler { .. }) => fail(
            span,
            format!(
                "no handler's guard accepts `{label}` at this point in the timeline — \
                 the machine drops it"
            ),
        ),
        Ok(FragmentNote::NotReady { .. }) => fail(
            span,
            format!(
                "`{label}` aborts on an undelivered projection — deliver or pin it \
                 before this step (§4.2)"
            ),
        ),
        Err(FragmentError::Structural(stmt)) => fail(
            span,
            format!(
                "`{label}` runs `{stmt}`, which needs a mounted machine — \
                 pin this state instead (§6.2)"
            ),
        ),
        Err(FragmentError::Invariant(msg)) => fail(span, msg),
    }
}

// ── shared pieces ───────────────────────────────────────────────────────────

/// Timeline projection deliveries mint strictly-increasing revisions per
/// instance, above the pins' revision 1 (mirrors the fixture driver).
#[derive(Default)]
struct RevisionCounter {
    next: BTreeMap<(String, String), u64>,
}

impl RevisionCounter {
    fn mint(&mut self, projection: &str, key: &Option<serde_json::Value>) -> u64 {
        let key_str = key.as_ref().map(ToString::to_string).unwrap_or_default();
        let counter = self
            .next
            .entry((projection.to_string(), key_str))
            .or_insert(1);
        *counter += 1;
        *counter
    }
}

/// `failed("<reason>")` in a timeline is a projection-failed delivery
/// (§9.3), mirroring the pin form (micro-decision #31).
fn failed_delivery(
    pin: &ast::ProjectionPin,
    fixture: &FixtureData,
) -> Result<Option<String>, ReplayError> {
    let ast::ExprKind::Call { name, args } = &pin.value.kind else {
        return Ok(None);
    };
    if name != "failed" {
        return Ok(None);
    }
    match args.as_slice() {
        [reason] => match static_arg(reason, fixture, pin.span)? {
            serde_json::Value::String(reason) => Ok(Some(reason)),
            _ => fail(pin.span, "`failed(…)` takes one reason string"),
        },
        _ => fail(pin.span, "`failed(…)` takes one reason string"),
    }
}

fn pin_target(
    pin: &ast::ProjectionPin,
    resolved: &Resolved,
    fixture: &FixtureData,
) -> Result<(Ident, Ident, Option<serde_json::Value>), ReplayError> {
    let (Ok(port), Ok(projection)) = (Ident::new(&pin.port), Ident::new(&pin.projection)) else {
        return fail(pin.span, "not a legal projection reference");
    };
    if !resolved.ports.contains_key(&port) {
        return fail(pin.span, format!("no linked port `{port}`"));
    }
    let key = match &pin.key {
        None => None,
        Some(expr) => Some(static_arg(expr, fixture, pin.span)?),
    };
    Ok((port, projection, key))
}

fn pin_update(
    pin: &ast::ProjectionPin,
    resolved: &Resolved,
    fixture: &FixtureData,
    revisions: &mut RevisionCounter,
    _x: &Projections,
) -> Result<ProjectionUpdate, ReplayError> {
    let (port, projection, key) = pin_target(pin, resolved, fixture)?;
    let value = static_arg(&pin.value, fixture, pin.span)?;
    let revision = revisions.mint(pin.projection.as_str(), &key);
    Ok(ProjectionUpdate {
        port,
        projection,
        key,
        revision,
        value,
    })
}

fn outcome_result(
    which: &ast::OutcomeKind,
    args: &[ast::Arg],
    span: Span,
) -> Result<OutcomeResult, ReplayError> {
    match which {
        ast::OutcomeKind::Ok => Ok(OutcomeResult::Ok),
        ast::OutcomeKind::Err => {
            // Legality (micro-decision #28) guarantees exactly one of
            // `refusal: <name>` / `reason: "<text>"`.
            let Some(arg) = args.first() else {
                return fail(span, "`.err` takes `refusal:` or `reason:` (§6.1)");
            };
            match (arg.name.as_str(), &arg.value.kind) {
                ("refusal", ast::ExprKind::Ident(name)) => match Ident::new(name) {
                    Ok(refusal) => Ok(OutcomeResult::Refused { refusal }),
                    Err(e) => fail(span, e.to_string()),
                },
                ("reason", ast::ExprKind::Str(reason)) => Ok(OutcomeResult::Unavailable {
                    reason: reason.clone(),
                }),
                _ => fail(
                    span,
                    "`.err` takes `refusal: <name>` or `reason: \"<text>\"`",
                ),
            }
        }
    }
}

/// Timeline argument values are static (§6.2): literals, records of
/// statics, or fixture references.
fn static_arg(
    expr: &ast::Expr,
    fixture: &FixtureData,
    span: Span,
) -> Result<serde_json::Value, ReplayError> {
    crate::preview::static_json(expr, fixture).map_err(|e| ReplayError { span, message: e })
}
