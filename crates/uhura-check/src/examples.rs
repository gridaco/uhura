//! Examples clause legality (§6.1 — the checker-enforced matrix).
//! Resolution to frozen snapshots is M3; derived replay is M4. This pass
//! guarantees the *shape*: clause sets by subject kind, `from` chains
//! (earlier-only, so no cycles), default uniqueness, pin targets, and the
//! static-value discipline of pinned expressions.

use std::collections::BTreeSet;

use uhura_base::{Diagnostic, Ident, Severity, Span, codes};
use uhura_syntax::ast;

use crate::manifest::Manifest;
use crate::resolve::{DefEnv, Resolved, SubjectKind, did_you_mean};

pub fn check_examples(
    file: &ast::ExamplesFile,
    subject: &DefEnv,
    resolved: &Resolved,
    manifest: &Manifest,
    file_span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    // ── imports: fixtures only, and known ones ─────────────────────────
    for use_decl in &file.uses {
        match use_decl {
            ast::Use::Fixture { name, span, .. } => {
                if Ident::new(name).is_ok_and(|n| !manifest.fixtures.contains_key(&n)) {
                    diags.push(Diagnostic::error(
                        codes::UNKNOWN_FIXTURE.0,
                        codes::UNKNOWN_FIXTURE.1,
                        format!("no fixture `{name}` in the manifest"),
                        *span,
                    ));
                }
            }
            ast::Use::Component { span, .. }
            | ast::Use::Surface { span, .. }
            | ast::Use::Port { span, .. } => {
                diags.push(Diagnostic::error(
                    codes::ILLEGAL_CLAUSE.0,
                    codes::ILLEGAL_CLAUSE.1,
                    "examples files import fixtures only; the subject's imports are its own \
                     (§6.1)"
                        .to_string(),
                    *span,
                ));
            }
        }
    }

    let is_page = matches!(subject.kind, SubjectKind::Page { .. });
    let is_component = matches!(subject.kind, SubjectKind::Component { .. });
    let is_surface = matches!(subject.kind, SubjectKind::Surface { .. });

    let mut declared: BTreeSet<&str> = BTreeSet::new();
    let mut default_span: Option<Span> = None;

    for example in &file.examples {
        if !declared.insert(&example.name) {
            diags.push(Diagnostic::error(
                codes::BAD_FROM.0,
                codes::BAD_FROM.1,
                format!("example `{}` is declared twice", example.name),
                example.span,
            ));
        }
        if example.is_default {
            match default_span {
                None => default_span = Some(example.span),
                Some(first) => diags.push(
                    Diagnostic::error(
                        codes::MULTIPLE_DEFAULTS.0,
                        codes::MULTIPLE_DEFAULTS.1,
                        "at most one example is `default` (§6.1)".to_string(),
                        example.span,
                    )
                    .with_label(first, "already defaulted here"),
                ),
            }
        }

        for clause in &example.clauses {
            let (legal, what, span) = match clause {
                ast::ExampleClause::Error { .. } => continue,
                ast::ExampleClause::Note { span, .. } => (true, "note", *span),
                ast::ExampleClause::From { name, span } => {
                    // Earlier-declared only — forward refs and cycles are
                    // unrepresentable (§6.2).
                    if !declared.contains(name.as_str()) || name == &example.name {
                        diags.push(Diagnostic::error(
                            codes::BAD_FROM.0,
                            codes::BAD_FROM.1,
                            format!(
                                "`from {name}` must name an example declared earlier in this \
                                 file (§6.2: no cycles, no forward refs)"
                            ),
                            *span,
                        ));
                    }
                    (true, "from", *span)
                }
                ast::ExampleClause::Params { entries, span } => {
                    check_params(entries, subject, diags);
                    (is_page, "params", *span)
                }
                ast::ExampleClause::Props { entries, span } => {
                    check_props(entries, subject, diags);
                    (is_component || is_surface, "props", *span)
                }
                ast::ExampleClause::State { entries, span } => {
                    check_state(entries, subject, diags);
                    (is_page || is_surface, "state", *span)
                }
                ast::ExampleClause::Projection(pin) => {
                    check_projection_pin(pin, resolved, diags);
                    (is_page || is_surface, "projection", pin.span)
                }
                ast::ExampleClause::Events { entries, span } => {
                    for event in entries {
                        check_event(event, subject, resolved, diags);
                    }
                    (is_page || is_surface, "events", *span)
                }
            };
            if !legal {
                diags.push(Diagnostic::error(
                    codes::ILLEGAL_CLAUSE.0,
                    codes::ILLEGAL_CLAUSE.1,
                    format!(
                        "`{what}` is not a {} clause (§6.1: components take props/from/note; \
                         pages add params/projection/state/events; surfaces take both sets)",
                        subject.kind.describe()
                    ),
                    span,
                ));
            }
        }
    }

    if default_span.is_none() && !file.examples.is_empty() {
        diags.push(Diagnostic::new(
            codes::NO_DEFAULT.0,
            codes::NO_DEFAULT.1,
            Severity::Info,
            "no `default` example — the canvas cover falls back to the first declared (§6.1)",
            file_span,
        ));
    }
}

fn check_params(entries: &[(String, ast::Expr)], subject: &DefEnv, diags: &mut Vec<Diagnostic>) {
    for (name, value) in entries {
        let known = Ident::new(name).is_ok_and(|n| subject.params.contains_key(&n));
        if !known {
            diags.push(Diagnostic::error(
                codes::BAD_PIN.0,
                codes::BAD_PIN.1,
                format!("the page declares no param `{name}`"),
                value.span,
            ));
        }
        require_static(value, diags);
    }
}

fn check_props(entries: &[(String, ast::Expr)], subject: &DefEnv, diags: &mut Vec<Diagnostic>) {
    for (name, value) in entries {
        let known = Ident::new(name).is_ok_and(|n| subject.props.contains_key(&n));
        if !known {
            let mut d = Diagnostic::error(
                codes::BAD_PIN.0,
                codes::BAD_PIN.1,
                format!("the subject declares no prop `{name}`"),
                value.span,
            );
            if let Ok(ident) = Ident::new(name)
                && let Some(s) = did_you_mean(&ident, subject.props.keys())
            {
                d = d.with_note(format!("did you mean `{s}`?"));
            }
            diags.push(d);
        }
        require_static(value, diags);
    }
}

fn check_state(entries: &[(String, ast::Expr)], subject: &DefEnv, diags: &mut Vec<Diagnostic>) {
    for (name, value) in entries {
        let known = Ident::new(name).is_ok_and(|n| subject.state.contains_key(&n));
        if !known {
            diags.push(Diagnostic::error(
                codes::BAD_PIN.0,
                codes::BAD_PIN.1,
                format!("the subject declares no state field `{name}`"),
                value.span,
            ));
        }
        require_static(value, diags);
    }
}

fn check_projection_pin(
    pin: &ast::ProjectionPin,
    resolved: &Resolved,
    diags: &mut Vec<Diagnostic>,
) {
    let Ok(port_name) = Ident::new(&pin.port) else {
        return;
    };
    let Some((contract, _)) = resolved.ports.get(&port_name) else {
        diags.push(Diagnostic::error(
            codes::UNKNOWN_PIN_TARGET.0,
            codes::UNKNOWN_PIN_TARGET.1,
            format!("no port `{}` in the manifest", pin.port),
            pin.span,
        ));
        return;
    };
    let Ok(proj_name) = Ident::new(&pin.projection) else {
        return;
    };
    let Some(decl) = contract.projections.get(&proj_name) else {
        diags.push(Diagnostic::error(
            codes::UNKNOWN_PIN_TARGET.0,
            codes::UNKNOWN_PIN_TARGET.1,
            format!("port `{port_name}` declares no projection `{proj_name}`"),
            pin.span,
        ));
        return;
    };
    match (&decl.key, &pin.key) {
        (Some(_), None) => diags.push(Diagnostic::error(
            codes::BAD_PIN.0,
            codes::BAD_PIN.1,
            format!("`{proj_name}` is keyed — pin an instance: `{port_name}.{proj_name}(<key>)`"),
            pin.span,
        )),
        (None, Some(_)) => diags.push(Diagnostic::error(
            codes::BAD_PIN.0,
            codes::BAD_PIN.1,
            format!("`{proj_name}` is not keyed — drop the key"),
            pin.span,
        )),
        _ => {}
    }
    if let Some(key) = &pin.key {
        require_static(key, diags);
    }
    // `failed("<reason>")` pins the failure state (micro-decision — mirrors
    // `projection-failed`, §9.3); anything else must be static data.
    match &pin.value.kind {
        ast::ExprKind::Call { name, args } if name == "failed" => {
            if !matches!(args.as_slice(), [one] if matches!(one.kind, ast::ExprKind::Str(_))) {
                diags.push(Diagnostic::error(
                    codes::BAD_PIN.0,
                    codes::BAD_PIN.1,
                    "`failed(…)` takes one reason string".to_string(),
                    pin.value.span,
                ));
            }
        }
        _ => require_static(&pin.value, diags),
    }
}

fn check_event(
    event: &ast::ExampleEvent,
    subject: &DefEnv,
    resolved: &Resolved,
    diags: &mut Vec<Diagnostic>,
) {
    match event {
        ast::ExampleEvent::Projection(pin) => check_projection_pin(pin, resolved, diags),
        ast::ExampleEvent::Semantic { name, args, span } => {
            let known = Ident::new(name).is_ok_and(|n| subject.events.contains_key(&n));
            if !known {
                diags.push(Diagnostic::error(
                    codes::BAD_EXAMPLE_EVENT.0,
                    codes::BAD_EXAMPLE_EVENT.1,
                    format!("the subject has no handler for `{name}` — replay would drop it"),
                    *span,
                ));
            }
            for arg in args {
                require_static(&arg.value, diags);
            }
        }
        ast::ExampleEvent::Outcome {
            command,
            which,
            args,
            span,
        } => {
            let Ok(cmd) = Ident::new(command) else {
                return;
            };
            let Some(info) = subject.commands.get(&cmd) else {
                diags.push(Diagnostic::error(
                    codes::BAD_EXAMPLE_EVENT.0,
                    codes::BAD_EXAMPLE_EVENT.1,
                    format!(
                        "the subject imports no command `{command}` — no send could be \
                         outstanding for this outcome"
                    ),
                    *span,
                ));
                return;
            };
            match which {
                ast::OutcomeKind::Ok => {
                    if !args.is_empty() {
                        diags.push(Diagnostic::error(
                            codes::BAD_EXAMPLE_EVENT.0,
                            codes::BAD_EXAMPLE_EVENT.1,
                            "ok payloads are empty for every spike command (§9.1)".to_string(),
                            *span,
                        ));
                    }
                }
                ast::OutcomeKind::Err => {
                    // Mirrors OutcomeResult (§9.3): a refused outcome names
                    // a declared refusal; an unavailable outcome carries a
                    // reason text (micro-decision — the design's §6.1
                    // excerpt shows only the refusal form).
                    match args.as_slice() {
                        [one] if one.name == "refusal" => {
                            let declared = declared_refusals(&cmd, subject, resolved);
                            match &one.value.kind {
                                ast::ExprKind::Ident(n)
                                    if Ident::new(n).is_ok_and(|n| declared.contains(&n))
                                        || n == "unavailable" => {}
                                ast::ExprKind::Ident(n) => diags.push(Diagnostic::error(
                                    codes::BAD_EXAMPLE_EVENT.0,
                                    codes::BAD_EXAMPLE_EVENT.1,
                                    format!(
                                        "`{n}` is not a declared refusal of `{command}` \
                                         (or `unavailable`)"
                                    ),
                                    one.value.span,
                                )),
                                _ => diags.push(Diagnostic::error(
                                    codes::BAD_EXAMPLE_EVENT.0,
                                    codes::BAD_EXAMPLE_EVENT.1,
                                    "the refusal is a bare name, not a string".to_string(),
                                    one.value.span,
                                )),
                            }
                        }
                        [one] if one.name == "reason" => {
                            if !matches!(one.value.kind, ast::ExprKind::Str(_)) {
                                diags.push(Diagnostic::error(
                                    codes::BAD_EXAMPLE_EVENT.0,
                                    codes::BAD_EXAMPLE_EVENT.1,
                                    "an unavailable outcome's `reason` is a text string (§9.3)"
                                        .to_string(),
                                    one.value.span,
                                ));
                            }
                        }
                        _ => diags.push(Diagnostic::error(
                            codes::BAD_EXAMPLE_EVENT.0,
                            codes::BAD_EXAMPLE_EVENT.1,
                            "`.err` takes exactly `(refusal: <name>)` or `(reason: \"<text>\")`"
                                .to_string(),
                            *span,
                        )),
                    }
                    let _ = info;
                }
            }
        }
    }
}

fn declared_refusals(cmd: &Ident, subject: &DefEnv, resolved: &Resolved) -> BTreeSet<Ident> {
    let Some(info) = subject.commands.get(cmd) else {
        return BTreeSet::new();
    };
    resolved
        .ports
        .get(&info.port)
        .and_then(|(contract, _)| contract.commands.get(cmd))
        .map(|c| c.refusals.clone())
        .unwrap_or_default()
}

/// Pinned values are static: literals, records of static values, and
/// fixture slice references (`fixture.<ns>.<name>` field chains). No state,
/// props, operators, or reads — an example is data, not a program (§6.2).
fn require_static(expr: &ast::Expr, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ast::ExprKind::Int(_)
        | ast::ExprKind::Str(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::None
        | ast::ExprKind::Error => {}
        ast::ExprKind::Record(entries) => {
            for (_, value) in entries {
                require_static(value, diags);
            }
        }
        ast::ExprKind::Field { .. } if fixture_rooted(expr) => {}
        _ => diags.push(Diagnostic::error(
            codes::BAD_PIN.0,
            codes::BAD_PIN.1,
            "pins are static: literals, records of literals, or `fixture.…` slice references \
             (§6.2)"
                .to_string(),
            expr.span,
        )),
    }
}

fn fixture_rooted(expr: &ast::Expr) -> bool {
    match &expr.kind {
        ast::ExprKind::Ident(name) => name == "fixture",
        ast::ExprKind::Field { base, .. } => fixture_rooted(base),
        _ => false,
    }
}
