//! Canonical project-compilation behavior.

use std::collections::BTreeMap;

use uhura_base::FileId;
use uhura_check::project_lock::CapturedPackage;
use uhura_check::project_manifest::{
    LogicalModulePath, ProjectManifest, ProjectPath, load_project_manifest,
};
use uhura_check::{
    CapturedPackageModules, ProjectSource, check_package_graph_with_evidence, compile_project,
};

const ROOT_MANIFEST: &str = r#"[project]
name = "test.counter"
version = 1
language = "0.4"

[modules]
counter = "counter.uhura"

[evidence.modules]
evidence = "counter.evidence.uhura"

[dependencies.shared]
package = "test.shared"
version = 1
path = "vendor/shared"
"#;

const DEPENDENCY_MANIFEST: &str = r#"[project]
name = "test.shared"
version = 1
language = "0.4"

[modules]
values = "values.uhura"
"#;

const ROOT_SOURCE: &str = r#"use shared::values::INITIAL;

pub machine Counter {
  events {
    Increment,
  }

  outcomes {
    commit Accepted,
  }

  state {
    count: Int = INITIAL,
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

const DEPENDENCY_SOURCE: &str = "pub const INITIAL: Int = 0;\n";

const EVIDENCE_SOURCE: &str = r#"use crate::counter::Counter;

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  pin done
}

example incremented = increment::done;
"#;

fn manifest(source: &str) -> ProjectManifest {
    load_project_manifest(source).unwrap()
}

fn dependency_capture(source: &[u8]) -> CapturedPackage {
    CapturedPackage {
        manifest: manifest(DEPENDENCY_MANIFEST),
        source: ProjectPath::parse("vendor/shared").unwrap(),
        modules: [(LogicalModulePath::parse("values").unwrap(), source.to_vec())]
            .into_iter()
            .collect(),
        resolved_dependencies: BTreeMap::new(),
        resources: BTreeMap::new(),
    }
}

#[test]
fn canonical_service_matches_the_explicit_parse_graph_check_pipeline() {
    let root_manifest = manifest(ROOT_MANIFEST);
    let dependency = dependency_capture(DEPENDENCY_SOURCE.as_bytes());
    let sources = [
        ProjectSource::new(FileId(0), "counter.uhura", ROOT_SOURCE),
        ProjectSource::new(FileId(1), "counter.evidence.uhura", EVIDENCE_SOURCE),
        ProjectSource::new(FileId(2), "vendor/shared/values.uhura", DEPENDENCY_SOURCE),
    ];

    let canonical = compile_project(&root_manifest, &sources, std::slice::from_ref(&dependency));
    assert!(
        canonical.diagnostics.is_empty(),
        "{:#?}",
        canonical.diagnostics
    );

    let root = uhura_syntax::parse(
        uhura_syntax::SourceIdentity::new(0, "test.counter@1", "counter", "counter.uhura"),
        ROOT_SOURCE,
    );
    let shared = uhura_syntax::parse(
        uhura_syntax::SourceIdentity::new(
            2,
            "test.shared@1",
            "values",
            "vendor/shared/values.uhura",
        ),
        DEPENDENCY_SOURCE,
    );
    assert!(root.diagnostics.is_empty());
    assert!(shared.diagnostics.is_empty());
    let evidence = uhura_syntax::parse(
        uhura_syntax::SourceIdentity::new(
            1,
            "test.counter@1",
            "evidence",
            "counter.evidence.uhura",
        ),
        EVIDENCE_SOURCE,
    );
    assert!(evidence.diagnostics.is_empty());
    let explicit = check_package_graph_with_evidence(
        "test.counter@1",
        &[
            CapturedPackageModules {
                package: "test.counter@1".into(),
                dependencies: [("shared".into(), "test.shared@1".into())]
                    .into_iter()
                    .collect(),
                modules: vec![root.module],
            },
            CapturedPackageModules {
                package: "test.shared@1".into(),
                dependencies: BTreeMap::new(),
                modules: vec![shared.module],
            },
        ],
        &[evidence.module],
    );
    assert!(
        explicit.diagnostics.is_empty(),
        "{:#?}",
        explicit.diagnostics
    );
    assert_eq!(canonical.program, explicit.program);
    assert_eq!(canonical.provenance, explicit.provenance);
}

#[test]
fn diagnostic_order_is_independent_of_source_inventory_order() {
    let mut root_manifest = manifest(ROOT_MANIFEST);
    root_manifest.evidence.clear();
    let bad_root = "pub mashine Counter {}\n";
    let bad_dependency = "pub const INITIAL: Int = ;\n";
    let dependency = dependency_capture(bad_dependency.as_bytes());
    let ordered = [
        ProjectSource::new(FileId(3), "counter.uhura", bad_root),
        ProjectSource::new(FileId(1), "vendor/shared/values.uhura", bad_dependency),
    ];
    let reversed = [ordered[1], ordered[0]];

    let first = compile_project(&root_manifest, &ordered, std::slice::from_ref(&dependency));
    let second = compile_project(&root_manifest, &reversed, std::slice::from_ref(&dependency));
    let signature = |diagnostics: &[uhura_base::Diagnostic]| {
        diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.span.file.0,
                    diagnostic.span.start,
                    diagnostic.span.end,
                    diagnostic.code,
                    diagnostic.rule,
                    diagnostic.message.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        signature(&first.diagnostics),
        signature(&second.diagnostics)
    );
    assert_eq!(
        signature(&first.diagnostics),
        vec![
            (
                1,
                25,
                26,
                "R1001",
                "uhura-0.4/parse/invalid-expression",
                "package.test.shared@1.modules.values: expected expression, found `;`".into(),
            ),
            (
                3,
                4,
                11,
                "R1001",
                "uhura-0.4/parse/invalid-declaration",
                "unknown module declaration `mashine`; expected `machine`, `part`, `ui`, `scenario`, `example`, `checkpoint`, `struct`, `enum`, `key`, `const`, or `fn`".into(),
            ),
        ]
    );
    assert!(first.program.is_none());
    assert!(first.provenance.is_none());
}
