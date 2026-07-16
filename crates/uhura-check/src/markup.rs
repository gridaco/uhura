//! The markup rules (§4.4, §4.8, §10): catalog authority, event
//! eligibility, children models, nested interactives, controlled
//! promotion, a11y completeness, one-root, the emit binding model, and the
//! availability-match requirement. Expressions inside markup type through
//! `Typer` with the view-position projection rule armed.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Diagnostic, Ident, Span, codes};
use uhura_syntax::ast;

use crate::catalog::{Catalog, ChildrenModel, ElementClass, PropType};
use crate::infer::Typer;
use crate::resolve::{DefEnv, Resolved, SubjectKind, did_you_mean};
use crate::types::{MapKey, Ty};

/// The single checker-wide classification for a markup element name.
///
/// An explicit component import normally selects the component. If that name
/// is also owned by the catalog, the reference is ambiguous and checking must
/// reject it instead of letting later passes choose different meanings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementResolution {
    CatalogElement,
    ImportedComponent,
    UnimportedComponent,
    Ambiguous,
    Unknown,
}

pub(crate) fn resolve_element(
    name: &Ident,
    component_imported: bool,
    resolved: &Resolved,
    catalog: Option<&Catalog>,
) -> ElementResolution {
    let is_component = resolved.components.contains_key(name);
    let is_catalog = catalog.is_some_and(|catalog| catalog.elements.contains_key(name));
    match (is_component, component_imported, is_catalog) {
        (true, true, true) => ElementResolution::Ambiguous,
        (true, true, false) => ElementResolution::ImportedComponent,
        (_, _, true) => ElementResolution::CatalogElement,
        (true, false, false) => ElementResolution::UnimportedComponent,
        (false, _, false) => ElementResolution::Unknown,
    }
}

/// Documented patterns for things that are deliberately not elements (§10);
/// surfaced as notes on `unknown-element`.
const UNKNOWN_ELEMENT_NOTES: &[(&str, &str)] = &[
    (
        "image",
        "`<image>` was renamed to `<img>`; the old spelling is not a compatibility alias",
    ),
    (
        "text-field",
        "`<text-field>` was renamed to `<textfield>`; the old spelling is not a compatibility alias",
    ),
    (
        "avatar",
        "the avatar pattern is `<img class=…>` — see docs/widgets/patterns",
    ),
    (
        "card",
        "the card pattern is `<view class=…>` — see docs/widgets/patterns",
    ),
    (
        "column",
        "layout is CSS: `<view class=…>` with flex-direction",
    ),
    ("row", "layout is CSS: `<view class=…>` with flex-direction"),
    ("stack", "layout is CSS: `<view class=…>`"),
    ("grid", "layout is CSS: `<view class=…>` with display: grid"),
    (
        "spacer",
        "spacing is CSS: gap/padding on the parent `<view>`",
    ),
    ("list", "`<view role=\"list\">` with one keyed `{#each}`"),
    (
        "sheet",
        "sheets are surfaces — `surface <name> modality sheet`",
    ),
    ("dialog", "dialogs are surfaces (core surface stack)"),
    (
        "video",
        "video is deferred: poster `<img>` + `video-off` badge pattern",
    ),
];

/// What one definition's markup walk produces for later passes.
pub struct MarkupFacts {
    /// Class names referenced from markup (for the style existence check).
    pub class_refs: Vec<(String, Span)>,
}

struct EmitUse {
    name: Ident,
    on_supplementary_region: bool,
}

pub struct MarkupChecker<'a> {
    pub typer: Typer<'a>,
    pub catalog: &'a Catalog,
    /// Component name → its expansion contains an interactive element.
    pub interactive_memo: &'a BTreeMap<Ident, bool>,
    class_refs: Vec<(String, Span)>,
    emit_uses: Vec<EmitUse>,
}

pub fn check_markup(
    env: &DefEnv,
    resolved: &Resolved,
    catalog: &Catalog,
    interactive_memo: &BTreeMap<Ident, bool>,
    markup: &ast::MarkupList,
    file_span: Span,
    diags: &mut Vec<Diagnostic>,
) -> MarkupFacts {
    let mut typer = Typer::new(env, resolved, diags);
    typer.in_view = true;
    let mut checker = MarkupChecker {
        typer,
        catalog,
        interactive_memo,
        class_refs: Vec::new(),
        emit_uses: Vec::new(),
    };

    // One root, and it must be keyable (§4.4/§8.1).
    let roots: Vec<&ast::Node> = markup
        .iter()
        .filter(|n| !matches!(n, ast::Node::Error { .. }))
        .collect();
    if roots.len() != 1 {
        checker.typer.diags.push(Diagnostic::error(
            codes::ONE_ROOT.0,
            codes::ONE_ROOT.1,
            format!(
                "a definition has exactly one root element, found {}",
                roots.len()
            ),
            roots.get(1).map_or(file_span, |n| node_span(n)),
        ));
    }
    if let Some(root) = roots.first()
        && !matches!(root, ast::Node::Element(_) | ast::Node::Match { .. })
    {
        checker.typer.diags.push(Diagnostic::error(
            codes::ONE_ROOT.0,
            codes::ONE_ROOT.1,
            "the root must be an element (or a `{#match}` whose arms each have one root)"
                .to_string(),
            node_span(root),
        ));
    }
    checker.walk_nodes(markup, false);

    // Supplementary regions need a same-named emit reachable from a
    // focusable element (§10 — name-level check).
    let focusable: BTreeSet<&Ident> = checker
        .emit_uses
        .iter()
        .filter(|u| !u.on_supplementary_region)
        .map(|u| &u.name)
        .collect();
    let mut flagged = BTreeSet::new();
    for emit_use in &checker.emit_uses {
        if emit_use.on_supplementary_region
            && !focusable.contains(&emit_use.name)
            && flagged.insert(emit_use.name.clone())
        {
            checker.typer.diags.push(Diagnostic::error(
                codes::SUPPLEMENTARY_UNREACHABLE.0,
                codes::SUPPLEMENTARY_UNREACHABLE.1,
                format!(
                    "`{}` is only reachable through a supplementary region; a focusable \
                     element in this definition must also emit it (§10)",
                    emit_use.name
                ),
                file_span,
            ));
        }
    }

    MarkupFacts {
        class_refs: checker.class_refs,
    }
}

fn node_span(node: &ast::Node) -> Span {
    match node {
        ast::Node::Element(el) => el.span,
        ast::Node::Text { span, .. }
        | ast::Node::If { span, .. }
        | ast::Node::Each { span, .. }
        | ast::Node::Match { span, .. }
        | ast::Node::Error { span } => *span,
    }
}

impl MarkupChecker<'_> {
    fn error(&mut self, code: (&'static str, &'static str), message: String, span: Span) {
        self.typer
            .diags
            .push(Diagnostic::error(code.0, code.1, message, span));
    }

    fn walk_nodes(&mut self, nodes: &ast::MarkupList, in_interactive: bool) {
        for node in nodes {
            self.walk_node(node, in_interactive);
        }
    }

    fn walk_node(&mut self, node: &ast::Node, in_interactive: bool) {
        match node {
            ast::Node::Error { .. } => {}
            ast::Node::Text { span, .. } => {
                self.error(
                    codes::INTERP_OUTSIDE_TEXT,
                    "text content (and `{expr}` interpolation) lives inside `<text>` only (§4.4)"
                        .to_string(),
                    *span,
                );
            }
            ast::Node::If {
                cond, then, els, ..
            } => {
                self.typer.check(cond, &Ty::Bool);
                self.walk_nodes(then, in_interactive);
                if let Some(els) = els {
                    self.walk_nodes(els, in_interactive);
                }
            }
            ast::Node::Each { .. } => self.walk_each(node, in_interactive),
            ast::Node::Match { .. } => self.walk_match(node, in_interactive),
            ast::Node::Element(el) => {
                let Ok(name) = Ident::new(&el.name) else {
                    return;
                };
                match resolve_element(
                    &name,
                    self.typer.env.component_imports.contains_key(&name),
                    self.typer.resolved,
                    Some(self.catalog),
                ) {
                    ElementResolution::CatalogElement => {
                        self.walk_element(el, &name, in_interactive);
                    }
                    ElementResolution::ImportedComponent
                    | ElementResolution::UnimportedComponent => {
                        self.walk_component_call(el, &name, in_interactive);
                    }
                    ElementResolution::Ambiguous => self.typer.diags.push(
                        Diagnostic::error(
                            codes::SHADOWED_NAME.0,
                            codes::SHADOWED_NAME.1,
                            format!(
                                "`<{name}>` is ambiguous: an imported component shadows a catalog element with the same name"
                            ),
                            el.span,
                        )
                        .with_label(
                            self.typer.env.component_imports[&name],
                            "component imported here",
                        ),
                    ),
                    ElementResolution::Unknown => {
                        let mut d = Diagnostic::error(
                            codes::UNKNOWN_ELEMENT.0,
                            codes::UNKNOWN_ELEMENT.1,
                            format!(
                                "`<{name}>` is neither a catalog element nor an imported component"
                            ),
                            el.span,
                        );
                        if let Some((_, note)) =
                            UNKNOWN_ELEMENT_NOTES
                                .iter()
                                .find(|(p, _)| *p == name.as_str())
                        {
                            d = d.with_note((*note).to_string());
                        } else if let Some(s) = did_you_mean(
                            &name,
                            self.catalog
                                .elements
                                .keys()
                                .chain(self.typer.resolved.components.keys()),
                        ) {
                            d = d.with_note(format!("did you mean `<{s}>`?"));
                        }
                        if self.typer.resolved.surfaces.contains_key(&name) {
                            d = d.with_note(format!(
                                "`{name}` is a surface — surfaces mount via `open-surface`, \
                                 not markup"
                            ));
                        }
                        self.typer.diags.push(d);
                    }
                }
            }
        }
    }

    // ── {#each} ────────────────────────────────────────────────────────

    fn walk_each(&mut self, node: &ast::Node, in_interactive: bool) {
        let ast::Node::Each {
            item,
            seq,
            key,
            body,
            span,
            ..
        } = node
        else {
            return;
        };
        let seq_ty = self.typer.infer(seq);
        let item_ty = match seq_ty {
            Ty::List(t) => *t,
            Ty::Map(k, _) => match k {
                MapKey::Id => Ty::Id,
                MapKey::Tag => Ty::Tag,
            },
            Ty::Error => Ty::Error,
            other => {
                self.error(
                    codes::BAD_OPERAND,
                    format!(
                        "`{{#each}}` iterates lists (items) and maps (keys), not {}",
                        other.describe()
                    ),
                    *span,
                );
                Ty::Error
            }
        };
        let mark = self.typer.locals.len();
        self.typer.push_local(item, item_ty, *span);
        let key_ty = self.typer.infer(key);
        if !matches!(key_ty, Ty::Id | Ty::Tag | Ty::Text | Ty::Int | Ty::Error) {
            self.error(
                codes::TYPE_MISMATCH,
                format!(
                    "each-keys are identity values (id | tag | text | int), got {}",
                    key_ty.describe()
                ),
                *span,
            );
        }
        self.walk_nodes(body, in_interactive);
        self.typer.truncate_locals(mark);
    }

    // ── {#match} ───────────────────────────────────────────────────────

    fn walk_match(&mut self, node: &ast::Node, in_interactive: bool) {
        let ast::Node::Match {
            scrutinee,
            arms,
            span,
            ..
        } = node
        else {
            return;
        };
        if let Some(proj_ty) = self.availability_scrutinee(scrutinee) {
            self.walk_availability_arms(arms, &proj_ty, *span, in_interactive);
        } else {
            let ty = self.typer.infer(scrutinee);
            match ty {
                Ty::Union(variants) => {
                    self.walk_union_arms(arms, &variants, *span, in_interactive);
                }
                Ty::Error => {
                    for arm in arms {
                        let mark = self.typer.locals.len();
                        if let Some(binding) = &arm.binding {
                            self.typer.push_local(binding, Ty::Error, arm.span);
                        }
                        self.walk_nodes(&arm.body, in_interactive);
                        self.typer.truncate_locals(mark);
                    }
                }
                other => {
                    self.error(
                        codes::BAD_UNION_ARMS,
                        format!(
                            "`{{#match}}` works on port unions and projection availability, \
                             not {}",
                            other.describe()
                        ),
                        *span,
                    );
                }
            }
        }
    }

    /// A scrutinee that is a projection read makes this an availability
    /// match (§9.2); returns the projection's value type.
    fn availability_scrutinee(&mut self, scrutinee: &ast::Expr) -> Option<Ty> {
        match &scrutinee.kind {
            ast::ExprKind::Ident(name) => {
                let ident = Ident::new(name).ok()?;
                let proj = self.typer.env.projections.get(&ident)?;
                if proj.key.is_some() {
                    self.error(
                        codes::WRONG_ARGS,
                        format!("projection `{ident}` is keyed — match on `{ident}(<key>)`"),
                        scrutinee.span,
                    );
                    return Some(Ty::Error);
                }
                Some(proj.ty.clone())
            }
            ast::ExprKind::Call { name, args } => {
                let ident = Ident::new(name).ok()?;
                let proj = self.typer.env.projections.get(&ident)?;
                let ty = proj.ty.clone();
                let key = proj.key.clone();
                match key {
                    Some(key_ty) if args.len() == 1 => {
                        // The key expression types in *non-view* position:
                        // it is data feeding the read, not a read itself.
                        self.typer.in_view = false;
                        self.typer.check(&args[0], &key_ty);
                        self.typer.in_view = true;
                    }
                    _ => {
                        self.error(
                            codes::WRONG_ARGS,
                            format!("keyed read `{ident}(<key>)` takes exactly one key"),
                            scrutinee.span,
                        );
                    }
                }
                Some(ty)
            }
            _ => None,
        }
    }

    fn walk_availability_arms(
        &mut self,
        arms: &[ast::MatchArm],
        ready_ty: &Ty,
        span: Span,
        in_interactive: bool,
    ) {
        let mut seen = BTreeSet::new();
        for arm in arms {
            let variant = match &arm.pattern {
                ast::MatchPattern::Else => {
                    self.error(
                        codes::BAD_AVAILABILITY_ARMS,
                        "availability matches spell out `loading | failed | ready` — no `{:else}`"
                            .to_string(),
                        arm.span,
                    );
                    continue;
                }
                ast::MatchPattern::Variant(v) => v.as_str(),
            };
            if !seen.insert(variant.to_string()) {
                self.error(
                    codes::BAD_AVAILABILITY_ARMS,
                    format!("duplicate `{{:when {variant}}}` arm"),
                    arm.span,
                );
            }
            let binding_ty = match variant {
                "loading" => {
                    if arm.binding.is_some() {
                        self.error(
                            codes::BAD_AVAILABILITY_ARMS,
                            "`loading` carries no value to bind".to_string(),
                            arm.span,
                        );
                    }
                    None
                }
                "failed" => Some(Ty::Text),
                "ready" => Some(ready_ty.clone()),
                other => {
                    self.error(
                        codes::BAD_AVAILABILITY_ARMS,
                        format!(
                            "`{other}` is not an availability arm (loading | failed | ready — \
                             §9.2: these are language arms, never contract types)"
                        ),
                        arm.span,
                    );
                    None
                }
            };
            let mark = self.typer.locals.len();
            if let (Some(binding), Some(ty)) = (&arm.binding, binding_ty) {
                self.typer.push_local(binding, ty, arm.span);
            }
            self.walk_nodes(&arm.body, in_interactive);
            self.typer.truncate_locals(mark);
        }
        for required in ["loading", "failed", "ready"] {
            if !seen.contains(required) {
                self.error(
                    codes::BAD_AVAILABILITY_ARMS,
                    format!(
                        "availability match is missing its `{{:when {required}}}` arm — \
                         absence is a state the design must show (§9.2)"
                    ),
                    span,
                );
            }
        }
    }

    fn walk_union_arms(
        &mut self,
        arms: &[ast::MatchArm],
        variants: &BTreeMap<Ident, BTreeMap<Ident, Ty>>,
        span: Span,
        in_interactive: bool,
    ) {
        let mut seen: BTreeSet<Ident> = BTreeSet::new();
        let mut has_else = false;
        for arm in arms {
            let mark = self.typer.locals.len();
            match &arm.pattern {
                ast::MatchPattern::Else => {
                    has_else = true;
                }
                ast::MatchPattern::Variant(v) => {
                    if let Ok(variant) = Ident::new(v) {
                        match variants.get(&variant) {
                            Some(fields) => {
                                if !seen.insert(variant.clone()) {
                                    self.error(
                                        codes::BAD_UNION_ARMS,
                                        format!("duplicate `{{:when {variant}}}` arm"),
                                        arm.span,
                                    );
                                }
                                if let Some(binding) = &arm.binding {
                                    self.typer.push_local(
                                        binding,
                                        Ty::Record(fields.clone()),
                                        arm.span,
                                    );
                                }
                            }
                            None => {
                                let names: Vec<&str> = variants.keys().map(Ident::as_str).collect();
                                self.error(
                                    codes::BAD_UNION_ARMS,
                                    format!(
                                        "`{variant}` is not a variant (union has {})",
                                        names.join(" | ")
                                    ),
                                    arm.span,
                                );
                            }
                        }
                    }
                }
            }
            self.walk_nodes(&arm.body, in_interactive);
            self.typer.truncate_locals(mark);
        }
        if !has_else {
            for variant in variants.keys() {
                if !seen.contains(variant) {
                    self.error(
                        codes::BAD_UNION_ARMS,
                        format!(
                            "non-exhaustive match: `{variant}` is unhandled (add the arm or \
                             `{{:else}}`)"
                        ),
                        span,
                    );
                }
            }
        }
    }

    // ── catalog elements ───────────────────────────────────────────────

    fn walk_element(&mut self, el: &ast::Element, name: &Ident, in_interactive: bool) {
        let decl = self.catalog.elements[name].clone();

        if decl.class == ElementClass::Interactive && in_interactive {
            self.error(
                codes::NESTED_INTERACTIVE,
                format!("`<{name}>` cannot nest inside another interactive element (§10)"),
                el.span,
            );
        }

        // ── attributes ─────────────────────────────────────────────────
        let mut bound: BTreeMap<Ident, Span> = BTreeMap::new();
        let mut role_literal: Option<String> = None;
        for attr in &el.attrs {
            let Ok(attr_name) = Ident::new(&attr.name) else {
                continue;
            };
            if let Some(prev) = bound.insert(attr_name.clone(), attr.span) {
                self.typer.diags.push(
                    Diagnostic::error(
                        codes::DUPLICATE_ATTR.0,
                        codes::DUPLICATE_ATTR.1,
                        format!("`{attr_name}` is bound twice"),
                        attr.span,
                    )
                    .with_label(prev, "first bound here"),
                );
                continue;
            }
            if attr_name.as_str() == "class" {
                self.collect_class_attr(attr);
                continue;
            }
            let Some(prop) = decl.props.get(&attr_name) else {
                let mut d = Diagnostic::error(
                    codes::UNKNOWN_PROP.0,
                    codes::UNKNOWN_PROP.1,
                    format!("`<{name}>` has no semantic prop `{attr_name}`"),
                    attr.span,
                );
                if let Some(s) = did_you_mean(&attr_name, decl.props.keys()) {
                    d = d.with_note(format!("did you mean `{s}`?"));
                } else {
                    d = d.with_note(
                        "styling props do not exist — layout and aesthetics are CSS (§10)"
                            .to_string(),
                    );
                }
                self.typer.diags.push(d);
                continue;
            };
            if name.as_str() == "view"
                && attr_name.as_str() == "role"
                && let ast::AttrValue::Literal(v) = &attr.value
            {
                role_literal = Some(v.clone());
            }
            self.check_prop_value(name, &attr_name, &prop.ty, &attr.value, attr.span);
        }

        for (prop_name, prop) in &decl.props {
            if prop.required && !bound.contains_key(prop_name) {
                let in_xor_group = decl
                    .exactly_one_of
                    .iter()
                    .any(|group| group.contains(prop_name));
                if !in_xor_group {
                    self.error(
                        codes::MISSING_REQUIRED_PROP,
                        format!("`<{name}>` requires `{prop_name}`"),
                        el.span,
                    );
                }
            }
        }
        for group in &decl.exactly_one_of {
            let present = group.iter().filter(|p| bound.contains_key(*p)).count();
            if present != 1 {
                let names: Vec<&str> = group.iter().map(Ident::as_str).collect();
                self.error(
                    codes::A11Y_ALT,
                    format!("`<{name}>` takes exactly one of {}", names.join(" / ")),
                    el.span,
                );
                continue;
            }

            // An exactly-one boolean branch is selected by its presence, so a
            // false or dynamic value would contradict the structural choice.
            // Treat it as a bare marker while keeping ordinary bool props
            // expression-capable.
            for prop_name in group {
                if !bound.contains_key(prop_name)
                    || !matches!(decl.props[prop_name].ty, PropType::Bool)
                {
                    continue;
                }
                let Some(attr) = el.attrs.iter().find(|attr| attr.name == prop_name.as_str())
                else {
                    continue;
                };
                if !matches!(attr.value, ast::AttrValue::Bare) {
                    self.error(
                        codes::A11Y_ALT,
                        format!(
                            "`<{name}>`'s `{prop_name}` alternative is a presence marker — write bare `{prop_name}`"
                        ),
                        attr.span,
                    );
                }
            }
        }

        // ── events ─────────────────────────────────────────────────────
        let mut events_bound: BTreeSet<Ident> = BTreeSet::new();
        for event_attr in &el.events {
            let Ok(event_name) = Ident::new(&event_attr.event) else {
                continue;
            };
            let Some(event_decl) = decl.events.get(&event_name) else {
                let mut d = Diagnostic::error(
                    codes::EVENT_NOT_DECLARED.0,
                    codes::EVENT_NOT_DECLARED.1,
                    format!("`<{name}>` declares no `{event_name}` event"),
                    event_attr.span,
                );
                if decl.class == ElementClass::Layout && !decl.viewport {
                    d = d.with_note(
                        "`on:` never attaches to layout elements — wrap the content in \
                         `<region>` (§4.8; never auto-repaired)"
                            .to_string(),
                    );
                }
                self.typer.diags.push(d);
                continue;
            };
            events_bound.insert(event_name.clone());
            match &event_attr.binding {
                ast::EventBinding::Forward => {
                    self.error(
                        codes::ELEMENT_EVENT_NEEDS_EMIT,
                        format!(
                            "element events bind explicitly: \
                             `on:{event_name}={{emit <machine-event>(…)}}` (§4.4)"
                        ),
                        event_attr.span,
                    );
                }
                ast::EventBinding::Emit {
                    name: emit_name,
                    args,
                } => {
                    self.check_emit_binding(emit_name, args, &event_decl.carries, event_attr.span);
                    if let Ok(emit) = Ident::new(emit_name) {
                        self.emit_uses.push(EmitUse {
                            name: emit,
                            on_supplementary_region: name.as_str() == "region"
                                && bound.iter().any(|(b, _)| b.as_str() == "supplementary"),
                        });
                    }
                }
            }
        }

        // Controlled promotion (§10): binding `value` obligates `change`.
        if let Some((prop, event)) = &decl.controlled
            && bound.contains_key(prop)
            && !events_bound.contains(event)
        {
            self.error(
                codes::CONTROLLED_PROMOTION,
                format!(
                    "binding `{prop}` makes `<{name}>` controlled — it must handle \
                     `on:{event}` (§10)"
                ),
                el.span,
            );
        }

        // ── children ───────────────────────────────────────────────────
        let now_interactive = in_interactive || decl.class == ElementClass::Interactive;
        let children: Vec<&ast::Node> = el
            .children
            .iter()
            .filter(|c| !matches!(c, ast::Node::Error { .. }))
            .collect();
        match decl.children {
            ChildrenModel::Any => self.walk_nodes(&el.children, now_interactive),
            ChildrenModel::None => {
                if !children.is_empty() {
                    self.error(
                        codes::BAD_CHILDREN,
                        format!("`<{name}>` takes no children"),
                        el.span,
                    );
                }
            }
            ChildrenModel::Text => {
                for child in &children {
                    match child {
                        ast::Node::Text { runs, .. } => {
                            for run in runs {
                                if let ast::TextRun::Interp(expr) = run {
                                    self.typer.check(expr, &Ty::Text);
                                }
                            }
                        }
                        other => {
                            self.error(
                                codes::BAD_CHILDREN,
                                format!("`<{name}>` holds text runs only"),
                                node_span(other),
                            );
                        }
                    }
                }
            }
            ChildrenModel::Content => {
                for child in &children {
                    let ok = match child {
                        ast::Node::Element(child_el) => Ident::new(&child_el.name)
                            .ok()
                            .and_then(|n| self.catalog.elements.get(&n))
                            .is_some_and(|d| d.class == ElementClass::Content),
                        _ => false,
                    };
                    if !ok {
                        self.error(
                            codes::BAD_CHILDREN,
                            format!(
                                "`<{name}>` children are content elements (text / img / video / icon)"
                            ),
                            node_span(child),
                        );
                    }
                }
                self.walk_nodes(&el.children, now_interactive);
            }
            ChildrenModel::One => {
                if children.len() != 1 || !matches!(children.first(), Some(ast::Node::Element(_))) {
                    self.error(
                        codes::BAD_CHILDREN,
                        format!("`<{name}>` wraps exactly one element"),
                        el.span,
                    );
                }
                self.walk_nodes(&el.children, now_interactive);
            }
            ChildrenModel::KeyedEach => {
                if children.len() != 1 || !matches!(children.first(), Some(ast::Node::Each { .. }))
                {
                    self.error(
                        codes::BAD_CHILDREN,
                        format!(
                            "`<{name}>` children come from exactly one keyed `{{#each}}` (§10)"
                        ),
                        el.span,
                    );
                }
                self.walk_nodes(&el.children, now_interactive);
            }
        }

        // role="list" requires one keyed each (§10 a11y completeness).
        if role_literal.as_deref() == Some("list")
            && (children.len() != 1 || !matches!(children.first(), Some(ast::Node::Each { .. })))
        {
            self.error(
                codes::LIST_NEEDS_KEYED_EACH,
                "`role=\"list\"` promises list semantics — children come from exactly one \
                 keyed `{#each}` (§10)"
                    .to_string(),
                el.span,
            );
        }
    }

    fn check_prop_value(
        &mut self,
        element: &Ident,
        prop: &Ident,
        ty: &PropType,
        value: &ast::AttrValue,
        span: Span,
    ) {
        let expected = match ty {
            PropType::Text => Ty::Text,
            PropType::Bool => Ty::Bool,
            PropType::Int => Ty::Int,
            PropType::Asset => Ty::Asset,
            PropType::Enum(values) => Ty::Enum(values.clone()),
            PropType::Icon => Ty::Enum(self.catalog.icons.clone()),
        };
        match value {
            ast::AttrValue::Bare => {
                if !matches!(ty, PropType::Bool) {
                    self.error(
                        codes::TYPE_MISMATCH,
                        format!(
                            "bare `{prop}` means `true`; `<{element}>`'s `{prop}` is {}",
                            ty.describe()
                        ),
                        span,
                    );
                }
            }
            ast::AttrValue::Literal(s) => match ty {
                PropType::Icon => {
                    if let Ok(icon) = Ident::new(s) {
                        if !self.catalog.icons.contains(&icon) {
                            let mut d = Diagnostic::error(
                                codes::UNKNOWN_ICON.0,
                                codes::UNKNOWN_ICON.1,
                                format!("`{s}` is not in the catalog icon set"),
                                span,
                            );
                            if let Some(near) = did_you_mean(&icon, self.catalog.icons.iter()) {
                                d = d.with_note(format!("did you mean `{near}`?"));
                            }
                            self.typer.diags.push(d);
                        }
                    } else {
                        self.error(
                            codes::UNKNOWN_ICON,
                            format!("`{s}` is not an icon name"),
                            span,
                        );
                    }
                }
                PropType::Enum(values) => {
                    if !values.iter().any(|v| v.as_str() == s) {
                        let names: Vec<&str> = values.iter().map(Ident::as_str).collect();
                        self.error(
                            codes::TYPE_MISMATCH,
                            format!("`\"{s}\"` is not one of {}", names.join(" | ")),
                            span,
                        );
                    }
                }
                PropType::Text => {}
                other => {
                    self.error(
                        codes::TYPE_MISMATCH,
                        format!("`{prop}` is {}, not a text literal", other.describe()),
                        span,
                    );
                }
            },
            ast::AttrValue::Expr(expr) => self.typer.check(expr, &expected),
        }
    }

    /// `on:<event>={emit <machine-event>(args)}` on a catalog element:
    /// the target signature must equal author args ∪ carried fields (§4.2).
    fn check_emit_binding(
        &mut self,
        emit_name: &str,
        args: &[ast::Arg],
        carries: &BTreeMap<Ident, PropType>,
        span: Span,
    ) {
        let Ok(emit) = Ident::new(emit_name) else {
            return;
        };
        for arg in args {
            if Ident::new(&arg.name).is_ok_and(|a| carries.contains_key(&a)) {
                self.error(
                    codes::CARRIED_FIELD_NAMED,
                    format!(
                        "`{}` is carried by the renderer — the author may not bind it (§4.2)",
                        arg.name
                    ),
                    arg.span,
                );
            }
        }

        let signature = self.target_signature(&emit, span);
        let Some(signature) = signature else {
            for arg in args {
                self.typer.infer(&arg.value);
            }
            return;
        };

        // Author args typecheck against the signature.
        for arg in args {
            match signature.iter().find(|(n, _)| n.as_str() == arg.name) {
                Some((_, ty)) => {
                    let ty = ty.clone();
                    self.typer.check(&arg.value, &ty);
                }
                None => {
                    self.error(
                        codes::WRONG_ARGS,
                        format!("`{emit}` has no param `{}`", arg.name),
                        arg.span,
                    );
                    self.typer.infer(&arg.value);
                }
            }
        }
        // Coverage: args ∪ carries must equal the signature.
        for (param, param_ty) in &signature {
            let by_author = args.iter().any(|a| a.name == param.as_str());
            let by_carry = carries.get(param).map(|c| match c {
                PropType::Text => Ty::Text,
                PropType::Bool => Ty::Bool,
                _ => Ty::Int,
            });
            match (by_author, by_carry) {
                (true, _) => {}
                (false, Some(carry_ty)) => {
                    if carry_ty != *param_ty {
                        self.error(
                            codes::WRONG_ARGS,
                            format!(
                                "carried field `{param}` is {}, but the handler declares {}",
                                carry_ty.describe(),
                                param_ty.describe()
                            ),
                            span,
                        );
                    }
                }
                (false, None) => {
                    self.error(
                        codes::WRONG_ARGS,
                        format!("`{emit}` needs `{param}` (§4.2: payload = args ∪ carried fields)"),
                        span,
                    );
                }
            }
        }
    }

    /// The machine-event signature an emit targets: own handlers for
    /// pages/surfaces, the `emits` declaration for components.
    fn target_signature(&mut self, emit: &Ident, span: Span) -> Option<Vec<(Ident, Ty)>> {
        let env = self.typer.env;
        if matches!(env.kind, SubjectKind::Component { .. }) {
            match env.emits.get(emit) {
                Some(sig) => Some(sig.clone()),
                None => {
                    let mut d = Diagnostic::error(
                        codes::UNDECLARED_EMIT.0,
                        codes::UNDECLARED_EMIT.1,
                        format!("`{emit}` is not declared in this component's `emits` block"),
                        span,
                    );
                    if let Some(s) = did_you_mean(emit, env.emits.keys()) {
                        d = d.with_note(format!("did you mean `{s}`?"));
                    }
                    self.typer.diags.push(d);
                    None
                }
            }
        } else {
            match env.events.get(emit) {
                Some(sig) => Some(sig.clone()),
                None => {
                    let mut d = Diagnostic::error(
                        codes::UNRESOLVED_NAME.0,
                        codes::UNRESOLVED_NAME.1,
                        format!("no handler for `{emit}` in this file's store"),
                        span,
                    );
                    if let Some(s) = did_you_mean(emit, env.events.keys()) {
                        d = d.with_note(format!("did you mean `{s}`?"));
                    }
                    self.typer.diags.push(d);
                    None
                }
            }
        }
    }

    // ── component calls ────────────────────────────────────────────────

    fn walk_component_call(&mut self, el: &ast::Element, name: &Ident, in_interactive: bool) {
        if !self.typer.env.component_imports.contains_key(name) {
            self.error(
                codes::UNKNOWN_ELEMENT,
                format!("`<{name}>` exists but is not imported — add `use component {name}`"),
                el.span,
            );
            return;
        }
        let target = &self.typer.resolved.components[name];
        let target_props: Vec<(Ident, Ty)> = target
            .props
            .iter()
            .map(|(n, t)| (n.clone(), t.clone()))
            .collect();
        let target_emits: BTreeMap<Ident, Vec<(Ident, Ty)>> = target.emits.clone();

        if in_interactive && self.interactive_memo.get(name).copied().unwrap_or(false) {
            self.error(
                codes::NESTED_INTERACTIVE,
                format!(
                    "`<{name}>` expands to interactive content — it cannot nest inside an \
                         interactive element (§10)"
                ),
                el.span,
            );
        }
        if !el.children.is_empty() {
            self.error(
                codes::BAD_CHILDREN,
                "components take no children in the spike (no slots — §14 deferred)".to_string(),
                el.span,
            );
        }

        // ── props ──────────────────────────────────────────────────────
        let mut bound: BTreeSet<Ident> = BTreeSet::new();
        for attr in &el.attrs {
            let Ok(attr_name) = Ident::new(&attr.name) else {
                continue;
            };
            if !bound.insert(attr_name.clone()) {
                self.error(
                    codes::DUPLICATE_ATTR,
                    format!("`{attr_name}` is bound twice"),
                    attr.span,
                );
                continue;
            }
            let Some((_, ty)) = target_props.iter().find(|(n, _)| *n == attr_name) else {
                let mut d = Diagnostic::error(
                    codes::UNKNOWN_PROP.0,
                    codes::UNKNOWN_PROP.1,
                    format!("`<{name}>` declares no prop `{attr_name}`"),
                    attr.span,
                );
                if attr_name.as_str() == "class" {
                    d = d.with_note(
                        "a component's root class is its own markup's business".to_string(),
                    );
                } else if let Some(s) =
                    did_you_mean(&attr_name, target_props.iter().map(|(n, _)| n))
                {
                    d = d.with_note(format!("did you mean `{s}`?"));
                }
                self.typer.diags.push(d);
                continue;
            };
            let ty = ty.clone();
            match &attr.value {
                ast::AttrValue::Bare => {
                    if ty != Ty::Bool {
                        self.error(
                            codes::TYPE_MISMATCH,
                            format!(
                                "bare `{attr_name}` means `true`; the prop is {}",
                                ty.describe()
                            ),
                            attr.span,
                        );
                    }
                }
                ast::AttrValue::Literal(s) => {
                    let lit = ast::Expr {
                        kind: ast::ExprKind::Str(s.clone()),
                        span: attr.span,
                    };
                    self.typer.check(&lit, &ty);
                }
                ast::AttrValue::Expr(expr) => self.typer.check(expr, &ty),
            }
        }
        for (prop_name, ty) in &target_props {
            if !bound.contains(prop_name) && !matches!(ty, Ty::Option(_)) {
                self.error(
                    codes::MISSING_REQUIRED_PROP,
                    format!("`<{name}>` requires `{prop_name}`"),
                    el.span,
                );
            }
        }

        // ── emit consumption (§4.4: one model, explicit) ───────────────
        let mut consumed: BTreeSet<Ident> = BTreeSet::new();
        for event_attr in &el.events {
            let Ok(emit) = Ident::new(&event_attr.event) else {
                continue;
            };
            let Some(emit_sig) = target_emits.get(&emit) else {
                let mut d = Diagnostic::error(
                    codes::UNDECLARED_EMIT.0,
                    codes::UNDECLARED_EMIT.1,
                    format!("`<{name}>` declares no emit `{emit}`"),
                    event_attr.span,
                );
                if let Some(s) = did_you_mean(&emit, target_emits.keys()) {
                    d = d.with_note(format!("did you mean `{s}`?"));
                }
                self.typer.diags.push(d);
                continue;
            };
            if !consumed.insert(emit.clone()) {
                self.error(
                    codes::DUPLICATE_ATTR,
                    format!("`on:{emit}` is bound twice"),
                    event_attr.span,
                );
            }
            match &event_attr.binding {
                ast::EventBinding::Forward => {
                    // Same name, same payload, enclosing machine scope.
                    let Some(own_sig) = self.target_signature(&emit, event_attr.span) else {
                        continue;
                    };
                    if own_sig != *emit_sig {
                        self.error(
                            codes::WRONG_ARGS,
                            format!(
                                "forwarding `{emit}` requires the identical signature in the \
                                 enclosing scope (§4.4)"
                            ),
                            event_attr.span,
                        );
                    }
                }
                ast::EventBinding::Emit {
                    name: rebind_name,
                    args,
                } => {
                    // Rebind: new event, args in caller scope, component
                    // payload discarded — so no carries here.
                    self.check_emit_binding(rebind_name, args, &BTreeMap::new(), event_attr.span);
                }
            }
        }
        for emit in target_emits.keys() {
            if !consumed.contains(emit) {
                self.typer.diags.push(Diagnostic::warning(
                    codes::UNHANDLED_EVENT.0,
                    codes::UNHANDLED_EVENT.1,
                    format!(
                        "`<{name}>` emits `{emit}` but this call site leaves it unbound — \
                         the control will be dead (§4.4)"
                    ),
                    el.span,
                ));
            }
        }
    }

    fn collect_class_attr(&mut self, attr: &ast::Attr) {
        match &attr.value {
            ast::AttrValue::Literal(s) => {
                for class in s.split_whitespace() {
                    self.class_refs.push((class.to_string(), attr.span));
                }
            }
            ast::AttrValue::Expr(expr) => {
                self.typer.check(expr, &Ty::Text);
                collect_string_literals(expr, &mut |s, span| {
                    for class in s.split_whitespace() {
                        self.class_refs.push((class.to_string(), span));
                    }
                });
            }
            ast::AttrValue::Bare => {
                self.error(
                    codes::TYPE_MISMATCH,
                    "`class` needs a value".to_string(),
                    attr.span,
                );
            }
        }
    }
}

/// Computes, for every component, whether its expansion contains an
/// interactive element (for the nested-interactives rule across component
/// boundaries). The import graph is a DAG, so plain recursion with a memo
/// terminates.
pub fn interactive_content_memo(
    resolved: &Resolved,
    sources: &[crate::resolve::ParsedSource],
    catalog: &Catalog,
) -> BTreeMap<Ident, bool> {
    fn nodes_interactive(
        nodes: &ast::MarkupList,
        env: &DefEnv,
        catalog: &Catalog,
        resolved: &Resolved,
        sources: &[crate::resolve::ParsedSource],
        memo: &mut BTreeMap<Ident, bool>,
    ) -> bool {
        nodes.iter().any(|node| match node {
            ast::Node::Element(el) => {
                let Ok(name) = Ident::new(&el.name) else {
                    return false;
                };
                match resolve_element(
                    &name,
                    env.component_imports.contains_key(&name),
                    resolved,
                    Some(catalog),
                ) {
                    ElementResolution::CatalogElement => {
                        catalog.elements[&name].class == ElementClass::Interactive
                            || nodes_interactive(
                                &el.children,
                                env,
                                catalog,
                                resolved,
                                sources,
                                memo,
                            )
                    }
                    ElementResolution::ImportedComponent => {
                        component_interactive(&name, catalog, resolved, sources, memo)
                    }
                    ElementResolution::Ambiguous => {
                        catalog.elements[&name].class == ElementClass::Interactive
                            || component_interactive(&name, catalog, resolved, sources, memo)
                    }
                    ElementResolution::UnimportedComponent | ElementResolution::Unknown => false,
                }
            }
            ast::Node::If { then, els, .. } => {
                nodes_interactive(then, env, catalog, resolved, sources, memo)
                    || els.as_ref().is_some_and(|e| {
                        nodes_interactive(e, env, catalog, resolved, sources, memo)
                    })
            }
            ast::Node::Each { body, .. } => {
                nodes_interactive(body, env, catalog, resolved, sources, memo)
            }
            ast::Node::Match { arms, .. } => arms
                .iter()
                .any(|arm| nodes_interactive(&arm.body, env, catalog, resolved, sources, memo)),
            _ => false,
        })
    }

    fn component_interactive(
        name: &Ident,
        catalog: &Catalog,
        resolved: &Resolved,
        sources: &[crate::resolve::ParsedSource],
        memo: &mut BTreeMap<Ident, bool>,
    ) -> bool {
        if let Some(&known) = memo.get(name) {
            return known;
        }
        memo.insert(name.clone(), false); // cycle backstop (DAG-checked anyway)
        let result =
            resolved
                .components
                .get(name)
                .is_some_and(|env| match &sources[env.source].parsed {
                    uhura_syntax::Parsed::Module(ast) => {
                        nodes_interactive(&ast.markup, env, catalog, resolved, sources, memo)
                    }
                    uhura_syntax::Parsed::Examples(_) => false,
                });
        memo.insert(name.clone(), result);
        result
    }

    let mut memo = BTreeMap::new();
    let names: Vec<Ident> = resolved.components.keys().cloned().collect();
    for name in names {
        component_interactive(&name, catalog, resolved, sources, &mut memo);
    }
    memo
}

fn collect_string_literals(expr: &ast::Expr, f: &mut impl FnMut(&str, Span)) {
    match &expr.kind {
        ast::ExprKind::Str(s) => f(s, expr.span),
        ast::ExprKind::If { cond, then, els } => {
            collect_string_literals(cond, f);
            collect_string_literals(then, f);
            collect_string_literals(els, f);
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            collect_string_literals(lhs, f);
            collect_string_literals(rhs, f);
        }
        ast::ExprKind::Unary { expr, .. } => collect_string_literals(expr, f),
        _ => {}
    }
}
