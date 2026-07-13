//! Wire JSON → typed `Value`, directed by the IR's baked-in type slices
//! (`TyIr`). This is the runtime half of L8: projection updates and `Ui`
//! event payloads arrive as inert JSON and must land as the same typed
//! values the checker's fixture decoder produces — ids as `Value::Id`,
//! optionals bare, unions single-variant, tags `"t-<n>"`.
//!
//! Kept semantically in lock-step with `uhura-check`'s pin decoder: a
//! projection value pinned by an example and the identical value delivered
//! through a script MUST decode to equal `Value`s (derived-replay goldens
//! enforce this end to end).

use std::collections::BTreeMap;

use uhura_base::{Ident, Value};

use crate::ir::{MapKeyIr, TyIr};

pub fn decode_value(json: &serde_json::Value, ty: &TyIr) -> Result<Value, String> {
    use serde_json::Value as J;
    match ty {
        TyIr::Bool => match json {
            J::Bool(b) => Ok(Value::Bool(*b)),
            other => Err(mismatch("bool", other)),
        },
        TyIr::Int => match json {
            J::Number(n) => n
                .as_i64()
                .map(Value::Int)
                .ok_or_else(|| format!("`{n}` is not an i64")),
            other => Err(mismatch("int", other)),
        },
        TyIr::Text => match json {
            J::String(s) => Ok(Value::Text(s.clone())),
            other => Err(mismatch("text", other)),
        },
        TyIr::Id | TyIr::Asset => match json {
            J::String(s) => Ok(Value::Id(s.clone())),
            other => Err(mismatch("an id", other)),
        },
        TyIr::Tag => match json {
            J::String(s) => parse_tag(s)
                .map(Value::Tag)
                .ok_or_else(|| format!("`{s}` is not a `t-<n>` tag")),
            other => Err(mismatch("a tag", other)),
        },
        TyIr::Enum(values) => match json {
            J::String(s) if values.iter().any(|v| v.as_str() == s) => Ok(Value::Text(s.clone())),
            J::String(s) => Err(format!("`{s}` is not one of the enum's values")),
            other => Err(mismatch("an enum value", other)),
        },
        TyIr::Option(inner) => match json {
            J::Null => Ok(Value::None),
            // Optionals hold their value BARE (§7.1 — options do not nest).
            present => decode_value(present, inner),
        },
        TyIr::List(inner) => match json {
            J::Array(items) => items
                .iter()
                .map(|item| decode_value(item, inner))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List),
            other => Err(mismatch("a list", other)),
        },
        TyIr::Map { key, value } => match json {
            J::Object(map) => {
                let mut record = BTreeMap::new();
                for (k, v) in map {
                    if *key == MapKeyIr::Tag && parse_tag(k).is_none() {
                        return Err(format!("`{k}` is not a `t-<n>` tag key"));
                    }
                    let k = Ident::new(k).map_err(|e| e.to_string())?;
                    record.insert(k, decode_value(v, value)?);
                }
                Ok(Value::Record(record))
            }
            other => Err(mismatch("a map object", other)),
        },
        TyIr::Record(fields) => {
            let J::Object(map) = json else {
                return Err(mismatch("a record object", json));
            };
            for k in map.keys() {
                if Ident::new(k)
                    .map(|k| !fields.contains_key(&k))
                    .unwrap_or(true)
                {
                    return Err(format!("`{k}` is not a field of the record"));
                }
            }
            let mut record = BTreeMap::new();
            for (field, field_ty) in fields {
                match map.get(field.as_str()) {
                    Some(v) => {
                        let decoded =
                            decode_value(v, field_ty).map_err(|e| format!("in `{field}`: {e}"))?;
                        record.insert(field.clone(), decoded);
                    }
                    None if matches!(field_ty, TyIr::Option(_)) => {
                        record.insert(field.clone(), Value::None);
                    }
                    None => return Err(format!("missing required field `{field}`")),
                }
            }
            Ok(Value::Record(record))
        }
        TyIr::Union(variants) => {
            let J::Object(map) = json else {
                return Err(mismatch("a single-variant union object", json));
            };
            if map.len() != 1 {
                return Err(format!(
                    "a union value has exactly one variant, got {}",
                    map.len()
                ));
            }
            let (variant, body) = map.iter().next().expect("len checked");
            let variant = Ident::new(variant).map_err(|e| e.to_string())?;
            let Some(fields) = variants.get(&variant) else {
                return Err(format!("`{variant}` is not a variant of the union"));
            };
            let payload = decode_value(body, &TyIr::Record(fields.clone()))
                .map_err(|e| format!("in `{variant}`: {e}"))?;
            let mut record = BTreeMap::new();
            record.insert(variant, payload);
            Ok(Value::Record(record))
        }
    }
}

pub fn parse_tag(s: &str) -> Option<u64> {
    s.strip_prefix("t-").and_then(|n| n.parse().ok())
}

fn mismatch(expected: &str, got: &serde_json::Value) -> String {
    let shape = match got {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a bool",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "a list",
        serde_json::Value::Object(_) => "an object",
    };
    format!("expected {expected}, got {shape}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_decode_nominally_and_optionals_stay_bare() {
        let ty = TyIr::Record(BTreeMap::from([
            (Ident::new("post").unwrap(), TyIr::Id),
            (
                Ident::new("cursor").unwrap(),
                TyIr::Option(Box::new(TyIr::Text)),
            ),
        ]));
        let v = decode_value(&serde_json::json!({ "post": "post-1" }), &ty).unwrap();
        let Value::Record(fields) = v else { panic!() };
        assert_eq!(
            fields[&Ident::new("post").unwrap()],
            Value::Id("post-1".into())
        );
        assert_eq!(fields[&Ident::new("cursor").unwrap()], Value::None);
    }

    #[test]
    fn tags_parse_the_minted_form() {
        assert_eq!(
            decode_value(&serde_json::json!("t-4"), &TyIr::Tag).unwrap(),
            Value::Tag(4)
        );
        assert!(decode_value(&serde_json::json!("c-4"), &TyIr::Tag).is_err());
    }
}
