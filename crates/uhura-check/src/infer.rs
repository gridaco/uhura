//! Expression typing (§4.3), statement legality (§4.2), and handler
//! discipline — multi-handler signatures, outcome shapes, guard order.

use std::collections::BTreeMap;

use uhura_base::{Diagnostic, Ident, Span, codes};
use uhura_syntax::ast;

use crate::resolve::{DefEnv, Resolved, SubjectKind, did_you_mean};
use crate::types::{MapKey, Ty, comparable, compatible};

pub struct Typer<'a> {
    pub env: &'a DefEnv,
    pub resolved: &'a Resolved,
    pub diags: &'a mut Vec<Diagnostic>,
    /// Innermost-last binding stack: handler params, `as` tags, each items,
    /// match bindings.
    pub locals: Vec<(Ident, Ty)>,
    /// View position: non-boot projection reads must sit inside `{#match}`
    /// availability (§9.2); guards/bodies read bare (transactional
    /// backstop, §4.2).
    pub in_view: bool,
}

impl<'a> Typer<'a> {
    pub fn new(env: &'a DefEnv, resolved: &'a Resolved, diags: &'a mut Vec<Diagnostic>) -> Self {
        Typer {
            env,
            resolved,
            diags,
            locals: Vec::new(),
            in_view: false,
        }
    }

    fn error(&mut self, code: (&'static str, &'static str), message: String, span: Span) -> Ty {
        self.diags
            .push(Diagnostic::error(code.0, code.1, message, span));
        Ty::Error
    }

    /// Pushes a local binding; local shadowing is forbidden like every
    /// other kind (§3).
    pub fn push_local(&mut self, name: &str, ty: Ty, span: Span) -> usize {
        let Ok(ident) = Ident::new(name) else {
            return self.locals.len();
        };
        let already = self.locals.iter().any(|(n, _)| *n == ident)
            || self.env.state.contains_key(&ident)
            || self.env.props.contains_key(&ident)
            || self.env.params.contains_key(&ident)
            || self.env.projections.contains_key(&ident);
        if already {
            self.diags.push(Diagnostic::error(
                codes::SHADOWED_NAME.0,
                codes::SHADOWED_NAME.1,
                format!("binding `{ident}` shadows an existing name"),
                span,
            ));
        }
        self.locals.push((ident, ty));
        self.locals.len() - 1
    }

    pub fn truncate_locals(&mut self, len: usize) {
        self.locals.truncate(len);
    }

    fn name_type(&mut self, name: &str, span: Span) -> Ty {
        let Ok(ident) = Ident::new(name) else {
            return Ty::Error;
        };
        if let Some((_, ty)) = self.locals.iter().rev().find(|(n, _)| *n == ident) {
            return ty.clone();
        }
        if let Some(ty) = self.env.state.get(&ident) {
            return ty.clone();
        }
        if let Some(ty) = self.env.props.get(&ident) {
            return ty.clone();
        }
        if let Some(ty) = self.env.params.get(&ident) {
            return ty.clone();
        }
        if let Some(proj) = self.env.projections.get(&ident) {
            if proj.key.is_some() {
                return self.error(
                    codes::WRONG_ARGS,
                    format!("projection `{ident}` is keyed — read it as `{ident}(<key>)`"),
                    span,
                );
            }
            if self.in_view && !proj.boot {
                return self.error(
                    codes::UNGUARDED_PROJECTION_READ,
                    format!(
                        "`{ident}` is absent until delivered — in markup, read it through \
                         `{{#match {ident}}}` availability arms (§9.2)"
                    ),
                    span,
                );
            }
            return proj.ty.clone();
        }
        let candidates = self
            .locals
            .iter()
            .map(|(n, _)| n)
            .chain(self.env.state.keys())
            .chain(self.env.props.keys())
            .chain(self.env.params.keys())
            .chain(self.env.projections.keys());
        let suggestion = did_you_mean(&ident, candidates).cloned();
        let mut d = Diagnostic::error(
            codes::UNRESOLVED_NAME.0,
            codes::UNRESOLVED_NAME.1,
            format!("nothing named `{ident}` is in scope"),
            span,
        );
        if let Some(s) = suggestion {
            d = d.with_note(format!("did you mean `{s}`?"));
        }
        self.diags.push(d);
        Ty::Error
    }

    pub fn infer(&mut self, e: &ast::Expr) -> Ty {
        match &e.kind {
            ast::ExprKind::Error => Ty::Error,
            ast::ExprKind::Int(_) => Ty::Int,
            ast::ExprKind::Str(_) => Ty::Text,
            ast::ExprKind::Bool(_) => Ty::Bool,
            ast::ExprKind::None => Ty::NoneLit,
            ast::ExprKind::Ident(name) => self.name_type(name, e.span),
            ast::ExprKind::Field { base, name } => {
                let base_ty = self.infer(base);
                let Ok(field) = Ident::new(name) else {
                    return Ty::Error;
                };
                match base_ty {
                    Ty::Error => Ty::Error,
                    Ty::Record(fields) => match fields.get(&field) {
                        Some(t) => t.clone(),
                        None => {
                            let suggestion = did_you_mean(&field, fields.keys()).cloned();
                            let mut d = Diagnostic::error(
                                codes::UNKNOWN_FIELD.0,
                                codes::UNKNOWN_FIELD.1,
                                format!(
                                    "no field `{field}` on {}",
                                    Ty::Record(fields.clone()).describe()
                                ),
                                e.span,
                            );
                            if let Some(s) = suggestion {
                                d = d.with_note(format!("did you mean `{s}`?"));
                            }
                            self.diags.push(d);
                            Ty::Error
                        }
                    },
                    Ty::Option(_) => self.error(
                        codes::UNKNOWN_FIELD,
                        format!("`.{field}` on an optional — settle it first with `??`"),
                        e.span,
                    ),
                    other => self.error(
                        codes::UNKNOWN_FIELD,
                        format!("{} has no fields", other.describe()),
                        e.span,
                    ),
                }
            }
            ast::ExprKind::Index { base, key } => {
                let base_ty = self.infer(base);
                match base_ty {
                    Ty::Error => {
                        self.infer(key);
                        Ty::Error
                    }
                    Ty::Map(k, v) => {
                        let key_ty = match k {
                            MapKey::Id => Ty::Id,
                            MapKey::Tag => Ty::Tag,
                        };
                        self.check(key, &key_ty);
                        Ty::Option(v)
                    }
                    Ty::List(t) => {
                        self.check(key, &Ty::Int);
                        Ty::Option(t)
                    }
                    other => self.error(
                        codes::BAD_INDEX,
                        format!(
                            "{} is not indexable (§4.3: maps and lists)",
                            other.describe()
                        ),
                        e.span,
                    ),
                }
            }
            ast::ExprKind::Call { name, args } => self.infer_call(name, args, e.span),
            ast::ExprKind::Unary { op, expr } => match op {
                ast::UnaryOp::Not => {
                    self.check(expr, &Ty::Bool);
                    Ty::Bool
                }
                ast::UnaryOp::Neg => {
                    self.check(expr, &Ty::Int);
                    Ty::Int
                }
            },
            ast::ExprKind::Binary { op, lhs, rhs } => self.infer_binary(*op, lhs, rhs, e.span),
            ast::ExprKind::If { cond, then, els } => {
                self.check(cond, &Ty::Bool);
                let t = self.infer(then);
                let f = self.infer(els);
                self.unify_branches(t, f, e.span)
            }
            ast::ExprKind::Record(entries) => {
                let mut fields = BTreeMap::new();
                for (name, value) in entries {
                    let ty = self.infer(value);
                    if let Ok(name) = Ident::new(name) {
                        fields.insert(name, ty);
                    }
                }
                Ty::Record(fields)
            }
        }
    }

    fn infer_call(&mut self, name: &str, args: &[ast::Expr], span: Span) -> Ty {
        match name {
            "to-text" => {
                if args.len() != 1 {
                    return self.error(
                        codes::BAD_BUILTIN_CALL,
                        "`to-text` takes exactly one argument".to_string(),
                        span,
                    );
                }
                let ty = self.infer(&args[0]);
                if !matches!(ty, Ty::Int | Ty::Text | Ty::Bool | Ty::Id | Ty::Error) {
                    return self.error(
                        codes::BAD_BUILTIN_CALL,
                        format!("`to-text` renders int/text/bool/id, not {}", ty.describe()),
                        span,
                    );
                }
                Ty::Text
            }
            "count" => {
                if args.len() != 1 {
                    return self.error(
                        codes::BAD_BUILTIN_CALL,
                        "`count` takes exactly one argument".to_string(),
                        span,
                    );
                }
                let ty = self.infer(&args[0]);
                if !matches!(ty, Ty::List(_) | Ty::Map(..) | Ty::Error) {
                    return self.error(
                        codes::BAD_BUILTIN_CALL,
                        format!("`count` counts lists and maps, not {}", ty.describe()),
                        span,
                    );
                }
                Ty::Int
            }
            other => {
                let Ok(ident) = Ident::new(other) else {
                    return Ty::Error;
                };
                if let Some(proj) = self.env.projections.get(&ident) {
                    let Some(key_ty) = proj.key.clone() else {
                        return self.error(
                            codes::WRONG_ARGS,
                            format!("projection `{ident}` is not keyed — read it bare"),
                            span,
                        );
                    };
                    if args.len() != 1 {
                        return self.error(
                            codes::WRONG_ARGS,
                            format!("keyed read `{ident}(<key>)` takes exactly one key"),
                            span,
                        );
                    }
                    self.check(&args[0], &key_ty);
                    if self.in_view {
                        return self.error(
                            codes::UNGUARDED_PROJECTION_READ,
                            format!(
                                "`{ident}(…)` is absent until delivered — in markup, read it \
                                 through `{{#match {ident}(…)}}` availability arms (§9.2)"
                            ),
                            span,
                        );
                    }
                    return proj.ty.clone();
                }
                self.error(
                    codes::UNRESOLVED_NAME,
                    format!(
                        "`{other}` is not a builtin (`to-text`, `count`) or a keyed projection"
                    ),
                    span,
                )
            }
        }
    }

    fn infer_binary(
        &mut self,
        op: ast::BinaryOp,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        span: Span,
    ) -> Ty {
        use ast::BinaryOp as B;
        match op {
            B::Add | B::Sub => {
                self.check(lhs, &Ty::Int);
                self.check(rhs, &Ty::Int);
                Ty::Int
            }
            B::Concat => {
                self.check(lhs, &Ty::Text);
                self.check(rhs, &Ty::Text);
                Ty::Text
            }
            B::And | B::Or => {
                self.check(lhs, &Ty::Bool);
                self.check(rhs, &Ty::Bool);
                Ty::Bool
            }
            B::Lt | B::Le | B::Gt | B::Ge => {
                self.check(lhs, &Ty::Int);
                self.check(rhs, &Ty::Int);
                Ty::Bool
            }
            B::Eq | B::NotEq => {
                let l = self.infer(lhs);
                let r = self.infer(rhs);
                if !comparable(&l, &r) {
                    self.error(
                        codes::BAD_OPERAND,
                        format!("cannot compare {} with {}", l.describe(), r.describe()),
                        span,
                    );
                }
                Ty::Bool
            }
            B::Coalesce => {
                let l = self.infer(lhs);
                match l {
                    Ty::Error => {
                        self.infer(rhs);
                        Ty::Error
                    }
                    Ty::Option(inner) => {
                        self.check(rhs, &inner);
                        *inner
                    }
                    other => {
                        self.infer(rhs);
                        self.error(
                            codes::BAD_OPERAND,
                            format!(
                                "`??` settles optionals; {} is not optional",
                                other.describe()
                            ),
                            span,
                        )
                    }
                }
            }
        }
    }

    fn unify_branches(&mut self, t: Ty, f: Ty, span: Span) -> Ty {
        if t.is_error() || f.is_error() {
            return Ty::Error;
        }
        if t == f {
            return t;
        }
        match (&t, &f) {
            (Ty::NoneLit, Ty::Option(_)) => f,
            (Ty::Option(_), Ty::NoneLit) => t,
            (Ty::NoneLit, _) => Ty::Option(Box::new(f)),
            (_, Ty::NoneLit) => Ty::Option(Box::new(t)),
            _ if compatible(&t, &f) => t,
            _ if compatible(&f, &t) => f,
            _ => self.error(
                codes::BAD_OPERAND,
                format!(
                    "`if` branches disagree: {} vs {}",
                    t.describe(),
                    f.describe()
                ),
                span,
            ),
        }
    }

    /// Expected-type-directed checking: string literals against enums,
    /// record literals field-wise, `if` branch-wise; everything else infers
    /// and tests compatibility.
    pub fn check(&mut self, e: &ast::Expr, expected: &Ty) {
        match (&e.kind, expected) {
            (_, Ty::Error) | (ast::ExprKind::Error, _) => {}
            (ast::ExprKind::Str(s), Ty::Enum(values)) => {
                if !values.iter().any(|v| v.as_str() == s) {
                    let list: Vec<&str> = values.iter().map(Ident::as_str).collect();
                    self.error(
                        codes::TYPE_MISMATCH,
                        format!("`\"{s}\"` is not one of {}", list.join(" | ")),
                        e.span,
                    );
                }
            }
            (ast::ExprKind::Record(entries), Ty::Record(fields)) => {
                let mut bound: BTreeMap<Ident, Span> = BTreeMap::new();
                for (name, value) in entries {
                    let Ok(name_ident) = Ident::new(name) else {
                        continue;
                    };
                    match fields.get(&name_ident) {
                        Some(field_ty) => {
                            self.check(value, field_ty);
                        }
                        None => {
                            self.error(
                                codes::UNKNOWN_FIELD,
                                format!("no field `{name}` on {}", expected.describe()),
                                value.span,
                            );
                        }
                    }
                    bound.insert(name_ident, value.span);
                }
                for (field, field_ty) in fields {
                    if !bound.contains_key(field) && !matches!(field_ty, Ty::Option(_)) {
                        self.error(
                            codes::TYPE_MISMATCH,
                            format!("record literal is missing required field `{field}`"),
                            e.span,
                        );
                    }
                }
            }
            (ast::ExprKind::Record(entries), Ty::Map(..)) if entries.is_empty() => {}
            (ast::ExprKind::If { cond, then, els }, _) => {
                self.check(cond, &Ty::Bool);
                self.check(then, expected);
                self.check(els, expected);
            }
            _ => {
                let ty = self.infer(e);
                if !compatible(expected, &ty) {
                    self.mismatch(expected, &ty, e.span);
                }
            }
        }
    }

    fn mismatch(&mut self, expected: &Ty, actual: &Ty, span: Span) {
        self.error(
            codes::TYPE_MISMATCH,
            format!(
                "expected {}, got {}",
                expected.describe(),
                actual.describe()
            ),
            span,
        );
    }

    // ── statements (§4.2) ──────────────────────────────────────────────

    pub fn check_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::Error { .. } => {}
            ast::Stmt::Set {
                path, value, span, ..
            } => self.check_set(path, value, *span),
            ast::Stmt::Send {
                command,
                args,
                bind,
                span,
                ..
            } => self.check_send(command, args, bind.as_deref(), *span),
            ast::Stmt::OpenSurface {
                name, args, span, ..
            } => {
                self.check_open_surface(name, args, *span);
            }
            ast::Stmt::Dismiss { span, .. } => {
                if !matches!(self.env.kind, SubjectKind::Surface { .. }) {
                    self.error(
                        codes::DISMISS_OUTSIDE_SURFACE,
                        "`dismiss` pops the surface instance — only surfaces have one".to_string(),
                        *span,
                    );
                }
            }
            ast::Stmt::Navigate { target, span, .. } => self.check_navigate(target, *span),
        }
    }

    fn check_set(&mut self, path: &ast::SetPath, value: &ast::Expr, _span: Span) {
        let Ok(field) = Ident::new(&path.field) else {
            return;
        };
        let Some(field_ty) = self.env.state.get(&field).cloned() else {
            let suggestion = did_you_mean(&field, self.env.state.keys()).cloned();
            let mut d = Diagnostic::error(
                codes::UNRESOLVED_NAME.0,
                codes::UNRESOLVED_NAME.1,
                format!("`set` writes own-scope state; no state field `{field}`"),
                path.span,
            );
            if let Some(s) = suggestion {
                d = d.with_note(format!("did you mean `{s}`?"));
            }
            self.diags.push(d);
            self.infer(value);
            return;
        };
        match (&path.key, field_ty) {
            (None, ty) => self.check(value, &ty),
            (Some(key), Ty::Map(k, v)) => {
                let key_ty = match k {
                    MapKey::Id => Ty::Id,
                    MapKey::Tag => Ty::Tag,
                };
                self.check(key, &key_ty);
                // `= none` removes the entry (§4.2), so the value position
                // is effectively optional.
                self.check(value, &Ty::Option(v));
            }
            (Some(_), other) => {
                self.error(
                    codes::BAD_INDEX,
                    format!(
                        "`{field}[…]` writes a map entry, but `{field}` is {}",
                        other.describe()
                    ),
                    path.span,
                );
                self.infer(value);
            }
        }
    }

    fn check_send(&mut self, command: &str, args: &[ast::Arg], bind: Option<&str>, span: Span) {
        let Ok(cmd_ident) = Ident::new(command) else {
            return;
        };
        let Some(info) = self.env.commands.get(&cmd_ident) else {
            let mut d = Diagnostic::error(
                codes::UNKNOWN_COMMAND.0,
                codes::UNKNOWN_COMMAND.1,
                format!("no command `{command}` is imported"),
                span,
            );
            if let Some(s) = did_you_mean(&cmd_ident, self.env.commands.keys()) {
                d = d.with_note(format!("did you mean `{s}`?"));
            } else {
                d = d.with_note("import it: `use port <p> { command <c> }`".to_string());
            }
            self.diags.push(d);
            for arg in args {
                self.infer(&arg.value);
            }
            return;
        };
        let payload = info.payload.clone();
        self.check_named_args(args, &payload, "command payload", span);
        if let Some(bind) = bind {
            self.push_local(bind, Ty::Tag, span);
        }
    }

    fn check_open_surface(&mut self, name: &str, args: &[ast::Arg], span: Span) {
        let Ok(surface) = Ident::new(name) else {
            return;
        };
        if !self.env.surface_imports.contains_key(&surface) {
            let mut d = Diagnostic::error(
                codes::UNKNOWN_SURFACE.0,
                codes::UNKNOWN_SURFACE.1,
                format!("no surface `{surface}` is imported"),
                span,
            );
            if self.resolved.surfaces.contains_key(&surface) {
                d = d.with_note(format!("add `use surface {surface}`"));
            }
            self.diags.push(d);
            for arg in args {
                self.infer(&arg.value);
            }
            return;
        }
        let Some(target) = self.resolved.surfaces.get(&surface) else {
            return; // unknown-import already diagnosed
        };
        let props: Vec<(Ident, Ty)> = target
            .props
            .iter()
            .map(|(n, t)| (n.clone(), t.clone()))
            .collect();
        self.check_named_args(args, &props, "surface props", span);
    }

    fn check_navigate(&mut self, target: &ast::NavTarget, span: Span) {
        let ast::NavTarget::Route { name, args } = target else {
            return;
        };
        let Ok(route) = Ident::new(name) else {
            return;
        };
        if !self.resolved.routes.contains_key(&route) {
            let mut d = Diagnostic::error(
                codes::UNKNOWN_ROUTE.0,
                codes::UNKNOWN_ROUTE.1,
                format!("no route `{route}` (routes come from `app/**/page.uhura` paths)"),
                span,
            );
            if let Some(s) = did_you_mean(&route, self.resolved.routes.keys()) {
                d = d.with_note(format!("did you mean `{s}`?"));
            }
            self.diags.push(d);
            for arg in args {
                self.infer(&arg.value);
            }
            return;
        }
        let params: Vec<(Ident, Ty)> = match self.resolved.pages.get(&route) {
            Some(page) => page
                .params
                .iter()
                .map(|(n, t)| (n.clone(), t.clone()))
                .collect(),
            None => Vec::new(),
        };
        self.check_named_args(args, &params, "route params", span);
    }

    /// Named-argument lists cover the declared fields exactly (§4.2).
    pub fn check_named_args(
        &mut self,
        args: &[ast::Arg],
        declared: &[(Ident, Ty)],
        what: &str,
        span: Span,
    ) {
        let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
        for arg in args {
            match declared.iter().find(|(n, _)| n.as_str() == arg.name) {
                Some((_, ty)) => {
                    let ty = ty.clone();
                    self.check(&arg.value, &ty);
                }
                None => {
                    self.error(
                        codes::WRONG_ARGS,
                        format!("`{}` is not part of the {what}", arg.name),
                        arg.span,
                    );
                    self.infer(&arg.value);
                }
            }
            if seen.insert(&arg.name, ()).is_some() {
                self.error(
                    codes::WRONG_ARGS,
                    format!("`{}` is given twice", arg.name),
                    arg.span,
                );
            }
        }
        for (name, ty) in declared {
            if !args.iter().any(|a| a.name == name.as_str()) && !matches!(ty, Ty::Option(_)) {
                self.error(codes::WRONG_ARGS, format!("missing `{name}`"), span);
            }
        }
    }
}

/// Checks a store block and returns the machine-event signature table
/// (event → params) that markup emit-checking consumes.
pub fn check_store(
    env: &DefEnv,
    resolved: &Resolved,
    store: &ast::Store,
    diags: &mut Vec<Diagnostic>,
) -> BTreeMap<Ident, Vec<(Ident, Ty)>> {
    let mut events: BTreeMap<Ident, Vec<(Ident, Ty)>> = BTreeMap::new();
    // Event key → span of an unguarded handler already seen.
    let mut unguarded: BTreeMap<String, Span> = BTreeMap::new();

    for handler in &store.handlers {
        let mut typer = Typer::new(env, resolved, diags);

        // ── signature ──────────────────────────────────────────────────
        match &handler.event {
            ast::EventRef::Semantic { name, span } => {
                let Ok(event) = Ident::new(name) else {
                    continue;
                };
                let mut sig: Vec<(Ident, Ty)> = Vec::new();
                for param in &handler.params {
                    let Ok(param_name) = Ident::new(&param.name) else {
                        continue;
                    };
                    let ty = match &param.ty {
                        Some(t) => crate::resolve::source_type(t, env, typer.diags),
                        None => {
                            typer.diags.push(Diagnostic::error(
                                codes::HANDLER_SIGNATURE_MISMATCH.0,
                                codes::HANDLER_SIGNATURE_MISMATCH.1,
                                format!("UI-event param `{param_name}` needs a type (§4.2)"),
                                param.span,
                            ));
                            Ty::Error
                        }
                    };
                    sig.push((param_name, ty));
                }
                match events.get(&event) {
                    None => {
                        events.insert(event.clone(), sig.clone());
                    }
                    Some(first) if *first != sig => {
                        typer.diags.push(Diagnostic::error(
                            codes::HANDLER_SIGNATURE_MISMATCH.0,
                            codes::HANDLER_SIGNATURE_MISMATCH.1,
                            format!(
                                "every `on {event}` handler must declare the identical \
                                 signature (§4.2)"
                            ),
                            *span,
                        ));
                    }
                    Some(_) => {}
                }
                for (i, (name, ty)) in sig.iter().enumerate() {
                    typer.push_local(name.as_str(), ty.clone(), handler.params[i].span);
                }
            }
            ast::EventRef::Outcome {
                command,
                which,
                span,
            } => {
                let Ok(cmd) = Ident::new(command) else {
                    continue;
                };
                let expected: &[&str] = match which {
                    ast::OutcomeKind::Ok => &["tag", "cmd"],
                    ast::OutcomeKind::Err => &["tag", "cmd", "refusal"],
                };
                let names: Vec<&str> = handler.params.iter().map(|p| p.name.as_str()).collect();
                let annotated = handler.params.iter().any(|p| p.ty.is_some());
                if names != expected || annotated {
                    typer.diags.push(Diagnostic::error(
                        codes::BAD_OUTCOME_SIGNATURE.0,
                        codes::BAD_OUTCOME_SIGNATURE.1,
                        format!(
                            "outcome handlers have the fixed name-only signature \
                             `on {command}.{}({})` (§4.2)",
                            match which {
                                ast::OutcomeKind::Ok => "ok",
                                ast::OutcomeKind::Err => "err",
                            },
                            expected.join(", ")
                        ),
                        *span,
                    ));
                }
                match env.commands.get(&cmd) {
                    None => {
                        typer.diags.push(Diagnostic::error(
                            codes::UNKNOWN_COMMAND.0,
                            codes::UNKNOWN_COMMAND.1,
                            format!("no command `{command}` is imported to have outcomes"),
                            *span,
                        ));
                    }
                    Some(info) => {
                        let cmd_record = Ty::Record(info.payload.iter().cloned().collect());
                        typer
                            .locals
                            .push((Ident::new("tag").expect("kebab"), Ty::Tag));
                        typer
                            .locals
                            .push((Ident::new("cmd").expect("kebab"), cmd_record));
                        if matches!(which, ast::OutcomeKind::Err) {
                            // Refusal names or "unavailable" — compared as text.
                            typer
                                .locals
                                .push((Ident::new("refusal").expect("kebab"), Ty::Text));
                        }
                    }
                }
            }
        }

        // ── guard order: unguarded-above-anything is unreachable ───────
        let event_key = match &handler.event {
            ast::EventRef::Semantic { name, .. } => format!("on {name}"),
            ast::EventRef::Outcome { command, which, .. } => format!(
                "on {command}.{}",
                match which {
                    ast::OutcomeKind::Ok => "ok",
                    ast::OutcomeKind::Err => "err",
                }
            ),
        };
        if let Some(prev) = unguarded.get(&event_key) {
            typer.diags.push(
                Diagnostic::error(
                    codes::UNREACHABLE_HANDLER.0,
                    codes::UNREACHABLE_HANDLER.1,
                    format!("this `{event_key}` handler is unreachable"),
                    handler.span,
                )
                .with_label(*prev, "an unguarded handler above always wins"),
            );
        }
        if handler.guard.is_none() {
            unguarded.entry(event_key).or_insert(handler.span);
        }

        // ── guard + body ───────────────────────────────────────────────
        if let Some(guard) = &handler.guard {
            typer.check(guard, &Ty::Bool);
        }
        let mut navigates = 0usize;
        for stmt in &handler.body {
            typer.check_stmt(stmt);
            if let ast::Stmt::Navigate { span, .. } = stmt {
                navigates += 1;
                if navigates > 1 {
                    typer.diags.push(Diagnostic::error(
                        codes::MULTIPLE_NAVIGATES.0,
                        codes::MULTIPLE_NAVIGATES.1,
                        "at most one `navigate` per handler (§4.2: ≤ 1/step)".to_string(),
                        *span,
                    ));
                }
            }
        }
    }
    events
}
