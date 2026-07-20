//! Shared canonical Uhura project admission for CLI commands.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use uhura_base::{Diagnostic, FileId, Severity, SourceMap, Span};
use uhura_check::project_lock::{
    CapturedPackage, ProjectLockIssue, check_project_lock, parse_project_lock,
};
use uhura_check::project_manifest::{
    LoadedProjectManifest, ProjectManifest, ProjectManifestIssue, load_project_manifest,
};
use uhura_core::Program;
use uhura_syntax::{SourceFile as SyntaxSourceFile, parse_project};

use crate::fsio::{SourceFile, walk_retired_sources, walk_sources};

pub(super) struct Project {
    pub files: Vec<SourceFile>,
    pub source_map: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
    pub program: Option<Program>,
}

pub(super) fn load(root: &Path, command: &str) -> Result<Project, ExitCode> {
    reject_retired_sources(root, command)?;
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

    match manifest {
        LoadedProjectManifest::Legacy03(_) => load_legacy_project(root, command, files, source_map),
        LoadedProjectManifest::V04(manifest) => {
            load_v04_project(root, files, source_map, manifest, manifest_file)
        }
    }
}

fn load_legacy_project(
    root: &Path,
    command: &str,
    files: Vec<SourceFile>,
    source_map: SourceMap,
) -> Result<Project, ExitCode> {
    if files.is_empty() {
        eprintln!(
            "uhura {command}: no .uhura sources under {}",
            root.display()
        );
        return Err(ExitCode::from(2));
    }

    let parsed = parse_project(files.iter().enumerate().map(|(index, file)| {
        SyntaxSourceFile::new(FileId(index as u32), file.rel_path.clone(), &file.text)
    }));
    let mut diagnostics = parsed
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            Diagnostic::new(
                "R1001",
                "uhura/parse",
                Severity::Error,
                diagnostic.message,
                Span::new(
                    FileId(diagnostic.span.file),
                    diagnostic.span.start,
                    diagnostic.span.end,
                ),
            )
        })
        .collect::<Vec<_>>();
    let mut checked = uhura_check::check_project(&parsed.project);
    diagnostics.append(&mut checked.diagnostics);
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
        )
    });
    if !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
        && let Some(program) = checked.program.as_mut()
    {
        program.freeze_program_hashes();
    }

    Ok(Project {
        files,
        source_map,
        diagnostics,
        program: checked.program,
    })
}

fn load_v04_project(
    root: &Path,
    files: Vec<SourceFile>,
    source_map: SourceMap,
    manifest: ProjectManifest,
    manifest_file: FileId,
) -> Result<Project, ExitCode> {
    let captured_dependencies = match capture_v04_dependencies(root, &files, &manifest) {
        Ok(packages) => packages,
        Err(messages) => {
            return Ok(Project {
                files,
                source_map,
                diagnostics: contract_diagnostics(messages, manifest_file),
                program: None,
            });
        }
    };
    let dependency_roots = captured_dependencies
        .iter()
        .map(|package| package.capture.source.as_str())
        .collect::<Vec<_>>();
    let mut diagnostics = validate_v04_sources(&files, &manifest, manifest_file, &dependency_roots);
    let program = if diagnostics.is_empty() {
        let mut modules = Vec::with_capacity(manifest.modules.len());
        for (logical, physical) in &manifest.modules {
            let (file, source) = files
                .iter()
                .enumerate()
                .find(|(_, source)| source.rel_path == physical.as_str())
                .expect("source admission proved every mapped 0.4 module exists");
            let identity = uhura_syntax::v04::SourceIdentity::new(
                file as u32,
                manifest.project.package_id().to_string(),
                logical.as_str(),
                physical.as_str(),
            );
            let parsed = uhura_syntax::v04::parse(identity, &source.text);
            diagnostics.extend(parsed.diagnostics.into_iter().map(|diagnostic| {
                Diagnostic::new(
                    "R1001",
                    "uhura/parse",
                    Severity::Error,
                    diagnostic.message,
                    Span::new(
                        FileId(diagnostic.span.file),
                        diagnostic.span.start,
                        diagnostic.span.end,
                    ),
                )
            }));
            modules.push(parsed.module);
        }
        let evidence_modules = if manifest.evidence.is_empty() {
            Vec::new()
        } else {
            let parsed = parse_project(manifest.evidence.iter().map(|physical| {
                let (file, source) = files
                    .iter()
                    .enumerate()
                    .find(|(_, source)| source.rel_path == physical.as_str())
                    .expect("source admission proved every mapped evidence file exists");
                SyntaxSourceFile::new(FileId(file as u32), physical.as_str(), source.text.as_str())
            }));
            diagnostics.extend(parsed.diagnostics.into_iter().map(|diagnostic| {
                Diagnostic::new(
                    "R1001",
                    "uhura/parse",
                    Severity::Error,
                    diagnostic.message,
                    Span::new(
                        FileId(diagnostic.span.file),
                        diagnostic.span.start,
                        diagnostic.span.end,
                    ),
                )
            }));
            parsed.project.modules
        };

        // Static checking is skipped when parsing failed. The 0.4 checker
        // consumes the complete manifest-resolved module graph and lowers to the
        // same kernel Program used by the legacy frontend.
        if diagnostics.is_empty() {
            let mut packages = vec![uhura_check::V04CapturedPackageModules {
                package: manifest.project.package_id().to_string(),
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
                modules,
            }];
            packages.extend(captured_dependencies.iter().map(|package| {
                uhura_check::V04CapturedPackageModules {
                    package: package.capture.package_id().to_string(),
                    dependencies: package
                        .capture
                        .resolved_dependencies
                        .iter()
                        .map(|(alias, dependency)| {
                            (alias.as_str().to_string(), dependency.to_string())
                        })
                        .collect(),
                    modules: package.modules.clone(),
                }
            }));
            let mut checked = uhura_check::check_v04_package_graph_with_evidence(
                &manifest.project.package_id().to_string(),
                &packages,
                &evidence_modules,
            );
            diagnostics.append(&mut checked.diagnostics);
            checked.program
        } else {
            None
        }
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

struct CliCapturedDependency {
    capture: CapturedPackage,
    modules: Vec<uhura_syntax::v04::Module>,
}

fn capture_v04_dependencies(
    root: &Path,
    files: &[SourceFile],
    manifest: &ProjectManifest,
) -> Result<Vec<CliCapturedDependency>, Vec<String>> {
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
                Ok(LoadedProjectManifest::V04(manifest)) => manifest,
                Ok(LoadedProjectManifest::Legacy03(_)) => {
                    messages.push(format!(
                        "package.{}: dependency manifest must select Uhura 0.4",
                        record.package
                    ));
                    continue;
                }
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
            .chain(package_manifest.evidence.iter())
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
                "package.{}.sources: `{unlisted}` is not listed in `[modules]` or `[evidence]`",
                record.package
            ));
        }
        let mut module_bytes = BTreeMap::new();
        let mut modules = Vec::new();
        for (logical, physical) in &package_manifest.modules {
            let global = format!("{}/{}", record.source.path, physical);
            let Some((file, source)) = files
                .iter()
                .enumerate()
                .find(|(_, source)| source.rel_path == global)
            else {
                messages.push(format!(
                    "package.{}.modules.{}: mapped source `{global}` is missing",
                    record.package, logical
                ));
                continue;
            };
            module_bytes.insert(logical.clone(), source.text.as_bytes().to_vec());
            let parsed = uhura_syntax::v04::parse(
                uhura_syntax::v04::SourceIdentity::new(
                    file as u32,
                    package_manifest.project.package_id().to_string(),
                    logical.as_str(),
                    global,
                ),
                &source.text,
            );
            messages.extend(parsed.diagnostics.into_iter().map(|diagnostic| {
                format!(
                    "package.{}.modules.{}: {}",
                    record.package, logical, diagnostic.message
                )
            }));
            modules.push(parsed.module);
        }
        let resolved_dependencies = package_manifest
            .dependencies
            .iter()
            .map(|(alias, dependency)| (alias.clone(), dependency.package_id()))
            .collect();
        captured.push(CliCapturedDependency {
            capture: CapturedPackage {
                manifest: package_manifest,
                source: record.source.path.clone(),
                modules: module_bytes,
                resolved_dependencies,
                resources: BTreeMap::new(),
            },
            modules,
        });
    }
    if !messages.is_empty() {
        messages.sort();
        return Err(messages);
    }
    let lock_captures = captured
        .iter()
        .map(|package| package.capture.clone())
        .collect::<Vec<_>>();
    check_project_lock(manifest, lock_text.as_deref(), &lock_captures)
        .map_err(lock_issue_messages)?;
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

fn validate_v04_sources(
    files: &[SourceFile],
    manifest: &ProjectManifest,
    manifest_file: FileId,
    dependency_roots: &[&str],
) -> Vec<Diagnostic> {
    let mut messages = Vec::new();
    let declared = manifest
        .modules
        .values()
        .chain(manifest.evidence.iter())
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
            "Uhura 0.4 source `{unlisted}` is not listed in `[modules]`"
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

fn reject_retired_sources(root: &Path, command: &str) -> Result<(), ExitCode> {
    let sources = walk_retired_sources(root).map_err(|error| {
        eprintln!("uhura {command}: {}: {error}", root.display());
        ExitCode::from(2)
    })?;
    let Some(source) = sources.first() else {
        return Ok(());
    };
    eprintln!(
        "uhura {command}: retired `.relay` source `{}`; rename it to `.uhura` and map it as a module in the current uhura.toml",
        source.display()
    );
    Err(ExitCode::from(2))
}

#[cfg(test)]
pub(super) fn checked_test_program() -> Program {
    const MACHINE: &str = r#"language uhura 0.3
module test.cli.machine@1

machine Counter {
  input = | increment
  command = Never
  outcome = | accepted commit
  state { count: Int = 0 }
  observe { count = count }
  on increment {
    set count = count + 1
    finish accepted
  }
}
"#;
    const EVIDENCE: &str = r#"language uhura 0.3
module test.cli.evidence@1

use evidence
import { Counter } from "test.cli.machine@1"

scenario increment for Counter {
  start
  send increment
  expect accepted commands []
  pin done
}

example incremented = increment::done
"#;
    const UI: &str = r#"language uhura 0.3
module test.cli.ui@1

use ui
import { Counter } from "test.cli.machine@1"

ui CounterView for Counter(view) {
  <button on press -> increment>Count: {view.count}</button>
}
"#;
    let parsed = parse_project([
        SyntaxSourceFile::new(FileId(0), "machine.uhura", MACHINE),
        SyntaxSourceFile::new(FileId(1), "evidence.uhura", EVIDENCE),
        SyntaxSourceFile::new(FileId(2), "ui.uhura", UI),
    ]);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = uhura_check::check_project(&parsed.project);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut program = checked.program.expect("test project lowers");
    program.freeze_program_hashes();
    program
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    const V04_MACHINE: &str = r#"pub machine Counter {
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
            "uhura-cli-v04-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_v04_manifest(root: &Path, extra: &str) {
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
    fn manifest_selected_v04_lowers_the_declared_module() {
        let root = project_root("accepted");
        write_v04_manifest(&root, "");
        std::fs::write(root.join("counter.uhura"), V04_MACHINE).unwrap();

        let project = load(&root, "check").expect("load 0.4 project");
        assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
        let program = project.program.expect("0.4 program");
        assert_eq!(program.language, "uhura 0.4");
        assert_eq!(program.machines.len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_selected_v04_lowers_the_complete_module_graph() {
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
            V04_MACHINE
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
        assert_eq!(program.language, "uhura 0.4");
        assert_eq!(program.machines.len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_selected_v04_attaches_separately_versioned_evidence() {
        let root = project_root("evidence");
        write_v04_manifest(
            &root,
            r#"[evidence]
sources = ["counter.evidence.uhura"]
"#,
        );
        std::fs::write(root.join("counter.uhura"), V04_MACHINE).unwrap();
        std::fs::write(
            root.join("counter.evidence.uhura"),
            r#"language uhura 0.3
module test.counter.evidence@1

use evidence
import { Counter } from "test.counter@1"

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  pin done
}

example incremented = increment::done
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
    fn manifest_selected_v04_rejects_missing_and_unlisted_sources() {
        let missing_root = project_root("missing");
        write_v04_manifest(&missing_root, "");
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
        write_v04_manifest(&unlisted_root, "");
        std::fs::write(unlisted_root.join("counter.uhura"), V04_MACHINE).unwrap();
        std::fs::write(unlisted_root.join("stray.uhura"), V04_MACHINE).unwrap();
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
    fn manifest_selected_v04_admits_exact_locked_path_dependency() {
        let root = project_root("dependencies");
        write_v04_manifest(
            &root,
            r#"[dependencies.shared]
package = "test.shared"
version = 1
path = "vendor/shared"
"#,
        );
        std::fs::create_dir_all(root.join("vendor/shared/deps/base")).unwrap();
        let root_source = V04_MACHINE
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
        let LoadedProjectManifest::V04(vendor_manifest) =
            load_project_manifest(vendor_manifest_text).unwrap()
        else {
            panic!("0.4 vendor manifest")
        };
        let uhura_check::project_manifest::LoadedProjectManifest::V04(base_manifest) =
            load_project_manifest(base_manifest_text).unwrap()
        else {
            panic!("0.4 base manifest")
        };
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
