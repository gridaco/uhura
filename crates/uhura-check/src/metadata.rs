//! Checked authoring metadata (RFC 0003 §9).
//!
//! This projection is compiler-owned and deliberately separate from runtime
//! IR.  Its identifiers describe one checked source revision; they are not a
//! promise of durable identity across arbitrary source edits.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;
use serde_json::json;
use uhura_base::{Diagnostic, Ident, SourceMap, Span, codes, hash_json};
use uhura_core::template::{DefinitionAddress, DefinitionKind, TemplateAddress, TemplateSegment};
use uhura_syntax::{Parsed, ast};

use crate::catalog::Catalog;
use crate::markup::{ElementResolution, resolve_element};
use crate::resolve::{ParsedSource, Resolved, SubjectKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetadataClass {
    Doc,
    Annotation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceTargetClass {
    SourceModule,
    ComponentDeclaration,
    PageDeclaration,
    SurfaceDeclaration,
    PropDeclaration,
    EmittedEventDeclaration,
    EmittedEventParameter,
    RouteParameter,
    StoreScope,
    StateField,
    EventHandler,
    OutcomeHandler,
    HandlerParameter,
    ExampleDeclaration,
    CatalogElement,
    ComponentInvocation,
    IfBlock,
    EachBlock,
    MatchBlock,
}

impl SourceTargetClass {
    pub fn is_annotatable(self) -> bool {
        matches!(
            self,
            Self::CatalogElement
                | Self::ComponentInvocation
                | Self::IfBlock
                | Self::EachBlock
                | Self::MatchBlock
        )
    }

    pub fn is_documentable(self) -> bool {
        !self.is_annotatable()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceOwnerKind {
    Module,
    Examples,
    Component,
    Page,
    Surface,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceOwner {
    pub kind: SourceOwnerKind,
    pub name: String,
}

/// A structural address in syntax, used as the stable input to a target ID.
/// It intentionally contains no byte offsets or metadata prose.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SourceSyntaxAddress(pub Vec<SourceSyntaxSegment>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceSyntaxSegment {
    Module,
    Definition,
    Props,
    Emits,
    RouteParameters,
    Store,
    State,
    Handlers,
    Parameters,
    Examples,
    Markup,
    Item(u32),
    Children,
    IfThen,
    IfElse,
    EachBody,
    MatchArms,
    Arm(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceTargetId(String);

impl SourceTargetId {
    pub fn from_parts(file: &str, class: SourceTargetClass, address: &SourceSyntaxAddress) -> Self {
        Self(hash_json(&json!({
            "file": file,
            "class": class,
            "address": address,
        })))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceTargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceMetadataId(String);

impl SourceMetadataId {
    pub fn from_parts(target: &SourceTargetId, class: MetadataClass, order: u32) -> Self {
        Self(hash_json(&json!({
            "target": target.as_str(),
            "class": class,
            "order": order,
        })))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceMetadataId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceTarget {
    pub id: SourceTargetId,
    pub class: SourceTargetClass,
    pub file: String,
    pub span: Span,
    pub address: SourceSyntaxAddress,
    pub owner: SourceOwner,
    pub label: String,
}

impl SourceTarget {
    pub fn new(
        class: SourceTargetClass,
        file: String,
        span: Span,
        address: SourceSyntaxAddress,
        owner: SourceOwner,
        label: String,
    ) -> Self {
        let id = SourceTargetId::from_parts(&file, class, &address);
        Self {
            id,
            class,
            file,
            span,
            address,
            owner,
            label,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMetadataEntry {
    pub id: SourceMetadataId,
    pub class: MetadataClass,
    pub kind: String,
    pub text: String,
    pub metadata_span: Span,
    pub target_id: SourceTargetId,
    pub order: u32,
}

impl SourceMetadataEntry {
    pub fn new(
        class: MetadataClass,
        kind: String,
        text: String,
        metadata_span: Span,
        target_id: SourceTargetId,
        order: u32,
    ) -> Self {
        let id = SourceMetadataId::from_parts(&target_id, class, order);
        Self {
            id,
            class,
            kind,
            text,
            metadata_span,
            target_id,
            order,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthoringProjection {
    pub targets: Vec<SourceTarget>,
    pub entries: Vec<SourceMetadataEntry>,
}

/// Checker-internal indexes built alongside the public logical projection.
/// They connect syntax to lowering and previews without adding source data to
/// runtime IR or asking consumers to reverse-engineer target labels/spans.
#[derive(Clone, Debug, Default)]
pub struct AuthoringCollection {
    pub projection: AuthoringProjection,
    pub template_origins: BTreeMap<TemplateAddress, SourceTargetId>,
    pub definition_targets: BTreeMap<DefinitionAddress, SourceTargetId>,
    pub example_targets: BTreeMap<(String, String), SourceTargetId>,
    pub(crate) template_origin_errors: Vec<String>,
}

impl AuthoringProjection {
    /// Checks the closed metadata/target contract before it crosses a wire
    /// boundary. Parsing and checking still own user-facing diagnostics.
    pub fn validate(&self) -> Result<(), String> {
        let mut targets = BTreeMap::new();
        for target in &self.targets {
            let expected = SourceTargetId::from_parts(&target.file, target.class, &target.address);
            if target.id != expected {
                return Err(format!("target `{}` has a non-canonical id", target.id));
            }
            if targets.insert(target.id.clone(), target).is_some() {
                return Err(format!("duplicate source target `{}`", target.id));
            }
        }

        let mut ids = BTreeSet::new();
        let mut next_orders: BTreeMap<(&SourceTargetId, MetadataClass), u32> = BTreeMap::new();
        for entry in &self.entries {
            if !ids.insert(entry.id.clone()) {
                return Err(format!("duplicate metadata entry `{}`", entry.id));
            }
            let Some(target) = targets.get(&entry.target_id) else {
                return Err(format!(
                    "metadata entry `{}` references an unknown target `{}`",
                    entry.id, entry.target_id
                ));
            };
            let expected = SourceMetadataId::from_parts(&entry.target_id, entry.class, entry.order);
            if entry.id != expected {
                return Err(format!(
                    "metadata entry `{}` has a non-canonical id",
                    entry.id
                ));
            }
            match entry.class {
                MetadataClass::Doc => {
                    if entry.kind != "doc" || entry.order != 0 || !target.class.is_documentable() {
                        return Err(format!(
                            "metadata entry `{}` is not a valid declaration doc",
                            entry.id
                        ));
                    }
                }
                MetadataClass::Annotation => {
                    if !valid_annotation_kind(&entry.kind) || !target.class.is_annotatable() {
                        return Err(format!(
                            "metadata entry `{}` is not a valid markup annotation",
                            entry.id
                        ));
                    }
                }
            }
            let next = next_orders
                .entry((&entry.target_id, entry.class))
                .or_default();
            if entry.order != *next {
                return Err(format!(
                    "metadata entries for target `{}` are not contiguous from zero",
                    entry.target_id
                ));
            }
            *next += 1;
        }
        Ok(())
    }
}

fn valid_annotation_kind(kind: &str) -> bool {
    if kind.is_empty() || kind.len() > 64 || !kind.is_ascii() {
        return false;
    }
    let bytes = kind.as_bytes();
    if !bytes[0].is_ascii_lowercase() || bytes.last() == Some(&b'-') {
        return false;
    }
    let mut previous_dash = false;
    for byte in bytes {
        if *byte == b'-' {
            if previous_dash {
                return false;
            }
            previous_dash = true;
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_dash = false;
        } else {
            return false;
        }
    }
    true
}

impl AuthoringCollection {
    pub fn doc_for_target(&self, target: &SourceTargetId) -> Option<SourceMetadataId> {
        self.projection
            .entries
            .iter()
            .find(|entry| entry.class == MetadataClass::Doc && entry.target_id == *target)
            .map(|entry| entry.id.clone())
    }

    pub(crate) fn template_origin_error(&self) -> Option<String> {
        (!self.template_origin_errors.is_empty()).then(|| self.template_origin_errors.join("; "))
    }
}

/// Builds RFC 0003's authoring projection even when unrelated checking has
/// failed. Invalid/recovery constructs are omitted; markup targets are added
/// only when catalog/component resolution can classify them independently.
pub fn collect_authoring(
    sources: &[ParsedSource],
    resolved: &Resolved,
    catalog: Option<&Catalog>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) -> AuthoringCollection {
    let env_by_source: BTreeMap<usize, _> = resolved
        .pages
        .values()
        .chain(resolved.components.values())
        .chain(resolved.surfaces.values())
        .map(|env| (env.source, env))
        .collect();

    let mut out = AuthoringCollection::default();
    for (source_index, source) in sources.iter().enumerate() {
        match &source.parsed {
            Parsed::Module(file) => {
                collect_module(
                    source,
                    file,
                    env_by_source.get(&source_index).copied(),
                    resolved,
                    catalog,
                    source_map,
                    diagnostics,
                    &mut out,
                );
            }
            Parsed::Examples(file) => collect_examples(source, file, source_map, &mut out),
        }
    }

    out.projection.targets.sort_by(|a, b| {
        (&a.file, a.span.start, a.class, &a.id).cmp(&(&b.file, b.span.start, b.class, &b.id))
    });
    out.projection.entries.sort_by(|a, b| {
        let a_file = out
            .projection
            .targets
            .iter()
            .find(|target| target.id == a.target_id)
            .map(|target| target.file.as_str())
            .unwrap_or_default();
        let b_file = out
            .projection
            .targets
            .iter()
            .find(|target| target.id == b.target_id)
            .map(|target| target.file.as_str())
            .unwrap_or_default();
        (a_file, a.metadata_span.start, a.order, &a.id).cmp(&(
            b_file,
            b.metadata_span.start,
            b.order,
            &b.id,
        ))
    });
    debug_assert!(out.projection.validate().is_ok());
    out
}

#[allow(clippy::too_many_arguments)]
fn collect_module(
    source: &ParsedSource,
    file: &ast::File,
    env: Option<&crate::resolve::DefEnv>,
    resolved: &Resolved,
    catalog: Option<&Catalog>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
    out: &mut AuthoringCollection,
) {
    let source_span = Span::new(
        source.file,
        0,
        u32::try_from(source_map.text(source.file).len()).expect("source text fits in u32"),
    );
    let module_owner = SourceOwner {
        kind: SourceOwnerKind::Module,
        name: source.rel_path.clone(),
    };
    let module_target = add_target(
        out,
        SourceTargetClass::SourceModule,
        source,
        source_span,
        SourceSyntaxAddress(vec![SourceSyntaxSegment::Module]),
        module_owner,
        source.rel_path.clone(),
        first_doc(&file.preamble, ast::DocForm::Inner),
    );
    let _ = module_target;

    let Some(identity) = module_identity(source, file, env) else {
        return;
    };
    let owner = identity.owner;

    let declaration_target = add_target(
        out,
        identity.declaration_class,
        source,
        identity.declaration_span,
        SourceSyntaxAddress(vec![SourceSyntaxSegment::Definition]),
        owner.clone(),
        owner.name.clone(),
        first_doc(&file.preamble, ast::DocForm::Outer),
    );
    if let Some(definition) = &identity.definition {
        out.definition_targets
            .insert(definition.clone(), declaration_target);
    }

    for (index, prop) in file.props.iter().enumerate() {
        add_target(
            out,
            SourceTargetClass::PropDeclaration,
            source,
            prop.span,
            address([
                SourceSyntaxSegment::Definition,
                SourceSyntaxSegment::Props,
                item(index),
            ]),
            owner.clone(),
            prop.name.clone(),
            first_doc(&prop.leading, ast::DocForm::Outer),
        );
    }
    for (emit_index, emit) in file.emits.iter().enumerate() {
        add_target(
            out,
            SourceTargetClass::EmittedEventDeclaration,
            source,
            emit.span,
            address([
                SourceSyntaxSegment::Definition,
                SourceSyntaxSegment::Emits,
                item(emit_index),
            ]),
            owner.clone(),
            emit.name.clone(),
            first_doc(&emit.leading, ast::DocForm::Outer),
        );
        for (param_index, param) in emit.params.iter().enumerate() {
            add_target(
                out,
                SourceTargetClass::EmittedEventParameter,
                source,
                param.span,
                address([
                    SourceSyntaxSegment::Definition,
                    SourceSyntaxSegment::Emits,
                    item(emit_index),
                    SourceSyntaxSegment::Parameters,
                    item(param_index),
                ]),
                owner.clone(),
                param.name.clone(),
                first_doc(&param.leading, ast::DocForm::Outer),
            );
        }
    }
    for (index, param) in file.params.iter().enumerate() {
        add_target(
            out,
            SourceTargetClass::RouteParameter,
            source,
            param.span,
            address([
                SourceSyntaxSegment::Definition,
                SourceSyntaxSegment::RouteParameters,
                item(index),
            ]),
            owner.clone(),
            param.name.clone(),
            first_doc(&param.leading, ast::DocForm::Outer),
        );
    }
    if let Some(store) = &file.store {
        add_target(
            out,
            SourceTargetClass::StoreScope,
            source,
            store.span,
            address([SourceSyntaxSegment::Definition, SourceSyntaxSegment::Store]),
            owner.clone(),
            "store".into(),
            first_doc(&store.leading, ast::DocForm::Outer),
        );
        for (index, field) in store.state.iter().enumerate() {
            add_target(
                out,
                SourceTargetClass::StateField,
                source,
                field.span,
                address([
                    SourceSyntaxSegment::Definition,
                    SourceSyntaxSegment::Store,
                    SourceSyntaxSegment::State,
                    item(index),
                ]),
                owner.clone(),
                field.name.clone(),
                first_doc(&field.leading, ast::DocForm::Outer),
            );
        }
        for (handler_index, handler) in store.handlers.iter().enumerate() {
            let (class, label) = match &handler.event {
                ast::EventRef::Semantic { name, .. } => {
                    (SourceTargetClass::EventHandler, name.clone())
                }
                ast::EventRef::Outcome { command, which, .. } => (
                    SourceTargetClass::OutcomeHandler,
                    format!(
                        "{}.{}",
                        command,
                        match which {
                            ast::OutcomeKind::Ok => "ok",
                            ast::OutcomeKind::Err => "err",
                        }
                    ),
                ),
            };
            add_target(
                out,
                class,
                source,
                handler.span,
                address([
                    SourceSyntaxSegment::Definition,
                    SourceSyntaxSegment::Store,
                    SourceSyntaxSegment::Handlers,
                    item(handler_index),
                ]),
                owner.clone(),
                label,
                first_doc(&handler.leading, ast::DocForm::Outer),
            );
            for (param_index, param) in handler.params.iter().enumerate() {
                add_target(
                    out,
                    SourceTargetClass::HandlerParameter,
                    source,
                    param.span,
                    address([
                        SourceSyntaxSegment::Definition,
                        SourceSyntaxSegment::Store,
                        SourceSyntaxSegment::Handlers,
                        item(handler_index),
                        SourceSyntaxSegment::Parameters,
                        item(param_index),
                    ]),
                    owner.clone(),
                    param.name.clone(),
                    first_doc(&param.leading, ast::DocForm::Outer),
                );
            }
        }
    }

    let component_imports = file
        .uses
        .iter()
        .filter_map(|use_decl| match use_decl {
            ast::Use::Component { name, .. } => Ident::new(name).ok(),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let source_prefix = vec![SourceSyntaxSegment::Definition, SourceSyntaxSegment::Markup];
    collect_markup_list(
        source,
        &file.markup,
        &source_prefix,
        identity.definition.as_ref(),
        &owner,
        &component_imports,
        resolved,
        catalog,
        diagnostics,
        out,
    );
}

fn collect_examples(
    source: &ParsedSource,
    file: &ast::ExamplesFile,
    source_map: &SourceMap,
    out: &mut AuthoringCollection,
) {
    let owner = SourceOwner {
        kind: SourceOwnerKind::Examples,
        name: source.rel_path.clone(),
    };
    add_target(
        out,
        SourceTargetClass::SourceModule,
        source,
        Span::new(
            source.file,
            0,
            u32::try_from(source_map.text(source.file).len()).expect("source text fits in u32"),
        ),
        address([SourceSyntaxSegment::Module]),
        owner.clone(),
        source.rel_path.clone(),
        first_doc(&file.preamble, ast::DocForm::Inner),
    );
    for (index, example) in file.examples.iter().enumerate() {
        let id = add_target(
            out,
            SourceTargetClass::ExampleDeclaration,
            source,
            example.span,
            address([SourceSyntaxSegment::Examples, item(index)]),
            owner.clone(),
            example.name.clone(),
            first_doc(&example.leading, ast::DocForm::Outer),
        );
        out.example_targets
            .insert((source.rel_path.clone(), example.name.clone()), id);
    }
}

struct ModuleIdentity {
    owner: SourceOwner,
    definition: Option<DefinitionAddress>,
    declaration_class: SourceTargetClass,
    declaration_span: Span,
}

fn module_identity(
    source: &ParsedSource,
    file: &ast::File,
    env: Option<&crate::resolve::DefEnv>,
) -> Option<ModuleIdentity> {
    let (kind, fallback_name, declaration_class, declaration_span) = match &file.kind {
        ast::DefKind::Component { name, span } => (
            SourceOwnerKind::Component,
            name.clone(),
            SourceTargetClass::ComponentDeclaration,
            *span,
        ),
        ast::DefKind::Surface { name, span, .. } => (
            SourceOwnerKind::Surface,
            name.clone(),
            SourceTargetClass::SurfaceDeclaration,
            *span,
        ),
        ast::DefKind::Page { span } => (
            SourceOwnerKind::Page,
            source.rel_path.clone(),
            SourceTargetClass::PageDeclaration,
            *span,
        ),
        ast::DefKind::Error { .. } => return None,
    };
    let definition = env.map(|env| definition_for_subject(&env.kind));
    let owner_name = env
        .map(|env| env.kind.name().to_string())
        .unwrap_or(fallback_name);
    let owner = SourceOwner {
        kind,
        name: owner_name,
    };
    Some(ModuleIdentity {
        owner,
        definition,
        declaration_class,
        declaration_span,
    })
}

fn definition_for_subject(subject: &SubjectKind) -> DefinitionAddress {
    let (kind, name) = match subject {
        SubjectKind::Page { route } => (DefinitionKind::Page, route.clone()),
        SubjectKind::Component { name } => (DefinitionKind::Component, name.clone()),
        SubjectKind::Surface { name, .. } => (DefinitionKind::Surface, name.clone()),
    };
    DefinitionAddress::new(kind, name)
}

#[allow(clippy::too_many_arguments)]
fn collect_markup_list(
    source: &ParsedSource,
    list: &ast::MarkupList,
    source_prefix: &[SourceSyntaxSegment],
    definition: Option<&DefinitionAddress>,
    owner: &SourceOwner,
    component_imports: &BTreeSet<Ident>,
    resolved: &Resolved,
    catalog: Option<&Catalog>,
    diagnostics: &mut Vec<Diagnostic>,
    out: &mut AuthoringCollection,
) {
    let semantic_nodes = list
        .nodes
        .iter()
        .filter(|node| !matches!(node, ast::Node::Text { .. } | ast::Node::Error { .. }))
        .count();
    let root_template = (semantic_nodes == 1)
        .then(|| definition.map(|definition| TemplateAddress::root(definition.clone())))
        .flatten();
    for (source_index, node) in list.nodes.iter().enumerate() {
        if matches!(node, ast::Node::Text { .. } | ast::Node::Error { .. }) {
            continue;
        }
        let mut source_address = source_prefix.to_vec();
        source_address.push(item(source_index));
        collect_markup_node(
            source,
            node,
            source_address,
            root_template.clone(),
            owner,
            component_imports,
            resolved,
            catalog,
            diagnostics,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_markup_node(
    source: &ParsedSource,
    node: &ast::Node,
    source_address: Vec<SourceSyntaxSegment>,
    template: Option<TemplateAddress>,
    owner: &SourceOwner,
    component_imports: &BTreeSet<Ident>,
    resolved: &Resolved,
    catalog: Option<&Catalog>,
    diagnostics: &mut Vec<Diagnostic>,
    out: &mut AuthoringCollection,
) {
    match node {
        ast::Node::Element(element) => {
            let name = Ident::new(&element.name).ok();
            let resolution = name.as_ref().map_or(ElementResolution::Unknown, |name| {
                resolve_element(name, component_imports.contains(name), resolved, catalog)
            });
            let class = match resolution {
                ElementResolution::CatalogElement => Some(SourceTargetClass::CatalogElement),
                ElementResolution::ImportedComponent => {
                    Some(SourceTargetClass::ComponentInvocation)
                }
                ElementResolution::UnimportedComponent
                | ElementResolution::Ambiguous
                | ElementResolution::Unknown => None,
            };
            if let Some(class) = class {
                let id = add_target_with_annotations(
                    out,
                    class,
                    source,
                    element.span,
                    SourceSyntaxAddress(source_address.clone()),
                    owner.clone(),
                    element.name.clone(),
                    &element.annotations,
                );
                record_template_origin(out, template.as_ref(), id);
            } else if catalog.is_some()
                || name.is_none()
                || resolution == ElementResolution::Ambiguous
            {
                let reason = if resolution == ElementResolution::Ambiguous {
                    "ambiguous"
                } else {
                    "unresolved"
                };
                for annotation in &element.annotations {
                    diagnostics.push(
                        Diagnostic::error(
                            codes::INCOMPATIBLE_METADATA_TARGET.0,
                            codes::INCOMPATIBLE_METADATA_TARGET.1,
                            format!(
                                "markup annotation cannot target {reason} element `<{}>`",
                                element.name
                            ),
                            annotation.span,
                        )
                        .with_label(element.span, "incompatible target"),
                    );
                }
            }
            if resolution == ElementResolution::CatalogElement {
                let mut prefix = source_address;
                prefix.push(SourceSyntaxSegment::Children);
                collect_nested_list(
                    source,
                    &element.children,
                    &prefix,
                    template.as_ref(),
                    NestedList::ElementChildren,
                    owner,
                    component_imports,
                    resolved,
                    catalog,
                    diagnostics,
                    out,
                );
            } else if catalog.is_none()
                && name.is_some()
                && resolution != ElementResolution::ImportedComponent
            {
                // The unavailable catalog may ultimately classify this as a
                // catalog element. Keep walking for independently classifiable
                // descendant blocks/components, but do not invent provenance.
                let mut prefix = source_address;
                prefix.push(SourceSyntaxSegment::Children);
                collect_nested_list(
                    source,
                    &element.children,
                    &prefix,
                    None,
                    NestedList::ElementChildren,
                    owner,
                    component_imports,
                    resolved,
                    catalog,
                    diagnostics,
                    out,
                );
            }
        }
        ast::Node::If {
            annotations,
            then,
            els,
            span,
            ..
        } => {
            let id = add_target_with_annotations(
                out,
                SourceTargetClass::IfBlock,
                source,
                *span,
                SourceSyntaxAddress(source_address.clone()),
                owner.clone(),
                "if".into(),
                annotations,
            );
            record_template_origin(out, template.as_ref(), id);
            let mut then_prefix = source_address.clone();
            then_prefix.push(SourceSyntaxSegment::IfThen);
            collect_nested_list(
                source,
                then,
                &then_prefix,
                template.as_ref(),
                NestedList::IfThen,
                owner,
                component_imports,
                resolved,
                catalog,
                diagnostics,
                out,
            );
            if let Some(els) = els {
                let mut else_prefix = source_address;
                else_prefix.push(SourceSyntaxSegment::IfElse);
                collect_nested_list(
                    source,
                    els,
                    &else_prefix,
                    template.as_ref(),
                    NestedList::IfElse,
                    owner,
                    component_imports,
                    resolved,
                    catalog,
                    diagnostics,
                    out,
                );
            }
        }
        ast::Node::Each {
            annotations,
            body,
            span,
            ..
        } => {
            let id = add_target_with_annotations(
                out,
                SourceTargetClass::EachBlock,
                source,
                *span,
                SourceSyntaxAddress(source_address.clone()),
                owner.clone(),
                "each".into(),
                annotations,
            );
            record_template_origin(out, template.as_ref(), id);
            let mut prefix = source_address;
            prefix.push(SourceSyntaxSegment::EachBody);
            collect_nested_list(
                source,
                body,
                &prefix,
                template.as_ref(),
                NestedList::EachBody,
                owner,
                component_imports,
                resolved,
                catalog,
                diagnostics,
                out,
            );
        }
        ast::Node::Match {
            annotations,
            arms,
            span,
            ..
        } => {
            let id = add_target_with_annotations(
                out,
                SourceTargetClass::MatchBlock,
                source,
                *span,
                SourceSyntaxAddress(source_address.clone()),
                owner.clone(),
                "match".into(),
                annotations,
            );
            record_template_origin(out, template.as_ref(), id);
            for (arm_index, arm) in arms.iter().enumerate() {
                let mut prefix = source_address.clone();
                prefix.extend([
                    SourceSyntaxSegment::MatchArms,
                    SourceSyntaxSegment::Arm(index_u32(arm_index)),
                ]);
                collect_nested_list(
                    source,
                    &arm.body,
                    &prefix,
                    template.as_ref(),
                    NestedList::MatchArm(arm_index),
                    owner,
                    component_imports,
                    resolved,
                    catalog,
                    diagnostics,
                    out,
                );
            }
        }
        ast::Node::Text { .. } | ast::Node::Error { .. } => {}
    }
}

#[derive(Clone, Copy)]
enum NestedList {
    ElementChildren,
    IfThen,
    IfElse,
    EachBody,
    MatchArm(usize),
}

#[allow(clippy::too_many_arguments)]
fn collect_nested_list(
    source: &ParsedSource,
    list: &ast::MarkupList,
    source_prefix: &[SourceSyntaxSegment],
    parent_template: Option<&TemplateAddress>,
    kind: NestedList,
    owner: &SourceOwner,
    component_imports: &BTreeSet<Ident>,
    resolved: &Resolved,
    catalog: Option<&Catalog>,
    diagnostics: &mut Vec<Diagnostic>,
    out: &mut AuthoringCollection,
) {
    let mut semantic_index = 0usize;
    for (source_index, node) in list.nodes.iter().enumerate() {
        if matches!(node, ast::Node::Text { .. } | ast::Node::Error { .. }) {
            continue;
        }
        let template = parent_template.map(|parent| {
            parent.child(match kind {
                NestedList::ElementChildren => TemplateSegment::ElementChild {
                    index: semantic_index,
                },
                NestedList::IfThen => TemplateSegment::IfThen {
                    index: semantic_index,
                },
                NestedList::IfElse => TemplateSegment::IfElse {
                    index: semantic_index,
                },
                NestedList::EachBody => TemplateSegment::EachBody {
                    index: semantic_index,
                },
                NestedList::MatchArm(arm) => TemplateSegment::MatchArm {
                    arm,
                    child: semantic_index,
                },
            })
        });
        let mut address = source_prefix.to_vec();
        address.push(item(source_index));
        collect_markup_node(
            source,
            node,
            address,
            template,
            owner,
            component_imports,
            resolved,
            catalog,
            diagnostics,
            out,
        );
        semantic_index += 1;
    }
}

fn record_template_origin(
    out: &mut AuthoringCollection,
    template: Option<&TemplateAddress>,
    target: SourceTargetId,
) {
    if let Some(template) = template {
        match out.template_origins.entry(template.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(target);
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                out.template_origin_errors.push(format!(
                    "duplicate source origins for template address {template:?}: targets `{}` and `{target}`",
                    entry.get()
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_target(
    out: &mut AuthoringCollection,
    class: SourceTargetClass,
    source: &ParsedSource,
    span: Span,
    address: SourceSyntaxAddress,
    owner: SourceOwner,
    label: String,
    doc: Option<&ast::DocComment>,
) -> SourceTargetId {
    let target = SourceTarget::new(class, source.rel_path.clone(), span, address, owner, label);
    let id = target.id.clone();
    out.projection.targets.push(target);
    if let Some(doc) = doc {
        out.projection.entries.push(SourceMetadataEntry::new(
            MetadataClass::Doc,
            "doc".into(),
            doc.text.clone(),
            doc.span,
            id.clone(),
            0,
        ));
    }
    id
}

#[allow(clippy::too_many_arguments)]
fn add_target_with_annotations(
    out: &mut AuthoringCollection,
    class: SourceTargetClass,
    source: &ParsedSource,
    span: Span,
    address: SourceSyntaxAddress,
    owner: SourceOwner,
    label: String,
    annotations: &[ast::MarkupAnnotation],
) -> SourceTargetId {
    let target = SourceTarget::new(class, source.rel_path.clone(), span, address, owner, label);
    let id = target.id.clone();
    out.projection.targets.push(target);
    for (order, annotation) in annotations.iter().enumerate() {
        out.projection.entries.push(SourceMetadataEntry::new(
            MetadataClass::Annotation,
            annotation.kind.clone(),
            annotation.text.clone(),
            annotation.span,
            id.clone(),
            index_u32(order),
        ));
    }
    id
}

fn first_doc(trivia: &ast::DslTrivia, form: ast::DocForm) -> Option<&ast::DocComment> {
    trivia.docs.iter().find(|doc| doc.form == form)
}

fn address<const N: usize>(segments: [SourceSyntaxSegment; N]) -> SourceSyntaxAddress {
    SourceSyntaxAddress(Vec::from(segments))
}

fn item(index: usize) -> SourceSyntaxSegment {
    SourceSyntaxSegment::Item(index_u32(index))
}

fn index_u32(index: usize) -> u32 {
    u32::try_from(index).expect("source syntax lists fit in u32")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uhura_base::FileId;

    fn target(span: Span) -> SourceTarget {
        SourceTarget::new(
            SourceTargetClass::CatalogElement,
            "components/card.uhura".into(),
            span,
            SourceSyntaxAddress(vec![
                SourceSyntaxSegment::Definition,
                SourceSyntaxSegment::Markup,
                SourceSyntaxSegment::Item(0),
            ]),
            SourceOwner {
                kind: SourceOwnerKind::Component,
                name: "card".into(),
            },
            "button".into(),
        )
    }

    #[test]
    fn target_id_ignores_source_span() {
        let a = target(Span::new(FileId(0), 10, 20));
        let b = target(Span::new(FileId(0), 110, 120));
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn metadata_id_ignores_prose_and_metadata_span() {
        let target = target(Span::new(FileId(0), 10, 20));
        let a = SourceMetadataEntry::new(
            MetadataClass::Annotation,
            "review-note".into(),
            "first".into(),
            Span::new(FileId(0), 0, 5),
            target.id.clone(),
            0,
        );
        let b = SourceMetadataEntry::new(
            MetadataClass::Annotation,
            "rationale".into(),
            "changed".into(),
            Span::new(FileId(0), 50, 90),
            target.id,
            0,
        );
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn doc_cannot_target_markup_occurrence() {
        let target = target(Span::new(FileId(0), 10, 20));
        let entry = SourceMetadataEntry::new(
            MetadataClass::Doc,
            "doc".into(),
            "not declaration documentation".into(),
            Span::new(FileId(0), 0, 5),
            target.id.clone(),
            0,
        );
        let projection = AuthoringProjection {
            targets: vec![target],
            entries: vec![entry],
        };
        assert!(projection.validate().is_err());
    }

    #[test]
    fn annotation_kind_must_match_the_source_grammar() {
        let target = target(Span::new(FileId(0), 10, 20));
        let invalid = ["Review", "review_note", "review--note", "review-", "é"]
            .into_iter()
            .map(str::to_string)
            .chain(std::iter::once("a".repeat(65)));
        for kind in invalid {
            let entry = SourceMetadataEntry::new(
                MetadataClass::Annotation,
                kind.clone(),
                "prose".into(),
                Span::new(FileId(0), 0, 5),
                target.id.clone(),
                0,
            );
            let projection = AuthoringProjection {
                targets: vec![target.clone()],
                entries: vec![entry],
            };
            assert!(projection.validate().is_err(), "accepted `{kind}`");
        }
    }

    #[test]
    fn duplicate_template_origin_is_recorded_without_overwriting_the_first() {
        let first = target(Span::new(FileId(0), 10, 20));
        let second = SourceTarget::new(
            SourceTargetClass::CatalogElement,
            "components/card.uhura".into(),
            Span::new(FileId(0), 30, 40),
            SourceSyntaxAddress(vec![
                SourceSyntaxSegment::Definition,
                SourceSyntaxSegment::Markup,
                SourceSyntaxSegment::Item(1),
            ]),
            SourceOwner {
                kind: SourceOwnerKind::Component,
                name: "card".into(),
            },
            "button".into(),
        );
        let template = TemplateAddress::root(DefinitionAddress::new(
            DefinitionKind::Component,
            Ident::new("card").unwrap(),
        ));
        let mut collection = AuthoringCollection::default();

        record_template_origin(&mut collection, Some(&template), first.id.clone());
        record_template_origin(&mut collection, Some(&template), second.id.clone());

        assert_eq!(collection.template_origins.get(&template), Some(&first.id));
        let error = collection
            .template_origin_error()
            .expect("duplicate is a release-visible invariant error");
        assert!(error.contains(first.id.as_str()));
        assert!(error.contains(second.id.as_str()));
    }
}
