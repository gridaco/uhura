//! Source-layout-sensitive provenance for manifest-resolved Uhura 0.4 source.
//!
//! This builder deliberately consumes the authored 0.4 syntax tree rather
//! than the flattened checker IR. It can therefore preserve exact source
//! occurrences while deriving node identities exclusively from public and
//! composition semantics.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use uhura_core::{Provenance, ProvenanceOccurrence, ProvenanceSource, semantic_node_id};
use uhura_syntax::v04::ast;

use crate::resolve_v04_project_modules;

/// Build a validated `uhura-provenance/0` sidecar for one closed, resolved
/// Uhura 0.4 package.
///
/// The caller is expected to supply the same parsed modules that were accepted
/// by [`crate::check_v04_project_modules`]. Resolution is repeated here only to
/// recover import targets without coupling provenance to the lowering pass.
/// Physical paths, logical modules, file numbers, byte ranges, aliases, and
/// formatting never participate in semantic node IDs.
pub fn build_v04_provenance(
    modules: &[ast::Module],
) -> Result<Provenance, V04ProvenanceBuildError> {
    Builder::new(modules, &[], true)?.build()
}

#[derive(Clone, Debug)]
pub(crate) struct V04ExternalReference {
    pub node: String,
    pub span: ast::Span,
}

pub(crate) fn build_v04_provenance_with_external_references(
    modules: &[ast::Module],
    references: &[V04ExternalReference],
) -> Result<Provenance, V04ProvenanceBuildError> {
    // A closed package graph constructs authored topology once from all
    // original modules and linker bindings. Package-local provenance remains
    // responsible only for its physical sources and semantic occurrences.
    Builder::new(modules, references, false)?.build()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V04ProvenanceBuildError {
    message: String,
}

impl V04ProvenanceBuildError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for V04ProvenanceBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for V04ProvenanceBuildError {}

#[derive(Clone, Copy)]
struct DeclarationRef<'a> {
    declaration: &'a ast::Declaration,
}

struct Builder<'a> {
    modules: &'a [ast::Module],
    external_references: &'a [V04ExternalReference],
    package: String,
    sources: Vec<ProvenanceSource>,
    source_by_file: BTreeMap<u32, u32>,
    source_bytes: BTreeMap<u32, u64>,
    source_text: BTreeMap<u32, &'a str>,
    declarations: BTreeMap<(String, String), DeclarationRef<'a>>,
    bindings: BTreeMap<(String, String), (String, String)>,
    occurrences: Vec<ProvenanceOccurrence>,
    include_topology: bool,
}

impl<'a> Builder<'a> {
    fn new(
        modules: &'a [ast::Module],
        external_references: &'a [V04ExternalReference],
        include_topology: bool,
    ) -> Result<Self, V04ProvenanceBuildError> {
        if modules.is_empty() {
            return Err(V04ProvenanceBuildError::new(
                "Uhura 0.4 provenance requires at least one resolved source module",
            ));
        }

        let resolved = resolve_v04_project_modules(modules);
        if !resolved.diagnostics().is_empty() {
            let rules = resolved
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.rule)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ");
            return Err(V04ProvenanceBuildError::new(format!(
                "cannot build provenance for a project with resolution diagnostics: {rules}"
            )));
        }

        let package = modules[0].identity.package.clone();
        let mut ordered = modules.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.identity
                .path
                .as_bytes()
                .cmp(right.identity.path.as_bytes())
                .then_with(|| {
                    left.identity
                        .module
                        .as_bytes()
                        .cmp(right.identity.module.as_bytes())
                })
                .then_with(|| left.identity.file.cmp(&right.identity.file))
        });

        let mut sources = Vec::with_capacity(ordered.len());
        let mut source_by_file = BTreeMap::new();
        let mut source_bytes = BTreeMap::new();
        let mut source_text = BTreeMap::new();
        let mut paths = BTreeSet::new();
        for (source, module) in ordered.into_iter().enumerate() {
            let source = u32::try_from(source).map_err(|_| {
                V04ProvenanceBuildError::new("provenance source table exceeds u32::MAX entries")
            })?;
            if source_by_file
                .insert(module.identity.file, source)
                .is_some()
            {
                return Err(V04ProvenanceBuildError::new(format!(
                    "resolved source file number {} occurs more than once",
                    module.identity.file
                )));
            }
            if !paths.insert(module.identity.path.as_str()) {
                return Err(V04ProvenanceBuildError::new(format!(
                    "resolved source path `{}` occurs more than once",
                    module.identity.path
                )));
            }
            let bytes = u64::try_from(module.source.len())
                .map_err(|_| V04ProvenanceBuildError::new("source byte length does not fit u64"))?;
            source_bytes.insert(source, bytes);
            source_text.insert(module.identity.file, module.source.as_str());
            sources.push(ProvenanceSource {
                source,
                package: module.identity.package.clone(),
                module: module.identity.module.clone(),
                path: module.identity.path.clone(),
                sha256: uhura_base::sha256_hex(module.source.as_bytes()),
                bytes,
            });
        }

        let mut declarations = BTreeMap::new();
        for module in modules {
            for declaration in &module.declarations {
                let (name, _, _) = declaration_header(declaration);
                declarations.insert(
                    (module.identity.module.clone(), name.text.clone()),
                    DeclarationRef { declaration },
                );
            }
        }

        let bindings = resolved
            .metadata
            .bindings
            .iter()
            .map(|binding| {
                (
                    (binding.module.clone(), binding.local_name.clone()),
                    (binding.target_module.clone(), binding.target_name.clone()),
                )
            })
            .collect();

        Ok(Self {
            modules,
            external_references,
            package,
            sources,
            source_by_file,
            source_bytes,
            source_text,
            declarations,
            bindings,
            occurrences: Vec::new(),
            include_topology,
        })
    }

    fn build(mut self) -> Result<Provenance, V04ProvenanceBuildError> {
        self.record_import_references()?;
        for reference in self.external_references.iter().cloned() {
            self.push_occurrence(reference.node, reference.span, "reference", "root")?;
        }

        let mut ordered = self.modules.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.identity
                .module
                .as_bytes()
                .cmp(right.identity.module.as_bytes())
                .then_with(|| left.identity.file.cmp(&right.identity.file))
        });
        for module in ordered {
            for declaration in &module.declarations {
                self.visit_public_declaration(module, declaration)?;
            }
        }

        let topology = if self.include_topology {
            let topology = crate::v04_topology::build_local(self.modules, &self.bindings)
                .map_err(V04ProvenanceBuildError::new)?;
            for occurrence in topology.occurrences {
                self.push_occurrence(
                    occurrence.node,
                    occurrence.span,
                    occurrence.role,
                    &occurrence.owner,
                )?;
            }
            topology.topology
        } else {
            Default::default()
        };

        Provenance::canonical_with_topology(self.sources, self.occurrences, topology)
            .map_err(V04ProvenanceBuildError::new)
    }

    fn record_import_references(&mut self) -> Result<(), V04ProvenanceBuildError> {
        let resolved = resolve_v04_project_modules(self.modules);
        for binding in &resolved.metadata.bindings {
            let Some(target) = self
                .declarations
                .get(&(binding.target_module.clone(), binding.target_name.clone()))
                .copied()
            else {
                continue;
            };
            let Some(node) = self.public_declaration_node(target.declaration) else {
                continue;
            };
            self.push_occurrence(node, binding.target_span, "reference", "root")?;
        }

        for module in self.modules {
            for declaration in &module.uses {
                if let Some(span) = standard_ui_profile_span(declaration) {
                    let node = semantic_node_id("uhura::ui", "root", "profile", "declaration/ui");
                    self.push_occurrence(node, span, "reference", "root")?;
                }
            }
        }
        Ok(())
    }

    fn visit_public_declaration(
        &mut self,
        module: &ast::Module,
        declaration: &ast::Declaration,
    ) -> Result<(), V04ProvenanceBuildError> {
        let (name, visibility, kind) = declaration_header(declaration);
        if visibility != ast::Visibility::Public {
            return Ok(());
        }
        let public_owner = format!("{}::{}", self.package, name.text);
        self.push_semantic(
            &public_owner,
            "root",
            kind,
            &format!("declaration/{}", name.text),
            name.span,
            "definition",
        )?;

        match &declaration.kind {
            ast::DeclarationKind::Machine(machine) => {
                self.visit_machine_members(
                    module,
                    &public_owner,
                    "root",
                    &machine.members,
                    "definition",
                )?;
                self.visit_composed_parts(module, &public_owner, machine)?;
            }
            ast::DeclarationKind::Part(part) => {
                self.visit_parameters(
                    &public_owner,
                    "root",
                    "part",
                    &part.parameters,
                    "definition",
                )?;
                self.visit_part_members(part, &public_owner, "root", "definition")?;
            }
            ast::DeclarationKind::Ui(ui) => {
                self.visit_ui(module, &public_owner, ui)?;
            }
            ast::DeclarationKind::Struct(value) => {
                for field in &value.fields {
                    self.push_semantic(
                        &public_owner,
                        "root",
                        "field",
                        &format!("field/{}", field.name.text),
                        field.name.span,
                        "definition",
                    )?;
                }
            }
            ast::DeclarationKind::Enum(value) => {
                for variant in &value.variants {
                    self.push_semantic(
                        &public_owner,
                        "root",
                        "variant",
                        &format!("variant/{}", variant.name.text),
                        variant.name.span,
                        "definition",
                    )?;
                    for field in &variant.fields {
                        self.push_semantic(
                            &public_owner,
                            "root",
                            "field",
                            &format!("variant/{}/field/{}", variant.name.text, field.name.text),
                            field.name.span,
                            "definition",
                        )?;
                    }
                }
            }
            ast::DeclarationKind::Function(value) => {
                self.visit_parameters(
                    &public_owner,
                    "root",
                    "function",
                    &value.parameters,
                    "definition",
                )?;
            }
            ast::DeclarationKind::Key(_) | ast::DeclarationKind::Const(_) => {}
        }
        Ok(())
    }

    fn visit_parameters(
        &mut self,
        public_owner: &str,
        owner: &str,
        prefix: &str,
        parameters: &[ast::Parameter],
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        for parameter in parameters {
            self.push_semantic(
                public_owner,
                owner,
                "parameter",
                &format!("{prefix}/parameter/{}", parameter.name.text),
                parameter.name.span,
                role,
            )?;
        }
        Ok(())
    }

    fn visit_machine_members(
        &mut self,
        module: &ast::Module,
        public_owner: &str,
        owner: &str,
        members: &[ast::MachineMember],
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        let mut ordinals = MemberOrdinals::default();
        for member in members {
            match &member.kind {
                ast::MachineMemberKind::Config(value) => {
                    self.visit_config(public_owner, owner, value, member.span, role)?
                }
                ast::MachineMemberKind::Require(value) => {
                    let ordinal = next(&mut ordinals.requires);
                    self.push_semantic(
                        public_owner,
                        owner,
                        "require",
                        &format!("require/{ordinal}"),
                        value.condition.span,
                        role,
                    )?;
                }
                ast::MachineMemberKind::Const(value) => self.push_semantic(
                    public_owner,
                    owner,
                    "const",
                    &format!("const/{}", value.name.text),
                    value.name.span,
                    role,
                )?,
                ast::MachineMemberKind::Function(value) => {
                    self.push_semantic(
                        public_owner,
                        owner,
                        "function",
                        &format!("function/{}", value.name.text),
                        value.name.span,
                        role,
                    )?;
                    self.visit_parameters(
                        public_owner,
                        owner,
                        &format!("function/{}", value.name.text),
                        &value.parameters,
                        role,
                    )?;
                }
                ast::MachineMemberKind::Part(value) => {
                    self.push_semantic(
                        public_owner,
                        owner,
                        "part",
                        &format!("part/{}", value.name.text),
                        value.name.span,
                        role,
                    )?;
                    self.record_type_reference(module, &value.part, owner)?;
                }
                ast::MachineMemberKind::Events(value) => self.visit_protocol(
                    public_owner,
                    owner,
                    "events",
                    "event",
                    value,
                    member.span,
                    role,
                )?,
                ast::MachineMemberKind::Commands(value) => self.visit_protocol(
                    public_owner,
                    owner,
                    "commands",
                    "command",
                    value,
                    member.span,
                    role,
                )?,
                ast::MachineMemberKind::Port(value) => self.push_semantic(
                    public_owner,
                    owner,
                    "port",
                    &format!("port/{}", value.name.text),
                    value.name.span,
                    role,
                )?,
                ast::MachineMemberKind::Outcomes(value) => {
                    self.visit_outcomes(public_owner, owner, "outcomes", value, member.span, role)?
                }
                ast::MachineMemberKind::State(value) => {
                    self.visit_state(public_owner, owner, value, member.span, role)?
                }
                ast::MachineMemberKind::Computed(value) => self.push_semantic(
                    public_owner,
                    owner,
                    "computed",
                    &format!("computed/{}", value.name.text),
                    value.name.span,
                    role,
                )?,
                ast::MachineMemberKind::Invariant(value) => {
                    self.visit_invariant(
                        public_owner,
                        owner,
                        value,
                        next(&mut ordinals.invariants),
                        member.span,
                        role,
                    )?;
                }
                ast::MachineMemberKind::Observe(value) => {
                    self.visit_observe(public_owner, owner, value, member.span, role)?
                }
                ast::MachineMemberKind::Handler(value) => {
                    let selector = selector_path(&value.input);
                    let ordinal = ordinals.handlers.entry(selector.clone()).or_default();
                    let current = *ordinal;
                    *ordinal += 1;
                    self.push_semantic(
                        public_owner,
                        owner,
                        "handler",
                        &format!("handler/{selector}/{current}"),
                        value.input.span,
                        role,
                    )?;
                    self.visit_unreachable_sites(
                        public_owner,
                        owner,
                        &format!("handler/{selector}"),
                        &value.body,
                        role,
                    )?;
                }
                ast::MachineMemberKind::Update(value) => {
                    self.push_semantic(
                        public_owner,
                        owner,
                        "update",
                        &format!("update/{}", value.name.text),
                        value.name.span,
                        role,
                    )?;
                    self.visit_parameters(
                        public_owner,
                        owner,
                        &format!("update/{}", value.name.text),
                        &value.parameters,
                        role,
                    )?;
                    self.visit_unreachable_sites(
                        public_owner,
                        owner,
                        &format!("update/{}", value.name.text),
                        &value.body,
                        role,
                    )?;
                }
                ast::MachineMemberKind::BeforeCommit(value) => {
                    let ordinal = next(&mut ordinals.before_commit);
                    self.push_semantic(
                        public_owner,
                        owner,
                        "before_commit",
                        &format!("before_commit/{ordinal}"),
                        member.span,
                        role,
                    )?;
                    let path = if ordinal == 0 {
                        "before-commit".to_string()
                    } else {
                        format!("before-commit/{ordinal}")
                    };
                    self.visit_unreachable_sites(public_owner, owner, &path, &value.body, role)?;
                }
            }
        }
        Ok(())
    }

    fn visit_part_members(
        &mut self,
        part: &ast::PartDeclaration,
        public_owner: &str,
        owner: &str,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        let mut ordinals = MemberOrdinals::default();
        for member in &part.members {
            match &member.kind {
                ast::PartMemberKind::Require(value) => {
                    let ordinal = next(&mut ordinals.requires);
                    self.push_semantic(
                        public_owner,
                        owner,
                        "require",
                        &format!("require/{ordinal}"),
                        value.condition.span,
                        role,
                    )?;
                }
                ast::PartMemberKind::RequiresOutcomes(value) => self.visit_outcomes(
                    public_owner,
                    owner,
                    "requires_outcomes",
                    value,
                    member.span,
                    role,
                )?,
                ast::PartMemberKind::Const(value) => self.push_semantic(
                    public_owner,
                    owner,
                    "const",
                    &format!("const/{}", value.name.text),
                    value.name.span,
                    role,
                )?,
                ast::PartMemberKind::Function(value) => {
                    self.push_semantic(
                        public_owner,
                        owner,
                        "function",
                        &format!("function/{}", value.name.text),
                        value.name.span,
                        role,
                    )?;
                    self.visit_parameters(
                        public_owner,
                        owner,
                        &format!("function/{}", value.name.text),
                        &value.parameters,
                        role,
                    )?;
                }
                ast::PartMemberKind::Events(value) => self.visit_protocol(
                    public_owner,
                    owner,
                    "events",
                    "event",
                    value,
                    member.span,
                    role,
                )?,
                ast::PartMemberKind::Commands(value) => self.visit_protocol(
                    public_owner,
                    owner,
                    "commands",
                    "command",
                    value,
                    member.span,
                    role,
                )?,
                ast::PartMemberKind::Port(value) => self.push_semantic(
                    public_owner,
                    owner,
                    "port",
                    &format!("port/{}", value.name.text),
                    value.name.span,
                    role,
                )?,
                ast::PartMemberKind::State(value) => {
                    self.visit_state(public_owner, owner, value, member.span, role)?
                }
                ast::PartMemberKind::Computed(value) => self.push_semantic(
                    public_owner,
                    owner,
                    "computed",
                    &format!("computed/{}", value.name.text),
                    value.name.span,
                    role,
                )?,
                ast::PartMemberKind::Invariant(value) => self.visit_invariant(
                    public_owner,
                    owner,
                    value,
                    next(&mut ordinals.invariants),
                    member.span,
                    role,
                )?,
                ast::PartMemberKind::Observe(value) => {
                    self.visit_observe(public_owner, owner, value, member.span, role)?
                }
                ast::PartMemberKind::Handler(value) => {
                    let selector = selector_path(&value.input);
                    let ordinal = ordinals.handlers.entry(selector.clone()).or_default();
                    let current = *ordinal;
                    *ordinal += 1;
                    self.push_semantic(
                        public_owner,
                        owner,
                        "handler",
                        &format!("handler/{selector}/{current}"),
                        value.input.span,
                        role,
                    )?;
                    self.visit_unreachable_sites(
                        public_owner,
                        owner,
                        &format!("handler/{selector}"),
                        &value.body,
                        role,
                    )?;
                }
                ast::PartMemberKind::Update(value) => {
                    self.push_semantic(
                        public_owner,
                        owner,
                        "update",
                        &format!("update/{}", value.name.text),
                        value.name.span,
                        role,
                    )?;
                    self.visit_parameters(
                        public_owner,
                        owner,
                        &format!("update/{}", value.name.text),
                        &value.parameters,
                        role,
                    )?;
                    self.visit_unreachable_sites(
                        public_owner,
                        owner,
                        &format!("update/{}", value.name.text),
                        &value.body,
                        role,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn visit_composed_parts(
        &mut self,
        module: &ast::Module,
        public_owner: &str,
        machine: &ast::MachineDeclaration,
    ) -> Result<(), V04ProvenanceBuildError> {
        let parts = machine
            .members
            .iter()
            .filter_map(|member| match &member.kind {
                ast::MachineMemberKind::Part(instance) => Some(instance),
                _ => None,
            })
            .collect::<Vec<_>>();
        for instance in parts {
            let Some(local_name) = singular_type_name(&instance.part) else {
                continue;
            };
            let Some(target) = self.resolve_declaration(&module.identity.module, local_name) else {
                continue;
            };
            let ast::DeclarationKind::Part(part) = &target.declaration.kind else {
                continue;
            };
            self.visit_part_members(part, public_owner, &instance.name.text, "generated")?;
        }
        Ok(())
    }

    fn visit_config(
        &mut self,
        public_owner: &str,
        owner: &str,
        value: &ast::ConfigSection,
        span: ast::Span,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        self.push_semantic(public_owner, owner, "config_section", "config", span, role)?;
        for field in &value.fields {
            self.push_semantic(
                public_owner,
                owner,
                "config",
                &format!("config/{}", field.name.text),
                field.name.span,
                role,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_protocol(
        &mut self,
        public_owner: &str,
        owner: &str,
        section: &str,
        item_kind: &str,
        value: &ast::ProtocolSection,
        span: ast::Span,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        self.push_semantic(
            public_owner,
            owner,
            &format!("{section}_section"),
            section,
            span,
            role,
        )?;
        for variant in &value.variants {
            self.push_semantic(
                public_owner,
                owner,
                item_kind,
                &format!("{section}/{}", variant.name.text),
                variant.name.span,
                role,
            )?;
            self.visit_parameters(
                public_owner,
                owner,
                &format!("{section}/{}", variant.name.text),
                &variant.parameters,
                role,
            )?;
        }
        Ok(())
    }

    fn visit_outcomes(
        &mut self,
        public_owner: &str,
        owner: &str,
        section: &str,
        value: &ast::OutcomeSection,
        span: ast::Span,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        self.push_semantic(
            public_owner,
            owner,
            &format!("{section}_section"),
            section,
            span,
            role,
        )?;
        for entry in &value.entries {
            let policy = match entry.policy {
                ast::OutcomePolicy::Commit => "commit",
                ast::OutcomePolicy::Abort => "abort",
            };
            self.push_semantic(
                public_owner,
                owner,
                "outcome",
                &format!("{section}/{policy}/{}", entry.variant.name.text),
                entry.variant.name.span,
                role,
            )?;
            self.visit_parameters(
                public_owner,
                owner,
                &format!("{section}/{policy}/{}", entry.variant.name.text),
                &entry.variant.parameters,
                role,
            )?;
        }
        Ok(())
    }

    fn visit_state(
        &mut self,
        public_owner: &str,
        owner: &str,
        value: &ast::StateSection,
        span: ast::Span,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        self.push_semantic(public_owner, owner, "state_section", "state", span, role)?;
        for field in &value.fields {
            self.push_semantic(
                public_owner,
                owner,
                "state",
                &format!("state/{}", field.name.text),
                field.name.span,
                role,
            )?;
        }
        Ok(())
    }

    fn visit_observe(
        &mut self,
        public_owner: &str,
        owner: &str,
        value: &ast::ObserveSection,
        span: ast::Span,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        self.push_semantic(
            public_owner,
            owner,
            "observe_section",
            "observe",
            span,
            role,
        )?;
        for field in &value.fields {
            self.push_semantic(
                public_owner,
                owner,
                "observation",
                &format!("observe/{}", field.name.text),
                field.name.span,
                role,
            )?;
        }
        Ok(())
    }

    fn visit_invariant(
        &mut self,
        public_owner: &str,
        owner: &str,
        value: &ast::InvariantDeclaration,
        ordinal: usize,
        span: ast::Span,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        self.push_semantic(
            public_owner,
            owner,
            "invariant",
            &format!("invariant/{ordinal}"),
            span,
            role,
        )?;
        for (condition, expression) in value.conditions.iter().enumerate() {
            self.push_semantic(
                public_owner,
                owner,
                "invariant_condition",
                &format!("invariant/{ordinal}/condition/{condition}"),
                expression.span,
                role,
            )?;
        }
        Ok(())
    }

    fn visit_unreachable_sites(
        &mut self,
        public_owner: &str,
        owner: &str,
        parent: &str,
        body: &ast::Block,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        for (path, span) in crate::v04_parts::authored_unreachable_sites(body, parent) {
            self.push_semantic(public_owner, owner, "unreachable", &path, span, role)?;
        }
        Ok(())
    }

    fn visit_ui(
        &mut self,
        module: &ast::Module,
        public_owner: &str,
        ui: &ast::UiDeclaration,
    ) -> Result<(), V04ProvenanceBuildError> {
        let machine_owner = singular_type_name(&ui.machine)
            .and_then(|name| self.resolve_declaration(&module.identity.module, name))
            .filter(|target| matches!(target.declaration.kind, ast::DeclarationKind::Machine(_)))
            .and_then(|target| self.public_owner(target.declaration));

        if let Some(machine_owner) = &machine_owner {
            let node = self.public_declaration_node_for_owner(
                machine_owner,
                "machine",
                &machine_name(machine_owner),
            );
            self.push_occurrence(node, ui.machine.span, "reference", "root")?;
        }
        self.push_semantic(
            public_owner,
            "root",
            "ui_binding",
            &format!("binding/{}", ui.observation.text),
            ui.observation.span,
            "definition",
        )?;
        self.visit_ui_nodes(
            public_owner,
            machine_owner.as_deref(),
            "tree",
            &ui.body.nodes,
        )
    }

    fn visit_ui_nodes(
        &mut self,
        public_owner: &str,
        machine_owner: Option<&str>,
        prefix: &str,
        nodes: &[ast::UiNode],
    ) -> Result<(), V04ProvenanceBuildError> {
        let mut ordinal = 0usize;
        for node in nodes {
            if ui_node_is_source_only(node) {
                continue;
            }
            let path = format!("{prefix}/{ordinal}");
            ordinal += 1;
            match &node.kind {
                ast::UiNodeKind::Text(_) => self.push_semantic(
                    public_owner,
                    "root",
                    "ui_text",
                    &format!("{path}/text"),
                    node.span,
                    "definition",
                )?,
                ast::UiNodeKind::Comment(_) => {}
                ast::UiNodeKind::Interpolation(_) => self.push_semantic(
                    public_owner,
                    "root",
                    "ui_interpolation",
                    &format!("{path}/interpolation"),
                    node.span,
                    "definition",
                )?,
                ast::UiNodeKind::Element(value) => {
                    self.push_semantic(
                        public_owner,
                        "root",
                        "ui_element",
                        &format!("{path}/element/{}", value.name.text),
                        node.span,
                        "definition",
                    )?;
                    let mut attribute_ordinals = BTreeMap::<(String, String), usize>::new();
                    for attribute in &value.attributes {
                        match attribute {
                            ast::UiAttribute::Boolean { name, span }
                            | ast::UiAttribute::StaticText { name, span, .. }
                            | ast::UiAttribute::Expression { name, span, .. } => {
                                let key = ("attribute".into(), name.text.clone());
                                let duplicate = attribute_ordinals.entry(key).or_default();
                                let current = *duplicate;
                                *duplicate += 1;
                                self.push_semantic(
                                    public_owner,
                                    "root",
                                    "ui_attribute",
                                    &format!("{path}/attribute/{}/{current}", name.text),
                                    *span,
                                    "definition",
                                )?;
                            }
                            ast::UiAttribute::Event { event, input, span } => {
                                let key = ("event".into(), event.text.clone());
                                let duplicate = attribute_ordinals.entry(key).or_default();
                                let current = *duplicate;
                                *duplicate += 1;
                                self.push_semantic(
                                    public_owner,
                                    "root",
                                    "ui_event_binding",
                                    &format!("{path}/event/{}/{current}", event.text),
                                    *span,
                                    "definition",
                                )?;
                                if let (Some(machine_owner), Some((owner, variant, target_span))) =
                                    (machine_owner, ui_input_selector(input))
                                {
                                    let node = semantic_node_id(
                                        machine_owner,
                                        &owner,
                                        "event",
                                        &format!("events/{variant}"),
                                    );
                                    self.push_occurrence(node, target_span, "reference", &owner)?;
                                }
                            }
                        }
                    }
                    self.visit_ui_nodes(
                        public_owner,
                        machine_owner,
                        &format!("{path}/children"),
                        &value.children,
                    )?;
                }
                ast::UiNodeKind::If(value) => {
                    let first_span = value
                        .else_span
                        .map_or(node.span, |else_span| value.open_span.through(else_span));
                    self.push_semantic(
                        public_owner,
                        "root",
                        "ui_if",
                        &format!("{path}/if"),
                        first_span,
                        "definition",
                    )?;
                    self.push_semantic(
                        public_owner,
                        "root",
                        "ui_condition",
                        &format!("{path}/condition"),
                        value.condition.span,
                        "definition",
                    )?;
                    self.visit_ui_nodes(
                        public_owner,
                        machine_owner,
                        &format!("{path}/then"),
                        &value.then_branch,
                    )?;
                    if let Some(branch) = &value.else_branch {
                        let else_path = format!("{prefix}/{ordinal}");
                        ordinal += 1;
                        let else_span = value
                            .else_span
                            .map_or(node.span, |span| span.through(value.close_span));
                        self.push_semantic(
                            public_owner,
                            "root",
                            "ui_if",
                            &format!("{else_path}/if"),
                            else_span,
                            "definition",
                        )?;
                        self.visit_ui_nodes(
                            public_owner,
                            machine_owner,
                            &format!("{else_path}/then"),
                            branch,
                        )?;
                    }
                }
                ast::UiNodeKind::Each(value) => {
                    self.push_semantic(
                        public_owner,
                        "root",
                        "ui_each",
                        &format!("{path}/each"),
                        node.span,
                        "definition",
                    )?;
                    self.push_semantic(
                        public_owner,
                        "root",
                        "ui_collection",
                        &format!("{path}/source"),
                        value.source.span,
                        "definition",
                    )?;
                    self.push_semantic(
                        public_owner,
                        "root",
                        "ui_binding",
                        &format!("{path}/pattern"),
                        value.pattern.span,
                        "definition",
                    )?;
                    self.push_semantic(
                        public_owner,
                        "root",
                        "ui_key",
                        &format!("{path}/key"),
                        value.key.span,
                        "definition",
                    )?;
                    self.visit_ui_nodes(
                        public_owner,
                        machine_owner,
                        &format!("{path}/children"),
                        &value.children,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn record_type_reference(
        &mut self,
        module: &ast::Module,
        path: &ast::TypePath,
        owner: &str,
    ) -> Result<(), V04ProvenanceBuildError> {
        let Some(local_name) = singular_type_name(path) else {
            return Ok(());
        };
        let Some(target) = self.resolve_declaration(&module.identity.module, local_name) else {
            return Ok(());
        };
        let Some(node) = self.public_declaration_node(target.declaration) else {
            return Ok(());
        };
        self.push_occurrence(node, path.span, "reference", owner)
    }

    fn resolve_declaration(&self, module: &str, local_name: &str) -> Option<DeclarationRef<'a>> {
        let target = self
            .bindings
            .get(&(module.to_owned(), local_name.to_owned()))
            .cloned()
            .unwrap_or_else(|| (module.to_owned(), local_name.to_owned()));
        self.declarations.get(&target).copied()
    }

    fn public_owner(&self, declaration: &ast::Declaration) -> Option<String> {
        let (name, visibility, _) = declaration_header(declaration);
        (visibility == ast::Visibility::Public).then(|| format!("{}::{}", self.package, name.text))
    }

    fn public_declaration_node(&self, declaration: &ast::Declaration) -> Option<String> {
        let (name, visibility, kind) = declaration_header(declaration);
        (visibility == ast::Visibility::Public).then(|| {
            self.public_declaration_node_for_owner(
                &format!("{}::{}", self.package, name.text),
                kind,
                &name.text,
            )
        })
    }

    fn public_declaration_node_for_owner(
        &self,
        public_owner: &str,
        kind: &str,
        name: &str,
    ) -> String {
        semantic_node_id(public_owner, "root", kind, &format!("declaration/{name}"))
    }

    #[allow(clippy::too_many_arguments)]
    fn push_semantic(
        &mut self,
        public_owner: &str,
        owner: &str,
        kind: &str,
        semantic_path: &str,
        span: ast::Span,
        role: &'static str,
    ) -> Result<(), V04ProvenanceBuildError> {
        self.push_occurrence(
            semantic_node_id(public_owner, owner, kind, semantic_path),
            span,
            role,
            owner,
        )
    }

    fn push_occurrence(
        &mut self,
        node: String,
        span: ast::Span,
        role: &'static str,
        owner: &str,
    ) -> Result<(), V04ProvenanceBuildError> {
        let Some(source) = self.source_by_file.get(&span.file).copied() else {
            return Err(V04ProvenanceBuildError::new(format!(
                "provenance span {}..{} references unknown source file {}",
                span.start, span.end, span.file
            )));
        };
        let bytes = self
            .source_bytes
            .get(&source)
            .copied()
            .expect("source index and byte table are constructed together");
        if span.start > span.end || u64::from(span.end) > bytes {
            return Err(V04ProvenanceBuildError::new(format!(
                "provenance span {}..{} exceeds the {} bytes of source file {}",
                span.start, span.end, bytes, span.file
            )));
        }
        let source_text = self
            .source_text
            .get(&span.file)
            .copied()
            .expect("source file and source text tables are constructed together");
        let start = usize::try_from(span.start).expect("u32 always fits usize on supported hosts");
        let end = usize::try_from(span.end).expect("u32 always fits usize on supported hosts");
        if !source_text.is_char_boundary(start) || !source_text.is_char_boundary(end) {
            return Err(V04ProvenanceBuildError::new(format!(
                "provenance span {}..{} splits a UTF-8 code point in source file {}",
                span.start, span.end, span.file
            )));
        }
        self.occurrences.push(ProvenanceOccurrence {
            node,
            source,
            start: span.start,
            end: span.end,
            role: role.into(),
            owner: owner.into(),
        });
        Ok(())
    }
}

#[derive(Default)]
struct MemberOrdinals {
    requires: usize,
    invariants: usize,
    before_commit: usize,
    handlers: BTreeMap<String, usize>,
}

fn next(value: &mut usize) -> usize {
    let current = *value;
    *value += 1;
    current
}

fn declaration_header(
    declaration: &ast::Declaration,
) -> (&ast::Identifier, ast::Visibility, &'static str) {
    match &declaration.kind {
        ast::DeclarationKind::Machine(value) => (&value.name, value.visibility, "machine"),
        ast::DeclarationKind::Part(value) => (&value.name, value.visibility, "part"),
        ast::DeclarationKind::Ui(value) => (&value.name, value.visibility, "ui"),
        ast::DeclarationKind::Struct(value) => (&value.name, value.visibility, "struct"),
        ast::DeclarationKind::Enum(value) => (&value.name, value.visibility, "enum"),
        ast::DeclarationKind::Key(value) => (&value.name, value.visibility, "key"),
        ast::DeclarationKind::Const(value) => (&value.name, value.visibility, "const"),
        ast::DeclarationKind::Function(value) => (&value.name, value.visibility, "function"),
    }
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

fn standard_ui_profile_span(declaration: &ast::UseDeclaration) -> Option<ast::Span> {
    if declaration.visibility != ast::Visibility::Private {
        return None;
    }
    let ast::ImportTree::Single { path, alias: None } = &declaration.tree else {
        return None;
    };
    let ast::ImportRoot::Package(package) = &path.root else {
        return None;
    };
    (package.text == "uhura" && path.segments.len() == 1 && path.segments[0].text == "ui")
        .then_some(path.segments[0].span)
}

fn ui_node_is_source_only(node: &ast::UiNode) -> bool {
    match &node.kind {
        ast::UiNodeKind::Comment(_) => true,
        ast::UiNodeKind::Text(value) => value.raw.chars().all(char::is_whitespace),
        _ => false,
    }
}

fn ui_input_selector(expression: &ast::Expression) -> Option<(String, String, ast::Span)> {
    let expression = match &expression.kind {
        ast::ExpressionKind::Call { callee, .. } | ast::ExpressionKind::Group(callee) => callee,
        _ => expression,
    };
    let ast::ExpressionKind::Name(name) = &expression.kind else {
        return None;
    };
    match name.segments.as_slice() {
        [variant] => Some(("root".into(), variant.text.clone(), name.span)),
        [owner, variant] => Some((owner.text.clone(), variant.text.clone(), name.span)),
        _ => None,
    }
}

fn machine_name(public_owner: &str) -> String {
    public_owner
        .rsplit_once("::")
        .map_or(public_owner, |(_, name)| name)
        .to_owned()
}
