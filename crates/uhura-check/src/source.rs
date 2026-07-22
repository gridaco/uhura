//! Uhura's semantic bridge into the shared deterministic kernel.
//!
//! Core, UI, and tooling evidence are parsed by the same headerless frontend.
//! This module resolves those authored trees and structurally adapts them into
//! the checker-neutral deterministic-kernel tree without printing or reparsing
//! source text.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Diagnostic, Label, has_errors};
use uhura_core::{MACHINE_PROGRAM_ID_PROTOCOL, PURE_CONTINUATION_LOCAL_PREFIX, Provenance};

use crate::checker::{
    CheckOutput, ImportAliases, PhysicalSourcePaths, check_project_with_import_aliases,
};
use crate::checker_ir as ast;
use crate::diagnostic::{codes, error};

mod references;
mod ui;

use references::{declaration_references, declaration_root_references};

/// Check and lower one manifest-resolved Uhura 0.4 module.
///
/// This remains the small embedding API. Project loaders should call
/// [`check_project_modules`] so all manifest-resolved modules participate in
/// one closed resolution pass.
pub fn check_module(module: &uhura_syntax::ast::Module) -> CheckOutput {
    check_project_modules(std::slice::from_ref(module))
}

/// Reusable result of closed current-package source resolution.
///
/// The authored modules remain private implementation input. The public
/// metadata is sufficient for a later `uhura-provenance/0` builder to create
/// its source table and definition/reference occurrences without recovering
/// logical ownership from flattened IR.
#[derive(Clone, Debug)]
pub struct ResolvedProject {
    pub metadata: ResolutionMetadata,
    modules: Vec<uhura_syntax::ast::Module>,
    resolution: Resolution,
}

impl ResolvedProject {
    pub fn diagnostics(&self) -> &[uhura_base::Diagnostic] {
        &self.resolution.diagnostics
    }

    pub(crate) fn lowered_declaration_name(
        &self,
        module: &str,
        authored_name: &str,
    ) -> Option<&str> {
        self.resolution
            .modules
            .get(module)?
            .bindings
            .get(authored_name)
            .map(String::as_str)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolutionMetadata {
    pub package: Option<String>,
    pub sources: Vec<ResolvedSource>,
    pub declarations: Vec<ResolvedDeclaration>,
    pub bindings: Vec<ResolvedBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSource {
    pub file: u32,
    pub package: String,
    pub module: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDeclaration {
    pub package: String,
    pub module: String,
    pub name: String,
    pub public_id: Option<String>,
    pub span: uhura_syntax::ast::Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBinding {
    pub module: String,
    pub local_name: String,
    pub target_module: String,
    pub target_name: String,
    pub reexport: bool,
    pub span: uhura_syntax::ast::Span,
    pub target_span: uhura_syntax::ast::Span,
}

/// One exact package captured from a checked 0.4 project lock.
///
/// `dependencies` maps this package's authored aliases to exact package IDs.
/// Every module must already carry this package's exact ID in its
/// [`uhura_syntax::ast::SourceIdentity`].
#[derive(Clone, Debug)]
pub struct CapturedPackageModules {
    pub package: String,
    pub dependencies: BTreeMap<String, String>,
    pub modules: Vec<uhura_syntax::ast::Module>,
}

/// Resolve all manifest-captured source without lowering or executing it.
pub fn resolve_project_modules(modules: &[uhura_syntax::ast::Module]) -> ResolvedProject {
    let resolution = Resolution::build(modules, None);
    ResolvedProject {
        metadata: resolution.metadata.clone(),
        modules: modules.to_vec(),
        resolution,
    }
}

/// Resolve, check, and lower every manifest-resolved module in one Uhura 0.4
/// package through the existing deterministic machine kernel.
///
/// Resolution is two phase and source-order independent. `use` declarations
/// become module-local bindings only; the adapter then flattens the resolved
/// package into the checker-neutral global IR. Logical module paths, physical
/// paths, aliases, re-export routes, and source ordering therefore never
/// become declaration or machine identity.
pub fn check_project_modules(modules: &[uhura_syntax::ast::Module]) -> CheckOutput {
    let resolved = resolve_project_modules(modules);
    check_resolved_project(&resolved)
}

/// Resolve and check core modules together with manifest-resolved,
/// tooling-only evidence modules.
pub fn check_project_modules_with_evidence(
    modules: &[uhura_syntax::ast::Module],
    evidence_modules: &[uhura_syntax::ast::Module],
) -> CheckOutput {
    let mut all_modules = modules.to_vec();
    all_modules.extend_from_slice(evidence_modules);
    let resolution = Resolution::build(&all_modules, None);
    let resolved = ResolvedProject {
        metadata: resolution.metadata.clone(),
        modules: modules.to_vec(),
        resolution,
    };
    check_resolved_project_with_evidence(&resolved, evidence_modules)
}

/// Lower one previously resolved project without repeating source resolution.
pub fn check_resolved_project(project: &ResolvedProject) -> CheckOutput {
    check_resolved_project_with_evidence(project, &[])
}

/// Lower one resolved 0.4 project and its admitted tooling-only evidence.
pub fn check_resolved_project_with_evidence(
    project: &ResolvedProject,
    evidence_modules: &[uhura_syntax::ast::Module],
) -> CheckOutput {
    let bindings = project
        .resolution
        .modules
        .iter()
        .map(|(module, resolution)| (module.clone(), resolution.bindings.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut composition_diagnostics = Vec::new();
    let composition = crate::parts::compose_project(
        &project.modules,
        &bindings,
        &project.resolution.standard_imports,
        &mut composition_diagnostics,
    );
    let mut modules = composition.modules.clone();
    crate::updates::lower_project(&mut modules, &mut composition_diagnostics);
    let mut adapter = ProjectAdapter::new(&modules, project.resolution.clone());
    adapter.diagnostics.extend(composition_diagnostics);
    let mut adapted = adapter.adapt();
    adapter.diagnostics.extend(crate::evidence::validate(
        &project.modules,
        evidence_modules,
    ));
    let mut shape_sources = project.modules.clone();
    shape_sources.extend_from_slice(evidence_modules);
    let mut evidence_adapter = ProjectAdapter::new_with_shapes(
        evidence_modules,
        &shape_sources,
        project.resolution.clone(),
    );
    let lowered_evidence = evidence_adapter.adapt();
    adapter.diagnostics.extend(evidence_adapter.diagnostics);
    if let (Some(core), Some(evidence)) = (
        adapted.modules.first_mut(),
        lowered_evidence.modules.into_iter().next(),
    ) {
        activate_evidence(core, evidence.span);
        core.declarations.extend(evidence.declarations);
    }
    let physical_source_paths = physical_source_paths(&project.modules, evidence_modules);

    let mut output =
        check_project_with_import_aliases(&adapted, &ImportAliases::new(), &physical_source_paths);
    output.diagnostics.extend(adapter.diagnostics);
    output.diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
            diagnostic.rule,
        )
    });
    output.diagnostics.dedup_by(|left, right| {
        left.code == right.code
            && left.rule == right.rule
            && left.span == right.span
            && left.message == right.message
    });
    suppress_contained_type_mismatch_cascades(&mut output.diagnostics);
    output.diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
            diagnostic.rule,
        )
    });

    if has_errors(&output.diagnostics) {
        output.program = None;
    } else {
        match crate::provenance::build_source_artifacts(&project.modules) {
            Ok(artifacts) => {
                output.provenance = Some(artifacts.provenance);
                output.authoring = artifacts.authoring;
            }
            Err(failure) => {
                output.diagnostics.push(error(
                    codes::UNSUPPORTED,
                    "uhura-0.4/provenance",
                    format!("could not build checked 0.4 provenance: {failure}"),
                    project
                        .modules
                        .first()
                        .map_or(ast::SourceSpan::empty(0, 0), |module| span(module.span)),
                ));
                output.program = None;
            }
        }
        if let Some(program) = &mut output.program {
            program.machine_program.language = "uhura 0.4".into();
            program.machine_program.identity_protocol = MACHINE_PROGRAM_ID_PROTOCOL.into();
            program.machine_program.composed_part_declarations =
                composition.machine_part_dependencies.clone();
            if let Err(failure) = composition.apply_site_ids(program) {
                output.diagnostics.push(error(
                    codes::UNSUPPORTED,
                    "uhura-0.4/site-identity",
                    format!("could not assign checked 0.4 fault-site identities: {failure}"),
                    project
                        .modules
                        .first()
                        .map_or(ast::SourceSpan::empty(0, 0), |module| span(module.span)),
                ));
                output.program = None;
            } else {
                program.freeze_program_hashes();
            }
        }
    }
    output
}

fn activate_evidence(module: &mut ast::Module, span: ast::SourceSpan) {
    if module
        .uses
        .iter()
        .any(|declaration| declaration.feature.value == "evidence")
    {
        return;
    }
    module.uses.push(ast::UseDecl {
        feature: ast::Spanned::new("evidence".into(), span),
        span,
    });
}

/// Bidirectional checking can discover the same mismatch once at the exact
/// value and once again at its enclosing assignment/call boundary. Preserve
/// the narrower authored location and retain the enclosing site as context
/// instead of reporting two indistinguishable errors.
fn suppress_contained_type_mismatch_cascades(diagnostics: &mut Vec<Diagnostic>) {
    let mut compact = Vec::<Diagnostic>::with_capacity(diagnostics.len());
    for mut diagnostic in std::mem::take(diagnostics) {
        let duplicate = compact.iter().position(|existing| {
            existing.code == diagnostic.code
                && existing.rule == "uhura/type-mismatch"
                && existing.rule == diagnostic.rule
                && existing.severity == diagnostic.severity
                && existing.message == diagnostic.message
                && existing.span.file == diagnostic.span.file
                && (span_contains(existing.span, diagnostic.span)
                    || span_contains(diagnostic.span, existing.span))
        });
        let Some(index) = duplicate else {
            compact.push(diagnostic);
            continue;
        };

        let existing = &mut compact[index];
        if span_contains(existing.span, diagnostic.span) && existing.span != diagnostic.span {
            let enclosing = existing.span;
            diagnostic.labels.append(&mut existing.labels);
            add_enclosing_type_label(&mut diagnostic, enclosing);
            *existing = diagnostic;
        } else if span_contains(diagnostic.span, existing.span) && existing.span != diagnostic.span
        {
            add_enclosing_type_label(existing, diagnostic.span);
        }
    }
    *diagnostics = compact;
}

fn span_contains(outer: uhura_base::Span, inner: uhura_base::Span) -> bool {
    outer.file == inner.file && outer.start <= inner.start && outer.end >= inner.end
}

fn add_enclosing_type_label(diagnostic: &mut Diagnostic, span: uhura_base::Span) {
    if diagnostic.labels.iter().any(|label| label.span == span) {
        return;
    }
    diagnostic.labels.push(Label {
        span,
        message: "the enclosing authored construct requires this type".into(),
    });
}

/// Check one exact, lock-resolved package graph.
///
/// The shared kernel receives one checker module per semantic package. Source
/// aliases, logical modules, physical paths, and acquisition layout are
/// erased before that boundary; exact package IDs and referenced public names
/// remain semantic.
pub fn check_package_graph_with_evidence(
    root_package: &str,
    packages: &[CapturedPackageModules],
    evidence_modules: &[uhura_syntax::ast::Module],
) -> CheckOutput {
    let mut diagnostics = Vec::new();
    let mut by_package = BTreeMap::<String, &CapturedPackageModules>::new();
    for package in packages {
        if by_package
            .insert(package.package.clone(), package)
            .is_some()
        {
            diagnostics.push(error(
                codes::DUPLICATE,
                "uhura-0.4/duplicate-package-capture",
                format!("package `{}` is captured more than once", package.package),
                package
                    .modules
                    .first()
                    .map_or(ast::SourceSpan::empty(0, 0), |module| span(module.span)),
            ));
        }
        for module in &package.modules {
            if module.identity.package != package.package {
                diagnostics.push(error(
                    codes::MODULE,
                    "uhura-0.4/package-capture-mismatch",
                    format!(
                        "captured package `{}` contains module `{}` with package identity `{}`",
                        package.package, module.identity.module, module.identity.package
                    ),
                    span(module.span),
                ));
            }
        }
    }
    if !by_package.contains_key(root_package) {
        diagnostics.push(error(
            codes::MODULE,
            "uhura-0.4/missing-root-package",
            format!("resolved graph does not contain root package `{root_package}`"),
            ast::SourceSpan::empty(0, 0),
        ));
    }

    let catalog = build_external_catalog(packages, &mut diagnostics);

    let mut lowered = ast::Project {
        modules: Vec::new(),
    };
    let mut import_aliases = ImportAliases::new();
    let mut external_references =
        BTreeMap::<String, Vec<crate::provenance::ExternalReference>>::new();
    let mut topology_bindings = Vec::new();
    let mut all_modules = Vec::new();
    let mut root_resolution = None;
    let mut root_core_modules = Vec::new();
    let mut ordered = packages.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.package.as_bytes().cmp(right.package.as_bytes()));
    let mut resolved_packages = Vec::with_capacity(ordered.len());
    let mut part_catalog = crate::parts::PartCatalog::default();
    for package in ordered {
        let context = ExternalResolutionContext {
            dependencies: &package.dependencies,
            catalog: &catalog,
        };
        let mut resolution_sources = package.modules.clone();
        if package.package == root_package {
            resolution_sources.extend_from_slice(evidence_modules);
            root_core_modules = package.modules.clone();
        }
        let resolution = Resolution::build(&resolution_sources, Some(&context));
        for ((target, name), imported) in &resolution.external_imports {
            import_aliases.insert(
                (package.package.clone(), target.clone(), name.clone()),
                imported.local_name.clone(),
            );
        }
        external_references.insert(
            package.package.clone(),
            resolution.external_references.clone(),
        );
        topology_bindings.extend(resolution.topology_bindings.iter().cloned());
        if package.package == root_package {
            root_resolution = Some(resolution.clone());
        }
        let bindings = resolution
            .modules
            .iter()
            .map(|(module, resolution)| (module.clone(), resolution.bindings.clone()))
            .collect::<BTreeMap<_, _>>();
        part_catalog.capture_package(
            &package.package,
            &package.modules,
            &bindings,
            &resolution.standard_imports,
            &mut diagnostics,
        );
        resolved_packages.push((package, resolution, bindings));
    }

    let mut composed_part_declarations = BTreeMap::new();
    let mut site_compositions = Vec::new();
    for (package, resolution, bindings) in resolved_packages {
        let mut composition_diagnostics = Vec::new();
        let composition = crate::parts::compose_package(
            &package.package,
            &package.modules,
            &bindings,
            &part_catalog,
            &mut composition_diagnostics,
        );
        composed_part_declarations.extend(composition.machine_part_dependencies.clone());
        let linked_public_declarations = composition.linked_public_declarations.clone();
        let standard_imports = composition.standard_imports.clone();
        let helper_bindings = composition.helper_bindings.clone();
        let mut modules = composition.modules.clone();
        crate::updates::lower_project(&mut modules, &mut composition_diagnostics);
        let mut resolution = resolution;
        resolution.standard_imports.extend(standard_imports);
        for (module, bindings) in helper_bindings {
            resolution.modules.insert(
                module,
                ModuleResolution {
                    visible_names: bindings.keys().cloned().collect(),
                    bindings,
                },
            );
        }
        link_composed_package_source(
            &package.package,
            &mut resolution,
            &catalog,
            &linked_public_declarations,
            &mut import_aliases,
        );
        let mut adapter = ProjectAdapter::new(&modules, resolution);
        adapter.diagnostics.extend(composition_diagnostics);
        let adapted = adapter.adapt();
        lowered.modules.extend(adapted.modules);
        diagnostics.extend(adapter.diagnostics);
        all_modules.extend(package.modules.iter().cloned());
        site_compositions.push(composition);
    }

    diagnostics.extend(crate::evidence::validate(
        &root_core_modules,
        evidence_modules,
    ));
    if !evidence_modules.is_empty() {
        let mut shape_sources = root_core_modules.clone();
        shape_sources.extend_from_slice(evidence_modules);
        let mut evidence_adapter = ProjectAdapter::new_with_shapes(
            evidence_modules,
            &shape_sources,
            root_resolution.unwrap_or_default(),
        );
        let adapted_evidence = evidence_adapter.adapt();
        diagnostics.extend(evidence_adapter.diagnostics);
        if let Some(evidence) = adapted_evidence.modules.into_iter().next() {
            if let Some(root) = lowered
                .modules
                .iter_mut()
                .find(|module| module.identity == evidence.identity)
            {
                activate_evidence(root, evidence.span);
                root.declarations.extend(evidence.declarations);
            } else {
                diagnostics.push(error(
                    codes::UNSUPPORTED,
                    "uhura-0.4/evidence-package",
                    "evidence could not be attached to its checked package",
                    evidence.span,
                ));
            }
        }
    }
    let physical_source_paths = physical_source_paths(&all_modules, evidence_modules);
    let mut output =
        check_project_with_import_aliases(&lowered, &import_aliases, &physical_source_paths);
    output.diagnostics.extend(diagnostics);
    output.diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
            diagnostic.rule,
        )
    });
    output.diagnostics.dedup_by(|left, right| {
        left.code == right.code
            && left.rule == right.rule
            && left.span == right.span
            && left.message == right.message
    });
    suppress_contained_type_mismatch_cascades(&mut output.diagnostics);
    output.diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
            diagnostic.rule,
        )
    });
    if has_errors(&output.diagnostics) {
        output.program = None;
        return output;
    }
    match build_package_graph_provenance(
        root_package,
        packages,
        &external_references,
        &topology_bindings,
        &site_compositions,
    ) {
        Ok(artifacts) => {
            output.provenance = Some(artifacts.provenance);
            output.authoring = artifacts.authoring;
        }
        Err(failure) => {
            output.diagnostics.push(error(
                codes::UNSUPPORTED,
                "uhura-0.4/provenance",
                format!("could not build checked 0.4 provenance: {failure}"),
                all_modules
                    .first()
                    .map_or(ast::SourceSpan::empty(0, 0), |module| span(module.span)),
            ));
            output.program = None;
            return output;
        }
    }
    if let Some(program) = &mut output.program {
        program.machine_program.language = "uhura 0.4".into();
        program.machine_program.identity_protocol = MACHINE_PROGRAM_ID_PROTOCOL.into();
        program.machine_program.composed_part_declarations = composed_part_declarations;
        let site_result = site_compositions
            .iter()
            .try_for_each(|composition| composition.apply_site_ids(program));
        if let Err(failure) = site_result {
            output.diagnostics.push(error(
                codes::UNSUPPORTED,
                "uhura-0.4/site-identity",
                format!("could not assign checked 0.4 fault-site identities: {failure}"),
                all_modules
                    .first()
                    .map_or(ast::SourceSpan::empty(0, 0), |module| span(module.span)),
            ));
            output.program = None;
        } else {
            program.freeze_program_hashes();
        }
    }
    output
}

fn physical_source_paths(
    modules: &[uhura_syntax::ast::Module],
    evidence_modules: &[uhura_syntax::ast::Module],
) -> PhysicalSourcePaths {
    let mut paths = PhysicalSourcePaths::new();
    for (file, path) in modules
        .iter()
        .map(|module| (module.identity.file, module.identity.path.as_str()))
        .chain(
            evidence_modules
                .iter()
                .map(|module| (module.identity.file, module.identity.path.as_str())),
        )
    {
        paths.entry(file).or_insert_with(|| path.to_string());
    }
    paths
}

fn link_composed_package_source(
    consumer_package: &str,
    resolution: &mut Resolution,
    catalog: &BTreeMap<(String, String, String), ExternalDeclaration>,
    linked_public_declarations: &BTreeSet<String>,
    import_aliases: &mut ImportAliases,
) {
    let mut declarations = BTreeMap::<(String, String), ExternalDeclaration>::new();
    for declaration in catalog.values() {
        if declaration.visibility != uhura_syntax::ast::Visibility::Public {
            continue;
        }
        declarations
            .entry((declaration.package.clone(), declaration.name.clone()))
            .or_insert_with(|| declaration.clone());
    }

    // Parts and UI declarations are source-composition inputs, not checker
    // module exports. Their source imports have already served resolution and
    // must not leak into the aggregate kernel linker.
    resolution.external_imports.retain(|identity, _| {
        declarations
            .get(identity)
            .is_none_or(|declaration| declaration.kind != "part" && declaration.kind != "ui")
    });

    // A copied public Part body may reference another public declaration from
    // its provider or from a transitive locked dependency. Give the structural
    // adapter a closed exact-PublicId linker environment. These bindings are
    // not source-visible and unused entries never enter reachable machine
    // identity.
    let synthetic = uhura_syntax::ast::Span::new(0, 0, 0);
    for ((target_package, target_name), declaration) in declarations {
        let public_id = format!("{target_package}::{target_name}");
        if !linked_public_declarations.contains(&public_id) {
            continue;
        }
        if target_package == consumer_package
            || declaration.kind == "part"
            || declaration.kind == "ui"
        {
            continue;
        }
        let lowering_name = external_lowering_name(&target_package, &target_name);
        resolution
            .external_imports
            .entry((target_package.clone(), target_name.clone()))
            .or_insert_with(|| ExternalImport {
                local_name: lowering_name.clone(),
                span: synthetic,
            });
        if let Some(record) = declaration.record {
            resolution
                .external_structs
                .insert(lowering_name.clone(), record);
        }
        for (variant, shape) in declaration.variants {
            resolution
                .external_variants
                .insert((lowering_name.clone(), variant), shape);
        }
        import_aliases.insert(
            (consumer_package.to_owned(), target_package, target_name),
            lowering_name,
        );
    }
}

fn build_package_graph_provenance(
    root_package: &str,
    packages: &[CapturedPackageModules],
    external_references: &BTreeMap<String, Vec<crate::provenance::ExternalReference>>,
    topology_bindings: &[crate::topology::TopologyBinding],
    site_compositions: &[crate::parts::CompositionOutput],
) -> Result<crate::provenance::SourceArtifacts, String> {
    let mut sources = Vec::new();
    let mut occurrences = Vec::new();
    let mut authoring = crate::AuthoringProjection::default();
    let mut source_by_file = BTreeMap::<(String, u32), u32>::new();
    let mut source_text = BTreeMap::<(String, u32), &str>::new();
    let mut original_modules = Vec::new();
    let mut ordered = packages.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.package.as_bytes().cmp(right.package.as_bytes()));
    for package in ordered {
        original_modules.extend(package.modules.iter().cloned());
        let module_file = package
            .modules
            .iter()
            .map(|module| {
                (
                    (
                        module.identity.package.as_str(),
                        module.identity.module.as_str(),
                        module.identity.path.as_str(),
                    ),
                    module.identity.file,
                )
            })
            .collect::<BTreeMap<_, _>>();
        for module in &package.modules {
            if source_text
                .insert(
                    (package.package.clone(), module.identity.file),
                    module.source.as_str(),
                )
                .is_some()
            {
                return Err(format!(
                    "package `{}` contains duplicate source file number {}",
                    package.package, module.identity.file
                ));
            }
        }
        // Package-local resolution supplies definition and current-package
        // reference occurrences. The closed package linker supplies resolved
        // external reference occurrences without putting locator spelling
        // into semantic node identity.
        let mut modules = package.modules.clone();
        for module in &mut modules {
            module.uses.retain(|declaration| {
                let root = match &declaration.tree {
                    uhura_syntax::ast::ImportTree::Single { path, .. } => &path.root,
                    uhura_syntax::ast::ImportTree::Group { prefix, .. } => &prefix.root,
                };
                matches!(root, uhura_syntax::ast::ImportRoot::Crate(_))
                    || matches!(root, uhura_syntax::ast::ImportRoot::Package(name) if name.text == "uhura")
            });
        }
        let references = external_references
            .get(&package.package)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let artifacts = crate::provenance::build_source_artifacts_with_external_references(
            &modules, references,
        )
        .map_err(|error| error.to_string())?;
        // The current Editor source inventory contains only root-package
        // files. Dependency annotations stay package-local until that
        // inventory becomes package-qualified as well.
        if package.package == root_package {
            authoring.append(artifacts.authoring);
        }
        let provenance = artifacts.provenance;
        let offset = u32::try_from(sources.len())
            .map_err(|_| "package-graph provenance exceeds u32::MAX sources".to_string())?;
        sources.extend(provenance.sources.into_iter().map(|mut source| {
            let file = module_file
                .get(&(
                    source.package.as_str(),
                    source.module.as_str(),
                    source.path.as_str(),
                ))
                .copied()
                .expect("package provenance sources originate from captured modules");
            source.source += offset;
            let previous = source_by_file.insert((source.package.clone(), file), source.source);
            assert!(
                previous.is_none(),
                "captured package source file identities are unique"
            );
            source
        }));
        occurrences.extend(provenance.occurrences.into_iter().map(|mut occurrence| {
            occurrence.source += offset;
            occurrence
        }));
    }

    for origin in site_compositions
        .iter()
        .flat_map(crate::parts::CompositionOutput::site_occurrences)
    {
        let source_key = (origin.source_package.clone(), origin.span.file);
        let source = source_by_file.get(&source_key).copied().ok_or_else(|| {
            format!(
                "fault-site span {}..{} references unknown source file {} in package `{}`",
                origin.span.start, origin.span.end, origin.span.file, origin.source_package,
            )
        })?;
        occurrences.push(uhura_core::ProvenanceOccurrence {
            node: origin.node.clone(),
            source,
            start: origin.span.start,
            end: origin.span.end,
            role: if origin.owner == "root" {
                "definition".into()
            } else {
                "generated".into()
            },
            owner: origin.owner.clone(),
        });
    }

    let topology = crate::topology::build_linked(&original_modules, topology_bindings)?;
    for occurrence in topology.occurrences {
        let source_key = (occurrence.source_package.clone(), occurrence.span.file);
        let source = source_by_file.get(&source_key).copied().ok_or_else(|| {
            format!(
                "authored topology span {}..{} references unknown source file {} in package `{}`",
                occurrence.span.start,
                occurrence.span.end,
                occurrence.span.file,
                occurrence.source_package,
            )
        })?;
        let text = source_text
            .get(&source_key)
            .copied()
            .expect("source index and source text maps are constructed together");
        let start = usize::try_from(occurrence.span.start)
            .expect("u32 always fits usize on supported hosts");
        let end =
            usize::try_from(occurrence.span.end).expect("u32 always fits usize on supported hosts");
        if start > end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return Err(format!(
                "authored topology span {}..{} is not a valid UTF-8 range in source file {} of package `{}`",
                occurrence.span.start,
                occurrence.span.end,
                occurrence.span.file,
                occurrence.source_package,
            ));
        }
        occurrences.push(uhura_core::ProvenanceOccurrence {
            node: occurrence.node,
            source,
            start: occurrence.span.start,
            end: occurrence.span.end,
            role: occurrence.role.into(),
            owner: occurrence.owner,
        });
    }
    let provenance = Provenance::canonical_with_topology(sources, occurrences, topology.topology)?;
    authoring.canonicalize()?;
    Ok(crate::provenance::SourceArtifacts {
        provenance,
        authoring,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DeclarationKey {
    module: String,
    name: String,
}

#[derive(Clone, Debug)]
struct DeclarationInfo {
    visibility: uhura_syntax::ast::Visibility,
}

#[derive(Clone, Debug)]
struct ImportRequest {
    module: String,
    target_module: String,
    target_name: String,
    local_name: String,
    span: uhura_syntax::ast::Span,
    target_span: uhura_syntax::ast::Span,
}

#[derive(Clone, Debug, Default)]
struct ModuleResolution {
    bindings: BTreeMap<String, String>,
    visible_names: BTreeSet<String>,
}

#[derive(Clone, Debug)]
struct ExternalDeclaration {
    visibility: uhura_syntax::ast::Visibility,
    package: String,
    module: String,
    name: String,
    kind: &'static str,
    record: Option<RecordShape>,
    variants: BTreeMap<String, RecordShape>,
}

#[derive(Clone, Debug)]
struct ExternalImport {
    local_name: String,
    span: uhura_syntax::ast::Span,
}

struct ExternalResolutionContext<'a> {
    dependencies: &'a BTreeMap<String, String>,
    catalog: &'a BTreeMap<(String, String, String), ExternalDeclaration>,
}

#[derive(Clone, Debug)]
struct ExternalReexportRequest {
    package: String,
    module: String,
    name: String,
    target: (String, String, String),
    span: uhura_syntax::ast::Span,
}

fn build_external_catalog(
    packages: &[CapturedPackageModules],
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) -> BTreeMap<(String, String, String), ExternalDeclaration> {
    let mut catalog = BTreeMap::new();
    let mut public_names = BTreeMap::<(String, String), (String, String)>::new();
    for package in packages {
        for module in &package.modules {
            for declaration in &module.declarations {
                let (name, visibility) = declaration_header(declaration);
                let target = (package.package.clone(), name.text.clone());
                let kind = match &declaration.kind {
                    uhura_syntax::ast::DeclarationKind::Machine(_) => "machine",
                    uhura_syntax::ast::DeclarationKind::Part(_) => "part",
                    uhura_syntax::ast::DeclarationKind::Ui(_) => "ui",
                    uhura_syntax::ast::DeclarationKind::Scenario(_) => "scenario",
                    uhura_syntax::ast::DeclarationKind::Example(_) => "example",
                    uhura_syntax::ast::DeclarationKind::Checkpoint(_) => "checkpoint",
                    uhura_syntax::ast::DeclarationKind::Struct(_) => "struct",
                    uhura_syntax::ast::DeclarationKind::Enum(_) => "enum",
                    uhura_syntax::ast::DeclarationKind::Key(_) => "key",
                    uhura_syntax::ast::DeclarationKind::Const(_) => "const",
                    uhura_syntax::ast::DeclarationKind::Function(_) => "function",
                };
                let record = match &declaration.kind {
                    uhura_syntax::ast::DeclarationKind::Struct(value) => Some(RecordShape {
                        fields: value
                            .fields
                            .iter()
                            .map(|field| field.name.text.clone())
                            .collect(),
                    }),
                    _ => None,
                };
                let variants = match &declaration.kind {
                    uhura_syntax::ast::DeclarationKind::Enum(value) => value
                        .variants
                        .iter()
                        .map(|variant| {
                            (
                                variant.name.text.clone(),
                                RecordShape {
                                    fields: variant
                                        .fields
                                        .iter()
                                        .map(|field| field.name.text.clone())
                                        .collect(),
                                },
                            )
                        })
                        .collect(),
                    _ => BTreeMap::new(),
                };
                catalog.insert(
                    (
                        package.package.clone(),
                        module.identity.module.clone(),
                        name.text.clone(),
                    ),
                    ExternalDeclaration {
                        visibility,
                        package: target.0.clone(),
                        module: module.identity.module.clone(),
                        name: target.1.clone(),
                        kind,
                        record,
                        variants,
                    },
                );
                if visibility == uhura_syntax::ast::Visibility::Public {
                    public_names.entry(target.clone()).or_insert(target);
                }
            }
        }
    }

    let mut pending = Vec::new();
    for package in packages {
        for module in &package.modules {
            for declaration in &module.uses {
                if declaration.visibility != uhura_syntax::ast::Visibility::Public {
                    continue;
                }
                let uhura_syntax::ast::ImportTree::Single { path, alias } = &declaration.tree
                else {
                    continue;
                };
                if alias.is_some() {
                    continue;
                }
                let Some((name, module_segments)) = path.segments.split_last() else {
                    continue;
                };
                if module_segments.is_empty() {
                    continue;
                }
                let target_package = match &path.root {
                    uhura_syntax::ast::ImportRoot::Crate(_) => package.package.clone(),
                    uhura_syntax::ast::ImportRoot::Package(alias) if alias.text == "uhura" => {
                        continue;
                    }
                    uhura_syntax::ast::ImportRoot::Package(alias) => {
                        let Some(target) = package.dependencies.get(&alias.text) else {
                            continue;
                        };
                        target.clone()
                    }
                };
                pending.push(ExternalReexportRequest {
                    package: package.package.clone(),
                    module: module.identity.module.clone(),
                    name: name.text.clone(),
                    target: (
                        target_package,
                        module_segments
                            .iter()
                            .map(|segment| segment.text.as_str())
                            .collect::<Vec<_>>()
                            .join("::"),
                        name.text.clone(),
                    ),
                    span: name.span,
                });
            }
        }
    }

    loop {
        let mut progressed = false;
        let mut next = Vec::new();
        for request in pending {
            let Some(target) = catalog.get(&request.target).cloned() else {
                next.push(request);
                continue;
            };
            if target.visibility != uhura_syntax::ast::Visibility::Public {
                continue;
            }
            let public_key = (request.package.clone(), request.name.clone());
            let target_identity = (target.package.clone(), target.name.clone());
            if let Some(previous) = public_names.get(&public_key)
                && previous != &target_identity
            {
                diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura-0.4/reexport-collision",
                    format!(
                        "re-exported public name `{}` in package `{}` resolves to both `{}::{}` and `{}::{}`",
                        request.name,
                        request.package,
                        previous.0,
                        previous.1,
                        target.package,
                        target.name,
                    ),
                    span(request.span),
                ));
                continue;
            }
            let route = (
                request.package.clone(),
                request.module.clone(),
                request.name.clone(),
            );
            if let Some(previous) = catalog.get(&route)
                && (previous.package != target.package || previous.name != target.name)
            {
                diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura-0.4/reexport-collision",
                    format!(
                        "re-exported locator `{}::{}::{}` collides with another declaration",
                        request.package, request.module, request.name
                    ),
                    span(request.span),
                ));
                continue;
            }
            public_names.insert(public_key, target_identity);
            catalog.insert(
                route,
                ExternalDeclaration {
                    visibility: uhura_syntax::ast::Visibility::Public,
                    package: target.package,
                    module: target.module,
                    name: target.name,
                    kind: target.kind,
                    record: target.record,
                    variants: target.variants,
                },
            );
            progressed = true;
        }
        if next.is_empty() || !progressed {
            break;
        }
        pending = next;
    }

    catalog
}

#[derive(Clone, Debug, Default)]
struct Resolution {
    package: Option<String>,
    modules: BTreeMap<String, ModuleResolution>,
    standard_imports: BTreeMap<(String, String), uhura_syntax::ast::Span>,
    external_imports: BTreeMap<(String, String), ExternalImport>,
    external_structs: BTreeMap<String, RecordShape>,
    external_variants: BTreeMap<(String, String), RecordShape>,
    external_references: Vec<crate::provenance::ExternalReference>,
    topology_bindings: Vec<crate::topology::TopologyBinding>,
    diagnostics: Vec<uhura_base::Diagnostic>,
    metadata: ResolutionMetadata,
}

impl Resolution {
    fn build(
        sources: &[uhura_syntax::ast::Module],
        external: Option<&ExternalResolutionContext<'_>>,
    ) -> Self {
        let mut resolution = Self::default();
        let mut declarations = BTreeMap::<DeclarationKey, DeclarationInfo>::new();
        let mut declaration_names = BTreeMap::<String, BTreeSet<String>>::new();
        let mut public_names = BTreeMap::<String, DeclarationKey>::new();
        let mut routes = BTreeMap::<(String, String), DeclarationKey>::new();

        if sources.is_empty() {
            resolution.diagnostics.push(error(
                codes::MODULE,
                "uhura-0.4/empty-project",
                "Uhura 0.4 project resolution requires at least one manifest-resolved module",
                ast::SourceSpan::empty(0, 0),
            ));
            return resolution;
        }

        let mut ordered = sources.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.identity
                .module
                .as_bytes()
                .cmp(right.identity.module.as_bytes())
                .then_with(|| left.identity.file.cmp(&right.identity.file))
        });

        let expected_package = ordered[0].identity.package.clone();
        resolution.package = Some(expected_package.clone());
        resolution.metadata.package = Some(expected_package.clone());
        let private_ownership =
            private_declaration_ownership(&ordered, &expected_package, &mut resolution.diagnostics);
        let mut flattened_name_counts = BTreeMap::<String, usize>::new();
        for source in &ordered {
            for declaration in &source.declarations {
                let (name, _) = declaration_header(declaration);
                *flattened_name_counts.entry(name.text.clone()).or_default() += 1;
            }
        }
        let mut seen_modules = BTreeSet::new();
        for source in &ordered {
            resolution.metadata.sources.push(ResolvedSource {
                file: source.identity.file,
                package: source.identity.package.clone(),
                module: source.identity.module.clone(),
                path: source.identity.path.clone(),
                bytes: source.source.len().try_into().unwrap_or(u64::MAX),
            });
            if source.identity.package != expected_package {
                resolution.diagnostics.push(error(
                    codes::MODULE,
                    "uhura-0.4/package-mismatch",
                    format!(
                        "resolved module `{}` belongs to `{}`, expected package `{expected_package}`",
                        source.identity.module, source.identity.package
                    ),
                    span(source.span),
                ));
            }
            if !seen_modules.insert(source.identity.module.clone()) {
                resolution.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura-0.4/duplicate-logical-module",
                    format!(
                        "logical module `{}` occurs more than once in the resolved project",
                        source.identity.module
                    ),
                    span(source.span),
                ));
            }
            resolution
                .modules
                .entry(source.identity.module.clone())
                .or_default();

            let local_names = declaration_names
                .entry(source.identity.module.clone())
                .or_default();
            for declaration in &source.declarations {
                let (name, visibility) = declaration_header(declaration);
                if !local_names.insert(name.text.clone()) {
                    resolution.diagnostics.push(error(
                        codes::DUPLICATE,
                        "uhura-0.4/duplicate-declaration",
                        format!(
                            "declaration `{}` occurs more than once in logical module `{}`",
                            name.text, source.identity.module
                        ),
                        span(name.span),
                    ));
                    continue;
                }
                let key = DeclarationKey {
                    module: source.identity.module.clone(),
                    name: name.text.clone(),
                };
                let info = DeclarationInfo { visibility };
                resolution.metadata.declarations.push(ResolvedDeclaration {
                    package: source.identity.package.clone(),
                    module: source.identity.module.clone(),
                    name: name.text.clone(),
                    public_id: (visibility == uhura_syntax::ast::Visibility::Public)
                        .then(|| format!("{}::{}", source.identity.package, name.text)),
                    span: name.span,
                });
                if visibility == uhura_syntax::ast::Visibility::Public {
                    if let Some(previous) = public_names.insert(name.text.clone(), key.clone())
                        && previous != key
                    {
                        resolution.diagnostics.push(error(
                            codes::DUPLICATE,
                            "uhura-0.4/public-name-collision",
                            format!(
                                "public name `{}` is already declared in logical module `{}`; public names are package-global",
                                name.text, previous.module
                            ),
                            span(name.span),
                        ));
                    }
                    routes.insert(
                        (source.identity.module.clone(), name.text.clone()),
                        key.clone(),
                    );
                }
                declarations.insert(key, info);
                let lowering_name = if visibility == uhura_syntax::ast::Visibility::Private {
                    let owners = private_ownership.get(&DeclarationKey {
                        module: source.identity.module.clone(),
                        name: name.text.clone(),
                    });
                    if private_declaration_requires_owner(declaration) {
                        private_lowering_name(
                            declaration,
                            owners.and_then(|owners| owners.iter().next().map(String::as_str)),
                        )
                    } else if flattened_name_counts.get(&name.text).copied().unwrap_or(0) > 1 {
                        private_structural_lowering_name(declaration, owners)
                    } else {
                        name.text.clone()
                    }
                } else {
                    name.text.clone()
                };
                let module = resolution
                    .modules
                    .entry(source.identity.module.clone())
                    .or_default();
                module.visible_names.insert(name.text.clone());
                module.bindings.insert(name.text.clone(), lowering_name);
            }
        }

        let mut reexports = Vec::new();
        let mut imports = Vec::new();
        for source in &ordered {
            for declaration in &source.uses {
                if ui::handle_standard_profile_use(declaration, &mut resolution.diagnostics) {
                    continue;
                }
                if resolution.handle_standard_use(source, declaration, &declaration_names) {
                    continue;
                }
                if resolution.handle_external_use(source, declaration, &declaration_names, external)
                {
                    continue;
                }
                match import_requests(source, declaration, &mut resolution.diagnostics) {
                    Some(requests)
                        if declaration.visibility == uhura_syntax::ast::Visibility::Public =>
                    {
                        reexports.extend(requests);
                    }
                    Some(requests) => imports.extend(requests),
                    None => {}
                }
            }
        }

        // Re-exports may target another re-export. Resolve the finite route
        // graph to a fixed point, then diagnose only the requests that remain.
        let mut pending = reexports;
        loop {
            let mut progressed = false;
            let mut next = Vec::new();
            for request in pending {
                let target = routes
                    .get(&(request.target_module.clone(), request.target_name.clone()))
                    .cloned();
                let Some(target) = target else {
                    next.push(request);
                    continue;
                };
                if declaration_names
                    .get(&request.module)
                    .is_some_and(|names| names.contains(&request.local_name))
                    || routes.contains_key(&(request.module.clone(), request.local_name.clone()))
                {
                    resolution.diagnostics.push(error(
                        codes::DUPLICATE,
                        "uhura-0.4/reexport-collision",
                        format!(
                            "re-exported name `{}` collides in logical module `{}`",
                            request.local_name, request.module
                        ),
                        span(request.span),
                    ));
                    continue;
                }
                routes.insert(
                    (request.module.clone(), request.local_name.clone()),
                    target.clone(),
                );
                let module = resolution
                    .modules
                    .entry(request.module.clone())
                    .or_default();
                module
                    .bindings
                    .insert(request.local_name.clone(), target.name.clone());
                module.visible_names.insert(request.local_name.clone());
                resolution
                    .topology_bindings
                    .push(crate::topology::TopologyBinding {
                        source_package: expected_package.clone(),
                        source_module: request.module.clone(),
                        local_name: request.local_name.clone(),
                        target_package: expected_package.clone(),
                        target_module: target.module.clone(),
                        target_name: target.name.clone(),
                    });
                resolution.metadata.bindings.push(ResolvedBinding {
                    module: request.module,
                    local_name: request.local_name,
                    target_module: target.module,
                    target_name: target.name,
                    reexport: true,
                    span: request.span,
                    target_span: request.target_span,
                });
                progressed = true;
            }
            if next.is_empty() {
                pending = next;
                break;
            }
            if !progressed {
                pending = next;
                break;
            }
            pending = next;
        }
        for request in pending {
            resolution.unresolved_import(&request, &declarations);
        }

        for request in imports {
            let Some(target) = routes
                .get(&(request.target_module.clone(), request.target_name.clone()))
                .cloned()
            else {
                resolution.unresolved_import(&request, &declarations);
                continue;
            };
            let local_declaration_collision = declaration_names
                .get(&request.module)
                .is_some_and(|names| names.contains(&request.local_name));
            let local_reexport_collision =
                routes.contains_key(&(request.module.clone(), request.local_name.clone()));
            let module = resolution
                .modules
                .entry(request.module.clone())
                .or_default();
            if local_declaration_collision
                || local_reexport_collision
                || module.bindings.contains_key(&request.local_name)
            {
                resolution.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura-0.4/import-collision",
                    format!(
                        "imported binding `{}` collides in logical module `{}`",
                        request.local_name, request.module
                    ),
                    span(request.span),
                ));
                continue;
            }
            module
                .bindings
                .insert(request.local_name.clone(), target.name.clone());
            module.visible_names.insert(request.local_name.clone());
            resolution
                .topology_bindings
                .push(crate::topology::TopologyBinding {
                    source_package: expected_package.clone(),
                    source_module: request.module.clone(),
                    local_name: request.local_name.clone(),
                    target_package: expected_package.clone(),
                    target_module: target.module.clone(),
                    target_name: target.name.clone(),
                });
            resolution.metadata.bindings.push(ResolvedBinding {
                module: request.module,
                local_name: request.local_name,
                target_module: target.module,
                target_name: target.name,
                reexport: false,
                span: request.span,
                target_span: request.target_span,
            });
        }

        let package_public_names = public_names.keys().cloned().collect::<BTreeSet<_>>();
        for source in &ordered {
            let visible_names = resolution
                .modules
                .get(&source.identity.module)
                .map(|module| &module.visible_names)
                .expect("every resolved source has a module scope");
            for (name, reference_span) in declaration_root_references(source) {
                if package_public_names.contains(&name) && !visible_names.contains(&name) {
                    resolution.diagnostics.push(error(
                        codes::IMPORT,
                        "uhura-0.4/unimported-name",
                        format!(
                            "package declaration `{name}` is not visible in logical module `{}`; import it explicitly with `use`",
                            source.identity.module
                        ),
                        span(reference_span),
                    ));
                }
            }
        }

        resolution.metadata.sources.sort_by(|left, right| {
            left.path
                .as_bytes()
                .cmp(right.path.as_bytes())
                .then_with(|| left.module.as_bytes().cmp(right.module.as_bytes()))
                .then_with(|| left.file.cmp(&right.file))
        });
        resolution.metadata.declarations.sort_by(|left, right| {
            left.name
                .as_bytes()
                .cmp(right.name.as_bytes())
                .then_with(|| left.module.as_bytes().cmp(right.module.as_bytes()))
                .then_with(|| {
                    (left.span.file, left.span.start, left.span.end).cmp(&(
                        right.span.file,
                        right.span.start,
                        right.span.end,
                    ))
                })
        });
        resolution.metadata.bindings.sort_by(|left, right| {
            left.module
                .as_bytes()
                .cmp(right.module.as_bytes())
                .then_with(|| left.local_name.as_bytes().cmp(right.local_name.as_bytes()))
                .then_with(|| {
                    left.target_name
                        .as_bytes()
                        .cmp(right.target_name.as_bytes())
                })
        });

        resolution
    }

    fn handle_standard_use(
        &mut self,
        source: &uhura_syntax::ast::Module,
        declaration: &uhura_syntax::ast::UseDeclaration,
        declaration_names: &BTreeMap<String, BTreeSet<String>>,
    ) -> bool {
        let Some(requests) = standard_import_requests(declaration, &mut self.diagnostics) else {
            return false;
        };
        let module = self
            .modules
            .entry(source.identity.module.clone())
            .or_default();
        for request in requests {
            if declaration_names
                .get(&source.identity.module)
                .is_some_and(|names| names.contains(&request.local_name))
                || module.bindings.contains_key(&request.local_name)
            {
                self.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura-0.4/import-collision",
                    format!(
                        "imported binding `{}` collides in logical module `{}`",
                        request.local_name, source.identity.module
                    ),
                    span(request.span),
                ));
                continue;
            }
            module
                .bindings
                .insert(request.local_name, request.name.clone());
            module.visible_names.insert(request.name.clone());
            self.standard_imports
                .entry((request.target, request.name))
                .or_insert(request.span);
        }
        true
    }

    fn handle_external_use(
        &mut self,
        source: &uhura_syntax::ast::Module,
        declaration: &uhura_syntax::ast::UseDeclaration,
        declaration_names: &BTreeMap<String, BTreeSet<String>>,
        external: Option<&ExternalResolutionContext<'_>>,
    ) -> bool {
        let root = match &declaration.tree {
            uhura_syntax::ast::ImportTree::Single { path, .. } => &path.root,
            uhura_syntax::ast::ImportTree::Group { prefix, .. } => &prefix.root,
        };
        let uhura_syntax::ast::ImportRoot::Package(alias) = root else {
            return false;
        };
        if alias.text == "uhura" {
            return false;
        }
        let Some(external) = external else {
            unsupported_package_import(alias, declaration.span, &mut self.diagnostics);
            return true;
        };
        let Some(target_package) = external.dependencies.get(&alias.text) else {
            self.diagnostics.push(error(
                codes::IMPORT,
                "uhura-0.4/unknown-dependency-alias",
                format!(
                    "`{}` is not a dependency alias of package `{}`",
                    alias.text, source.identity.package
                ),
                span(alias.span),
            ));
            return true;
        };

        let requests = match &declaration.tree {
            uhura_syntax::ast::ImportTree::Single { path, alias } => {
                let Some((name, modules)) = path.segments.split_last() else {
                    self.diagnostics.push(error(
                        codes::IMPORT,
                        "uhura-0.4/invalid-external-import",
                        "an external import requires a logical module and public name",
                        span(path.span),
                    ));
                    return true;
                };
                if modules.is_empty() {
                    self.diagnostics.push(error(
                        codes::IMPORT,
                        "uhura-0.4/invalid-external-import",
                        "an external import requires a logical module before the public name",
                        span(path.span),
                    ));
                    return true;
                }
                vec![(
                    modules
                        .iter()
                        .map(|segment| segment.text.as_str())
                        .collect::<Vec<_>>()
                        .join("::"),
                    name.text.clone(),
                    alias.as_ref().unwrap_or(name).text.clone(),
                    alias.as_ref().map_or(name.span, |value| value.span),
                    name.span,
                )]
            }
            uhura_syntax::ast::ImportTree::Group { prefix, items } => {
                if prefix.segments.is_empty() {
                    self.diagnostics.push(error(
                        codes::IMPORT,
                        "uhura-0.4/invalid-external-import",
                        "a grouped external import requires a logical module",
                        span(prefix.span),
                    ));
                    return true;
                }
                let module = prefix
                    .segments
                    .iter()
                    .map(|segment| segment.text.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                items
                    .iter()
                    .map(|item| {
                        (
                            module.clone(),
                            item.name.text.clone(),
                            item.alias.as_ref().unwrap_or(&item.name).text.clone(),
                            item.alias
                                .as_ref()
                                .map_or(item.name.span, |value| value.span),
                            item.name.span,
                        )
                    })
                    .collect()
            }
        };

        for (target_module, target_name, local_name, binding_span, target_span) in requests {
            let key = (
                target_package.clone(),
                target_module.clone(),
                target_name.clone(),
            );
            let Some(target) = external.catalog.get(&key) else {
                self.diagnostics.push(error(
                    codes::IMPORT,
                    "uhura-0.4/unresolved-external-import",
                    format!(
                        "`{}::{target_module}::{target_name}` does not resolve in locked package `{target_package}`",
                        alias.text
                    ),
                    span(target_span),
                ));
                continue;
            };
            if target.visibility != uhura_syntax::ast::Visibility::Public {
                self.diagnostics.push(error(
                    codes::IMPORT,
                    "uhura-0.4/private-external-import",
                    format!(
                        "`{}::{target_module}::{target_name}` names a private declaration",
                        alias.text
                    ),
                    span(target_span),
                ));
                continue;
            }
            let module = self
                .modules
                .entry(source.identity.module.clone())
                .or_default();
            if declaration_names
                .get(&source.identity.module)
                .is_some_and(|names| names.contains(&local_name))
                || module.bindings.contains_key(&local_name)
            {
                self.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura-0.4/import-collision",
                    format!(
                        "imported binding `{local_name}` collides in logical module `{}`",
                        source.identity.module
                    ),
                    span(binding_span),
                ));
                continue;
            }
            let lowering_name = external_lowering_name(&target.package, &target.name);
            module
                .bindings
                .insert(local_name.clone(), lowering_name.clone());
            module.visible_names.insert(local_name.clone());
            if let Some(record) = &target.record {
                self.external_structs
                    .insert(lowering_name.clone(), record.clone());
            }
            for (variant, shape) in &target.variants {
                self.external_variants
                    .insert((lowering_name.clone(), variant.clone()), shape.clone());
            }
            self.external_imports
                .entry((target.package.clone(), target.name.clone()))
                .or_insert(ExternalImport {
                    local_name: lowering_name,
                    span: binding_span,
                });
            self.external_references
                .push(crate::provenance::ExternalReference {
                    node: uhura_core::semantic_node_id(
                        &format!("{}::{}", target.package, target.name),
                        "root",
                        target.kind,
                        &format!("declaration/{}", target.name),
                    ),
                    span: target_span,
                });
            self.topology_bindings
                .push(crate::topology::TopologyBinding {
                    source_package: source.identity.package.clone(),
                    source_module: source.identity.module.clone(),
                    local_name: local_name.clone(),
                    target_package: target.package.clone(),
                    target_module: target.module.clone(),
                    target_name: target.name.clone(),
                });
            self.metadata.bindings.push(ResolvedBinding {
                module: source.identity.module.clone(),
                local_name,
                target_module,
                target_name,
                reexport: declaration.visibility == uhura_syntax::ast::Visibility::Public,
                span: binding_span,
                target_span,
            });
        }
        true
    }

    fn unresolved_import(
        &mut self,
        request: &ImportRequest,
        declarations: &BTreeMap<DeclarationKey, DeclarationInfo>,
    ) {
        let target_key = DeclarationKey {
            module: request.target_module.clone(),
            name: request.target_name.clone(),
        };
        if declarations
            .get(&target_key)
            .is_some_and(|value| value.visibility == uhura_syntax::ast::Visibility::Private)
        {
            self.diagnostics.push(error(
                codes::IMPORT,
                "uhura-0.4/private-import",
                format!(
                    "`crate::{}::{}` names a private declaration",
                    request.target_module, request.target_name
                ),
                span(request.target_span),
            ));
        } else {
            self.diagnostics.push(error(
                codes::IMPORT,
                "uhura-0.4/unresolved-import",
                format!(
                    "`crate::{}::{}` does not resolve to one public declaration",
                    request.target_module, request.target_name
                ),
                span(request.target_span),
            ));
        }
    }
}

#[derive(Clone, Debug)]
struct StandardImportRequest {
    target: String,
    name: String,
    local_name: String,
    span: uhura_syntax::ast::Span,
}

fn standard_import_requests(
    declaration: &uhura_syntax::ast::UseDeclaration,
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) -> Option<Vec<StandardImportRequest>> {
    let root = match &declaration.tree {
        uhura_syntax::ast::ImportTree::Single { path, .. } => &path.root,
        uhura_syntax::ast::ImportTree::Group { prefix, .. } => &prefix.root,
    };
    let uhura_syntax::ast::ImportRoot::Package(root) = root else {
        return None;
    };
    if root.text != "uhura" {
        return None;
    }
    if declaration.visibility == uhura_syntax::ast::Visibility::Public {
        diagnostics.push(error(
            codes::IMPORT,
            "uhura-0.4/standard-reexport",
            "standard-library bindings are lexical capabilities and cannot be re-exported",
            span(declaration.span),
        ));
        return Some(Vec::new());
    }

    let mut requests = Vec::new();
    match &declaration.tree {
        uhura_syntax::ast::ImportTree::Single { path, alias } => {
            let Some((name, modules)) = path.segments.split_last() else {
                diagnostics.push(error(
                    codes::IMPORT,
                    "uhura-0.4/invalid-standard-import",
                    "a standard import requires a module and exported name",
                    span(path.span),
                ));
                return Some(Vec::new());
            };
            if modules.len() != 1 {
                diagnostics.push(error(
                    codes::IMPORT,
                    "uhura-0.4/invalid-standard-import",
                    "standard imports have exact form `uhura::<module>::<name>`",
                    span(path.span),
                ));
                return Some(Vec::new());
            }
            if let Some(target) = standard_import_target(&modules[0].text, &name.text) {
                requests.push(StandardImportRequest {
                    target: target.into(),
                    name: name.text.clone(),
                    local_name: alias.as_ref().unwrap_or(name).text.clone(),
                    span: alias.as_ref().map_or(name.span, |value| value.span),
                });
            } else {
                invalid_standard_export(&modules[0], name, diagnostics);
            }
        }
        uhura_syntax::ast::ImportTree::Group { prefix, items } => {
            if prefix.segments.len() != 1 {
                diagnostics.push(error(
                    codes::IMPORT,
                    "uhura-0.4/invalid-standard-import",
                    "grouped standard imports have exact form `uhura::<module>::{...}`",
                    span(prefix.span),
                ));
                return Some(Vec::new());
            }
            for item in items {
                if let Some(target) =
                    standard_import_target(&prefix.segments[0].text, &item.name.text)
                {
                    requests.push(StandardImportRequest {
                        target: target.into(),
                        name: item.name.text.clone(),
                        local_name: item.alias.as_ref().unwrap_or(&item.name).text.clone(),
                        span: item
                            .alias
                            .as_ref()
                            .map_or(item.name.span, |value| value.span),
                    });
                } else {
                    invalid_standard_export(&prefix.segments[0], &item.name, diagnostics);
                }
            }
        }
    }
    Some(requests)
}

fn standard_import_target(module: &str, name: &str) -> Option<&'static str> {
    match (module, name) {
        ("boundary", "Token") => Some("uhura.boundary@1"),
        ("observation", "Observation") => Some("uhura.observation@1"),
        ("ports", "RequestPort" | "SinkPort") => Some("uhura.ports@1"),
        ("web_router", "Router" | "Routes" | "Link") => Some("uhura.web_router@1"),
        ("ui_surface", "Surface") => Some("uhura.ui_surface@1"),
        _ => None,
    }
}

fn invalid_standard_export(
    module: &uhura_syntax::ast::Identifier,
    name: &uhura_syntax::ast::Identifier,
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) {
    diagnostics.push(error(
        codes::IMPORT,
        "uhura-0.4/unknown-standard-export",
        format!(
            "`uhura::{}::{}` is not an Uhura 0.4 standard export",
            module.text, name.text
        ),
        span(name.span),
    ));
}

fn import_requests(
    source: &uhura_syntax::ast::Module,
    declaration: &uhura_syntax::ast::UseDeclaration,
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) -> Option<Vec<ImportRequest>> {
    if declaration.visibility == uhura_syntax::ast::Visibility::Public
        && !matches!(
            &declaration.tree,
            uhura_syntax::ast::ImportTree::Single { alias: None, .. }
        )
    {
        diagnostics.push(error(
            codes::IMPORT,
            "uhura-0.4/invalid-reexport",
            "`pub use` must be one singular, unaliased current-package locator",
            span(declaration.span),
        ));
        return None;
    }

    match &declaration.tree {
        uhura_syntax::ast::ImportTree::Single { path, alias } => {
            let (module, name) =
                current_package_locator(&path.root, &path.segments, path.span, diagnostics)?;
            Some(vec![ImportRequest {
                module: source.identity.module.clone(),
                target_module: module,
                target_name: name.text.clone(),
                local_name: alias.as_ref().unwrap_or(name).text.clone(),
                span: alias.as_ref().map_or(name.span, |value| value.span),
                target_span: name.span,
            }])
        }
        uhura_syntax::ast::ImportTree::Group { prefix, items } => {
            let module = match &prefix.root {
                uhura_syntax::ast::ImportRoot::Crate(_) if !prefix.segments.is_empty() => prefix
                    .segments
                    .iter()
                    .map(|value| value.text.as_str())
                    .collect::<Vec<_>>()
                    .join("::"),
                uhura_syntax::ast::ImportRoot::Crate(_) => {
                    diagnostics.push(error(
                        codes::IMPORT,
                        "uhura-0.4/invalid-import-locator",
                        "a current-package grouped import requires a logical module path",
                        span(prefix.span),
                    ));
                    return None;
                }
                uhura_syntax::ast::ImportRoot::Package(root) => {
                    unsupported_package_import(root, prefix.span, diagnostics);
                    return None;
                }
            };
            Some(
                items
                    .iter()
                    .map(|item| ImportRequest {
                        module: source.identity.module.clone(),
                        target_module: module.clone(),
                        target_name: item.name.text.clone(),
                        local_name: item.alias.as_ref().unwrap_or(&item.name).text.clone(),
                        span: item
                            .alias
                            .as_ref()
                            .map_or(item.name.span, |value| value.span),
                        target_span: item.name.span,
                    })
                    .collect(),
            )
        }
    }
}

fn current_package_locator<'a>(
    root: &uhura_syntax::ast::ImportRoot,
    segments: &'a [uhura_syntax::ast::Identifier],
    locator_span: uhura_syntax::ast::Span,
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) -> Option<(String, &'a uhura_syntax::ast::Identifier)> {
    if let uhura_syntax::ast::ImportRoot::Package(root) = root {
        unsupported_package_import(root, locator_span, diagnostics);
        return None;
    }
    let Some((name, modules)) = segments.split_last() else {
        diagnostics.push(error(
            codes::IMPORT,
            "uhura-0.4/invalid-import-locator",
            "a current-package import requires a logical module and public name",
            span(locator_span),
        ));
        return None;
    };
    if modules.is_empty() {
        diagnostics.push(error(
            codes::IMPORT,
            "uhura-0.4/invalid-import-locator",
            "`crate` imports require at least one logical module segment before the public name",
            span(locator_span),
        ));
        return None;
    }
    Some((
        modules
            .iter()
            .map(|value| value.text.as_str())
            .collect::<Vec<_>>()
            .join("::"),
        name,
    ))
}

fn unsupported_package_import(
    root: &uhura_syntax::ast::Identifier,
    locator_span: uhura_syntax::ast::Span,
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) {
    let (rule, message) = if root.text == "uhura" {
        (
            "uhura-0.4/unknown-standard-export",
            "the named standard package feature is not defined by this language profile"
                .to_string(),
        )
    } else {
        (
            "uhura-0.4/unknown-dependency-alias",
            format!(
                "`{}` is not available without an exact lock-resolved dependency binding",
                root.text
            ),
        )
    };
    diagnostics.push(error(codes::IMPORT, rule, message, span(locator_span)));
}

fn declaration_header(
    declaration: &uhura_syntax::ast::Declaration,
) -> (
    &uhura_syntax::ast::Identifier,
    uhura_syntax::ast::Visibility,
) {
    match &declaration.kind {
        uhura_syntax::ast::DeclarationKind::Machine(value) => (&value.name, value.visibility),
        uhura_syntax::ast::DeclarationKind::Part(value) => (&value.name, value.visibility),
        uhura_syntax::ast::DeclarationKind::Ui(value) => (&value.name, value.visibility),
        uhura_syntax::ast::DeclarationKind::Scenario(value) => {
            (&value.name, uhura_syntax::ast::Visibility::Private)
        }
        uhura_syntax::ast::DeclarationKind::Example(value)
        | uhura_syntax::ast::DeclarationKind::Checkpoint(value) => {
            (&value.name, uhura_syntax::ast::Visibility::Private)
        }
        uhura_syntax::ast::DeclarationKind::Struct(value) => (&value.name, value.visibility),
        uhura_syntax::ast::DeclarationKind::Enum(value) => (&value.name, value.visibility),
        uhura_syntax::ast::DeclarationKind::Key(value) => (&value.name, value.visibility),
        uhura_syntax::ast::DeclarationKind::Const(value) => (&value.name, value.visibility),
        uhura_syntax::ast::DeclarationKind::Function(value) => (&value.name, value.visibility),
    }
}

fn private_declaration_ownership(
    sources: &[&uhura_syntax::ast::Module],
    package: &str,
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) -> BTreeMap<DeclarationKey, BTreeSet<String>> {
    let mut declarations = BTreeMap::<DeclarationKey, &uhura_syntax::ast::Declaration>::new();
    let mut references = BTreeMap::<DeclarationKey, BTreeSet<String>>::new();
    for source in sources {
        for declaration in &source.declarations {
            let (name, _) = declaration_header(declaration);
            let key = DeclarationKey {
                module: source.identity.module.clone(),
                name: name.text.clone(),
            };
            declarations.entry(key.clone()).or_insert(declaration);
            references.entry(key).or_default().extend(
                declaration_references(declaration)
                    .into_iter()
                    .map(|(name, _)| name),
            );
        }
    }

    let mut ownership = BTreeMap::<DeclarationKey, BTreeSet<String>>::new();
    for (root, declaration) in &declarations {
        let (name, visibility) = declaration_header(declaration);
        if visibility != uhura_syntax::ast::Visibility::Public {
            continue;
        }
        let owner = format!("{package}::{}", name.text);
        let mut pending = references
            .get(root)
            .into_iter()
            .flatten()
            .filter_map(|reference| {
                private_reference_target(&root.module, reference, &declarations)
            })
            .collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        while let Some(target) = pending.pop() {
            if !visited.insert(target.clone()) {
                continue;
            }
            ownership
                .entry(target.clone())
                .or_default()
                .insert(owner.clone());
            pending.extend(
                references
                    .get(&target)
                    .into_iter()
                    .flatten()
                    .filter_map(|reference| {
                        private_reference_target(&target.module, reference, &declarations)
                    }),
            );
        }
    }

    for (key, owners) in &ownership {
        if owners.len() <= 1 {
            continue;
        }
        let Some(declaration) = declarations.get(key) else {
            continue;
        };
        if !private_declaration_requires_owner(declaration) {
            continue;
        }
        let (name, _) = declaration_header(declaration);
        diagnostics.push(error(
            codes::DUPLICATE,
            "uhura-0.4/private-identity-multiple-owners",
            format!(
                "private {} `{}` is reachable from multiple public owners ({}); make it an explicit `pub` package declaration or split it per owner",
                private_declaration_kind(declaration),
                name.text,
                owners.iter().cloned().collect::<Vec<_>>().join(", "),
            ),
            span(name.span),
        ));
    }

    ownership
}

fn private_reference_target(
    module: &str,
    reference: &str,
    declarations: &BTreeMap<DeclarationKey, &uhura_syntax::ast::Declaration>,
) -> Option<DeclarationKey> {
    let key = DeclarationKey {
        module: module.to_owned(),
        name: reference.to_owned(),
    };
    declarations.get(&key).and_then(|declaration| {
        let (_, visibility) = declaration_header(declaration);
        (visibility == uhura_syntax::ast::Visibility::Private).then_some(key)
    })
}

fn private_declaration_requires_owner(declaration: &uhura_syntax::ast::Declaration) -> bool {
    matches!(
        declaration.kind,
        uhura_syntax::ast::DeclarationKind::Machine(_)
            | uhura_syntax::ast::DeclarationKind::Ui(_)
            | uhura_syntax::ast::DeclarationKind::Struct(_)
            | uhura_syntax::ast::DeclarationKind::Enum(_)
            | uhura_syntax::ast::DeclarationKind::Key(_)
    )
}

fn private_declaration_kind(declaration: &uhura_syntax::ast::Declaration) -> &'static str {
    match declaration.kind {
        uhura_syntax::ast::DeclarationKind::Machine(_) => "machine",
        uhura_syntax::ast::DeclarationKind::Part(_) => "Part",
        uhura_syntax::ast::DeclarationKind::Ui(_) => "UI declaration",
        uhura_syntax::ast::DeclarationKind::Struct(_) => "struct",
        uhura_syntax::ast::DeclarationKind::Enum(_) => "enum",
        uhura_syntax::ast::DeclarationKind::Key(_) => "key",
        uhura_syntax::ast::DeclarationKind::Const(_) => "const",
        uhura_syntax::ast::DeclarationKind::Function(_) => "function",
        uhura_syntax::ast::DeclarationKind::Scenario(_) => "scenario",
        uhura_syntax::ast::DeclarationKind::Example(_) => "example",
        uhura_syntax::ast::DeclarationKind::Checkpoint(_) => "checkpoint",
    }
}

fn private_lowering_name(
    declaration: &uhura_syntax::ast::Declaration,
    public_owner: Option<&str>,
) -> String {
    let (name, _) = declaration_header(declaration);
    let mut declaration = declaration.clone();
    if let uhura_syntax::ast::DeclarationKind::Ui(ui) = &mut declaration.kind {
        erase_private_ui_authoring_metadata(&mut ui.body.nodes);
    }
    let mut declaration = serde_json::to_value(declaration)
        .expect("the serialization-friendly 0.4 AST must encode as JSON");
    erase_source_coordinates(&mut declaration);
    let fingerprint = uhura_base::hash_json(&serde_json::json!({
        "protocol": "uhura-private-identity/0",
        "owner": public_owner,
        "declaration": declaration,
    }));
    format!(
        "__uhura_private_{}_{name}",
        &fingerprint[..24],
        name = name.text
    )
}

fn erase_private_ui_authoring_metadata(nodes: &mut Vec<uhura_syntax::ast::UiNode>) {
    nodes.retain_mut(|node| match &mut node.kind {
        uhura_syntax::ast::UiNodeKind::Comment(_) => false,
        uhura_syntax::ast::UiNodeKind::Element(value) => {
            value.annotations.clear();
            erase_private_ui_authoring_metadata(&mut value.children);
            true
        }
        uhura_syntax::ast::UiNodeKind::If(value) => {
            value.annotations.clear();
            erase_private_ui_authoring_metadata(&mut value.then_branch);
            if let Some(branch) = &mut value.else_branch {
                erase_private_ui_authoring_metadata(branch);
            }
            true
        }
        uhura_syntax::ast::UiNodeKind::Each(value) => {
            value.annotations.clear();
            erase_private_ui_authoring_metadata(&mut value.children);
            true
        }
        uhura_syntax::ast::UiNodeKind::Text(value) => !value.raw.chars().all(char::is_whitespace),
        uhura_syntax::ast::UiNodeKind::Interpolation(_) => true,
    });

    let mut semantic = Vec::<uhura_syntax::ast::UiNode>::with_capacity(nodes.len());
    for node in std::mem::take(nodes) {
        if let uhura_syntax::ast::UiNodeKind::Text(text) = &node.kind
            && let Some(previous) = semantic.last_mut()
            && let uhura_syntax::ast::UiNodeKind::Text(previous_text) = &mut previous.kind
        {
            previous_text.raw.push_str(&text.raw);
            previous.span = previous.span.through(node.span);
            continue;
        }
        semantic.push(node);
    }
    *nodes = semantic;
}

fn private_structural_lowering_name(
    declaration: &uhura_syntax::ast::Declaration,
    public_owners: Option<&BTreeSet<String>>,
) -> String {
    let (name, _) = declaration_header(declaration);
    let mut declaration = serde_json::to_value(declaration)
        .expect("the serialization-friendly 0.4 AST must encode as JSON");
    erase_source_coordinates(&mut declaration);
    let owners = public_owners.into_iter().flatten().collect::<Vec<_>>();
    let fingerprint = uhura_base::hash_json(&serde_json::json!({
        "protocol": "uhura-private-structural-lowering/0",
        "owners": owners,
        "declaration": declaration,
    }));
    format!(
        "__uhura_private_structural_{}_{name}",
        &fingerprint[..24],
        name = name.text
    )
}

pub(super) fn external_lowering_name(package: &str, public_name: &str) -> String {
    let fingerprint = uhura_base::sha256_hex(
        format!("uhura-external-lowering/0\0{package}\0{public_name}").as_bytes(),
    );
    format!("__uhura_external_{}_{public_name}", &fingerprint[..24])
}

fn erase_source_coordinates(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                erase_source_coordinates(value);
            }
        }
        serde_json::Value::Object(values) => {
            values.retain(|key, _| key != "span" && key != "semicolon" && !key.ends_with("_span"));
            for value in values.values_mut() {
                erase_source_coordinates(value);
            }
        }
        _ => {}
    }
}

fn span(value: uhura_syntax::ast::Span) -> ast::SourceSpan {
    ast::SourceSpan::new(value.file, value.start, value.end)
}

fn source_name_expression(
    name: String,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::Expression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::ExpressionKind::Name(uhura_syntax::ast::QualifiedName {
            segments: vec![uhura_syntax::ast::Identifier::new(name, span)],
            span,
        }),
        span,
    )
}

#[derive(Clone, Debug)]
struct RecordShape {
    fields: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct ExprEnv {
    values: BTreeMap<String, ast::Expr>,
}

#[derive(Clone, Debug)]
enum PureContinuation {
    Function,
    Shared {
        name: ast::Name,
        span: uhura_syntax::ast::Span,
    },
    LetThen {
        name: uhura_syntax::ast::Identifier,
        ty: Option<uhura_syntax::ast::TypeExpression>,
        remainder: uhura_syntax::ast::Block,
        env: ExprEnv,
        next: Box<PureContinuation>,
        span: uhura_syntax::ast::Span,
    },
    DiscardThen {
        remainder: uhura_syntax::ast::Block,
        env: ExprEnv,
        next: Box<PureContinuation>,
        span: uhura_syntax::ast::Span,
    },
    Unary {
        operator: uhura_syntax::ast::UnaryOperator,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    BinaryLeft {
        operator: uhura_syntax::ast::BinaryOperator,
        right: uhura_syntax::ast::Expression,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    ShortCircuitLeft {
        operator: uhura_syntax::ast::BinaryOperator,
        right: uhura_syntax::ast::Expression,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    BinaryRight {
        operator: uhura_syntax::ast::BinaryOperator,
        left: ast::Expr,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    CompareLeft {
        operator: uhura_syntax::ast::ComparisonOperator,
        right: uhura_syntax::ast::Expression,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    CompareRight {
        operator: uhura_syntax::ast::ComparisonOperator,
        left: ast::Expr,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    Member {
        member: uhura_syntax::ast::Identifier,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    IndexValue {
        index: uhura_syntax::ast::Expression,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    IndexIndex {
        value: ast::Expr,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    Is {
        pattern: uhura_syntax::ast::Pattern,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    Sequence {
        tuple: bool,
        completed: Vec<ast::Expr>,
        remaining: Vec<uhura_syntax::ast::Expression>,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    IfCondition {
        expression: uhura_syntax::ast::IfExpression,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    MatchSubject {
        expression: uhura_syntax::ast::MatchExpression,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    CallCallee {
        arguments: Vec<uhura_syntax::ast::CallArgument>,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    CallArgument {
        callee: uhura_syntax::ast::Expression,
        arguments: Vec<uhura_syntax::ast::CallArgument>,
        index: usize,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    RecordField {
        record: uhura_syntax::ast::RecordExpression,
        index: usize,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
    RecordBase {
        record: uhura_syntax::ast::RecordExpression,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        next: Box<PureContinuation>,
    },
}

struct ProjectAdapter<'a> {
    sources: &'a [uhura_syntax::ast::Module],
    shape_sources: &'a [uhura_syntax::ast::Module],
    resolution: Resolution,
    diagnostics: Vec<uhura_base::Diagnostic>,
    sort_declarations: bool,
}

impl<'a> ProjectAdapter<'a> {
    fn new(sources: &'a [uhura_syntax::ast::Module], mut resolution: Resolution) -> Self {
        Self {
            sources,
            shape_sources: sources,
            diagnostics: std::mem::take(&mut resolution.diagnostics),
            resolution,
            sort_declarations: true,
        }
    }

    fn new_with_shapes(
        sources: &'a [uhura_syntax::ast::Module],
        shape_sources: &'a [uhura_syntax::ast::Module],
        mut resolution: Resolution,
    ) -> Self {
        Self {
            sources,
            shape_sources,
            diagnostics: std::mem::take(&mut resolution.diagnostics),
            resolution,
            sort_declarations: false,
        }
    }

    fn adapt(&mut self) -> ast::Project {
        let Some(first) = self.sources.first() else {
            return ast::Project {
                modules: Vec::new(),
            };
        };
        let (package_path, major) = package_identity(first, &mut self.diagnostics);
        let (mut structs, mut variants) = project_record_shapes(self.shape_sources);
        structs.extend(self.resolution.external_structs.clone());
        variants.extend(self.resolution.external_variants.clone());

        let mut ordered = self.sources.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.identity
                .module
                .as_bytes()
                .cmp(right.identity.module.as_bytes())
                .then_with(|| left.identity.file.cmp(&right.identity.file))
        });
        let mut declarations = Vec::new();
        for source in ordered {
            let bindings = self
                .resolution
                .modules
                .get(&source.identity.module)
                .map(|value| value.bindings.clone())
                .unwrap_or_default();
            let private_functions = source
                .declarations
                .iter()
                .filter_map(|declaration| match &declaration.kind {
                    uhura_syntax::ast::DeclarationKind::Function(value)
                        if value.visibility == uhura_syntax::ast::Visibility::Private =>
                    {
                        Some((value.name.text.clone(), value.clone()))
                    }
                    _ => None,
                })
                .collect();
            let mut scoped_structs = structs.clone();
            for (authored, lowered) in &bindings {
                if let Some(shape) = structs.get(authored) {
                    scoped_structs.insert(lowered.clone(), shape.clone());
                }
            }
            let mut scoped_variants = variants.clone();
            for ((owner, constructor), shape) in &variants {
                if let Some(lowered) = bindings.get(owner) {
                    scoped_variants.insert((lowered.clone(), constructor.clone()), shape.clone());
                }
            }
            let mut adapter = Adapter::new(
                source,
                bindings,
                scoped_structs,
                scoped_variants,
                private_functions,
            );
            declarations.extend(
                source
                    .declarations
                    .iter()
                    .filter_map(|declaration| adapter.declaration(declaration)),
            );
            self.diagnostics.extend(adapter.diagnostics);
        }
        if self.sort_declarations {
            declarations.sort_by(|left, right| {
                lowered_declaration_key(left)
                    .as_bytes()
                    .cmp(lowered_declaration_key(right).as_bytes())
                    .then_with(|| {
                        lowered_declaration_rank(left).cmp(&lowered_declaration_rank(right))
                    })
            });
        }

        // One package-global checker module is the deliberate lowering target.
        // Its source coordinates are synthetic and stable: every authored node
        // keeps its own exact file/span, while no source locator becomes a
        // semantic owner or public ID.
        let synthetic = ast::SourceSpan::empty(0, 0);
        let uses = ui::profile_activation(self.sources, &mut self.diagnostics);
        let mut imports = checker_standard_imports(&self.resolution.standard_imports);
        imports.extend(checker_external_imports(&self.resolution.external_imports));
        imports.sort_by(|left, right| {
            left.target
                .as_bytes()
                .cmp(right.target.as_bytes())
                .then_with(|| {
                    left.names[0]
                        .value
                        .as_bytes()
                        .cmp(right.names[0].value.as_bytes())
                })
        });
        ast::Project {
            modules: vec![ast::Module {
                source_id: ast::SourceId {
                    file: 0,
                    path: "<resolved-project>".into(),
                },
                span: synthetic,
                language: ast::LanguageHeader {
                    name: ast::Spanned::new("uhura".into(), synthetic),
                    version: "0.4".into(),
                    span: synthetic,
                },
                identity: ast::ModuleIdentity {
                    path: package_path
                        .into_iter()
                        .map(|part| ast::Spanned::new(part, synthetic))
                        .collect(),
                    major,
                    span: synthetic,
                },
                uses,
                imports,
                declarations,
            }],
        }
    }
}

fn checker_standard_imports(
    imports: &BTreeMap<(String, String), uhura_syntax::ast::Span>,
) -> Vec<ast::ImportDecl> {
    let mut grouped = BTreeMap::<&str, Vec<(&str, uhura_syntax::ast::Span)>>::new();
    for ((target, name), import_span) in imports {
        grouped
            .entry(target)
            .or_default()
            .push((name, *import_span));
    }
    grouped
        .into_iter()
        .map(|(target, values)| {
            let source_span = span(values[0].1);
            let (logical, major) = target
                .rsplit_once('@')
                .expect("standard module targets carry an exact major");
            ast::ImportDecl {
                names: values
                    .into_iter()
                    .map(|(name, import_span)| ast::Spanned::new(name.into(), span(import_span)))
                    .collect(),
                target: target.into(),
                identity: ast::ModuleIdentity {
                    path: logical
                        .split('.')
                        .map(|segment| ast::Spanned::new(segment.into(), source_span))
                        .collect(),
                    major: major.into(),
                    span: source_span,
                },
                target_span: source_span,
                span: source_span,
            }
        })
        .collect()
}

fn checker_external_imports(
    imports: &BTreeMap<(String, String), ExternalImport>,
) -> Vec<ast::ImportDecl> {
    let mut grouped = BTreeMap::<&str, Vec<(&str, uhura_syntax::ast::Span)>>::new();
    for ((target, name), import) in imports {
        grouped.entry(target).or_default().push((name, import.span));
    }
    grouped
        .into_iter()
        .map(|(target, mut values)| {
            values.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            let source_span = span(values[0].1);
            let (logical, major) = target
                .rsplit_once('@')
                .expect("resolved package IDs carry an exact compatibility version");
            ast::ImportDecl {
                names: values
                    .into_iter()
                    .map(|(name, import_span)| ast::Spanned::new(name.into(), span(import_span)))
                    .collect(),
                target: target.into(),
                identity: ast::ModuleIdentity {
                    path: logical
                        .split('.')
                        .map(|segment| ast::Spanned::new(segment.into(), source_span))
                        .collect(),
                    major: major.into(),
                    span: source_span,
                },
                target_span: source_span,
                span: source_span,
            }
        })
        .collect()
}

fn project_record_shapes(
    sources: &[uhura_syntax::ast::Module],
) -> (
    BTreeMap<String, RecordShape>,
    BTreeMap<(String, String), RecordShape>,
) {
    let mut structs = BTreeMap::new();
    let mut variants = BTreeMap::new();
    for source in sources {
        for declaration in &source.declarations {
            match &declaration.kind {
                uhura_syntax::ast::DeclarationKind::Struct(value) => {
                    structs.insert(
                        value.name.text.clone(),
                        RecordShape {
                            fields: value
                                .fields
                                .iter()
                                .map(|field| field.name.text.clone())
                                .collect(),
                        },
                    );
                }
                uhura_syntax::ast::DeclarationKind::Enum(value) => {
                    for variant in &value.variants {
                        variants.insert(
                            (value.name.text.clone(), variant.name.text.clone()),
                            RecordShape {
                                fields: variant
                                    .fields
                                    .iter()
                                    .map(|field| field.name.text.clone())
                                    .collect(),
                            },
                        );
                    }
                }
                uhura_syntax::ast::DeclarationKind::Machine(value) => {
                    for member in &value.members {
                        match &member.kind {
                            uhura_syntax::ast::MachineMemberKind::Events(section)
                            | uhura_syntax::ast::MachineMemberKind::Commands(section) => {
                                capture_protocol_shapes(
                                    &value.name.text,
                                    &section.variants,
                                    &mut variants,
                                );
                            }
                            uhura_syntax::ast::MachineMemberKind::Outcomes(section) => {
                                let entries = section
                                    .entries
                                    .iter()
                                    .map(|entry| &entry.variant)
                                    .collect::<Vec<_>>();
                                capture_protocol_shapes(&value.name.text, entries, &mut variants);
                            }
                            _ => {}
                        }
                    }
                }
                uhura_syntax::ast::DeclarationKind::Part(value) => {
                    for member in &value.members {
                        match &member.kind {
                            uhura_syntax::ast::PartMemberKind::Events(section)
                            | uhura_syntax::ast::PartMemberKind::Commands(section) => {
                                capture_protocol_shapes(
                                    &value.name.text,
                                    &section.variants,
                                    &mut variants,
                                );
                            }
                            uhura_syntax::ast::PartMemberKind::RequiresOutcomes(section) => {
                                let entries = section
                                    .entries
                                    .iter()
                                    .map(|entry| &entry.variant)
                                    .collect::<Vec<_>>();
                                capture_protocol_shapes(&value.name.text, entries, &mut variants);
                            }
                            _ => {}
                        }
                    }
                }
                uhura_syntax::ast::DeclarationKind::Ui(_) => {}
                _ => {}
            }
        }
    }
    (structs, variants)
}

fn capture_protocol_shapes<'a>(
    owner: &str,
    variants: impl IntoIterator<Item = &'a uhura_syntax::ast::ProtocolVariant>,
    shapes: &mut BTreeMap<(String, String), RecordShape>,
) {
    for variant in variants {
        shapes.insert(
            (owner.to_owned(), variant.name.text.clone()),
            RecordShape {
                fields: variant
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.text.clone())
                    .collect(),
            },
        );
    }
}

fn package_identity(
    source: &uhura_syntax::ast::Module,
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) -> (Vec<String>, String) {
    let Some((name, major)) = source.identity.package.rsplit_once('@') else {
        diagnostics.push(error(
            codes::UNSUPPORTED,
            "uhura-0.4/invalid-package-identity",
            "resolved 0.4 source identity must contain an exact package ID such as `examples.programs@1`",
            span(source.span),
        ));
        return (vec![source.identity.package.clone()], "0".into());
    };
    let valid_name = !name.is_empty()
        && name.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && segment
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_lowercase)
                && segment
                    .as_bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        });
    let valid_major = major
        .parse::<u64>()
        .is_ok_and(|value| value > 0 && !major.starts_with('0'));
    if !valid_name || !valid_major {
        diagnostics.push(error(
            codes::UNSUPPORTED,
            "uhura-0.4/invalid-package-identity",
            format!(
                "invalid resolved package identity `{}`; expected lowercase dotted kebab-case plus a positive compatibility version",
                source.identity.package
            ),
            span(source.span),
        ));
    }
    (
        name.split('.').map(str::to_owned).collect(),
        major.to_owned(),
    )
}

fn lowered_declaration_key(declaration: &ast::Declaration) -> &str {
    match &declaration.value {
        ast::DeclarationKind::Const(value) => &value.name.value,
        ast::DeclarationKind::Key(value) => &value.name.value,
        ast::DeclarationKind::Type(value) => &value.name.value,
        ast::DeclarationKind::Function(value) => &value.name.value,
        ast::DeclarationKind::Machine(value) => &value.name.value,
        ast::DeclarationKind::Ui(value) => &value.name.value,
        ast::DeclarationKind::Scenario(value) => &value.name.value,
        ast::DeclarationKind::Example(value) | ast::DeclarationKind::Checkpoint(value) => {
            &value.name.value
        }
    }
}

fn lowered_declaration_name_mut(declaration: &mut ast::DeclarationKind) -> &mut String {
    match declaration {
        ast::DeclarationKind::Const(value) => &mut value.name.value,
        ast::DeclarationKind::Key(value) => &mut value.name.value,
        ast::DeclarationKind::Type(value) => &mut value.name.value,
        ast::DeclarationKind::Function(value) => &mut value.name.value,
        ast::DeclarationKind::Machine(value) => &mut value.name.value,
        ast::DeclarationKind::Ui(value) => &mut value.name.value,
        ast::DeclarationKind::Scenario(value) => &mut value.name.value,
        ast::DeclarationKind::Example(value) | ast::DeclarationKind::Checkpoint(value) => {
            &mut value.name.value
        }
    }
}

fn lowered_declaration_rank(declaration: &ast::Declaration) -> u8 {
    match declaration.value {
        ast::DeclarationKind::Type(_) => 0,
        ast::DeclarationKind::Key(_) => 1,
        ast::DeclarationKind::Const(_) => 2,
        ast::DeclarationKind::Function(_) => 3,
        ast::DeclarationKind::Machine(_) => 4,
        ast::DeclarationKind::Ui(_) => 5,
        ast::DeclarationKind::Scenario(_) => 6,
        ast::DeclarationKind::Example(_) => 7,
        ast::DeclarationKind::Checkpoint(_) => 8,
    }
}

fn port_order_key(name: &str) -> (u8, &str, &str) {
    match name.split_once('.') {
        Some((owner, port)) => (1, owner, port),
        None => (0, "", name),
    }
}

fn machine_non_port_value_names(
    machine: &uhura_syntax::ast::MachineDeclaration,
) -> BTreeSet<String> {
    machine
        .members
        .iter()
        .flat_map(|member| match &member.kind {
            uhura_syntax::ast::MachineMemberKind::Config(value) => value
                .fields
                .iter()
                .map(|field| field.name.text.clone())
                .collect::<Vec<_>>(),
            uhura_syntax::ast::MachineMemberKind::Const(value) => vec![value.name.text.clone()],
            uhura_syntax::ast::MachineMemberKind::Function(value) => vec![value.name.text.clone()],
            uhura_syntax::ast::MachineMemberKind::State(value) => value
                .fields
                .iter()
                .map(|field| field.name.text.clone())
                .collect(),
            uhura_syntax::ast::MachineMemberKind::Computed(value) => vec![value.name.text.clone()],
            uhura_syntax::ast::MachineMemberKind::Update(value) => vec![value.name.text.clone()],
            _ => Vec::new(),
        })
        .collect()
}

fn lower_protocol_variant(value: &str) -> String {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_lowercase().chain(characters).collect::<String>()
}

struct Adapter<'a> {
    source: &'a uhura_syntax::ast::Module,
    diagnostics: Vec<uhura_base::Diagnostic>,
    bindings: BTreeMap<String, String>,
    structs: BTreeMap<String, RecordShape>,
    variants: BTreeMap<(String, String), RecordShape>,
    private_functions: BTreeMap<String, uhura_syntax::ast::FunctionDeclaration>,
    inlining: Vec<String>,
    inline_private_calls: bool,
    pure_temporary: u64,
}

impl<'a> Adapter<'a> {
    fn new(
        source: &'a uhura_syntax::ast::Module,
        bindings: BTreeMap<String, String>,
        structs: BTreeMap<String, RecordShape>,
        variants: BTreeMap<(String, String), RecordShape>,
        private_functions: BTreeMap<String, uhura_syntax::ast::FunctionDeclaration>,
    ) -> Self {
        Self {
            source,
            diagnostics: Vec::new(),
            bindings,
            structs,
            variants,
            private_functions,
            inlining: Vec::new(),
            inline_private_calls: false,
            pure_temporary: 0,
        }
    }

    fn declaration(
        &mut self,
        declaration: &uhura_syntax::ast::Declaration,
    ) -> Option<ast::Declaration> {
        // Generated pure-local ordinals are semantic only within one source
        // declaration. Reordering independent declarations must not perturb
        // checked IR identity.
        self.pure_temporary = 0;
        let span = self.span(declaration.span);
        let mut value = match &declaration.kind {
            uhura_syntax::ast::DeclarationKind::Struct(value) => {
                ast::DeclarationKind::Type(self.struct_declaration(value))
            }
            uhura_syntax::ast::DeclarationKind::Enum(value) => {
                ast::DeclarationKind::Type(self.enum_declaration(value))
            }
            uhura_syntax::ast::DeclarationKind::Key(value) => {
                ast::DeclarationKind::Key(ast::KeyDecl {
                    name: self.name(&value.name),
                    over: self.ty(&value.value),
                })
            }
            uhura_syntax::ast::DeclarationKind::Const(value) => {
                ast::DeclarationKind::Const(self.const_declaration(value))
            }
            uhura_syntax::ast::DeclarationKind::Function(value) => {
                ast::DeclarationKind::Function(self.function_declaration(value))
            }
            uhura_syntax::ast::DeclarationKind::Machine(value) => {
                ast::DeclarationKind::Machine(self.machine_declaration(value))
            }
            uhura_syntax::ast::DeclarationKind::Part(_) => {
                self.unsupported(
                    declaration.span,
                    "part composition must run before core lowering",
                );
                return None;
            }
            uhura_syntax::ast::DeclarationKind::Ui(value) => {
                ast::DeclarationKind::Ui(self.ui_declaration(value))
            }
            uhura_syntax::ast::DeclarationKind::Scenario(value) => {
                ast::DeclarationKind::Scenario(self.scenario_declaration(value))
            }
            uhura_syntax::ast::DeclarationKind::Example(value) => {
                ast::DeclarationKind::Example(self.evidence_alias(value))
            }
            uhura_syntax::ast::DeclarationKind::Checkpoint(value) => {
                ast::DeclarationKind::Checkpoint(self.evidence_alias(value))
            }
        };
        let (authored_name, _) = declaration_header(declaration);
        if let Some(lowered_name) = self.bindings.get(&authored_name.text) {
            *lowered_declaration_name_mut(&mut value) = lowered_name.clone();
        }
        Some(ast::Spanned::new(value, span))
    }

    fn scenario_declaration(
        &mut self,
        value: &uhura_syntax::ast::ScenarioDeclaration,
    ) -> ast::ScenarioDecl {
        let origin = match &value.origin {
            uhura_syntax::ast::ScenarioOrigin::Machine {
                machine,
                configuration,
            } => {
                let name = machine
                    .segments
                    .last()
                    .map(|segment| self.resolved_name(&segment.name))
                    .unwrap_or_else(|| {
                        self.unsupported(machine.span, "scenario machine path must contain a name");
                        ast::Spanned::new("<error>".into(), self.span(machine.span))
                    });
                ast::ScenarioOrigin::Machine {
                    machine: name,
                    configuration: configuration
                        .as_ref()
                        .map(|value| self.expr(value, &ExprEnv::default())),
                }
            }
            uhura_syntax::ast::ScenarioOrigin::Snapshot(reference) => {
                ast::ScenarioOrigin::Snapshot(self.evidence_reference(reference))
            }
        };
        ast::ScenarioDecl {
            name: self.name(&value.name),
            origin,
            steps: value
                .steps
                .iter()
                .map(|step| ast::Spanned::new(self.evidence_step(step), self.span(step.span)))
                .collect(),
        }
    }

    fn evidence_alias(
        &mut self,
        value: &uhura_syntax::ast::EvidenceAliasDeclaration,
    ) -> ast::EvidenceAliasDecl {
        ast::EvidenceAliasDecl {
            name: self.name(&value.name),
            presentation: value
                .presentation
                .as_ref()
                .map(|name| self.resolved_name(name)),
            kind: value.kind.map(|kind| match kind {
                uhura_syntax::ast::EvidencePresentationKind::Page => {
                    ast::EvidencePresentationKind::Page
                }
                uhura_syntax::ast::EvidencePresentationKind::Component => {
                    ast::EvidencePresentationKind::Component
                }
                uhura_syntax::ast::EvidencePresentationKind::Surface => {
                    ast::EvidencePresentationKind::Surface
                }
            }),
            is_default: value.is_default,
            note: value.note.clone(),
            target: self.evidence_reference(&value.target),
        }
    }

    fn evidence_reference(&self, value: &uhura_syntax::ast::EvidenceReference) -> ast::EvidenceRef {
        ast::EvidenceRef {
            path: self.resolved_path(&value.path),
            span: self.span(value.span),
        }
    }

    fn evidence_step(&mut self, value: &uhura_syntax::ast::EvidenceStep) -> ast::EvidenceStepKind {
        match &value.kind {
            uhura_syntax::ast::EvidenceStepKind::Bind { port, fixture } => {
                ast::EvidenceStepKind::Bind {
                    port: self.name(port),
                    fixture: self.expr(fixture, &ExprEnv::default()),
                }
            }
            uhura_syntax::ast::EvidenceStepKind::Start => ast::EvidenceStepKind::Start,
            uhura_syntax::ast::EvidenceStepKind::Send(value) => {
                ast::EvidenceStepKind::Send(self.expr(value, &ExprEnv::default()))
            }
            uhura_syntax::ast::EvidenceStepKind::Deliver(value) => {
                ast::EvidenceStepKind::Deliver(self.expr(value, &ExprEnv::default()))
            }
            uhura_syntax::ast::EvidenceStepKind::ExpectReaction { outcome, commands } => {
                ast::EvidenceStepKind::ExpectReaction {
                    outcome: self.pattern(outcome),
                    commands: commands
                        .iter()
                        .map(|value| self.expr(value, &ExprEnv::default()))
                        .collect(),
                }
            }
            uhura_syntax::ast::EvidenceStepKind::ExpectObservationPattern(value) => {
                ast::EvidenceStepKind::ExpectObservationPattern(self.pattern(value))
            }
            uhura_syntax::ast::EvidenceStepKind::ExpectInspectionPattern(value) => {
                ast::EvidenceStepKind::ExpectInspectionPattern(self.pattern(value))
            }
            uhura_syntax::ast::EvidenceStepKind::ExpectObservationWhere(value) => {
                ast::EvidenceStepKind::ExpectObservationWhere(self.expr(value, &ExprEnv::default()))
            }
            uhura_syntax::ast::EvidenceStepKind::ExpectRestore { commands } => {
                ast::EvidenceStepKind::ExpectRestore {
                    commands: commands
                        .iter()
                        .map(|value| self.expr(value, &ExprEnv::default()))
                        .collect(),
                }
            }
            uhura_syntax::ast::EvidenceStepKind::ExpectSnapshot { target } => {
                ast::EvidenceStepKind::ExpectSnapshot {
                    target: self.evidence_reference(target),
                }
            }
            uhura_syntax::ast::EvidenceStepKind::Pin(value) => {
                ast::EvidenceStepKind::Pin(self.name(value))
            }
        }
    }

    fn struct_declaration(
        &mut self,
        value: &uhura_syntax::ast::StructDeclaration,
    ) -> ast::TypeDecl {
        let span = self.span(value.name.span);
        ast::TypeDecl {
            name: self.name(&value.name),
            parameters: Vec::new(),
            body: ast::TypeBody::Alias(ast::Spanned::new(
                ast::TypeExprKind::Record(
                    value
                        .fields
                        .iter()
                        .map(|field| self.type_field(field))
                        .collect(),
                ),
                span,
            )),
        }
    }

    fn enum_declaration(&mut self, value: &uhura_syntax::ast::EnumDeclaration) -> ast::TypeDecl {
        let span = self.span(value.name.span);
        ast::TypeDecl {
            name: self.name(&value.name),
            parameters: Vec::new(),
            body: ast::TypeBody::Sum(ast::ClosedSum {
                variants: value
                    .variants
                    .iter()
                    .map(|variant| ast::Variant {
                        name: self.name(&variant.name),
                        payload: if variant.fields.is_empty() {
                            ast::VariantPayload::Unit
                        } else {
                            ast::VariantPayload::Named(
                                variant
                                    .fields
                                    .iter()
                                    .map(|field| self.type_field(field))
                                    .collect(),
                            )
                        },
                        span: self.span(variant.span),
                    })
                    .collect(),
                leading_pipe: false,
                span,
            }),
        }
    }

    fn const_declaration(&mut self, value: &uhura_syntax::ast::ConstDeclaration) -> ast::ConstDecl {
        ast::ConstDecl {
            name: self.name(&value.name),
            ty: self.ty(&value.ty),
            value: self.expr(&value.value, &ExprEnv::default()),
        }
    }

    fn function_declaration(
        &mut self,
        value: &uhura_syntax::ast::FunctionDeclaration,
    ) -> ast::FunctionDecl {
        ast::FunctionDecl {
            name: self.name(&value.name),
            parameters: value
                .parameters
                .iter()
                .map(|parameter| self.parameter(parameter))
                .collect(),
            result: self.ty(&value.result),
            body: self.pure_block_expression(&value.body, &ExprEnv::default()),
        }
    }

    fn machine_declaration(
        &mut self,
        value: &uhura_syntax::ast::MachineDeclaration,
    ) -> ast::MachineDecl {
        let mut members = Vec::new();
        let mut has_events = false;
        let mut has_nonempty_events = false;
        let mut has_commands = false;
        let mut has_outcomes = false;
        let mut requires_outcome = false;
        let mut has_state = false;
        let mut has_observe = false;

        let mut ports = value
            .members
            .iter()
            .filter_map(|member| match &member.kind {
                uhura_syntax::ast::MachineMemberKind::Port(port) => Some((port, member.span)),
                _ => None,
            })
            .collect::<Vec<_>>();
        ports.sort_by(|(left, _), (right, _)| {
            port_order_key(&left.name.text).cmp(&port_order_key(&right.name.text))
        });
        let non_port_values = machine_non_port_value_names(value);
        let mut seen_ports = BTreeSet::new();
        for (port, member_span) in ports {
            if !seen_ports.insert(port.name.text.clone()) {
                self.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura-0.4/duplicate-port",
                    format!("port `{}` is declared more than once", port.name.text),
                    self.span(port.name.span),
                ));
                continue;
            }
            if non_port_values.contains(&port.name.text) {
                self.diagnostics.push(error(
                    codes::DUPLICATE,
                    "uhura-0.4/port-value-collision",
                    format!(
                        "port `{}` collides with another value owned by the same machine or part",
                        port.name.text
                    ),
                    self.span(port.name.span),
                ));
            }
            members.push(ast::Spanned::new(
                ast::MachineMemberKind::Port(self.port_declaration(port)),
                self.span(member_span),
            ));
        }

        for member in &value.members {
            // Machine member placement is nonsemantic. Keep compiler-private
            // pure names stable within each independently checked member.
            self.pure_temporary = 0;
            let lowered = match &member.kind {
                uhura_syntax::ast::MachineMemberKind::Config(section) => {
                    ast::MachineMemberKind::Config(ast::FieldBlock {
                        fields: section
                            .fields
                            .iter()
                            .map(|field| self.type_field(field))
                            .collect(),
                    })
                }
                uhura_syntax::ast::MachineMemberKind::Require(requirement) => {
                    ast::MachineMemberKind::Require(
                        self.expr(&requirement.condition, &ExprEnv::default()),
                    )
                }
                uhura_syntax::ast::MachineMemberKind::Const(declaration) => {
                    ast::MachineMemberKind::Const(self.const_declaration(declaration))
                }
                uhura_syntax::ast::MachineMemberKind::Function(declaration) => {
                    ast::MachineMemberKind::Function(self.function_declaration(declaration))
                }
                uhura_syntax::ast::MachineMemberKind::Part(_) => {
                    self.unsupported(
                        member.span,
                        "part instance composition must run before core lowering",
                    );
                    continue;
                }
                uhura_syntax::ast::MachineMemberKind::Events(section) => {
                    has_events = true;
                    has_nonempty_events |= !section.variants.is_empty();
                    ast::MachineMemberKind::Input(self.protocol_domain(section))
                }
                uhura_syntax::ast::MachineMemberKind::Commands(section) => {
                    has_commands = true;
                    ast::MachineMemberKind::Command(self.protocol_domain(section))
                }
                uhura_syntax::ast::MachineMemberKind::Port(_) => continue,
                uhura_syntax::ast::MachineMemberKind::Outcomes(section) => {
                    has_outcomes = true;
                    ast::MachineMemberKind::Outcome(self.outcome_declaration(section))
                }
                uhura_syntax::ast::MachineMemberKind::State(section) => {
                    has_state = true;
                    ast::MachineMemberKind::State(ast::StateDecl {
                        fields: section
                            .fields
                            .iter()
                            .map(|field| ast::InitializedField {
                                name: self.name(&field.name),
                                ty: self.ty(&field.ty),
                                value: self.initializer_expr(&field.initial),
                                span: self.span(field.span),
                            })
                            .collect(),
                    })
                }
                uhura_syntax::ast::MachineMemberKind::Computed(declaration) => {
                    ast::MachineMemberKind::Derive(ast::DeriveDecl {
                        name: self.name(&declaration.name),
                        ty: declaration.ty.as_ref().map(|ty| self.ty(ty)),
                        value: self.expr(&declaration.value, &ExprEnv::default()),
                    })
                }
                uhura_syntax::ast::MachineMemberKind::Invariant(declaration) => {
                    ast::MachineMemberKind::Invariant(ast::InvariantDecl {
                        expressions: declaration
                            .conditions
                            .iter()
                            .map(|value| self.expr(value, &ExprEnv::default()))
                            .collect(),
                        braced: declaration.grouped,
                    })
                }
                uhura_syntax::ast::MachineMemberKind::Observe(section) => {
                    has_observe = true;
                    ast::MachineMemberKind::Observe(ast::ObserveDecl {
                        fields: section
                            .fields
                            .iter()
                            .map(|field| {
                                let value = if let Some(value) = &field.value {
                                    self.expr(value, &ExprEnv::default())
                                } else {
                                    ast::Spanned::new(
                                        ast::ExprKind::Name(self.name(&field.name)),
                                        self.span(field.name.span),
                                    )
                                };
                                ast::ObserveField {
                                    name: self.name(&field.name),
                                    ty: None,
                                    value,
                                    span: self.span(field.span),
                                }
                            })
                            .collect(),
                    })
                }
                uhura_syntax::ast::MachineMemberKind::Handler(declaration) => {
                    requires_outcome = true;
                    ast::MachineMemberKind::Handler(self.handler_declaration(declaration))
                }
                uhura_syntax::ast::MachineMemberKind::Update(declaration) => {
                    requires_outcome = true;
                    ast::MachineMemberKind::Transition(ast::TransitionDecl {
                        name: self.name(&declaration.name),
                        parameters: declaration
                            .parameters
                            .iter()
                            .map(|parameter| self.parameter(parameter))
                            .collect(),
                        body: self.reaction_block(&declaration.body, &ExprEnv::default(), true),
                    })
                }
                uhura_syntax::ast::MachineMemberKind::BeforeCommit(declaration) => {
                    requires_outcome = true;
                    ast::MachineMemberKind::BeforeCommit(self.reaction_block(
                        &declaration.body,
                        &ExprEnv::default(),
                        false,
                    ))
                }
            };
            members.push(ast::Spanned::new(lowered, self.span(member.span)));
        }

        let synthetic = ast::SourceSpan::empty(self.source.identity.file, value.name.span.end);
        if !has_events {
            members.push(ast::Spanned::new(
                ast::MachineMemberKind::Input(ast::SumDomain::Never(ast::Spanned::new(
                    "Never".into(),
                    synthetic,
                ))),
                synthetic,
            ));
        }
        if !has_commands {
            members.push(ast::Spanned::new(
                ast::MachineMemberKind::Command(ast::SumDomain::Never(ast::Spanned::new(
                    "Never".into(),
                    synthetic,
                ))),
                synthetic,
            ));
        }
        if !has_outcomes {
            if has_nonempty_events || requires_outcome {
                self.unsupported(
                    value.name.span,
                    format!(
                        "machine `{}` may omit outcomes only when its complete input sum is empty and no source form requires an outcome",
                        value.name.text
                    ),
                );
            }
            members.push(ast::Spanned::new(
                ast::MachineMemberKind::Outcome(ast::OutcomeDecl {
                    variants: Vec::new(),
                    leading_pipe: false,
                }),
                synthetic,
            ));
        }
        if !has_state {
            members.push(ast::Spanned::new(
                ast::MachineMemberKind::State(ast::StateDecl { fields: Vec::new() }),
                synthetic,
            ));
        }
        if !has_observe {
            members.push(ast::Spanned::new(
                ast::MachineMemberKind::Observe(ast::ObserveDecl { fields: Vec::new() }),
                synthetic,
            ));
        }

        ast::MachineDecl {
            name: self.name(&value.name),
            members,
        }
    }

    fn protocol_domain(&mut self, value: &uhura_syntax::ast::ProtocolSection) -> ast::SumDomain {
        if value.variants.is_empty() {
            return ast::SumDomain::Never(ast::Spanned::new(
                "Never".into(),
                ast::SourceSpan::empty(self.source.identity.file, 0),
            ));
        }
        let span = value
            .variants
            .first()
            .map(|variant| self.span(variant.span))
            .unwrap_or_default();
        ast::SumDomain::Sum(ast::ClosedSum {
            variants: value
                .variants
                .iter()
                .map(|variant| self.protocol_variant(variant))
                .collect(),
            leading_pipe: false,
            span,
        })
    }

    fn protocol_variant(&mut self, value: &uhura_syntax::ast::ProtocolVariant) -> ast::Variant {
        ast::Variant {
            name: self.name(&value.name),
            payload: if value.parameters.is_empty() {
                ast::VariantPayload::Unit
            } else {
                ast::VariantPayload::Named(
                    value
                        .parameters
                        .iter()
                        .map(|parameter| ast::TypeField {
                            name: self.name(&parameter.name),
                            ty: self.ty(&parameter.ty),
                            span: self.span(parameter.span),
                        })
                        .collect(),
                )
            },
            span: self.span(value.span),
        }
    }

    fn port_declaration(&mut self, value: &uhura_syntax::ast::PortDeclaration) -> ast::PortDecl {
        let contract_name = value
            .contract
            .segments
            .last()
            .map(|segment| self.resolved_text(&segment.name.text))
            .unwrap_or("");
        let expected_configuration = match contract_name {
            "Router" => Some("routes"),
            "Observation" | "RequestPort" | "SinkPort" => None,
            _ => value.fields.first().map(|field| field.name.text.as_str()),
        };
        let valid_configuration = match expected_configuration {
            Some(expected) => value.fields.len() == 1 && value.fields[0].name.text == expected,
            None => value.fields.is_empty(),
        };
        if !valid_configuration {
            let expected = expected_configuration.map_or_else(
                || "an empty binding `{}`".to_owned(),
                |field| format!("exactly `{{ {field}: ... }}`"),
            );
            self.diagnostics.push(error(
                codes::PORT,
                "uhura-0.4/port-configuration",
                format!("port contract `{contract_name}` requires {expected}"),
                self.span(value.contract.span),
            ));
        }

        let contract = uhura_syntax::ast::Node::new(
            uhura_syntax::ast::TypeExpressionKind::Path(value.contract.clone()),
            value.contract.span,
        );
        ast::PortDecl {
            name: self.name(&value.name),
            contract: self.ty(&contract),
            configuration: value
                .fields
                .iter()
                .map(|field| match &field.value {
                    Some(expression) => self.expr(expression, &ExprEnv::default()),
                    None => ast::Spanned::new(
                        ast::ExprKind::Name(self.name(&field.name)),
                        self.span(field.span),
                    ),
                })
                .collect(),
        }
    }

    fn outcome_declaration(
        &mut self,
        value: &uhura_syntax::ast::OutcomeSection,
    ) -> ast::OutcomeDecl {
        ast::OutcomeDecl {
            variants: value
                .entries
                .iter()
                .map(|entry| ast::OutcomeVariant {
                    variant: self.protocol_variant(&entry.variant),
                    policy: ast::Spanned::new(
                        match entry.policy {
                            uhura_syntax::ast::OutcomePolicy::Commit => ast::OutcomePolicy::Commit,
                            uhura_syntax::ast::OutcomePolicy::Abort => ast::OutcomePolicy::Abort,
                        },
                        self.span(entry.span),
                    ),
                    span: self.span(entry.span),
                })
                .collect(),
            leading_pipe: false,
        }
    }

    fn protocol_selector_name(&self, value: &uhura_syntax::ast::ProtocolSelector) -> String {
        value.owner.as_ref().map_or_else(
            || value.variant.text.clone(),
            |owner| {
                format!(
                    "{}.{}",
                    owner.text,
                    lower_protocol_variant(&value.variant.text)
                )
            },
        )
    }

    fn handler_declaration(
        &mut self,
        value: &uhura_syntax::ast::HandlerDeclaration,
    ) -> ast::HandlerDecl {
        let input_name = self.protocol_selector_name(&value.input);
        let input = if value.parameters.is_empty() {
            ast::Spanned::new(
                ast::PatternKind::Name(ast::Spanned::new(input_name, self.span(value.input.span))),
                self.span(value.input.span),
            )
        } else {
            ast::Spanned::new(
                ast::PatternKind::Constructor {
                    path: vec![ast::Spanned::new(input_name, self.span(value.input.span))],
                    arguments: value
                        .parameters
                        .iter()
                        .map(|pattern| self.pattern(pattern))
                        .collect(),
                },
                self.span(value.input.span),
            )
        };
        ast::HandlerDecl {
            input,
            body: if value.body.statements.is_empty()
                && let Some(tail) = &value.body.tail
                && self.is_named_call(tail)
            {
                ast::HandlerBody::Delegate(self.expr(tail, &ExprEnv::default()))
            } else {
                ast::HandlerBody::Block(self.reaction_block(&value.body, &ExprEnv::default(), true))
            },
        }
    }

    fn pure_block_expression(
        &mut self,
        block: &uhura_syntax::ast::Block,
        env: &ExprEnv,
    ) -> ast::Expr {
        if self.block_contains_return(block) {
            return self.pure_flow_block(block, env, PureContinuation::Function);
        }
        self.plain_pure_block_expression(block, env)
    }

    fn plain_pure_block_expression(
        &mut self,
        block: &uhura_syntax::ast::Block,
        env: &ExprEnv,
    ) -> ast::Expr {
        let mut statements = self.statements(block, env, false);
        if let Some(tail) = &block.tail {
            statements.push(ast::Spanned::new(
                ast::StatementKind::Expr(self.expr(tail, env)),
                self.span(tail.span),
            ));
        }
        ast::Spanned::new(
            ast::ExprKind::Block(ast::Block {
                statements,
                span: self.span(block.span),
            }),
            self.span(block.span),
        )
    }

    fn pure_flow_block(
        &mut self,
        block: &uhura_syntax::ast::Block,
        env: &ExprEnv,
        continuation: PureContinuation,
    ) -> ast::Expr {
        self.pure_flow_block_at(block, 0, env, continuation)
    }

    fn pure_flow_block_at(
        &mut self,
        block: &uhura_syntax::ast::Block,
        index: usize,
        env: &ExprEnv,
        continuation: PureContinuation,
    ) -> ast::Expr {
        let Some(statement) = block.statements.get(index) else {
            return if let Some(tail) = block.tail.as_deref() {
                self.pure_flow_expression(tail, env, continuation)
            } else {
                let unit =
                    ast::Spanned::new(ast::ExprKind::Tuple(Vec::new()), self.span(block.span));
                self.apply_pure_continuation(unit, continuation)
            };
        };
        let remainder = uhura_syntax::ast::Block {
            statements: block.statements[index + 1..].to_vec(),
            tail: block.tail.clone(),
            span: block.span,
        };
        match &statement.kind {
            uhura_syntax::ast::StatementKind::Let {
                name, ty, value, ..
            } => self.pure_flow_expression(
                value,
                env,
                PureContinuation::LetThen {
                    name: name.clone(),
                    ty: ty.clone(),
                    remainder,
                    env: env.clone(),
                    next: Box::new(continuation),
                    span: statement.span,
                },
            ),
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => self
                .pure_flow_expression(
                    expression,
                    env,
                    PureContinuation::DiscardThen {
                        remainder,
                        env: env.clone(),
                        next: Box::new(continuation),
                        span: statement.span,
                    },
                ),
            _ => {
                if self.statement_contains_return(statement) {
                    self.unsupported(
                        statement.span,
                        "lexical `return` cannot be nested in an effectful statement of a pure function",
                    );
                }
                let mut lowered = self.statements(
                    &uhura_syntax::ast::Block {
                        statements: vec![statement.clone()],
                        tail: None,
                        span: statement.span,
                    },
                    env,
                    false,
                );
                lowered.push(ast::Spanned::new(
                    ast::StatementKind::Expr(self.pure_flow_block(&remainder, env, continuation)),
                    self.span(remainder.span),
                ));
                ast::Spanned::new(
                    ast::ExprKind::Block(ast::Block {
                        statements: lowered,
                        span: self.span(block.span),
                    }),
                    self.span(block.span),
                )
            }
        }
    }

    fn pure_flow_expression(
        &mut self,
        expression: &uhura_syntax::ast::Expression,
        env: &ExprEnv,
        continuation: PureContinuation,
    ) -> ast::Expr {
        if !self.contains_return(expression) {
            let value = self.expr(expression, env);
            return self.apply_pure_continuation(value, continuation);
        }
        match &expression.kind {
            uhura_syntax::ast::ExpressionKind::Return(value) => {
                let value = value.as_deref().map_or_else(
                    || {
                        uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::ExpressionKind::Unit,
                            expression.span,
                        )
                    },
                    Clone::clone,
                );
                self.pure_flow_expression(&value, env, PureContinuation::Function)
            }
            uhura_syntax::ast::ExpressionKind::Group(value) => {
                self.pure_flow_expression(value, env, continuation)
            }
            uhura_syntax::ast::ExpressionKind::Block(block) => {
                self.pure_flow_block(block, env, continuation)
            }
            uhura_syntax::ast::ExpressionKind::If(value) => self.pure_flow_expression(
                &value.condition,
                env,
                PureContinuation::IfCondition {
                    expression: value.clone(),
                    env: env.clone(),
                    span: expression.span,
                    next: Box::new(continuation),
                },
            ),
            uhura_syntax::ast::ExpressionKind::Match(value) => self.pure_flow_expression(
                &value.value,
                env,
                PureContinuation::MatchSubject {
                    expression: value.clone(),
                    env: env.clone(),
                    span: expression.span,
                    next: Box::new(continuation),
                },
            ),
            uhura_syntax::ast::ExpressionKind::Unary { operator, value } => self
                .pure_flow_expression(
                    value,
                    env,
                    PureContinuation::Unary {
                        operator: *operator,
                        span: expression.span,
                        next: Box::new(continuation),
                    },
                ),
            uhura_syntax::ast::ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                let next = if matches!(
                    operator,
                    uhura_syntax::ast::BinaryOperator::And | uhura_syntax::ast::BinaryOperator::Or
                ) && self.contains_return(right)
                {
                    PureContinuation::ShortCircuitLeft {
                        operator: *operator,
                        right: (**right).clone(),
                        env: env.clone(),
                        span: expression.span,
                        next: Box::new(continuation),
                    }
                } else {
                    PureContinuation::BinaryLeft {
                        operator: *operator,
                        right: (**right).clone(),
                        env: env.clone(),
                        span: expression.span,
                        next: Box::new(continuation),
                    }
                };
                self.pure_flow_expression(left, env, next)
            }
            uhura_syntax::ast::ExpressionKind::Compare {
                operator,
                left,
                right,
            } => self.pure_flow_expression(
                left,
                env,
                PureContinuation::CompareLeft {
                    operator: *operator,
                    right: (**right).clone(),
                    env: env.clone(),
                    span: expression.span,
                    next: Box::new(continuation),
                },
            ),
            uhura_syntax::ast::ExpressionKind::Member { value, member } => self
                .pure_flow_expression(
                    value,
                    env,
                    PureContinuation::Member {
                        member: member.clone(),
                        span: expression.span,
                        next: Box::new(continuation),
                    },
                ),
            uhura_syntax::ast::ExpressionKind::Index { value, index } => self.pure_flow_expression(
                value,
                env,
                PureContinuation::IndexValue {
                    index: (**index).clone(),
                    env: env.clone(),
                    span: expression.span,
                    next: Box::new(continuation),
                },
            ),
            uhura_syntax::ast::ExpressionKind::Is { value, pattern } => self.pure_flow_expression(
                value,
                env,
                PureContinuation::Is {
                    pattern: pattern.clone(),
                    span: expression.span,
                    next: Box::new(continuation),
                },
            ),
            uhura_syntax::ast::ExpressionKind::Sequence(values)
            | uhura_syntax::ast::ExpressionKind::Tuple(values) => {
                let tuple = matches!(expression.kind, uhura_syntax::ast::ExpressionKind::Tuple(_));
                let Some((first, remaining)) = values.split_first() else {
                    let value = ast::Spanned::new(
                        if tuple {
                            ast::ExprKind::Tuple(Vec::new())
                        } else {
                            ast::ExprKind::Sequence(Vec::new())
                        },
                        self.span(expression.span),
                    );
                    return self.apply_pure_continuation(value, continuation);
                };
                self.pure_flow_expression(
                    first,
                    env,
                    PureContinuation::Sequence {
                        tuple,
                        completed: Vec::new(),
                        remaining: remaining.to_vec(),
                        env: env.clone(),
                        span: expression.span,
                        next: Box::new(continuation),
                    },
                )
            }
            uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
                if arguments.iter().any(|argument| {
                    matches!(argument, uhura_syntax::ast::CallArgument::Binder(value) if self.contains_return(&value.body))
                }) {
                    self.unsupported(
                        expression.span,
                        "a non-escaping collection binder cannot use lexical `return`",
                    );
                    return ast::Spanned::new(
                        ast::ExprKind::Error,
                        self.span(expression.span),
                    );
                }
                if self.contains_return(callee) {
                    self.pure_flow_expression(
                        callee,
                        env,
                        PureContinuation::CallCallee {
                            arguments: arguments.clone(),
                            env: env.clone(),
                            span: expression.span,
                            next: Box::new(continuation),
                        },
                    )
                } else {
                    self.pure_flow_call_arguments(
                        (**callee).clone(),
                        arguments.clone(),
                        0,
                        env.clone(),
                        expression.span,
                        continuation,
                    )
                }
            }
            uhura_syntax::ast::ExpressionKind::Record(value) => {
                self.pure_flow_record(value.clone(), 0, env.clone(), expression.span, continuation)
            }
            uhura_syntax::ast::ExpressionKind::AnonymousRecord(_) => {
                self.unsupported(
                    expression.span,
                    "an evidence record cannot contain lexical `return`",
                );
                ast::Spanned::new(ast::ExprKind::Error, self.span(expression.span))
            }
            uhura_syntax::ast::ExpressionKind::Literal(_)
            | uhura_syntax::ast::ExpressionKind::Unit
            | uhura_syntax::ast::ExpressionKind::Name(_) => {
                unreachable!("return-free leaves were lowered before control-flow decomposition")
            }
        }
    }

    fn pure_flow_call_arguments(
        &mut self,
        callee: uhura_syntax::ast::Expression,
        arguments: Vec<uhura_syntax::ast::CallArgument>,
        start: usize,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        continuation: PureContinuation,
    ) -> ast::Expr {
        if let Some((index, value)) =
            arguments
                .iter()
                .enumerate()
                .skip(start)
                .find_map(|(index, argument)| match argument {
                    uhura_syntax::ast::CallArgument::Expression(value)
                        if self.contains_return(value) =>
                    {
                        Some((index, value.clone()))
                    }
                    _ => None,
                })
        {
            return self.pure_flow_expression(
                &value,
                &env.clone(),
                PureContinuation::CallArgument {
                    callee,
                    arguments,
                    index,
                    env,
                    span,
                    next: Box::new(continuation),
                },
            );
        }
        let call = uhura_syntax::ast::Node::new(
            uhura_syntax::ast::ExpressionKind::Call {
                callee: Box::new(callee),
                arguments,
            },
            span,
        );
        let value = self.expr(&call, &env);
        self.apply_pure_continuation(value, continuation)
    }

    fn pure_flow_record(
        &mut self,
        record: uhura_syntax::ast::RecordExpression,
        start: usize,
        env: ExprEnv,
        span: uhura_syntax::ast::Span,
        continuation: PureContinuation,
    ) -> ast::Expr {
        if let Some((index, value)) =
            record
                .fields
                .iter()
                .enumerate()
                .skip(start)
                .find_map(|(index, field)| {
                    field
                        .value
                        .as_ref()
                        .filter(|value| self.contains_return(value))
                        .cloned()
                        .map(|value| (index, value))
                })
        {
            return self.pure_flow_expression(
                &value,
                &env.clone(),
                PureContinuation::RecordField {
                    record,
                    index,
                    env,
                    span,
                    next: Box::new(continuation),
                },
            );
        }
        if let Some(base) = record.base.as_deref().cloned()
            && self.contains_return(&base)
        {
            return self.pure_flow_expression(
                &base,
                &env.clone(),
                PureContinuation::RecordBase {
                    record,
                    env,
                    span,
                    next: Box::new(continuation),
                },
            );
        }
        let value = self.record_expression(span, &record, &env);
        self.apply_pure_continuation(value, continuation)
    }

    fn share_pure_continuation(
        &mut self,
        continuation: PureContinuation,
        span: uhura_syntax::ast::Span,
    ) -> (Option<ast::Statement>, PureContinuation) {
        if matches!(&continuation, PureContinuation::Shared { .. }) {
            return (None, continuation);
        }

        let source_span = self.span(span);
        let ordinal = self.pure_temporary;
        self.pure_temporary += 1;
        let continuation_name = format!("{PURE_CONTINUATION_LOCAL_PREFIX}{ordinal}");
        let parameter_name = format!("{continuation_name}_value");
        let parameter = ast::Spanned::new(parameter_name.clone(), source_span);
        let parameter_value =
            ast::Spanned::new(ast::ExprKind::Name(parameter.clone()), source_span);
        let body = self.apply_pure_continuation(parameter_value, continuation);
        let lambda = ast::Spanned::new(
            ast::ExprKind::Lambda {
                parameters: vec![ast::Spanned::new(
                    ast::PatternKind::Name(parameter),
                    source_span,
                )],
                body: Box::new(body),
            },
            source_span,
        );
        let name = ast::Spanned::new(continuation_name, source_span);
        let binding = ast::Spanned::new(
            ast::StatementKind::Let {
                name: name.clone(),
                ty: None,
                value: lambda,
            },
            source_span,
        );
        (Some(binding), PureContinuation::Shared { name, span })
    }

    fn wrap_pure_continuation_binding(
        &self,
        binding: Option<ast::Statement>,
        mut expression: ast::Expr,
        span: uhura_syntax::ast::Span,
    ) -> ast::Expr {
        let Some(binding) = binding else {
            return expression;
        };
        let source_span = self.span(span);
        if let ast::ExprKind::Block(block) = &mut expression.value {
            block.statements.insert(0, binding);
            block.span = source_span;
            expression.span = source_span;
            return expression;
        }
        ast::Spanned::new(
            ast::ExprKind::Block(ast::Block {
                statements: vec![
                    binding,
                    ast::Spanned::new(ast::StatementKind::Expr(expression), source_span),
                ],
                span: source_span,
            }),
            source_span,
        )
    }

    fn prepend_pure_statement(
        &self,
        statement: ast::Statement,
        mut rest: ast::Expr,
        span: uhura_syntax::ast::Span,
    ) -> ast::Expr {
        let source_span = self.span(span);
        if let ast::ExprKind::Block(block) = &mut rest.value {
            block.statements.insert(0, statement);
            block.span = source_span;
            rest.span = source_span;
            return rest;
        }
        ast::Spanned::new(
            ast::ExprKind::Block(ast::Block {
                statements: vec![
                    statement,
                    ast::Spanned::new(ast::StatementKind::Expr(rest), source_span),
                ],
                span: source_span,
            }),
            source_span,
        )
    }

    fn apply_pure_continuation(
        &mut self,
        value: ast::Expr,
        continuation: PureContinuation,
    ) -> ast::Expr {
        match continuation {
            PureContinuation::Function => value,
            PureContinuation::Shared { name, span } => {
                let source_span = self.span(span);
                ast::Spanned::new(
                    ast::ExprKind::Call {
                        callee: Box::new(ast::Spanned::new(ast::ExprKind::Name(name), source_span)),
                        arguments: vec![value],
                    },
                    source_span,
                )
            }
            PureContinuation::LetThen {
                name,
                ty,
                remainder,
                env,
                next,
                span,
            } => {
                let rest = self.pure_flow_block(&remainder, &env, *next);
                let statement = ast::Spanned::new(
                    ast::StatementKind::Let {
                        name: self.name(&name),
                        ty: ty.as_ref().map(|ty| self.ty(ty)),
                        value,
                    },
                    self.span(span),
                );
                self.prepend_pure_statement(statement, rest, span)
            }
            PureContinuation::DiscardThen {
                remainder,
                env,
                next,
                span,
            } => {
                let temporary = format!("__uhura_pure_discard_{}", self.pure_temporary);
                self.pure_temporary += 1;
                let source_span = self.span(span);
                let rest = self.pure_flow_block(&remainder, &env, *next);
                let statement = ast::Spanned::new(
                    ast::StatementKind::Let {
                        name: ast::Spanned::new(temporary, source_span),
                        ty: Some(ast::Spanned::new(
                            ast::TypeExprKind::Tuple(Vec::new()),
                            source_span,
                        )),
                        value,
                    },
                    source_span,
                );
                self.prepend_pure_statement(statement, rest, span)
            }
            PureContinuation::Unary {
                operator,
                span,
                next,
            } => {
                let source_span = self.span(span);
                let value = ast::Spanned::new(
                    ast::ExprKind::Unary {
                        op: ast::Spanned::new(
                            match operator {
                                uhura_syntax::ast::UnaryOperator::Not => ast::UnaryOp::Not,
                                uhura_syntax::ast::UnaryOperator::Negate => ast::UnaryOp::Negate,
                            },
                            source_span,
                        ),
                        operand: Box::new(value),
                    },
                    source_span,
                );
                self.apply_pure_continuation(value, *next)
            }
            PureContinuation::BinaryLeft {
                operator,
                right,
                env,
                span,
                next,
            } => self.pure_flow_expression(
                &right,
                &env,
                PureContinuation::BinaryRight {
                    operator,
                    left: value,
                    span,
                    next,
                },
            ),
            PureContinuation::ShortCircuitLeft {
                operator,
                right,
                env,
                span,
                next,
            } => {
                let (binding, next) = self.share_pure_continuation(*next, span);
                let right_branch = self.pure_flow_expression(&right, &env, next.clone());
                let short_value = ast::Spanned::new(
                    ast::ExprKind::Bool(matches!(operator, uhura_syntax::ast::BinaryOperator::Or)),
                    self.span(span),
                );
                let short_branch = self.apply_pure_continuation(short_value, next);
                let (then_branch, else_branch) = match operator {
                    uhura_syntax::ast::BinaryOperator::And => (right_branch, short_branch),
                    uhura_syntax::ast::BinaryOperator::Or => (short_branch, right_branch),
                    _ => unreachable!("only boolean short-circuit operators use this frame"),
                };
                let expression = ast::Spanned::new(
                    ast::ExprKind::If {
                        condition: Box::new(value),
                        then_branch: Box::new(then_branch),
                        else_branch: Some(Box::new(else_branch)),
                    },
                    self.span(span),
                );
                self.wrap_pure_continuation_binding(binding, expression, span)
            }
            PureContinuation::BinaryRight {
                operator,
                left,
                span,
                next,
            } => {
                let source_span = self.span(span);
                let value = ast::Spanned::new(
                    ast::ExprKind::Binary {
                        left: Box::new(left),
                        op: ast::Spanned::new(
                            match operator {
                                uhura_syntax::ast::BinaryOperator::Multiply => {
                                    ast::BinaryOp::Multiply
                                }
                                uhura_syntax::ast::BinaryOperator::Add => ast::BinaryOp::Add,
                                uhura_syntax::ast::BinaryOperator::Subtract => {
                                    ast::BinaryOp::Subtract
                                }
                                uhura_syntax::ast::BinaryOperator::And => ast::BinaryOp::And,
                                uhura_syntax::ast::BinaryOperator::Or => ast::BinaryOp::Or,
                            },
                            source_span,
                        ),
                        right: Box::new(value),
                    },
                    source_span,
                );
                self.apply_pure_continuation(value, *next)
            }
            PureContinuation::CompareLeft {
                operator,
                right,
                env,
                span,
                next,
            } => self.pure_flow_expression(
                &right,
                &env,
                PureContinuation::CompareRight {
                    operator,
                    left: value,
                    span,
                    next,
                },
            ),
            PureContinuation::CompareRight {
                operator,
                left,
                span,
                next,
            } => {
                let source_span = self.span(span);
                let value = ast::Spanned::new(
                    ast::ExprKind::Binary {
                        left: Box::new(left),
                        op: ast::Spanned::new(
                            match operator {
                                uhura_syntax::ast::ComparisonOperator::Equal => {
                                    ast::BinaryOp::Equal
                                }
                                uhura_syntax::ast::ComparisonOperator::NotEqual => {
                                    ast::BinaryOp::NotEqual
                                }
                                uhura_syntax::ast::ComparisonOperator::Less => ast::BinaryOp::Less,
                                uhura_syntax::ast::ComparisonOperator::LessEqual => {
                                    ast::BinaryOp::LessEqual
                                }
                                uhura_syntax::ast::ComparisonOperator::Greater => {
                                    ast::BinaryOp::Greater
                                }
                                uhura_syntax::ast::ComparisonOperator::GreaterEqual => {
                                    ast::BinaryOp::GreaterEqual
                                }
                            },
                            source_span,
                        ),
                        right: Box::new(value),
                    },
                    source_span,
                );
                self.apply_pure_continuation(value, *next)
            }
            PureContinuation::Member { member, span, next } => {
                let source_span = self.span(span);
                let value = ast::Spanned::new(
                    ast::ExprKind::Member {
                        receiver: Box::new(value),
                        member: self.name(&member),
                    },
                    source_span,
                );
                self.apply_pure_continuation(value, *next)
            }
            PureContinuation::IndexValue {
                index,
                env,
                span,
                next,
            } => self.pure_flow_expression(
                &index,
                &env,
                PureContinuation::IndexIndex { value, span, next },
            ),
            PureContinuation::IndexIndex {
                value: receiver,
                span,
                next,
            } => {
                let source_span = self.span(span);
                let value = ast::Spanned::new(
                    ast::ExprKind::Index {
                        receiver: Box::new(receiver),
                        index: Box::new(value),
                    },
                    source_span,
                );
                self.apply_pure_continuation(value, *next)
            }
            PureContinuation::Is {
                pattern,
                span,
                next,
            } => {
                let source_span = self.span(span);
                let value = ast::Spanned::new(
                    ast::ExprKind::Is {
                        value: Box::new(value),
                        pattern: self.pattern(&pattern),
                    },
                    source_span,
                );
                self.apply_pure_continuation(value, *next)
            }
            PureContinuation::Sequence {
                tuple,
                mut completed,
                mut remaining,
                env,
                span,
                next,
            } => {
                completed.push(value);
                if remaining.is_empty() {
                    let source_span = self.span(span);
                    let value = ast::Spanned::new(
                        if tuple {
                            ast::ExprKind::Tuple(completed)
                        } else {
                            ast::ExprKind::Sequence(completed)
                        },
                        source_span,
                    );
                    self.apply_pure_continuation(value, *next)
                } else {
                    let first = remaining.remove(0);
                    self.pure_flow_expression(
                        &first,
                        &env.clone(),
                        PureContinuation::Sequence {
                            tuple,
                            completed,
                            remaining,
                            env,
                            span,
                            next,
                        },
                    )
                }
            }
            PureContinuation::CallCallee {
                arguments,
                mut env,
                span,
                next,
            } => {
                let temporary = format!("__uhura_pure_call_callee_{}", self.pure_temporary);
                self.pure_temporary += 1;
                env.values.insert(temporary.clone(), value);
                self.pure_flow_call_arguments(
                    source_name_expression(temporary, span),
                    arguments,
                    0,
                    env,
                    span,
                    *next,
                )
            }
            PureContinuation::CallArgument {
                callee,
                mut arguments,
                index,
                mut env,
                span,
                next,
            } => {
                let temporary = format!("__uhura_pure_call_argument_{}", self.pure_temporary);
                self.pure_temporary += 1;
                env.values.insert(temporary.clone(), value);
                arguments[index] = uhura_syntax::ast::CallArgument::Expression(
                    source_name_expression(temporary, span),
                );
                self.pure_flow_call_arguments(callee, arguments, index + 1, env, span, *next)
            }
            PureContinuation::RecordField {
                mut record,
                index,
                mut env,
                span,
                next,
            } => {
                let temporary = format!("__uhura_pure_record_field_{}", self.pure_temporary);
                self.pure_temporary += 1;
                env.values.insert(temporary.clone(), value);
                record.fields[index].value = Some(source_name_expression(temporary, span));
                self.pure_flow_record(record, index + 1, env, span, *next)
            }
            PureContinuation::RecordBase {
                mut record,
                mut env,
                span,
                next,
            } => {
                let temporary = format!("__uhura_pure_record_base_{}", self.pure_temporary);
                self.pure_temporary += 1;
                env.values.insert(temporary.clone(), value);
                record.base = Some(Box::new(source_name_expression(temporary, span)));
                let value = self.record_expression(span, &record, &env);
                self.apply_pure_continuation(value, *next)
            }
            PureContinuation::IfCondition {
                expression,
                env,
                span,
                next,
            } => {
                let (binding, next) = self.share_pure_continuation(*next, span);
                let then_branch = self.pure_flow_block(&expression.then_branch, &env, next.clone());
                let else_branch = if let Some(branch) = expression.else_branch.as_ref() {
                    match branch {
                        uhura_syntax::ast::ElseBranch::Block(block) => {
                            self.pure_flow_block(block, &env, next.clone())
                        }
                        uhura_syntax::ast::ElseBranch::If(value) => {
                            self.pure_flow_expression(value, &env, next.clone())
                        }
                    }
                } else {
                    self.apply_pure_continuation(
                        ast::Spanned::new(ast::ExprKind::Tuple(Vec::new()), self.span(span)),
                        next,
                    )
                };
                let expression = ast::Spanned::new(
                    ast::ExprKind::If {
                        condition: Box::new(value),
                        then_branch: Box::new(then_branch),
                        else_branch: Some(Box::new(else_branch)),
                    },
                    self.span(span),
                );
                self.wrap_pure_continuation_binding(binding, expression, span)
            }
            PureContinuation::MatchSubject {
                expression,
                env,
                span,
                next,
            } => {
                let (binding, next) = self.share_pure_continuation(*next, span);
                let expression = ast::Spanned::new(
                    ast::ExprKind::Match {
                        subject: Box::new(value),
                        arms: expression
                            .arms
                            .iter()
                            .map(|arm| ast::MatchArm {
                                pattern: self.pattern(&arm.pattern),
                                body: self.pure_flow_expression(&arm.value, &env, next.clone()),
                                span: self.span(arm.span),
                            })
                            .collect(),
                    },
                    self.span(span),
                );
                self.wrap_pure_continuation_binding(binding, expression, span)
            }
        }
    }

    fn reaction_block(
        &mut self,
        block: &uhura_syntax::ast::Block,
        env: &ExprEnv,
        terminal_tail: bool,
    ) -> ast::Block {
        let mut statements = self.statements(block, env, true);
        if let Some(tail) = &block.tail {
            let value = self.effect_expr(tail, env, terminal_tail);
            statements.push(ast::Spanned::new(
                ast::StatementKind::Expr(value),
                self.span(tail.span),
            ));
        }
        ast::Block {
            statements,
            span: self.span(block.span),
        }
    }

    fn statements(
        &mut self,
        block: &uhura_syntax::ast::Block,
        env: &ExprEnv,
        reaction: bool,
    ) -> Vec<ast::Statement> {
        block
            .statements
            .iter()
            .map(|statement| {
                let value = match &statement.kind {
                    uhura_syntax::ast::StatementKind::Let {
                        name, ty, value, ..
                    } => ast::StatementKind::Let {
                        name: self.name(name),
                        ty: ty.as_ref().map(|ty| self.ty(ty)),
                        value: if reaction {
                            self.effect_expr(value, env, false)
                        } else {
                            self.expr(value, env)
                        },
                    },
                    uhura_syntax::ast::StatementKind::Assign { target, value, .. } => {
                        ast::StatementKind::Set {
                            target: self.name(target),
                            value: self.expr(value, env),
                        }
                    }
                    uhura_syntax::ast::StatementKind::Emit { output, .. } => {
                        let callee = if let Some(owner) = &output.selector.owner {
                            ast::ExprKind::Member {
                                receiver: Box::new(ast::Spanned::new(
                                    ast::ExprKind::Name(self.name(owner)),
                                    self.span(owner.span),
                                )),
                                member: ast::Spanned::new(
                                    lower_protocol_variant(&output.selector.variant.text),
                                    self.span(output.selector.variant.span),
                                ),
                            }
                        } else {
                            ast::ExprKind::Name(self.name(&output.selector.variant))
                        };
                        ast::StatementKind::Emit(ast::Spanned::new(
                            ast::ExprKind::Call {
                                callee: Box::new(ast::Spanned::new(
                                    callee,
                                    self.span(output.selector.span),
                                )),
                                arguments: output
                                    .arguments
                                    .iter()
                                    .map(|value| self.expr(value, env))
                                    .collect(),
                            },
                            self.span(output.span),
                        ))
                    }
                    uhura_syntax::ast::StatementKind::While {
                        condition,
                        decreases,
                        body,
                    } => ast::StatementKind::While {
                        condition: self.expr(condition, env),
                        decreases: self.expr(decreases, env),
                        body: self.reaction_block(body, env, false),
                    },
                    uhura_syntax::ast::StatementKind::Unreachable { .. } => {
                        ast::StatementKind::Expr(ast::Spanned::new(
                            ast::ExprKind::Unreachable,
                            self.span(statement.span),
                        ))
                    }
                    uhura_syntax::ast::StatementKind::Expression { expression, .. }
                    | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                        let value = if reaction {
                            self.effect_expr(expression, env, false)
                        } else {
                            self.expr(expression, env)
                        };
                        ast::StatementKind::Expr(value)
                    }
                };
                ast::Spanned::new(value, self.span(statement.span))
            })
            .collect()
    }

    fn effect_expr(
        &mut self,
        expression: &uhura_syntax::ast::Expression,
        env: &ExprEnv,
        terminal: bool,
    ) -> ast::Expr {
        let span = self.span(expression.span);
        match &expression.kind {
            uhura_syntax::ast::ExpressionKind::Return(value) => ast::Spanned::new(
                ast::ExprKind::Finish(Box::new(value.as_ref().map_or_else(
                    || ast::Spanned::new(ast::ExprKind::Tuple(Vec::new()), span),
                    |value| self.expr(value, env),
                ))),
                span,
            ),
            uhura_syntax::ast::ExpressionKind::If(value) => {
                let then_branch = ast::Spanned::new(
                    ast::ExprKind::Block(self.reaction_block(&value.then_branch, env, terminal)),
                    self.span(value.then_branch.span),
                );
                let else_branch = value.else_branch.as_ref().map(|branch| {
                    Box::new(match branch {
                        uhura_syntax::ast::ElseBranch::Block(block) => ast::Spanned::new(
                            ast::ExprKind::Block(self.reaction_block(block, env, terminal)),
                            self.span(block.span),
                        ),
                        uhura_syntax::ast::ElseBranch::If(value) => {
                            self.effect_expr(value, env, terminal)
                        }
                    })
                });
                ast::Spanned::new(
                    ast::ExprKind::If {
                        condition: Box::new(self.expr(&value.condition, env)),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    },
                    span,
                )
            }
            uhura_syntax::ast::ExpressionKind::Match(value) => ast::Spanned::new(
                ast::ExprKind::Match {
                    subject: Box::new(self.expr(&value.value, env)),
                    arms: value
                        .arms
                        .iter()
                        .map(|arm| ast::MatchArm {
                            pattern: self.pattern(&arm.pattern),
                            body: self.effect_expr(&arm.value, env, terminal),
                            span: self.span(arm.span),
                        })
                        .collect(),
                },
                span,
            ),
            uhura_syntax::ast::ExpressionKind::Block(block) => ast::Spanned::new(
                ast::ExprKind::Block(self.reaction_block(block, env, terminal)),
                span,
            ),
            _ if terminal => ast::Spanned::new(
                ast::ExprKind::Finish(Box::new(self.expr(expression, env))),
                span,
            ),
            _ => self.expr(expression, env),
        }
    }

    fn initializer_expr(&mut self, expression: &uhura_syntax::ast::Expression) -> ast::Expr {
        let previous = self.inline_private_calls;
        self.inline_private_calls = true;
        let value = self.expr(expression, &ExprEnv::default());
        self.inline_private_calls = previous;
        value
    }

    fn expr(&mut self, expression: &uhura_syntax::ast::Expression, env: &ExprEnv) -> ast::Expr {
        let span = self.span(expression.span);
        let value = match &expression.kind {
            uhura_syntax::ast::ExpressionKind::Literal(value) => match value {
                uhura_syntax::ast::Literal::Bool(value) => ast::ExprKind::Bool(*value),
                uhura_syntax::ast::Literal::Integer { raw } => ast::ExprKind::Integer(raw.clone()),
                uhura_syntax::ast::Literal::Decimal { raw } => ast::ExprKind::Decimal(raw.clone()),
                uhura_syntax::ast::Literal::Text { value, .. } => {
                    ast::ExprKind::Text(value.clone())
                }
            },
            uhura_syntax::ast::ExpressionKind::Unit => ast::ExprKind::Tuple(Vec::new()),
            uhura_syntax::ast::ExpressionKind::Sequence(values) => {
                ast::ExprKind::Sequence(values.iter().map(|value| self.expr(value, env)).collect())
            }
            uhura_syntax::ast::ExpressionKind::Tuple(values) => {
                ast::ExprKind::Tuple(values.iter().map(|value| self.expr(value, env)).collect())
            }
            uhura_syntax::ast::ExpressionKind::Group(value) => return self.expr(value, env),
            uhura_syntax::ast::ExpressionKind::Name(value) => {
                if value.segments.len() == 1
                    && let Some(value) = env.values.get(&value.segments[0].text)
                {
                    return value.clone();
                }
                if value.segments.len() == 2 && self.is_variant_name(value) {
                    ast::ExprKind::Member {
                        receiver: Box::new(ast::Spanned::new(
                            ast::ExprKind::Name(self.resolved_name(&value.segments[0])),
                            self.span(value.segments[0].span),
                        )),
                        member: self.name(&value.segments[1]),
                    }
                } else {
                    if value.segments.len() != 1 {
                        self.unsupported(
                            value.span,
                            format!(
                                "value resolution must complete before lowering unresolved qualified value `{}`",
                                value
                                    .segments
                                    .iter()
                                    .map(|segment| segment.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join("::")
                            ),
                        );
                    }
                    ast::ExprKind::Name(self.qualified_value_name(value))
                }
            }
            uhura_syntax::ast::ExpressionKind::Record(value) => {
                return self.record_expression(expression.span, value, env);
            }
            uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => ast::ExprKind::Record(
                entries
                    .iter()
                    .map(|entry| ast::RecordEntry {
                        key: self.expr(&entry.key, env),
                        value: self.expr(&entry.value, env),
                        span: self.span(entry.span),
                    })
                    .collect(),
            ),
            uhura_syntax::ast::ExpressionKind::Block(value) => {
                return self.pure_block_expression(value, env);
            }
            uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
                if let uhura_syntax::ast::ExpressionKind::Name(name) = &callee.kind
                    && name.segments.len() == 1
                    && self.inline_private_calls
                    && let Some(function) =
                        self.private_functions.get(&name.segments[0].text).cloned()
                {
                    return self.inline_private_function(
                        expression.span,
                        &function,
                        arguments,
                        env,
                    );
                }
                return self.call_expression(expression.span, callee, arguments, env);
            }
            uhura_syntax::ast::ExpressionKind::Member { value, member } => ast::ExprKind::Member {
                receiver: Box::new(self.expr(value, env)),
                member: self.name(member),
            },
            uhura_syntax::ast::ExpressionKind::Index { value, index } => ast::ExprKind::Index {
                receiver: Box::new(self.expr(value, env)),
                index: Box::new(self.expr(index, env)),
            },
            uhura_syntax::ast::ExpressionKind::Unary { operator, value } => ast::ExprKind::Unary {
                op: ast::Spanned::new(
                    match operator {
                        uhura_syntax::ast::UnaryOperator::Not => ast::UnaryOp::Not,
                        uhura_syntax::ast::UnaryOperator::Negate => ast::UnaryOp::Negate,
                    },
                    span,
                ),
                operand: Box::new(self.expr(value, env)),
            },
            uhura_syntax::ast::ExpressionKind::Binary {
                operator,
                left,
                right,
            } => ast::ExprKind::Binary {
                left: Box::new(self.expr(left, env)),
                op: ast::Spanned::new(
                    match operator {
                        uhura_syntax::ast::BinaryOperator::Multiply => ast::BinaryOp::Multiply,
                        uhura_syntax::ast::BinaryOperator::Add => ast::BinaryOp::Add,
                        uhura_syntax::ast::BinaryOperator::Subtract => ast::BinaryOp::Subtract,
                        uhura_syntax::ast::BinaryOperator::And => ast::BinaryOp::And,
                        uhura_syntax::ast::BinaryOperator::Or => ast::BinaryOp::Or,
                    },
                    span,
                ),
                right: Box::new(self.expr(right, env)),
            },
            uhura_syntax::ast::ExpressionKind::Compare {
                operator,
                left,
                right,
            } => ast::ExprKind::Binary {
                left: Box::new(self.expr(left, env)),
                op: ast::Spanned::new(
                    match operator {
                        uhura_syntax::ast::ComparisonOperator::Equal => ast::BinaryOp::Equal,
                        uhura_syntax::ast::ComparisonOperator::NotEqual => ast::BinaryOp::NotEqual,
                        uhura_syntax::ast::ComparisonOperator::Less => ast::BinaryOp::Less,
                        uhura_syntax::ast::ComparisonOperator::LessEqual => {
                            ast::BinaryOp::LessEqual
                        }
                        uhura_syntax::ast::ComparisonOperator::Greater => ast::BinaryOp::Greater,
                        uhura_syntax::ast::ComparisonOperator::GreaterEqual => {
                            ast::BinaryOp::GreaterEqual
                        }
                    },
                    span,
                ),
                right: Box::new(self.expr(right, env)),
            },
            uhura_syntax::ast::ExpressionKind::Is { value, pattern } => ast::ExprKind::Is {
                value: Box::new(self.expr(value, env)),
                pattern: self.pattern(pattern),
            },
            uhura_syntax::ast::ExpressionKind::If(value) => ast::ExprKind::If {
                condition: Box::new(self.expr(&value.condition, env)),
                then_branch: Box::new(self.pure_block_expression(&value.then_branch, env)),
                else_branch: value.else_branch.as_ref().map(|branch| {
                    Box::new(match branch {
                        uhura_syntax::ast::ElseBranch::Block(block) => {
                            self.pure_block_expression(block, env)
                        }
                        uhura_syntax::ast::ElseBranch::If(value) => self.expr(value, env),
                    })
                }),
            },
            uhura_syntax::ast::ExpressionKind::Match(value) => ast::ExprKind::Match {
                subject: Box::new(self.expr(&value.value, env)),
                arms: value
                    .arms
                    .iter()
                    .map(|arm| ast::MatchArm {
                        pattern: self.pattern(&arm.pattern),
                        body: self.expr(&arm.value, env),
                        span: self.span(arm.span),
                    })
                    .collect(),
            },
            uhura_syntax::ast::ExpressionKind::Return(value) => {
                self.unsupported(
                    expression.span,
                    "lexical `return` requires an enclosing function, update, or handler body",
                );
                ast::ExprKind::Finish(Box::new(value.as_ref().map_or_else(
                    || ast::Spanned::new(ast::ExprKind::Tuple(Vec::new()), span),
                    |value| self.expr(value, env),
                )))
            }
        };
        ast::Spanned::new(value, span)
    }

    fn inline_private_function(
        &mut self,
        call_span: uhura_syntax::ast::Span,
        function: &uhura_syntax::ast::FunctionDeclaration,
        arguments: &[uhura_syntax::ast::CallArgument],
        env: &ExprEnv,
    ) -> ast::Expr {
        if self.inlining.contains(&function.name.text) {
            self.diagnostics.push(error(
                codes::DEPENDENCY_CYCLE,
                "uhura-0.4/recursive-private-function",
                format!(
                    "private function `{}` participates in a call cycle",
                    function.name.text
                ),
                self.span(call_span),
            ));
            return ast::Spanned::new(ast::ExprKind::Unreachable, self.span(call_span));
        }
        if arguments.len() != function.parameters.len()
            || arguments
                .iter()
                .any(|argument| matches!(argument, uhura_syntax::ast::CallArgument::Binder(_)))
        {
            self.diagnostics.push(error(
                codes::ARITY,
                "uhura-0.4/private-function-arguments",
                format!(
                    "private function `{}` expects {} ordinary argument(s)",
                    function.name.text,
                    function.parameters.len()
                ),
                self.span(call_span),
            ));
            return ast::Spanned::new(ast::ExprKind::Unreachable, self.span(call_span));
        }

        let mut nested = env.clone();
        for (parameter, argument) in function.parameters.iter().zip(arguments) {
            let uhura_syntax::ast::CallArgument::Expression(argument) = argument else {
                unreachable!("binder arguments were rejected above")
            };
            nested
                .values
                .insert(parameter.name.text.clone(), self.expr(argument, env));
        }
        self.inlining.push(function.name.text.clone());
        let value = self.pure_block_expression(&function.body, &nested);
        self.inlining.pop();
        ast::Spanned::new(value.value, self.span(call_span))
    }

    fn call_expression(
        &mut self,
        span: uhura_syntax::ast::Span,
        callee: &uhura_syntax::ast::Expression,
        arguments: &[uhura_syntax::ast::CallArgument],
        env: &ExprEnv,
    ) -> ast::Expr {
        if let Some(path) = self.qualified_callee(callee) {
            match path.as_slice() {
                [owner, method] if owner == "Table" && method == "from" => {
                    return self.table_from(span, arguments, env);
                }
                [owner, method] if owner == "Map" && method == "from" => {
                    return self.map_from(span, arguments, env);
                }
                [owner, method] if owner == "Routes" && method == "from" => {
                    return self.routes_from(span, arguments, env);
                }
                [owner, method] if owner == "Seq" && method == "from_options" => {
                    return self.seq_from_options(span, arguments, env);
                }
                [owner, method] if owner == "Set" && method == "filter_map" => {
                    return self.set_filter_map(span, arguments, env);
                }
                [owner, method]
                    if matches!(
                        (owner.as_str(), method.as_str()),
                        ("Ratio", "checked_from") | ("NonEmpty", "checked_from")
                    ) =>
                {
                    let receiver = ast::Spanned::new(
                        ast::ExprKind::Name(ast::Spanned::new(
                            owner.clone(),
                            self.span(callee.span),
                        )),
                        self.span(callee.span),
                    );
                    return ast::Spanned::new(
                        ast::ExprKind::Call {
                            callee: Box::new(ast::Spanned::new(
                                ast::ExprKind::Member {
                                    receiver: Box::new(receiver),
                                    member: ast::Spanned::new(
                                        "from".into(),
                                        self.span(callee.span),
                                    ),
                                },
                                self.span(callee.span),
                            )),
                            arguments: self.call_arguments(arguments, env),
                        },
                        self.span(span),
                    );
                }
                [owner, method] if matches!(owner.as_str(), "Map" | "Set") && method == "empty" => {
                    if !arguments.is_empty() {
                        self.unsupported(span, format!("`{owner}::empty()` takes no arguments"));
                    }
                    return ast::Spanned::new(
                        ast::ExprKind::Member {
                            receiver: Box::new(ast::Spanned::new(
                                ast::ExprKind::Name(ast::Spanned::new(
                                    owner.clone(),
                                    self.span(callee.span),
                                )),
                                self.span(callee.span),
                            )),
                            member: ast::Spanned::new("empty".into(), self.span(callee.span)),
                        },
                        self.span(span),
                    );
                }
                _ => {}
            }
        }

        if let uhura_syntax::ast::ExpressionKind::Member { value, member } = &callee.kind {
            if arguments.is_empty()
                && matches!(
                    member.text.as_str(),
                    "values"
                        | "entries"
                        | "entries_by_key"
                        | "uncons"
                        | "len"
                        | "is_unique"
                        | "is_empty"
                )
            {
                let member_name = match member.text.as_str() {
                    "len" => "size",
                    "is_unique" => "unique",
                    value => value,
                };
                return ast::Spanned::new(
                    ast::ExprKind::Member {
                        receiver: Box::new(self.expr(value, env)),
                        member: ast::Spanned::new(member_name.into(), self.span(member.span)),
                    },
                    self.span(span),
                );
            }
            if member.text == "try_map_values"
                && arguments.len() == 1
                && let uhura_syntax::ast::CallArgument::Binder(binder) = &arguments[0]
            {
                let (parameters, body) = self.entry_binder(binder, env);
                return ast::Spanned::new(
                    ast::ExprKind::Call {
                        callee: Box::new(ast::Spanned::new(
                            ast::ExprKind::Member {
                                receiver: Box::new(self.expr(value, env)),
                                member: self.name(member),
                            },
                            self.span(callee.span),
                        )),
                        arguments: vec![ast::Spanned::new(
                            ast::ExprKind::Lambda {
                                parameters,
                                body: Box::new(body),
                            },
                            self.span(binder.span),
                        )],
                    },
                    self.span(span),
                );
            }
            if self.is_entries_call(value)
                && arguments.len() == 1
                && let uhura_syntax::ast::CallArgument::Binder(binder) = &arguments[0]
            {
                let (parameters, body) = self.entry_binder(binder, env);
                let receiver = self.expr(value, env);
                return ast::Spanned::new(
                    ast::ExprKind::Call {
                        callee: Box::new(ast::Spanned::new(
                            ast::ExprKind::Member {
                                receiver: Box::new(receiver),
                                member: self.name(member),
                            },
                            self.span(callee.span),
                        )),
                        arguments: vec![ast::Spanned::new(
                            ast::ExprKind::Lambda {
                                parameters,
                                body: Box::new(body),
                            },
                            self.span(binder.span),
                        )],
                    },
                    self.span(span),
                );
            }
        }

        ast::Spanned::new(
            ast::ExprKind::Call {
                callee: Box::new(self.callee_expression(callee, env)),
                arguments: self.call_arguments(arguments, env),
            },
            self.span(span),
        )
    }

    fn callee_expression(
        &mut self,
        callee: &uhura_syntax::ast::Expression,
        env: &ExprEnv,
    ) -> ast::Expr {
        if let uhura_syntax::ast::ExpressionKind::Name(name) = &callee.kind
            && name.segments.len() == 2
            && !self.is_variant_name(name)
        {
            let receiver = ast::Spanned::new(
                ast::ExprKind::Name(self.resolved_name(&name.segments[0])),
                self.span(name.segments[0].span),
            );
            return ast::Spanned::new(
                ast::ExprKind::Member {
                    receiver: Box::new(receiver),
                    member: self.name(&name.segments[1]),
                },
                self.span(callee.span),
            );
        }
        self.expr(callee, env)
    }

    fn call_arguments(
        &mut self,
        arguments: &[uhura_syntax::ast::CallArgument],
        env: &ExprEnv,
    ) -> Vec<ast::Expr> {
        arguments
            .iter()
            .map(|argument| match argument {
                uhura_syntax::ast::CallArgument::Expression(value) => self.expr(value, env),
                uhura_syntax::ast::CallArgument::Binder(value) => {
                    let child = env.clone();
                    ast::Spanned::new(
                        ast::ExprKind::Lambda {
                            parameters: vec![ast::Spanned::new(
                                ast::PatternKind::Name(self.name(&value.parameter)),
                                self.span(value.parameter.span),
                            )],
                            body: Box::new(self.expr(&value.body, &child)),
                        },
                        self.span(value.span),
                    )
                }
            })
            .collect()
    }

    fn entry_binder(
        &mut self,
        binder: &uhura_syntax::ast::BinderExpression,
        env: &ExprEnv,
    ) -> (Vec<ast::Pattern>, ast::Expr) {
        let child = env.clone();
        (
            vec![ast::Spanned::new(
                ast::PatternKind::Name(self.name(&binder.parameter)),
                self.span(binder.parameter.span),
            )],
            self.expr(&binder.body, &child),
        )
    }

    fn table_from(
        &mut self,
        span: uhura_syntax::ast::Span,
        arguments: &[uhura_syntax::ast::CallArgument],
        env: &ExprEnv,
    ) -> ast::Expr {
        let Some(uhura_syntax::ast::CallArgument::Expression(argument)) = arguments.first() else {
            self.unsupported(
                span,
                "`Table::from` requires one sequence of `(key, value)` pairs",
            );
            return ast::Spanned::new(ast::ExprKind::Record(Vec::new()), self.span(span));
        };
        if arguments.len() != 1 {
            self.unsupported(span, "`Table::from` requires exactly one argument");
        }
        let uhura_syntax::ast::ExpressionKind::Sequence(values) = &argument.kind else {
            self.unsupported(
                argument.span,
                "`Table::from` requires a literal sequence so total key coverage is statically visible",
            );
            return ast::Spanned::new(ast::ExprKind::Record(Vec::new()), self.span(span));
        };
        let mut entries = Vec::new();
        for pair in values {
            let uhura_syntax::ast::ExpressionKind::Tuple(items) = &pair.kind else {
                self.unsupported(
                    pair.span,
                    "`Table::from` entries must be `(key, value)` pairs",
                );
                continue;
            };
            if items.len() != 2 {
                self.unsupported(
                    pair.span,
                    "`Table::from` entries must contain exactly two values",
                );
                continue;
            }
            entries.push(ast::RecordEntry {
                key: if let uhura_syntax::ast::ExpressionKind::Name(name) = &items[0].kind
                    && self.is_variant_name(name)
                {
                    ast::Spanned::new(
                        ast::ExprKind::Name(self.qualified_value_name(name)),
                        self.span(items[0].span),
                    )
                } else {
                    self.expr(&items[0], env)
                },
                value: self.expr(&items[1], env),
                span: self.span(pair.span),
            });
        }
        ast::Spanned::new(ast::ExprKind::Record(entries), self.span(span))
    }

    fn map_from(
        &mut self,
        span: uhura_syntax::ast::Span,
        arguments: &[uhura_syntax::ast::CallArgument],
        env: &ExprEnv,
    ) -> ast::Expr {
        let Some(uhura_syntax::ast::CallArgument::Expression(argument)) = arguments.first() else {
            self.unsupported(
                span,
                "`Map::from` requires one sequence of `(key, value)` pairs",
            );
            return ast::Spanned::new(ast::ExprKind::Record(Vec::new()), self.span(span));
        };
        if arguments.len() != 1 {
            self.unsupported(span, "`Map::from` requires exactly one argument");
        }
        let uhura_syntax::ast::ExpressionKind::Sequence(values) = &argument.kind else {
            self.unsupported(
                argument.span,
                "`Map::from` requires a literal sequence so duplicate constant keys can be rejected statically",
            );
            return ast::Spanned::new(ast::ExprKind::Record(Vec::new()), self.span(span));
        };
        let entries = values
            .iter()
            .filter_map(|pair| {
                let uhura_syntax::ast::ExpressionKind::Tuple(items) = &pair.kind else {
                    self.unsupported(
                        pair.span,
                        "`Map::from` entries must be `(key, value)` pairs",
                    );
                    return None;
                };
                if items.len() != 2 {
                    self.unsupported(
                        pair.span,
                        "`Map::from` entries must contain exactly two values",
                    );
                    return None;
                }
                Some(ast::RecordEntry {
                    key: self.expr(&items[0], env),
                    value: self.expr(&items[1], env),
                    span: self.span(pair.span),
                })
            })
            .collect();
        ast::Spanned::new(ast::ExprKind::Record(entries), self.span(span))
    }

    fn routes_from(
        &mut self,
        span: uhura_syntax::ast::Span,
        arguments: &[uhura_syntax::ast::CallArgument],
        env: &ExprEnv,
    ) -> ast::Expr {
        let Some(uhura_syntax::ast::CallArgument::Expression(argument)) = arguments.first() else {
            self.unsupported(
                span,
                "`Routes::from` requires one sequence of `(constructor, pattern)` pairs",
            );
            return self.routes_call(span, Vec::new());
        };
        if arguments.len() != 1 {
            self.unsupported(span, "`Routes::from` requires exactly one argument");
        }
        let uhura_syntax::ast::ExpressionKind::Sequence(values) = &argument.kind else {
            self.unsupported(
                argument.span,
                "`Routes::from` requires a literal sequence so route coverage is statically visible",
            );
            return self.routes_call(span, Vec::new());
        };
        let entries = values
            .iter()
            .filter_map(|pair| {
                let uhura_syntax::ast::ExpressionKind::Tuple(items) = &pair.kind else {
                    self.unsupported(
                        pair.span,
                        "`Routes::from` entries must be `(constructor, pattern)` pairs",
                    );
                    return None;
                };
                if items.len() != 2 {
                    self.unsupported(
                        pair.span,
                        "`Routes::from` entries must contain exactly two values",
                    );
                    return None;
                }
                let uhura_syntax::ast::ExpressionKind::Literal(uhura_syntax::ast::Literal::Text {
                    value: constructor,
                    ..
                }) = &items[0].kind
                else {
                    self.unsupported(
                        items[0].span,
                        "route constructors in `Routes::from` must be text literals",
                    );
                    return None;
                };
                Some(ast::RecordEntry {
                    key: ast::Spanned::new(
                        ast::ExprKind::Name(ast::Spanned::new(
                            constructor.clone(),
                            self.span(items[0].span),
                        )),
                        self.span(items[0].span),
                    ),
                    value: self.expr(&items[1], env),
                    span: self.span(pair.span),
                })
            })
            .collect();
        self.routes_call(span, entries)
    }

    fn routes_call(
        &mut self,
        span: uhura_syntax::ast::Span,
        entries: Vec<ast::RecordEntry>,
    ) -> ast::Expr {
        let source_span = self.span(span);
        ast::Spanned::new(
            ast::ExprKind::Call {
                callee: Box::new(ast::Spanned::new(
                    ast::ExprKind::Name(ast::Spanned::new("routes".into(), source_span)),
                    source_span,
                )),
                arguments: vec![ast::Spanned::new(
                    ast::ExprKind::Record(entries),
                    source_span,
                )],
            },
            source_span,
        )
    }

    fn seq_from_options(
        &mut self,
        span: uhura_syntax::ast::Span,
        arguments: &[uhura_syntax::ast::CallArgument],
        env: &ExprEnv,
    ) -> ast::Expr {
        let Some(uhura_syntax::ast::CallArgument::Expression(argument)) = arguments.first() else {
            self.unsupported(span, "`Seq::from_options` requires one sequence expression");
            return ast::Spanned::new(ast::ExprKind::Sequence(Vec::new()), self.span(span));
        };
        if arguments.len() != 1 {
            self.unsupported(span, "`Seq::from_options` requires exactly one argument");
        }
        let source_span = self.span(span);
        ast::Spanned::new(
            ast::ExprKind::Call {
                callee: Box::new(ast::Spanned::new(
                    ast::ExprKind::Member {
                        receiver: Box::new(self.expr(argument, env)),
                        member: ast::Spanned::new("from_options".into(), source_span),
                    },
                    source_span,
                )),
                arguments: Vec::new(),
            },
            source_span,
        )
    }

    fn set_filter_map(
        &mut self,
        span: uhura_syntax::ast::Span,
        arguments: &[uhura_syntax::ast::CallArgument],
        env: &ExprEnv,
    ) -> ast::Expr {
        let (
            Some(uhura_syntax::ast::CallArgument::Expression(source)),
            Some(uhura_syntax::ast::CallArgument::Binder(binder)),
        ) = (arguments.first(), arguments.get(1))
        else {
            self.unsupported(
                span,
                "`Set::filter_map` requires a source collection and one binder",
            );
            return self.empty_set_comprehension(span);
        };
        if arguments.len() != 2 {
            self.unsupported(span, "`Set::filter_map` requires exactly two arguments");
        }

        let (parameters, body) = if self.is_entries_call(source) {
            self.entry_binder(binder, env)
        } else {
            let child = env.clone();
            (
                vec![ast::Spanned::new(
                    ast::PatternKind::Name(self.name(&binder.parameter)),
                    self.span(binder.parameter.span),
                )],
                self.expr(&binder.body, &child),
            )
        };
        let source_span = self.span(span);
        ast::Spanned::new(
            ast::ExprKind::Call {
                callee: Box::new(ast::Spanned::new(
                    ast::ExprKind::Member {
                        receiver: Box::new(self.expr(source, env)),
                        member: ast::Spanned::new("filter_map".into(), source_span),
                    },
                    source_span,
                )),
                arguments: vec![ast::Spanned::new(
                    ast::ExprKind::Lambda {
                        parameters,
                        body: Box::new(body),
                    },
                    self.span(binder.span),
                )],
            },
            source_span,
        )
    }

    fn empty_set_comprehension(&mut self, span: uhura_syntax::ast::Span) -> ast::Expr {
        let wildcard = ast::Spanned::new(ast::PatternKind::Wildcard, self.span(span));
        ast::Spanned::new(
            ast::ExprKind::SetComprehension {
                binding: wildcard,
                source: Box::new(ast::Spanned::new(
                    ast::ExprKind::Sequence(Vec::new()),
                    self.span(span),
                )),
                filters: Vec::new(),
                value: Box::new(ast::Spanned::new(
                    ast::ExprKind::Tuple(Vec::new()),
                    self.span(span),
                )),
            },
            self.span(span),
        )
    }

    fn record_expression(
        &mut self,
        span: uhura_syntax::ast::Span,
        value: &uhura_syntax::ast::RecordExpression,
        env: &ExprEnv,
    ) -> ast::Expr {
        let mut segments = value
            .constructor
            .segments
            .iter()
            .map(|segment| segment.text.clone())
            .collect::<Vec<_>>();
        if let Some(first) = segments.first_mut() {
            *first = self.resolved_text(first).to_owned();
        }
        let constructor = segments.last().map(String::as_str).unwrap_or_default();
        let owner = segments
            .len()
            .checked_sub(2)
            .and_then(|index| segments.get(index))
            .map(String::as_str);
        if owner == Some("Token") && matches!(constructor, "Known" | "Unknown") {
            if value.base.is_some() {
                self.unsupported(span, "`Token` constructors cannot use record-update syntax");
            }
            let shape = RecordShape {
                fields: vec!["value".into()],
            };
            let arguments = self.ordered_fields(span, &shape, &value.fields, env);
            return ast::Spanned::new(
                ast::ExprKind::Call {
                    callee: Box::new(ast::Spanned::new(
                        ast::ExprKind::Name(ast::Spanned::new(
                            lower_protocol_variant(constructor),
                            self.span(value.constructor.span),
                        )),
                        self.span(value.constructor.span),
                    )),
                    arguments,
                },
                self.span(span),
            );
        }
        if let Some(owner) = owner
            && let Some(shape) = self
                .variants
                .get(&(owner.to_owned(), constructor.to_owned()))
                .cloned()
        {
            if value.base.is_some() {
                self.unsupported(span, "enum variants cannot use record-update base syntax");
            }
            let arguments = self.ordered_fields(span, &shape, &value.fields, env);
            return ast::Spanned::new(
                ast::ExprKind::Call {
                    callee: Box::new(ast::Spanned::new(
                        ast::ExprKind::Member {
                            receiver: Box::new(ast::Spanned::new(
                                ast::ExprKind::Name(ast::Spanned::new(
                                    owner.to_owned(),
                                    self.span(value.constructor.span),
                                )),
                                self.span(value.constructor.span),
                            )),
                            member: ast::Spanned::new(
                                constructor.to_owned(),
                                self.span(value.constructor.span),
                            ),
                        },
                        self.span(value.constructor.span),
                    )),
                    arguments,
                },
                self.span(span),
            );
        }

        let fields = value
            .fields
            .iter()
            .map(|field| self.record_entry(field, env))
            .collect::<Vec<_>>();
        if let Some(base) = &value.base {
            ast::Spanned::new(
                ast::ExprKind::Update {
                    base: Box::new(self.expr(base, env)),
                    fields,
                },
                self.span(span),
            )
        } else {
            if !self.structs.contains_key(constructor) {
                self.unsupported(
                    value.constructor.span,
                    format!("unknown record constructor `{constructor}`"),
                );
            }
            ast::Spanned::new(ast::ExprKind::Record(fields), self.span(span))
        }
    }

    fn ordered_fields(
        &mut self,
        span: uhura_syntax::ast::Span,
        shape: &RecordShape,
        fields: &[uhura_syntax::ast::FieldInitializer],
        env: &ExprEnv,
    ) -> Vec<ast::Expr> {
        let mut supplied = BTreeMap::new();
        for field in fields {
            if supplied.insert(field.name.text.as_str(), field).is_some() {
                self.unsupported(
                    field.span,
                    format!("field `{}` is initialized more than once", field.name.text),
                );
            }
        }
        let mut result = Vec::new();
        for name in &shape.fields {
            if let Some(field) = supplied.remove(name.as_str()) {
                result.push(if let Some(value) = &field.value {
                    self.expr(value, env)
                } else {
                    ast::Spanned::new(
                        ast::ExprKind::Name(self.name(&field.name)),
                        self.span(field.span),
                    )
                });
            } else {
                self.unsupported(span, format!("record variant is missing field `{name}`"));
            }
        }
        for name in supplied.keys() {
            self.unsupported(span, format!("record variant has unknown field `{name}`"));
        }
        result
    }

    fn record_entry(
        &mut self,
        field: &uhura_syntax::ast::FieldInitializer,
        env: &ExprEnv,
    ) -> ast::RecordEntry {
        ast::RecordEntry {
            key: ast::Spanned::new(
                ast::ExprKind::Name(self.name(&field.name)),
                self.span(field.name.span),
            ),
            value: if let Some(value) = &field.value {
                self.expr(value, env)
            } else {
                ast::Spanned::new(
                    ast::ExprKind::Name(self.name(&field.name)),
                    self.span(field.name.span),
                )
            },
            span: self.span(field.span),
        }
    }

    fn pattern(&mut self, pattern: &uhura_syntax::ast::Pattern) -> ast::Pattern {
        let span = self.span(pattern.span);
        let value = match &pattern.kind {
            uhura_syntax::ast::PatternKind::Wildcard => ast::PatternKind::Wildcard,
            uhura_syntax::ast::PatternKind::Binder(value) => {
                ast::PatternKind::Name(self.name(value))
            }
            uhura_syntax::ast::PatternKind::Literal(value) => match value {
                uhura_syntax::ast::PatternLiteral::Bool(value) => ast::PatternKind::Bool(*value),
                uhura_syntax::ast::PatternLiteral::Integer { raw, negative } => {
                    ast::PatternKind::Integer(if *negative {
                        format!("-{raw}")
                    } else {
                        raw.clone()
                    })
                }
                uhura_syntax::ast::PatternLiteral::Decimal { raw, negative } => {
                    ast::PatternKind::Decimal(if *negative {
                        format!("-{raw}")
                    } else {
                        raw.clone()
                    })
                }
                uhura_syntax::ast::PatternLiteral::Text { value, .. } => {
                    ast::PatternKind::Text(value.clone())
                }
                uhura_syntax::ast::PatternLiteral::Unit => ast::PatternKind::Tuple(Vec::new()),
            },
            uhura_syntax::ast::PatternKind::Group(value) => return self.pattern(value),
            uhura_syntax::ast::PatternKind::Tuple(values) => {
                ast::PatternKind::Tuple(values.iter().map(|value| self.pattern(value)).collect())
            }
            uhura_syntax::ast::PatternKind::Constructor(value) => {
                if value.segments.len() == 1 {
                    let name = self.qualified_pattern_name(value);
                    ast::PatternKind::Name(name)
                } else {
                    ast::PatternKind::Constructor {
                        path: self.resolved_path(&value.segments),
                        arguments: Vec::new(),
                    }
                }
            }
            uhura_syntax::ast::PatternKind::TupleConstructor {
                constructor,
                arguments,
            } => ast::PatternKind::Constructor {
                path: if constructor.segments.len() == 1 {
                    vec![self.qualified_pattern_name(constructor)]
                } else {
                    self.resolved_path(&constructor.segments)
                },
                arguments: arguments
                    .iter()
                    .map(|argument| self.pattern(argument))
                    .collect(),
            },
            uhura_syntax::ast::PatternKind::Record {
                constructor,
                fields,
                rest,
            } => {
                let mut segments = constructor
                    .segments
                    .iter()
                    .map(|segment| segment.text.clone())
                    .collect::<Vec<_>>();
                if let Some(first) = segments.first_mut() {
                    *first = self.resolved_text(first).to_owned();
                }
                let name = segments.last().map(String::as_str).unwrap_or_default();
                let owner = segments
                    .len()
                    .checked_sub(2)
                    .and_then(|index| segments.get(index))
                    .map(String::as_str);
                let unqualified_shape = if owner.is_none() {
                    let mut matches = self
                        .variants
                        .iter()
                        .filter(|((_, variant), _)| variant == name)
                        .map(|(_, shape)| shape);
                    let shape = matches.next().cloned();
                    if shape.as_ref().is_some_and(|shape| {
                        matches.all(|candidate| candidate.fields == shape.fields)
                    }) {
                        shape
                    } else {
                        None
                    }
                } else {
                    None
                };
                if owner == Some("Token") && matches!(name, "Known" | "Unknown") {
                    let mut supplied = fields
                        .iter()
                        .map(|field| (field.name.text.as_str(), field))
                        .collect::<BTreeMap<_, _>>();
                    let argument = if let Some(field) = supplied.remove("value") {
                        if let Some(value) = &field.pattern {
                            self.pattern(value)
                        } else {
                            ast::Spanned::new(
                                ast::PatternKind::Name(self.name(&field.name)),
                                self.span(field.span),
                            )
                        }
                    } else {
                        self.unsupported(
                            pattern.span,
                            "`Token` constructor pattern is missing field `value`",
                        );
                        ast::Spanned::new(ast::PatternKind::Wildcard, span)
                    };
                    for field_name in supplied.keys() {
                        self.unsupported(
                            pattern.span,
                            format!("`Token` constructor pattern has unknown field `{field_name}`"),
                        );
                    }
                    if *rest {
                        self.unsupported(
                            pattern.span,
                            "`Token` constructor patterns are closed and cannot use `..`",
                        );
                    }
                    ast::PatternKind::Constructor {
                        path: vec![ast::Spanned::new(lower_protocol_variant(name), span)],
                        arguments: vec![argument],
                    }
                } else if let Some(owner) = owner
                    && let Some(shape) = self
                        .variants
                        .get(&(owner.to_owned(), name.to_owned()))
                        .cloned()
                {
                    let mut supplied = fields
                        .iter()
                        .map(|field| (field.name.text.as_str(), field))
                        .collect::<BTreeMap<_, _>>();
                    let mut arguments = Vec::new();
                    for field_name in &shape.fields {
                        if let Some(field) = supplied.remove(field_name.as_str()) {
                            arguments.push(if let Some(pattern) = &field.pattern {
                                self.pattern(pattern)
                            } else {
                                ast::Spanned::new(
                                    ast::PatternKind::Name(self.name(&field.name)),
                                    self.span(field.span),
                                )
                            });
                        } else if *rest {
                            arguments.push(ast::Spanned::new(ast::PatternKind::Wildcard, span));
                        } else {
                            self.unsupported(
                                pattern.span,
                                format!("record variant pattern is missing field `{field_name}`"),
                            );
                        }
                    }
                    for field_name in supplied.keys() {
                        self.unsupported(
                            pattern.span,
                            format!("record variant pattern has unknown field `{field_name}`"),
                        );
                    }
                    ast::PatternKind::Constructor {
                        path: vec![
                            ast::Spanned::new(owner.to_owned(), span),
                            ast::Spanned::new(name.to_owned(), span),
                        ],
                        arguments,
                    }
                } else if let Some(shape) = unqualified_shape {
                    let mut supplied = fields
                        .iter()
                        .map(|field| (field.name.text.as_str(), field))
                        .collect::<BTreeMap<_, _>>();
                    let mut arguments = Vec::new();
                    for field_name in &shape.fields {
                        if let Some(field) = supplied.remove(field_name.as_str()) {
                            arguments.push(if let Some(pattern) = &field.pattern {
                                self.pattern(pattern)
                            } else {
                                ast::Spanned::new(
                                    ast::PatternKind::Name(self.name(&field.name)),
                                    self.span(field.span),
                                )
                            });
                        } else if *rest {
                            arguments.push(ast::Spanned::new(ast::PatternKind::Wildcard, span));
                        } else {
                            self.unsupported(
                                pattern.span,
                                format!("record variant pattern is missing field `{field_name}`"),
                            );
                        }
                    }
                    for field_name in supplied.keys() {
                        self.unsupported(
                            pattern.span,
                            format!("record variant pattern has unknown field `{field_name}`"),
                        );
                    }
                    ast::PatternKind::Constructor {
                        path: vec![self.qualified_pattern_name(constructor)],
                        arguments,
                    }
                } else {
                    ast::PatternKind::Record {
                        fields: fields
                            .iter()
                            .map(|field| ast::RecordPatternField {
                                name: self.name(&field.name),
                                pattern: if let Some(pattern) = &field.pattern {
                                    self.pattern(pattern)
                                } else {
                                    ast::Spanned::new(
                                        ast::PatternKind::Name(self.name(&field.name)),
                                        self.span(field.span),
                                    )
                                },
                                span: self.span(field.span),
                            })
                            .collect(),
                        open: *rest,
                    }
                }
            }
            uhura_syntax::ast::PatternKind::AnonymousRecord { fields, rest } => {
                ast::PatternKind::Record {
                    fields: fields
                        .iter()
                        .map(|field| ast::RecordPatternField {
                            name: self.name(&field.name),
                            pattern: if let Some(pattern) = &field.pattern {
                                self.pattern(pattern)
                            } else {
                                ast::Spanned::new(
                                    ast::PatternKind::Name(self.name(&field.name)),
                                    self.span(field.span),
                                )
                            },
                            span: self.span(field.span),
                        })
                        .collect(),
                    open: *rest,
                }
            }
            uhura_syntax::ast::PatternKind::Alternative(values) => ast::PatternKind::Alternative(
                values.iter().map(|value| self.pattern(value)).collect(),
            ),
        };
        ast::Spanned::new(value, span)
    }

    fn qualified_value_name(&self, value: &uhura_syntax::ast::QualifiedName) -> ast::Name {
        let last = value.segments.last().expect("parser emits non-empty names");
        let text = match last.text.as_str() {
            "Some" => "some",
            "None" => "none",
            value => value,
        };
        let text = if value.segments.len() == 1 {
            self.resolved_text(text)
        } else {
            text
        };
        ast::Spanned::new(text.into(), self.span(last.span))
    }

    fn qualified_pattern_name(&self, value: &uhura_syntax::ast::QualifiedName) -> ast::Name {
        self.qualified_value_name(value)
    }

    fn qualified_callee(&self, value: &uhura_syntax::ast::Expression) -> Option<Vec<String>> {
        let uhura_syntax::ast::ExpressionKind::Name(value) = &value.kind else {
            return None;
        };
        Some(
            value
                .segments
                .iter()
                .enumerate()
                .map(|(index, segment)| {
                    if index == 0 {
                        self.resolved_text(&segment.text).to_owned()
                    } else {
                        segment.text.clone()
                    }
                })
                .collect(),
        )
    }

    fn is_variant_name(&self, value: &uhura_syntax::ast::QualifiedName) -> bool {
        if value.segments.len() != 2 {
            return false;
        }
        self.variants.contains_key(&(
            self.resolved_text(&value.segments[0].text).to_owned(),
            value.segments[1].text.clone(),
        ))
    }

    fn is_entries_call(&self, value: &uhura_syntax::ast::Expression) -> bool {
        match &value.kind {
            uhura_syntax::ast::ExpressionKind::Call { callee, arguments }
                if arguments.is_empty() =>
            {
                matches!(
                    &callee.kind,
                    uhura_syntax::ast::ExpressionKind::Member { member, .. } if member.text == "entries"
                )
            }
            _ => false,
        }
    }

    fn is_named_call(&self, value: &uhura_syntax::ast::Expression) -> bool {
        matches!(
            &value.kind,
            uhura_syntax::ast::ExpressionKind::Call { callee, .. }
                if matches!(
                    &callee.kind,
                    uhura_syntax::ast::ExpressionKind::Name(name) if name.segments.len() == 1
                )
        )
    }

    fn type_field(&mut self, value: &uhura_syntax::ast::TypedField) -> ast::TypeField {
        ast::TypeField {
            name: self.name(&value.name),
            ty: self.ty(&value.ty),
            span: self.span(value.span),
        }
    }

    fn parameter(&mut self, value: &uhura_syntax::ast::Parameter) -> ast::Parameter {
        ast::Parameter {
            name: self.name(&value.name),
            ty: self.ty(&value.ty),
            span: self.span(value.span),
        }
    }

    fn ty(&mut self, value: &uhura_syntax::ast::TypeExpression) -> ast::TypeExpr {
        let span = self.span(value.span);
        let kind = match &value.kind {
            uhura_syntax::ast::TypeExpressionKind::Path(path) => {
                let Some(last) = path.segments.last() else {
                    self.unsupported(value.span, "type paths must contain a name");
                    return ast::Spanned::new(
                        ast::TypeExprKind::Named {
                            path: vec![ast::Spanned::new("Never".into(), span)],
                            arguments: Vec::new(),
                        },
                        span,
                    );
                };
                if path.segments.len() != 1 {
                    self.unsupported(
                        path.span,
                        "type resolution must complete before lowering qualified cross-module paths",
                    );
                }
                ast::TypeExprKind::Named {
                    path: vec![self.resolved_name(&last.name)],
                    arguments: last
                        .arguments
                        .iter()
                        .map(|argument| self.ty(argument))
                        .collect(),
                }
            }
            uhura_syntax::ast::TypeExpressionKind::Unit => ast::TypeExprKind::Tuple(Vec::new()),
            uhura_syntax::ast::TypeExpressionKind::Tuple(values) => {
                ast::TypeExprKind::Tuple(values.iter().map(|value| self.ty(value)).collect())
            }
        };
        ast::Spanned::new(kind, span)
    }

    fn contains_return(&self, expression: &uhura_syntax::ast::Expression) -> bool {
        match &expression.kind {
            uhura_syntax::ast::ExpressionKind::Return(_) => true,
            uhura_syntax::ast::ExpressionKind::Sequence(values)
            | uhura_syntax::ast::ExpressionKind::Tuple(values) => {
                values.iter().any(|value| self.contains_return(value))
            }
            uhura_syntax::ast::ExpressionKind::Group(value)
            | uhura_syntax::ast::ExpressionKind::Unary { value, .. } => self.contains_return(value),
            uhura_syntax::ast::ExpressionKind::Record(value) => {
                value
                    .fields
                    .iter()
                    .filter_map(|field| field.value.as_ref())
                    .any(|value| self.contains_return(value))
                    || value
                        .base
                        .as_deref()
                        .is_some_and(|value| self.contains_return(value))
            }
            uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => {
                entries.iter().any(|entry| {
                    self.contains_return(&entry.key) || self.contains_return(&entry.value)
                })
            }
            uhura_syntax::ast::ExpressionKind::Block(value) => {
                value
                    .statements
                    .iter()
                    .any(|statement| self.statement_contains_return(statement))
                    || value
                        .tail
                        .as_deref()
                        .is_some_and(|value| self.contains_return(value))
            }
            uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
                self.contains_return(callee)
                    || arguments.iter().any(|argument| match argument {
                        uhura_syntax::ast::CallArgument::Expression(value) => {
                            self.contains_return(value)
                        }
                        uhura_syntax::ast::CallArgument::Binder(value) => {
                            self.contains_return(&value.body)
                        }
                    })
            }
            uhura_syntax::ast::ExpressionKind::Member { value, .. } => self.contains_return(value),
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
            } => self.contains_return(value) || self.contains_return(index),
            uhura_syntax::ast::ExpressionKind::Is { value, .. } => self.contains_return(value),
            uhura_syntax::ast::ExpressionKind::If(value) => {
                self.contains_return(&value.condition)
                    || value
                        .then_branch
                        .statements
                        .iter()
                        .any(|statement| self.statement_contains_return(statement))
                    || value
                        .then_branch
                        .tail
                        .as_deref()
                        .is_some_and(|value| self.contains_return(value))
                    || value
                        .else_branch
                        .as_ref()
                        .is_some_and(|branch| match branch {
                            uhura_syntax::ast::ElseBranch::Block(block) => {
                                block
                                    .statements
                                    .iter()
                                    .any(|statement| self.statement_contains_return(statement))
                                    || block
                                        .tail
                                        .as_deref()
                                        .is_some_and(|value| self.contains_return(value))
                            }
                            uhura_syntax::ast::ElseBranch::If(value) => self.contains_return(value),
                        })
            }
            uhura_syntax::ast::ExpressionKind::Match(value) => {
                self.contains_return(&value.value)
                    || value
                        .arms
                        .iter()
                        .any(|arm| self.contains_return(&arm.value))
            }
            uhura_syntax::ast::ExpressionKind::Literal(_)
            | uhura_syntax::ast::ExpressionKind::Unit
            | uhura_syntax::ast::ExpressionKind::Name(_) => false,
        }
    }

    fn block_contains_return(&self, block: &uhura_syntax::ast::Block) -> bool {
        block
            .statements
            .iter()
            .any(|statement| self.statement_contains_return(statement))
            || block
                .tail
                .as_deref()
                .is_some_and(|value| self.contains_return(value))
    }

    fn statement_contains_return(&self, statement: &uhura_syntax::ast::Statement) -> bool {
        match &statement.kind {
            uhura_syntax::ast::StatementKind::Let { value, .. }
            | uhura_syntax::ast::StatementKind::Assign { value, .. } => self.contains_return(value),
            uhura_syntax::ast::StatementKind::Emit { output, .. } => output
                .arguments
                .iter()
                .any(|value| self.contains_return(value)),
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                self.contains_return(condition)
                    || self.contains_return(decreases)
                    || body
                        .statements
                        .iter()
                        .any(|statement| self.statement_contains_return(statement))
                    || body
                        .tail
                        .as_deref()
                        .is_some_and(|value| self.contains_return(value))
            }
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                self.contains_return(expression)
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => false,
        }
    }

    fn name(&self, value: &uhura_syntax::ast::Identifier) -> ast::Name {
        ast::Spanned::new(value.text.clone(), self.span(value.span))
    }

    fn resolved_text<'b>(&'b self, value: &'b str) -> &'b str {
        self.bindings.get(value).map_or(value, String::as_str)
    }

    fn resolved_name(&self, value: &uhura_syntax::ast::Identifier) -> ast::Name {
        ast::Spanned::new(
            self.resolved_text(&value.text).to_owned(),
            self.span(value.span),
        )
    }

    fn resolved_path(&self, values: &[uhura_syntax::ast::Identifier]) -> Vec<ast::Name> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                if index == 0 {
                    self.resolved_name(value)
                } else {
                    self.name(value)
                }
            })
            .collect()
    }

    fn span(&self, value: uhura_syntax::ast::Span) -> ast::SourceSpan {
        ast::SourceSpan::new(value.file, value.start, value.end)
    }

    fn unsupported(&mut self, span: uhura_syntax::ast::Span, message: impl Into<String>) {
        self.diagnostics.push(error(
            codes::UNSUPPORTED,
            "uhura-0.4/unsupported",
            message,
            self.span(span),
        ));
    }
}
