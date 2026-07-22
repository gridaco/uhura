#![allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]

use std::collections::{BTreeMap, BTreeSet};

use super::diagnostic::{codes, error};
use super::types::{ConstructorInfo, Ty, TypeInfo, TypeRegistry, TypeShape, compatible, join};
use super::ui_catalog::{
    self, AttributeKind as UiAttributeKind, Availability as UiElementAvailability,
    Constraint as UiConstraint, ContentModel as UiContentModel, ElementContext as UiElementContext,
    EventContract as UiEventContract, EventPayload as UiEventPayload,
};
use crate::checker_ir as ast;
use uhura_base::{Diagnostic, has_errors};
use uhura_core::ir::{
    CommandDef, EvidenceRef as IrEvidenceRef, EvidenceStep as IrEvidenceStep, Handler as IrHandler,
    Machine as IrMachine, ObservationField, OutcomeDef, OutcomePolicy as IrOutcomePolicy, PortDef,
    Presentation, Scenario as IrScenario, ScenarioOrigin as IrScenarioOrigin, SourceRef,
    StateField, Statement, StatementMatchArm, Transition as IrTransition,
    UiAttribute as IrUiAttribute, UiAttributeValue as IrUiAttributeValue, UiCase as IrUiCase,
    UiNode as IrUiNode,
};
use uhura_core::{
    BinaryOp as IrBinaryOp, BoundaryNumber, ConstructorDef, Decimal, EvidenceExampleMetadata,
    EvidencePresentationKind, Expr as IrExpr, Function as IrFunction,
    INLINE_UPDATE_JOIN_LOCAL_PREFIX, INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX, MatchArm as IrMatchArm,
    PURE_CONTINUATION_LOCAL_PREFIX, Pattern as IrPattern, Program, TypeDef, TypeRef,
    UnaryOp as IrUnaryOp, Value,
};

#[derive(Clone, Debug)]
pub(crate) struct DeferredPresentation {
    pub(crate) module: String,
    pub(crate) declaration: ast::UiDecl,
    pub(crate) span: ast::SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct DeferredEvidence {
    pub(crate) module: String,
    pub(crate) declaration: ast::Declaration,
}

#[derive(Debug)]
pub struct CheckOutput {
    pub program: Option<Program>,
    pub diagnostics: Vec<Diagnostic>,
    /// Source-layout-sensitive semantic-node occurrences. Callers without
    /// source-layout metadata leave this empty.
    pub provenance: Option<uhura_core::Provenance>,
    /// Checked source-only authoring metadata. This never enters executable
    /// program identity or runtime behavior.
    pub authoring: crate::AuthoringProjection,
}

#[derive(Clone, Debug)]
enum Export {
    Type(TypeRef),
    Const {
        id: String,
        ty: TypeRef,
    },
    Function {
        id: String,
        params: Vec<TypeRef>,
        result: TypeRef,
    },
    Machine {
        id: String,
    },
    Presentation {
        id: String,
    },
    PortContract,
    PureHelper,
    UiElement,
}

#[derive(Clone, Debug)]
struct ModuleEnv<'a> {
    module: &'a ast::Module,
    id: String,
    physical_source_paths: BTreeMap<u32, String>,
    semantic_paths: BTreeMap<(u32, u32), String>,
    exports: BTreeMap<String, Export>,
    imports: BTreeMap<String, Export>,
    features: BTreeSet<String>,
}

impl ModuleEnv<'_> {
    fn lookup(&self, name: &str) -> Option<&Export> {
        self.exports.get(name).or_else(|| self.imports.get(name))
    }
}

#[derive(Clone, Debug)]
struct Binding {
    lowered: String,
    ty: Ty,
}

#[derive(Clone)]
struct DeferredPureContinuation {
    parameters: Vec<ast::Pattern>,
    body: ast::Expr,
    definition_scope: Scope,
    parameter_types: Option<Vec<Ty>>,
    result: TypeRef,
}

#[derive(Clone)]
struct CompiledPureContinuation {
    lambda: IrExpr,
    parameters: Vec<Ty>,
    result: Ty,
}

#[derive(Clone, Copy, Debug, Default)]
struct NumericBounds {
    min: Option<i64>,
    max: Option<i64>,
}

#[derive(Clone, Debug, Default)]
struct Scope {
    values: BTreeMap<String, Binding>,
    types: BTreeMap<String, TypeRef>,
    constructors: BTreeMap<String, Vec<ConstructorInfo>>,
    functions: BTreeMap<String, (String, Vec<TypeRef>, TypeRef)>,
    transitions: BTreeMap<String, (Vec<TypeRef>, String)>,
    state_fields: BTreeMap<String, TypeRef>,
    config_fields: BTreeMap<String, TypeRef>,
    port_receive: BTreeMap<String, ConstructorInfo>,
    port_send: BTreeMap<String, ConstructorInfo>,
    outcome_type: Option<TypeRef>,
    command_type: Option<TypeRef>,
    input_type: Option<TypeRef>,
    numeric_bounds: BTreeMap<String, NumericBounds>,
    less_equal: BTreeSet<(String, String)>,
    less_than: BTreeSet<(String, String)>,
}

impl Scope {
    fn child(&self) -> Self {
        self.clone()
    }

    fn bind(&mut self, source: impl Into<String>, lowered: impl Into<String>, ty: Ty) {
        let source = source.into();
        let lowered = lowered.into();
        let bounds = match ty.as_value() {
            Some(TypeRef::Nat) => Some(NumericBounds {
                min: Some(0),
                max: None,
            }),
            Some(TypeRef::PositiveInt) => Some(NumericBounds {
                min: Some(1),
                max: None,
            }),
            Some(TypeRef::Ratio) => Some(NumericBounds {
                min: Some(0),
                max: Some(1),
            }),
            _ => None,
        };
        self.values.insert(
            source,
            Binding {
                lowered: lowered.clone(),
                ty,
            },
        );
        self.numeric_bounds.remove(&lowered);
        if let Some(bounds) = bounds {
            self.numeric_bounds.insert(lowered, bounds);
        }
    }

    fn invalidate_path(&mut self, path: &str) {
        let mentions = |candidate: &str| {
            candidate == path
                || candidate
                    .strip_prefix(path)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        };
        self.numeric_bounds
            .retain(|candidate, _| !mentions(candidate));
        self.less_equal
            .retain(|(left, right)| !mentions(left) && !mentions(right));
        self.less_than
            .retain(|(left, right)| !mentions(left) && !mentions(right));
    }
}

pub(crate) type ImportAliases = BTreeMap<(String, String, String), String>;
pub(crate) type PhysicalSourcePaths = BTreeMap<u32, String>;

pub(crate) fn check_project_with_import_aliases(
    project: &ast::Project,
    import_aliases: &ImportAliases,
    physical_source_paths: &PhysicalSourcePaths,
) -> CheckOutput {
    let mut checker = Checker::new(project, import_aliases, physical_source_paths);
    checker.run()
}

struct Checker<'a> {
    project: &'a ast::Project,
    import_aliases: &'a ImportAliases,
    physical_source_paths: PhysicalSourcePaths,
    diagnostics: Vec<Diagnostic>,
    registry: TypeRegistry,
    modules: BTreeMap<String, ModuleEnv<'a>>,
    program: Program,
    presentations: Vec<DeferredPresentation>,
    evidence: Vec<DeferredEvidence>,
    pure_continuations: BTreeMap<String, DeferredPureContinuation>,
    lower_expr_depth: usize,
    draining_pure_continuations: bool,
}

impl<'a> Checker<'a> {
    fn new(
        project: &'a ast::Project,
        import_aliases: &'a ImportAliases,
        physical_source_paths: &PhysicalSourcePaths,
    ) -> Self {
        Self {
            project,
            import_aliases,
            physical_source_paths: physical_source_paths.clone(),
            diagnostics: Vec::new(),
            registry: TypeRegistry::default(),
            modules: BTreeMap::new(),
            program: Program::new(),
            presentations: Vec::new(),
            evidence: Vec::new(),
            pure_continuations: BTreeMap::new(),
            lower_expr_depth: 0,
            draining_pure_continuations: false,
        }
    }

    fn run(&mut self) -> CheckOutput {
        self.collect_modules();
        self.collect_imports();
        self.check_import_cycles();
        self.collect_type_shapes();
        self.collect_value_signatures();
        self.lower_declarations();
        self.lower_presentations();
        self.lower_evidence();
        self.check_recursion();

        self.diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.span.file.0,
                diagnostic.span.start,
                diagnostic.span.end,
                diagnostic.code,
            )
        });
        self.diagnostics.dedup_by(|left, right| {
            left.code == right.code && left.span == right.span && left.message == right.message
        });
        let program = if has_errors(&self.diagnostics) {
            None
        } else {
            // The current frontend still has to attach canonical provenance
            // and authored fault-site identities. It freezes the complete
            // executable artifact after that finalization step.
            Some(std::mem::take(&mut self.program))
        };
        CheckOutput {
            program,
            diagnostics: std::mem::take(&mut self.diagnostics),
            provenance: None,
            authoring: crate::AuthoringProjection::default(),
        }
    }

    fn collect_modules(&mut self) {
        for module in &self.project.modules {
            if module.language.name.value != "uhura" || module.language.version != "0.4" {
                self.diagnostics.push(error(
                    codes::HEADER,
                    "uhura/header",
                    format!(
                        "expected internal Uhura kernel version `0.4`, found `language {} {}`",
                        module.language.name.value, module.language.version
                    ),
                    module.language.span,
                ));
            }
            let logical = module.identity.logical_name();
            let id = format!("{}@{}", logical, module.identity.major);
            if self.modules.contains_key(&id) {
                self.diagnostics.push(error(
                    codes::MODULE,
                    "uhura/duplicate-module",
                    format!("module `{id}` occurs more than once"),
                    module.identity.span,
                ));
                continue;
            }
            let mut features = BTreeSet::new();
            for feature in &module.uses {
                if !matches!(feature.feature.value.as_str(), "ui" | "evidence") {
                    self.diagnostics.push(error(
                        codes::FEATURE,
                        "uhura/unknown-feature",
                        format!("unknown opt-in feature `{}`", feature.feature.value),
                        feature.feature.span,
                    ));
                } else if !features.insert(feature.feature.value.clone()) {
                    self.diagnostics.push(error(
                        codes::DUPLICATE,
                        "uhura/duplicate-feature",
                        format!("feature `{}` is enabled twice", feature.feature.value),
                        feature.feature.span,
                    ));
                }
            }
            self.program.machine_program.modules.push(id.clone());
            self.modules.insert(
                id.clone(),
                ModuleEnv {
                    module,
                    id,
                    physical_source_paths: self.physical_source_paths.clone(),
                    semantic_paths: semantic_path_index(module),
                    exports: BTreeMap::new(),
                    imports: BTreeMap::new(),
                    features,
                },
            );
        }

        let ids = self.modules.keys().cloned().collect::<Vec<_>>();
        for id in ids {
            let declarations = self.modules[&id].module.declarations.clone();
            let mut exports = BTreeMap::new();
            for declaration in declarations {
                let (name, export) = match &declaration.value {
                    ast::DeclarationKind::Key(value) => (
                        value.name.value.clone(),
                        Export::Type(TypeRef::Named {
                            id: qualify(&id, &value.name.value),
                        }),
                    ),
                    ast::DeclarationKind::Type(value) => (
                        value.name.value.clone(),
                        Export::Type(TypeRef::Named {
                            id: qualify(&id, &value.name.value),
                        }),
                    ),
                    ast::DeclarationKind::Const(value) => (
                        value.name.value.clone(),
                        Export::Const {
                            id: qualify(&id, &value.name.value),
                            ty: TypeRef::Never,
                        },
                    ),
                    ast::DeclarationKind::Function(value) => (
                        value.name.value.clone(),
                        Export::Function {
                            id: qualify(&id, &value.name.value),
                            params: Vec::new(),
                            result: TypeRef::Never,
                        },
                    ),
                    ast::DeclarationKind::Machine(value) => (
                        value.name.value.clone(),
                        Export::Machine {
                            id: qualify(&id, &value.name.value),
                        },
                    ),
                    ast::DeclarationKind::Ui(value) => (
                        value.name.value.clone(),
                        Export::Presentation {
                            id: qualify(&id, &value.name.value),
                        },
                    ),
                    ast::DeclarationKind::Scenario(_)
                    | ast::DeclarationKind::Example(_)
                    | ast::DeclarationKind::Checkpoint(_) => continue,
                };
                if exports.insert(name.clone(), export).is_some() {
                    self.diagnostics.push(error(
                        codes::DUPLICATE,
                        "uhura/duplicate-declaration",
                        format!("`{name}` is declared more than once in module `{id}`"),
                        declaration.span,
                    ));
                }
            }
            self.modules.get_mut(&id).expect("module").exports = exports;
        }
    }

    fn collect_imports(&mut self) {
        let module_ids = self.modules.keys().cloned().collect::<Vec<_>>();
        for module_id in module_ids {
            let imports = self.modules[&module_id].module.imports.clone();
            let mut resolved = BTreeMap::new();
            for import in imports {
                for name in import.names {
                    let local_name = self
                        .import_aliases
                        .get(&(module_id.clone(), import.target.clone(), name.value.clone()))
                        .cloned()
                        .unwrap_or_else(|| name.value.clone());
                    let export = if let Some(module) = self.modules.get(&import.target) {
                        module.exports.get(&name.value).cloned()
                    } else {
                        standard_export(&import.target, &name.value)
                    };
                    match export {
                        Some(export) => {
                            if self.modules[&module_id].exports.contains_key(&local_name)
                                || resolved.insert(local_name.clone(), export).is_some()
                            {
                                self.diagnostics.push(error(
                                    codes::DUPLICATE,
                                    "uhura/import-collision",
                                    format!(
                                        "imported name `{}` collides in this module",
                                        local_name
                                    ),
                                    name.span,
                                ));
                            }
                        }
                        None => self.diagnostics.push(error(
                            codes::IMPORT,
                            "uhura/unresolved-import",
                            format!(
                                "`{}` is not exported by module `{}`",
                                name.value, import.target
                            ),
                            name.span,
                        )),
                    }
                }
            }
            self.modules.get_mut(&module_id).expect("module").imports = resolved;
        }
    }

    fn check_import_cycles(&mut self) {
        fn visit(
            id: &str,
            modules: &BTreeMap<String, ModuleEnv<'_>>,
            active: &mut Vec<String>,
            complete: &mut BTreeSet<String>,
            cycles: &mut Vec<(String, ast::SourceSpan)>,
        ) {
            if complete.contains(id) {
                return;
            }
            active.push(id.to_string());
            if let Some(module) = modules.get(id) {
                for import in &module.module.imports {
                    if !modules.contains_key(&import.target) {
                        continue;
                    }
                    if active.iter().any(|active_id| active_id == &import.target) {
                        cycles.push((
                            format!(
                                "import cycle closes from `{id}` back to `{}`",
                                import.target
                            ),
                            import.span,
                        ));
                    } else {
                        visit(&import.target, modules, active, complete, cycles);
                    }
                }
            }
            active.pop();
            complete.insert(id.to_string());
        }

        let mut active = Vec::new();
        let mut complete = BTreeSet::new();
        let mut cycles = Vec::new();
        for id in self.modules.keys() {
            visit(id, &self.modules, &mut active, &mut complete, &mut cycles);
        }
        cycles.sort_by_key(|(_, span)| (span.file, span.start, span.end));
        cycles.dedup_by_key(|(_, span)| (span.file, span.start, span.end));
        for (message, span) in cycles {
            self.diagnostics.push(error(
                codes::DEPENDENCY_CYCLE,
                "uhura/import-cycle",
                message,
                span,
            ));
        }
    }

    fn collect_type_shapes(&mut self) {
        let module_ids = self.modules.keys().cloned().collect::<Vec<_>>();
        for module_id in module_ids {
            let module = self.modules[&module_id].clone();
            let scope = self.module_scope(&module);
            for declaration in &module.module.declarations {
                match &declaration.value {
                    ast::DeclarationKind::Key(key) => {
                        let underlying = self.resolve_type(&module, &scope, &key.over);
                        self.reject_persisted_finite_view(
                            &underlying,
                            key.over.span,
                            &format!("key `{}`", key.name.value),
                        );
                        let id = qualify(&module_id, &key.name.value);
                        self.registry.insert(TypeInfo {
                            id: id.clone(),
                            shape: TypeShape::Key(underlying.clone()),
                        });
                        self.program
                            .machine_program
                            .types
                            .insert(id.clone(), TypeDef::Key { id, underlying });
                    }
                    ast::DeclarationKind::Type(ty) => {
                        if !ty.parameters.is_empty() {
                            self.diagnostics.push(error(
                                codes::UNSUPPORTED,
                                "uhura/user-generic",
                                "user-declared generic types are reserved but are not supported",
                                declaration.span,
                            ));
                            continue;
                        }
                        let id = qualify(&module_id, &ty.name.value);
                        self.install_type_body(&module, &scope, &id, &ty.body, declaration.span);
                    }
                    _ => {}
                }
            }
        }
    }

    fn collect_value_signatures(&mut self) {
        let module_ids = self.modules.keys().cloned().collect::<Vec<_>>();
        for module_id in module_ids {
            let module = self.modules[&module_id].clone();
            let scope = self.module_scope(&module);
            let mut updates = Vec::new();
            for declaration in &module.module.declarations {
                match &declaration.value {
                    ast::DeclarationKind::Const(value) => {
                        let ty = self.resolve_type(&module, &scope, &value.ty);
                        updates.push((
                            value.name.value.clone(),
                            Export::Const {
                                id: qualify(&module_id, &value.name.value),
                                ty,
                            },
                        ));
                    }
                    ast::DeclarationKind::Function(value) => {
                        let params = value
                            .parameters
                            .iter()
                            .map(|parameter| self.resolve_type(&module, &scope, &parameter.ty))
                            .collect();
                        let result = self.resolve_type(&module, &scope, &value.result);
                        updates.push((
                            value.name.value.clone(),
                            Export::Function {
                                id: qualify(&module_id, &value.name.value),
                                params,
                                result,
                            },
                        ));
                    }
                    _ => {}
                }
            }
            for (name, export) in updates {
                self.modules
                    .get_mut(&module_id)
                    .expect("module")
                    .exports
                    .insert(name, export);
            }
        }
        // Imports carry value signatures, so refresh them after declaration
        // signatures have replaced their collection placeholders.
        self.collect_imports_refresh();
    }

    fn collect_imports_refresh(&mut self) {
        let module_ids = self.modules.keys().cloned().collect::<Vec<_>>();
        for module_id in module_ids {
            let imports = self.modules[&module_id].module.imports.clone();
            let mut resolved = BTreeMap::new();
            for import in imports {
                for name in import.names {
                    let local_name = self
                        .import_aliases
                        .get(&(module_id.clone(), import.target.clone(), name.value.clone()))
                        .cloned()
                        .unwrap_or_else(|| name.value.clone());
                    if let Some(export) = self
                        .modules
                        .get(&import.target)
                        .and_then(|module| module.exports.get(&name.value))
                        .cloned()
                        .or_else(|| standard_export(&import.target, &name.value))
                    {
                        resolved.insert(local_name, export);
                    }
                }
            }
            self.modules.get_mut(&module_id).expect("module").imports = resolved;
        }
    }

    fn lower_declarations(&mut self) {
        let roots = self
            .project
            .modules
            .iter()
            .map(|module| {
                format!(
                    "{}@{}",
                    module.identity.logical_name(),
                    module.identity.major
                )
            })
            .collect::<Vec<_>>();
        let module_ids = dependency_order(&self.modules, &roots);

        // Global constants are declarative values, not source-order
        // statements. Probe their already typed expressions to recover exact
        // resolved constant references, reject real cycles, then evaluate in
        // dependency order. The probe is diagnostic-free: authoritative
        // lowering runs once after all of a constant's dependencies exist.
        let mut constants = BTreeMap::new();
        for module_id in &module_ids {
            let module = self.modules[module_id].clone();
            for declaration in &module.module.declarations {
                if let ast::DeclarationKind::Const(value) = &declaration.value {
                    constants.insert(
                        qualify(&module.id, &value.name.value),
                        (module.id.clone(), value.clone(), declaration.span),
                    );
                }
            }
        }
        let constant_ids = constants.keys().cloned().collect::<BTreeSet<_>>();
        let mut constant_graph = BTreeMap::new();
        for (id, (module_id, declaration, _)) in &constants {
            let module = self.modules[module_id].clone();
            let scope = self.module_scope(&module);
            let expected = self.resolve_type(&module, &scope, &declaration.ty);
            let checkpoint = self.diagnostics.len();
            let (expression, _) = self.lower_expr(
                &module,
                &scope,
                &declaration.value,
                Some(&expected),
                ExprMode::Pure,
            );
            self.diagnostics.truncate(checkpoint);
            let mut dependencies = BTreeSet::new();
            collect_names(&expression, &mut dependencies);
            dependencies.retain(|name| constant_ids.contains(name));
            constant_graph.insert(id.clone(), dependencies);
        }
        let cyclic_constants = cyclic_nodes(&constant_graph);
        for id in &cyclic_constants {
            let (_, declaration, declaration_span) =
                constants.get(id).expect("cyclic constant is declared");
            self.diagnostics.push(error(
                codes::DEPENDENCY_CYCLE,
                "uhura/recursive-constant",
                format!(
                    "constant `{}` participates in a compile-time dependency cycle",
                    declaration.name.value
                ),
                *declaration_span,
            ));
        }
        for id in graph_dependency_order(&constant_graph, &cyclic_constants) {
            let (module_id, declaration, declaration_span) =
                constants.get(&id).expect("ordered constant is declared");
            let module = self.modules[module_id].clone();
            self.lower_global_const(&module, declaration, *declaration_span);
        }

        for module_id in module_ids {
            let module = self.modules[&module_id].clone();
            for declaration in &module.module.declarations {
                match &declaration.value {
                    ast::DeclarationKind::Const(_) => {}
                    ast::DeclarationKind::Function(value) => {
                        self.lower_global_function(&module, value, declaration.span)
                    }
                    ast::DeclarationKind::Machine(value) => {
                        self.lower_machine(&module, value, declaration.span)
                    }
                    ast::DeclarationKind::Ui(value) => {
                        self.presentations.push(DeferredPresentation {
                            module: module.id.clone(),
                            declaration: value.clone(),
                            span: declaration.span,
                        })
                    }
                    ast::DeclarationKind::Scenario(_)
                    | ast::DeclarationKind::Example(_)
                    | ast::DeclarationKind::Checkpoint(_) => self.evidence.push(DeferredEvidence {
                        module: module.id.clone(),
                        declaration: declaration.clone(),
                    }),
                    ast::DeclarationKind::Key(_) | ast::DeclarationKind::Type(_) => {}
                }
            }
        }
    }

    fn check_recursion(&mut self) {
        let functions = self.program.machine_program.functions.clone();
        let global_ids = functions.keys().cloned().collect::<BTreeSet<_>>();
        let mut global_graph = BTreeMap::new();
        for (id, function) in &functions {
            let mut calls = BTreeSet::new();
            collect_calls(&function.body, &mut calls);
            calls.retain(|call| global_ids.contains(call));
            global_graph.insert(id.clone(), calls);
        }
        for id in cyclic_nodes(&global_graph) {
            let function = &functions[&id];
            self.diagnostics.push(error(
                codes::DEPENDENCY_CYCLE,
                "uhura/recursive-function",
                format!(
                    "pure function `{id}` participates in a call cycle; Uhura functions must terminate"
                ),
                self.physical_span(&function.source),
            ));
        }

        let machines = self.program.machine_program.machines.clone();
        for (machine_id, machine) in machines {
            let function_ids = machine.functions.keys().cloned().collect::<BTreeSet<_>>();
            let mut function_graph = BTreeMap::new();
            for (name, function) in &machine.functions {
                let mut calls = BTreeSet::new();
                collect_calls(&function.body, &mut calls);
                calls.retain(|call| function_ids.contains(call));
                function_graph.insert(name.clone(), calls);
            }
            for name in cyclic_nodes(&function_graph) {
                let function = &machine.functions[&name];
                self.diagnostics.push(error(
                    codes::DEPENDENCY_CYCLE,
                    "uhura/recursive-machine-function",
                    format!("machine function `{machine_id}.{name}` participates in a call cycle"),
                    self.physical_span(&function.source),
                ));
            }

            let derive_ids = machine
                .derives
                .iter()
                .map(|(name, _, _, _)| name.clone())
                .collect::<BTreeSet<_>>();
            let mut derive_graph = BTreeMap::new();
            for (name, _, expression, _) in &machine.derives {
                let mut names = BTreeSet::new();
                collect_names(expression, &mut names);
                names.retain(|candidate| derive_ids.contains(candidate));
                derive_graph.insert(name.clone(), names);
            }
            for name in cyclic_nodes(&derive_graph) {
                let source = &machine
                    .derives
                    .iter()
                    .find(|(candidate, _, _, _)| candidate == &name)
                    .expect("cyclic derive exists")
                    .3;
                self.diagnostics.push(error(
                    codes::DEPENDENCY_CYCLE,
                    "uhura/recursive-derive",
                    format!("derive `{machine_id}.{name}` participates in a dependency cycle"),
                    self.physical_span(source),
                ));
            }
        }
    }

    fn physical_span(&self, source: &SourceRef) -> ast::SourceSpan {
        let file = self
            .modules
            .values()
            .find(|module| module.module.source_id.path == source.path)
            .map(|module| module.module.source_id.file)
            .unwrap_or(0);
        ast::SourceSpan::new(file, source.start, source.end)
    }

    fn module_scope(&self, module: &ModuleEnv<'_>) -> Scope {
        let mut scope = Scope::default();
        for (name, export) in module.exports.iter().chain(module.imports.iter()) {
            match export {
                Export::Type(ty) => {
                    scope.types.insert(name.clone(), ty.clone());
                }
                Export::Const { id, ty } => {
                    scope.bind(name, id, Ty::value(ty.clone()));
                }
                Export::Function { id, params, result } => {
                    scope
                        .functions
                        .insert(name.clone(), (id.clone(), params.clone(), result.clone()));
                }
                _ => {}
            }
        }
        scope
    }

    fn install_type_body(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        id: &str,
        body: &ast::TypeBody,
        span: ast::SourceSpan,
    ) {
        match body {
            ast::TypeBody::Alias(alias) => {
                let resolved = self.resolve_type(module, scope, alias);
                let shape = match &resolved {
                    TypeRef::Record { fields } => TypeShape::Record(fields.clone()),
                    _ => TypeShape::Alias(resolved.clone()),
                };
                self.registry.insert(TypeInfo {
                    id: id.into(),
                    shape,
                });
                if let TypeRef::Record { fields } = resolved {
                    self.program.machine_program.types.insert(
                        id.into(),
                        TypeDef::Record {
                            id: id.into(),
                            fields,
                        },
                    );
                }
            }
            ast::TypeBody::Sum(sum) => {
                let constructors = sum
                    .variants
                    .iter()
                    .map(|variant| self.lower_constructor_def(module, scope, variant))
                    .collect::<Vec<_>>();
                self.registry.insert(TypeInfo {
                    id: id.into(),
                    shape: TypeShape::Sum(constructors.clone()),
                });
                self.program.machine_program.types.insert(
                    id.into(),
                    TypeDef::Sum {
                        id: id.into(),
                        constructors,
                    },
                );
            }
        }
        if self.program.machine_program.types.contains_key(id)
            && !self.registry.types.contains_key(id)
        {
            self.diagnostics.push(error(
                codes::UNKNOWN_TYPE,
                "uhura/type-shape",
                format!("could not establish a type shape for `{id}`"),
                span,
            ));
        }
    }

    fn lower_constructor_def(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        variant: &ast::Variant,
    ) -> ConstructorDef {
        let fields = match &variant.payload {
            ast::VariantPayload::Unit => Vec::new(),
            ast::VariantPayload::Positional(values) => values
                .iter()
                .map(|ty| (None, self.resolve_type(module, scope, ty)))
                .collect(),
            ast::VariantPayload::Named(values) => values
                .iter()
                .map(|field| {
                    (
                        Some(field.name.value.clone()),
                        self.resolve_type(module, scope, &field.ty),
                    )
                })
                .collect(),
        };
        ConstructorDef {
            name: variant.name.value.clone(),
            fields,
        }
    }

    fn resolve_type(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        ty: &ast::TypeExpr,
    ) -> TypeRef {
        match &ty.value {
            ast::TypeExprKind::Record(fields) => TypeRef::Record {
                fields: fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.value.clone(),
                            self.resolve_type(module, scope, &field.ty),
                        )
                    })
                    .collect(),
            },
            ast::TypeExprKind::Tuple(values) => TypeRef::Tuple {
                values: values
                    .iter()
                    .map(|value| self.resolve_type(module, scope, value))
                    .collect(),
            },
            ast::TypeExprKind::Named { path, arguments } => {
                let name = path
                    .iter()
                    .map(|part| part.value.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let args = arguments
                    .iter()
                    .map(|argument| self.resolve_type(module, scope, argument))
                    .collect::<Vec<_>>();
                if let Some(value) = builtin_type(&name, &args) {
                    if let TypeRef::Table { key, .. } = &value
                        && self.registry.finite_constructors(key).is_none()
                    {
                        self.diagnostics.push(error(
                            codes::NOT_TOTAL,
                            "uhura/table-key-not-finite",
                            format!(
                                "`Table` key `{}` must be a closed finite constructor type",
                                key.canonical_name()
                            ),
                            ty.span,
                        ));
                    }
                    return value;
                }
                if path.len() == 1 {
                    if let Some(value) = scope.types.get(&name) {
                        if !args.is_empty() {
                            self.diagnostics.push(error(
                                codes::ARITY,
                                "uhura/type-arity",
                                format!("type `{name}` does not accept type arguments"),
                                ty.span,
                            ));
                        }
                        return value.clone();
                    }
                    if matches!(name.as_str(), "Token" | "Routes") && args.len() == 1 {
                        return TypeRef::Named {
                            id: format!("{name}<{}>", args[0].canonical_name()),
                        };
                    }
                }
                self.diagnostics.push(error(
                    codes::UNKNOWN_TYPE,
                    "uhura/unknown-type",
                    format!("unknown type `{name}` in module `{}`", module.id),
                    ty.span,
                ));
                TypeRef::Never
            }
        }
    }

    fn reject_persisted_finite_view(
        &mut self,
        ty: &TypeRef,
        span: ast::SourceSpan,
        boundary: &str,
    ) {
        let Some(path) = finite_view_path(ty, &self.registry, &mut BTreeSet::new()) else {
            return;
        };
        self.diagnostics.push(error(
            codes::TYPE_MISMATCH,
            "uhura-0.4/ephemeral-finite-view",
            format!(
                "`FiniteView` is an ephemeral evaluator view and cannot appear in {boundary}; nested path: `{}`",
                path.join(" -> ")
            ),
            span,
        ));
    }

    fn reject_persisted_constructor_finite_views(
        &mut self,
        constructor: &ConstructorDef,
        variant: &ast::Variant,
        boundary: &str,
    ) {
        let spans = match &variant.payload {
            ast::VariantPayload::Unit => Vec::new(),
            ast::VariantPayload::Positional(values) => {
                values.iter().map(|value| value.span).collect()
            }
            ast::VariantPayload::Named(fields) => {
                fields.iter().map(|field| field.ty.span).collect()
            }
        };
        for (index, ((name, ty), span)) in constructor.fields.iter().zip(spans).enumerate() {
            let field = name.as_ref().map_or_else(
                || format!("positional field #{}", index + 1),
                |name| format!("field `{name}`"),
            );
            self.reject_persisted_finite_view(
                ty,
                span,
                &format!("{boundary} constructor `{}` {field}", constructor.name),
            );
        }
    }

    // Remaining lowering methods are kept below the expression engine so the
    // semantic context is explicit at each call site.
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExprMode {
    Pure,
    Projection,
    Reaction,
    Ui,
    Evidence,
}

impl Checker<'_> {
    fn try_compile_routes(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        declaration: &ast::ConstDecl,
        expected: &TypeRef,
    ) -> bool {
        let TypeRef::Named { id: routes_id } = expected else {
            return false;
        };
        if !routes_id.starts_with("Routes<") {
            return false;
        }
        let ast::TypeExprKind::Named { path, arguments } = &declaration.ty.value else {
            return false;
        };
        if path.last().map(|name| name.value.as_str()) != Some("Routes") || arguments.len() != 1 {
            return false;
        }
        let location = self.resolve_type(module, scope, &arguments[0]);
        let ast::ExprKind::Call {
            callee,
            arguments: route_args,
        } = &declaration.value.value
        else {
            self.diagnostics.push(error(
                codes::PORT,
                "uhura/routes-value",
                "`Routes<Location>` constants must be initialized by `routes({...})`",
                declaration.value.span,
            ));
            return true;
        };
        if !matches!(&callee.value, ast::ExprKind::Name(name) if name.value == "routes")
            || route_args.len() != 1
        {
            self.diagnostics.push(error(
                codes::PORT,
                "uhura/routes-value",
                "route table initialization must call `routes` with one record",
                declaration.value.span,
            ));
            return true;
        }
        let ast::ExprKind::Record(entries) = &route_args[0].value else {
            self.diagnostics.push(error(
                codes::PORT,
                "uhura/routes-value",
                "`routes` requires a constructor-to-pattern record",
                route_args[0].span,
            ));
            return true;
        };
        let constructors = self
            .registry
            .constructors_for(&location)
            .into_iter()
            .map(|constructor| {
                let fields = constructor
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(index, (name, ty))| {
                        let name = name.clone().unwrap_or_else(|| format!("_{index}"));
                        self.route_field(module, &name, ty, declaration.value.span)
                    })
                    .collect();
                uhura_port::RouteConstructorDecl::new(constructor.name, fields)
            })
            .collect::<Vec<_>>();
        let patterns = entries
            .iter()
            .filter_map(|entry| {
                let constructor = record_key(&entry.key)?;
                let ast::ExprKind::Text(pattern) = &entry.value.value else {
                    self.diagnostics.push(error(
                        codes::PORT,
                        "uhura/route-pattern-literal",
                        "route patterns must be text literals",
                        entry.value.span,
                    ));
                    return None;
                };
                Some(uhura_port::RoutePatternDecl::new(
                    constructor,
                    pattern.clone(),
                ))
            })
            .collect::<Vec<_>>();
        match uhura_port::RouteTable::compile(port_ty(&location), constructors, patterns) {
            Ok(routes) => {
                let id = qualify(&module.id, &declaration.name.value);
                let canonical = uhura_base::to_canonical_json(
                    &serde_json::to_value(&routes)
                        .expect("a checked Uhura route table is serializable"),
                );
                // `Routes<Location>` is a checked host configuration, not an
                // ordinary Uhura collection. Keep its executable structure in
                // `route_tables`, while exposing the canonical immutable value
                // through constants so port configuration and evidence fixture
                // expressions evaluate through the same ordinary name lookup.
                self.program
                    .machine_program
                    .constants
                    .insert(id.clone(), Value::Text(canonical));
                self.program
                    .machine_program
                    .constant_types
                    .insert(id.clone(), expected.clone());
                self.program.route_tables.insert(id, routes);
            }
            Err(route_error) => self.diagnostics.push(error(
                codes::PORT,
                "uhura/invalid-route-table",
                route_error.to_string(),
                declaration.value.span,
            )),
        }
        true
    }

    fn route_field(
        &mut self,
        _module: &ModuleEnv<'_>,
        name: &str,
        ty: &TypeRef,
        span: ast::SourceSpan,
    ) -> uhura_port::RouteFieldDecl {
        use uhura_port::RouteFieldKind;
        let kind = match ty {
            TypeRef::Text => RouteFieldKind::Text,
            TypeRef::Named { .. } if self.key_is_text(ty) => RouteFieldKind::TextKey {
                type_name: port_ty(ty),
            },
            TypeRef::Option { value } if matches!(value.as_ref(), TypeRef::Text) => {
                RouteFieldKind::OptionalText
            }
            TypeRef::Option { value } if self.key_is_text(value) => {
                RouteFieldKind::OptionalTextKey {
                    type_name: port_ty(value),
                }
            }
            _ => {
                self.diagnostics.push(error(
                    codes::PORT,
                    "uhura/route-field-type",
                    format!(
                        "route field `{name}` must be Text, a Text key, or an optional form; found `{}`",
                        ty.canonical_name()
                    ),
                    span,
                ));
                RouteFieldKind::Text
            }
        };
        uhura_port::RouteFieldDecl::new(name, kind)
    }

    fn key_is_text(&self, ty: &TypeRef) -> bool {
        match self.registry.shape(ty) {
            Some(TypeShape::Key(TypeRef::Text)) => true,
            Some(TypeShape::Alias(alias)) => self.key_is_text(alias),
            _ => false,
        }
    }

    fn lower_global_const(
        &mut self,
        module: &ModuleEnv<'_>,
        declaration: &ast::ConstDecl,
        span: ast::SourceSpan,
    ) {
        let scope = self.module_scope(module);
        let expected = self.resolve_type(module, &scope, &declaration.ty);
        self.reject_persisted_finite_view(
            &expected,
            declaration.ty.span,
            &format!("constant `{}`", declaration.name.value),
        );
        if self.try_compile_routes(module, &scope, declaration, &expected) {
            return;
        }
        let (expression, actual) = self.lower_expr(
            module,
            &scope,
            &declaration.value,
            Some(&expected),
            ExprMode::Pure,
        );
        self.expect_type(&actual, &expected, declaration.value.span);
        match const_eval(&expression, &self.program) {
            Ok(value) => {
                let id = qualify(&module.id, &declaration.name.value);
                self.program
                    .machine_program
                    .constants
                    .insert(id.clone(), value);
                self.program
                    .machine_program
                    .constant_types
                    .insert(id, expected);
            }
            Err(message) => self.diagnostics.push(error(
                codes::EFFECT,
                "uhura/non-constant-expression",
                format!(
                    "constant `{}` is not compile-time total: {message}",
                    declaration.name.value
                ),
                span,
            )),
        }
    }

    fn lower_global_function(
        &mut self,
        module: &ModuleEnv<'_>,
        declaration: &ast::FunctionDecl,
        span: ast::SourceSpan,
    ) {
        let mut scope = self.module_scope(module);
        let params = declaration
            .parameters
            .iter()
            .map(|parameter| {
                let ty = self.resolve_type(module, &scope, &parameter.ty);
                scope.bind(
                    &parameter.name.value,
                    &parameter.name.value,
                    Ty::value(ty.clone()),
                );
                (parameter.name.value.clone(), ty)
            })
            .collect::<Vec<_>>();
        let result = self.resolve_type(module, &scope, &declaration.result);
        if reaction_control(&declaration.body) {
            self.diagnostics.push(error(
                codes::EFFECT,
                "uhura/effect-in-pure-function",
                format!(
                    "pure function `{}` contains reaction control (`finish`, `unreachable`, or a reaction block)",
                    declaration.name.value
                ),
                declaration.body.span,
            ));
            return;
        }
        let (body, actual) = self.lower_expr(
            module,
            &scope,
            &declaration.body,
            Some(&result),
            ExprMode::Pure,
        );
        self.expect_type(&actual, &result, declaration.body.span);
        let id = qualify(&module.id, &declaration.name.value);
        self.program.machine_program.functions.insert(
            id.clone(),
            IrFunction {
                id,
                params,
                result,
                body,
                source: source(module, span),
            },
        );
    }

    fn lower_expr(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        expression: &ast::Expr,
        expected: Option<&TypeRef>,
        mode: ExprMode,
    ) -> (IrExpr, Ty) {
        let root = self.lower_expr_depth == 0 && !self.draining_pure_continuations;
        self.lower_expr_depth += 1;
        let (mut value, ty) = self.lower_expr_inner(module, scope, expression, expected, mode);
        self.lower_expr_depth -= 1;
        if root {
            value = self.drain_pure_continuations(module, mode, value);
        }
        (value, ty)
    }

    fn lower_expr_inner(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        expression: &ast::Expr,
        expected: Option<&TypeRef>,
        mode: ExprMode,
    ) -> (IrExpr, Ty) {
        let (value, ty) = match &expression.value {
            ast::ExprKind::Integer(text) => {
                let target = expected.filter(|value| {
                    matches!(
                        value,
                        TypeRef::Int
                            | TypeRef::Nat
                            | TypeRef::PositiveInt
                            | TypeRef::Decimal
                            | TypeRef::Ratio
                            | TypeRef::BoundaryNumber
                    )
                });
                let ty = target.cloned().unwrap_or(TypeRef::Int);
                match exact_number_value(text, &ty) {
                    Ok(value) => (IrExpr::Literal { value }, Ty::value(ty)),
                    Err(message) => {
                        self.diagnostics.push(error(
                            codes::INVALID_REFINEMENT,
                            "uhura/number-refinement",
                            message,
                            expression.span,
                        ));
                        (
                            IrExpr::Literal {
                                value: exact_integer("0", "Int").expect("zero"),
                            },
                            Ty::Unknown,
                        )
                    }
                }
            }
            ast::ExprKind::Decimal(text) => {
                let ty = expected
                    .filter(|value| {
                        matches!(
                            value,
                            TypeRef::Decimal | TypeRef::Ratio | TypeRef::BoundaryNumber
                        )
                    })
                    .cloned()
                    .unwrap_or(TypeRef::Decimal);
                match exact_number_value(text, &ty) {
                    Ok(value) => (IrExpr::Literal { value }, Ty::value(ty)),
                    Err(message) => {
                        self.diagnostics.push(error(
                            codes::INVALID_REFINEMENT,
                            "uhura/number-refinement",
                            message,
                            expression.span,
                        ));
                        (
                            IrExpr::Literal {
                                value: exact_decimal("0").expect("zero"),
                            },
                            Ty::Unknown,
                        )
                    }
                }
            }
            ast::ExprKind::Text(value) => (
                IrExpr::Literal {
                    value: Value::Text(value.clone()),
                },
                Ty::value(TypeRef::Text),
            ),
            ast::ExprKind::Bool(value) => (
                IrExpr::Literal {
                    value: Value::Bool(*value),
                },
                Ty::value(TypeRef::Bool),
            ),
            ast::ExprKind::Name(name) => self.lower_name(module, scope, name, expected),
            ast::ExprKind::Tuple(values) => {
                let expected_values = match expected {
                    Some(TypeRef::Tuple { values }) => Some(values.as_slice()),
                    _ => None,
                };
                let lowered = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        self.lower_expr(
                            module,
                            scope,
                            value,
                            expected_values.and_then(|values| values.get(index)),
                            mode,
                        )
                    })
                    .collect::<Vec<_>>();
                (
                    IrExpr::Tuple {
                        values: lowered.iter().map(|(value, _)| value.clone()).collect(),
                    },
                    Ty::value(TypeRef::Tuple {
                        values: lowered
                            .into_iter()
                            .map(|(_, ty)| ty.into_value().unwrap_or(TypeRef::Never))
                            .collect(),
                    }),
                )
            }
            ast::ExprKind::Sequence(values) => {
                let item_expected = match expected {
                    Some(TypeRef::Seq { value })
                    | Some(TypeRef::NonEmpty { value })
                    | Some(TypeRef::Set { value }) => Some(value.as_ref()),
                    _ => None,
                };
                let lowered = values
                    .iter()
                    .map(|value| self.lower_expr(module, scope, value, item_expected, mode))
                    .collect::<Vec<_>>();
                let item_ty = lowered
                    .iter()
                    .map(|(_, ty)| ty.clone())
                    .reduce(|left, right| join(&left, &right))
                    .and_then(Ty::into_value)
                    .or_else(|| item_expected.cloned())
                    .unwrap_or(TypeRef::Never);
                (
                    IrExpr::Seq {
                        values: lowered.into_iter().map(|(value, _)| value).collect(),
                    },
                    Ty::value(TypeRef::Seq {
                        value: Box::new(item_ty),
                    }),
                )
            }
            ast::ExprKind::Record(entries) => {
                self.lower_record(module, scope, entries, expected, mode, expression.span)
            }
            ast::ExprKind::Unary { op, operand } => {
                let (value, ty) = self.lower_expr(module, scope, operand, None, mode);
                let op = match op.value {
                    ast::UnaryOp::Not => IrUnaryOp::Not,
                    ast::UnaryOp::Negate => IrUnaryOp::Negate,
                };
                let result = match (op, ty.as_value()) {
                    (IrUnaryOp::Not, Some(TypeRef::Bool)) => Ty::value(TypeRef::Bool),
                    (
                        IrUnaryOp::Negate,
                        Some(TypeRef::Int | TypeRef::Nat | TypeRef::PositiveInt),
                    ) => Ty::value(TypeRef::Int),
                    (IrUnaryOp::Negate, Some(TypeRef::Decimal)) => Ty::value(TypeRef::Decimal),
                    (IrUnaryOp::Negate, Some(TypeRef::BoundaryNumber)) => {
                        Ty::value(TypeRef::BoundaryNumber)
                    }
                    (IrUnaryOp::Negate, Some(TypeRef::Ratio)) => {
                        self.diagnostics.push(error(
                            codes::INVALID_REFINEMENT,
                            "uhura/ratio-negation",
                            "negating a `Ratio` escapes [0,1]",
                            expression.span,
                        ));
                        Ty::value(TypeRef::Ratio)
                    }
                    _ => {
                        self.diagnostics.push(error(
                            codes::TYPE_MISMATCH,
                            "uhura/invalid-unary-operand",
                            format!("invalid operand `{}` for unary operator", ty.display()),
                            operand.span,
                        ));
                        Ty::Unknown
                    }
                };
                (
                    IrExpr::Unary {
                        op,
                        value: Box::new(value),
                    },
                    result,
                )
            }
            ast::ExprKind::Binary { left, op, right } => {
                let op = lower_binary(op.value);
                if matches!(op, IrBinaryOp::And | IrBinaryOp::Or) {
                    let left_source = left.as_ref();
                    let (left_ir, refined) = self.lower_condition(module, scope, left_source, mode);
                    let false_scope;
                    let right_scope = if op == IrBinaryOp::And {
                        &refined
                    } else {
                        false_scope =
                            refined_numeric_scope(scope, left_source, false, &self.registry);
                        &false_scope
                    };
                    let right_span = right.span;
                    let (right, right_ty) =
                        self.lower_expr(module, right_scope, right, Some(&TypeRef::Bool), mode);
                    self.expect_type(&right_ty, &TypeRef::Bool, right_span);
                    (
                        IrExpr::Binary {
                            op,
                            left: Box::new(left_ir),
                            right: Box::new(right),
                        },
                        Ty::value(TypeRef::Bool),
                    )
                } else {
                    // A contextual result refinement applies after the binary
                    // operation, not independently to each operand. In
                    // particular, `let serial: PositiveInt = counter + 1`
                    // must add while `counter` is still `Nat`; refining the
                    // zero-valued counter before addition would make a total
                    // expression fail spuriously.
                    let (left_ir, left_ty) = self.lower_expr(module, scope, left, None, mode);
                    let right_expected = left_ty.as_value().filter(|ty| {
                        !matches!(ty, TypeRef::Int | TypeRef::Nat | TypeRef::PositiveInt)
                    });
                    let (right_ir, right_ty) =
                        self.lower_expr(module, scope, right, right_expected, mode);
                    if !compatible(&left_ty, &right_ty) {
                        self.type_mismatch(&right_ty, &left_ty, right.span);
                    }
                    let result = if matches!(
                        op,
                        IrBinaryOp::Equal
                            | IrBinaryOp::NotEqual
                            | IrBinaryOp::Less
                            | IrBinaryOp::LessEqual
                            | IrBinaryOp::Greater
                            | IrBinaryOp::GreaterEqual
                    ) {
                        Ty::value(TypeRef::Bool)
                    } else {
                        self.arithmetic_result_type(
                            scope,
                            op,
                            &left_ir,
                            &left_ty,
                            &right_ir,
                            &right_ty,
                            expression.span,
                        )
                    };
                    (
                        IrExpr::Binary {
                            op,
                            left: Box::new(left_ir),
                            right: Box::new(right_ir),
                        },
                        result,
                    )
                }
            }
            ast::ExprKind::Is { value, pattern } => {
                let (value_ir, value_ty) = self.lower_expr(module, scope, value, None, mode);
                let mut child = scope.child();
                let pattern = self.lower_pattern(
                    module,
                    &mut child,
                    pattern,
                    value_ty.as_value(),
                    PatternUse::Condition,
                );
                (
                    IrExpr::Is {
                        value: Box::new(value_ir),
                        pattern,
                    },
                    Ty::value(TypeRef::Bool),
                )
            }
            ast::ExprKind::Call { callee, arguments } => self.lower_call(
                module,
                scope,
                callee,
                arguments,
                expected,
                mode,
                expression.span,
            ),
            ast::ExprKind::Member { receiver, member } => {
                self.lower_member(module, scope, receiver, member, mode)
            }
            ast::ExprKind::Index { receiver, index } => {
                let (value, receiver_ty) = self.lower_expr(module, scope, receiver, None, mode);
                let (key_ty, value_ty) = match receiver_ty.as_value() {
                    Some(TypeRef::Table { key, value }) => {
                        (key.as_ref().clone(), value.as_ref().clone())
                    }
                    Some(TypeRef::Map { key, value }) => {
                        let (code, rule, message) = if mode == ExprMode::Projection {
                            (
                                codes::PROJECTION_NOT_TOTAL,
                                "uhura/projection-partial-index",
                                "invariants, derives, and observations must be total and fault-free; use `Map.get` and handle the returned `Option` explicitly",
                            )
                        } else {
                            (
                                codes::PARTIAL_OPERATION,
                                "uhura/partial-index",
                                "only finite `Table<K,V>` supports `value[key]`; use `Map.get` for maps",
                            )
                        };
                        self.diagnostics
                            .push(error(code, rule, message, receiver.span));
                        (key.as_ref().clone(), value.as_ref().clone())
                    }
                    _ => {
                        self.diagnostics.push(error(
                            codes::PARTIAL_OPERATION,
                            "uhura/partial-index",
                            "only finite `Table<K,V>` supports `value[key]`; use `Map.get` for maps",
                            receiver.span,
                        ));
                        (TypeRef::Never, TypeRef::Never)
                    }
                };
                let (key, actual_key) = self.lower_expr(module, scope, index, Some(&key_ty), mode);
                self.expect_type(&actual_key, &key_ty, index.span);
                (
                    IrExpr::Index {
                        value: Box::new(value),
                        key: Box::new(key),
                    },
                    Ty::value(value_ty),
                )
            }
            ast::ExprKind::Update { base, fields } => {
                let (base_ir, base_ty) = self.lower_expr(module, scope, base, expected, mode);
                let record_fields = base_ty
                    .as_value()
                    .and_then(|ty| self.registry.fields(ty))
                    .unwrap_or_default();
                let mut lowered = Vec::new();
                for field in fields {
                    let Some(name) = record_key(&field.key) else {
                        self.diagnostics.push(error(
                            codes::TYPE_MISMATCH,
                            "uhura/record-field-name",
                            "record update fields must be identifiers",
                            field.key.span,
                        ));
                        continue;
                    };
                    let expected = record_fields
                        .iter()
                        .find(|(field, _)| field == &name)
                        .map(|(_, ty)| ty);
                    if expected.is_none() {
                        self.diagnostics.push(error(
                            codes::UNKNOWN_NAME,
                            "uhura/unknown-record-field",
                            format!("record update names unknown field `{name}`"),
                            field.key.span,
                        ));
                    }
                    let (value, actual) =
                        self.lower_expr(module, scope, &field.value, expected, mode);
                    if let Some(expected) = expected {
                        self.expect_type(&actual, expected, field.value.span);
                    }
                    lowered.push((name, value));
                }
                (
                    IrExpr::Update {
                        value: Box::new(base_ir),
                        fields: lowered,
                    },
                    base_ty,
                )
            }
            ast::ExprKind::Lambda { parameters, body } => {
                // Lambda argument types are supplied by a total collection
                // method. A free-standing lambda has no stable first-order IR
                // type and is rejected by the caller if it escapes.
                let mut child = scope.child();
                let mut params = Vec::new();
                for pattern in parameters {
                    params.extend(self.lower_lambda_pattern(&mut child, pattern, None));
                }
                let (body, result) = self.lower_expr(module, &child, body, None, mode);
                (
                    IrExpr::Lambda {
                        params,
                        body: Box::new(body),
                    },
                    Ty::Function(Vec::new(), Box::new(result)),
                )
            }
            ast::ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if reaction_control(expression) && mode == ExprMode::Reaction {
                    self.diagnostics.push(error(
                        codes::EFFECT,
                        "uhura/reaction-value-control",
                        "reaction control must be lowered in statement position",
                        expression.span,
                    ));
                }
                let (condition_ir, refined) = self.lower_condition(module, scope, condition, mode);
                let else_scope = refined_numeric_scope(scope, condition, false, &self.registry);
                let (then_value, then_ty) =
                    self.lower_expr(module, &refined, then_branch, expected, mode);
                let (else_value, else_ty) = if let Some(else_branch) = else_branch {
                    self.lower_expr(module, &else_scope, else_branch, expected, mode)
                } else {
                    (
                        IrExpr::Literal { value: Value::Unit },
                        Ty::value(TypeRef::Unit),
                    )
                };
                if !compatible(&then_ty, &else_ty) {
                    self.type_mismatch(&else_ty, &then_ty, expression.span);
                }
                (
                    IrExpr::If {
                        condition: Box::new(condition_ir),
                        then_value: Box::new(then_value),
                        else_value: Box::new(else_value),
                    },
                    join(&then_ty, &else_ty),
                )
            }
            ast::ExprKind::Match { subject, arms } => self.lower_value_match(
                module,
                scope,
                subject,
                arms,
                expected,
                mode,
                expression.span,
            ),
            ast::ExprKind::Collect(clauses) => {
                let expected_item = match expected {
                    Some(TypeRef::Seq { value }) => Some(value.as_ref()),
                    _ => None,
                };
                let lowered = clauses
                    .iter()
                    .map(|clause| {
                        let (condition, refined) =
                            self.lower_condition(module, scope, &clause.condition, mode);
                        let (value, actual) =
                            self.lower_expr(module, &refined, &clause.value, expected_item, mode);
                        if let Some(expected) = expected_item {
                            self.expect_type(&actual, expected, clause.value.span);
                        }
                        (condition, value, actual)
                    })
                    .collect::<Vec<_>>();
                let item = lowered
                    .iter()
                    .map(|(_, _, ty)| ty.clone())
                    .reduce(|left, right| join(&left, &right))
                    .and_then(Ty::into_value)
                    .or_else(|| expected_item.cloned())
                    .unwrap_or(TypeRef::Never);
                (
                    IrExpr::Collect {
                        clauses: lowered
                            .into_iter()
                            .map(|(condition, value, _)| (condition, value))
                            .collect(),
                    },
                    Ty::value(TypeRef::Seq {
                        value: Box::new(item),
                    }),
                )
            }
            ast::ExprKind::SetComprehension {
                binding,
                source: collection,
                filters,
                value,
            } => {
                let (source_ir, source_ty) = self.lower_expr(module, scope, collection, None, mode);
                let item_ty = collection_item_type(source_ty.as_value()).unwrap_or(TypeRef::Never);
                let mut child = scope.child();
                let pattern = self.lower_pattern(
                    module,
                    &mut child,
                    binding,
                    Some(&item_ty),
                    PatternUse::Binding,
                );
                let mut conditions = Vec::new();
                for filter in filters {
                    let (condition, refined) = self.lower_condition(module, &child, filter, mode);
                    child = refined;
                    conditions.push(condition);
                }
                let expected_item = match expected {
                    Some(TypeRef::Set { value }) => Some(value.as_ref()),
                    _ => None,
                };
                let (value, result) = self.lower_expr(module, &child, value, expected_item, mode);
                let result_type = TypeRef::Set {
                    value: Box::new(
                        result
                            .into_value()
                            .or_else(|| expected_item.cloned())
                            .unwrap_or(TypeRef::Never),
                    ),
                };
                (
                    IrExpr::SetComprehension {
                        pattern,
                        source: Box::new(source_ir),
                        conditions,
                        value: Box::new(value),
                        result_type: result_type.clone(),
                    },
                    Ty::value(result_type),
                )
            }
            ast::ExprKind::Block(block) if mode != ExprMode::Reaction => {
                self.lower_pure_block(module, scope, block, expected, mode)
            }
            ast::ExprKind::Block(_) | ast::ExprKind::Finish(_) | ast::ExprKind::Unreachable => {
                self.diagnostics.push(error(
                    codes::EFFECT,
                    "uhura/reaction-control-in-value",
                    "reaction block/terminal control is only valid in a handler, transition, or `before commit` body",
                    expression.span,
                ));
                (IrExpr::Literal { value: Value::Unit }, Ty::Never)
            }
            ast::ExprKind::Error => {
                self.diagnostics.push(error(
                    codes::UNSUPPORTED,
                    "uhura/error-expression",
                    "cannot lower a recovered parser error expression",
                    expression.span,
                ));
                (IrExpr::Literal { value: Value::Unit }, Ty::Unknown)
            }
        };
        if let Some(expected) = expected {
            self.coerce(scope, value, ty, expected, expression.span)
        } else {
            (value, ty)
        }
    }

    fn lower_name(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        name: &ast::Name,
        expected: Option<&TypeRef>,
    ) -> (IrExpr, Ty) {
        if let Some(binding) = scope.values.get(&name.value) {
            return (
                IrExpr::Name {
                    name: binding.lowered.clone(),
                },
                binding.ty.clone(),
            );
        }
        if let Ok(constructor) = self.resolve_constructor(scope, &name.value, expected)
            && constructor.fields.is_empty()
        {
            return (
                IrExpr::Constructor {
                    type_id: constructor.type_id.clone(),
                    constructor: constructor.name,
                    fields: Vec::new(),
                },
                Ty::value(TypeRef::Named {
                    id: constructor.type_id,
                }),
            );
        }
        self.diagnostics.push(error(
            codes::UNKNOWN_NAME,
            "uhura/unknown-name",
            format!(
                "unknown value or nullary constructor `{}` in `{}`",
                name.value, module.id
            ),
            name.span,
        ));
        (IrExpr::Literal { value: Value::Unit }, Ty::Unknown)
    }

    fn lower_pure_block(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        block: &ast::Block,
        expected: Option<&TypeRef>,
        mode: ExprMode,
    ) -> (IrExpr, Ty) {
        self.lower_pure_block_at(
            module,
            scope,
            &block.statements,
            0,
            expected,
            mode,
            block.span,
        )
    }

    fn lower_pure_block_at(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        statements: &[ast::Statement],
        index: usize,
        expected: Option<&TypeRef>,
        mode: ExprMode,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        let Some(statement) = statements.get(index) else {
            return (
                IrExpr::Literal { value: Value::Unit },
                Ty::value(TypeRef::Unit),
            );
        };
        match &statement.value {
            ast::StatementKind::Let {
                name,
                ty: None,
                value:
                    ast::Spanned {
                        value: ast::ExprKind::Lambda { parameters, body },
                        ..
                    },
            } if name.value.starts_with(PURE_CONTINUATION_LOCAL_PREFIX) => {
                self.pure_continuations.insert(
                    name.value.clone(),
                    DeferredPureContinuation {
                        parameters: parameters.clone(),
                        body: body.as_ref().clone(),
                        definition_scope: scope.clone(),
                        parameter_types: None,
                        result: expected.cloned().unwrap_or(TypeRef::Never),
                    },
                );
                let result_type = expected.cloned().unwrap_or(TypeRef::Never);
                let mut child = scope.child();
                child.bind(
                    &name.value,
                    &name.value,
                    Ty::Function(
                        vec![Ty::Unknown; parameters.len()],
                        Box::new(Ty::value(result_type)),
                    ),
                );
                let (rest, result) = self.lower_pure_block_at(
                    module,
                    &child,
                    statements,
                    index + 1,
                    expected,
                    mode,
                    span,
                );
                let lambda = IrExpr::Lambda {
                    params: parameters
                        .iter()
                        .filter_map(|parameter| match &parameter.value {
                            ast::PatternKind::Name(name) => Some(name.value.clone()),
                            _ => None,
                        })
                        .collect(),
                    body: Box::new(IrExpr::Literal { value: Value::Unit }),
                };
                (
                    IrExpr::Let {
                        bindings: vec![(name.value.clone(), lambda)],
                        value: Box::new(rest),
                    },
                    result,
                )
            }
            ast::StatementKind::Let { name, ty, value } => {
                let annotation = ty.as_ref().map(|ty| self.resolve_type(module, scope, ty));
                let (value_ir, actual) =
                    self.lower_expr(module, scope, value, annotation.as_ref(), mode);
                if let Some(annotation) = &annotation {
                    self.expect_type(&actual, annotation, value.span);
                }
                let binding_ty = annotation
                    .or_else(|| actual.into_value())
                    .unwrap_or(TypeRef::Never);
                let mut child = scope.child();
                child.bind(&name.value, &name.value, Ty::value(binding_ty));
                let (rest, result) = self.lower_pure_block_at(
                    module,
                    &child,
                    statements,
                    index + 1,
                    expected,
                    mode,
                    span,
                );
                (
                    IrExpr::Let {
                        bindings: vec![(name.value.clone(), value_ir)],
                        value: Box::new(rest),
                    },
                    result,
                )
            }
            ast::StatementKind::Expr(value) if index + 1 == statements.len() => {
                self.lower_expr(module, scope, value, expected, mode)
            }
            ast::StatementKind::Expr(value) => {
                self.diagnostics.push(error(
                    codes::EFFECT,
                    "uhura/discarded-pure-expression",
                    "only the final expression of a pure block may be unbound",
                    value.span,
                ));
                self.lower_pure_block_at(module, scope, statements, index + 1, expected, mode, span)
            }
            ast::StatementKind::Set { .. }
            | ast::StatementKind::Emit(_)
            | ast::StatementKind::While { .. } => {
                self.diagnostics.push(error(
                    codes::EFFECT,
                    "uhura/effect-in-pure-block",
                    "state writes, commands, and loops are not valid in a pure value block",
                    statement.span,
                ));
                (IrExpr::Literal { value: Value::Unit }, Ty::Unknown)
            }
        }
    }

    fn lower_record(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        entries: &[ast::RecordEntry],
        expected: Option<&TypeRef>,
        mode: ExprMode,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        if let Some(TypeRef::Map { key, value }) = expected {
            let mut lowered = Vec::new();
            let mut canonical_keys = BTreeSet::new();
            for entry in entries {
                let (key_ir, actual_key) =
                    self.lower_expr(module, scope, &entry.key, Some(key), mode);
                self.expect_type(&actual_key, key, entry.key.span);
                match const_eval(&key_ir, &self.program) {
                    Ok(value) => {
                        let canonical = value.canonical_bytes();
                        if !canonical_keys.insert(canonical) {
                            self.diagnostics.push(error(
                                codes::NOT_TOTAL,
                                "uhura/duplicate-map-key",
                                "map literal keys must be canonically distinct",
                                entry.key.span,
                            ));
                        }
                    }
                    Err(_) => self.diagnostics.push(error(
                        codes::NOT_TOTAL,
                        "uhura/dynamic-map-key",
                        "map literal keys must be compile-time constants; use `Map.from_unique` for dynamic keys",
                        entry.key.span,
                    )),
                }
                let (value_ir, actual_value) =
                    self.lower_expr(module, scope, &entry.value, Some(value), mode);
                self.expect_type(&actual_value, value, entry.value.span);
                lowered.push((key_ir, value_ir));
            }
            return (
                IrExpr::Map {
                    entries: lowered,
                    result_type: expected.cloned().expect("map expected"),
                },
                Ty::value(expected.cloned().expect("map expected")),
            );
        }

        if let Some(TypeRef::Table { key, value }) = expected {
            let mut lowered = Vec::new();
            let mut seen = BTreeSet::new();
            for entry in entries {
                let Some(name) = record_key(&entry.key) else {
                    self.diagnostics.push(error(
                        codes::TYPE_MISMATCH,
                        "uhura/table-key",
                        "Table literal keys must be closed nullary constructors or key spellings",
                        entry.key.span,
                    ));
                    continue;
                };
                if !seen.insert(name.clone()) {
                    self.diagnostics.push(error(
                        codes::DUPLICATE,
                        "uhura/duplicate-table-key",
                        format!("Table key `{name}` is repeated"),
                        entry.key.span,
                    ));
                }
                let (value_ir, actual) =
                    self.lower_expr(module, scope, &entry.value, Some(value), mode);
                self.expect_type(&actual, value, entry.value.span);
                lowered.push((name, value_ir));
            }
            if let Some(constructors) = self.registry.finite_constructors(key) {
                let actual = seen;
                if constructors != actual {
                    self.diagnostics.push(error(
                        codes::NOT_TOTAL,
                        "uhura/incomplete-table",
                        format!(
                            "Table literal must cover exactly [{}]; found [{}]",
                            constructors.into_iter().collect::<Vec<_>>().join(", "),
                            actual.into_iter().collect::<Vec<_>>().join(", ")
                        ),
                        span,
                    ));
                }
            }
            return (
                IrExpr::Table {
                    key_type: key.canonical_name(),
                    entries: lowered,
                },
                Ty::value(expected.cloned().expect("table expected")),
            );
        }

        let expected_fields = expected.and_then(|ty| self.registry.fields(ty));
        let mut fields = Vec::new();
        let mut types = Vec::new();
        let mut seen = BTreeSet::new();
        for entry in entries {
            let Some(name) = record_key(&entry.key) else {
                self.diagnostics.push(error(
                    codes::TYPE_MISMATCH,
                    "uhura/record-field-name",
                    "record fields must use identifier keys",
                    entry.key.span,
                ));
                continue;
            };
            if !seen.insert(name.clone()) {
                self.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura/duplicate-record-field",
                    format!("record field `{name}` is repeated"),
                    entry.key.span,
                ));
            }
            let field_expected = expected_fields
                .as_ref()
                .and_then(|fields| fields.iter().find(|(field, _)| field == &name))
                .map(|(_, ty)| ty);
            if expected_fields.is_some() && field_expected.is_none() {
                self.diagnostics.push(error(
                    codes::UNKNOWN_NAME,
                    "uhura/unknown-record-field",
                    format!("unknown field `{name}` for expected record"),
                    entry.key.span,
                ));
            }
            let (value, actual) =
                self.lower_expr(module, scope, &entry.value, field_expected, mode);
            if let Some(expected) = field_expected {
                self.expect_type(&actual, expected, entry.value.span);
            }
            fields.push((name.clone(), value));
            types.push((name, actual.into_value().unwrap_or(TypeRef::Never)));
        }
        if let Some(expected_fields) = expected_fields {
            let missing = expected_fields
                .iter()
                .map(|(name, _)| name)
                .filter(|name| !seen.contains(*name))
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                self.diagnostics.push(error(
                    codes::TYPE_MISMATCH,
                    "uhura/missing-record-field",
                    format!("record is missing field(s): {}", missing.join(", ")),
                    span,
                ));
            }
        }
        (
            IrExpr::Record { fields },
            Ty::value(
                expected
                    .cloned()
                    .unwrap_or(TypeRef::Record { fields: types }),
            ),
        )
    }

    fn lower_condition(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        expression: &ast::Expr,
        mode: ExprMode,
    ) -> (IrExpr, Scope) {
        match &expression.value {
            ast::ExprKind::Is { value, pattern } => {
                let (value_ir, value_ty) = self.lower_expr(module, scope, value, None, mode);
                let mut refined = scope.child();
                let pattern = self.lower_pattern(
                    module,
                    &mut refined,
                    pattern,
                    value_ty.as_value(),
                    PatternUse::Condition,
                );
                (
                    IrExpr::Is {
                        value: Box::new(value_ir),
                        pattern,
                    },
                    refined,
                )
            }
            ast::ExprKind::Binary { left, op, right } if op.value == ast::BinaryOp::And => {
                let (left, mut refined) = self.lower_condition(module, scope, left, mode);
                let (right, right_refined) = self.lower_condition(module, &refined, right, mode);
                refined = right_refined;
                (
                    IrExpr::Binary {
                        op: IrBinaryOp::And,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    refined,
                )
            }
            _ => {
                let (value, ty) =
                    self.lower_expr(module, scope, expression, Some(&TypeRef::Bool), mode);
                self.expect_type(&ty, &TypeRef::Bool, expression.span);
                (
                    value,
                    refined_numeric_scope(scope, expression, true, &self.registry),
                )
            }
        }
    }

    fn lower_value_match(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        subject: &ast::Expr,
        arms: &[ast::MatchArm],
        expected: Option<&TypeRef>,
        mode: ExprMode,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        let (value, subject_ty) = self.lower_expr(module, scope, subject, None, mode);
        let inferred_expected = expected
            .cloned()
            .or_else(|| self.probe_match_result(module, scope, arms, subject_ty.as_value(), mode));
        let expected = inferred_expected.as_ref();
        let mut lowered = Vec::new();
        let mut result = Ty::Never;
        let mut covered = BTreeSet::new();
        let mut wildcard = false;
        for arm in arms {
            let mut child = scope.child();
            let pattern = self.lower_pattern(
                module,
                &mut child,
                &arm.pattern,
                subject_ty.as_value(),
                PatternUse::Match,
            );
            self.record_pattern_coverage(&pattern, &mut covered, &mut wildcard, arm.pattern.span);
            let (body, ty) = self.lower_expr(module, &child, &arm.body, expected, mode);
            result = join(&result, &ty);
            lowered.push(IrMatchArm {
                pattern,
                value: body,
            });
        }
        self.check_exhaustive(subject_ty.as_value(), &covered, wildcard, span);
        (
            IrExpr::Match {
                value: Box::new(value),
                arms: lowered,
            },
            result,
        )
    }

    fn probe_match_result(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        arms: &[ast::MatchArm],
        subject_ty: Option<&TypeRef>,
        mode: ExprMode,
    ) -> Option<TypeRef> {
        for arm in arms {
            if reaction_control(&arm.body) {
                continue;
            }
            let checkpoint = self.diagnostics.len();
            let mut child = scope.child();
            let _ = self.lower_pattern(
                module,
                &mut child,
                &arm.pattern,
                subject_ty,
                PatternUse::Match,
            );
            let (_, ty) = self.lower_expr(module, &child, &arm.body, None, mode);
            self.diagnostics.truncate(checkpoint);
            if let Some(ty) = ty.into_value()
                && ty != TypeRef::Never
            {
                return Some(ty);
            }
        }
        None
    }

    fn lower_member(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        receiver: &ast::Expr,
        member: &ast::Name,
        mode: ExprMode,
    ) -> (IrExpr, Ty) {
        if let Some(mut path) = ast_member_path(receiver) {
            path.push(member.value.clone());
            let qualified = path[1..].join(".");
            if path.len() > 2
                && let Some(binding) = scope.values.get(&path[0])
                && let Some(TypeRef::Record { fields }) = binding.ty.as_value()
                && let Some((_, field_ty)) = fields.iter().find(|(name, _)| name == &qualified)
            {
                return (
                    IrExpr::Field {
                        value: Box::new(IrExpr::Name {
                            name: binding.lowered.clone(),
                        }),
                        field: qualified,
                    },
                    Ty::value(field_ty.clone()),
                );
            }
            let constructor_name = path.join(".");
            if let Ok(constructor) = self.resolve_constructor(scope, &constructor_name, None)
                && constructor.fields.is_empty()
            {
                return (
                    IrExpr::Constructor {
                        type_id: constructor.type_id.clone(),
                        constructor: constructor.name,
                        fields: Vec::new(),
                    },
                    Ty::value(TypeRef::Named {
                        id: constructor.type_id,
                    }),
                );
            }
        }
        if let ast::ExprKind::Name(type_name) = &receiver.value
            && let Some(ty @ TypeRef::Named { .. }) = scope.types.get(&type_name.value)
            && let Ok(constructor) = self.resolve_constructor(scope, &member.value, Some(ty))
            && constructor.fields.is_empty()
        {
            return (
                IrExpr::Constructor {
                    type_id: constructor.type_id.clone(),
                    constructor: constructor.name,
                    fields: Vec::new(),
                },
                Ty::value(TypeRef::Named {
                    id: constructor.type_id,
                }),
            );
        }
        if let ast::ExprKind::Name(type_name) = &receiver.value
            && matches!(type_name.value.as_str(), "Map" | "Set")
            && member.value == "empty"
        {
            return (
                if type_name.value == "Map" {
                    IrExpr::Map {
                        entries: Vec::new(),
                        result_type: TypeRef::Never,
                    }
                } else {
                    IrExpr::SetComprehension {
                        pattern: IrPattern::Ignore,
                        source: Box::new(IrExpr::Seq { values: Vec::new() }),
                        conditions: Vec::new(),
                        value: Box::new(IrExpr::Literal { value: Value::Unit }),
                        result_type: TypeRef::Never,
                    }
                },
                Ty::Unknown,
            );
        }
        let (value, ty) = self.lower_expr(module, scope, receiver, None, mode);
        let result = match ty.as_value() {
            Some(TypeRef::Named { .. }) => match self.registry.shape(ty.as_value().expect("value"))
            {
                Some(TypeShape::Key(underlying)) if member.value == "value" => {
                    Some(underlying.clone())
                }
                _ => self
                    .registry
                    .fields(ty.as_value().expect("value"))
                    .and_then(|fields| {
                        fields
                            .into_iter()
                            .find(|(name, _)| name == &member.value)
                            .map(|(_, ty)| ty)
                    }),
            },
            Some(TypeRef::Record { fields }) => fields
                .iter()
                .find(|(name, _)| name == &member.value)
                .map(|(_, ty)| ty.clone()),
            Some(TypeRef::Seq { value }) => match member.value.as_str() {
                "size" => Some(TypeRef::Nat),
                "is_empty" | "unique" => Some(TypeRef::Bool),
                "uncons" => Some(TypeRef::Option {
                    value: Box::new(TypeRef::Record {
                        fields: vec![
                            ("head".into(), value.as_ref().clone()),
                            (
                                "tail".into(),
                                TypeRef::Seq {
                                    value: value.clone(),
                                },
                            ),
                        ],
                    }),
                }),
                _ => None,
            },
            Some(TypeRef::Map { key, value }) => match member.value.as_str() {
                "size" => Some(TypeRef::Nat),
                "is_empty" => Some(TypeRef::Bool),
                "entries" => Some(TypeRef::FiniteView {
                    value: Box::new(TypeRef::Record {
                        fields: vec![
                            ("key".into(), key.as_ref().clone()),
                            ("value".into(), value.as_ref().clone()),
                        ],
                    }),
                }),
                "entries_by_key" => Some(TypeRef::Seq {
                    value: Box::new(TypeRef::Tuple {
                        values: vec![key.as_ref().clone(), value.as_ref().clone()],
                    }),
                }),
                "values" => Some(TypeRef::FiniteView {
                    value: value.clone(),
                }),
                _ => None,
            },
            Some(TypeRef::Table { value, .. }) if member.value == "values" => Some(TypeRef::Seq {
                value: value.clone(),
            }),
            Some(TypeRef::Set { .. }) if member.value == "size" => Some(TypeRef::Nat),
            Some(TypeRef::Set { .. }) if member.value == "is_empty" => Some(TypeRef::Bool),
            Some(TypeRef::Text) if member.value == "is_empty" => Some(TypeRef::Bool),
            _ => None,
        };
        if let Some(result) = result {
            let ir = if matches!(
                member.value.as_str(),
                "entries" | "entries_by_key" | "values" | "uncons" | "unique"
            ) {
                IrExpr::Method {
                    value: Box::new(value),
                    method: member.value.clone(),
                    args: Vec::new(),
                    result_type: result.clone(),
                }
            } else {
                IrExpr::Field {
                    value: Box::new(value),
                    field: member.value.clone(),
                }
            };
            (ir, Ty::value(result))
        } else {
            self.diagnostics.push(error(
                codes::UNKNOWN_NAME,
                "uhura/unknown-member",
                format!(
                    "type `{}` has no total member `{}`",
                    ty.display(),
                    member.value
                ),
                member.span,
            ));
            (
                IrExpr::Field {
                    value: Box::new(value),
                    field: member.value.clone(),
                },
                Ty::Unknown,
            )
        }
    }

    fn lower_call(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        callee: &ast::Expr,
        arguments: &[ast::Expr],
        expected: Option<&TypeRef>,
        mode: ExprMode,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        if let ast::ExprKind::Name(name) = &callee.value
            && name.value.starts_with(PURE_CONTINUATION_LOCAL_PREFIX)
            && self.pure_continuations.contains_key(&name.value)
        {
            return self.lower_pure_continuation_call(
                module,
                scope,
                &name.value,
                arguments,
                mode,
                span,
            );
        }
        if let ast::ExprKind::Name(name) = &callee.value {
            if name.value == "finite" {
                if arguments.len() != 1 {
                    self.arity("finite", 1, arguments.len(), span);
                }
                let (argument, actual) = arguments.first().map_or(
                    (IrExpr::Literal { value: Value::Unit }, Ty::Unknown),
                    |argument| {
                        self.lower_expr(module, scope, argument, Some(&TypeRef::Decimal), mode)
                    },
                );
                self.expect_type(&actual, &TypeRef::Decimal, span);
                return (
                    IrExpr::Constructor {
                        type_id: TypeRef::BoundaryNumber.canonical_name(),
                        constructor: "finite".into(),
                        fields: vec![(Some("value".into()), argument)],
                    },
                    Ty::value(TypeRef::BoundaryNumber),
                );
            }
            if name.value == "some" && expected.is_none() && arguments.len() == 1 {
                let (argument, ty) = self.lower_expr(module, scope, &arguments[0], None, mode);
                if let Some(inner) = ty.into_value() {
                    let option = TypeRef::Option {
                        value: Box::new(inner.clone()),
                    };
                    return (
                        IrExpr::Constructor {
                            type_id: option.canonical_name(),
                            constructor: "some".into(),
                            fields: vec![(Some("value".into()), argument)],
                        },
                        Ty::value(option),
                    );
                }
            }
            if let Some((id, params, result)) = scope.functions.get(&name.value) {
                return self
                    .lower_resolved_call(module, scope, id, params, result, arguments, mode, span);
            }
            if matches!(name.value.as_str(), "min" | "max") {
                let first_expected = expected;
                let lowered = arguments
                    .iter()
                    .map(|argument| self.lower_expr(module, scope, argument, first_expected, mode))
                    .collect::<Vec<_>>();
                if lowered.len() != 2 {
                    self.arity(name.value.as_str(), 2, lowered.len(), span);
                }
                let ty = lowered
                    .first()
                    .map(|(_, ty)| ty.clone())
                    .unwrap_or(Ty::Unknown);
                return (
                    IrExpr::Call {
                        function: name.value.clone(),
                        args: lowered.into_iter().map(|(value, _)| value).collect(),
                        result_type: ty.as_value().cloned().unwrap_or(TypeRef::Never),
                    },
                    ty,
                );
            }
            if let Some(TypeRef::Named { id }) = scope.types.get(&name.value) {
                let underlying = match self.registry.shape(&TypeRef::Named { id: id.clone() }) {
                    Some(TypeShape::Key(underlying)) => Some(underlying.clone()),
                    _ => None,
                };
                if let Some(underlying) = underlying {
                    if arguments.len() != 1 {
                        self.arity(&name.value, 1, arguments.len(), span);
                    }
                    let argument = arguments
                        .first()
                        .map(|argument| {
                            self.lower_expr(module, scope, argument, Some(&underlying), mode)
                                .0
                        })
                        .unwrap_or(IrExpr::Literal { value: Value::Unit });
                    return (
                        IrExpr::Key {
                            type_id: id.clone(),
                            value: Box::new(argument),
                        },
                        Ty::value(TypeRef::Named { id: id.clone() }),
                    );
                }
            }
            if let Ok(constructor) = self.resolve_constructor(scope, &name.value, expected) {
                return self.lower_constructor_call(
                    module,
                    scope,
                    constructor,
                    arguments,
                    mode,
                    span,
                );
            }
            if name.value == "routes" {
                // The route declaration itself is compiled by the const pass.
                let args = arguments
                    .iter()
                    .map(|argument| self.lower_expr(module, scope, argument, None, mode).0)
                    .collect();
                return (
                    IrExpr::Call {
                        function: "routes".into(),
                        args,
                        result_type: expected.cloned().unwrap_or(TypeRef::Never),
                    },
                    Ty::value(expected.cloned().unwrap_or(TypeRef::Never)),
                );
            }
        }

        if let ast::ExprKind::Member { receiver, member } = &callee.value {
            if let Some(path) = ast_member_path(callee) {
                let qualified = path.join(".");
                if let Ok(constructor) = self.resolve_constructor(scope, &qualified, expected) {
                    return self.lower_constructor_call(
                        module,
                        scope,
                        constructor,
                        arguments,
                        mode,
                        span,
                    );
                }
            }
            if let ast::ExprKind::Name(type_name) = &receiver.value {
                if let Some(ty @ TypeRef::Named { .. }) = scope.types.get(&type_name.value)
                    && let Ok(constructor) =
                        self.resolve_constructor(scope, &member.value, Some(ty))
                {
                    return self.lower_constructor_call(
                        module,
                        scope,
                        constructor,
                        arguments,
                        mode,
                        span,
                    );
                }
                let function = format!("{}.{}", type_name.value, member.value);
                if member.value == "fixture" && mode == ExprMode::Evidence {
                    let args = arguments
                        .iter()
                        .map(|argument| self.lower_expr(module, scope, argument, None, mode).0)
                        .collect();
                    return (
                        IrExpr::Call {
                            function,
                            args,
                            result_type: TypeRef::Never,
                        },
                        Ty::Unknown,
                    );
                }
                if matches!(
                    function.as_str(),
                    "Int.from"
                        | "Ratio.from"
                        | "NonEmpty.from"
                        | "Map.from_unique"
                        | "Set.from_unique"
                ) {
                    let arg_expected = match function.as_str() {
                        "Int.from" | "Ratio.from" => Some(TypeRef::BoundaryNumber),
                        _ => None,
                    };
                    let lowered = arguments
                        .iter()
                        .map(|argument| {
                            self.lower_expr(module, scope, argument, arg_expected.as_ref(), mode)
                        })
                        .collect::<Vec<_>>();
                    let inferred = match function.as_str() {
                        "Int.from" => TypeRef::Option {
                            value: Box::new(TypeRef::Int),
                        },
                        "Ratio.from" => TypeRef::Option {
                            value: Box::new(TypeRef::Ratio),
                        },
                        "NonEmpty.from" => {
                            match lowered.first().and_then(|(_, ty)| ty.as_value()) {
                                Some(TypeRef::Seq { value }) => TypeRef::Option {
                                    value: Box::new(TypeRef::NonEmpty {
                                        value: value.clone(),
                                    }),
                                },
                                _ => TypeRef::Never,
                            }
                        }
                        "Map.from_unique" => {
                            match lowered.first().and_then(|(_, ty)| ty.as_value()) {
                                Some(TypeRef::Seq { value }) => match value.as_ref() {
                                    TypeRef::Tuple { values } if values.len() == 2 => {
                                        TypeRef::Option {
                                            value: Box::new(TypeRef::Map {
                                                key: Box::new(values[0].clone()),
                                                value: Box::new(values[1].clone()),
                                            }),
                                        }
                                    }
                                    _ => TypeRef::Never,
                                },
                                _ => TypeRef::Never,
                            }
                        }
                        "Set.from_unique" => {
                            match lowered.first().and_then(|(_, ty)| ty.as_value()) {
                                Some(TypeRef::Seq { value }) => TypeRef::Option {
                                    value: Box::new(TypeRef::Set {
                                        value: value.clone(),
                                    }),
                                },
                                _ => TypeRef::Never,
                            }
                        }
                        _ => TypeRef::Never,
                    };
                    let result = expected.cloned().unwrap_or(inferred);
                    return (
                        IrExpr::Call {
                            function,
                            args: lowered.into_iter().map(|(value, _)| value).collect(),
                            result_type: result.clone(),
                        },
                        Ty::value(result),
                    );
                }
            }
            if let ast::ExprKind::Name(port) = &receiver.value {
                let qualified = format!("{}.{}", port.value, member.value);
                if let Some(constructor) = scope.port_send.get(&qualified).or_else(|| {
                    (mode == ExprMode::Evidence)
                        .then(|| scope.port_receive.get(&qualified))
                        .flatten()
                }) {
                    return self.lower_constructor_call(
                        module,
                        scope,
                        constructor.clone(),
                        arguments,
                        mode,
                        span,
                    );
                }
            }
            let receiver_expected = (member.value == "from_options")
                .then_some(expected)
                .flatten()
                .and_then(|expected| match expected {
                    TypeRef::Seq { value } => Some(TypeRef::Seq {
                        value: Box::new(TypeRef::Option {
                            value: value.clone(),
                        }),
                    }),
                    _ => None,
                });
            let (receiver_ir, receiver_ty) =
                self.lower_expr(module, scope, receiver, receiver_expected.as_ref(), mode);
            return self.lower_method_call(
                module,
                scope,
                receiver_ir,
                receiver_ty,
                &member.value,
                arguments,
                expected,
                mode,
                span,
            );
        }

        let (function, function_ty) = self.lower_expr(module, scope, callee, None, mode);
        let args = arguments
            .iter()
            .map(|argument| self.lower_expr(module, scope, argument, None, mode).0)
            .collect::<Vec<_>>();
        match function_ty {
            Ty::Function(_, result) => (
                IrExpr::Invoke {
                    function: Box::new(function),
                    args,
                },
                *result,
            ),
            _ => {
                self.diagnostics.push(error(
                    codes::TYPE_MISMATCH,
                    "uhura/not-callable",
                    format!("`{}` is not callable", function_ty.display()),
                    callee.span,
                ));
                (
                    IrExpr::Invoke {
                        function: Box::new(function),
                        args,
                    },
                    Ty::Unknown,
                )
            }
        }
    }

    fn lower_pure_continuation_call(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        name: &str,
        arguments: &[ast::Expr],
        mode: ExprMode,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        let signature = self
            .pure_continuations
            .get(name)
            .map(|continuation| {
                (
                    continuation.parameters.len(),
                    continuation.parameter_types.clone(),
                    continuation.result.clone(),
                )
            })
            .expect("generated pure continuation is registered before invocation");
        if signature.0 != arguments.len() {
            self.arity(name, signature.0, arguments.len(), span);
        }
        let lowered = arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let expected = signature
                    .1
                    .as_ref()
                    .and_then(|parameters| parameters.get(index))
                    .and_then(Ty::as_value);
                let (value, actual) = self.lower_expr(module, scope, argument, expected, mode);
                if let Some(expected) = expected {
                    self.expect_type(&actual, expected, argument.span);
                }
                (value, actual)
            })
            .collect::<Vec<_>>();
        let actual_types = lowered.iter().map(|(_, ty)| ty.clone()).collect::<Vec<_>>();
        let continuation = self
            .pure_continuations
            .get_mut(name)
            .expect("generated pure continuation remains registered");
        continuation.parameter_types = Some(match continuation.parameter_types.take() {
            Some(previous) => previous
                .into_iter()
                .zip(actual_types)
                .map(|(left, right)| join(&left, &right))
                .collect(),
            None => actual_types,
        });
        (
            IrExpr::Invoke {
                function: Box::new(IrExpr::Name { name: name.into() }),
                args: lowered.into_iter().map(|(value, _)| value).collect(),
            },
            Ty::value(signature.2),
        )
    }

    fn drain_pure_continuations(
        &mut self,
        module: &ModuleEnv<'_>,
        mode: ExprMode,
        expression: IrExpr,
    ) -> IrExpr {
        if self.pure_continuations.is_empty() {
            return expression;
        }

        self.draining_pure_continuations = true;
        let mut compiled = BTreeMap::new();
        loop {
            let Some(name) = self
                .pure_continuations
                .keys()
                .find(|name| !compiled.contains_key(*name))
                .cloned()
            else {
                break;
            };
            let deferred = self.pure_continuations[&name].clone();
            let parameter_types = deferred
                .parameter_types
                .clone()
                .unwrap_or_else(|| vec![Ty::Unknown; deferred.parameters.len()]);
            let mut continuation_scope = deferred.definition_scope.clone();
            let mut parameter_names = Vec::new();
            for (index, parameter) in deferred.parameters.iter().enumerate() {
                let actual = parameter_types.get(index).cloned().unwrap_or(Ty::Unknown);
                match &parameter.value {
                    ast::PatternKind::Name(parameter) => {
                        continuation_scope.bind(&parameter.value, &parameter.value, actual);
                        parameter_names.push(parameter.value.clone());
                    }
                    _ => {
                        self.diagnostics.push(error(
                            codes::UNSUPPORTED,
                            "uhura/internal-pure-continuation-pattern",
                            "compiler-generated pure continuations require named parameters",
                            parameter.span,
                        ));
                        parameter_names.push(format!("_continuation_{}", parameter.span.start));
                    }
                }
            }
            let (body, actual) = self.lower_expr(
                module,
                &continuation_scope,
                &deferred.body,
                Some(&deferred.result),
                mode,
            );
            self.expect_type(&actual, &deferred.result, deferred.body.span);
            compiled.insert(
                name,
                CompiledPureContinuation {
                    lambda: IrExpr::Lambda {
                        params: parameter_names,
                        body: Box::new(body),
                    },
                    parameters: parameter_types,
                    result: Ty::value(deferred.result),
                },
            );
        }
        self.pure_continuations.clear();
        self.draining_pure_continuations = false;

        let compiled = compiled
            .into_iter()
            .map(|(name, continuation)| {
                debug_assert_eq!(continuation.parameters.len(), 1);
                debug_assert!(matches!(continuation.result, Ty::Value(_)));
                (name, continuation.lambda)
            })
            .collect();
        materialize_pure_continuation_bindings(expression, &compiled, &mut BTreeMap::new())
    }

    fn lower_resolved_call(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        id: &str,
        params: &[TypeRef],
        result: &TypeRef,
        arguments: &[ast::Expr],
        mode: ExprMode,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        if params.len() != arguments.len() {
            self.arity(id, params.len(), arguments.len(), span);
        }
        let args = arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let expected = params.get(index);
                let (value, actual) = self.lower_expr(module, scope, argument, expected, mode);
                if let Some(expected) = expected {
                    self.expect_type(&actual, expected, argument.span);
                }
                value
            })
            .collect();
        (
            IrExpr::Call {
                function: id.into(),
                args,
                result_type: result.clone(),
            },
            Ty::value(result.clone()),
        )
    }

    fn lower_constructor_call(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        constructor: ConstructorInfo,
        arguments: &[ast::Expr],
        mode: ExprMode,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        if constructor.fields.len() != arguments.len() {
            self.arity(
                &constructor.name,
                constructor.fields.len(),
                arguments.len(),
                span,
            );
        }
        let fields = arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let expected = constructor.fields.get(index).map(|(_, ty)| ty);
                let (value, actual) = self.lower_expr(module, scope, argument, expected, mode);
                if let Some(expected) = expected {
                    self.expect_type(&actual, expected, argument.span);
                }
                (
                    constructor
                        .fields
                        .get(index)
                        .and_then(|(name, _)| name.clone()),
                    value,
                )
            })
            .collect();
        (
            IrExpr::Constructor {
                type_id: constructor.type_id.clone(),
                constructor: constructor.name,
                fields,
            },
            Ty::value(TypeRef::Named {
                id: constructor.type_id,
            }),
        )
    }

    fn lower_method_call(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        receiver: IrExpr,
        receiver_ty: Ty,
        method: &str,
        arguments: &[ast::Expr],
        expected: Option<&TypeRef>,
        mode: ExprMode,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        let receiver_value = receiver_ty.as_value().cloned();
        let (params, fixed_result, lambda_item) = match (receiver_value.as_ref(), method) {
            (Some(TypeRef::Seq { value }), "from_options")
                if matches!(value.as_ref(), TypeRef::Option { .. }) =>
            {
                let TypeRef::Option { value } = value.as_ref() else {
                    unreachable!("guard proves Option item")
                };
                (
                    Vec::new(),
                    TypeRef::Seq {
                        value: value.clone(),
                    },
                    None,
                )
            }
            (Some(TypeRef::Seq { value }), "append" | "without" | "contains") => (
                vec![value.as_ref().clone()],
                if method == "contains" {
                    TypeRef::Bool
                } else {
                    receiver_value.clone().unwrap()
                },
                None,
            ),
            (Some(TypeRef::Map { key, value }), "get") => (
                vec![key.as_ref().clone()],
                TypeRef::Option {
                    value: value.clone(),
                },
                None,
            ),
            (Some(TypeRef::Map { key, value }), "put") => (
                vec![key.as_ref().clone(), value.as_ref().clone()],
                receiver_value.clone().unwrap(),
                None,
            ),
            (Some(TypeRef::Map { key, .. }), "remove") => (
                vec![key.as_ref().clone()],
                receiver_value.clone().unwrap(),
                None,
            ),
            (Some(TypeRef::Set { value }), "add" | "remove" | "contains") => (
                vec![value.as_ref().clone()],
                if method == "contains" {
                    TypeRef::Bool
                } else {
                    receiver_value.clone().unwrap()
                },
                None,
            ),
            (Some(TypeRef::Table { key, value }), "set") => (
                vec![key.as_ref().clone(), value.as_ref().clone()],
                receiver_value.clone().unwrap(),
                None,
            ),
            (
                Some(ty @ (TypeRef::Seq { .. } | TypeRef::NonEmpty { .. })),
                "all" | "any" | "count" | "map" | "filter" | "try_map",
            )
            | (
                Some(
                    ty @ (TypeRef::Seq { .. }
                    | TypeRef::NonEmpty { .. }
                    | TypeRef::Set { .. }
                    | TypeRef::FiniteView { .. }),
                ),
                "filter_map",
            )
            | (Some(ty @ TypeRef::FiniteView { .. }), "all" | "any" | "count")
            | (Some(ty @ TypeRef::Map { .. }), "try_map_values") => {
                let item = collection_item_type(Some(ty)).unwrap_or(TypeRef::Never);
                let result = match method {
                    "all" | "any" => TypeRef::Bool,
                    "count" => TypeRef::Nat,
                    _ => TypeRef::Never,
                };
                (Vec::new(), result, Some(item))
            }
            _ => {
                self.diagnostics.push(error(
                    codes::PARTIAL_OPERATION,
                    "uhura/unknown-total-method",
                    format!(
                        "`{}` has no supported total method `{method}`",
                        receiver_ty.display()
                    ),
                    span,
                ));
                (Vec::new(), TypeRef::Never, None)
            }
        };
        let (args, result) = if let Some(item) = lambda_item {
            if arguments.len() != 1 {
                self.arity(method, 1, arguments.len(), span);
            }
            let expected_lambda = expected
                .and_then(|expected| match (method, expected) {
                    ("map", TypeRef::Seq { value }) => Some(value.as_ref().clone()),
                    ("try_map", TypeRef::Option { value }) => match value.as_ref() {
                        TypeRef::Seq { value } => Some(TypeRef::Option {
                            value: value.clone(),
                        }),
                        _ => None,
                    },
                    ("try_map_values", TypeRef::Option { value }) => match value.as_ref() {
                        TypeRef::Map { value, .. } => Some(TypeRef::Option {
                            value: value.clone(),
                        }),
                        _ => None,
                    },
                    ("filter_map", TypeRef::Set { value }) => Some(TypeRef::Option {
                        value: value.clone(),
                    }),
                    _ => None,
                })
                .or_else(|| {
                    matches!(method, "all" | "any" | "count" | "filter").then_some(TypeRef::Bool)
                });
            let (lambda, lambda_ty) = arguments.first().map_or(
                (
                    IrExpr::Lambda {
                        params: Vec::new(),
                        body: Box::new(IrExpr::Literal { value: Value::Unit }),
                    },
                    Ty::Unknown,
                ),
                |argument| {
                    self.lower_typed_lambda(
                        module,
                        scope,
                        argument,
                        &item,
                        expected_lambda.as_ref(),
                        mode,
                    )
                },
            );
            let lambda_value = lambda_ty.into_value().unwrap_or(TypeRef::Never);
            if method == "filter_map" && !matches!(&lambda_value, TypeRef::Option { .. }) {
                self.diagnostics.push(error(
                    codes::TYPE_MISMATCH,
                    "uhura/filter-map-option",
                    "`filter_map` binder must return `Option<T>`",
                    arguments.first().map_or(span, |argument| argument.span),
                ));
            }
            let inferred = match method {
                "all" | "any" | "count" => fixed_result.clone(),
                "map" => TypeRef::Seq {
                    value: Box::new(lambda_value),
                },
                "filter" => TypeRef::Seq {
                    value: Box::new(item.clone()),
                },
                "try_map" => {
                    let inner = match lambda_value {
                        TypeRef::Option { value } => value,
                        _ => Box::new(TypeRef::Never),
                    };
                    TypeRef::Option {
                        value: Box::new(TypeRef::Seq { value: inner }),
                    }
                }
                "try_map_values" => {
                    let inner = match lambda_value {
                        TypeRef::Option { value } => value,
                        _ => Box::new(TypeRef::Never),
                    };
                    match receiver_value.as_ref() {
                        Some(TypeRef::Map { key, .. }) => TypeRef::Option {
                            value: Box::new(TypeRef::Map {
                                key: key.clone(),
                                value: inner,
                            }),
                        },
                        _ => TypeRef::Never,
                    }
                }
                "filter_map" => {
                    let inner = match lambda_value {
                        TypeRef::Option { value } => value,
                        _ => Box::new(TypeRef::Never),
                    };
                    TypeRef::Set { value: inner }
                }
                _ => TypeRef::Never,
            };
            (vec![lambda], expected.cloned().unwrap_or(inferred))
        } else {
            if params.len() != arguments.len() {
                self.arity(method, params.len(), arguments.len(), span);
            }
            let args = arguments
                .iter()
                .enumerate()
                .map(|(index, argument)| {
                    let expected = params.get(index);
                    let (value, actual) = self.lower_expr(module, scope, argument, expected, mode);
                    if let Some(expected) = expected {
                        self.expect_type(&actual, expected, argument.span);
                    }
                    value
                })
                .collect();
            (args, fixed_result)
        };
        (
            IrExpr::Method {
                value: Box::new(receiver),
                method: method.into(),
                args,
                result_type: result.clone(),
            },
            Ty::value(result),
        )
    }

    fn lower_typed_lambda(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        expression: &ast::Expr,
        item: &TypeRef,
        expected_result: Option<&TypeRef>,
        mode: ExprMode,
    ) -> (IrExpr, Ty) {
        let ast::ExprKind::Lambda { parameters, body } = &expression.value else {
            self.diagnostics.push(error(
                codes::TYPE_MISMATCH,
                "uhura/expected-lambda",
                "total collection operations require an inline pure lambda",
                expression.span,
            ));
            return (
                IrExpr::Lambda {
                    params: Vec::new(),
                    body: Box::new(IrExpr::Literal { value: Value::Unit }),
                },
                Ty::Unknown,
            );
        };
        let param_types = match item {
            TypeRef::Tuple { values } if parameters.len() == values.len() => values.clone(),
            _ => vec![item.clone()],
        };
        if parameters.len() != param_types.len() {
            self.arity(
                "lambda",
                param_types.len(),
                parameters.len(),
                expression.span,
            );
        }
        let mut child = scope.child();
        let mut names = Vec::new();
        for (index, pattern) in parameters.iter().enumerate() {
            names.extend(self.lower_lambda_pattern(&mut child, pattern, param_types.get(index)));
        }
        let (body, result) = self.lower_expr(module, &child, body, expected_result, mode);
        (
            IrExpr::Lambda {
                params: names,
                body: Box::new(body),
            },
            result,
        )
    }

    fn lower_lambda_pattern(
        &mut self,
        scope: &mut Scope,
        pattern: &ast::Pattern,
        expected: Option<&TypeRef>,
    ) -> Vec<String> {
        match &pattern.value {
            ast::PatternKind::Name(name) => {
                scope.bind(
                    &name.value,
                    &name.value,
                    Ty::value(expected.cloned().unwrap_or(TypeRef::Never)),
                );
                vec![name.value.clone()]
            }
            ast::PatternKind::Wildcard => vec![format!("_lambda_{}", pattern.span.start)],
            _ => {
                self.diagnostics.push(error(
                    codes::UNSUPPORTED,
                    "uhura/lambda-pattern",
                    "Uhura lambda parameters must be names or `_`; tuple collection entries use multiple named parameters",
                    pattern.span,
                ));
                vec![format!("_lambda_{}", pattern.span.start)]
            }
        }
    }

    fn resolve_constructor(
        &self,
        scope: &Scope,
        name: &str,
        expected: Option<&TypeRef>,
    ) -> Result<ConstructorInfo, Vec<ConstructorInfo>> {
        if let Some(expected) = expected {
            let local = scope
                .constructors
                .get(name)
                .into_iter()
                .flatten()
                .filter(|constructor| {
                    constructor.type_id == expected.canonical_name()
                        || matches!(expected, TypeRef::Named { id } if id == &constructor.type_id)
                })
                .cloned()
                .collect::<Vec<_>>();
            if local.len() == 1 {
                return Ok(local[0].clone());
            }
            if let Ok(value) = self.registry.constructor(name, Some(expected)) {
                return Ok(value);
            }
            return Err(local);
        }
        let local = scope.constructors.get(name).cloned().unwrap_or_default();
        if local.len() == 1 {
            Ok(local[0].clone())
        } else if local.is_empty() {
            self.registry.constructor(name, expected)
        } else {
            Err(local)
        }
    }

    fn arithmetic_result_type(
        &mut self,
        scope: &Scope,
        op: IrBinaryOp,
        left_expression: &IrExpr,
        left: &Ty,
        right_expression: &IrExpr,
        right: &Ty,
        span: ast::SourceSpan,
    ) -> Ty {
        let (Some(left), Some(right)) = (left.as_value(), right.as_value()) else {
            return Ty::Unknown;
        };
        let result = match (op, left, right) {
            (
                IrBinaryOp::Add | IrBinaryOp::Subtract | IrBinaryOp::Multiply,
                TypeRef::Int | TypeRef::Nat | TypeRef::PositiveInt,
                TypeRef::Int | TypeRef::Nat | TypeRef::PositiveInt,
            ) => Some(TypeRef::Int),
            (
                IrBinaryOp::Add | IrBinaryOp::Subtract | IrBinaryOp::Multiply,
                TypeRef::Decimal,
                TypeRef::Decimal,
            ) => Some(TypeRef::Decimal),
            (
                IrBinaryOp::Add | IrBinaryOp::Subtract | IrBinaryOp::Multiply,
                TypeRef::Ratio,
                TypeRef::Ratio,
            ) => {
                if !ratio_arithmetic_proven(scope, op, left_expression, right_expression) {
                    self.diagnostics.push(error(
                        codes::INVALID_REFINEMENT,
                        "uhura/ratio-arithmetic",
                        match op {
                            IrBinaryOp::Add => {
                                "cannot prove that `Ratio` addition remains at most 1 from active path facts"
                            }
                            IrBinaryOp::Subtract => {
                                "cannot prove that `Ratio` subtraction remains non-negative from active path facts"
                            }
                            IrBinaryOp::Multiply => {
                                unreachable!("Ratio multiplication is closed over [0,1]")
                            }
                            _ => unreachable!("match admits only Ratio arithmetic"),
                        },
                        span,
                    ));
                }
                Some(TypeRef::Ratio)
            }
            _ => None,
        };
        result.map_or_else(
            || {
                self.diagnostics.push(error(
                    codes::TYPE_MISMATCH,
                    "uhura/invalid-arithmetic-operands",
                    format!(
                        "operator requires compatible exact numeric operands; found `{}` and `{}`",
                        left.canonical_name(),
                        right.canonical_name()
                    ),
                    span,
                ));
                Ty::Unknown
            },
            Ty::value,
        )
    }

    fn coerce(
        &mut self,
        scope: &Scope,
        mut expression: IrExpr,
        actual: Ty,
        expected: &TypeRef,
        span: ast::SourceSpan,
    ) -> (IrExpr, Ty) {
        match &mut expression {
            IrExpr::Map { result_type, .. }
                if *result_type == TypeRef::Never && matches!(expected, TypeRef::Map { .. }) =>
            {
                *result_type = expected.clone();
            }
            IrExpr::SetComprehension { result_type, .. }
                if *result_type == TypeRef::Never && matches!(expected, TypeRef::Set { .. }) =>
            {
                *result_type = expected.clone();
            }
            _ => {}
        }
        if self.types_compatible(&actual, &Ty::value(expected.clone())) {
            if let Some(actual_ref) = actual.as_value() {
                if actual_ref == expected {
                    return (expression, Ty::value(expected.clone()));
                }
                let (function, needs_proof) = match (actual_ref, expected) {
                    (TypeRef::PositiveInt, TypeRef::Nat) => (Some("__coerce_nat"), false),
                    (TypeRef::PositiveInt | TypeRef::Nat, TypeRef::Int) => {
                        (Some("__coerce_int"), false)
                    }
                    (TypeRef::Int, TypeRef::Nat) => (Some("__coerce_nat"), true),
                    (TypeRef::Int | TypeRef::Nat, TypeRef::PositiveInt) => {
                        (Some("__coerce_positive"), true)
                    }
                    _ => (None, false),
                };
                if let Some(function) = function {
                    if needs_proof && !self.integer_refinement_proven(scope, &expression, expected)
                    {
                        self.diagnostics.push(error(
                            codes::INVALID_REFINEMENT,
                            "uhura/unproved-integer-refinement",
                            format!(
                                "cannot prove that `{}` satisfies `{}` from its type, active path facts, and invariants",
                                actual_ref.canonical_name(),
                                expected.canonical_name()
                            ),
                            span,
                        ));
                        return (expression, actual);
                    }
                    return (
                        IrExpr::Call {
                            function: function.into(),
                            args: vec![expression],
                            result_type: expected.clone(),
                        },
                        Ty::value(expected.clone()),
                    );
                }
            }
            (expression, Ty::value(expected.clone()))
        } else {
            self.type_mismatch(&actual, &Ty::value(expected.clone()), span);
            (expression, actual)
        }
    }

    fn integer_refinement_proven(
        &self,
        scope: &Scope,
        expression: &IrExpr,
        expected: &TypeRef,
    ) -> bool {
        let minimum = self.proved_integer_lower_bound(scope, expression);
        match expected {
            TypeRef::Nat => {
                minimum.is_some_and(|value| value >= 0)
                    || integer_difference_non_negative(expression, scope)
            }
            TypeRef::PositiveInt => minimum.is_some_and(|value| value >= 1),
            _ => true,
        }
    }

    fn proved_integer_lower_bound(&self, scope: &Scope, expression: &IrExpr) -> Option<i64> {
        integer_lower_bound(expression, scope)
            .or_else(|| {
                ir_numeric_path(expression)
                    .and_then(|path| static_integer_minimum_for_path(&self.registry, scope, &path))
            })
            .or_else(|| match expression {
                IrExpr::Binary {
                    op: IrBinaryOp::Add,
                    left,
                    right,
                } => self
                    .proved_integer_lower_bound(scope, left)?
                    .checked_add(self.proved_integer_lower_bound(scope, right)?),
                IrExpr::Binary {
                    op: IrBinaryOp::Multiply,
                    left,
                    right,
                } => {
                    let left = self.proved_integer_lower_bound(scope, left)?;
                    let right = self.proved_integer_lower_bound(scope, right)?;
                    (left >= 0 && right >= 0).then(|| left.saturating_mul(right))
                }
                _ => None,
            })
    }

    fn expect_type(&mut self, actual: &Ty, expected: &TypeRef, span: ast::SourceSpan) {
        if !self.types_compatible(actual, &Ty::value(expected.clone())) {
            self.type_mismatch(actual, &Ty::value(expected.clone()), span);
        }
    }

    fn types_compatible(&self, actual: &Ty, expected: &Ty) -> bool {
        match (actual, expected) {
            (Ty::Value(actual), Ty::Value(expected)) => {
                self.value_types_compatible(actual, expected)
            }
            _ => compatible(actual, expected),
        }
    }

    fn value_types_compatible(&self, actual: &TypeRef, expected: &TypeRef) -> bool {
        if super::types::value_compatible(actual, expected) {
            return true;
        }
        let actual_shape = match actual {
            TypeRef::Named { .. } => self.registry.shape(actual),
            _ => None,
        };
        let expected_shape = match expected {
            TypeRef::Named { .. } => self.registry.shape(expected),
            _ => None,
        };
        match (actual, expected) {
            (TypeRef::Record { fields: actual }, TypeRef::Named { .. }) => {
                matches!(expected_shape, Some(TypeShape::Record(expected)) if self.record_types_compatible(actual, expected))
            }
            (TypeRef::Named { .. }, TypeRef::Record { fields: expected }) => {
                matches!(actual_shape, Some(TypeShape::Record(actual)) if self.record_types_compatible(actual, expected))
            }
            (TypeRef::Named { .. }, TypeRef::Named { .. }) => {
                match (actual_shape, expected_shape) {
                    (Some(TypeShape::Alias(actual)), _) => {
                        self.value_types_compatible(actual, expected)
                    }
                    (_, Some(TypeShape::Alias(expected))) => {
                        self.value_types_compatible(actual, expected)
                    }
                    (Some(TypeShape::Record(actual)), Some(TypeShape::Record(expected))) => {
                        self.record_types_compatible(actual, expected)
                    }
                    _ => false,
                }
            }
            (TypeRef::Option { value: actual }, TypeRef::Option { value: expected })
            | (TypeRef::Seq { value: actual }, TypeRef::Seq { value: expected })
            | (TypeRef::NonEmpty { value: actual }, TypeRef::NonEmpty { value: expected })
            | (TypeRef::Set { value: actual }, TypeRef::Set { value: expected })
            | (TypeRef::FiniteView { value: actual }, TypeRef::FiniteView { value: expected }) => {
                self.value_types_compatible(actual, expected)
            }
            (
                TypeRef::Map {
                    key: actual_key,
                    value: actual_value,
                },
                TypeRef::Map {
                    key: expected_key,
                    value: expected_value,
                },
            )
            | (
                TypeRef::Table {
                    key: actual_key,
                    value: actual_value,
                },
                TypeRef::Table {
                    key: expected_key,
                    value: expected_value,
                },
            ) => {
                self.value_types_compatible(actual_key, expected_key)
                    && self.value_types_compatible(actual_value, expected_value)
            }
            (TypeRef::Tuple { values: actual }, TypeRef::Tuple { values: expected }) => {
                actual.len() == expected.len()
                    && actual
                        .iter()
                        .zip(expected)
                        .all(|(actual, expected)| self.value_types_compatible(actual, expected))
            }
            _ => false,
        }
    }

    fn record_types_compatible(
        &self,
        actual: &[(String, TypeRef)],
        expected: &[(String, TypeRef)],
    ) -> bool {
        actual.len() == expected.len()
            && actual.iter().zip(expected).all(
                |((actual_name, actual), (expected_name, expected))| {
                    actual_name == expected_name && self.value_types_compatible(actual, expected)
                },
            )
    }

    fn type_mismatch(&mut self, actual: &Ty, expected: &Ty, span: ast::SourceSpan) {
        self.diagnostics.push(error(
            codes::TYPE_MISMATCH,
            "uhura/type-mismatch",
            format!(
                "expected `{}`, found `{}`",
                expected.display(),
                actual.display()
            ),
            span,
        ));
    }

    fn arity(&mut self, name: &str, expected: usize, actual: usize, span: ast::SourceSpan) {
        self.diagnostics.push(error(
            codes::ARITY,
            "uhura/arity",
            format!("`{name}` expects {expected} argument(s), found {actual}"),
            span,
        ));
    }

    fn check_exhaustive(
        &mut self,
        subject: Option<&TypeRef>,
        covered: &BTreeSet<String>,
        wildcard: bool,
        span: ast::SourceSpan,
    ) {
        if wildcard {
            return;
        }
        if matches!(subject, Some(TypeRef::Never)) {
            return;
        }
        let patterns = covered
            .iter()
            .filter_map(|item| item.strip_prefix("pattern:"))
            .filter_map(|json| serde_json::from_str::<IrPattern>(json).ok())
            .collect::<Vec<_>>();
        let exhaustive = subject.is_some_and(|ty| self.patterns_cover_type(ty, &patterns));
        if !exhaustive {
            self.diagnostics.push(error(
                codes::NOT_EXHAUSTIVE,
                "uhura/non-exhaustive-match",
                "match is not exhaustive; cover every closed case or add a final wildcard arm",
                span,
            ));
        }
    }

    fn patterns_cover_type(&self, ty: &TypeRef, patterns: &[IrPattern]) -> bool {
        if patterns
            .iter()
            .any(|pattern| self.pattern_covers_type(ty, pattern))
        {
            return true;
        }
        match ty {
            TypeRef::Bool => {
                let mut seen = BTreeSet::new();
                for pattern in flatten_alternatives(patterns) {
                    if let IrPattern::Literal {
                        value: Value::Bool(value),
                    } = pattern
                    {
                        seen.insert(*value);
                    }
                }
                seen.len() == 2
            }
            TypeRef::Option { .. } | TypeRef::Named { .. } => {
                let constructors = self.registry.constructors_for(ty);
                !constructors.is_empty()
                    && constructors.iter().all(|constructor| {
                        let rows = flatten_alternatives(patterns)
                            .into_iter()
                            .filter_map(|pattern| match pattern {
                                IrPattern::Constructor {
                                    type_id,
                                    constructor: name,
                                    fields,
                                } if type_id == &constructor.type_id
                                    && name == &constructor.name =>
                                {
                                    Some(fields.as_slice())
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        self.pattern_rows_cover_product(
                            &constructor
                                .fields
                                .iter()
                                .map(|(_, ty)| ty.clone())
                                .collect::<Vec<_>>(),
                            &rows,
                        )
                    })
            }
            _ => false,
        }
    }

    fn pattern_rows_cover_product(&self, types: &[TypeRef], rows: &[&[IrPattern]]) -> bool {
        if rows.is_empty() {
            return false;
        }
        if types.is_empty() {
            return true;
        }
        if rows
            .iter()
            .any(|row| row.len() == types.len() && row.iter().all(pattern_irrefutable))
        {
            return true;
        }
        (0..types.len()).any(|varying| {
            rows.iter().all(|row| {
                row.len() == types.len()
                    && row
                        .iter()
                        .enumerate()
                        .all(|(index, pattern)| index == varying || pattern_irrefutable(pattern))
            }) && self.patterns_cover_type(
                &types[varying],
                &rows
                    .iter()
                    .map(|row| row[varying].clone())
                    .collect::<Vec<_>>(),
            )
        })
    }

    fn pattern_covers_type(&self, ty: &TypeRef, pattern: &IrPattern) -> bool {
        match pattern {
            IrPattern::Ignore | IrPattern::Bind { .. } => true,
            IrPattern::Alternative { patterns } => self.patterns_cover_type(ty, patterns),
            IrPattern::Tuple { values } => match ty {
                TypeRef::Tuple { values: types } if types.len() == values.len() => types
                    .iter()
                    .zip(values)
                    .all(|(ty, pattern)| self.pattern_covers_type(ty, pattern)),
                _ => false,
            },
            IrPattern::Record { fields, rest } => {
                let Some(expected) = self.registry.fields(ty).or_else(|| match ty {
                    TypeRef::Record { fields } => Some(fields.clone()),
                    _ => None,
                }) else {
                    return false;
                };
                expected.iter().all(|(name, ty)| {
                    fields
                        .iter()
                        .find(|(field, _)| field == name)
                        .is_some_and(|(_, pattern)| self.pattern_covers_type(ty, pattern))
                        || *rest
                })
            }
            IrPattern::Literal { .. } | IrPattern::Constructor { .. } => false,
        }
    }

    fn record_pattern_coverage(
        &mut self,
        pattern: &IrPattern,
        covered: &mut BTreeSet<String>,
        wildcard: &mut bool,
        span: ast::SourceSpan,
    ) {
        if *wildcard {
            self.diagnostics.push(error(
                codes::NOT_EXHAUSTIVE,
                "uhura/arm-after-wildcard",
                "a wildcard match arm is residual and must be final",
                span,
            ));
            return;
        }
        let mut atoms = BTreeSet::new();
        let mut arm_wildcard = false;
        pattern_coverage(pattern, &mut atoms, &mut arm_wildcard);
        let overlaps_prior = !arm_wildcard
            && covered.iter().any(|item| {
                item.strip_prefix("pattern:")
                    .and_then(|json| serde_json::from_str::<IrPattern>(json).ok())
                    .is_some_and(|prior| patterns_overlap(&prior, pattern))
            });
        if overlaps_prior {
            self.diagnostics.push(error(
                codes::NOT_EXHAUSTIVE,
                "uhura/overlapping-match-arm",
                "match arm overlaps an earlier explicit arm",
                span,
            ));
        }
        covered.extend(atoms);
        if let Ok(json) = serde_json::to_string(pattern) {
            covered.insert(format!("pattern:{json}"));
        }
        *wildcard = arm_wildcard;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PatternUse {
    Binding,
    Condition,
    Match,
    Handler,
    Evidence,
}

impl Checker<'_> {
    fn lower_pattern(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &mut Scope,
        pattern: &ast::Pattern,
        expected: Option<&TypeRef>,
        usage: PatternUse,
    ) -> IrPattern {
        match &pattern.value {
            ast::PatternKind::Wildcard | ast::PatternKind::Rest => IrPattern::Ignore,
            ast::PatternKind::Integer(value) => IrPattern::Literal {
                value: exact_number_value(value, expected.unwrap_or(&TypeRef::Int))
                    .unwrap_or_else(|_| exact_integer("0", "Int").expect("zero")),
            },
            ast::PatternKind::Decimal(value) => IrPattern::Literal {
                value: exact_number_value(value, expected.unwrap_or(&TypeRef::Decimal))
                    .unwrap_or_else(|_| exact_decimal("0").expect("zero")),
            },
            ast::PatternKind::Text(value) => IrPattern::Literal {
                value: Value::Text(value.clone()),
            },
            ast::PatternKind::Bool(value) => IrPattern::Literal {
                value: Value::Bool(*value),
            },
            ast::PatternKind::Name(name) => {
                if let Ok(constructor) = self.resolve_constructor(scope, &name.value, expected)
                    && constructor.fields.is_empty()
                {
                    return IrPattern::Constructor {
                        type_id: constructor.type_id,
                        constructor: constructor.name,
                        fields: Vec::new(),
                    };
                }
                if is_binding_reserved_builtin(&name.value) {
                    self.diagnostics.push(error(
                        codes::DUPLICATE,
                        "uhura/reserved-pattern-binding",
                        format!(
                            "`{}` is a binding-reserved Uhura builtin, not a pattern variable",
                            name.value
                        ),
                        name.span,
                    ));
                }
                if usage == PatternUse::Handler {
                    self.diagnostics.push(error(
                        codes::UNKNOWN_NAME,
                        "uhura/unknown-handler-input",
                        format!(
                            "handler input `{}` is not a declared input constructor",
                            name.value
                        ),
                        name.span,
                    ));
                }
                let lowered = if scope.values.contains_key(&name.value) {
                    // Pattern variables are lexically scoped. CPS lowering of
                    // a controlled match may place the continuation inside an
                    // arm, so an arm-local name must never alias an outer
                    // state/config/local binding in the flat runtime locals
                    // map after its source scope has ended.
                    format!(
                        "__uhura_bind_{}_{}_{}",
                        pattern.span.file, pattern.span.start, name.value
                    )
                } else {
                    name.value.clone()
                };
                scope.bind(
                    &name.value,
                    &lowered,
                    Ty::value(expected.cloned().unwrap_or(TypeRef::Never)),
                );
                IrPattern::Bind { name: lowered }
            }
            ast::PatternKind::Constructor { path, arguments } => {
                let qualified = path
                    .iter()
                    .map(|part| part.value.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                if path.len() == 1
                    && let Some(TypeRef::Named { id }) = scope.types.get(&qualified)
                    && let Some(TypeShape::Key(underlying)) =
                        self.registry.shape(&TypeRef::Named { id: id.clone() })
                    && arguments.len() == 1
                {
                    let underlying = underlying.clone();
                    let scalar = match &arguments[0].value {
                        ast::PatternKind::Integer(value) => exact_number_value(value, &underlying),
                        ast::PatternKind::Decimal(value) => exact_number_value(value, &underlying),
                        ast::PatternKind::Text(value) if underlying == TypeRef::Text => {
                            Ok(Value::Text(value.clone()))
                        }
                        _ => Err("key patterns require one exact literal payload".into()),
                    };
                    if let Ok(value) = scalar {
                        return IrPattern::Literal {
                            value: Value::Key {
                                type_id: id.clone(),
                                value: Box::new(value),
                            },
                        };
                    }
                }
                let nominal_constructor = (path.len() == 2)
                    .then(|| scope.types.get(&path[0].value))
                    .flatten()
                    .and_then(|ty| {
                        self.resolve_constructor(scope, &path[1].value, Some(ty))
                            .ok()
                    });
                let constructor = nominal_constructor.or_else(|| {
                    scope
                        .port_receive
                        .get(&qualified)
                        .or_else(|| scope.port_send.get(&qualified))
                        .cloned()
                        .or_else(|| {
                            path.last().and_then(|name| {
                                self.resolve_constructor(scope, &name.value, expected).ok()
                            })
                        })
                });
                let Some(constructor) = constructor else {
                    self.diagnostics.push(error(
                        codes::UNKNOWN_NAME,
                        "uhura/unknown-constructor",
                        format!("unknown or ambiguous constructor `{qualified}`"),
                        pattern.span,
                    ));
                    return IrPattern::Ignore;
                };
                if constructor.fields.len() != arguments.len() {
                    self.arity(
                        &qualified,
                        constructor.fields.len(),
                        arguments.len(),
                        pattern.span,
                    );
                }
                let fields = arguments
                    .iter()
                    .enumerate()
                    .map(|(index, argument)| {
                        let child_usage = if usage == PatternUse::Handler {
                            PatternUse::Binding
                        } else {
                            usage
                        };
                        self.lower_pattern(
                            module,
                            scope,
                            argument,
                            constructor.fields.get(index).map(|(_, ty)| ty),
                            child_usage,
                        )
                    })
                    .collect();
                IrPattern::Constructor {
                    type_id: constructor.type_id,
                    constructor: constructor.name,
                    fields,
                }
            }
            ast::PatternKind::Tuple(values) => {
                let expected_values = match expected {
                    Some(TypeRef::Tuple { values }) => Some(values.as_slice()),
                    _ => None,
                };
                IrPattern::Tuple {
                    values: values
                        .iter()
                        .enumerate()
                        .map(|(index, value)| {
                            self.lower_pattern(
                                module,
                                scope,
                                value,
                                expected_values.and_then(|values| values.get(index)),
                                usage,
                            )
                        })
                        .collect(),
                }
            }
            ast::PatternKind::Record { fields, open } => {
                let expected_fields = expected.and_then(|ty| self.registry.fields(ty));
                IrPattern::Record {
                    fields: fields
                        .iter()
                        .map(|field| {
                            let expected = expected_fields
                                .as_ref()
                                .and_then(|values| {
                                    values.iter().find(|(name, _)| name == &field.name.value)
                                })
                                .map(|(_, ty)| ty);
                            (
                                field.name.value.clone(),
                                self.lower_pattern(module, scope, &field.pattern, expected, usage),
                            )
                        })
                        .collect(),
                    rest: *open,
                }
            }
            ast::PatternKind::Alternative(values) => {
                let original = scope.clone();
                let mut alternatives = Vec::new();
                let mut first_bindings: Option<BTreeMap<String, Binding>> = None;
                for value in values {
                    let mut child = original.clone();
                    alternatives
                        .push(self.lower_pattern(module, &mut child, value, expected, usage));
                    let new = child
                        .values
                        .into_iter()
                        .filter(|(name, _)| !original.values.contains_key(name))
                        .collect::<BTreeMap<_, _>>();
                    if let Some(first) = &first_bindings {
                        if first.keys().collect::<Vec<_>>() != new.keys().collect::<Vec<_>>() {
                            self.diagnostics.push(error(
                                codes::TYPE_MISMATCH,
                                "uhura/alternative-bindings",
                                "every alternative pattern must bind the same names",
                                pattern.span,
                            ));
                        }
                    } else {
                        first_bindings = Some(new);
                    }
                }
                if let Some(bindings) = first_bindings {
                    scope.values.extend(bindings);
                }
                IrPattern::Alternative {
                    patterns: alternatives,
                }
            }
            ast::PatternKind::Error => {
                self.diagnostics.push(error(
                    codes::UNSUPPORTED,
                    "uhura/error-pattern",
                    "cannot lower a recovered parser error pattern",
                    pattern.span,
                ));
                IrPattern::Ignore
            }
        }
    }
}

impl Checker<'_> {
    fn lower_machine(
        &mut self,
        module: &ModuleEnv<'_>,
        declaration: &ast::MachineDecl,
        span: ast::SourceSpan,
    ) {
        self.validate_machine_members(declaration, span);
        let machine_id = qualify(&module.id, &declaration.name.value);
        let mut scope = self.module_scope(module);

        // Machine-local type identities are predeclared so mutually referring
        // records and sums resolve independently of source order.
        for member in &declaration.members {
            match &member.value {
                ast::MachineMemberKind::Key(key) => {
                    scope.types.insert(
                        key.name.value.clone(),
                        TypeRef::Named {
                            id: machine_qualify(
                                &module.id,
                                &declaration.name.value,
                                &key.name.value,
                            ),
                        },
                    );
                }
                ast::MachineMemberKind::Type(ty) => {
                    scope.types.insert(
                        ty.name.value.clone(),
                        TypeRef::Named {
                            id: machine_qualify(
                                &module.id,
                                &declaration.name.value,
                                &ty.name.value,
                            ),
                        },
                    );
                }
                _ => {}
            }
        }
        for member in &declaration.members {
            match &member.value {
                ast::MachineMemberKind::Key(key) => {
                    let id = machine_qualify(&module.id, &declaration.name.value, &key.name.value);
                    let underlying = self.resolve_type(module, &scope, &key.over);
                    self.reject_persisted_finite_view(
                        &underlying,
                        key.over.span,
                        &format!("key `{}`", key.name.value),
                    );
                    self.registry.insert(TypeInfo {
                        id: id.clone(),
                        shape: TypeShape::Key(underlying.clone()),
                    });
                    self.program
                        .machine_program
                        .types
                        .insert(id.clone(), TypeDef::Key { id, underlying });
                }
                ast::MachineMemberKind::Type(ty) => {
                    let id = machine_qualify(&module.id, &declaration.name.value, &ty.name.value);
                    self.install_type_body(module, &scope, &id, &ty.body, member.span);
                }
                _ => {}
            }
        }
        self.populate_scope_constructors(&mut scope);

        // Predeclare local constants, functions, and transitions.
        for member in &declaration.members {
            match &member.value {
                ast::MachineMemberKind::Const(value) => {
                    let ty = self.resolve_type(module, &scope, &value.ty);
                    let id =
                        machine_qualify(&module.id, &declaration.name.value, &value.name.value);
                    scope.bind(&value.name.value, &id, Ty::value(ty));
                }
                ast::MachineMemberKind::Function(value) => {
                    let params = value
                        .parameters
                        .iter()
                        .map(|parameter| self.resolve_type(module, &scope, &parameter.ty))
                        .collect();
                    let result = self.resolve_type(module, &scope, &value.result);
                    scope.functions.insert(
                        value.name.value.clone(),
                        (value.name.value.clone(), params, result),
                    );
                }
                ast::MachineMemberKind::Transition(value) => {
                    let params = value
                        .parameters
                        .iter()
                        .map(|parameter| self.resolve_type(module, &scope, &parameter.ty))
                        .collect();
                    scope
                        .transitions
                        .insert(value.name.value.clone(), (params, value.name.value.clone()));
                }
                _ => {}
            }
        }

        let config_member = unique_member(&declaration.members, |member| match member {
            ast::MachineMemberKind::Config(value) => Some(value),
            _ => None,
        });
        let config = if let Some(config) = config_member {
            let fields = config
                .fields
                .iter()
                .map(|field| {
                    let ty = self.resolve_type(module, &scope, &field.ty);
                    self.reject_persisted_finite_view(
                        &ty,
                        field.ty.span,
                        &format!("machine configuration field `{}`", field.name.value),
                    );
                    scope
                        .config_fields
                        .insert(field.name.value.clone(), ty.clone());
                    scope.bind(&field.name.value, &field.name.value, Ty::value(ty.clone()));
                    (field.name.value.clone(), ty)
                })
                .collect();
            TypeRef::Record { fields }
        } else {
            TypeRef::Unit
        };

        let (input_def, input_constructors) = self.machine_sum_domain(
            module,
            &scope,
            &machine_id,
            "Input",
            unique_member(&declaration.members, |member| match member {
                ast::MachineMemberKind::Input(value) => Some(value),
                _ => None,
            }),
            span,
        );
        let input_ty = TypeRef::Named {
            id: input_def.id().to_string(),
        };
        scope.input_type = Some(input_ty.clone());
        self.registry.insert(TypeInfo {
            id: input_def.id().to_string(),
            shape: TypeShape::Sum(input_constructors.clone()),
        });
        self.program
            .machine_program
            .types
            .insert(input_def.id().to_string(), input_def.clone());
        for constructor in input_constructors {
            scope
                .constructors
                .entry(constructor.name.clone())
                .or_default()
                .push(ConstructorInfo {
                    type_id: input_def.id().into(),
                    name: constructor.name,
                    fields: constructor.fields,
                });
        }

        let (command_def, command_constructors) = self.machine_sum_domain(
            module,
            &scope,
            &machine_id,
            "Command",
            unique_member(&declaration.members, |member| match member {
                ast::MachineMemberKind::Command(value) => Some(value),
                _ => None,
            }),
            span,
        );
        let command_ty = TypeRef::Named {
            id: command_def.id().to_string(),
        };
        scope.command_type = Some(command_ty.clone());
        self.registry.insert(TypeInfo {
            id: command_def.id().to_string(),
            shape: TypeShape::Sum(command_constructors.clone()),
        });
        self.program
            .machine_program
            .types
            .insert(command_def.id().to_string(), command_def.clone());
        for constructor in &command_constructors {
            scope
                .constructors
                .entry(constructor.name.clone())
                .or_default()
                .push(ConstructorInfo {
                    type_id: command_def.id().into(),
                    name: constructor.name.clone(),
                    fields: constructor.fields.clone(),
                });
        }

        let (outcomes, outcome_constructors, outcome_ty) =
            self.machine_outcomes(module, &scope, &machine_id, declaration, span);
        scope.outcome_type = Some(outcome_ty.clone());
        for constructor in &outcome_constructors {
            scope
                .constructors
                .entry(constructor.name.clone())
                .or_default()
                .push(ConstructorInfo {
                    type_id: outcome_ty.canonical_name(),
                    name: constructor.name.clone(),
                    fields: constructor.fields.clone(),
                });
        }

        let ports = declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::Port(port) => {
                    Some(self.lower_port(module, &machine_id, &mut scope, port, member.span))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        // State initialization is deliberately non-sequential. It may use
        // configuration, module/machine constants, constructors, and builtin
        // pure values, but no state field, function, transition, or derive.
        let mut initializer_scope = scope.child();
        initializer_scope.functions.clear();
        initializer_scope.transitions.clear();
        initializer_scope.state_fields.clear();
        let initializer_lookup = initializer_scope.child();
        for requirement in declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::Require(value) => Some(value),
                _ => None,
            })
        {
            install_numeric_condition(
                &mut initializer_scope,
                &initializer_lookup,
                requirement,
                true,
                &self.registry,
            );
        }

        let mut state = Vec::new();
        for state_decl in declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::State(value) => Some(value),
                _ => None,
            })
            .take(1)
        {
            for field in &state_decl.fields {
                let ty = self.resolve_type(module, &scope, &field.ty);
                self.reject_persisted_finite_view(
                    &ty,
                    field.ty.span,
                    &format!("state field `{}`", field.name.value),
                );
                scope
                    .state_fields
                    .insert(field.name.value.clone(), ty.clone());
                scope.bind(&field.name.value, &field.name.value, Ty::value(ty.clone()));
                state.push(StateField {
                    name: field.name.value.clone(),
                    ty,
                    initial: IrExpr::Literal { value: Value::Unit },
                    source: source(module, field.span),
                });
            }
        }
        let mut state_index = 0usize;
        for state_decl in declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::State(value) => Some(value),
                _ => None,
            })
            .take(1)
        {
            for field in &state_decl.fields {
                let expected = state[state_index].ty.clone();
                let (initial, actual) = self.lower_expr(
                    module,
                    &initializer_scope,
                    &field.value,
                    Some(&expected),
                    ExprMode::Pure,
                );
                self.expect_type(&actual, &expected, field.value.span);
                state[state_index].initial = initial;
                state_index += 1;
            }
        }

        let mut functions = BTreeMap::new();
        for member in &declaration.members {
            if let ast::MachineMemberKind::Function(function) = &member.value {
                let mut fn_scope = scope.child();
                let params = function
                    .parameters
                    .iter()
                    .map(|parameter| {
                        let ty = self.resolve_type(module, &fn_scope, &parameter.ty);
                        fn_scope.bind(
                            &parameter.name.value,
                            &parameter.name.value,
                            Ty::value(ty.clone()),
                        );
                        (parameter.name.value.clone(), ty)
                    })
                    .collect::<Vec<_>>();
                let result = self.resolve_type(module, &fn_scope, &function.result);
                if reaction_control(&function.body) {
                    self.diagnostics.push(error(
                        codes::EFFECT,
                        "uhura/effect-in-pure-function",
                        format!(
                            "machine function `{}` contains reaction control",
                            function.name.value
                        ),
                        function.body.span,
                    ));
                    continue;
                }
                let (body, actual) = self.lower_expr(
                    module,
                    &fn_scope,
                    &function.body,
                    Some(&result),
                    ExprMode::Pure,
                );
                self.expect_type(&actual, &result, function.body.span);
                functions.insert(
                    function.name.value.clone(),
                    IrFunction {
                        id: function.name.value.clone(),
                        params,
                        result,
                        body,
                        source: source(module, member.span),
                    },
                );
            }
        }

        // Machine-local constants are immutable program constants with a
        // machine-qualified identity, so runtime lookup remains collision-free.
        for member in &declaration.members {
            if let ast::MachineMemberKind::Const(value) = &member.value {
                let expected = self.resolve_type(module, &scope, &value.ty);
                self.reject_persisted_finite_view(
                    &expected,
                    value.ty.span,
                    &format!("machine constant `{}`", value.name.value),
                );
                let (expression, actual) = self.lower_expr(
                    module,
                    &scope,
                    &value.value,
                    Some(&expected),
                    ExprMode::Pure,
                );
                self.expect_type(&actual, &expected, value.value.span);
                match const_eval(&expression, &self.program) {
                    Ok(constant) => {
                        let id =
                            machine_qualify(&module.id, &declaration.name.value, &value.name.value);
                        self.program
                            .machine_program
                            .constants
                            .insert(id.clone(), constant);
                        self.program
                            .machine_program
                            .constant_types
                            .insert(id, expected);
                    }
                    Err(message) => self.diagnostics.push(error(
                        codes::EFFECT,
                        "uhura/non-constant-expression",
                        format!(
                            "machine constant `{}` is not total: {message}",
                            value.name.value
                        ),
                        member.span,
                    )),
                }
            }
        }

        let mut derives = Vec::new();
        let derive_names = declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::Derive(value) => Some(value.name.value.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let inferred_names = declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::Derive(value) if value.ty.is_none() => {
                    Some(value.name.value.clone())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();

        // Explicit signatures are visible independent of declaration order.
        for member in &declaration.members {
            if let ast::MachineMemberKind::Derive(value) = &member.value
                && let Some(annotation) = &value.ty
            {
                let ty = self.resolve_type(module, &scope, annotation);
                scope.bind(&value.name.value, &value.name.value, Ty::value(ty));
            }
        }

        // Unannotated derives are inferred in dependency order. The probe
        // lowering is diagnostic-free because safety facts from invariants are
        // installed only after every derive has a type; the authoritative
        // lowering below runs once with the complete scope.
        let mut inferred_types = BTreeMap::new();
        let mut pending = inferred_names.clone();
        while !pending.is_empty() {
            let ready = declaration
                .members
                .iter()
                .filter_map(|member| match &member.value {
                    ast::MachineMemberKind::Derive(value)
                        if pending.contains(&value.name.value) =>
                    {
                        let mut dependencies = BTreeSet::new();
                        collect_source_names(&value.value, &mut BTreeSet::new(), &mut dependencies);
                        dependencies.retain(|name| derive_names.contains(name));
                        (!dependencies.iter().any(|name| pending.contains(name)))
                            .then_some(value.name.value.clone())
                    }
                    _ => None,
                })
                .collect::<BTreeSet<_>>();
            if ready.is_empty() {
                for name in &pending {
                    let member = declaration
                        .members
                        .iter()
                        .find(|member| {
                            matches!(
                                &member.value,
                                ast::MachineMemberKind::Derive(value)
                                    if &value.name.value == name
                            )
                        })
                        .expect("pending derive is declared");
                    self.diagnostics.push(error(
                        codes::DEPENDENCY_CYCLE,
                        "uhura/recursive-derive-inference",
                        format!(
                            "computed value `{name}` needs an explicit type because its inferred-type dependencies are cyclic"
                        ),
                        member.span,
                    ));
                    scope.bind(name, name, Ty::value(TypeRef::Never));
                    inferred_types.insert(name.clone(), TypeRef::Never);
                }
                break;
            }
            for name in ready {
                let value = declaration
                    .members
                    .iter()
                    .find_map(|member| match &member.value {
                        ast::MachineMemberKind::Derive(value) if value.name.value == name => {
                            Some(value)
                        }
                        _ => None,
                    })
                    .expect("ready derive is declared");
                let diagnostic_count = self.diagnostics.len();
                let (_, actual) =
                    self.lower_expr(module, &scope, &value.value, None, ExprMode::Projection);
                self.diagnostics.truncate(diagnostic_count);
                let ty = actual.into_value().unwrap_or(TypeRef::Never);
                if !inferred_type_is_complete(&ty) {
                    self.diagnostics.push(error(
                        codes::TYPE_MISMATCH,
                        "uhura/derive-type-inference",
                        format!(
                            "computed value `{name}` does not have one complete inferable type; add an explicit type annotation"
                        ),
                        value.value.span,
                    ));
                }
                scope.bind(&name, &name, Ty::value(ty.clone()));
                inferred_types.insert(name.clone(), ty);
                pending.remove(&name);
            }
        }
        let invariant_lookup = scope.child();
        for expression in declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::Invariant(value) => Some(&value.expressions),
                _ => None,
            })
            .flatten()
        {
            install_numeric_condition(
                &mut scope,
                &invariant_lookup,
                expression,
                true,
                &self.registry,
            );
        }
        for member in &declaration.members {
            if let ast::MachineMemberKind::Derive(value) = &member.value {
                let ty = value
                    .ty
                    .as_ref()
                    .map(|annotation| self.resolve_type(module, &scope, annotation))
                    .or_else(|| inferred_types.get(&value.name.value).cloned())
                    .unwrap_or(TypeRef::Never);
                let (expression, actual) = self.lower_expr(
                    module,
                    &scope,
                    &value.value,
                    Some(&ty),
                    ExprMode::Projection,
                );
                self.expect_type(&actual, &ty, value.value.span);
                derives.push((
                    value.name.value.clone(),
                    ty,
                    expression,
                    source(module, member.span),
                ));
            }
        }

        let requires = declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::Require(value) => Some((value, member.span)),
                _ => None,
            })
            .map(|(value, span)| {
                let (value, ty) =
                    self.lower_expr(module, &scope, value, Some(&TypeRef::Bool), ExprMode::Pure);
                self.expect_type(&ty, &TypeRef::Bool, span);
                (value, source(module, span))
            })
            .collect::<Vec<_>>();

        let mut invariants = Vec::new();
        for (value, span) in declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::Invariant(value) => Some((value, member.span)),
                _ => None,
            })
        {
            for expression in &value.expressions {
                let (expression_ir, ty) = self.lower_expr(
                    module,
                    &scope,
                    expression,
                    Some(&TypeRef::Bool),
                    ExprMode::Projection,
                );
                self.expect_type(&ty, &TypeRef::Bool, expression.span);
                invariants.push((expression_ir, source(module, span)));
            }
        }

        let mut observation = Vec::new();
        for observe in declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::Observe(value) => Some(value),
                _ => None,
            })
            .take(1)
        {
            for field in &observe.fields {
                let declared = field
                    .ty
                    .as_ref()
                    .map(|ty| self.resolve_type(module, &scope, ty));
                let (expression, actual) = self.lower_expr(
                    module,
                    &scope,
                    &field.value,
                    declared.as_ref(),
                    ExprMode::Projection,
                );
                let ty = declared
                    .or_else(|| actual.into_value())
                    .unwrap_or(TypeRef::Never);
                self.reject_persisted_finite_view(
                    &ty,
                    field
                        .ty
                        .as_ref()
                        .map_or(field.value.span, |declared| declared.span),
                    &format!("observation field `{}`", field.name.value),
                );
                observation.push(ObservationField {
                    name: field.name.value.clone(),
                    ty,
                    expression,
                    source: source(module, field.span),
                });
            }
        }

        let mut transitions = BTreeMap::new();
        for member in &declaration.members {
            if let ast::MachineMemberKind::Transition(value) = &member.value {
                let mut transition_scope = scope.child();
                let params = value
                    .parameters
                    .iter()
                    .map(|parameter| {
                        let ty = self.resolve_type(module, &transition_scope, &parameter.ty);
                        transition_scope.bind(
                            &parameter.name.value,
                            &parameter.name.value,
                            Ty::value(ty.clone()),
                        );
                        (parameter.name.value.clone(), ty)
                    })
                    .collect::<Vec<_>>();
                let body = self.lower_reaction_block(
                    module,
                    &transition_scope,
                    &value.body,
                    &outcome_ty,
                    Vec::new(),
                );
                if !statements_terminal(&body) {
                    self.diagnostics.push(error(
                        codes::TRANSITION_SHAPE,
                        "uhura/transition-fallthrough",
                        format!(
                            "transition `{}` may fall through without `finish`",
                            value.name.value
                        ),
                        member.span,
                    ));
                }
                transitions.insert(
                    value.name.value.clone(),
                    IrTransition {
                        name: value.name.value.clone(),
                        params,
                        body,
                        source: source(module, member.span),
                    },
                );
            }
        }

        let mut handlers = BTreeMap::new();
        for member in &declaration.members {
            if let ast::MachineMemberKind::Handler(handler) = &member.value {
                let input_name = handler_input_name(&handler.input);
                if handlers.contains_key(&input_name) {
                    self.diagnostics.push(error(
                        codes::INPUT_COVERAGE,
                        "uhura/duplicate-handler",
                        format!("input `{input_name}` has more than one handler"),
                        handler.input.span,
                    ));
                    continue;
                }
                let mut handler_scope = scope.child();
                let expected = scope
                    .port_receive
                    .get(&input_name)
                    .map(|constructor| TypeRef::Named {
                        id: constructor.type_id.clone(),
                    })
                    .or_else(|| Some(input_ty.clone()));
                let pattern = self.lower_pattern(
                    module,
                    &mut handler_scope,
                    &handler.input,
                    expected.as_ref(),
                    PatternUse::Handler,
                );
                let body = match &handler.body {
                    ast::HandlerBody::Block(block) => self.lower_reaction_block(
                        module,
                        &handler_scope,
                        block,
                        &outcome_ty,
                        Vec::new(),
                    ),
                    ast::HandlerBody::Delegate(expression) => {
                        self.lower_delegate(module, &handler_scope, expression, member.span)
                    }
                };
                if !statements_terminal(&body) {
                    self.diagnostics.push(error(
                        codes::TRANSITION_SHAPE,
                        "uhura/handler-fallthrough",
                        format!("handler `{input_name}` may fall through without `finish`"),
                        member.span,
                    ));
                }
                handlers.insert(
                    input_name.clone(),
                    IrHandler {
                        input: input_name,
                        pattern,
                        body,
                        source: source(module, member.span),
                    },
                );
            }
        }
        let expected_handlers = match &input_def {
            TypeDef::Sum { constructors, .. } => constructors
                .iter()
                .map(|constructor| constructor.name.clone())
                .chain(ports.iter().flat_map(|port| {
                    port.receive
                        .iter()
                        .map(|constructor| format!("{}.{}", port.name, constructor.name))
                }))
                .collect::<BTreeSet<_>>(),
            _ => BTreeSet::new(),
        };
        let actual_handlers = handlers.keys().cloned().collect::<BTreeSet<_>>();
        for input in expected_handlers.difference(&actual_handlers) {
            self.diagnostics.push(error(
                codes::INPUT_COVERAGE,
                "uhura/missing-handler",
                format!("input `{input}` has no handler"),
                span,
            ));
        }
        for input in actual_handlers.difference(&expected_handlers) {
            self.diagnostics.push(error(
                codes::INPUT_COVERAGE,
                "uhura/extra-handler",
                format!("handler `{input}` does not belong to the machine input domain"),
                handlers
                    .get(input)
                    .map(|handler| self.physical_span(&handler.source))
                    .unwrap_or(span),
            ));
        }

        let before_commit = declaration
            .members
            .iter()
            .filter_map(|member| match &member.value {
                ast::MachineMemberKind::BeforeCommit(value) => Some((value, member.span)),
                _ => None,
            })
            .next()
            .map(|(block, span)| {
                if block_contains_finish_control(block) {
                    self.diagnostics.push(error(
                        codes::EFFECT,
                        "uhura/terminal-in-before-commit",
                        "`before commit` may reconcile the draft or fault, but cannot finish or replace the selected outcome",
                        span,
                    ));
                }
                self.lower_reaction_block(module, &scope, block, &outcome_ty, Vec::new())
            })
            .unwrap_or_default();

        let local_commands = command_constructors
            .into_iter()
            .map(|constructor| CommandDef {
                constructor,
                source: source(module, span),
            })
            .collect();

        self.program.machine_program.machines.insert(
            machine_id.clone(),
            IrMachine {
                id: machine_id,
                config,
                requires,
                ports,
                local_input: input_def,
                local_commands,
                outcomes,
                state,
                functions,
                derives,
                invariants,
                observation,
                transitions,
                handlers,
                before_commit,
                source: source(module, span),
            },
        );
    }

    fn validate_machine_members(
        &mut self,
        declaration: &ast::MachineDecl,
        machine_span: ast::SourceSpan,
    ) {
        let mut singleton_spans: BTreeMap<&'static str, Vec<ast::SourceSpan>> = BTreeMap::new();
        let mut require_spans = Vec::new();
        for member in &declaration.members {
            let name = match &member.value {
                ast::MachineMemberKind::Config(_) => Some("config"),
                ast::MachineMemberKind::Input(_) => Some("input"),
                ast::MachineMemberKind::Command(_) => Some("command"),
                ast::MachineMemberKind::Outcome(_) => Some("outcome"),
                ast::MachineMemberKind::State(_) => Some("state"),
                ast::MachineMemberKind::Observe(_) => Some("observe"),
                ast::MachineMemberKind::BeforeCommit(_) => Some("before commit"),
                ast::MachineMemberKind::Require(_) => {
                    require_spans.push(member.span);
                    None
                }
                _ => None,
            };
            if let Some(name) = name {
                singleton_spans.entry(name).or_default().push(member.span);
            }
        }

        for required in ["input", "command", "outcome", "state", "observe"] {
            if singleton_spans.get(required).is_none_or(Vec::is_empty) {
                self.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura/missing-machine-member",
                    format!("machine must declare exactly one `{required}` member"),
                    machine_span,
                ));
            }
        }
        for (name, spans) in singleton_spans {
            for span in spans.into_iter().skip(1) {
                self.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura/duplicate-machine-member",
                    format!("machine may not declare `{name}` more than once"),
                    span,
                ));
            }
        }
        if !require_spans.is_empty()
            && !declaration
                .members
                .iter()
                .any(|member| matches!(member.value, ast::MachineMemberKind::Config(_)))
        {
            for span in require_spans {
                self.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura/require-without-config",
                    "`require` is valid only for a machine with `config`",
                    span,
                ));
            }
        }
    }

    fn populate_scope_constructors(&self, scope: &mut Scope) {
        for ty in scope.types.values() {
            for constructor in self.registry.constructors_for(ty) {
                scope
                    .constructors
                    .entry(constructor.name.clone())
                    .or_default()
                    .push(constructor);
            }
        }
    }

    fn machine_sum_domain(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        machine_id: &str,
        name: &str,
        domain: Option<&ast::SumDomain>,
        span: ast::SourceSpan,
    ) -> (TypeDef, Vec<ConstructorDef>) {
        let id = format!("{machine_id}.{name}");
        let constructors = match domain {
            Some(ast::SumDomain::Never(_)) | None => Vec::new(),
            Some(ast::SumDomain::Sum(sum)) => sum
                .variants
                .iter()
                .map(|variant| {
                    let constructor = self.lower_constructor_def(module, scope, variant);
                    self.reject_persisted_constructor_finite_views(
                        &constructor,
                        variant,
                        &name.to_ascii_lowercase(),
                    );
                    constructor
                })
                .collect(),
        };
        let _ = span;
        (
            TypeDef::Sum {
                id,
                constructors: constructors.clone(),
            },
            constructors,
        )
    }

    fn machine_outcomes(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        machine_id: &str,
        declaration: &ast::MachineDecl,
        _span: ast::SourceSpan,
    ) -> (Vec<OutcomeDef>, Vec<ConstructorDef>, TypeRef) {
        let id = format!("{machine_id}.Outcome");
        let Some(outcome) = unique_member(&declaration.members, |member| match member {
            ast::MachineMemberKind::Outcome(value) => Some(value),
            _ => None,
        }) else {
            return (Vec::new(), Vec::new(), TypeRef::Named { id });
        };
        let mut definitions = Vec::new();
        let mut constructors = Vec::new();
        for value in &outcome.variants {
            let constructor = self.lower_constructor_def(module, scope, &value.variant);
            self.reject_persisted_constructor_finite_views(&constructor, &value.variant, "outcome");
            definitions.push(OutcomeDef {
                constructor: constructor.clone(),
                policy: match value.policy.value {
                    ast::OutcomePolicy::Commit => IrOutcomePolicy::Commit,
                    ast::OutcomePolicy::Abort => IrOutcomePolicy::Abort,
                },
                source: source(module, value.span),
            });
            constructors.push(constructor);
        }
        self.registry.insert(TypeInfo {
            id: id.clone(),
            shape: TypeShape::Sum(constructors.clone()),
        });
        self.program.machine_program.types.insert(
            id.clone(),
            TypeDef::Sum {
                id: id.clone(),
                constructors: constructors.clone(),
            },
        );
        (definitions, constructors, TypeRef::Named { id })
    }

    fn lower_port(
        &mut self,
        module: &ModuleEnv<'_>,
        machine_id: &str,
        scope: &mut Scope,
        port: &ast::PortDecl,
        span: ast::SourceSpan,
    ) -> PortDef {
        let ast::TypeExprKind::Named { path, arguments } = &port.contract.value else {
            self.diagnostics.push(error(
                codes::PORT,
                "uhura/port-contract",
                "port contracts must be named generic standard contracts",
                port.contract.span,
            ));
            return PortDef {
                name: port.name.value.clone(),
                contract: "<invalid>".into(),
                contract_instance: None,
                type_arguments: Vec::new(),
                configuration: None,
                receive: Vec::new(),
                send: Vec::new(),
                contract_hash: String::new(),
                source: source(module, span),
            };
        };
        let contract = path.last().map(|name| name.value.as_str()).unwrap_or("");
        let type_arguments = arguments
            .iter()
            .map(|argument| self.resolve_type(module, scope, argument))
            .collect::<Vec<_>>();
        for (index, (argument, source)) in type_arguments.iter().zip(arguments).enumerate() {
            self.reject_persisted_finite_view(
                argument,
                source.span,
                &format!(
                    "port `{}` contract type argument #{}",
                    port.name.value,
                    index + 1
                ),
            );
        }
        let configuration = port.configuration.first().map(|value| {
            let (expression, ty) = self.lower_expr(module, scope, value, None, ExprMode::Pure);
            if let Some(ty) = ty.as_value() {
                self.reject_persisted_finite_view(
                    ty,
                    value.span,
                    &format!("port `{}` configuration", port.name.value),
                );
            }
            expression
        });
        let (receive, send, contract_instance, expected_arity) = match contract {
            "Observation" => {
                let value = type_arguments.first().cloned().unwrap_or(TypeRef::Never);
                let instance = uhura_port::observation_instance(port_ty(&value));
                (
                    vec![ConstructorDef {
                        name: "observed".into(),
                        fields: vec![(Some("value".into()), value)],
                    }],
                    Vec::new(),
                    Some(instance),
                    1,
                )
            }
            "RequestPort" => {
                let id = type_arguments.first().cloned().unwrap_or(TypeRef::Never);
                let payload = type_arguments.get(1).cloned().unwrap_or(TypeRef::Never);
                let settlement = type_arguments.get(2).cloned().unwrap_or(TypeRef::Never);
                let instance = uhura_port::request_port_instance(
                    port_ty(&id),
                    port_ty(&payload),
                    port_ty(&settlement),
                );
                (
                    vec![ConstructorDef {
                        name: "settled".into(),
                        fields: vec![
                            (Some("id".into()), id.clone()),
                            (Some("result".into()), settlement),
                        ],
                    }],
                    vec![ConstructorDef {
                        name: "request".into(),
                        fields: vec![(Some("id".into()), id), (Some("payload".into()), payload)],
                    }],
                    Some(instance),
                    3,
                )
            }
            "SinkPort" => {
                let value = type_arguments.first().cloned().unwrap_or(TypeRef::Never);
                let instance = uhura_port::sink_port_instance(port_ty(&value));
                (
                    Vec::new(),
                    vec![ConstructorDef {
                        name: "send".into(),
                        fields: vec![(Some("value".into()), value)],
                    }],
                    Some(instance),
                    1,
                )
            }
            "Router" => {
                let location = type_arguments.first().cloned().unwrap_or(TypeRef::Never);
                let route_id = port
                    .configuration
                    .first()
                    .and_then(|value| match &value.value {
                        ast::ExprKind::Name(name) => scope.values.get(&name.value),
                        _ => None,
                    })
                    .map(|binding| binding.lowered.clone());
                let instance = route_id
                    .as_ref()
                    .and_then(|id| self.program.route_tables.get(id))
                    .map(|routes| uhura_port::router_instance(port_ty(&location), routes));
                (
                    vec![ConstructorDef {
                        name: "changed".into(),
                        fields: vec![(Some("location".into()), location.clone())],
                    }],
                    vec![
                        ConstructorDef {
                            name: "push".into(),
                            fields: vec![(Some("location".into()), location.clone())],
                        },
                        ConstructorDef {
                            name: "replace".into(),
                            fields: vec![(Some("location".into()), location)],
                        },
                        ConstructorDef {
                            name: "back".into(),
                            fields: Vec::new(),
                        },
                    ],
                    instance,
                    1,
                )
            }
            other => {
                self.diagnostics.push(error(
                    codes::PORT,
                    "uhura/unknown-port-contract",
                    format!("unsupported or unresolved port contract `{other}`"),
                    port.contract.span,
                ));
                (Vec::new(), Vec::new(), None, type_arguments.len())
            }
        };
        if type_arguments.len() != expected_arity {
            self.arity(
                contract,
                expected_arity,
                type_arguments.len(),
                port.contract.span,
            );
        }
        let contract_instance = contract_instance.and_then(|instance| match instance {
            Ok(instance) => Some(instance),
            Err(model_error) => {
                self.diagnostics.push(error(
                    codes::PORT,
                    "uhura/port-contract-instance",
                    model_error.to_string(),
                    port.contract.span,
                ));
                None
            }
        });
        let contract_identity = contract_instance
            .as_ref()
            .map(|instance| instance.identity.to_string())
            .unwrap_or_else(|| contract.to_string());
        let contract_hash = contract_instance
            .as_ref()
            .map(|instance| instance.content_hash.clone())
            .unwrap_or_default();
        for constructor in &receive {
            let qualified = format!("{}.{}", port.name.value, constructor.name);
            scope.port_receive.insert(
                qualified.clone(),
                ConstructorInfo {
                    type_id: format!("{machine_id}::port.{}.Receive", port.name.value),
                    name: qualified,
                    fields: constructor.fields.clone(),
                },
            );
        }
        for constructor in &send {
            let qualified = format!("{}.{}", port.name.value, constructor.name);
            scope.port_send.insert(
                qualified.clone(),
                ConstructorInfo {
                    type_id: format!("{machine_id}::port.{}.Send", port.name.value),
                    name: qualified,
                    fields: constructor.fields.clone(),
                },
            );
        }
        PortDef {
            name: port.name.value.clone(),
            contract: contract_identity,
            contract_instance,
            type_arguments,
            configuration,
            receive,
            send,
            contract_hash,
            source: source(module, span),
        }
    }

    fn lower_delegate(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        expression: &ast::Expr,
        span: ast::SourceSpan,
    ) -> Vec<Statement> {
        let ast::ExprKind::Call { callee, arguments } = &expression.value else {
            self.diagnostics.push(error(
                codes::TRANSITION_SHAPE,
                "uhura/delegate-shape",
                "expression-bodied handlers must call one named transition",
                expression.span,
            ));
            return Vec::new();
        };
        let ast::ExprKind::Name(name) = &callee.value else {
            self.diagnostics.push(error(
                codes::TRANSITION_SHAPE,
                "uhura/delegate-shape",
                "handler delegate target must be a transition name",
                callee.span,
            ));
            return Vec::new();
        };
        let Some((params, lowered)) = scope.transitions.get(&name.value) else {
            self.diagnostics.push(error(
                codes::TRANSITION_SHAPE,
                "uhura/unknown-transition",
                format!("unknown transition `{}`", name.value),
                name.span,
            ));
            return Vec::new();
        };
        if params.len() != arguments.len() {
            self.arity(&name.value, params.len(), arguments.len(), expression.span);
        }
        let args = arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                self.lower_expr(
                    module,
                    scope,
                    argument,
                    params.get(index),
                    ExprMode::Reaction,
                )
                .0
            })
            .collect();
        vec![Statement::Delegate {
            transition: lowered.clone(),
            args,
            source: source(module, span),
        }]
    }
}

impl Checker<'_> {
    fn lower_presentations(&mut self) {
        let deferred = self.presentations.clone();
        for value in deferred {
            let Some(module) = self.modules.get(&value.module).cloned() else {
                continue;
            };
            if !module.features.contains("ui") {
                self.diagnostics.push(error(
                    codes::UI_NOT_ENABLED,
                    "uhura/ui-without-use",
                    "UI declarations require `use ui`",
                    value.span,
                ));
            }
            let Some(machine_id) = self.resolve_machine(&module, &value.declaration.machine) else {
                continue;
            };
            let Some(machine) = self
                .program
                .machine_program
                .machines
                .get(&machine_id)
                .cloned()
            else {
                self.diagnostics.push(error(
                    codes::UNKNOWN_NAME,
                    "uhura/ui-machine-unavailable",
                    format!("machine `{machine_id}` was not checked before this presentation"),
                    value.declaration.machine.span,
                ));
                continue;
            };
            let mut scope = self.module_scope(&module);
            self.populate_scope_constructors(&mut scope);
            let observation_ty = TypeRef::Record {
                fields: machine
                    .observation
                    .iter()
                    .map(|field| (field.name.clone(), field.ty.clone()))
                    .collect(),
            };
            scope.bind(
                &value.declaration.binding.value,
                &value.declaration.binding.value,
                Ty::value(observation_ty),
            );
            self.install_machine_io_scope(&machine, &mut scope);
            let nodes = self.lower_ui_nodes(&module, &scope, &value.declaration.nodes, &machine);
            let id = qualify(&module.id, &value.declaration.name.value);
            self.program.presentations.insert(
                id.clone(),
                Presentation {
                    id,
                    machine: machine_id,
                    binding: value.declaration.binding.value,
                    nodes,
                    source: source(&module, value.span),
                },
            );
        }
    }

    fn resolve_machine(&mut self, module: &ModuleEnv<'_>, name: &ast::Name) -> Option<String> {
        match module.lookup(&name.value) {
            Some(Export::Machine { id }) => Some(id.clone()),
            _ => {
                self.diagnostics.push(error(
                    codes::UNKNOWN_NAME,
                    "uhura/unknown-machine",
                    format!("`{}` does not resolve to a machine", name.value),
                    name.span,
                ));
                None
            }
        }
    }

    fn resolve_presentation(&mut self, module: &ModuleEnv<'_>, name: &ast::Name) -> Option<String> {
        match module.lookup(&name.value) {
            Some(Export::Presentation { id }) => Some(id.clone()),
            _ => {
                self.diagnostics.push(error(
                    codes::UNKNOWN_NAME,
                    "uhura/unknown-presentation",
                    format!("`{}` does not resolve to a UI presentation", name.value),
                    name.span,
                ));
                None
            }
        }
    }

    fn install_machine_io_scope(&self, machine: &IrMachine, scope: &mut Scope) {
        if let TypeDef::Sum { id, constructors } = &machine.local_input {
            let ty = TypeRef::Named { id: id.clone() };
            scope.input_type = Some(ty);
            for constructor in constructors {
                scope
                    .constructors
                    .entry(constructor.name.clone())
                    .or_default()
                    .push(ConstructorInfo {
                        type_id: id.clone(),
                        name: constructor.name.clone(),
                        fields: constructor.fields.clone(),
                    });
            }
        }
        let outcome_id = format!("{}.Outcome", machine.id);
        scope.outcome_type = Some(TypeRef::Named {
            id: outcome_id.clone(),
        });
        for outcome in &machine.outcomes {
            scope
                .constructors
                .entry(outcome.constructor.name.clone())
                .or_default()
                .push(ConstructorInfo {
                    type_id: outcome_id.clone(),
                    name: outcome.constructor.name.clone(),
                    fields: outcome.constructor.fields.clone(),
                });
        }
        let command_id = format!("{}.Command", machine.id);
        scope.command_type = Some(TypeRef::Named {
            id: command_id.clone(),
        });
        for command in &machine.local_commands {
            scope
                .constructors
                .entry(command.constructor.name.clone())
                .or_default()
                .push(ConstructorInfo {
                    type_id: command_id.clone(),
                    name: command.constructor.name.clone(),
                    fields: command.constructor.fields.clone(),
                });
        }
        for port in &machine.ports {
            for constructor in &port.receive {
                let name = format!("{}.{}", port.name, constructor.name);
                scope.port_receive.insert(
                    name.clone(),
                    ConstructorInfo {
                        type_id: format!("{}::port.{}.Receive", machine.id, port.name),
                        name,
                        fields: constructor.fields.clone(),
                    },
                );
            }
            for constructor in &port.send {
                let name = format!("{}.{}", port.name, constructor.name);
                scope.port_send.insert(
                    name.clone(),
                    ConstructorInfo {
                        type_id: format!("{}::port.{}.Send", machine.id, port.name),
                        name,
                        fields: constructor.fields.clone(),
                    },
                );
            }
        }
    }

    fn lower_ui_nodes(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        nodes: &[ast::UiNode],
        machine: &IrMachine,
    ) -> Vec<IrUiNode> {
        nodes
            .iter()
            .map(|node| match &node.value {
                ast::UiNodeKind::Text(value) => IrUiNode::Text {
                    value: value.clone(),
                    source: source(module, node.span),
                },
                ast::UiNodeKind::Interpolation(value) => {
                    let (value, ty) =
                        self.lower_expr(module, scope, value, None, ExprMode::Ui);
                    if !ty
                        .as_value()
                        .is_some_and(|ty| self.ui_scalar_type(ty))
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-interpolation-type",
                            "UI interpolation requires Text, Bool, an exact numeric scalar, or a nominal scalar key",
                            node.span,
                        ));
                    }
                    IrUiNode::Interpolation {
                        value,
                        source: source(module, node.span),
                    }
                }
                ast::UiNodeKind::Element(element) => {
                    self.check_ui_element_shape(module, element, node.span);
                    let attributes = element
                        .attributes
                        .iter()
                        .map(|attribute| {
                            let value = match &attribute.value {
                                ast::UiAttributeValue::Text(value) => IrUiAttributeValue::Text {
                                    value: value.clone(),
                                },
                                ast::UiAttributeValue::Expression(value) => {
                                    let expected =
                                        self.ui_attribute_expected_type(element, &attribute.name);
                                    let (value, ty) = self.lower_expr(
                                        module,
                                        scope,
                                        value,
                                        expected.as_ref(),
                                        ExprMode::Ui,
                                    );
                                    if let Some(expected) = expected {
                                        self.expect_type(&ty, &expected, attribute.span);
                                    }
                                    self.check_ui_attribute_type(
                                        element,
                                        &attribute.name,
                                        &ty,
                                        attribute.span,
                                    );
                                    IrUiAttributeValue::Expression { value }
                                }
                                ast::UiAttributeValue::Event { event, input } => {
                                    let event_payload = self.check_ui_event(
                                        module,
                                        element,
                                        &event.value,
                                        event.span,
                                    );
                                    let mut event_scope = scope.child();
                                    if let Some(event_payload) = event_payload {
                                        event_scope.bind(
                                            "event",
                                            "event",
                                            Ty::value(event_payload),
                                        );
                                    }
                                    let (input, actual) = self.lower_expr(
                                        module,
                                        &event_scope,
                                        input,
                                        scope.input_type.as_ref(),
                                        ExprMode::Ui,
                                    );
                                    if let Some(expected) = &scope.input_type {
                                        self.expect_type(&actual, expected, attribute.span);
                                    }
                                    IrUiAttributeValue::Event {
                                        event: event.value.clone(),
                                        input,
                                    }
                                }
                            };
                            IrUiAttribute {
                                name: attribute.name.clone(),
                                value,
                                source: source(module, attribute.span),
                            }
                        })
                        .collect();
                    IrUiNode::Element {
                        name: element.name.value.clone(),
                        attributes,
                        children: self.lower_ui_nodes(module, scope, &element.children, machine),
                        source: source(module, node.span),
                    }
                }
                ast::UiNodeKind::If {
                    condition,
                    children,
                } => {
                    let (condition, refined) =
                        self.lower_condition(module, scope, condition, ExprMode::Ui);
                    IrUiNode::If {
                        condition,
                        children: self.lower_ui_nodes(module, &refined, children, machine),
                        source: source(module, node.span),
                    }
                }
                ast::UiNodeKind::Match { subject, cases } => {
                    let (value, ty) = self.lower_expr(module, scope, subject, None, ExprMode::Ui);
                    let mut covered = BTreeSet::new();
                    let mut wildcard = false;
                    let cases = cases
                        .iter()
                        .map(|case| {
                            let mut child = scope.child();
                            let pattern = self.lower_pattern(
                                module,
                                &mut child,
                                &case.pattern,
                                ty.as_value(),
                                PatternUse::Match,
                            );
                            self.record_pattern_coverage(
                                &pattern,
                                &mut covered,
                                &mut wildcard,
                                case.pattern.span,
                            );
                            IrUiCase {
                                pattern,
                                children: self.lower_ui_nodes(
                                    module,
                                    &child,
                                    &case.children,
                                    machine,
                                ),
                                source: source(module, case.span),
                            }
                        })
                        .collect();
                    self.check_exhaustive(ty.as_value(), &covered, wildcard, node.span);
                    IrUiNode::Match {
                        value,
                        cases,
                        source: source(module, node.span),
                    }
                }
                ast::UiNodeKind::Each {
                    source: collection,
                    pattern,
                    key,
                    children,
                } => {
                    let (value, ty) =
                        self.lower_expr(module, scope, collection, None, ExprMode::Ui);
                    if !matches!(ty.as_value(), Some(TypeRef::Seq { .. })) {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-each-source",
                            "UI `each` accepts only a semantically ordered `Seq`",
                            collection.span,
                        ));
                    }
                    let item = collection_item_type(ty.as_value()).unwrap_or(TypeRef::Never);
                    let mut child = scope.child();
                    let pattern = self.lower_pattern(
                        module,
                        &mut child,
                        pattern,
                        Some(&item),
                        PatternUse::Binding,
                    );
                    let (key, key_ty) =
                        self.lower_expr(module, &child, key, None, ExprMode::Ui);
                    if !key_ty
                        .as_value()
                        .is_some_and(|ty| self.ui_key_type(ty))
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-each-key-type",
                            "UI repetition keys must be scalar or nominal scalar-key values",
                            node.span,
                        ));
                    }
                    IrUiNode::Each {
                        value,
                        pattern,
                        key: Box::new(key),
                        children: self.lower_ui_nodes(module, &child, children, machine),
                        source: source(module, node.span),
                    }
                }
            })
            .collect()
    }

    fn check_ui_event(
        &mut self,
        module: &ModuleEnv<'_>,
        element: &ast::UiElement,
        event: &str,
        span: ast::SourceSpan,
    ) -> Option<TypeRef> {
        let name = element.name.value.as_str();
        let catalog = self.ui_catalog();
        let Some(spec) = catalog.element(name) else {
            // A presentation-shaped tag is rejected once at the element
            // boundary. Recover with Unit here so an event edge does not
            // misleadingly imply that presentation invocation exists.
            if matches!(module.lookup(name), Some(Export::Presentation { .. })) {
                return Some(TypeRef::Unit);
            }
            self.diagnostics.push(error(
                codes::UI,
                "uhura/ui-event",
                format!("`<{name}>` does not declare a checked `{event}` event"),
                span,
            ));
            return None;
        };
        match catalog.event(spec, event, ui_element_context(element)) {
            UiEventContract::Admitted(payload) => Some(match payload {
                UiEventPayload::Unit => TypeRef::Unit,
                UiEventPayload::TextField(field) => TypeRef::Record {
                    fields: vec![(field.into(), TypeRef::Text)],
                },
                UiEventPayload::BoundaryNumberField(field) => TypeRef::Record {
                    fields: vec![(field.into(), TypeRef::BoundaryNumber)],
                },
            }),
            UiEventContract::RequiresTextInput => {
                self.diagnostics.push(error(
                    codes::UI,
                    "uhura/ui-event",
                    "`on input` is admitted by text-shaped `<input>`; use checked `on change` for `<input type=\"number\">`",
                    span,
                ));
                None
            }
            UiEventContract::RequiresNumberInput => {
                self.diagnostics.push(error(
                    codes::UI,
                    "uhura/ui-event",
                    "`on change` is admitted only by `<input type=\"number\">`",
                    span,
                ));
                None
            }
            UiEventContract::Unknown => {
                self.diagnostics.push(error(
                    codes::UI,
                    "uhura/ui-event",
                    format!("`<{name}>` does not declare a checked `{event}` event"),
                    span,
                ));
                None
            }
        }
    }

    fn check_ui_element_shape(
        &mut self,
        module: &ModuleEnv<'_>,
        element: &ast::UiElement,
        span: ast::SourceSpan,
    ) {
        let name = element.name.value.as_str();
        let catalog = self.ui_catalog();
        let spec = catalog.element(name);
        let imported_ui_element = matches!(module.lookup(name), Some(Export::UiElement));
        let imported_presentation =
            matches!(module.lookup(name), Some(Export::Presentation { .. }));
        let admitted = spec.is_some_and(|spec| match spec.availability {
            UiElementAvailability::Native => true,
            UiElementAvailability::StandardImport => imported_ui_element,
        });
        if imported_presentation {
            self.diagnostics.push(error(
                codes::UI,
                "uhura/ui-presentation-invocation-unavailable",
                format!(
                    "`<{name}>` resolves to a UI presentation, but presentation invocation is not part of Uhura 0.4; inline its markup or use a checked element"
                ),
                element.name.span,
            ));
        } else if !admitted {
            self.diagnostics.push(error(
                codes::UI,
                "uhura/unknown-ui-element",
                format!("`<{name}>` is not a native or imported checked UI element"),
                element.name.span,
            ));
        }
        if admitted
            && spec.is_some_and(|spec| spec.content == UiContentModel::Void)
            && !element.children.is_empty()
        {
            self.diagnostics.push(error(
                codes::UI,
                "uhura/ui-void-children",
                format!("`<{name}>` is void and cannot have children"),
                span,
            ));
        }
        let mut seen = BTreeSet::new();
        for attribute in &element.attributes {
            let identity = match &attribute.value {
                ast::UiAttributeValue::Event { event, .. } => {
                    format!("on {}", event.value)
                }
                _ => attribute.name.clone(),
            };
            if !seen.insert(identity.clone()) {
                self.diagnostics.push(error(
                    codes::UI,
                    "uhura/duplicate-ui-attribute",
                    format!("UI attribute `{identity}` is repeated"),
                    attribute.span,
                ));
            }
            let valid = match &attribute.value {
                ast::UiAttributeValue::Event { .. } => admitted,
                _ => {
                    admitted
                        && spec.is_some_and(|spec| {
                            catalog
                                .attribute(spec, &attribute.name, ui_element_context(element))
                                .is_some()
                        })
                }
            };
            if !valid {
                self.diagnostics.push(error(
                    codes::UI,
                    "uhura/invalid-ui-attribute",
                    format!("`{}` is not valid on `<{name}>`", attribute.name),
                    attribute.span,
                ));
            }
            match &attribute.value {
                ast::UiAttributeValue::Text(value) => {
                    if self
                        .ui_attribute_kind(element, &attribute.name)
                        .is_some_and(UiAttributeKind::requires_expression)
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-attribute-type",
                            format!(
                                "attribute `{}` on `<{name}>` requires a checked expression",
                                attribute.name
                            ),
                            attribute.span,
                        ));
                    }
                    if let Some(UiAttributeKind::StaticToken(values)) =
                        self.ui_attribute_kind(element, &attribute.name)
                        && !values.contains(&value.as_str())
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-attribute-value",
                            format!(
                                "attribute `{}` on `<{name}>` must be one of {}",
                                attribute.name,
                                values.join(", ")
                            ),
                            attribute.span,
                        ));
                    }
                }
                ast::UiAttributeValue::Expression(_) => {
                    if matches!(
                        self.ui_attribute_kind(element, &attribute.name),
                        Some(UiAttributeKind::StaticToken(_))
                    ) {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-attribute-value",
                            format!(
                                "attribute `{}` on `<{name}>` requires a quoted checked token",
                                attribute.name
                            ),
                            attribute.span,
                        ));
                    }
                }
                ast::UiAttributeValue::Event { .. } => {}
            }
        }
        if admitted {
            let spec = spec.expect("admitted UI elements have a catalogue entry");
            for required in spec.required_attributes {
                if !seen.contains(*required) {
                    self.diagnostics.push(error(
                        codes::UI,
                        "uhura/missing-ui-attribute",
                        format!("`<{name}>` requires `{required}`"),
                        span,
                    ));
                }
            }
            for constraint in spec.constraints {
                match constraint {
                    UiConstraint::ExactlyOneAttribute(attributes) => {
                        let alternatives = attributes
                            .iter()
                            .filter(|attribute| seen.contains(**attribute))
                            .count();
                        if alternatives != 1 {
                            self.diagnostics.push(error(
                                codes::UI,
                                "uhura/ui-attribute-alternative",
                                format!(
                                    "`<{name}>` requires exactly one of {}",
                                    attributes
                                        .iter()
                                        .map(|attribute| format!("`{attribute}`"))
                                        .collect::<Vec<_>>()
                                        .join(" or ")
                                ),
                                span,
                            ));
                        }
                    }
                    UiConstraint::Controlled { attribute, event }
                        if seen.contains(*attribute) && !has_ui_event(element, event) =>
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-controlled-field",
                            format!("`<{name} {attribute}={{...}}>` must handle `{event}`"),
                            span,
                        ));
                    }
                    UiConstraint::Controlled { .. } => {}
                    UiConstraint::AccessibleName { attributes }
                        if !attributes.iter().any(|attribute| seen.contains(*attribute))
                            && !ui_nodes_have_accessible_text(&element.children) =>
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-accessible-name",
                            format!(
                                "`<{name}>` requires visible text{}",
                                if attributes.is_empty() {
                                    String::new()
                                } else {
                                    format!(
                                        " or one of {}",
                                        attributes
                                            .iter()
                                            .map(|attribute| format!("`{attribute}`"))
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    )
                                }
                            ),
                            span,
                        ));
                    }
                    UiConstraint::AccessibleName { .. } => {}
                    UiConstraint::NoInteractiveDescendants
                        if ui_nodes_contain_interactive_element(&element.children, catalog) =>
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-nested-interactive",
                            format!("`<{name}>` cannot contain another interactive element"),
                            span,
                        ));
                    }
                    UiConstraint::NoInteractiveDescendants => {}
                    UiConstraint::AtLeastOneEvent(events)
                        if !events.iter().any(|event| has_ui_event(element, event)) =>
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-missing-event",
                            format!(
                                "`<{name}>` requires one of {}",
                                events
                                    .iter()
                                    .map(|event| format!("`on {event}`"))
                                    .collect::<Vec<_>>()
                                    .join(" or ")
                            ),
                            span,
                        ));
                    }
                    UiConstraint::AtLeastOneEvent(_) => {}
                    UiConstraint::NeutralListItems {
                        element: item_element,
                    } if ui_element_has_text_attribute(element, "role", "list")
                        && !ui_nodes_are_neutral_list_items(&element.children, item_element) =>
                    {
                        self.diagnostics.push(error(
                            codes::UI,
                            "uhura/ui-list-item-boundary",
                            "`<view role=\"list\">` requires each rendered direct child to be an unroled `<view>`; nest buttons, regions, and other semantics inside that boundary",
                            span,
                        ));
                    }
                    UiConstraint::NeutralListItems { .. } => {}
                }
            }
        }
    }

    fn check_ui_attribute_type(
        &mut self,
        element: &ast::UiElement,
        attribute: &str,
        ty: &Ty,
        span: ast::SourceSpan,
    ) {
        let valid = match self.ui_attribute_kind(element, attribute) {
            Some(UiAttributeKind::Text | UiAttributeKind::StaticToken(_)) => {
                matches!(ty.as_value(), Some(TypeRef::Text))
            }
            Some(UiAttributeKind::Bool) => {
                matches!(ty.as_value(), Some(TypeRef::Bool))
            }
            Some(UiAttributeKind::ExactNumeric) => {
                ty.as_value().is_some_and(|ty| self.exact_numeric_type(ty))
            }
            Some(UiAttributeKind::Ratio) => {
                matches!(ty.as_value(), Some(TypeRef::Ratio))
            }
            Some(UiAttributeKind::Key) => ty.as_value().is_some_and(|ty| self.ui_key_type(ty)),
            Some(UiAttributeKind::CheckedExpression) => true,
            None => true,
        };
        if !valid {
            self.diagnostics.push(error(
                codes::UI,
                "uhura/ui-attribute-type",
                format!(
                    "attribute `{attribute}` has invalid type `{}`",
                    ty.display()
                ),
                span,
            ));
        }
    }

    fn ui_catalog(&self) -> ui_catalog::Catalog {
        ui_catalog::current()
    }

    fn ui_attribute_kind(
        &self,
        element: &ast::UiElement,
        attribute: &str,
    ) -> Option<UiAttributeKind> {
        let catalog = self.ui_catalog();
        let spec = catalog.element(&element.name.value)?;
        catalog.attribute(spec, attribute, ui_element_context(element))
    }

    fn ui_attribute_expected_type(
        &self,
        element: &ast::UiElement,
        attribute: &str,
    ) -> Option<TypeRef> {
        match self.ui_attribute_kind(element, attribute) {
            Some(UiAttributeKind::Text | UiAttributeKind::StaticToken(_)) => Some(TypeRef::Text),
            Some(UiAttributeKind::Bool) => Some(TypeRef::Bool),
            Some(UiAttributeKind::Ratio) => Some(TypeRef::Ratio),
            Some(
                UiAttributeKind::ExactNumeric
                | UiAttributeKind::CheckedExpression
                | UiAttributeKind::Key,
            )
            | None => None,
        }
    }

    fn exact_numeric_type(&self, ty: &TypeRef) -> bool {
        matches!(
            ty,
            TypeRef::Int | TypeRef::Nat | TypeRef::PositiveInt | TypeRef::Decimal | TypeRef::Ratio
        )
    }

    fn ui_scalar_type(&self, ty: &TypeRef) -> bool {
        matches!(ty, TypeRef::Text | TypeRef::Bool)
            || self.exact_numeric_type(ty)
            || matches!(
                self.registry.shape(ty),
                Some(TypeShape::Key(
                    TypeRef::Text
                        | TypeRef::Bool
                        | TypeRef::Int
                        | TypeRef::Nat
                        | TypeRef::PositiveInt
                        | TypeRef::Decimal
                        | TypeRef::Ratio
                ))
            )
    }

    fn ui_key_type(&self, ty: &TypeRef) -> bool {
        self.ui_scalar_type(ty)
            || matches!(
                self.registry.shape(ty),
                Some(TypeShape::Sum(constructors))
                    if constructors.iter().all(|constructor| constructor.fields.is_empty())
            )
    }
}

fn has_ui_event(element: &ast::UiElement, event_name: &str) -> bool {
    element.attributes.iter().any(|attribute| {
        matches!(
            &attribute.value,
            ast::UiAttributeValue::Event { event, .. } if event.value == event_name
        )
    })
}

fn ui_element_has_text_attribute(
    element: &ast::UiElement,
    attribute_name: &str,
    expected: &str,
) -> bool {
    element.attributes.iter().any(|attribute| {
        attribute.name == attribute_name
            && matches!(
                &attribute.value,
                ast::UiAttributeValue::Text(value) if value == expected
            )
    })
}

fn ui_nodes_are_neutral_list_items(nodes: &[ast::UiNode], item_element: &str) -> bool {
    nodes.iter().all(|node| match &node.value {
        ast::UiNodeKind::Text(value) => value.trim().is_empty(),
        ast::UiNodeKind::Element(element) => {
            element.name.value == item_element
                && !element
                    .attributes
                    .iter()
                    .any(|attribute| attribute.name == "role")
        }
        ast::UiNodeKind::If { children, .. } | ast::UiNodeKind::Each { children, .. } => {
            ui_nodes_are_neutral_list_items(children, item_element)
        }
        ast::UiNodeKind::Match { cases, .. } => cases
            .iter()
            .all(|case| ui_nodes_are_neutral_list_items(&case.children, item_element)),
        ast::UiNodeKind::Interpolation(_) => false,
    })
}

fn ui_nodes_have_accessible_text(nodes: &[ast::UiNode]) -> bool {
    nodes.iter().any(|node| match &node.value {
        ast::UiNodeKind::Text(value) => !value.trim().is_empty(),
        ast::UiNodeKind::Interpolation(_) => true,
        ast::UiNodeKind::Element(element) => ui_nodes_have_accessible_text(&element.children),
        ast::UiNodeKind::If { children, .. } | ast::UiNodeKind::Each { children, .. } => {
            ui_nodes_have_accessible_text(children)
        }
        ast::UiNodeKind::Match { cases, .. } => cases
            .iter()
            .any(|case| ui_nodes_have_accessible_text(&case.children)),
    })
}

fn ui_nodes_contain_interactive_element(
    nodes: &[ast::UiNode],
    catalog: ui_catalog::Catalog,
) -> bool {
    nodes.iter().any(|node| match &node.value {
        ast::UiNodeKind::Element(element) => {
            catalog.is_interactive(element.name.value.as_str())
                || ui_nodes_contain_interactive_element(&element.children, catalog)
        }
        ast::UiNodeKind::If { children, .. } | ast::UiNodeKind::Each { children, .. } => {
            ui_nodes_contain_interactive_element(children, catalog)
        }
        ast::UiNodeKind::Match { cases, .. } => cases
            .iter()
            .any(|case| ui_nodes_contain_interactive_element(&case.children, catalog)),
        ast::UiNodeKind::Text(_) | ast::UiNodeKind::Interpolation(_) => false,
    })
}

fn ui_element_context(element: &ast::UiElement) -> UiElementContext {
    UiElementContext {
        static_number_input: input_is_static_number(element),
    }
}

fn input_is_static_number(element: &ast::UiElement) -> bool {
    element.attributes.iter().any(|attribute| {
        attribute.name == "type"
            && matches!(
                &attribute.value,
                ast::UiAttributeValue::Text(value) if value == "number"
            )
    })
}

impl Checker<'_> {
    fn lower_evidence(&mut self) {
        let deferred = self.evidence.clone();
        // Alias targets are installed first so a replay scenario can originate
        // from a checkpoint declared later in source order.
        for value in &deferred {
            let Some(module) = self.modules.get(&value.module).cloned() else {
                continue;
            };
            match &value.declaration.value {
                ast::DeclarationKind::Example(alias) => {
                    if !module.features.contains("evidence") {
                        self.evidence_feature_error(&module, value.declaration.span);
                    }
                    if let Some(reference) = self.lower_evidence_ref(&module, &alias.target) {
                        let id = qualify(&module.id, &alias.name.value);
                        let presentation = alias
                            .presentation
                            .as_ref()
                            .and_then(|name| self.resolve_presentation(&module, name));
                        let kind = alias.kind.map(|kind| match kind {
                            ast::EvidencePresentationKind::Page => EvidencePresentationKind::Page,
                            ast::EvidencePresentationKind::Component => {
                                EvidencePresentationKind::Component
                            }
                            ast::EvidencePresentationKind::Surface => {
                                EvidencePresentationKind::Surface
                            }
                        });
                        if alias.is_default
                            && presentation.as_ref().is_some_and(|presentation| {
                                self.program
                                    .evidence
                                    .example_metadata
                                    .values()
                                    .any(|metadata| {
                                        metadata.is_default
                                            && metadata.presentation.as_ref() == Some(presentation)
                                    })
                            })
                        {
                            self.diagnostics.push(error(
                                codes::EVIDENCE,
                                "uhura/duplicate-default-example",
                                format!(
                                    "presentation `{}` has more than one default example",
                                    presentation.as_deref().unwrap_or("<unresolved>")
                                ),
                                value.declaration.span,
                            ));
                        }
                        self.program.evidence.examples.insert(id.clone(), reference);
                        self.program.evidence.example_metadata.insert(
                            id.clone(),
                            EvidenceExampleMetadata {
                                presentation,
                                kind,
                                is_default: alias.is_default,
                                note: alias.note.clone(),
                            },
                        );
                        self.program
                            .evidence
                            .example_sources
                            .insert(id, source(&module, value.declaration.span));
                    }
                }
                ast::DeclarationKind::Checkpoint(alias) => {
                    if !module.features.contains("evidence") {
                        self.evidence_feature_error(&module, value.declaration.span);
                    }
                    if let Some(reference) = self.lower_evidence_ref(&module, &alias.target) {
                        let id = qualify(&module.id, &alias.name.value);
                        self.program
                            .evidence
                            .checkpoints
                            .insert(id.clone(), reference);
                        self.program
                            .evidence
                            .checkpoint_sources
                            .insert(id, source(&module, value.declaration.span));
                    }
                }
                _ => {}
            }
        }

        for value in deferred {
            let ast::DeclarationKind::Scenario(scenario) = &value.declaration.value else {
                continue;
            };
            let Some(module) = self.modules.get(&value.module).cloned() else {
                continue;
            };
            if !module.features.contains("evidence") {
                self.evidence_feature_error(&module, value.declaration.span);
            }
            let (machine_id, snapshot_reference) = match &scenario.origin {
                ast::ScenarioOrigin::Machine { machine, .. } => {
                    let Some(machine) = self.resolve_machine(&module, machine) else {
                        continue;
                    };
                    (machine, None)
                }
                ast::ScenarioOrigin::Snapshot(reference) => {
                    let Some(reference) = self.lower_evidence_ref(&module, reference) else {
                        continue;
                    };
                    let reference = self.expand_checkpoint_reference(&module, reference);
                    let machine = self
                        .scenario_machine(&reference.scenario)
                        .unwrap_or_else(|| "<unresolved-machine>".into());
                    (machine, Some(reference))
                }
            };
            let Some(machine) = self
                .program
                .machine_program
                .machines
                .get(&machine_id)
                .cloned()
            else {
                self.diagnostics.push(error(
                    codes::EVIDENCE,
                    "uhura/evidence-machine",
                    format!("scenario machine `{machine_id}` is unavailable"),
                    value.declaration.span,
                ));
                continue;
            };
            let mut scope = self.module_scope(&module);
            self.populate_scope_constructors(&mut scope);
            let origin = match (&scenario.origin, snapshot_reference) {
                (
                    ast::ScenarioOrigin::Machine {
                        machine: machine_name,
                        configuration,
                    },
                    None,
                ) => {
                    let configuration = match configuration {
                        None if machine.config == TypeRef::Unit => Value::Unit,
                        None => {
                            self.diagnostics.push(error(
                                codes::EVIDENCE,
                                "uhura/missing-scenario-configuration",
                                format!(
                                    "scenario for `{}` requires a compile-time configuration of type `{}`; write `for {}(...)`",
                                    machine.id,
                                    machine.config.canonical_name(),
                                    machine_name.value,
                                ),
                                machine_name.span,
                            ));
                            Value::Unit
                        }
                        Some(configuration) => {
                            let diagnostics_before = self.diagnostics.len();
                            let (expression, actual) = self.lower_expr(
                                &module,
                                &scope,
                                configuration,
                                Some(&machine.config),
                                ExprMode::Pure,
                            );
                            self.expect_type(&actual, &machine.config, configuration.span);
                            if self.diagnostics.len() != diagnostics_before {
                                Value::Unit
                            } else {
                                match const_eval(&expression, &self.program) {
                                    Ok(value) => match self
                                        .program
                                        .machine_program
                                        .canonicalize_value(&machine.config, &value)
                                    {
                                        Ok(value) => value,
                                        Err(value_error) => {
                                            self.diagnostics.push(error(
                                                codes::EVIDENCE,
                                                "uhura/invalid-scenario-configuration",
                                                format!(
                                                    "scenario configuration for `{}` is invalid: {value_error}",
                                                    machine.id
                                                ),
                                                configuration.span,
                                            ));
                                            Value::Unit
                                        }
                                    },
                                    Err(message) => {
                                        self.diagnostics.push(error(
                                            codes::EFFECT,
                                            "uhura/non-constant-scenario-configuration",
                                            format!(
                                                "scenario configuration for `{}` is not compile-time total: {message}",
                                                machine.id
                                            ),
                                            configuration.span,
                                        ));
                                        Value::Unit
                                    }
                                }
                            }
                        }
                    };
                    IrScenarioOrigin::Machine {
                        machine: machine_id.clone(),
                        configuration,
                    }
                }
                (ast::ScenarioOrigin::Snapshot(_), Some(reference)) => {
                    IrScenarioOrigin::Snapshot { reference }
                }
                _ => unreachable!("scenario origin lowering preserves its source form"),
            };
            self.install_machine_io_scope(&machine, &mut scope);
            let observation = TypeRef::Record {
                fields: machine
                    .observation
                    .iter()
                    .map(|field| (field.name.clone(), field.ty.clone()))
                    .collect(),
            };
            let inspection = TypeRef::Record {
                fields: machine
                    .state
                    .iter()
                    .map(|field| (field.name.clone(), field.ty.clone()))
                    .chain(
                        machine
                            .derives
                            .iter()
                            .map(|(name, ty, _, _)| (name.clone(), ty.clone())),
                    )
                    .collect(),
            };
            let mut pins = BTreeSet::new();
            let steps = scenario
                .steps
                .iter()
                .map(|step| match &step.value {
                    ast::EvidenceStepKind::Bind { port, fixture } => {
                        if !machine.ports.iter().any(|value| value.name == port.value) {
                            self.diagnostics.push(error(
                                codes::EVIDENCE,
                                "uhura/evidence-port",
                                format!("machine `{}` has no port `{}`", machine.id, port.value),
                                port.span,
                            ));
                        }
                        let (fixture, _) =
                            self.lower_expr(&module, &scope, fixture, None, ExprMode::Evidence);
                        IrEvidenceStep::Bind {
                            port: port.value.clone(),
                            fixture,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::Start => IrEvidenceStep::Start {
                        source: source(&module, step.span),
                    },
                    ast::EvidenceStepKind::Send(input) => {
                        let (input, actual) = self.lower_expr(
                            &module,
                            &scope,
                            input,
                            scope.input_type.as_ref(),
                            ExprMode::Evidence,
                        );
                        if let Some(expected) = &scope.input_type {
                            self.expect_type(&actual, expected, step.span);
                        }
                        IrEvidenceStep::Send {
                            input,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::Deliver(input) => {
                        let (input, _) =
                            self.lower_expr(&module, &scope, input, None, ExprMode::Evidence);
                        IrEvidenceStep::Deliver {
                            input,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::ExpectReaction { outcome, commands } => {
                        let mut pattern_scope = scope.child();
                        let outcome = self.lower_pattern(
                            &module,
                            &mut pattern_scope,
                            outcome,
                            scope.outcome_type.as_ref(),
                            PatternUse::Evidence,
                        );
                        let commands = commands
                            .iter()
                            .map(|command| {
                                self.lower_expr(&module, &scope, command, None, ExprMode::Evidence)
                                    .0
                            })
                            .collect();
                        IrEvidenceStep::ExpectReaction {
                            outcome,
                            commands,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::ExpectObservationPattern(pattern) => {
                        let mut pattern_scope = scope.child();
                        let pattern = self.lower_pattern(
                            &module,
                            &mut pattern_scope,
                            pattern,
                            Some(&observation),
                            PatternUse::Evidence,
                        );
                        IrEvidenceStep::ExpectObservationPattern {
                            pattern,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::ExpectInspectionPattern(pattern) => {
                        let mut pattern_scope = scope.child();
                        let pattern = self.lower_pattern(
                            &module,
                            &mut pattern_scope,
                            pattern,
                            Some(&inspection),
                            PatternUse::Evidence,
                        );
                        IrEvidenceStep::ExpectInspectionPattern {
                            pattern,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::ExpectObservationWhere(condition) => {
                        let mut observation_scope = scope.child();
                        for field in &machine.observation {
                            observation_scope.bind(
                                &field.name,
                                &field.name,
                                Ty::value(field.ty.clone()),
                            );
                        }
                        let (condition, _) = self.lower_condition(
                            &module,
                            &observation_scope,
                            condition,
                            ExprMode::Evidence,
                        );
                        IrEvidenceStep::ExpectObservationWhere {
                            condition,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::ExpectRestore { commands } => {
                        let commands = commands
                            .iter()
                            .map(|command| {
                                self.lower_expr(&module, &scope, command, None, ExprMode::Evidence)
                                    .0
                            })
                            .collect();
                        IrEvidenceStep::ExpectRestore {
                            commands,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::ExpectSnapshot { target } => {
                        let reference =
                            self.lower_evidence_ref(&module, target)
                                .unwrap_or(IrEvidenceRef {
                                    scenario: "<invalid>".into(),
                                    pin: "<invalid>".into(),
                                });
                        IrEvidenceStep::ExpectSnapshot {
                            reference,
                            source: source(&module, step.span),
                        }
                    }
                    ast::EvidenceStepKind::Pin(name) => {
                        if !pins.insert(name.value.clone()) {
                            self.diagnostics.push(error(
                                codes::DUPLICATE,
                                "uhura/duplicate-pin",
                                format!("scenario pin `{}` is repeated", name.value),
                                name.span,
                            ));
                        }
                        IrEvidenceStep::Pin {
                            name: name.value.clone(),
                            source: source(&module, step.span),
                        }
                    }
                })
                .collect();
            let id = qualify(&module.id, &scenario.name.value);
            self.program.evidence.scenarios.insert(
                id.clone(),
                IrScenario {
                    id,
                    origin,
                    steps,
                    source: source(&module, value.declaration.span),
                },
            );
        }

        // An editor example is evidence for one machine snapshot and, when it
        // names a presentation, that presentation must consume the same
        // machine. Keeping this invariant in the checked program prevents the
        // host from guessing or producing a presentation × example product.
        for value in &self.evidence.clone() {
            let ast::DeclarationKind::Example(alias) = &value.declaration.value else {
                continue;
            };
            let Some(module) = self.modules.get(&value.module).cloned() else {
                continue;
            };
            let example_id = qualify(&module.id, &alias.name.value);
            let Some(metadata) = self
                .program
                .evidence
                .example_metadata
                .get(&example_id)
                .cloned()
            else {
                continue;
            };
            let Some(presentation_id) = metadata.presentation else {
                continue;
            };
            let Some(reference) = self.program.evidence.examples.get(&example_id) else {
                continue;
            };
            let Some(machine_id) = self.scenario_machine(&reference.scenario) else {
                continue;
            };
            let Some(presentation) = self.program.presentations.get(&presentation_id) else {
                continue;
            };
            if presentation.machine != machine_id {
                self.diagnostics.push(error(
                    codes::EVIDENCE,
                    "uhura/example-presentation-machine",
                    format!(
                        "example `{example_id}` snapshots machine `{machine_id}`, but presentation `{presentation_id}` targets `{}`",
                        presentation.machine
                    ),
                    value.declaration.span,
                ));
            }
        }
    }

    fn evidence_feature_error(&mut self, module: &ModuleEnv<'_>, span: ast::SourceSpan) {
        self.diagnostics.push(error(
            codes::EVIDENCE_NOT_ENABLED,
            "uhura/evidence-without-use",
            format!(
                "evidence declarations in `{}` require `use evidence`",
                module.id
            ),
            span,
        ));
    }

    fn lower_evidence_ref(
        &mut self,
        module: &ModuleEnv<'_>,
        reference: &ast::EvidenceRef,
    ) -> Option<IrEvidenceRef> {
        let parts = reference
            .path
            .iter()
            .map(|part| part.value.clone())
            .collect::<Vec<_>>();
        match parts.as_slice() {
            [scenario, pin] => Some(IrEvidenceRef {
                scenario: qualify(&module.id, scenario),
                pin: pin.clone(),
            }),
            [checkpoint] => {
                let id = qualify(&module.id, checkpoint);
                self.program
                    .evidence
                    .checkpoints
                    .get(&id)
                    .cloned()
                    .or_else(|| {
                        Some(IrEvidenceRef {
                            scenario: id,
                            pin: checkpoint.clone(),
                        })
                    })
            }
            _ => {
                self.diagnostics.push(error(
                    codes::EVIDENCE,
                    "uhura/evidence-reference",
                    "evidence references must be `scenario::pin` or a checkpoint name",
                    reference.span,
                ));
                None
            }
        }
    }

    fn expand_checkpoint_reference(
        &self,
        module: &ModuleEnv<'_>,
        reference: IrEvidenceRef,
    ) -> IrEvidenceRef {
        self.program
            .evidence
            .checkpoints
            .get(&reference.scenario)
            .cloned()
            .or_else(|| {
                self.program
                    .evidence
                    .checkpoints
                    .get(&qualify(&module.id, &reference.pin))
                    .cloned()
            })
            .unwrap_or(reference)
    }

    fn scenario_machine(&self, scenario: &str) -> Option<String> {
        let value = self.program.evidence.scenarios.get(scenario)?;
        match &value.origin {
            IrScenarioOrigin::Machine { machine, .. } => Some(machine.clone()),
            IrScenarioOrigin::Snapshot { reference } => self.scenario_machine(&reference.scenario),
        }
    }
}

impl Checker<'_> {
    fn lower_reaction_block(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        block: &ast::Block,
        outcome: &TypeRef,
        continuation: Vec<Statement>,
    ) -> Vec<Statement> {
        self.lower_reaction_sequence(module, scope, &block.statements, 0, outcome, continuation)
    }

    fn lower_reaction_sequence(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        statements: &[ast::Statement],
        index: usize,
        outcome: &TypeRef,
        continuation: Vec<Statement>,
    ) -> Vec<Statement> {
        let Some(statement) = statements.get(index) else {
            return continuation;
        };
        match &statement.value {
            ast::StatementKind::Let { name, ty, value } => {
                let expected = ty.as_ref().map(|ty| self.resolve_type(module, scope, ty));
                self.lower_bind_control_tail(
                    module,
                    scope,
                    &name.value,
                    expected.as_ref(),
                    value,
                    outcome,
                    statements,
                    index + 1,
                    continuation,
                    statement.span,
                )
            }
            ast::StatementKind::Set { target, value } => {
                let expected = scope.state_fields.get(&target.value).cloned();
                if expected.is_none() {
                    self.diagnostics.push(error(
                        codes::EFFECT,
                        "uhura/set-non-state",
                        format!("`set` target `{}` is not a state field", target.value),
                        target.span,
                    ));
                }
                if reaction_control(value) {
                    self.diagnostics.push(error(
                        codes::EFFECT,
                        "uhura/control-in-set",
                        "terminal control cannot appear inside a state assignment value",
                        value.span,
                    ));
                }
                let (value, actual) =
                    self.lower_expr(module, scope, value, expected.as_ref(), ExprMode::Reaction);
                if let Some(expected) = &expected {
                    self.expect_type(&actual, expected, statement.span);
                }
                let mut rest_scope = scope.child();
                rest_scope.invalidate_path(&target.value);
                let type_minimum = match expected.as_ref() {
                    Some(TypeRef::Nat) => Some(0),
                    Some(TypeRef::PositiveInt) => Some(1),
                    _ => None,
                };
                let assigned_minimum = integer_lower_bound(&value, scope).or(type_minimum);
                if assigned_minimum.is_some() {
                    rest_scope.numeric_bounds.insert(
                        target.value.clone(),
                        NumericBounds {
                            min: assigned_minimum,
                            max: None,
                        },
                    );
                }
                let rest = self.lower_reaction_sequence(
                    module,
                    &rest_scope,
                    statements,
                    index + 1,
                    outcome,
                    continuation,
                );
                let mut output = vec![Statement::Set {
                    field: target.value.clone(),
                    value,
                    source: source(module, statement.span),
                }];
                output.extend(rest);
                output
            }
            ast::StatementKind::Emit(value) => {
                let expected = scope.command_type.as_ref();
                let expression_expected = if is_qualified_call(value) {
                    None
                } else {
                    expected
                };
                let (value, actual) = self.lower_expr(
                    module,
                    scope,
                    value,
                    expression_expected,
                    ExprMode::Reaction,
                );
                if let Some(expected) = expected {
                    // Qualified port sends have their own nominal identity and
                    // are admitted alongside the local command sum.
                    let is_port = matches!(&value, IrExpr::Constructor { constructor, .. } if constructor.contains('.'));
                    if !is_port {
                        self.expect_type(&actual, expected, statement.span);
                    }
                }
                let rest = self.lower_reaction_sequence(
                    module,
                    scope,
                    statements,
                    index + 1,
                    outcome,
                    continuation,
                );
                let mut output = vec![Statement::Emit {
                    value,
                    source: source(module, statement.span),
                }];
                output.extend(rest);
                output
            }
            ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                let break_local = inline_update_loop_exit_local(body);
                if !loop_decrease_proven(condition, decreases, body) {
                    self.diagnostics.push(error(
                        codes::TERMINATION,
                        "uhura/unproved-loop-decrease",
                        "`while` measure is not proved to decrease strictly on every back edge",
                        statement.span,
                    ));
                }
                let (condition, loop_scope) =
                    self.lower_condition(module, scope, condition, ExprMode::Reaction);
                let (_, ty) = self.lower_expr(
                    module,
                    &loop_scope,
                    decreases,
                    Some(&TypeRef::Nat),
                    ExprMode::Reaction,
                );
                if !matches!(
                    ty.as_value(),
                    Some(TypeRef::Int | TypeRef::Nat | TypeRef::PositiveInt)
                ) {
                    self.diagnostics.push(error(
                        codes::TERMINATION,
                        "uhura/loop-measure",
                        "`decreases` must be an exact non-negative integer expression",
                        decreases.span,
                    ));
                }
                let body =
                    self.lower_reaction_block(module, &loop_scope, body, outcome, Vec::new());
                let rest = self.lower_reaction_sequence(
                    module,
                    scope,
                    statements,
                    index + 1,
                    outcome,
                    continuation,
                );
                let mut output = vec![Statement::While {
                    condition,
                    body,
                    break_local,
                    source: source(module, statement.span),
                }];
                output.extend(rest);
                output
            }
            ast::StatementKind::Expr(expression)
                if !guaranteed_update_joins(expression).is_empty() =>
            {
                let joins = guaranteed_update_joins(expression);
                let mut joined_scope = scope.child();
                for (name, ty) in joins {
                    let ty = self.resolve_type(module, scope, &ty);
                    joined_scope.bind(&name, &name, Ty::value(ty));
                }
                let mut output =
                    self.lower_reaction_expression(module, scope, expression, outcome, Vec::new());
                output.extend(self.lower_reaction_sequence(
                    module,
                    &joined_scope,
                    statements,
                    index + 1,
                    outcome,
                    continuation,
                ));
                output
            }
            ast::StatementKind::Expr(
                expression @ ast::Spanned {
                    value:
                        ast::ExprKind::If {
                            condition,
                            then_branch,
                            else_branch,
                        },
                    ..
                },
            ) if reaction_control(expression) => {
                let (condition_ir, then_scope) =
                    self.lower_condition(module, scope, condition, ExprMode::Reaction);
                let else_scope = refined_numeric_scope(scope, condition, false, &self.registry);
                let then_continuation = if source_expr_terminal(then_branch) {
                    Vec::new()
                } else {
                    self.lower_reaction_sequence(
                        module,
                        &then_scope,
                        statements,
                        index + 1,
                        outcome,
                        continuation.clone(),
                    )
                };
                let else_continuation = self.lower_reaction_sequence(
                    module,
                    &else_scope,
                    statements,
                    index + 1,
                    outcome,
                    continuation,
                );
                let then_body = self.lower_reaction_expression(
                    module,
                    &then_scope,
                    then_branch,
                    outcome,
                    then_continuation,
                );
                let else_body = else_branch
                    .as_ref()
                    .map_or(else_continuation.clone(), |branch| {
                        self.lower_reaction_expression(
                            module,
                            &else_scope,
                            branch,
                            outcome,
                            else_continuation,
                        )
                    });
                vec![Statement::If {
                    condition: condition_ir,
                    then_body,
                    else_body,
                    source: source(module, expression.span),
                }]
            }
            ast::StatementKind::Expr(expression) => {
                let rest = self.lower_reaction_sequence(
                    module,
                    scope,
                    statements,
                    index + 1,
                    outcome,
                    continuation,
                );
                self.lower_reaction_expression(module, scope, expression, outcome, rest)
            }
        }
    }

    fn lower_bind_control_tail(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        name: &str,
        annotation: Option<&TypeRef>,
        expression: &ast::Expr,
        outcome: &TypeRef,
        statements: &[ast::Statement],
        next_index: usize,
        continuation: Vec<Statement>,
        statement_span: ast::SourceSpan,
    ) -> Vec<Statement> {
        match &expression.value {
            ast::ExprKind::Match { subject, arms } if reaction_control(expression) => {
                let (value, subject_ty) =
                    self.lower_expr(module, scope, subject, None, ExprMode::Reaction);
                let inferred_annotation = annotation.cloned().or_else(|| {
                    self.probe_match_result(
                        module,
                        scope,
                        arms,
                        subject_ty.as_value(),
                        ExprMode::Reaction,
                    )
                });
                let annotation = inferred_annotation.as_ref();
                let mut covered = BTreeSet::new();
                let mut wildcard = false;
                let arms = arms
                    .iter()
                    .map(|arm| {
                        let mut child = scope.child();
                        let pattern = self.lower_pattern(
                            module,
                            &mut child,
                            &arm.pattern,
                            subject_ty.as_value(),
                            PatternUse::Match,
                        );
                        self.record_pattern_coverage(
                            &pattern,
                            &mut covered,
                            &mut wildcard,
                            arm.pattern.span,
                        );
                        let body = self.lower_bind_branch_tail(
                            module,
                            &child,
                            scope,
                            name,
                            annotation,
                            &arm.body,
                            outcome,
                            statements,
                            next_index,
                            continuation.clone(),
                            statement_span,
                        );
                        StatementMatchArm { pattern, body }
                    })
                    .collect();
                self.check_exhaustive(subject_ty.as_value(), &covered, wildcard, expression.span);
                vec![Statement::Match {
                    value,
                    arms,
                    source: source(module, expression.span),
                }]
            }
            ast::ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } if reaction_control(expression) => {
                let (condition, refined) =
                    self.lower_condition(module, scope, condition, ExprMode::Reaction);
                let then_body = self.lower_bind_branch_tail(
                    module,
                    &refined,
                    scope,
                    name,
                    annotation,
                    then_branch,
                    outcome,
                    statements,
                    next_index,
                    continuation.clone(),
                    statement_span,
                );
                let else_body = else_branch.as_ref().map_or_else(
                    || continuation.clone(),
                    |branch| {
                        self.lower_bind_branch_tail(
                            module,
                            scope,
                            scope,
                            name,
                            annotation,
                            branch,
                            outcome,
                            statements,
                            next_index,
                            continuation.clone(),
                            statement_span,
                        )
                    },
                );
                vec![Statement::If {
                    condition,
                    then_body,
                    else_body,
                    source: source(module, expression.span),
                }]
            }
            _ => {
                let (value, actual) =
                    self.lower_expr(module, scope, expression, annotation, ExprMode::Reaction);
                let ty = annotation
                    .cloned()
                    .or_else(|| actual.clone().into_value())
                    .unwrap_or(TypeRef::Never);
                let mut child = scope.child();
                child.bind(name, name, Ty::value(ty));
                let rest = self.lower_reaction_sequence(
                    module,
                    &child,
                    statements,
                    next_index,
                    outcome,
                    continuation,
                );
                let mut output = vec![Statement::Let {
                    name: name.into(),
                    value,
                    source: source(module, statement_span),
                }];
                output.extend(rest);
                output
            }
        }
    }

    fn lower_bind_branch_tail(
        &mut self,
        module: &ModuleEnv<'_>,
        value_scope: &Scope,
        tail_scope: &Scope,
        name: &str,
        annotation: Option<&TypeRef>,
        expression: &ast::Expr,
        outcome: &TypeRef,
        statements: &[ast::Statement],
        next_index: usize,
        continuation: Vec<Statement>,
        statement_span: ast::SourceSpan,
    ) -> Vec<Statement> {
        if reaction_control(expression) {
            let tail = if source_expr_terminal(expression) {
                Vec::new()
            } else {
                self.lower_reaction_sequence(
                    module,
                    tail_scope,
                    statements,
                    next_index,
                    outcome,
                    continuation,
                )
            };
            return self.lower_reaction_expression(module, value_scope, expression, outcome, tail);
        }
        let (value, actual) = self.lower_expr(
            module,
            value_scope,
            expression,
            annotation,
            ExprMode::Reaction,
        );
        if let Some(annotation) = annotation {
            self.expect_type(&actual, annotation, expression.span);
        }
        let ty = annotation
            .cloned()
            .or_else(|| actual.into_value())
            .unwrap_or(TypeRef::Never);
        let mut child = tail_scope.child();
        child.bind(name, name, Ty::value(ty));
        let rest = self.lower_reaction_sequence(
            module,
            &child,
            statements,
            next_index,
            outcome,
            continuation,
        );
        let mut output = vec![Statement::Let {
            name: name.into(),
            value,
            source: source(module, statement_span),
        }];
        output.extend(rest);
        output
    }

    fn lower_reaction_expression(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        expression: &ast::Expr,
        outcome: &TypeRef,
        continuation: Vec<Statement>,
    ) -> Vec<Statement> {
        match &expression.value {
            ast::ExprKind::Finish(value) => {
                self.lower_finish_value(module, scope, value, outcome, expression.span)
            }
            ast::ExprKind::Unreachable => vec![Statement::Unreachable {
                source: source(module, expression.span),
            }],
            ast::ExprKind::Block(block) => {
                self.lower_reaction_block(module, scope, block, outcome, continuation)
            }
            ast::ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let (condition, refined) =
                    self.lower_condition(module, scope, condition, ExprMode::Reaction);
                let then_body = self.lower_reaction_expression(
                    module,
                    &refined,
                    then_branch,
                    outcome,
                    continuation.clone(),
                );
                let else_body = else_branch.as_ref().map_or_else(
                    || continuation.clone(),
                    |branch| {
                        self.lower_reaction_expression(
                            module,
                            scope,
                            branch,
                            outcome,
                            continuation.clone(),
                        )
                    },
                );
                vec![Statement::If {
                    condition,
                    then_body,
                    else_body,
                    source: source(module, expression.span),
                }]
            }
            ast::ExprKind::Match { subject, arms } => {
                let (value, subject_ty) =
                    self.lower_expr(module, scope, subject, None, ExprMode::Reaction);
                let mut covered = BTreeSet::new();
                let mut wildcard = false;
                let arms = arms
                    .iter()
                    .map(|arm| {
                        let mut child = scope.child();
                        let pattern = self.lower_pattern(
                            module,
                            &mut child,
                            &arm.pattern,
                            subject_ty.as_value(),
                            PatternUse::Match,
                        );
                        self.record_pattern_coverage(
                            &pattern,
                            &mut covered,
                            &mut wildcard,
                            arm.pattern.span,
                        );
                        let body = self.lower_reaction_expression(
                            module,
                            &child,
                            &arm.body,
                            outcome,
                            continuation.clone(),
                        );
                        StatementMatchArm { pattern, body }
                    })
                    .collect();
                self.check_exhaustive(subject_ty.as_value(), &covered, wildcard, expression.span);
                vec![Statement::Match {
                    value,
                    arms,
                    source: source(module, expression.span),
                }]
            }
            _ => {
                self.diagnostics.push(error(
                    codes::EFFECT,
                    "uhura/discarded-pure-expression",
                    "a pure value cannot be used as a reaction statement; bind it or finish with an outcome",
                    expression.span,
                ));
                continuation
            }
        }
    }

    fn lower_finish_value(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        value: &ast::Expr,
        outcome: &TypeRef,
        finish_span: ast::SourceSpan,
    ) -> Vec<Statement> {
        if let ast::ExprKind::Call { callee, arguments } = &value.value
            && let ast::ExprKind::Name(name) = &callee.value
            && let Some((index, controlled)) = arguments
                .iter()
                .enumerate()
                .find(|(_, argument)| reaction_control(argument))
            && let Ok(constructor) = self.resolve_constructor(scope, &name.value, Some(outcome))
        {
            return self.lower_finish_constructor_control(
                module,
                scope,
                constructor,
                arguments,
                index,
                controlled,
                outcome,
                finish_span,
            );
        }
        let value_span = value.span;
        let (value, actual) =
            self.lower_expr(module, scope, value, Some(outcome), ExprMode::Reaction);
        self.expect_type(&actual, outcome, value_span);
        vec![Statement::Finish {
            outcome: value,
            source: source(module, finish_span),
        }]
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_finish_constructor_control(
        &mut self,
        module: &ModuleEnv<'_>,
        scope: &Scope,
        constructor: ConstructorInfo,
        arguments: &[ast::Expr],
        controlled_index: usize,
        controlled: &ast::Expr,
        outcome: &TypeRef,
        finish_span: ast::SourceSpan,
    ) -> Vec<Statement> {
        match &controlled.value {
            ast::ExprKind::Match { subject, arms } => {
                let (subject, subject_ty) =
                    self.lower_expr(module, scope, subject, None, ExprMode::Reaction);
                let mut covered = BTreeSet::new();
                let mut wildcard = false;
                let arms = arms
                    .iter()
                    .map(|arm| {
                        let mut child = scope.child();
                        let pattern = self.lower_pattern(
                            module,
                            &mut child,
                            &arm.pattern,
                            subject_ty.as_value(),
                            PatternUse::Match,
                        );
                        self.record_pattern_coverage(
                            &pattern,
                            &mut covered,
                            &mut wildcard,
                            arm.pattern.span,
                        );
                        let body = if matches!(arm.body.value, ast::ExprKind::Unreachable) {
                            vec![Statement::Unreachable {
                                source: source(module, arm.body.span),
                            }]
                        } else {
                            let fields = arguments
                                .iter()
                                .enumerate()
                                .map(|(index, argument)| {
                                    let argument = if index == controlled_index {
                                        &arm.body
                                    } else {
                                        argument
                                    };
                                    let expected = constructor.fields.get(index).map(|(_, ty)| ty);
                                    let (value, actual) = self.lower_expr(
                                        module,
                                        &child,
                                        argument,
                                        expected,
                                        ExprMode::Reaction,
                                    );
                                    if let Some(expected) = expected {
                                        self.expect_type(&actual, expected, argument.span);
                                    }
                                    (
                                        constructor
                                            .fields
                                            .get(index)
                                            .and_then(|(name, _)| name.clone()),
                                        value,
                                    )
                                })
                                .collect();
                            vec![Statement::Finish {
                                outcome: IrExpr::Constructor {
                                    type_id: constructor.type_id.clone(),
                                    constructor: constructor.name.clone(),
                                    fields,
                                },
                                source: source(module, finish_span),
                            }]
                        };
                        StatementMatchArm { pattern, body }
                    })
                    .collect();
                self.check_exhaustive(subject_ty.as_value(), &covered, wildcard, controlled.span);
                vec![Statement::Match {
                    value: subject,
                    arms,
                    source: source(module, controlled.span),
                }]
            }
            ast::ExprKind::Unreachable => vec![Statement::Unreachable {
                source: source(module, controlled.span),
            }],
            _ => {
                self.diagnostics.push(error(
                    codes::UNSUPPORTED,
                    "uhura/nested-terminal-control",
                    "nested terminal control is supported only through a finite match",
                    controlled.span,
                ));
                let _ = outcome;
                Vec::new()
            }
        }
    }
}

fn materialize_pure_continuation_bindings(
    expression: IrExpr,
    compiled: &BTreeMap<String, IrExpr>,
    expanded: &mut BTreeMap<String, IrExpr>,
) -> IrExpr {
    fn continuation(
        name: &str,
        compiled: &BTreeMap<String, IrExpr>,
        expanded: &mut BTreeMap<String, IrExpr>,
    ) -> IrExpr {
        if let Some(expression) = expanded.get(name) {
            return expression.clone();
        }
        let expression = compiled
            .get(name)
            .cloned()
            .expect("every generated continuation binding is compiled");
        let expression = materialize_pure_continuation_bindings(expression, compiled, expanded);
        expanded.insert(name.into(), expression.clone());
        expression
    }

    match expression {
        IrExpr::Constructor {
            type_id,
            constructor: constructor_name,
            fields,
        } => IrExpr::Constructor {
            type_id,
            constructor: constructor_name,
            fields: fields
                .into_iter()
                .map(|(name, value)| {
                    (
                        name,
                        materialize_pure_continuation_bindings(value, compiled, expanded),
                    )
                })
                .collect(),
        },
        IrExpr::Key { type_id, value } => IrExpr::Key {
            type_id,
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
        },
        IrExpr::Tuple { values } => IrExpr::Tuple {
            values: values
                .into_iter()
                .map(|value| materialize_pure_continuation_bindings(value, compiled, expanded))
                .collect(),
        },
        IrExpr::Record { fields } => IrExpr::Record {
            fields: fields
                .into_iter()
                .map(|(name, value)| {
                    (
                        name,
                        materialize_pure_continuation_bindings(value, compiled, expanded),
                    )
                })
                .collect(),
        },
        IrExpr::Seq { values } => IrExpr::Seq {
            values: values
                .into_iter()
                .map(|value| materialize_pure_continuation_bindings(value, compiled, expanded))
                .collect(),
        },
        IrExpr::Map {
            entries,
            result_type,
        } => IrExpr::Map {
            entries: entries
                .into_iter()
                .map(|(key, value)| {
                    (
                        materialize_pure_continuation_bindings(key, compiled, expanded),
                        materialize_pure_continuation_bindings(value, compiled, expanded),
                    )
                })
                .collect(),
            result_type,
        },
        IrExpr::Table { key_type, entries } => IrExpr::Table {
            key_type,
            entries: entries
                .into_iter()
                .map(|(name, value)| {
                    (
                        name,
                        materialize_pure_continuation_bindings(value, compiled, expanded),
                    )
                })
                .collect(),
        },
        IrExpr::Unary { op, value } => IrExpr::Unary {
            op,
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
        },
        IrExpr::Binary { op, left, right } => IrExpr::Binary {
            op,
            left: Box::new(materialize_pure_continuation_bindings(
                *left, compiled, expanded,
            )),
            right: Box::new(materialize_pure_continuation_bindings(
                *right, compiled, expanded,
            )),
        },
        IrExpr::Call {
            function,
            args,
            result_type,
        } => IrExpr::Call {
            function,
            args: args
                .into_iter()
                .map(|value| materialize_pure_continuation_bindings(value, compiled, expanded))
                .collect(),
            result_type,
        },
        IrExpr::Invoke { function, args } => IrExpr::Invoke {
            function: Box::new(materialize_pure_continuation_bindings(
                *function, compiled, expanded,
            )),
            args: args
                .into_iter()
                .map(|value| materialize_pure_continuation_bindings(value, compiled, expanded))
                .collect(),
        },
        IrExpr::Field { value, field } => IrExpr::Field {
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
            field,
        },
        IrExpr::Index { value, key } => IrExpr::Index {
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
            key: Box::new(materialize_pure_continuation_bindings(
                *key, compiled, expanded,
            )),
        },
        IrExpr::Method {
            value,
            method,
            args,
            result_type,
        } => IrExpr::Method {
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
            method,
            args: args
                .into_iter()
                .map(|value| materialize_pure_continuation_bindings(value, compiled, expanded))
                .collect(),
            result_type,
        },
        IrExpr::If {
            condition,
            then_value,
            else_value,
        } => IrExpr::If {
            condition: Box::new(materialize_pure_continuation_bindings(
                *condition, compiled, expanded,
            )),
            then_value: Box::new(materialize_pure_continuation_bindings(
                *then_value,
                compiled,
                expanded,
            )),
            else_value: Box::new(materialize_pure_continuation_bindings(
                *else_value,
                compiled,
                expanded,
            )),
        },
        IrExpr::Match { value, arms } => IrExpr::Match {
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
            arms: arms
                .into_iter()
                .map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    value: materialize_pure_continuation_bindings(arm.value, compiled, expanded),
                })
                .collect(),
        },
        IrExpr::Is { value, pattern } => IrExpr::Is {
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
            pattern,
        },
        IrExpr::Update { value, fields } => IrExpr::Update {
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
            fields: fields
                .into_iter()
                .map(|(name, value)| {
                    (
                        name,
                        materialize_pure_continuation_bindings(value, compiled, expanded),
                    )
                })
                .collect(),
        },
        IrExpr::Let { bindings, value } => IrExpr::Let {
            bindings: bindings
                .into_iter()
                .map(|(name, value)| {
                    let value = if name.starts_with(PURE_CONTINUATION_LOCAL_PREFIX) {
                        continuation(&name, compiled, expanded)
                    } else {
                        materialize_pure_continuation_bindings(value, compiled, expanded)
                    };
                    (name, value)
                })
                .collect(),
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
        },
        IrExpr::Lambda { params, body } => IrExpr::Lambda {
            params,
            body: Box::new(materialize_pure_continuation_bindings(
                *body, compiled, expanded,
            )),
        },
        IrExpr::Collect { clauses } => IrExpr::Collect {
            clauses: clauses
                .into_iter()
                .map(|(condition, value)| {
                    (
                        materialize_pure_continuation_bindings(condition, compiled, expanded),
                        materialize_pure_continuation_bindings(value, compiled, expanded),
                    )
                })
                .collect(),
        },
        IrExpr::SetComprehension {
            pattern,
            source,
            conditions,
            value,
            result_type,
        } => IrExpr::SetComprehension {
            pattern,
            source: Box::new(materialize_pure_continuation_bindings(
                *source, compiled, expanded,
            )),
            conditions: conditions
                .into_iter()
                .map(|condition| {
                    materialize_pure_continuation_bindings(condition, compiled, expanded)
                })
                .collect(),
            value: Box::new(materialize_pure_continuation_bindings(
                *value, compiled, expanded,
            )),
            result_type,
        },
        expression @ (IrExpr::Literal { .. } | IrExpr::Name { .. }) => expression,
    }
}

fn unique_member<'a, T>(
    members: &'a [ast::MachineMember],
    mut select: impl FnMut(&'a ast::MachineMemberKind) -> Option<&'a T>,
) -> Option<&'a T> {
    members.iter().find_map(|member| select(&member.value))
}

fn handler_input_name(pattern: &ast::Pattern) -> String {
    match &pattern.value {
        ast::PatternKind::Name(name) => name.value.clone(),
        ast::PatternKind::Constructor { path, .. } => path
            .iter()
            .map(|part| part.value.as_str())
            .collect::<Vec<_>>()
            .join("."),
        _ => format!("<invalid:{}>", pattern.span.start),
    }
}

fn statements_terminal(statements: &[Statement]) -> bool {
    statements.last().is_some_and(|statement| match statement {
        Statement::Finish { .. } | Statement::Unreachable { .. } | Statement::Delegate { .. } => {
            true
        }
        Statement::If {
            then_body,
            else_body,
            ..
        } => statements_terminal(then_body) && statements_terminal(else_body),
        Statement::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| statements_terminal(&arm.body))
        }
        Statement::While { .. }
        | Statement::Let { .. }
        | Statement::Set { .. }
        | Statement::Emit { .. } => false,
    })
}

fn port_ty(ty: &TypeRef) -> uhura_port::TypeRef {
    uhura_port::TypeRef::new(ty.canonical_name()).expect("checker type spelling is canonical")
}

fn standard_export(module: &str, name: &str) -> Option<Export> {
    match (module, name) {
        ("uhura.boundary@1", "Token") => Some(Export::Type(TypeRef::Named { id: "Token".into() })),
        ("uhura.observation@1", "Observation")
        | ("uhura.ports@1", "RequestPort" | "SinkPort")
        | ("uhura.web_router@1", "Router") => Some(Export::PortContract),
        ("uhura.web_router@1", "Routes") => Some(Export::Type(TypeRef::Named {
            id: "Routes".into(),
        })),
        ("uhura.web_router@1", "routes") => Some(Export::PureHelper),
        ("uhura.web_router@1", "Link") | ("uhura.ui_surface@1", "Surface") => {
            Some(Export::UiElement)
        }
        _ => None,
    }
}

fn is_binding_reserved_builtin(value: &str) -> bool {
    matches!(
        value,
        "Bool"
            | "Unit"
            | "Never"
            | "Int"
            | "Nat"
            | "PositiveInt"
            | "Decimal"
            | "BoundaryNumber"
            | "Ratio"
            | "Text"
            | "Option"
            | "Seq"
            | "NonEmpty"
            | "Set"
            | "Map"
            | "Table"
            | "FiniteView"
            | "min"
            | "max"
    )
}

fn finite_view_path(
    ty: &TypeRef,
    registry: &TypeRegistry,
    visited: &mut BTreeSet<String>,
) -> Option<Vec<String>> {
    fn nested(segment: impl Into<String>, path: Option<Vec<String>>) -> Option<Vec<String>> {
        path.map(|mut path| {
            path.insert(0, segment.into());
            path
        })
    }

    match ty {
        TypeRef::FiniteView { .. } => Some(vec!["FiniteView".into()]),
        TypeRef::Option { value } => {
            nested("Option.value", finite_view_path(value, registry, visited))
        }
        TypeRef::Seq { value } => nested("Seq.item", finite_view_path(value, registry, visited)),
        TypeRef::NonEmpty { value } => {
            nested("NonEmpty.item", finite_view_path(value, registry, visited))
        }
        TypeRef::Set { value } => nested("Set.item", finite_view_path(value, registry, visited)),
        TypeRef::Map { key, value } => nested("Map.key", finite_view_path(key, registry, visited))
            .or_else(|| nested("Map.value", finite_view_path(value, registry, visited))),
        TypeRef::Table { key, value } => {
            nested("Table.key", finite_view_path(key, registry, visited))
                .or_else(|| nested("Table.value", finite_view_path(value, registry, visited)))
        }
        TypeRef::Tuple { values } => values.iter().enumerate().find_map(|(index, value)| {
            nested(
                format!("Tuple[{}]", index + 1),
                finite_view_path(value, registry, visited),
            )
        }),
        TypeRef::Record { fields } => fields.iter().find_map(|(name, value)| {
            nested(
                format!("Record.{name}"),
                finite_view_path(value, registry, visited),
            )
        }),
        TypeRef::Named { id } => {
            if id.contains("FiniteView<") {
                return Some(vec![id.clone()]);
            }
            if !visited.insert(id.clone()) {
                return None;
            }
            let name = id.rsplit("::").next().unwrap_or(id);
            match registry.types.get(id).map(|info| &info.shape) {
                Some(TypeShape::Alias(value)) | Some(TypeShape::Key(value)) => {
                    nested(name, finite_view_path(value, registry, visited))
                }
                Some(TypeShape::Record(fields)) => fields.iter().find_map(|(field, value)| {
                    nested(
                        format!("{name}.{field}"),
                        finite_view_path(value, registry, visited),
                    )
                }),
                Some(TypeShape::Sum(constructors)) => constructors.iter().find_map(|constructor| {
                    constructor
                        .fields
                        .iter()
                        .enumerate()
                        .find_map(|(index, (field, value))| {
                            let field = field
                                .as_deref()
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| format!("#{}", index + 1));
                            nested(
                                format!("{name}.{}.{field}", constructor.name),
                                finite_view_path(value, registry, visited),
                            )
                        })
                }),
                None => None,
            }
        }
        TypeRef::Bool
        | TypeRef::Unit
        | TypeRef::Never
        | TypeRef::Int
        | TypeRef::Nat
        | TypeRef::PositiveInt
        | TypeRef::Decimal
        | TypeRef::BoundaryNumber
        | TypeRef::Ratio
        | TypeRef::Text => None,
    }
}

fn builtin_type(name: &str, args: &[TypeRef]) -> Option<TypeRef> {
    let scalar = match name {
        "Bool" => Some(TypeRef::Bool),
        "Unit" => Some(TypeRef::Unit),
        "Never" => Some(TypeRef::Never),
        "Int" => Some(TypeRef::Int),
        "Nat" => Some(TypeRef::Nat),
        "PositiveInt" => Some(TypeRef::PositiveInt),
        "Decimal" => Some(TypeRef::Decimal),
        "BoundaryNumber" => Some(TypeRef::BoundaryNumber),
        "Ratio" => Some(TypeRef::Ratio),
        "Text" => Some(TypeRef::Text),
        _ => None,
    };
    if scalar.is_some() {
        return scalar.filter(|_| args.is_empty());
    }
    match (name, args) {
        ("Option", [value]) => Some(TypeRef::Option {
            value: Box::new(value.clone()),
        }),
        ("Seq", [value]) => Some(TypeRef::Seq {
            value: Box::new(value.clone()),
        }),
        ("NonEmpty", [value]) => Some(TypeRef::NonEmpty {
            value: Box::new(value.clone()),
        }),
        ("Set", [value]) => Some(TypeRef::Set {
            value: Box::new(value.clone()),
        }),
        ("Map", [key, value]) => Some(TypeRef::Map {
            key: Box::new(key.clone()),
            value: Box::new(value.clone()),
        }),
        ("Table", [key, value]) => Some(TypeRef::Table {
            key: Box::new(key.clone()),
            value: Box::new(value.clone()),
        }),
        ("FiniteView", [value]) => Some(TypeRef::FiniteView {
            value: Box::new(value.clone()),
        }),
        ("Token" | "Routes", [value]) => Some(TypeRef::Named {
            id: format!("{name}<{}>", value.canonical_name()),
        }),
        _ => None,
    }
}

fn qualify(module: &str, name: &str) -> String {
    format!("{module}::{name}")
}

fn dependency_order(modules: &BTreeMap<String, ModuleEnv<'_>>, roots: &[String]) -> Vec<String> {
    fn visit(
        id: &str,
        modules: &BTreeMap<String, ModuleEnv<'_>>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
        output: &mut Vec<String>,
    ) {
        if visited.contains(id) || !modules.contains_key(id) {
            return;
        }
        if !visiting.insert(id.to_string()) {
            // Type-only import cycles are legal. The stable root/source order
            // breaks value-lowering ties; an actual constant cycle is still
            // rejected by constant evaluation.
            return;
        }
        let mut dependencies = modules[id]
            .module
            .imports
            .iter()
            .map(|import| import.target.clone())
            .filter(|target| modules.contains_key(target))
            .collect::<Vec<_>>();
        dependencies.sort();
        dependencies.dedup();
        for dependency in dependencies {
            visit(&dependency, modules, visiting, visited, output);
        }
        visiting.remove(id);
        if visited.insert(id.to_string()) {
            output.push(id.to_string());
        }
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut output = Vec::new();
    for id in roots {
        visit(id, modules, &mut visiting, &mut visited, &mut output);
    }
    for id in modules.keys() {
        visit(id, modules, &mut visiting, &mut visited, &mut output);
    }
    output
}

fn machine_qualify(module: &str, machine: &str, name: &str) -> String {
    format!("{module}::{machine}.{name}")
}

fn source(module: &ModuleEnv<'_>, span: ast::SourceSpan) -> SourceRef {
    let semantic_path = module
        .semantic_paths
        .get(&(span.start, span.end))
        .cloned()
        .unwrap_or_else(|| "unknown".into());
    SourceRef {
        // Runtime faults and evidence pins carry a semantic source identity.
        // Physical paths and byte offsets remain useful editor coordinates,
        // but must not perturb a program hash or fault identity when a file is
        // moved or whitespace is reformatted.
        id: format!("{}#{semantic_path}", module.id),
        path: module
            .physical_source_paths
            .get(&span.file)
            .cloned()
            .unwrap_or_else(|| module.module.source_id.path.clone()),
        start: span.start,
        end: span.end,
    }
}

fn semantic_path_index(module: &ast::Module) -> BTreeMap<(u32, u32), String> {
    let mut output = BTreeMap::new();
    if let Ok(value) = serde_json::to_value(module) {
        collect_semantic_paths(&value, String::new(), &mut output);
    }
    output
}

fn collect_semantic_paths(
    value: &serde_json::Value,
    path: String,
    output: &mut BTreeMap<(u32, u32), String>,
) {
    match value {
        serde_json::Value::Object(fields) => {
            if let Some(span) = fields.get("span").and_then(json_span_range) {
                output.entry(span).or_insert_with(|| {
                    if path.is_empty() {
                        "module".into()
                    } else {
                        path.clone()
                    }
                });
            }
            for (name, child) in fields {
                if matches!(name.as_str(), "source" | "source_id") {
                    continue;
                }
                let child_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}.{name}")
                };
                collect_semantic_paths(child, child_path, output);
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                collect_semantic_paths(child, format!("{path}[{index}]"), output);
            }
        }
        _ => {}
    }
}

fn json_span_range(value: &serde_json::Value) -> Option<(u32, u32)> {
    let fields = value.as_object()?;
    Some((
        fields.get("start")?.as_u64()?.try_into().ok()?,
        fields.get("end")?.as_u64()?.try_into().ok()?,
    ))
}

fn collect_calls(expression: &IrExpr, calls: &mut BTreeSet<String>) {
    match expression {
        IrExpr::Call { function, args, .. } => {
            calls.insert(function.clone());
            for value in args {
                collect_calls(value, calls);
            }
        }
        IrExpr::Invoke { function, args } => {
            collect_calls(function, calls);
            for value in args {
                collect_calls(value, calls);
            }
        }
        IrExpr::Constructor { fields, .. } => {
            for (_, value) in fields {
                collect_calls(value, calls);
            }
        }
        IrExpr::Key { value, .. } | IrExpr::Unary { value, .. } | IrExpr::Field { value, .. } => {
            collect_calls(value, calls)
        }
        IrExpr::Tuple { values } | IrExpr::Seq { values } => {
            for value in values {
                collect_calls(value, calls);
            }
        }
        IrExpr::Record { fields } => {
            for (_, value) in fields {
                collect_calls(value, calls);
            }
        }
        IrExpr::Map { entries, .. } => {
            for (key, value) in entries {
                collect_calls(key, calls);
                collect_calls(value, calls);
            }
        }
        IrExpr::Table { entries, .. } => {
            for (_, value) in entries {
                collect_calls(value, calls);
            }
        }
        IrExpr::Binary { left, right, .. } => {
            collect_calls(left, calls);
            collect_calls(right, calls);
        }
        IrExpr::Index { value, key } => {
            collect_calls(value, calls);
            collect_calls(key, calls);
        }
        IrExpr::Method { value, args, .. } => {
            collect_calls(value, calls);
            for arg in args {
                collect_calls(arg, calls);
            }
        }
        IrExpr::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_calls(condition, calls);
            collect_calls(then_value, calls);
            collect_calls(else_value, calls);
        }
        IrExpr::Match { value, arms } => {
            collect_calls(value, calls);
            for arm in arms {
                collect_calls(&arm.value, calls);
            }
        }
        IrExpr::Is { value, .. } => collect_calls(value, calls),
        IrExpr::Update { value, fields } => {
            collect_calls(value, calls);
            for (_, value) in fields {
                collect_calls(value, calls);
            }
        }
        IrExpr::Let { bindings, value } => {
            for (_, value) in bindings {
                collect_calls(value, calls);
            }
            collect_calls(value, calls);
        }
        IrExpr::Lambda { body, .. } => collect_calls(body, calls),
        IrExpr::Collect { clauses } => {
            for (condition, value) in clauses {
                collect_calls(condition, calls);
                collect_calls(value, calls);
            }
        }
        IrExpr::SetComprehension {
            source,
            conditions,
            value,
            ..
        } => {
            collect_calls(source, calls);
            for condition in conditions {
                collect_calls(condition, calls);
            }
            collect_calls(value, calls);
        }
        IrExpr::Literal { .. } | IrExpr::Name { .. } => {}
    }
}

fn collect_names(expression: &IrExpr, names: &mut BTreeSet<String>) {
    fn walk(value: &serde_json::Value, names: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Object(fields) => {
                if fields.get("kind").and_then(serde_json::Value::as_str) == Some("name")
                    && let Some(name) = fields.get("name").and_then(serde_json::Value::as_str)
                {
                    names.insert(name.to_string());
                }
                for child in fields.values() {
                    walk(child, names);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    walk(child, names);
                }
            }
            _ => {}
        }
    }
    if let Ok(value) = serde_json::to_value(expression) {
        walk(&value, names);
    }
}

fn inferred_type_is_complete(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Never => false,
        TypeRef::Option { value }
        | TypeRef::Seq { value }
        | TypeRef::NonEmpty { value }
        | TypeRef::Set { value }
        | TypeRef::FiniteView { value } => inferred_type_is_complete(value),
        TypeRef::Map { key, value } | TypeRef::Table { key, value } => {
            inferred_type_is_complete(key) && inferred_type_is_complete(value)
        }
        TypeRef::Tuple { values } => values.iter().all(inferred_type_is_complete),
        TypeRef::Record { fields } => fields
            .iter()
            .all(|(_, field)| inferred_type_is_complete(field)),
        TypeRef::Bool
        | TypeRef::Unit
        | TypeRef::Int
        | TypeRef::Nat
        | TypeRef::PositiveInt
        | TypeRef::Decimal
        | TypeRef::BoundaryNumber
        | TypeRef::Ratio
        | TypeRef::Text
        | TypeRef::Named { .. } => true,
    }
}

fn collect_source_names(
    expression: &ast::Expr,
    bound: &mut BTreeSet<String>,
    names: &mut BTreeSet<String>,
) {
    match &expression.value {
        ast::ExprKind::Name(name) => {
            if !bound.contains(&name.value) {
                names.insert(name.value.clone());
            }
        }
        ast::ExprKind::Tuple(values) | ast::ExprKind::Sequence(values) => {
            for value in values {
                collect_source_names(value, bound, names);
            }
        }
        ast::ExprKind::Record(fields) => {
            for field in fields {
                collect_source_names(&field.value, bound, names);
            }
        }
        ast::ExprKind::Block(block) => collect_source_block_names(block, bound, names),
        ast::ExprKind::Unary { operand, .. }
        | ast::ExprKind::Is { value: operand, .. }
        | ast::ExprKind::Finish(operand) => collect_source_names(operand, bound, names),
        ast::ExprKind::Binary { left, right, .. }
        | ast::ExprKind::Index {
            receiver: left,
            index: right,
        } => {
            collect_source_names(left, bound, names);
            collect_source_names(right, bound, names);
        }
        ast::ExprKind::Call { callee, arguments } => {
            collect_source_names(callee, bound, names);
            for argument in arguments {
                collect_source_names(argument, bound, names);
            }
        }
        ast::ExprKind::Member { receiver, .. } => {
            collect_source_names(receiver, bound, names);
        }
        ast::ExprKind::Update { base, fields } => {
            collect_source_names(base, bound, names);
            for field in fields {
                collect_source_names(&field.value, bound, names);
            }
        }
        ast::ExprKind::Lambda { parameters, body } => {
            let mut child = bound.clone();
            for parameter in parameters {
                collect_pattern_bindings(parameter, &mut child);
            }
            collect_source_names(body, &mut child, names);
        }
        ast::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_source_names(condition, bound, names);
            collect_source_names(then_branch, &mut bound.clone(), names);
            if let Some(else_branch) = else_branch {
                collect_source_names(else_branch, &mut bound.clone(), names);
            }
        }
        ast::ExprKind::Match { subject, arms } => {
            collect_source_names(subject, bound, names);
            for arm in arms {
                let mut child = bound.clone();
                collect_pattern_bindings(&arm.pattern, &mut child);
                collect_source_names(&arm.body, &mut child, names);
            }
        }
        ast::ExprKind::Collect(clauses) => {
            for clause in clauses {
                collect_source_names(&clause.condition, bound, names);
                collect_source_names(&clause.value, bound, names);
            }
        }
        ast::ExprKind::SetComprehension {
            binding,
            source,
            filters,
            value,
        } => {
            collect_source_names(source, bound, names);
            let mut child = bound.clone();
            collect_pattern_bindings(binding, &mut child);
            for filter in filters {
                collect_source_names(filter, &mut child, names);
            }
            collect_source_names(value, &mut child, names);
        }
        ast::ExprKind::Integer(_)
        | ast::ExprKind::Decimal(_)
        | ast::ExprKind::Text(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::Unreachable
        | ast::ExprKind::Error => {}
    }
}

fn collect_source_block_names(
    block: &ast::Block,
    bound: &mut BTreeSet<String>,
    names: &mut BTreeSet<String>,
) {
    let mut child = bound.clone();
    for statement in &block.statements {
        match &statement.value {
            ast::StatementKind::Let { name, value, .. } => {
                collect_source_names(value, &mut child, names);
                child.insert(name.value.clone());
            }
            ast::StatementKind::Set { value, .. }
            | ast::StatementKind::Emit(value)
            | ast::StatementKind::Expr(value) => {
                collect_source_names(value, &mut child, names);
            }
            ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                collect_source_names(condition, &mut child, names);
                collect_source_names(decreases, &mut child, names);
                collect_source_block_names(body, &mut child, names);
            }
        }
    }
}

fn collect_pattern_bindings(pattern: &ast::Pattern, bound: &mut BTreeSet<String>) {
    match &pattern.value {
        ast::PatternKind::Name(name) => {
            bound.insert(name.value.clone());
        }
        ast::PatternKind::Constructor { arguments, .. }
        | ast::PatternKind::Tuple(arguments)
        | ast::PatternKind::Alternative(arguments) => {
            for argument in arguments {
                collect_pattern_bindings(argument, bound);
            }
        }
        ast::PatternKind::Record { fields, .. } => {
            for field in fields {
                collect_pattern_bindings(&field.pattern, bound);
            }
        }
        ast::PatternKind::Wildcard
        | ast::PatternKind::Rest
        | ast::PatternKind::Integer(_)
        | ast::PatternKind::Decimal(_)
        | ast::PatternKind::Text(_)
        | ast::PatternKind::Bool(_)
        | ast::PatternKind::Error => {}
    }
}

fn cyclic_nodes(graph: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    fn reaches(
        origin: &str,
        current: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        visited: &mut BTreeSet<String>,
    ) -> bool {
        if !visited.insert(current.to_string()) {
            return false;
        }
        graph.get(current).is_some_and(|next| {
            next.iter()
                .any(|candidate| candidate == origin || reaches(origin, candidate, graph, visited))
        })
    }

    graph
        .keys()
        .filter(|origin| reaches(origin, origin, graph, &mut BTreeSet::new()))
        .cloned()
        .collect()
}

fn graph_dependency_order(
    graph: &BTreeMap<String, BTreeSet<String>>,
    excluded: &BTreeSet<String>,
) -> Vec<String> {
    fn visit(
        node: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        excluded: &BTreeSet<String>,
        visited: &mut BTreeSet<String>,
        output: &mut Vec<String>,
    ) {
        if excluded.contains(node) || !visited.insert(node.to_string()) {
            return;
        }
        if let Some(dependencies) = graph.get(node) {
            for dependency in dependencies {
                visit(dependency, graph, excluded, visited, output);
            }
        }
        output.push(node.to_string());
    }

    let mut visited = BTreeSet::new();
    let mut output = Vec::new();
    for node in graph.keys() {
        visit(node, graph, excluded, &mut visited, &mut output);
    }
    output
}

fn lower_binary(value: ast::BinaryOp) -> IrBinaryOp {
    match value {
        ast::BinaryOp::Or => IrBinaryOp::Or,
        ast::BinaryOp::And => IrBinaryOp::And,
        ast::BinaryOp::Equal => IrBinaryOp::Equal,
        ast::BinaryOp::NotEqual => IrBinaryOp::NotEqual,
        ast::BinaryOp::Less => IrBinaryOp::Less,
        ast::BinaryOp::LessEqual => IrBinaryOp::LessEqual,
        ast::BinaryOp::Greater => IrBinaryOp::Greater,
        ast::BinaryOp::GreaterEqual => IrBinaryOp::GreaterEqual,
        ast::BinaryOp::Add => IrBinaryOp::Add,
        ast::BinaryOp::Subtract => IrBinaryOp::Subtract,
        ast::BinaryOp::Multiply => IrBinaryOp::Multiply,
    }
}

fn ir_numeric_path(expression: &IrExpr) -> Option<String> {
    match expression {
        IrExpr::Name { name } => Some(name.clone()),
        IrExpr::Field { value, field } => {
            ir_numeric_path(value).map(|path| format!("{path}.{field}"))
        }
        _ => None,
    }
}

fn ast_member_path(expression: &ast::Expr) -> Option<Vec<String>> {
    match &expression.value {
        ast::ExprKind::Name(name) => Some(vec![name.value.clone()]),
        ast::ExprKind::Member { receiver, member } => {
            let mut path = ast_member_path(receiver)?;
            path.push(member.value.clone());
            Some(path)
        }
        _ => None,
    }
}

fn ast_numeric_path(scope: &Scope, expression: &ast::Expr) -> Option<String> {
    match &expression.value {
        ast::ExprKind::Name(name) => Some(
            scope
                .values
                .get(&name.value)
                .map(|binding| binding.lowered.clone())
                .unwrap_or_else(|| name.value.clone()),
        ),
        ast::ExprKind::Member { receiver, member } => {
            ast_numeric_path(scope, receiver).map(|path| format!("{path}.{}", member.value))
        }
        _ => None,
    }
}

fn static_integer_minimum_for_path(
    registry: &TypeRegistry,
    scope: &Scope,
    path: &str,
) -> Option<i64> {
    let (base, binding) = scope
        .values
        .values()
        .filter(|binding| {
            path == binding.lowered
                || path
                    .strip_prefix(&binding.lowered)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        })
        .max_by_key(|binding| binding.lowered.len())
        .map(|binding| (binding.lowered.as_str(), binding))?;
    let mut ty = binding.ty.as_value()?.clone();
    if let Some(suffix) = path.strip_prefix(base) {
        for field in suffix.trim_start_matches('.').split('.') {
            if field.is_empty() {
                continue;
            }
            ty = registry
                .fields(&ty)?
                .into_iter()
                .find(|(name, _)| name == field)?
                .1;
        }
    }
    match ty {
        TypeRef::Nat => Some(0),
        TypeRef::PositiveInt => Some(1),
        _ => None,
    }
}

fn refined_numeric_scope(
    scope: &Scope,
    expression: &ast::Expr,
    truth: bool,
    registry: &TypeRegistry,
) -> Scope {
    let mut refined = scope.child();
    install_numeric_condition(&mut refined, scope, expression, truth, registry);
    refined
}

fn install_numeric_condition(
    refined: &mut Scope,
    lookup: &Scope,
    expression: &ast::Expr,
    truth: bool,
    registry: &TypeRegistry,
) {
    match &expression.value {
        ast::ExprKind::Unary { op, operand } if op.value == ast::UnaryOp::Not => {
            install_numeric_condition(refined, lookup, operand, !truth, registry);
        }
        ast::ExprKind::Binary { left, op, right }
            if (op.value == ast::BinaryOp::And && truth)
                || (op.value == ast::BinaryOp::Or && !truth) =>
        {
            install_numeric_condition(refined, lookup, left, truth, registry);
            install_numeric_condition(refined, lookup, right, truth, registry);
        }
        ast::ExprKind::Binary { left, op, right } => {
            install_numeric_comparison(refined, lookup, left, op.value, right, truth, registry);
        }
        _ => {}
    }
}

fn install_numeric_comparison(
    refined: &mut Scope,
    lookup: &Scope,
    left: &ast::Expr,
    op: ast::BinaryOp,
    right: &ast::Expr,
    truth: bool,
    registry: &TypeRegistry,
) {
    let effective = if truth { op } else { negate_comparison(op) };
    if let (Some(path), Some(value)) = (ast_numeric_path(lookup, left), integer_literal(right)) {
        apply_path_bound(refined, path, effective, value);
        return;
    }
    if let (Some(value), Some(path)) = (integer_literal(left), ast_numeric_path(lookup, right)) {
        apply_path_bound(refined, path, reverse_comparison(effective), value);
        return;
    }
    let (Some(left), Some(right)) = (
        ast_numeric_path(lookup, left),
        ast_numeric_path(lookup, right),
    ) else {
        return;
    };
    match effective {
        ast::BinaryOp::Less => {
            refined.less_than.insert((left.clone(), right.clone()));
            if let Some(maximum) = lookup
                .numeric_bounds
                .get(&right)
                .and_then(|bounds| bounds.max)
            {
                refined.numeric_bounds.entry(left).or_default().max =
                    Some(maximum.saturating_sub(1));
            }
        }
        ast::BinaryOp::LessEqual | ast::BinaryOp::Equal => {
            refined.less_equal.insert((left.clone(), right.clone()));
            if effective == ast::BinaryOp::Equal {
                refined.less_equal.insert((right, left));
            }
        }
        ast::BinaryOp::Greater => {
            refined.less_than.insert((right.clone(), left.clone()));
            if let Some(minimum) = lookup
                .numeric_bounds
                .get(&right)
                .and_then(|bounds| bounds.min)
                .or_else(|| static_integer_minimum_for_path(registry, lookup, &right))
            {
                let bounds = refined.numeric_bounds.entry(left).or_default();
                let minimum = minimum.saturating_add(1);
                bounds.min = Some(bounds.min.map_or(minimum, |old| old.max(minimum)));
            }
        }
        ast::BinaryOp::GreaterEqual => {
            refined.less_equal.insert((right.clone(), left.clone()));
            if let Some(minimum) = lookup
                .numeric_bounds
                .get(&right)
                .and_then(|bounds| bounds.min)
                .or_else(|| static_integer_minimum_for_path(registry, lookup, &right))
            {
                let bounds = refined.numeric_bounds.entry(left).or_default();
                bounds.min = Some(bounds.min.map_or(minimum, |old| old.max(minimum)));
            }
        }
        _ => {}
    }
}

fn integer_literal(expression: &ast::Expr) -> Option<i64> {
    match &expression.value {
        ast::ExprKind::Integer(value) => value.parse().ok(),
        ast::ExprKind::Decimal(value) => {
            let value = value.parse::<Decimal>().ok()?;
            value
                .is_integral()
                .then(|| value.canonical_text().parse().ok())
                .flatten()
        }
        ast::ExprKind::Unary { op, operand } if op.value == ast::UnaryOp::Negate => {
            integer_literal(operand)?.checked_neg()
        }
        _ => None,
    }
}

fn apply_path_bound(scope: &mut Scope, path: String, op: ast::BinaryOp, value: i64) {
    let bounds = scope.numeric_bounds.entry(path).or_default();
    match op {
        ast::BinaryOp::Less => {
            bounds.max = Some(bounds.max.map_or(value - 1, |old| old.min(value - 1)));
        }
        ast::BinaryOp::LessEqual => {
            bounds.max = Some(bounds.max.map_or(value, |old| old.min(value)));
        }
        ast::BinaryOp::Greater => {
            bounds.min = Some(bounds.min.map_or(value + 1, |old| old.max(value + 1)));
        }
        ast::BinaryOp::GreaterEqual => {
            bounds.min = Some(bounds.min.map_or(value, |old| old.max(value)));
        }
        ast::BinaryOp::Equal => {
            bounds.min = Some(value);
            bounds.max = Some(value);
        }
        ast::BinaryOp::NotEqual => {
            if bounds.min == Some(value) {
                bounds.min = Some(value.saturating_add(1));
            }
            if bounds.max == Some(value) {
                bounds.max = Some(value.saturating_sub(1));
            }
        }
        _ => {}
    }
}

fn negate_comparison(op: ast::BinaryOp) -> ast::BinaryOp {
    match op {
        ast::BinaryOp::Equal => ast::BinaryOp::NotEqual,
        ast::BinaryOp::NotEqual => ast::BinaryOp::Equal,
        ast::BinaryOp::Less => ast::BinaryOp::GreaterEqual,
        ast::BinaryOp::LessEqual => ast::BinaryOp::Greater,
        ast::BinaryOp::Greater => ast::BinaryOp::LessEqual,
        ast::BinaryOp::GreaterEqual => ast::BinaryOp::Less,
        other => other,
    }
}

fn reverse_comparison(op: ast::BinaryOp) -> ast::BinaryOp {
    match op {
        ast::BinaryOp::Less => ast::BinaryOp::Greater,
        ast::BinaryOp::LessEqual => ast::BinaryOp::GreaterEqual,
        ast::BinaryOp::Greater => ast::BinaryOp::Less,
        ast::BinaryOp::GreaterEqual => ast::BinaryOp::LessEqual,
        other => other,
    }
}

fn integer_lower_bound(expression: &IrExpr, scope: &Scope) -> Option<i64> {
    match expression {
        IrExpr::Literal {
            value: Value::Integer { value, .. },
        } => value.to_string().parse().ok(),
        IrExpr::Name { .. } | IrExpr::Field { .. } => {
            ir_numeric_path(expression).and_then(|path| {
                scope
                    .numeric_bounds
                    .get(&path)
                    .and_then(|bounds| bounds.min)
            })
        }
        IrExpr::Binary {
            op: IrBinaryOp::Add,
            left,
            right,
        } => integer_lower_bound(left, scope)?.checked_add(integer_lower_bound(right, scope)?),
        IrExpr::Binary {
            op: IrBinaryOp::Multiply,
            left,
            right,
        } => {
            let left = integer_lower_bound(left, scope)?;
            let right = integer_lower_bound(right, scope)?;
            (left >= 0 && right >= 0).then(|| left.saturating_mul(right))
        }
        IrExpr::Call { function, args, .. }
            if matches!(
                function.as_str(),
                "__coerce_int" | "__coerce_nat" | "__coerce_positive"
            ) =>
        {
            let value = args
                .first()
                .and_then(|value| integer_lower_bound(value, scope));
            match function.as_str() {
                "__coerce_nat" => Some(value.unwrap_or(0).max(0)),
                "__coerce_positive" => Some(value.unwrap_or(1).max(1)),
                _ => value,
            }
        }
        _ => None,
    }
}

fn integer_difference_non_negative(expression: &IrExpr, scope: &Scope) -> bool {
    let IrExpr::Binary {
        op: IrBinaryOp::Subtract,
        left,
        right,
    } = expression
    else {
        return false;
    };
    let Some(left) = ir_numeric_path(left) else {
        return false;
    };
    let Some(right) = ir_numeric_path(right) else {
        return false;
    };
    scope.less_equal.contains(&(right, left))
}

fn ratio_arithmetic_proven(scope: &Scope, op: IrBinaryOp, left: &IrExpr, right: &IrExpr) -> bool {
    match op {
        IrBinaryOp::Multiply => true,
        IrBinaryOp::Add => {
            let Some(left_maximum) = ratio_bound(scope, left, false) else {
                return false;
            };
            let Some(right_maximum) = ratio_bound(scope, right, false) else {
                return false;
            };
            left_maximum.add(&right_maximum) <= Decimal::one()
        }
        IrBinaryOp::Subtract => {
            if left == right {
                return true;
            }
            if let (Some(left), Some(right)) = (ir_numeric_path(left), ir_numeric_path(right))
                && (scope.less_equal.contains(&(right.clone(), left.clone()))
                    || scope.less_than.contains(&(right, left)))
            {
                return true;
            }
            let Some(left_minimum) = ratio_bound(scope, left, true) else {
                return false;
            };
            let Some(right_maximum) = ratio_bound(scope, right, false) else {
                return false;
            };
            left_minimum >= right_maximum
        }
        _ => false,
    }
}

fn ratio_bound(scope: &Scope, expression: &IrExpr, minimum: bool) -> Option<Decimal> {
    match expression {
        IrExpr::Literal {
            value: Value::Ratio(value),
        } => Some(value.clone()),
        IrExpr::Name { .. } | IrExpr::Field { .. } => {
            let path = ir_numeric_path(expression)?;
            let bounds = scope.numeric_bounds.get(&path);
            let value = if minimum {
                bounds.and_then(|bounds| bounds.min).unwrap_or(0)
            } else {
                bounds.and_then(|bounds| bounds.max).unwrap_or(1)
            };
            value.to_string().parse().ok()
        }
        _ => None,
    }
}

fn exact_integer(text: &str, kind: &str) -> Result<Value, String> {
    Value::from_wire_json(&serde_json::json!({"$": kind, "value": text}))
        .map_err(|error| error.to_string())
}

fn exact_decimal(text: &str) -> Result<Value, String> {
    Value::from_wire_json(&serde_json::json!({"$": "Decimal", "value": text}))
        .map_err(|error| error.to_string())
}

fn exact_number_value(text: &str, ty: &TypeRef) -> Result<Value, String> {
    match ty {
        TypeRef::Int => exact_integer(text, "Int"),
        TypeRef::Nat => exact_integer(text, "Nat"),
        TypeRef::PositiveInt => exact_integer(text, "PositiveInt"),
        TypeRef::Decimal => exact_decimal(text),
        TypeRef::Ratio => Value::from_wire_json(&serde_json::json!({
            "$": "Ratio",
            "value": text,
        }))
        .map_err(|error| error.to_string()),
        TypeRef::BoundaryNumber => Value::from_wire_json(&serde_json::json!({
            "$": "BoundaryNumber",
            "case": "finite",
            "value": text,
        }))
        .map_err(|error| error.to_string()),
        _ => Err(format!(
            "`{text}` is not a literal for `{}`",
            ty.canonical_name()
        )),
    }
}

fn record_key(expression: &ast::Expr) -> Option<String> {
    match &expression.value {
        ast::ExprKind::Name(name) => Some(name.value.clone()),
        ast::ExprKind::Text(value) => Some(value.clone()),
        _ => None,
    }
}

fn is_qualified_call(expression: &ast::Expr) -> bool {
    matches!(
        &expression.value,
        ast::ExprKind::Call { callee, .. }
            if matches!(callee.value, ast::ExprKind::Member { .. })
    )
}

fn collection_item_type(ty: Option<&TypeRef>) -> Option<TypeRef> {
    match ty? {
        TypeRef::Seq { value }
        | TypeRef::NonEmpty { value }
        | TypeRef::Set { value }
        | TypeRef::FiniteView { value } => Some(value.as_ref().clone()),
        TypeRef::Map { key, value } => Some(TypeRef::Record {
            fields: vec![
                ("key".into(), key.as_ref().clone()),
                ("value".into(), value.as_ref().clone()),
            ],
        }),
        TypeRef::Table { key, value } => Some(TypeRef::Tuple {
            values: vec![key.as_ref().clone(), value.as_ref().clone()],
        }),
        _ => None,
    }
}

fn pattern_coverage(pattern: &IrPattern, covered: &mut BTreeSet<String>, wildcard: &mut bool) {
    match pattern {
        IrPattern::Ignore | IrPattern::Bind { .. } => *wildcard = true,
        IrPattern::Constructor {
            constructor,
            fields,
            ..
        } => {
            if fields.iter().all(pattern_irrefutable) {
                covered.insert(format!("constructor:{constructor}"));
            }
        }
        IrPattern::Alternative { patterns } => {
            for pattern in patterns {
                pattern_coverage(pattern, covered, wildcard);
            }
        }
        IrPattern::Literal { value } => {
            let atom = match value {
                Value::Bool(value) => format!("literal:{value}"),
                _ => format!(
                    "literal:{}",
                    uhura_core::codec::hex(&value.canonical_bytes())
                ),
            };
            covered.insert(atom);
        }
        IrPattern::Tuple { .. } | IrPattern::Record { .. } => {}
    }
}

fn pattern_irrefutable(pattern: &IrPattern) -> bool {
    match pattern {
        IrPattern::Ignore | IrPattern::Bind { .. } => true,
        IrPattern::Tuple { values } => values.iter().all(pattern_irrefutable),
        IrPattern::Record { fields, .. } => fields
            .iter()
            .all(|(_, pattern)| pattern_irrefutable(pattern)),
        IrPattern::Alternative { patterns } => patterns.iter().any(pattern_irrefutable),
        IrPattern::Literal { .. } | IrPattern::Constructor { .. } => false,
    }
}

fn flatten_alternatives(patterns: &[IrPattern]) -> Vec<&IrPattern> {
    fn push<'a>(pattern: &'a IrPattern, output: &mut Vec<&'a IrPattern>) {
        if let IrPattern::Alternative { patterns } = pattern {
            for pattern in patterns {
                push(pattern, output);
            }
        } else {
            output.push(pattern);
        }
    }
    let mut output = Vec::new();
    for pattern in patterns {
        push(pattern, &mut output);
    }
    output
}

fn patterns_overlap(left: &IrPattern, right: &IrPattern) -> bool {
    match (left, right) {
        (IrPattern::Ignore | IrPattern::Bind { .. }, _)
        | (_, IrPattern::Ignore | IrPattern::Bind { .. }) => true,
        (IrPattern::Literal { value: left }, IrPattern::Literal { value: right }) => left == right,
        (
            IrPattern::Constructor {
                type_id: left_type,
                constructor: left_constructor,
                fields: left_fields,
            },
            IrPattern::Constructor {
                type_id: right_type,
                constructor: right_constructor,
                fields: right_fields,
            },
        ) => {
            left_type == right_type
                && left_constructor == right_constructor
                && left_fields.len() == right_fields.len()
                && left_fields
                    .iter()
                    .zip(right_fields)
                    .all(|(left, right)| patterns_overlap(left, right))
        }
        (IrPattern::Tuple { values: left }, IrPattern::Tuple { values: right }) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| patterns_overlap(left, right))
        }
        (
            IrPattern::Record {
                fields: left,
                rest: left_rest,
            },
            IrPattern::Record {
                fields: right,
                rest: right_rest,
            },
        ) => {
            let shared_are_compatible = left.iter().all(|(name, left_pattern)| {
                right
                    .iter()
                    .find(|(right_name, _)| right_name == name)
                    .is_none_or(|(_, right_pattern)| patterns_overlap(left_pattern, right_pattern))
            });
            shared_are_compatible
                && (*left_rest
                    || *right_rest
                    || left
                        .iter()
                        .map(|(name, _)| name)
                        .eq(right.iter().map(|(name, _)| name)))
        }
        (IrPattern::Alternative { patterns }, other)
        | (other, IrPattern::Alternative { patterns }) => patterns
            .iter()
            .any(|pattern| patterns_overlap(pattern, other)),
        _ => false,
    }
}

fn loop_decrease_proven(condition: &ast::Expr, decreases: &ast::Expr, body: &ast::Block) -> bool {
    // `updates::lower_project` runs before the checker-neutral bridge and
    // transitively inlines every non-terminal update. Consequently this body
    // contains the complete write effect of every update call that can reach a
    // loop back edge: `assignments_to` sees a called update's writes exactly as
    // it sees authored assignments. Outcome-valued transitions are terminal
    // control and therefore do not contribute a back edge.
    if let Some(sequence) = sequence_size_measure(decreases) {
        let tails = uncons_tail_bindings(condition, &sequence);
        if tails.is_empty() {
            return false;
        }
        let assignments = assignments_to(body, &sequence);
        return assignments.iter().all(|assignment| {
            matches!(
                &assignment.value,
                ast::ExprKind::Name(name) if tails.contains(&name.value)
            )
        }) && loop_fallthrough_paths_assign(body, &sequence);
    }
    if let ast::ExprKind::Name(measure) = &decreases.value {
        let assignments = assignments_to(body, &measure.value);
        return assignments
            .iter()
            .all(|assignment| numeric_decrement_of(assignment, &measure.value))
            && loop_fallthrough_paths_assign(body, &measure.value);
    }
    false
}

fn numeric_decrement_of(expression: &ast::Expr, measure: &str) -> bool {
    matches!(
        &expression.value,
        ast::ExprKind::Binary { left, op, right }
            if op.value == ast::BinaryOp::Subtract
                && matches!(&left.value, ast::ExprKind::Name(name) if name.value == measure)
                && integer_literal(right).is_some_and(|value| value > 0)
    )
}

fn loop_fallthrough_paths_assign(block: &ast::Block, target: &str) -> bool {
    fn sequence<'a>(
        expressions: impl IntoIterator<Item = &'a ast::Expr>,
        mut paths: BTreeSet<bool>,
        target: &str,
    ) -> BTreeSet<bool> {
        for expression in expressions {
            paths = flow_expression(expression, paths, target);
            if paths.is_empty() {
                break;
            }
        }
        paths
    }

    fn generated_loop_exit(value: &ast::Expr) -> bool {
        matches!(
            &value.value,
            ast::ExprKind::Call { callee, arguments }
                if arguments.len() == 1
                    && matches!(
                        &callee.value,
                        ast::ExprKind::Name(name) if name.value == "some"
                    )
        )
    }

    fn flow_expression(
        expression: &ast::Expr,
        paths: BTreeSet<bool>,
        target: &str,
    ) -> BTreeSet<bool> {
        match &expression.value {
            ast::ExprKind::Block(block) => flow_block(block, paths, target),
            ast::ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let paths = flow_expression(condition, paths, target);
                let mut joined = BTreeSet::new();
                joined.extend(flow_expression(then_branch, paths.clone(), target));
                if let Some(else_branch) = else_branch {
                    joined.extend(flow_expression(else_branch, paths, target));
                } else {
                    joined.extend(paths);
                }
                joined
            }
            ast::ExprKind::Match { subject, arms } => {
                let paths = flow_expression(subject, paths, target);
                let mut joined = BTreeSet::new();
                for arm in arms {
                    joined.extend(flow_expression(&arm.body, paths.clone(), target));
                }
                joined
            }
            ast::ExprKind::Finish(_) | ast::ExprKind::Unreachable => BTreeSet::new(),
            ast::ExprKind::Tuple(values) | ast::ExprKind::Sequence(values) => {
                sequence(values, paths, target)
            }
            ast::ExprKind::Record(fields) => {
                sequence(fields.iter().map(|field| &field.value), paths, target)
            }
            ast::ExprKind::Unary { operand, .. }
            | ast::ExprKind::Is { value: operand, .. }
            | ast::ExprKind::Member {
                receiver: operand, ..
            } => flow_expression(operand, paths, target),
            ast::ExprKind::Lambda { .. } => paths,
            ast::ExprKind::Binary { left, right, .. }
            | ast::ExprKind::Index {
                receiver: left,
                index: right,
            } => {
                let paths = flow_expression(left, paths, target);
                flow_expression(right, paths, target)
            }
            ast::ExprKind::Call { callee, arguments } => {
                let paths = flow_expression(callee, paths, target);
                sequence(arguments, paths, target)
            }
            ast::ExprKind::Update { base, fields } => {
                let paths = flow_expression(base, paths, target);
                sequence(fields.iter().map(|field| &field.value), paths, target)
            }
            ast::ExprKind::Collect(clauses) => {
                let mut paths = paths;
                for clause in clauses {
                    paths = flow_expression(&clause.condition, paths, target);
                    paths = flow_expression(&clause.value, paths, target);
                }
                paths
            }
            ast::ExprKind::SetComprehension {
                source,
                filters,
                value,
                ..
            } => {
                let paths = flow_expression(source, paths, target);
                let paths = sequence(filters, paths, target);
                flow_expression(value, paths, target)
            }
            ast::ExprKind::Integer(_)
            | ast::ExprKind::Decimal(_)
            | ast::ExprKind::Text(_)
            | ast::ExprKind::Bool(_)
            | ast::ExprKind::Name(_)
            | ast::ExprKind::Error => paths,
        }
    }

    fn flow_block(block: &ast::Block, mut paths: BTreeSet<bool>, target: &str) -> BTreeSet<bool> {
        for statement in &block.statements {
            let mut next = BTreeSet::new();
            for assigned in paths {
                let singleton = BTreeSet::from([assigned]);
                match &statement.value {
                    ast::StatementKind::Let { name, value, .. } => {
                        let evaluated = flow_expression(value, singleton, target);
                        if !name.value.starts_with(INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX)
                            || !generated_loop_exit(value)
                        {
                            next.extend(evaluated);
                        }
                    }
                    ast::StatementKind::Set {
                        target: assigned_target,
                        value,
                    } => {
                        let evaluated = flow_expression(value, singleton, target);
                        if assigned_target.value == target {
                            next.extend(evaluated.into_iter().map(|_| true));
                        } else {
                            next.extend(evaluated);
                        }
                    }
                    ast::StatementKind::Emit(value) | ast::StatementKind::Expr(value) => {
                        next.extend(flow_expression(value, singleton, target));
                    }
                    // A nested loop may execute zero times, so it cannot by
                    // itself establish the enclosing loop's decrease. Any
                    // propagated lexical return is represented by its
                    // following generated match.
                    ast::StatementKind::While { .. } => {
                        next.insert(assigned);
                    }
                }
            }
            paths = next;
            if paths.is_empty() {
                break;
            }
        }
        paths
    }

    flow_block(block, BTreeSet::from([false]), target)
        .into_iter()
        .all(|assigned| assigned)
}

fn sequence_size_measure(expression: &ast::Expr) -> Option<String> {
    let ast::ExprKind::Member { receiver, member } = &expression.value else {
        return None;
    };
    if member.value != "size" {
        return None;
    }
    match &receiver.value {
        ast::ExprKind::Name(name) => Some(name.value.clone()),
        _ => None,
    }
}

fn uncons_tail_bindings(condition: &ast::Expr, sequence: &str) -> BTreeSet<String> {
    fn visit(expression: &ast::Expr, sequence: &str, tails: &mut BTreeSet<String>) {
        match &expression.value {
            ast::ExprKind::Binary { left, right, .. } => {
                visit(left, sequence, tails);
                visit(right, sequence, tails);
            }
            ast::ExprKind::Unary { operand, .. } => visit(operand, sequence, tails),
            ast::ExprKind::Is { value, pattern }
                if matches!(
                    &value.value,
                    ast::ExprKind::Member { receiver, member }
                        if member.value == "uncons"
                            && matches!(&receiver.value, ast::ExprKind::Name(name) if name.value == sequence)
                ) =>
            {
                if let ast::PatternKind::Constructor { path, arguments } = &pattern.value
                    && path.last().is_some_and(|name| name.value == "some")
                    && let Some(ast::Spanned {
                        value: ast::PatternKind::Record { fields, .. },
                        ..
                    }) = arguments.first()
                    && let Some(field) = fields.iter().find(|field| field.name.value == "tail")
                    && let ast::PatternKind::Name(name) = &field.pattern.value
                {
                    tails.insert(name.value.clone());
                }
            }
            _ => {}
        }
    }
    let mut tails = BTreeSet::new();
    visit(condition, sequence, &mut tails);
    tails
}

fn assignments_to<'a>(block: &'a ast::Block, target: &str) -> Vec<&'a ast::Expr> {
    fn expressions_in<'a>(
        expression: &'a ast::Expr,
        target: &str,
        output: &mut Vec<&'a ast::Expr>,
    ) {
        match &expression.value {
            ast::ExprKind::Block(block) => statements_in(block, target, output),
            ast::ExprKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                expressions_in(then_branch, target, output);
                if let Some(else_branch) = else_branch {
                    expressions_in(else_branch, target, output);
                }
            }
            ast::ExprKind::Match { arms, .. } => {
                for arm in arms {
                    expressions_in(&arm.body, target, output);
                }
            }
            _ => {}
        }
    }
    fn statements_in<'a>(block: &'a ast::Block, target: &str, output: &mut Vec<&'a ast::Expr>) {
        for statement in &block.statements {
            match &statement.value {
                ast::StatementKind::Set {
                    target: assigned,
                    value,
                } if assigned.value == target => output.push(value),
                ast::StatementKind::While { body, .. } => statements_in(body, target, output),
                ast::StatementKind::Let { value, .. }
                | ast::StatementKind::Emit(value)
                | ast::StatementKind::Expr(value)
                | ast::StatementKind::Set { value, .. } => {
                    expressions_in(value, target, output);
                }
            }
        }
    }
    let mut output = Vec::new();
    statements_in(block, target, &mut output);
    output
}

fn block_contains_finish_control(block: &ast::Block) -> bool {
    block
        .statements
        .iter()
        .any(|statement| match &statement.value {
            ast::StatementKind::Let { value, .. }
            | ast::StatementKind::Set { value, .. }
            | ast::StatementKind::Emit(value)
            | ast::StatementKind::Expr(value) => expression_contains_finish_control(value),
            ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                expression_contains_finish_control(condition)
                    || expression_contains_finish_control(decreases)
                    || block_contains_finish_control(body)
            }
        })
}

fn expression_contains_finish_control(expression: &ast::Expr) -> bool {
    match &expression.value {
        ast::ExprKind::Finish(_) => true,
        ast::ExprKind::Unreachable => false,
        ast::ExprKind::Block(block) => block_contains_finish_control(block),
        ast::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expression_contains_finish_control(condition)
                || expression_contains_finish_control(then_branch)
                || else_branch
                    .as_deref()
                    .is_some_and(expression_contains_finish_control)
        }
        ast::ExprKind::Match { subject, arms } => {
            expression_contains_finish_control(subject)
                || arms
                    .iter()
                    .any(|arm| expression_contains_finish_control(&arm.body))
        }
        ast::ExprKind::Tuple(values) | ast::ExprKind::Sequence(values) => {
            values.iter().any(expression_contains_finish_control)
        }
        ast::ExprKind::Record(fields) => fields
            .iter()
            .any(|field| expression_contains_finish_control(&field.value)),
        ast::ExprKind::Unary { operand, .. }
        | ast::ExprKind::Is { value: operand, .. }
        | ast::ExprKind::Member {
            receiver: operand, ..
        }
        | ast::ExprKind::Lambda { body: operand, .. } => {
            expression_contains_finish_control(operand)
        }
        ast::ExprKind::Binary { left, right, .. }
        | ast::ExprKind::Index {
            receiver: left,
            index: right,
        } => expression_contains_finish_control(left) || expression_contains_finish_control(right),
        ast::ExprKind::Call { callee, arguments } => {
            expression_contains_finish_control(callee)
                || arguments.iter().any(expression_contains_finish_control)
        }
        ast::ExprKind::Update { base, fields } => {
            expression_contains_finish_control(base)
                || fields
                    .iter()
                    .any(|field| expression_contains_finish_control(&field.value))
        }
        ast::ExprKind::Collect(clauses) => clauses.iter().any(|clause| {
            expression_contains_finish_control(&clause.condition)
                || expression_contains_finish_control(&clause.value)
        }),
        ast::ExprKind::SetComprehension {
            source,
            filters,
            value,
            ..
        } => {
            expression_contains_finish_control(source)
                || filters.iter().any(expression_contains_finish_control)
                || expression_contains_finish_control(value)
        }
        ast::ExprKind::Integer(_)
        | ast::ExprKind::Decimal(_)
        | ast::ExprKind::Text(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::Name(_)
        | ast::ExprKind::Error => false,
    }
}

fn reaction_control(expression: &ast::Expr) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match &expression.value {
            ast::ExprKind::Finish(_) | ast::ExprKind::Unreachable => return true,
            ast::ExprKind::Block(block) => {
                for statement in &block.statements {
                    match &statement.value {
                        ast::StatementKind::Set { .. }
                        | ast::StatementKind::Emit(_)
                        | ast::StatementKind::While { .. } => return true,
                        ast::StatementKind::Let { value, .. } | ast::StatementKind::Expr(value) => {
                            pending.push(value)
                        }
                    }
                }
            }
            ast::ExprKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                pending.push(then_branch);
                if let Some(else_branch) = else_branch {
                    pending.push(else_branch);
                }
            }
            ast::ExprKind::Match { arms, .. } => {
                pending.extend(arms.iter().map(|arm| &arm.body));
            }
            ast::ExprKind::Tuple(values) | ast::ExprKind::Sequence(values) => {
                pending.extend(values);
            }
            ast::ExprKind::Record(fields) => {
                pending.extend(fields.iter().map(|field| &field.value));
            }
            ast::ExprKind::Unary { operand, .. }
            | ast::ExprKind::Is { value: operand, .. }
            | ast::ExprKind::Member {
                receiver: operand, ..
            }
            | ast::ExprKind::Lambda { body: operand, .. } => pending.push(operand),
            ast::ExprKind::Binary { left, right, .. }
            | ast::ExprKind::Index {
                receiver: left,
                index: right,
            } => {
                pending.push(left);
                pending.push(right);
            }
            ast::ExprKind::Call { callee, arguments } => {
                pending.push(callee);
                pending.extend(arguments);
            }
            ast::ExprKind::Update { base, fields } => {
                pending.push(base);
                pending.extend(fields.iter().map(|field| &field.value));
            }
            ast::ExprKind::Collect(clauses) => {
                for clause in clauses {
                    pending.push(&clause.condition);
                    pending.push(&clause.value);
                }
            }
            ast::ExprKind::SetComprehension {
                source,
                filters,
                value,
                ..
            } => {
                pending.push(source);
                pending.extend(filters);
                pending.push(value);
            }
            ast::ExprKind::Integer(_)
            | ast::ExprKind::Decimal(_)
            | ast::ExprKind::Text(_)
            | ast::ExprKind::Bool(_)
            | ast::ExprKind::Name(_)
            | ast::ExprKind::Error => {}
        }
    }
    false
}

fn inline_update_loop_exit_local(block: &ast::Block) -> Option<String> {
    fn expression(expr: &ast::Expr, names: &mut BTreeSet<String>) {
        match &expr.value {
            ast::ExprKind::Block(block) => statements(block, names),
            ast::ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                expression(condition, names);
                expression(then_branch, names);
                if let Some(else_branch) = else_branch {
                    expression(else_branch, names);
                }
            }
            ast::ExprKind::Match { subject, arms } => {
                expression(subject, names);
                for arm in arms {
                    expression(&arm.body, names);
                }
            }
            ast::ExprKind::Tuple(values) | ast::ExprKind::Sequence(values) => {
                for value in values {
                    expression(value, names);
                }
            }
            ast::ExprKind::Record(fields) => {
                for field in fields {
                    expression(&field.value, names);
                }
            }
            ast::ExprKind::Unary { operand, .. }
            | ast::ExprKind::Is { value: operand, .. }
            | ast::ExprKind::Member {
                receiver: operand, ..
            }
            | ast::ExprKind::Lambda { body: operand, .. }
            | ast::ExprKind::Finish(operand) => expression(operand, names),
            ast::ExprKind::Binary { left, right, .. }
            | ast::ExprKind::Index {
                receiver: left,
                index: right,
            } => {
                expression(left, names);
                expression(right, names);
            }
            ast::ExprKind::Call { callee, arguments } => {
                expression(callee, names);
                for argument in arguments {
                    expression(argument, names);
                }
            }
            ast::ExprKind::Update { base, fields } => {
                expression(base, names);
                for field in fields {
                    expression(&field.value, names);
                }
            }
            ast::ExprKind::Collect(clauses) => {
                for clause in clauses {
                    expression(&clause.condition, names);
                    expression(&clause.value, names);
                }
            }
            ast::ExprKind::SetComprehension {
                source,
                filters,
                value,
                ..
            } => {
                expression(source, names);
                for filter in filters {
                    expression(filter, names);
                }
                expression(value, names);
            }
            ast::ExprKind::Integer(_)
            | ast::ExprKind::Decimal(_)
            | ast::ExprKind::Text(_)
            | ast::ExprKind::Bool(_)
            | ast::ExprKind::Name(_)
            | ast::ExprKind::Unreachable
            | ast::ExprKind::Error => {}
        }
    }

    fn statements(block: &ast::Block, names: &mut BTreeSet<String>) {
        for statement in &block.statements {
            match &statement.value {
                ast::StatementKind::Let { name, value, .. } => {
                    if name.value.starts_with(INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX)
                        && matches!(
                            &value.value,
                            ast::ExprKind::Call { callee, arguments }
                                if arguments.len() == 1
                                    && matches!(
                                        &callee.value,
                                        ast::ExprKind::Name(callee) if callee.value == "some"
                                    )
                        )
                    {
                        names.insert(name.value.clone());
                    }
                    expression(value, names);
                }
                ast::StatementKind::Set { value, .. }
                | ast::StatementKind::Emit(value)
                | ast::StatementKind::Expr(value) => expression(value, names),
                // A nested loop owns its own exact break local. Its generated
                // post-loop match remains a sibling statement and propagates
                // any lexical return into this loop's local.
                ast::StatementKind::While { .. } => {}
            }
        }
    }

    let mut names = BTreeSet::new();
    statements(block, &mut names);
    debug_assert!(
        names.len() <= 1,
        "one source loop owns one lexical exit local"
    );
    names.into_iter().next()
}

fn guaranteed_update_joins(expression: &ast::Expr) -> BTreeMap<String, ast::TypeExpr> {
    match &expression.value {
        ast::ExprKind::Block(block) => {
            let mut joins = BTreeMap::new();
            for statement in &block.statements {
                match &statement.value {
                    ast::StatementKind::Let {
                        name, ty: Some(ty), ..
                    } if name.value.starts_with(INLINE_UPDATE_JOIN_LOCAL_PREFIX) => {
                        joins.insert(name.value.clone(), ty.clone());
                    }
                    ast::StatementKind::Let { value, .. } | ast::StatementKind::Expr(value) => {
                        joins.extend(guaranteed_update_joins(value));
                    }
                    ast::StatementKind::Set { .. }
                    | ast::StatementKind::Emit(_)
                    | ast::StatementKind::While { .. } => {}
                }
            }
            joins
        }
        ast::ExprKind::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => intersect_update_joins(
            guaranteed_update_joins(then_branch),
            guaranteed_update_joins(else_branch),
        ),
        ast::ExprKind::Match { arms, .. } => {
            let mut arms = arms.iter();
            let Some(first) = arms.next() else {
                return BTreeMap::new();
            };
            arms.fold(guaranteed_update_joins(&first.body), |joins, arm| {
                intersect_update_joins(joins, guaranteed_update_joins(&arm.body))
            })
        }
        _ => BTreeMap::new(),
    }
}

fn intersect_update_joins(
    mut left: BTreeMap<String, ast::TypeExpr>,
    right: BTreeMap<String, ast::TypeExpr>,
) -> BTreeMap<String, ast::TypeExpr> {
    left.retain(|name, ty| right.get(name) == Some(ty));
    left
}

fn source_expr_terminal(expression: &ast::Expr) -> bool {
    match &expression.value {
        ast::ExprKind::Finish(_) | ast::ExprKind::Unreachable => true,
        ast::ExprKind::Block(block) => source_block_terminal(block),
        ast::ExprKind::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => source_expr_terminal(then_branch) && source_expr_terminal(else_branch),
        ast::ExprKind::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| source_expr_terminal(&arm.body))
        }
        _ => false,
    }
}

fn source_block_terminal(block: &ast::Block) -> bool {
    block
        .statements
        .last()
        .is_some_and(|statement| match &statement.value {
            ast::StatementKind::Expr(expression) => source_expr_terminal(expression),
            _ => false,
        })
}

fn const_eval(expression: &IrExpr, program: &Program) -> Result<Value, String> {
    match expression {
        IrExpr::Literal { value } => Ok(value.clone()),
        IrExpr::Name { name } => program
            .machine_program
            .constants
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown constant `{name}`")),
        IrExpr::Constructor {
            type_id,
            constructor,
            fields,
        } => {
            let fields = fields
                .iter()
                .map(|(name, value)| const_eval(value, program).map(|value| (name.clone(), value)))
                .collect::<Result<Vec<_>, _>>()?;
            if type_id == "BoundaryNumber" && constructor == "finite" {
                return match fields.as_slice() {
                    [(_, Value::Decimal(value))] => {
                        Ok(Value::Boundary(BoundaryNumber::Finite(value.clone())))
                    }
                    [(_, _)] => Err("BoundaryNumber.finite needs an exact Decimal".into()),
                    _ => Err("BoundaryNumber.finite needs one argument".into()),
                };
            }
            Ok(Value::variant(type_id, constructor, fields))
        }
        IrExpr::Key { type_id, value } => Ok(Value::Key {
            type_id: type_id.clone(),
            value: Box::new(const_eval(value, program)?),
        }),
        IrExpr::Tuple { values } => Ok(Value::Tuple(
            values
                .iter()
                .map(|value| const_eval(value, program))
                .collect::<Result<_, _>>()?,
        )),
        IrExpr::Record { fields } => Value::record(
            fields
                .iter()
                .map(|(name, value)| const_eval(value, program).map(|value| (name.clone(), value)))
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(|error| error.to_string()),
        IrExpr::Seq { values } => Ok(Value::Seq(
            values
                .iter()
                .map(|value| const_eval(value, program))
                .collect::<Result<_, _>>()?,
        )),
        IrExpr::Map {
            entries,
            result_type,
        } => {
            let value = Value::Map(
                entries
                    .iter()
                    .map(|(key, value)| {
                        Ok((const_eval(key, program)?, const_eval(value, program)?))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            );
            program
                .machine_program
                .canonicalize_value(result_type, &value)
                .map_err(|error| error.to_string())
        }
        IrExpr::Table { key_type, entries } => Ok(Value::Table {
            key_type: key_type.clone(),
            entries: entries
                .iter()
                .map(|(name, value)| const_eval(value, program).map(|value| (name.clone(), value)))
                .collect::<Result<_, _>>()?,
        }),
        IrExpr::SetComprehension {
            source,
            result_type,
            ..
        } if matches!(source.as_ref(), IrExpr::Seq { values } if values.is_empty()) => program
            .machine_program
            .canonicalize_value(result_type, &Value::Set(Vec::new()))
            .map_err(|error| error.to_string()),
        _ => Err("expression uses runtime evaluation".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::collect_semantic_paths;
    use std::collections::BTreeMap;

    fn semantic_paths(value: serde_json::Value) -> BTreeMap<(u32, u32), String> {
        let mut output = BTreeMap::new();
        collect_semantic_paths(&value, String::new(), &mut output);
        output
    }

    #[test]
    fn semantic_path_index_records_nested_object_and_array_paths() {
        let paths = semantic_paths(serde_json::json!({
            "declarations": [{
                "body": {
                    "span": { "file": 7, "start": 10, "end": 20 }
                }
            }]
        }));

        assert_eq!(
            paths.get(&(10, 20)).map(String::as_str),
            Some("declarations[0].body")
        );
    }

    #[test]
    fn semantic_path_index_excludes_embedded_source_identity_subtrees() {
        let paths = semantic_paths(serde_json::json!({
            "declaration": {
                "span": { "file": 7, "start": 1, "end": 2 },
                "source": {
                    "span": { "file": 7, "start": 3, "end": 4 }
                },
                "source_id": {
                    "span": { "file": 7, "start": 5, "end": 6 }
                }
            }
        }));

        assert_eq!(paths.get(&(1, 2)).map(String::as_str), Some("declaration"));
        assert!(!paths.contains_key(&(3, 4)));
        assert!(!paths.contains_key(&(5, 6)));
    }

    #[test]
    fn semantic_path_index_keeps_the_first_depth_first_duplicate_span() {
        let paths = semantic_paths(serde_json::json!({
            "alpha": {
                "span": { "file": 7, "start": 10, "end": 20 }
            },
            "beta": {
                "span": { "file": 9, "start": 10, "end": 20 }
            }
        }));

        assert_eq!(paths.len(), 1);
        assert_eq!(paths.get(&(10, 20)).map(String::as_str), Some("alpha"));
    }
}
