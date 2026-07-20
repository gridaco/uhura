//! Authored ownership and dependency topology for checked Uhura 0.4 source.
//!
//! Part composition is erased before the one runtime IR is produced. This
//! pass preserves only source-language inspection facts and binds each fact
//! to validated provenance occurrences.

use std::collections::{BTreeMap, BTreeSet};

use uhura_core::{
    AuthoredInteractionEdge, AuthoredInteractionNode, AuthoredInteractionTopology,
    InteractionGraphEdge, InteractionGraphEdgeKind, InteractionGraphNode, InteractionGraphNodeKind,
    ProvenanceSelector, interaction_node_id, semantic_node_id,
};
use uhura_syntax::v04::ast;

#[derive(Clone, Debug)]
pub(super) struct TopologyOccurrence {
    pub source_package: String,
    pub node: String,
    pub span: ast::Span,
    pub role: &'static str,
    pub owner: String,
}

pub(super) struct TopologyBuild {
    pub topology: AuthoredInteractionTopology,
    pub occurrences: Vec<TopologyOccurrence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct V04TopologyBinding {
    pub source_package: String,
    pub source_module: String,
    pub local_name: String,
    pub target_package: String,
    pub target_module: String,
    pub target_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Capability {
    Reads,
    Updates,
}

#[derive(Clone, Debug)]
struct Handle {
    provider: String,
    capability: Capability,
}

#[derive(Clone, Debug, Default)]
struct Catalog {
    reads: BTreeMap<String, InteractionGraphNode>,
    updates: BTreeMap<String, InteractionGraphNode>,
}

#[derive(Clone, Copy)]
struct DeclarationRef<'a> {
    module: &'a ast::Module,
    declaration: &'a ast::Declaration,
}

struct Instance<'a> {
    name: String,
    target: DeclarationRef<'a>,
    part: &'a ast::PartDeclaration,
    syntax: &'a ast::PartInstance,
    handles: BTreeMap<String, Handle>,
    catalog: Catalog,
}

struct Scope<'a> {
    local: &'a Catalog,
    instances: &'a BTreeMap<String, Catalog>,
    handles: &'a BTreeMap<String, Handle>,
}

#[derive(Clone, Copy)]
struct TopologyContext<'a> {
    source_package: &'a str,
    machine: &'a str,
}

#[derive(Clone, Debug)]
struct Reference {
    target: InteractionGraphNode,
    span: ast::Span,
}

struct Builder<'a> {
    modules: &'a [ast::Module],
    bindings: BTreeMap<(String, String, String), (String, String, String)>,
    declarations: BTreeMap<(String, String, String), DeclarationRef<'a>>,
    nodes: Vec<AuthoredInteractionNode>,
    edges: Vec<AuthoredInteractionEdge>,
    occurrences: Vec<TopologyOccurrence>,
}

pub(super) fn build_local(
    modules: &[ast::Module],
    bindings: &BTreeMap<(String, String), (String, String)>,
) -> Result<TopologyBuild, String> {
    let package = modules
        .first()
        .map(|module| module.identity.package.as_str())
        .unwrap_or_default();
    let bindings = bindings
        .iter()
        .map(
            |((source_module, local_name), (target_module, target_name))| V04TopologyBinding {
                source_package: package.into(),
                source_module: source_module.clone(),
                local_name: local_name.clone(),
                target_package: package.into(),
                target_module: target_module.clone(),
                target_name: target_name.clone(),
            },
        )
        .collect::<Vec<_>>();
    build_linked(modules, &bindings)
}

pub(super) fn build_linked(
    modules: &[ast::Module],
    bindings: &[V04TopologyBinding],
) -> Result<TopologyBuild, String> {
    Builder::new(modules, bindings).build()
}

impl<'a> Builder<'a> {
    fn new(modules: &'a [ast::Module], bindings: &[V04TopologyBinding]) -> Self {
        let declarations = modules
            .iter()
            .flat_map(|module| {
                module.declarations.iter().map(move |declaration| {
                    (
                        (
                            module.identity.package.clone(),
                            module.identity.module.clone(),
                            declaration_name(declaration).to_string(),
                        ),
                        DeclarationRef {
                            module,
                            declaration,
                        },
                    )
                })
            })
            .collect();
        let bindings = bindings
            .iter()
            .map(|binding| {
                (
                    (
                        binding.source_package.clone(),
                        binding.source_module.clone(),
                        binding.local_name.clone(),
                    ),
                    (
                        binding.target_package.clone(),
                        binding.target_module.clone(),
                        binding.target_name.clone(),
                    ),
                )
            })
            .collect();
        Self {
            modules,
            bindings,
            declarations,
            nodes: Vec::new(),
            edges: Vec::new(),
            occurrences: Vec::new(),
        }
    }

    fn build(mut self) -> Result<TopologyBuild, String> {
        for module in self.modules {
            for declaration in &module.declarations {
                let ast::DeclarationKind::Machine(machine) = &declaration.kind else {
                    continue;
                };
                if machine.visibility == ast::Visibility::Public {
                    self.machine(module, machine)?;
                }
            }
        }
        Ok(TopologyBuild {
            topology: AuthoredInteractionTopology::canonical(self.nodes, self.edges)?,
            occurrences: self.occurrences,
        })
    }

    fn machine(
        &mut self,
        module: &'a ast::Module,
        machine: &'a ast::MachineDeclaration,
    ) -> Result<(), String> {
        let machine_id = format!("{}::{}", module.identity.package, machine.name.text);
        let machine_source = source_selector(
            &machine_id,
            "root",
            "machine",
            &format!("declaration/{}", machine.name.text),
            "definition",
        );
        let machine_node = graph_node(
            "machine",
            InteractionGraphNodeKind::Machine,
            &machine_id,
            &machine_id,
        );
        let module_label = format!("{}::{}", module.identity.package, module.identity.module);
        let module_node = graph_node(
            "module",
            InteractionGraphNodeKind::Module,
            &machine_id,
            &module_label,
        );
        self.add_node(machine_node.clone(), [machine_source.clone()]);
        self.add_node(module_node.clone(), [machine_source.clone()]);
        self.add_edge(
            &module_node,
            &machine_node,
            InteractionGraphEdgeKind::Owns,
            [machine_source],
        );

        let root_catalog = machine_catalog(&machine_id, &machine.members);
        let mut instances = self.instances(module, &machine_id, machine)?;
        let instance_catalogs = instances
            .iter()
            .map(|instance| (instance.name.clone(), instance.catalog.clone()))
            .collect::<BTreeMap<_, _>>();
        for instance in &mut instances {
            instance.handles = instance_handles(instance, &instance_catalogs);
        }

        self.machine_members(
            TopologyContext {
                source_package: &module.identity.package,
                machine: &machine_id,
            },
            &machine_node,
            &machine.members,
            &root_catalog,
            &instance_catalogs,
        );
        for instance in &instances {
            self.part(&machine_id, &machine_node, instance, &instance_catalogs);
        }
        Ok(())
    }

    fn instances(
        &self,
        module: &'a ast::Module,
        machine_id: &str,
        machine: &'a ast::MachineDeclaration,
    ) -> Result<Vec<Instance<'a>>, String> {
        machine
            .members
            .iter()
            .filter_map(|member| match &member.kind {
                ast::MachineMemberKind::Part(instance) => Some(instance),
                _ => None,
            })
            .map(|syntax| {
                let local_name = singular_type_name(&syntax.part)
                    .ok_or_else(|| format!("part `{}` has no singular type", syntax.name.text))?;
                let target = self.resolve(module, local_name).ok_or_else(|| {
                    format!("accepted part `{}` no longer resolves", syntax.name.text)
                })?;
                let ast::DeclarationKind::Part(part) = &target.declaration.kind else {
                    return Err(format!(
                        "accepted part `{}` no longer resolves to a part declaration",
                        syntax.name.text
                    ));
                };
                Ok(Instance {
                    name: syntax.name.text.clone(),
                    target,
                    part,
                    syntax,
                    handles: BTreeMap::new(),
                    catalog: part_catalog(machine_id, &syntax.name.text, &part.members),
                })
            })
            .collect()
    }

    fn part(
        &mut self,
        machine: &str,
        machine_node: &InteractionGraphNode,
        instance: &Instance<'a>,
        instances: &BTreeMap<String, Catalog>,
    ) {
        let semantic =
            semantic_node_id(machine, "root", "part", &format!("part/{}", instance.name));
        self.occurrences.push(TopologyOccurrence {
            source_package: instance.target.module.identity.package.clone(),
            node: semantic.clone(),
            span: instance.part.name.span,
            role: "generated",
            owner: instance.name.clone(),
        });
        let composition_source = ProvenanceSelector {
            node: semantic.clone(),
            role: "definition".into(),
            owner: "root".into(),
        };
        let definition_source = ProvenanceSelector {
            node: semantic,
            role: "generated".into(),
            owner: instance.name.clone(),
        };
        let part_node = graph_node(
            "part",
            InteractionGraphNodeKind::Part,
            machine,
            &instance.name,
        );
        self.add_node(
            part_node.clone(),
            [composition_source.clone(), definition_source.clone()],
        );
        self.add_edge(
            machine_node,
            &part_node,
            InteractionGraphEdgeKind::Composes,
            [composition_source],
        );

        let module_label = format!(
            "{}::{}",
            instance.target.module.identity.package, instance.target.module.identity.module
        );
        let module_node = graph_node(
            "module",
            InteractionGraphNodeKind::Module,
            machine,
            &module_label,
        );
        self.add_node(module_node.clone(), [definition_source.clone()]);
        self.add_edge(
            &module_node,
            &part_node,
            InteractionGraphEdgeKind::Owns,
            [definition_source],
        );
        self.part_members(machine, &part_node, instance, instances);
    }

    fn machine_members(
        &mut self,
        context: TopologyContext<'_>,
        owner_node: &InteractionGraphNode,
        members: &[ast::MachineMember],
        catalog: &Catalog,
        instances: &BTreeMap<String, Catalog>,
    ) {
        let handles = BTreeMap::new();
        let scope = Scope {
            local: catalog,
            instances,
            handles: &handles,
        };
        let mut handlers = BTreeMap::<String, usize>::new();
        let mut invariants = 0usize;
        for member in members {
            match &member.kind {
                ast::MachineMemberKind::State(value) => {
                    self.states(context, "root", owner_node, value, "definition");
                }
                ast::MachineMemberKind::Computed(value) => {
                    self.computed(context, "root", owner_node, value, "definition", &scope);
                }
                ast::MachineMemberKind::Invariant(value) => {
                    self.invariant(
                        context,
                        "root",
                        owner_node,
                        value,
                        member.span,
                        invariants,
                        "definition",
                        &scope,
                    );
                    invariants += 1;
                }
                ast::MachineMemberKind::Observe(value) => {
                    self.observations(context, "root", owner_node, value, "definition", &scope);
                }
                ast::MachineMemberKind::Update(value) => {
                    self.update(context, "root", owner_node, value, "definition", &scope);
                }
                ast::MachineMemberKind::Handler(value) => {
                    let selector = selector_path(&value.input);
                    let ordinal = handlers.entry(selector.clone()).or_default();
                    let current = *ordinal;
                    *ordinal += 1;
                    self.handler(
                        context,
                        "root",
                        owner_node,
                        value,
                        "definition",
                        &selector,
                        current,
                        false,
                        &scope,
                    );
                }
                _ => {}
            }
        }
    }

    fn part_members(
        &mut self,
        machine: &str,
        owner_node: &InteractionGraphNode,
        instance: &Instance<'_>,
        instances: &BTreeMap<String, Catalog>,
    ) {
        let context = TopologyContext {
            source_package: &instance.target.module.identity.package,
            machine,
        };
        let owner = instance.name.as_str();
        let scope = Scope {
            local: &instance.catalog,
            instances,
            handles: &instance.handles,
        };
        let mut handlers = BTreeMap::<String, usize>::new();
        let mut invariants = 0usize;
        for member in &instance.part.members {
            match &member.kind {
                ast::PartMemberKind::State(value) => {
                    self.states(context, owner, owner_node, value, "generated");
                }
                ast::PartMemberKind::Computed(value) => {
                    self.computed(context, owner, owner_node, value, "generated", &scope);
                }
                ast::PartMemberKind::Invariant(value) => {
                    self.invariant(
                        context,
                        owner,
                        owner_node,
                        value,
                        member.span,
                        invariants,
                        "generated",
                        &scope,
                    );
                    invariants += 1;
                }
                ast::PartMemberKind::Observe(value) => {
                    self.observations(context, owner, owner_node, value, "generated", &scope);
                }
                ast::PartMemberKind::Update(value) => {
                    self.update(context, owner, owner_node, value, "generated", &scope);
                }
                ast::PartMemberKind::Handler(value) => {
                    let selector = selector_path(&value.input);
                    let ordinal = handlers.entry(selector.clone()).or_default();
                    let current = *ordinal;
                    *ordinal += 1;
                    self.handler(
                        context,
                        owner,
                        owner_node,
                        value,
                        "generated",
                        &selector,
                        current,
                        true,
                        &scope,
                    );
                }
                _ => {}
            }
        }
    }

    fn states(
        &mut self,
        context: TopologyContext<'_>,
        owner: &str,
        owner_node: &InteractionGraphNode,
        value: &ast::StateSection,
        role: &'static str,
    ) {
        for field in &value.fields {
            let state = graph_node(
                "state",
                InteractionGraphNodeKind::State,
                context.machine,
                &qualified(owner, &field.name.text),
            );
            let source = source_selector(
                context.machine,
                owner,
                "state",
                &format!("state/{}", field.name.text),
                role,
            );
            self.record_occurrence(context, &source, field.name.span, role);
            self.add_node(state.clone(), [source.clone()]);
            self.add_edge(owner_node, &state, InteractionGraphEdgeKind::Owns, [source]);
        }
    }

    fn computed(
        &mut self,
        context: TopologyContext<'_>,
        owner: &str,
        owner_node: &InteractionGraphNode,
        value: &ast::ComputedDeclaration,
        role: &'static str,
        scope: &Scope<'_>,
    ) {
        let computed = graph_node(
            "computed",
            InteractionGraphNodeKind::Computed,
            context.machine,
            &qualified(owner, &value.name.text),
        );
        let source = source_selector(
            context.machine,
            owner,
            "computed",
            &format!("computed/{}", value.name.text),
            role,
        );
        self.record_occurrence(context, &source, value.name.span, role);
        self.add_node(computed.clone(), [source.clone()]);
        self.add_edge(
            owner_node,
            &computed,
            InteractionGraphEdgeKind::Owns,
            [source],
        );
        let mut reads = Vec::new();
        collect_reads(&value.value, scope, &mut reads);
        self.dependencies(
            context,
            owner,
            &format!("computed/{}", value.name.text),
            &computed,
            InteractionGraphEdgeKind::Reads,
            reads,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn invariant(
        &mut self,
        context: TopologyContext<'_>,
        owner: &str,
        owner_node: &InteractionGraphNode,
        value: &ast::InvariantDeclaration,
        span: ast::Span,
        ordinal: usize,
        role: &'static str,
        scope: &Scope<'_>,
    ) {
        let label = qualified(owner, &format!("invariant {}", ordinal + 1));
        let invariant = graph_node(
            "invariant",
            InteractionGraphNodeKind::Invariant,
            context.machine,
            &label,
        );
        let source = source_selector(
            context.machine,
            owner,
            "invariant",
            &format!("invariant/{ordinal}"),
            role,
        );
        self.record_occurrence(context, &source, span, role);
        self.add_node(invariant.clone(), [source.clone()]);
        self.add_edge(
            owner_node,
            &invariant,
            InteractionGraphEdgeKind::Owns,
            [source],
        );
        let mut reads = Vec::new();
        for condition in &value.conditions {
            collect_reads(condition, scope, &mut reads);
        }
        self.dependencies(
            context,
            owner,
            &format!("invariant/{ordinal}"),
            &invariant,
            InteractionGraphEdgeKind::Reads,
            reads,
        );
    }

    fn observations(
        &mut self,
        context: TopologyContext<'_>,
        owner: &str,
        owner_node: &InteractionGraphNode,
        value: &ast::ObserveSection,
        role: &'static str,
        scope: &Scope<'_>,
    ) {
        for field in &value.fields {
            let observation = graph_node(
                "observation",
                InteractionGraphNodeKind::Observation,
                context.machine,
                &qualified(owner, &field.name.text),
            );
            let source = source_selector(
                context.machine,
                owner,
                "observation",
                &format!("observe/{}", field.name.text),
                role,
            );
            self.record_occurrence(context, &source, field.name.span, role);
            self.add_node(observation.clone(), [source.clone()]);
            self.add_edge(
                owner_node,
                &observation,
                InteractionGraphEdgeKind::Owns,
                [source],
            );
            let mut reads = Vec::new();
            if let Some(expression) = &field.value {
                collect_reads(expression, scope, &mut reads);
            } else if let Some(target) = scope.local.reads.get(&field.name.text) {
                reads.push(Reference {
                    target: target.clone(),
                    span: field.name.span,
                });
            }
            self.dependencies(
                context,
                owner,
                &format!("observe/{}", field.name.text),
                &observation,
                InteractionGraphEdgeKind::Observes,
                reads,
            );
        }
    }

    fn update(
        &mut self,
        context: TopologyContext<'_>,
        owner: &str,
        owner_node: &InteractionGraphNode,
        value: &ast::UpdateDeclaration,
        role: &'static str,
        scope: &Scope<'_>,
    ) {
        let update = graph_node(
            "update",
            InteractionGraphNodeKind::Update,
            context.machine,
            &qualified(owner, &value.name.text),
        );
        let source = source_selector(
            context.machine,
            owner,
            "update",
            &format!("update/{}", value.name.text),
            role,
        );
        self.record_occurrence(context, &source, value.name.span, role);
        self.add_node(update.clone(), [source.clone()]);
        self.add_edge(
            owner_node,
            &update,
            InteractionGraphEdgeKind::Owns,
            [source],
        );
        let mut reads = Vec::new();
        collect_reads_in_block(
            &value.body,
            scope,
            value
                .parameters
                .iter()
                .map(|parameter| &parameter.name.text),
            &mut reads,
        );
        self.dependencies(
            context,
            owner,
            &format!("update/{}", value.name.text),
            &update,
            InteractionGraphEdgeKind::Reads,
            reads,
        );
        let mut calls = Vec::new();
        collect_calls_in_block(&value.body, scope, &mut calls);
        self.dependencies(
            context,
            owner,
            &format!("update/{}", value.name.text),
            &update,
            InteractionGraphEdgeKind::Calls,
            calls,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn handler(
        &mut self,
        context: TopologyContext<'_>,
        owner: &str,
        owner_node: &InteractionGraphNode,
        value: &ast::HandlerDeclaration,
        role: &'static str,
        selector: &str,
        ordinal: usize,
        composed: bool,
        scope: &Scope<'_>,
    ) {
        let lowered = if composed {
            composed_selector(owner, &value.input)
        } else {
            selector.to_string()
        };
        let input = graph_node(
            "input",
            InteractionGraphNodeKind::Input,
            context.machine,
            &lowered,
        );
        let source = source_selector(
            context.machine,
            owner,
            "handler",
            &format!("handler/{selector}/{ordinal}"),
            role,
        );
        self.record_occurrence(context, &source, value.input.span, role);
        self.add_node(input.clone(), [source.clone()]);
        self.add_edge(owner_node, &input, InteractionGraphEdgeKind::Owns, [source]);
        let mut lexical = BTreeSet::new();
        for pattern in &value.parameters {
            collect_pattern_bindings(pattern, &mut lexical);
        }
        let mut reads = Vec::new();
        collect_reads_in_block(&value.body, scope, lexical.iter(), &mut reads);
        self.dependencies(
            context,
            owner,
            &format!("handler/{selector}/{ordinal}"),
            &input,
            InteractionGraphEdgeKind::Reads,
            reads,
        );
        let mut calls = Vec::new();
        collect_calls_in_block(&value.body, scope, &mut calls);
        self.dependencies(
            context,
            owner,
            &format!("handler/{selector}/{ordinal}"),
            &input,
            InteractionGraphEdgeKind::Calls,
            calls,
        );
    }

    fn dependencies(
        &mut self,
        context: TopologyContext<'_>,
        owner: &str,
        actor_path: &str,
        actor: &InteractionGraphNode,
        kind: InteractionGraphEdgeKind,
        references: Vec<Reference>,
    ) {
        let relation = match kind {
            InteractionGraphEdgeKind::Reads => "read",
            InteractionGraphEdgeKind::Calls => "call",
            InteractionGraphEdgeKind::Observes => "observe",
            _ => unreachable!("only authored dependency edge kinds are accepted"),
        };
        for (ordinal, reference) in references.into_iter().enumerate() {
            let semantic = semantic_node_id(
                context.machine,
                owner,
                "dependency",
                &format!("{actor_path}/{relation}/{ordinal}/{}", reference.target.id),
            );
            self.occurrences.push(TopologyOccurrence {
                source_package: context.source_package.into(),
                node: semantic.clone(),
                span: reference.span,
                role: "reference",
                owner: owner.into(),
            });
            self.add_edge(
                actor,
                &reference.target,
                kind,
                [ProvenanceSelector {
                    node: semantic,
                    role: "reference".into(),
                    owner: owner.into(),
                }],
            );
        }
    }

    fn resolve(&self, module: &ast::Module, local_name: &str) -> Option<DeclarationRef<'a>> {
        let key = self
            .bindings
            .get(&(
                module.identity.package.clone(),
                module.identity.module.clone(),
                local_name.to_string(),
            ))
            .cloned()
            .unwrap_or_else(|| {
                (
                    module.identity.package.clone(),
                    module.identity.module.clone(),
                    local_name.to_string(),
                )
            });
        self.declarations.get(&key).copied()
    }

    fn add_node(
        &mut self,
        node: InteractionGraphNode,
        sources: impl IntoIterator<Item = ProvenanceSelector>,
    ) {
        self.nodes.push(AuthoredInteractionNode {
            node,
            sources: sources.into_iter().collect(),
        });
    }

    fn record_occurrence(
        &mut self,
        context: TopologyContext<'_>,
        selector: &ProvenanceSelector,
        span: ast::Span,
        role: &'static str,
    ) {
        self.occurrences.push(TopologyOccurrence {
            source_package: context.source_package.into(),
            node: selector.node.clone(),
            span,
            role,
            owner: selector.owner.clone(),
        });
    }

    fn add_edge(
        &mut self,
        from: &InteractionGraphNode,
        to: &InteractionGraphNode,
        kind: InteractionGraphEdgeKind,
        sources: impl IntoIterator<Item = ProvenanceSelector>,
    ) {
        self.edges.push(AuthoredInteractionEdge {
            edge: InteractionGraphEdge {
                from: from.id.clone(),
                to: to.id.clone(),
                kind,
            },
            sources: sources.into_iter().collect(),
        });
    }
}

fn graph_node(
    id_kind: &str,
    kind: InteractionGraphNodeKind,
    machine: &str,
    label: &str,
) -> InteractionGraphNode {
    InteractionGraphNode {
        id: interaction_node_id(
            id_kind,
            machine,
            if kind == InteractionGraphNodeKind::Machine {
                ""
            } else {
                label
            },
        ),
        kind,
        machine: machine.into(),
        label: label.into(),
    }
}

fn source_selector(
    machine: &str,
    owner: &str,
    kind: &str,
    path: &str,
    role: &str,
) -> ProvenanceSelector {
    ProvenanceSelector {
        node: semantic_node_id(machine, owner, kind, path),
        role: role.into(),
        owner: owner.into(),
    }
}

fn qualified(owner: &str, name: &str) -> String {
    if owner == "root" {
        name.into()
    } else {
        format!("{owner}.{name}")
    }
}

fn declaration_name(declaration: &ast::Declaration) -> &str {
    match &declaration.kind {
        ast::DeclarationKind::Machine(value) => &value.name.text,
        ast::DeclarationKind::Part(value) => &value.name.text,
        ast::DeclarationKind::Ui(value) => &value.name.text,
        ast::DeclarationKind::Struct(value) => &value.name.text,
        ast::DeclarationKind::Enum(value) => &value.name.text,
        ast::DeclarationKind::Key(value) => &value.name.text,
        ast::DeclarationKind::Const(value) => &value.name.text,
        ast::DeclarationKind::Function(value) => &value.name.text,
    }
}

fn machine_catalog(machine: &str, members: &[ast::MachineMember]) -> Catalog {
    let mut catalog = Catalog::default();
    for member in members {
        match &member.kind {
            ast::MachineMemberKind::State(value) => {
                for field in &value.fields {
                    catalog.reads.insert(
                        field.name.text.clone(),
                        graph_node(
                            "state",
                            InteractionGraphNodeKind::State,
                            machine,
                            &field.name.text,
                        ),
                    );
                }
            }
            ast::MachineMemberKind::Computed(value) => {
                catalog.reads.insert(
                    value.name.text.clone(),
                    graph_node(
                        "computed",
                        InteractionGraphNodeKind::Computed,
                        machine,
                        &value.name.text,
                    ),
                );
            }
            ast::MachineMemberKind::Update(value) => {
                catalog.updates.insert(
                    value.name.text.clone(),
                    graph_node(
                        "update",
                        InteractionGraphNodeKind::Update,
                        machine,
                        &value.name.text,
                    ),
                );
            }
            _ => {}
        }
    }
    catalog
}

fn part_catalog(machine: &str, owner: &str, members: &[ast::PartMember]) -> Catalog {
    let mut catalog = Catalog::default();
    for member in members {
        match &member.kind {
            ast::PartMemberKind::State(value) => {
                for field in &value.fields {
                    catalog.reads.insert(
                        field.name.text.clone(),
                        graph_node(
                            "state",
                            InteractionGraphNodeKind::State,
                            machine,
                            &qualified(owner, &field.name.text),
                        ),
                    );
                }
            }
            ast::PartMemberKind::Computed(value) => {
                catalog.reads.insert(
                    value.name.text.clone(),
                    graph_node(
                        "computed",
                        InteractionGraphNodeKind::Computed,
                        machine,
                        &qualified(owner, &value.name.text),
                    ),
                );
            }
            ast::PartMemberKind::Update(value) => {
                catalog.updates.insert(
                    value.name.text.clone(),
                    graph_node(
                        "update",
                        InteractionGraphNodeKind::Update,
                        machine,
                        &qualified(owner, &value.name.text),
                    ),
                );
            }
            _ => {}
        }
    }
    catalog
}

fn instance_handles(
    instance: &Instance<'_>,
    instances: &BTreeMap<String, Catalog>,
) -> BTreeMap<String, Handle> {
    instance
        .part
        .parameters
        .iter()
        .zip(&instance.syntax.arguments)
        .filter_map(|(parameter, argument)| {
            let required = capability_parameter(&parameter.ty)?;
            let (provider, supplied) = dependency_argument(argument)?;
            (required == supplied && instances.contains_key(&provider)).then(|| {
                (
                    parameter.name.text.clone(),
                    Handle {
                        provider,
                        capability: required,
                    },
                )
            })
        })
        .collect()
}

fn capability_parameter(ty: &ast::TypeExpression) -> Option<Capability> {
    let ast::TypeExpressionKind::Path(path) = &ty.kind else {
        return None;
    };
    if path.segments.len() != 2 {
        return None;
    }
    match path.segments[1].name.text.as_str() {
        "Reads" => Some(Capability::Reads),
        "Updates" => Some(Capability::Updates),
        _ => None,
    }
}

fn dependency_argument(expression: &ast::Expression) -> Option<(String, Capability)> {
    let (path, _) = expression_path(expression)?;
    if path.len() != 2 {
        return None;
    }
    let capability = match path[1].as_str() {
        "reads" => Capability::Reads,
        "updates" => Capability::Updates,
        _ => return None,
    };
    Some((path[0].clone(), capability))
}

fn singular_type_name(path: &ast::TypePath) -> Option<&str> {
    (path.segments.len() == 1 && path.segments[0].arguments.is_empty())
        .then(|| path.segments[0].name.text.as_str())
}

fn selector_path(selector: &ast::ProtocolSelector) -> String {
    selector.owner.as_ref().map_or_else(
        || selector.variant.text.clone(),
        |owner| format!("{}.{}", owner.text, selector.variant.text),
    )
}

fn composed_selector(owner: &str, selector: &ast::ProtocolSelector) -> String {
    selector.owner.as_ref().map_or_else(
        || format!("{owner}.{}", selector.variant.text),
        |port| format!("{owner}.{}.{}", port.text, selector.variant.text),
    )
}

fn expression_path(expression: &ast::Expression) -> Option<(Vec<String>, ast::Span)> {
    match &expression.kind {
        ast::ExpressionKind::Name(name) => Some((
            name.segments
                .iter()
                .map(|segment| segment.text.clone())
                .collect(),
            name.segments.last()?.span,
        )),
        ast::ExpressionKind::Member { value, member } => {
            let (mut path, _) = expression_path(value)?;
            path.push(member.text.clone());
            Some((path, member.span))
        }
        ast::ExpressionKind::Group(value) => expression_path(value),
        _ => None,
    }
}

fn resolve_read(path: &[String], scope: &Scope<'_>) -> Option<InteractionGraphNode> {
    match path {
        [name] => scope.local.reads.get(name).cloned(),
        [handle, member] => {
            let binding = scope.handles.get(handle)?;
            (binding.capability == Capability::Reads)
                .then(|| {
                    scope
                        .instances
                        .get(&binding.provider)?
                        .reads
                        .get(member)
                        .cloned()
                })
                .flatten()
        }
        [instance, capability, member] if capability == "reads" => {
            scope.instances.get(instance)?.reads.get(member).cloned()
        }
        _ => None,
    }
}

fn resolve_update(path: &[String], scope: &Scope<'_>) -> Option<InteractionGraphNode> {
    match path {
        [name] => scope.local.updates.get(name).cloned(),
        [handle, member] => {
            let binding = scope.handles.get(handle)?;
            (binding.capability == Capability::Updates)
                .then(|| {
                    scope
                        .instances
                        .get(&binding.provider)?
                        .updates
                        .get(member)
                        .cloned()
                })
                .flatten()
        }
        [instance, capability, member] if capability == "updates" => {
            scope.instances.get(instance)?.updates.get(member).cloned()
        }
        _ => None,
    }
}

fn collect_reads(expression: &ast::Expression, scope: &Scope<'_>, output: &mut Vec<Reference>) {
    collect_reads_scoped(expression, scope, &BTreeSet::new(), output);
}

fn collect_reads_in_block<'a>(
    block: &ast::Block,
    scope: &Scope<'_>,
    lexical: impl IntoIterator<Item = &'a String>,
    output: &mut Vec<Reference>,
) {
    let lexical = lexical.into_iter().cloned().collect::<BTreeSet<_>>();
    collect_reads_in_block_scoped(block, scope, &lexical, output);
}

fn collect_reads_scoped(
    expression: &ast::Expression,
    scope: &Scope<'_>,
    lexical: &BTreeSet<String>,
    output: &mut Vec<Reference>,
) {
    if let Some((path, span)) = expression_path(expression)
        && path.first().is_some_and(|name| !lexical.contains(name))
        && let Some(target) = resolve_read(&path, scope)
    {
        output.push(Reference { target, span });
        return;
    }
    match &expression.kind {
        ast::ExpressionKind::Sequence(values) | ast::ExpressionKind::Tuple(values) => {
            for value in values {
                collect_reads_scoped(value, scope, lexical, output);
            }
        }
        ast::ExpressionKind::Group(value)
        | ast::ExpressionKind::Unary { value, .. }
        | ast::ExpressionKind::Return(Some(value)) => {
            collect_reads_scoped(value, scope, lexical, output);
        }
        ast::ExpressionKind::Record(value) => {
            for field in &value.fields {
                if let Some(value) = &field.value {
                    collect_reads_scoped(value, scope, lexical, output);
                } else if !lexical.contains(&field.name.text)
                    && let Some(target) = scope.local.reads.get(&field.name.text)
                {
                    output.push(Reference {
                        target: target.clone(),
                        span: field.name.span,
                    });
                }
            }
            if let Some(base) = &value.base {
                collect_reads_scoped(base, scope, lexical, output);
            }
        }
        ast::ExpressionKind::Block(block) => {
            collect_reads_in_block_scoped(block, scope, lexical, output);
        }
        ast::ExpressionKind::Call { callee, arguments } => {
            collect_reads_scoped(callee, scope, lexical, output);
            for argument in arguments {
                match argument {
                    ast::CallArgument::Expression(value) => {
                        collect_reads_scoped(value, scope, lexical, output);
                    }
                    ast::CallArgument::Binder(value) => {
                        let mut child = lexical.clone();
                        child.insert(value.parameter.text.clone());
                        collect_reads_scoped(&value.body, scope, &child, output);
                    }
                }
            }
        }
        ast::ExpressionKind::Member { value, .. } => {
            collect_reads_scoped(value, scope, lexical, output);
        }
        ast::ExpressionKind::Index { value, index }
        | ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => {
            collect_reads_scoped(value, scope, lexical, output);
            collect_reads_scoped(index, scope, lexical, output);
        }
        ast::ExpressionKind::Is { value, .. } => {
            collect_reads_scoped(value, scope, lexical, output);
        }
        ast::ExpressionKind::If(value) => {
            collect_reads_scoped(&value.condition, scope, lexical, output);
            collect_reads_in_block_scoped(&value.then_branch, scope, lexical, output);
            if let Some(branch) = &value.else_branch {
                match branch {
                    ast::ElseBranch::Block(block) => {
                        collect_reads_in_block_scoped(block, scope, lexical, output);
                    }
                    ast::ElseBranch::If(value) => {
                        collect_reads_scoped(value, scope, lexical, output);
                    }
                }
            }
        }
        ast::ExpressionKind::Match(value) => {
            collect_reads_scoped(&value.value, scope, lexical, output);
            for arm in &value.arms {
                let mut child = lexical.clone();
                collect_pattern_bindings(&arm.pattern, &mut child);
                collect_reads_scoped(&arm.value, scope, &child, output);
            }
        }
        ast::ExpressionKind::Literal(_)
        | ast::ExpressionKind::Unit
        | ast::ExpressionKind::Name(_)
        | ast::ExpressionKind::Return(None) => {}
    }
}

fn collect_reads_in_block_scoped(
    block: &ast::Block,
    scope: &Scope<'_>,
    lexical: &BTreeSet<String>,
    output: &mut Vec<Reference>,
) {
    let mut local = lexical.clone();
    for statement in &block.statements {
        match &statement.kind {
            ast::StatementKind::Let { name, value, .. } => {
                collect_reads_scoped(value, scope, &local, output);
                local.insert(name.text.clone());
            }
            ast::StatementKind::Assign { value, .. } => {
                collect_reads_scoped(value, scope, &local, output);
            }
            ast::StatementKind::Emit { output: value, .. } => {
                for argument in &value.arguments {
                    collect_reads_scoped(argument, scope, &local, output);
                }
            }
            ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                collect_reads_scoped(condition, scope, &local, output);
                collect_reads_scoped(decreases, scope, &local, output);
                collect_reads_in_block_scoped(body, scope, &local, output);
            }
            ast::StatementKind::Expression { expression, .. }
            | ast::StatementKind::BlockExpression(expression) => {
                collect_reads_scoped(expression, scope, &local, output);
            }
            ast::StatementKind::Unreachable { .. } => {}
        }
    }
    if let Some(tail) = &block.tail {
        collect_reads_scoped(tail, scope, &local, output);
    }
}

fn collect_pattern_bindings(pattern: &ast::Pattern, names: &mut BTreeSet<String>) {
    match &pattern.kind {
        ast::PatternKind::Binder(value) => {
            names.insert(value.text.clone());
        }
        ast::PatternKind::Group(value) => collect_pattern_bindings(value, names),
        ast::PatternKind::Tuple(values) | ast::PatternKind::Alternative(values) => {
            for value in values {
                collect_pattern_bindings(value, names);
            }
        }
        ast::PatternKind::TupleConstructor { arguments, .. } => {
            for argument in arguments {
                collect_pattern_bindings(argument, names);
            }
        }
        ast::PatternKind::Record { fields, .. } => {
            for field in fields {
                if let Some(value) = &field.pattern {
                    collect_pattern_bindings(value, names);
                } else {
                    names.insert(field.name.text.clone());
                }
            }
        }
        ast::PatternKind::Wildcard
        | ast::PatternKind::Literal(_)
        | ast::PatternKind::Constructor(_) => {}
    }
}

fn collect_calls_in_block(block: &ast::Block, scope: &Scope<'_>, output: &mut Vec<Reference>) {
    let mut visitor = |expression: &ast::Expression| {
        collect_calls(expression, scope, output);
    };
    visit_block(block, &mut visitor);
}

fn collect_calls(expression: &ast::Expression, scope: &Scope<'_>, output: &mut Vec<Reference>) {
    if let ast::ExpressionKind::Call { callee, arguments } = &expression.kind {
        if let Some((path, span)) = expression_path(callee)
            && let Some(target) = resolve_update(&path, scope)
        {
            output.push(Reference { target, span });
        } else {
            collect_calls(callee, scope, output);
        }
        for argument in arguments {
            match argument {
                ast::CallArgument::Expression(value) => collect_calls(value, scope, output),
                ast::CallArgument::Binder(value) => collect_calls(&value.body, scope, output),
            }
        }
        return;
    }
    match &expression.kind {
        ast::ExpressionKind::Block(block) => collect_calls_in_block(block, scope, output),
        ast::ExpressionKind::If(value) => {
            collect_calls(&value.condition, scope, output);
            collect_calls_in_block(&value.then_branch, scope, output);
            if let Some(branch) = &value.else_branch {
                match branch {
                    ast::ElseBranch::Block(block) => collect_calls_in_block(block, scope, output),
                    ast::ElseBranch::If(value) => collect_calls(value, scope, output),
                }
            }
        }
        ast::ExpressionKind::Match(value) => {
            collect_calls(&value.value, scope, output);
            for arm in &value.arms {
                collect_calls(&arm.value, scope, output);
            }
        }
        _ => {
            let mut visitor = |child: &ast::Expression| collect_calls(child, scope, output);
            visit_children(expression, &mut visitor);
        }
    }
}

fn visit_children(expression: &ast::Expression, visitor: &mut dyn FnMut(&ast::Expression)) {
    match &expression.kind {
        ast::ExpressionKind::Sequence(values) | ast::ExpressionKind::Tuple(values) => {
            for value in values {
                visitor(value);
            }
        }
        ast::ExpressionKind::Group(value)
        | ast::ExpressionKind::Unary { value, .. }
        | ast::ExpressionKind::Return(Some(value)) => visitor(value),
        ast::ExpressionKind::Record(value) => {
            for field in &value.fields {
                if let Some(value) = &field.value {
                    visitor(value);
                }
            }
            if let Some(base) = &value.base {
                visitor(base);
            }
        }
        ast::ExpressionKind::Call { callee, arguments } => {
            visitor(callee);
            for argument in arguments {
                match argument {
                    ast::CallArgument::Expression(value) => visitor(value),
                    ast::CallArgument::Binder(value) => visitor(&value.body),
                }
            }
        }
        ast::ExpressionKind::Member { value, .. } => visitor(value),
        ast::ExpressionKind::Index { value, index }
        | ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => {
            visitor(value);
            visitor(index);
        }
        ast::ExpressionKind::Is { value, .. } => visitor(value),
        ast::ExpressionKind::If(value) => {
            visitor(&value.condition);
            visit_block(&value.then_branch, visitor);
            if let Some(branch) = &value.else_branch {
                match branch {
                    ast::ElseBranch::Block(block) => visit_block(block, visitor),
                    ast::ElseBranch::If(value) => visitor(value),
                }
            }
        }
        ast::ExpressionKind::Match(value) => {
            visitor(&value.value);
            for arm in &value.arms {
                visitor(&arm.value);
            }
        }
        ast::ExpressionKind::Block(block) => visit_block(block, visitor),
        ast::ExpressionKind::Literal(_)
        | ast::ExpressionKind::Unit
        | ast::ExpressionKind::Name(_)
        | ast::ExpressionKind::Return(None) => {}
    }
}

fn visit_block(block: &ast::Block, visitor: &mut dyn FnMut(&ast::Expression)) {
    for statement in &block.statements {
        match &statement.kind {
            ast::StatementKind::Let { value, .. } | ast::StatementKind::Assign { value, .. } => {
                visitor(value)
            }
            ast::StatementKind::Emit { output, .. } => {
                for argument in &output.arguments {
                    visitor(argument);
                }
            }
            ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                visitor(condition);
                visitor(decreases);
                visit_block(body, visitor);
            }
            ast::StatementKind::Expression { expression, .. }
            | ast::StatementKind::BlockExpression(expression) => visitor(expression),
            ast::StatementKind::Unreachable { .. } => {}
        }
    }
    if let Some(tail) = &block.tail {
        visitor(tail);
    }
}
