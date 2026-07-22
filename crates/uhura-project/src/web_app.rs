use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Diagnostic, Severity, SourceMap, Span};
use uhura_check::project_manifest::{
    FrameworkProfile, LogicalModulePath, ProjectManifest, ProjectPath,
};
use uhura_syntax::ast::{DeclarationKind, UiBinding, Visibility};
use uhura_syntax::{SourceIdentity, parse};

use crate::resolve::{
    AdmittedSource, AdmittedSourceKind, ResolvedRoute, ResolvedUiRole, ResolvedUiSubject,
    ResolvedWebApplication,
};

const GENERATED_ROUTE_LOGICAL: &str = "framework::routes";
const GENERATED_ROUTE_PATH: &str = ".uhura/generated/web-app/routes.uhura";
const GENERATED_APPLICATION_LOGICAL: &str = "framework::application";
const GENERATED_APPLICATION_PATH: &str = ".uhura/generated/web-app/application.uhura";

#[derive(Clone, Debug)]
struct DiscoveredSubject {
    role: ResolvedUiRole,
    logical: String,
    path: String,
    declaration: String,
    route: Option<ResolvedRoute>,
    evidence_logical: Option<String>,
    evidence_path: Option<String>,
}

#[derive(Clone, Debug)]
struct EvidenceCandidate {
    file: uhura_base::FileId,
    role: ResolvedUiRole,
    logical: String,
    path: String,
    subject_path: String,
}

pub(crate) fn expand_web_app(
    manifest: &mut ProjectManifest,
    sources: &mut Vec<AdmittedSource>,
    source_map: &mut SourceMap,
    manifest_file: uhura_base::FileId,
    dependency_roots: &[&str],
) -> Result<ResolvedWebApplication, Vec<Diagnostic>> {
    let framework = manifest
        .framework
        .clone()
        .expect("web-app expansion requires a selected framework");
    debug_assert_eq!(framework.profile, FrameworkProfile::WebApp);

    let mut diagnostics = Vec::new();
    if framework.machine.module() == framework.location.module() {
        diagnostics.push(project_error(
            manifest_file,
            "framework.location must be declared outside framework.machine's logical module; the machine imports the generated route table",
        ));
    }

    let explicit_modules = manifest
        .modules
        .iter()
        .map(|(logical, path)| (logical.as_str().to_owned(), path.as_str().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let explicit_evidence = manifest
        .evidence
        .iter()
        .map(|(logical, path)| (logical.as_str().to_owned(), path.as_str().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let explicitly_mapped_paths = explicit_modules
        .values()
        .chain(explicit_evidence.values())
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut subjects = Vec::new();
    let mut evidence = Vec::new();
    for source in sources.iter().filter(|source| {
        source.kind == AdmittedSourceKind::Authored
            && !dependency_roots
                .iter()
                .any(|root| path_is_within(&source.path, root))
    }) {
        let candidate = if source.path == "app/page.uhura" {
            Some(discover_page(&source.path, &[]))
        } else if let Some(relative) = source.path.strip_prefix("app/") {
            discover_app_source(&source.path, relative, source.file, &mut diagnostics)
        } else if let Some(relative) = source.path.strip_prefix("components/") {
            discover_named_source(
                ResolvedUiRole::Component,
                "components",
                &source.path,
                relative,
                source.file,
                &mut diagnostics,
            )
        } else if let Some(relative) = source.path.strip_prefix("surfaces/") {
            discover_named_source(
                ResolvedUiRole::Surface,
                "surfaces",
                &source.path,
                relative,
                source.file,
                &mut diagnostics,
            )
        } else {
            None
        };

        match candidate {
            Some(Discovery::Subject(subject)) => subjects.push((source.file, subject)),
            Some(Discovery::Evidence(candidate)) => evidence.push(candidate),
            None => {}
        }
    }

    if !subjects
        .iter()
        .any(|(_, subject)| subject.path == "app/page.uhura")
    {
        diagnostics.push(project_error(
            manifest_file,
            "framework web-app@1 requires root page `app/page.uhura`",
        ));
    }

    let subject_paths = subjects
        .iter()
        .map(|(_, subject)| subject.path.clone())
        .collect::<BTreeSet<_>>();
    for candidate in evidence {
        if !subject_paths.contains(&candidate.subject_path) {
            diagnostics.push(project_error(
                candidate.file,
                format!(
                    "framework evidence `{}` is orphaned; expected sibling `{}`",
                    candidate.path, candidate.subject_path
                ),
            ));
            continue;
        }
        if let Some((_, subject)) = subjects
            .iter_mut()
            .find(|(_, subject)| subject.path == candidate.subject_path)
        {
            debug_assert_eq!(subject.role, candidate.role);
            subject.evidence_logical = Some(candidate.logical);
            subject.evidence_path = Some(candidate.path);
        }
    }

    let mut logical_owners = explicit_modules
        .iter()
        .map(|(logical, path)| (logical.clone(), format!("[modules] `{path}`")))
        .chain(
            explicit_evidence
                .iter()
                .map(|(logical, path)| (logical.clone(), format!("[evidence.modules] `{path}`"))),
        )
        .collect::<BTreeMap<_, _>>();
    let mut declaration_owners = BTreeMap::<String, String>::new();
    let mut route_shapes = BTreeMap::<String, String>::new();

    for (file, subject) in &mut subjects {
        if explicitly_mapped_paths.contains(&subject.path) {
            diagnostics.push(Diagnostic::new(
                "UH2001",
                "contract/invalid-project",
                Severity::Error,
                format!(
                    "framework-owned source `{}` must not also be mapped explicitly",
                    subject.path
                ),
                Span::new(*file, 0, 0),
            ));
        }
        reserve_logical(
            &mut logical_owners,
            &subject.logical,
            &subject.path,
            *file,
            &mut diagnostics,
        );
        if let Some(evidence_logical) = &subject.evidence_logical {
            let evidence_path = subject
                .evidence_path
                .as_deref()
                .expect("evidence logical and physical paths are paired");
            if explicitly_mapped_paths.contains(evidence_path) {
                diagnostics.push(project_error(
                    source_file(sources, evidence_path)
                        .expect("discovered evidence belongs to an admitted source"),
                    format!(
                        "framework-owned evidence `{evidence_path}` must not also be mapped explicitly"
                    ),
                ));
            }
            let evidence_file = source_file(sources, evidence_path)
                .expect("discovered evidence belongs to an admitted source");
            reserve_logical(
                &mut logical_owners,
                evidence_logical,
                evidence_path,
                evidence_file,
                &mut diagnostics,
            );
        }

        let Some(source) = sources.iter().find(|source| source.file == *file) else {
            continue;
        };
        if let Some(declaration) = validate_role_source(
            source,
            &manifest.project.package_id().to_string(),
            &subject.logical,
            subject.role,
            &mut diagnostics,
        ) {
            subject.declaration = declaration;
            if let Some(route) = subject.route.as_mut() {
                route.constructor = subject
                    .declaration
                    .strip_suffix("Page")
                    .expect("validated page declaration ends in Page")
                    .to_owned();
            }
            if let Some(previous) =
                declaration_owners.insert(subject.declaration.clone(), subject.path.clone())
            {
                diagnostics.push(Diagnostic::new(
                    "UH2001",
                    "contract/invalid-project",
                    Severity::Error,
                    format!(
                        "framework declarations `{}` and `{}` both publish `{}`",
                        previous, subject.path, subject.declaration
                    ),
                    Span::new(*file, 0, 0),
                ));
            }
        }
        if let Some(route) = &subject.route {
            let shape = route_shape(&route.pattern);
            if let Some(previous) = route_shapes.insert(shape, subject.path.clone()) {
                diagnostics.push(Diagnostic::new(
                    "UH2001",
                    "contract/invalid-project",
                    Severity::Error,
                    format!(
                        "page routes `{previous}` and `{}` have the same match shape",
                        subject.path
                    ),
                    Span::new(*file, 0, 0),
                ));
            }
        }
    }

    for (logical, path) in [
        (GENERATED_ROUTE_LOGICAL, GENERATED_ROUTE_PATH),
        (GENERATED_APPLICATION_LOGICAL, GENERATED_APPLICATION_PATH),
    ] {
        if explicitly_mapped_paths.contains(path)
            || sources.iter().any(|source| source.path == path)
        {
            diagnostics.push(project_error(
                manifest_file,
                format!("framework reserved generated path `{path}` is already occupied"),
            ));
        }
        if let Some(previous) = logical_owners.insert(logical.to_owned(), path.to_owned()) {
            diagnostics.push(project_error(
                manifest_file,
                format!("framework generated logical module `{logical}` collides with {previous}"),
            ));
        }
    }

    let available_modules = logical_owners.keys().cloned().collect::<BTreeSet<_>>();
    for (label, locator) in [
        ("framework.machine", &framework.machine),
        ("framework.location", &framework.location),
    ] {
        if !available_modules.contains(locator.module().as_str()) {
            diagnostics.push(project_error(
                manifest_file,
                format!(
                    "{label} names unknown logical module `{}`",
                    locator.module()
                ),
            ));
        }
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    subjects.sort_by(|(_, left), (_, right)| {
        role_rank(left.role)
            .cmp(&role_rank(right.role))
            .then_with(|| left.path.cmp(&right.path))
    });
    let package = manifest.project.package_id().to_string();
    let resolved_subjects = subjects
        .iter()
        .map(|(_, subject)| ResolvedUiSubject {
            role: subject.role,
            logical: subject.logical.clone(),
            path: subject.path.clone(),
            declaration: subject.declaration.clone(),
            declaration_id: format!("{package}::{}", subject.declaration),
            route: subject.route.clone(),
            evidence_logical: subject.evidence_logical.clone(),
            evidence_path: subject.evidence_path.clone(),
        })
        .collect::<Vec<_>>();
    let root_page = resolved_subjects
        .iter()
        .find(|subject| subject.path == "app/page.uhura")
        .expect("validated framework has a root page")
        .declaration_id
        .clone();

    for subject in &resolved_subjects {
        manifest.modules.insert(
            LogicalModulePath::parse(&subject.logical).expect("discovered logical is valid"),
            ProjectPath::parse(&subject.path).expect("captured source path is safe"),
        );
        if let (Some(logical), Some(path)) = (&subject.evidence_logical, &subject.evidence_path) {
            manifest.evidence.insert(
                LogicalModulePath::parse(logical).expect("discovered evidence logical is valid"),
                ProjectPath::parse(path).expect("captured evidence path is safe"),
            );
        }
    }

    let route_source = generated_routes(&framework.location, &resolved_subjects);
    let application_source =
        generated_application(&framework.machine, &framework.location, &resolved_subjects);
    insert_generated(
        manifest,
        sources,
        source_map,
        GENERATED_ROUTE_LOGICAL,
        GENERATED_ROUTE_PATH,
        route_source,
    );
    insert_generated(
        manifest,
        sources,
        source_map,
        GENERATED_APPLICATION_LOGICAL,
        GENERATED_APPLICATION_PATH,
        application_source,
    );

    Ok(ResolvedWebApplication {
        machine: framework.machine.as_str().to_owned(),
        location: framework.location.as_str().to_owned(),
        application: format!("{package}::Application"),
        application_module: GENERATED_APPLICATION_LOGICAL.to_owned(),
        route_table: format!("{package}::APPLICATION_ROUTES"),
        route_module: GENERATED_ROUTE_LOGICAL.to_owned(),
        root_page,
        subjects: resolved_subjects,
    })
}

enum Discovery {
    Subject(DiscoveredSubject),
    Evidence(EvidenceCandidate),
}

fn discover_app_source(
    full_path: &str,
    relative: &str,
    file: uhura_base::FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Discovery> {
    let parts = relative.split('/').collect::<Vec<_>>();
    let (file_name, directories) = parts.split_last()?;
    if !matches!(*file_name, "page.uhura" | "page.examples.uhura") {
        diagnostics.push(Diagnostic::new(
            "UH2001",
            "contract/invalid-project",
            Severity::Error,
            format!(
                "unrecognized framework source `{full_path}`; app directories admit only `page.uhura` and `page.examples.uhura`"
            ),
            Span::new(file, 0, 0),
        ));
        return None;
    }
    let route = match route_from_directories(directories) {
        Ok(route) => route,
        Err(message) => {
            diagnostics.push(Diagnostic::new(
                "UH2001",
                "contract/invalid-project",
                Severity::Error,
                format!("{full_path}: {message}"),
                Span::new(file, 0, 0),
            ));
            return None;
        }
    };
    let logical = page_logical(directories);
    if *file_name == "page.uhura" {
        Some(Discovery::Subject(DiscoveredSubject {
            role: ResolvedUiRole::Page,
            logical,
            path: full_path.to_owned(),
            declaration: String::new(),
            route: Some(route),
            evidence_logical: None,
            evidence_path: None,
        }))
    } else {
        Some(Discovery::Evidence(EvidenceCandidate {
            file,
            role: ResolvedUiRole::Page,
            logical: format!("framework::evidence::{logical}"),
            path: full_path.to_owned(),
            subject_path: if directories.is_empty() {
                "app/page.uhura".to_owned()
            } else {
                format!("app/{}/page.uhura", directories.join("/"))
            },
        }))
    }
}

fn discover_page(full_path: &str, directories: &[&str]) -> Discovery {
    Discovery::Subject(DiscoveredSubject {
        role: ResolvedUiRole::Page,
        logical: page_logical(directories),
        path: full_path.to_owned(),
        declaration: String::new(),
        route: Some(route_from_directories(directories).expect("root route is valid")),
        evidence_logical: None,
        evidence_path: None,
    })
}

fn discover_named_source(
    role: ResolvedUiRole,
    root: &str,
    full_path: &str,
    relative: &str,
    file: uhura_base::FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Discovery> {
    let stem = relative.strip_suffix(".uhura")?;
    let evidence = stem.ends_with(".examples");
    let subject_stem = stem.strip_suffix(".examples").unwrap_or(stem);
    let parts = subject_stem.split('/').collect::<Vec<_>>();
    if parts.is_empty()
        || parts.iter().any(|part| !kebab_segment(part))
        || relative.contains('[')
        || relative.contains(']')
        || stem.contains('.') && !evidence
    {
        diagnostics.push(Diagnostic::new(
            "UH2001",
            "contract/invalid-project",
            Severity::Error,
            format!(
                "unrecognized framework source `{full_path}`; {root} paths use lowercase kebab-case directories and basenames"
            ),
            Span::new(file, 0, 0),
        ));
        return None;
    }
    let logical = format!(
        "{root}::{}",
        parts
            .iter()
            .map(|part| part.replace('-', "_"))
            .collect::<Vec<_>>()
            .join("::")
    );
    if evidence {
        Some(Discovery::Evidence(EvidenceCandidate {
            file,
            role,
            logical: format!("framework::evidence::{logical}"),
            path: full_path.to_owned(),
            subject_path: format!("{root}/{subject_stem}.uhura"),
        }))
    } else {
        Some(Discovery::Subject(DiscoveredSubject {
            role,
            logical,
            path: full_path.to_owned(),
            declaration: String::new(),
            route: None,
            evidence_logical: None,
            evidence_path: None,
        }))
    }
}

fn route_from_directories(directories: &[&str]) -> Result<ResolvedRoute, String> {
    let mut logical = Vec::new();
    let mut path = Vec::new();
    let mut parameters = Vec::new();
    for directory in directories {
        if directory.starts_with('[') || directory.ends_with(']') {
            let Some(parameter) = directory
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
            else {
                return Err(format!(
                    "malformed dynamic route directory `{directory}`; expected `[lower_snake]`"
                ));
            };
            if !lower_name(parameter) {
                return Err(format!(
                    "dynamic route directory `{directory}` must use `[lower_snake]`"
                ));
            }
            if parameters.iter().any(|known| known == parameter) {
                return Err(format!(
                    "route parameter `{parameter}` occurs more than once"
                ));
            }
            logical.push(format!("param_{parameter}"));
            path.push(format!("{{{parameter}}}"));
            parameters.push(parameter.to_owned());
        } else {
            if directory.contains('[') || directory.contains(']') || !kebab_segment(directory) {
                return Err(format!(
                    "static route directory `{directory}` must use lowercase kebab-case"
                ));
            }
            logical.push(directory.replace('-', "_"));
            path.push((*directory).to_owned());
        }
    }
    Ok(ResolvedRoute {
        constructor: String::new(),
        pattern: if path.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", path.join("/"))
        },
        parameters,
    })
}

fn page_logical(directories: &[&str]) -> String {
    let mut parts = vec!["app".to_owned()];
    for directory in directories {
        let value = directory
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .map_or_else(
                || directory.replace('-', "_"),
                |parameter| format!("param__{parameter}"),
            );
        parts.push(value);
    }
    parts.join("::")
}

fn validate_role_source(
    source: &AdmittedSource,
    package: &str,
    logical: &str,
    role: ResolvedUiRole,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let parsed = parse(
        SourceIdentity::new(source.file.0, package, logical, &source.path),
        &source.text,
    );
    if !parsed.diagnostics.is_empty() {
        diagnostics.extend(
            parsed
                .diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.into_public_diagnostic()),
        );
        return None;
    }
    let public_ui = parsed
        .module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.kind {
            DeclarationKind::Ui(ui) if ui.visibility == Visibility::Public => Some(ui),
            _ => None,
        })
        .collect::<Vec<_>>();
    if public_ui.len() != 1 {
        diagnostics.push(Diagnostic::new(
            "UH2001",
            "contract/invalid-project",
            Severity::Error,
            format!(
                "framework {} source `{}` must declare exactly one public UI",
                role_name(role),
                source.path
            ),
            Span::new(source.file, 0, 0),
        ));
        return None;
    }
    let ui = public_ui[0];
    let valid_binding = match role {
        ResolvedUiRole::Page => matches!(ui.binding, UiBinding::Machine { .. }),
        ResolvedUiRole::Component | ResolvedUiRole::Surface => {
            matches!(ui.binding, UiBinding::Component { .. })
        }
    };
    if !valid_binding {
        diagnostics.push(Diagnostic::new(
            "UH2001",
            "contract/invalid-project",
            Severity::Error,
            format!(
                "framework {} `{}` must use a {}-bound UI declaration",
                role_name(role),
                source.path,
                if role == ResolvedUiRole::Page {
                    "machine"
                } else {
                    "component"
                }
            ),
            Span::new(source.file, ui.name.span.start, ui.name.span.end),
        ));
    }
    let name = ui.name.text.clone();
    match role {
        ResolvedUiRole::Page => {
            let Some(constructor) = name.strip_suffix("Page").filter(|name| !name.is_empty())
            else {
                diagnostics.push(Diagnostic::new(
                    "UH2001",
                    "contract/invalid-project",
                    Severity::Error,
                    format!(
                        "framework page `{}` must use an UpperCamel name ending in `Page`",
                        source.path
                    ),
                    Span::new(source.file, ui.name.span.start, ui.name.span.end),
                ));
                return None;
            };
            let _ = constructor;
        }
        ResolvedUiRole::Component | ResolvedUiRole::Surface => {
            let basename = source
                .path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".uhura"))
                .expect("discovered role source has an Uhura basename");
            let expected = upper_camel(basename);
            if name != expected {
                diagnostics.push(Diagnostic::new(
                    "UH2001",
                    "contract/invalid-project",
                    Severity::Error,
                    format!(
                        "framework {} `{}` must declare `{expected}`, found `{name}`",
                        role_name(role),
                        source.path
                    ),
                    Span::new(source.file, ui.name.span.start, ui.name.span.end),
                ));
            }
        }
    }
    Some(name)
}

fn generated_routes(
    location: &uhura_check::project_manifest::FrameworkLocator,
    subjects: &[ResolvedUiSubject],
) -> String {
    let routes = subjects
        .iter()
        .filter_map(|subject| {
            subject
                .route
                .as_ref()
                .map(|route| format!("(\"{}\", \"{}\")", route.constructor, route.pattern))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "use uhura::web_router::Routes;\nuse {};\n\npub const APPLICATION_ROUTES: Routes<{}> = Routes::from([{routes}]);\n",
        location.as_str(),
        location.declaration(),
    )
}

fn generated_application(
    machine: &uhura_check::project_manifest::FrameworkLocator,
    location: &uhura_check::project_manifest::FrameworkLocator,
    subjects: &[ResolvedUiSubject],
) -> String {
    let pages = subjects
        .iter()
        .filter(|subject| subject.role == ResolvedUiRole::Page)
        .collect::<Vec<_>>();
    let root = pages
        .iter()
        .find(|subject| subject.path == "app/page.uhura")
        .expect("validated web app has a root page");
    let mut imports = vec![
        "use uhura::ui;".to_owned(),
        format!("use {};", machine.as_str()),
        format!("use {};", location.as_str()),
    ];
    imports.extend(
        pages
            .iter()
            .map(|page| format!("use crate::{}::{};", page.logical, page.declaration)),
    );
    imports.sort();
    imports.dedup();

    let mut body = String::new();
    body.push_str("  {#if view.location is None}\n");
    body.push_str(&format!("    <{}/>", root.declaration));
    body.push_str("\n  {:else}\n");
    render_page_chain(&pages, 0, 2, root, location.declaration(), &mut body);
    body.push_str("  {/if}\n");
    format!(
        "{}\n\npub ui Application for {}(view) {{\n{body}}}\n",
        imports.join("\n"),
        machine.declaration(),
    )
}

fn render_page_chain(
    pages: &[&ResolvedUiSubject],
    index: usize,
    depth: usize,
    root: &ResolvedUiSubject,
    location: &str,
    out: &mut String,
) {
    let indent = "  ".repeat(depth);
    let child = "  ".repeat(depth + 1);
    let Some(page) = pages.get(index) else {
        out.push_str(&format!("{indent}<{} />\n", root.declaration));
        return;
    };
    let route = page.route.as_ref().expect("page has route metadata");
    out.push_str(&format!(
        "{indent}{{#if view.location is {}}}\n",
        route_pattern(location, route)
    ));
    out.push_str(&format!("{child}<{} />\n", page.declaration));
    out.push_str(&format!("{indent}{{:else}}\n"));
    render_page_chain(pages, index + 1, depth + 1, root, location, out);
    out.push_str(&format!("{indent}{{/if}}\n"));
}

fn route_pattern(location: &str, route: &ResolvedRoute) -> String {
    if route.parameters.is_empty() {
        format!("Some({location}::{})", route.constructor)
    } else {
        format!(
            "Some({location}::{} {{ {} }})",
            route.constructor,
            route.parameters.join(", ")
        )
    }
}

fn insert_generated(
    manifest: &mut ProjectManifest,
    sources: &mut Vec<AdmittedSource>,
    source_map: &mut SourceMap,
    logical: &str,
    path: &str,
    text: String,
) {
    let file = source_map.add(path.to_owned(), text.clone());
    sources.push(AdmittedSource {
        file,
        path: path.to_owned(),
        text,
        kind: AdmittedSourceKind::Generated,
    });
    manifest.modules.insert(
        LogicalModulePath::parse(logical).expect("generated logical is valid"),
        ProjectPath::parse(path).expect("generated path is valid"),
    );
}

fn reserve_logical(
    owners: &mut BTreeMap<String, String>,
    logical: &str,
    path: &str,
    file: uhura_base::FileId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(previous) = owners.insert(logical.to_owned(), format!("`{path}`")) {
        diagnostics.push(Diagnostic::new(
            "UH2001",
            "contract/invalid-project",
            Severity::Error,
            format!("framework logical module `{logical}` for `{path}` collides with {previous}"),
            Span::new(file, 0, 0),
        ));
    }
}

fn project_error(file: uhura_base::FileId, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        "UH2001",
        "contract/invalid-project",
        Severity::Error,
        message,
        Span::new(file, 0, 0),
    )
}

fn source_file(sources: &[AdmittedSource], path: &str) -> Option<uhura_base::FileId> {
    sources
        .iter()
        .find(|source| source.path == path)
        .map(|source| source.file)
}

fn path_is_within(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn route_shape(pattern: &str) -> String {
    pattern
        .split('/')
        .map(|part| {
            if part.starts_with('{') && part.ends_with('}') {
                "{}"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn role_rank(role: ResolvedUiRole) -> u8 {
    match role {
        ResolvedUiRole::Page => 0,
        ResolvedUiRole::Component => 1,
        ResolvedUiRole::Surface => 2,
    }
}

fn role_name(role: ResolvedUiRole) -> &'static str {
    match role {
        ResolvedUiRole::Page => "page",
        ResolvedUiRole::Component => "component",
        ResolvedUiRole::Surface => "surface",
    }
}

fn kebab_segment(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut previous_dash = false;
    for byte in &bytes[1..] {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => previous_dash = false,
            b'-' if !previous_dash => previous_dash = true,
            _ => return false,
        }
    }
    !previous_dash
}

fn lower_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|next| next.is_ascii_lowercase() || next.is_ascii_digit() || next == '_')
}

fn upper_camel(value: &str) -> String {
    value
        .split('-')
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + characters.as_str()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::page_logical;

    #[test]
    fn dynamic_route_logical_sentinel_cannot_result_from_static_kebab_case() {
        assert_eq!(page_logical(&["param-user"]), "app::param_user");
        assert_eq!(page_logical(&["[user]"]), "app::param__user");
    }
}
