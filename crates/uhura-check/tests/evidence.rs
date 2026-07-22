use uhura_check::check_project_modules_with_evidence;
use uhura_core::{EvidencePresentationKind, RenderNode, Value};
use uhura_syntax::{SourceIdentity, parse};

const CORE: &str = r#"
pub enum Reason {
  Nope,
}

pub machine Counter {
  events {
    Increment,
    Reject,
  }

  outcomes {
    commit Accepted,
    abort Invalid(reason: Reason),
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

  on Reject {
    return Invalid(Reason::Nope);
  }
}
"#;

fn core() -> uhura_syntax::Module {
    let parsed = parse(
        SourceIdentity::new(1, "example.core@1", "counter", "counter.uhura"),
        CORE,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    parsed.module
}

fn evidence(source: &str) -> Vec<uhura_syntax::Module> {
    let parsed = parse(
        SourceIdentity::new(
            2,
            "example.core@1",
            "evidence/counter",
            "evidence/counter.uhura",
        ),
        source,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    vec![parsed.module]
}

fn evidence_module(file: u32, logical: &str, source: &str) -> uhura_syntax::Module {
    let parsed = parse(
        SourceIdentity::new(file, "example.core@1", logical, format!("{logical}.uhura")),
        source,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    parsed.module
}

fn component_core() -> uhura_syntax::Module {
    evidence_module(
        4,
        "components",
        r#"use uhura::ui;

pub const PREVIEW_MESSAGE: Text = "A deterministic preview";

pub ui Notice(message: Text) emits {
  Dismissed,
} {
  <button on press -> Dismissed>{message}</button>
}

pub ui Empty() {
  <p>Empty component</p>
}
"#,
    )
}

fn assert_no_render_events(nodes: &[RenderNode]) {
    for node in nodes {
        if let RenderNode::Element {
            events, children, ..
        } = node
        {
            assert!(events.is_empty(), "component evidence must be inert");
            assert_no_render_events(children);
        }
    }
}

#[test]
fn manifest_role_evidence_runs_against_the_core() {
    let evidence = evidence(
        r#"use crate::counter::{Counter, Reason};

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  expect observation { count: 1 }
  send Reject
  expect Invalid { reason: Reason::Nope } commands []
  pin done
}

example incremented = increment::done;
"#,
    );
    let output = check_project_modules_with_evidence(&[core()], &evidence);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert!(output.provenance.is_some());
    let program = output.program.expect("0.4 core plus evidence");
    assert_eq!(program.machine_program.language, "uhura 0.4");
    let report = program.run_evidence();
    assert!(report.passed, "{:#?}", report.failures);
    assert_eq!(report.scenarios.len(), 1);
}

#[test]
fn snapshot_scenario_inference_preserves_authored_evidence_order() {
    let evidence = evidence(
        r#"use crate::counter::Counter;

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  pin done
}

scenario continued from increment::done {
  expect observation { count: 1 }
  pin same
}
"#,
    );
    let output = check_project_modules_with_evidence(&[core()], &evidence);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let report = output
        .program
        .expect("ordered evidence lowers")
        .run_evidence();
    assert!(report.passed, "{:#?}", report.failures);
    assert_eq!(report.scenarios.len(), 2);
}

#[test]
fn evidence_record_variant_patterns_are_checked_by_field_name() {
    let evidence = evidence(
        r#"use crate::counter::{Counter, Reason};

scenario invalid_shape for Counter {
  start
  send Reject
  expect Invalid { wrong: Reason::Nope } commands []
}
"#,
    );
    let output = check_project_modules_with_evidence(&[core()], &evidence);
    assert!(output.program.is_none());
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura-0.4/unsupported"
            && diagnostic.message.contains("unknown field `wrong`")
    }));
}

#[test]
fn evidence_cannot_import_private_core_or_contribute_values() {
    let private_core = CORE.replace("pub machine Counter", "machine Counter");
    let parsed = parse(
        SourceIdentity::new(1, "example.core@1", "counter", "counter.uhura"),
        &private_core,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    let evidence = evidence(
        r#"use crate::counter::Counter;

const ILLICIT: Int = 1;
"#,
    );
    let output = check_project_modules_with_evidence(&[parsed.module], &evidence);
    assert!(output.program.is_none());
    let rules = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(rules.contains("uhura-0.4/private-import"));
    assert!(rules.contains("uhura-0.4/core-declaration-in-evidence"));
}

#[test]
fn explicit_crate_evidence_references_cross_admitted_evidence_modules() {
    let shared = evidence_module(
        2,
        "shared",
        r#"use crate::counter::Counter;

scenario base for Counter {
  start
  pin frame
}

checkpoint saved = base::frame;
"#,
    );
    let consumer = evidence_module(
        3,
        "consumer",
        r#"scenario resumed from crate::shared::saved {
  expect observation { count: 0 }
  pin same
}

example direct = crate::shared::base::frame;
"#,
    );
    let output = check_project_modules_with_evidence(&[core()], &[shared, consumer]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let report = output
        .program
        .expect("cross-module evidence")
        .run_evidence();
    assert!(report.passed, "{:#?}", report.failures);
    assert_eq!(report.scenarios.len(), 2);
    assert_eq!(report.artifacts.examples.len(), 1);
}

#[test]
fn pure_component_examples_lower_exact_canonical_props_and_project_inertly() {
    let evidence = evidence(
        r#"use crate::counter::Counter;
use crate::components::{Empty, Notice, PREVIEW_MESSAGE};

scenario component_frame for Counter {
  start
  pin shown
}

example notice
  for Notice(message: PREVIEW_MESSAGE) as component default
  = component_frame::shown;

example empty
  for Empty() as component
  = component_frame::shown;
"#,
    );
    let output = check_project_modules_with_evidence(&[core(), component_core()], &evidence);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let program = output.program.expect("pure component examples lower");
    let metadata = &program.evidence.example_metadata["example.core@1::notice"];
    assert_eq!(
        metadata.presentation.as_deref(),
        Some("example.core@1::Notice")
    );
    assert_eq!(metadata.kind, Some(EvidencePresentationKind::Component));
    assert_eq!(
        metadata.component_props,
        vec![(
            "message".into(),
            Value::Text("A deterministic preview".into())
        )]
    );

    let (instance, _) = program
        .machine_program
        .admit("example.core@1::Counter", Value::Unit, "component-evidence")
        .expect("evidence snapshot machine admits");
    let projection = program
        .project_component(
            &instance,
            metadata.presentation.as_deref().expect("component target"),
            &metadata.component_props,
        )
        .expect("direct component projects");
    assert!(projection.bindings.is_empty());
    assert_no_render_events(&projection.document.nodes);
}

#[test]
fn component_example_calls_are_mandatory_and_props_are_exact() {
    let missing_call = evidence(
        r#"use crate::counter::Counter;
use crate::components::Empty;

scenario frame for Counter {
  start
  pin shown
}

example invalid for Empty as component = frame::shown;
"#,
    );
    let output = check_project_modules_with_evidence(&[core(), component_core()], &missing_call);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.rule == "uhura/component-example-argument-list" })
    );

    let invalid_props = evidence(
        r#"use crate::counter::Counter;
use crate::components::{Notice, PREVIEW_MESSAGE};

scenario frame for Counter {
  start
  pin shown
}

example invalid
  for Notice(extra: PREVIEW_MESSAGE) as component
  = frame::shown;
"#,
    );
    let output = check_project_modules_with_evidence(&[core(), component_core()], &invalid_props);
    assert!(output.program.is_none());
    let rules = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(rules.contains("uhura/unknown-component-example-prop"));
    assert!(rules.contains("uhura/missing-component-example-prop"));
}
