//! Pure, canonical Uhura project compilation.
//!
//! Filesystem discovery, symlink policy, lock acquisition, and source
//! admission belong to the embedding CLI or host. Once those adapters have an
//! admitted manifest, exact dependency captures, and an in-memory source
//! inventory, this module owns the remaining frontend pipeline:
//!
//! 1. parse root and dependency core modules,
//! 2. parse manifest-resolved evidence modules with the same frontend,
//! 3. assemble the exact package graph,
//! 4. check and lower it, and
//! 5. return one deterministically ordered diagnostic/provenance result.

use std::collections::BTreeMap;

use uhura_base::{Diagnostic, FileId, Severity, Span, has_errors};

use crate::checker::CheckOutput;
use crate::project_lock::CapturedPackage;
use crate::project_manifest::ProjectManifest;

/// One already-admitted UTF-8 source in the embedding source map.
///
/// `file` must be the identifier assigned by the caller's [`uhura_base::SourceMap`].
/// Paths are project-relative and include a dependency's captured source root.
#[derive(Clone, Copy, Debug)]
pub struct ProjectSource<'a> {
    pub file: FileId,
    pub path: &'a str,
    pub text: &'a str,
}

impl<'a> ProjectSource<'a> {
    pub const fn new(file: FileId, path: &'a str, text: &'a str) -> Self {
        Self { file, path, text }
    }
}

/// Compile one admitted, lock-resolved Uhura 0.4 project without performing IO.
///
/// Dependency captures must already have passed lock validation. The source
/// inventory is nevertheless cross-checked against each captured module so a
/// caller cannot accidentally compile bytes different from the locked bytes.
///
/// This function deliberately stops at the language boundary. Registry-backed
/// resource checks are the next admission hook: after a successful result,
/// callers with checked project resources should run checks such as
/// [`crate::check_program_icon_tokens`] against the returned program before
/// publishing or executing it.
pub fn compile_project(
    manifest: &ProjectManifest,
    sources: &[ProjectSource<'_>],
    dependencies: &[CapturedPackage],
) -> CheckOutput {
    let (source_by_path, mut diagnostics) = index_sources(sources);
    let root_package = manifest.project.package_id().to_string();
    let mut packages = Vec::with_capacity(dependencies.len() + 1);

    let root_modules = manifest
        .modules
        .iter()
        .filter_map(|(logical, physical)| {
            let source = required_source(
                &source_by_path,
                physical.as_str(),
                format!(
                    "project.modules.{logical}: mapped source `{}` is missing from the admitted inventory",
                    physical.as_str()
                ),
                &mut diagnostics,
            )?;
            Some(parse_core_module(
                source,
                &root_package,
                logical.as_str(),
                None,
                &mut diagnostics,
            ))
        })
        .collect();
    packages.push(crate::source::CapturedPackageModules {
        package: root_package.clone(),
        dependencies: manifest
            .dependencies
            .iter()
            .map(|(alias, dependency)| {
                (
                    alias.as_str().to_string(),
                    dependency.package_id().to_string(),
                )
            })
            .collect(),
        modules: root_modules,
    });

    let mut dependencies = dependencies.iter().collect::<Vec<_>>();
    dependencies.sort_by_key(|capture| capture.package_id().to_string());
    for capture in dependencies {
        let package = capture.package_id().to_string();
        let mut modules = Vec::with_capacity(capture.manifest.modules.len());
        for (logical, physical) in &capture.manifest.modules {
            let global_path = format!("{}/{}", capture.source.as_str(), physical.as_str());
            let Some(source) = required_source(
                &source_by_path,
                &global_path,
                format!(
                    "package.{package}.modules.{logical}: mapped source `{global_path}` is missing from the admitted inventory"
                ),
                &mut diagnostics,
            ) else {
                continue;
            };
            let Some(captured_bytes) = capture.modules.get(logical) else {
                diagnostics.push(project_contract_error(
                    format!(
                        "package.{package}.modules.{logical}: the lock capture has no bytes for `{global_path}`"
                    ),
                    source_span(source),
                ));
                continue;
            };
            if captured_bytes.as_slice() != source.text.as_bytes() {
                diagnostics.push(project_contract_error(
                    format!(
                        "package.{package}.modules.{logical}: admitted source `{global_path}` differs from the lock-captured bytes"
                    ),
                    source_span(source),
                ));
                continue;
            }
            modules.push(parse_core_module(
                source,
                &package,
                logical.as_str(),
                Some(format!("package.{package}.modules.{logical}")),
                &mut diagnostics,
            ));
        }
        packages.push(crate::source::CapturedPackageModules {
            package,
            dependencies: capture
                .resolved_dependencies
                .iter()
                .map(|(alias, dependency)| (alias.as_str().to_string(), dependency.to_string()))
                .collect(),
            modules,
        });
    }

    let evidence_modules = if manifest.evidence.is_empty() {
        Vec::new()
    } else {
        manifest
            .evidence
            .iter()
            .filter_map(|(logical, physical)| {
                let source = required_source(
                    &source_by_path,
                    physical.as_str(),
                    format!(
                        "project.evidence.modules.{}: mapped source `{}` is missing from the admitted inventory",
                        logical,
                        physical.as_str()
                    ),
                    &mut diagnostics,
                )?;
                Some(parse_core_module(
                    source,
                    &root_package,
                    logical.as_str(),
                    Some(format!("project.evidence.modules.{logical}")),
                    &mut diagnostics,
                ))
            })
            .collect()
    };

    canonicalize_diagnostics(&mut diagnostics);
    if has_errors(&diagnostics) {
        return failed_output(diagnostics);
    }

    let mut output = crate::source::check_package_graph_with_evidence(
        &root_package,
        &packages,
        &evidence_modules,
    );
    output.diagnostics.extend(diagnostics);
    canonicalize_diagnostics(&mut output.diagnostics);
    if has_errors(&output.diagnostics) {
        output.program = None;
    }
    output
}

fn index_sources<'a>(
    sources: &'a [ProjectSource<'a>],
) -> (BTreeMap<&'a str, &'a ProjectSource<'a>>, Vec<Diagnostic>) {
    let mut by_path = BTreeMap::new();
    let mut by_file = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for source in sources {
        if let Some(previous) = by_path.insert(source.path, source) {
            diagnostics.push(project_contract_error(
                format!(
                    "admitted source path `{}` occurs more than once (file ids {} and {})",
                    source.path, previous.file.0, source.file.0
                ),
                source_span(source),
            ));
        }
        if let Some(previous) = by_file.insert(source.file, source.path)
            && previous != source.path
        {
            diagnostics.push(project_contract_error(
                format!(
                    "admitted file id {} is shared by `{previous}` and `{}`",
                    source.file.0, source.path
                ),
                source_span(source),
            ));
        }
    }
    (by_path, diagnostics)
}

fn required_source<'a>(
    sources: &BTreeMap<&'a str, &'a ProjectSource<'a>>,
    path: &str,
    missing: String,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<&'a ProjectSource<'a>> {
    let source = sources.get(path).copied();
    if source.is_none() {
        diagnostics.push(project_contract_error(missing, Span::new(FileId(0), 0, 0)));
    }
    source
}

fn parse_core_module(
    source: &ProjectSource<'_>,
    package: &str,
    logical: &str,
    context: Option<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> uhura_syntax::Module {
    let parsed = uhura_syntax::parse(
        uhura_syntax::SourceIdentity::new(source.file.0, package, logical, source.path),
        source.text,
    );
    diagnostics.extend(parsed.diagnostics.into_iter().map(|diagnostic| {
        let mut diagnostic = diagnostic.into_public_diagnostic();
        if let Some(context) = &context {
            diagnostic.message = format!("{context}: {}", diagnostic.message);
        }
        diagnostic
    }));
    parsed.module
}

fn project_contract_error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::new(
        "UH2001",
        "contract/invalid-project",
        Severity::Error,
        message,
        span,
    )
}

fn source_span(source: &ProjectSource<'_>) -> Span {
    Span::new(
        source.file,
        0,
        source.text.len().min(u32::MAX as usize) as u32,
    )
}

fn canonicalize_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|left, right| {
        (
            left.span.file,
            left.span.start,
            left.span.end,
            left.code,
            left.rule,
            left.severity,
            left.message.as_str(),
        )
            .cmp(&(
                right.span.file,
                right.span.start,
                right.span.end,
                right.code,
                right.rule,
                right.severity,
                right.message.as_str(),
            ))
    });
}

fn failed_output(diagnostics: Vec<Diagnostic>) -> CheckOutput {
    CheckOutput {
        program: None,
        diagnostics,
        provenance: None,
        authoring: crate::AuthoringProjection::default(),
    }
}
