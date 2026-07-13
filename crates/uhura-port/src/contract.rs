//! The port contract model (design §9.1) — the data a `ports/*.port.toml`
//! file declares, its well-formedness rules, the canonical form whose
//! SHA-256 is pinned in `uhura.lock`, and contract-typed JSON decoding
//! (the L8 primitive shared by fixtures and projection application).

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Ident, Value, hash_json};

use crate::types::{RESERVED_TYPE_NAMES, TypeExpr};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortContract {
    pub name: Ident,
    pub version: String,
    pub types: BTreeMap<Ident, TypeDecl>,
    pub projections: BTreeMap<Ident, ProjectionDecl>,
    pub refusals: BTreeSet<Ident>,
    pub commands: BTreeMap<Ident, CommandDecl>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeDecl {
    Record {
        fields: BTreeMap<Ident, TypeExpr>,
    },
    /// Closed, exhaustively matchable. Each variant carries a field record.
    Union {
        variants: BTreeMap<Ident, BTreeMap<Ident, TypeExpr>>,
    },
    Enum {
        values: BTreeSet<Ident>,
    },
    /// A named nominal identity type.
    Id,
    /// Echoable, never inspectable (cursors). Wire values are strings in
    /// the spike.
    Opaque,
    /// A named asset-reference type.
    Asset,
}

impl TypeDecl {
    pub fn kind_str(&self) -> &'static str {
        match self {
            TypeDecl::Record { .. } => "record",
            TypeDecl::Union { .. } => "union",
            TypeDecl::Enum { .. } => "enum",
            TypeDecl::Id => "id",
            TypeDecl::Opaque => "opaque",
            TypeDecl::Asset => "asset",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionDecl {
    pub ty: TypeExpr,
    /// Keyed projections: the key's id type (`key = "<id-type>"`).
    pub key: Option<TypeExpr>,
    /// Delivered before `Init`; bare reads are legal (design §9.2).
    pub boot: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandDecl {
    pub payload: BTreeMap<Ident, TypeExpr>,
    /// Declared refusals; the outcome union is implicitly extended by
    /// `unavailable { reason: text }`. Ok payloads are empty for every
    /// spike command (§9.1).
    pub refusals: BTreeSet<Ident>,
}

/// A contract-level problem, located by TOML key path (contracts have no
/// source spans; the check pass wraps these into file-scoped diagnostics).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractIssue {
    /// e.g. `commands.like-post.refusals`
    pub path: String,
    pub message: String,
}

impl ContractIssue {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        ContractIssue {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl PortContract {
    /// Contract-local well-formedness (cross-source linking is the check
    /// pipeline's job): reserved names, dangling type references, key/boot
    /// legality, refusal references, and value-recursion (rejected outright
    /// in the spike — no contract type may reach itself).
    pub fn well_formedness(&self) -> Vec<ContractIssue> {
        let mut issues = Vec::new();

        for name in self.types.keys() {
            if RESERVED_TYPE_NAMES.contains(&name.as_str()) {
                issues.push(ContractIssue::new(
                    format!("types.{name}"),
                    format!("`{name}` is reserved by the type grammar"),
                ));
            }
        }

        let check_expr = |path: &str, ty: &TypeExpr, issues: &mut Vec<ContractIssue>| {
            let mut refs = Vec::new();
            ty.named_refs(&mut refs);
            for r in refs {
                if !self.types.contains_key(r) {
                    issues.push(ContractIssue::new(
                        path.to_string(),
                        format!("type `{r}` is not declared in port `{}`", self.name),
                    ));
                }
            }
        };

        for (name, decl) in &self.types {
            match decl {
                TypeDecl::Record { fields } => {
                    for (f, ty) in fields {
                        check_expr(&format!("types.{name}.fields.{f}"), ty, &mut issues);
                    }
                }
                TypeDecl::Union { variants } => {
                    if variants.is_empty() {
                        issues.push(ContractIssue::new(
                            format!("types.{name}"),
                            "a union needs at least one variant",
                        ));
                    }
                    for (v, fields) in variants {
                        for (f, ty) in fields {
                            check_expr(&format!("types.{name}.variants.{v}.{f}"), ty, &mut issues);
                        }
                    }
                }
                TypeDecl::Enum { values } => {
                    if values.is_empty() {
                        issues.push(ContractIssue::new(
                            format!("types.{name}"),
                            "an enum needs at least one value",
                        ));
                    }
                }
                TypeDecl::Id | TypeDecl::Opaque | TypeDecl::Asset => {}
            }
        }

        for (name, proj) in &self.projections {
            check_expr(&format!("projections.{name}.type"), &proj.ty, &mut issues);
            if let Some(key) = &proj.key {
                check_expr(&format!("projections.{name}.key"), key, &mut issues);
                if !self.is_id_type(key) {
                    issues.push(ContractIssue::new(
                        format!("projections.{name}.key"),
                        format!("projection keys must be id types, not `{key}`"),
                    ));
                }
                if proj.boot {
                    issues.push(ContractIssue::new(
                        format!("projections.{name}"),
                        "a keyed projection cannot be `boot = true` \
                         (boot delivery has no key to name an instance)",
                    ));
                }
            }
        }

        for (name, cmd) in &self.commands {
            for (f, ty) in &cmd.payload {
                check_expr(&format!("commands.{name}.payload.{f}"), ty, &mut issues);
            }
            for r in &cmd.refusals {
                if !self.refusals.contains(r) {
                    issues.push(ContractIssue::new(
                        format!("commands.{name}.refusals"),
                        format!("refusal `{r}` is not declared in port `{}`", self.name),
                    ));
                }
            }
        }

        issues.extend(self.recursion_issues());
        issues
    }

    /// True if `ty` denotes identity: the builtin `id` or a declared
    /// `kind = "id"` type.
    pub fn is_id_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Id => true,
            TypeExpr::Named(n) => matches!(self.types.get(n), Some(TypeDecl::Id)),
            _ => false,
        }
    }

    /// Rejects every cycle through `[types]` — a self-reaching contract
    /// type has no finite canonical form (spike micro-decision; the corpus
    /// needs none).
    fn recursion_issues(&self) -> Vec<ContractIssue> {
        let mut issues = Vec::new();
        for start in self.types.keys() {
            let mut stack = vec![start];
            let mut visited = BTreeSet::new();
            let mut cyclic = false;
            while let Some(name) = stack.pop() {
                if !visited.insert(name) {
                    continue;
                }
                for referenced in self.direct_refs(name) {
                    if referenced == start {
                        cyclic = true;
                    }
                    stack.push(referenced);
                }
            }
            if cyclic {
                issues.push(ContractIssue::new(
                    format!("types.{start}"),
                    format!("type `{start}` is recursive; contract types must be finite"),
                ));
            }
        }
        issues
    }

    fn direct_refs(&self, name: &Ident) -> Vec<&Ident> {
        let mut out = Vec::new();
        match self.types.get(name) {
            Some(TypeDecl::Record { fields }) => {
                for ty in fields.values() {
                    ty.named_refs(&mut out);
                }
            }
            Some(TypeDecl::Union { variants }) => {
                for fields in variants.values() {
                    for ty in fields.values() {
                        ty.named_refs(&mut out);
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// The canonical JSON form — sorted keys by construction (`BTreeMap`
    /// backing), type expressions in their strict string grammar. This is
    /// the byte form `uhura.lock` pins.
    pub fn to_canonical_json(&self) -> serde_json::Value {
        use serde_json::{Map, Value as J, json};
        let types: Map<String, J> = self
            .types
            .iter()
            .map(|(name, decl)| {
                let body = match decl {
                    TypeDecl::Record { fields } => json!({
                        "kind": "record",
                        "fields": type_map_json(fields),
                    }),
                    TypeDecl::Union { variants } => json!({
                        "kind": "union",
                        "variants": variants
                            .iter()
                            .map(|(v, fields)| (v.to_string(), J::Object(type_map_json(fields))))
                            .collect::<Map<String, J>>(),
                    }),
                    TypeDecl::Enum { values } => json!({
                        "kind": "enum",
                        "values": values.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    }),
                    TypeDecl::Id | TypeDecl::Opaque | TypeDecl::Asset => json!({
                        "kind": decl.kind_str(),
                    }),
                };
                (name.to_string(), body)
            })
            .collect();
        let projections: Map<String, J> = self
            .projections
            .iter()
            .map(|(name, p)| {
                (
                    name.to_string(),
                    json!({
                        "type": p.ty.to_string(),
                        "key": p.key.as_ref().map(ToString::to_string),
                        "boot": p.boot,
                    }),
                )
            })
            .collect();
        let commands: Map<String, J> = self
            .commands
            .iter()
            .map(|(name, c)| {
                (
                    name.to_string(),
                    json!({
                        "payload": type_map_json(&c.payload),
                        "refusals": c.refusals.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    }),
                )
            })
            .collect();
        json!({
            "contract": "uhura-port/0",
            "name": self.name.to_string(),
            "version": self.version,
            "types": types,
            "projections": projections,
            "refusals": self.refusals.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "commands": commands,
        })
    }

    /// SHA-256 of the canonical form — the `uhura.lock` pin (§9.1).
    pub fn canonical_hash(&self) -> String {
        hash_json(&self.to_canonical_json())
    }

    /// Decodes wire/fixture JSON against a contract type into a typed
    /// `Value`. Strict: no missing or extra record fields, integers only,
    /// unions as single-key `{ "<variant>": { fields } }` objects, enums
    /// as declared value strings, id/asset/opaque as strings.
    pub fn decode_value(&self, json: &serde_json::Value, ty: &TypeExpr) -> Result<Value, String> {
        use serde_json::Value as J;
        match ty {
            TypeExpr::Bool => match json {
                J::Bool(b) => Ok(Value::Bool(*b)),
                other => Err(type_mismatch("bool", other)),
            },
            TypeExpr::Int => match json {
                J::Number(n) => n
                    .as_i64()
                    .map(Value::Int)
                    .ok_or_else(|| format!("`{n}` is not an i64 (floats do not exist)")),
                other => Err(type_mismatch("int", other)),
            },
            TypeExpr::Text => match json {
                J::String(s) => Ok(Value::Text(s.clone())),
                other => Err(type_mismatch("text", other)),
            },
            TypeExpr::Id | TypeExpr::Asset => match json {
                J::String(s) => Ok(Value::Id(s.clone())),
                other => Err(type_mismatch(&ty.to_string(), other)),
            },
            // Optionals decode BARE (`Value::None` or the inner value —
            // `Value::Some` is never constructed; options do not nest,
            // §7.1), matching the runtime representation everywhere.
            TypeExpr::Option(inner) => match json {
                J::Null => Ok(Value::None),
                present => self.decode_value(present, inner),
            },
            TypeExpr::List(inner) => match json {
                J::Array(items) => items
                    .iter()
                    .map(|item| self.decode_value(item, inner))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::List),
                other => Err(type_mismatch(&ty.to_string(), other)),
            },
            TypeExpr::Named(name) => match self.types.get(name) {
                None => Err(format!("type `{name}` is not declared")),
                Some(TypeDecl::Id | TypeDecl::Opaque | TypeDecl::Asset) => match json {
                    J::String(s) => Ok(Value::Id(s.clone())),
                    other => Err(type_mismatch(name.as_str(), other)),
                },
                Some(TypeDecl::Enum { values }) => match json {
                    J::String(s) if values.iter().any(|v| v.as_str() == s) => {
                        Ok(Value::Text(s.clone()))
                    }
                    J::String(s) => Err(format!("`{s}` is not a value of enum `{name}`")),
                    other => Err(type_mismatch(name.as_str(), other)),
                },
                Some(TypeDecl::Record { fields }) => {
                    self.decode_record(json, name.as_str(), fields)
                }
                Some(TypeDecl::Union { variants }) => {
                    let J::Object(obj) = json else {
                        return Err(type_mismatch(name.as_str(), json));
                    };
                    if obj.len() != 1 {
                        return Err(format!(
                            "a `{name}` union value is a single-variant object, got {} keys",
                            obj.len()
                        ));
                    }
                    let (variant, body) = obj.iter().next().expect("len checked");
                    let Ok(variant_ident) = Ident::new(variant) else {
                        return Err(format!("`{variant}` is not a variant of `{name}`"));
                    };
                    let Some(fields) = variants.get(&variant_ident) else {
                        return Err(format!("`{variant}` is not a variant of `{name}`"));
                    };
                    let payload = self.decode_record(body, variant, fields)?;
                    let mut rec = BTreeMap::new();
                    rec.insert(variant_ident, payload);
                    Ok(Value::Record(rec))
                }
            },
        }
    }

    fn decode_record(
        &self,
        json: &serde_json::Value,
        name: &str,
        fields: &BTreeMap<Ident, TypeExpr>,
    ) -> Result<Value, String> {
        let serde_json::Value::Object(obj) = json else {
            return Err(type_mismatch(name, json));
        };
        for key in obj.keys() {
            if Ident::new(key).map_or(true, |k| !fields.contains_key(&k)) {
                return Err(format!("`{key}` is not a field of `{name}`"));
            }
        }
        let mut rec = BTreeMap::new();
        for (field, ty) in fields {
            let Some(field_json) = obj.get(field.as_str()) else {
                if matches!(ty, TypeExpr::Option(_)) {
                    rec.insert(field.clone(), Value::None);
                    continue;
                }
                return Err(format!("`{name}` is missing required field `{field}`"));
            };
            let value = self
                .decode_value(field_json, ty)
                .map_err(|e| format!("in `{name}.{field}`: {e}"))?;
            rec.insert(field.clone(), value);
        }
        Ok(Value::Record(rec))
    }
}

fn type_map_json(fields: &BTreeMap<Ident, TypeExpr>) -> serde_json::Map<String, serde_json::Value> {
    fields
        .iter()
        .map(|(f, ty)| (f.to_string(), serde_json::Value::String(ty.to_string())))
        .collect()
}

fn type_mismatch(expected: &str, got: &serde_json::Value) -> String {
    let shape = match got {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a bool",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "a list",
        serde_json::Value::Object(_) => "an object",
    };
    format!("expected `{expected}`, got {shape}")
}
