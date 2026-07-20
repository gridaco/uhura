use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use num_bigint::BigInt;
use num_traits::Signed as _;
use serde::{Deserialize, Serialize};

use super::codec::{decode_hex_32, frame, hash, hex, nat, nat_u64};
use super::ir::{
    BinaryOp, Expr, Function, Machine, MatchArm, OutcomePolicy, Pattern, Program, SourceRef,
    Statement, StatementMatchArm, TypeRef, UnaryOp,
};
use super::value::{BoundaryNumber, Decimal, Value, ValueError};

pub const CHECKPOINT_PROTOCOL: &str = "uhura-checkpoint/0";
pub const GENESIS_RECEIPT_PROTOCOL: &str = "uhura-genesis-receipt/0";
pub const INGRESS_RECORD_PROTOCOL: &str = "uhura-ingress-record/0";
pub const REACTION_RECEIPT_PROTOCOL: &str = "uhura-reaction-receipt/0";

/// Prefix reserved for compiler-generated locals that carry a source-level
/// inline update result across a lowered reaction control-flow join.
///
/// These names are not source-visible. The 0.4 frontend emits them and the
/// reference runtime preserves them when a statement `if` or `match` restores
/// its lexical locals after the selected branch completes.
pub const INLINE_UPDATE_JOIN_LOCAL_PREFIX: &str = "__uhura_update_join_";

/// Prefix reserved for compiler-generated total `Option<T>` locals that carry
/// a lexical update return out of a source-level `while` body.
///
/// The matching IR `While::break_local` names one exact local. The runtime
/// preserves a selected `Some(value)` across the loop body's lexical scope and
/// exits that loop without exposing a general non-local control primitive.
pub const INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX: &str = "__uhura_update_loop_exit_";

/// Prefix reserved for compiler-generated, pure local continuations.
///
/// The checker emits these as an IR `Let` binding whose value is a `Lambda`.
/// The runtime keeps the lambda in an evaluator-private closure table instead
/// of exposing a function value through Uhura's serializable value model.
pub const PURE_CONTINUATION_LOCAL_PREFIX: &str = "__uhura_pure_continuation_";

fn is_inline_update_join_local(name: &str) -> bool {
    name.starts_with(INLINE_UPDATE_JOIN_LOCAL_PREFIX)
}

fn is_inline_update_loop_exit_local(name: &str) -> bool {
    name.starts_with(INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX)
}

fn is_inline_update_control_local(name: &str) -> bool {
    is_inline_update_join_local(name) || is_inline_update_loop_exit_local(name)
}

fn is_pure_continuation_local(name: &str) -> bool {
    name.starts_with(PURE_CONTINUATION_LOCAL_PREFIX)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceLifecycle {
    Running,
    Faulted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProgramFault {
    InvariantViolation { source: String },
    UnreachableReached { source: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum IngressAttempt {
    TransportText { text: String },
    Value { value: Value },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IngressRejectionKind {
    MalformedTransport,
    InvalidValue,
    Lifecycle,
    MissingMachine,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngressRecord {
    pub protocol: String,
    pub instance: String,
    pub machine_program_hash: String,
    /// Sequence in the independent ingress-rejection log. It never consumes
    /// or aliases a machine reaction sequence number.
    pub ordinal: u64,
    /// The next machine sequence observed when the rejection occurred.
    pub machine_sequence: u64,
    pub rejection: IngressRejectionKind,
    pub message: String,
    pub attempt: IngressAttempt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisReceipt {
    pub protocol: String,
    pub instance: String,
    pub machine_program_hash: String,
    pub configuration_hash: String,
    pub sequence: u64,
    pub initial_observation: Value,
    pub initial_state_hash: String,
}

impl GenesisReceipt {
    /// Canonical JSON transport. Semantic identity bytes are owned by the
    /// checked [`Program`] and exposed by
    /// [`Program::canonical_genesis_receipt_bytes`].
    pub fn to_canonical_string(&self) -> String {
        canonical_transport(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ReactionResolution {
    Completed {
        outcome: Value,
        policy: OutcomePolicy,
    },
    Fault {
        fault: ProgramFault,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionReceipt {
    pub protocol: String,
    pub instance: String,
    pub machine_program_hash: String,
    pub configuration_hash: String,
    pub sequence: u64,
    pub input: Value,
    pub resolution: ReactionResolution,
    pub ordered_commands: Vec<Value>,
    pub post_observation: Value,
    pub pre_state_hash: String,
    pub post_state_hash: String,
}

impl ReactionReceipt {
    /// Canonical JSON transport, not the machine-kernel semantic receipt encoding.
    pub fn to_canonical_string(&self) -> String {
        canonical_transport(self)
    }

    pub fn canonical_transport_bytes(&self) -> Vec<u8> {
        self.to_canonical_string().into_bytes()
    }
}

impl IngressRecord {
    /// Canonical JSON transport, not the machine-kernel semantic ingress encoding.
    pub fn to_canonical_string(&self) -> String {
        canonical_transport(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub protocol: String,
    pub instance: String,
    pub machine: String,
    pub machine_program_hash: String,
    pub configuration: Value,
    pub state: Value,
    pub inbox: Vec<Value>,
    pub lifecycle: InstanceLifecycle,
    pub next_sequence: u64,
    pub trace_prefix_hash: String,
}

impl Checkpoint {
    /// Canonical JSON transport, not the machine-kernel semantic checkpoint encoding.
    /// Use [`Program::canonical_checkpoint_bytes`] for replay identity bytes.
    pub fn to_canonical_string(&self) -> String {
        canonical_transport(self)
    }
}

fn canonical_transport(value: &impl Serialize) -> String {
    uhura_base::to_canonical_json(
        &serde_json::to_value(value).expect("Uhura machine transport artifact is serializable"),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Step {
    pub instance: Instance,
    pub receipt: ReactionReceipt,
}

struct ReactionTransition {
    receipt: ReactionReceipt,
    state: Option<Value>,
    observation: Option<Value>,
    lifecycle: InstanceLifecycle,
    next_sequence: u64,
    trace_prefix_hash: String,
}

impl ReactionTransition {
    fn apply(self, instance: &mut Instance) -> ReactionReceipt {
        let Self {
            receipt,
            state,
            observation,
            lifecycle,
            next_sequence,
            trace_prefix_hash,
        } = self;
        if let Some(state) = state {
            instance.state = state;
        }
        if let Some(observation) = observation {
            instance.observation = observation;
        }
        instance.lifecycle = lifecycle;
        instance.next_sequence = next_sequence;
        instance.trace_prefix_hash = trace_prefix_hash;
        instance.receipts.push(receipt.clone());
        receipt
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instance {
    pub id: String,
    pub machine: String,
    pub program_hash: String,
    pub configuration: Value,
    pub state: Value,
    pub observation: Value,
    pub inbox: VecDeque<Value>,
    pub lifecycle: InstanceLifecycle,
    pub next_sequence: u64,
    pub trace_prefix_hash: String,
    pub receipts: Vec<ReactionReceipt>,
    pub ingress_prefix_hash: String,
    pub next_ingress_ordinal: u64,
    pub ingress_records: Vec<IngressRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionError {
    pub message: String,
    pub source: Option<SourceRef>,
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AdmissionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IngressError {
    pub message: String,
    pub record: Option<Box<IngressRecord>>,
}

impl fmt::Display for IngressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for IngressError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubmissionError {
    Ingress(IngressError),
    Reaction(RuntimeError),
}

impl fmt::Display for SubmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ingress(error) => write!(formatter, "ingress: {error}"),
            Self::Reaction(error) => write!(formatter, "reaction: {error}"),
        }
    }
}

impl std::error::Error for SubmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Ingress(error) => Some(error),
            Self::Reaction(error) => Some(error),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreError {
    pub message: String,
}

impl fmt::Display for RestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RestoreError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    pub message: String,
    pub source: Option<SourceRef>,
}

impl RuntimeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    fn at(mut self, source: &SourceRef) -> Self {
        self.source = Some(source.clone());
        self
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RuntimeError {}

impl Program {
    pub fn canonical_genesis_receipt_bytes(
        &self,
        machine_id: &str,
        receipt: &GenesisReceipt,
    ) -> Result<Vec<u8>, RuntimeError> {
        let machine = self
            .machines
            .get(machine_id)
            .ok_or_else(|| RuntimeError::new(format!("unknown machine `{machine_id}`")))?;
        validate_genesis_receipt(self, machine, receipt)?;
        genesis_semantic_bytes(self, machine, receipt)
    }

    pub fn canonical_reaction_receipt_bytes(
        &self,
        machine_id: &str,
        receipt: &ReactionReceipt,
    ) -> Result<Vec<u8>, RuntimeError> {
        let machine = self
            .machines
            .get(machine_id)
            .ok_or_else(|| RuntimeError::new(format!("unknown machine `{machine_id}`")))?;
        validate_reaction_receipt(self, machine, receipt)?;
        reaction_semantic_bytes(self, machine, receipt)
    }

    pub fn canonical_ingress_record_bytes(
        &self,
        record: &IngressRecord,
    ) -> Result<Vec<u8>, RuntimeError> {
        ingress_semantic_bytes(record)
    }

    pub fn canonical_checkpoint_bytes(
        &self,
        checkpoint: &Checkpoint,
    ) -> Result<Vec<u8>, RuntimeError> {
        // Restore is the single complete validation boundary for checkpoint
        // protocol, compatibility, invariants, FIFO values and typed order.
        self.restore(checkpoint)
            .map_err(|error| RuntimeError::new(error.message))?;
        let machine = self
            .machines
            .get(&checkpoint.machine)
            .expect("restore established the checkpoint machine");
        checkpoint_semantic_bytes(self, machine, checkpoint)
    }

    pub fn admit(
        &self,
        machine_id: &str,
        configuration: Value,
        instance_id: impl Into<String>,
    ) -> Result<(Instance, GenesisReceipt), AdmissionError> {
        self.validate_protocol().map_err(|message| AdmissionError {
            message,
            source: None,
        })?;
        let machine = self
            .machines
            .get(machine_id)
            .ok_or_else(|| AdmissionError {
                message: format!("unknown Uhura machine `{machine_id}`"),
                source: None,
            })?;
        let canonical_configuration = self
            .canonicalize_value(&machine.config, &configuration)
            .map_err(|error| AdmissionError {
                message: format!("invalid machine configuration: {error}"),
                source: None,
            })?;
        if canonical_configuration != configuration {
            return Err(AdmissionError {
                message: "invalid machine configuration: value is not in canonical typed order"
                    .into(),
                source: None,
            });
        }
        let configuration = canonical_configuration;
        let instance_id = instance_id.into();
        validate_instance_identity(&instance_id).map_err(|message| AdmissionError {
            message,
            source: None,
        })?;
        let empty_state = BTreeMap::new();
        let context = EvalContext::new(self, machine, &configuration, &empty_state);
        for (require, source) in &machine.requires {
            if !context
                .eval_condition(require)
                .map_err(|error| AdmissionError {
                    message: error.message,
                    source: Some(source.clone()),
                })?
                .0
            {
                return Err(AdmissionError {
                    message: "machine configuration requirement is false".into(),
                    source: Some(source.clone()),
                });
            }
        }
        let state = initialize_state(self, machine, &configuration)?;
        let state_value = state_value(machine, &state);
        let context = EvalContext::new(self, machine, &configuration, &state);
        for (invariant, source) in &machine.invariants {
            if !context
                .eval_condition(invariant)
                .map_err(|error| AdmissionError {
                    message: error.message,
                    source: Some(source.clone()),
                })?
                .0
            {
                return Err(AdmissionError {
                    message: "initial state violates an invariant".into(),
                    source: Some(source.clone()),
                });
            }
        }
        let observation =
            observe_record(self, machine, &configuration, &state).map_err(|error| {
                AdmissionError {
                    message: error.message,
                    source: error.source,
                }
            })?;
        let program_hash = self
            .program_hashes
            .get(machine_id)
            .cloned()
            .ok_or_else(|| AdmissionError {
                message: format!("machine `{machine_id}` has no frozen machine-program hash"),
                source: None,
            })?;
        validate_hash("machine-program hash", &program_hash).map_err(|message| AdmissionError {
            message,
            source: None,
        })?;
        let configuration_hash =
            typed_value_hash(self, "configuration", &machine.config, &configuration).map_err(
                |error| AdmissionError {
                    message: error.message,
                    source: error.source,
                },
            )?;
        let state_type = Program::machine_state_type(machine);
        let state_hash =
            typed_value_hash(self, "state", &state_type, &state_value).map_err(|error| {
                AdmissionError {
                    message: error.message,
                    source: error.source,
                }
            })?;
        let genesis = GenesisReceipt {
            protocol: GENESIS_RECEIPT_PROTOCOL.into(),
            instance: instance_id.clone(),
            machine_program_hash: program_hash.clone(),
            configuration_hash,
            sequence: 0,
            initial_observation: observation.clone(),
            initial_state_hash: state_hash,
        };
        let trace_prefix_hash = hex(&hash(
            "trace-prefix",
            &[
                genesis_semantic_bytes(self, machine, &genesis).map_err(|error| {
                    AdmissionError {
                        message: error.message,
                        source: error.source,
                    }
                })?,
            ],
        ));
        Ok((
            Instance {
                id: instance_id,
                machine: machine_id.into(),
                program_hash,
                configuration,
                state: state_value,
                observation,
                inbox: VecDeque::new(),
                lifecycle: InstanceLifecycle::Running,
                next_sequence: 1,
                trace_prefix_hash,
                receipts: Vec::new(),
                ingress_prefix_hash: hex(&hash("ingress-prefix", &[])),
                next_ingress_ordinal: 1,
                ingress_records: Vec::new(),
            },
            genesis,
        ))
    }

    pub fn react(&self, instance: &Instance, input: Value) -> Result<Step, RuntimeError> {
        let transition = self.reaction_transition(instance, input)?;
        let mut next = instance.clone();
        let receipt = transition.apply(&mut next);
        Ok(Step {
            instance: next,
            receipt,
        })
    }

    /// React to one already-admitted input and commit the transition directly
    /// to `instance`.
    ///
    /// Unlike [`Program::react`], this path does not clone the instance or its
    /// retained audit history. A runtime error leaves `instance` unchanged.
    pub fn react_mut(
        &self,
        instance: &mut Instance,
        input: Value,
    ) -> Result<ReactionReceipt, RuntimeError> {
        let transition = self.reaction_transition(instance, input)?;
        Ok(transition.apply(instance))
    }

    fn reaction_transition(
        &self,
        instance: &Instance,
        input: Value,
    ) -> Result<ReactionTransition, RuntimeError> {
        if instance.lifecycle != InstanceLifecycle::Running {
            return Err(RuntimeError::new("the Uhura machine instance is faulted"));
        }
        let following_sequence = instance.next_sequence.checked_add(1).ok_or_else(|| {
            RuntimeError::new(
                "Uhura reaction sequence capacity is exhausted; no receipt was produced",
            )
        })?;
        let machine = self
            .machines
            .get(&instance.machine)
            .ok_or_else(|| RuntimeError::new("instance machine is absent from the program"))?;
        let canonical_input = self
            .canonicalize_input(machine, &input)
            .map_err(|error| RuntimeError::new(format!("invalid admitted input: {error}")))?;
        if canonical_input != input {
            return Err(RuntimeError::new(
                "invalid admitted input: value is not in canonical typed order",
            ));
        }
        let input = canonical_input;
        let (input_name, _) = variant_parts(&input)
            .ok_or_else(|| RuntimeError::new("an admitted input must be a closed variant"))?;
        let handler = machine
            .handlers
            .get(input_name)
            .ok_or_else(|| RuntimeError::new(format!("no handler for input `{input_name}`")))?;
        let Value::Record(pre_state_fields) = &instance.state else {
            return Err(RuntimeError::new("instance state is not a record"));
        };
        let pre_state = record_map(pre_state_fields)?;
        let mut draft = pre_state.clone();
        let mut locals = BTreeMap::new();
        let reconstructed = input.clone();
        if !match_pattern(&handler.pattern, &reconstructed, &mut locals)? {
            return Err(RuntimeError::new(format!(
                "input `{input_name}` did not match its checked handler pattern"
            ))
            .at(&handler.source));
        }
        let pre_state_value = state_value(machine, &pre_state);
        let state_type = Program::machine_state_type(machine);
        let pre_state_hash = typed_value_hash(self, "state", &state_type, &pre_state_value)?;
        let mut commands = Vec::new();
        let control = {
            let mut context = EvalContext::new(self, machine, &instance.configuration, &draft);
            context.locals = locals;
            context.run_statements(&handler.body, &mut draft, &mut commands)?
        };
        let sequence = instance.next_sequence;
        let configuration_hash = typed_value_hash(
            self,
            "configuration",
            &machine.config,
            &instance.configuration,
        )?;
        let (resolution, published_commands, post_state, lifecycle) = match control {
            Control::Finish(outcome) => {
                let outcome = self
                    .canonicalize_outcome(machine, &outcome)
                    .map_err(|error| RuntimeError::new(format!("invalid outcome: {error}")))?;
                let constructor = variant_constructor(&outcome).ok_or_else(|| {
                    RuntimeError::new("`finish` did not produce an outcome constructor")
                })?;
                let definition = machine
                    .outcome(constructor)
                    .ok_or_else(|| RuntimeError::new(format!("unknown outcome `{constructor}`")))?;
                match definition.policy {
                    OutcomePolicy::Abort => (
                        ReactionResolution::Completed {
                            outcome,
                            policy: OutcomePolicy::Abort,
                        },
                        Vec::new(),
                        pre_state.clone(),
                        InstanceLifecycle::Running,
                    ),
                    OutcomePolicy::Commit => {
                        if !machine.before_commit.is_empty() {
                            let mut context =
                                EvalContext::new(self, machine, &instance.configuration, &draft);
                            let settlement = context.run_statements(
                                &machine.before_commit,
                                &mut draft,
                                &mut commands,
                            )?;
                            match settlement {
                                Control::Continue => {}
                                Control::Finish(_) => {
                                    return Err(RuntimeError::new(
                                        "`before commit` attempted to replace the outcome",
                                    ));
                                }
                                Control::Fault(fault) => {
                                    return self.fault_transition(
                                        instance,
                                        input,
                                        fault,
                                        pre_state_hash,
                                    );
                                }
                            }
                        }
                        let context =
                            EvalContext::new(self, machine, &instance.configuration, &draft);
                        for (invariant, source) in &machine.invariants {
                            if !context.eval_condition(invariant)?.0 {
                                return self.fault_transition(
                                    instance,
                                    input,
                                    ProgramFault::InvariantViolation {
                                        source: source.id.clone(),
                                    },
                                    pre_state_hash,
                                );
                            }
                        }
                        let post_state_value = state_value(machine, &draft);
                        let canonical_state = self
                            .canonicalize_value(&state_type, &post_state_value)
                            .map_err(|error| {
                                RuntimeError::new(format!("invalid committed state: {error}"))
                            })?;
                        let Value::Record(canonical_state_fields) = canonical_state else {
                            unreachable!("machine state type is a record")
                        };
                        draft = record_map(&canonical_state_fields)?;
                        commands = commands
                            .iter()
                            .map(|command| {
                                self.canonicalize_command(machine, command)
                                    .map_err(|error| {
                                        RuntimeError::new(format!(
                                            "invalid emitted command: {error}"
                                        ))
                                    })
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        (
                            ReactionResolution::Completed {
                                outcome,
                                policy: OutcomePolicy::Commit,
                            },
                            commands,
                            draft,
                            InstanceLifecycle::Running,
                        )
                    }
                }
            }
            Control::Fault(fault) => {
                return self.fault_transition(instance, input, fault, pre_state_hash);
            }
            Control::Continue => {
                return Err(RuntimeError::new(format!(
                    "handler `{input_name}` fell through without `finish` or `unreachable`"
                ))
                .at(&handler.source));
            }
        };
        let post_state_value = state_value(machine, &post_state);
        let post_state_hash = typed_value_hash(self, "state", &state_type, &post_state_value)?;
        let post_observation = observe_record(self, machine, &instance.configuration, &post_state)?;
        let receipt = ReactionReceipt {
            protocol: REACTION_RECEIPT_PROTOCOL.into(),
            instance: instance.id.clone(),
            machine_program_hash: instance.program_hash.clone(),
            configuration_hash,
            sequence,
            input,
            resolution,
            ordered_commands: published_commands,
            post_observation: post_observation.clone(),
            pre_state_hash,
            post_state_hash,
        };
        let trace_prefix_hash =
            next_trace_prefix(self, machine, &instance.trace_prefix_hash, &receipt)?;
        Ok(ReactionTransition {
            receipt,
            state: Some(post_state_value),
            observation: Some(post_observation),
            lifecycle,
            next_sequence: following_sequence,
            trace_prefix_hash,
        })
    }

    fn fault_transition(
        &self,
        instance: &Instance,
        input: Value,
        fault: ProgramFault,
        state_hash: String,
    ) -> Result<ReactionTransition, RuntimeError> {
        let machine = self
            .machines
            .get(&instance.machine)
            .ok_or_else(|| RuntimeError::new("instance machine is absent from the program"))?;
        let following_sequence = instance.next_sequence.checked_add(1).ok_or_else(|| {
            RuntimeError::new(
                "Uhura reaction sequence capacity is exhausted; no fault receipt was produced",
            )
        })?;
        let receipt = ReactionReceipt {
            protocol: REACTION_RECEIPT_PROTOCOL.into(),
            instance: instance.id.clone(),
            machine_program_hash: instance.program_hash.clone(),
            configuration_hash: typed_value_hash(
                self,
                "configuration",
                &machine.config,
                &instance.configuration,
            )?,
            sequence: instance.next_sequence,
            input,
            resolution: ReactionResolution::Fault { fault },
            ordered_commands: Vec::new(),
            post_observation: instance.observation.clone(),
            pre_state_hash: state_hash.clone(),
            post_state_hash: state_hash,
        };
        let trace_prefix_hash =
            next_trace_prefix(self, machine, &instance.trace_prefix_hash, &receipt)?;
        Ok(ReactionTransition {
            receipt,
            state: None,
            observation: None,
            lifecycle: InstanceLifecycle::Faulted,
            next_sequence: following_sequence,
            trace_prefix_hash,
        })
    }

    pub fn enqueue(&self, instance: &mut Instance, input: Value) -> Result<(), IngressError> {
        if instance.lifecycle != InstanceLifecycle::Running {
            return Err(self.record_ingress_rejection(
                instance,
                IngressRejectionKind::Lifecycle,
                "submission to a faulted Uhura machine instance",
                IngressAttempt::Value { value: input },
            ));
        }
        let Some(machine) = self.machines.get(&instance.machine) else {
            return Err(self.record_ingress_rejection(
                instance,
                IngressRejectionKind::MissingMachine,
                "instance machine is absent from the program",
                IngressAttempt::Value { value: input },
            ));
        };
        let input = match self.canonicalize_input(machine, &input) {
            Ok(canonical) if canonical == input => canonical,
            Ok(_) => {
                return Err(self.record_ingress_rejection(
                    instance,
                    IngressRejectionKind::InvalidValue,
                    "invalid Uhura ingress: value is not in canonical typed order",
                    IngressAttempt::Value { value: input },
                ));
            }
            Err(error) => {
                return Err(self.record_ingress_rejection(
                    instance,
                    IngressRejectionKind::InvalidValue,
                    format!("invalid Uhura ingress: {error}"),
                    IngressAttempt::Value { value: input },
                ));
            }
        };
        instance.inbox.push_back(input);
        Ok(())
    }

    /// Append one rejection to the independent ingress audit log. This is the
    /// browser/host hook for malformed transport that cannot be decoded into a
    /// [`Value`]. It does not touch the FIFO or the machine sequence.
    pub fn reject_ingress_transport(
        &self,
        instance: &mut Instance,
        text: impl Into<String>,
        message: impl Into<String>,
    ) -> IngressError {
        self.record_ingress_rejection(
            instance,
            IngressRejectionKind::MalformedTransport,
            message,
            IngressAttempt::TransportText { text: text.into() },
        )
    }

    fn record_ingress_rejection(
        &self,
        instance: &mut Instance,
        rejection: IngressRejectionKind,
        message: impl Into<String>,
        attempt: IngressAttempt,
    ) -> IngressError {
        let message = message.into();
        let Some(next_ordinal) = instance.next_ingress_ordinal.checked_add(1) else {
            return IngressError {
                message: format!(
                    "{message}; the ingress rejection log exhausted its ordinal capacity"
                ),
                record: None,
            };
        };
        let record = IngressRecord {
            protocol: INGRESS_RECORD_PROTOCOL.into(),
            instance: instance.id.clone(),
            machine_program_hash: instance.program_hash.clone(),
            ordinal: instance.next_ingress_ordinal,
            machine_sequence: instance.next_sequence,
            rejection,
            message: message.clone(),
            attempt,
        };
        let prefix = ingress_prefix_after(&instance.ingress_prefix_hash, &record);
        match prefix {
            Ok(prefix) => {
                instance.ingress_prefix_hash = prefix;
                instance.next_ingress_ordinal = next_ordinal;
                instance.ingress_records.push(record.clone());
                IngressError {
                    message,
                    record: Some(Box::new(record)),
                }
            }
            Err(prefix_error) => IngressError {
                message: format!("{message}; could not record ingress rejection: {prefix_error}"),
                record: None,
            },
        }
    }

    pub fn drain_one(&self, instance: &Instance) -> Result<Option<Step>, RuntimeError> {
        let Some(input) = instance.inbox.front().cloned() else {
            return Ok(None);
        };
        let transition = self.reaction_transition(instance, input)?;
        let mut next = instance.clone();
        next.inbox.pop_front();
        let receipt = transition.apply(&mut next);
        Ok(Some(Step {
            instance: next,
            receipt,
        }))
    }

    /// Drain one queued input directly into `instance` without cloning its
    /// retained receipt or ingress history.
    ///
    /// The queued input is consumed only after the reaction has been evaluated
    /// successfully. A runtime error therefore leaves the instance and FIFO
    /// unchanged.
    pub fn drain_one_mut(
        &self,
        instance: &mut Instance,
    ) -> Result<Option<ReactionReceipt>, RuntimeError> {
        let Some(input) = instance.inbox.front().cloned() else {
            return Ok(None);
        };
        let transition = self.reaction_transition(instance, input)?;
        instance.inbox.pop_front();
        Ok(Some(transition.apply(instance)))
    }

    /// Admit and drain one input as a single in-place host operation.
    ///
    /// Rejected ingress is audited exactly as in [`Program::enqueue`]. If an
    /// admitted input cannot react, the new queue entry is rolled back and the
    /// pre-existing instance remains unchanged.
    pub fn submit_one(
        &self,
        instance: &mut Instance,
        input: Value,
    ) -> Result<ReactionReceipt, SubmissionError> {
        self.enqueue(instance, input)
            .map_err(SubmissionError::Ingress)?;
        match self.drain_one_mut(instance) {
            Ok(Some(receipt)) => Ok(receipt),
            Ok(None) => unreachable!("successful enqueue always leaves one queued input"),
            Err(error) => {
                let submitted = instance.inbox.pop_back();
                debug_assert!(
                    submitted.is_some(),
                    "failed reaction retains the newly submitted queue entry"
                );
                Err(SubmissionError::Reaction(error))
            }
        }
    }

    pub fn checkpoint(&self, instance: &Instance) -> Checkpoint {
        Checkpoint {
            protocol: CHECKPOINT_PROTOCOL.into(),
            instance: instance.id.clone(),
            machine: instance.machine.clone(),
            machine_program_hash: instance.program_hash.clone(),
            configuration: instance.configuration.clone(),
            state: instance.state.clone(),
            inbox: instance.inbox.iter().cloned().collect(),
            lifecycle: instance.lifecycle,
            next_sequence: instance.next_sequence,
            trace_prefix_hash: instance.trace_prefix_hash.clone(),
        }
    }

    pub fn restore(&self, checkpoint: &Checkpoint) -> Result<Instance, RestoreError> {
        if checkpoint.protocol != CHECKPOINT_PROTOCOL {
            return Err(RestoreError {
                message: format!("unsupported checkpoint protocol `{}`", checkpoint.protocol),
            });
        }
        validate_instance_identity(&checkpoint.instance)
            .map_err(|message| RestoreError { message })?;
        validate_hash(
            "checkpoint machine-program hash",
            &checkpoint.machine_program_hash,
        )
        .map_err(|message| RestoreError { message })?;
        validate_hash(
            "checkpoint trace-prefix hash",
            &checkpoint.trace_prefix_hash,
        )
        .map_err(|message| RestoreError { message })?;
        if checkpoint.next_sequence == 0 {
            return Err(RestoreError {
                message: "checkpoint next sequence must be at least 1".into(),
            });
        }
        let Some(expected) = self.program_hashes.get(&checkpoint.machine) else {
            return Err(RestoreError {
                message: format!("unknown machine `{}`", checkpoint.machine),
            });
        };
        if expected != &checkpoint.machine_program_hash {
            return Err(RestoreError {
                message: "checkpoint machine program hash is incompatible".into(),
            });
        }
        let machine = self
            .machines
            .get(&checkpoint.machine)
            .ok_or_else(|| RestoreError {
                message: format!("unknown machine `{}`", checkpoint.machine),
            })?;
        let configuration = self
            .canonicalize_value(&machine.config, &checkpoint.configuration)
            .map_err(|error| RestoreError {
                message: format!("invalid checkpoint configuration: {error}"),
            })?;
        if configuration != checkpoint.configuration {
            return Err(RestoreError {
                message: "checkpoint configuration is not in canonical typed order".into(),
            });
        }
        let empty_state = BTreeMap::new();
        let requirement_context = EvalContext::new(self, machine, &configuration, &empty_state);
        for (requirement, source) in &machine.requires {
            if !requirement_context
                .eval_condition(requirement)
                .map_err(|error| RestoreError {
                    message: format!(
                        "checkpoint configuration requirement `{}` failed to evaluate: {}",
                        source.id, error.message
                    ),
                })?
                .0
            {
                return Err(RestoreError {
                    message: format!(
                        "checkpoint configuration violates requirement `{}`",
                        source.id
                    ),
                });
            }
        }
        let state_type = Program::machine_state_type(machine);
        let state_value = self
            .canonicalize_value(&state_type, &checkpoint.state)
            .map_err(|error| RestoreError {
                message: format!("invalid checkpoint state: {error}"),
            })?;
        if state_value != checkpoint.state {
            return Err(RestoreError {
                message: "checkpoint state is not in canonical typed field/collection order".into(),
            });
        }
        let Value::Record(state_fields) = &state_value else {
            unreachable!("machine state type is a record")
        };
        let state = record_map(state_fields).map_err(|error| RestoreError {
            message: error.message,
        })?;
        let invariant_context = EvalContext::new(self, machine, &configuration, &state);
        for (invariant, source) in &machine.invariants {
            if !invariant_context
                .eval_condition(invariant)
                .map_err(|error| RestoreError {
                    message: format!(
                        "checkpoint invariant `{}` failed to evaluate: {}",
                        source.id, error.message
                    ),
                })?
                .0
            {
                return Err(RestoreError {
                    message: format!("checkpoint state violates invariant `{}`", source.id),
                });
            }
        }
        let inbox = checkpoint
            .inbox
            .iter()
            .enumerate()
            .map(|(index, input)| {
                let canonical =
                    self.canonicalize_input(machine, input)
                        .map_err(|error| RestoreError {
                            message: format!("invalid checkpoint inbox value {index}: {error}"),
                        })?;
                if &canonical != input {
                    return Err(RestoreError {
                        message: format!(
                            "checkpoint inbox value {index} is not in canonical typed order"
                        ),
                    });
                }
                Ok(canonical)
            })
            .collect::<Result<VecDeque<_>, _>>()?;
        let observation =
            observe_record(self, machine, &configuration, &state).map_err(|error| {
                RestoreError {
                    message: error.message,
                }
            })?;
        Ok(Instance {
            id: checkpoint.instance.clone(),
            machine: checkpoint.machine.clone(),
            program_hash: checkpoint.machine_program_hash.clone(),
            configuration,
            state: state_value,
            observation,
            inbox,
            lifecycle: checkpoint.lifecycle,
            next_sequence: checkpoint.next_sequence,
            trace_prefix_hash: checkpoint.trace_prefix_hash.clone(),
            receipts: Vec::new(),
            ingress_prefix_hash: hex(&hash("ingress-prefix", &[])),
            next_ingress_ordinal: 1,
            ingress_records: Vec::new(),
        })
    }
}

fn initialize_state(
    program: &Program,
    machine: &Machine,
    configuration: &Value,
) -> Result<BTreeMap<String, Value>, AdmissionError> {
    let mut fields = BTreeMap::new();
    for field in &machine.state {
        let context = EvalContext::new(program, machine, configuration, &BTreeMap::new());
        let value = context
            .eval(&field.initial)
            .map_err(|error| AdmissionError {
                message: error.message,
                source: Some(field.source.clone()),
            })?;
        let value = program
            .canonicalize_value(&field.ty, &value)
            .map_err(|error| AdmissionError {
                message: format!("invalid initial state field `{}`: {error}", field.name),
                source: Some(field.source.clone()),
            })?;
        fields.insert(field.name.clone(), value);
    }
    Ok(fields)
}

fn observe_record(
    program: &Program,
    machine: &Machine,
    configuration: &Value,
    state: &BTreeMap<String, Value>,
) -> Result<Value, RuntimeError> {
    let mut fields = Vec::new();
    for field in &machine.observation {
        let context = EvalContext::new(program, machine, configuration, state);
        let value = context.eval(&field.expression)?;
        let value = program
            .canonicalize_value(&field.ty, &value)
            .map_err(|error| {
                RuntimeError::new(format!(
                    "invalid observation field `{}`: {error}",
                    field.name
                ))
                .at(&field.source)
            })?;
        fields.push((field.name.clone(), value));
    }
    Ok(Value::Record(fields))
}

fn state_value(machine: &Machine, state: &BTreeMap<String, Value>) -> Value {
    Value::Record(
        machine
            .state
            .iter()
            .filter_map(|field| {
                state
                    .get(&field.name)
                    .cloned()
                    .map(|value| (field.name.clone(), value))
            })
            .collect(),
    )
}

pub(super) fn record_map(
    fields: &[(String, Value)],
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    let mut record = BTreeMap::new();
    for (name, value) in fields {
        if record.insert(name.clone(), value.clone()).is_some() {
            return Err(RuntimeError::new(format!(
                "record contains duplicate field `{name}`"
            )));
        }
    }
    Ok(record)
}

fn typed_value_hash(
    program: &Program,
    domain: &str,
    expected: &TypeRef,
    value: &Value,
) -> Result<String, RuntimeError> {
    let bytes = program
        .canonical_value_bytes(expected, value)
        .map_err(|error| RuntimeError::new(format!("invalid typed {domain} value: {error}")))?;
    Ok(hex(&hash(domain, &[bytes])))
}

fn next_trace_prefix(
    program: &Program,
    machine: &Machine,
    previous: &str,
    receipt: &ReactionReceipt,
) -> Result<String, RuntimeError> {
    let previous = decode_hash(previous)
        .map_err(|message| RuntimeError::new(format!("invalid trace prefix: {message}")))?;
    Ok(hex(&hash(
        "trace-prefix",
        &[
            previous,
            program.canonical_reaction_receipt_bytes(&machine.id, receipt)?,
        ],
    )))
}

fn validate_genesis_receipt(
    program: &Program,
    machine: &Machine,
    receipt: &GenesisReceipt,
) -> Result<(), RuntimeError> {
    if receipt.protocol != GENESIS_RECEIPT_PROTOCOL {
        return Err(RuntimeError::new(format!(
            "unsupported genesis receipt protocol `{}`",
            receipt.protocol
        )));
    }
    validate_instance_identity(&receipt.instance).map_err(RuntimeError::new)?;
    validate_hash("machine-program hash", &receipt.machine_program_hash)
        .map_err(RuntimeError::new)?;
    validate_hash("configuration hash", &receipt.configuration_hash).map_err(RuntimeError::new)?;
    validate_hash("initial state hash", &receipt.initial_state_hash).map_err(RuntimeError::new)?;
    if receipt.sequence != 0 {
        return Err(RuntimeError::new("genesis receipt sequence must be zero"));
    }
    if program.program_hashes.get(&machine.id) != Some(&receipt.machine_program_hash) {
        return Err(RuntimeError::new(
            "genesis receipt machine-program hash is incompatible",
        ));
    }
    program
        .validate_value(
            &Program::machine_observation_type(machine),
            &receipt.initial_observation,
        )
        .map_err(|error| RuntimeError::new(format!("invalid initial observation: {error}")))
}

fn validate_reaction_receipt(
    program: &Program,
    machine: &Machine,
    receipt: &ReactionReceipt,
) -> Result<(), RuntimeError> {
    if receipt.protocol != REACTION_RECEIPT_PROTOCOL {
        return Err(RuntimeError::new(format!(
            "unsupported reaction receipt protocol `{}`",
            receipt.protocol
        )));
    }
    validate_instance_identity(&receipt.instance).map_err(RuntimeError::new)?;
    validate_hash("machine-program hash", &receipt.machine_program_hash)
        .map_err(RuntimeError::new)?;
    validate_hash("configuration hash", &receipt.configuration_hash).map_err(RuntimeError::new)?;
    validate_hash("pre-state hash", &receipt.pre_state_hash).map_err(RuntimeError::new)?;
    validate_hash("post-state hash", &receipt.post_state_hash).map_err(RuntimeError::new)?;
    if receipt.sequence == 0 {
        return Err(RuntimeError::new(
            "reaction receipt sequence must be at least one",
        ));
    }
    if program.program_hashes.get(&machine.id) != Some(&receipt.machine_program_hash) {
        return Err(RuntimeError::new(
            "reaction receipt machine-program hash is incompatible",
        ));
    }
    program
        .canonicalize_input(machine, &receipt.input)
        .map_err(|error| RuntimeError::new(format!("invalid receipt input: {error}")))?;
    for command in &receipt.ordered_commands {
        program
            .canonicalize_command(machine, command)
            .map_err(|error| RuntimeError::new(format!("invalid receipt command: {error}")))?;
    }
    program
        .validate_value(
            &Program::machine_observation_type(machine),
            &receipt.post_observation,
        )
        .map_err(|error| RuntimeError::new(format!("invalid post observation: {error}")))?;
    match &receipt.resolution {
        ReactionResolution::Completed { outcome, policy } => {
            let outcome = program
                .canonicalize_outcome(machine, outcome)
                .map_err(|error| RuntimeError::new(format!("invalid receipt outcome: {error}")))?;
            let constructor =
                variant_constructor(&outcome).expect("canonical outcome is a closed variant");
            let expected = machine
                .outcome(constructor)
                .expect("canonical outcome definition exists")
                .policy;
            if *policy != expected {
                return Err(RuntimeError::new(format!(
                    "receipt outcome `{constructor}` uses the wrong commit policy"
                )));
            }
            if *policy == OutcomePolicy::Abort
                && (receipt.pre_state_hash != receipt.post_state_hash
                    || !receipt.ordered_commands.is_empty())
            {
                return Err(RuntimeError::new(
                    "abort receipt must preserve state and publish no commands",
                ));
            }
        }
        ReactionResolution::Fault { .. } => {
            if receipt.pre_state_hash != receipt.post_state_hash
                || !receipt.ordered_commands.is_empty()
            {
                return Err(RuntimeError::new(
                    "fault receipt must preserve state and publish no commands",
                ));
            }
        }
    }
    Ok(())
}

fn genesis_semantic_bytes(
    program: &Program,
    machine: &Machine,
    receipt: &GenesisReceipt,
) -> Result<Vec<u8>, RuntimeError> {
    let observation_type = Program::machine_observation_type(machine);
    Ok(frame(
        "genesis-receipt",
        &[
            instance_identity_bytes(&receipt.instance),
            decode_hash(&receipt.machine_program_hash).map_err(RuntimeError::new)?,
            decode_hash(&receipt.configuration_hash).map_err(RuntimeError::new)?,
            nat_u64(receipt.sequence),
            program
                .canonical_value_bytes(&observation_type, &receipt.initial_observation)
                .map_err(|error| RuntimeError::new(error.to_string()))?,
            decode_hash(&receipt.initial_state_hash).map_err(RuntimeError::new)?,
        ],
    ))
}

fn reaction_semantic_bytes(
    program: &Program,
    machine: &Machine,
    receipt: &ReactionReceipt,
) -> Result<Vec<u8>, RuntimeError> {
    let resolution = match &receipt.resolution {
        ReactionResolution::Completed { outcome, policy } => frame(
            "completed",
            &[
                program
                    .canonical_outcome_bytes(machine, outcome)
                    .map_err(|error| RuntimeError::new(error.to_string()))?,
                vec![match policy {
                    OutcomePolicy::Commit => 0,
                    OutcomePolicy::Abort => 1,
                }],
            ],
        ),
        ReactionResolution::Fault { fault } => frame("fault", &[program_fault_bytes(fault)]),
    };
    let commands = receipt
        .ordered_commands
        .iter()
        .map(|command| program.canonical_command_bytes(machine, command))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    let observation_type = Program::machine_observation_type(machine);
    Ok(frame(
        "reaction-receipt",
        &[
            instance_identity_bytes(&receipt.instance),
            decode_hash(&receipt.machine_program_hash).map_err(RuntimeError::new)?,
            decode_hash(&receipt.configuration_hash).map_err(RuntimeError::new)?,
            nat_u64(receipt.sequence),
            program
                .canonical_input_bytes(machine, &receipt.input)
                .map_err(|error| RuntimeError::new(error.to_string()))?,
            resolution,
            frame("command-list", &commands),
            program
                .canonical_value_bytes(&observation_type, &receipt.post_observation)
                .map_err(|error| RuntimeError::new(error.to_string()))?,
            decode_hash(&receipt.pre_state_hash).map_err(RuntimeError::new)?,
            decode_hash(&receipt.post_state_hash).map_err(RuntimeError::new)?,
        ],
    ))
}

fn checkpoint_semantic_bytes(
    program: &Program,
    machine: &Machine,
    checkpoint: &Checkpoint,
) -> Result<Vec<u8>, RuntimeError> {
    let state_type = Program::machine_state_type(machine);
    let inbox = checkpoint
        .inbox
        .iter()
        .map(|input| program.canonical_input_bytes(machine, input))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    Ok(frame(
        "checkpoint",
        &[
            instance_identity_bytes(&checkpoint.instance),
            declaration_identity_bytes(&checkpoint.machine),
            decode_hash(&checkpoint.machine_program_hash).map_err(RuntimeError::new)?,
            program
                .canonical_value_bytes(&machine.config, &checkpoint.configuration)
                .map_err(|error| RuntimeError::new(error.to_string()))?,
            program
                .canonical_value_bytes(&state_type, &checkpoint.state)
                .map_err(|error| RuntimeError::new(error.to_string()))?,
            frame("inbox", &inbox),
            vec![match checkpoint.lifecycle {
                InstanceLifecycle::Running => 0,
                InstanceLifecycle::Faulted => 1,
            }],
            nat_u64(checkpoint.next_sequence),
            decode_hash(&checkpoint.trace_prefix_hash).map_err(RuntimeError::new)?,
        ],
    ))
}

fn ingress_semantic_bytes(record: &IngressRecord) -> Result<Vec<u8>, RuntimeError> {
    if record.protocol != INGRESS_RECORD_PROTOCOL {
        return Err(RuntimeError::new(format!(
            "unsupported ingress record protocol `{}`",
            record.protocol
        )));
    }
    validate_instance_identity(&record.instance).map_err(RuntimeError::new)?;
    validate_hash("ingress machine-program hash", &record.machine_program_hash)
        .map_err(RuntimeError::new)?;
    if record.ordinal == 0 {
        return Err(RuntimeError::new("ingress ordinal must be at least one"));
    }
    if record.machine_sequence == 0 {
        return Err(RuntimeError::new(
            "ingress machine sequence must be at least one",
        ));
    }
    let attempt = match &record.attempt {
        IngressAttempt::TransportText { text } => {
            frame("transport-text", &[text.as_bytes().to_vec()])
        }
        IngressAttempt::Value { value } => frame(
            "wire-value",
            &[uhura_base::to_canonical_json(&value.to_wire_json()).into_bytes()],
        ),
    };
    Ok(frame(
        "ingress-record",
        &[
            instance_identity_bytes(&record.instance),
            decode_hash(&record.machine_program_hash).map_err(RuntimeError::new)?,
            nat_u64(record.ordinal),
            nat_u64(record.machine_sequence),
            vec![match record.rejection {
                IngressRejectionKind::MalformedTransport => 0,
                IngressRejectionKind::InvalidValue => 1,
                IngressRejectionKind::Lifecycle => 2,
                IngressRejectionKind::MissingMachine => 3,
            }],
            attempt,
        ],
    ))
}

fn ingress_prefix_after(previous: &str, record: &IngressRecord) -> Result<String, String> {
    let previous = decode_hash(previous).map_err(|message| format!("invalid prefix: {message}"))?;
    let record = ingress_semantic_bytes(record).map_err(|error| error.message)?;
    Ok(hex(&hash("ingress-prefix", &[previous, record])))
}

fn instance_identity_bytes(identity: &str) -> Vec<u8> {
    frame("instance-identity", &[identity.as_bytes().to_vec()])
}

fn declaration_identity_bytes(identity: &str) -> Vec<u8> {
    frame("declaration-identity", &[identity.as_bytes().to_vec()])
}

fn program_fault_bytes(fault: &ProgramFault) -> Vec<u8> {
    let (ordinal, source) = match fault {
        ProgramFault::InvariantViolation { source } => (0, source),
        ProgramFault::UnreachableReached { source } => (1, source),
    };
    frame(
        "program-fault",
        &[
            nat(ordinal),
            frame("source-identity", &[source.as_bytes().to_vec()]),
        ],
    )
}

fn validate_instance_identity(identity: &str) -> Result<(), String> {
    if identity.is_empty() {
        return Err("Uhura machine instance identity cannot be empty".into());
    }
    if identity.chars().any(char::is_control) {
        return Err("Uhura machine instance identity cannot contain control characters".into());
    }
    Ok(())
}

fn validate_hash(name: &str, value: &str) -> Result<(), String> {
    decode_hash(value)
        .map(|_| ())
        .map_err(|message| format!("{name} {message}"))
}

fn decode_hash(value: &str) -> Result<Vec<u8>, String> {
    decode_hex_32(value).map(|bytes| bytes.to_vec())
}

type VariantField = (Option<String>, Value);

fn variant_parts(value: &Value) -> Option<(&str, &[VariantField])> {
    match value {
        Value::Variant {
            constructor,
            fields,
            ..
        } => Some((constructor, fields)),
        _ => None,
    }
}

fn variant_constructor(value: &Value) -> Option<&str> {
    variant_parts(value).map(|(constructor, _)| constructor)
}

enum Control {
    Continue,
    Finish(Value),
    Fault(ProgramFault),
}

#[derive(Clone)]
struct PureContinuationClosure {
    params: Vec<String>,
    body: Expr,
    captured_locals: BTreeMap<String, Value>,
}

struct EvalContext<'a> {
    program: &'a Program,
    machine: &'a Machine,
    configuration: &'a Value,
    state: BTreeMap<String, Value>,
    locals: BTreeMap<String, Value>,
    pure_continuations: BTreeMap<String, PureContinuationClosure>,
    derives: std::cell::RefCell<BTreeMap<String, Value>>,
    call_stack: std::cell::RefCell<Vec<String>>,
}

impl<'a> EvalContext<'a> {
    fn new(
        program: &'a Program,
        machine: &'a Machine,
        configuration: &'a Value,
        state: &BTreeMap<String, Value>,
    ) -> Self {
        Self {
            program,
            machine,
            configuration,
            state: state.clone(),
            locals: BTreeMap::new(),
            pure_continuations: BTreeMap::new(),
            derives: std::cell::RefCell::new(BTreeMap::new()),
            call_stack: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn child(&self) -> Self {
        Self {
            program: self.program,
            machine: self.machine,
            configuration: self.configuration,
            state: self.state.clone(),
            locals: self.locals.clone(),
            pure_continuations: self.pure_continuations.clone(),
            derives: std::cell::RefCell::new(self.derives.borrow().clone()),
            call_stack: std::cell::RefCell::new(self.call_stack.borrow().clone()),
        }
    }

    fn run_statements(
        &mut self,
        statements: &[Statement],
        draft: &mut BTreeMap<String, Value>,
        commands: &mut Vec<Value>,
    ) -> Result<Control, RuntimeError> {
        for statement in statements {
            self.state.clone_from(draft);
            self.derives.borrow_mut().clear();
            let control = match statement {
                Statement::Let {
                    name,
                    value,
                    source,
                } => {
                    let value = self.eval(value).map_err(|error| error.at(source))?;
                    self.locals.insert(name.clone(), value);
                    Control::Continue
                }
                Statement::Set {
                    field,
                    value,
                    source,
                } => {
                    if !draft.contains_key(field) {
                        return Err(
                            RuntimeError::new(format!("unknown state field `{field}`")).at(source)
                        );
                    }
                    let value = self.eval(value).map_err(|error| error.at(source))?;
                    let expected = self
                        .machine
                        .state
                        .iter()
                        .find(|state| state.name == *field)
                        .map(|state| &state.ty)
                        .ok_or_else(|| {
                            RuntimeError::new(format!("unknown state field `{field}`")).at(source)
                        })?;
                    let value =
                        self.program
                            .canonicalize_value(expected, &value)
                            .map_err(|error| {
                                RuntimeError::new(format!(
                                    "invalid value for state field `{field}`: {error}"
                                ))
                                .at(source)
                            })?;
                    draft.insert(field.clone(), value);
                    Control::Continue
                }
                Statement::Emit { value, source } => {
                    let value = self.eval(value).map_err(|error| error.at(source))?;
                    let value = self
                        .program
                        .canonicalize_command(self.machine, &value)
                        .map_err(|error| {
                            RuntimeError::new(format!("invalid emitted command: {error}"))
                                .at(source)
                        })?;
                    commands.push(value);
                    Control::Continue
                }
                Statement::If {
                    condition,
                    then_body,
                    else_body,
                    source,
                } => {
                    let (condition, bindings) = self
                        .eval_condition(condition)
                        .map_err(|error| error.at(source))?;
                    let saved = self.locals.clone();
                    self.locals.extend(bindings);
                    let control = if condition {
                        self.run_statements(then_body, draft, commands)?
                    } else {
                        self.locals = saved.clone();
                        self.run_statements(else_body, draft, commands)?
                    };
                    let update_control = self
                        .locals
                        .iter()
                        .filter(|(name, _)| is_inline_update_control_local(name))
                        .map(|(name, value)| (name.clone(), value.clone()))
                        .collect::<BTreeMap<_, _>>();
                    self.locals = saved;
                    self.locals.extend(update_control);
                    control
                }
                Statement::Match {
                    value,
                    arms,
                    source,
                } => {
                    let value = self.eval(value).map_err(|error| error.at(source))?;
                    self.run_statement_match(&value, arms, draft, commands)?
                }
                Statement::While {
                    condition,
                    decreases,
                    body,
                    break_local,
                    source,
                } => {
                    let mut previous = None;
                    loop {
                        // The condition and decrease measure belong to the next
                        // loop step. Observe the draft produced by the previous
                        // body before evaluating either one.
                        self.state.clone_from(draft);
                        self.derives.borrow_mut().clear();
                        let (keep_going, bindings) = self
                            .eval_condition(condition)
                            .map_err(|error| error.at(source))?;
                        if !keep_going {
                            break Control::Continue;
                        }
                        let measure =
                            integer_value(&self.eval(decreases).map_err(|error| error.at(source))?)
                                .ok_or_else(|| {
                                    RuntimeError::new("loop decrease measure is not an integer")
                                        .at(source)
                                })?;
                        if measure.is_negative() {
                            return Err(
                                RuntimeError::new("loop decrease measure is negative").at(source)
                            );
                        }
                        if let Some(previous) = previous
                            && measure >= previous
                        {
                            return Err(RuntimeError::new(
                                "loop decrease measure did not become strictly smaller",
                            )
                            .at(source));
                        }
                        previous = Some(measure);
                        let saved = self.locals.clone();
                        self.locals.extend(bindings);
                        let control = self.run_statements(body, draft, commands)?;
                        let selected_break = break_local.as_ref().and_then(|name| {
                            self.locals.get(name).and_then(|value| {
                                (variant_constructor(value) == Some("some"))
                                    .then(|| (name.clone(), value.clone()))
                            })
                        });
                        self.locals = saved;
                        if !matches!(control, Control::Continue) {
                            break control;
                        }
                        if let Some((name, value)) = selected_break {
                            self.locals.insert(name, value);
                            break Control::Continue;
                        }
                    }
                }
                Statement::Finish { outcome, source } => {
                    let value = self.eval(outcome).map_err(|error| error.at(source))?;
                    let value = self
                        .program
                        .canonicalize_outcome(self.machine, &value)
                        .map_err(|error| {
                            RuntimeError::new(format!("invalid outcome: {error}")).at(source)
                        })?;
                    Control::Finish(value)
                }
                Statement::Unreachable { source } => {
                    Control::Fault(ProgramFault::UnreachableReached {
                        source: source.id.clone(),
                    })
                }
                Statement::Delegate {
                    transition,
                    args,
                    source,
                } => {
                    let transition =
                        self.machine.transitions.get(transition).ok_or_else(|| {
                            RuntimeError::new("unknown named transition").at(source)
                        })?;
                    let values = args
                        .iter()
                        .map(|argument| self.eval(argument))
                        .collect::<Result<Vec<_>, _>>()?;
                    if values.len() != transition.params.len() {
                        return Err(RuntimeError::new(format!(
                            "transition `{}` expected {} arguments, got {}",
                            transition.name,
                            transition.params.len(),
                            values.len()
                        ))
                        .at(source));
                    }
                    let saved = self.locals.clone();
                    self.locals = transition
                        .params
                        .iter()
                        .map(|(name, _)| name.clone())
                        .zip(values)
                        .collect();
                    let control = self.run_statements(&transition.body, draft, commands)?;
                    self.locals = saved;
                    control
                }
            };
            if !matches!(control, Control::Continue) {
                return Ok(control);
            }
        }
        Ok(Control::Continue)
    }

    fn run_statement_match(
        &mut self,
        value: &Value,
        arms: &[StatementMatchArm],
        draft: &mut BTreeMap<String, Value>,
        commands: &mut Vec<Value>,
    ) -> Result<Control, RuntimeError> {
        for arm in arms {
            let mut bindings = BTreeMap::new();
            if match_pattern(&arm.pattern, value, &mut bindings)? {
                let saved = self.locals.clone();
                self.locals.extend(bindings);
                let control = self.run_statements(&arm.body, draft, commands)?;
                let update_control = self
                    .locals
                    .iter()
                    .filter(|(name, _)| is_inline_update_control_local(name))
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect::<BTreeMap<_, _>>();
                self.locals = saved;
                self.locals.extend(update_control);
                return Ok(control);
            }
        }
        Err(RuntimeError::new(format!(
            "checked statement match was not exhaustive for `{}` value `{value:?}`",
            value.type_identity(),
        )))
    }

    fn eval(&self, expression: &Expr) -> Result<Value, RuntimeError> {
        match expression {
            Expr::Literal { value } => Ok(value.clone()),
            Expr::Name { name } => self.lookup(name),
            Expr::Constructor {
                type_id,
                constructor,
                fields,
            } => {
                let fields = fields
                    .iter()
                    .map(|(name, value)| self.eval(value).map(|value| (name.clone(), value)))
                    .collect::<Result<Vec<_>, _>>()?;
                if type_id == "BoundaryNumber" && constructor == "finite" {
                    return match fields.as_slice() {
                        [(_, Value::Decimal(value))] => {
                            Ok(Value::Boundary(BoundaryNumber::Finite(value.clone())))
                        }
                        [(_, _)] => Err(RuntimeError::new(
                            "BoundaryNumber.finite needs an exact Decimal",
                        )),
                        _ => Err(RuntimeError::new(
                            "BoundaryNumber.finite needs one argument",
                        )),
                    };
                }
                Ok(Value::variant(type_id, constructor, fields))
            }
            Expr::Key { type_id, value } => Ok(Value::Key {
                type_id: type_id.clone(),
                value: Box::new(self.eval(value)?),
            }),
            Expr::Tuple { values } => Ok(Value::Tuple(
                values
                    .iter()
                    .map(|value| self.eval(value))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Expr::Record { fields } => Value::record(
                fields
                    .iter()
                    .map(|(name, value)| self.eval(value).map(|value| (name.clone(), value)))
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(value_error),
            Expr::Seq { values } => Ok(Value::Seq(
                values
                    .iter()
                    .map(|value| self.eval(value))
                    .collect::<Result<_, _>>()?,
            )),
            Expr::Map {
                entries,
                result_type,
            } => self.canonicalize_checked_value(
                result_type,
                Value::Map(
                    entries
                        .iter()
                        .map(|(key, value)| Ok((self.eval(key)?, self.eval(value)?)))
                        .collect::<Result<Vec<_>, RuntimeError>>()?,
                ),
                "map literal",
            ),
            Expr::Table { key_type, entries } => Ok(Value::Table {
                key_type: key_type.clone(),
                entries: entries
                    .iter()
                    .map(|(name, value)| self.eval(value).map(|value| (name.clone(), value)))
                    .collect::<Result<_, _>>()?,
            }),
            Expr::Unary { op, value } => {
                let value = self.eval(value)?;
                match (op, value) {
                    (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
                    (UnaryOp::Negate, Value::Integer { value, .. }) => Ok(Value::int(-value)),
                    (UnaryOp::Negate, Value::Decimal(value)) => {
                        Ok(Value::Decimal(Decimal::zero().subtract(&value)))
                    }
                    (UnaryOp::Negate, Value::Boundary(BoundaryNumber::Finite(value))) => Ok(
                        Value::Boundary(BoundaryNumber::Finite(Decimal::zero().subtract(&value))),
                    ),
                    _ => Err(RuntimeError::new("invalid unary operand")),
                }
            }
            Expr::Binary { op, left, right } => self.eval_binary(*op, left, right),
            Expr::Call {
                function,
                args,
                result_type,
            } => self.call(function, args, result_type),
            Expr::Invoke { function, args } => match function.as_ref() {
                Expr::Name { name } if self.pure_continuations.contains_key(name) => {
                    let closure = self
                        .pure_continuations
                        .get(name)
                        .cloned()
                        .expect("continuation existence checked");
                    if closure.params.len() != args.len() {
                        return Err(RuntimeError::new(format!(
                            "pure continuation `{name}` expected {} arguments, got {}",
                            closure.params.len(),
                            args.len()
                        )));
                    }
                    let values = args
                        .iter()
                        .map(|argument| self.eval(argument))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut child = self.child();
                    child.locals = closure.captured_locals;
                    child.locals.extend(closure.params.into_iter().zip(values));
                    child.eval(&closure.body)
                }
                Expr::Name { name } => {
                    let result_type = self
                        .machine
                        .functions
                        .get(name)
                        .or_else(|| self.program.functions.get(name))
                        .map(|function| &function.result)
                        .ok_or_else(|| RuntimeError::new(format!("unknown callable `{name}`")))?;
                    self.call(name, args, result_type)
                }
                _ => Err(RuntimeError::new(
                    "Uhura only invokes statically resolved callables",
                )),
            },
            Expr::Field { value, field } => {
                let value = self.eval(value)?;
                field_value(&value, field)
            }
            Expr::Index { value, key } => {
                let value = self.eval(value)?;
                let key = self.eval(key)?;
                table_index(&value, &key)
            }
            Expr::Method {
                value,
                method,
                args,
                result_type,
            } => self.eval_method(value, method, args, result_type),
            Expr::If {
                condition,
                then_value,
                else_value,
            } => {
                let (condition, bindings) = self.eval_condition(condition)?;
                let mut child = self.child();
                child.locals.extend(bindings);
                if condition {
                    child.eval(then_value)
                } else {
                    child.eval(else_value)
                }
            }
            Expr::Match { value, arms } => {
                let value = self.eval(value)?;
                self.eval_match(&value, arms)
            }
            Expr::Is { value, pattern } => {
                let value = self.eval(value)?;
                Ok(Value::Bool(match_pattern(
                    pattern,
                    &value,
                    &mut BTreeMap::new(),
                )?))
            }
            Expr::Update { value, fields } => {
                let Value::Record(mut record) = self.eval(value)? else {
                    return Err(RuntimeError::new("record update base is not a record"));
                };
                for (name, expression) in fields {
                    if !record.iter().any(|(field, _)| field == name) {
                        return Err(RuntimeError::new(format!(
                            "record update names unknown field `{name}`"
                        )));
                    }
                    let value = self.eval(expression)?;
                    let (_, current) = record
                        .iter_mut()
                        .find(|(field, _)| field == name)
                        .expect("record field existence checked");
                    *current = value;
                }
                Ok(Value::Record(record))
            }
            Expr::Let { bindings, value } => {
                let mut child = self.child();
                for (name, expression) in bindings {
                    if is_pure_continuation_local(name)
                        && let Expr::Lambda { params, body } = expression
                    {
                        child.pure_continuations.insert(
                            name.clone(),
                            PureContinuationClosure {
                                params: params.clone(),
                                body: body.as_ref().clone(),
                                captured_locals: child.locals.clone(),
                            },
                        );
                    } else {
                        let value = child.eval(expression)?;
                        child.locals.insert(name.clone(), value);
                    }
                }
                child.eval(value)
            }
            Expr::Lambda { .. } => Err(RuntimeError::new(
                "a Uhura lambda cannot escape its collection operation",
            )),
            Expr::Collect { clauses } => {
                let mut values = Vec::new();
                for (condition, value) in clauses {
                    if self.eval_condition(condition)?.0 {
                        values.push(self.eval(value)?);
                    }
                }
                Ok(Value::Seq(values))
            }
            Expr::SetComprehension {
                pattern,
                source,
                conditions,
                value,
                result_type,
            } => {
                let source = finite_values(&self.eval(source)?)?;
                let mut values = Vec::new();
                for item in source {
                    let mut bindings = BTreeMap::new();
                    if !match_pattern(pattern, &item, &mut bindings)? {
                        continue;
                    }
                    let mut child = self.child();
                    child.locals.extend(bindings);
                    let mut accepted = true;
                    for condition in conditions {
                        let (condition, bindings) = child.eval_condition(condition)?;
                        if !condition {
                            accepted = false;
                            break;
                        }
                        child.locals.extend(bindings);
                    }
                    if accepted {
                        values.push(child.eval(value)?);
                    }
                }
                self.canonicalize_checked_value(
                    result_type,
                    Value::Set(values),
                    "set comprehension",
                )
            }
        }
    }

    fn eval_condition(
        &self,
        expression: &Expr,
    ) -> Result<(bool, BTreeMap<String, Value>), RuntimeError> {
        match expression {
            Expr::Is { value, pattern } => {
                let value = self.eval(value)?;
                let mut bindings = BTreeMap::new();
                Ok((match_pattern(pattern, &value, &mut bindings)?, bindings))
            }
            Expr::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => {
                let (left, bindings) = self.eval_condition(left)?;
                if !left {
                    return Ok((false, BTreeMap::new()));
                }
                let mut child = self.child();
                child.locals.extend(bindings.clone());
                let (right, right_bindings) = child.eval_condition(right)?;
                let mut all = bindings;
                all.extend(right_bindings);
                Ok((right, if right { all } else { BTreeMap::new() }))
            }
            Expr::Binary {
                op: BinaryOp::Or,
                left,
                right,
            } => {
                let (left, left_bindings) = self.eval_condition(left)?;
                if left {
                    return Ok((true, left_bindings));
                }
                self.eval_condition(right)
            }
            Expr::Unary {
                op: UnaryOp::Not,
                value,
            } => {
                let (value, _) = self.eval_condition(value)?;
                Ok((!value, BTreeMap::new()))
            }
            _ => match self.eval(expression)? {
                Value::Bool(value) => Ok((value, BTreeMap::new())),
                _ => Err(RuntimeError::new("condition is not Bool")),
            },
        }
    }

    fn canonicalize_checked_value(
        &self,
        expected: &TypeRef,
        value: Value,
        operation: &str,
    ) -> Result<Value, RuntimeError> {
        self.program
            .canonicalize_value(expected, &value)
            .map_err(|error| RuntimeError::new(format!("invalid checked {operation}: {error}")))
    }

    fn eval_match(&self, value: &Value, arms: &[MatchArm]) -> Result<Value, RuntimeError> {
        for arm in arms {
            let mut bindings = BTreeMap::new();
            if match_pattern(&arm.pattern, value, &mut bindings)? {
                let mut child = self.child();
                child.locals.extend(bindings);
                return child.eval(&arm.value);
            }
        }
        Err(RuntimeError::new("checked value match was not exhaustive"))
    }

    fn eval_binary(&self, op: BinaryOp, left: &Expr, right: &Expr) -> Result<Value, RuntimeError> {
        if op == BinaryOp::And {
            let (left, bindings) = self.eval_condition(left)?;
            if !left {
                return Ok(Value::Bool(false));
            }
            let mut child = self.child();
            child.locals.extend(bindings);
            return child.eval(right);
        }
        if op == BinaryOp::Or {
            let (left, bindings) = self.eval_condition(left)?;
            if left {
                return Ok(Value::Bool(true));
            }
            let mut child = self.child();
            child.locals.extend(bindings);
            return child.eval(right);
        }
        let left = self.eval(left)?;
        let right = self.eval(right)?;
        match op {
            BinaryOp::Equal => Ok(Value::Bool(value_equal(&left, &right))),
            BinaryOp::NotEqual => Ok(Value::Bool(!value_equal(&left, &right))),
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply => {
                numeric_binary(op, left, right)
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                compare_values(op, &left, &right)
            }
            BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
        }
    }

    fn lookup(&self, name: &str) -> Result<Value, RuntimeError> {
        if let Some(value) = self.locals.get(name) {
            return Ok(value.clone());
        }
        if let Some(value) = self.state.get(name) {
            return Ok(value.clone());
        }
        if let Value::Record(fields) = self.configuration
            && let Some((_, value)) = fields.iter().find(|(field, _)| field == name)
        {
            return Ok(value.clone());
        }
        if let Some(value) = self.program.constants.get(name) {
            return Ok(value.clone());
        }
        if let Some(value) = self.derives.borrow().get(name) {
            return Ok(value.clone());
        }
        if let Some((_, _, expression, _)) = self
            .machine
            .derives
            .iter()
            .find(|(derive, _, _, _)| derive == name)
        {
            let value = self.eval(expression)?;
            self.derives
                .borrow_mut()
                .insert(name.to_owned(), value.clone());
            return Ok(value);
        }
        Err(RuntimeError::new(format!("unknown value `{name}`")))
    }

    fn call(
        &self,
        function: &str,
        args: &[Expr],
        result_type: &TypeRef,
    ) -> Result<Value, RuntimeError> {
        match function {
            "min" | "max" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new(format!(
                        "`{function}` needs two arguments"
                    )));
                }
                let left = self.eval(&args[0])?;
                let right = self.eval(&args[1])?;
                let order = value_cmp(&left, &right)?;
                Ok(
                    if (function == "min" && order.is_le()) || (function == "max" && order.is_ge())
                    {
                        left
                    } else {
                        right
                    },
                )
            }
            "Int.from" => match self.eval(single_argument(function, args)?)? {
                Value::Boundary(value) => {
                    option_value(result_type, value.integer().map(Value::int))
                }
                _ => Err(RuntimeError::new("Int.from needs BoundaryNumber")),
            },
            "Ratio.from" => match self.eval(single_argument(function, args)?)? {
                Value::Boundary(value) => {
                    option_value(result_type, value.ratio().map(Value::Ratio))
                }
                _ => Err(RuntimeError::new("Ratio.from needs BoundaryNumber")),
            },
            "NonEmpty.from" => match self.eval(single_argument(function, args)?)? {
                Value::Seq(values) => option_value(
                    result_type,
                    (!values.is_empty()).then_some(Value::NonEmpty(values)),
                ),
                _ => Err(RuntimeError::new("NonEmpty.from needs Seq")),
            },
            "Map.from_unique" => match self.eval(single_argument(function, args)?)? {
                Value::Seq(values) => {
                    let pairs = values
                        .into_iter()
                        .map(|value| match value {
                            Value::Tuple(mut values) if values.len() == 2 => {
                                let right = values.pop().expect("two values");
                                let left = values.pop().expect("two values");
                                Ok((left, right))
                            }
                            _ => Err(RuntimeError::new(
                                "Map.from_unique needs a sequence of pairs",
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if has_duplicate_keys(&pairs) {
                        option_value(result_type, None)
                    } else {
                        let map_type = option_inner(result_type, "Map.from_unique")?;
                        let map = self.canonicalize_checked_value(
                            map_type,
                            Value::Map(pairs),
                            "Map.from_unique result",
                        )?;
                        option_value(result_type, Some(map))
                    }
                }
                _ => Err(RuntimeError::new("Map.from_unique needs Seq")),
            },
            "Set.from_unique" => match self.eval(single_argument(function, args)?)? {
                Value::Seq(values) => {
                    let unique = !has_duplicates(&values);
                    if !unique {
                        option_value(result_type, None)
                    } else {
                        let set_type = option_inner(result_type, "Set.from_unique")?;
                        let set = self.canonicalize_checked_value(
                            set_type,
                            Value::Set(values),
                            "Set.from_unique result",
                        )?;
                        option_value(result_type, Some(set))
                    }
                }
                _ => Err(RuntimeError::new("Set.from_unique needs Seq")),
            },
            "__coerce_nat" => {
                let value = self.eval(single_argument(function, args)?)?;
                let integer = integer_value(&value)
                    .ok_or_else(|| RuntimeError::new("Nat coercion needs integer"))?;
                Value::nat(integer).map_err(value_error)
            }
            "__coerce_positive" => {
                let value = self.eval(single_argument(function, args)?)?;
                let integer = integer_value(&value)
                    .ok_or_else(|| RuntimeError::new("PositiveInt coercion needs integer"))?;
                Value::positive(integer).map_err(value_error)
            }
            "__coerce_int" => {
                let value = self.eval(single_argument(function, args)?)?;
                let integer = integer_value(&value)
                    .ok_or_else(|| RuntimeError::new("Int widening needs integer"))?;
                Ok(Value::int(integer))
            }
            _ => {
                let function = self
                    .machine
                    .functions
                    .get(function)
                    .or_else(|| self.program.functions.get(function))
                    .ok_or_else(|| RuntimeError::new(format!("unknown function `{function}`")))?;
                self.call_function(function, args)
            }
        }
    }

    fn call_function(&self, function: &Function, args: &[Expr]) -> Result<Value, RuntimeError> {
        if function.params.len() != args.len() {
            return Err(RuntimeError::new(format!(
                "function `{}` expected {} arguments, got {}",
                function.id,
                function.params.len(),
                args.len()
            ))
            .at(&function.source));
        }
        if self.call_stack.borrow().contains(&function.id) {
            return Err(RuntimeError::new(format!(
                "recursive function call `{}` reached the runtime",
                function.id
            ))
            .at(&function.source));
        }
        let values = args
            .iter()
            .map(|argument| self.eval(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let mut child = self.child();
        child.locals = function
            .params
            .iter()
            .map(|(name, _)| name.clone())
            .zip(values)
            .collect();
        child.call_stack.borrow_mut().push(function.id.clone());
        child.eval(&function.body)
    }

    fn eval_method(
        &self,
        receiver: &Expr,
        method: &str,
        args: &[Expr],
        result_type: &TypeRef,
    ) -> Result<Value, RuntimeError> {
        let value = self.eval(receiver)?;
        match method {
            "is_empty" => Ok(Value::Bool(match &value {
                Value::Seq(values) | Value::NonEmpty(values) | Value::Set(values) => {
                    values.is_empty()
                }
                Value::Map(entries) => entries.is_empty(),
                Value::Text(text) => text.is_empty(),
                _ => return Err(RuntimeError::new("is_empty receiver is unsupported")),
            })),
            "size" => {
                let size = match &value {
                    Value::Seq(values) | Value::NonEmpty(values) | Value::Set(values) => {
                        values.len()
                    }
                    Value::Map(entries) => entries.len(),
                    _ => return Err(RuntimeError::new("size receiver is unsupported")),
                };
                Value::nat(size).map_err(value_error)
            }
            "unique" => match &value {
                Value::Seq(values) => Ok(Value::Bool(!has_duplicates(values))),
                _ => Err(RuntimeError::new("unique receiver is not Seq")),
            },
            "contains" => {
                let needle = self.eval(single_argument(method, args)?)?;
                match &value {
                    Value::Seq(values) | Value::NonEmpty(values) | Value::Set(values) => Ok(
                        Value::Bool(values.iter().any(|value| value_equal(value, &needle))),
                    ),
                    _ => Err(RuntimeError::new("contains receiver is unsupported")),
                }
            }
            "append" => {
                let item = self.eval(single_argument(method, args)?)?;
                match value {
                    Value::Seq(mut values) => {
                        values.push(item);
                        Ok(Value::Seq(values))
                    }
                    _ => Err(RuntimeError::new("append receiver is not Seq")),
                }
            }
            "without" => {
                let item = self.eval(single_argument(method, args)?)?;
                match value {
                    Value::Seq(mut values) => {
                        values.retain(|value| !value_equal(value, &item));
                        Ok(Value::Seq(values))
                    }
                    _ => Err(RuntimeError::new("without receiver is not Seq")),
                }
            }
            "uncons" => match value {
                Value::Seq(values) if values.is_empty() => option_value(result_type, None),
                Value::Seq(mut values) => {
                    let head = values.remove(0);
                    option_value(
                        result_type,
                        Some(Value::Record(vec![
                            ("head".into(), head),
                            ("tail".into(), Value::Seq(values)),
                        ])),
                    )
                }
                _ => Err(RuntimeError::new("uncons receiver is not Seq")),
            },
            "from_options" => match value {
                Value::Seq(values) => {
                    let mut output = Vec::new();
                    for value in values {
                        if let Some(value) = option_parts(value)? {
                            output.push(value);
                        }
                    }
                    self.canonicalize_checked_value(
                        result_type,
                        Value::Seq(output),
                        "Seq.from_options result",
                    )
                }
                _ => Err(RuntimeError::new("from_options receiver is not Seq")),
            },
            "get" => {
                let key = self.eval(single_argument(method, args)?)?;
                match &value {
                    Value::Map(entries) => option_value(
                        result_type,
                        entries
                            .iter()
                            .find(|(entry, _)| value_equal(entry, &key))
                            .map(|(_, value)| value.clone()),
                    ),
                    _ => Err(RuntimeError::new("get receiver is not Map")),
                }
            }
            "put" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("put needs key and value"));
                }
                let key = self.eval(&args[0])?;
                let next = self.eval(&args[1])?;
                match value {
                    Value::Map(mut entries) => {
                        if let Some((_, value)) = entries
                            .iter_mut()
                            .find(|(entry, _)| value_equal(entry, &key))
                        {
                            *value = next;
                        } else {
                            entries.push((key, next));
                        }
                        self.canonicalize_checked_value(
                            result_type,
                            Value::Map(entries),
                            "Map.put result",
                        )
                    }
                    _ => Err(RuntimeError::new("put receiver is not Map")),
                }
            }
            "remove" => {
                let target = self.eval(single_argument(method, args)?)?;
                match value {
                    Value::Map(mut entries) => {
                        entries.retain(|(key, _)| !value_equal(key, &target));
                        self.canonicalize_checked_value(
                            result_type,
                            Value::Map(entries),
                            "Map.remove result",
                        )
                    }
                    Value::Set(mut values) => {
                        values.retain(|value| !value_equal(value, &target));
                        self.canonicalize_checked_value(
                            result_type,
                            Value::Set(values),
                            "Set.remove result",
                        )
                    }
                    _ => Err(RuntimeError::new("remove receiver is unsupported")),
                }
            }
            "add" => {
                let item = self.eval(single_argument(method, args)?)?;
                match value {
                    Value::Set(mut values) => {
                        if !values.iter().any(|existing| value_equal(existing, &item)) {
                            values.push(item);
                        }
                        self.canonicalize_checked_value(
                            result_type,
                            Value::Set(values),
                            "Set.add result",
                        )
                    }
                    _ => Err(RuntimeError::new("add receiver is not Set")),
                }
            }
            "set" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("table.set needs key and value"));
                }
                let key = constructor_or_key_name(&self.eval(&args[0])?)?;
                let next = self.eval(&args[1])?;
                match value {
                    Value::Table {
                        key_type,
                        mut entries,
                    } => {
                        let (_, value) = entries
                            .iter_mut()
                            .find(|(name, _)| name == &key)
                            .ok_or_else(|| RuntimeError::new("unknown Table key"))?;
                        *value = next;
                        Ok(Value::Table { key_type, entries })
                    }
                    _ => Err(RuntimeError::new("set receiver is not Table")),
                }
            }
            "entries" => match value {
                Value::Map(entries) => {
                    let records = matches!(
                        result_type,
                        TypeRef::FiniteView { value }
                            if matches!(value.as_ref(), TypeRef::Record { .. })
                    );
                    Ok(Value::Seq(
                        entries
                            .into_iter()
                            .map(|(key, value)| {
                                if records {
                                    Value::Record(vec![
                                        ("key".into(), key),
                                        ("value".into(), value),
                                    ])
                                } else {
                                    Value::Tuple(vec![key, value])
                                }
                            })
                            .collect(),
                    ))
                }
                _ => Err(RuntimeError::new("entries receiver is not Map")),
            },
            "entries_by_key" => match value {
                Value::Map(entries) => Ok(Value::Seq(
                    entries
                        .into_iter()
                        .map(|(key, value)| Value::Tuple(vec![key, value]))
                        .collect(),
                )),
                _ => Err(RuntimeError::new("entries_by_key receiver is not Map")),
            },
            "values" => match value {
                Value::Map(entries) => Ok(Value::Seq(
                    entries.into_iter().map(|(_, value)| value).collect(),
                )),
                Value::Table { entries, .. } => Ok(Value::Seq(
                    entries.into_iter().map(|(_, value)| value).collect(),
                )),
                _ => Err(RuntimeError::new("values receiver is unsupported")),
            },
            "all" | "any" | "count" | "map" | "filter" | "try_map" | "try_map_values"
            | "filter_map" => self.collection_function(value, method, args, result_type),
            _ => Err(RuntimeError::new(format!(
                "unsupported total method `{method}`"
            ))),
        }
    }

    fn collection_function(
        &self,
        value: Value,
        method: &str,
        args: &[Expr],
        result_type: &TypeRef,
    ) -> Result<Value, RuntimeError> {
        let lambda = match single_argument(method, args)? {
            Expr::Lambda { params, body } => (params, body.as_ref()),
            _ => {
                return Err(RuntimeError::new(format!(
                    "`{method}` requires a pure lambda"
                )));
            }
        };
        let map_entries_are_records = lambda.0.len() == 1;
        let items = match value {
            Value::Seq(values) | Value::NonEmpty(values) | Value::Set(values) => values,
            Value::Map(entries) => entries
                .into_iter()
                .map(|(key, value)| {
                    if map_entries_are_records {
                        Value::Record(vec![("key".into(), key), ("value".into(), value)])
                    } else {
                        Value::Tuple(vec![key, value])
                    }
                })
                .collect(),
            _ => {
                return Err(RuntimeError::new(format!(
                    "`{method}` receiver is not a finite collection"
                )));
            }
        };
        match method {
            "all" => {
                for item in items {
                    if !lambda_bool(self, lambda, item)? {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            "any" => {
                for item in items {
                    if lambda_bool(self, lambda, item)? {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            "count" => {
                let mut count = 0usize;
                for item in items {
                    if lambda_bool(self, lambda, item)? {
                        count += 1;
                    }
                }
                Value::nat(count).map_err(value_error)
            }
            "map" => Ok(Value::Seq(
                items
                    .into_iter()
                    .map(|item| lambda_value(self, lambda, item))
                    .collect::<Result<_, _>>()?,
            )),
            "filter" => {
                let mut output = Vec::new();
                for item in items {
                    if lambda_bool(self, lambda, item.clone())? {
                        output.push(item);
                    }
                }
                self.canonicalize_checked_value(
                    result_type,
                    Value::Seq(output),
                    "Seq.filter result",
                )
            }
            "try_map" => {
                let mut output = Vec::new();
                for item in items {
                    match option_parts(lambda_value(self, lambda, item)?)? {
                        None => return option_value(result_type, None),
                        Some(value) => output.push(value),
                    }
                }
                option_value(result_type, Some(Value::Seq(output)))
            }
            "try_map_values" => {
                let mut output = Vec::new();
                for item in items {
                    let key = match &item {
                        Value::Record(entry) => entry
                            .iter()
                            .find_map(|(field, value)| (field == "key").then(|| value.clone()))
                            .ok_or_else(|| {
                                RuntimeError::new("try_map_values internal Entry has no key")
                            })?,
                        Value::Tuple(pair) if pair.len() == 2 => pair[0].clone(),
                        _ => {
                            return Err(RuntimeError::new(
                                "try_map_values internal entry has the wrong shape",
                            ));
                        }
                    };
                    match option_parts(lambda_value(self, lambda, item)?)? {
                        None => return option_value(result_type, None),
                        Some(value) => output.push((key, value)),
                    }
                }
                let map_type = option_inner(result_type, "try_map_values")?;
                let map = self.canonicalize_checked_value(
                    map_type,
                    Value::Map(output),
                    "try_map_values result",
                )?;
                option_value(result_type, Some(map))
            }
            "filter_map" => {
                let mut output = Vec::new();
                for item in items {
                    if let Some(value) = option_parts(lambda_value(self, lambda, item)?)?
                        && !output.iter().any(|existing| value_equal(existing, &value))
                    {
                        output.push(value);
                    }
                }
                self.canonicalize_checked_value(
                    result_type,
                    Value::Set(output),
                    "Set.filter_map result",
                )
            }
            _ => unreachable!("caller filtered methods"),
        }
    }
}

fn single_argument<'a>(name: &str, args: &'a [Expr]) -> Result<&'a Expr, RuntimeError> {
    if args.len() == 1 {
        Ok(&args[0])
    } else {
        Err(RuntimeError::new(format!(
            "`{name}` requires exactly one argument"
        )))
    }
}

fn lambda_value(
    parent: &EvalContext<'_>,
    lambda: (&Vec<String>, &Expr),
    item: Value,
) -> Result<Value, RuntimeError> {
    let (params, body) = lambda;
    let values = if params.len() == 1 {
        vec![item]
    } else {
        match item {
            Value::Tuple(values) if values.len() == params.len() => values,
            _ => return Err(RuntimeError::new("lambda parameter shape mismatch")),
        }
    };
    let mut child = parent.child();
    child.locals.extend(params.iter().cloned().zip(values));
    child.eval(body)
}

fn lambda_bool(
    parent: &EvalContext<'_>,
    lambda: (&Vec<String>, &Expr),
    item: Value,
) -> Result<bool, RuntimeError> {
    match lambda_value(parent, lambda, item)? {
        Value::Bool(value) => Ok(value),
        _ => Err(RuntimeError::new(
            "collection predicate did not return Bool",
        )),
    }
}

fn field_value(value: &Value, field: &str) -> Result<Value, RuntimeError> {
    match value {
        Value::Record(fields) => fields
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| value.clone())
            .ok_or_else(|| RuntimeError::new(format!("record has no field `{field}`"))),
        Value::Key { value, .. } if field == "value" => Ok(value.as_ref().clone()),
        Value::Variant { fields, .. } => fields
            .iter()
            .find(|(name, _)| name.as_deref() == Some(field))
            .map(|(_, value)| value.clone())
            .ok_or_else(|| RuntimeError::new(format!("variant has no field `{field}`"))),
        Value::Seq(values) | Value::NonEmpty(values) | Value::Set(values) if field == "size" => {
            Value::nat(values.len()).map_err(value_error)
        }
        Value::Map(entries) if field == "size" => Value::nat(entries.len()).map_err(value_error),
        Value::Seq(values) if field == "is_empty" => Ok(Value::Bool(values.is_empty())),
        Value::Map(entries) if field == "is_empty" => Ok(Value::Bool(entries.is_empty())),
        Value::Text(text) if field == "is_empty" => Ok(Value::Bool(text.is_empty())),
        _ => Err(RuntimeError::new(format!(
            "value has no total field `{field}`"
        ))),
    }
}

fn table_index(value: &Value, key: &Value) -> Result<Value, RuntimeError> {
    let key = constructor_or_key_name(key)?;
    match value {
        Value::Table { entries, .. } => entries
            .iter()
            .find(|(name, _)| name == &key)
            .map(|(_, value)| value.clone())
            .ok_or_else(|| RuntimeError::new(format!("Table has no key `{key}`"))),
        _ => Err(RuntimeError::new(
            "only total Table values support indexing",
        )),
    }
}

fn constructor_or_key_name(value: &Value) -> Result<String, RuntimeError> {
    match value {
        Value::Variant {
            constructor,
            fields,
            ..
        } if fields.is_empty() => Ok(constructor.clone()),
        Value::Key { value, .. } => match value.as_ref() {
            Value::Text(value) => Ok(value.clone()),
            other => Ok(hex(&hash("table-key", &[other.canonical_bytes()]))),
        },
        _ => Err(RuntimeError::new("value is not a closed Table key")),
    }
}

fn numeric_binary(op: BinaryOp, left: Value, right: Value) -> Result<Value, RuntimeError> {
    match (left, right) {
        (Value::Integer { value: left, .. }, Value::Integer { value: right, .. }) => {
            Ok(Value::int(match op {
                BinaryOp::Add => left + right,
                BinaryOp::Subtract => left - right,
                BinaryOp::Multiply => left * right,
                _ => unreachable!("numeric operation"),
            }))
        }
        (Value::Decimal(left), Value::Decimal(right)) => Ok(Value::Decimal(match op {
            BinaryOp::Add => left.add(&right),
            BinaryOp::Subtract => left.subtract(&right),
            BinaryOp::Multiply => left.multiply(&right),
            _ => unreachable!("numeric operation"),
        })),
        (Value::Ratio(left), Value::Ratio(right)) => {
            let result = match op {
                BinaryOp::Add => left.add(&right),
                BinaryOp::Subtract => left.subtract(&right),
                BinaryOp::Multiply => left.multiply(&right),
                _ => unreachable!("numeric operation"),
            };
            Value::ratio(result).map_err(value_error)
        }
        _ => Err(RuntimeError::new(
            "numeric operands require one exact numeric family",
        )),
    }
}

fn compare_values(op: BinaryOp, left: &Value, right: &Value) -> Result<Value, RuntimeError> {
    let order = value_cmp(left, right)?;
    Ok(Value::Bool(match op {
        BinaryOp::Less => order.is_lt(),
        BinaryOp::LessEqual => order.is_le(),
        BinaryOp::Greater => order.is_gt(),
        BinaryOp::GreaterEqual => order.is_ge(),
        _ => unreachable!("comparison operation"),
    }))
}

fn value_cmp(left: &Value, right: &Value) -> Result<std::cmp::Ordering, RuntimeError> {
    match (left, right) {
        (Value::Integer { value: left, .. }, Value::Integer { value: right, .. }) => {
            Ok(left.cmp(right))
        }
        (Value::Decimal(left), Value::Decimal(right))
        | (Value::Ratio(left), Value::Ratio(right)) => Ok(left.cmp(right)),
        (Value::Text(left), Value::Text(right)) => Ok(left.cmp(right)),
        _ => Err(RuntimeError::new(
            "Uhura ordering requires compatible ordered scalar values",
        )),
    }
}

fn value_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Integer { value: left, .. }, Value::Integer { value: right, .. }) => left == right,
        _ => left == right,
    }
}

fn integer_value(value: &Value) -> Option<BigInt> {
    match value {
        Value::Integer { value, .. } => Some(value.clone()),
        _ => None,
    }
}

fn option_value(result_type: &TypeRef, value: Option<Value>) -> Result<Value, RuntimeError> {
    if !matches!(result_type, TypeRef::Option { .. }) {
        return Err(RuntimeError::new(format!(
            "checked option primitive has non-Option result type `{}`",
            result_type.canonical_name()
        )));
    }
    let type_id = result_type.canonical_name();
    Ok(match value {
        Some(value) => Value::variant(type_id, "some", vec![(Some("value".into()), value)]),
        None => Value::variant(type_id, "none", Vec::new()),
    })
}

fn option_parts(value: Value) -> Result<Option<Value>, RuntimeError> {
    match value {
        Value::Variant {
            constructor,
            fields,
            ..
        } if constructor == "none" && fields.is_empty() => Ok(None),
        Value::Variant {
            constructor,
            mut fields,
            ..
        } if constructor == "some" && fields.len() == 1 => Ok(Some(fields.remove(0).1)),
        _ => Err(RuntimeError::new("expected Option value")),
    }
}

fn option_inner<'a>(
    result_type: &'a TypeRef,
    operation: &str,
) -> Result<&'a TypeRef, RuntimeError> {
    match result_type {
        TypeRef::Option { value } => Ok(value),
        _ => Err(RuntimeError::new(format!(
            "checked `{operation}` result is not Option"
        ))),
    }
}

pub(super) fn finite_values(value: &Value) -> Result<Vec<Value>, RuntimeError> {
    match value {
        Value::Seq(values) | Value::NonEmpty(values) | Value::Set(values) => Ok(values.clone()),
        Value::Map(entries) => Ok(entries
            .iter()
            .map(|(key, value)| Value::Tuple(vec![key.clone(), value.clone()]))
            .collect()),
        _ => Err(RuntimeError::new("expected a finite collection view")),
    }
}

fn has_duplicates(values: &[Value]) -> bool {
    values.iter().enumerate().any(|(index, value)| {
        values[index + 1..]
            .iter()
            .any(|next| value_equal(value, next))
    })
}

fn has_duplicate_keys(entries: &[(Value, Value)]) -> bool {
    entries.iter().enumerate().any(|(index, (key, _))| {
        entries[index + 1..]
            .iter()
            .any(|(next, _)| value_equal(key, next))
    })
}

fn value_error(error: ValueError) -> RuntimeError {
    RuntimeError::new(error.0)
}

pub(super) fn match_pattern(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut BTreeMap<String, Value>,
) -> Result<bool, RuntimeError> {
    match pattern {
        Pattern::Ignore => Ok(true),
        Pattern::Bind { name } => {
            bindings.insert(name.clone(), value.clone());
            Ok(true)
        }
        Pattern::Literal { value: expected } => Ok(value_equal(expected, value)),
        Pattern::Constructor {
            type_id,
            constructor,
            fields,
        } => {
            let Value::Variant {
                type_id: actual_type,
                constructor: actual_constructor,
                fields: actual_fields,
            } = value
            else {
                return Ok(false);
            };
            if type_id != actual_type
                || constructor != actual_constructor
                || fields.len() != actual_fields.len()
            {
                return Ok(false);
            }
            for (pattern, (_, value)) in fields.iter().zip(actual_fields) {
                if !match_pattern(pattern, value, bindings)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Tuple { values } => {
            let Value::Tuple(actual) = value else {
                return Ok(false);
            };
            if values.len() != actual.len() {
                return Ok(false);
            }
            for (pattern, value) in values.iter().zip(actual) {
                if !match_pattern(pattern, value, bindings)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Record { fields, rest } => {
            let Value::Record(actual) = value else {
                return Ok(false);
            };
            if !rest && fields.len() != actual.len() {
                return Ok(false);
            }
            for (name, pattern) in fields {
                let Some((_, value)) = actual.iter().find(|(field, _)| field == name) else {
                    return Ok(false);
                };
                if !match_pattern(pattern, value, bindings)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Pattern::Alternative { patterns } => {
            for pattern in patterns {
                let mut candidate = bindings.clone();
                if match_pattern(pattern, value, &mut candidate)? {
                    *bindings = candidate;
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

pub(super) fn evaluate_with_locals(
    program: &Program,
    machine: &Machine,
    configuration: &Value,
    state: &BTreeMap<String, Value>,
    locals: BTreeMap<String, Value>,
    expression: &Expr,
) -> Result<Value, RuntimeError> {
    let mut context = EvalContext::new(program, machine, configuration, state);
    context.locals = locals;
    context.eval(expression)
}

pub(super) fn evaluate_condition_with_locals(
    program: &Program,
    machine: &Machine,
    configuration: &Value,
    state: &BTreeMap<String, Value>,
    locals: BTreeMap<String, Value>,
    expression: &Expr,
) -> Result<(bool, BTreeMap<String, Value>), RuntimeError> {
    let mut context = EvalContext::new(program, machine, configuration, state);
    context.locals = locals;
    context.eval_condition(expression)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ConstructorDef, ObservationField, OutcomeDef, StateField, TypeDef, TypeRef};

    fn source(id: &str) -> SourceRef {
        SourceRef::synthetic(id)
    }

    fn counter_program() -> Program {
        let mut program = Program::new();
        let machine_id = "example.counter@1::Counter".to_string();
        let input_type = format!("{machine_id}.Input");
        let outcome_type = format!("{machine_id}.Outcome");
        let machine = Machine {
            id: machine_id.clone(),
            config: TypeRef::Record {
                fields: vec![
                    ("minimum".into(), TypeRef::Int),
                    ("maximum".into(), TypeRef::Int),
                    ("initial".into(), TypeRef::Int),
                ],
            },
            requires: vec![(
                Expr::Binary {
                    op: BinaryOp::And,
                    left: Box::new(Expr::Binary {
                        op: BinaryOp::LessEqual,
                        left: Box::new(Expr::Name {
                            name: "minimum".into(),
                        }),
                        right: Box::new(Expr::Name {
                            name: "initial".into(),
                        }),
                    }),
                    right: Box::new(Expr::Binary {
                        op: BinaryOp::LessEqual,
                        left: Box::new(Expr::Name {
                            name: "initial".into(),
                        }),
                        right: Box::new(Expr::Name {
                            name: "maximum".into(),
                        }),
                    }),
                },
                source("require"),
            )],
            ports: Vec::new(),
            local_input: TypeDef::Sum {
                id: input_type.clone(),
                constructors: vec![ConstructorDef {
                    name: "increment".into(),
                    fields: Vec::new(),
                }],
            },
            local_commands: Vec::new(),
            outcomes: vec![OutcomeDef {
                constructor: ConstructorDef {
                    name: "accepted".into(),
                    fields: Vec::new(),
                },
                policy: OutcomePolicy::Commit,
                source: source("accepted"),
            }],
            state: vec![StateField {
                name: "count".into(),
                ty: TypeRef::Int,
                initial: Expr::Name {
                    name: "initial".into(),
                },
                source: source("count"),
            }],
            functions: BTreeMap::new(),
            derives: Vec::new(),
            invariants: vec![(
                Expr::Binary {
                    op: BinaryOp::GreaterEqual,
                    left: Box::new(Expr::Name {
                        name: "count".into(),
                    }),
                    right: Box::new(Expr::Name {
                        name: "minimum".into(),
                    }),
                },
                source("count-at-least-minimum"),
            )],
            observation: vec![ObservationField {
                name: "count".into(),
                ty: TypeRef::Int,
                expression: Expr::Name {
                    name: "count".into(),
                },
                source: source("observe-count"),
            }],
            transitions: BTreeMap::new(),
            handlers: BTreeMap::from([(
                "increment".into(),
                crate::ir::Handler {
                    input: "increment".into(),
                    pattern: Pattern::Constructor {
                        type_id: input_type.clone(),
                        constructor: "increment".into(),
                        fields: Vec::new(),
                    },
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
                            source: source("increment-count"),
                        },
                        Statement::Finish {
                            outcome: Expr::Constructor {
                                type_id: outcome_type.clone(),
                                constructor: "accepted".into(),
                                fields: Vec::new(),
                            },
                            source: source("finish"),
                        },
                    ],
                    source: source("increment"),
                },
            )]),
            before_commit: Vec::new(),
            source: source("counter"),
        };
        program.machines.insert(machine_id, machine);
        program.freeze_program_hashes();
        program
    }

    fn counter_config() -> Value {
        Value::Record(vec![
            ("minimum".into(), Value::int(0)),
            ("maximum".into(), Value::int(10)),
            ("initial".into(), Value::int(1)),
        ])
    }

    fn increment_input() -> Value {
        Value::variant("example.counter@1::Counter.Input", "increment", Vec::new())
    }

    #[test]
    fn boundary_finite_constructor_evaluates_a_nonliteral_decimal_exactly() {
        use std::str::FromStr as _;

        let program = counter_program();
        let machine = program
            .machines
            .get("example.counter@1::Counter")
            .expect("counter machine");
        let expression = Expr::Constructor {
            type_id: TypeRef::BoundaryNumber.canonical_name(),
            constructor: "finite".into(),
            fields: vec![(
                Some("value".into()),
                Expr::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Name {
                        name: "base".into(),
                    }),
                    right: Box::new(Expr::Literal {
                        value: Value::Decimal(Decimal::from_str("0.20").unwrap()),
                    }),
                },
            )],
        };
        let value = evaluate_with_locals(
            &program,
            machine,
            &counter_config(),
            &BTreeMap::new(),
            BTreeMap::from([(
                "base".into(),
                Value::Decimal(Decimal::from_str("0.10").unwrap()),
            )]),
            &expression,
        )
        .unwrap();

        assert_eq!(
            value,
            Value::Boundary(BoundaryNumber::Finite(Decimal::from_str("0.30").unwrap()))
        );
    }

    fn collection_program() -> (Program, TypeRef, TypeRef) {
        let mut program = Program::new();
        let machine_id = "example.collections@1::Collections".to_string();
        let input_type = format!("{machine_id}.Input");
        let outcome_type = format!("{machine_id}.Outcome");
        let sequence_type = TypeRef::Seq {
            value: Box::new(TypeRef::Int),
        };
        let set_type = TypeRef::Set {
            value: Box::new(sequence_type.clone()),
        };
        let map_type = TypeRef::Map {
            key: Box::new(sequence_type),
            value: Box::new(TypeRef::Int),
        };
        let nested_values = || Expr::Seq {
            values: vec![
                Expr::Seq {
                    values: vec![Expr::Literal {
                        value: Value::int(0),
                    }],
                },
                Expr::Seq { values: Vec::new() },
            ],
        };
        program.machines.insert(
            machine_id.clone(),
            Machine {
                id: machine_id.clone(),
                config: TypeRef::Unit,
                requires: Vec::new(),
                ports: Vec::new(),
                local_input: TypeDef::Sum {
                    id: input_type.clone(),
                    constructors: vec![ConstructorDef {
                        name: "touch".into(),
                        fields: Vec::new(),
                    }],
                },
                local_commands: Vec::new(),
                outcomes: vec![OutcomeDef {
                    constructor: ConstructorDef {
                        name: "done".into(),
                        fields: Vec::new(),
                    },
                    policy: OutcomePolicy::Commit,
                    source: source("done"),
                }],
                state: vec![
                    StateField {
                        name: "sets".into(),
                        ty: set_type.clone(),
                        initial: Expr::SetComprehension {
                            pattern: Pattern::Bind {
                                name: "item".into(),
                            },
                            source: Box::new(nested_values()),
                            conditions: Vec::new(),
                            value: Box::new(Expr::Name {
                                name: "item".into(),
                            }),
                            result_type: set_type.clone(),
                        },
                        source: source("sets"),
                    },
                    StateField {
                        name: "maps".into(),
                        ty: map_type.clone(),
                        initial: Expr::Map {
                            entries: vec![
                                (
                                    Expr::Seq {
                                        values: vec![Expr::Literal {
                                            value: Value::int(0),
                                        }],
                                    },
                                    Expr::Literal {
                                        value: Value::int(1),
                                    },
                                ),
                                (
                                    Expr::Seq { values: Vec::new() },
                                    Expr::Literal {
                                        value: Value::int(2),
                                    },
                                ),
                            ],
                            result_type: map_type.clone(),
                        },
                        source: source("maps"),
                    },
                ],
                functions: BTreeMap::new(),
                derives: Vec::new(),
                invariants: Vec::new(),
                observation: vec![
                    ObservationField {
                        name: "sets".into(),
                        ty: set_type.clone(),
                        expression: Expr::Name {
                            name: "sets".into(),
                        },
                        source: source("observe-sets"),
                    },
                    ObservationField {
                        name: "maps".into(),
                        ty: map_type.clone(),
                        expression: Expr::Name {
                            name: "maps".into(),
                        },
                        source: source("observe-maps"),
                    },
                ],
                transitions: BTreeMap::new(),
                handlers: BTreeMap::from([(
                    "touch".into(),
                    crate::ir::Handler {
                        input: "touch".into(),
                        pattern: Pattern::Constructor {
                            type_id: input_type,
                            constructor: "touch".into(),
                            fields: Vec::new(),
                        },
                        body: vec![
                            Statement::Set {
                                field: "sets".into(),
                                value: Expr::Method {
                                    value: Box::new(Expr::Name {
                                        name: "sets".into(),
                                    }),
                                    method: "add".into(),
                                    args: vec![Expr::Seq {
                                        values: vec![Expr::Literal {
                                            value: Value::int(2),
                                        }],
                                    }],
                                    result_type: set_type.clone(),
                                },
                                source: source("add-set"),
                            },
                            Statement::Set {
                                field: "maps".into(),
                                value: Expr::Method {
                                    value: Box::new(Expr::Name {
                                        name: "maps".into(),
                                    }),
                                    method: "put".into(),
                                    args: vec![
                                        Expr::Seq {
                                            values: vec![Expr::Literal {
                                                value: Value::int(2),
                                            }],
                                        },
                                        Expr::Literal {
                                            value: Value::int(3),
                                        },
                                    ],
                                    result_type: map_type.clone(),
                                },
                                source: source("put-map"),
                            },
                            Statement::Finish {
                                outcome: Expr::Constructor {
                                    type_id: outcome_type,
                                    constructor: "done".into(),
                                    fields: Vec::new(),
                                },
                                source: source("finish-collections"),
                            },
                        ],
                        source: source("touch"),
                    },
                )]),
                before_commit: Vec::new(),
                source: source("collections"),
            },
        );
        program.freeze_program_hashes();
        (program, set_type, map_type)
    }

    #[test]
    fn commit_publishes_state_and_abort_would_not() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";
        let config = Value::Record(vec![
            ("minimum".into(), Value::int(0)),
            ("maximum".into(), Value::int(10)),
            ("initial".into(), Value::int(1)),
        ]);
        let (instance, genesis) = program.admit(machine, config, "test/1").unwrap();
        assert_eq!(genesis.sequence, 0);
        let step = program
            .react(
                &instance,
                Value::variant(format!("{machine}.Input"), "increment", Vec::new()),
            )
            .unwrap();
        assert_eq!(
            step.instance.state,
            Value::Record(vec![("count".into(), Value::int(2))])
        );
        assert_eq!(step.receipt.sequence, 1);
    }

    #[test]
    fn in_place_reaction_preserves_the_immutable_reaction_contract() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";
        let (initial, _) = program
            .admit(machine, counter_config(), "test/react-mut")
            .unwrap();
        let history = program.react(&initial, increment_input()).unwrap().instance;

        let expected = program.react(&history, increment_input()).unwrap();
        let mut actual = history;
        let actual_receipt = program.react_mut(&mut actual, increment_input()).unwrap();

        assert_eq!(actual_receipt, expected.receipt);
        assert_eq!(actual, expected.instance);
        assert_eq!(actual.receipts.len(), 2);
        assert_eq!(
            program
                .canonical_reaction_receipt_bytes(machine, &actual_receipt)
                .unwrap(),
            program
                .canonical_reaction_receipt_bytes(machine, &expected.receipt)
                .unwrap(),
        );
    }

    #[test]
    fn while_back_edge_observes_a_final_state_mutation() {
        let mut program = counter_program();
        let machine_id = "example.counter@1::Counter";
        let machine = program.machines.get_mut(machine_id).unwrap();
        machine.before_commit = vec![Statement::While {
            condition: Expr::Binary {
                op: BinaryOp::Greater,
                left: Box::new(Expr::Name {
                    name: "count".into(),
                }),
                right: Box::new(Expr::Literal {
                    value: Value::int(0),
                }),
            },
            decreases: Expr::Name {
                name: "count".into(),
            },
            body: vec![Statement::Set {
                field: "count".into(),
                value: Expr::Binary {
                    op: BinaryOp::Subtract,
                    left: Box::new(Expr::Name {
                        name: "count".into(),
                    }),
                    right: Box::new(Expr::Literal {
                        value: Value::int(1),
                    }),
                },
                source: source("decrement-count"),
            }],
            break_local: None,
            source: source("drain-count"),
        }];
        program.freeze_program_hashes();

        let (instance, _) = program
            .admit(machine_id, counter_config(), "test/while-back-edge")
            .unwrap();
        let step = program.react(&instance, increment_input()).unwrap();

        assert_eq!(
            step.instance.state,
            Value::Record(vec![("count".into(), Value::int(0))])
        );
    }

    #[test]
    fn checkpoint_restore_preserves_identity_and_replay_bytes() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";
        let config = Value::Record(vec![
            ("minimum".into(), Value::int(0)),
            ("maximum".into(), Value::int(10)),
            ("initial".into(), Value::int(1)),
        ]);
        let (instance, _) = program.admit(machine, config, "test/1").unwrap();
        let checkpoint = program.checkpoint(&instance);
        let restored = program.restore(&checkpoint).unwrap();
        let input = Value::variant(format!("{machine}.Input"), "increment", Vec::new());
        let first = program.react(&instance, input.clone()).unwrap();
        let second = program.react(&restored, input).unwrap();
        assert_eq!(
            program
                .canonical_reaction_receipt_bytes(machine, &first.receipt)
                .unwrap(),
            program
                .canonical_reaction_receipt_bytes(machine, &second.receipt)
                .unwrap()
        );
        assert_eq!(
            program.canonical_checkpoint_bytes(&checkpoint).unwrap(),
            program
                .canonical_checkpoint_bytes(&program.checkpoint(&restored))
                .unwrap(),
        );
    }

    #[test]
    fn exhausted_sequence_rejects_reaction_without_a_receipt() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";
        let config = Value::Record(vec![
            ("minimum".into(), Value::int(0)),
            ("maximum".into(), Value::int(10)),
            ("initial".into(), Value::int(1)),
        ]);
        let (mut instance, _) = program.admit(machine, config, "test/exhausted").unwrap();
        instance.next_sequence = u64::MAX;
        let before = instance.clone();

        let error = program
            .react(
                &instance,
                Value::variant(format!("{machine}.Input"), "increment", Vec::new()),
            )
            .unwrap_err();

        assert!(error.message.contains("sequence capacity is exhausted"));
        assert_eq!(instance, before);
        assert!(instance.receipts.is_empty());
    }

    #[test]
    fn checked_generic_result_identity_survives_empty_collections() {
        let program = counter_program();
        let machine = program.machines.get("example.counter@1::Counter").unwrap();
        let configuration = Value::Record(vec![
            ("minimum".into(), Value::int(0)),
            ("maximum".into(), Value::int(10)),
            ("initial".into(), Value::int(1)),
        ]);
        let state = BTreeMap::from([("count".into(), Value::int(1))]);
        let context = EvalContext::new(&program, machine, &configuration, &state);

        let exact_map = TypeRef::Option {
            value: Box::new(TypeRef::Map {
                key: Box::new(TypeRef::Text),
                value: Box::new(TypeRef::Int),
            }),
        };
        let map = context
            .eval(&Expr::Call {
                function: "Map.from_unique".into(),
                args: vec![Expr::Seq { values: Vec::new() }],
                result_type: exact_map.clone(),
            })
            .unwrap();
        assert_eq!(map.type_identity(), exact_map.canonical_name());

        let exact_set = TypeRef::Option {
            value: Box::new(TypeRef::Set {
                value: Box::new(TypeRef::Text),
            }),
        };
        let set = context
            .eval(&Expr::Call {
                function: "Set.from_unique".into(),
                args: vec![Expr::Seq { values: Vec::new() }],
                result_type: exact_set.clone(),
            })
            .unwrap();
        assert_eq!(set.type_identity(), exact_set.canonical_name());

        let exact_sequence = TypeRef::Option {
            value: Box::new(TypeRef::Seq {
                value: Box::new(TypeRef::Text),
            }),
        };
        let sequence = context
            .eval(&Expr::Method {
                value: Box::new(Expr::Seq { values: Vec::new() }),
                method: "try_map".into(),
                args: vec![Expr::Lambda {
                    params: vec!["value".into()],
                    body: Box::new(Expr::Constructor {
                        type_id: "Option<Text>".into(),
                        constructor: "some".into(),
                        fields: vec![(
                            Some("value".into()),
                            Expr::Name {
                                name: "value".into(),
                            },
                        )],
                    }),
                }],
                result_type: exact_sequence.clone(),
            })
            .unwrap();
        assert_eq!(sequence.type_identity(), exact_sequence.canonical_name());
    }

    #[test]
    fn ordinary_lambda_bindings_do_not_gain_internal_continuation_semantics() {
        let program = counter_program();
        let machine = program.machines.get("example.counter@1::Counter").unwrap();
        let configuration = Value::Record(vec![
            ("minimum".into(), Value::int(0)),
            ("maximum".into(), Value::int(10)),
            ("initial".into(), Value::int(1)),
        ]);
        let state = BTreeMap::from([("count".into(), Value::int(1))]);
        let context = EvalContext::new(&program, machine, &configuration, &state);
        let lambda = Expr::Lambda {
            params: vec!["value".into()],
            body: Box::new(Expr::Name {
                name: "value".into(),
            }),
        };

        let escaped = context
            .eval(&Expr::Let {
                bindings: vec![("ordinary".into(), lambda.clone())],
                value: Box::new(Expr::Invoke {
                    function: Box::new(Expr::Name {
                        name: "ordinary".into(),
                    }),
                    args: vec![Expr::Literal {
                        value: Value::int(1),
                    }],
                }),
            })
            .expect_err("ordinary lambda binding must not become a closure");
        assert!(escaped.message.contains("lambda cannot escape"));

        let invoked = context
            .eval(&Expr::Invoke {
                function: Box::new(lambda),
                args: vec![Expr::Literal {
                    value: Value::int(1),
                }],
            })
            .expect_err("dynamic lambda invocation remains unavailable");
        assert!(invoked.message.contains("statically resolved callables"));
    }

    #[test]
    fn admission_and_restore_reject_noncanonical_or_invalid_external_state() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";

        let reordered = Value::Record(vec![
            ("maximum".into(), Value::int(10)),
            ("minimum".into(), Value::int(0)),
            ("initial".into(), Value::int(1)),
        ]);
        assert!(program.admit(machine, reordered, "test/reordered").is_err());
        assert!(
            program
                .admit(
                    machine,
                    Value::Record(vec![
                        ("minimum".into(), Value::int(0)),
                        ("maximum".into(), Value::Text("ten".into())),
                        ("initial".into(), Value::int(1)),
                    ]),
                    "test/wrong-type",
                )
                .is_err()
        );
        assert!(
            program
                .admit(
                    machine,
                    Value::Record(vec![
                        ("minimum".into(), Value::int(5)),
                        ("maximum".into(), Value::int(10)),
                        ("initial".into(), Value::int(1)),
                    ]),
                    "test/requirement",
                )
                .is_err()
        );

        let (instance, _) = program
            .admit(machine, counter_config(), "test/restore-validation")
            .unwrap();
        let checkpoint = program.checkpoint(&instance);

        let mut malformed_hash = checkpoint.clone();
        malformed_hash.trace_prefix_hash = "ABC".into();
        assert!(program.restore(&malformed_hash).is_err());

        let mut zero_sequence = checkpoint.clone();
        zero_sequence.next_sequence = 0;
        assert!(program.restore(&zero_sequence).is_err());

        let mut reordered_config = checkpoint.clone();
        let Value::Record(fields) = &mut reordered_config.configuration else {
            unreachable!()
        };
        fields.swap(0, 1);
        assert!(program.restore(&reordered_config).is_err());

        let mut missing_state = checkpoint.clone();
        missing_state.state = Value::Record(Vec::new());
        assert!(program.restore(&missing_state).is_err());

        let mut invariant_violation = checkpoint.clone();
        invariant_violation.state = Value::Record(vec![("count".into(), Value::int(-1))]);
        assert!(program.restore(&invariant_violation).is_err());

        let mut invalid_inbox = checkpoint;
        invalid_inbox.inbox.push(Value::variant(
            format!("{machine}.Input"),
            "unknown",
            Vec::new(),
        ));
        assert!(program.restore(&invalid_inbox).is_err());
    }

    #[test]
    fn ingress_rejections_are_audited_without_consuming_machine_sequence() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";
        let (mut instance, _) = program
            .admit(machine, counter_config(), "test/ingress")
            .unwrap();
        let initial_prefix = instance.ingress_prefix_hash.clone();
        let rejected = Value::variant(format!("{machine}.Input"), "unknown", Vec::new());
        let error = program.enqueue(&mut instance, rejected).unwrap_err();

        assert!(error.record.is_some());
        assert_eq!(instance.next_sequence, 1);
        assert!(instance.inbox.is_empty());
        assert_eq!(instance.ingress_records.len(), 1);
        assert_eq!(instance.next_ingress_ordinal, 2);
        assert_ne!(instance.ingress_prefix_hash, initial_prefix);

        let before_transport_sequence = instance.next_sequence;
        program.reject_ingress_transport(&mut instance, "{", "invalid JSON");
        assert_eq!(instance.next_sequence, before_transport_sequence);
        assert_eq!(instance.ingress_records.len(), 2);
        assert!(matches!(
            instance.ingress_records[1].attempt,
            IngressAttempt::TransportText { .. }
        ));
    }

    #[test]
    fn queued_values_behind_a_fault_remain_inspectable() {
        let mut program = counter_program();
        program
            .machines
            .get_mut("example.counter@1::Counter")
            .unwrap()
            .handlers
            .get_mut("increment")
            .unwrap()
            .body = vec![Statement::Unreachable {
            source: source("forced-fault"),
        }];
        program.freeze_program_hashes();
        let (mut instance, _) = program
            .admit(
                "example.counter@1::Counter",
                counter_config(),
                "test/fault-queue",
            )
            .unwrap();
        program.enqueue(&mut instance, increment_input()).unwrap();
        program.enqueue(&mut instance, increment_input()).unwrap();

        let step = program.drain_one(&instance).unwrap().unwrap();
        assert_eq!(step.instance.lifecycle, InstanceLifecycle::Faulted);
        assert_eq!(step.instance.next_sequence, 2);
        assert_eq!(step.instance.inbox.len(), 1);
        assert!(matches!(
            step.receipt.resolution,
            ReactionResolution::Fault { .. }
        ));

        let mut faulted = step.instance;
        let queued = faulted.inbox.clone();
        let error = program
            .enqueue(&mut faulted, increment_input())
            .unwrap_err();
        assert!(error.record.is_some());
        assert_eq!(faulted.next_sequence, 2);
        assert_eq!(faulted.inbox, queued);
        assert_eq!(faulted.ingress_records.len(), 1);
    }

    #[test]
    fn in_place_drain_preserves_fifo_receipts_ingress_and_hashes() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";
        let (mut queued, _) = program
            .admit(machine, counter_config(), "test/drain-mut")
            .unwrap();
        program
            .enqueue(
                &mut queued,
                Value::variant(format!("{machine}.Input"), "unknown", Vec::new()),
            )
            .unwrap_err();
        program.enqueue(&mut queued, increment_input()).unwrap();
        program.enqueue(&mut queued, increment_input()).unwrap();

        let mut actual = queued.clone();
        let expected_first = program.drain_one(&queued).unwrap().unwrap();
        let actual_first = program.drain_one_mut(&mut actual).unwrap().unwrap();
        assert_eq!(actual_first, expected_first.receipt);
        assert_eq!(actual, expected_first.instance);
        assert_eq!(actual.inbox.len(), 1);
        assert_eq!(actual.ingress_records, queued.ingress_records);
        assert_eq!(actual.ingress_prefix_hash, queued.ingress_prefix_hash);
        assert_eq!(
            actual.trace_prefix_hash,
            expected_first.instance.trace_prefix_hash
        );

        let expected_second = program
            .drain_one(&expected_first.instance)
            .unwrap()
            .unwrap();
        let actual_second = program.drain_one_mut(&mut actual).unwrap().unwrap();
        assert_eq!(actual_second, expected_second.receipt);
        assert_eq!(actual, expected_second.instance);
        assert!(actual.inbox.is_empty());

        let complete = actual.clone();
        assert!(program.drain_one_mut(&mut actual).unwrap().is_none());
        assert_eq!(actual, complete);
    }

    #[test]
    fn in_place_submission_matches_fault_and_rejection_semantics() {
        let machine = "example.counter@1::Counter";
        let mut faulting_program = counter_program();
        faulting_program
            .machines
            .get_mut(machine)
            .unwrap()
            .handlers
            .get_mut("increment")
            .unwrap()
            .body = vec![Statement::Unreachable {
            source: source("forced-submit-fault"),
        }];
        faulting_program.freeze_program_hashes();
        let (initial, _) = faulting_program
            .admit(machine, counter_config(), "test/submit-fault")
            .unwrap();

        let mut legacy_queued = initial.clone();
        faulting_program
            .enqueue(&mut legacy_queued, increment_input())
            .unwrap();
        let expected = faulting_program.drain_one(&legacy_queued).unwrap().unwrap();
        let mut actual = initial;
        let actual_receipt = faulting_program
            .submit_one(&mut actual, increment_input())
            .unwrap();
        assert_eq!(actual_receipt, expected.receipt);
        assert_eq!(actual, expected.instance);
        assert_eq!(actual.lifecycle, InstanceLifecycle::Faulted);
        assert_eq!(actual.receipts.len(), 1);

        let program = counter_program();
        let (initial, _) = program
            .admit(machine, counter_config(), "test/submit-rejection")
            .unwrap();
        let rejected = Value::variant(format!("{machine}.Input"), "unknown", Vec::new());
        let mut expected_rejection = initial.clone();
        let expected_error = program
            .enqueue(&mut expected_rejection, rejected.clone())
            .unwrap_err();
        let mut actual_rejection = initial;
        let actual_error = match program
            .submit_one(&mut actual_rejection, rejected)
            .unwrap_err()
        {
            SubmissionError::Ingress(error) => error,
            SubmissionError::Reaction(error) => {
                panic!("invalid ingress reached the reaction: {error}")
            }
        };
        assert_eq!(actual_error, expected_error);
        assert_eq!(actual_rejection, expected_rejection);

        let mut broken_program = counter_program();
        broken_program
            .machines
            .get_mut(machine)
            .unwrap()
            .handlers
            .get_mut("increment")
            .unwrap()
            .body = Vec::new();
        broken_program.freeze_program_hashes();
        let (mut retained_queue, _) = broken_program
            .admit(machine, counter_config(), "test/submit-rollback")
            .unwrap();
        broken_program
            .enqueue(&mut retained_queue, increment_input())
            .unwrap();
        let before = retained_queue.clone();
        let error = broken_program
            .submit_one(&mut retained_queue, increment_input())
            .unwrap_err();
        assert!(matches!(error, SubmissionError::Reaction(_)));
        assert_eq!(retained_queue, before);
    }

    #[test]
    fn in_place_submission_preserves_a_checkpoint_restored_fifo() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";
        let (mut queued, _) = program
            .admit(machine, counter_config(), "test/submit-restored-fifo")
            .unwrap();
        program.enqueue(&mut queued, increment_input()).unwrap();
        let checkpoint = program.checkpoint(&queued);
        let restored = program.restore(&checkpoint).unwrap();
        assert_eq!(
            restored.inbox,
            VecDeque::from([increment_input()]),
            "checkpoint restore must preload the pending FIFO"
        );

        let mut legacy_queued = restored.clone();
        program
            .enqueue(&mut legacy_queued, increment_input())
            .unwrap();
        let expected = program.drain_one(&legacy_queued).unwrap().unwrap();

        let mut actual = restored;
        let actual_receipt = program.submit_one(&mut actual, increment_input()).unwrap();
        assert_eq!(actual_receipt, expected.receipt);
        assert_eq!(actual, expected.instance);
        assert_eq!(
            actual.inbox,
            VecDeque::from([increment_input()]),
            "submit must drain the restored front and retain the new tail"
        );
        assert_eq!(
            actual.state,
            Value::Record(vec![("count".into(), Value::int(2))])
        );
    }

    #[test]
    fn nested_empty_collection_order_survives_checkpoint_and_replay() {
        let (program, set_type, map_type) = collection_program();
        let machine = "example.collections@1::Collections";
        let empty = Value::Seq(Vec::new());
        let nonempty = Value::Seq(vec![Value::int(0)]);

        let typed_set = program
            .canonicalize_value(
                &set_type,
                &Value::Set(vec![nonempty.clone(), empty.clone()]),
            )
            .unwrap();
        let mut expected_set_values = vec![Value::Seq(vec![Value::int(0)]), Value::Seq(Vec::new())];
        let TypeRef::Set {
            value: element_type,
        } = &set_type
        else {
            unreachable!()
        };
        expected_set_values.sort_by_cached_key(|value| {
            program.canonical_value_bytes(element_type, value).unwrap()
        });
        assert_eq!(typed_set, Value::Set(expected_set_values));

        let typed_map = program
            .canonicalize_value(
                &map_type,
                &Value::Map(vec![(nonempty, Value::int(1)), (empty, Value::int(2))]),
            )
            .unwrap();
        let mut expected_map_entries = vec![
            (Value::Seq(vec![Value::int(0)]), Value::int(1)),
            (Value::Seq(Vec::new()), Value::int(2)),
        ];
        let TypeRef::Map { key: key_type, .. } = &map_type else {
            unreachable!()
        };
        expected_map_entries
            .sort_by_cached_key(|(key, _)| program.canonical_value_bytes(key_type, key).unwrap());
        assert_eq!(typed_map, Value::Map(expected_map_entries));

        let (instance, _) = program
            .admit(machine, Value::Unit, "test/nested-collections")
            .unwrap();
        assert_eq!(
            instance.state,
            Value::Record(vec![
                ("sets".into(), typed_set.clone()),
                ("maps".into(), typed_map.clone()),
            ])
        );
        let checkpoint = program.checkpoint(&instance);
        let checkpoint_bytes = program.canonical_checkpoint_bytes(&checkpoint).unwrap();
        let restored = program.restore(&checkpoint).unwrap();
        assert_eq!(
            program
                .canonical_checkpoint_bytes(&program.checkpoint(&restored))
                .unwrap(),
            checkpoint_bytes,
        );

        let input = Value::variant(format!("{machine}.Input"), "touch", Vec::new());
        let first = program.react(&instance, input.clone()).unwrap();
        let replay = program.react(&restored, input).unwrap();
        assert_eq!(
            program
                .canonical_reaction_receipt_bytes(machine, &first.receipt)
                .unwrap(),
            program
                .canonical_reaction_receipt_bytes(machine, &replay.receipt)
                .unwrap(),
        );

        let mut reordered = checkpoint;
        let Value::Record(fields) = &mut reordered.state else {
            unreachable!()
        };
        let Value::Set(values) = &mut fields[0].1 else {
            unreachable!()
        };
        values.reverse();
        assert!(program.restore(&reordered).is_err());
    }

    #[test]
    fn semantic_artifact_bytes_and_hashes_match_golden_vectors() {
        let program = counter_program();
        let machine = "example.counter@1::Counter";
        let (instance, genesis) = program
            .admit(machine, counter_config(), "test/golden")
            .unwrap();
        let genesis_bytes = program
            .canonical_genesis_receipt_bytes(machine, &genesis)
            .unwrap();
        let step = program.react(&instance, increment_input()).unwrap();
        let reaction_bytes = program
            .canonical_reaction_receipt_bytes(machine, &step.receipt)
            .unwrap();
        let checkpoint = program.checkpoint(&step.instance);
        let checkpoint_bytes = program.canonical_checkpoint_bytes(&checkpoint).unwrap();
        let mut rejected = step.instance.clone();
        program
            .enqueue(
                &mut rejected,
                Value::variant(format!("{machine}.Input"), "unknown", Vec::new()),
            )
            .unwrap_err();
        let ingress_bytes = program
            .canonical_ingress_record_bytes(&rejected.ingress_records[0])
            .unwrap();

        for (name, bytes, expected_bytes, expected_hash) in [
            (
                "genesis",
                genesis_bytes,
                "0f67656e657369732d72656365697074061f11696e7374616e63652d6964656e74697479010b746573742f676f6c64656e20da49b0aaa010e5c1cd49335f69eedf6862d4b1eef857fb93c2a5b36486ffe73320aa928aca0047d331f95fe30e3be3addedd17e8a49530f57a4d1cf13ca9e9deeb01004d0576616c7565021f0b7265636f72642d747970650111056669656c640205636f756e7403496e7425067265636f7264011c056669656c640205636f756e740e0576616c75650203496e7402000120723b031dcdd7cac11dc690e2588e98822ee02ff3f1f111bd31dfceb593f4d9f4",
                "3a440f5e40dbb0f2e20750df45ab1cdf98b24369fba02ade487dcd9b04c3a1fb",
            ),
            (
                "reaction",
                reaction_bytes,
                "107265616374696f6e2d726563656970740a1f11696e7374616e63652d6964656e74697479010b746573742f676f6c64656e20da49b0aaa010e5c1cd49335f69eedf6862d4b1eef857fb93c2a5b36486ffe73320aa928aca0047d331f95fe30e3be3addedd17e8a49530f57a4d1cf13ca9e9deeb0101340576616c756502206578616d706c652e636f756e74657240313a3a436f756e7465722e496e7075740b0776617269616e740101004409636f6d706c6574656402360576616c756502226578616d706c652e636f756e74657240313a3a436f756e7465722e4f7574636f6d650b0776617269616e7401010001000e0c636f6d6d616e642d6c697374004d0576616c7565021f0b7265636f72642d747970650111056669656c640205636f756e7403496e7425067265636f7264011c056669656c640205636f756e740e0576616c75650203496e7402000220723b031dcdd7cac11dc690e2588e98822ee02ff3f1f111bd31dfceb593f4d9f42059dfa7fe6d63a6690f17fc6649531fb53262981debd898c70ab44e1fa44aff0d",
                "1b640648c0b2986c5b8e64746d799c8bdbb635c2741df4add4b34fe1bd9fa729",
            ),
            (
                "checkpoint",
                checkpoint_bytes,
                "0a636865636b706f696e74091f11696e7374616e63652d6964656e74697479010b746573742f676f6c64656e31146465636c61726174696f6e2d6964656e74697479011a6578616d706c652e636f756e74657240313a3a436f756e74657220da49b0aaa010e5c1cd49335f69eedf6862d4b1eef857fb93c2a5b36486ffe733b7010576616c756502490b7265636f72642d747970650313056669656c6402076d696e696d756d03496e7413056669656c6402076d6178696d756d03496e7413056669656c640207696e697469616c03496e7465067265636f7264031e056669656c6402076d696e696d756d0e0576616c75650203496e740200001e056669656c6402076d6178696d756d0e0576616c75650203496e7402000a1e056669656c640207696e697469616c0e0576616c75650203496e740200014d0576616c7565021f0b7265636f72642d747970650111056669656c640205636f756e7403496e7425067265636f7264011c056669656c640205636f756e740e0576616c75650203496e740200020705696e626f780001000102208de0886378e34eeee2953be4ed8bb75673ecd2df777b0c9c447791445a16d97f",
                "b5b644dd293cc61527332ff2ddca150cfb1bbb9ee90143eef76b4befebc4bffd",
            ),
            (
                "ingress",
                ingress_bytes,
                "0e696e67726573732d7265636f7264061f11696e7374616e63652d6964656e74697479010b746573742f676f6c64656e20da49b0aaa010e5c1cd49335f69eedf6862d4b1eef857fb93c2a5b36486ffe733010101020101630a776972652d76616c756501567b2224223a2276617269616e74222c2263617365223a22756e6b6e6f776e222c226669656c6473223a5b5d2c2274797065223a226578616d706c652e636f756e74657240313a3a436f756e7465722e496e707574227d",
                "adf2f89eed301b3bf08cd6e7b0172776e76f22b84c8a91638957ba75bfdbcabf",
            ),
        ] {
            assert_eq!(hex(&bytes), expected_bytes, "{name} bytes");
            assert_eq!(
                hex(&hash("golden-artifact", &[bytes])),
                expected_hash,
                "{name} hash"
            );
        }
    }
}
