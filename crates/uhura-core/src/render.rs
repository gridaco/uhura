use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use uhura_port::{
    RouteAtom, RouteFieldKind, RouteFieldValue, RouteLocation, RouteTable, TypeRef as PortTypeRef,
};

use super::codec::{hash, hex};
use super::ir::{Expr, Machine, Program, SourceRef, UiAttributeValue, UiNode};
use super::runtime::{
    Instance, evaluate_condition_with_locals, evaluate_with_locals, finite_values, match_pattern,
    record_map,
};
use super::value::{BoundaryNumber, Value};

pub const VIEW_PROTOCOL: &str = "uhura-view/1";
pub const PROJECTION_SOURCES_PROTOCOL: &str = "uhura-projection-sources/0";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RenderDocument {
    pub protocol: String,
    pub presentation: String,
    pub machine: String,
    pub instance: String,
    pub sequence: u64,
    pub nodes: Vec<RenderNode>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RenderNode {
    Text {
        key: String,
        text: String,
    },
    Element {
        key: String,
        element: String,
        attributes: Vec<RenderAttribute>,
        events: Vec<RenderEvent>,
        children: Vec<RenderNode>,
        surface: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RenderAttribute {
    pub name: String,
    pub value: RenderAttributeValue,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RenderAttributeValue {
    Bool(bool),
    Text(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RenderEvent {
    pub event: String,
    pub binding: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventBinding {
    pub input: Expr,
    pub locals: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Projection {
    pub document: RenderDocument,
    pub bindings: BTreeMap<String, EventBinding>,
    /// Physical source navigation keyed by the opaque rendered-node keys.
    ///
    /// This sidecar is not part of [`RenderDocument`] and therefore cannot
    /// affect projection reconciliation, stale-event identity, or machine
    /// semantics.
    pub sources: ProjectionSources,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectionSources {
    pub protocol: String,
    pub presentation: String,
    pub nodes: BTreeMap<String, SourceRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderError(pub String);

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RenderError {}

impl Program {
    pub fn project(
        &self,
        instance: &Instance,
        presentation_id: &str,
    ) -> Result<Projection, RenderError> {
        let presentation = self
            .presentations
            .get(presentation_id)
            .ok_or_else(|| RenderError(format!("unknown presentation `{presentation_id}`")))?;
        if presentation.machine != instance.machine {
            return Err(RenderError(format!(
                "presentation `{presentation_id}` targets `{}`, not `{}`",
                presentation.machine, instance.machine
            )));
        }
        let machine = self
            .machine_program
            .machines
            .get(&instance.machine)
            .ok_or_else(|| RenderError("instance machine is absent from the program".into()))?;
        let Value::Record(state_fields) = &instance.state else {
            return Err(RenderError("instance state is not a record".into()));
        };
        let state = record_map(state_fields).map_err(|error| RenderError(error.to_string()))?;
        let mut locals = BTreeMap::new();
        locals.insert(presentation.binding.clone(), instance.observation.clone());
        let mut projector = Projector {
            program: self,
            machine,
            instance,
            state,
            bindings: BTreeMap::new(),
            sources: BTreeMap::new(),
            surfaces: BTreeSet::new(),
        };
        let nodes = projector.nodes(&presentation.nodes, &locals, "root")?;
        Ok(Projection {
            document: RenderDocument {
                protocol: VIEW_PROTOCOL.into(),
                presentation: presentation.id.clone(),
                machine: instance.machine.clone(),
                instance: instance.id.clone(),
                sequence: instance.next_sequence.saturating_sub(1),
                nodes,
            },
            bindings: projector.bindings,
            sources: ProjectionSources {
                protocol: PROJECTION_SOURCES_PROTOCOL.into(),
                presentation: presentation.id.clone(),
                nodes: projector.sources,
            },
        })
    }

    pub fn resolve_ui_input(
        &self,
        instance: &Instance,
        projection: &Projection,
        binding_id: &str,
        event: Value,
    ) -> Result<Value, RenderError> {
        if projection.document.instance != instance.id
            || projection.document.sequence != instance.next_sequence.saturating_sub(1)
        {
            return Err(RenderError(
                "UI event refers to a stale Uhura projection".into(),
            ));
        }
        let binding = projection
            .bindings
            .get(binding_id)
            .ok_or_else(|| RenderError(format!("unknown UI event binding `{binding_id}`")))?;
        let machine = self
            .machine_program
            .machines
            .get(&instance.machine)
            .ok_or_else(|| RenderError("instance machine is absent from the program".into()))?;
        let Value::Record(state_fields) = &instance.state else {
            return Err(RenderError("instance state is not a record".into()));
        };
        let state = record_map(state_fields).map_err(|error| RenderError(error.to_string()))?;
        let mut locals = binding.locals.clone();
        locals.insert("event".into(), event);
        evaluate_with_locals(
            &self.machine_program,
            machine,
            &instance.configuration,
            &state,
            locals,
            &binding.input,
        )
        .map_err(|error| RenderError(error.to_string()))
    }
}

struct Projector<'a> {
    program: &'a Program,
    machine: &'a Machine,
    instance: &'a Instance,
    state: BTreeMap<String, Value>,
    bindings: BTreeMap<String, EventBinding>,
    sources: BTreeMap<String, SourceRef>,
    surfaces: BTreeSet<Vec<u8>>,
}

impl Projector<'_> {
    fn nodes(
        &mut self,
        nodes: &[UiNode],
        locals: &BTreeMap<String, Value>,
        path: &str,
    ) -> Result<Vec<RenderNode>, RenderError> {
        let mut output = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            let node_path = format!("{path}.{index}");
            self.node(node, locals, &node_path, &mut output)?;
        }
        Ok(output)
    }

    fn node(
        &mut self,
        node: &UiNode,
        locals: &BTreeMap<String, Value>,
        path: &str,
        output: &mut Vec<RenderNode>,
    ) -> Result<(), RenderError> {
        match node {
            UiNode::Text { value, source } => {
                let key = self.render_key(source, path)?;
                output.push(RenderNode::Text {
                    key,
                    text: value.clone(),
                });
            }
            UiNode::Interpolation { value, source } => {
                let value = self.eval(value, locals)?;
                let key = self.render_key(source, path)?;
                output.push(RenderNode::Text {
                    key,
                    text: display_value(&value)?,
                });
            }
            UiNode::Element {
                name,
                attributes,
                children,
                source,
            } => {
                let mut rendered_attributes = Vec::new();
                let mut events = Vec::new();
                let mut semantic = BTreeMap::<String, (Expr, Value)>::new();
                let mut surface_key = None;
                for attribute in attributes {
                    match &attribute.value {
                        UiAttributeValue::Text { value } => {
                            rendered_attributes.push(RenderAttribute {
                                name: attribute.name.clone(),
                                value: RenderAttributeValue::Text(value.clone()),
                            });
                        }
                        UiAttributeValue::Expression { value } => {
                            let evaluated = self.eval(value, locals)?;
                            if attribute.name == "key" {
                                surface_key = Some(evaluated.canonical_bytes());
                            }
                            semantic
                                .insert(attribute.name.clone(), (value.clone(), evaluated.clone()));
                            if !matches!(attribute.name.as_str(), "key" | "routes" | "to") {
                                rendered_attributes.push(RenderAttribute {
                                    name: attribute.name.clone(),
                                    value: attribute_value(&evaluated)?,
                                });
                            }
                        }
                        UiAttributeValue::Event { event, input } => {
                            let binding = event_key(&source.id, path, event);
                            if self
                                .bindings
                                .insert(
                                    binding.clone(),
                                    EventBinding {
                                        input: input.clone(),
                                        locals: event_binding_locals(input, locals),
                                    },
                                )
                                .is_some()
                            {
                                return Err(RenderError(format!(
                                    "duplicate UI event identity `{binding}`"
                                )));
                            }
                            events.push(RenderEvent {
                                event: event.clone(),
                                binding,
                            });
                        }
                    }
                }

                let surface = name == "Surface";
                let surface_identity = if surface {
                    let identity = surface_key
                        .ok_or_else(|| RenderError("Surface is missing a checked `key`".into()))?;
                    if !self.surfaces.insert(identity.clone()) {
                        return Err(RenderError(
                            "one projection contains duplicate Surface keys".into(),
                        ));
                    }
                    Some(identity)
                } else {
                    None
                };
                let element = if name == "Link" {
                    let (routes_expression, _) = semantic
                        .get("routes")
                        .ok_or_else(|| RenderError("Link is missing checked `routes`".into()))?;
                    let (_, target) = semantic
                        .get("to")
                        .ok_or_else(|| RenderError("Link is missing checked `to`".into()))?;
                    let table = self.route_table(routes_expression)?;
                    let location = route_location(table, target)?;
                    let href = table
                        .encode(&location)
                        .map_err(|error| RenderError(error.to_string()))?;
                    rendered_attributes.push(RenderAttribute {
                        name: "href".into(),
                        value: RenderAttributeValue::Text(href),
                    });
                    "a".to_string()
                } else if surface {
                    "dialog".to_string()
                } else {
                    name.clone()
                };
                let key = match surface_identity {
                    Some(identity) => self.render_surface_key(source, identity)?,
                    None => self.render_key(source, path)?,
                };
                output.push(RenderNode::Element {
                    key,
                    element,
                    attributes: rendered_attributes,
                    events,
                    children: self.nodes(children, locals, &format!("{path}.children"))?,
                    surface,
                });
            }
            UiNode::If {
                condition,
                children,
                ..
            } => {
                let (matches, bindings) = evaluate_condition_with_locals(
                    &self.program.machine_program,
                    self.machine,
                    &self.instance.configuration,
                    &self.state,
                    locals.clone(),
                    condition,
                )
                .map_err(|error| RenderError(error.to_string()))?;
                if matches {
                    let mut scoped = locals.clone();
                    scoped.extend(bindings);
                    output.extend(self.nodes(children, &scoped, &format!("{path}.if"))?);
                }
            }
            UiNode::Match { value, cases, .. } => {
                let value = self.eval(value, locals)?;
                let mut matched = false;
                for (index, case) in cases.iter().enumerate() {
                    let mut bindings = BTreeMap::new();
                    if match_pattern(&case.pattern, &value, &mut bindings)
                        .map_err(|error| RenderError(error.to_string()))?
                    {
                        let mut scoped = locals.clone();
                        scoped.extend(bindings);
                        output.extend(self.nodes(
                            &case.children,
                            &scoped,
                            &format!("{path}.case.{index}"),
                        )?);
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Err(RenderError("checked UI match was not exhaustive".into()));
                }
            }
            UiNode::Each {
                value,
                pattern,
                key,
                children,
                ..
            } => {
                let items = finite_values(&self.eval(value, locals)?)
                    .map_err(|error| RenderError(error.to_string()))?;
                let mut identities = BTreeSet::new();
                let mut rows = Vec::with_capacity(items.len());
                for item in items {
                    let mut bindings = BTreeMap::new();
                    if !match_pattern(pattern, &item, &mut bindings)
                        .map_err(|error| RenderError(error.to_string()))?
                    {
                        return Err(RenderError(
                            "checked UI each pattern did not match its item".into(),
                        ));
                    }
                    let mut scoped = locals.clone();
                    scoped.extend(bindings);
                    // The checker restricts repetition keys to scalar values
                    // or nominal wrappers over scalar values, whose canonical
                    // bytes are injective for this checked domain.
                    let identity = self.eval(key, &scoped)?.canonical_bytes();
                    if !identities.insert(identity.clone()) {
                        return Err(RenderError(
                            "one UI each expansion contains duplicate keys".into(),
                        ));
                    }
                    rows.push((scoped, identity));
                }
                for (scoped, identity) in rows {
                    output.extend(self.nodes(
                        children,
                        &scoped,
                        &format!("{path}.item.{}", hex(&hash("ui-key", &[identity]))),
                    )?);
                }
            }
        }
        Ok(())
    }

    fn render_key(&mut self, source: &SourceRef, path: &str) -> Result<String, RenderError> {
        let key = node_key(&source.id, path);
        self.record_render_key(key, source)
    }

    fn render_surface_key(
        &mut self,
        source: &SourceRef,
        identity: Vec<u8>,
    ) -> Result<String, RenderError> {
        let key = surface_node_key(identity);
        self.record_render_key(key, source)
    }

    fn record_render_key(
        &mut self,
        key: String,
        source: &SourceRef,
    ) -> Result<String, RenderError> {
        if self.sources.insert(key.clone(), source.clone()).is_some() {
            return Err(RenderError(format!(
                "duplicate rendered-node identity `{key}`"
            )));
        }
        Ok(key)
    }

    fn eval(
        &self,
        expression: &Expr,
        locals: &BTreeMap<String, Value>,
    ) -> Result<Value, RenderError> {
        evaluate_with_locals(
            &self.program.machine_program,
            self.machine,
            &self.instance.configuration,
            &self.state,
            locals.clone(),
            expression,
        )
        .map_err(|error| RenderError(error.to_string()))
    }

    fn route_table(&self, expression: &Expr) -> Result<&RouteTable, RenderError> {
        let Expr::Name { name } = expression else {
            return Err(RenderError(
                "Link routes must resolve to one declared route constant".into(),
            ));
        };
        self.program
            .route_tables
            .get(name)
            .or_else(|| {
                self.program
                    .route_tables
                    .iter()
                    .find(|(candidate, _)| candidate.ends_with(&format!("::{name}")))
                    .map(|(_, table)| table)
            })
            .ok_or_else(|| RenderError(format!("no checked route table for `{name}`")))
    }
}

fn node_key(source: &str, path: &str) -> String {
    hex(&hash(
        "ui-node",
        &[source.as_bytes().to_vec(), path.as_bytes().to_vec()],
    ))
}

fn surface_node_key(identity: Vec<u8>) -> String {
    hex(&hash("ui-surface-node", &[identity]))
}

fn event_key(source: &str, path: &str, event: &str) -> String {
    hex(&hash(
        "ui-event",
        &[
            source.as_bytes().to_vec(),
            path.as_bytes().to_vec(),
            event.as_bytes().to_vec(),
        ],
    ))
}

fn event_binding_locals(input: &Expr, locals: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let mut names = BTreeSet::new();
    collect_expression_names(input, &mut names);
    names
        .into_iter()
        .filter_map(|name| locals.get(&name).cloned().map(|value| (name, value)))
        .collect()
}

fn collect_expression_names(expression: &Expr, names: &mut BTreeSet<String>) {
    match expression {
        Expr::Literal { .. } => {}
        Expr::Name { name } => {
            names.insert(name.clone());
        }
        Expr::Constructor { fields, .. } => {
            for (_, value) in fields {
                collect_expression_names(value, names);
            }
        }
        Expr::Record { fields } => {
            for (_, value) in fields {
                collect_expression_names(value, names);
            }
        }
        Expr::Key { value, .. }
        | Expr::Unary { value, .. }
        | Expr::Field { value, .. }
        | Expr::Is { value, .. } => collect_expression_names(value, names),
        Expr::Tuple { values } | Expr::Seq { values } => {
            for value in values {
                collect_expression_names(value, names);
            }
        }
        Expr::Map { entries, .. } | Expr::Collect { clauses: entries } => {
            for (key, value) in entries {
                collect_expression_names(key, names);
                collect_expression_names(value, names);
            }
        }
        Expr::Table { entries, .. } => {
            for (_, value) in entries {
                collect_expression_names(value, names);
            }
        }
        Expr::Binary { left, right, .. }
        | Expr::Index {
            value: left,
            key: right,
        } => {
            collect_expression_names(left, names);
            collect_expression_names(right, names);
        }
        Expr::Call { args, .. } => {
            for argument in args {
                collect_expression_names(argument, names);
            }
        }
        Expr::Invoke { function, args } => {
            collect_expression_names(function, names);
            for argument in args {
                collect_expression_names(argument, names);
            }
        }
        Expr::Method { value, args, .. } => {
            collect_expression_names(value, names);
            for argument in args {
                collect_expression_names(argument, names);
            }
        }
        Expr::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_expression_names(condition, names);
            collect_expression_names(then_value, names);
            collect_expression_names(else_value, names);
        }
        Expr::Match { value, arms } => {
            collect_expression_names(value, names);
            for arm in arms {
                collect_expression_names(&arm.value, names);
            }
        }
        Expr::Update { value, fields } => {
            collect_expression_names(value, names);
            for (_, value) in fields {
                collect_expression_names(value, names);
            }
        }
        Expr::Let { bindings, value } => {
            for (_, binding) in bindings {
                collect_expression_names(binding, names);
            }
            collect_expression_names(value, names);
        }
        Expr::Lambda { body, .. } => collect_expression_names(body, names),
        Expr::SetComprehension {
            source,
            conditions,
            value,
            ..
        } => {
            collect_expression_names(source, names);
            for condition in conditions {
                collect_expression_names(condition, names);
            }
            collect_expression_names(value, names);
        }
    }
}

fn display_value(value: &Value) -> Result<String, RenderError> {
    match value {
        Value::Text(value) => Ok(value.clone()),
        Value::Integer { value, .. } => Ok(value.to_string()),
        Value::Decimal(value) | Value::Ratio(value) => Ok(value.canonical_text()),
        Value::Boundary(BoundaryNumber::Finite(value)) => Ok(value.canonical_text()),
        Value::Boundary(BoundaryNumber::Nan) => Ok("nan".into()),
        Value::Boundary(BoundaryNumber::PositiveInfinity) => Ok("positive_infinity".into()),
        Value::Boundary(BoundaryNumber::NegativeInfinity) => Ok("negative_infinity".into()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Key { value, .. } => display_value(value),
        Value::Unit => Ok(String::new()),
        Value::Variant {
            constructor,
            fields,
            ..
        } if fields.is_empty() => Ok(constructor.clone()),
        _ => Err(RenderError(format!(
            "{} is not a display scalar",
            value.type_identity()
        ))),
    }
}

fn attribute_value(value: &Value) -> Result<RenderAttributeValue, RenderError> {
    match value {
        Value::Bool(value) => Ok(RenderAttributeValue::Bool(*value)),
        _ => display_value(value).map(RenderAttributeValue::Text),
    }
}

fn route_location(table: &RouteTable, value: &Value) -> Result<RouteLocation, RenderError> {
    let Value::Variant {
        constructor,
        fields,
        ..
    } = value
    else {
        return Err(RenderError("Link target is not a Location variant".into()));
    };
    let declaration = table
        .constructors()
        .iter()
        .find(|candidate| candidate.name == *constructor)
        .ok_or_else(|| {
            RenderError(format!(
                "Location constructor `{constructor}` is absent from its Routes value"
            ))
        })?;
    if fields.len() != declaration.fields.len() {
        return Err(RenderError(format!(
            "Location constructor `{constructor}` has the wrong field arity"
        )));
    }
    let mut output = BTreeMap::new();
    for (index, declaration) in declaration.fields.iter().enumerate() {
        let (actual_name, value) = &fields[index];
        if actual_name
            .as_deref()
            .is_some_and(|name| name != declaration.name)
        {
            return Err(RenderError(format!(
                "Location field `{}` is out of declaration order",
                declaration.name
            )));
        }
        output.insert(
            declaration.name.clone(),
            route_field_value(&declaration.kind, value)?,
        );
    }
    Ok(RouteLocation::new(constructor, output))
}

fn route_field_value(kind: &RouteFieldKind, value: &Value) -> Result<RouteFieldValue, RenderError> {
    match kind {
        RouteFieldKind::Text => Ok(RouteFieldValue::Required(RouteAtom::Text {
            value: route_text(value)?,
        })),
        RouteFieldKind::TextKey { type_name } => {
            Ok(RouteFieldValue::Required(route_key(type_name, value)?))
        }
        RouteFieldKind::OptionalText => {
            Ok(RouteFieldValue::Optional(option_route_atom(value, None)?))
        }
        RouteFieldKind::OptionalTextKey { type_name } => Ok(RouteFieldValue::Optional(
            option_route_atom(value, Some(type_name))?,
        )),
    }
}

fn option_route_atom(
    value: &Value,
    key_type: Option<&PortTypeRef>,
) -> Result<Option<RouteAtom>, RenderError> {
    let Value::Variant {
        constructor,
        fields,
        ..
    } = value
    else {
        return Err(RenderError("optional route field is not Option".into()));
    };
    match constructor.as_str() {
        "none" if fields.is_empty() => Ok(None),
        "some" if fields.len() == 1 => match key_type {
            Some(type_name) => Ok(Some(route_key(type_name, &fields[0].1)?)),
            None => Ok(Some(RouteAtom::Text {
                value: route_text(&fields[0].1)?,
            })),
        },
        _ => Err(RenderError("optional route field is ill-shaped".into())),
    }
}

fn route_key(type_name: &PortTypeRef, value: &Value) -> Result<RouteAtom, RenderError> {
    let Value::Key { type_id, value } = value else {
        return Err(RenderError("route key field is not nominal".into()));
    };
    if type_id != type_name.as_str() {
        return Err(RenderError(format!(
            "route key `{type_id}` does not match `{type_name}`"
        )));
    }
    Ok(RouteAtom::Key {
        type_name: type_name.clone(),
        value: route_text(value)?,
    })
}

fn route_text(value: &Value) -> Result<String, RenderError> {
    match value {
        Value::Text(value) => Ok(value.clone()),
        Value::Key { value, .. } => route_text(value),
        _ => Err(RenderError("route component is not Text-backed".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    use crate::ir::{Pattern, Presentation, SourceRef, TypeDef, TypeRef, UiAttribute};
    use crate::runtime::InstanceLifecycle;

    const MACHINE: &str = "test.render@1::Rows";
    const PRESENTATION: &str = "test.render@1::Web";

    fn duplicate_each_program(children: Vec<UiNode>, keys: &[i64]) -> (Program, Instance) {
        let source = SourceRef::synthetic("test/rows");
        let mut program = Program::new();
        program.machine_program.machines.insert(
            MACHINE.into(),
            Machine {
                id: MACHINE.into(),
                config: TypeRef::Unit,
                requires: Vec::new(),
                ports: Vec::new(),
                local_input: TypeDef::Sum {
                    id: format!("{MACHINE}.Input"),
                    constructors: Vec::new(),
                },
                local_commands: Vec::new(),
                outcomes: Vec::new(),
                state: Vec::new(),
                functions: BTreeMap::new(),
                derives: Vec::new(),
                invariants: Vec::new(),
                observation: Vec::new(),
                transitions: BTreeMap::new(),
                handlers: BTreeMap::new(),
                before_commit: Vec::new(),
                source: source.clone(),
            },
        );
        program.presentations.insert(
            PRESENTATION.into(),
            Presentation {
                id: PRESENTATION.into(),
                machine: MACHINE.into(),
                binding: "rows".into(),
                nodes: vec![UiNode::Each {
                    value: Expr::Name {
                        name: "rows".into(),
                    },
                    pattern: Pattern::Tuple {
                        values: vec![
                            Pattern::Bind { name: "key".into() },
                            Pattern::Bind {
                                name: "label".into(),
                            },
                        ],
                    },
                    key: Box::new(Expr::Name { name: "key".into() }),
                    children,
                    source: source.clone(),
                }],
                source,
            },
        );
        let observation = Value::Seq(
            keys.iter()
                .enumerate()
                .map(|(index, key)| {
                    Value::Tuple(vec![Value::int(*key), Value::Text(format!("row-{index}"))])
                })
                .collect(),
        );
        let instance = Instance {
            id: "render/rows".into(),
            machine: MACHINE.into(),
            program_hash: "00".repeat(32),
            configuration: Value::Unit,
            state: Value::Record(Vec::new()),
            observation,
            inbox: VecDeque::new(),
            lifecycle: InstanceLifecycle::Running,
            next_sequence: 0,
            trace_prefix_hash: "00".repeat(32),
            receipts: Vec::new(),
            ingress_prefix_hash: "00".repeat(32),
            next_ingress_ordinal: 0,
            ingress_records: Vec::new(),
        };
        (program, instance)
    }

    fn keyed_surface_program(surface_count: usize) -> (Program, Instance) {
        let (mut program, mut instance) = duplicate_each_program(Vec::new(), &[]);
        let presentation = program.presentations.get_mut(PRESENTATION).unwrap();
        presentation.binding = "surface_key".into();
        presentation.nodes = (0..surface_count)
            .map(|index| UiNode::Element {
                name: "Surface".into(),
                attributes: vec![UiAttribute {
                    name: "key".into(),
                    value: UiAttributeValue::Expression {
                        value: Expr::Name {
                            name: "surface_key".into(),
                        },
                    },
                    source: SourceRef::synthetic(format!("test/surface-{index}/key")),
                }],
                children: Vec::new(),
                source: SourceRef::synthetic(format!("test/surface-{index}")),
            })
            .collect();
        instance.observation = Value::Text("first".into());
        (program, instance)
    }

    #[test]
    fn display_never_rounds_exact_values() {
        assert_eq!(
            display_value(&Value::int(
                "900719925474099312345"
                    .parse::<num_bigint::BigInt>()
                    .unwrap()
            ))
            .unwrap(),
            "900719925474099312345"
        );
    }

    #[test]
    fn keys_are_stable_and_path_sensitive() {
        let source = SourceRef::synthetic("web/paragraph");
        assert_eq!(
            node_key(&source.id, "root.0"),
            node_key(&source.id, "root.0")
        );
        assert_ne!(
            node_key(&source.id, "root.0"),
            node_key(&source.id, "root.1")
        );
    }

    #[test]
    fn surface_node_identity_includes_the_evaluated_checked_key() {
        let (program, mut instance) = keyed_surface_program(1);
        let first = program.project(&instance, PRESENTATION).unwrap();
        instance.observation = Value::Text("second".into());
        let second = program.project(&instance, PRESENTATION).unwrap();
        let RenderNode::Element {
            key: first_key,
            surface: true,
            ..
        } = &first.document.nodes[0]
        else {
            panic!("expected first Surface projection")
        };
        let RenderNode::Element {
            key: second_key,
            surface: true,
            ..
        } = &second.document.nodes[0]
        else {
            panic!("expected second Surface projection")
        };
        assert_ne!(first_key, second_key);
        assert_eq!(
            first.sources.nodes[first_key].id,
            second.sources.nodes[second_key].id
        );
    }

    #[test]
    fn surface_lifetime_key_survives_source_and_render_path_changes() {
        let (mut program, instance) = keyed_surface_program(1);
        let first = program.project(&instance, PRESENTATION).unwrap();
        let RenderNode::Element {
            key: first_key,
            surface: true,
            ..
        } = &first.document.nodes[0]
        else {
            panic!("expected first Surface projection")
        };

        let presentation = program.presentations.get_mut(PRESENTATION).unwrap();
        let UiNode::Element { source, .. } = &mut presentation.nodes[0] else {
            panic!("expected authored Surface")
        };
        *source = SourceRef::synthetic("test/moved-surface");
        presentation.nodes.insert(
            0,
            UiNode::Text {
                value: "before".into(),
                source: SourceRef::synthetic("test/before-surface"),
            },
        );

        let second = program.project(&instance, PRESENTATION).unwrap();
        let RenderNode::Element {
            key: second_key,
            surface: true,
            ..
        } = &second.document.nodes[1]
        else {
            panic!("expected moved Surface projection")
        };
        assert_eq!(first_key, second_key);
        assert_eq!(second.sources.nodes[second_key].id, "test/moved-surface");
    }

    #[test]
    fn surface_key_still_rejects_duplicate_lifetimes_in_one_projection() {
        let (program, instance) = keyed_surface_program(2);
        assert_eq!(
            program
                .project(&instance, PRESENTATION)
                .unwrap_err()
                .to_string(),
            "one projection contains duplicate Surface keys"
        );
    }

    #[test]
    fn event_bindings_capture_only_expression_names_that_are_current_locals() {
        let large_unused_model = Value::Seq((0..4_096).map(Value::int).collect());
        let locals = BTreeMap::from([
            ("call_symbol".into(), Value::Text("not a local call".into())),
            (
                "call_type_symbol".into(),
                Value::Text("not a local type".into()),
            ),
            (
                "method_symbol".into(),
                Value::Text("not a local method".into()),
            ),
            (
                "method_type_symbol".into(),
                Value::Text("not a local type".into()),
            ),
            ("required".into(), Value::Text("argument".into())),
            ("row".into(), Value::Text("selected row".into())),
            ("unused_model".into(), large_unused_model),
        ]);
        let input = Expr::Call {
            function: "call_symbol".into(),
            args: vec![Expr::Method {
                value: Box::new(Expr::Name { name: "row".into() }),
                method: "method_symbol".into(),
                args: vec![Expr::Name {
                    name: "required".into(),
                }],
                result_type: TypeRef::Named {
                    id: "method_type_symbol".into(),
                },
            }],
            result_type: TypeRef::Named {
                id: "call_type_symbol".into(),
            },
        };

        assert_eq!(
            event_binding_locals(&input, &locals),
            BTreeMap::from([
                ("required".into(), Value::Text("argument".into())),
                ("row".into(), Value::Text("selected row".into())),
            ])
        );
    }

    #[test]
    fn each_rejects_duplicate_keys_before_rendering_text_rows() {
        let (program, instance) = duplicate_each_program(
            vec![UiNode::Interpolation {
                value: Expr::Name {
                    name: "label".into(),
                },
                source: SourceRef::synthetic("test/label"),
            }],
            &[1, 1],
        );
        assert_eq!(
            program
                .project(&instance, PRESENTATION)
                .unwrap_err()
                .to_string(),
            "one UI each expansion contains duplicate keys",
        );
    }

    #[test]
    fn each_rejects_duplicate_eventful_rows_and_accepts_unique_keys() {
        let children = vec![UiNode::Element {
            name: "button".into(),
            attributes: vec![UiAttribute {
                name: "on-click".into(),
                value: UiAttributeValue::Event {
                    event: "click".into(),
                    input: Expr::Name {
                        name: "label".into(),
                    },
                },
                source: SourceRef::synthetic("test/click"),
            }],
            children: vec![UiNode::Text {
                value: "Choose".into(),
                source: SourceRef::synthetic("test/button-label"),
            }],
            source: SourceRef::synthetic("test/button"),
        }];
        let (program, duplicate) = duplicate_each_program(children.clone(), &[7, 7]);
        assert_eq!(
            program
                .project(&duplicate, PRESENTATION)
                .unwrap_err()
                .to_string(),
            "one UI each expansion contains duplicate keys",
        );

        let (program, unique) = duplicate_each_program(children, &[7, 8]);
        let projection = program.project(&unique, PRESENTATION).unwrap();
        assert_eq!(projection.document.nodes.len(), 2);
        assert_eq!(projection.bindings.len(), 2);
        assert!(projection.bindings.values().all(|binding| {
            binding.locals.keys().cloned().collect::<Vec<_>>() == vec!["label".to_string()]
        }));
        assert_eq!(projection.sources.protocol, PROJECTION_SOURCES_PROTOCOL);
        assert_eq!(projection.sources.presentation, PRESENTATION);
        assert_eq!(projection.sources.nodes.len(), 4);
        assert!(
            projection
                .sources
                .nodes
                .values()
                .all(|source| source.id == "test/button" || source.id == "test/button-label")
        );
    }

    #[test]
    fn captured_row_and_model_locals_resolve_with_the_runtime_event() {
        let children = vec![UiNode::Element {
            name: "button".into(),
            attributes: vec![UiAttribute {
                name: "on-click".into(),
                value: UiAttributeValue::Event {
                    event: "click".into(),
                    input: Expr::Tuple {
                        values: vec![
                            Expr::Name { name: "key".into() },
                            Expr::Name {
                                name: "label".into(),
                            },
                            Expr::Name {
                                name: "rows".into(),
                            },
                            Expr::Name {
                                name: "event".into(),
                            },
                        ],
                    },
                },
                source: SourceRef::synthetic("test/click"),
            }],
            children: Vec::new(),
            source: SourceRef::synthetic("test/button"),
        }];
        let (program, instance) = duplicate_each_program(children, &[11]);
        let expected_model = instance.observation.clone();
        let projection = program.project(&instance, PRESENTATION).unwrap();
        let (binding_id, binding) = projection.bindings.iter().next().unwrap();

        assert_eq!(
            binding.locals.keys().cloned().collect::<Vec<_>>(),
            vec!["key".to_string(), "label".to_string(), "rows".to_string()]
        );
        assert_eq!(
            program
                .resolve_ui_input(
                    &instance,
                    &projection,
                    binding_id,
                    Value::Text("click payload".into()),
                )
                .unwrap(),
            Value::Tuple(vec![
                Value::int(11),
                Value::Text("row-0".into()),
                expected_model,
                Value::Text("click payload".into()),
            ])
        );
    }
}
