use uhura_base::FileId;
use uhura_check::check_v04_project_modules_with_evidence;
use uhura_syntax::v04::{SourceIdentity, parse as parse_v04};
use uhura_syntax::{SourceFile, parse_project};

const CORE: &str = r#"
pub machine Counter {
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

fn core() -> uhura_syntax::v04::Module {
    let parsed = parse_v04(
        SourceIdentity::new(1, "example.core@1", "counter", "counter.uhura"),
        CORE,
    );
    assert!(parsed.is_ok(), "{:#?}", parsed.diagnostics);
    parsed.module
}

fn evidence(source: &str) -> Vec<uhura_syntax::ast::Module> {
    let parsed = parse_project([SourceFile::new(FileId(2), "evidence/counter.uhura", source)]);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    parsed.project.modules
}

#[test]
fn separately_versioned_evidence_runs_against_the_v04_core() {
    let evidence = evidence(
        r#"language uhura 0.3
module example.evidence@1

use evidence
import { Counter } from "example.core@1"

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  expect observation { count: 1 }
  pin done
}

example incremented = increment::done
"#,
    );
    let output = check_v04_project_modules_with_evidence(&[core()], &evidence);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert!(output.provenance.is_some());
    let program = output.program.expect("0.4 core plus evidence");
    assert_eq!(program.language, "uhura 0.4");
    let report = program.run_evidence();
    assert!(report.passed, "{:#?}", report.failures);
    assert_eq!(report.scenarios.len(), 1);
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
        r#"language uhura 0.3
module example.evidence@1

use evidence
import { Counter } from "example.core@1"

const illicit: Int = 1
"#,
    );
    let output = check_v04_project_modules_with_evidence(&[parsed.module], &evidence);
    assert!(output.program.is_none());
    let rules = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(rules.contains("uhura-0.4/private-evidence-import"));
    assert!(rules.contains("uhura-0.4/evidence-core-declaration"));
}
