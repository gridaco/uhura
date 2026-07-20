use std::collections::BTreeMap;
use std::fmt;

use uhura_port::{RouteAtom, RouteFieldKind, RouteFieldValue, RouteLocation, RouteTable};

use super::ir::{Expr, Pattern, Program};
use super::value::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteRuntimeError(pub String);

impl fmt::Display for RouteRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RouteRuntimeError {}

impl Program {
    pub fn decode_route_input(
        &self,
        machine_id: &str,
        port_name: &str,
        url: &str,
    ) -> Result<Value, RouteRuntimeError> {
        let (table, resolved_input_type) = self.route_table_for_port(machine_id, port_name)?;
        let location = table
            .decode(url)
            .map_err(|error| RouteRuntimeError(error.to_string()))?;
        let location = route_value(table, location)?;
        Ok(Value::variant(
            resolved_input_type,
            format!("{port_name}.changed"),
            vec![(Some("location".into()), location)],
        ))
    }

    pub fn encode_route_location(
        &self,
        machine_id: &str,
        port_name: &str,
        location: &Value,
    ) -> Result<String, RouteRuntimeError> {
        let (table, _) = self.route_table_for_port(machine_id, port_name)?;
        table
            .encode(&value_route_location(table, location)?)
            .map_err(|error| RouteRuntimeError(error.to_string()))
    }

    fn route_table_for_port(
        &self,
        machine_id: &str,
        port_name: &str,
    ) -> Result<(&RouteTable, String), RouteRuntimeError> {
        let machine = self
            .machines
            .get(machine_id)
            .ok_or_else(|| RouteRuntimeError(format!("unknown machine `{machine_id}`")))?;
        let port = machine
            .ports
            .iter()
            .find(|port| port.name == port_name)
            .ok_or_else(|| {
                RouteRuntimeError(format!("machine `{machine_id}` has no port `{port_name}`"))
            })?;
        let Some(Expr::Name { name }) = &port.configuration else {
            return Err(RouteRuntimeError(format!(
                "port `{port_name}` has no resolved route-table configuration"
            )));
        };
        let table = self
            .route_tables
            .get(name)
            .or_else(|| {
                self.route_tables
                    .iter()
                    .find(|(candidate, _)| candidate.ends_with(&format!("::{name}")))
                    .map(|(_, table)| table)
            })
            .ok_or_else(|| {
                RouteRuntimeError(format!(
                    "port `{port_name}` configuration `{name}` is not a checked Routes value"
                ))
            })?;
        let input = machine
            .handlers
            .get(&format!("{port_name}.changed"))
            .ok_or_else(|| {
                RouteRuntimeError(format!(
                    "machine `{machine_id}` does not handle `{port_name}.changed`"
                ))
            })?;
        let Pattern::Constructor { type_id, .. } = &input.pattern else {
            return Err(RouteRuntimeError(
                "resolved route handler has no constructor pattern".into(),
            ));
        };
        Ok((table, type_id.clone()))
    }
}

fn route_value(table: &RouteTable, location: RouteLocation) -> Result<Value, RouteRuntimeError> {
    let declaration = table
        .constructors()
        .iter()
        .find(|candidate| candidate.name == location.constructor)
        .ok_or_else(|| RouteRuntimeError("decoded route constructor is undeclared".into()))?;
    let mut fields = Vec::with_capacity(declaration.fields.len());
    for declaration in &declaration.fields {
        let value = location.fields.get(&declaration.name).ok_or_else(|| {
            RouteRuntimeError(format!("decoded route omitted `{}`", declaration.name))
        })?;
        fields.push((
            Some(declaration.name.clone()),
            route_field_to_value(&declaration.kind, value)?,
        ));
    }
    Ok(Value::variant(
        table.location_type().as_str(),
        location.constructor,
        fields,
    ))
}

fn route_field_to_value(
    kind: &RouteFieldKind,
    value: &RouteFieldValue,
) -> Result<Value, RouteRuntimeError> {
    match (kind, value) {
        (RouteFieldKind::Text, RouteFieldValue::Required(atom))
        | (RouteFieldKind::TextKey { .. }, RouteFieldValue::Required(atom)) => {
            route_atom_to_value(atom)
        }
        (RouteFieldKind::OptionalText, RouteFieldValue::Optional(value))
        | (RouteFieldKind::OptionalTextKey { .. }, RouteFieldValue::Optional(value)) => {
            let option_type = match kind {
                RouteFieldKind::OptionalText => "Option<Text>".to_string(),
                RouteFieldKind::OptionalTextKey { type_name } => {
                    format!("Option<{}>", type_name.as_str())
                }
                RouteFieldKind::Text | RouteFieldKind::TextKey { .. } => unreachable!(),
            };
            Ok(match value {
                None => Value::variant(option_type, "none", Vec::new()),
                Some(atom) => Value::variant(
                    option_type,
                    "some",
                    vec![(Some("value".into()), route_atom_to_value(atom)?)],
                ),
            })
        }
        _ => Err(RouteRuntimeError(
            "decoded route field does not match its declaration".into(),
        )),
    }
}

fn route_atom_to_value(atom: &RouteAtom) -> Result<Value, RouteRuntimeError> {
    Ok(match atom {
        RouteAtom::Text { value } => Value::Text(value.clone()),
        RouteAtom::Key { type_name, value } => Value::Key {
            type_id: type_name.as_str().into(),
            value: Box::new(Value::Text(value.clone())),
        },
    })
}

fn value_route_location(
    table: &RouteTable,
    value: &Value,
) -> Result<RouteLocation, RouteRuntimeError> {
    let Value::Variant {
        type_id,
        constructor,
        fields,
    } = value
    else {
        return Err(RouteRuntimeError("route value is not a variant".into()));
    };
    if type_id != table.location_type().as_str() {
        return Err(RouteRuntimeError(format!(
            "route location type `{type_id}` does not match `{}`",
            table.location_type().as_str()
        )));
    }
    let declaration = table
        .constructors()
        .iter()
        .find(|candidate| candidate.name == *constructor)
        .ok_or_else(|| RouteRuntimeError(format!("unknown route constructor `{constructor}`")))?;
    if fields.len() != declaration.fields.len() {
        return Err(RouteRuntimeError(format!(
            "route constructor `{constructor}` has the wrong arity"
        )));
    }
    let mut output = BTreeMap::new();
    for (index, declaration) in declaration.fields.iter().enumerate() {
        if fields[index].0.as_deref() != Some(declaration.name.as_str()) {
            return Err(RouteRuntimeError(format!(
                "route constructor `{constructor}` field {index} must be named `{}`",
                declaration.name
            )));
        }
        output.insert(
            declaration.name.clone(),
            value_route_field(&declaration.kind, &fields[index].1)?,
        );
    }
    Ok(RouteLocation::new(constructor, output))
}

fn value_route_field(
    kind: &RouteFieldKind,
    value: &Value,
) -> Result<RouteFieldValue, RouteRuntimeError> {
    match kind {
        RouteFieldKind::Text => Ok(RouteFieldValue::Required(RouteAtom::Text {
            value: value_text(value)?,
        })),
        RouteFieldKind::TextKey { type_name } => Ok(RouteFieldValue::Required(value_key(
            type_name.as_str(),
            value,
        )?)),
        RouteFieldKind::OptionalText => Ok(RouteFieldValue::Optional(
            value_option("Option<Text>", value)?
                .map(|value| value_text(value).map(|value| RouteAtom::Text { value }))
                .transpose()?,
        )),
        RouteFieldKind::OptionalTextKey { type_name } => Ok(RouteFieldValue::Optional(
            value_option(&format!("Option<{}>", type_name.as_str()), value)?
                .map(|value| value_key(type_name.as_str(), value))
                .transpose()?,
        )),
    }
}

fn value_option<'a>(
    expected_type: &str,
    value: &'a Value,
) -> Result<Option<&'a Value>, RouteRuntimeError> {
    let Value::Variant {
        type_id,
        constructor,
        fields,
    } = value
    else {
        return Err(RouteRuntimeError(
            "optional route value is not Option".into(),
        ));
    };
    if type_id != expected_type {
        return Err(RouteRuntimeError(format!(
            "optional route type `{type_id}` does not match `{expected_type}`"
        )));
    }
    match constructor.as_str() {
        "none" if fields.is_empty() => Ok(None),
        "some" if fields.len() == 1 && fields[0].0.as_deref() == Some("value") => {
            Ok(Some(&fields[0].1))
        }
        _ => Err(RouteRuntimeError(
            "optional route value is ill-shaped".into(),
        )),
    }
}

fn value_text(value: &Value) -> Result<String, RouteRuntimeError> {
    match value {
        Value::Text(value) => Ok(value.clone()),
        _ => Err(RouteRuntimeError("route value is not Text".into())),
    }
}

fn value_key(type_id: &str, value: &Value) -> Result<RouteAtom, RouteRuntimeError> {
    let Value::Key {
        type_id: actual,
        value,
    } = value
    else {
        return Err(RouteRuntimeError("route value is not a nominal key".into()));
    };
    if actual != type_id {
        return Err(RouteRuntimeError(format!(
            "route key `{actual}` does not match `{type_id}`"
        )));
    }
    Ok(RouteAtom::Key {
        type_name: uhura_port::TypeRef::new(type_id)
            .map_err(|error| RouteRuntimeError(error.to_string()))?,
        value: value_text(value)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uhura_port::{RouteConstructorDecl, RouteFieldDecl, RoutePatternDecl, TypeRef};

    #[test]
    fn typed_location_round_trips_the_pinned_codec() {
        let table = RouteTable::compile(
            TypeRef::new("Location").unwrap(),
            vec![RouteConstructorDecl::new(
                "order",
                vec![RouteFieldDecl::new(
                    "order",
                    RouteFieldKind::TextKey {
                        type_name: TypeRef::new("OrderId").unwrap(),
                    },
                )],
            )],
            vec![RoutePatternDecl::new("order", "/orders/{order}")],
        )
        .unwrap();
        let value = Value::variant(
            "Location",
            "order",
            vec![(
                Some("order".into()),
                Value::Key {
                    type_id: "OrderId".into(),
                    value: Box::new(Value::Text("order-100".into())),
                },
            )],
        );
        let location = value_route_location(&table, &value).unwrap();
        let url = table.encode(&location).unwrap();
        assert_eq!(url, "/orders/order-100");
        assert_eq!(
            route_value(&table, table.decode(&url).unwrap()).unwrap(),
            value
        );
    }

    #[test]
    fn decoded_optional_route_fields_keep_their_exact_declared_types() {
        let table = RouteTable::compile(
            TypeRef::new("Location").unwrap(),
            vec![RouteConstructorDecl::new(
                "flow",
                vec![
                    RouteFieldDecl::new(
                        "order",
                        RouteFieldKind::TextKey {
                            type_name: TypeRef::new("OrderId").unwrap(),
                        },
                    ),
                    RouteFieldDecl::new("step", RouteFieldKind::OptionalText),
                ],
            )],
            vec![RoutePatternDecl::new(
                "flow",
                "/orders/{order}/return?step={step?}",
            )],
        )
        .unwrap();

        for (url, expected_step) in [
            (
                "/orders/order-100/return?step=items",
                Value::variant(
                    "Option<Text>",
                    "some",
                    vec![(Some("value".into()), Value::Text("items".into()))],
                ),
            ),
            (
                "/orders/order-100/return",
                Value::variant("Option<Text>", "none", Vec::new()),
            ),
        ] {
            let Value::Variant { fields, .. } =
                route_value(&table, table.decode(url).unwrap()).unwrap()
            else {
                panic!("route decoder must produce a location variant");
            };
            assert_eq!(fields[1].1, expected_step);
        }
    }

    #[test]
    fn encoder_rejects_structural_lookalikes_without_nominal_identity() {
        let table = RouteTable::compile(
            TypeRef::new("Location").unwrap(),
            vec![RouteConstructorDecl::new(
                "flow",
                vec![RouteFieldDecl::new("step", RouteFieldKind::OptionalText)],
            )],
            vec![RoutePatternDecl::new("flow", "/flow?step={step?}")],
        )
        .unwrap();
        let valid_step = Value::variant(
            "Option<Text>",
            "some",
            vec![(Some("value".into()), Value::Text("items".into()))],
        );

        for invalid in [
            Value::variant(
                "OtherLocation",
                "flow",
                vec![(Some("step".into()), valid_step.clone())],
            ),
            Value::variant(
                "Location",
                "flow",
                vec![(Some("other".into()), valid_step.clone())],
            ),
            Value::variant(
                "Location",
                "flow",
                vec![(
                    Some("step".into()),
                    Value::variant(
                        "Option<Other>",
                        "some",
                        vec![(Some("value".into()), Value::Text("items".into()))],
                    ),
                )],
            ),
            Value::variant(
                "Location",
                "flow",
                vec![(
                    Some("step".into()),
                    Value::variant(
                        "Option<Text>",
                        "some",
                        vec![(Some("other".into()), Value::Text("items".into()))],
                    ),
                )],
            ),
        ] {
            assert!(value_route_location(&table, &invalid).is_err());
        }
    }
}
