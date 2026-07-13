//! Example resolution (§6.2): every example becomes a frozen,
//! ready-to-evaluate preview payload — `Page { route, U, X }` or
//! `Fragment { definition, props, state, X }`. Pinned examples bind
//! literally; derived examples (any `events` in the `from` chain) REPLAY
//! through the public machine (`crate::replay`) — self-verifying design.
//!
//! Rules implemented: `from` = merge (child wins), timelines concatenate
//! down the chain, state-pins taint (pinned badge propagates; projection
//! pins do not — §6.2), boot projections auto-bind from the fixture's
//! `boot.<name>` slices, every pin decodes against its structural type
//! (L8 at the use site), and replay failures attribute to the first
//! failing step with descendants reporting "blocked by ancestor".

use std::collections::BTreeMap;

use uhura_base::{Diagnostic, Ident, Span, Value, codes};
use uhura_core::ir::ProgramIr;
use uhura_core::state::{
    Counters, NavEntry, ProjectionSnapshot, Projections, UiState, initial_state,
};
use uhura_syntax::{Parsed, ast};

use crate::fixture::{FixtureData, decode_against_ty};
use crate::replay;
use crate::resolve::{DefEnv, ParsedSource, Resolved, SubjectKind};
use crate::types::Ty;

#[derive(Clone, Debug)]
pub struct ResolvedPreview {
    pub subject: SubjectKind,
    pub example: String,
    pub is_default: bool,
    /// A state pin somewhere in the chain — the caption marks `pinned`.
    pub pinned: bool,
    /// Resolved by timeline replay (`from X → events…` provenance).
    pub derived: bool,
    /// Commands unsettled at the end of a derived timeline (§6.2 caption:
    /// "N command(s) in flight"); always 0 for pinned examples.
    pub in_flight: usize,
    /// Direct parent, for the provenance caption (`from first-page`).
    pub from: Option<String>,
    pub note: Option<String>,
    pub payload: PreviewPayload,
}

#[derive(Clone, Debug)]
pub enum PreviewPayload {
    Page {
        route: Ident,
        u: UiState,
        x: Projections,
    },
    Fragment {
        surface: bool,
        name: Ident,
        props: BTreeMap<Ident, Value>,
        state: BTreeMap<Ident, Value>,
        x: Projections,
    },
}

/// The merged content of an example after walking its `from` chain.
#[derive(Clone, Default)]
struct Effective<'a> {
    params: BTreeMap<String, &'a ast::Expr>,
    props: BTreeMap<String, &'a ast::Expr>,
    state: BTreeMap<String, &'a ast::Expr>,
    /// (port, projection, key-literal-json) → pin.
    projections: BTreeMap<(String, String, String), &'a ast::ProjectionPin>,
    /// The concatenated timeline down the `from` chain, ancestor-first.
    events: Vec<&'a ast::ExampleEvent>,
    state_pinned: bool,
}

pub fn resolve_previews(
    program: &ProgramIr,
    resolved: &Resolved,
    sources: &[ParsedSource],
    fixtures: &BTreeMap<Ident, FixtureData>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<ResolvedPreview> {
    let mut previews = Vec::new();

    // Deterministic subject order: pages, surfaces, components, by name;
    // examples stay in declaration order (§6.3).
    let mut subjects: Vec<(&Ident, &DefEnv)> = Vec::new();
    subjects.extend(resolved.pages.iter());
    subjects.extend(resolved.surfaces.iter());
    subjects.extend(resolved.components.iter());

    for (_, env) in subjects {
        let Some((examples_idx, _)) = resolved
            .example_subjects
            .iter()
            .find(|(_, subject_idx)| **subject_idx == env.source)
        else {
            continue;
        };
        let src = &sources[*examples_idx];
        let Parsed::Examples(file) = &src.parsed else {
            continue;
        };
        resolve_file(
            program,
            resolved,
            env,
            src,
            file,
            fixtures,
            diags,
            &mut previews,
        );
    }
    previews
}

#[allow(clippy::too_many_arguments)]
fn resolve_file(
    program: &ProgramIr,
    resolved: &Resolved,
    env: &DefEnv,
    src: &ParsedSource,
    file: &ast::ExamplesFile,
    fixtures: &BTreeMap<Ident, FixtureData>,
    diags: &mut Vec<Diagnostic>,
    previews: &mut Vec<ResolvedPreview>,
) {
    // Single fixture import per file in the spike; its slices are the
    // `fixture.` root (micro-decision).
    let fixture_imports: Vec<&str> = file
        .uses
        .iter()
        .filter_map(|u| match u {
            ast::Use::Fixture { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if fixture_imports.len() > 1 {
        diags.push(Diagnostic::error(
            codes::MULTIPLE_FIXTURES.0,
            codes::MULTIPLE_FIXTURES.1,
            "the spike resolves one `use fixture` per examples file".to_string(),
            Span::new(src.file, 0, 0),
        ));
        return;
    }
    let empty = FixtureData::default();
    let fixture: &FixtureData = fixture_imports
        .first()
        .and_then(|name| Ident::new(name).ok())
        .and_then(|name| fixtures.get(&name))
        .unwrap_or(&empty);

    let mut by_name: BTreeMap<&str, Effective<'_>> = BTreeMap::new();
    let mut failed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for example in &file.examples {
        let mut effective = Effective::default();
        let mut from: Option<String> = None;
        let mut note: Option<String> = None;

        // Parent first (earlier-declared only — legality enforced).
        for clause in &example.clauses {
            if let ast::ExampleClause::From { name, .. } = clause {
                from = Some(name.clone());
                if let Some(parent) = by_name.get(name.as_str()) {
                    effective = parent.clone();
                }
            }
        }

        for clause in &example.clauses {
            match clause {
                ast::ExampleClause::From { .. } | ast::ExampleClause::Error { .. } => {}
                ast::ExampleClause::Note { text, .. } => note = Some(text.clone()),
                ast::ExampleClause::Params { entries, .. } => {
                    for (name, value) in entries {
                        effective.params.insert(name.clone(), value);
                    }
                }
                ast::ExampleClause::Props { entries, .. } => {
                    for (name, value) in entries {
                        effective.props.insert(name.clone(), value);
                    }
                }
                ast::ExampleClause::State { entries, .. } => {
                    effective.state_pinned = true;
                    for (name, value) in entries {
                        effective.state.insert(name.clone(), value);
                    }
                }
                ast::ExampleClause::Projection(pin) => {
                    let key = pin
                        .key
                        .as_ref()
                        .and_then(|k| static_json(k, fixture).ok())
                        .map(|j| j.to_string())
                        .unwrap_or_default();
                    effective
                        .projections
                        .insert((pin.port.clone(), pin.projection.clone(), key), pin);
                }
                ast::ExampleClause::Events { entries, .. } => effective.events.extend(entries),
            }
        }

        by_name.insert(&example.name, effective.clone());

        // A failed ancestor poisons descendants with one pointed
        // diagnostic each — never a cascade of step errors (§6.2).
        if let Some(parent) = &from
            && failed.contains(parent.as_str())
        {
            diags.push(Diagnostic::error(
                codes::REPLAY_BLOCKED.0,
                codes::REPLAY_BLOCKED.1,
                format!(
                    "`{}` is blocked by ancestor `{parent}` — fix that replay first",
                    example.name
                ),
                example.span,
            ));
            failed.insert(example.name.clone());
            continue;
        }

        let derived = !effective.events.is_empty();
        let Some(bindings) = resolve_bindings(
            program,
            resolved,
            env,
            fixture,
            &effective,
            example.span,
            diags,
        ) else {
            if derived {
                failed.insert(example.name.clone());
            }
            continue; // errors already diagnosed
        };

        let (payload, in_flight) = if derived {
            let input = replay::ReplayInput {
                x: bindings.x,
                params: bindings.params,
                props: bindings.props,
                state_pins: bindings.state_pins,
                events: &effective.events,
                span: example.span,
            };
            match replay::replay(program, resolved, env, fixture, input) {
                Ok(outcome) => (outcome.payload, outcome.in_flight),
                Err(e) => {
                    diags.push(Diagnostic::error(
                        codes::REPLAY_STEP.0,
                        codes::REPLAY_STEP.1,
                        format!("in example `{}`: {}", example.name, e.message),
                        e.span,
                    ));
                    failed.insert(example.name.clone());
                    continue;
                }
            }
        } else {
            match assemble_pinned(program, env, bindings, example.span, diags) {
                Some(payload) => (payload, 0),
                None => continue,
            }
        };
        previews.push(ResolvedPreview {
            subject: env.kind.clone(),
            example: example.name.clone(),
            is_default: example.is_default,
            pinned: effective.state_pinned,
            derived,
            in_flight,
            from,
            note,
            payload,
        });
    }
}

/// The typed bindings an example resolves to before any machine runs:
/// projections (pins + boot auto-bind), params/props, and state pins —
/// shared between pinned assembly and derived replay (§6.2 evaluation
/// order: parent chain → pins → timeline fold).
pub(crate) struct Bindings {
    pub x: Projections,
    pub params: BTreeMap<Ident, Value>,
    pub props: BTreeMap<Ident, Value>,
    pub state_pins: BTreeMap<Ident, Value>,
}

fn resolve_bindings(
    program: &ProgramIr,
    resolved: &Resolved,
    env: &DefEnv,
    fixture: &FixtureData,
    effective: &Effective<'_>,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) -> Option<Bindings> {
    let mut ok = true;
    let pin_error = |diags: &mut Vec<Diagnostic>, ok: &mut bool, at: Span, msg: String| {
        diags.push(Diagnostic::error(
            codes::PIN_DECODE.0,
            codes::PIN_DECODE.1,
            msg,
            at,
        ));
        *ok = false;
    };

    // ── projections: pins + boot auto-bind ─────────────────────────────
    let mut x = Projections::default();
    for ((_, _, _), pin) in &effective.projections {
        let Some((port_name, proj_name)) = Ident::new(&pin.port)
            .ok()
            .zip(Ident::new(&pin.projection).ok())
        else {
            continue;
        };
        let Some((contract, port_types)) = resolved.ports.get(&port_name) else {
            continue; // legality pass diagnosed
        };
        let Some(decl) = contract.projections.get(&proj_name) else {
            continue;
        };
        let key = match (&decl.key, &pin.key) {
            (Some(key_ty), Some(key_expr)) => {
                let key_ty = port_types.from_expr(contract, key_ty);
                match static_json(key_expr, fixture).and_then(|j| decode_against_ty(&j, &key_ty)) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        pin_error(diags, &mut ok, pin.span, format!("projection key: {e}"));
                        continue;
                    }
                }
            }
            (None, None) => None,
            _ => continue, // keyedness mismatch diagnosed by legality
        };
        // `failed("<reason>")` pins the failure state (micro-decision —
        // mirrors `projection-failed`, §9.3).
        if let ast::ExprKind::Call { name, args } = &pin.value.kind
            && name == "failed"
        {
            match args.as_slice() {
                [reason] => match static_json(reason, fixture) {
                    Ok(serde_json::Value::String(reason)) => {
                        x.failed.insert((proj_name.clone(), key), reason);
                    }
                    _ => pin_error(
                        diags,
                        &mut ok,
                        pin.span,
                        "`failed(…)` takes one reason string".to_string(),
                    ),
                },
                _ => pin_error(
                    diags,
                    &mut ok,
                    pin.span,
                    "`failed(…)` takes one reason string".to_string(),
                ),
            }
            continue;
        }
        let ty = port_types.from_expr(contract, &decl.ty);
        match static_json(&pin.value, fixture).and_then(|j| decode_against_ty(&j, &ty)) {
            Ok(value) => {
                x.snapshots.insert(
                    (proj_name.clone(), key),
                    ProjectionSnapshot { revision: 1, value },
                );
            }
            Err(e) => pin_error(
                diags,
                &mut ok,
                pin.span,
                format!("`{}.{}`: {e}", pin.port, pin.projection),
            ),
        }
    }

    // Boot projections auto-bind from `boot.<name>` unless pinned (§6.1).
    for (proj_name, proj) in &program.projections {
        let instance = (proj_name.clone(), None);
        // A `failed("…")` pin is a binding too — auto-bind must not
        // override it with the boot slice.
        if !proj.boot || x.snapshots.contains_key(&instance) || x.failed.contains_key(&instance) {
            continue;
        }
        let Some((contract, port_types)) = resolved.ports.get(&proj.port) else {
            continue;
        };
        let Some(decl) = contract.projections.get(proj_name) else {
            continue;
        };
        let ty = port_types.from_expr(contract, &decl.ty);
        match fixture.get("boot", proj_name.as_str()) {
            None => {
                diags.push(Diagnostic::error(
                    codes::BOOT_UNBOUND.0,
                    codes::BOOT_UNBOUND.1,
                    format!(
                        "boot projection `{proj_name}` needs a `boot.{proj_name}` fixture \
                         slice for the resolver to auto-bind (§6.1)"
                    ),
                    span,
                ));
                ok = false;
            }
            Some(json) => match decode_against_ty(json, &ty) {
                Ok(value) => {
                    x.snapshots.insert(
                        (proj_name.clone(), None),
                        ProjectionSnapshot { revision: 1, value },
                    );
                }
                Err(e) => pin_error(diags, &mut ok, span, format!("boot.{proj_name}: {e}")),
            },
        }
    }

    // ── typed pin decoding against the subject's declarations ──────────
    let decode_map = |entries: &BTreeMap<String, &ast::Expr>,
                      declared: &BTreeMap<Ident, Ty>,
                      what: &str,
                      diags: &mut Vec<Diagnostic>,
                      ok: &mut bool|
     -> BTreeMap<Ident, Value> {
        let mut out = BTreeMap::new();
        for (name, expr) in entries {
            let Ok(ident) = Ident::new(name) else {
                continue;
            };
            let Some(ty) = declared.get(&ident) else {
                continue; // legality pass diagnosed
            };
            match static_json(expr, fixture).and_then(|j| decode_against_ty(&j, ty)) {
                Ok(value) => {
                    out.insert(ident, value);
                }
                Err(e) => {
                    diags.push(Diagnostic::error(
                        codes::PIN_DECODE.0,
                        codes::PIN_DECODE.1,
                        format!("{what} `{name}`: {e}"),
                        expr.span,
                    ));
                    *ok = false;
                }
            }
        }
        out
    };

    let state_pins = decode_map(&effective.state, &env.state, "state pin", diags, &mut ok);

    let (params, props) = match &env.kind {
        SubjectKind::Page { .. } => {
            let params = decode_map(&effective.params, &env.params, "param", diags, &mut ok);
            for name in env.params.keys() {
                if !params.contains_key(name) {
                    pin_error(
                        diags,
                        &mut ok,
                        span,
                        format!("page previews need `params {{ {name} = … }}` to mount"),
                    );
                }
            }
            (params, BTreeMap::new())
        }
        SubjectKind::Component { .. } | SubjectKind::Surface { .. } => {
            let props = decode_map(&effective.props, &env.props, "prop", diags, &mut ok);
            for (prop, ty) in &env.props {
                if !props.contains_key(prop) && !matches!(ty, Ty::Option(_)) {
                    pin_error(
                        diags,
                        &mut ok,
                        span,
                        format!("this example leaves required prop `{prop}` unbound"),
                    );
                }
            }
            (BTreeMap::new(), props)
        }
    };

    if !ok {
        return None;
    }
    Some(Bindings {
        x,
        params,
        props,
        state_pins,
    })
}

/// A pinned example freezes its bindings directly — no machine runs.
fn assemble_pinned(
    program: &ProgramIr,
    env: &DefEnv,
    bindings: Bindings,
    _span: Span,
    _diags: &mut [Diagnostic],
) -> Option<PreviewPayload> {
    match &env.kind {
        SubjectKind::Page { route } => {
            let def = program.pages.get(route)?;
            let mut state = initial_state(def);
            state.extend(bindings.state_pins);
            let mut counters = Counters::default();
            let serial = counters.mint_page();
            Some(PreviewPayload::Page {
                route: route.clone(),
                u: UiState {
                    rev: 0,
                    nav: vec![NavEntry {
                        serial,
                        route: route.clone(),
                        params: bindings.params,
                        state,
                    }],
                    surfaces: Vec::new(),
                    pending: BTreeMap::new(),
                    counters,
                },
                x: bindings.x,
            })
        }
        SubjectKind::Component { name } | SubjectKind::Surface { name, .. } => {
            let surface = matches!(env.kind, SubjectKind::Surface { .. });
            let def = if surface {
                program.surfaces.get(name)?
            } else {
                program.components.get(name)?
            };
            let mut state = initial_state(def);
            state.extend(bindings.state_pins);
            Some(PreviewPayload::Fragment {
                surface,
                name: name.clone(),
                props: bindings.props,
                state,
                x: bindings.x,
            })
        }
    }
}

/// Evaluates a static pin expression (§6.2: literals, records of statics,
/// `fixture.<ns>.<name>[.<field>…]` references) to raw JSON.
pub(crate) fn static_json(
    expr: &ast::Expr,
    fixture: &FixtureData,
) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    match &expr.kind {
        ast::ExprKind::Int(i) => Ok(J::Number((*i).into())),
        ast::ExprKind::Str(s) => Ok(J::String(s.clone())),
        ast::ExprKind::Bool(b) => Ok(J::Bool(*b)),
        ast::ExprKind::None => Ok(J::Null),
        ast::ExprKind::Record(entries) => {
            let mut map = serde_json::Map::new();
            for (name, value) in entries {
                map.insert(name.clone(), static_json(value, fixture)?);
            }
            Ok(J::Object(map))
        }
        ast::ExprKind::Field { .. } | ast::ExprKind::Ident(_) => {
            let mut path = Vec::new();
            collect_path(expr, &mut path)?;
            let ["fixture", ns, name, rest @ ..] = path.as_slice() else {
                return Err("pins reference slices as `fixture.<ns>.<name>`".to_string());
            };
            let mut value = fixture
                .get(ns, name)
                .ok_or_else(|| format!("no fixture slice `{ns}.{name}`"))?;
            for field in rest {
                value = value
                    .get(field)
                    .ok_or_else(|| format!("no field `{field}` in `{ns}.{name}`"))?;
            }
            Ok(value.clone())
        }
        _ => Err("pins are static: literals, records, or fixture references (§6.2)".to_string()),
    }
}

fn collect_path<'a>(expr: &'a ast::Expr, out: &mut Vec<&'a str>) -> Result<(), String> {
    match &expr.kind {
        ast::ExprKind::Ident(name) => {
            out.push(name);
            Ok(())
        }
        ast::ExprKind::Field { base, name } => {
            collect_path(base, out)?;
            out.push(name);
            Ok(())
        }
        _ => Err("pins are static: literals, records, or fixture references (§6.2)".to_string()),
    }
}
