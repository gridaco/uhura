use uhura_check::check_v04_project_modules_with_evidence;
use uhura_syntax::v04::{SourceIdentity, parse as parse_v04};

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

fn core() -> uhura_syntax::v04::Module {
    let parsed = parse_v04(
        SourceIdentity::new(1, "example.core@1", "counter", "counter.uhura"),
        CORE,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    parsed.module
}

fn evidence(source: &str) -> Vec<uhura_syntax::v04::Module> {
    let parsed = parse_v04(
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

#[test]
fn manifest_role_evidence_runs_against_the_v04_core() {
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
    let output = check_v04_project_modules_with_evidence(&[core()], &evidence);
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
    let output = check_v04_project_modules_with_evidence(&[core()], &evidence);
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
    let output = check_v04_project_modules_with_evidence(&[core()], &evidence);
    assert!(output.program.is_none());
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.rule == "uhura-0.4/unsupported"
            && diagnostic.message.contains("unknown field `wrong`")
    }));
}

#[test]
fn evidence_cannot_import_private_core_or_contribute_values() {
    let private_core = CORE.replace("pub machine Counter", "machine Counter");
    let parsed = parse_v04(
        SourceIdentity::new(1, "example.core@1", "counter", "counter.uhura"),
        &private_core,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    let evidence = evidence(
        r#"use crate::counter::Counter;

const ILLICIT: Int = 1;
"#,
    );
    let output = check_v04_project_modules_with_evidence(&[parsed.module], &evidence);
    assert!(output.program.is_none());
    let rules = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(rules.contains("uhura-0.4/private-import"));
    assert!(rules.contains("uhura-0.4/core-declaration-in-evidence"));
}
