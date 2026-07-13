//! Lowering — checked AST → `uhura-ir/0` (§12.2). Gated on zero errors, so
//! resolution here is a straight replay of the rules the passes already
//! enforced; anything unresolvable lowers to a placeholder rather than
//! panicking. Node ordinals are assigned depth-first pre-order per
//! definition (§8.1 keys). Spans go to a side table keyed by IR path — the
//! IR bytes stay location-independent (examples-invariance, §6.1).

use std::collections::BTreeMap;

use serde::Serialize;
use uhura_base::{Ident, Span};
use uhura_core::ir;
use uhura_syntax::{Parsed, ast};

use crate::catalog::{Catalog, EventKind, PropType};
use crate::infer::Typer;
use crate::manifest::Manifest;
use crate::resolve::{DefEnv, ParsedSource, Resolved, RouteSeg};
use crate::types::{MapKey, Ty};

/// One side-table entry; `file` is the corpus-relative path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SpanEntry {
    pub file: String,
    pub start: u32,
    pub end: u32,
}

pub struct Lowered {
    pub program: ir::ProgramIr,
    pub spans: BTreeMap<String, SpanEntry>,
}

pub fn lower(
    manifest: &Manifest,
    resolved: &Resolved,
    catalog: &Catalog,
    sources: &[ParsedSource],
) -> Lowered {
    let mut spans = BTreeMap::new();

    let ports = resolved
        .ports
        .iter()
        .map(|(name, (contract, _))| {
            (
                name.clone(),
                ir::PortPin {
                    version: contract.version.clone(),
                    hash: contract.canonical_hash(),
                },
            )
        })
        .collect();

    let mut projections = BTreeMap::new();
    for (port_name, (contract, port_types)) in &resolved.ports {
        for (proj_name, decl) in &contract.projections {
            projections.insert(
                proj_name.clone(),
                ir::ProjectionIr {
                    port: port_name.clone(),
                    boot: decl.boot,
                    ty: lower_ty(&port_types.from_expr(contract, &decl.ty)),
                    key: decl
                        .key
                        .as_ref()
                        .map(|k| lower_ty(&port_types.from_expr(contract, k))),
                },
            );
        }
    }

    let mut element_events = BTreeMap::new();
    let mut element_props = BTreeMap::new();
    for (el_name, el) in &catalog.elements {
        if !el.props.is_empty() {
            let props: BTreeMap<Ident, ir::PropKindIr> = el
                .props
                .iter()
                .map(|(prop, decl)| {
                    (
                        prop.clone(),
                        match decl.ty {
                            PropType::Text => ir::PropKindIr::Plain,
                            PropType::Bool => ir::PropKindIr::Bool,
                            PropType::Int => ir::PropKindIr::Int,
                            PropType::Enum(_) | PropType::Icon => ir::PropKindIr::Token,
                            PropType::Asset => ir::PropKindIr::Asset,
                        },
                    )
                })
                .collect();
            element_props.insert(el_name.clone(), props);
        }
        if el.events.is_empty() {
            continue;
        }
        let events: BTreeMap<Ident, ir::ElementEventIr> = el
            .events
            .iter()
            .map(|(event, decl)| {
                (
                    event.clone(),
                    ir::ElementEventIr {
                        kind: match decl.kind {
                            EventKind::Input => ir::EventKindIr::Input,
                            EventKind::Observe => ir::EventKindIr::Observe,
                        },
                        carries: decl
                            .carries
                            .iter()
                            .map(|(f, ty)| {
                                (
                                    f.clone(),
                                    match ty {
                                        PropType::Bool => ir::CarryTypeIr::Bool,
                                        PropType::Int => ir::CarryTypeIr::Int,
                                        _ => ir::CarryTypeIr::Text,
                                    },
                                )
                            })
                            .collect(),
                    },
                )
            })
            .collect();
        element_events.insert(el_name.clone(), events);
    }

    let routes = resolved
        .routes
        .iter()
        .map(|(name, info)| {
            (
                name.clone(),
                ir::RouteIr {
                    segments: info
                        .segments
                        .iter()
                        .map(|seg| match seg {
                            RouteSeg::Static(s) => ir::RouteSegIr::Static(s.clone()),
                            RouteSeg::Param(p) => ir::RouteSegIr::Param(p.clone()),
                        })
                        .collect(),
                    params: info.params.clone(),
                },
            )
        })
        .collect();

    let lower_defs =
        |defs: &BTreeMap<Ident, DefEnv>, prefix: &str, spans: &mut BTreeMap<String, SpanEntry>| {
            defs.iter()
                .filter_map(|(name, env)| {
                    let src = &sources[env.source];
                    let Parsed::Module(ast) = &src.parsed else {
                        return None;
                    };
                    let path = format!("{prefix}.{name}");
                    let def = lower_def(env, ast, resolved, src, &path, spans);
                    Some((name.clone(), def))
                })
                .collect::<BTreeMap<Ident, ir::DefIr>>()
        };

    let pages = lower_defs(&resolved.pages, "pages", &mut spans);
    let components = lower_defs(&resolved.components, "components", &mut spans);
    let surfaces = lower_defs(&resolved.surfaces, "surfaces", &mut spans);

    let program = ir::ProgramIr {
        protocol: ir::IR_PROTOCOL.to_string(),
        app: manifest.app_name.clone(),
        entry: manifest.entry.clone(),
        catalog: ir::CatalogPin {
            name: catalog.name.clone(),
            version: catalog.version.clone(),
            hash: catalog.canonical_hash(),
        },
        ports,
        projections,
        element_events,
        element_props,
        routes,
        pages,
        components,
        surfaces,
    };
    Lowered { program, spans }
}

/// Check-land `Ty` → the IR's runtime decode grammar. Nominal id/cursor
/// types collapse to `Id` (nominal identity is a check-time concern; the
/// wire form is a string either way). `Error`/`NoneLit` cannot survive a
/// zero-error check in a declared signature; they lower to `Text` to keep
/// the IR total.
fn lower_ty(ty: &Ty) -> ir::TyIr {
    match ty {
        Ty::Bool => ir::TyIr::Bool,
        Ty::Int => ir::TyIr::Int,
        Ty::Text => ir::TyIr::Text,
        Ty::Id | Ty::Nominal { .. } => ir::TyIr::Id,
        Ty::Tag => ir::TyIr::Tag,
        Ty::Asset => ir::TyIr::Asset,
        Ty::Enum(values) => ir::TyIr::Enum(values.iter().cloned().collect()),
        Ty::Record(fields) => ir::TyIr::Record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), lower_ty(ty)))
                .collect(),
        ),
        Ty::Union(variants) => ir::TyIr::Union(
            variants
                .iter()
                .map(|(variant, fields)| {
                    (
                        variant.clone(),
                        fields
                            .iter()
                            .map(|(name, ty)| (name.clone(), lower_ty(ty)))
                            .collect(),
                    )
                })
                .collect(),
        ),
        Ty::List(inner) => ir::TyIr::List(Box::new(lower_ty(inner))),
        Ty::Map(key, inner) => ir::TyIr::Map {
            key: match key {
                MapKey::Id => ir::MapKeyIr::Id,
                MapKey::Tag => ir::MapKeyIr::Tag,
            },
            value: Box::new(lower_ty(inner)),
        },
        Ty::Option(inner) => ir::TyIr::Option(Box::new(lower_ty(inner))),
        Ty::NoneLit | Ty::Error => ir::TyIr::Text,
    }
}

fn record_span(spans: &mut BTreeMap<String, SpanEntry>, key: String, rel_path: &str, span: Span) {
    spans.insert(
        key,
        SpanEntry {
            file: rel_path.to_string(),
            start: span.start,
            end: span.end,
        },
    );
}

fn lower_def(
    env: &DefEnv,
    ast: &ast::File,
    resolved: &Resolved,
    src: &ParsedSource,
    path: &str,
    spans: &mut BTreeMap<String, SpanEntry>,
) -> ir::DefIr {
    record_span(spans, path.to_string(), &src.rel_path, def_span(ast));

    let modality = match &ast.kind {
        ast::DefKind::Surface { modality, .. } => {
            Some(modality.clone().unwrap_or_else(|| "sheet".to_string()))
        }
        _ => None,
    };

    let props = ast
        .props
        .iter()
        .filter_map(|p| Ident::new(&p.name).ok())
        .collect();
    let emits = ast
        .emits
        .iter()
        .filter_map(|e| Ident::new(&e.name).ok())
        .collect();
    let params = ast
        .params
        .iter()
        .filter_map(|p| Ident::new(&p.name).ok())
        .collect();

    let mut state = BTreeMap::new();
    let mut handlers = Vec::new();
    if let Some(store) = &ast.store {
        for field in &store.state {
            let Ok(name) = Ident::new(&field.name) else {
                continue;
            };
            state.insert(name, lower_init(&field.init));
        }
        for (i, handler) in store.handlers.iter().enumerate() {
            record_span(
                spans,
                format!("{path}/handler/{i}"),
                &src.rel_path,
                handler.span,
            );
            if let Some(h) = lower_handler(env, resolved, handler) {
                handlers.push(h);
            }
        }
    }

    let mut ctx = LowerCtx {
        env,
        resolved,
        locals: Vec::new(),
        next_ord: 0,
    };
    let root_nodes = ctx.lower_nodes(&ast.markup);
    let root = root_nodes.into_iter().next().unwrap_or_else(|| {
        // Zero-error gating means this only happens for a markupless def,
        // which the one-root rule already rejected; keep IR total anyway.
        ir::NodeIr::Element(ir::ElementIr {
            element: Ident::new("view").expect("kebab"),
            ord: 0,
            class: None,
            props: vec![],
            events: vec![],
            text: vec![],
            children: vec![],
        })
    });

    // Machine-event signatures (typed by the check pass) bake into the IR
    // so the runtime can type payload JSON and enforce eligibility (§7.2).
    let events = env
        .events
        .iter()
        .map(|(event, params)| {
            (
                event.clone(),
                params
                    .iter()
                    .map(|(name, ty)| ir::EventParamIr {
                        name: name.clone(),
                        ty: lower_ty(ty),
                    })
                    .collect(),
            )
        })
        .collect();

    ir::DefIr {
        modality,
        props,
        emits,
        params,
        state,
        events,
        handlers,
        root,
    }
}

fn def_span(ast: &ast::File) -> Span {
    match &ast.kind {
        ast::DefKind::Component { span, .. }
        | ast::DefKind::Page { span }
        | ast::DefKind::Surface { span, .. }
        | ast::DefKind::Error { span } => *span,
    }
}

fn lower_init(lit: &ast::Literal) -> ir::InitValue {
    match lit {
        ast::Literal::Int(i) => ir::InitValue::Int(*i),
        ast::Literal::Str(s) => ir::InitValue::Text(s.clone()),
        ast::Literal::Bool(b) => ir::InitValue::Bool(*b),
        ast::Literal::None | ast::Literal::Error => ir::InitValue::None,
        ast::Literal::EmptyMap => ir::InitValue::EmptyMap,
    }
}

fn lower_handler(
    env: &DefEnv,
    resolved: &Resolved,
    handler: &ast::Handler,
) -> Option<ir::HandlerIr> {
    let on = match &handler.event {
        ast::EventRef::Semantic { name, .. } => ir::EventKeyIr::Semantic {
            event: Ident::new(name).ok()?,
        },
        ast::EventRef::Outcome { command, which, .. } => ir::EventKeyIr::Outcome {
            command: Ident::new(command).ok()?,
            which: match which {
                ast::OutcomeKind::Ok => ir::OutcomeKindIr::Ok,
                ast::OutcomeKind::Err => ir::OutcomeKindIr::Err,
            },
        },
    };
    let params: Vec<Ident> = handler
        .params
        .iter()
        .filter_map(|p| Ident::new(&p.name).ok())
        .collect();

    // Rebuild the handler-scope binding types the checker established.
    let mut scratch = Vec::new();
    let locals: Vec<(Ident, Ty)> = match &handler.event {
        ast::EventRef::Semantic { .. } => handler
            .params
            .iter()
            .filter_map(|p| {
                let name = Ident::new(&p.name).ok()?;
                let ty =
                    p.ty.as_ref()
                        .map(|t| crate::resolve::source_type(t, env, &mut scratch))
                        .unwrap_or(Ty::Error);
                Some((name, ty))
            })
            .collect(),
        ast::EventRef::Outcome { command, which, .. } => {
            let payload = Ident::new(command)
                .ok()
                .and_then(|c| env.commands.get(&c))
                .map(|info| Ty::Record(info.payload.iter().cloned().collect()))
                .unwrap_or(Ty::Error);
            let mut locals = vec![
                (Ident::new("tag").expect("kebab"), Ty::Tag),
                (Ident::new("cmd").expect("kebab"), payload),
            ];
            if matches!(which, ast::OutcomeKind::Err) {
                locals.push((Ident::new("refusal").expect("kebab"), Ty::Text));
            }
            locals
        }
    };
    let mut ctx = LowerCtx {
        env,
        resolved,
        locals,
        next_ord: 0,
    };
    let guard = handler.guard.as_ref().map(|g| ctx.lower_expr(g));
    let mut body = Vec::new();
    for stmt in &handler.body {
        if let Some(s) = ctx.lower_stmt(stmt) {
            body.push(s);
        }
    }
    Some(ir::HandlerIr {
        on,
        params,
        guard,
        body,
    })
}

struct LowerCtx<'a> {
    env: &'a DefEnv,
    resolved: &'a Resolved,
    /// Names bound locally (handler params, `as` tags, each items, match
    /// bindings) — they lower to `BindingRef`. Types ride along so
    /// re-inference (each-over classification, union arms) stays exact.
    locals: Vec<(Ident, Ty)>,
    next_ord: u32,
}

impl LowerCtx<'_> {
    fn ord(&mut self) -> u32 {
        let ord = self.next_ord;
        self.next_ord += 1;
        ord
    }

    fn lower_expr(&mut self, e: &ast::Expr) -> ir::ExprIr {
        match &e.kind {
            ast::ExprKind::Error => ir::ExprIr::None,
            ast::ExprKind::Int(i) => ir::ExprIr::Int(*i),
            ast::ExprKind::Str(s) => ir::ExprIr::Text(s.clone()),
            ast::ExprKind::Bool(b) => ir::ExprIr::Bool(*b),
            ast::ExprKind::None => ir::ExprIr::None,
            ast::ExprKind::Ident(name) => self.lower_name(name),
            ast::ExprKind::Field { base, name } => ir::ExprIr::Field {
                base: Box::new(self.lower_expr(base)),
                name: Ident::new(name).unwrap_or_else(|_| Ident::new("x").expect("kebab")),
            },
            ast::ExprKind::Index { base, key } => ir::ExprIr::Index {
                base: Box::new(self.lower_expr(base)),
                key: Box::new(self.lower_expr(key)),
            },
            ast::ExprKind::Call { name, args } => match name.as_str() {
                "to-text" => ir::ExprIr::ToText(Box::new(
                    args.first()
                        .map_or(ir::ExprIr::Text(String::new()), |a| self.lower_expr(a)),
                )),
                "count" => ir::ExprIr::Count(Box::new(
                    args.first()
                        .map_or(ir::ExprIr::Int(0), |a| self.lower_expr(a)),
                )),
                other => {
                    let projection =
                        Ident::new(other).unwrap_or_else(|_| Ident::new("x").expect("kebab"));
                    ir::ExprIr::ProjectionKeyed {
                        projection,
                        key: Box::new(
                            args.first()
                                .map_or(ir::ExprIr::None, |a| self.lower_expr(a)),
                        ),
                    }
                }
            },
            ast::ExprKind::Unary { op, expr } => ir::ExprIr::Unary {
                op: match op {
                    ast::UnaryOp::Not => ir::UnaryOpIr::Not,
                    ast::UnaryOp::Neg => ir::UnaryOpIr::Neg,
                },
                expr: Box::new(self.lower_expr(expr)),
            },
            ast::ExprKind::Binary { op, lhs, rhs } => ir::ExprIr::Binary {
                op: lower_binop(*op),
                lhs: Box::new(self.lower_expr(lhs)),
                rhs: Box::new(self.lower_expr(rhs)),
            },
            ast::ExprKind::If { cond, then, els } => ir::ExprIr::If {
                cond: Box::new(self.lower_expr(cond)),
                then: Box::new(self.lower_expr(then)),
                els: Box::new(self.lower_expr(els)),
            },
            ast::ExprKind::Record(entries) => ir::ExprIr::RecordLit(
                entries
                    .iter()
                    .filter_map(|(name, value)| {
                        Some(ir::ArgIr {
                            name: Ident::new(name).ok()?,
                            value: self.lower_expr(value),
                        })
                    })
                    .collect(),
            ),
        }
    }

    fn lower_name(&mut self, name: &str) -> ir::ExprIr {
        let Ok(ident) = Ident::new(name) else {
            return ir::ExprIr::None;
        };
        if self.locals.iter().any(|(l, _)| *l == ident) {
            return ir::ExprIr::BindingRef(ident);
        }
        if self.env.state.contains_key(&ident) {
            return ir::ExprIr::StateRef(ident);
        }
        if self.env.props.contains_key(&ident) {
            return ir::ExprIr::PropRef(ident);
        }
        if self.env.params.contains_key(&ident) {
            return ir::ExprIr::ParamRef(ident);
        }
        if self.env.projections.contains_key(&ident) {
            return ir::ExprIr::ProjectionRef(ident);
        }
        // Zero-error gating: unreachable for checked programs.
        ir::ExprIr::None
    }

    fn lower_args(&mut self, args: &[ast::Arg]) -> Vec<ir::ArgIr> {
        args.iter()
            .filter_map(|arg| {
                Some(ir::ArgIr {
                    name: Ident::new(&arg.name).ok()?,
                    value: self.lower_expr(&arg.value),
                })
            })
            .collect()
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> Option<ir::StmtIr> {
        match stmt {
            ast::Stmt::Error { .. } => None,
            ast::Stmt::Set { path, value, .. } => Some(ir::StmtIr::Set {
                field: Ident::new(&path.field).ok()?,
                key: path.key.as_ref().map(|k| self.lower_expr(k)),
                value: self.lower_expr(value),
            }),
            ast::Stmt::Send {
                command,
                args,
                bind,
                ..
            } => {
                let command = Ident::new(command).ok()?;
                let port = self.env.commands.get(&command)?.port.clone();
                let args = self.lower_args(args);
                let bind = bind.as_ref().and_then(|b| Ident::new(b).ok());
                if let Some(b) = &bind {
                    self.locals.push((b.clone(), Ty::Tag));
                }
                Some(ir::StmtIr::Send {
                    port,
                    command,
                    args,
                    bind,
                })
            }
            ast::Stmt::OpenSurface { name, args, .. } => Some(ir::StmtIr::OpenSurface {
                surface: Ident::new(name).ok()?,
                args: self.lower_args(args),
            }),
            ast::Stmt::Dismiss { .. } => Some(ir::StmtIr::Dismiss),
            ast::Stmt::Navigate { target, .. } => match target {
                ast::NavTarget::Back => Some(ir::StmtIr::NavigateBack),
                ast::NavTarget::Route { name, args } => Some(ir::StmtIr::Navigate {
                    route: Ident::new(name).ok()?,
                    args: self.lower_args(args),
                }),
            },
        }
    }

    fn lower_nodes(&mut self, nodes: &[ast::Node]) -> Vec<ir::NodeIr> {
        nodes.iter().filter_map(|n| self.lower_node(n)).collect()
    }

    fn lower_node(&mut self, node: &ast::Node) -> Option<ir::NodeIr> {
        match node {
            ast::Node::Error { .. } | ast::Node::Text { .. } => None,
            ast::Node::If {
                cond, then, els, ..
            } => Some(ir::NodeIr::If {
                cond: self.lower_expr(cond),
                then: self.lower_nodes(then),
                els: els
                    .as_ref()
                    .map(|e| self.lower_nodes(e))
                    .unwrap_or_default(),
            }),
            ast::Node::Each {
                item,
                seq,
                key,
                body,
                ..
            } => {
                let ord = self.ord();
                let seq_ty = self.infer_ty(seq);
                let over = match &seq_ty {
                    Ty::Map(MapKey::Id, _) => ir::OverIr::MapIdKeys,
                    Ty::Map(MapKey::Tag, _) => ir::OverIr::MapTagKeys,
                    _ => ir::OverIr::List,
                };
                let item_ty = match seq_ty {
                    Ty::List(t) => *t,
                    Ty::Map(MapKey::Id, _) => Ty::Id,
                    Ty::Map(MapKey::Tag, _) => Ty::Tag,
                    _ => Ty::Error,
                };
                let seq_ir = self.lower_expr(seq);
                let item_ident = Ident::new(item).ok()?;
                self.locals.push((item_ident.clone(), item_ty));
                let key_ir = self.lower_expr(key);
                let body_ir = self.lower_nodes(body);
                self.locals.pop();
                Some(ir::NodeIr::Each(ir::EachIr {
                    ord,
                    item: item_ident,
                    over,
                    seq: seq_ir,
                    key: key_ir,
                    body: body_ir,
                }))
            }
            ast::Node::Match {
                scrutinee, arms, ..
            } => {
                let source = self.match_source(scrutinee);
                // Arm binding types mirror the markup pass: availability
                // ready binds the projection value / failed binds text;
                // union arms bind the variant's field record.
                let ready_ty = match &source {
                    ir::MatchSourceIr::Availability { projection, .. } => self
                        .env
                        .projections
                        .get(projection)
                        .map(|p| p.ty.clone())
                        .unwrap_or(Ty::Error),
                    ir::MatchSourceIr::Union { .. } => self.infer_ty(scrutinee),
                };
                let is_availability = matches!(source, ir::MatchSourceIr::Availability { .. });
                let arms = arms
                    .iter()
                    .map(|arm| {
                        let variant = match &arm.pattern {
                            ast::MatchPattern::Else => None,
                            ast::MatchPattern::Variant(v) => Ident::new(v).ok(),
                        };
                        let binding = arm.binding.as_ref().and_then(|b| Ident::new(b).ok());
                        if let Some(b) = &binding {
                            let ty = if is_availability {
                                match variant.as_ref().map(Ident::as_str) {
                                    Some("ready") => ready_ty.clone(),
                                    _ => Ty::Text,
                                }
                            } else {
                                match (&ready_ty, &variant) {
                                    (Ty::Union(variants), Some(v)) => variants
                                        .get(v)
                                        .map(|fields| Ty::Record(fields.clone()))
                                        .unwrap_or(Ty::Error),
                                    _ => Ty::Error,
                                }
                            };
                            self.locals.push((b.clone(), ty));
                        }
                        let body = self.lower_nodes(&arm.body);
                        if binding.is_some() {
                            self.locals.pop();
                        }
                        ir::MatchArmIr {
                            variant,
                            binding,
                            body,
                        }
                    })
                    .collect();
                Some(ir::NodeIr::Match(ir::MatchIr { source, arms }))
            }
            ast::Node::Element(el) => {
                let name = Ident::new(&el.name).ok()?;
                if self.resolved.components.contains_key(&name)
                    && self.env.component_imports.contains_key(&name)
                {
                    Some(self.lower_component_call(el, name))
                } else {
                    Some(self.lower_element(el, name))
                }
            }
        }
    }

    /// Availability vs union classification — mirrors the markup pass.
    fn match_source(&mut self, scrutinee: &ast::Expr) -> ir::MatchSourceIr {
        match &scrutinee.kind {
            ast::ExprKind::Ident(name) => {
                if let Ok(ident) = Ident::new(name)
                    && self.env.projections.contains_key(&ident)
                    && !self.locals.iter().any(|(l, _)| *l == ident)
                {
                    return ir::MatchSourceIr::Availability {
                        projection: ident,
                        key: None,
                    };
                }
                ir::MatchSourceIr::Union {
                    value: self.lower_expr(scrutinee),
                }
            }
            ast::ExprKind::Call { name, args } => {
                if let Ok(ident) = Ident::new(name)
                    && self.env.projections.contains_key(&ident)
                {
                    return ir::MatchSourceIr::Availability {
                        projection: ident,
                        key: args.first().map(|a| self.lower_expr(a)),
                    };
                }
                ir::MatchSourceIr::Union {
                    value: self.lower_expr(scrutinee),
                }
            }
            _ => ir::MatchSourceIr::Union {
                value: self.lower_expr(scrutinee),
            },
        }
    }

    fn lower_element(&mut self, el: &ast::Element, element: Ident) -> ir::NodeIr {
        let ord = self.ord();
        let mut class = None;
        let mut props = Vec::new();
        for attr in &el.attrs {
            let Ok(attr_name) = Ident::new(&attr.name) else {
                continue;
            };
            let value = match &attr.value {
                ast::AttrValue::Bare => ir::ExprIr::Bool(true),
                ast::AttrValue::Literal(s) => ir::ExprIr::Text(s.clone()),
                ast::AttrValue::Expr(e) => self.lower_expr(e),
            };
            if attr_name.as_str() == "class" {
                class = Some(value);
            } else {
                props.push(ir::ArgIr {
                    name: attr_name,
                    value,
                });
            }
        }
        let events = el
            .events
            .iter()
            .filter_map(|event_attr| {
                let event = Ident::new(&event_attr.event).ok()?;
                match &event_attr.binding {
                    ast::EventBinding::Forward => None, // rejected by markup pass
                    ast::EventBinding::Emit { name, args } => Some(ir::ElementEventBindingIr {
                        event,
                        emit: Ident::new(name).ok()?,
                        args: self.lower_args(args),
                    }),
                }
            })
            .collect();
        let text = el
            .children
            .iter()
            .filter_map(|c| match c {
                ast::Node::Text { runs, .. } => Some(runs),
                _ => None,
            })
            .flatten()
            .map(|run| match run {
                ast::TextRun::Literal(s) => ir::TextRunIr::Literal(s.clone()),
                ast::TextRun::Interp(e) => ir::TextRunIr::Interp(self.lower_expr(e)),
            })
            .collect();
        let children = self.lower_nodes(&el.children);
        ir::NodeIr::Element(ir::ElementIr {
            element,
            ord,
            class,
            props,
            events,
            text,
            children,
        })
    }

    fn lower_component_call(&mut self, el: &ast::Element, component: Ident) -> ir::NodeIr {
        let ord = self.ord();
        let props = el
            .attrs
            .iter()
            .filter_map(|attr| {
                let name = Ident::new(&attr.name).ok()?;
                let value = match &attr.value {
                    ast::AttrValue::Bare => ir::ExprIr::Bool(true),
                    ast::AttrValue::Literal(s) => ir::ExprIr::Text(s.clone()),
                    ast::AttrValue::Expr(e) => self.lower_expr(e),
                };
                Some(ir::ArgIr { name, value })
            })
            .collect();
        let emits = el
            .events
            .iter()
            .filter_map(|event_attr| {
                let emit = Ident::new(&event_attr.event).ok()?;
                let target = match &event_attr.binding {
                    ast::EventBinding::Forward => ir::EmitTargetIr::Forward,
                    ast::EventBinding::Emit { name, args } => ir::EmitTargetIr::Rebind {
                        event: Ident::new(name).ok()?,
                        args: self.lower_args(args),
                    },
                };
                Some(ir::EmitBindingIr { emit, target })
            })
            .collect();
        ir::NodeIr::Component(ir::ComponentCallIr {
            component,
            ord,
            props,
            emits,
        })
    }

    /// Re-infers an expression's type with the current typed bindings —
    /// the program is already clean, so this is a lookup, not a re-check.
    fn infer_ty(&self, e: &ast::Expr) -> Ty {
        let mut scratch = Vec::new();
        let mut typer = Typer::new(self.env, self.resolved, &mut scratch);
        typer.locals = self.locals.clone();
        typer.infer(e)
    }
}

fn lower_binop(op: ast::BinaryOp) -> ir::BinaryOpIr {
    match op {
        ast::BinaryOp::Add => ir::BinaryOpIr::Add,
        ast::BinaryOp::Sub => ir::BinaryOpIr::Sub,
        ast::BinaryOp::Concat => ir::BinaryOpIr::Concat,
        ast::BinaryOp::Eq => ir::BinaryOpIr::Eq,
        ast::BinaryOp::NotEq => ir::BinaryOpIr::NotEq,
        ast::BinaryOp::Lt => ir::BinaryOpIr::Lt,
        ast::BinaryOp::Le => ir::BinaryOpIr::Le,
        ast::BinaryOp::Gt => ir::BinaryOpIr::Gt,
        ast::BinaryOp::Ge => ir::BinaryOpIr::Ge,
        ast::BinaryOp::And => ir::BinaryOpIr::And,
        ast::BinaryOp::Or => ir::BinaryOpIr::Or,
        ast::BinaryOp::Coalesce => ir::BinaryOpIr::Coalesce,
    }
}
