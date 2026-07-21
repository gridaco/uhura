use std::fmt;

use num_bigint::Sign;

use super::codec::{frame, nat, nat_u64};
use super::ir::{ConstructorDef, Machine, MachineProgram, PortDef, TypeDef, TypeRef};
use super::value::{BoundaryNumber, Decimal, IntegerKind, Value};

fn signed_integer_body(value: &num_bigint::BigInt) -> Vec<u8> {
    let (sign, magnitude) = value.to_bytes_be();
    let mut body = Vec::with_capacity(magnitude.len() + 1);
    body.push(match sign {
        Sign::Minus => 1,
        Sign::NoSign | Sign::Plus => 0,
    });
    body.extend_from_slice(&magnitude);
    body
}

fn decimal_body(value: &Decimal) -> Vec<u8> {
    frame(
        "decimal",
        &[
            signed_integer_body(value.coefficient()),
            nat_u64(u64::from(value.scale())),
        ],
    )
}

/// A deterministic failure to admit one runtime payload as a checked Uhura
/// type. Runtime values deliberately remain compact payloads; semantic type
/// identity is supplied by the checked boundary that owns the value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueTypeError {
    pub path: String,
    pub message: String,
}

impl ValueTypeError {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ValueTypeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ValueTypeError {}

impl MachineProgram {
    /// Validates and canonicalizes a payload against one checked Uhura type.
    ///
    /// In particular, collection identity never comes from the first element:
    /// an empty or nested collection is interpreted solely through `expected`.
    pub fn canonicalize_value(
        &self,
        expected: &TypeRef,
        value: &Value,
    ) -> Result<Value, ValueTypeError> {
        self.canonicalize_value_at(expected, value, "$".into())
    }

    pub fn validate_value(&self, expected: &TypeRef, value: &Value) -> Result<(), ValueTypeError> {
        self.canonicalize_value(expected, value).map(|_| ())
    }

    /// Complete typed canonical value bytes used by semantic hashes.
    pub fn canonical_value_bytes(
        &self,
        expected: &TypeRef,
        value: &Value,
    ) -> Result<Vec<u8>, ValueTypeError> {
        let value = self.canonicalize_value(expected, value)?;
        self.canonical_value_bytes_unchecked(expected, &value, "$".into())
    }

    /// Strict tagged-JSON decoding with the checked type supplied out of band.
    /// Re-encoding the canonicalized value must reproduce the exact JSON tree,
    /// which rejects noncanonical numerics and collection ordering.
    pub fn decode_wire_value(
        &self,
        expected: &TypeRef,
        json: &serde_json::Value,
    ) -> Result<Value, ValueTypeError> {
        let decoded = Value::from_wire_json(json)
            .map_err(|error| ValueTypeError::new("$", error.to_string()))?;
        let canonical = self.canonicalize_value(expected, &decoded)?;
        if canonical.to_wire_json() != *json {
            return Err(ValueTypeError::new(
                "$",
                "value is not canonical exact tagged Uhura JSON",
            ));
        }
        Ok(canonical)
    }

    pub(crate) fn machine_state_type(machine: &Machine) -> TypeRef {
        TypeRef::Record {
            fields: machine
                .state
                .iter()
                .map(|field| (field.name.clone(), field.ty.clone()))
                .collect(),
        }
    }

    pub(crate) fn machine_observation_type(machine: &Machine) -> TypeRef {
        TypeRef::Record {
            fields: machine
                .observation
                .iter()
                .map(|field| (field.name.clone(), field.ty.clone()))
                .collect(),
        }
    }

    pub fn canonicalize_input(
        &self,
        machine: &Machine,
        value: &Value,
    ) -> Result<Value, ValueTypeError> {
        let (type_id, constructor, fields) = variant_parts(value)?;
        if let Some((port, case)) = qualified_port_case(machine, constructor) {
            let expected_id = format!("{}::port.{}.Receive", machine.id, port.name);
            if type_id != expected_id {
                return Err(ValueTypeError::new(
                    "$",
                    format!("input type is `{type_id}`, expected `{expected_id}`"),
                ));
            }
            let definition = find_constructor(&port.receive, case, "input")?;
            let fields = self.canonicalize_constructor_fields(definition, fields, "$".into())?;
            return Ok(Value::variant(expected_id, constructor, fields));
        }

        let TypeDef::Sum { id, constructors } = &machine.local_input else {
            return Err(ValueTypeError::new(
                "$",
                "machine local input is not a closed sum",
            ));
        };
        if type_id != id {
            return Err(ValueTypeError::new(
                "$",
                format!("input type is `{type_id}`, expected `{id}`"),
            ));
        }
        let definition = find_constructor(constructors, constructor, "input")?;
        let fields = self.canonicalize_constructor_fields(definition, fields, "$".into())?;
        Ok(Value::variant(id, constructor, fields))
    }

    pub(crate) fn canonical_input_bytes(
        &self,
        machine: &Machine,
        value: &Value,
    ) -> Result<Vec<u8>, ValueTypeError> {
        let value = self.canonicalize_input(machine, value)?;
        let (type_id, constructor, fields) = variant_parts(&value)?;
        if let Some((port, case)) = qualified_port_case(machine, constructor) {
            return self.canonical_variant_bytes(type_id, &port.receive, case, fields, "$".into());
        }
        let TypeDef::Sum { constructors, .. } = &machine.local_input else {
            unreachable!("canonicalized local input is a sum")
        };
        self.canonical_variant_bytes(type_id, constructors, constructor, fields, "$".into())
    }

    pub(crate) fn canonicalize_command(
        &self,
        machine: &Machine,
        value: &Value,
    ) -> Result<Value, ValueTypeError> {
        let (type_id, constructor, fields) = variant_parts(value)?;
        if let Some((port, case)) = qualified_port_case(machine, constructor) {
            let expected_id = format!("{}::port.{}.Send", machine.id, port.name);
            if type_id != expected_id {
                return Err(ValueTypeError::new(
                    "$",
                    format!("command type is `{type_id}`, expected `{expected_id}`"),
                ));
            }
            let definition = find_constructor(&port.send, case, "command")?;
            let fields = self.canonicalize_constructor_fields(definition, fields, "$".into())?;
            return Ok(Value::variant(expected_id, constructor, fields));
        }

        let expected_id = format!("{}.Command", machine.id);
        if type_id != expected_id {
            return Err(ValueTypeError::new(
                "$",
                format!("command type is `{type_id}`, expected `{expected_id}`"),
            ));
        }
        let definitions = machine
            .local_commands
            .iter()
            .map(|command| command.constructor.clone())
            .collect::<Vec<_>>();
        let definition = find_constructor(&definitions, constructor, "command")?;
        let fields = self.canonicalize_constructor_fields(definition, fields, "$".into())?;
        Ok(Value::variant(expected_id, constructor, fields))
    }

    pub(crate) fn canonical_command_bytes(
        &self,
        machine: &Machine,
        value: &Value,
    ) -> Result<Vec<u8>, ValueTypeError> {
        let value = self.canonicalize_command(machine, value)?;
        let (type_id, constructor, fields) = variant_parts(&value)?;
        if let Some((port, case)) = qualified_port_case(machine, constructor) {
            return self.canonical_variant_bytes(type_id, &port.send, case, fields, "$".into());
        }
        let definitions = machine
            .local_commands
            .iter()
            .map(|command| command.constructor.clone())
            .collect::<Vec<_>>();
        self.canonical_variant_bytes(type_id, &definitions, constructor, fields, "$".into())
    }

    pub(crate) fn canonicalize_outcome(
        &self,
        machine: &Machine,
        value: &Value,
    ) -> Result<Value, ValueTypeError> {
        let expected_id = format!("{}.Outcome", machine.id);
        let (type_id, constructor, fields) = variant_parts(value)?;
        if type_id != expected_id {
            return Err(ValueTypeError::new(
                "$",
                format!("outcome type is `{type_id}`, expected `{expected_id}`"),
            ));
        }
        let definitions = machine
            .outcomes
            .iter()
            .map(|outcome| outcome.constructor.clone())
            .collect::<Vec<_>>();
        let definition = find_constructor(&definitions, constructor, "outcome")?;
        let fields = self.canonicalize_constructor_fields(definition, fields, "$".into())?;
        Ok(Value::variant(expected_id, constructor, fields))
    }

    pub(crate) fn canonical_outcome_bytes(
        &self,
        machine: &Machine,
        value: &Value,
    ) -> Result<Vec<u8>, ValueTypeError> {
        let value = self.canonicalize_outcome(machine, value)?;
        let (type_id, constructor, fields) = variant_parts(&value)?;
        let definitions = machine
            .outcomes
            .iter()
            .map(|outcome| outcome.constructor.clone())
            .collect::<Vec<_>>();
        self.canonical_variant_bytes(type_id, &definitions, constructor, fields, "$".into())
    }

    fn canonicalize_value_at(
        &self,
        expected: &TypeRef,
        value: &Value,
        path: String,
    ) -> Result<Value, ValueTypeError> {
        match expected {
            TypeRef::Bool => match value {
                Value::Bool(value) => Ok(Value::Bool(*value)),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Unit => match value {
                Value::Unit => Ok(Value::Unit),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Never | TypeRef::FiniteView { .. } => Err(ValueTypeError::new(
                path,
                format!(
                    "{} has no storable runtime value",
                    expected.canonical_name()
                ),
            )),
            TypeRef::Int | TypeRef::Nat | TypeRef::PositiveInt => {
                let expected_kind = match expected {
                    TypeRef::Int => IntegerKind::Int,
                    TypeRef::Nat => IntegerKind::Nat,
                    TypeRef::PositiveInt => IntegerKind::PositiveInt,
                    _ => unreachable!(),
                };
                match value {
                    Value::Integer {
                        kind,
                        value: integer,
                    } if *kind == expected_kind => Ok(Value::Integer {
                        kind: *kind,
                        value: integer.clone(),
                    }),
                    _ => Err(type_mismatch(&path, expected, value)),
                }
            }
            TypeRef::Decimal => match value {
                Value::Decimal(value) => Ok(Value::Decimal(value.clone())),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::BoundaryNumber => match value {
                Value::Boundary(value) => Ok(Value::Boundary(value.clone())),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Ratio => match value {
                Value::Ratio(value) => Ok(Value::Ratio(value.clone())),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Text => match value {
                Value::Text(value) => Ok(Value::Text(value.clone())),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Named { id } => self.canonicalize_named(id, value, path),
            TypeRef::Option { value: inner } => {
                let type_id = expected.canonical_name();
                let (actual_type, constructor, fields) = variant_parts(value)?;
                if actual_type != type_id {
                    return Err(ValueTypeError::new(
                        path,
                        format!("value type is `{actual_type}`, expected `{type_id}`"),
                    ));
                }
                match (constructor, fields) {
                    ("none", []) => Ok(Value::variant(type_id, "none", Vec::new())),
                    ("some", [(Some(name), value)]) if name == "value" => Ok(Value::variant(
                        type_id,
                        "some",
                        vec![(
                            Some("value".into()),
                            self.canonicalize_value_at(inner, value, format!("{path}.value"))?,
                        )],
                    )),
                    _ => Err(ValueTypeError::new(path, "ill-shaped Option constructor")),
                }
            }
            TypeRef::Seq { value: inner } => match value {
                Value::Seq(values) => Ok(Value::Seq(
                    values
                        .iter()
                        .enumerate()
                        .map(|(index, value)| {
                            self.canonicalize_value_at(inner, value, format!("{path}[{index}]"))
                        })
                        .collect::<Result<_, _>>()?,
                )),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::NonEmpty { value: inner } => match value {
                Value::NonEmpty(values) if !values.is_empty() => Ok(Value::NonEmpty(
                    values
                        .iter()
                        .enumerate()
                        .map(|(index, value)| {
                            self.canonicalize_value_at(inner, value, format!("{path}[{index}]"))
                        })
                        .collect::<Result<_, _>>()?,
                )),
                Value::NonEmpty(_) => Err(ValueTypeError::new(path, "NonEmpty cannot be empty")),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Set { value: inner } => match value {
                Value::Set(values) => {
                    let mut values = values
                        .iter()
                        .enumerate()
                        .map(|(index, value)| {
                            self.canonicalize_value_at(inner, value, format!("{path}[{index}]"))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    values.sort_by_cached_key(|value| {
                        self.canonical_value_bytes_unchecked(inner, value, path.clone())
                            .expect("canonicalized set member encodes")
                    });
                    if values.windows(2).any(|pair| pair[0] == pair[1]) {
                        return Err(ValueTypeError::new(path, "Set contains a duplicate value"));
                    }
                    Ok(Value::Set(values))
                }
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Map { key, value: inner } => match value {
                Value::Map(entries) => {
                    let mut entries = entries
                        .iter()
                        .enumerate()
                        .map(|(index, (entry_key, entry_value))| {
                            Ok((
                                self.canonicalize_value_at(
                                    key,
                                    entry_key,
                                    format!("{path}[{index}].key"),
                                )?,
                                self.canonicalize_value_at(
                                    inner,
                                    entry_value,
                                    format!("{path}[{index}].value"),
                                )?,
                            ))
                        })
                        .collect::<Result<Vec<_>, ValueTypeError>>()?;
                    entries.sort_by_cached_key(|(key_value, _)| {
                        self.canonical_value_bytes_unchecked(key, key_value, path.clone())
                            .expect("canonicalized map key encodes")
                    });
                    for pair in entries.windows(2) {
                        let left =
                            self.canonical_value_bytes_unchecked(key, &pair[0].0, path.clone())?;
                        let right =
                            self.canonical_value_bytes_unchecked(key, &pair[1].0, path.clone())?;
                        if left == right {
                            return Err(ValueTypeError::new(path, "Map contains a duplicate key"));
                        }
                    }
                    Ok(Value::Map(entries))
                }
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Table { key, value: inner } => match value {
                Value::Table { key_type, entries } => {
                    let expected_key = key.canonical_name();
                    if key_type != &expected_key {
                        return Err(ValueTypeError::new(
                            path,
                            format!("Table key type is `{key_type}`, expected `{expected_key}`"),
                        ));
                    }
                    let constructors = self.nullary_constructors(key)?;
                    if entries.len() != constructors.len() {
                        return Err(ValueTypeError::new(
                            path,
                            format!(
                                "Table has {} entries, expected {}",
                                entries.len(),
                                constructors.len()
                            ),
                        ));
                    }
                    let mut output = Vec::with_capacity(entries.len());
                    for (index, ((actual, value), expected_name)) in
                        entries.iter().zip(&constructors).enumerate()
                    {
                        if actual != expected_name {
                            return Err(ValueTypeError::new(
                                format!("{path}[{index}]"),
                                format!(
                                    "Table key is `{actual}`, expected declaration-order key `{expected_name}`"
                                ),
                            ));
                        }
                        output.push((
                            actual.clone(),
                            self.canonicalize_value_at(inner, value, format!("{path}.{actual}"))?,
                        ));
                    }
                    Ok(Value::Table {
                        key_type: expected_key,
                        entries: output,
                    })
                }
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Tuple {
                values: expected_values,
            } => match value {
                Value::Tuple(values) if values.len() == expected_values.len() => Ok(Value::Tuple(
                    expected_values
                        .iter()
                        .zip(values)
                        .enumerate()
                        .map(|(index, (expected, value))| {
                            self.canonicalize_value_at(expected, value, format!("{path}[{index}]"))
                        })
                        .collect::<Result<_, _>>()?,
                )),
                Value::Tuple(values) => Err(ValueTypeError::new(
                    path,
                    format!(
                        "tuple has {} values, expected {}",
                        values.len(),
                        expected_values.len()
                    ),
                )),
                _ => Err(type_mismatch(&path, expected, value)),
            },
            TypeRef::Record { fields } => self.canonicalize_record(fields, value, path),
        }
    }

    fn canonicalize_named(
        &self,
        id: &str,
        value: &Value,
        path: String,
    ) -> Result<Value, ValueTypeError> {
        if let Some(definition) = self.types.get(id) {
            return match definition {
                TypeDef::Key { underlying, .. } => match value {
                    Value::Key {
                        type_id,
                        value: inner,
                    } if type_id == id => Ok(Value::Key {
                        type_id: id.into(),
                        value: Box::new(self.canonicalize_value_at(
                            underlying,
                            inner,
                            format!("{path}.value"),
                        )?),
                    }),
                    _ => Err(ValueTypeError::new(
                        path,
                        format!("value is not nominal key `{id}`"),
                    )),
                },
                TypeDef::Record { fields, .. } => self.canonicalize_record(fields, value, path),
                TypeDef::Sum { constructors, .. } => {
                    let (actual_id, constructor, fields) = variant_parts(value)?;
                    if actual_id != id {
                        return Err(ValueTypeError::new(
                            path,
                            format!("sum type is `{actual_id}`, expected `{id}`"),
                        ));
                    }
                    let definition = find_constructor(constructors, constructor, "sum")?;
                    Ok(Value::variant(
                        id,
                        constructor,
                        self.canonicalize_constructor_fields(definition, fields, path)?,
                    ))
                }
            };
        }

        if let Some(inner) = generic_argument(id, "Token") {
            let inner = parse_canonical_type(inner)?;
            let (actual_id, constructor, fields) = variant_parts(value)?;
            if actual_id != id {
                return Err(ValueTypeError::new(
                    path,
                    format!("token type is `{actual_id}`, expected `{id}`"),
                ));
            }
            let field_type = match constructor {
                "known" => inner,
                "unknown" => TypeRef::Text,
                _ => {
                    return Err(ValueTypeError::new(
                        path,
                        format!("unknown Token constructor `{constructor}`"),
                    ));
                }
            };
            let [(Some(name), field)] = fields else {
                return Err(ValueTypeError::new(
                    path,
                    "Token needs one named `value` field",
                ));
            };
            if name != "value" {
                return Err(ValueTypeError::new(
                    path,
                    "Token field must be named `value`",
                ));
            }
            return Ok(Value::variant(
                id,
                constructor,
                vec![(
                    Some("value".into()),
                    self.canonicalize_value_at(&field_type, field, format!("{path}.value"))?,
                )],
            ));
        }

        // `Routes<Location>` is an opaque checked immutable host
        // configuration represented by its canonical route-table text.
        if generic_argument(id, "Routes").is_some() {
            return match value {
                Value::Text(text) => Ok(Value::Text(text.clone())),
                _ => Err(ValueTypeError::new(
                    path,
                    format!("opaque `{id}` configuration must be canonical Text"),
                )),
            };
        }

        Err(ValueTypeError::new(
            path,
            format!("unknown Uhura type `{id}`"),
        ))
    }

    fn canonicalize_record(
        &self,
        expected_fields: &[(String, TypeRef)],
        value: &Value,
        path: String,
    ) -> Result<Value, ValueTypeError> {
        let Value::Record(fields) = value else {
            return Err(ValueTypeError::new(path, "value is not a record"));
        };
        if fields.len() != expected_fields.len() {
            return Err(ValueTypeError::new(
                path,
                format!(
                    "record has {} fields, expected {}",
                    fields.len(),
                    expected_fields.len()
                ),
            ));
        }
        let mut output = Vec::with_capacity(fields.len());
        for ((actual_name, actual), (expected_name, expected)) in fields.iter().zip(expected_fields)
        {
            if actual_name != expected_name {
                return Err(ValueTypeError::new(
                    path,
                    format!(
                        "record field is `{actual_name}`, expected declaration-order field `{expected_name}`"
                    ),
                ));
            }
            output.push((
                actual_name.clone(),
                self.canonicalize_value_at(expected, actual, format!("{path}.{actual_name}"))?,
            ));
        }
        Ok(Value::Record(output))
    }

    fn canonicalize_constructor_fields(
        &self,
        definition: &ConstructorDef,
        fields: &[(Option<String>, Value)],
        path: String,
    ) -> Result<Vec<(Option<String>, Value)>, ValueTypeError> {
        if fields.len() != definition.fields.len() {
            return Err(ValueTypeError::new(
                path,
                format!(
                    "constructor `{}` has {} fields, expected {}",
                    definition.name,
                    fields.len(),
                    definition.fields.len()
                ),
            ));
        }
        fields
            .iter()
            .zip(&definition.fields)
            .enumerate()
            .map(|(index, ((actual_name, actual), (expected_name, expected)))| {
                if actual_name != expected_name {
                    return Err(ValueTypeError::new(
                        format!("{path}[{index}]"),
                        format!(
                            "constructor field name is {actual_name:?}, expected {expected_name:?}"
                        ),
                    ));
                }
                Ok((
                    actual_name.clone(),
                    self.canonicalize_value_at(
                        expected,
                        actual,
                        expected_name
                            .as_ref()
                            .map_or_else(|| format!("{path}[{index}]"), |name| format!("{path}.{name}")),
                    )?,
                ))
            })
            .collect()
    }

    fn canonical_value_bytes_unchecked(
        &self,
        expected: &TypeRef,
        value: &Value,
        path: String,
    ) -> Result<Vec<u8>, ValueTypeError> {
        let body = self.canonical_value_body(expected, value, path)?;
        Ok(frame(
            "value",
            &[canonical_type_identity_bytes(expected)?, body],
        ))
    }

    fn canonical_value_body(
        &self,
        expected: &TypeRef,
        value: &Value,
        path: String,
    ) -> Result<Vec<u8>, ValueTypeError> {
        match (expected, value) {
            (TypeRef::Unit, Value::Unit) => Ok(Vec::new()),
            (TypeRef::Bool, Value::Bool(value)) => Ok(vec![u8::from(*value)]),
            (TypeRef::Int | TypeRef::Nat | TypeRef::PositiveInt, Value::Integer { value, .. }) => {
                let (sign, magnitude) = value.to_bytes_be();
                if matches!(expected, TypeRef::Int) {
                    let mut body = Vec::with_capacity(magnitude.len() + 1);
                    body.push(match sign {
                        Sign::Minus => 1,
                        Sign::NoSign | Sign::Plus => 0,
                    });
                    body.extend_from_slice(&magnitude);
                    Ok(body)
                } else {
                    debug_assert_ne!(sign, Sign::Minus);
                    Ok(magnitude)
                }
            }
            (TypeRef::Decimal, Value::Decimal(value)) | (TypeRef::Ratio, Value::Ratio(value)) => {
                Ok(decimal_body(value))
            }
            (TypeRef::BoundaryNumber, Value::Boundary(value)) => Ok(match value {
                BoundaryNumber::Finite(value) => frame("variant", &[nat(0), decimal_body(value)]),
                BoundaryNumber::Nan => frame("variant", &[nat(1)]),
                BoundaryNumber::PositiveInfinity => frame("variant", &[nat(2)]),
                BoundaryNumber::NegativeInfinity => frame("variant", &[nat(3)]),
            }),
            (TypeRef::Text, Value::Text(value)) => Ok(value.as_bytes().to_vec()),
            (TypeRef::Named { id }, Value::Key { value, .. })
                if matches!(self.types.get(id), Some(TypeDef::Key { .. })) =>
            {
                let Some(TypeDef::Key { underlying, .. }) = self.types.get(id) else {
                    unreachable!()
                };
                self.canonical_value_body(underlying, value, format!("{path}.value"))
            }
            (TypeRef::Named { id }, Value::Record(fields))
                if matches!(self.types.get(id), Some(TypeDef::Record { .. })) =>
            {
                let Some(TypeDef::Record {
                    fields: expected_fields,
                    ..
                }) = self.types.get(id)
                else {
                    unreachable!()
                };
                self.record_body(expected_fields, fields, path)
            }
            (
                TypeRef::Named { id },
                Value::Variant {
                    constructor,
                    fields,
                    ..
                },
            ) if matches!(self.types.get(id), Some(TypeDef::Sum { .. })) => {
                let Some(TypeDef::Sum { constructors, .. }) = self.types.get(id) else {
                    unreachable!()
                };
                self.variant_body(constructors, constructor, fields, path)
            }
            (
                TypeRef::Named { id },
                Value::Variant {
                    constructor,
                    fields,
                    ..
                },
            ) if generic_argument(id, "Token").is_some() => {
                let inner = parse_canonical_type(generic_argument(id, "Token").unwrap())?;
                let field_type = if constructor == "known" {
                    inner
                } else {
                    TypeRef::Text
                };
                let ordinal = if constructor == "known" { 0 } else { 1 };
                Ok(frame(
                    "variant",
                    &[
                        nat(ordinal),
                        self.canonical_value_bytes_unchecked(
                            &field_type,
                            &fields[0].1,
                            format!("{path}.value"),
                        )?,
                    ],
                ))
            }
            (TypeRef::Named { id }, Value::Text(text))
                if generic_argument(id, "Routes").is_some() =>
            {
                Ok(text.as_bytes().to_vec())
            }
            (
                TypeRef::Option { value: inner },
                Value::Variant {
                    constructor,
                    fields,
                    ..
                },
            ) => Ok(if constructor == "none" {
                frame("variant", &[nat(0)])
            } else {
                frame(
                    "variant",
                    &[
                        nat(1),
                        self.canonical_value_bytes_unchecked(
                            inner,
                            &fields[0].1,
                            format!("{path}.value"),
                        )?,
                    ],
                )
            }),
            (
                TypeRef::Tuple {
                    values: expected_values,
                },
                Value::Tuple(values),
            ) => Ok(frame(
                "tuple",
                &expected_values
                    .iter()
                    .zip(values)
                    .enumerate()
                    .map(|(index, (expected, value))| {
                        self.canonical_value_bytes_unchecked(
                            expected,
                            value,
                            format!("{path}[{index}]"),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            (TypeRef::Record { fields: expected }, Value::Record(fields)) => {
                self.record_body(expected, fields, path)
            }
            (TypeRef::Seq { value: inner }, Value::Seq(values))
            | (TypeRef::NonEmpty { value: inner }, Value::NonEmpty(values)) => {
                let mut parts = Vec::with_capacity(values.len() + 1);
                parts.push(nat(values.len()));
                for (index, value) in values.iter().enumerate() {
                    parts.push(self.canonical_value_bytes_unchecked(
                        inner,
                        value,
                        format!("{path}[{index}]"),
                    )?);
                }
                Ok(frame("items", &parts))
            }
            (TypeRef::Set { value: inner }, Value::Set(values)) => {
                let mut encoded = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        self.canonical_value_bytes_unchecked(
                            inner,
                            value,
                            format!("{path}[{index}]"),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                encoded.sort();
                let mut parts = vec![nat(encoded.len())];
                parts.extend(encoded);
                Ok(frame("items", &parts))
            }
            (TypeRef::Map { key, value: inner }, Value::Map(entries)) => {
                let mut encoded = entries
                    .iter()
                    .enumerate()
                    .map(|(index, (entry_key, entry_value))| {
                        Ok((
                            self.canonical_value_bytes_unchecked(
                                key,
                                entry_key,
                                format!("{path}[{index}].key"),
                            )?,
                            self.canonical_value_bytes_unchecked(
                                inner,
                                entry_value,
                                format!("{path}[{index}].value"),
                            )?,
                        ))
                    })
                    .collect::<Result<Vec<_>, ValueTypeError>>()?;
                encoded.sort_by(|left, right| left.0.cmp(&right.0));
                Ok(frame(
                    "map",
                    &encoded
                        .into_iter()
                        .map(|(key, value)| frame("entry", &[key, value]))
                        .collect::<Vec<_>>(),
                ))
            }
            (TypeRef::Table { value: inner, .. }, Value::Table { entries, .. }) => Ok(frame(
                "table",
                &entries
                    .iter()
                    .map(|(name, value)| {
                        Ok(frame(
                            "slot",
                            &[
                                name.as_bytes().to_vec(),
                                self.canonical_value_bytes_unchecked(
                                    inner,
                                    value,
                                    format!("{path}.{name}"),
                                )?,
                            ],
                        ))
                    })
                    .collect::<Result<Vec<_>, ValueTypeError>>()?,
            )),
            _ => Err(type_mismatch(&path, expected, value)),
        }
    }

    fn record_body(
        &self,
        expected: &[(String, TypeRef)],
        fields: &[(String, Value)],
        path: String,
    ) -> Result<Vec<u8>, ValueTypeError> {
        Ok(frame(
            "record",
            &expected
                .iter()
                .zip(fields)
                .map(|((name, expected), (_, value))| {
                    Ok(frame(
                        "field",
                        &[
                            name.as_bytes().to_vec(),
                            self.canonical_value_bytes_unchecked(
                                expected,
                                value,
                                format!("{path}.{name}"),
                            )?,
                        ],
                    ))
                })
                .collect::<Result<Vec<_>, ValueTypeError>>()?,
        ))
    }

    fn variant_body(
        &self,
        definitions: &[ConstructorDef],
        constructor: &str,
        fields: &[(Option<String>, Value)],
        path: String,
    ) -> Result<Vec<u8>, ValueTypeError> {
        let ordinal = definitions
            .iter()
            .position(|definition| definition.name == constructor)
            .ok_or_else(|| {
                ValueTypeError::new(&path, format!("unknown constructor `{constructor}`"))
            })?;
        let definition = &definitions[ordinal];
        let mut parts = vec![nat(ordinal)];
        for (index, ((name, expected), (_, value))) in
            definition.fields.iter().zip(fields).enumerate()
        {
            parts.push(self.canonical_value_bytes_unchecked(
                expected,
                value,
                name.as_ref().map_or_else(
                    || format!("{path}[{index}]"),
                    |name| format!("{path}.{name}"),
                ),
            )?);
        }
        Ok(frame("variant", &parts))
    }

    fn canonical_variant_bytes(
        &self,
        type_id: &str,
        definitions: &[ConstructorDef],
        constructor: &str,
        fields: &[(Option<String>, Value)],
        path: String,
    ) -> Result<Vec<u8>, ValueTypeError> {
        Ok(frame(
            "value",
            &[
                type_id.as_bytes().to_vec(),
                self.variant_body(definitions, constructor, fields, path)?,
            ],
        ))
    }

    fn nullary_constructors(&self, ty: &TypeRef) -> Result<Vec<String>, ValueTypeError> {
        let TypeRef::Named { id } = ty else {
            return Err(ValueTypeError::new(
                "$",
                "Table key must be a named closed sum",
            ));
        };
        let Some(TypeDef::Sum { constructors, .. }) = self.types.get(id) else {
            return Err(ValueTypeError::new(
                "$",
                format!("Table key `{id}` is not a closed sum"),
            ));
        };
        if constructors
            .iter()
            .any(|constructor| !constructor.fields.is_empty())
        {
            return Err(ValueTypeError::new(
                "$",
                format!("Table key `{id}` has a non-nullary constructor"),
            ));
        }
        Ok(constructors
            .iter()
            .map(|constructor| constructor.name.clone())
            .collect())
    }
}

/// Recursive canonical TypeId bytes. Declared and builtin identities are
/// exact UTF-8 identities; instantiated and structural types frame their
/// constructor and component identities instead of relying on display text.
pub fn canonical_type_identity_bytes(ty: &TypeRef) -> Result<Vec<u8>, ValueTypeError> {
    match ty {
        TypeRef::Bool
        | TypeRef::Unit
        | TypeRef::Never
        | TypeRef::Int
        | TypeRef::Nat
        | TypeRef::PositiveInt
        | TypeRef::Decimal
        | TypeRef::BoundaryNumber
        | TypeRef::Ratio
        | TypeRef::Text => Ok(ty.canonical_name().into_bytes()),
        TypeRef::Named { id } => {
            for constructor in ["Token", "Routes"] {
                if let Some(inner) = generic_argument(id, constructor) {
                    return Ok(frame(
                        "type-application",
                        &[
                            constructor.as_bytes().to_vec(),
                            canonical_type_identity_bytes(&parse_canonical_type(inner)?)?,
                        ],
                    ));
                }
            }
            Ok(id.as_bytes().to_vec())
        }
        TypeRef::Option { value }
        | TypeRef::Seq { value }
        | TypeRef::NonEmpty { value }
        | TypeRef::Set { value }
        | TypeRef::FiniteView { value } => {
            let constructor = match ty {
                TypeRef::Option { .. } => "Option",
                TypeRef::Seq { .. } => "Seq",
                TypeRef::NonEmpty { .. } => "NonEmpty",
                TypeRef::Set { .. } => "Set",
                TypeRef::FiniteView { .. } => "FiniteView",
                _ => unreachable!(),
            };
            Ok(frame(
                "type-application",
                &[
                    constructor.as_bytes().to_vec(),
                    canonical_type_identity_bytes(value)?,
                ],
            ))
        }
        TypeRef::Map { key, value } | TypeRef::Table { key, value } => {
            let constructor = if matches!(ty, TypeRef::Map { .. }) {
                "Map"
            } else {
                "Table"
            };
            Ok(frame(
                "type-application",
                &[
                    constructor.as_bytes().to_vec(),
                    canonical_type_identity_bytes(key)?,
                    canonical_type_identity_bytes(value)?,
                ],
            ))
        }
        TypeRef::Tuple { values } => Ok(frame(
            "tuple-type",
            &values
                .iter()
                .map(canonical_type_identity_bytes)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        TypeRef::Record { fields } => Ok(frame(
            "record-type",
            &fields
                .iter()
                .map(|(name, ty)| {
                    Ok(frame(
                        "field",
                        &[name.as_bytes().to_vec(), canonical_type_identity_bytes(ty)?],
                    ))
                })
                .collect::<Result<Vec<_>, ValueTypeError>>()?,
        )),
    }
}

type VariantParts<'a> = (&'a str, &'a str, &'a [(Option<String>, Value)]);

fn variant_parts(value: &Value) -> Result<VariantParts<'_>, ValueTypeError> {
    match value {
        Value::Variant {
            type_id,
            constructor,
            fields,
        } => Ok((type_id, constructor, fields)),
        _ => Err(ValueTypeError::new(
            "$",
            "value is not a closed sum constructor",
        )),
    }
}

fn find_constructor<'a>(
    constructors: &'a [ConstructorDef],
    name: &str,
    family: &str,
) -> Result<&'a ConstructorDef, ValueTypeError> {
    constructors
        .iter()
        .find(|constructor| constructor.name == name)
        .ok_or_else(|| ValueTypeError::new("$", format!("unknown {family} constructor `{name}`")))
}

fn type_mismatch(path: &str, expected: &TypeRef, value: &Value) -> ValueTypeError {
    ValueTypeError::new(
        path,
        format!(
            "expected `{}`, got runtime `{}`",
            expected.canonical_name(),
            value.type_identity()
        ),
    )
}

fn qualified_port_case<'a>(
    machine: &'a Machine,
    constructor: &'a str,
) -> Option<(&'a PortDef, &'a str)> {
    machine.ports.iter().find_map(|port| {
        let suffix = constructor
            .strip_prefix(port.name.as_str())?
            .strip_prefix('.')?;
        (!suffix.is_empty() && !suffix.contains('.')).then_some((port, suffix))
    })
}

fn generic_argument<'a>(id: &'a str, constructor: &str) -> Option<&'a str> {
    let prefix = format!("{constructor}<");
    id.strip_prefix(&prefix)?.strip_suffix('>')
}

fn parse_canonical_type(source: &str) -> Result<TypeRef, ValueTypeError> {
    let source = source.trim();
    let scalar = match source {
        "Bool" => Some(TypeRef::Bool),
        "Unit" => Some(TypeRef::Unit),
        "Never" => Some(TypeRef::Never),
        "Int" => Some(TypeRef::Int),
        "Nat" => Some(TypeRef::Nat),
        "PositiveInt" => Some(TypeRef::PositiveInt),
        "Decimal" => Some(TypeRef::Decimal),
        "BoundaryNumber" => Some(TypeRef::BoundaryNumber),
        "Ratio" => Some(TypeRef::Ratio),
        "Text" => Some(TypeRef::Text),
        _ => None,
    };
    if let Some(scalar) = scalar {
        return Ok(scalar);
    }
    for constructor in ["Option", "Seq", "NonEmpty", "Set", "FiniteView"] {
        if let Some(inner) = generic_argument(source, constructor) {
            let inner = Box::new(parse_canonical_type(inner)?);
            return Ok(match constructor {
                "Option" => TypeRef::Option { value: inner },
                "Seq" => TypeRef::Seq { value: inner },
                "NonEmpty" => TypeRef::NonEmpty { value: inner },
                "Set" => TypeRef::Set { value: inner },
                _ => TypeRef::FiniteView { value: inner },
            });
        }
    }
    for constructor in ["Map", "Table"] {
        if let Some(inner) = generic_argument(source, constructor) {
            let parts = split_top_level(inner, ',');
            if parts.len() != 2 {
                return Err(ValueTypeError::new(
                    "$",
                    format!("invalid canonical type `{source}`"),
                ));
            }
            let key = Box::new(parse_canonical_type(parts[0])?);
            let value = Box::new(parse_canonical_type(parts[1])?);
            return Ok(if constructor == "Map" {
                TypeRef::Map { key, value }
            } else {
                TypeRef::Table { key, value }
            });
        }
    }
    if source.starts_with('(') && source.ends_with(')') {
        let inner = &source[1..source.len() - 1];
        let values = if inner.is_empty() {
            Vec::new()
        } else {
            split_top_level(inner, ',')
                .into_iter()
                .map(parse_canonical_type)
                .collect::<Result<_, _>>()?
        };
        return Ok(TypeRef::Tuple { values });
    }
    Ok(TypeRef::Named { id: source.into() })
}

fn split_top_level(source: &str, separator: char) -> Vec<&str> {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut output = Vec::new();
    for (index, character) in source.char_indices() {
        match character {
            '<' | '(' | '{' => depth += 1,
            '>' | ')' | '}' => depth = depth.saturating_sub(1),
            value if value == separator && depth == 0 => {
                output.push(source[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    output.push(source[start..].trim());
    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::ir::{PortDef, SourceRef};
    use crate::{CommandDef, ConstructorDef, Machine, MachineProgram, TypeDef};

    fn dotted_protocol_machine(with_port: bool) -> Machine {
        let constructor = ConstructorDef {
            name: "counter.Tick".into(),
            fields: Vec::new(),
        };
        let command = ConstructorDef {
            name: "counter.Changed".into(),
            fields: Vec::new(),
        };
        Machine {
            id: "example@1::Machine".into(),
            config: TypeRef::Unit,
            requires: Vec::new(),
            ports: with_port
                .then(|| PortDef {
                    name: "counter".into(),
                    contract: "example.ports@1::Counter".into(),
                    contract_instance: None,
                    type_arguments: Vec::new(),
                    configuration: None,
                    receive: vec![ConstructorDef {
                        name: "Tick".into(),
                        fields: Vec::new(),
                    }],
                    send: vec![ConstructorDef {
                        name: "Changed".into(),
                        fields: Vec::new(),
                    }],
                    contract_hash: "test".into(),
                    source: SourceRef::synthetic("port.counter"),
                })
                .into_iter()
                .collect(),
            local_input: TypeDef::Sum {
                id: "example@1::Machine.Input".into(),
                constructors: vec![constructor],
            },
            local_commands: vec![CommandDef {
                constructor: command,
                source: SourceRef::synthetic("command.counter.Changed"),
            }],
            outcomes: Vec::new(),
            state: Vec::new(),
            functions: BTreeMap::new(),
            derives: Vec::new(),
            invariants: Vec::new(),
            observation: Vec::new(),
            transitions: BTreeMap::new(),
            handlers: BTreeMap::new(),
            before_commit: Vec::new(),
            source: SourceRef::synthetic("machine"),
        }
    }

    #[test]
    fn dotted_local_protocols_are_not_misclassified_as_ports() {
        let program = MachineProgram::new();
        let machine = dotted_protocol_machine(false);
        let input = Value::variant("example@1::Machine.Input", "counter.Tick", Vec::new());
        assert_eq!(program.canonicalize_input(&machine, &input).unwrap(), input);
        let command = Value::variant("example@1::Machine.Command", "counter.Changed", Vec::new());
        assert_eq!(
            program.canonicalize_command(&machine, &command).unwrap(),
            command
        );
    }

    #[test]
    fn a_declared_port_prefix_retains_port_precedence() {
        let program = MachineProgram::new();
        let machine = dotted_protocol_machine(true);
        let port_input = Value::variant(
            "example@1::Machine::port.counter.Receive",
            "counter.Tick",
            Vec::new(),
        );
        assert_eq!(
            program.canonicalize_input(&machine, &port_input).unwrap(),
            port_input
        );
        let ambiguous_local =
            Value::variant("example@1::Machine.Input", "counter.Tick", Vec::new());
        assert!(
            program
                .canonicalize_input(&machine, &ambiguous_local)
                .is_err()
        );
    }

    #[test]
    fn nested_part_owned_port_locators_use_the_full_declared_prefix() {
        let program = MachineProgram::new();
        let mut machine = dotted_protocol_machine(true);
        machine.ports[0].name = "counter.api".into();
        let port_input = Value::variant(
            "example@1::Machine::port.counter.api.Receive",
            "counter.api.Tick",
            Vec::new(),
        );
        assert_eq!(
            program.canonicalize_input(&machine, &port_input).unwrap(),
            port_input
        );
        let port_command = Value::variant(
            "example@1::Machine::port.counter.api.Send",
            "counter.api.Changed",
            Vec::new(),
        );
        assert_eq!(
            program
                .canonicalize_command(&machine, &port_command)
                .unwrap(),
            port_command
        );
    }

    #[test]
    fn empty_and_nested_collection_bytes_keep_checked_identity() {
        let program = MachineProgram::new();
        let text = TypeRef::Seq {
            value: Box::new(TypeRef::Seq {
                value: Box::new(TypeRef::Text),
            }),
        };
        let integer = TypeRef::Seq {
            value: Box::new(TypeRef::Seq {
                value: Box::new(TypeRef::Int),
            }),
        };
        let empty = Value::Seq(vec![Value::Seq(Vec::new())]);
        assert_ne!(
            program.canonical_value_bytes(&text, &empty).unwrap(),
            program.canonical_value_bytes(&integer, &empty).unwrap(),
        );
        assert_ne!(
            program
                .canonical_value_bytes(
                    &TypeRef::Map {
                        key: Box::new(TypeRef::Text),
                        value: Box::new(TypeRef::Int),
                    },
                    &Value::Map(Vec::new()),
                )
                .unwrap(),
            program
                .canonical_value_bytes(
                    &TypeRef::Map {
                        key: Box::new(TypeRef::Text),
                        value: Box::new(TypeRef::Text),
                    },
                    &Value::Map(Vec::new()),
                )
                .unwrap(),
        );
    }

    #[test]
    fn nominal_keys_and_sums_are_closed() {
        let mut program = MachineProgram::new();
        program.types = BTreeMap::from([
            (
                "example@1::Id".into(),
                TypeDef::Key {
                    id: "example@1::Id".into(),
                    underlying: TypeRef::Text,
                },
            ),
            (
                "example@1::Choice".into(),
                TypeDef::Sum {
                    id: "example@1::Choice".into(),
                    constructors: vec![ConstructorDef {
                        name: "yes".into(),
                        fields: Vec::new(),
                    }],
                },
            ),
        ]);
        assert!(
            program
                .validate_value(
                    &TypeRef::Named {
                        id: "example@1::Id".into()
                    },
                    &Value::Key {
                        type_id: "example@1::Other".into(),
                        value: Box::new(Value::Text("x".into())),
                    },
                )
                .is_err()
        );
        assert!(
            program
                .validate_value(
                    &TypeRef::Named {
                        id: "example@1::Choice".into()
                    },
                    &Value::variant("example@1::Choice", "no", Vec::new()),
                )
                .is_err()
        );
    }

    #[test]
    fn table_requires_complete_declaration_order() {
        let mut program = MachineProgram::new();
        program.types.insert(
            "example@1::Slot".into(),
            TypeDef::Sum {
                id: "example@1::Slot".into(),
                constructors: vec![
                    ConstructorDef {
                        name: "first".into(),
                        fields: Vec::new(),
                    },
                    ConstructorDef {
                        name: "second".into(),
                        fields: Vec::new(),
                    },
                ],
            },
        );
        let ty = TypeRef::Table {
            key: Box::new(TypeRef::Named {
                id: "example@1::Slot".into(),
            }),
            value: Box::new(TypeRef::Int),
        };
        let reversed = Value::Table {
            key_type: "example@1::Slot".into(),
            entries: vec![
                ("second".into(), Value::int(2)),
                ("first".into(), Value::int(1)),
            ],
        };
        assert!(program.validate_value(&ty, &reversed).is_err());
    }

    #[test]
    fn typed_wire_decode_rejects_noncanonical_collection_order() {
        let program = MachineProgram::new();
        let set = serde_json::json!({
            "$": "set",
            "items": [
                {"$": "Int", "value": "2"},
                {"$": "Int", "value": "1"},
            ],
        });
        assert!(
            program
                .decode_wire_value(
                    &TypeRef::Set {
                        value: Box::new(TypeRef::Int),
                    },
                    &set,
                )
                .is_err()
        );

        let map = serde_json::json!({
            "$": "map",
            "entries": [
                [{"$": "Text", "value": "z"}, {"$": "Int", "value": "1"}],
                [{"$": "Text", "value": "a"}, {"$": "Int", "value": "2"}],
            ],
        });
        assert!(
            program
                .decode_wire_value(
                    &TypeRef::Map {
                        key: Box::new(TypeRef::Text),
                        value: Box::new(TypeRef::Int),
                    },
                    &map,
                )
                .is_err()
        );
    }

    #[test]
    fn typed_numeric_bytes_and_hashes_match_golden_vectors() {
        use std::str::FromStr as _;

        let program = MachineProgram::new();
        let vectors = [
            (
                "int-negative-256",
                TypeRef::Int,
                Value::int(-256),
                "0576616c75650203496e7403010100",
                "742976e4a61c96acb746a237766c899e99a21e61b52267a0a7e7764f411c4afe",
            ),
            (
                "nat-256",
                TypeRef::Nat,
                Value::nat(256).unwrap(),
                "0576616c756502034e6174020100",
                "3e15cada3edd41c44a2e4510e177405448e11d2a10296da9e44fd1dcab95fced",
            ),
            (
                "decimal-negative-12.3",
                TypeRef::Decimal,
                Value::Decimal(Decimal::from_str("-12.30").unwrap()),
                "0576616c75650207446563696d616c0e07646563696d616c0202017b0101",
                "8dbe6ea55eda955377997de0a79030f0a2e89e0e48ace583b31f9dc52f78e08a",
            ),
            (
                "boundary-finite-negative-12.3",
                TypeRef::BoundaryNumber,
                Value::Boundary(BoundaryNumber::Finite(Decimal::from_str("-12.30").unwrap())),
                "0576616c7565020e426f756e646172794e756d6265721a0776617269616e740201000e07646563696d616c0202017b0101",
                "e7b95e07bd8ea0a2ef5aa8c308efc04b9c2c06b01ec50113e655f6052a708e20",
            ),
        ];
        for (name, ty, value, expected_bytes, expected_hash) in vectors {
            let bytes = program.canonical_value_bytes(&ty, &value).unwrap();
            assert_eq!(crate::codec::hex(&bytes), expected_bytes, "{name} bytes");
            assert_eq!(
                crate::codec::hex(&crate::codec::hash("golden-value", &[bytes])),
                expected_hash,
                "{name} hash"
            );
        }
    }
}
