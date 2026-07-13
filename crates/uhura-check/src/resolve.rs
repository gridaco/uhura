//! Routes from paths, definition/header discipline, import resolution, and
//! port linking (design §3, §9.1) — everything that turns parsed files into
//! named, connected definitions with typed environments.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Diagnostic, FileId, Ident, Span, codes};
use uhura_port::PortContract;
use uhura_syntax::ast;
use uhura_syntax::{Parsed, SourceKind};

use crate::types::{MapKey, PortTypes, Ty};

/// One parsed source, owned by the pipeline.
pub struct ParsedSource {
    pub file: FileId,
    pub rel_path: String,
    pub kind: SourceKind,
    pub parsed: Parsed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubjectKind {
    Page {
        route: Ident,
    },
    Component {
        name: Ident,
    },
    Surface {
        name: Ident,
        modality: Option<String>,
    },
}

impl SubjectKind {
    pub fn name(&self) -> &Ident {
        match self {
            SubjectKind::Page { route } => route,
            SubjectKind::Component { name } | SubjectKind::Surface { name, .. } => name,
        }
    }

    pub fn describe(&self) -> &'static str {
        match self {
            SubjectKind::Page { .. } => "page",
            SubjectKind::Component { .. } => "component",
            SubjectKind::Surface { .. } => "surface",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RouteInfo {
    /// Static segments and params in path order.
    pub segments: Vec<RouteSeg>,
    pub params: Vec<Ident>,
    pub file: FileId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteSeg {
    Static(String),
    Param(Ident),
}

#[derive(Clone, Debug)]
pub struct ProjInfo {
    pub port: Ident,
    pub ty: Ty,
    /// Key type for keyed projections.
    pub key: Option<Ty>,
    pub boot: bool,
}

#[derive(Clone, Debug)]
pub struct CmdInfo {
    pub port: Ident,
    /// Payload fields, contract order.
    pub payload: Vec<(Ident, Ty)>,
}

/// The typed environment of one definition file.
pub struct DefEnv {
    pub kind: SubjectKind,
    pub file: FileId,
    /// Index into the pipeline's `Vec<ParsedSource>`.
    pub source: usize,
    pub component_imports: BTreeMap<Ident, Span>,
    pub surface_imports: BTreeMap<Ident, Span>,
    /// Imported `type` items → structural type.
    pub type_items: BTreeMap<Ident, Ty>,
    pub projections: BTreeMap<Ident, ProjInfo>,
    pub commands: BTreeMap<Ident, CmdInfo>,
    pub props: BTreeMap<Ident, Ty>,
    pub params: BTreeMap<Ident, Ty>,
    pub state: BTreeMap<Ident, Ty>,
    /// Component emit signatures (declared `emits { … }`).
    pub emits: BTreeMap<Ident, Vec<(Ident, Ty)>>,
    /// Machine-event signatures (pages/surfaces): first handler's params.
    /// Filled by the typecheck pass.
    pub events: BTreeMap<Ident, Vec<(Ident, Ty)>>,
}

/// Everything resolution produces.
pub struct Resolved {
    pub routes: BTreeMap<Ident, RouteInfo>,
    /// Definition environments keyed by (kind-discriminated) name.
    pub pages: BTreeMap<Ident, DefEnv>,
    pub components: BTreeMap<Ident, DefEnv>,
    pub surfaces: BTreeMap<Ident, DefEnv>,
    /// Examples file index → subject source index.
    pub example_subjects: BTreeMap<usize, usize>,
    /// Loaded port contracts + expansions, keyed by port name.
    pub ports: BTreeMap<Ident, (PortContract, PortTypes)>,
}

pub fn resolve(
    sources: &[ParsedSource],
    ports: BTreeMap<Ident, (PortContract, PortTypes)>,
    entry: &Ident,
    manifest_file: FileId,
    diags: &mut Vec<Diagnostic>,
) -> Resolved {
    let mut routes = BTreeMap::new();
    let mut pages = BTreeMap::new();
    let mut components = BTreeMap::new();
    let mut surfaces = BTreeMap::new();

    // Global uniqueness of projection/command names across ports — what
    // lets `X` key snapshots by bare name (§7.1) and envelopes route
    // without qualification.
    let mut seen: BTreeMap<&Ident, &Ident> = BTreeMap::new();
    for (port_name, (contract, _)) in &ports {
        for item in contract.projections.keys().chain(contract.commands.keys()) {
            if let Some(other) = seen.insert(item, port_name) {
                diags.push(Diagnostic::error(
                    codes::PORT_NAME_COLLISION.0,
                    codes::PORT_NAME_COLLISION.1,
                    format!(
                        "`{item}` is declared by both port `{other}` and port `{port_name}`; \
                         projection and command names are app-global"
                    ),
                    Span::new(manifest_file, 0, 0),
                ));
            }
        }
    }

    // ── subjects: path discipline, headers, routes ─────────────────────
    for (idx, src) in sources.iter().enumerate() {
        let Parsed::Module(ast) = &src.parsed else {
            continue;
        };
        let Some(kind) = subject_kind(src, ast, diags) else {
            continue;
        };
        let env = build_env(idx, src, ast, &kind, &ports, diags);
        match kind {
            SubjectKind::Page { ref route } => {
                let info = route_info(&src.rel_path, src.file);
                if let Some(existing) = routes.get(route) {
                    let existing: &RouteInfo = existing;
                    diags.push(
                        Diagnostic::error(
                            codes::ROUTE_COLLISION.0,
                            codes::ROUTE_COLLISION.1,
                            format!("route `{route}` is defined twice"),
                            Span::new(src.file, 0, 0),
                        )
                        .with_label(Span::new(existing.file, 0, 0), "also defined here"),
                    );
                } else {
                    routes.insert(route.clone(), info);
                    pages.insert(route.clone(), env);
                }
            }
            SubjectKind::Component { ref name } => {
                components.insert(name.clone(), env);
            }
            SubjectKind::Surface { ref name, .. } => {
                surfaces.insert(name.clone(), env);
            }
        }
    }

    // Components and surfaces share the importable namespace; a collision
    // would make `use` ambiguous.
    for name in components.keys() {
        if surfaces.contains_key(name) {
            let file = components[name].file;
            diags.push(Diagnostic::error(
                codes::SHADOWED_NAME.0,
                codes::SHADOWED_NAME.1,
                format!("`{name}` is defined as both a component and a surface"),
                Span::new(file, 0, 0),
            ));
        }
    }

    if !routes.contains_key(entry) {
        diags.push(Diagnostic::error(
            codes::ENTRY_ROUTE_MISSING.0,
            codes::ENTRY_ROUTE_MISSING.1,
            format!("manifest entry route `{entry}` has no `app/…/page.uhura`"),
            Span::new(manifest_file, 0, 0),
        ));
    }

    // ── import validation + DAG ────────────────────────────────────────
    for env in pages
        .values()
        .chain(components.values())
        .chain(surfaces.values())
    {
        for (name, span) in &env.component_imports {
            if !components.contains_key(name) {
                let mut d = Diagnostic::error(
                    codes::UNKNOWN_IMPORT.0,
                    codes::UNKNOWN_IMPORT.1,
                    format!("no component named `{name}` in the corpus"),
                    *span,
                );
                if let Some(suggestion) = did_you_mean(name, components.keys()) {
                    d = d.with_note(format!("did you mean `{suggestion}`?"));
                }
                if surfaces.contains_key(name) {
                    d = d.with_note(format!("`{name}` is a surface — `use surface {name}`"));
                }
                diags.push(d);
            }
        }
        for (name, span) in &env.surface_imports {
            if !surfaces.contains_key(name) {
                let mut d = Diagnostic::error(
                    codes::UNKNOWN_IMPORT.0,
                    codes::UNKNOWN_IMPORT.1,
                    format!("no surface named `{name}` in the corpus"),
                    *span,
                );
                if components.contains_key(name) {
                    d = d.with_note(format!("`{name}` is a component — `use component {name}`"));
                }
                diags.push(d);
            }
        }
    }

    // Import cycles over the component/surface graph (pages are roots,
    // never importable).
    let mut graph: BTreeMap<&Ident, Vec<&Ident>> = BTreeMap::new();
    for (name, env) in components.iter().chain(surfaces.iter()) {
        graph.insert(
            name,
            env.component_imports
                .keys()
                .chain(env.surface_imports.keys())
                .collect(),
        );
    }
    for start in graph.keys().copied() {
        let mut stack: Vec<&Ident> = vec![start];
        let mut visited = BTreeSet::new();
        while let Some(n) = stack.pop() {
            if !visited.insert(n) {
                continue;
            }
            for dep in graph.get(n).into_iter().flatten() {
                if *dep == start {
                    let file = components
                        .get(start)
                        .or_else(|| surfaces.get(start))
                        .map(|e| e.file)
                        .expect("start came from the graph");
                    diags.push(Diagnostic::error(
                        codes::IMPORT_CYCLE.0,
                        codes::IMPORT_CYCLE.1,
                        format!(
                            "`{start}` reaches itself through its imports (the graph must be a DAG)"
                        ),
                        Span::new(file, 0, 0),
                    ));
                    stack.clear();
                    break;
                }
                stack.push(dep);
            }
        }
    }

    // ── examples pairing ───────────────────────────────────────────────
    let mut by_path: BTreeMap<&str, usize> = BTreeMap::new();
    for (idx, src) in sources.iter().enumerate() {
        if src.kind == SourceKind::Module {
            by_path.insert(&src.rel_path, idx);
        }
    }
    let mut example_subjects = BTreeMap::new();
    for (idx, src) in sources.iter().enumerate() {
        if src.kind != SourceKind::Examples {
            continue;
        }
        let subject_path = src.rel_path.replace(".examples.uhura", ".uhura");
        match by_path.get(subject_path.as_str()) {
            Some(&subject) => {
                example_subjects.insert(idx, subject);
            }
            None => diags.push(Diagnostic::error(
                codes::ORPHAN_EXAMPLES_FILE.0,
                codes::ORPHAN_EXAMPLES_FILE.1,
                format!("no subject `{subject_path}` for this examples file"),
                Span::new(src.file, 0, 0),
            )),
        }
    }

    Resolved {
        routes,
        pages,
        components,
        surfaces,
        example_subjects,
        ports,
    }
}

/// Decides what a module file defines and enforces the path/header
/// discipline (§3: path defines, header matches basename).
fn subject_kind(
    src: &ParsedSource,
    ast: &ast::File,
    diags: &mut Vec<Diagnostic>,
) -> Option<SubjectKind> {
    let dir = src.rel_path.split('/').next().unwrap_or("");
    let basename = src
        .rel_path
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".uhura");

    let mismatch = |diags: &mut Vec<Diagnostic>, span: Span, msg: String| {
        diags.push(Diagnostic::error(
            codes::HEADER_BASENAME_MISMATCH.0,
            codes::HEADER_BASENAME_MISMATCH.1,
            msg,
            span,
        ));
    };

    match &ast.kind {
        ast::DefKind::Error { .. } => None,
        ast::DefKind::Page { span } => {
            if dir != "app" || basename != "page" {
                diags.push(Diagnostic::error(
                    codes::WRONG_DIRECTORY.0,
                    codes::WRONG_DIRECTORY.1,
                    "a `page` lives at `app/**/page.uhura`".to_string(),
                    *span,
                ));
                return None;
            }
            let route = route_name(&src.rel_path)?;
            let route = match Ident::new(&route) {
                Ok(r) => r,
                Err(_) => {
                    diags.push(Diagnostic::error(
                        codes::BAD_PAGE_PATH.0,
                        codes::BAD_PAGE_PATH.1,
                        format!("`{}` does not yield a kebab-case route name", src.rel_path),
                        *span,
                    ));
                    return None;
                }
            };
            Some(SubjectKind::Page { route })
        }
        ast::DefKind::Component { name, span } => {
            if dir != "components" {
                diags.push(Diagnostic::error(
                    codes::WRONG_DIRECTORY.0,
                    codes::WRONG_DIRECTORY.1,
                    "a `component` lives under `components/`".to_string(),
                    *span,
                ));
                return None;
            }
            if name != basename {
                mismatch(
                    diags,
                    *span,
                    format!("header says `component {name}` but the file is `{basename}.uhura`"),
                );
                return None;
            }
            let name = Ident::new(name).ok()?;
            Some(SubjectKind::Component { name })
        }
        ast::DefKind::Surface {
            name,
            modality,
            span,
        } => {
            if dir != "surfaces" {
                diags.push(Diagnostic::error(
                    codes::WRONG_DIRECTORY.0,
                    codes::WRONG_DIRECTORY.1,
                    "a `surface` lives under `surfaces/`".to_string(),
                    *span,
                ));
                return None;
            }
            if name != basename {
                mismatch(
                    diags,
                    *span,
                    format!("header says `surface {name}` but the file is `{basename}.uhura`"),
                );
                return None;
            }
            let name = Ident::new(name).ok()?;
            Some(SubjectKind::Surface {
                name,
                modality: modality.clone(),
            })
        }
    }
}

/// `app/profile/[user]/page.uhura` → route name `profile`.
fn route_name(rel_path: &str) -> Option<String> {
    let statics: Vec<&str> = rel_path
        .strip_prefix("app/")?
        .strip_suffix("/page.uhura")?
        .split('/')
        .filter(|seg| !seg.starts_with('['))
        .collect();
    if statics.is_empty() {
        None
    } else {
        Some(statics.join("-"))
    }
}

fn route_info(rel_path: &str, file: FileId) -> RouteInfo {
    let mut segments = Vec::new();
    let mut params = Vec::new();
    if let Some(inner) = rel_path
        .strip_prefix("app/")
        .and_then(|p| p.strip_suffix("/page.uhura"))
    {
        for seg in inner.split('/') {
            if let Some(param) = seg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                if let Ok(param) = Ident::new(param) {
                    segments.push(RouteSeg::Param(param.clone()));
                    params.push(param);
                }
            } else {
                segments.push(RouteSeg::Static(seg.to_string()));
            }
        }
    }
    RouteInfo {
        segments,
        params,
        file,
    }
}

/// Builds the typed environment: imports, port items, declared props/
/// params/state — plus the §3 declaration-placement rules.
fn build_env(
    source: usize,
    src: &ParsedSource,
    ast: &ast::File,
    kind: &SubjectKind,
    ports: &BTreeMap<Ident, (PortContract, PortTypes)>,
    diags: &mut Vec<Diagnostic>,
) -> DefEnv {
    let file = src.file;
    let mut env = DefEnv {
        kind: kind.clone(),
        file,
        source,
        component_imports: BTreeMap::new(),
        surface_imports: BTreeMap::new(),
        type_items: BTreeMap::new(),
        projections: BTreeMap::new(),
        commands: BTreeMap::new(),
        props: BTreeMap::new(),
        params: BTreeMap::new(),
        state: BTreeMap::new(),
        emits: BTreeMap::new(),
        events: BTreeMap::new(),
    };

    // Expression-namespace shadowing ledger: state, props, params, and
    // projections share one unqualified namespace (§3 shadowing forbidden).
    let mut expr_names: BTreeMap<Ident, Span> = BTreeMap::new();
    let claim = |diags: &mut Vec<Diagnostic>,
                 expr_names: &mut BTreeMap<Ident, Span>,
                 name: &Ident,
                 span: Span,
                 what: &str| {
        if let Some(prev) = expr_names.insert(name.clone(), span) {
            diags.push(
                Diagnostic::error(
                    codes::SHADOWED_NAME.0,
                    codes::SHADOWED_NAME.1,
                    format!("`{name}` ({what}) shadows an earlier declaration"),
                    span,
                )
                .with_label(prev, "first declared here"),
            );
            false
        } else {
            true
        }
    };

    for use_decl in &ast.uses {
        match use_decl {
            ast::Use::Component { name, span, .. } => {
                let Ok(name) = Ident::new(name) else { continue };
                if env.component_imports.insert(name.clone(), *span).is_some() {
                    duplicate_import(diags, &name, *span);
                }
            }
            ast::Use::Surface { name, span, .. } => {
                let Ok(name) = Ident::new(name) else { continue };
                if env.surface_imports.insert(name.clone(), *span).is_some() {
                    duplicate_import(diags, &name, *span);
                }
            }
            ast::Use::Fixture { span, .. } => {
                diags.push(Diagnostic::error(
                    codes::MISPLACED_DECLARATION.0,
                    codes::MISPLACED_DECLARATION.1,
                    "`use fixture` belongs in `.examples.uhura` files only".to_string(),
                    *span,
                ));
            }
            ast::Use::Port {
                name, items, span, ..
            } => {
                let Ok(port_name) = Ident::new(name) else {
                    continue;
                };
                let Some((contract, port_types)) = ports.get(&port_name) else {
                    diags.push(Diagnostic::error(
                        codes::UNKNOWN_PORT.0,
                        codes::UNKNOWN_PORT.1,
                        format!("no port `{port_name}` in the manifest"),
                        *span,
                    ));
                    continue;
                };
                for item in items {
                    let Ok(item_name) = Ident::new(&item.name) else {
                        continue;
                    };
                    match item.kind {
                        ast::PortItemKind::Projection => {
                            match contract.projections.get(&item_name) {
                                None => unknown_port_item(
                                    diags,
                                    "projection",
                                    &item_name,
                                    &port_name,
                                    item.span,
                                ),
                                Some(decl) => {
                                    if claim(
                                        diags,
                                        &mut expr_names,
                                        &item_name,
                                        item.span,
                                        "projection",
                                    ) {
                                        env.projections.insert(
                                            item_name.clone(),
                                            ProjInfo {
                                                port: port_name.clone(),
                                                ty: port_types.from_expr(contract, &decl.ty),
                                                key: decl
                                                    .key
                                                    .as_ref()
                                                    .map(|k| port_types.from_expr(contract, k)),
                                                boot: decl.boot,
                                            },
                                        );
                                    }
                                }
                            }
                        }
                        ast::PortItemKind::Command => match contract.commands.get(&item_name) {
                            None => unknown_port_item(
                                diags, "command", &item_name, &port_name, item.span,
                            ),
                            Some(decl) => {
                                if env.commands.contains_key(&item_name) {
                                    duplicate_import(diags, &item_name, item.span);
                                } else {
                                    env.commands.insert(
                                        item_name.clone(),
                                        CmdInfo {
                                            port: port_name.clone(),
                                            payload: decl
                                                .payload
                                                .iter()
                                                .map(|(f, ty)| {
                                                    (f.clone(), port_types.from_expr(contract, ty))
                                                })
                                                .collect(),
                                        },
                                    );
                                }
                            }
                        },
                        ast::PortItemKind::Type => match port_types.named(&item_name) {
                            None => {
                                unknown_port_item(diags, "type", &item_name, &port_name, item.span)
                            }
                            Some(ty) => {
                                if env.type_items.contains_key(&item_name) {
                                    duplicate_import(diags, &item_name, item.span);
                                } else {
                                    env.type_items.insert(item_name.clone(), ty.clone());
                                }
                            }
                        },
                    }
                }
            }
        }
    }

    // ── declaration placement (§4.1) ───────────────────────────────────
    let is_page = matches!(kind, SubjectKind::Page { .. });
    let is_component = matches!(kind, SubjectKind::Component { .. });
    if is_page && !ast.props.is_empty() {
        diags.push(Diagnostic::error(
            codes::MISPLACED_DECLARATION.0,
            codes::MISPLACED_DECLARATION.1,
            "pages take route `param`s, not `props`".to_string(),
            ast.props[0].span,
        ));
    }
    if !is_page && !ast.params.is_empty() {
        diags.push(Diagnostic::error(
            codes::MISPLACED_DECLARATION.0,
            codes::MISPLACED_DECLARATION.1,
            "`param` declares a route segment; only pages have routes".to_string(),
            ast.params[0].span,
        ));
    }
    if !is_component && !ast.emits.is_empty() {
        diags.push(Diagnostic::error(
            codes::MISPLACED_DECLARATION.0,
            codes::MISPLACED_DECLARATION.1,
            "only components declare `emits`; pages and surfaces consume them".to_string(),
            ast.emits[0].span,
        ));
    }
    if is_component && ast.store.is_some() {
        diags.push(Diagnostic::error(
            codes::STORE_NOT_ALLOWED.0,
            codes::STORE_NOT_ALLOWED.1,
            "components are pure templates — no `store` (design §4)".to_string(),
            ast.store.as_ref().expect("just checked").span,
        ));
    }

    for prop in &ast.props {
        let Ok(name) = Ident::new(&prop.name) else {
            continue;
        };
        let ty = source_type(&prop.ty, &env, diags);
        if claim(diags, &mut expr_names, &name, prop.span, "prop") {
            env.props.insert(name, ty);
        }
    }
    for param in &ast.params {
        let Ok(name) = Ident::new(&param.name) else {
            continue;
        };
        let ty = source_type(&param.ty, &env, diags);
        if claim(diags, &mut expr_names, &name, param.span, "param") {
            env.params.insert(name, ty);
        }
    }
    if let SubjectKind::Page { route } = kind {
        let info = route_info(&src.rel_path, file);
        let declared: BTreeSet<&Ident> = env.params.keys().collect();
        let path_params: BTreeSet<&Ident> = info.params.iter().collect();
        if declared != path_params {
            diags.push(Diagnostic::error(
                codes::PARAM_ROUTE_MISMATCH.0,
                codes::PARAM_ROUTE_MISMATCH.1,
                format!(
                    "route `{route}` has path params {:?} but the page declares {:?}",
                    info.params.iter().map(Ident::as_str).collect::<Vec<_>>(),
                    env.params.keys().map(Ident::as_str).collect::<Vec<_>>(),
                ),
                Span::new(file, 0, 0),
            ));
        }
    }

    for emit in &ast.emits {
        let Ok(name) = Ident::new(&emit.name) else {
            continue;
        };
        let mut sig = Vec::new();
        for (param_name, ty) in &emit.params {
            let Ok(param_name) = Ident::new(param_name) else {
                continue;
            };
            sig.push((param_name, source_type(ty, &env, diags)));
        }
        if env.emits.insert(name.clone(), sig).is_some() {
            diags.push(Diagnostic::error(
                codes::SHADOWED_NAME.0,
                codes::SHADOWED_NAME.1,
                format!("emit `{name}` is declared twice"),
                emit.span,
            ));
        }
    }

    if let Some(store) = &ast.store {
        for field in &store.state {
            let Ok(name) = Ident::new(&field.name) else {
                continue;
            };
            let ty = source_type(&field.ty, &env, diags);
            if claim(diags, &mut expr_names, &name, field.span, "state field") {
                env.state.insert(name, ty);
            } else {
                diags.push(Diagnostic::error(
                    codes::DUPLICATE_STATE_FIELD.0,
                    codes::DUPLICATE_STATE_FIELD.1,
                    format!(
                        "state field `{}` collides with another declaration",
                        field.name
                    ),
                    field.span,
                ));
            }
        }
    }

    env
}

/// Converts a source type expression (§4.3 grammar) against the file's
/// imported type items.
pub fn source_type(ty: &ast::TypeExpr, env: &DefEnv, diags: &mut Vec<Diagnostic>) -> Ty {
    match &ty.kind {
        ast::TypeKind::Error => Ty::Error,
        ast::TypeKind::Option(inner) => Ty::Option(Box::new(source_type(inner, env, diags))),
        ast::TypeKind::List(inner) => Ty::List(Box::new(source_type(inner, env, diags))),
        ast::TypeKind::Map(key, value) => {
            let key = match key.as_str() {
                "id" => MapKey::Id,
                "tag" => MapKey::Tag,
                other => {
                    diags.push(Diagnostic::error(
                        codes::BAD_MAP_KEY.0,
                        codes::BAD_MAP_KEY.1,
                        format!("map keys are `id` or `tag`, not `{other}` (§4.3)"),
                        ty.span,
                    ));
                    return Ty::Error;
                }
            };
            Ty::Map(key, Box::new(source_type(value, env, diags)))
        }
        ast::TypeKind::Name(name) => match name.as_str() {
            "bool" => Ty::Bool,
            "int" => Ty::Int,
            "text" => Ty::Text,
            "id" => Ty::Id,
            "tag" => Ty::Tag,
            other => {
                let Ok(ident) = Ident::new(other) else {
                    return Ty::Error;
                };
                match env.type_items.get(&ident) {
                    Some(t) => t.clone(),
                    None => {
                        let mut d = Diagnostic::error(
                            codes::UNRESOLVED_NAME.0,
                            codes::UNRESOLVED_NAME.1,
                            format!("`{other}` is not a builtin type or an imported `type` item"),
                            ty.span,
                        );
                        if let Some(s) = did_you_mean(&ident, env.type_items.keys()) {
                            d = d.with_note(format!("did you mean `{s}`?"));
                        } else {
                            d = d.with_note(
                                "import contract types explicitly: `use port <p> { type <t> }`"
                                    .to_string(),
                            );
                        }
                        diags.push(d);
                        Ty::Error
                    }
                }
            }
        },
    }
}

fn duplicate_import(diags: &mut Vec<Diagnostic>, name: &Ident, span: Span) {
    diags.push(Diagnostic::error(
        codes::DUPLICATE_IMPORT.0,
        codes::DUPLICATE_IMPORT.1,
        format!("`{name}` is imported twice"),
        span,
    ));
}

fn unknown_port_item(
    diags: &mut Vec<Diagnostic>,
    what: &str,
    item: &Ident,
    port: &Ident,
    span: Span,
) {
    diags.push(Diagnostic::error(
        codes::UNKNOWN_PORT_ITEM.0,
        codes::UNKNOWN_PORT_ITEM.1,
        format!("port `{port}` declares no {what} `{item}`"),
        span,
    ));
}

/// Closest name by edit distance ≤ 2, for did-you-mean notes.
pub fn did_you_mean<'a>(
    target: &Ident,
    candidates: impl Iterator<Item = &'a Ident>,
) -> Option<&'a Ident> {
    candidates
        .map(|c| (edit_distance(target.as_str(), c.as_str()), c))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current.push((prev[j] + cost).min(prev[j + 1] + 1).min(current[j] + 1));
        }
        prev = current;
    }
    prev[b.len()]
}
