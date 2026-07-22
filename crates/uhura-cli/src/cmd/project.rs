//! Shared canonical Uhura project admission for CLI commands.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use uhura_base::{Diagnostic, FileId, Severity, SourceMap, Span};
use uhura_check::resource_manifest::ResourceManifest;
use uhura_check::{CheckedIconFonts, IconFontInput};
use uhura_core::Program;
use uhura_project::{
    ProjectSourceSnapshot, ResolvedProject, capture_project_snapshot, resolve_project,
};

use crate::fsio::SourceFile;

pub(super) struct Project {
    pub files: Vec<SourceFile>,
    pub source_map: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
    pub program: Option<Program>,
}

pub(super) fn load(root: &Path, _command: &str) -> Result<Project, ExitCode> {
    let snapshot = capture_project_snapshot(root);
    let resolved = match resolve_project(&snapshot) {
        Ok(resolved) => resolved,
        Err(rejection) => {
            return Ok(Project {
                files: Vec::new(),
                source_map: rejection.source_map,
                diagnostics: rejection.diagnostics,
                program: None,
            });
        }
    };
    Ok(load_checked_project(root, &snapshot, resolved))
}

fn load_checked_project(
    root: &Path,
    snapshot: &ProjectSourceSnapshot,
    resolved: ResolvedProject,
) -> Project {
    let manifest_file = resolved.manifest_file();
    let files = resolved
        .non_generated_sources()
        .map(|source| SourceFile {
            rel_path: source.path.clone(),
            abs_path: root.join(&source.path),
            text: source.text.clone(),
        })
        .collect::<Vec<_>>();
    let icon_fonts =
        match load_cli_icon_fonts(snapshot, &resolved.manifest().resources, manifest_file) {
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
                return Project {
                    files,
                    source_map: resolved.into_source_map(),
                    diagnostics,
                    program: None,
                };
            }
        };
    let mut checked = resolved.check();
    let mut diagnostics = std::mem::take(&mut checked.diagnostics);
    if let Some(program) = checked.program.as_ref() {
        diagnostics.extend(uhura_check::icon_token_diagnostics(
            program,
            &icon_fonts,
            resolved
                .sources()
                .iter()
                .map(|source| (source.file, source.path.as_str())),
        ));
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        checked.program = None;
    }
    let program = checked.program;
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

    Project {
        files,
        source_map: resolved.into_source_map(),
        diagnostics,
        program,
    }
}

fn load_cli_icon_fonts(
    snapshot: &ProjectSourceSnapshot,
    resources: &ResourceManifest,
    manifest_file: FileId,
) -> Result<CheckedIconFonts, Vec<Diagnostic>> {
    let mut inputs = BTreeMap::new();
    let mut messages = Vec::new();
    for (name, family) in &resources.icons.families {
        let font_bytes = match snapshot.read_bytes(Path::new(&family.font)) {
            Ok(bytes) => bytes,
            Err(message) => {
                messages.push(format!("icons.{name}.font: {message}"));
                None
            }
        };
        let glyphs_text = match snapshot.read_text(&family.glyphs) {
            Ok(text) => text,
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
    let manifest = uhura_check::project_manifest::load_project_manifest(MANIFEST)
        .expect("test manifest is current");
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
    use uhura_check::project_lock::CapturedPackage;
    use uhura_check::project_manifest::load_project_manifest;

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
    fn web_app_cli_rejects_evidence_role_drift_before_publication() {
        let root = project_root("framework-evidence-role");
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("components")).unwrap();
        std::fs::write(
            root.join("uhura.toml"),
            r#"[project]
name = "test.web-app"
version = 1
language = "0.4"

[framework]
profile = "web-app"
version = 1
machine = "crate::program::App"
location = "crate::routing::Location"

[modules]
program = "machine.uhura"
routing = "routing.uhura"
"#,
        )
        .unwrap();
        std::fs::write(root.join("routing.uhura"), "pub enum Location { Home }\n").unwrap();
        std::fs::write(
            root.join("machine.uhura"),
            r#"use uhura::web_router::Router;
use crate::framework::routes::APPLICATION_ROUTES;
use crate::routing::Location;

pub machine App {
  port router = Router<Location> { routes: APPLICATION_ROUTES };
  events { Refresh }
  outcomes { commit Accepted }
  state { location: Option<Location> = None }
  observe { location }
  on Refresh { Accepted }
  on router.Changed(next) {
    location = Some(next);
    Accepted
  }
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("app/page.uhura"),
            r#"use uhura::ui;
use crate::program::App;

pub ui HomePage for App(view) {
  <main>Home</main>
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("components/card.uhura"),
            r#"use uhura::ui;

pub ui Card(label: Text) {
  <p>{label}</p>
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("components/card.examples.uhura"),
            r#"use crate::components::card::Card;
use crate::program::App;

scenario card_state for App {
  start
  pin frame
}

example card for Card(label: "Hello") as surface = card_state::frame;
"#,
        )
        .unwrap();

        let project = load(&root, "check").expect("framework role drift is diagnostic output");
        assert!(project.program.is_none());
        let diagnostic = project
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.rule == "uhura/framework-evidence-role")
            .expect("shared project validation reaches the CLI");
        assert_eq!(diagnostic.code, "R1004");
        assert_eq!(
            project.source_map.path(diagnostic.span.file),
            "components/card.examples.uhura"
        );
        assert!(diagnostic.message.contains("declares `surface`"));
        assert!(diagnostic.message.contains("role is `component`"));

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
        assert!(
            project
                .files
                .iter()
                .any(|source| source.rel_path == "vendor/shared/values.uhura")
        );
        assert!(
            project
                .files
                .iter()
                .any(|source| source.rel_path == "vendor/shared/deps/base/values.uhura")
        );

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
