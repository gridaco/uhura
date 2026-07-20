//! `uhura trace [path] [--script=<scenario>] [--expanded]`.
//!
//! Evidence is source-authored executable behavior. The trace command exposes
//! that same deterministic runner as canonical JSONL; there is no second
//! fixture-script engine.

use std::process::ExitCode;

use uhura_base::to_canonical_json;

use crate::CommonArgs;

pub const TRACE_PROTOCOL: &str = "uhura-trace/0";

pub fn run(common: &CommonArgs, selector: Option<&str>, expanded: bool) -> ExitCode {
    let program = match super::project::require_program(&common.root, "trace") {
        Ok(program) => program,
        Err(code) => return code,
    };
    match evidence_trace_lines(&program, selector, expanded) {
        Ok((lines, passed)) => {
            for line in lines {
                println!("{line}");
            }
            if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("uhura trace: {error}");
            ExitCode::from(1)
        }
    }
}

pub fn evidence_trace_lines(
    program: &uhura_core::Program,
    selector: Option<&str>,
    expanded: bool,
) -> Result<(Vec<String>, bool), String> {
    if program.evidence.scenarios.is_empty() {
        return Err("the Uhura program declares no evidence scenarios".into());
    }
    let selected = selector
        .map(|selector| resolve_scenario(program, selector))
        .transpose()?;
    let report = program.run_evidence();
    let scenarios = report
        .scenarios
        .iter()
        .filter(|scenario| {
            selected
                .as_deref()
                .is_none_or(|identity| scenario.scenario == identity)
        })
        .collect::<Vec<_>>();
    if scenarios.is_empty() {
        return Err("the evidence runner produced no selected scenario".into());
    }

    let mut lines = Vec::new();
    for scenario in &scenarios {
        let origin = program
            .evidence
            .scenarios
            .get(&scenario.scenario)
            .map(|definition| exact_transport_json(&definition.origin))
            .unwrap_or(serde_json::Value::Null);
        lines.push(line(serde_json::json!({
            "protocol": TRACE_PROTOCOL,
            "kind": "scenario-start",
            "scenario": scenario.scenario,
            "machine": scenario.machine,
            "origin": origin,
            "totalSteps": scenario.total_steps.to_string(),
        })));
        if let Some(genesis) = &scenario.genesis {
            lines.push(line(serde_json::json!({
                "protocol": TRACE_PROTOCOL,
                "kind": "genesis",
                "scenario": scenario.scenario,
                "receipt": exact_transport_json(genesis),
            })));
        }
        for (index, receipt) in scenario.receipts.iter().enumerate() {
            lines.push(line(serde_json::json!({
                "protocol": TRACE_PROTOCOL,
                "kind": "reaction",
                "scenario": scenario.scenario,
                "index": index.to_string(),
                "receipt": exact_transport_json(receipt),
            })));
        }
        let mut summary = serde_json::json!({
            "protocol": TRACE_PROTOCOL,
            "kind": "scenario",
            "scenario": scenario.scenario,
            "machine": scenario.machine,
            "status": scenario.status,
            "totalSteps": scenario.total_steps.to_string(),
            "executedSteps": scenario.executed_steps.to_string(),
            "publishedPins": scenario.published_pins,
            "failure": exact_transport_json(&scenario.failure),
        });
        if expanded {
            summary
                .as_object_mut()
                .expect("trace summary is an object")
                .insert(
                    "finalSnapshot".into(),
                    exact_transport_json(&scenario.final_snapshot),
                );
        }
        lines.push(line(summary));
    }

    let failures = report
        .failures
        .iter()
        .filter(|failure| {
            selected
                .as_deref()
                .is_none_or(|identity| failure.scenario.as_deref() == Some(identity))
        })
        .collect::<Vec<_>>();
    let passed = scenarios
        .iter()
        .all(|scenario| scenario.status == uhura_core::ScenarioStatus::Passed)
        && failures.is_empty();
    lines.push(line(serde_json::json!({
        "protocol": TRACE_PROTOCOL,
        "kind": "result",
        "selection": selected,
        "scenarioCount": scenarios.len().to_string(),
        "passed": passed,
        "failures": exact_transport_json(&failures),
    })));
    Ok((lines, passed))
}

fn resolve_scenario(program: &uhura_core::Program, selector: &str) -> Result<String, String> {
    if program.evidence.scenarios.contains_key(selector) {
        return Ok(selector.into());
    }
    let matches = program
        .evidence
        .scenarios
        .keys()
        .filter(|identity| {
            identity
                .rsplit_once("::")
                .map_or(identity.as_str(), |(_, name)| name)
                == selector
        })
        .cloned()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [identity] => Ok(identity.clone()),
        [] => Err(format!(
            "no evidence scenario matches `--script={selector}`"
        )),
        _ => Err(format!(
            "scenario name `{selector}` is ambiguous; use one of: {}",
            matches.join(", ")
        )),
    }
}

fn line(value: serde_json::Value) -> String {
    to_canonical_json(&value)
}

fn exact_transport_json(value: &impl serde::Serialize) -> serde_json::Value {
    let mut value = serde_json::to_value(value).expect("trace value serializes");
    stringify_exact_numeric_fields(&mut value);
    value
}

fn stringify_exact_numeric_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                stringify_exact_numeric_fields(value);
            }
        }
        serde_json::Value::Object(object) => {
            for (name, value) in object {
                if matches!(
                    name.as_str(),
                    "sequence" | "next_sequence" | "ordinal" | "machine_sequence" | "step_index"
                ) && value.is_number()
                {
                    *value = serde_json::Value::String(value.to_string());
                } else {
                    stringify_exact_numeric_fields(value);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_selects_one_scenario_and_is_deterministic() {
        let program = super::super::project::checked_test_program();
        let first = evidence_trace_lines(&program, Some("increment"), true).unwrap();
        let second = evidence_trace_lines(&program, Some("increment"), true).unwrap();
        assert_eq!(first, second);
        assert!(first.1);
        assert!(first.0.iter().all(|record| {
            serde_json::from_str::<serde_json::Value>(record).unwrap()["protocol"] == TRACE_PROTOCOL
        }));
    }
}
