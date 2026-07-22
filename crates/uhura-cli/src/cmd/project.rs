//! Shared canonical Uhura project admission for CLI commands.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use uhura_base::{Diagnostic, FileId, Severity, SourceMap, Span};
use uhura_check::project_lock::{
    CapturedPackage, ProjectLockIssue, check_project_lock, parse_project_lock,
};
use uhura_check::project_manifest::{ProjectManifest, ProjectManifestIssue, load_project_manifest};
use uhura_check::resource_manifest::ResourceManifest;
use uhura_check::{CheckedIconFonts, IconFontInput};
use uhura_core::Program;

use crate::fsio::{SourceFile, walk_sources};

pub(super) struct Project {
    pub files: Vec<SourceFile>,
    pub source_map: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
    pub program: Option<Program>,
}

pub(super) fn load(root: &Path, command: &str) -> Result<Project, ExitCode> {
    let files = walk_sources(root).map_err(|error| {
        eprintln!("uhura {command}: {}: {error}", root.display());
        ExitCode::from(2)
    })?;

    let mut source_map = SourceMap::new();
    for file in &files {
        source_map.add(file.rel_path.clone(), file.text.clone());
    }
    let manifest_text = match std::fs::read_to_string(root.join("uhura.toml")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            eprintln!(
                "uhura {command}: {}: {error}",
                root.join("uhura.toml").display()
            );
            return Err(ExitCode::from(2));
        }
    };
    let manifest_file = source_map.add("uhura.toml", manifest_text.clone());
    let manifest = match load_project_manifest(&manifest_text) {
        Ok(manifest) => manifest,
        Err(issues) => {
            return Ok(Project {
                files,
                source_map,
                diagnostics: project_manifest_diagnostics(issues, manifest_file),
                program: None,
            });
        }
    };

    load_checked_project(root, files, source_map, manifest, manifest_file)
}

fn load_checked_project(
    root: &Path,
    files: Vec<SourceFile>,
    source_map: SourceMap,
    manifest: ProjectManifest,
    manifest_file: FileId,
) -> Result<Project, ExitCode> {
    let icon_fonts = match load_cli_icon_fonts(root, &manifest.resources, manifest_file) {
        Ok(fonts) => fonts,
        Err(mut diagnostics) => {
            diagnostics.sort_by_key(|diagnostic| {
                (
                    diagnostic.span.file.0,
                    diagnostic.span.start,
                    diagnostic.span.end,
                    diagnostic.code,
                    diagnostic.rule,
                )
            });
            return Ok(Project {
                files,
                source_map,
                diagnostics,
                program: None,
            });
        }
    };
    let captured_dependencies = match capture_dependencies(root, &files, &manifest) {
        Ok(packages) => packages,
        Err(messages) => {
            let mut diagnostics = contract_diagnostics(messages, manifest_file);
            diagnostics.sort_by_key(|diagnostic| {
                (
                    diagnostic.span.file.0,
                    diagnostic.span.start,
                    diagnostic.span.end,
                    diagnostic.code,
                    diagnostic.rule,
                )
            });
            return Ok(Project {
                files,
                source_map,
                diagnostics,
                program: None,
            });
        }
    };
    let dependency_roots = captured_dependencies
        .iter()
        .map(|package| package.source.as_str())
        .collect::<Vec<_>>();
    let mut diagnostics = validate_sources(&files, &manifest, manifest_file, &dependency_roots);
    let program = if diagnostics.is_empty() {
        let sources = files
            .iter()
            .enumerate()
            .map(|(file, source)| {
                uhura_check::ProjectSource::new(FileId(file as u32), &source.rel_path, &source.text)
            })
            .collect::<Vec<_>>();
        let mut checked = uhura_check::compile_project(&manifest, &sources, &captured_dependencies);
        diagnostics.append(&mut checked.diagnostics);
        if let Some(program) = checked.program.as_ref() {
            diagnostics.extend(uhura_check::icon_token_diagnostics(
                program,
                &icon_fonts,
                files
                    .iter()
                    .enumerate()
                    .map(|(file, source)| (FileId(file as u32), source.rel_path.as_str())),
            ));
        }
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            checked.program = None;
        }
        checked.program
    } else {
        None
    };
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
        )
    });
    let mut program = program;
    if !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
        && let Some(program) = program.as_mut()
    {
        program.freeze_program_hashes();
    }

    Ok(Project {
        files,
        source_map,
        diagnostics,
        program,
    })
}

fn load_cli_icon_fonts(
    root: &Path,
    resources: &ResourceManifest,
    manifest_file: FileId,
) -> Result<CheckedIconFonts, Vec<Diagnostic>> {
    let canonical_root = std::fs::canonicalize(root).map_err(|error| {
        icon_resource_diagnostics(
            [format!("project root `{}`: {error}", root.display())],
            manifest_file,
        )
    })?;
    let mut inputs = BTreeMap::new();
    let mut messages = Vec::new();
    for (name, family) in &resources.icons.families {
        let font_bytes = match read_project_resource(root, &canonical_root, &family.font) {
            Ok(bytes) => bytes.map(Arc::<[u8]>::from),
            Err(message) => {
                messages.push(format!("icons.{name}.font: {message}"));
                None
            }
        };
        let glyphs_text = match read_project_resource(root, &canonical_root, &family.glyphs) {
            Ok(Some(bytes)) => match String::from_utf8(bytes) {
                Ok(text) => Some(text),
                Err(error) => {
                    messages.push(format!(
                        "icons.{name}.glyphs: resource is not UTF-8: {error}"
                    ));
                    None
                }
            },
            Ok(None) => None,
            Err(message) => {
                messages.push(format!("icons.{name}.glyphs: {message}"));
                None
            }
        };
        inputs.insert(
            name.clone(),
            IconFontInput {
                font_path: family.font.clone(),
                font_bytes,
                glyphs_path: family.glyphs.clone(),
                glyphs_text,
            },
        );
    }
    if !messages.is_empty() {
        return Err(icon_resource_diagnostics(messages, manifest_file));
    }
    uhura_check::icon_fonts::load_icon_fonts(&resources.icons, &inputs).map_err(|issues| {
        icon_resource_diagnostics(
            issues
                .into_iter()
                .map(|issue| format!("{}: {}", issue.path, issue.message)),
            manifest_file,
        )
    })
}

fn read_project_resource(
    root: &Path,
    canonical_root: &Path,
    relative: &str,
) -> Result<Option<Vec<u8>>, String> {
    let candidate = root.join(relative);
    let canonical = match std::fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !canonical.starts_with(canonical_root) {
        return Err(format!("`{relative}` escapes the project root"));
    }
    let metadata = std::fs::metadata(&canonical).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(format!("`{relative}` is not a regular file"));
    }
    std::fs::read(canonical)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn icon_resource_diagnostics(
    messages: impl IntoIterator<Item = String>,
    manifest_file: FileId,
) -> Vec<Diagnostic> {
    messages
        .into_iter()
        .map(|message| {
            Diagnostic::new(
                "UH2010",
                "contract/invalid-icon-font",
                Severity::Error,
                message,
                Span::new(manifest_file, 0, 0),
            )
        })
        .collect()
}

fn capture_dependencies(
    root: &Path,
    files: &[SourceFile],
    manifest: &ProjectManifest,
) -> Result<Vec<CapturedPackage>, Vec<String>> {
    let lock_path = root.join("uhura.lock");
    let lock_text = match std::fs::read_to_string(&lock_path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(vec![format!("uhura.lock: {error}")]);
        }
    };
    if manifest.dependencies.is_empty() {
        return check_project_lock(manifest, lock_text.as_deref(), &[])
            .map(|_| Vec::new())
            .map_err(lock_issue_messages);
    }
    let lock = parse_project_lock(
        lock_text
            .as_deref()
            .ok_or_else(|| vec!["uhura.lock: lock file is required".to_string()])?,
    )
    .map_err(lock_issue_messages)?;
    let canonical_root =
        std::fs::canonicalize(root).map_err(|error| vec![format!("project root: {error}")])?;
    let dependency_roots = lock
        .packages
        .values()
        .map(|record| record.source.path.as_str())
        .collect::<Vec<_>>();
    let mut captured = Vec::new();
    let mut messages = Vec::new();
    for record in lock.packages.values() {
        let package_root = root.join(record.source.path.as_str());
        let canonical_package = match std::fs::canonicalize(&package_root) {
            Ok(path) if path.starts_with(&canonical_root) => path,
            Ok(_) => {
                messages.push(format!(
                    "package.{}.source.path: `{}` escapes the project root",
                    record.package, record.source.path
                ));
                continue;
            }
            Err(error) => {
                messages.push(format!(
                    "package.{}.source.path: {}: {error}",
                    record.package, record.source.path
                ));
                continue;
            }
        };
        let manifest_path = canonical_package.join("uhura.toml");
        let package_manifest = match std::fs::read_to_string(&manifest_path) {
            Ok(text) => match load_project_manifest(&text) {
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
            },
            Err(error) => {
                messages.push(format!(
                    "package.{}.manifest: {}: {error}",
                    record.package,
                    manifest_path.display()
                ));
                continue;
            }
        };
        let declared_sources = package_manifest
            .modules
            .values()
            .chain(package_manifest.evidence.values())
            .map(|path| format!("{}/{}", record.source.path, path))
            .collect::<BTreeSet<_>>();
        let discovered_sources = files
            .iter()
            .filter(|source| {
                owning_dependency_root(&source.rel_path, &dependency_roots)
                    == Some(record.source.path.as_str())
            })
            .map(|source| source.rel_path.clone())
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
            let Some(source) = files.iter().find(|source| source.rel_path == global) else {
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

fn contract_diagnostics(messages: Vec<String>, manifest_file: FileId) -> Vec<Diagnostic> {
    messages
        .into_iter()
        .map(|message| {
            Diagnostic::new(
                "UH2001",
                "contract/invalid-project",
                Severity::Error,
                message,
                Span::new(manifest_file, 0, 0),
            )
        })
        .collect()
}

fn validate_sources(
    files: &[SourceFile],
    manifest: &ProjectManifest,
    manifest_file: FileId,
    dependency_roots: &[&str],
) -> Vec<Diagnostic> {
    let mut messages = Vec::new();
    let declared = manifest
        .modules
        .values()
        .chain(manifest.evidence.values())
        .map(|path| path.as_str())
        .collect::<BTreeSet<_>>();
    let discovered = files
        .iter()
        .filter(|source| {
            !dependency_roots
                .iter()
                .any(|root| path_is_within(&source.rel_path, root))
        })
        .map(|source| source.rel_path.as_str())
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

    let mut physical = BTreeMap::<std::path::PathBuf, Vec<&str>>::new();
    for source in files {
        if let Ok(path) = std::fs::canonicalize(&source.abs_path) {
            physical
                .entry(path)
                .or_default()
                .push(source.rel_path.as_str());
        }
    }
    for aliases in physical.values_mut() {
        aliases.sort_unstable();
        aliases.dedup();
        if aliases.len() > 1 {
            messages.push(format!(
                "Uhura source paths {} resolve to the same physical file",
                aliases
                    .iter()
                    .map(|path| format!("`{path}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    messages.sort();
    messages
        .into_iter()
        .map(|message| {
            Diagnostic::new(
                "UH2001",
                "contract/invalid-project",
                Severity::Error,
                message,
                Span::new(manifest_file, 0, 0),
            )
        })
        .collect()
}

fn project_manifest_diagnostics(
    issues: Vec<ProjectManifestIssue>,
    manifest_file: FileId,
) -> Vec<Diagnostic> {
    issues
        .into_iter()
        .map(|issue| {
            let message = if issue.path.is_empty() {
                issue.message
            } else {
                format!("{}: {}", issue.path, issue.message)
            };
            Diagnostic::new(
                "UH2001",
                "contract/invalid-project",
                Severity::Error,
                message,
                Span::new(manifest_file, 0, 0),
            )
        })
        .collect()
}

pub(super) fn require_program(root: &Path, command: &str) -> Result<Program, ExitCode> {
    let project = load(root, command)?;
    if project
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        print!(
            "{}",
            uhura_base::render_text(&project.diagnostics, &project.source_map)
        );
        eprintln!("uhura {command}: the source check must come up clean first");
        return Err(ExitCode::from(1));
    }
    project.program.ok_or_else(|| {
        eprintln!("uhura {command}: clean source produced no machine program");
        ExitCode::from(2)
    })
}

#[cfg(test)]
pub(super) fn checked_test_program() -> Program {
    const MANIFEST: &str = r#"[project]
name = "test.cli"
version = 1
language = "0.4"

[modules]
counter = "machine.uhura"

[evidence.modules]
evidence = "evidence.uhura"
"#;
    const MACHINE: &str = r#"pub machine Counter {
  events {
    Increment,
  }

  outcomes {
    commit Accepted,
  }

  state {
    count: Int = 0,
  }

  observe {
    count,
  }

  on Increment {
    count = count + 1;
    Accepted
  }
}
"#;
    const EVIDENCE: &str = r#"use crate::counter::Counter;

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  pin done
}

example incremented = increment::done;
"#;
    let manifest = load_project_manifest(MANIFEST).expect("test manifest is current");
    let sources = [
        uhura_check::ProjectSource::new(FileId(0), "machine.uhura", MACHINE),
        uhura_check::ProjectSource::new(FileId(1), "evidence.uhura", EVIDENCE),
    ];
    let checked = uhura_check::compile_project(&manifest, &sources, &[]);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut program = checked.program.expect("test project lowers");
    program.freeze_program_hashes();
    program
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    const COUNTER_MACHINE: &str = r#"pub machine Counter {
  events {
    Increment,
  }

  outcomes {
    commit Accepted,
  }

  state {
    count: Int = 0,
  }

  observe {
    count,
  }

  on Increment {
    count = count + 1;
    Accepted
  }
}
"#;

    fn project_root(label: &str) -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "uhura-cli-project-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_manifest(root: &Path, extra: &str) {
        std::fs::write(
            root.join("uhura.toml"),
            format!(
                r#"[project]
name = "test.counter"
version = 1
language = "0.4"

[modules]
counter = "counter.uhura"

{extra}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn project_admission_requires_the_current_manifest() {
        for (label, manifest) in [
            ("missing", None),
            (
                "resource-only",
                Some(
                    r#"[icons]
default = "lucide"
"#,
                ),
            ),
        ] {
            let root = project_root(label);
            std::fs::write(root.join("counter.uhura"), COUNTER_MACHINE).unwrap();
            if let Some(manifest) = manifest {
                std::fs::write(root.join("uhura.toml"), manifest).unwrap();
            }

            let project = load(&root, "check").expect("invalid projects are diagnostic results");
            assert!(project.program.is_none());
            assert!(project.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "UH2001"
                    && (diagnostic.message.contains("project")
                        || diagnostic.message.contains("modules"))
            }));
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn manifest_selected_lowers_the_declared_module() {
        let root = project_root("accepted");
        write_manifest(&root, "");
        std::fs::write(root.join("counter.uhura"), COUNTER_MACHINE).unwrap();

        let project = load(&root, "check").expect("load 0.4 project");
        assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
        let program = project.program.expect("0.4 program");
        assert_eq!(program.machine_program.language, "uhura 0.4");
        assert_eq!(program.machine_program.machines.len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_selected_rejects_unknown_icons_before_publication() {
        let root = project_root("unknown-icon");
        std::fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "test.counter"
version = 1
language = "0.4"

[modules]
counter = "counter.uhura"
ui = "ui.uhura"
"#,
        )
        .unwrap();
        std::fs::write(root.join("counter.uhura"), COUNTER_MACHINE).unwrap();
        std::fs::write(
            root.join("ui.uhura"),
            r#"use uhura::ui;
use crate::counter::Counter;

pub ui CounterWeb for Counter(view) {
  <button label="Increment" on press -> Increment>
    <icon name="definitely-not-a-lucide-glyph" />
  </button>
}
"#,
        )
        .unwrap();

        let project = load(&root, "check").expect("load project with unknown icon");
        assert!(project.program.is_none());
        let diagnostic = project
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.rule == "uhura/unknown-icon")
            .expect("unknown icon diagnostic");
        assert_eq!(diagnostic.code, "UH5017");
        assert_eq!(project.source_map.path(diagnostic.span.file), "ui.uhura");
        assert!(diagnostic.message.contains("definitely-not-a-lucide-glyph"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_selected_preserves_parse_kind_in_public_diagnostics() {
        let root = project_root("parse-diagnostic");
        write_manifest(&root, "");
        std::fs::write(root.join("counter.uhura"), "pub mashine Counter {}\n").unwrap();

        let project = load(&root, "check").expect("load malformed 0.4 project");
        assert!(project.program.is_none());
        assert_eq!(
            uhura_base::to_envelope(&project.diagnostics, &project.source_map),
            serde_json::json!({
                "format": "uhura-diagnostics",
                "version": 0,
                "summary": {
                    "errors": 1,
                    "warnings": 0,
                },
                "diagnostics": [{
                    "code": "R1001",
                    "rule": "uhura-0.4/parse/invalid-declaration",
                    "severity": "error",
                "message": "unknown module declaration `mashine`; expected `machine`, `part`, `ui`, `scenario`, `example`, `checkpoint`, `struct`, `enum`, `key`, `const`, or `fn`",
                    "file": "counter.uhura",
                    "span": {
                        "offset": 4,
                        "len": 7,
                        "start": { "line": 1, "col": 5 },
                        "end": { "line": 1, "col": 12 },
                    },
                    "fix": {
                        "title": "Replace `mashine` with `machine`",
                        "edits": [{
                            "file": "counter.uhura",
                            "offset": 4,
                            "len": 7,
                            "insert": "machine",
                        }],
                    },
                }],
            })
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_selected_lowers_the_complete_module_graph() {
        let root = project_root("modules");
        std::fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "test.counter"
version = 1
language = "0.4"

[modules]
counter = "counter.uhura"
support = "support.uhura"
"#,
        )
        .unwrap();
        std::fs::write(root.join("support.uhura"), "pub const INITIAL: Int = 0;\n").unwrap();
        std::fs::write(
            root.join("counter.uhura"),
            COUNTER_MACHINE
                .replace(
                    "pub machine Counter {",
                    "use crate::support::INITIAL;\n\npub machine Counter {",
                )
                .replace("count: Int = 0,", "count: Int = INITIAL,"),
        )
        .unwrap();

        let project = load(&root, "check").expect("load multi-module 0.4 project");
        assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
        let program = project.program.expect("0.4 program");
        assert_eq!(program.machine_program.language, "uhura 0.4");
        assert_eq!(program.machine_program.machines.len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_selected_attaches_manifest_role_evidence() {
        let root = project_root("evidence");
        write_manifest(
            &root,
            r#"[evidence.modules]
evidence = "counter.evidence.uhura"
"#,
        );
        std::fs::write(root.join("counter.uhura"), COUNTER_MACHINE).unwrap();
        std::fs::write(
            root.join("counter.evidence.uhura"),
            r#"use crate::counter::Counter;

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  pin done
}

example incremented = increment::done;
"#,
        )
        .unwrap();

        let project = load(&root, "check").expect("load 0.4 project with evidence");
        assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
        let program = project.program.expect("0.4 program with evidence");
        assert!(program.run_evidence().passed);
        assert_eq!(program.evidence.scenarios.len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_selected_rejects_missing_and_unlisted_sources() {
        let missing_root = project_root("missing");
        write_manifest(&missing_root, "");
        let missing = load(&missing_root, "check").expect("diagnosed 0.4 project");
        assert!(missing.program.is_none());
        assert!(
            missing
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("`counter.uhura` is missing") })
        );
        std::fs::remove_dir_all(missing_root).unwrap();

        let unlisted_root = project_root("unlisted");
        write_manifest(&unlisted_root, "");
        std::fs::write(unlisted_root.join("counter.uhura"), COUNTER_MACHINE).unwrap();
        std::fs::write(unlisted_root.join("stray.uhura"), COUNTER_MACHINE).unwrap();
        let unlisted = load(&unlisted_root, "check").expect("diagnosed 0.4 project");
        assert!(unlisted.program.is_none());
        assert!(
            unlisted
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("`stray.uhura` is not listed") })
        );
        std::fs::remove_dir_all(unlisted_root).unwrap();
    }

    #[test]
    fn manifest_selected_admits_exact_locked_path_dependency() {
        let root = project_root("dependencies");
        write_manifest(
            &root,
            r#"[dependencies.shared]
package = "test.shared"
version = 1
path = "vendor/shared"
"#,
        );
        std::fs::create_dir_all(root.join("vendor/shared/deps/base")).unwrap();
        let root_source = COUNTER_MACHINE
            .replace("pub machine", "use shared::values::INITIAL;\n\npub machine")
            .replace("count: Int = 0", "count: Int = INITIAL");
        std::fs::write(root.join("counter.uhura"), root_source).unwrap();
        let vendor_manifest_text = r#"[project]
name = "test.shared"
version = 1
language = "0.4"

[modules]
values = "values.uhura"

[dependencies.base]
package = "test.base"
version = 1
path = "deps/base"
"#;
        let vendor_source =
            b"use base::values::Base;\npub struct Wrapper { value: Base }\npub const INITIAL: Int = 0;\n";
        let base_manifest_text = r#"[project]
name = "test.base"
version = 1
language = "0.4"

[modules]
values = "values.uhura"
"#;
        let base_source = b"pub struct Base { value: Int }\n";
        std::fs::write(root.join("vendor/shared/uhura.toml"), vendor_manifest_text).unwrap();
        std::fs::write(root.join("vendor/shared/values.uhura"), vendor_source).unwrap();
        std::fs::write(
            root.join("vendor/shared/deps/base/uhura.toml"),
            base_manifest_text,
        )
        .unwrap();
        std::fs::write(
            root.join("vendor/shared/deps/base/values.uhura"),
            base_source,
        )
        .unwrap();
        let vendor_manifest = load_project_manifest(vendor_manifest_text).unwrap();
        let base_manifest = load_project_manifest(base_manifest_text).unwrap();
        let base_capture = CapturedPackage {
            manifest: base_manifest,
            source: uhura_check::project_manifest::ProjectPath::parse("vendor/shared/deps/base")
                .unwrap(),
            modules: [(
                uhura_check::project_manifest::LogicalModulePath::parse("values").unwrap(),
                base_source.to_vec(),
            )]
            .into_iter()
            .collect(),
            resolved_dependencies: BTreeMap::new(),
            resources: BTreeMap::new(),
        };
        let vendor_capture = CapturedPackage {
            manifest: vendor_manifest,
            source: uhura_check::project_manifest::ProjectPath::parse("vendor/shared").unwrap(),
            modules: [(
                uhura_check::project_manifest::LogicalModulePath::parse("values").unwrap(),
                vendor_source.to_vec(),
            )]
            .into_iter()
            .collect(),
            resolved_dependencies: [(
                uhura_check::project_manifest::DependencyAlias::parse("base").unwrap(),
                uhura_check::project_manifest::PackageId::parse("test.base@1").unwrap(),
            )]
            .into_iter()
            .collect(),
            resources: BTreeMap::new(),
        };
        let integrity = vendor_capture.artifact_integrity().unwrap();
        let base_integrity = base_capture.artifact_integrity().unwrap();
        std::fs::write(
            root.join("uhura.lock"),
            format!(
                r#"protocol = "uhura-lock/0"

[root]
package = "test.counter@1"
dependencies = {{ shared = "test.shared@1" }}

[[package]]
package = "test.shared@1"
source = {{ kind = "path", path = "vendor/shared" }}
integrity = "{integrity}"
dependencies = {{ base = "test.base@1" }}

[[package]]
package = "test.base@1"
source = {{ kind = "path", path = "vendor/shared/deps/base" }}
integrity = "{base_integrity}"
dependencies = {{}}
"#
            ),
        )
        .unwrap();

        let project = load(&root, "check").expect("checked locked 0.4 project");
        assert!(project.diagnostics.is_empty(), "{:#?}", project.diagnostics);
        assert!(project.program.is_some());

        std::fs::write(
            root.join("vendor/shared/stray.uhura"),
            b"pub const STRAY: Int = 1;\n",
        )
        .unwrap();
        let unlisted = load(&root, "check").expect("diagnosed dependency inventory");
        assert!(unlisted.program.is_none());
        assert!(unlisted.diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("vendor/shared/stray.uhura")
                && diagnostic.message.contains("not listed")
        }));
        std::fs::remove_file(root.join("vendor/shared/stray.uhura")).unwrap();

        std::fs::write(
            root.join("vendor/shared/uhura.toml"),
            format!(
                r#"{vendor_manifest_text}

[assets]
manifest = "assets/manifest.toml"
"#
            ),
        )
        .unwrap();
        let resource_bearing = load(&root, "check").expect("diagnosed resource-bearing dependency");
        assert!(resource_bearing.program.is_none());
        assert!(resource_bearing.diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("vendored dependency packages are source-only in Uhura 0.4")
        }));
        std::fs::write(root.join("vendor/shared/uhura.toml"), vendor_manifest_text).unwrap();

        std::fs::write(
            root.join("vendor/shared/values.uhura"),
            b"pub const INITIAL: Int = 1;\n",
        )
        .unwrap();
        let changed = load(&root, "check").expect("diagnosed changed package");
        assert!(changed.program.is_none());
        assert!(
            changed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("integrity"))
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
