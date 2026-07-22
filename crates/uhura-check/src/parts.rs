//! Static part composition.
//!
//! Parts are a source-ownership boundary, not a second execution model. This
//! pass validates a closed direct composition and rewrites it to the ordinary
//! aggregate machine AST consumed by the existing checker bridge.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::Diagnostic;
use uhura_core::ir::SourceRef;
use uhura_core::{Program, SiteIdentityFrame, Statement, semantic_node_id};

use crate::checker_ir as ast;
use crate::diagnostic::{codes, error};

#[derive(Clone, Debug)]
struct PartTemplate {
    package: String,
    module: String,
    visibility: uhura_syntax::ast::Visibility,
    public_id: Option<String>,
    declaration: uhura_syntax::ast::PartDeclaration,
    bindings: BTreeMap<String, String>,
    external_bindings: BTreeMap<String, String>,
    standard_imports: BTreeMap<(String, String), uhura_syntax::ast::Span>,
    helper: Option<PartHelperTemplate>,
}

#[derive(Clone, Debug)]
struct PartHelperTemplate {
    module: uhura_syntax::ast::Module,
    bindings: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PartCatalog {
    local: BTreeMap<(String, String), PartTemplate>,
    external: BTreeMap<String, PartTemplate>,
    kinds: BTreeMap<(String, String), &'static str>,
    external_kinds: BTreeMap<String, &'static str>,
    external_identities: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub(super) struct CompositionOutput {
    pub modules: Vec<uhura_syntax::ast::Module>,
    pub machine_part_dependencies: BTreeMap<String, BTreeSet<String>>,
    pub linked_public_declarations: BTreeSet<String>,
    pub standard_imports: BTreeMap<(String, String), uhura_syntax::ast::Span>,
    pub helper_bindings: BTreeMap<String, BTreeMap<String, String>>,
    machine_sites: BTreeMap<String, MachineSites>,
}

#[derive(Clone, Debug, Default)]
struct MachineSites {
    invariants: Vec<SiteOrigin>,
    unreachable: BTreeMap<SiteContainer, Vec<SiteOrigin>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SiteContainer {
    Handler(String),
    Update(String),
    BeforeCommit,
}

#[derive(Clone, Debug)]
pub(super) struct SiteOrigin {
    pub node: String,
    pub owner: String,
    kind: &'static str,
    path: String,
    pub source_package: String,
    pub span: uhura_syntax::ast::Span,
    lowered_span: uhura_syntax::ast::Span,
}

impl CompositionOutput {
    pub(super) fn apply_site_ids(&self, program: &mut Program) -> Result<(), String> {
        let core = &mut program.machine_program;
        let (machines, site_identities) = (&mut core.machines, &mut core.site_identities);
        for (machine_id, sites) in &self.machine_sites {
            let Some(machine) = machines.get_mut(machine_id) else {
                continue;
            };
            if machine.invariants.len() != sites.invariants.len() {
                return Err(format!(
                    "machine `{machine_id}` lowered {} invariant obligations from {} authored obligations",
                    machine.invariants.len(),
                    sites.invariants.len()
                ));
            }
            for ((_, source), origin) in machine.invariants.iter_mut().zip(&sites.invariants) {
                assign_origin(source, origin, machine_id, site_identities);
            }

            for (input, handler) in &mut machine.handlers {
                if let Some(origins) = sites
                    .unreachable
                    .get(&SiteContainer::Handler(input.clone()))
                {
                    assign_container_sites(&mut handler.body, origins, machine_id, site_identities);
                }
            }
            for (name, transition) in &mut machine.transitions {
                if let Some(origins) = sites.unreachable.get(&SiteContainer::Update(name.clone())) {
                    assign_container_sites(
                        &mut transition.body,
                        origins,
                        machine_id,
                        site_identities,
                    );
                }
            }
            if let Some(origins) = sites.unreachable.get(&SiteContainer::BeforeCommit) {
                assign_container_sites(
                    &mut machine.before_commit,
                    origins,
                    machine_id,
                    site_identities,
                );
            }

            let all_origins = sites.unreachable.values().flatten().collect::<Vec<_>>();
            for (input, handler) in &mut machine.handlers {
                assign_fallback_sites(
                    &mut handler.body,
                    input,
                    &all_origins,
                    machine_id,
                    site_identities,
                );
            }
            for (name, transition) in &mut machine.transitions {
                assign_fallback_sites(
                    &mut transition.body,
                    name,
                    &all_origins,
                    machine_id,
                    site_identities,
                );
            }
            assign_fallback_sites(
                &mut machine.before_commit,
                "root",
                &all_origins,
                machine_id,
                site_identities,
            );
            let mut missing = false;
            for handler in machine.handlers.values_mut() {
                visit_unreachable_sources(&mut handler.body, &mut |source| {
                    missing |= !is_site_id(&source.id);
                });
            }
            for transition in machine.transitions.values_mut() {
                visit_unreachable_sources(&mut transition.body, &mut |source| {
                    missing |= !is_site_id(&source.id);
                });
            }
            visit_unreachable_sources(&mut machine.before_commit, &mut |source| {
                missing |= !is_site_id(&source.id);
            });
            if missing {
                return Err(format!(
                    "machine `{machine_id}` contains a lowered fault site without one canonical authored origin"
                ));
            }
        }
        Ok(())
    }

    pub(super) fn site_occurrences(&self) -> impl Iterator<Item = &SiteOrigin> {
        self.machine_sites.values().flat_map(|sites| {
            sites
                .invariants
                .iter()
                .chain(sites.unreachable.values().flatten())
        })
    }
}

fn assign_origin(
    source: &mut SourceRef,
    origin: &SiteOrigin,
    machine_id: &str,
    identities: &mut BTreeMap<String, SiteIdentityFrame>,
) {
    source.id.clone_from(&origin.node);
    source.start = origin.span.start;
    source.end = origin.span.end;
    identities.entry(origin.node.clone()).or_insert_with(|| {
        SiteIdentityFrame::new(machine_id, &origin.owner, origin.kind, &origin.path)
    });
}

fn assign_container_sites(
    statements: &mut [Statement],
    origins: &[SiteOrigin],
    machine_id: &str,
    identities: &mut BTreeMap<String, SiteIdentityFrame>,
) {
    let mut used = vec![false; origins.len()];
    visit_unreachable_sources(statements, &mut |source| {
        let Some((index, origin)) = origins.iter().enumerate().find(|(index, origin)| {
            !used[*index]
                && source.start == origin.lowered_span.start
                && source.end == origin.lowered_span.end
        }) else {
            return;
        };
        assign_origin(source, origin, machine_id, identities);
        used[index] = true;
    });
}

fn assign_fallback_sites(
    statements: &mut [Statement],
    container: &str,
    origins: &[&SiteOrigin],
    machine_id: &str,
    identities: &mut BTreeMap<String, SiteIdentityFrame>,
) {
    visit_unreachable_sources(statements, &mut |source| {
        if is_site_id(&source.id) {
            return;
        }
        let mut matches = origins.iter().copied().filter(|origin| {
            source.start == origin.lowered_span.start && source.end == origin.lowered_span.end
        });
        let first = matches.next();
        let selected = first.and_then(|first| {
            matches
                .find(|origin| {
                    origin.owner != "root"
                        && (container == origin.owner
                            || container.starts_with(&format!("{}.", origin.owner)))
                })
                .or(Some(first))
        });
        if let Some(origin) = selected {
            assign_origin(source, origin, machine_id, identities);
        }
    });
}

fn visit_unreachable_sources(statements: &mut [Statement], visit: &mut impl FnMut(&mut SourceRef)) {
    for statement in statements {
        match statement {
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                visit_unreachable_sources(then_body, visit);
                visit_unreachable_sources(else_body, visit);
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    visit_unreachable_sources(&mut arm.body, visit);
                }
            }
            Statement::While { body, .. } => visit_unreachable_sources(body, visit),
            Statement::Unreachable { source } => visit(source),
            Statement::Let { .. }
            | Statement::Set { .. }
            | Statement::Emit { .. }
            | Statement::Finish { .. }
            | Statement::Delegate { .. } => {}
        }
    }
}

fn is_site_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CapabilityKind {
    Reads,
    Updates,
}

impl CapabilityKind {
    fn source_name(self) -> &'static str {
        match self {
            Self::Reads => "reads",
            Self::Updates => "updates",
        }
    }
}

#[derive(Clone, Debug)]
struct CapabilityParameter {
    parameter: uhura_syntax::ast::Parameter,
    declaration: String,
    kind: CapabilityKind,
}

#[derive(Clone, Debug)]
enum ParameterKind {
    Configuration(uhura_syntax::ast::Parameter),
    Capability(CapabilityParameter),
}

#[derive(Clone, Debug)]
struct HandleBinding {
    provider: String,
    kind: CapabilityKind,
    members: BTreeSet<String>,
}

#[derive(Clone, Debug)]
struct UnitUpdate {
    owner: String,
    name: String,
    parameters: Vec<uhura_syntax::ast::Parameter>,
    body: uhura_syntax::ast::Block,
    context: RewriteContext,
}

#[derive(Clone, Debug, Default)]
struct RewriteContext {
    owner: Option<String>,
    bindings: BTreeMap<String, String>,
    owned: BTreeMap<String, String>,
    ports: BTreeMap<String, String>,
    substitutions: BTreeMap<String, uhura_syntax::ast::Expression>,
    handles: BTreeMap<String, HandleBinding>,
    instances: BTreeMap<String, InstanceInterface>,
    lexical: BTreeMap<String, String>,
    local_prefix: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct InstanceInterface {
    declaration: String,
    reads: BTreeSet<String>,
    updates: BTreeSet<String>,
}

#[derive(Clone, Debug)]
struct InstancePlan {
    name: String,
    template: PartTemplate,
    bindings: BTreeMap<String, String>,
    arguments: Vec<uhura_syntax::ast::Expression>,
    parameters: Vec<ParameterKind>,
    handles: BTreeMap<String, HandleBinding>,
    owned: BTreeMap<String, String>,
    span: uhura_syntax::ast::Span,
}

#[derive(Clone, Debug, Default)]
struct MachinePlan {
    instances: Vec<InstancePlan>,
    interfaces: BTreeMap<String, InstanceInterface>,
    unit_updates: BTreeMap<String, UnitUpdate>,
    public_parts: BTreeSet<String>,
    linked_public_declarations: BTreeSet<String>,
}

struct AggregateSections<'a> {
    events: &'a mut uhura_syntax::ast::ProtocolSection,
    commands: &'a mut uhura_syntax::ast::ProtocolSection,
    state: &'a mut uhura_syntax::ast::StateSection,
    observe: &'a mut uhura_syntax::ast::ObserveSection,
    other: &'a mut Vec<uhura_syntax::ast::MachineMember>,
}

pub(super) fn compose_project(
    sources: &[uhura_syntax::ast::Module],
    bindings: &BTreeMap<String, BTreeMap<String, String>>,
    standard_imports: &BTreeMap<(String, String), uhura_syntax::ast::Span>,
    diagnostics: &mut Vec<Diagnostic>,
) -> CompositionOutput {
    let package = sources
        .first()
        .map(|source| source.identity.package.as_str())
        .unwrap_or_default();
    let mut catalog = PartCatalog::default();
    catalog.capture_package(package, sources, bindings, standard_imports, diagnostics);
    compose_package(package, sources, bindings, &catalog, diagnostics)
}

pub(super) fn compose_package(
    package: &str,
    sources: &[uhura_syntax::ast::Module],
    bindings: &BTreeMap<String, BTreeMap<String, String>>,
    catalog: &PartCatalog,
    diagnostics: &mut Vec<Diagnostic>,
) -> CompositionOutput {
    let machines = collect_machines(sources);
    let mut output = sources.to_vec();
    let mut machine_part_dependencies = BTreeMap::new();
    let mut linked_public_declarations = BTreeSet::new();
    let mut standard_imports = BTreeMap::new();
    let mut helpers =
        BTreeMap::<String, (uhura_syntax::ast::Module, BTreeMap<String, String>)>::new();
    let mut machine_sites = BTreeMap::new();

    for module in &mut output {
        let module_bindings = bindings
            .get(&module.identity.module)
            .cloned()
            .unwrap_or_default();
        let module_name = module.identity.module.clone();
        let declarations = std::mem::take(&mut module.declarations);
        module.declarations = declarations
            .into_iter()
            .filter_map(|mut declaration| match &mut declaration.kind {
                uhura_syntax::ast::DeclarationKind::Part(_) => None,
                uhura_syntax::ast::DeclarationKind::Machine(machine) => {
                    let mut plan = plan_machine(
                        package,
                        &module_name,
                        machine,
                        &module_bindings,
                        catalog,
                        &machines,
                        diagnostics,
                    );
                    let lowered_name = module_bindings
                        .get(&machine.name.text)
                        .map(String::as_str)
                        .unwrap_or(&machine.name.text);
                    let machine_id = format!("{package}::{lowered_name}");
                    let mut sites = collect_machine_sites(package, &machine_id, machine, &plan);
                    if !plan.public_parts.is_empty() {
                        machine_part_dependencies
                            .insert(machine_id.clone(), plan.public_parts.clone());
                    }
                    linked_public_declarations.extend(plan.linked_public_declarations.clone());
                    for instance in &plan.instances {
                        if instance.template.package == package {
                            continue;
                        }
                        standard_imports.extend(instance.template.standard_imports.clone());
                        let Some(helper) = &instance.template.helper else {
                            continue;
                        };
                        let mut helper_module = helper.module.clone();
                        helper_module.identity.package = package.to_owned();
                        helpers
                            .entry(helper_module.identity.module.clone())
                            .or_insert_with(|| (helper_module, helper.bindings.clone()));
                    }
                    mark_lowered_site_spans(machine, &mut plan, &mut sites);
                    compose_machine(machine, &module_bindings, plan, diagnostics);
                    machine_sites.insert(machine_id, sites);
                    Some(declaration)
                }
                _ => Some(declaration),
            })
            .collect();
    }
    let mut helper_bindings = BTreeMap::new();
    for (module, (helper, bindings)) in helpers {
        helper_bindings.insert(module, bindings);
        output.push(helper);
    }

    CompositionOutput {
        modules: output,
        machine_part_dependencies,
        linked_public_declarations,
        standard_imports,
        helper_bindings,
        machine_sites,
    }
}

fn collect_machine_sites(
    package: &str,
    machine_id: &str,
    machine: &uhura_syntax::ast::MachineDeclaration,
    plan: &MachinePlan,
) -> MachineSites {
    let mut sites = MachineSites::default();
    let mut invariant = 0;
    let mut before_commit = 0;
    let root = "root";
    for member in &machine.members {
        match &member.kind {
            uhura_syntax::ast::MachineMemberKind::Invariant(value) => {
                collect_invariant_sites(
                    &mut sites,
                    machine_id,
                    root,
                    invariant,
                    value,
                    member.span,
                );
                invariant += 1;
            }
            uhura_syntax::ast::MachineMemberKind::Handler(value) => {
                let semantic = format!("handler/{}", source_selector_name(&value.input));
                let lowered = lowered_selector_name(&value.input);
                collect_unreachable_block(
                    &mut sites,
                    machine_id,
                    root,
                    SiteContainer::Handler(lowered),
                    &semantic,
                    &value.body,
                );
            }
            uhura_syntax::ast::MachineMemberKind::Update(value) => {
                let semantic = format!("update/{}", value.name.text);
                collect_unreachable_block(
                    &mut sites,
                    machine_id,
                    root,
                    SiteContainer::Update(value.name.text.clone()),
                    &semantic,
                    &value.body,
                );
            }
            uhura_syntax::ast::MachineMemberKind::BeforeCommit(value) => {
                let semantic = if before_commit == 0 {
                    "before-commit".to_string()
                } else {
                    format!("before-commit/{before_commit}")
                };
                collect_unreachable_block(
                    &mut sites,
                    machine_id,
                    root,
                    SiteContainer::BeforeCommit,
                    &semantic,
                    &value.body,
                );
                before_commit += 1;
            }
            _ => {}
        }
    }
    fill_site_source_package(&mut sites, package);

    for instance in &plan.instances {
        let owner = instance.name.as_str();
        let mut invariant = 0;
        for member in &instance.template.declaration.members {
            match &member.kind {
                uhura_syntax::ast::PartMemberKind::Invariant(value) => {
                    collect_invariant_sites(
                        &mut sites,
                        machine_id,
                        owner,
                        invariant,
                        value,
                        member.span,
                    );
                    invariant += 1;
                }
                uhura_syntax::ast::PartMemberKind::Handler(value) => {
                    let semantic = format!("handler/{}", source_selector_name(&value.input));
                    let lowered = lowered_part_selector_name(instance, &value.input);
                    collect_unreachable_block(
                        &mut sites,
                        machine_id,
                        owner,
                        SiteContainer::Handler(lowered),
                        &semantic,
                        &value.body,
                    );
                }
                uhura_syntax::ast::PartMemberKind::Update(value) => {
                    let semantic = format!("update/{}", value.name.text);
                    let lowered = instance
                        .owned
                        .get(&value.name.text)
                        .cloned()
                        .unwrap_or_else(|| format!("{owner}.{}", value.name.text));
                    collect_unreachable_block(
                        &mut sites,
                        machine_id,
                        owner,
                        SiteContainer::Update(lowered),
                        &semantic,
                        &value.body,
                    );
                }
                _ => {}
            }
        }
        fill_site_source_package(&mut sites, &instance.template.package);
    }
    sites
}

fn collect_invariant_sites(
    sites: &mut MachineSites,
    machine_id: &str,
    owner: &str,
    ordinal: usize,
    declaration: &uhura_syntax::ast::InvariantDeclaration,
    span: uhura_syntax::ast::Span,
) {
    for condition in 0..declaration.conditions.len() {
        let (kind, path, origin_span) = if declaration.conditions.len() == 1 {
            ("invariant", format!("invariant/{ordinal}"), span)
        } else {
            (
                "invariant_condition",
                format!("invariant/{ordinal}/condition/{condition}"),
                declaration.conditions[condition].span,
            )
        };
        sites.invariants.push(SiteOrigin {
            node: semantic_node_id(machine_id, owner, kind, &path),
            owner: owner.into(),
            kind,
            path,
            source_package: String::new(),
            span: origin_span,
            lowered_span: origin_span,
        });
    }
}

fn fill_site_source_package(sites: &mut MachineSites, package: &str) {
    for origin in sites
        .invariants
        .iter_mut()
        .chain(sites.unreachable.values_mut().flatten())
    {
        if origin.source_package.is_empty() {
            origin.source_package = package.into();
        }
    }
}

fn mark_lowered_site_spans(
    machine: &mut uhura_syntax::ast::MachineDeclaration,
    plan: &mut MachinePlan,
    sites: &mut MachineSites,
) {
    let mut marker = 0u32;
    for origins in sites.unreachable.values_mut() {
        for origin in origins {
            origin.lowered_span =
                uhura_syntax::ast::Span::new(u32::MAX, marker, marker.saturating_add(1));
            marker = marker.saturating_add(2);
        }
    }

    let origins = sites
        .unreachable
        .values()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    let mut used = BTreeSet::new();

    for member in &mut machine.members {
        let body = match &mut member.kind {
            uhura_syntax::ast::MachineMemberKind::Handler(value) => &mut value.body,
            uhura_syntax::ast::MachineMemberKind::Update(value) => &mut value.body,
            uhura_syntax::ast::MachineMemberKind::BeforeCommit(value) => &mut value.body,
            _ => continue,
        };
        mark_owner_site_spans(body, "root", &origins, &mut used);
    }
    for instance in &mut plan.instances {
        for member in &mut instance.template.declaration.members {
            let body = match &mut member.kind {
                uhura_syntax::ast::PartMemberKind::Handler(value) => &mut value.body,
                uhura_syntax::ast::PartMemberKind::Update(value) => &mut value.body,
                _ => continue,
            };
            mark_owner_site_spans(body, &instance.name, &origins, &mut used);
        }
    }
}

fn mark_owner_site_spans(
    body: &mut uhura_syntax::ast::Block,
    owner: &str,
    origins: &[SiteOrigin],
    used: &mut BTreeSet<String>,
) {
    visit_authored_unreachable_spans(body, &mut |site| {
        let Some(origin) = origins.iter().find(|origin| {
            origin.owner == owner && *site == origin.span && !used.contains(&origin.node)
        }) else {
            return;
        };
        *site = origin.lowered_span;
        used.insert(origin.node.clone());
    });
}

fn visit_authored_unreachable_spans(
    block: &mut uhura_syntax::ast::Block,
    visit: &mut impl FnMut(&mut uhura_syntax::ast::Span),
) {
    for statement in &mut block.statements {
        match &mut statement.kind {
            uhura_syntax::ast::StatementKind::Let { value, .. }
            | uhura_syntax::ast::StatementKind::Assign { value, .. } => {
                visit_expression_unreachable_spans(value, visit);
            }
            uhura_syntax::ast::StatementKind::Emit { output, .. } => {
                for argument in &mut output.arguments {
                    visit_expression_unreachable_spans(argument, visit);
                }
            }
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                visit_expression_unreachable_spans(condition, visit);
                visit_expression_unreachable_spans(decreases, visit);
                visit_authored_unreachable_spans(body, visit);
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => visit(&mut statement.span),
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                visit_expression_unreachable_spans(expression, visit);
            }
        }
    }
    if let Some(tail) = &mut block.tail {
        visit_expression_unreachable_spans(tail, visit);
    }
}

fn visit_expression_unreachable_spans(
    expression: &mut uhura_syntax::ast::Expression,
    visit: &mut impl FnMut(&mut uhura_syntax::ast::Span),
) {
    use uhura_syntax::ast::ExpressionKind as E;
    match &mut expression.kind {
        E::Sequence(values) | E::Tuple(values) => {
            for value in values {
                visit_expression_unreachable_spans(value, visit);
            }
        }
        E::Group(value)
        | E::Member { value, .. }
        | E::Unary { value, .. }
        | E::Is { value, .. } => visit_expression_unreachable_spans(value, visit),
        E::Record(value) => {
            for field in &mut value.fields {
                if let Some(value) = &mut field.value {
                    visit_expression_unreachable_spans(value, visit);
                }
            }
            if let Some(base) = &mut value.base {
                visit_expression_unreachable_spans(base, visit);
            }
        }
        E::AnonymousRecord(entries) => {
            for entry in entries {
                visit_expression_unreachable_spans(&mut entry.key, visit);
                visit_expression_unreachable_spans(&mut entry.value, visit);
            }
        }
        E::Block(block) => visit_authored_unreachable_spans(block, visit),
        E::Call { callee, arguments } => {
            visit_expression_unreachable_spans(callee, visit);
            for argument in arguments {
                match argument {
                    uhura_syntax::ast::CallArgument::Expression(value) => {
                        visit_expression_unreachable_spans(value, visit);
                    }
                    uhura_syntax::ast::CallArgument::Binder(value) => {
                        visit_expression_unreachable_spans(&mut value.body, visit);
                    }
                }
            }
        }
        E::Index { value, index }
        | E::Binary {
            left: value,
            right: index,
            ..
        }
        | E::Compare {
            left: value,
            right: index,
            ..
        } => {
            visit_expression_unreachable_spans(value, visit);
            visit_expression_unreachable_spans(index, visit);
        }
        E::If(value) => {
            visit_expression_unreachable_spans(&mut value.condition, visit);
            visit_authored_unreachable_spans(&mut value.then_branch, visit);
            if let Some(branch) = &mut value.else_branch {
                match branch {
                    uhura_syntax::ast::ElseBranch::Block(block) => {
                        visit_authored_unreachable_spans(block, visit);
                    }
                    uhura_syntax::ast::ElseBranch::If(value) => {
                        visit_expression_unreachable_spans(value, visit);
                    }
                }
            }
        }
        E::Match(value) => {
            visit_expression_unreachable_spans(&mut value.value, visit);
            for arm in &mut value.arms {
                visit_expression_unreachable_spans(&mut arm.value, visit);
            }
        }
        E::Return(Some(value)) => visit_expression_unreachable_spans(value, visit),
        E::Literal(_) | E::Unit | E::Name(_) | E::Return(None) => {}
    }
}

fn collect_unreachable_block(
    sites: &mut MachineSites,
    machine_id: &str,
    owner: &str,
    container: SiteContainer,
    parent: &str,
    block: &uhura_syntax::ast::Block,
) {
    for (index, statement) in block.statements.iter().enumerate() {
        let path = format!("{parent}/statement/{index}");
        collect_unreachable_statement(sites, machine_id, owner, &container, &path, statement);
    }
    if let Some(tail) = &block.tail {
        collect_unreachable_expression(
            sites,
            machine_id,
            owner,
            &container,
            &format!("{parent}/tail"),
            tail,
        );
    }
}

pub(super) fn authored_unreachable_sites(
    block: &uhura_syntax::ast::Block,
    parent: &str,
) -> Vec<(String, uhura_syntax::ast::Span)> {
    let mut sites = MachineSites::default();
    collect_unreachable_block(
        &mut sites,
        "site-map",
        "root",
        SiteContainer::BeforeCommit,
        parent,
        block,
    );
    sites
        .unreachable
        .remove(&SiteContainer::BeforeCommit)
        .unwrap_or_default()
        .into_iter()
        .map(|origin| (origin.path, origin.span))
        .collect()
}

fn collect_unreachable_statement(
    sites: &mut MachineSites,
    machine_id: &str,
    owner: &str,
    container: &SiteContainer,
    path: &str,
    statement: &uhura_syntax::ast::Statement,
) {
    match &statement.kind {
        uhura_syntax::ast::StatementKind::Let { value, .. }
        | uhura_syntax::ast::StatementKind::Assign { value, .. } => {
            collect_unreachable_expression(sites, machine_id, owner, container, path, value);
        }
        uhura_syntax::ast::StatementKind::Emit { output, .. } => {
            for (index, argument) in output.arguments.iter().enumerate() {
                collect_unreachable_expression(
                    sites,
                    machine_id,
                    owner,
                    container,
                    &format!("{path}/argument/{index}"),
                    argument,
                );
            }
        }
        uhura_syntax::ast::StatementKind::While {
            condition,
            decreases,
            body,
        } => {
            collect_unreachable_expression(
                sites,
                machine_id,
                owner,
                container,
                &format!("{path}/condition"),
                condition,
            );
            collect_unreachable_expression(
                sites,
                machine_id,
                owner,
                container,
                &format!("{path}/decreases"),
                decreases,
            );
            collect_unreachable_block(
                sites,
                machine_id,
                owner,
                container.clone(),
                &format!("{path}/body"),
                body,
            );
        }
        uhura_syntax::ast::StatementKind::Unreachable { .. } => {
            sites
                .unreachable
                .entry(container.clone())
                .or_default()
                .push(SiteOrigin {
                    node: semantic_node_id(machine_id, owner, "unreachable", path),
                    owner: owner.into(),
                    kind: "unreachable",
                    path: path.into(),
                    source_package: String::new(),
                    span: statement.span,
                    lowered_span: statement.span,
                });
        }
        uhura_syntax::ast::StatementKind::Expression { expression, .. }
        | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
            collect_unreachable_expression(sites, machine_id, owner, container, path, expression);
        }
    }
}

fn collect_unreachable_expression(
    sites: &mut MachineSites,
    machine_id: &str,
    owner: &str,
    container: &SiteContainer,
    path: &str,
    expression: &uhura_syntax::ast::Expression,
) {
    use uhura_syntax::ast::ExpressionKind as E;
    match &expression.kind {
        E::Sequence(values) | E::Tuple(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_unreachable_expression(
                    sites,
                    machine_id,
                    owner,
                    container,
                    &format!("{path}/value/{index}"),
                    value,
                );
            }
        }
        E::Group(value)
        | E::Member { value, .. }
        | E::Unary { value, .. }
        | E::Is { value, .. } => {
            collect_unreachable_expression(sites, machine_id, owner, container, path, value);
        }
        E::Record(value) => {
            for field in &value.fields {
                if let Some(value) = &field.value {
                    collect_unreachable_expression(
                        sites,
                        machine_id,
                        owner,
                        container,
                        &format!("{path}/field/{}", field.name.text),
                        value,
                    );
                }
            }
            if let Some(base) = &value.base {
                collect_unreachable_expression(
                    sites,
                    machine_id,
                    owner,
                    container,
                    &format!("{path}/base"),
                    base,
                );
            }
        }
        E::AnonymousRecord(entries) => {
            for (index, entry) in entries.iter().enumerate() {
                collect_unreachable_expression(
                    sites,
                    machine_id,
                    owner,
                    container,
                    &format!("{path}/entry/{index}/key"),
                    &entry.key,
                );
                collect_unreachable_expression(
                    sites,
                    machine_id,
                    owner,
                    container,
                    &format!("{path}/entry/{index}/value"),
                    &entry.value,
                );
            }
        }
        E::Block(block) => {
            collect_unreachable_block(sites, machine_id, owner, container.clone(), path, block);
        }
        E::Call { callee, arguments } => {
            collect_unreachable_expression(
                sites,
                machine_id,
                owner,
                container,
                &format!("{path}/callee"),
                callee,
            );
            for (index, argument) in arguments.iter().enumerate() {
                let value = match argument {
                    uhura_syntax::ast::CallArgument::Expression(value) => value,
                    uhura_syntax::ast::CallArgument::Binder(value) => &value.body,
                };
                collect_unreachable_expression(
                    sites,
                    machine_id,
                    owner,
                    container,
                    &format!("{path}/argument/{index}"),
                    value,
                );
            }
        }
        E::Index { value, index }
        | E::Binary {
            left: value,
            right: index,
            ..
        }
        | E::Compare {
            left: value,
            right: index,
            ..
        } => {
            collect_unreachable_expression(
                sites,
                machine_id,
                owner,
                container,
                &format!("{path}/left"),
                value,
            );
            collect_unreachable_expression(
                sites,
                machine_id,
                owner,
                container,
                &format!("{path}/right"),
                index,
            );
        }
        E::If(value) => {
            collect_unreachable_expression(
                sites,
                machine_id,
                owner,
                container,
                &format!("{path}/condition"),
                &value.condition,
            );
            collect_unreachable_block(
                sites,
                machine_id,
                owner,
                container.clone(),
                &format!("{path}/then"),
                &value.then_branch,
            );
            if let Some(branch) = &value.else_branch {
                match branch {
                    uhura_syntax::ast::ElseBranch::Block(block) => collect_unreachable_block(
                        sites,
                        machine_id,
                        owner,
                        container.clone(),
                        &format!("{path}/else"),
                        block,
                    ),
                    uhura_syntax::ast::ElseBranch::If(value) => collect_unreachable_expression(
                        sites,
                        machine_id,
                        owner,
                        container,
                        &format!("{path}/else"),
                        value,
                    ),
                }
            }
        }
        E::Match(value) => {
            collect_unreachable_expression(
                sites,
                machine_id,
                owner,
                container,
                &format!("{path}/value"),
                &value.value,
            );
            for arm in &value.arms {
                collect_unreachable_expression(
                    sites,
                    machine_id,
                    owner,
                    container,
                    &format!("{path}/branch/{}", pattern_site_path(&arm.pattern)),
                    &arm.value,
                );
            }
        }
        E::Return(Some(value)) => {
            collect_unreachable_expression(sites, machine_id, owner, container, path, value);
        }
        E::Literal(_) | E::Unit | E::Name(_) | E::Return(None) => {}
    }
}

fn pattern_site_path(pattern: &uhura_syntax::ast::Pattern) -> String {
    use uhura_syntax::ast::PatternKind as P;
    match &pattern.kind {
        P::Wildcard => "_".into(),
        P::Binder(_) => "bind".into(),
        P::Literal(value) => {
            use uhura_syntax::ast::PatternLiteral as L;
            match value {
                L::Bool(value) => format!("literal/bool/{value}"),
                L::Integer { raw, negative } => {
                    let canonical = raw.trim_start_matches('0');
                    let canonical = if canonical.is_empty() { "0" } else { canonical };
                    let sign = if *negative && canonical != "0" {
                        "-"
                    } else {
                        ""
                    };
                    format!("literal/integer/{sign}{canonical}")
                }
                L::Decimal { raw, negative } => {
                    let signed = if *negative {
                        format!("-{raw}")
                    } else {
                        raw.clone()
                    };
                    let canonical = signed
                        .parse::<uhura_core::Decimal>()
                        .map_or(signed, |value| value.to_string());
                    format!("literal/decimal/{canonical}")
                }
                L::Text { value, .. } => {
                    format!("literal/text/{}", uhura_base::sha256_hex(value.as_bytes()))
                }
                L::Unit => "literal/unit".into(),
            }
        }
        P::Group(value) => pattern_site_path(value),
        P::Tuple(values) => format!(
            "tuple/{}",
            values
                .iter()
                .map(pattern_site_path)
                .collect::<Vec<_>>()
                .join("/")
        ),
        P::Constructor(value) => value
            .segments
            .iter()
            .map(|value| value.text.as_str())
            .collect::<Vec<_>>()
            .join("."),
        P::TupleConstructor {
            constructor,
            arguments,
        } => format!(
            "{}/{}",
            constructor
                .segments
                .iter()
                .map(|value| value.text.as_str())
                .collect::<Vec<_>>()
                .join("."),
            arguments
                .iter()
                .map(pattern_site_path)
                .collect::<Vec<_>>()
                .join("/")
        ),
        P::Record {
            constructor,
            fields,
            rest,
        } => {
            let mut fields = fields
                .iter()
                .map(|field| {
                    format!(
                        "{}={}",
                        field.name.text,
                        field
                            .pattern
                            .as_ref()
                            .map_or_else(|| "bind".into(), pattern_site_path)
                    )
                })
                .collect::<Vec<_>>();
            fields.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            format!(
                "{}/record/{}/{}",
                constructor
                    .segments
                    .iter()
                    .map(|value| value.text.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
                fields.join("/"),
                rest
            )
        }
        P::AnonymousRecord { fields, rest } => {
            let mut fields = fields
                .iter()
                .map(|field| {
                    format!(
                        "{}={}",
                        field.name.text,
                        field
                            .pattern
                            .as_ref()
                            .map_or_else(|| "bind".into(), pattern_site_path)
                    )
                })
                .collect::<Vec<_>>();
            fields.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            format!("anonymous-record/{}/{}", fields.join("/"), rest)
        }
        P::Alternative(values) => {
            let mut values = values.iter().map(pattern_site_path).collect::<Vec<_>>();
            values.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            values.join("|")
        }
    }
}

fn source_selector_name(selector: &uhura_syntax::ast::ProtocolSelector) -> String {
    selector.owner.as_ref().map_or_else(
        || selector.variant.text.clone(),
        |owner| format!("{}.{}", owner.text, selector.variant.text),
    )
}

fn lowered_selector_name(selector: &uhura_syntax::ast::ProtocolSelector) -> String {
    selector.owner.as_ref().map_or_else(
        || selector.variant.text.clone(),
        |owner| {
            format!(
                "{}.{}",
                owner.text,
                lower_protocol_variant_for_site(&selector.variant.text)
            )
        },
    )
}

fn lowered_part_selector_name(
    instance: &InstancePlan,
    selector: &uhura_syntax::ast::ProtocolSelector,
) -> String {
    selector.owner.as_ref().map_or_else(
        || format!("{}.{}", instance.name, selector.variant.text),
        |owner| {
            let port = instance
                .template
                .declaration
                .members
                .iter()
                .filter_map(|member| match &member.kind {
                    uhura_syntax::ast::PartMemberKind::Port(port) => Some(port),
                    _ => None,
                })
                .find(|port| port.name.text == owner.text)
                .map_or_else(
                    || format!("{}.{}", instance.name, owner.text),
                    |port| format!("{}.{}", instance.name, port.name.text),
                );
            format!(
                "{port}.{}",
                lower_protocol_variant_for_site(&selector.variant.text)
            )
        },
    )
}

fn lower_protocol_variant_for_site(value: &str) -> String {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_lowercase().chain(characters).collect()
}

impl PartCatalog {
    pub(super) fn capture_package(
        &mut self,
        package: &str,
        sources: &[uhura_syntax::ast::Module],
        bindings: &BTreeMap<String, BTreeMap<String, String>>,
        standard_imports: &BTreeMap<(String, String), uhura_syntax::ast::Span>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let public_names = sources
            .iter()
            .flat_map(|source| &source.declarations)
            .filter_map(|declaration| match &declaration.kind {
                uhura_syntax::ast::DeclarationKind::Machine(value)
                    if value.visibility == uhura_syntax::ast::Visibility::Public =>
                {
                    Some(value.name.text.clone())
                }
                uhura_syntax::ast::DeclarationKind::Part(value)
                    if value.visibility == uhura_syntax::ast::Visibility::Public =>
                {
                    Some(value.name.text.clone())
                }
                uhura_syntax::ast::DeclarationKind::Ui(value)
                    if value.visibility == uhura_syntax::ast::Visibility::Public =>
                {
                    Some(value.name.text.clone())
                }
                uhura_syntax::ast::DeclarationKind::Struct(value)
                    if value.visibility == uhura_syntax::ast::Visibility::Public =>
                {
                    Some(value.name.text.clone())
                }
                uhura_syntax::ast::DeclarationKind::Enum(value)
                    if value.visibility == uhura_syntax::ast::Visibility::Public =>
                {
                    Some(value.name.text.clone())
                }
                uhura_syntax::ast::DeclarationKind::Key(value)
                    if value.visibility == uhura_syntax::ast::Visibility::Public =>
                {
                    Some(value.name.text.clone())
                }
                uhura_syntax::ast::DeclarationKind::Const(value)
                    if value.visibility == uhura_syntax::ast::Visibility::Public =>
                {
                    Some(value.name.text.clone())
                }
                uhura_syntax::ast::DeclarationKind::Function(value)
                    if value.visibility == uhura_syntax::ast::Visibility::Public =>
                {
                    Some(value.name.text.clone())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        for source in sources {
            let module_bindings = bindings
                .get(&source.identity.module)
                .cloned()
                .unwrap_or_default();
            for declaration in &source.declarations {
                let (name, visibility, kind) = match &declaration.kind {
                    uhura_syntax::ast::DeclarationKind::Machine(value) => {
                        (&value.name, value.visibility, "machine")
                    }
                    uhura_syntax::ast::DeclarationKind::Part(value) => {
                        (&value.name, value.visibility, "part")
                    }
                    uhura_syntax::ast::DeclarationKind::Ui(value) => {
                        (&value.name, value.visibility, "ui")
                    }
                    uhura_syntax::ast::DeclarationKind::Struct(value) => {
                        (&value.name, value.visibility, "struct")
                    }
                    uhura_syntax::ast::DeclarationKind::Enum(value) => {
                        (&value.name, value.visibility, "enum")
                    }
                    uhura_syntax::ast::DeclarationKind::Key(value) => {
                        (&value.name, value.visibility, "key")
                    }
                    uhura_syntax::ast::DeclarationKind::Const(value) => {
                        (&value.name, value.visibility, "const")
                    }
                    uhura_syntax::ast::DeclarationKind::Function(value) => {
                        (&value.name, value.visibility, "function")
                    }
                    uhura_syntax::ast::DeclarationKind::Scenario(_)
                    | uhura_syntax::ast::DeclarationKind::Example(_)
                    | uhura_syntax::ast::DeclarationKind::Checkpoint(_) => continue,
                };
                let lowered = module_bindings
                    .get(&name.text)
                    .cloned()
                    .unwrap_or_else(|| name.text.clone());
                self.kinds
                    .insert((package.to_owned(), lowered.clone()), kind);
                if visibility == uhura_syntax::ast::Visibility::Public {
                    let external = crate::source::external_lowering_name(package, &name.text);
                    self.external_kinds.insert(external.clone(), kind);
                    self.external_identities
                        .insert(external, format!("{package}::{}", name.text));
                }

                let uhura_syntax::ast::DeclarationKind::Part(part) = &declaration.kind else {
                    continue;
                };
                let private_declarations = source
                    .declarations
                    .iter()
                    .filter(|declaration| {
                        matches!(
                            &declaration.kind,
                            uhura_syntax::ast::DeclarationKind::Struct(value)
                                if value.visibility == uhura_syntax::ast::Visibility::Private
                        ) || matches!(
                            &declaration.kind,
                            uhura_syntax::ast::DeclarationKind::Enum(value)
                                if value.visibility == uhura_syntax::ast::Visibility::Private
                        ) || matches!(
                            &declaration.kind,
                            uhura_syntax::ast::DeclarationKind::Key(value)
                                if value.visibility == uhura_syntax::ast::Visibility::Private
                        ) || matches!(
                            &declaration.kind,
                            uhura_syntax::ast::DeclarationKind::Const(value)
                                if value.visibility == uhura_syntax::ast::Visibility::Private
                        ) || matches!(
                            &declaration.kind,
                            uhura_syntax::ast::DeclarationKind::Function(value)
                                if value.visibility == uhura_syntax::ast::Visibility::Private
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let private_lowerings = private_declarations
                    .iter()
                    .filter_map(|declaration| {
                        let name = match &declaration.kind {
                            uhura_syntax::ast::DeclarationKind::Struct(value) => &value.name.text,
                            uhura_syntax::ast::DeclarationKind::Enum(value) => &value.name.text,
                            uhura_syntax::ast::DeclarationKind::Key(value) => &value.name.text,
                            uhura_syntax::ast::DeclarationKind::Const(value) => &value.name.text,
                            uhura_syntax::ast::DeclarationKind::Function(value) => &value.name.text,
                            _ => return None,
                        };
                        module_bindings.get(name).map(|lowered| lowered.as_str())
                    })
                    .collect::<BTreeSet<_>>();
                let external_bindings = module_bindings
                    .iter()
                    .map(|(authored, lowered)| {
                        let lowered = if public_names.contains(lowered) {
                            crate::source::external_lowering_name(package, lowered)
                        } else if private_lowerings.contains(lowered.as_str()) {
                            private_part_lowering_name(package, &part.name.text, lowered)
                        } else {
                            lowered.clone()
                        };
                        (authored.clone(), lowered)
                    })
                    .collect::<BTreeMap<_, _>>();
                let helper = (!private_declarations.is_empty()).then(|| {
                    let scope = private_part_scope(package, &part.name.text);
                    let mut module = source.clone();
                    module.identity.module = scope;
                    module.uses.clear();
                    module.declarations = private_declarations;
                    PartHelperTemplate {
                        module,
                        bindings: external_bindings.clone(),
                    }
                });
                let template = PartTemplate {
                    package: package.to_owned(),
                    module: source.identity.module.clone(),
                    visibility: part.visibility,
                    public_id: (part.visibility == uhura_syntax::ast::Visibility::Public)
                        .then(|| format!("{package}::{}", part.name.text)),
                    declaration: part.clone(),
                    bindings: module_bindings.clone(),
                    external_bindings,
                    standard_imports: standard_imports
                        .iter()
                        .filter(|(_, import_span)| import_span.file == source.identity.file)
                        .map(|(identity, import_span)| (identity.clone(), *import_span))
                        .collect(),
                    helper,
                };
                let local_key = (package.to_owned(), lowered);
                if self.local.insert(local_key, template.clone()).is_some() {
                    composition_error(
                        diagnostics,
                        codes::DUPLICATE,
                        "uhura-0.4/part-name-collision",
                        format!(
                            "part declaration `{}` cannot be represented by one package-global composition identity",
                            part.name.text
                        ),
                        part.name.span,
                    );
                    continue;
                }
                if part.visibility == uhura_syntax::ast::Visibility::Public {
                    let external = crate::source::external_lowering_name(package, &part.name.text);
                    if self.external.insert(external, template).is_some() {
                        composition_error(
                            diagnostics,
                            codes::DUPLICATE,
                            "uhura-0.4/part-name-collision",
                            format!(
                                "public part declaration `{}` has more than one resolved provider",
                                part.name.text
                            ),
                            part.name.span,
                        );
                    }
                }
            }
        }
    }

    fn resolve(&self, package: &str, name: &str) -> Option<&PartTemplate> {
        self.local
            .get(&(package.to_owned(), name.to_owned()))
            .or_else(|| self.external.get(name))
    }

    fn kind(&self, package: &str, name: &str) -> Option<&'static str> {
        self.kinds
            .get(&(package.to_owned(), name.to_owned()))
            .copied()
            .or_else(|| self.external_kinds.get(name).copied())
    }

    fn external_identity(&self, name: &str) -> Option<&str> {
        self.external_identities.get(name).map(String::as_str)
    }
}

fn private_part_scope(package: &str, part: &str) -> String {
    let fingerprint =
        uhura_base::sha256_hex(format!("uhura-part-private/0\0{package}\0{part}").as_bytes());
    format!("__uhura_part_private_{}", &fingerprint[..24])
}

fn private_part_lowering_name(package: &str, part: &str, name: &str) -> String {
    format!("{}_{}", private_part_scope(package, part), name)
}

fn collect_machines(sources: &[uhura_syntax::ast::Module]) -> BTreeSet<String> {
    sources
        .iter()
        .flat_map(|source| &source.declarations)
        .filter_map(|declaration| match &declaration.kind {
            uhura_syntax::ast::DeclarationKind::Machine(machine) => Some(machine.name.text.clone()),
            _ => None,
        })
        .collect()
}

fn plan_machine(
    package: &str,
    module: &str,
    machine: &uhura_syntax::ast::MachineDeclaration,
    bindings: &BTreeMap<String, String>,
    catalog: &PartCatalog,
    machines: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> MachinePlan {
    let instances = machine
        .members
        .iter()
        .filter_map(|member| match &member.kind {
            uhura_syntax::ast::MachineMemberKind::Part(instance) => Some((instance, member.span)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let port_names = machine
        .members
        .iter()
        .filter_map(|member| match &member.kind {
            uhura_syntax::ast::MachineMemberKind::Port(port) => Some(port.name.text.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut preliminary = Vec::new();
    for (instance, span) in instances {
        if !seen.insert(instance.name.text.clone()) {
            composition_error(
                diagnostics,
                codes::DUPLICATE,
                "uhura-0.4/duplicate-part-instance",
                format!(
                    "machine `{}` composes part name `{}` more than once",
                    machine.name.text, instance.name.text
                ),
                instance.name.span,
            );
            continue;
        }
        if port_names.contains(instance.name.text.as_str()) {
            composition_error(
                diagnostics,
                codes::DUPLICATE,
                "uhura-0.4/part-port-owner-collision",
                format!(
                    "part owner `{}` collides with a root port prefix; dotted protocol constructors would be ambiguous",
                    instance.name.text
                ),
                instance.name.span,
            );
            continue;
        }
        let Some(local_name) = singular_type_name(&instance.part) else {
            composition_error(
                diagnostics,
                codes::TYPE_MISMATCH,
                "uhura-0.4/invalid-part-target",
                "a part instance must name one imported or same-module part declaration without type arguments",
                instance.part.span,
            );
            continue;
        };
        let canonical = bindings
            .get(local_name)
            .map(String::as_str)
            .unwrap_or(local_name);
        let Some(template) = catalog.resolve(package, canonical).cloned() else {
            let message = if machines.contains(canonical)
                || catalog.kind(package, canonical) == Some("machine")
            {
                format!("`{canonical}` is a machine and cannot be composed as a part")
            } else {
                format!("part declaration `{local_name}` does not resolve")
            };
            composition_error(
                diagnostics,
                codes::UNKNOWN_TYPE,
                "uhura-0.4/unknown-part",
                message,
                instance.part.span,
            );
            continue;
        };
        let explicitly_imported = bindings.contains_key(local_name);
        let same_module = template.package == package && template.module == module;
        if !same_module && !explicitly_imported {
            composition_error(
                diagnostics,
                codes::IMPORT,
                "uhura-0.4/unimported-part",
                format!(
                    "part `{canonical}` is declared in logical module `{}` and must be named by an explicit `use`",
                    template.module
                ),
                instance.part.span,
            );
            continue;
        }
        if !same_module && template.visibility != uhura_syntax::ast::Visibility::Public {
            composition_error(
                diagnostics,
                codes::IMPORT,
                "uhura-0.4/private-part",
                format!(
                    "part `{canonical}` is private to logical module `{}`",
                    template.module
                ),
                instance.part.span,
            );
            continue;
        }
        preliminary.push((
            instance.name.text.clone(),
            template,
            instance.arguments.clone(),
            span,
        ));
    }
    preliminary.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    let mut interfaces = BTreeMap::new();
    for (name, template, _, _) in &preliminary {
        let template_bindings = if template.package == package {
            &template.bindings
        } else {
            &template.external_bindings
        };
        interfaces.insert(
            name.clone(),
            InstanceInterface {
                declaration: template_bindings
                    .get(&template.declaration.name.text)
                    .cloned()
                    .unwrap_or_else(|| template.declaration.name.text.clone()),
                reads: public_computed_names(&template.declaration),
                updates: public_update_names(&template.declaration, diagnostics),
            },
        );
    }

    let mut plans = Vec::new();
    let mut public_parts = BTreeSet::new();
    let mut linked_public_declarations = BTreeSet::new();
    let mut read_edges = BTreeMap::<String, BTreeSet<String>>::new();
    let mut update_edges = BTreeMap::<String, BTreeSet<String>>::new();
    for (name, template, arguments, span) in preliminary {
        if let Some(public_id) = &template.public_id {
            public_parts.insert(public_id.clone());
        }
        let template_bindings = if template.package == package {
            template.bindings.clone()
        } else {
            template.external_bindings.clone()
        };
        if template.package != package {
            linked_public_declarations.extend(
                template_bindings
                    .values()
                    .filter_map(|name| catalog.external_identity(name))
                    .map(str::to_owned),
            );
        }
        let parameters = template
            .declaration
            .parameters
            .iter()
            .map(|parameter| classify_parameter(parameter, &template_bindings))
            .collect::<Vec<_>>();
        if parameters.len() != arguments.len() {
            composition_error(
                diagnostics,
                codes::ARITY,
                "uhura-0.4/part-argument-arity",
                format!(
                    "part `{}` expects {} composition arguments, got {}",
                    template.declaration.name.text,
                    parameters.len(),
                    arguments.len()
                ),
                span,
            );
        }
        let mut handles = BTreeMap::new();
        for (index, parameter) in parameters.iter().enumerate() {
            let Some(argument) = arguments.get(index) else {
                continue;
            };
            match parameter {
                ParameterKind::Configuration(_) => {
                    if dependency_argument(argument).is_some() {
                        composition_error(
                            diagnostics,
                            codes::TYPE_MISMATCH,
                            "uhura-0.4/unexpected-capability-argument",
                            "an ordinary part parameter cannot receive a `reads` or `updates` capability",
                            argument.span,
                        );
                    }
                    reject_non_config_binding(machine, argument, diagnostics);
                }
                ParameterKind::Capability(capability) => {
                    let Some((provider, kind)) = dependency_argument(argument) else {
                        composition_error(
                            diagnostics,
                            codes::TYPE_MISMATCH,
                            "uhura-0.4/invalid-capability-argument",
                            format!(
                                "parameter `{}` requires one exact direct-sibling `.{}` handle",
                                capability.parameter.name.text,
                                capability.kind.source_name()
                            ),
                            argument.span,
                        );
                        continue;
                    };
                    if provider == name {
                        composition_error(
                            diagnostics,
                            codes::DEPENDENCY_CYCLE,
                            "uhura-0.4/self-part-dependency",
                            "a part capability must name another direct sibling",
                            argument.span,
                        );
                        continue;
                    }
                    if kind != capability.kind {
                        composition_error(
                            diagnostics,
                            codes::TYPE_MISMATCH,
                            "uhura-0.4/wrong-capability-kind",
                            format!(
                                "parameter `{}` requires `.{}`, not `.{}`",
                                capability.parameter.name.text,
                                capability.kind.source_name(),
                                kind.source_name()
                            ),
                            argument.span,
                        );
                        continue;
                    }
                    let Some(interface) = interfaces.get(provider) else {
                        composition_error(
                            diagnostics,
                            codes::UNKNOWN_NAME,
                            "uhura-0.4/non-sibling-capability",
                            format!("`{provider}` is not a direct sibling part instance"),
                            argument.span,
                        );
                        continue;
                    };
                    if interface.declaration != capability.declaration {
                        composition_error(
                            diagnostics,
                            codes::TYPE_MISMATCH,
                            "uhura-0.4/capability-nominality",
                            format!(
                                "parameter `{}` requires `{}::{}`, but `{provider}` is an instance of `{}`",
                                capability.parameter.name.text,
                                capability.declaration,
                                match capability.kind {
                                    CapabilityKind::Reads => "Reads",
                                    CapabilityKind::Updates => "Updates",
                                },
                                interface.declaration
                            ),
                            argument.span,
                        );
                        continue;
                    }
                    let members = match kind {
                        CapabilityKind::Reads => {
                            read_edges
                                .entry(name.clone())
                                .or_default()
                                .insert(provider.to_owned());
                            interface.reads.clone()
                        }
                        CapabilityKind::Updates => {
                            update_edges
                                .entry(name.clone())
                                .or_default()
                                .insert(provider.to_owned());
                            interface.updates.clone()
                        }
                    };
                    handles.insert(
                        capability.parameter.name.text.clone(),
                        HandleBinding {
                            provider: provider.to_owned(),
                            kind,
                            members,
                        },
                    );
                }
            }
        }
        let owned = owned_names(&name, &template.declaration, diagnostics);
        plans.push(InstancePlan {
            name: name.clone(),
            template,
            bindings: template_bindings,
            arguments,
            parameters,
            handles,
            owned,
            span,
        });
    }
    reject_cycles("read", &read_edges, diagnostics, machine.name.span);
    reject_cycles("update", &update_edges, diagnostics, machine.name.span);

    let plan = MachinePlan {
        instances: plans,
        interfaces,
        unit_updates: BTreeMap::new(),
        public_parts,
        linked_public_declarations,
    };
    reject_unit_update_cycles(&plan.unit_updates, diagnostics, machine.name.span);
    plan
}

fn singular_type_name(path: &uhura_syntax::ast::TypePath) -> Option<&str> {
    (path.segments.len() == 1 && path.segments[0].arguments.is_empty())
        .then(|| path.segments[0].name.text.as_str())
}

fn classify_parameter(
    parameter: &uhura_syntax::ast::Parameter,
    bindings: &BTreeMap<String, String>,
) -> ParameterKind {
    let uhura_syntax::ast::TypeExpressionKind::Path(path) = &parameter.ty.kind else {
        return ParameterKind::Configuration(parameter.clone());
    };
    if path.segments.len() != 2
        || path
            .segments
            .iter()
            .any(|segment| !segment.arguments.is_empty())
    {
        return ParameterKind::Configuration(parameter.clone());
    }
    let declaration = bindings
        .get(&path.segments[0].name.text)
        .cloned()
        .unwrap_or_else(|| path.segments[0].name.text.clone());
    let kind = match path.segments[1].name.text.as_str() {
        "Reads" => CapabilityKind::Reads,
        "Updates" => CapabilityKind::Updates,
        _ => return ParameterKind::Configuration(parameter.clone()),
    };
    ParameterKind::Capability(CapabilityParameter {
        parameter: parameter.clone(),
        declaration,
        kind,
    })
}

fn dependency_argument(
    expression: &uhura_syntax::ast::Expression,
) -> Option<(&str, CapabilityKind)> {
    let uhura_syntax::ast::ExpressionKind::Member { value, member } = &expression.kind else {
        return None;
    };
    let uhura_syntax::ast::ExpressionKind::Name(name) = &value.kind else {
        return None;
    };
    if name.segments.len() != 1 {
        return None;
    }
    let kind = match member.text.as_str() {
        "reads" => CapabilityKind::Reads,
        "updates" => CapabilityKind::Updates,
        _ => return None,
    };
    Some((name.segments[0].text.as_str(), kind))
}

fn public_computed_names(part: &uhura_syntax::ast::PartDeclaration) -> BTreeSet<String> {
    part.members
        .iter()
        .filter_map(|member| match &member.kind {
            uhura_syntax::ast::PartMemberKind::Computed(value)
                if value.visibility == uhura_syntax::ast::Visibility::Public =>
            {
                Some(value.name.text.clone())
            }
            _ => None,
        })
        .collect()
}

fn public_update_names(
    part: &uhura_syntax::ast::PartDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    part.members
        .iter()
        .filter_map(|member| match &member.kind {
            uhura_syntax::ast::PartMemberKind::Update(value)
                if value.visibility == uhura_syntax::ast::Visibility::Public =>
            {
                if update_mentions_outcome(value) {
                    composition_error(
                        diagnostics,
                        codes::EFFECT,
                        "uhura-0.4/public-update-outcome",
                        format!(
                            "public part update `{}` must be outcome-independent",
                            value.name.text
                        ),
                        member.span,
                    );
                    None
                } else {
                    Some(value.name.text.clone())
                }
            }
            _ => None,
        })
        .collect()
}

fn is_unit_update(update: &uhura_syntax::ast::UpdateDeclaration) -> bool {
    update.result.is_none()
        || matches!(
            update.result.as_ref().map(|value| &value.kind),
            Some(uhura_syntax::ast::TypeExpressionKind::Unit)
        )
}

fn update_mentions_outcome(update: &uhura_syntax::ast::UpdateDeclaration) -> bool {
    update
        .parameters
        .iter()
        .any(|parameter| type_mentions(&parameter.ty, "Outcome"))
        || update
            .result
            .as_ref()
            .is_some_and(|result| type_mentions(result, "Outcome"))
}

fn type_mentions(ty: &uhura_syntax::ast::TypeExpression, name: &str) -> bool {
    match &ty.kind {
        uhura_syntax::ast::TypeExpressionKind::Path(path) => path.segments.iter().any(|segment| {
            segment.name.text == name
                || segment
                    .arguments
                    .iter()
                    .any(|argument| type_mentions(argument, name))
        }),
        uhura_syntax::ast::TypeExpressionKind::Tuple(values) => {
            values.iter().any(|value| type_mentions(value, name))
        }
        uhura_syntax::ast::TypeExpressionKind::Unit => false,
    }
}

fn owned_names(
    owner: &str,
    part: &uhura_syntax::ast::PartDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for member in &part.members {
        let name = match &member.kind {
            uhura_syntax::ast::PartMemberKind::Const(value) => Some(&value.name),
            uhura_syntax::ast::PartMemberKind::Function(value) => Some(&value.name),
            uhura_syntax::ast::PartMemberKind::Port(value) => Some(&value.name),
            uhura_syntax::ast::PartMemberKind::State(value) => {
                for field in &value.fields {
                    insert_owned_name(owner, &mut names, &field.name, diagnostics);
                }
                None
            }
            uhura_syntax::ast::PartMemberKind::Computed(value) => Some(&value.name),
            uhura_syntax::ast::PartMemberKind::Update(value) => Some(&value.name),
            _ => None,
        };
        if let Some(name) = name {
            insert_owned_name(owner, &mut names, name, diagnostics);
        }
    }
    names
}

fn insert_owned_name(
    owner: &str,
    names: &mut BTreeMap<String, String>,
    name: &uhura_syntax::ast::Identifier,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if names
        .insert(name.text.clone(), format!("{owner}.{}", name.text))
        .is_some()
    {
        composition_error(
            diagnostics,
            codes::DUPLICATE,
            "uhura-0.4/part-member-collision",
            format!(
                "part-owned name `{}` is declared in more than one value namespace",
                name.text
            ),
            name.span,
        );
    }
}

fn context_for_instance(
    instance: &InstancePlan,
    interfaces: &BTreeMap<String, InstanceInterface>,
) -> RewriteContext {
    let mut substitutions = BTreeMap::new();
    for (index, parameter) in instance.parameters.iter().enumerate() {
        if let ParameterKind::Configuration(parameter) = parameter
            && let Some(argument) = instance.arguments.get(index)
        {
            substitutions.insert(parameter.name.text.clone(), argument.clone());
        }
    }
    let ports = instance
        .template
        .declaration
        .members
        .iter()
        .filter_map(|member| match &member.kind {
            uhura_syntax::ast::PartMemberKind::Port(port) => Some((
                port.name.text.clone(),
                format!("{}.{}", instance.name, port.name.text),
            )),
            _ => None,
        })
        .collect();
    RewriteContext {
        owner: Some(instance.name.clone()),
        bindings: instance.bindings.clone(),
        owned: instance.owned.clone(),
        ports,
        substitutions,
        handles: instance.handles.clone(),
        instances: interfaces.clone(),
        lexical: BTreeMap::new(),
        local_prefix: None,
    }
}

fn compose_machine(
    machine: &mut uhura_syntax::ast::MachineDeclaration,
    machine_bindings: &BTreeMap<String, String>,
    plan: MachinePlan,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if plan.instances.is_empty() {
        return;
    }
    let root_outcomes = root_outcomes(machine, machine_bindings);
    let mut root_context = RewriteContext {
        bindings: machine_bindings.clone(),
        instances: plan.interfaces.clone(),
        ..RewriteContext::default()
    };

    let original = std::mem::take(&mut machine.members);
    let mut config = None;
    let mut events = None;
    let mut commands = None;
    let mut outcomes = None;
    let mut state = None;
    let mut observe = None;
    let mut root_other = Vec::new();
    for mut member in original {
        match member.kind {
            uhura_syntax::ast::MachineMemberKind::Part(_) => {}
            uhura_syntax::ast::MachineMemberKind::Config(value) => {
                config = Some((value, member.span))
            }
            uhura_syntax::ast::MachineMemberKind::Events(value) => {
                events = Some((value, member.span))
            }
            uhura_syntax::ast::MachineMemberKind::Commands(value) => {
                commands = Some((value, member.span))
            }
            uhura_syntax::ast::MachineMemberKind::Outcomes(value) => {
                outcomes = Some((value, member.span))
            }
            uhura_syntax::ast::MachineMemberKind::State(value) => {
                state = Some((value, member.span))
            }
            uhura_syntax::ast::MachineMemberKind::Observe(value) => {
                observe = Some((value, member.span))
            }
            _ => {
                rewrite_machine_member(
                    &mut member,
                    &mut root_context,
                    &plan.unit_updates,
                    diagnostics,
                );
                root_other.push(member);
            }
        }
    }

    let synthetic = machine.name.span;
    let mut events = events.unwrap_or((
        uhura_syntax::ast::ProtocolSection {
            variants: Vec::new(),
        },
        synthetic,
    ));
    let mut commands = commands.unwrap_or((
        uhura_syntax::ast::ProtocolSection {
            variants: Vec::new(),
        },
        synthetic,
    ));
    let mut state = state.unwrap_or((
        uhura_syntax::ast::StateSection { fields: Vec::new() },
        synthetic,
    ));
    let mut observe = observe.unwrap_or((
        uhura_syntax::ast::ObserveSection { fields: Vec::new() },
        synthetic,
    ));
    for field in &mut observe.0.fields {
        if let Some(expression) = &mut field.value {
            rewrite_expression(expression, &mut root_context, diagnostics);
        }
    }
    let mut part_other = Vec::new();

    for instance in &plan.instances {
        validate_required_outcomes(instance, &root_outcomes, diagnostics);
        validate_part_initializers(instance, diagnostics);
        let mut context = context_for_instance(instance, &plan.interfaces);
        let mut aggregate = AggregateSections {
            events: &mut events.0,
            commands: &mut commands.0,
            state: &mut state.0,
            observe: &mut observe.0,
            other: &mut part_other,
        };
        append_part_members(
            instance,
            &mut context,
            &plan.unit_updates,
            &mut aggregate,
            diagnostics,
        );
    }

    let mut rebuilt = Vec::new();
    if let Some((value, span)) = config {
        rebuilt.push(uhura_syntax::ast::Node::new(
            uhura_syntax::ast::MachineMemberKind::Config(value),
            span,
        ));
    }
    rebuilt.push(uhura_syntax::ast::Node::new(
        uhura_syntax::ast::MachineMemberKind::Events(events.0),
        events.1,
    ));
    rebuilt.push(uhura_syntax::ast::Node::new(
        uhura_syntax::ast::MachineMemberKind::Commands(commands.0),
        commands.1,
    ));
    if let Some((value, span)) = outcomes {
        rebuilt.push(uhura_syntax::ast::Node::new(
            uhura_syntax::ast::MachineMemberKind::Outcomes(value),
            span,
        ));
    }
    rebuilt.push(uhura_syntax::ast::Node::new(
        uhura_syntax::ast::MachineMemberKind::State(state.0),
        state.1,
    ));
    rebuilt.push(uhura_syntax::ast::Node::new(
        uhura_syntax::ast::MachineMemberKind::Observe(observe.0),
        observe.1,
    ));
    rebuilt.extend(root_other);
    rebuilt.extend(part_other);
    machine.members = rebuilt;
}

fn append_part_members(
    instance: &InstancePlan,
    context: &mut RewriteContext,
    unit_updates: &BTreeMap<String, UnitUpdate>,
    aggregate: &mut AggregateSections<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for parameter in &instance.parameters {
        let ParameterKind::Configuration(parameter) = parameter else {
            continue;
        };
        let Some(argument) = context.substitutions.get(&parameter.name.text).cloned() else {
            continue;
        };
        let mut parameter = parameter.clone();
        rewrite_type(&mut parameter.ty, context);
        aggregate.other.extend(configuration_binding_members(
            &instance.name,
            &parameter,
            argument,
        ));
    }

    for member in &instance.template.declaration.members {
        let mut machine_member = match &member.kind {
            uhura_syntax::ast::PartMemberKind::Require(value) => {
                uhura_syntax::ast::MachineMemberKind::Require(value.clone())
            }
            uhura_syntax::ast::PartMemberKind::RequiresOutcomes(_) => continue,
            uhura_syntax::ast::PartMemberKind::Const(value) => {
                uhura_syntax::ast::MachineMemberKind::Const(value.clone())
            }
            uhura_syntax::ast::PartMemberKind::Function(value) => {
                uhura_syntax::ast::MachineMemberKind::Function(value.clone())
            }
            uhura_syntax::ast::PartMemberKind::Events(value) => {
                let mut value = value.clone();
                for variant in &mut value.variants {
                    for parameter in &mut variant.parameters {
                        rewrite_type(&mut parameter.ty, context);
                    }
                    variant.name.text = format!("{}.{}", instance.name, variant.name.text);
                }
                aggregate.events.variants.extend(value.variants);
                continue;
            }
            uhura_syntax::ast::PartMemberKind::Commands(value) => {
                let mut value = value.clone();
                for variant in &mut value.variants {
                    for parameter in &mut variant.parameters {
                        rewrite_type(&mut parameter.ty, context);
                    }
                    variant.name.text = format!("{}.{}", instance.name, variant.name.text);
                }
                aggregate.commands.variants.extend(value.variants);
                continue;
            }
            uhura_syntax::ast::PartMemberKind::Port(value) => {
                let mut value = value.clone();
                rewrite_type_path(&mut value.contract, context);
                for field in &mut value.fields {
                    let mut expression = field.value.take().unwrap_or_else(|| {
                        name_expression(field.name.text.clone(), field.name.span)
                    });
                    rewrite_expression(&mut expression, context, diagnostics);
                    field.value = Some(expression);
                }
                value.name.text = format!("{}.{}", instance.name, value.name.text);
                aggregate.other.push(uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::MachineMemberKind::Port(value),
                    member.span,
                ));
                continue;
            }
            uhura_syntax::ast::PartMemberKind::State(value) => {
                let mut value = value.clone();
                for field in &mut value.fields {
                    rewrite_type(&mut field.ty, context);
                    rewrite_expression(&mut field.initial, context, diagnostics);
                    field.name.text = format!("{}.{}", instance.name, field.name.text);
                }
                aggregate.state.fields.extend(value.fields);
                continue;
            }
            uhura_syntax::ast::PartMemberKind::Computed(value) => {
                if value.visibility == uhura_syntax::ast::Visibility::Public && value.ty.is_none() {
                    composition_error(
                        diagnostics,
                        codes::TYPE_MISMATCH,
                        "uhura-0.4/public-computed-type-required",
                        format!(
                            "public part computed `{}` requires an explicit type because it defines the nominal `Reads` interface",
                            value.name.text
                        ),
                        member.span,
                    );
                    continue;
                }
                uhura_syntax::ast::MachineMemberKind::Computed(value.clone())
            }
            uhura_syntax::ast::PartMemberKind::Invariant(value) => {
                uhura_syntax::ast::MachineMemberKind::Invariant(value.clone())
            }
            uhura_syntax::ast::PartMemberKind::Observe(value) => {
                let mut value = value.clone();
                for field in &mut value.fields {
                    if let Some(expression) = &mut field.value {
                        rewrite_expression(expression, context, diagnostics);
                    } else if let Some(name) = context.owned.get(&field.name.text) {
                        field.value = Some(name_expression(name.clone(), field.name.span));
                    }
                    field.name.text = format!("{}.{}", instance.name, field.name.text);
                }
                aggregate.observe.fields.extend(value.fields);
                continue;
            }
            uhura_syntax::ast::PartMemberKind::Handler(value) => {
                let mut value = value.clone();
                if let Some(owner) = &mut value.input.owner {
                    if let Some(port) = context.ports.get(&owner.text) {
                        owner.text.clone_from(port);
                    } else {
                        composition_error(
                            diagnostics,
                            codes::UNKNOWN_NAME,
                            "uhura-0.4/unknown-part-port",
                            format!(
                                "qualified handler owner `{}` is not a port declared by part `{}`",
                                owner.text, instance.name
                            ),
                            value.input.span,
                        );
                    }
                } else {
                    value.input.variant.text =
                        format!("{}.{}", instance.name, value.input.variant.text);
                }
                let mut child = context.clone();
                for pattern in &mut value.parameters {
                    register_pattern_bindings(pattern, &mut child, None);
                    rewrite_pattern(pattern, &mut child);
                }
                rewrite_block(&mut value.body, &mut child, unit_updates, diagnostics);
                aggregate.other.push(uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::MachineMemberKind::Handler(value),
                    member.span,
                ));
                continue;
            }
            uhura_syntax::ast::PartMemberKind::Update(value) => {
                if value.visibility == uhura_syntax::ast::Visibility::Public
                    && update_mentions_outcome(value)
                {
                    continue;
                }
                uhura_syntax::ast::MachineMemberKind::Update(value.clone())
            }
        };
        let mut node = uhura_syntax::ast::Node::new(machine_member, member.span);
        rewrite_machine_member(&mut node, context, unit_updates, diagnostics);
        machine_member = node.kind;
        aggregate
            .other
            .push(uhura_syntax::ast::Node::new(machine_member, node.span));
    }
}

fn configuration_binding_members(
    owner: &str,
    parameter: &uhura_syntax::ast::Parameter,
    argument: uhura_syntax::ast::Expression,
) -> Vec<uhura_syntax::ast::MachineMember> {
    let function_name = format!("{owner}.config.{}", parameter.name.text);
    let function = uhura_syntax::ast::FunctionDeclaration {
        visibility: uhura_syntax::ast::Visibility::Private,
        name: uhura_syntax::ast::Identifier::new(function_name.clone(), parameter.name.span),
        parameters: vec![uhura_syntax::ast::Parameter {
            name: uhura_syntax::ast::Identifier::new("value", parameter.name.span),
            ty: parameter.ty.clone(),
            span: parameter.span,
        }],
        result: type_name("Bool", parameter.ty.span),
        body: uhura_syntax::ast::Block {
            statements: Vec::new(),
            tail: Some(Box::new(bool_expression(true, parameter.name.span))),
            span: parameter.span,
        },
    };
    let requirement = uhura_syntax::ast::RequireDeclaration {
        condition: call_expression(function_name, vec![argument], parameter.span),
        semicolon: parameter.name.span,
    };
    vec![
        uhura_syntax::ast::Node::new(
            uhura_syntax::ast::MachineMemberKind::Function(function),
            parameter.span,
        ),
        uhura_syntax::ast::Node::new(
            uhura_syntax::ast::MachineMemberKind::Require(requirement),
            parameter.span,
        ),
    ]
}

fn rewrite_machine_member(
    member: &mut uhura_syntax::ast::MachineMember,
    context: &mut RewriteContext,
    unit_updates: &BTreeMap<String, UnitUpdate>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &mut member.kind {
        uhura_syntax::ast::MachineMemberKind::Require(value) => {
            rewrite_expression(&mut value.condition, context, diagnostics);
        }
        uhura_syntax::ast::MachineMemberKind::Const(value) => {
            rewrite_owned_identifier(&mut value.name, context);
            rewrite_type(&mut value.ty, context);
            rewrite_expression(&mut value.value, context, diagnostics);
        }
        uhura_syntax::ast::MachineMemberKind::Function(value) => {
            rewrite_owned_identifier(&mut value.name, context);
            let mut child = context.clone();
            for parameter in &mut value.parameters {
                rewrite_type(&mut parameter.ty, context);
                child
                    .lexical
                    .insert(parameter.name.text.clone(), parameter.name.text.clone());
            }
            rewrite_type(&mut value.result, context);
            rewrite_block(&mut value.body, &mut child, &BTreeMap::new(), diagnostics);
        }
        uhura_syntax::ast::MachineMemberKind::Computed(value) => {
            rewrite_owned_identifier(&mut value.name, context);
            if let Some(ty) = &mut value.ty {
                rewrite_type(ty, context);
            }
            rewrite_expression(&mut value.value, context, diagnostics);
        }
        uhura_syntax::ast::MachineMemberKind::Invariant(value) => {
            for condition in &mut value.conditions {
                rewrite_expression(condition, context, diagnostics);
            }
        }
        uhura_syntax::ast::MachineMemberKind::Handler(value) => {
            rewrite_block(&mut value.body, context, unit_updates, diagnostics);
        }
        uhura_syntax::ast::MachineMemberKind::Update(value) => {
            rewrite_owned_identifier(&mut value.name, context);
            let mut child = context.clone();
            for parameter in &mut value.parameters {
                rewrite_type(&mut parameter.ty, context);
                child
                    .lexical
                    .insert(parameter.name.text.clone(), parameter.name.text.clone());
            }
            if let Some(result) = &mut value.result {
                rewrite_type(result, context);
            }
            rewrite_block(&mut value.body, &mut child, unit_updates, diagnostics);
        }
        uhura_syntax::ast::MachineMemberKind::BeforeCommit(value) => {
            rewrite_block(&mut value.body, context, unit_updates, diagnostics);
        }
        uhura_syntax::ast::MachineMemberKind::Config(_)
        | uhura_syntax::ast::MachineMemberKind::Part(_)
        | uhura_syntax::ast::MachineMemberKind::Events(_)
        | uhura_syntax::ast::MachineMemberKind::Commands(_)
        | uhura_syntax::ast::MachineMemberKind::Port(_)
        | uhura_syntax::ast::MachineMemberKind::Outcomes(_)
        | uhura_syntax::ast::MachineMemberKind::State(_)
        | uhura_syntax::ast::MachineMemberKind::Observe(_) => {}
    }
}

fn rewrite_block(
    block: &mut uhura_syntax::ast::Block,
    context: &mut RewriteContext,
    unit_updates: &BTreeMap<String, UnitUpdate>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let original = std::mem::take(&mut block.statements);
    let mut output = Vec::new();
    for mut statement in original {
        if let uhura_syntax::ast::StatementKind::Expression { expression, .. } = &mut statement.kind
            && let Some((target, arguments)) = update_call(expression, context, diagnostics)
            && let Some(update) = unit_updates.get(&target).cloned()
        {
            if arguments.len() != update.parameters.len() {
                composition_error(
                    diagnostics,
                    codes::ARITY,
                    "uhura-0.4/update-argument-arity",
                    format!(
                        "update `{target}` expects {} arguments, got {}",
                        update.parameters.len(),
                        arguments.len()
                    ),
                    expression.span,
                );
                continue;
            }
            let mut update_context = update.context.clone();
            let prefix = format!(
                "__uhura_{}_{}_",
                update.owner.replace('.', "_"),
                update.name
            );
            update_context.local_prefix = Some(prefix.clone());
            let mut prefix_statements = Vec::new();
            for (parameter, mut argument) in update.parameters.iter().zip(arguments) {
                rewrite_expression(&mut argument, context, diagnostics);
                let lowered = format!("{prefix}arg_{}", parameter.name.text);
                update_context
                    .lexical
                    .insert(parameter.name.text.clone(), lowered.clone());
                let mut parameter_type = parameter.ty.clone();
                rewrite_type(&mut parameter_type, &update.context);
                prefix_statements.push(uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::StatementKind::Let {
                        name: uhura_syntax::ast::Identifier::new(lowered, parameter.name.span),
                        ty: Some(parameter_type),
                        value: argument,
                        semicolon: parameter.name.span,
                    },
                    parameter.span,
                ));
            }
            let mut body = update.body.clone();
            if let Some(tail) = &body.tail
                && !matches!(tail.kind, uhura_syntax::ast::ExpressionKind::Unit)
            {
                composition_error(
                    diagnostics,
                    codes::TYPE_MISMATCH,
                    "uhura-0.4/unit-update-tail",
                    format!(
                        "unit update `{}.{}` cannot produce a non-Unit tail value",
                        update.owner, update.name
                    ),
                    tail.span,
                );
            }
            body.tail = None;
            rewrite_block(&mut body, &mut update_context, unit_updates, diagnostics);
            output.extend(prefix_statements);
            output.extend(body.statements);
            continue;
        }
        rewrite_statement(&mut statement, context, unit_updates, diagnostics);
        if let uhura_syntax::ast::StatementKind::Let { name, .. } = &mut statement.kind {
            let original = name.text.clone();
            if let Some(prefix) = &context.local_prefix {
                name.text = format!("{prefix}local_{original}");
            }
            context.lexical.insert(original, name.text.clone());
        }
        output.push(statement);
    }
    if let Some(tail) = &mut block.tail {
        rewrite_reaction_expression(tail, context, unit_updates, diagnostics);
    }
    block.statements = output;
}

fn rewrite_statement(
    statement: &mut uhura_syntax::ast::Statement,
    context: &mut RewriteContext,
    unit_updates: &BTreeMap<String, UnitUpdate>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &mut statement.kind {
        uhura_syntax::ast::StatementKind::Let { ty, value, .. } => {
            if let Some(ty) = ty {
                rewrite_type(ty, context);
            }
            rewrite_expression(value, context, diagnostics);
        }
        uhura_syntax::ast::StatementKind::Assign { target, value, .. } => {
            rewrite_owned_identifier(target, context);
            rewrite_expression(value, context, diagnostics);
        }
        uhura_syntax::ast::StatementKind::Emit { output, .. } => {
            if let Some(owner) = &mut output.selector.owner {
                if let Some(port) = context.ports.get(&owner.text) {
                    owner.text.clone_from(port);
                } else if context.owner.is_some() {
                    composition_error(
                        diagnostics,
                        codes::UNKNOWN_NAME,
                        "uhura-0.4/unknown-part-port",
                        format!(
                            "qualified emission owner `{}` is not a port declared by part `{}`",
                            owner.text,
                            context.owner.as_deref().unwrap_or("<root>")
                        ),
                        output.selector.span,
                    );
                }
            } else if !output.selector.variant.text.contains('.')
                && let Some(owner) = &context.owner
            {
                output.selector.variant.text = format!("{owner}.{}", output.selector.variant.text);
            }
            for argument in &mut output.arguments {
                rewrite_expression(argument, context, diagnostics);
            }
        }
        uhura_syntax::ast::StatementKind::While {
            condition,
            decreases,
            body,
        } => {
            rewrite_expression(condition, context, diagnostics);
            rewrite_expression(decreases, context, diagnostics);
            let mut child = context.clone();
            rewrite_block(body, &mut child, unit_updates, diagnostics);
        }
        uhura_syntax::ast::StatementKind::Expression { expression, .. }
        | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
            rewrite_reaction_expression(expression, context, unit_updates, diagnostics);
        }
        uhura_syntax::ast::StatementKind::Unreachable { .. } => {}
    }
}

fn rewrite_reaction_expression(
    expression: &mut uhura_syntax::ast::Expression,
    context: &mut RewriteContext,
    unit_updates: &BTreeMap<String, UnitUpdate>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &mut expression.kind {
        uhura_syntax::ast::ExpressionKind::Block(block) => {
            let mut child = context.clone();
            rewrite_block(block, &mut child, unit_updates, diagnostics);
        }
        uhura_syntax::ast::ExpressionKind::If(value) => {
            rewrite_expression(&mut value.condition, context, diagnostics);
            let mut then_context = context.clone();
            rewrite_block(
                &mut value.then_branch,
                &mut then_context,
                unit_updates,
                diagnostics,
            );
            if let Some(branch) = &mut value.else_branch {
                match branch {
                    uhura_syntax::ast::ElseBranch::Block(block) => {
                        let mut child = context.clone();
                        rewrite_block(block, &mut child, unit_updates, diagnostics);
                    }
                    uhura_syntax::ast::ElseBranch::If(value) => {
                        rewrite_reaction_expression(value, context, unit_updates, diagnostics);
                    }
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Match(value) => {
            rewrite_expression(&mut value.value, context, diagnostics);
            for arm in &mut value.arms {
                let mut child = context.clone();
                let prefix = child.local_prefix.clone();
                register_pattern_bindings(&arm.pattern, &mut child, prefix);
                rewrite_pattern(&mut arm.pattern, &mut child);
                rewrite_reaction_expression(&mut arm.value, &mut child, unit_updates, diagnostics);
            }
        }
        _ => rewrite_expression(expression, context, diagnostics),
    }
}

fn rewrite_expression(
    expression: &mut uhura_syntax::ast::Expression,
    context: &mut RewriteContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let uhura_syntax::ast::ExpressionKind::Name(name) = &expression.kind
        && name.segments.len() == 1
        && let Some(replacement) = context.substitutions.get(&name.segments[0].text)
    {
        *expression = replacement.clone();
        return;
    }
    match &mut expression.kind {
        uhura_syntax::ast::ExpressionKind::Literal(_) | uhura_syntax::ast::ExpressionKind::Unit => {
        }
        uhura_syntax::ast::ExpressionKind::Sequence(values)
        | uhura_syntax::ast::ExpressionKind::Tuple(values) => {
            for value in values {
                rewrite_expression(value, context, diagnostics);
            }
        }
        uhura_syntax::ast::ExpressionKind::Group(value)
        | uhura_syntax::ast::ExpressionKind::Unary { value, .. } => {
            rewrite_expression(value, context, diagnostics);
        }
        uhura_syntax::ast::ExpressionKind::Name(name) => rewrite_qualified_name(name, context),
        uhura_syntax::ast::ExpressionKind::Record(value) => {
            rewrite_qualified_name(&mut value.constructor, context);
            for field in &mut value.fields {
                if let Some(value) = &mut field.value {
                    rewrite_expression(value, context, diagnostics);
                }
            }
            if let Some(base) = &mut value.base {
                rewrite_expression(base, context, diagnostics);
            }
        }
        uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => {
            for entry in entries {
                rewrite_expression(&mut entry.key, context, diagnostics);
                rewrite_expression(&mut entry.value, context, diagnostics);
            }
        }
        uhura_syntax::ast::ExpressionKind::Block(value) => {
            rewrite_block(value, context, &BTreeMap::new(), diagnostics);
        }
        uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
            if let Some(target) = flattened_update_target(callee, context, diagnostics) {
                **callee = name_expression(target, callee.span);
            } else {
                rewrite_expression(callee, context, diagnostics);
            }
            for argument in arguments {
                if let uhura_syntax::ast::CallArgument::Expression(value) = argument {
                    rewrite_expression(value, context, diagnostics);
                } else if let uhura_syntax::ast::CallArgument::Binder(value) = argument {
                    let mut child = context.clone();
                    child
                        .lexical
                        .insert(value.parameter.text.clone(), value.parameter.text.clone());
                    rewrite_expression(&mut value.body, &mut child, diagnostics);
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Member { value, member } => {
            if let Some(target) = flattened_read_target(value, member, context, diagnostics) {
                *expression = name_expression(target, expression.span);
            } else {
                rewrite_expression(value, context, diagnostics);
            }
        }
        uhura_syntax::ast::ExpressionKind::Index { value, index } => {
            rewrite_expression(value, context, diagnostics);
            rewrite_expression(index, context, diagnostics);
        }
        uhura_syntax::ast::ExpressionKind::Binary { left, right, .. }
        | uhura_syntax::ast::ExpressionKind::Compare { left, right, .. } => {
            rewrite_expression(left, context, diagnostics);
            rewrite_expression(right, context, diagnostics);
        }
        uhura_syntax::ast::ExpressionKind::Is { value, pattern } => {
            rewrite_expression(value, context, diagnostics);
            rewrite_pattern(pattern, context);
        }
        uhura_syntax::ast::ExpressionKind::If(value) => {
            rewrite_expression(&mut value.condition, context, diagnostics);
            let mut then_context = context.clone();
            rewrite_block(
                &mut value.then_branch,
                &mut then_context,
                &BTreeMap::new(),
                diagnostics,
            );
            if let Some(branch) = &mut value.else_branch {
                match branch {
                    uhura_syntax::ast::ElseBranch::Block(block) => {
                        let mut child = context.clone();
                        rewrite_block(block, &mut child, &BTreeMap::new(), diagnostics);
                    }
                    uhura_syntax::ast::ElseBranch::If(value) => {
                        rewrite_expression(value, context, diagnostics);
                    }
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Match(value) => {
            rewrite_expression(&mut value.value, context, diagnostics);
            for arm in &mut value.arms {
                let mut child = context.clone();
                let prefix = child.local_prefix.clone();
                register_pattern_bindings(&arm.pattern, &mut child, prefix);
                rewrite_pattern(&mut arm.pattern, &mut child);
                rewrite_expression(&mut arm.value, &mut child, diagnostics);
            }
        }
        uhura_syntax::ast::ExpressionKind::Return(value) => {
            if let Some(value) = value {
                rewrite_expression(value, context, diagnostics);
            }
        }
    }
}

fn flattened_read_target(
    value: &uhura_syntax::ast::Expression,
    member: &uhura_syntax::ast::Identifier,
    context: &RewriteContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    if let uhura_syntax::ast::ExpressionKind::Name(name) = &value.kind
        && name.segments.len() == 1
        && let Some(handle) = context.handles.get(&name.segments[0].text)
    {
        if handle.kind != CapabilityKind::Reads || !handle.members.contains(&member.text) {
            composition_error(
                diagnostics,
                codes::UNKNOWN_NAME,
                "uhura-0.4/undeclared-part-read",
                format!(
                    "`{}.{}` is not present in the declared `Reads` capability",
                    name.segments[0].text, member.text
                ),
                member.span,
            );
            return None;
        }
        return Some(format!("{}.{}", handle.provider, member.text));
    }
    if let Some((instance, capability)) = nested_capability(value)
        && capability == CapabilityKind::Reads
        && let Some(interface) = context.instances.get(instance)
    {
        if !interface.reads.contains(&member.text) {
            composition_error(
                diagnostics,
                codes::UNKNOWN_NAME,
                "uhura-0.4/private-part-read",
                format!(
                    "`{instance}.reads` does not expose computed member `{}`",
                    member.text
                ),
                member.span,
            );
            return None;
        }
        return Some(format!("{instance}.{}", member.text));
    }
    None
}

fn flattened_update_target(
    callee: &uhura_syntax::ast::Expression,
    context: &RewriteContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let uhura_syntax::ast::ExpressionKind::Member { value, member } = &callee.kind else {
        return None;
    };
    if let uhura_syntax::ast::ExpressionKind::Name(name) = &value.kind
        && name.segments.len() == 1
        && let Some(handle) = context.handles.get(&name.segments[0].text)
    {
        if handle.kind != CapabilityKind::Updates || !handle.members.contains(&member.text) {
            composition_error(
                diagnostics,
                codes::UNKNOWN_NAME,
                "uhura-0.4/undeclared-part-update",
                format!(
                    "`{}.{}` is not present in the declared `Updates` capability",
                    name.segments[0].text, member.text
                ),
                member.span,
            );
            return None;
        }
        return Some(format!("{}.{}", handle.provider, member.text));
    }
    if let Some((instance, capability)) = nested_capability(value)
        && capability == CapabilityKind::Updates
        && let Some(interface) = context.instances.get(instance)
    {
        if !interface.updates.contains(&member.text) {
            composition_error(
                diagnostics,
                codes::UNKNOWN_NAME,
                "uhura-0.4/private-part-update",
                format!(
                    "`{instance}.updates` does not expose update `{}`",
                    member.text
                ),
                member.span,
            );
            return None;
        }
        return Some(format!("{instance}.{}", member.text));
    }
    if let uhura_syntax::ast::ExpressionKind::Name(name) = &value.kind
        && name.segments.len() == 1
        && let Some(target) = context.owned.get(&member.text)
        && name.segments[0].text
            == target
                .split_once('.')
                .map(|(owner, _)| owner)
                .unwrap_or_default()
    {
        return Some(target.clone());
    }
    if let uhura_syntax::ast::ExpressionKind::Name(name) = &callee.kind
        && name.segments.len() == 1
        && let Some(target) = context.owned.get(&name.segments[0].text)
    {
        return Some(target.clone());
    }
    None
}

fn update_call(
    expression: &uhura_syntax::ast::Expression,
    context: &RewriteContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(String, Vec<uhura_syntax::ast::Expression>)> {
    let uhura_syntax::ast::ExpressionKind::Call { callee, arguments } = &expression.kind else {
        return None;
    };
    let target = flattened_update_target(callee, context, diagnostics).or_else(|| {
        let uhura_syntax::ast::ExpressionKind::Name(name) = &callee.kind else {
            return None;
        };
        (name.segments.len() == 1)
            .then(|| context.owned.get(&name.segments[0].text).cloned())
            .flatten()
    })?;
    let values = arguments
        .iter()
        .filter_map(|argument| match argument {
            uhura_syntax::ast::CallArgument::Expression(value) => Some(value.clone()),
            uhura_syntax::ast::CallArgument::Binder(_) => None,
        })
        .collect::<Vec<_>>();
    Some((target, values))
}

fn nested_capability(expression: &uhura_syntax::ast::Expression) -> Option<(&str, CapabilityKind)> {
    let uhura_syntax::ast::ExpressionKind::Member { value, member } = &expression.kind else {
        return None;
    };
    let uhura_syntax::ast::ExpressionKind::Name(name) = &value.kind else {
        return None;
    };
    if name.segments.len() != 1 {
        return None;
    }
    let kind = match member.text.as_str() {
        "reads" => CapabilityKind::Reads,
        "updates" => CapabilityKind::Updates,
        _ => return None,
    };
    Some((name.segments[0].text.as_str(), kind))
}

fn rewrite_owned_identifier(
    identifier: &mut uhura_syntax::ast::Identifier,
    context: &RewriteContext,
) {
    if let Some(value) = context.owned.get(&identifier.text) {
        identifier.text.clone_from(value);
    }
}

fn rewrite_qualified_name(name: &mut uhura_syntax::ast::QualifiedName, context: &RewriteContext) {
    let singular = name.segments.len() == 1;
    let Some(first) = name.segments.first_mut() else {
        return;
    };
    if singular {
        if let Some(value) = context.lexical.get(&first.text) {
            first.text.clone_from(value);
            return;
        }
        if let Some(value) = context.owned.get(&first.text) {
            first.text.clone_from(value);
            return;
        }
    }
    if let Some(value) = context.bindings.get(&first.text) {
        first.text.clone_from(value);
    }
}

fn rewrite_type(ty: &mut uhura_syntax::ast::TypeExpression, context: &RewriteContext) {
    match &mut ty.kind {
        uhura_syntax::ast::TypeExpressionKind::Path(path) => rewrite_type_path(path, context),
        uhura_syntax::ast::TypeExpressionKind::Tuple(values) => {
            for value in values {
                rewrite_type(value, context);
            }
        }
        uhura_syntax::ast::TypeExpressionKind::Unit => {}
    }
}

fn rewrite_type_path(path: &mut uhura_syntax::ast::TypePath, context: &RewriteContext) {
    if let Some(first) = path.segments.first_mut()
        && let Some(value) = context.bindings.get(&first.name.text)
    {
        first.name.text.clone_from(value);
    }
    for segment in &mut path.segments {
        for argument in &mut segment.arguments {
            rewrite_type(argument, context);
        }
    }
}

fn rewrite_pattern(pattern: &mut uhura_syntax::ast::Pattern, context: &mut RewriteContext) {
    match &mut pattern.kind {
        uhura_syntax::ast::PatternKind::Binder(value) => {
            if let Some(lowered) = context.lexical.get(&value.text) {
                value.text.clone_from(lowered);
            }
        }
        uhura_syntax::ast::PatternKind::Group(value) => rewrite_pattern(value, context),
        uhura_syntax::ast::PatternKind::Tuple(values)
        | uhura_syntax::ast::PatternKind::Alternative(values) => {
            for value in values {
                rewrite_pattern(value, context);
            }
        }
        uhura_syntax::ast::PatternKind::Constructor(name) => rewrite_qualified_name(name, context),
        uhura_syntax::ast::PatternKind::TupleConstructor {
            constructor,
            arguments,
        } => {
            rewrite_qualified_name(constructor, context);
            for argument in arguments {
                rewrite_pattern(argument, context);
            }
        }
        uhura_syntax::ast::PatternKind::Record {
            constructor,
            fields,
            ..
        } => {
            rewrite_qualified_name(constructor, context);
            for field in fields {
                if let Some(value) = &mut field.pattern {
                    rewrite_pattern(value, context);
                } else if let Some(lowered) = context.lexical.get(&field.name.text) {
                    field.pattern = Some(uhura_syntax::ast::Node::new(
                        uhura_syntax::ast::PatternKind::Binder(uhura_syntax::ast::Identifier::new(
                            lowered.clone(),
                            field.name.span,
                        )),
                        field.span,
                    ));
                }
            }
        }
        uhura_syntax::ast::PatternKind::AnonymousRecord { fields, .. } => {
            for field in fields {
                if let Some(value) = &mut field.pattern {
                    rewrite_pattern(value, context);
                } else if let Some(lowered) = context.lexical.get(&field.name.text) {
                    field.pattern = Some(uhura_syntax::ast::Node::new(
                        uhura_syntax::ast::PatternKind::Binder(uhura_syntax::ast::Identifier::new(
                            lowered.clone(),
                            field.name.span,
                        )),
                        field.span,
                    ));
                }
            }
        }
        uhura_syntax::ast::PatternKind::Wildcard | uhura_syntax::ast::PatternKind::Literal(_) => {}
    }
}

fn register_pattern_bindings(
    pattern: &uhura_syntax::ast::Pattern,
    context: &mut RewriteContext,
    prefix: Option<String>,
) {
    let mut names = BTreeSet::new();
    collect_pattern_bindings(pattern, &mut names);
    for name in names {
        let lowered = prefix
            .as_ref()
            .map_or_else(|| name.clone(), |prefix| format!("{prefix}bind_{name}"));
        context.lexical.insert(name, lowered);
    }
}

fn collect_pattern_bindings(pattern: &uhura_syntax::ast::Pattern, names: &mut BTreeSet<String>) {
    match &pattern.kind {
        uhura_syntax::ast::PatternKind::Binder(value) => {
            names.insert(value.text.clone());
        }
        uhura_syntax::ast::PatternKind::Group(value) => collect_pattern_bindings(value, names),
        uhura_syntax::ast::PatternKind::Tuple(values)
        | uhura_syntax::ast::PatternKind::Alternative(values) => {
            for value in values {
                collect_pattern_bindings(value, names);
            }
        }
        uhura_syntax::ast::PatternKind::TupleConstructor { arguments, .. } => {
            for argument in arguments {
                collect_pattern_bindings(argument, names);
            }
        }
        uhura_syntax::ast::PatternKind::Record { fields, .. }
        | uhura_syntax::ast::PatternKind::AnonymousRecord { fields, .. } => {
            for field in fields {
                if let Some(value) = &field.pattern {
                    collect_pattern_bindings(value, names);
                } else {
                    names.insert(field.name.text.clone());
                }
            }
        }
        uhura_syntax::ast::PatternKind::Wildcard
        | uhura_syntax::ast::PatternKind::Literal(_)
        | uhura_syntax::ast::PatternKind::Constructor(_) => {}
    }
}

fn validate_required_outcomes(
    instance: &InstancePlan,
    root: &BTreeMap<String, OutcomeSignature>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let requirements = instance
        .template
        .declaration
        .members
        .iter()
        .filter_map(|member| match &member.kind {
            uhura_syntax::ast::PartMemberKind::RequiresOutcomes(value) => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();
    if requirements.len() > 1 {
        composition_error(
            diagnostics,
            codes::DUPLICATE,
            "uhura-0.4/duplicate-requires-outcomes",
            format!(
                "part `{}` declares `requires outcomes` more than once",
                instance.template.declaration.name.text
            ),
            instance.span,
        );
    }
    let required = requirements.first();
    let requires_outcome_family =
        instance
            .template
            .declaration
            .members
            .iter()
            .any(|member| match &member.kind {
                uhura_syntax::ast::PartMemberKind::Handler(_) => true,
                uhura_syntax::ast::PartMemberKind::Update(value) => value
                    .result
                    .as_ref()
                    .is_some_and(|result| type_mentions(result, "Outcome")),
                _ => false,
            });
    if requires_outcome_family && required.is_none() {
        composition_error(
            diagnostics,
            codes::OUTCOME,
            "uhura-0.4/missing-requires-outcomes",
            format!(
                "part `{}` has handlers and must declare its exact enclosing outcome requirements",
                instance.template.declaration.name.text
            ),
            instance.span,
        );
        return;
    }
    let mut declared = BTreeSet::new();
    if let Some(required) = required {
        for entry in &required.entries {
            if !declared.insert(entry.variant.name.text.clone()) {
                composition_error(
                    diagnostics,
                    codes::DUPLICATE,
                    "uhura-0.4/duplicate-required-outcome",
                    format!(
                        "part `{}` requires outcome `{}` more than once",
                        instance.template.declaration.name.text, entry.variant.name.text
                    ),
                    entry.span,
                );
                continue;
            }
            let actual = outcome_signature(entry, &instance.bindings);
            match root.get(&entry.variant.name.text) {
                Some(expected) if expected == &actual => {}
                Some(_) => composition_error(
                    diagnostics,
                    codes::OUTCOME,
                    "uhura-0.4/outcome-requirement-mismatch",
                    format!(
                        "part `{}` requires outcome `{}` with a policy or payload that does not exactly match the enclosing machine",
                        instance.template.declaration.name.text, entry.variant.name.text
                    ),
                    entry.span,
                ),
                None => composition_error(
                    diagnostics,
                    codes::OUTCOME,
                    "uhura-0.4/unsatisfied-outcome-requirement",
                    format!(
                        "enclosing machine does not declare required outcome `{}` for part `{}`",
                        entry.variant.name.text, instance.template.declaration.name.text
                    ),
                    entry.span,
                ),
            }
        }
    }
    for member in &instance.template.declaration.members {
        match &member.kind {
            uhura_syntax::ast::PartMemberKind::Handler(value) => {
                reject_undeclared_outcomes_in_block(
                    &value.body,
                    root,
                    &declared,
                    &instance.name,
                    diagnostics,
                );
            }
            uhura_syntax::ast::PartMemberKind::Update(value)
                if !is_unit_update(value)
                    && value.visibility == uhura_syntax::ast::Visibility::Private =>
            {
                reject_undeclared_outcomes_in_block(
                    &value.body,
                    root,
                    &declared,
                    &instance.name,
                    diagnostics,
                );
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OutcomeSignature {
    policy: uhura_syntax::ast::OutcomePolicy,
    fields: Vec<(String, String)>,
}

fn root_outcomes(
    machine: &uhura_syntax::ast::MachineDeclaration,
    bindings: &BTreeMap<String, String>,
) -> BTreeMap<String, OutcomeSignature> {
    machine
        .members
        .iter()
        .find_map(|member| match &member.kind {
            uhura_syntax::ast::MachineMemberKind::Outcomes(value) => Some(value),
            _ => None,
        })
        .map(|section| {
            section
                .entries
                .iter()
                .map(|entry| {
                    (
                        entry.variant.name.text.clone(),
                        outcome_signature(entry, bindings),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn outcome_signature(
    entry: &uhura_syntax::ast::OutcomeEntry,
    bindings: &BTreeMap<String, String>,
) -> OutcomeSignature {
    OutcomeSignature {
        policy: entry.policy,
        fields: entry
            .variant
            .parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.name.text.clone(),
                    canonical_type(&parameter.ty, bindings),
                )
            })
            .collect(),
    }
}

fn canonical_type(
    ty: &uhura_syntax::ast::TypeExpression,
    bindings: &BTreeMap<String, String>,
) -> String {
    match &ty.kind {
        uhura_syntax::ast::TypeExpressionKind::Unit => "()".into(),
        uhura_syntax::ast::TypeExpressionKind::Tuple(values) => format!(
            "({})",
            values
                .iter()
                .map(|value| canonical_type(value, bindings))
                .collect::<Vec<_>>()
                .join(",")
        ),
        uhura_syntax::ast::TypeExpressionKind::Path(path) => path
            .segments
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                let name = if index == 0 {
                    bindings
                        .get(&segment.name.text)
                        .map(String::as_str)
                        .unwrap_or(&segment.name.text)
                } else {
                    &segment.name.text
                };
                if segment.arguments.is_empty() {
                    name.to_owned()
                } else {
                    format!(
                        "{name}<{}>",
                        segment
                            .arguments
                            .iter()
                            .map(|value| canonical_type(value, bindings))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("::"),
    }
}

fn reject_undeclared_outcomes_in_block(
    block: &uhura_syntax::ast::Block,
    root: &BTreeMap<String, OutcomeSignature>,
    declared: &BTreeSet<String>,
    owner: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    visit_block_expressions(block, &mut |expression| {
        let name = outcome_expression_name(expression);
        if let Some(name) = name
            && root.contains_key(name)
            && !declared.contains(name)
        {
            composition_error(
                diagnostics,
                codes::OUTCOME,
                "uhura-0.4/undeclared-required-outcome",
                format!(
                    "part `{owner}` selects enclosing outcome `{name}` without declaring it in `requires outcomes`"
                ),
                expression.span,
            );
        }
    });
}

fn outcome_expression_name(expression: &uhura_syntax::ast::Expression) -> Option<&str> {
    match &expression.kind {
        uhura_syntax::ast::ExpressionKind::Name(name) if name.segments.len() == 1 => {
            Some(name.segments[0].text.as_str())
        }
        uhura_syntax::ast::ExpressionKind::Call { callee, .. } => {
            let uhura_syntax::ast::ExpressionKind::Name(name) = &callee.kind else {
                return None;
            };
            (name.segments.len() == 1).then(|| name.segments[0].text.as_str())
        }
        _ => None,
    }
}

fn validate_part_initializers(instance: &InstancePlan, diagnostics: &mut Vec<Diagnostic>) {
    let handle_names = instance
        .parameters
        .iter()
        .filter_map(|parameter| match parameter {
            ParameterKind::Capability(value) => Some(value.parameter.name.text.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    for member in &instance.template.declaration.members {
        match &member.kind {
            uhura_syntax::ast::PartMemberKind::Require(value) => {
                reject_names(
                    &value.condition,
                    &handle_names,
                    "part requirement",
                    diagnostics,
                );
            }
            uhura_syntax::ast::PartMemberKind::State(value) => {
                for field in &value.fields {
                    reject_names(
                        &field.initial,
                        &handle_names,
                        "part state initializer",
                        diagnostics,
                    );
                }
            }
            _ => {}
        }
    }
}

fn reject_non_config_binding(
    machine: &uhura_syntax::ast::MachineDeclaration,
    expression: &uhura_syntax::ast::Expression,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let forbidden = machine
        .members
        .iter()
        .flat_map(|member| match &member.kind {
            uhura_syntax::ast::MachineMemberKind::State(value) => value
                .fields
                .iter()
                .map(|field| field.name.text.as_str())
                .collect::<Vec<_>>(),
            uhura_syntax::ast::MachineMemberKind::Computed(value) => vec![value.name.text.as_str()],
            _ => Vec::new(),
        })
        .collect::<BTreeSet<_>>();
    reject_names(
        expression,
        &forbidden,
        "part configuration binding",
        diagnostics,
    );
}

fn reject_names(
    expression: &uhura_syntax::ast::Expression,
    forbidden: &BTreeSet<&str>,
    context: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    visit_expression(expression, &mut |value| {
        let uhura_syntax::ast::ExpressionKind::Name(name) = &value.kind else {
            return;
        };
        if name.segments.len() == 1 && forbidden.contains(name.segments[0].text.as_str()) {
            composition_error(
                diagnostics,
                codes::EFFECT,
                "uhura-0.4/non-config-part-binding",
                format!(
                    "{context} cannot read non-configuration value `{}`",
                    name.segments[0].text
                ),
                value.span,
            );
        }
    });
}

fn reject_cycles(
    kind: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    diagnostics: &mut Vec<Diagnostic>,
    span: uhura_syntax::ast::Span,
) {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for node in graph.keys() {
        if graph_has_cycle(node, graph, &mut visiting, &mut visited) {
            composition_error(
                diagnostics,
                codes::DEPENDENCY_CYCLE,
                "uhura-0.4/part-dependency-cycle",
                format!("composed part {kind} capabilities form a dependency cycle"),
                span,
            );
            break;
        }
    }
}

fn graph_has_cycle(
    node: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if visited.contains(node) {
        return false;
    }
    if !visiting.insert(node.to_owned()) {
        return true;
    }
    if graph.get(node).is_some_and(|edges| {
        edges
            .iter()
            .any(|edge| graph_has_cycle(edge, graph, visiting, visited))
    }) {
        return true;
    }
    visiting.remove(node);
    visited.insert(node.to_owned());
    false
}

fn reject_unit_update_cycles(
    updates: &BTreeMap<String, UnitUpdate>,
    diagnostics: &mut Vec<Diagnostic>,
    span: uhura_syntax::ast::Span,
) {
    let mut graph = BTreeMap::<String, BTreeSet<String>>::new();
    for (name, update) in updates {
        visit_block_expressions(&update.body, &mut |expression| {
            let mut sink = Vec::new();
            if let Some((target, _)) = update_call(expression, &update.context, &mut sink)
                && updates.contains_key(&target)
            {
                graph.entry(name.clone()).or_default().insert(target);
            }
        });
    }
    reject_cycles("update-call", &graph, diagnostics, span);
}

fn visit_block_expressions(
    block: &uhura_syntax::ast::Block,
    visitor: &mut impl FnMut(&uhura_syntax::ast::Expression),
) {
    for statement in &block.statements {
        match &statement.kind {
            uhura_syntax::ast::StatementKind::Let { value, .. }
            | uhura_syntax::ast::StatementKind::Assign { value, .. } => {
                visit_expression(value, visitor);
            }
            uhura_syntax::ast::StatementKind::Emit { output, .. } => {
                for value in &output.arguments {
                    visit_expression(value, visitor);
                }
            }
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                visit_expression(condition, visitor);
                visit_expression(decreases, visitor);
                visit_block_expressions(body, visitor);
            }
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                visit_expression(expression, visitor);
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => {}
        }
    }
    if let Some(tail) = &block.tail {
        visit_expression(tail, visitor);
    }
}

fn visit_expression(
    expression: &uhura_syntax::ast::Expression,
    visitor: &mut impl FnMut(&uhura_syntax::ast::Expression),
) {
    visitor(expression);
    match &expression.kind {
        uhura_syntax::ast::ExpressionKind::Sequence(values)
        | uhura_syntax::ast::ExpressionKind::Tuple(values) => {
            for value in values {
                visit_expression(value, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::Group(value)
        | uhura_syntax::ast::ExpressionKind::Unary { value, .. } => {
            visit_expression(value, visitor)
        }
        uhura_syntax::ast::ExpressionKind::Record(value) => {
            for field in &value.fields {
                if let Some(value) = &field.value {
                    visit_expression(value, visitor);
                }
            }
            if let Some(base) = &value.base {
                visit_expression(base, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => {
            for entry in entries {
                visit_expression(&entry.key, visitor);
                visit_expression(&entry.value, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::Block(value) => visit_block_expressions(value, visitor),
        uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
            visit_expression(callee, visitor);
            for argument in arguments {
                match argument {
                    uhura_syntax::ast::CallArgument::Expression(value) => {
                        visit_expression(value, visitor);
                    }
                    uhura_syntax::ast::CallArgument::Binder(value) => {
                        visit_expression(&value.body, visitor);
                    }
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Member { value, .. } => visit_expression(value, visitor),
        uhura_syntax::ast::ExpressionKind::Index { value, index }
        | uhura_syntax::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | uhura_syntax::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => {
            visit_expression(value, visitor);
            visit_expression(index, visitor);
        }
        uhura_syntax::ast::ExpressionKind::Is { value, .. } => visit_expression(value, visitor),
        uhura_syntax::ast::ExpressionKind::If(value) => {
            visit_expression(&value.condition, visitor);
            visit_block_expressions(&value.then_branch, visitor);
            if let Some(branch) = &value.else_branch {
                match branch {
                    uhura_syntax::ast::ElseBranch::Block(value) => {
                        visit_block_expressions(value, visitor);
                    }
                    uhura_syntax::ast::ElseBranch::If(value) => visit_expression(value, visitor),
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Match(value) => {
            visit_expression(&value.value, visitor);
            for arm in &value.arms {
                visit_expression(&arm.value, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::Return(value) => {
            if let Some(value) = value {
                visit_expression(value, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::Literal(_)
        | uhura_syntax::ast::ExpressionKind::Unit
        | uhura_syntax::ast::ExpressionKind::Name(_) => {}
    }
}

fn type_name(name: &str, span: uhura_syntax::ast::Span) -> uhura_syntax::ast::TypeExpression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::TypeExpressionKind::Path(uhura_syntax::ast::TypePath {
            segments: vec![uhura_syntax::ast::TypePathSegment {
                name: uhura_syntax::ast::Identifier::new(name, span),
                arguments: Vec::new(),
                span,
            }],
            span,
        }),
        span,
    )
}

fn name_expression(name: String, span: uhura_syntax::ast::Span) -> uhura_syntax::ast::Expression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::ExpressionKind::Name(uhura_syntax::ast::QualifiedName {
            segments: vec![uhura_syntax::ast::Identifier::new(name, span)],
            span,
        }),
        span,
    )
}

fn bool_expression(value: bool, span: uhura_syntax::ast::Span) -> uhura_syntax::ast::Expression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::ExpressionKind::Literal(uhura_syntax::ast::Literal::Bool(value)),
        span,
    )
}

fn call_expression(
    name: String,
    arguments: Vec<uhura_syntax::ast::Expression>,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::Expression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::ExpressionKind::Call {
            callee: Box::new(name_expression(name, span)),
            arguments: arguments
                .into_iter()
                .map(uhura_syntax::ast::CallArgument::Expression)
                .collect(),
        },
        span,
    )
}

fn composition_error(
    diagnostics: &mut Vec<Diagnostic>,
    code: &'static str,
    rule: &'static str,
    message: impl Into<String>,
    span: uhura_syntax::ast::Span,
) {
    diagnostics.push(error(
        code,
        rule,
        message,
        ast::SourceSpan::new(span.file, span.start, span.end),
    ));
}
