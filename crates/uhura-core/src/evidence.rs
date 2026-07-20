//! Deterministic execution of checked Uhura evidence declarations.
//!
//! Evidence is deliberately a client of the ordinary [`Program`] runtime. A
//! scenario can bind sealed fixtures and decide which inputs to submit, but it
//! cannot run a second transition implementation, synthesize host success, or
//! mutate an instance between reactions.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uhura_port::{
    OBSERVATION_CONTRACT_HASH, REQUEST_PORT_CONTRACT_HASH, ROUTER_CONTRACT_HASH,
    SINK_PORT_CONTRACT_HASH,
};

use super::ir::{
    EvidenceRef, EvidenceStep, Expr, Machine, PortDef, Program, Scenario, ScenarioOrigin, SourceRef,
};
use super::runtime::{
    CHECKPOINT_PROTOCOL, Checkpoint, GenesisReceipt, Instance, InstanceLifecycle, ReactionReceipt,
    ReactionResolution, evaluate_condition_with_locals, evaluate_with_locals, match_pattern,
    record_map,
};
use super::value::Value;

pub const EVIDENCE_REPORT_PROTOCOL: &str = "uhura-evidence-report/0";

/// Executes a program's checked evidence suite in deterministic source order.
pub struct EvidenceRunner<'program> {
    program: &'program Program,
}

impl<'program> EvidenceRunner<'program> {
    pub fn new(program: &'program Program) -> Self {
        Self { program }
    }

    /// Run every scenario and resolve every exported artifact.
    ///
    /// A failed scenario does not publish any of its pins. Later scenarios are
    /// still visited and receive an explicit missing-origin failure instead of
    /// being silently skipped.
    pub fn run(&self) -> EvidenceReport {
        let mut report = EvidenceReport {
            protocol: EVIDENCE_REPORT_PROTOCOL.into(),
            passed: true,
            scenarios: Vec::new(),
            artifacts: EvidenceArtifacts::default(),
            failures: Vec::new(),
        };
        let mut published_pins = BTreeMap::<PinKey, EvidencePinArtifact>::new();
        let mut seen_scenario_ids = BTreeSet::new();
        let mut scenarios = self.program.evidence.scenarios.iter().collect::<Vec<_>>();
        scenarios.sort_by(|(left_key, left), (right_key, right)| {
            source_order(&left.source)
                .cmp(&source_order(&right.source))
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left_key.cmp(right_key))
        });

        for (map_key, scenario) in scenarios {
            let id_is_new = seen_scenario_ids.insert(scenario.id.clone());
            let structural_failure = if map_key != &scenario.id {
                Some(Failure::at(
                    EvidenceFailureCode::MalformedEvidence,
                    &scenario.id,
                    None,
                    &scenario.source,
                    format!(
                        "scenario map key `{map_key}` does not match declared id `{}`",
                        scenario.id
                    ),
                ))
            } else if !id_is_new {
                Some(Failure::at(
                    EvidenceFailureCode::MalformedEvidence,
                    &scenario.id,
                    None,
                    &scenario.source,
                    format!("scenario id `{}` is declared more than once", scenario.id),
                ))
            } else {
                None
            };

            let execution = match structural_failure {
                Some(failure) => ScenarioExecution::failed_without_context(scenario, failure),
                None => self.execute_scenario(scenario, &published_pins),
            };
            if let Some(failure) = &execution.failure {
                report.failures.push(failure.public.as_ref().clone());
            } else {
                for (name, snapshot) in &execution.pins {
                    let artifact = EvidencePinArtifact {
                        scenario: scenario.id.clone(),
                        pin: name.clone(),
                        source: execution
                            .pin_sources
                            .get(name)
                            .cloned()
                            .unwrap_or_else(|| scenario.source.clone()),
                        source_id: execution
                            .pin_sources
                            .get(name)
                            .map_or_else(|| source_id(&scenario.source), source_id),
                        snapshot: snapshot.clone(),
                    };
                    published_pins.insert(PinKey::new(&scenario.id, name), artifact.clone());
                    report
                        .artifacts
                        .pins
                        .insert(format!("{}::{name}", scenario.id), artifact);
                }
            }
            report.scenarios.push(execution.into_report(scenario));
        }

        self.resolve_examples(&published_pins, &mut report);
        self.resolve_checkpoints(&published_pins, &mut report);
        report.passed = report.failures.is_empty();
        report
    }

    fn execute_scenario(
        &self,
        scenario: &Scenario,
        published_pins: &BTreeMap<PinKey, EvidencePinArtifact>,
    ) -> ScenarioExecution {
        let context = match self.prepare_context(scenario, published_pins) {
            Ok(context) => context,
            Err(failure) => return ScenarioExecution::failed_without_context(scenario, failure),
        };
        let mut execution = ScenarioExecution::new(context);
        let result = self.run_steps(scenario, published_pins, &mut execution);
        if let Err(failure) = result {
            execution.failure = Some(failure);
        } else if execution.context.instance.is_none() {
            execution.failure = Some(Failure::at(
                EvidenceFailureCode::InvalidLifecycle,
                &scenario.id,
                None,
                &scenario.source,
                "fresh scenario ended without `start`",
            ));
        }
        execution
    }

    fn prepare_context(
        &self,
        scenario: &Scenario,
        published_pins: &BTreeMap<PinKey, EvidencePinArtifact>,
    ) -> Result<ScenarioContext, Failure> {
        match &scenario.origin {
            ScenarioOrigin::Machine {
                machine,
                configuration,
            } => {
                self.program
                    .machine_program
                    .machines
                    .get(machine)
                    .ok_or_else(|| {
                        Failure::at(
                            EvidenceFailureCode::UnknownMachine,
                            &scenario.id,
                            None,
                            &scenario.source,
                            format!("unknown evidence machine `{machine}`"),
                        )
                    })?;
                Ok(ScenarioContext {
                    machine: machine.clone(),
                    configuration: configuration.clone(),
                    instance: None,
                    genesis: None,
                    fixtures: BTreeMap::new(),
                    receipts: Vec::new(),
                    restored: false,
                    executable_steps: 0,
                    last_step_was_reaction: false,
                })
            }
            ScenarioOrigin::Snapshot { reference } => {
                let pin = self
                    .resolve_reference(reference, published_pins)
                    .map_err(|message| {
                        Failure::at(
                            EvidenceFailureCode::MissingSnapshot,
                            &scenario.id,
                            None,
                            &scenario.source,
                            message,
                        )
                    })?;
                let instance = self.restore_snapshot(&pin.snapshot).map_err(|message| {
                    Failure::at(
                        EvidenceFailureCode::RestoreFailed,
                        &scenario.id,
                        None,
                        &scenario.source,
                        message,
                    )
                })?;
                Ok(ScenarioContext {
                    machine: pin.snapshot.machine.clone(),
                    configuration: pin.snapshot.configuration.clone(),
                    instance: Some(instance),
                    genesis: None,
                    fixtures: pin
                        .snapshot
                        .fixtures
                        .iter()
                        .map(|(name, fixture)| {
                            (name.clone(), FixtureBinding::from(fixture.clone()))
                        })
                        .collect(),
                    receipts: Vec::new(),
                    restored: true,
                    executable_steps: 0,
                    last_step_was_reaction: false,
                })
            }
        }
    }

    fn run_steps(
        &self,
        scenario: &Scenario,
        published_pins: &BTreeMap<PinKey, EvidencePinArtifact>,
        execution: &mut ScenarioExecution,
    ) -> Result<(), Failure> {
        for (step_index, step) in scenario.steps.iter().enumerate() {
            let source = step_source(step);
            let result = self.run_step(scenario, step_index, step, published_pins, execution);
            match result {
                Ok(()) => execution.executed_steps = step_index + 1,
                Err(mut failure) => {
                    if failure.public.source_id.is_empty() {
                        failure.public.source_id = source_id(source);
                    }
                    if failure.public.source.id.is_empty()
                        && failure.public.source.path == "<generated>"
                    {
                        failure.public.source = source.clone();
                    }
                    return Err(failure);
                }
            }
        }
        Ok(())
    }

    fn run_step(
        &self,
        scenario: &Scenario,
        step_index: usize,
        step: &EvidenceStep,
        published_pins: &BTreeMap<PinKey, EvidencePinArtifact>,
        execution: &mut ScenarioExecution,
    ) -> Result<(), Failure> {
        let source = step_source(step);
        let failure = |code, message: String| {
            Failure::at(code, &scenario.id, Some(step_index), source, message)
        };
        match step {
            EvidenceStep::Bind { port, fixture, .. } => {
                if execution.context.restored || execution.context.instance.is_some() {
                    return Err(failure(
                        EvidenceFailureCode::InvalidLifecycle,
                        "fixtures can only be bound before `start` in a fresh scenario".into(),
                    ));
                }
                if execution.context.fixtures.contains_key(port) {
                    return Err(failure(
                        EvidenceFailureCode::InvalidFixture,
                        format!("port `{port}` is bound more than once"),
                    ));
                }
                let machine = self
                    .machine(&execution.context.machine)
                    .map_err(|message| failure(EvidenceFailureCode::UnknownMachine, message))?;
                let port_definition = machine
                    .ports
                    .iter()
                    .find(|definition| definition.name == *port)
                    .ok_or_else(|| {
                        failure(
                            EvidenceFailureCode::InvalidFixture,
                            format!("fixture binds undeclared port `{port}`"),
                        )
                    })?;
                let binding = self
                    .evaluate_fixture(machine, port_definition, fixture)
                    .map_err(|message| failure(EvidenceFailureCode::InvalidFixture, message))?;
                execution.context.fixtures.insert(port.clone(), binding);
                execution.context.last_step_was_reaction = false;
            }
            EvidenceStep::Start { .. } => {
                if execution.context.restored || execution.context.instance.is_some() {
                    return Err(failure(
                        EvidenceFailureCode::InvalidLifecycle,
                        "`start` is permitted exactly once in a fresh scenario".into(),
                    ));
                }
                let machine = self
                    .machine(&execution.context.machine)
                    .map_err(|message| failure(EvidenceFailureCode::UnknownMachine, message))?;
                self.verify_complete_bindings(machine, &execution.context.fixtures)
                    .map_err(|message| failure(EvidenceFailureCode::IncompleteBindings, message))?;
                let instance_id = format!("evidence:{}", scenario.id);
                let (instance, genesis) = self
                    .program
                    .machine_program
                    .admit(
                        &execution.context.machine,
                        execution.context.configuration.clone(),
                        instance_id,
                    )
                    .map_err(|error| {
                        failure(
                            EvidenceFailureCode::AdmissionFailed,
                            format!("machine admission failed: {error}"),
                        )
                    })?;
                execution.context.instance = Some(instance);
                execution.context.genesis = Some(genesis);
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = false;
            }
            EvidenceStep::Send { input, .. } => {
                let value = self
                    .evaluate_current(&execution.context, input, BTreeMap::new())
                    .map_err(|message| failure(EvidenceFailureCode::EvaluationFailed, message))?;
                if self
                    .qualified_input_port(&execution.context.machine, &value)
                    .is_some()
                {
                    return Err(failure(
                        EvidenceFailureCode::InvalidInputKind,
                        "`send` requires a local input; use `deliver` for a qualified port input"
                            .into(),
                    ));
                }
                self.execute_reaction(&mut execution.context, value)
                    .map_err(|message| failure(EvidenceFailureCode::ReactionFailed, message))?;
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = true;
            }
            EvidenceStep::Deliver { input, .. } => {
                let value = self
                    .evaluate_current(&execution.context, input, BTreeMap::new())
                    .map_err(|message| failure(EvidenceFailureCode::EvaluationFailed, message))?;
                let (port, constructor) = self
                    .qualified_input_port(&execution.context.machine, &value)
                    .ok_or_else(|| {
                        failure(
                            EvidenceFailureCode::InvalidInputKind,
                            "`deliver` requires an input qualified by a declared port".into(),
                        )
                    })?;
                let machine = self
                    .machine(&execution.context.machine)
                    .map_err(|message| failure(EvidenceFailureCode::UnknownMachine, message))?;
                let port_definition = machine
                    .ports
                    .iter()
                    .find(|definition| definition.name == port)
                    .expect("qualified_input_port returned a declared port");
                if !execution.context.fixtures.contains_key(&port) {
                    return Err(failure(
                        EvidenceFailureCode::IncompleteBindings,
                        format!("delivery uses unbound port `{port}`"),
                    ));
                }
                if !constructor_declared(&port_definition.receive, &port, &constructor) {
                    return Err(failure(
                        EvidenceFailureCode::InvalidInputKind,
                        format!("`{constructor}` is not a receive constructor of port `{port}`"),
                    ));
                }
                self.execute_reaction(&mut execution.context, value)
                    .map_err(|message| failure(EvidenceFailureCode::ReactionFailed, message))?;
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = true;
            }
            EvidenceStep::ExpectReaction {
                outcome, commands, ..
            } => {
                if !execution.context.last_step_was_reaction {
                    return Err(failure(
                        EvidenceFailureCode::InvalidLifecycle,
                        "reaction expectation must immediately follow `send` or `deliver`".into(),
                    ));
                }
                let receipt = execution.context.receipts.last().ok_or_else(|| {
                    failure(
                        EvidenceFailureCode::InvalidLifecycle,
                        "reaction expectation has no preceding receipt".into(),
                    )
                })?;
                let actual_outcome = match &receipt.resolution {
                    ReactionResolution::Completed { outcome, .. } => outcome,
                    ReactionResolution::Fault { fault } => {
                        return Err(failure(
                            EvidenceFailureCode::ExpectationMismatch,
                            format!("reaction faulted instead of producing an outcome: {fault:?}"),
                        ));
                    }
                };
                let mut bindings = BTreeMap::new();
                let outcome_matches = match_pattern(outcome, actual_outcome, &mut bindings)
                    .map_err(|error| {
                        failure(
                            EvidenceFailureCode::EvaluationFailed,
                            format!("outcome pattern evaluation failed: {error}"),
                        )
                    })?;
                if !outcome_matches {
                    return Err(failure(
                        EvidenceFailureCode::ExpectationMismatch,
                        format!("outcome pattern did not match `{actual_outcome:?}`"),
                    ));
                }
                let expected_commands = commands
                    .iter()
                    .map(|command| {
                        self.evaluate_current(&execution.context, command, bindings.clone())
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|message| failure(EvidenceFailureCode::EvaluationFailed, message))?;
                if expected_commands != receipt.ordered_commands {
                    return Err(failure(
                        EvidenceFailureCode::ExpectationMismatch,
                        format!(
                            "ordered commands differ; expected {expected_commands:?}, got {:?}",
                            receipt.ordered_commands
                        ),
                    ));
                }
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = false;
            }
            EvidenceStep::ExpectObservationPattern { pattern, .. } => {
                let instance = current_instance(&execution.context)
                    .map_err(|message| failure(EvidenceFailureCode::InvalidLifecycle, message))?;
                let mut bindings = BTreeMap::new();
                let matches = match_pattern(pattern, &instance.observation, &mut bindings)
                    .map_err(|error| {
                        failure(
                            EvidenceFailureCode::EvaluationFailed,
                            format!("observation pattern evaluation failed: {error}"),
                        )
                    })?;
                if !matches {
                    return Err(failure(
                        EvidenceFailureCode::ExpectationMismatch,
                        format!(
                            "observation pattern did not match `{:?}`",
                            instance.observation
                        ),
                    ));
                }
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = false;
            }
            EvidenceStep::ExpectInspectionPattern { pattern, .. } => {
                let inspection = inspection_value(&execution.context)
                    .map_err(|message| failure(EvidenceFailureCode::InvalidLifecycle, message))?;
                let mut bindings = BTreeMap::new();
                let matches =
                    match_pattern(pattern, &inspection, &mut bindings).map_err(|error| {
                        failure(
                            EvidenceFailureCode::EvaluationFailed,
                            format!("inspection pattern evaluation failed: {error}"),
                        )
                    })?;
                if !matches {
                    return Err(failure(
                        EvidenceFailureCode::ExpectationMismatch,
                        format!("inspection pattern did not match `{inspection:?}`"),
                    ));
                }
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = false;
            }
            EvidenceStep::ExpectObservationWhere { condition, .. } => {
                let instance = current_instance(&execution.context)
                    .map_err(|message| failure(EvidenceFailureCode::InvalidLifecycle, message))?;
                let locals = record_map(match &instance.observation {
                    Value::Record(fields) => fields,
                    _ => {
                        return Err(failure(
                            EvidenceFailureCode::MalformedEvidence,
                            "public observation must be a record for `expect observation where`"
                                .into(),
                        ));
                    }
                })
                .map_err(|error| {
                    failure(EvidenceFailureCode::EvaluationFailed, error.to_string())
                })?;
                let state = instance_state(instance)
                    .map_err(|message| failure(EvidenceFailureCode::EvaluationFailed, message))?;
                let machine = self
                    .machine(&execution.context.machine)
                    .map_err(|message| failure(EvidenceFailureCode::UnknownMachine, message))?;
                let (matches, _) = evaluate_condition_with_locals(
                    &self.program.machine_program,
                    machine,
                    &instance.configuration,
                    &state,
                    locals,
                    condition,
                )
                .map_err(|error| {
                    failure(
                        EvidenceFailureCode::EvaluationFailed,
                        format!("observation predicate failed to evaluate: {error}"),
                    )
                })?;
                if !matches {
                    return Err(failure(
                        EvidenceFailureCode::ExpectationMismatch,
                        "observation predicate evaluated to false".into(),
                    ));
                }
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = false;
            }
            EvidenceStep::ExpectRestore { commands, .. } => {
                if !execution.context.restored || execution.context.executable_steps != 0 {
                    return Err(failure(
                        EvidenceFailureCode::InvalidLifecycle,
                        "restore expectation must be the first executable step of a restored scenario"
                            .into(),
                    ));
                }
                if !commands.is_empty() {
                    return Err(failure(
                        EvidenceFailureCode::MalformedEvidence,
                        "restore is inert; the only valid expected command list is `[]`".into(),
                    ));
                }
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = false;
            }
            EvidenceStep::ExpectSnapshot { reference, .. } => {
                let actual = snapshot_from_context(&execution.context)
                    .map_err(|message| failure(EvidenceFailureCode::InvalidLifecycle, message))?;
                let expected = if reference.scenario == scenario.id {
                    execution.pins.get(&reference.pin).ok_or_else(|| {
                        failure(
                            EvidenceFailureCode::MissingSnapshot,
                            format!(
                                "unknown local snapshot `{}::{}`",
                                reference.scenario, reference.pin
                            ),
                        )
                    })?
                } else {
                    &self
                        .resolve_reference(reference, published_pins)
                        .map_err(|message| failure(EvidenceFailureCode::MissingSnapshot, message))?
                        .snapshot
                };
                if &actual != expected {
                    return Err(failure(
                        EvidenceFailureCode::SnapshotMismatch,
                        format!(
                            "snapshot differs from `{}::{}`",
                            reference.scenario, reference.pin
                        ),
                    ));
                }
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = false;
            }
            EvidenceStep::Pin { name, .. } => {
                if execution.pins.contains_key(name) {
                    return Err(failure(
                        EvidenceFailureCode::MalformedEvidence,
                        format!("pin `{name}` is declared more than once in this scenario"),
                    ));
                }
                let snapshot = snapshot_from_context(&execution.context)
                    .map_err(|message| failure(EvidenceFailureCode::InvalidLifecycle, message))?;
                execution.pins.insert(name.clone(), snapshot);
                execution.pin_sources.insert(name.clone(), source.clone());
                execution.context.executable_steps += 1;
                execution.context.last_step_was_reaction = false;
            }
        }
        Ok(())
    }

    fn execute_reaction(&self, context: &mut ScenarioContext, input: Value) -> Result<(), String> {
        let mut admitted = current_instance(context)?.clone();
        self.program
            .machine_program
            .enqueue(&mut admitted, input)
            .map_err(|error| error.to_string())?;
        let step = self
            .program
            .machine_program
            .drain_one(&admitted)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "submitted input did not produce a reaction".to_string())?;
        self.record_fixture_commands(context, &step.receipt.ordered_commands)?;
        let fault = match &step.receipt.resolution {
            ReactionResolution::Fault { fault } => Some(format!("{fault:?}")),
            ReactionResolution::Completed { .. } => None,
        };
        context.receipts.push(step.receipt);
        context.instance = Some(step.instance);
        if let Some(fault) = fault {
            return Err(format!("reaction entered the faulted lifecycle: {fault}"));
        }
        Ok(())
    }

    fn record_fixture_commands(
        &self,
        context: &mut ScenarioContext,
        commands: &[Value],
    ) -> Result<(), String> {
        let machine = self.machine(&context.machine)?;
        for command in commands {
            let Some((port, constructor)) = qualified_port_value(machine, command) else {
                continue;
            };
            let port_definition = machine
                .ports
                .iter()
                .find(|definition| definition.name == port)
                .expect("qualified_port_value returned a declared port");
            if !constructor_declared(&port_definition.send, &port, &constructor) {
                return Err(format!(
                    "`{constructor}` is not a send constructor of port `{port}`"
                ));
            }
            let fixture = context
                .fixtures
                .get_mut(&port)
                .ok_or_else(|| format!("command targets unbound port `{port}`"))?;
            fixture.commands.push(command.clone());
        }
        Ok(())
    }

    fn evaluate_fixture(
        &self,
        machine: &Machine,
        port: &PortDef,
        expression: &Expr,
    ) -> Result<FixtureBinding, String> {
        let Expr::Call { function, args, .. } = expression else {
            return Err("fixture binding must call `Contract.fixture(...)`".into());
        };
        let Some(contract_name) = function.strip_suffix(".fixture") else {
            return Err(format!(
                "fixture binding calls `{function}` instead of `Contract.fixture(...)`"
            ));
        };
        let expected_contract_name = short_contract_name(&port.contract);
        if short_contract_name(contract_name) != expected_contract_name {
            return Err(format!(
                "fixture contract `{contract_name}` is incompatible with port `{}` contract `{}`",
                port.name, port.contract
            ));
        }
        let expected_contract_hash = standard_fixture_contract_hash(expected_contract_name)
            .ok_or_else(|| {
                format!(
                    "no sealed fixture implementation is registered for contract `{}`",
                    port.contract
                )
            })?;
        if port.contract_hash != expected_contract_hash {
            return Err(format!(
                "fixture contract hash for port `{}` is incompatible; expected `{expected_contract_hash}`, got `{}`",
                port.name, port.contract_hash
            ));
        }
        if args.len() > 1 {
            return Err(format!(
                "fixture `{function}` accepts at most one configuration value"
            ));
        }
        let empty_state = BTreeMap::new();
        let evaluate = |value: &Expr| {
            evaluate_with_locals(
                &self.program.machine_program,
                machine,
                &Value::Unit,
                &empty_state,
                BTreeMap::new(),
                value,
            )
            .map_err(|error| error.to_string())
        };
        let expected_configuration = port
            .configuration
            .as_ref()
            .map(evaluate)
            .transpose()?
            .unwrap_or(Value::Unit);
        let configuration = args
            .first()
            .map(evaluate)
            .transpose()?
            .unwrap_or(Value::Unit);
        if configuration != expected_configuration {
            return Err(format!(
                "fixture configuration for port `{}` differs from the declared port configuration",
                port.name
            ));
        }
        Ok(FixtureBinding {
            port: port.name.clone(),
            contract: port.contract.clone(),
            contract_hash: port.contract_hash.clone(),
            configuration,
            commands: Vec::new(),
        })
    }

    fn verify_complete_bindings(
        &self,
        machine: &Machine,
        fixtures: &BTreeMap<String, FixtureBinding>,
    ) -> Result<(), String> {
        let mut declared = BTreeSet::new();
        for port in &machine.ports {
            if !declared.insert(port.name.as_str()) {
                return Err(format!("machine repeats port `{}`", port.name));
            }
        }
        let bound = fixtures.keys().map(String::as_str).collect::<BTreeSet<_>>();
        if declared == bound {
            return Ok(());
        }
        let missing = declared
            .difference(&bound)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let extra = bound
            .difference(&declared)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "fixture bindings are incomplete; missing [{missing}], extra [{extra}]"
        ))
    }

    fn evaluate_current(
        &self,
        context: &ScenarioContext,
        expression: &Expr,
        locals: BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        let instance = current_instance(context)?;
        let state = instance_state(instance)?;
        let machine = self.machine(&context.machine)?;
        evaluate_with_locals(
            &self.program.machine_program,
            machine,
            &instance.configuration,
            &state,
            locals,
            expression,
        )
        .map_err(|error| error.to_string())
    }

    fn qualified_input_port(&self, machine_id: &str, value: &Value) -> Option<(String, String)> {
        let machine = self.program.machine_program.machines.get(machine_id)?;
        qualified_port_value(machine, value)
    }

    fn machine(&self, id: &str) -> Result<&Machine, String> {
        self.program
            .machine_program
            .machines
            .get(id)
            .ok_or_else(|| format!("unknown machine `{id}`"))
    }

    fn restore_snapshot(&self, snapshot: &EvidenceSnapshot) -> Result<Instance, String> {
        let expected_hash = self.machine_program_hash(&snapshot.machine)?;
        if expected_hash != snapshot.machine_program_hash {
            return Err(format!(
                "snapshot machine program hash `{}` is incompatible with `{expected_hash}`",
                snapshot.machine_program_hash
            ));
        }
        let checkpoint = Checkpoint {
            protocol: CHECKPOINT_PROTOCOL.into(),
            instance: snapshot.instance.clone(),
            machine: snapshot.machine.clone(),
            machine_program_hash: snapshot.machine_program_hash.clone(),
            configuration: snapshot.configuration.clone(),
            state: snapshot.state.clone(),
            inbox: snapshot.inbox.clone(),
            lifecycle: snapshot.lifecycle,
            next_sequence: snapshot.next_sequence,
            trace_prefix_hash: snapshot.trace_prefix_hash.clone(),
        };
        self.program
            .machine_program
            .restore(&checkpoint)
            .map_err(|error| error.to_string())
    }

    fn machine_program_hash(&self, machine: &str) -> Result<String, String> {
        if !self.program.machine_program.machines.contains_key(machine) {
            return Err(format!("unknown machine `{machine}`"));
        }
        self.program
            .machine_program
            .program_hashes
            .get(machine)
            .cloned()
            .ok_or_else(|| format!("machine `{machine}` has no frozen machine-program hash"))
    }

    fn resolve_reference<'pins>(
        &self,
        reference: &EvidenceRef,
        pins: &'pins BTreeMap<PinKey, EvidencePinArtifact>,
    ) -> Result<&'pins EvidencePinArtifact, String> {
        let mut current = reference;
        let mut visited = BTreeSet::new();
        loop {
            if !current.scenario.is_empty()
                && !current.pin.is_empty()
                && let Some(pin) = pins.get(&PinKey::new(&current.scenario, &current.pin))
            {
                return Ok(pin);
            }
            let alias = if current.pin.is_empty() {
                self.program.evidence.checkpoints.get(&current.scenario)
            } else if current.scenario.is_empty() {
                self.program.evidence.checkpoints.get(&current.pin)
            } else {
                None
            };
            let Some(alias) = alias else {
                return Err(format!(
                    "unknown evidence snapshot `{}::{}`",
                    current.scenario, current.pin
                ));
            };
            let marker = format!("{}::{}", current.scenario, current.pin);
            if !visited.insert(marker) {
                return Err("checkpoint reference cycle".into());
            }
            current = alias;
        }
    }

    fn resolve_examples(
        &self,
        pins: &BTreeMap<PinKey, EvidencePinArtifact>,
        report: &mut EvidenceReport,
    ) {
        for (name, reference) in &self.program.evidence.examples {
            let source = self
                .program
                .evidence
                .example_sources
                .get(name)
                .cloned()
                .unwrap_or_else(|| SourceRef::synthetic(format!("evidence:example:{name}")));
            match self.resolve_reference(reference, pins) {
                Ok(pin) => {
                    report.artifacts.examples.insert(
                        name.clone(),
                        EvidenceExampleArtifact {
                            name: name.clone(),
                            reference: reference.clone(),
                            source,
                            metadata: self
                                .program
                                .evidence
                                .example_metadata
                                .get(name)
                                .cloned()
                                .unwrap_or_default(),
                            observation: pin.snapshot.observation.clone(),
                            snapshot: pin.snapshot.clone(),
                        },
                    );
                }
                Err(message) => report.failures.push(EvidenceFailure {
                    code: EvidenceFailureCode::MissingSnapshot,
                    scenario: None,
                    step_index: None,
                    source_id: source_id(&source),
                    source,
                    message,
                }),
            }
        }
    }

    fn resolve_checkpoints(
        &self,
        pins: &BTreeMap<PinKey, EvidencePinArtifact>,
        report: &mut EvidenceReport,
    ) {
        for (name, reference) in &self.program.evidence.checkpoints {
            let source = self
                .program
                .evidence
                .checkpoint_sources
                .get(name)
                .cloned()
                .unwrap_or_else(|| SourceRef::synthetic(format!("evidence:checkpoint:{name}")));
            match self.resolve_reference(reference, pins) {
                Ok(pin) => {
                    let snapshot = &pin.snapshot;
                    report.artifacts.checkpoints.insert(
                        name.clone(),
                        EvidenceCheckpointArtifact {
                            name: name.clone(),
                            reference: reference.clone(),
                            source,
                            checkpoint: Checkpoint {
                                protocol: CHECKPOINT_PROTOCOL.into(),
                                instance: snapshot.instance.clone(),
                                machine: snapshot.machine.clone(),
                                machine_program_hash: snapshot.machine_program_hash.clone(),
                                configuration: snapshot.configuration.clone(),
                                state: snapshot.state.clone(),
                                inbox: snapshot.inbox.clone(),
                                lifecycle: snapshot.lifecycle,
                                next_sequence: snapshot.next_sequence,
                                trace_prefix_hash: snapshot.trace_prefix_hash.clone(),
                            },
                            fixtures: snapshot.fixtures.clone(),
                        },
                    );
                }
                Err(message) => report.failures.push(EvidenceFailure {
                    code: EvidenceFailureCode::MissingSnapshot,
                    scenario: None,
                    step_index: None,
                    source_id: source_id(&source),
                    source,
                    message,
                }),
            }
        }
    }
}

impl Program {
    /// Convenience entry point for the deterministic evidence runner.
    pub fn run_evidence(&self) -> EvidenceReport {
        EvidenceRunner::new(self).run()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceReport {
    pub protocol: String,
    pub passed: bool,
    pub scenarios: Vec<ScenarioReport>,
    pub artifacts: EvidenceArtifacts,
    pub failures: Vec<EvidenceFailure>,
}

impl EvidenceReport {
    pub fn to_canonical_string(&self) -> String {
        uhura_base::to_canonical_json(
            &serde_json::to_value(self).expect("Uhura evidence report is serializable"),
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceArtifacts {
    pub pins: BTreeMap<String, EvidencePinArtifact>,
    pub examples: BTreeMap<String, EvidenceExampleArtifact>,
    pub checkpoints: BTreeMap<String, EvidenceCheckpointArtifact>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidencePinArtifact {
    pub scenario: String,
    pub pin: String,
    pub source_id: String,
    pub source: SourceRef,
    pub snapshot: EvidenceSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceExampleArtifact {
    pub name: String,
    pub reference: EvidenceRef,
    pub source: SourceRef,
    #[serde(default)]
    pub metadata: super::ir::EvidenceExampleMetadata,
    pub observation: Value,
    pub snapshot: EvidenceSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceCheckpointArtifact {
    pub name: String,
    pub reference: EvidenceRef,
    pub source: SourceRef,
    pub checkpoint: Checkpoint,
    pub fixtures: BTreeMap<String, FixtureSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceSnapshot {
    pub machine: String,
    pub machine_program_hash: String,
    pub instance: String,
    pub configuration: Value,
    pub state: Value,
    pub observation: Value,
    pub inbox: Vec<Value>,
    pub lifecycle: InstanceLifecycle,
    pub next_sequence: u64,
    pub fixtures: BTreeMap<String, FixtureSnapshot>,
    pub trace_prefix_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FixtureSnapshot {
    pub port: String,
    pub contract: String,
    pub contract_hash: String,
    pub configuration: Value,
    pub commands: Vec<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScenarioReport {
    pub scenario: String,
    pub machine: Option<String>,
    pub status: ScenarioStatus,
    pub total_steps: usize,
    pub executed_steps: usize,
    pub genesis: Option<GenesisReceipt>,
    pub receipts: Vec<ReactionReceipt>,
    pub final_snapshot: Option<EvidenceSnapshot>,
    pub published_pins: Vec<String>,
    pub failure: Option<EvidenceFailure>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScenarioStatus {
    Passed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceFailure {
    pub code: EvidenceFailureCode,
    pub scenario: Option<String>,
    pub step_index: Option<usize>,
    pub source_id: String,
    pub source: SourceRef,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceFailureCode {
    MalformedEvidence,
    UnknownMachine,
    InvalidFixture,
    IncompleteBindings,
    InvalidLifecycle,
    InvalidInputKind,
    AdmissionFailed,
    RestoreFailed,
    ReactionFailed,
    EvaluationFailed,
    ExpectationMismatch,
    MissingSnapshot,
    SnapshotMismatch,
}

struct Failure {
    public: Box<EvidenceFailure>,
}

impl Failure {
    fn at(
        code: EvidenceFailureCode,
        scenario: &str,
        step_index: Option<usize>,
        source: &SourceRef,
        message: impl Into<String>,
    ) -> Self {
        Self {
            public: Box::new(EvidenceFailure {
                code,
                scenario: Some(scenario.into()),
                step_index,
                source_id: source_id(source),
                source: source.clone(),
                message: message.into(),
            }),
        }
    }
}

struct ScenarioExecution {
    context: ScenarioContext,
    pins: BTreeMap<String, EvidenceSnapshot>,
    pin_sources: BTreeMap<String, SourceRef>,
    executed_steps: usize,
    failure: Option<Failure>,
}

impl ScenarioExecution {
    fn new(context: ScenarioContext) -> Self {
        Self {
            context,
            pins: BTreeMap::new(),
            pin_sources: BTreeMap::new(),
            executed_steps: 0,
            failure: None,
        }
    }

    fn failed_without_context(scenario: &Scenario, failure: Failure) -> Self {
        let machine = match &scenario.origin {
            ScenarioOrigin::Machine { machine, .. } => machine.clone(),
            ScenarioOrigin::Snapshot { .. } => String::new(),
        };
        Self {
            context: ScenarioContext {
                machine,
                configuration: Value::Unit,
                instance: None,
                genesis: None,
                fixtures: BTreeMap::new(),
                receipts: Vec::new(),
                restored: matches!(&scenario.origin, ScenarioOrigin::Snapshot { .. }),
                executable_steps: 0,
                last_step_was_reaction: false,
            },
            pins: BTreeMap::new(),
            pin_sources: BTreeMap::new(),
            executed_steps: 0,
            failure: Some(failure),
        }
    }

    fn into_report(self, scenario: &Scenario) -> ScenarioReport {
        let final_snapshot = snapshot_from_context(&self.context).ok();
        let failure = self.failure.map(|failure| *failure.public);
        ScenarioReport {
            scenario: scenario.id.clone(),
            machine: (!self.context.machine.is_empty()).then_some(self.context.machine),
            status: if failure.is_some() {
                ScenarioStatus::Failed
            } else {
                ScenarioStatus::Passed
            },
            total_steps: scenario.steps.len(),
            executed_steps: self.executed_steps,
            genesis: self.context.genesis,
            receipts: self.context.receipts,
            final_snapshot,
            published_pins: if failure.is_none() {
                self.pins.keys().cloned().collect()
            } else {
                Vec::new()
            },
            failure,
        }
    }
}

struct ScenarioContext {
    machine: String,
    configuration: Value,
    instance: Option<Instance>,
    genesis: Option<GenesisReceipt>,
    fixtures: BTreeMap<String, FixtureBinding>,
    receipts: Vec<ReactionReceipt>,
    restored: bool,
    executable_steps: usize,
    last_step_was_reaction: bool,
}

#[derive(Clone)]
struct FixtureBinding {
    port: String,
    contract: String,
    contract_hash: String,
    configuration: Value,
    commands: Vec<Value>,
}

impl From<FixtureSnapshot> for FixtureBinding {
    fn from(snapshot: FixtureSnapshot) -> Self {
        Self {
            port: snapshot.port,
            contract: snapshot.contract,
            contract_hash: snapshot.contract_hash,
            configuration: snapshot.configuration,
            commands: snapshot.commands,
        }
    }
}

impl From<&FixtureBinding> for FixtureSnapshot {
    fn from(binding: &FixtureBinding) -> Self {
        Self {
            port: binding.port.clone(),
            contract: binding.contract.clone(),
            contract_hash: binding.contract_hash.clone(),
            configuration: binding.configuration.clone(),
            commands: binding.commands.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PinKey {
    scenario: String,
    pin: String,
}

impl PinKey {
    fn new(scenario: &str, pin: &str) -> Self {
        Self {
            scenario: scenario.into(),
            pin: pin.into(),
        }
    }
}

fn snapshot_from_context(context: &ScenarioContext) -> Result<EvidenceSnapshot, String> {
    let instance = current_instance(context)?;
    Ok(EvidenceSnapshot {
        machine: instance.machine.clone(),
        machine_program_hash: instance.program_hash.clone(),
        instance: instance.id.clone(),
        configuration: instance.configuration.clone(),
        state: instance.state.clone(),
        observation: instance.observation.clone(),
        inbox: instance.inbox.iter().cloned().collect(),
        lifecycle: instance.lifecycle,
        next_sequence: instance.next_sequence,
        fixtures: context
            .fixtures
            .iter()
            .map(|(name, binding)| (name.clone(), FixtureSnapshot::from(binding)))
            .collect(),
        trace_prefix_hash: instance.trace_prefix_hash.clone(),
    })
}

fn inspection_value(context: &ScenarioContext) -> Result<Value, String> {
    let instance = current_instance(context)?;
    let Value::Record(state_fields) = &instance.state else {
        return Err("instance state is not a record".into());
    };
    const RESERVED: [&str; 5] = [
        "state",
        "observation",
        "inbox",
        "lifecycle",
        "next_sequence",
    ];
    if let Some((name, _)) = state_fields
        .iter()
        .find(|(name, _)| RESERVED.contains(&name.as_str()))
    {
        return Err(format!(
            "state field `{name}` collides with a reserved inspection field"
        ));
    }
    let mut fields = state_fields.clone();
    fields.extend([
        ("state".into(), instance.state.clone()),
        ("observation".into(), instance.observation.clone()),
        (
            "inbox".into(),
            Value::Seq(instance.inbox.iter().cloned().collect()),
        ),
        (
            "lifecycle".into(),
            Value::Text(
                match instance.lifecycle {
                    InstanceLifecycle::Running => "running",
                    InstanceLifecycle::Faulted => "faulted",
                }
                .into(),
            ),
        ),
        (
            "next_sequence".into(),
            Value::nat(instance.next_sequence).expect("a u64 sequence always belongs to Uhura Nat"),
        ),
    ]);
    Ok(Value::Record(fields))
}

fn current_instance(context: &ScenarioContext) -> Result<&Instance, String> {
    context
        .instance
        .as_ref()
        .ok_or_else(|| "scenario has not started".into())
}

fn instance_state(instance: &Instance) -> Result<BTreeMap<String, Value>, String> {
    match &instance.state {
        Value::Record(fields) => record_map(fields).map_err(|error| error.to_string()),
        _ => Err("instance state is not a record".into()),
    }
}

fn qualified_port_value(machine: &Machine, value: &Value) -> Option<(String, String)> {
    let Value::Variant { constructor, .. } = value else {
        return None;
    };
    machine.ports.iter().find_map(|port| {
        constructor
            .strip_prefix(&format!("{}.", port.name))
            .filter(|suffix| !suffix.is_empty() && !suffix.contains('.'))
            .map(|suffix| (port.name.clone(), suffix.to_string()))
    })
}

fn constructor_declared(
    constructors: &[super::ir::ConstructorDef],
    port: &str,
    name: &str,
) -> bool {
    constructors
        .iter()
        .any(|constructor| constructor.name == name || constructor.name == format!("{port}.{name}"))
}

fn short_contract_name(contract: &str) -> &str {
    let name = contract
        .rsplit_once("::")
        .map(|(_, name)| name)
        .unwrap_or(contract);
    name.rsplit_once('.').map(|(_, name)| name).unwrap_or(name)
}

fn standard_fixture_contract_hash(contract: &str) -> Option<&'static str> {
    match contract {
        "Observation" => Some(OBSERVATION_CONTRACT_HASH),
        "RequestPort" => Some(REQUEST_PORT_CONTRACT_HASH),
        "SinkPort" => Some(SINK_PORT_CONTRACT_HASH),
        "Router" => Some(ROUTER_CONTRACT_HASH),
        _ => None,
    }
}

fn source_order(source: &SourceRef) -> (&str, u32, u32, &str) {
    (&source.path, source.start, source.end, &source.id)
}

fn source_id(source: &SourceRef) -> String {
    if source.id.is_empty() {
        format!("{}:{}:{}", source.path, source.start, source.end)
    } else {
        source.id.clone()
    }
}

fn step_source(step: &EvidenceStep) -> &SourceRef {
    match step {
        EvidenceStep::Bind { source, .. }
        | EvidenceStep::Start { source }
        | EvidenceStep::Send { source, .. }
        | EvidenceStep::Deliver { source, .. }
        | EvidenceStep::ExpectReaction { source, .. }
        | EvidenceStep::ExpectObservationPattern { source, .. }
        | EvidenceStep::ExpectInspectionPattern { source, .. }
        | EvidenceStep::ExpectObservationWhere { source, .. }
        | EvidenceStep::ExpectRestore { source, .. }
        | EvidenceStep::ExpectSnapshot { source, .. }
        | EvidenceStep::Pin { source, .. } => source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        BinaryOp, CommandDef, ConstructorDef, Handler, ObservationField, OutcomeDef, OutcomePolicy,
        Pattern, StateField, Statement, TypeDef, TypeRef,
    };

    fn source(id: &str, start: u32) -> SourceRef {
        SourceRef {
            id: id.into(),
            path: "evidence.uhura".into(),
            start,
            end: start + 1,
        }
    }

    fn accepted() -> Value {
        Value::variant("example.counter@1::Counter.Outcome", "accepted", Vec::new())
    }

    fn increment() -> Value {
        Value::variant("example.counter@1::Counter.Input", "increment", Vec::new())
    }

    fn observed() -> Value {
        Value::variant(
            "example.counter@1::Counter::port.events.Receive",
            "events.observed",
            vec![(Some("value".into()), Value::int(1))],
        )
    }

    fn audit_command() -> Value {
        Value::variant(
            "example.counter@1::Counter::port.audit.Send",
            "audit.send",
            vec![(Some("value".into()), Value::int(1))],
        )
    }

    fn counter_program() -> Program {
        let machine_id = "example.counter@1::Counter".to_string();
        let mut program = Program::new();
        let input_type = TypeDef::Sum {
            id: "example.counter@1::Counter.Input".into(),
            constructors: vec![ConstructorDef {
                name: "increment".into(),
                fields: Vec::new(),
            }],
        };
        let handler_source = source("handler.increment", 1);
        let handler = Handler {
            input: "increment".into(),
            pattern: Pattern::Literal { value: increment() },
            body: vec![
                Statement::Set {
                    field: "count".into(),
                    value: Expr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Name {
                            name: "count".into(),
                        }),
                        right: Box::new(Expr::Literal {
                            value: Value::int(1),
                        }),
                    },
                    source: handler_source.clone(),
                },
                Statement::Emit {
                    value: Expr::Literal {
                        value: audit_command(),
                    },
                    source: handler_source.clone(),
                },
                Statement::Finish {
                    outcome: Expr::Literal { value: accepted() },
                    source: handler_source.clone(),
                },
            ],
            source: handler_source,
        };
        let observed_source = source("handler.events-observed", 1);
        let observed_handler = Handler {
            input: "events.observed".into(),
            pattern: Pattern::Literal { value: observed() },
            body: vec![
                Statement::Set {
                    field: "count".into(),
                    value: Expr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Name {
                            name: "count".into(),
                        }),
                        right: Box::new(Expr::Literal {
                            value: Value::int(1),
                        }),
                    },
                    source: observed_source.clone(),
                },
                Statement::Finish {
                    outcome: Expr::Literal { value: accepted() },
                    source: observed_source.clone(),
                },
            ],
            source: observed_source,
        };
        let machine = Machine {
            id: machine_id.clone(),
            config: TypeRef::Unit,
            requires: Vec::new(),
            ports: vec![
                PortDef {
                    name: "audit".into(),
                    contract: "uhura.ports@1::SinkPort".into(),
                    contract_instance: Some(
                        uhura_port::sink_port_instance(uhura_port::TypeRef::new("Int").unwrap())
                            .unwrap(),
                    ),
                    type_arguments: vec![TypeRef::Int],
                    configuration: None,
                    receive: Vec::new(),
                    send: vec![ConstructorDef {
                        name: "send".into(),
                        fields: vec![(Some("value".into()), TypeRef::Int)],
                    }],
                    contract_hash: SINK_PORT_CONTRACT_HASH.into(),
                    source: source("port.audit", 1),
                },
                PortDef {
                    name: "events".into(),
                    contract: "uhura.observation@1::Observation".into(),
                    contract_instance: Some(
                        uhura_port::observation_instance(uhura_port::TypeRef::new("Int").unwrap())
                            .unwrap(),
                    ),
                    type_arguments: vec![TypeRef::Int],
                    configuration: None,
                    receive: vec![ConstructorDef {
                        name: "observed".into(),
                        fields: vec![(Some("value".into()), TypeRef::Int)],
                    }],
                    send: Vec::new(),
                    contract_hash: OBSERVATION_CONTRACT_HASH.into(),
                    source: source("port.events", 1),
                },
            ],
            local_input: input_type,
            local_commands: vec![CommandDef {
                constructor: ConstructorDef {
                    name: "audit.send".into(),
                    fields: vec![(Some("value".into()), TypeRef::Int)],
                },
                source: source("command.audit", 1),
            }],
            outcomes: vec![OutcomeDef {
                constructor: ConstructorDef {
                    name: "accepted".into(),
                    fields: Vec::new(),
                },
                policy: OutcomePolicy::Commit,
                source: source("outcome.accepted", 1),
            }],
            state: vec![StateField {
                name: "count".into(),
                ty: TypeRef::Int,
                initial: Expr::Literal {
                    value: Value::int(0),
                },
                source: source("state.count", 1),
            }],
            functions: BTreeMap::new(),
            derives: Vec::new(),
            invariants: Vec::new(),
            observation: vec![ObservationField {
                name: "count".into(),
                ty: TypeRef::Int,
                expression: Expr::Name {
                    name: "count".into(),
                },
                source: source("observe.count", 1),
            }],
            transitions: BTreeMap::new(),
            handlers: BTreeMap::from([
                ("increment".into(), handler),
                ("events.observed".into(), observed_handler),
            ]),
            before_commit: Vec::new(),
            source: source("machine.counter", 1),
        };
        program
            .machine_program
            .machines
            .insert(machine_id.clone(), machine);
        program.freeze_program_hashes();

        let audit_fixture = EvidenceStep::Bind {
            port: "audit".into(),
            fixture: Expr::Call {
                function: "SinkPort.fixture".into(),
                args: Vec::new(),
                result_type: TypeRef::Never,
            },
            source: source("canonical.bind", 10),
        };
        let events_fixture = EvidenceStep::Bind {
            port: "events".into(),
            fixture: Expr::Call {
                function: "Observation.fixture".into(),
                args: Vec::new(),
                result_type: TypeRef::Never,
            },
            source: source("canonical.bind-events", 10),
        };
        let start = EvidenceStep::Start {
            source: source("canonical.start", 11),
        };
        let send = |id: &str, offset| EvidenceStep::Send {
            input: Expr::Literal { value: increment() },
            source: source(id, offset),
        };
        let expect = |id: &str, offset| EvidenceStep::ExpectReaction {
            outcome: Pattern::Literal { value: accepted() },
            commands: vec![Expr::Literal {
                value: audit_command(),
            }],
            source: source(id, offset),
        };
        program.evidence.scenarios.insert(
            "canonical".into(),
            Scenario {
                id: "canonical".into(),
                origin: ScenarioOrigin::Machine {
                    machine: machine_id,
                    configuration: Value::Unit,
                },
                steps: vec![
                    audit_fixture,
                    events_fixture,
                    start,
                    send("canonical.send", 12),
                    expect("canonical.expect", 13),
                    EvidenceStep::Deliver {
                        input: Expr::Literal { value: observed() },
                        source: source("canonical.deliver", 14),
                    },
                    EvidenceStep::ExpectReaction {
                        outcome: Pattern::Literal { value: accepted() },
                        commands: Vec::new(),
                        source: source("canonical.expect-delivery", 15),
                    },
                    EvidenceStep::ExpectObservationPattern {
                        pattern: Pattern::Record {
                            fields: vec![(
                                "count".into(),
                                Pattern::Literal {
                                    value: Value::int(2),
                                },
                            )],
                            rest: false,
                        },
                        source: source("canonical.observe", 16),
                    },
                    EvidenceStep::ExpectInspectionPattern {
                        pattern: Pattern::Record {
                            fields: vec![(
                                "count".into(),
                                Pattern::Literal {
                                    value: Value::int(2),
                                },
                            )],
                            rest: true,
                        },
                        source: source("canonical.inspect", 17),
                    },
                    EvidenceStep::ExpectObservationWhere {
                        condition: Expr::Binary {
                            op: BinaryOp::Equal,
                            left: Box::new(Expr::Name {
                                name: "count".into(),
                            }),
                            right: Box::new(Expr::Literal {
                                value: Value::int(2),
                            }),
                        },
                        source: source("canonical.where", 18),
                    },
                    EvidenceStep::Pin {
                        name: "after_one".into(),
                        source: source("canonical.pin", 19),
                    },
                ],
                source: source("scenario.canonical", 10),
            },
        );
        program.evidence.scenarios.insert(
            "replay".into(),
            Scenario {
                id: "replay".into(),
                origin: ScenarioOrigin::Snapshot {
                    reference: EvidenceRef {
                        scenario: "canonical".into(),
                        pin: "after_one".into(),
                    },
                },
                steps: vec![
                    EvidenceStep::ExpectRestore {
                        commands: Vec::new(),
                        source: source("replay.restore", 21),
                    },
                    send("replay.send", 22),
                    expect("replay.expect", 23),
                    EvidenceStep::Pin {
                        name: "final".into(),
                        source: source("replay.pin", 24),
                    },
                ],
                source: source("scenario.replay", 20),
            },
        );
        program.evidence.scenarios.insert(
            "replay_again".into(),
            Scenario {
                id: "replay_again".into(),
                origin: ScenarioOrigin::Snapshot {
                    reference: EvidenceRef {
                        scenario: "canonical".into(),
                        pin: "after_one".into(),
                    },
                },
                steps: vec![
                    EvidenceStep::ExpectRestore {
                        commands: Vec::new(),
                        source: source("again.restore", 31),
                    },
                    send("again.send", 32),
                    expect("again.expect", 33),
                    EvidenceStep::ExpectSnapshot {
                        reference: EvidenceRef {
                            scenario: "replay".into(),
                            pin: "final".into(),
                        },
                        source: source("again.snapshot", 34),
                    },
                ],
                source: source("scenario.replay-again", 30),
            },
        );
        program.evidence.examples.insert(
            "one".into(),
            EvidenceRef {
                scenario: "canonical".into(),
                pin: "after_one".into(),
            },
        );
        program
            .evidence
            .example_sources
            .insert("one".into(), source("example.one", 40));
        program.evidence.checkpoints.insert(
            "one".into(),
            EvidenceRef {
                scenario: "canonical".into(),
                pin: "after_one".into(),
            },
        );
        program
            .evidence
            .checkpoint_sources
            .insert("one".into(), source("checkpoint.one", 41));
        program
    }

    #[test]
    fn runs_fixtures_replay_and_exact_snapshot_comparison() {
        let report = EvidenceRunner::new(&counter_program()).run();
        assert!(report.passed, "{:#?}", report.failures);
        assert_eq!(report.scenarios.len(), 3);
        assert_eq!(
            report.artifacts.examples["one"].observation,
            Value::Record(vec![("count".into(), Value::int(2))])
        );
        assert_eq!(
            report.artifacts.pins["replay::final"].snapshot.fixtures["audit"].commands,
            vec![audit_command(), audit_command()]
        );
        assert_eq!(
            report.artifacts.pins["replay::final"].source,
            source("replay.pin", 24)
        );
        assert_eq!(
            report.artifacts.examples["one"].source,
            source("example.one", 40)
        );
        assert_eq!(
            report.artifacts.checkpoints["one"].checkpoint.next_sequence,
            3
        );
        assert_eq!(
            report.artifacts.checkpoints["one"].source,
            source("checkpoint.one", 41)
        );
        let canonical = report.to_canonical_string();
        assert_eq!(
            serde_json::from_str::<EvidenceReport>(&canonical).unwrap(),
            report
        );
    }

    #[test]
    fn fresh_scenario_configuration_is_admitted_and_retained_by_snapshot_replay() {
        let mut program = counter_program();
        let configuration = Value::record([("seed".into(), Value::int(7))]).unwrap();
        program
            .machine_program
            .machines
            .get_mut("example.counter@1::Counter")
            .expect("counter machine")
            .config = TypeRef::Record {
            fields: vec![("seed".into(), TypeRef::Int)],
        };
        let ScenarioOrigin::Machine {
            configuration: scenario_configuration,
            ..
        } = &mut program
            .evidence
            .scenarios
            .get_mut("canonical")
            .expect("canonical scenario")
            .origin
        else {
            panic!("canonical is a fresh scenario");
        };
        *scenario_configuration = configuration.clone();
        program.freeze_program_hashes();

        let first = EvidenceRunner::new(&program).run();
        let second = EvidenceRunner::new(&program).run();
        assert_eq!(first, second);
        assert!(first.passed, "{:#?}", first.failures);
        assert_eq!(
            first.artifacts.pins["canonical::after_one"]
                .snapshot
                .configuration,
            configuration
        );
        assert_eq!(
            first.artifacts.pins["replay::final"].snapshot.configuration,
            configuration
        );
    }

    #[test]
    fn missing_fixture_fails_at_start_with_stable_source_id() {
        let mut program = counter_program();
        let scenario = program.evidence.scenarios.get_mut("canonical").unwrap();
        scenario.steps.remove(0);
        let report = EvidenceRunner::new(&program).run();
        assert!(!report.passed);
        let failure = report
            .failures
            .iter()
            .find(|failure| failure.scenario.as_deref() == Some("canonical"))
            .unwrap();
        assert_eq!(failure.code, EvidenceFailureCode::IncompleteBindings);
        assert_eq!(failure.source_id, "canonical.start");
        assert_eq!(failure.source, source("canonical.start", 11));
        assert!(!report.artifacts.pins.contains_key("canonical::after_one"));
    }

    #[test]
    fn evidence_requires_a_frozen_current_machine_identity() {
        let mut program = counter_program();
        program.machine_program.program_hashes.clear();

        let report = EvidenceRunner::new(&program).run();
        assert!(!report.passed);
        assert!(report.failures.iter().any(|failure| {
            failure.code == EvidenceFailureCode::AdmissionFailed
                && failure.message.contains("no frozen machine-program hash")
        }));
    }

    #[test]
    fn missing_registrations_keep_their_authored_physical_sources() {
        let mut program = counter_program();
        program.evidence.examples.insert(
            "missing".into(),
            EvidenceRef {
                scenario: "canonical".into(),
                pin: "absent".into(),
            },
        );
        program.evidence.example_sources.insert(
            "missing".into(),
            SourceRef {
                id: "example.missing".into(),
                path: "nested/conformance.uhura".into(),
                start: 71,
                end: 92,
            },
        );
        let report = EvidenceRunner::new(&program).run();
        let failure = report
            .failures
            .iter()
            .find(|failure| failure.source_id == "example.missing")
            .expect("missing example registration fails");
        assert_eq!(failure.source.path, "nested/conformance.uhura");
        assert_eq!((failure.source.start, failure.source.end), (71, 92));
    }

    #[test]
    fn failed_scenario_does_not_publish_pins_and_dependents_are_explicit_failures() {
        let mut program = counter_program();
        let scenario = program.evidence.scenarios.get_mut("canonical").unwrap();
        let EvidenceStep::ExpectReaction { commands, .. } = &mut scenario.steps[4] else {
            panic!("expected reaction step");
        };
        commands.clear();
        let report = EvidenceRunner::new(&program).run();
        assert!(!report.passed);
        assert_eq!(report.scenarios[0].status, ScenarioStatus::Failed);
        assert_eq!(report.scenarios[1].status, ScenarioStatus::Failed);
        assert_eq!(report.scenarios[2].status, ScenarioStatus::Failed);
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == EvidenceFailureCode::ExpectationMismatch)
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == EvidenceFailureCode::MissingSnapshot)
        );
    }
}
