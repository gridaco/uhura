//! Lossless browser boundary for the Uhura machine runtime.
//!
//! This module is a projection of `uhura-core`, not a second interpreter.
//! Uhura machine values cross the Wasm boundary as tagged JSON, exact
//! numerics and sequence identifiers cross as text, and canonical semantic
//! receipts and checkpoints remain available as opaque strings.

use serde_json::{Map, Value as JsonValue, json};
use uhura_base::to_canonical_json;
use uhura_core::codec::{decode_hex_32, hex};
use uhura_core::{
    Checkpoint, GenesisReceipt, IngressAttempt, IngressRecord, Instance, InstanceLifecycle,
    OutcomePolicy, Program, ProgramFault, Projection, ReactionReceipt, ReactionResolution, Value,
};
use wasm_bindgen::prelude::*;

pub const BROWSER_PROTOCOL: &str = "uhura-browser/3";
pub const RUNTIME_SNAPSHOT_PROTOCOL: &str = "uhura-runtime-snapshot/0";

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProjectionFailure {
    code: &'static str,
    message: String,
    machine: String,
    presentation: String,
    instance: String,
    sequence: u64,
}

/// A single admitted Uhura machine instance and its optional pure web
/// presentation.
///
/// `Session` owns no host adapter and performs no foreign work. Committed
/// qualified commands are returned to the browser as port-targeted DTOs; any
/// later adapter result must be submitted as a new qualified input.
#[wasm_bindgen]
pub struct Session {
    program: Program,
    machine: String,
    instance: Instance,
    genesis: GenesisReceipt,
    presentation: Option<String>,
    projection: Option<Projection>,
    /// Session-local exact-text freshness identity for presentation output.
    ///
    /// This is deliberately independent of the machine sequence: restoring a
    /// checkpoint may revisit an earlier machine sequence, but must never make
    /// a browser event retained from the abandoned projection current again.
    projection_revision: u64,
    projection_failure: Option<ProjectionFailure>,
}

#[wasm_bindgen]
impl Session {
    /// Load one canonical `uhura-ir/1` program and admit one named machine.
    ///
    /// `configuration_json` must be one lossless tagged Uhura value. The
    /// caller supplies the complete stable instance identity; the runtime never
    /// invents an identity from wall time or browser randomness.
    #[wasm_bindgen(constructor)]
    pub fn new(
        ir_json: &str,
        machine: &str,
        configuration_json: &str,
        instance: &str,
        presentation: Option<String>,
        expected_identity_json: &str,
    ) -> Result<Session, String> {
        let expected = parse_expected_identity(expected_identity_json)?;
        let program = Program::from_json(ir_json)?;
        if program.identity_protocol != expected.protocol {
            return Err(format!(
                "Uhura machine identity protocol mismatch: host expected `{}`, IR declares `{}`",
                expected.protocol, program.identity_protocol
            ));
        }
        let machine = resolve_machine(&program, machine)?;
        let machine_program_hash = program
            .program_hashes
            .get(&machine)
            .expect("resolved machine has a recomputed program hash");
        if machine_program_hash != &expected.machine_program_hash {
            return Err(format!(
                "Uhura machine-program identity mismatch for `{machine}`"
            ));
        }
        let presentation = presentation
            .map(|name| resolve_presentation(&program, &name, &machine))
            .transpose()?;
        match (&presentation, &expected.presentation_hash) {
            (Some(presentation), Some(expected_hash)) => {
                let actual = program
                    .presentation_hashes
                    .get(presentation)
                    .expect("resolved presentation has a recomputed presentation hash");
                if actual != expected_hash {
                    return Err(format!(
                        "Uhura presentation identity mismatch for `{presentation}`"
                    ));
                }
            }
            (None, None) => {}
            (Some(presentation), None) => {
                return Err(format!(
                    "Uhura presentation `{presentation}` is missing its expected identity"
                ));
            }
            (None, Some(_)) => {
                return Err(
                    "Uhura host supplied a presentation identity without a presentation".into(),
                );
            }
        }
        let configuration_json = parse_json(configuration_json, "configuration")?;
        let configuration_type = &program
            .machines
            .get(&machine)
            .expect("resolved machine exists")
            .config;
        let configuration = program
            .decode_wire_value(configuration_type, &configuration_json)
            .map_err(|error| format!("configuration: {error}"))?;
        let instance_identity = exact_identity(instance)?;
        let (admitted, genesis) = program
            .admit(&machine, configuration, instance_identity)
            .map_err(|error| format!("admission: {error}"))?;
        let (projection, projection_failure) =
            project_presentation(&program, presentation.as_deref(), &admitted);
        let projection_revision = u64::from(presentation.is_some());
        Ok(Self {
            program,
            machine,
            instance: admitted,
            genesis,
            presentation,
            projection,
            projection_revision,
            projection_failure,
        })
    }

    /// Browser transport protocols spoken by this session.
    pub fn protocols(&self) -> String {
        crate::protocols()
    }

    /// Browser-safe genesis receipt. Sequence is exact text, never a JS
    /// number.
    pub fn genesis(&self) -> String {
        canonical(&browser_genesis(&self.genesis))
    }

    /// Exact Uhura machine genesis bytes as lowercase hexadecimal. This is
    /// distinct from the canonical JSON transport returned by `genesis()`.
    pub fn semantic_genesis(&self) -> Result<String, String> {
        self.program
            .canonical_genesis_receipt_bytes(&self.machine, &self.genesis)
            .map(|bytes| hex(&bytes))
            .map_err(|error| format!("semantic genesis: {error}"))
    }

    /// Current successful `uhura-view/1` document. The document sequence is
    /// exact text. Call [`Session::presentation`] when a host must distinguish
    /// a headless session from a recoverable projection failure.
    pub fn view(&self) -> Result<String, String> {
        if let Some(projection) = &self.projection {
            return Ok(canonical(&browser_view(projection)));
        }
        if let Some(failure) = &self.projection_failure {
            return Err(format!(
                "Uhura presentation projection failed at sequence {}: {}",
                failure.sequence, failure.message
            ));
        }
        Err("this Uhura machine session has no presentation".into())
    }

    /// Exact session-local freshness revision for the current successful
    /// presentation projection. Direct `view()` consumers must return this
    /// value unchanged when dispatching one of that view's event bindings.
    pub fn projection_revision(&self) -> Result<String, String> {
        self.projection
            .as_ref()
            .map(|_| self.projection_revision.to_string())
            .ok_or_else(|| {
                if self.projection_failure.is_some() {
                    "this Uhura machine session has no successful projection".to_string()
                } else {
                    "this Uhura machine session has no presentation".to_string()
                }
            })
    }

    /// Current presentation outcome for `uhura-browser/3`.
    ///
    /// A declared presentation is always represented by either a correlated
    /// view or a structured, recoverable projection error. Projection is pure
    /// presentation work and never changes machine admission or state.
    pub fn presentation(&self) -> String {
        canonical(&browser_presentation(
            self.projection.as_ref(),
            self.projection_revision,
            self.projection_failure.as_ref(),
        ))
    }

    /// Privileged `uhura-browser/3` inspection. Ordinary presentation code
    /// receives only the declared observation through `view()`.
    pub fn inspect(&self) -> Result<String, String> {
        browser_inspection(self).map(|inspection| canonical(&inspection))
    }

    /// Exact current reaction sequence as canonical natural text.
    pub fn next_sequence(&self) -> String {
        self.instance.next_sequence.to_string()
    }

    /// The complete port requirements required by the checked machine. A host
    /// must admit matching adapters before it starts pumping the session.
    pub fn port_requirements(&self) -> String {
        let requirements = self
            .program
            .machines
            .get(&self.machine)
            .map(|machine| {
                machine
                    .ports
                    .iter()
                    .map(|port| {
                        let instance = port
                            .contract_instance
                            .as_ref()
                            .expect("validated Uhura IR retains every port contract instance");
                        json!({
                            "port": port.name,
                            "contract": port.contract,
                            "contractHash": port.contract_hash,
                            "contractInstanceHash": instance.instance_hash(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        canonical(&JsonValue::Array(requirements))
    }

    /// Submit one browser resolved-input DTO:
    ///
    /// - `{"source":"local","value": <tagged Uhura value>}`
    /// - `{"source":"port","port":"router","value": <tagged Uhura value>}`
    ///
    /// The value inside a port DTO uses the contract's unqualified case. The
    /// semantic receipt retains the checked qualified constructor
    /// (`router.changed`).
    pub fn submit(&mut self, input_json: &str) -> Result<String, String> {
        let input = match parse_resolved_input(input_json) {
            Ok(input) => input,
            Err(message) => {
                let error =
                    self.program
                        .reject_ingress_transport(&mut self.instance, input_json, message);
                return Err(format!("ingress: {error}"));
            }
        };
        self.submit_core(input)
    }

    /// Submit one exact semantic input value. A constructor named `port.case`
    /// is classified as a port input in the browser result; an unqualified
    /// constructor is local. The value itself is passed unchanged to the core.
    pub fn submit_value(&mut self, value_json: &str) -> Result<String, String> {
        let input = match parse_uhura_value(value_json, "input")
            .and_then(|input| require_variant(&input, "input").map(|_| input))
        {
            Ok(input) => input,
            Err(message) => {
                let error =
                    self.program
                        .reject_ingress_transport(&mut self.instance, value_json, message);
                return Err(format!("ingress: {error}"));
            }
        };
        self.submit_core(input)
    }

    /// Resolve and dispatch one event binding from the stored projection.
    ///
    /// The renderer must return the exact-text, session-local projection
    /// revision with the binding ID. It rejects a binding retained from an
    /// older view even when the deterministic binding ID and machine sequence
    /// are unchanged after checkpoint restore. `event_json` must be a tagged
    /// Uhura record.
    pub fn dispatch_ui(
        &mut self,
        binding_id: &str,
        projection_revision: &str,
        event_json: &str,
    ) -> Result<String, String> {
        let expected = parse_sequence(projection_revision)?;
        let projection = self
            .projection
            .as_ref()
            .ok_or_else(|| "this Uhura machine session has no presentation".to_string())?;
        if self.projection_revision != expected {
            return Err(format!(
                "stale Uhura projection: event used revision {expected}, current revision is {}",
                self.projection_revision
            ));
        }
        let event = parse_uhura_value(event_json, "UI event")?;
        if !matches!(event, Value::Record(_)) {
            return Err("UI event must be an exact tagged Uhura record".into());
        }
        let input = self
            .program
            .resolve_ui_input(&self.instance, projection, binding_id, event)
            .map_err(|error| format!("UI event: {error}"))?;
        self.submit_core(input)
    }

    /// Decode one URL through the checked Routes configuration of a named
    /// Router port. Decoding yields a later port input DTO and never runs a
    /// reaction implicitly.
    pub fn decode_route(&self, port: &str, url: &str) -> Result<String, String> {
        let input = self
            .program
            .decode_route_input(&self.machine, port, url)
            .map_err(|error| format!("route decode: {error}"))?;
        let resolved = browser_resolved_value(&input, Direction::Input)?;
        match resolved.get("source").and_then(JsonValue::as_str) {
            Some("port") if resolved.get("port").and_then(JsonValue::as_str) == Some(port) => {
                Ok(canonical(&JsonValue::Object(resolved)))
            }
            _ => Err(format!(
                "route decoder for `{port}` did not produce a `{port}.changed` input"
            )),
        }
    }

    /// Encode one exact Location value through a named Router port.
    pub fn encode_route(&self, port: &str, location_json: &str) -> Result<String, String> {
        let location = parse_uhura_value(location_json, "route location")?;
        self.program
            .encode_route_location(&self.machine, port, &location)
            .map_err(|error| format!("route encode: {error}"))
    }

    /// Canonical JSON checkpoint transport. Pass this exact string unchanged
    /// to `restore`; exact sequence numbers therefore never travel through a
    /// JS number.
    pub fn checkpoint(&self) -> String {
        self.program
            .checkpoint(&self.instance)
            .to_canonical_string()
    }

    /// Exact Uhura machine checkpoint bytes as lowercase hexadecimal.
    pub fn semantic_checkpoint(&self) -> Result<String, String> {
        let checkpoint = self.program.checkpoint(&self.instance);
        self.program
            .canonical_checkpoint_bytes(&checkpoint)
            .map(|bytes| hex(&bytes))
            .map_err(|error| format!("semantic checkpoint: {error}"))
    }

    /// Restore a compatible semantic checkpoint without allocating a new
    /// identity or producing a receipt. This session deliberately refuses a
    /// checkpoint for a different admitted identity/configuration because a
    /// genesis receipt is immutable and is not part of the checkpoint.
    pub fn restore(&mut self, checkpoint_json: &str) -> Result<(), String> {
        let checkpoint: Checkpoint = serde_json::from_str(checkpoint_json)
            .map_err(|error| format!("checkpoint: {error}"))?;
        if checkpoint.to_canonical_string() != checkpoint_json {
            return Err("checkpoint is not canonical Uhura checkpoint text".into());
        }
        let restored = self
            .program
            .restore(&checkpoint)
            .map_err(|error| format!("checkpoint: {error}"))?;
        if self.program.checkpoint(&restored).to_canonical_string() != checkpoint_json {
            return Err("checkpoint is not canonical for its checked machine types".into());
        }
        if restored.id != self.genesis.instance {
            return Err(
                "checkpoint identity does not match this Uhura machine session genesis".into(),
            );
        }
        if restored.machine != self.machine {
            return Err("checkpoint machine does not match this Uhura machine session".into());
        }
        if restored.configuration != self.instance.configuration {
            return Err(
                "checkpoint configuration does not match this Uhura machine session genesis".into(),
            );
        }
        self.ensure_projection_refresh_capacity()?;
        self.instance = restored;
        self.refresh_projection();
        Ok(())
    }

    /// Exact Uhura machine bytes for the last reaction receipt as lowercase
    /// hexadecimal. It is intentionally separate from JSON transport and the
    /// browser projection.
    pub fn semantic_receipt(&self) -> Result<String, String> {
        self.instance
            .receipts
            .last()
            .ok_or_else(|| "no Uhura machine reaction receipt exists yet".to_string())
            .and_then(|receipt| {
                self.program
                    .canonical_reaction_receipt_bytes(&self.machine, receipt)
                    .map(|bytes| hex(&bytes))
                    .map_err(|error| format!("semantic receipt: {error}"))
            })
    }
}

impl Session {
    fn submit_core(&mut self, input: Value) -> Result<String, String> {
        self.ensure_projection_refresh_capacity()?;
        let receipt = self
            .program
            .submit_one(&mut self.instance, input)
            .map_err(|error| error.to_string())?;
        self.refresh_projection();
        let envelope = browser_step(self, &receipt)?;
        Ok(canonical(&envelope))
    }

    fn ensure_projection_refresh_capacity(&self) -> Result<(), String> {
        if self.presentation.is_some() && self.projection_revision == u64::MAX {
            return Err("Uhura presentation projection revision space is exhausted".to_string());
        }
        Ok(())
    }

    fn refresh_projection(&mut self) {
        if self.presentation.is_some() {
            self.projection_revision = self
                .projection_revision
                .checked_add(1)
                .expect("projection refresh capacity is checked before mutation");
        }
        let (projection, projection_failure) =
            project_presentation(&self.program, self.presentation.as_deref(), &self.instance);
        self.projection = projection;
        self.projection_failure = projection_failure;
    }
}

fn project_presentation(
    program: &Program,
    presentation: Option<&str>,
    instance: &Instance,
) -> (Option<Projection>, Option<ProjectionFailure>) {
    let Some(presentation) = presentation else {
        return (None, None);
    };
    match program.project(instance, presentation) {
        Ok(projection) => (Some(projection), None),
        Err(error) => (
            None,
            Some(ProjectionFailure {
                code: "projection-failed",
                message: error.to_string(),
                machine: instance.machine.clone(),
                presentation: presentation.to_string(),
                instance: instance.id.clone(),
                sequence: instance.next_sequence.saturating_sub(1),
            }),
        ),
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Input,
    Command,
}

fn canonical(value: &JsonValue) -> String {
    to_canonical_json(value)
}

fn parse_json(source: &str, context: &str) -> Result<JsonValue, String> {
    serde_json::from_str(source).map_err(|error| format!("{context}: {error}"))
}

fn parse_uhura_value(source: &str, context: &str) -> Result<Value, String> {
    let json = parse_json(source, context)?;
    uhura_value_from_json(&json, context)
}

fn uhura_value_from_json(json: &JsonValue, context: &str) -> Result<Value, String> {
    let value = Value::from_wire_json(json).map_err(|error| format!("{context}: {error}"))?;
    if value.to_wire_json() != *json {
        return Err(format!(
            "{context} is not canonical exact tagged Uhura JSON"
        ));
    }
    Ok(value)
}

struct ExpectedIdentity {
    protocol: String,
    machine_program_hash: String,
    presentation_hash: Option<String>,
}

fn parse_expected_identity(source: &str) -> Result<ExpectedIdentity, String> {
    let json = parse_json(source, "expected Uhura machine identity")?;
    let object = json
        .as_object()
        .ok_or_else(|| "expected Uhura machine identity must be an object".to_string())?;
    if object.len() != 3
        || !object.contains_key("identityProtocol")
        || !object.contains_key("machineProgramHash")
        || !object.contains_key("presentationHash")
    {
        return Err(
            "expected Uhura machine identity must contain exactly `identityProtocol`, `machineProgramHash`, and `presentationHash`"
                .into(),
        );
    }
    let protocol = object
        .get("identityProtocol")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "expected Uhura machine identity protocol must be nonempty text".to_string()
        })?
        .to_string();
    let machine_program_hash = object
        .get("machineProgramHash")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| "expected Uhura machine-program hash must be text".to_string())?
        .to_string();
    decode_hex_32(&machine_program_hash)
        .map_err(|error| format!("expected Uhura machine-program hash: {error}"))?;
    let presentation_hash = match object.get("presentationHash") {
        Some(JsonValue::Null) => None,
        Some(JsonValue::String(hash)) => {
            decode_hex_32(hash)
                .map_err(|error| format!("expected Uhura presentation hash: {error}"))?;
            Some(hash.clone())
        }
        _ => return Err("expected Uhura presentation hash must be text or null".into()),
    };
    Ok(ExpectedIdentity {
        protocol,
        machine_program_hash,
        presentation_hash,
    })
}

fn exact_identity(identity: &str) -> Result<String, String> {
    if identity.is_empty() {
        return Err("Uhura machine instance identity cannot be empty".into());
    }
    if identity.chars().any(char::is_control) {
        return Err("Uhura machine instance identity cannot contain control characters".into());
    }
    Ok(identity.into())
}

fn resolve_machine(program: &Program, requested: &str) -> Result<String, String> {
    resolve_named(program.machines.keys(), requested, "machine")
}

fn resolve_presentation(
    program: &Program,
    requested: &str,
    machine: &str,
) -> Result<String, String> {
    let resolved = resolve_named(program.presentations.keys(), requested, "presentation")?;
    let presentation = program
        .presentations
        .get(&resolved)
        .expect("resolved presentation exists");
    if presentation.machine != machine {
        return Err(format!(
            "presentation `{resolved}` targets `{}`, not `{machine}`",
            presentation.machine
        ));
    }
    Ok(resolved)
}

fn resolve_named<'a>(
    names: impl Iterator<Item = &'a String>,
    requested: &str,
    kind: &str,
) -> Result<String, String> {
    let names = names.cloned().collect::<Vec<_>>();
    if names.iter().any(|name| name == requested) {
        return Ok(requested.into());
    }
    let suffix = format!("::{requested}");
    let matches = names
        .into_iter()
        .filter(|name| name.ends_with(&suffix))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [name] => Ok(name.clone()),
        [] => Err(format!("unknown Uhura {kind} `{requested}`")),
        _ => Err(format!("ambiguous Uhura {kind} `{requested}`")),
    }
}

fn parse_sequence(source: &str) -> Result<u64, String> {
    let parsed = source
        .parse::<u64>()
        .map_err(|_| format!("invalid Uhura machine sequence `{source}`"))?;
    if parsed.to_string() != source {
        return Err(format!(
            "Uhura machine sequence is not canonical natural text: `{source}`"
        ));
    }
    Ok(parsed)
}

fn require_variant(value: &Value, context: &str) -> Result<(), String> {
    if matches!(value, Value::Variant { .. }) {
        Ok(())
    } else {
        Err(format!("{context} must be a closed Uhura variant"))
    }
}

fn parse_resolved_input(source: &str) -> Result<Value, String> {
    let json = parse_json(source, "resolved input")?;
    let object = json
        .as_object()
        .ok_or_else(|| "resolved input must be an object".to_string())?;
    let source = object
        .get("source")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| "resolved input needs text `source`".to_string())?;
    let value = uhura_value_from_json(
        object
            .get("value")
            .ok_or_else(|| "resolved input needs `value`".to_string())?,
        "resolved input value",
    )?;
    require_variant(&value, "resolved input value")?;
    match source {
        "local" => {
            if object.len() != 2 || !object.contains_key("source") || !object.contains_key("value")
            {
                return Err(
                    "local resolved input must contain exactly `source` and `value`".into(),
                );
            }
            ensure_local_constructor(value)
        }
        "port" => {
            if object.len() != 3
                || !object.contains_key("source")
                || !object.contains_key("port")
                || !object.contains_key("value")
            {
                return Err(
                    "port resolved input must contain exactly `source`, `port`, and `value`".into(),
                );
            }
            let port = object
                .get("port")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "port resolved input needs text `port`".to_string())?;
            qualify_constructor(value, port)
        }
        other => Err(format!("unknown resolved input source `{other}`")),
    }
}

fn ensure_local_constructor(value: Value) -> Result<Value, String> {
    let Value::Variant { constructor, .. } = &value else {
        unreachable!("caller checked variant");
    };
    if constructor.contains('.') {
        return Err(format!(
            "local resolved input cannot use qualified constructor `{constructor}`"
        ));
    }
    Ok(value)
}

fn qualify_constructor(mut value: Value, port: &str) -> Result<Value, String> {
    if port.is_empty() || port.contains('.') {
        return Err(format!("invalid Uhura port identity `{port}`"));
    }
    let Value::Variant { constructor, .. } = &mut value else {
        unreachable!("caller checked variant");
    };
    if let Some((actual_port, case)) = constructor.split_once('.') {
        if actual_port != port {
            return Err(format!(
                "input constructor `{constructor}` does not belong to port `{port}`"
            ));
        }
        if case.is_empty() || case.contains('.') {
            return Err(format!(
                "invalid qualified input constructor `{constructor}`"
            ));
        }
        return Ok(value);
    }
    if constructor.is_empty() {
        return Err("Uhura input constructor cannot be empty".into());
    }
    *constructor = format!("{port}.{constructor}");
    Ok(value)
}

fn unqualify_value(value: &Value) -> (Option<String>, Value) {
    let Value::Variant {
        type_id,
        constructor,
        fields,
    } = value
    else {
        return (None, value.clone());
    };
    let Some((port, case)) = constructor.split_once('.') else {
        return (None, value.clone());
    };
    if port.is_empty() || case.is_empty() || case.contains('.') {
        return (None, value.clone());
    }
    (
        Some(port.into()),
        Value::variant(type_id, case, fields.clone()),
    )
}

fn browser_resolved_value(
    value: &Value,
    direction: Direction,
) -> Result<Map<String, JsonValue>, String> {
    require_variant(
        value,
        match direction {
            Direction::Input => "receipt input",
            Direction::Command => "published command",
        },
    )?;
    let (port, unqualified) = unqualify_value(value);
    let mut object = Map::new();
    match (direction, port) {
        (Direction::Input, None) => {
            object.insert("source".into(), json!("local"));
        }
        (Direction::Input, Some(port)) => {
            object.insert("source".into(), json!("port"));
            object.insert("port".into(), json!(port));
        }
        (Direction::Command, None) => {
            object.insert("target".into(), json!("local"));
        }
        (Direction::Command, Some(port)) => {
            object.insert("target".into(), json!("port"));
            object.insert("port".into(), json!(port));
        }
    }
    object.insert("value".into(), unqualified.to_wire_json());
    Ok(object)
}

fn browser_genesis(receipt: &GenesisReceipt) -> JsonValue {
    json!({
        "protocol": receipt.protocol,
        "kind": "genesis",
        "instance": receipt.instance,
        "machineProgramHash": receipt.machine_program_hash,
        "configurationHash": receipt.configuration_hash,
        "sequence": receipt.sequence.to_string(),
        "initialObservation": receipt.initial_observation.to_wire_json(),
        "initialStateHash": receipt.initial_state_hash,
    })
}

fn browser_resolution(resolution: &ReactionResolution) -> JsonValue {
    match resolution {
        ReactionResolution::Completed { outcome, policy } => json!({
            "kind": "completed",
            "outcome": outcome.to_wire_json(),
            "disposition": match policy {
                OutcomePolicy::Commit => "commit",
                OutcomePolicy::Abort => "abort",
            },
        }),
        ReactionResolution::Fault { fault } => json!({
            "kind": "fault",
            "fault": browser_fault(fault),
        }),
    }
}

fn browser_fault(fault: &ProgramFault) -> JsonValue {
    match fault {
        ProgramFault::InvariantViolation { source } => json!({
            "code": "invariant-violation",
            "message": format!("a committed draft violated Uhura machine invariant `{source}`"),
        }),
        ProgramFault::UnreachableReached { source } => json!({
            "code": "unreachable-reached",
            "message": format!("Uhura machine execution reached `unreachable` at `{source}`"),
        }),
    }
}

fn browser_reaction(receipt: &ReactionReceipt) -> Result<JsonValue, String> {
    let input = JsonValue::Object(browser_resolved_value(&receipt.input, Direction::Input)?);
    let commands = receipt
        .ordered_commands
        .iter()
        .map(|command| browser_resolved_value(command, Direction::Command).map(JsonValue::Object))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "protocol": receipt.protocol,
        "kind": "reaction",
        "instance": receipt.instance,
        "machineProgramHash": receipt.machine_program_hash,
        "configurationHash": receipt.configuration_hash,
        "sequence": receipt.sequence.to_string(),
        "input": input,
        "resolution": browser_resolution(&receipt.resolution),
        "orderedCommands": commands,
        "postObservation": receipt.post_observation.to_wire_json(),
        "preStateHash": receipt.pre_state_hash,
        "postStateHash": receipt.post_state_hash,
    }))
}

fn browser_step(session: &Session, receipt: &ReactionReceipt) -> Result<JsonValue, String> {
    Ok(json!({
        "protocol": BROWSER_PROTOCOL,
        "receipt": browser_reaction(receipt)?,
        "snapshot": browser_runtime_snapshot(session, receipt),
        "presentation": browser_presentation(
            session.projection.as_ref(),
            session.projection_revision,
            session.projection_failure.as_ref(),
        ),
    }))
}

fn browser_presentation(
    projection: Option<&Projection>,
    projection_revision: u64,
    projection_failure: Option<&ProjectionFailure>,
) -> JsonValue {
    match (projection, projection_failure) {
        (Some(projection), None) => json!({
            "kind": "view",
            "projectionRevision": projection_revision.to_string(),
            "view": browser_view(projection),
        }),
        (None, Some(failure)) => json!({
            "kind": "error",
            "error": browser_projection_failure(failure),
        }),
        (None, None) => json!({ "kind": "none" }),
        _ => {
            unreachable!("one Uhura session must retain a view, a projection failure, or neither")
        }
    }
}

fn browser_projection_failure(failure: &ProjectionFailure) -> JsonValue {
    json!({
        "code": failure.code,
        "message": failure.message,
        "machine": failure.machine,
        "presentation": failure.presentation,
        "instance": failure.instance,
        "sequence": failure.sequence.to_string(),
    })
}

fn browser_runtime_snapshot(session: &Session, receipt: &ReactionReceipt) -> JsonValue {
    json!({
        "protocol": RUNTIME_SNAPSHOT_PROTOCOL,
        "instance": session.instance.id,
        "machineProgramHash": session.instance.program_hash,
        "presentation": session.presentation,
        "presentationHash": session.presentation.as_ref().map(|presentation| {
            session.program.presentation_hashes
                .get(presentation)
                .expect("resolved presentation has a frozen identity")
        }),
        "configurationHash": session.genesis.configuration_hash,
        "state": session.instance.state.to_wire_json(),
        "stateHash": receipt.post_state_hash,
        "lifecycle": match session.instance.lifecycle {
            InstanceLifecycle::Running => "running",
            InstanceLifecycle::Faulted => "faulted",
        },
        "nextSequence": session.instance.next_sequence.to_string(),
        "tracePrefixHash": session.instance.trace_prefix_hash,
        "ingressPrefixHash": session.instance.ingress_prefix_hash,
        "nextIngressOrdinal": session.instance.next_ingress_ordinal.to_string(),
    })
}

fn browser_view_base(projection: &Projection) -> JsonValue {
    let mut view = serde_json::to_value(&projection.document)
        .expect("an Uhura render document is serializable");
    if let Some(object) = view.as_object_mut() {
        object.insert(
            "sequence".into(),
            JsonValue::String(projection.document.sequence.to_string()),
        );
    }
    view
}

fn browser_view(projection: &Projection) -> JsonValue {
    browser_view_base(projection)
}

fn browser_inspection(session: &Session) -> Result<JsonValue, String> {
    let mut receipts = vec![browser_genesis(&session.genesis)];
    receipts.extend(
        session
            .instance
            .receipts
            .iter()
            .map(browser_reaction)
            .collect::<Result<Vec<_>, _>>()?,
    );
    let inbox = session
        .instance
        .inbox
        .iter()
        .map(|input| browser_resolved_value(input, Direction::Input).map(JsonValue::Object))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "protocol": BROWSER_PROTOCOL,
        "identityProtocol": session.program.identity_protocol,
        "instance": session.instance.id,
        "machineProgramHash": session.instance.program_hash,
        "presentation": session.presentation,
        "presentationHash": session.presentation.as_ref().map(|presentation| {
            session.program.presentation_hashes
                .get(presentation)
                .expect("resolved presentation has a frozen identity")
        }),
        "configurationHash": session.genesis.configuration_hash,
        "configuration": session.instance.configuration.to_wire_json(),
        "state": session.instance.state.to_wire_json(),
        "observation": session.instance.observation.to_wire_json(),
        "inbox": inbox,
        "lifecycle": match session.instance.lifecycle {
            InstanceLifecycle::Running => "running",
            InstanceLifecycle::Faulted => "faulted",
        },
        "nextSequence": session.instance.next_sequence.to_string(),
        "tracePrefixHash": session.instance.trace_prefix_hash,
        "receipts": receipts,
        "ingressPrefixHash": session.instance.ingress_prefix_hash,
        "nextIngressOrdinal": session.instance.next_ingress_ordinal.to_string(),
        "ingressRecords": session
            .instance
            .ingress_records
            .iter()
            .map(browser_ingress_record)
            .collect::<Vec<_>>(),
    }))
}

fn browser_ingress_record(record: &IngressRecord) -> JsonValue {
    let attempt = match &record.attempt {
        IngressAttempt::TransportText { text } => json!({
            "kind": "transport-text",
            "text": text,
        }),
        IngressAttempt::Value { value } => json!({
            "kind": "value",
            "value": value.to_wire_json(),
        }),
    };
    json!({
        "protocol": record.protocol,
        "instance": record.instance,
        "machineProgramHash": record.machine_program_hash,
        "ordinal": record.ordinal.to_string(),
        "machineSequence": record.machine_sequence.to_string(),
        "rejection": serde_json::to_value(record.rejection)
            .expect("ingress rejection kind serializes"),
        "message": record.message,
        "attempt": attempt,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use uhura_core::ir::{
        BinaryOp, CommandDef, ConstructorDef, Expr, Handler, Machine, ObservationField, OutcomeDef,
        PortDef, Presentation, SourceRef, StateField, Statement, TypeDef, TypeRef, UiAttribute,
        UiAttributeValue, UiNode,
    };

    fn source(id: &str) -> SourceRef {
        SourceRef::synthetic(id)
    }

    fn counter_program() -> (Program, String, String) {
        let mut program = Program::new();
        let machine_id = "example.counter@1::Counter".to_string();
        let presentation_id = "example.counter_web@1::counter".to_string();
        let input_type = format!("{machine_id}.Input");
        let outcome_type = format!("{machine_id}.Outcome");
        program.machines.insert(
            machine_id.clone(),
            Machine {
                id: machine_id.clone(),
                config: TypeRef::Record {
                    fields: vec![("initial".into(), TypeRef::Int)],
                },
                requires: Vec::new(),
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
                invariants: Vec::new(),
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
                    Handler {
                        input: "increment".into(),
                        pattern: uhura_core::Pattern::Constructor {
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
                                    type_id: outcome_type,
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
            },
        );
        program.presentations.insert(
            presentation_id.clone(),
            Presentation {
                id: presentation_id.clone(),
                machine: machine_id.clone(),
                binding: "model".into(),
                nodes: vec![UiNode::Element {
                    name: "button".into(),
                    attributes: vec![UiAttribute {
                        name: "on".into(),
                        value: UiAttributeValue::Event {
                            event: "activate".into(),
                            input: Expr::Constructor {
                                type_id: input_type,
                                constructor: "increment".into(),
                                fields: Vec::new(),
                            },
                        },
                        source: source("button-activate"),
                    }],
                    children: vec![UiNode::Interpolation {
                        value: Expr::Field {
                            value: Box::new(Expr::Name {
                                name: "model".into(),
                            }),
                            field: "count".into(),
                        },
                        source: source("count-text"),
                    }],
                    source: source("button"),
                }],
                source: source("counter-ui"),
            },
        );
        program.freeze_program_hashes();
        (program, machine_id, presentation_id)
    }

    fn expected_identity(program: &Program, machine: &str, presentation: Option<&str>) -> String {
        canonical(&json!({
            "identityProtocol": program.identity_protocol,
            "machineProgramHash": program.program_hashes[machine],
            "presentationHash": presentation.map(|presentation| &program.presentation_hashes[presentation]),
        }))
    }

    fn session() -> Session {
        let (program, machine, presentation) = counter_program();
        let configuration = Value::from_wire_json(&json!({
            "$": "record",
            "fields": [{
                "name": "initial",
                "value": { "$": "Int", "value": "9007199254740993" },
            }],
        }))
        .unwrap();
        Session::new(
            &program.to_canonical_string(),
            &machine,
            &canonical(&configuration.to_wire_json()),
            "browser/test-1",
            Some(presentation.clone()),
            &expected_identity(&program, &machine, Some(&presentation)),
        )
        .unwrap()
    }

    #[test]
    fn port_requirements_publish_exact_instantiated_contract_identity() {
        let (mut program, machine, presentation) = counter_program();
        let instance =
            uhura_port::sink_port_instance(uhura_port::TypeRef::new("Int").unwrap()).unwrap();
        program
            .machines
            .get_mut(&machine)
            .unwrap()
            .ports
            .push(PortDef {
                name: "audit".into(),
                contract: instance.identity.to_string(),
                contract_instance: Some(instance.clone()),
                type_arguments: vec![TypeRef::Int],
                configuration: None,
                receive: Vec::new(),
                send: vec![ConstructorDef {
                    name: "send".into(),
                    fields: vec![(Some("value".into()), TypeRef::Int)],
                }],
                contract_hash: instance.content_hash.clone(),
                source: source("audit"),
            });
        program.freeze_program_hashes();
        let configuration = canonical(&json!({
            "$": "record",
            "fields": [{
                "name": "initial",
                "value": { "$": "Int", "value": "1" },
            }],
        }));
        let session = Session::new(
            &program.to_canonical_string(),
            &machine,
            &configuration,
            "browser/port-requirements",
            Some(presentation.clone()),
            &expected_identity(&program, &machine, Some(&presentation)),
        )
        .unwrap();
        let requirements: JsonValue = serde_json::from_str(&session.port_requirements()).unwrap();
        assert_eq!(requirements[0]["port"], "audit");
        assert_eq!(requirements[0]["contract"], instance.identity.to_string());
        assert_eq!(requirements[0]["contractHash"], instance.content_hash);
        assert_eq!(
            requirements[0]["contractInstanceHash"],
            instance.instance_hash()
        );
    }

    #[test]
    fn constructor_rejects_host_identity_mismatches_before_admission() {
        let (program, machine, presentation) = counter_program();
        let configuration = canonical(&json!({
            "$": "record",
            "fields": [{
                "name": "initial",
                "value": { "$": "Int", "value": "1" },
            }],
        }));
        let call = |identity: String| {
            Session::new(
                &program.to_canonical_string(),
                &machine,
                &configuration,
                "browser/identity-test",
                Some(presentation.clone()),
                &identity,
            )
            .err()
            .expect("identity mismatch rejects construction")
        };

        let mut identity: JsonValue =
            serde_json::from_str(&expected_identity(&program, &machine, Some(&presentation)))
                .unwrap();
        identity["machineProgramHash"] = JsonValue::String("00".repeat(32));
        assert!(call(canonical(&identity)).contains("machine-program identity mismatch"));

        identity =
            serde_json::from_str(&expected_identity(&program, &machine, Some(&presentation)))
                .unwrap();
        identity["presentationHash"] = JsonValue::String("11".repeat(32));
        assert!(call(canonical(&identity)).contains("presentation identity mismatch"));

        identity =
            serde_json::from_str(&expected_identity(&program, &machine, Some(&presentation)))
                .unwrap();
        identity["unexpected"] = JsonValue::Bool(true);
        assert!(call(canonical(&identity)).contains("must contain exactly"));
    }

    fn increment_value() -> Value {
        Value::variant("example.counter@1::Counter.Input", "increment", Vec::new())
    }

    fn projection_failure_program() -> (Program, String, String) {
        let (mut program, machine, presentation) = counter_program();
        let command_type = format!("{machine}.Command");
        let machine_definition = program.machines.get_mut(&machine).unwrap();
        machine_definition.local_commands.push(CommandDef {
            constructor: ConstructorDef {
                name: "reported".into(),
                fields: Vec::new(),
            },
            source: source("reported-command"),
        });
        machine_definition
            .handlers
            .get_mut("increment")
            .unwrap()
            .body
            .insert(
                1,
                Statement::Emit {
                    value: Expr::Constructor {
                        type_id: command_type,
                        constructor: "reported".into(),
                        fields: Vec::new(),
                    },
                    source: source("emit-reported"),
                },
            );

        let surface = |id: &str| UiNode::Element {
            name: "Surface".into(),
            attributes: vec![UiAttribute {
                name: "key".into(),
                value: UiAttributeValue::Expression {
                    value: Expr::Literal {
                        value: Value::Text("same-surface".into()),
                    },
                },
                source: source(&format!("{id}-key")),
            }],
            children: Vec::new(),
            source: source(id),
        };
        program.presentations.get_mut(&presentation).unwrap().nodes = vec![UiNode::If {
            condition: Expr::Binary {
                op: BinaryOp::Equal,
                left: Box::new(Expr::Field {
                    value: Box::new(Expr::Name {
                        name: "model".into(),
                    }),
                    field: "count".into(),
                }),
                right: Box::new(Expr::Literal {
                    value: Value::int(1),
                }),
            },
            children: vec![surface("first-surface"), surface("second-surface")],
            source: source("duplicate-surfaces-at-one"),
        }];
        program.freeze_program_hashes();
        (program, machine, presentation)
    }

    fn first_binding(session: &Session) -> String {
        let projection = session.projection.as_ref().unwrap();
        projection.bindings.keys().next().unwrap().clone()
    }

    #[test]
    fn browser_round_trip_is_exact_and_includes_view() {
        let mut session = session();
        let genesis: JsonValue = serde_json::from_str(&session.genesis()).unwrap();
        assert_eq!(genesis["protocol"], "uhura-genesis-receipt/0");
        assert_eq!(genesis["sequence"], "0");
        assert_eq!(
            genesis["initialObservation"]["fields"][0]["value"]["value"],
            "9007199254740993"
        );
        let input = json!({
            "source": "local",
            "value": increment_value().to_wire_json(),
        });
        let step: JsonValue =
            serde_json::from_str(&session.submit(&canonical(&input)).unwrap()).unwrap();
        assert_eq!(step["protocol"], BROWSER_PROTOCOL);
        assert_eq!(step["receipt"]["protocol"], "uhura-reaction-receipt/0");
        assert_eq!(step["receipt"]["sequence"], "1");
        assert_eq!(step["presentation"]["kind"], "view");
        assert_eq!(step["presentation"]["projectionRevision"], "2");
        let view = &step["presentation"]["view"];
        assert_eq!(view["protocol"], "uhura-view/1");
        assert_eq!(view["sequence"], "1");
        assert!(view.get("projectionRevision").is_none());
        assert_eq!(
            view["nodes"][0]["events"][0]["binding"],
            first_binding(&session)
        );
        assert_eq!(session.projection_revision().unwrap(), "2");
        assert_eq!(
            step["receipt"]["postObservation"]["fields"][0]["value"]["value"],
            "9007199254740994"
        );
        assert_eq!(step["snapshot"]["protocol"], RUNTIME_SNAPSHOT_PROTOCOL);
        assert_eq!(step["snapshot"]["nextSequence"], "2");
        let inspection: JsonValue = serde_json::from_str(&session.inspect().unwrap()).unwrap();
        assert_eq!(
            inspection["identityProtocol"],
            session.program.identity_protocol
        );
        assert_eq!(
            inspection["presentation"].as_str(),
            session.presentation.as_deref()
        );
        assert_eq!(
            inspection["presentationHash"],
            session.program.presentation_hashes[session.presentation.as_ref().unwrap()]
        );
        assert_eq!(inspection["nextSequence"], "2");
        assert_eq!(
            session.semantic_receipt().unwrap(),
            hex(&session
                .program
                .canonical_reaction_receipt_bytes(&session.machine, &session.instance.receipts[0],)
                .unwrap())
        );
    }

    #[test]
    fn projection_failure_commits_the_machine_and_recovers_without_trace_drift() {
        let (program, machine, presentation) = projection_failure_program();
        let configuration = canonical(&json!({
            "$": "record",
            "fields": [{
                "name": "initial",
                "value": { "$": "Int", "value": "0" },
            }],
        }));
        let ir = program.to_canonical_string();
        let instance = "browser/projection-failure";
        let mut presented = Session::new(
            &ir,
            &machine,
            &configuration,
            instance,
            Some(presentation.clone()),
            &expected_identity(&program, &machine, Some(&presentation)),
        )
        .unwrap();
        let mut headless = Session::new(
            &ir,
            &machine,
            &configuration,
            instance,
            None,
            &expected_identity(&program, &machine, None),
        )
        .unwrap();
        let initial: JsonValue = serde_json::from_str(&presented.presentation()).unwrap();
        assert_eq!(initial["kind"], "view");

        let input = canonical(&increment_value().to_wire_json());
        let presented_step: JsonValue =
            serde_json::from_str(&presented.submit_value(&input).unwrap()).unwrap();
        let headless_step: JsonValue =
            serde_json::from_str(&headless.submit_value(&input).unwrap()).unwrap();
        assert_eq!(presented_step["receipt"]["sequence"], "1");
        assert_eq!(
            presented_step["receipt"]["orderedCommands"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(presented_step["receipt"], headless_step["receipt"]);
        assert_eq!(
            presented_step["receipt"]["orderedCommands"],
            headless_step["receipt"]["orderedCommands"]
        );
        assert_eq!(presented_step["presentation"]["kind"], "error");
        assert_eq!(
            presented_step["presentation"]["error"]["code"],
            "projection-failed"
        );
        assert_eq!(
            presented_step["presentation"]["error"]["sequence"],
            presented_step["receipt"]["sequence"]
        );
        assert!(
            presented_step["presentation"]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("duplicate Surface keys")
        );
        assert_eq!(headless_step["presentation"]["kind"], "none");
        assert!(presented.projection.is_none());
        assert!(presented.projection_failure.is_some());
        assert_eq!(presented.next_sequence(), "2");
        assert_eq!(presented.semantic_receipt(), headless.semantic_receipt());
        assert_eq!(presented.checkpoint(), headless.checkpoint());

        let failed_checkpoint = presented.checkpoint();
        let recovered_step: JsonValue =
            serde_json::from_str(&presented.submit_value(&input).unwrap()).unwrap();
        let headless_recovered: JsonValue =
            serde_json::from_str(&headless.submit_value(&input).unwrap()).unwrap();
        assert_eq!(recovered_step["receipt"], headless_recovered["receipt"]);
        assert_eq!(recovered_step["presentation"]["kind"], "view");
        assert_eq!(
            recovered_step["presentation"]["view"]["sequence"],
            recovered_step["receipt"]["sequence"]
        );
        assert!(presented.projection.is_some());
        assert!(presented.projection_failure.is_none());

        presented.restore(&failed_checkpoint).unwrap();
        let restored: JsonValue = serde_json::from_str(&presented.presentation()).unwrap();
        assert_eq!(restored["kind"], "error");
        assert_eq!(presented.next_sequence(), "2");
        let recovered_again: JsonValue =
            serde_json::from_str(&presented.submit_value(&input).unwrap()).unwrap();
        assert_eq!(recovered_again["presentation"]["kind"], "view");
    }

    #[test]
    fn initial_projection_failure_does_not_reject_machine_admission() {
        let (program, machine, presentation) = projection_failure_program();
        let configuration = canonical(&json!({
            "$": "record",
            "fields": [{
                "name": "initial",
                "value": { "$": "Int", "value": "1" },
            }],
        }));
        let session = Session::new(
            &program.to_canonical_string(),
            &machine,
            &configuration,
            "browser/initial-projection-failure",
            Some(presentation.clone()),
            &expected_identity(&program, &machine, Some(&presentation)),
        )
        .unwrap();
        let presentation: JsonValue = serde_json::from_str(&session.presentation()).unwrap();
        assert_eq!(presentation["kind"], "error");
        assert_eq!(presentation["error"]["sequence"], "0");
        assert_eq!(session.next_sequence(), "1");
        assert_eq!(
            serde_json::from_str::<JsonValue>(&session.genesis()).unwrap()["sequence"],
            "0"
        );
    }

    #[test]
    fn ui_dispatch_rejects_a_stale_projection_revision() {
        let mut session = session();
        let binding = first_binding(&session);
        let old_revision = session.projection_revision.to_string();
        session
            .submit_value(&canonical(&increment_value().to_wire_json()))
            .unwrap();
        let event = canonical(&Value::Record(Vec::new()).to_wire_json());
        let error = session
            .dispatch_ui(&binding, &old_revision, &event)
            .unwrap_err();
        assert!(error.contains("stale Uhura projection"));

        let current = session.projection_revision.to_string();
        session.dispatch_ui(&binding, &current, &event).unwrap();
        assert_eq!(session.next_sequence(), "3");
    }

    #[test]
    fn checkpoint_restore_never_revalidates_an_abandoned_projection() {
        let mut session = session();
        let checkpoint = session.checkpoint();
        let initial_binding = first_binding(&session);
        let initial_revision = session.projection_revision.to_string();
        let initial_sequence = session.projection.as_ref().unwrap().document.sequence;

        session
            .submit_value(&canonical(&increment_value().to_wire_json()))
            .unwrap();
        session.restore(&checkpoint).unwrap();

        assert_eq!(
            session.projection.as_ref().unwrap().document.sequence,
            initial_sequence,
            "restore deliberately revisits the checkpoint machine sequence"
        );
        assert_ne!(session.projection_revision.to_string(), initial_revision);
        let event = canonical(&Value::Record(Vec::new()).to_wire_json());
        let error = session
            .dispatch_ui(&initial_binding, &initial_revision, &event)
            .unwrap_err();
        assert!(error.contains("stale Uhura projection"));

        let restored_revision = session.projection_revision.to_string();
        session
            .dispatch_ui(&initial_binding, &restored_revision, &event)
            .unwrap();
    }

    #[test]
    fn exhausted_projection_revisions_reject_before_machine_mutation() {
        let mut session = session();
        session.projection_revision = u64::MAX;
        let before = session.checkpoint();
        let error = session
            .submit_value(&canonical(&increment_value().to_wire_json()))
            .unwrap_err();
        assert!(error.contains("projection revision space is exhausted"));
        assert_eq!(session.checkpoint(), before);
    }

    #[test]
    fn checkpoint_restore_replays_identical_semantic_receipt_bytes() {
        let mut session = session();
        let checkpoint = session.checkpoint();
        session
            .submit_value(&canonical(&increment_value().to_wire_json()))
            .unwrap();
        let first = session.semantic_receipt().unwrap();
        session.restore(&checkpoint).unwrap();
        assert_eq!(session.next_sequence(), "1");
        session
            .submit_value(&canonical(&increment_value().to_wire_json()))
            .unwrap();
        assert_eq!(session.semantic_receipt().unwrap(), first);
        assert_eq!(session.instance.id, "browser/test-1");
    }

    #[test]
    fn malformed_ingress_is_inspectable_and_consumes_no_machine_sequence() {
        let mut session = session();
        let before = session.next_sequence();
        let error = session.submit_value("{").unwrap_err();
        assert!(error.contains("ingress"));
        assert_eq!(session.next_sequence(), before);

        let inspection: JsonValue = serde_json::from_str(&session.inspect().unwrap()).unwrap();
        assert_eq!(inspection["nextSequence"], before);
        assert_eq!(inspection["nextIngressOrdinal"], "2");
        assert_eq!(inspection["ingressRecords"].as_array().unwrap().len(), 1);
        assert_eq!(
            inspection["ingressRecords"][0]["rejection"],
            "malformed-transport"
        );
    }

    #[test]
    fn qualified_values_map_at_the_browser_edge_without_mutating_core_identity() {
        let browser = json!({
            "source": "port",
            "port": "router",
            "value": {
                "$": "variant",
                "type": "uhura.web_router@1::RouterReceive<Location>",
                "case": "changed",
                "fields": [],
            },
        });
        let semantic = parse_resolved_input(&canonical(&browser)).unwrap();
        let Value::Variant { constructor, .. } = &semantic else {
            panic!("resolved input is a variant");
        };
        assert_eq!(constructor, "router.changed");

        let projected = browser_resolved_value(&semantic, Direction::Input).unwrap();
        assert_eq!(projected["source"], "port");
        assert_eq!(projected["port"], "router");
        assert_eq!(projected["value"]["case"], "changed");
        let Value::Variant { constructor, .. } = semantic else {
            unreachable!();
        };
        assert_eq!(constructor, "router.changed");

        let command = Value::variant(
            "app.instagram@1::Instagram::port.mutations.Send",
            "mutations.request",
            Vec::new(),
        );
        let projected = browser_resolved_value(&command, Direction::Command).unwrap();
        assert_eq!(projected["target"], "port");
        assert_eq!(projected["port"], "mutations");
        assert_eq!(
            projected["value"]["type"],
            "app.instagram@1::Instagram::port.mutations.Send"
        );
        assert_eq!(projected["value"]["case"], "request");
        let Value::Variant { constructor, .. } = command else {
            unreachable!();
        };
        assert_eq!(constructor, "mutations.request");
    }

    #[test]
    fn browser_boundary_rejects_noncanonical_numeric_shortcuts() {
        assert!(
            parse_uhura_value(r#"{"$":"Int","value":"01"}"#, "test")
                .unwrap_err()
                .contains("not canonical")
        );
        assert!(
            parse_uhura_value(r#"{"$":"Decimal","value":"1.0"}"#, "test")
                .unwrap_err()
                .contains("not canonical")
        );
        assert!(parse_uhura_value(r#"{"$":"Int","value":1}"#, "test").is_err());
    }
}
