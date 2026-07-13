//! Fixture data (design §9.5): named slices in `fixtures/standard.toml`,
//! loaded as raw JSON values and typed at every binding site against the
//! expected structural type (L8 — an ill-typed fixture is a link error at
//! the site that binds it).
//!
//! Slices may reference each other with `"@<ns>.<name>"` strings (spike
//! micro-decision — keeps `feed.page-1` from duplicating every post);
//! references resolve at load with cycle detection.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Ident, Value};

use crate::types::{MapKey, Ty};

/// Namespace → slice name → raw JSON value (references resolved).
#[derive(Clone, Debug, Default)]
pub struct FixtureData {
    pub slices: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureIssue {
    /// `<ns>.<name>` (or a TOML path for structural problems).
    pub path: String,
    pub message: String,
}

impl FixtureData {
    pub fn get(&self, ns: &str, name: &str) -> Option<&serde_json::Value> {
        self.slices.get(ns).and_then(|m| m.get(name))
    }
}

pub fn load_fixture(text: &str) -> Result<FixtureData, Vec<FixtureIssue>> {
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => {
            return Err(vec![FixtureIssue {
                path: String::new(),
                message: format!("invalid TOML: {e}"),
            }]);
        }
    };

    let mut issues = Vec::new();
    let mut raw: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    for (ns, section) in &table {
        let Some(section) = section.as_table() else {
            issues.push(FixtureIssue {
                path: ns.clone(),
                message: "a fixture namespace is a table of slices".into(),
            });
            continue;
        };
        let mut slices = BTreeMap::new();
        for (name, value) in section {
            match toml_to_json(value) {
                Ok(json) => {
                    slices.insert(name.clone(), json);
                }
                Err(message) => issues.push(FixtureIssue {
                    path: format!("{ns}.{name}"),
                    message,
                }),
            }
        }
        raw.insert(ns.clone(), slices);
    }
    if !issues.is_empty() {
        return Err(issues);
    }

    // Resolve `@ns.name` references (deep), with cycle detection.
    let mut resolved: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new();
    let keys: Vec<(String, String)> = raw
        .iter()
        .flat_map(|(ns, m)| m.keys().map(move |n| (ns.clone(), n.clone())))
        .collect();
    for (ns, name) in keys {
        let mut in_flight = BTreeSet::new();
        match resolve_slice(&raw, &ns, &name, &mut in_flight) {
            Ok(json) => {
                resolved.entry(ns).or_default().insert(name, json);
            }
            Err(message) => issues.push(FixtureIssue {
                path: format!("{ns}.{name}"),
                message,
            }),
        }
    }
    if issues.is_empty() {
        Ok(FixtureData { slices: resolved })
    } else {
        Err(issues)
    }
}

fn resolve_slice(
    raw: &BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    ns: &str,
    name: &str,
    in_flight: &mut BTreeSet<String>,
) -> Result<serde_json::Value, String> {
    let key = format!("{ns}.{name}");
    if !in_flight.insert(key.clone()) {
        return Err(format!("slice reference cycle through `@{key}`"));
    }
    let value = raw
        .get(ns)
        .and_then(|m| m.get(name))
        .ok_or_else(|| format!("no slice `{key}`"))?;
    let resolved = resolve_refs(raw, value, in_flight)?;
    in_flight.remove(&key);
    Ok(resolved)
}

fn resolve_refs(
    raw: &BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    value: &serde_json::Value,
    in_flight: &mut BTreeSet<String>,
) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    match value {
        J::String(s) => {
            if let Some(reference) = s.strip_prefix('@') {
                // Driver substitution markers (§9.5) pass through verbatim:
                // scripts resolve them at delivery time (`fresh-id`,
                // `payload.<field>` — the only two). Slices carrying them
                // are script-reply material and never legal as pins.
                if reference == "fresh-id" || reference.starts_with("payload.") {
                    return Ok(value.clone());
                }
                let Some((ns, name)) = reference.split_once('.') else {
                    return Err(format!("`@{reference}` is not a `@<ns>.<name>` reference"));
                };
                resolve_slice(raw, ns, name, in_flight)
            } else {
                Ok(value.clone())
            }
        }
        J::Array(items) => items
            .iter()
            .map(|item| resolve_refs(raw, item, in_flight))
            .collect::<Result<Vec<_>, _>>()
            .map(J::Array),
        J::Object(map) => map
            .iter()
            .map(|(k, v)| Ok((k.clone(), resolve_refs(raw, v, in_flight)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(J::Object),
        _ => Ok(value.clone()),
    }
}

/// TOML → JSON, refusing everything the value model refuses (floats,
/// datetimes — §7.5 determinism by type shape).
fn toml_to_json(value: &toml::Value) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    match value {
        toml::Value::String(s) => Ok(J::String(s.clone())),
        toml::Value::Integer(i) => Ok(J::Number((*i).into())),
        toml::Value::Boolean(b) => Ok(J::Bool(*b)),
        toml::Value::Float(_) => Err("floats do not exist in fixture data (§7.5)".into()),
        toml::Value::Datetime(_) => {
            Err("no clocks: time labels are provider-formatted text (§9.1)".into())
        }
        toml::Value::Array(items) => items
            .iter()
            .map(toml_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(J::Array),
        toml::Value::Table(table) => table
            .iter()
            .map(|(k, v)| Ok((k.clone(), toml_to_json(v)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(J::Object),
    }
}

/// Decodes raw fixture JSON against a structural type — the use-site half
/// of L8. Strict on records (no unknown fields; absent optionals become
/// `none`); unions are single-variant objects; optionals decode bare.
pub fn decode_against_ty(json: &serde_json::Value, ty: &Ty) -> Result<Value, String> {
    use serde_json::Value as J;
    match ty {
        Ty::Error => Err("cannot decode against an erroneous type".into()),
        Ty::Bool => match json {
            J::Bool(b) => Ok(Value::Bool(*b)),
            other => Err(mismatch("bool", other)),
        },
        Ty::Int => match json {
            J::Number(n) => n
                .as_i64()
                .map(Value::Int)
                .ok_or_else(|| format!("`{n}` is not an i64")),
            other => Err(mismatch("int", other)),
        },
        Ty::Text => match json {
            J::String(s) => Ok(Value::Text(s.clone())),
            other => Err(mismatch("text", other)),
        },
        Ty::Id | Ty::Asset | Ty::Nominal { .. } => match json {
            J::String(s) => Ok(Value::Id(s.clone())),
            other => Err(mismatch(&ty.describe(), other)),
        },
        Ty::Tag => Err("tags are core-minted; fixtures never carry them".into()),
        Ty::NoneLit => match json {
            J::Null => Ok(Value::None),
            other => Err(mismatch("none", other)),
        },
        Ty::Enum(values) => match json {
            J::String(s) if values.iter().any(|v| v.as_str() == s) => Ok(Value::Text(s.clone())),
            J::String(s) => Err(format!("`{s}` is not one of the enum's values")),
            other => Err(mismatch("an enum value", other)),
        },
        Ty::Option(inner) => match json {
            J::Null => Ok(Value::None),
            present => decode_against_ty(present, inner),
        },
        Ty::List(inner) => match json {
            J::Array(items) => items
                .iter()
                .map(|item| decode_against_ty(item, inner))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List),
            other => Err(mismatch(&ty.describe(), other)),
        },
        Ty::Map(key_kind, inner) => match json {
            J::Object(map) => {
                let mut record = BTreeMap::new();
                for (k, v) in map {
                    if *key_kind == MapKey::Tag {
                        return Err(
                            "tag-keyed maps are core state; fixtures never carry them".to_string()
                        );
                    }
                    let key = Ident::new(k).map_err(|e| e.to_string())?;
                    record.insert(key, decode_against_ty(v, inner)?);
                }
                Ok(Value::Record(record))
            }
            other => Err(mismatch(&ty.describe(), other)),
        },
        Ty::Record(fields) => {
            let J::Object(map) = json else {
                return Err(mismatch(&ty.describe(), json));
            };
            for k in map.keys() {
                if Ident::new(k)
                    .map(|k| !fields.contains_key(&k))
                    .unwrap_or(true)
                {
                    return Err(format!("`{k}` is not a field of {}", ty.describe()));
                }
            }
            let mut record = BTreeMap::new();
            for (field, field_ty) in fields {
                match map.get(field.as_str()) {
                    Some(v) => {
                        let decoded = decode_against_ty(v, field_ty)
                            .map_err(|e| format!("in `{field}`: {e}"))?;
                        record.insert(field.clone(), decoded);
                    }
                    None if matches!(field_ty, Ty::Option(_)) => {
                        record.insert(field.clone(), Value::None);
                    }
                    None => {
                        return Err(format!("missing required field `{field}`"));
                    }
                }
            }
            Ok(Value::Record(record))
        }
        Ty::Union(variants) => {
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
            let variant_ident = Ident::new(variant).map_err(|e| e.to_string())?;
            let Some(fields) = variants.get(&variant_ident) else {
                return Err(format!("`{variant}` is not a variant of the union"));
            };
            let payload = decode_against_ty(body, &Ty::Record(fields.clone()))
                .map_err(|e| format!("in `{variant}`: {e}"))?;
            let mut record = BTreeMap::new();
            record.insert(variant_ident, payload);
            Ok(Value::Record(record))
        }
    }
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
