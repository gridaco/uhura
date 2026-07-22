use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use uhura_base::{Diagnostic, FileId, Severity, SourceMap, Span};
use uhura_check::project_lock::{
    CapturedPackage, ProjectLockIssue, check_project_lock, parse_project_lock,
};
use uhura_check::project_manifest::{
    FrameworkProfile, ProjectManifest, ProjectManifestIssue, load_project_manifest,
};
use uhura_check::{CheckOutput, ProjectSource, compile_project};

use crate::web_app::expand_web_app;
use crate::{ProjectSourceSnapshot, RESOLVED_APPLICATION_PROTOCOL};

/// One UTF-8 source admitted from a coherent project snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedSource {
    pub file: FileId,
    pub path: String,
    pub text: String,
    pub kind: AdmittedSourceKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdmittedSourceKind {
    Authored,
    Generated,
}

/// Stable project-layer output available to framework and tooling consumers.
///
/// This artifact records resolution only. It does not change checker or
/// runtime meaning, and explicit projects remain the default profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedApplication {
    pub protocol: &'static str,
    pub source_revision: String,
    pub package: String,
    pub language: String,
    pub profile: ResolvedProfile,
    pub modules: Vec<ResolvedModule>,
    pub evidence_modules: Vec<ResolvedModule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_app: Option<ResolvedWebApplication>,
}

impl ResolvedApplication {
    #[must_use]
    pub fn canonical_json(&self) -> String {
        let value = serde_json::to_value(self).expect("resolved application is serializable");
        uhura_base::to_canonical_json(&value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ResolvedProfile {
    Explicit,
    WebApp { version: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedModule {
    pub logical: String,
    pub path: String,
    pub source_kind: AdmittedSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<ResolvedUiRole>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolvedUiRole {
    Page,
    Component,
    Surface,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRoute {
    pub constructor: String,
    pub pattern: String,
    pub parameters: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedUiSubject {
    pub role: ResolvedUiRole,
    pub logical: String,
    pub path: String,
    pub declaration: String,
    pub declaration_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<ResolvedRoute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_logical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedWebApplication {
    pub machine: String,
    pub location: String,
    pub application: String,
    pub application_module: String,
    pub route_table: String,
    pub route_module: String,
    pub root_page: String,
    pub subjects: Vec<ResolvedUiSubject>,
}

/// A fully admitted project ready for pure checking.
pub struct ResolvedProject {
    manifest: ProjectManifest,
    sources: Vec<AdmittedSource>,
    dependencies: Vec<CapturedPackage>,
    source_map: SourceMap,
    manifest_file: FileId,
    application: ResolvedApplication,
}

impl ResolvedProject {
    #[must_use]
    pub fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    #[must_use]
    pub fn sources(&self) -> &[AdmittedSource] {
        &self.sources
    }

    /// Root-authored sources selected by the augmented root manifest.
    ///
    /// This is the write boundary for authoring tools such as `uhura fmt`:
    /// generated modules and captured dependency sources are both excluded.
    pub fn root_authored_sources(&self) -> impl Iterator<Item = &AdmittedSource> {
        let root_paths = self
            .manifest
            .modules
            .values()
            .chain(self.manifest.evidence.values())
            .map(|path| path.as_str())
            .collect::<BTreeSet<_>>();
        self.sources.iter().filter(move |source| {
            source.kind == AdmittedSourceKind::Authored && root_paths.contains(source.path.as_str())
        })
    }

    /// Every captured physical source admitted to checking.
    ///
    /// Generated modules are excluded, while locked dependency sources remain
    /// available for diagnostics, provenance, and read-only Editor inspection.
    pub fn non_generated_sources(&self) -> impl Iterator<Item = &AdmittedSource> {
        self.sources
            .iter()
            .filter(|source| source.kind == AdmittedSourceKind::Authored)
    }

    #[must_use]
    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    #[must_use]
    pub const fn manifest_file(&self) -> FileId {
        self.manifest_file
    }

    #[must_use]
    pub fn application(&self) -> &ResolvedApplication {
        &self.application
    }

    #[must_use]
    pub fn check(&self) -> CheckOutput {
        let sources = self
            .sources
            .iter()
            .map(|source| ProjectSource::new(source.file, &source.path, &source.text))
            .collect::<Vec<_>>();
        let mut checked = compile_project(&self.manifest, &sources, &self.dependencies);
        if let (Some(application), Some(program)) =
            (&self.application.web_app, checked.program.as_ref())
        {
            let mut diagnostics = framework_router_diagnostics(
                &self.manifest,
                application,
                program,
                &self.sources,
                self.manifest_file,
            );
            diagnostics.extend(framework_evidence_diagnostics(
                application,
                program,
                &self.sources,
                self.manifest_file,
            ));
            if !diagnostics.is_empty() {
                checked.diagnostics.extend(diagnostics);
                checked.program = None;
            }
        }
        checked
    }

    #[must_use]
    pub fn into_source_map(self) -> SourceMap {
        self.source_map
    }
}

fn framework_router_diagnostics(
    manifest: &ProjectManifest,
    application: &ResolvedWebApplication,
    program: &uhura_core::Program,
    sources: &[AdmittedSource],
    manifest_file: FileId,
) -> Vec<Diagnostic> {
    let framework = manifest
        .framework
        .as_ref()
        .expect("web application metadata has framework configuration");
    let machine_id = format!(
        "{}::{}",
        manifest.project.package_id(),
        framework.machine.declaration()
    );
    match selected_web_app_router_port_for(application, program, &machine_id) {
        Ok(_) => Vec::new(),
        Err(message) => {
            let span = program
                .machine_program
                .machines
                .get(&machine_id)
                .map(|machine| {
                    machine
                        .ports
                        .iter()
                        .find(|port| port.contract == uhura_port::ROUTER_CONTRACT_ID)
                        .map_or_else(
                            || source_ref_span(&machine.source, sources, manifest_file),
                            |port| source_ref_span(&port.source, sources, manifest_file),
                        )
                })
                .unwrap_or_else(|| Span::new(manifest_file, 0, 0));
            vec![framework_router_error(span, message)]
        }
    }
}

/// Select the one framework Router port that owns the generated route table.
///
/// Explicit projects return `Ok(None)`. A web application returns one exact
/// port name or an error when its configured machine has zero or multiple
/// Router ports backed by the generated `APPLICATION_ROUTES` value.
pub fn selected_web_app_router_port(
    application: &ResolvedApplication,
    program: &uhura_core::Program,
) -> Result<Option<String>, String> {
    let Some(web_app) = application.web_app.as_ref() else {
        return Ok(None);
    };
    let declaration = web_app
        .machine
        .rsplit("::")
        .next()
        .expect("validated framework machine locator has a declaration");
    let machine_id = format!("{}::{declaration}", application.package);
    selected_web_app_router_port_for(web_app, program, &machine_id).map(Some)
}

fn selected_web_app_router_port_for(
    application: &ResolvedWebApplication,
    program: &uhura_core::Program,
    machine_id: &str,
) -> Result<String, String> {
    let machine = program
        .machine_program
        .machines
        .get(machine_id)
        .ok_or_else(|| format!("framework.machine resolved no checked machine `{machine_id}`"))?;
    let routers = machine
        .ports
        .iter()
        .filter(|port| port.contract == uhura_port::ROUTER_CONTRACT_ID)
        .collect::<Vec<_>>();
    let matching = routers
        .iter()
        .filter(|port| {
            matches!(
                &port.configuration,
                Some(uhura_core::ir::Expr::Name { name }) if name == &application.route_table
            )
        })
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [port] => Ok(port.name.clone()),
        [] if routers.is_empty() => Err(format!(
            "framework machine `{machine_id}` must declare a Router port configured with generated `{}`",
            application.route_table
        )),
        [] => {
            let configured = routers
                .iter()
                .map(|port| match &port.configuration {
                    Some(uhura_core::ir::Expr::Name { name }) => format!("`{name}`"),
                    _ => "a non-route-table expression".to_owned(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "framework machine `{machine_id}` configures Router port route table {configured}; expected generated `{}`",
                application.route_table
            ))
        }
        ports => Err(format!(
            "framework machine `{machine_id}` must have exactly one Router port configured with generated `{}`; found [{}]",
            application.route_table,
            ports
                .iter()
                .map(|port| port.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn framework_router_error(span: Span, message: String) -> Diagnostic {
    Diagnostic::new(
        uhura_base::codes::machine::PORT,
        "uhura/framework-router",
        Severity::Error,
        message,
        span,
    )
}

fn source_ref_span(
    source: &uhura_core::ir::SourceRef,
    sources: &[AdmittedSource],
    fallback: FileId,
) -> Span {
    sources
        .iter()
        .find(|candidate| candidate.path == source.path)
        .map(|candidate| Span::new(candidate.file, source.start, source.end))
        .unwrap_or_else(|| Span::new(fallback, 0, 0))
}

fn framework_evidence_diagnostics(
    application: &ResolvedWebApplication,
    program: &uhura_core::Program,
    sources: &[AdmittedSource],
    manifest_file: FileId,
) -> Vec<Diagnostic> {
    let subjects = application
        .subjects
        .iter()
        .map(|subject| (subject.declaration_id.as_str(), subject))
        .collect::<BTreeMap<_, _>>();
    let sibling_subjects = application
        .subjects
        .iter()
        .filter_map(|subject| {
            subject
                .evidence_path
                .as_deref()
                .map(|path| (path, subject.declaration_id.as_str()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = Vec::new();

    for (example, metadata) in &program.evidence.example_metadata {
        let source = program.evidence.example_sources.get(example);
        let span = source.map_or_else(
            || Span::new(manifest_file, 0, 0),
            |source| source_ref_span(source, sources, manifest_file),
        );
        let Some(presentation) = metadata.presentation.as_deref() else {
            diagnostics.push(framework_evidence_error(
                span,
                format!(
                    "framework evidence example `{example}` must target a discovered page, component, or surface"
                ),
            ));
            continue;
        };
        let Some(subject) = subjects.get(presentation) else {
            diagnostics.push(framework_evidence_error(
                span,
                format!(
                    "framework evidence example `{example}` targets `{presentation}`, which is not a discovered page, component, or surface"
                ),
            ));
            continue;
        };
        let declared = evidence_role(metadata.kind);
        if declared != subject.role {
            diagnostics.push(framework_evidence_error(
                span,
                format!(
                    "framework evidence example `{example}` declares `{}` for `{presentation}`, but its discovered role is `{}`",
                    role_name(declared),
                    role_name(subject.role),
                ),
            ));
            continue;
        }
        if let Some(expected) = source
            .and_then(|source| sibling_subjects.get(source.path.as_str()))
            .copied()
            && presentation != expected
        {
            diagnostics.push(framework_evidence_error(
                span,
                format!(
                    "framework sibling evidence example `{example}` targets `{presentation}`, but its colocated subject is `{expected}`"
                ),
            ));
        }
    }

    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.message.clone(),
        )
    });
    diagnostics
}

fn evidence_role(kind: Option<uhura_core::EvidencePresentationKind>) -> ResolvedUiRole {
    match kind {
        Some(uhura_core::EvidencePresentationKind::Component) => ResolvedUiRole::Component,
        Some(uhura_core::EvidencePresentationKind::Surface) => ResolvedUiRole::Surface,
        Some(uhura_core::EvidencePresentationKind::Page) | None => ResolvedUiRole::Page,
    }
}

const fn role_name(role: ResolvedUiRole) -> &'static str {
    match role {
        ResolvedUiRole::Page => "page",
        ResolvedUiRole::Component => "component",
        ResolvedUiRole::Surface => "surface",
    }
}

fn framework_evidence_error(span: Span, message: String) -> Diagnostic {
    Diagnostic::new(
        "R1004",
        "uhura/framework-evidence-role",
        Severity::Error,
        message,
        span,
    )
}

/// Admission failure paired with the exact source map used by its spans.
pub struct ProjectRejection {
    pub source_map: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
}

/// Resolve one coherent project snapshot, including the selected root-only
/// framework profile when present.
pub fn resolve_project(
    snapshot: &ProjectSourceSnapshot,
) -> Result<ResolvedProject, ProjectRejection> {
    let capture_failures = snapshot.capture_failure_messages();
    if !capture_failures.is_empty() {
        let mut source_map = SourceMap::new();
        let manifest_file = source_map.add("uhura.toml", "");
        return Err(rejection(
            source_map,
            capture_failures
                .into_iter()
                .map(|message| format!("could not capture a complete project snapshot: {message}")),
            manifest_file,
            "UH9000",
            "uhura/source",
        ));
    }

    let source_entries = match snapshot.files.sources() {
        Ok(sources) => sources,
        Err(message) => {
            let mut source_map = SourceMap::new();
            let manifest_file = source_map.add("uhura.toml", "");
            return Err(rejection(
                source_map,
                [message],
                manifest_file,
                "UH2001",
                "contract/invalid-project",
            ));
        }
    };
    let mut source_map = SourceMap::new();
    let mut sources = source_entries
        .into_iter()
        .map(|(path, text)| {
            let file = source_map.add(path.clone(), text.clone());
            AdmittedSource {
                file,
                path,
                text,
                kind: AdmittedSourceKind::Authored,
            }
        })
        .collect::<Vec<_>>();

    let manifest_text = match snapshot.files.text("uhura.toml") {
        Ok(text) => text.unwrap_or_default(),
        Err(message) => {
            let manifest_file = source_map.add("uhura.toml", "");
            return Err(rejection(
                source_map,
                [format!("uhura.toml: {message}")],
                manifest_file,
                "UH2001",
                "contract/invalid-project",
            ));
        }
    };
    let manifest_file = source_map.add("uhura.toml", manifest_text.clone());
    let mut manifest = match load_project_manifest(&manifest_text) {
        Ok(manifest) => manifest,
        Err(issues) => {
            return Err(rejection(
                source_map,
                manifest_issue_messages(issues),
                manifest_file,
                "UH2001",
                "contract/invalid-project",
            ));
        }
    };

    let dependencies = match capture_dependencies(snapshot, &sources, &manifest) {
        Ok(dependencies) => dependencies,
        Err(messages) => {
            return Err(rejection(
                source_map,
                messages,
                manifest_file,
                "UH2001",
                "contract/invalid-project",
            ));
        }
    };
    let dependency_roots = dependencies
        .iter()
        .map(|package| package.source.as_str())
        .collect::<Vec<_>>();
    let web_app = if manifest.framework.is_some() {
        match expand_web_app(
            &mut manifest,
            &mut sources,
            &mut source_map,
            manifest_file,
            &dependency_roots,
        ) {
            Ok(application) => Some(application),
            Err(mut diagnostics) => {
                diagnostics.sort_by_key(|diagnostic| {
                    (
                        diagnostic.span.file.0,
                        diagnostic.span.start,
                        diagnostic.span.end,
                        diagnostic.code,
                        diagnostic.message.clone(),
                    )
                });
                return Err(ProjectRejection {
                    source_map,
                    diagnostics,
                });
            }
        }
    } else {
        None
    };
    let messages = validate_source_inventory(snapshot, &sources, &manifest, &dependency_roots);
    if !messages.is_empty() {
        return Err(rejection(
            source_map,
            messages,
            manifest_file,
            "UH2001",
            "contract/invalid-project",
        ));
    }

    let profile = manifest
        .framework
        .as_ref()
        .map_or(ResolvedProfile::Explicit, |framework| {
            match framework.profile {
                FrameworkProfile::WebApp => ResolvedProfile::WebApp {
                    version: framework.version,
                },
            }
        });
    let application = ResolvedApplication {
        protocol: RESOLVED_APPLICATION_PROTOCOL,
        source_revision: snapshot.source_revision_id().to_owned(),
        package: manifest.project.package_id().to_string(),
        language: manifest.project.language.clone(),
        profile,
        modules: resolved_modules(&manifest.modules, &sources, web_app.as_ref(), false),
        evidence_modules: resolved_modules(&manifest.evidence, &sources, web_app.as_ref(), true),
        web_app,
    };

    Ok(ResolvedProject {
        manifest,
        sources,
        dependencies,
        source_map,
        manifest_file,
        application,
    })
}

fn resolved_modules(
    modules: &BTreeMap<
        uhura_check::project_manifest::LogicalModulePath,
        uhura_check::project_manifest::ProjectPath,
    >,
    sources: &[AdmittedSource],
    web_app: Option<&ResolvedWebApplication>,
    evidence: bool,
) -> Vec<ResolvedModule> {
    modules
        .iter()
        .map(|(logical, path)| ResolvedModule {
            logical: logical.as_str().to_owned(),
            path: path.as_str().to_owned(),
            source_kind: sources
                .iter()
                .find(|source| source.path == path.as_str())
                .expect("resolved manifest module has one admitted source")
                .kind,
            role: web_app.and_then(|application| {
                application.subjects.iter().find_map(|subject| {
                    let role_path = if evidence {
                        subject.evidence_path.as_deref()
                    } else {
                        Some(subject.path.as_str())
                    };
                    (role_path == Some(path.as_str())).then_some(subject.role)
                })
            }),
        })
        .collect()
}

fn rejection(
    source_map: SourceMap,
    messages: impl IntoIterator<Item = String>,
    file: FileId,
    code: &'static str,
    rule: &'static str,
) -> ProjectRejection {
    let mut diagnostics = messages
        .into_iter()
        .map(|message| Diagnostic::new(code, rule, Severity::Error, message, Span::new(file, 0, 0)))
        .collect::<Vec<_>>();
    diagnostics.sort_by(|left, right| left.message.cmp(&right.message));
    ProjectRejection {
        source_map,
        diagnostics,
    }
}

fn manifest_issue_messages(issues: Vec<ProjectManifestIssue>) -> Vec<String> {
    issues
        .into_iter()
        .map(|issue| {
            if issue.path.is_empty() {
                issue.message
            } else {
                format!("{}: {}", issue.path, issue.message)
            }
        })
        .collect()
}

fn capture_dependencies(
    snapshot: &ProjectSourceSnapshot,
    sources: &[AdmittedSource],
    manifest: &ProjectManifest,
) -> Result<Vec<CapturedPackage>, Vec<String>> {
    let lock_text = snapshot
        .files
        .text("uhura.lock")
        .map_err(|message| vec![message])?;
    if manifest.dependencies.is_empty() {
        return check_project_lock(manifest, lock_text.as_deref(), &[])
            .map(|_| Vec::new())
            .map_err(lock_issue_messages);
    }
    let lock = parse_project_lock(
        lock_text
            .as_deref()
            .ok_or_else(|| vec!["uhura.lock: lock file is required".to_owned()])?,
    )
    .map_err(lock_issue_messages)?;
    let dependency_roots = lock
        .packages
        .values()
        .map(|record| record.source.path.as_str())
        .collect::<Vec<_>>();
    let mut captured = Vec::new();
    let mut messages = Vec::new();
    for record in lock.packages.values() {
        let manifest_path = format!("{}/uhura.toml", record.source.path);
        let manifest_text = match snapshot.files.text(&manifest_path) {
            Ok(Some(text)) => text,
            Ok(None) => {
                messages.push(format!(
                    "package.{}.manifest: `{manifest_path}` is missing",
                    record.package
                ));
                continue;
            }
            Err(error) => {
                messages.push(format!("package.{}.manifest: {error}", record.package));
                continue;
            }
        };
        let package_manifest = match load_project_manifest(&manifest_text) {
            Ok(manifest) => manifest,
            Err(issues) => {
                messages.extend(issues.into_iter().map(|issue| {
                    format!(
                        "package.{}.manifest.{}: {}",
                        record.package, issue.path, issue.message
                    )
                }));
                continue;
            }
        };
        if package_manifest.framework.is_some() {
            messages.push(format!(
                "package.{}.manifest.framework: framework profiles are root-project configuration",
                record.package
            ));
            continue;
        }
        let declared_sources = package_manifest
            .modules
            .values()
            .chain(package_manifest.evidence.values())
            .map(|path| format!("{}/{}", record.source.path, path))
            .collect::<BTreeSet<_>>();
        let discovered_sources = sources
            .iter()
            .filter(|source| {
                owning_dependency_root(&source.path, &dependency_roots)
                    == Some(record.source.path.as_str())
            })
            .map(|source| source.path.clone())
            .collect::<BTreeSet<_>>();
        for unlisted in discovered_sources.difference(&declared_sources) {
            messages.push(format!(
                "package.{}.sources: `{unlisted}` is not listed in `[modules]` or `[evidence.modules]`",
                record.package
            ));
        }
        let mut module_bytes = BTreeMap::new();
        for (logical, physical) in &package_manifest.modules {
            let global = format!("{}/{}", record.source.path, physical);
            let Some(source) = sources.iter().find(|source| source.path == global) else {
                messages.push(format!(
                    "package.{}.modules.{}: mapped source `{global}` is missing",
                    record.package, logical
                ));
                continue;
            };
            module_bytes.insert(logical.clone(), source.text.as_bytes().to_vec());
        }
        let resolved_dependencies = package_manifest
            .dependencies
            .iter()
            .map(|(alias, dependency)| (alias.clone(), dependency.package_id()))
            .collect();
        captured.push(CapturedPackage {
            manifest: package_manifest,
            source: record.source.path.clone(),
            modules: module_bytes,
            resolved_dependencies,
            resources: BTreeMap::new(),
        });
    }
    if !messages.is_empty() {
        messages.sort();
        return Err(messages);
    }
    check_project_lock(manifest, lock_text.as_deref(), &captured).map_err(lock_issue_messages)?;
    Ok(captured)
}

fn lock_issue_messages(issues: Vec<ProjectLockIssue>) -> Vec<String> {
    issues
        .into_iter()
        .map(|issue| {
            if issue.path.is_empty() {
                issue.message
            } else {
                format!("{}: {}", issue.path, issue.message)
            }
        })
        .collect()
}

fn path_is_within(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn owning_dependency_root<'a>(path: &str, roots: &[&'a str]) -> Option<&'a str> {
    roots
        .iter()
        .copied()
        .filter(|root| path_is_within(path, root))
        .max_by_key(|root| root.len())
}

fn validate_source_inventory(
    snapshot: &ProjectSourceSnapshot,
    sources: &[AdmittedSource],
    manifest: &ProjectManifest,
    dependency_roots: &[&str],
) -> Vec<String> {
    let mut messages = Vec::new();
    let declared = manifest
        .modules
        .values()
        .chain(manifest.evidence.values())
        .map(|path| path.as_str())
        .collect::<BTreeSet<_>>();
    let discovered = sources
        .iter()
        .filter(|source| {
            !dependency_roots
                .iter()
                .any(|root| path_is_within(&source.path, root))
        })
        .map(|source| source.path.as_str())
        .collect::<BTreeSet<_>>();
    for missing in declared.difference(&discovered) {
        messages.push(format!(
            "mapped Uhura 0.4 source `{missing}` is missing from the project"
        ));
    }
    for unlisted in discovered.difference(&declared) {
        messages.push(format!(
            "Uhura 0.4 source `{unlisted}` is not listed in `[modules]` or `[evidence.modules]`"
        ));
    }
    for aliases in snapshot.files.duplicate_sources() {
        messages.push(format!(
            "Uhura source paths {} resolve to the same physical file",
            aliases
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    messages.sort();
    messages
}
