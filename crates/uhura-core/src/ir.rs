use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::value::{IntegerKind, Value};

pub const IR_PROTOCOL: &str = "uhura-ir/1";
pub const LANGUAGE: &str = "uhura 0.4";
pub const MACHINE_PROGRAM_ID_PROTOCOL: &str = "uhura-machine-program/0";
pub const MACHINE_UI_INTERFACE_ID_PROTOCOL: &str = "uhura-machine-ui-interface/0";
pub const PRESENTATION_ID_PROTOCOL: &str = "uhura-presentation/0";
pub const EVIDENCE_ID_PROTOCOL: &str = "uhura-evidence/0";
pub const DEPLOYMENT_ID_PROTOCOL: &str = "uhura-deployment/0";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    pub id: String,
    pub path: String,
    pub start: u32,
    pub end: u32,
}

impl SourceRef {
    pub fn synthetic(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            path: "<generated>".into(),
            start: 0,
            end: 0,
        }
    }
}

/// The canonical semantic frame from which one runtime-observable `SiteId`
/// is derived.
///
/// Checked Uhura 0.4 source retains this compact frame in public IR so an
/// external decoder can verify a supplied fault identity instead of trusting
/// any arbitrary 64-character hexadecimal string.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SiteIdentityFrame {
    pub public_owner: String,
    pub composition_owner: String,
    pub kind: String,
    pub path: String,
}

impl SiteIdentityFrame {
    #[must_use]
    pub fn new(
        public_owner: impl Into<String>,
        composition_owner: impl Into<String>,
        kind: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            public_owner: public_owner.into(),
            composition_owner: composition_owner.into(),
            kind: kind.into(),
            path: path.into(),
        }
    }

    #[must_use]
    pub fn site_id(&self) -> String {
        crate::semantic_node_id(
            &self.public_owner,
            &self.composition_owner,
            &self.kind,
            &self.path,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TypeRef {
    Bool,
    Unit,
    Never,
    Int,
    Nat,
    PositiveInt,
    Decimal,
    BoundaryNumber,
    Ratio,
    Text,
    Named {
        id: String,
    },
    Option {
        value: Box<TypeRef>,
    },
    Seq {
        value: Box<TypeRef>,
    },
    NonEmpty {
        value: Box<TypeRef>,
    },
    Set {
        value: Box<TypeRef>,
    },
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
    },
    Table {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
    },
    FiniteView {
        value: Box<TypeRef>,
    },
    Tuple {
        values: Vec<TypeRef>,
    },
    Record {
        fields: Vec<(String, TypeRef)>,
    },
}

impl TypeRef {
    pub fn canonical_name(&self) -> String {
        match self {
            Self::Bool => "Bool".into(),
            Self::Unit => "Unit".into(),
            Self::Never => "Never".into(),
            Self::Int => "Int".into(),
            Self::Nat => "Nat".into(),
            Self::PositiveInt => "PositiveInt".into(),
            Self::Decimal => "Decimal".into(),
            Self::BoundaryNumber => "BoundaryNumber".into(),
            Self::Ratio => "Ratio".into(),
            Self::Text => "Text".into(),
            Self::Named { id } => id.clone(),
            Self::Option { value } => format!("Option<{}>", value.canonical_name()),
            Self::Seq { value } => format!("Seq<{}>", value.canonical_name()),
            Self::NonEmpty { value } => format!("NonEmpty<{}>", value.canonical_name()),
            Self::Set { value } => format!("Set<{}>", value.canonical_name()),
            Self::Map { key, value } => {
                format!("Map<{},{}>", key.canonical_name(), value.canonical_name())
            }
            Self::Table { key, value } => {
                format!("Table<{},{}>", key.canonical_name(), value.canonical_name())
            }
            Self::FiniteView { value } => format!("FiniteView<{}>", value.canonical_name()),
            Self::Tuple { values } => format!(
                "({})",
                values
                    .iter()
                    .map(Self::canonical_name)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Record { fields } => format!(
                "{{{}}}",
                fields
                    .iter()
                    .map(|(name, ty)| format!("{name}:{}", ty.canonical_name()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TypeDef {
    Key {
        id: String,
        underlying: TypeRef,
    },
    Record {
        id: String,
        fields: Vec<(String, TypeRef)>,
    },
    Sum {
        id: String,
        constructors: Vec<ConstructorDef>,
    },
}

impl TypeDef {
    pub fn id(&self) -> &str {
        match self {
            Self::Key { id, .. } | Self::Record { id, .. } | Self::Sum { id, .. } => id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConstructorDef {
    pub name: String,
    pub fields: Vec<(Option<String>, TypeRef)>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub id: String,
    pub params: Vec<(String, TypeRef)>,
    pub result: TypeRef,
    pub body: Expr,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortDef {
    pub name: String,
    pub contract: String,
    /// The exact resolved contract instance admitted by the checker. Its
    /// compatibility projection covers generic arguments, immutable
    /// configuration, and canonical codecs; a generic content hash alone is
    /// insufficient at a host boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_instance: Option<uhura_port::ContractInstance>,
    pub type_arguments: Vec<TypeRef>,
    pub configuration: Option<Expr>,
    pub receive: Vec<ConstructorDef>,
    pub send: Vec<ConstructorDef>,
    pub contract_hash: String,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StateField {
    pub name: String,
    pub ty: TypeRef,
    pub initial: Expr,
    pub source: SourceRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutcomePolicy {
    Commit,
    Abort,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutcomeDef {
    pub constructor: ConstructorDef,
    pub policy: OutcomePolicy,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandDef {
    pub constructor: ConstructorDef,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Handler {
    /// Fully flattened input identity, for example `submit` or
    /// `router.changed`.
    pub input: String,
    pub pattern: Pattern,
    pub body: Vec<Statement>,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub params: Vec<(String, TypeRef)>,
    pub body: Vec<Statement>,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservationField {
    pub name: String,
    pub ty: TypeRef,
    pub expression: Expr,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    pub id: String,
    pub config: TypeRef,
    pub requires: Vec<(Expr, SourceRef)>,
    pub ports: Vec<PortDef>,
    pub local_input: TypeDef,
    pub local_commands: Vec<CommandDef>,
    pub outcomes: Vec<OutcomeDef>,
    pub state: Vec<StateField>,
    pub functions: BTreeMap<String, Function>,
    pub derives: Vec<(String, TypeRef, Expr, SourceRef)>,
    pub invariants: Vec<(Expr, SourceRef)>,
    pub observation: Vec<ObservationField>,
    pub transitions: BTreeMap<String, Transition>,
    pub handlers: BTreeMap<String, Handler>,
    pub before_commit: Vec<Statement>,
    pub source: SourceRef,
}

impl Machine {
    pub fn outcome(&self, constructor: &str) -> Option<&OutcomeDef> {
        self.outcomes
            .iter()
            .find(|outcome| outcome.constructor.name == constructor)
    }
}

/// The source-neutral executable machine program.
///
/// This value owns everything required by typed-value validation and the
/// deterministic runtime. Presentation, routing, and evidence remain
/// application concerns on [`Program`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MachineProgram {
    pub protocol: String,
    pub language: String,
    /// Module-layout-independent identity mechanism used by this artifact.
    pub identity_protocol: String,
    pub modules: Vec<String>,
    pub types: BTreeMap<String, TypeDef>,
    pub constants: BTreeMap<String, Value>,
    #[serde(default)]
    pub constant_types: BTreeMap<String, TypeRef>,
    pub functions: BTreeMap<String, Function>,
    pub machines: BTreeMap<String, Machine>,
    /// Exact public Part declarations statically composed into each machine.
    ///
    /// Static composition lowers to one aggregate machine, so the runtime does
    /// not schedule or checkpoint these declaration identities.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub composed_part_declarations: BTreeMap<String, BTreeSet<String>>,
    /// Verifiable derivation frames for every runtime-observable fault site.
    /// The map key is the resulting `SiteId`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub site_identities: BTreeMap<String, SiteIdentityFrame>,
    pub program_hashes: BTreeMap<String, String>,
}

/// One complete checked application artifact.
///
/// `machine_program` is flattened deliberately: `uhura-ir/1` is the current
/// wire contract for one complete application.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Program {
    #[serde(flatten)]
    pub machine_program: MachineProgram,
    #[serde(default)]
    pub presentations: BTreeMap<String, Presentation>,
    #[serde(default)]
    pub evidence: EvidenceSuite,
    #[serde(default)]
    pub route_tables: BTreeMap<String, uhura_port::RouteTable>,
    #[serde(default)]
    pub presentation_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub evidence_hashes: BTreeMap<String, String>,
}

struct ProfileHashes {
    presentations: BTreeMap<String, String>,
    evidence: BTreeMap<String, String>,
}

impl MachineProgram {
    pub fn new() -> Self {
        Self {
            protocol: IR_PROTOCOL.into(),
            language: LANGUAGE.into(),
            identity_protocol: MACHINE_PROGRAM_ID_PROTOCOL.into(),
            modules: Vec::new(),
            types: BTreeMap::new(),
            constants: BTreeMap::new(),
            constant_types: BTreeMap::new(),
            functions: BTreeMap::new(),
            machines: BTreeMap::new(),
            composed_part_declarations: BTreeMap::new(),
            site_identities: BTreeMap::new(),
            program_hashes: BTreeMap::new(),
        }
    }

    pub fn validate_protocol(&self) -> Result<(), String> {
        if self.protocol != IR_PROTOCOL {
            return Err(format!("expected `{IR_PROTOCOL}`, got `{}`", self.protocol));
        }
        if self.language != LANGUAGE {
            return Err(format!(
                "expected Uhura language `{LANGUAGE}`, got `{}`",
                self.language
            ));
        }
        if self.identity_protocol != MACHINE_PROGRAM_ID_PROTOCOL {
            return Err(format!(
                "expected identity protocol `{MACHINE_PROGRAM_ID_PROTOCOL}`, got `{}`",
                self.identity_protocol
            ));
        }
        self.validate_v04_finite_view_boundaries()?;
        for (machine, declarations) in &self.composed_part_declarations {
            if !self.machines.contains_key(machine) {
                return Err(format!(
                    "Uhura composed-Part identity names unknown machine `{machine}`"
                ));
            }
            if declarations.iter().any(|declaration| {
                let Some((package, name)) = declaration.rsplit_once("::") else {
                    return true;
                };
                let Some((package_name, version)) = package.rsplit_once('@') else {
                    return true;
                };
                package_name.is_empty()
                    || name.is_empty()
                    || name.contains("::")
                    || version.parse::<u64>().map_or(true, |version| version == 0)
            }) {
                return Err(format!(
                    "Uhura machine `{machine}` has a malformed composed Part PublicId"
                ));
            }
        }
        self.validate_internal_loop_exits()?;
        self.validate_port_contract_instances()?;
        Ok(())
    }

    /// Freezes the current machine identities without an application profile.
    pub fn freeze_program_hashes(&mut self) {
        self.try_freeze_program_hashes()
            .expect("checked Uhura machine program must have hashable semantic material");
    }

    /// Fallible machine-identity recomputation for externally supplied IR.
    pub fn try_freeze_program_hashes(&mut self) -> Result<(), String> {
        let mut application = Program {
            machine_program: self.clone(),
            presentations: BTreeMap::new(),
            evidence: EvidenceSuite::default(),
            route_tables: BTreeMap::new(),
            presentation_hashes: BTreeMap::new(),
            evidence_hashes: BTreeMap::new(),
        };
        application.try_freeze_program_hashes()?;
        *self = application.machine_program;
        Ok(())
    }

    fn validate_v04_finite_view_boundaries(&self) -> Result<(), String> {
        for (id, definition) in &self.types {
            if let TypeDef::Key { underlying, .. } = definition {
                self.validate_v04_boundary_type(underlying, &format!("key `{id}`"))?;
            }
        }
        for (id, ty) in &self.constant_types {
            self.validate_v04_boundary_type(ty, &format!("constant `{id}`"))?;
        }
        for machine in self.machines.values() {
            self.validate_v04_boundary_type(
                &machine.config,
                &format!("machine `{}` configuration", machine.id),
            )?;
            self.validate_v04_boundary_definition(
                &machine.local_input,
                &format!("machine `{}` input", machine.id),
            )?;
            for command in &machine.local_commands {
                self.validate_v04_boundary_constructor(
                    &command.constructor,
                    &format!("machine `{}` command", machine.id),
                )?;
            }
            for outcome in &machine.outcomes {
                self.validate_v04_boundary_constructor(
                    &outcome.constructor,
                    &format!("machine `{}` outcome", machine.id),
                )?;
            }
            for field in &machine.state {
                self.validate_v04_boundary_type(
                    &field.ty,
                    &format!("machine `{}` state field `{}`", machine.id, field.name),
                )?;
            }
            for field in &machine.observation {
                self.validate_v04_boundary_type(
                    &field.ty,
                    &format!(
                        "machine `{}` observation field `{}`",
                        machine.id, field.name
                    ),
                )?;
            }
            for port in &machine.ports {
                for (index, ty) in port.type_arguments.iter().enumerate() {
                    self.validate_v04_boundary_type(
                        ty,
                        &format!(
                            "machine `{}` port `{}` contract type argument #{}",
                            machine.id,
                            port.name,
                            index + 1
                        ),
                    )?;
                }
                if let Some(configuration) = &port.configuration
                    && let Some(path) = self.v04_expression_finite_view_path(configuration)
                {
                    return Err(format!(
                        "Uhura 0.4 `FiniteView` is ephemeral and cannot cross machine `{}` port `{}` configuration; nested path: `{}`",
                        machine.id,
                        port.name,
                        path.join(" -> ")
                    ));
                }
                for constructor in &port.receive {
                    self.validate_v04_boundary_constructor(
                        constructor,
                        &format!("machine `{}` port `{}` receive", machine.id, port.name),
                    )?;
                }
                for constructor in &port.send {
                    self.validate_v04_boundary_constructor(
                        constructor,
                        &format!("machine `{}` port `{}` send", machine.id, port.name),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn validate_v04_boundary_definition(
        &self,
        definition: &TypeDef,
        boundary: &str,
    ) -> Result<(), String> {
        match definition {
            TypeDef::Key { underlying, .. } => {
                self.validate_v04_boundary_type(underlying, boundary)
            }
            TypeDef::Record { fields, .. } => {
                for (name, ty) in fields {
                    self.validate_v04_boundary_type(ty, &format!("{boundary} field `{name}`"))?;
                }
                Ok(())
            }
            TypeDef::Sum { constructors, .. } => {
                for constructor in constructors {
                    self.validate_v04_boundary_constructor(constructor, boundary)?;
                }
                Ok(())
            }
        }
    }

    fn validate_v04_boundary_constructor(
        &self,
        constructor: &ConstructorDef,
        boundary: &str,
    ) -> Result<(), String> {
        for (index, (name, ty)) in constructor.fields.iter().enumerate() {
            let field = name.as_ref().map_or_else(
                || format!("positional field #{}", index + 1),
                |name| format!("field `{name}`"),
            );
            self.validate_v04_boundary_type(
                ty,
                &format!("{boundary} constructor `{}` {field}", constructor.name),
            )?;
        }
        Ok(())
    }

    fn validate_v04_boundary_type(&self, ty: &TypeRef, boundary: &str) -> Result<(), String> {
        let Some(path) = finite_view_path(ty, &self.types, &mut BTreeSet::new()) else {
            return Ok(());
        };
        Err(format!(
            "Uhura 0.4 `FiniteView` is ephemeral and cannot cross {boundary}; nested path: `{}`",
            path.join(" -> ")
        ))
    }

    fn v04_expression_finite_view_path(&self, expression: &Expr) -> Option<Vec<String>> {
        match expression {
            Expr::Call { result_type, .. }
            | Expr::Method { result_type, .. }
            | Expr::Map { result_type, .. }
            | Expr::SetComprehension { result_type, .. } => {
                finite_view_path(result_type, &self.types, &mut BTreeSet::new())
            }
            Expr::Name { name } => self
                .constant_types
                .get(name)
                .and_then(|ty| finite_view_path(ty, &self.types, &mut BTreeSet::new())),
            Expr::Constructor { type_id, .. } | Expr::Key { type_id, .. } => finite_view_path(
                &TypeRef::Named {
                    id: type_id.clone(),
                },
                &self.types,
                &mut BTreeSet::new(),
            ),
            Expr::Tuple { values } => values.iter().enumerate().find_map(|(index, value)| {
                nested_finite_view_path(
                    format!("Tuple[{}]", index + 1),
                    self.v04_expression_finite_view_path(value),
                )
            }),
            Expr::Record { fields } => fields.iter().find_map(|(name, value)| {
                nested_finite_view_path(
                    format!("Record.{name}"),
                    self.v04_expression_finite_view_path(value),
                )
            }),
            Expr::Seq { values } => values.iter().find_map(|value| {
                nested_finite_view_path("Seq.item", self.v04_expression_finite_view_path(value))
            }),
            Expr::Table { entries, .. } => entries.iter().find_map(|(_, value)| {
                nested_finite_view_path("Table.value", self.v04_expression_finite_view_path(value))
            }),
            Expr::If {
                then_value,
                else_value,
                ..
            } => self
                .v04_expression_finite_view_path(then_value)
                .or_else(|| self.v04_expression_finite_view_path(else_value)),
            Expr::Match { arms, .. } => arms
                .iter()
                .find_map(|arm| self.v04_expression_finite_view_path(&arm.value)),
            Expr::Update { value, fields } => {
                self.v04_expression_finite_view_path(value).or_else(|| {
                    fields.iter().find_map(|(name, value)| {
                        nested_finite_view_path(
                            format!("Record.{name}"),
                            self.v04_expression_finite_view_path(value),
                        )
                    })
                })
            }
            Expr::Let { value, .. } => self.v04_expression_finite_view_path(value),
            Expr::Invoke { function, .. } => match function.as_ref() {
                Expr::Lambda { body, .. } => self.v04_expression_finite_view_path(body),
                _ => None,
            },
            Expr::Collect { clauses } => clauses.iter().find_map(|(_, value)| {
                nested_finite_view_path("Seq.item", self.v04_expression_finite_view_path(value))
            }),
            Expr::Field { value, field } => match value.as_ref() {
                Expr::Record { fields } => fields
                    .iter()
                    .find(|(name, _)| name == field)
                    .and_then(|(_, value)| self.v04_expression_finite_view_path(value)),
                _ => self
                    .v04_expression_declared_type(expression)
                    .and_then(|ty| finite_view_path(&ty, &self.types, &mut BTreeSet::new())),
            },
            Expr::Literal { .. }
            | Expr::Unary { .. }
            | Expr::Binary { .. }
            | Expr::Is { .. }
            | Expr::Lambda { .. } => None,
            Expr::Index { .. } => self
                .v04_expression_declared_type(expression)
                .and_then(|ty| finite_view_path(&ty, &self.types, &mut BTreeSet::new())),
        }
    }

    fn v04_expression_declared_type(&self, expression: &Expr) -> Option<TypeRef> {
        match expression {
            Expr::Literal { value } => match value {
                Value::Unit => Some(TypeRef::Unit),
                Value::Bool(_) => Some(TypeRef::Bool),
                Value::Integer { kind, .. } => Some(match kind {
                    IntegerKind::Int => TypeRef::Int,
                    IntegerKind::Nat => TypeRef::Nat,
                    IntegerKind::PositiveInt => TypeRef::PositiveInt,
                }),
                Value::Decimal(_) => Some(TypeRef::Decimal),
                Value::Ratio(_) => Some(TypeRef::Ratio),
                Value::Boundary(_) => Some(TypeRef::BoundaryNumber),
                Value::Text(_) => Some(TypeRef::Text),
                Value::Key { type_id, .. } | Value::Variant { type_id, .. } => {
                    Some(TypeRef::Named {
                        id: type_id.clone(),
                    })
                }
                Value::Tuple(values) => values
                    .iter()
                    .map(|value| {
                        self.v04_expression_declared_type(&Expr::Literal {
                            value: value.clone(),
                        })
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(|values| TypeRef::Tuple { values }),
                Value::Record(fields) => fields
                    .iter()
                    .map(|(name, value)| {
                        self.v04_expression_declared_type(&Expr::Literal {
                            value: value.clone(),
                        })
                        .map(|ty| (name.clone(), ty))
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(|fields| TypeRef::Record { fields }),
                Value::Seq(values) => values.first().and_then(|value| {
                    self.v04_expression_declared_type(&Expr::Literal {
                        value: value.clone(),
                    })
                    .map(|value| TypeRef::Seq {
                        value: Box::new(value),
                    })
                }),
                Value::NonEmpty(values) => values.first().and_then(|value| {
                    self.v04_expression_declared_type(&Expr::Literal {
                        value: value.clone(),
                    })
                    .map(|value| TypeRef::NonEmpty {
                        value: Box::new(value),
                    })
                }),
                Value::Set(_) | Value::Map(_) | Value::Table { .. } => None,
            },
            Expr::Name { name } => self.constant_types.get(name).cloned(),
            Expr::Constructor { type_id, .. } | Expr::Key { type_id, .. } => Some(TypeRef::Named {
                id: type_id.clone(),
            }),
            Expr::Tuple { values } => values
                .iter()
                .map(|value| self.v04_expression_declared_type(value))
                .collect::<Option<Vec<_>>>()
                .map(|values| TypeRef::Tuple { values }),
            Expr::Record { fields } => fields
                .iter()
                .map(|(name, value)| {
                    self.v04_expression_declared_type(value)
                        .map(|ty| (name.clone(), ty))
                })
                .collect::<Option<Vec<_>>>()
                .map(|fields| TypeRef::Record { fields }),
            Expr::Seq { values } => values.first().and_then(|value| {
                self.v04_expression_declared_type(value)
                    .map(|value| TypeRef::Seq {
                        value: Box::new(value),
                    })
            }),
            Expr::Map { result_type, .. }
            | Expr::Call { result_type, .. }
            | Expr::Method { result_type, .. }
            | Expr::SetComprehension { result_type, .. } => Some(result_type.clone()),
            Expr::Table {
                key_type, entries, ..
            } => entries.first().and_then(|(_, value)| {
                self.v04_expression_declared_type(value)
                    .map(|value| TypeRef::Table {
                        key: Box::new(TypeRef::Named {
                            id: key_type.clone(),
                        }),
                        value: Box::new(value),
                    })
            }),
            Expr::Unary { op, value } => match op {
                UnaryOp::Not => Some(TypeRef::Bool),
                UnaryOp::Negate => self.v04_expression_declared_type(value),
            },
            Expr::Binary {
                op, left, right: _, ..
            } => match op {
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply => {
                    self.v04_expression_declared_type(left)
                }
                BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
                | BinaryOp::And
                | BinaryOp::Or => Some(TypeRef::Bool),
            },
            Expr::Invoke { function, .. } => match function.as_ref() {
                Expr::Lambda { body, .. } => self.v04_expression_declared_type(body),
                _ => None,
            },
            Expr::Field { value, field } => {
                let value = self.v04_expression_declared_type(value)?;
                self.v04_field_type(&value, field, &mut BTreeSet::new())
            }
            Expr::Index { value, .. } => match self.v04_expression_declared_type(value)? {
                TypeRef::Seq { value }
                | TypeRef::NonEmpty { value }
                | TypeRef::FiniteView { value } => Some(*value),
                TypeRef::Map { value, .. } | TypeRef::Table { value, .. } => Some(*value),
                _ => None,
            },
            Expr::If {
                then_value,
                else_value,
                ..
            } => self
                .v04_expression_declared_type(then_value)
                .or_else(|| self.v04_expression_declared_type(else_value)),
            Expr::Match { arms, .. } => arms
                .first()
                .and_then(|arm| self.v04_expression_declared_type(&arm.value)),
            Expr::Is { .. } => Some(TypeRef::Bool),
            Expr::Update { value, .. } | Expr::Let { value, .. } => {
                self.v04_expression_declared_type(value)
            }
            Expr::Collect { clauses } => clauses.first().and_then(|(_, value)| {
                self.v04_expression_declared_type(value)
                    .map(|value| TypeRef::Seq {
                        value: Box::new(value),
                    })
            }),
            Expr::Lambda { .. } => None,
        }
    }

    fn v04_field_type(
        &self,
        ty: &TypeRef,
        field: &str,
        visited: &mut BTreeSet<String>,
    ) -> Option<TypeRef> {
        match ty {
            TypeRef::Record { fields } => fields
                .iter()
                .find(|(name, _)| name == field)
                .map(|(_, ty)| ty.clone()),
            TypeRef::Named { id } if visited.insert(id.clone()) => match self.types.get(id) {
                Some(TypeDef::Record { fields, .. }) => fields
                    .iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, ty)| ty.clone()),
                Some(TypeDef::Key { underlying, .. }) => {
                    self.v04_field_type(underlying, field, visited)
                }
                Some(TypeDef::Sum { .. }) | None => None,
            },
            _ => None,
        }
    }

    fn validate_internal_loop_exits(&self) -> Result<(), String> {
        fn option_constructor<'a>(expression: &'a Expr, constructor: &str) -> Option<&'a str> {
            match expression {
                Expr::Constructor {
                    type_id,
                    constructor: actual,
                    fields,
                } if actual == constructor
                    && type_id.starts_with("Option<")
                    && ((constructor == "none" && fields.is_empty())
                        || (constructor == "some" && fields.len() == 1)) =>
                {
                    Some(type_id)
                }
                _ => None,
            }
        }

        fn selected_type(statements: &[Statement], name: &str) -> Option<String> {
            for statement in statements {
                match statement {
                    Statement::Let {
                        name: binding,
                        value,
                        ..
                    } if binding == name => {
                        if let Some(type_id) = option_constructor(value, "some") {
                            return Some(type_id.to_owned());
                        }
                    }
                    Statement::If {
                        then_body,
                        else_body,
                        ..
                    } => {
                        if let Some(type_id) = selected_type(then_body, name)
                            .or_else(|| selected_type(else_body, name))
                        {
                            return Some(type_id);
                        }
                    }
                    Statement::Match { arms, .. } => {
                        if let Some(type_id) =
                            arms.iter().find_map(|arm| selected_type(&arm.body, name))
                        {
                            return Some(type_id);
                        }
                    }
                    Statement::While { body, .. } => {
                        if let Some(type_id) = selected_type(body, name) {
                            return Some(type_id);
                        }
                    }
                    _ => {}
                }
            }
            None
        }

        fn statements(body: &[Statement]) -> Result<(), String> {
            for (index, statement) in body.iter().enumerate() {
                match statement {
                    Statement::If {
                        then_body,
                        else_body,
                        ..
                    } => {
                        statements(then_body)?;
                        statements(else_body)?;
                    }
                    Statement::Match { arms, .. } => {
                        for arm in arms {
                            statements(&arm.body)?;
                        }
                    }
                    Statement::While {
                        body: loop_body,
                        break_local,
                        ..
                    } => {
                        if let Some(name) = break_local {
                            if !name
                                .starts_with(crate::runtime::INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX)
                            {
                                return Err(format!(
                                    "loop break local `{name}` is not in the compiler-reserved namespace"
                                ));
                            }
                            let initialized = index.checked_sub(1).and_then(|previous| match &body
                                [previous]
                            {
                                Statement::Let {
                                    name: binding,
                                    value,
                                    ..
                                } if binding == name => option_constructor(value, "none"),
                                _ => None,
                            });
                            let selected = selected_type(loop_body, name);
                            if initialized.is_none() || selected.as_deref() != initialized {
                                return Err(format!(
                                    "loop break local `{name}` must be the immediately initialized total Option local selected by its body"
                                ));
                            }
                        }
                        statements(loop_body)?;
                    }
                    Statement::Let { .. }
                    | Statement::Set { .. }
                    | Statement::Emit { .. }
                    | Statement::Finish { .. }
                    | Statement::Unreachable { .. }
                    | Statement::Delegate { .. } => {}
                }
            }
            Ok(())
        }

        for machine in self.machines.values() {
            for transition in machine.transitions.values() {
                statements(&transition.body)?;
            }
            for handler in machine.handlers.values() {
                statements(&handler.body)?;
            }
            statements(&machine.before_commit)?;
        }
        Ok(())
    }

    fn validate_port_contract_instances(&self) -> Result<(), String> {
        for machine in self.machines.values() {
            for port in &machine.ports {
                let resolved = port.contract_instance.as_ref().ok_or_else(|| {
                    format!(
                        "Uhura port `{}.{}` has no resolved contract instance",
                        machine.id, port.name
                    )
                })?;
                let expected = self.expected_standard_port_instance(port, resolved)?;
                validate_port_contract_instance(machine, port, resolved, &expected)?;
            }
        }
        Ok(())
    }

    fn expected_standard_port_instance(
        &self,
        port: &PortDef,
        resolved: &uhura_port::ContractInstance,
    ) -> Result<uhura_port::ContractInstance, String> {
        let boundary_types = port
            .type_arguments
            .iter()
            .map(|ty| {
                uhura_port::TypeRef::new(ty.canonical_name()).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let contract = port
            .contract
            .rsplit("::")
            .next()
            .unwrap_or(port.contract.as_str());
        match contract {
            "Observation" => {
                let [value] = boundary_types.as_slice() else {
                    return Err(format!(
                        "Observation port `{}` must have one type argument",
                        port.name
                    ));
                };
                uhura_port::observation_instance(value.clone()).map_err(|error| error.to_string())
            }
            "RequestPort" => {
                let [id, payload, settlement] = boundary_types.as_slice() else {
                    return Err(format!(
                        "RequestPort port `{}` must have three type arguments",
                        port.name
                    ));
                };
                uhura_port::request_port_instance(id.clone(), payload.clone(), settlement.clone())
                    .map_err(|error| error.to_string())
            }
            "SinkPort" => {
                let [value] = boundary_types.as_slice() else {
                    return Err(format!(
                        "SinkPort port `{}` must have one type argument",
                        port.name
                    ));
                };
                uhura_port::sink_port_instance(value.clone()).map_err(|error| error.to_string())
            }
            "Router" => {
                let [location] = boundary_types.as_slice() else {
                    return Err(format!(
                        "Router port `{}` must have one type argument",
                        port.name
                    ));
                };
                if !matches!(port.configuration, Some(Expr::Name { .. })) {
                    return Err(format!(
                        "Router port `{}` must retain its resolved route-table configuration",
                        port.name
                    ));
                }
                let routes: uhura_port::RouteTable = serde_json::from_value(
                    resolved.configuration.as_value().clone(),
                )
                .map_err(|error| {
                    format!(
                        "Router port `{}` has invalid resolved route-table configuration: {error}",
                        port.name
                    )
                })?;
                uhura_port::router_instance(location.clone(), &routes)
                    .map_err(|error| error.to_string())
            }
            other => Err(format!(
                "Uhura port `{}` uses unsupported contract `{other}`",
                port.name
            )),
        }
    }
}

impl Program {
    pub fn new() -> Self {
        Self {
            machine_program: MachineProgram::new(),
            presentations: BTreeMap::new(),
            evidence: EvidenceSuite::default(),
            route_tables: BTreeMap::new(),
            presentation_hashes: BTreeMap::new(),
            evidence_hashes: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn as_machine_program(&self) -> &MachineProgram {
        &self.machine_program
    }

    pub fn as_machine_program_mut(&mut self) -> &mut MachineProgram {
        &mut self.machine_program
    }

    #[must_use]
    pub fn into_machine_program(self) -> MachineProgram {
        self.machine_program
    }

    pub fn validate_protocol(&self) -> Result<(), String> {
        self.machine_program.validate_protocol()?;
        self.validate_port_contract_instances()?;
        Ok(())
    }

    fn validate_port_contract_instances(&self) -> Result<(), String> {
        for machine in self.machine_program.machines.values() {
            for port in &machine.ports {
                let resolved = port.contract_instance.as_ref().ok_or_else(|| {
                    format!(
                        "Uhura port `{}.{}` has no resolved contract instance",
                        machine.id, port.name
                    )
                })?;
                let expected = self.expected_standard_port_instance(port)?;
                validate_port_contract_instance(machine, port, resolved, &expected)?;
            }
        }
        Ok(())
    }

    fn expected_standard_port_instance(
        &self,
        port: &PortDef,
    ) -> Result<uhura_port::ContractInstance, String> {
        let boundary_types = port
            .type_arguments
            .iter()
            .map(|ty| {
                uhura_port::TypeRef::new(ty.canonical_name()).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let contract = port
            .contract
            .rsplit("::")
            .next()
            .unwrap_or(port.contract.as_str());
        match contract {
            "Observation" => {
                let [value] = boundary_types.as_slice() else {
                    return Err(format!(
                        "Observation port `{}` must have one type argument",
                        port.name
                    ));
                };
                uhura_port::observation_instance(value.clone()).map_err(|error| error.to_string())
            }
            "RequestPort" => {
                let [id, payload, settlement] = boundary_types.as_slice() else {
                    return Err(format!(
                        "RequestPort port `{}` must have three type arguments",
                        port.name
                    ));
                };
                uhura_port::request_port_instance(id.clone(), payload.clone(), settlement.clone())
                    .map_err(|error| error.to_string())
            }
            "SinkPort" => {
                let [value] = boundary_types.as_slice() else {
                    return Err(format!(
                        "SinkPort port `{}` must have one type argument",
                        port.name
                    ));
                };
                uhura_port::sink_port_instance(value.clone()).map_err(|error| error.to_string())
            }
            "Router" => {
                let [location] = boundary_types.as_slice() else {
                    return Err(format!(
                        "Router port `{}` must have one type argument",
                        port.name
                    ));
                };
                let Some(Expr::Name { name }) = &port.configuration else {
                    return Err(format!(
                        "Router port `{}` must retain its resolved route-table configuration",
                        port.name
                    ));
                };
                let routes = self.route_tables.get(name).ok_or_else(|| {
                    format!(
                        "Router port `{}` refers to unknown route table `{name}`",
                        port.name
                    )
                })?;
                uhura_port::router_instance(location.clone(), routes)
                    .map_err(|error| error.to_string())
            }
            other => Err(format!(
                "Uhura port `{}` uses unsupported contract `{other}`",
                port.name
            )),
        }
    }

    pub fn to_canonical_string(&self) -> String {
        uhura_base::to_canonical_json(
            &serde_json::to_value(self).expect("Uhura machine IR is serializable"),
        )
    }

    pub fn from_json(source: &str) -> Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_str(source).map_err(|error| format!("Uhura machine IR: {error}"))?;
        let canonical = uhura_base::try_to_canonical_json(&json)
            .map_err(|error| format!("Uhura machine IR is not canonical: {error}"))?;
        if canonical != source {
            return Err("Uhura machine IR must be exact canonical `uhura-ir/1` JSON".to_string());
        }

        let mut program: Self = serde_json::from_value(json.clone())
            .map_err(|error| format!("Uhura machine IR: {error}"))?;
        let typed_json = serde_json::to_value(&program)
            .map_err(|error| format!("Uhura machine IR cannot be represented: {error}"))?;
        if typed_json != json {
            return Err(
                "Uhura machine IR does not match the closed `uhura-ir/1` schema".to_string(),
            );
        }
        program.validate_protocol()?;
        let supplied_program = program.machine_program.program_hashes.clone();
        let supplied_presentations = program.presentation_hashes.clone();
        let supplied_evidence = program.evidence_hashes.clone();
        program.try_freeze_program_hashes()?;
        if !supplied_program.is_empty()
            && supplied_program != program.machine_program.program_hashes
        {
            return Err(
                "Uhura machine IR machine-program hashes do not match executable semantics".into(),
            );
        }
        if !supplied_presentations.is_empty()
            && supplied_presentations != program.presentation_hashes
        {
            return Err(
                "Uhura machine IR presentation hashes do not match executable semantics".into(),
            );
        }
        if !supplied_evidence.is_empty() && supplied_evidence != program.evidence_hashes {
            return Err(
                "Uhura machine IR evidence hashes do not match executable semantics".into(),
            );
        }
        Ok(program)
    }

    /// Recomputes every current identity from executable semantics.
    ///
    /// Presentation, evidence, physical source paths, and byte spans are
    /// deliberately excluded. Semantic source identities remain because the
    /// runtime exposes them in faults and receipts. Only declarations
    /// transitively referenced by the selected checked machine enter its
    /// material.
    pub fn freeze_program_hashes(&mut self) {
        self.try_freeze_program_hashes()
            .expect("checked Uhura IR must have hashable semantic material");
    }

    /// Fallible identity recomputation for externally supplied IR.
    pub fn try_freeze_program_hashes(&mut self) -> Result<(), String> {
        if self.machine_program.protocol != IR_PROTOCOL
            || self.machine_program.language != LANGUAGE
            || self.machine_program.identity_protocol != MACHINE_PROGRAM_ID_PROTOCOL
        {
            self.machine_program.validate_protocol()?;
        }
        self.machine_program.assign_v04_site_ids();
        self.machine_program.validate_v04_site_ids()?;
        self.assign_v04_presentation_node_ids();
        let program_hashes = self
            .machine_program
            .machines
            .iter()
            .map(|(id, machine)| {
                let material = self.machine_program_material(id, machine);
                let bytes = uhura_base::try_to_canonical_json(&material)
                    .map_err(|error| {
                        format!("Uhura machine `{id}` has noncanonical semantic material: {error}")
                    })?
                    .into_bytes();
                Ok((
                    id.clone(),
                    super::codec::hex(&super::codec::hash(MACHINE_PROGRAM_ID_PROTOCOL, &[bytes])),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let profile_hashes = self.compute_profile_hashes(&program_hashes)?;
        self.machine_program.program_hashes = program_hashes;
        self.presentation_hashes = profile_hashes.presentations;
        self.evidence_hashes = profile_hashes.evidence;
        Ok(())
    }

    fn compute_profile_hashes(
        &self,
        program_hashes: &BTreeMap<String, String>,
    ) -> Result<ProfileHashes, String> {
        let presentation_hashes = self
            .presentations
            .iter()
            .map(|(id, presentation)| {
                let mut references = SemanticReferences::default();
                collect_presentation_references(presentation, &mut references);
                if presentation.id != *id {
                    return Err(format!(
                        "Uhura presentation map key `{id}` differs from its resolved PublicId `{}`",
                        presentation.id
                    ));
                }
                let material = serde_json::json!({
                    "binding": presentation.binding,
                    "nodes": semantic_json(&presentation.nodes),
                    "dependencies": self.dependency_material(references),
                });
                let semantic_ir = uhura_base::try_to_canonical_json(&material)
                    .map_err(|error| {
                        format!(
                            "Uhura presentation `{id}` has noncanonical semantic material: {error}"
                        )
                    })?
                    .into_bytes();
                let interface = match self.machine_program.machines.get(&presentation.machine) {
                    Some(machine) if machine.id != presentation.machine => {
                        return Err(format!(
                            "Uhura presentation `{id}` resolves machine key `{}` to mismatched PublicId `{}`",
                            presentation.machine, machine.id
                        ));
                    }
                    Some(machine) => {
                        machine_ui_interface_hash(&self.machine_program, machine).map_err(
                            |error| {
                            format!(
                                "Uhura presentation `{id}` has invalid machine interface type material: {error}"
                            )
                        },
                        )?
                    }
                    None => {
                        return Err(format!(
                            "Uhura presentation `{id}` binds unknown resolved machine PublicId `{}`",
                            presentation.machine
                        ));
                    }
                };
                Ok((
                    id.clone(),
                    super::codec::hex(&super::codec::hash(
                        PRESENTATION_ID_PROTOCOL,
                        &[
                            presentation.id.as_bytes().to_vec(),
                            presentation.machine.as_bytes().to_vec(),
                            interface.to_vec(),
                            semantic_ir,
                        ],
                    )),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;

        let evidence_machines = self
            .evidence
            .scenarios
            .keys()
            .filter_map(|id| scenario_machine(&self.evidence, id, &mut BTreeSet::new()))
            .collect::<BTreeSet<_>>();
        let evidence_hashes = evidence_machines
            .into_iter()
            .map(|machine| {
                let scenario_ids = self
                    .evidence
                    .scenarios
                    .keys()
                    .filter(|id| {
                        scenario_machine(&self.evidence, id, &mut BTreeSet::new()).as_deref()
                            == Some(machine.as_str())
                    })
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let scenarios = scenario_ids
                    .iter()
                    .filter_map(|id| {
                        self.evidence
                            .scenarios
                            .get(id)
                            .map(|scenario| (id.clone(), semantic_json(scenario)))
                    })
                    .collect::<BTreeMap<_, _>>();
                let examples = self
                    .evidence
                    .examples
                    .iter()
                    .filter(|(_, reference)| scenario_ids.contains(&reference.scenario))
                    .map(|(id, reference)| (id.clone(), semantic_json(reference)))
                    .collect::<BTreeMap<_, _>>();
                let checkpoints = self
                    .evidence
                    .checkpoints
                    .iter()
                    .filter(|(_, reference)| scenario_ids.contains(&reference.scenario))
                    .map(|(id, reference)| (id.clone(), semantic_json(reference)))
                    .collect::<BTreeMap<_, _>>();
                let mut references = SemanticReferences::default();
                for id in &scenario_ids {
                    if let Some(scenario) = self.evidence.scenarios.get(id) {
                        collect_scenario_references(scenario, &mut references);
                    }
                }
                let semantic_ir = uhura_base::try_to_canonical_json(&serde_json::json!({
                    "identityProtocol": self.machine_program.identity_protocol,
                    "machine": machine,
                    "scenarios": scenarios,
                    "examples": examples,
                    "checkpoints": checkpoints,
                    "dependencies": self.dependency_material(references),
                }))
                .map_err(|error| {
                    format!("Uhura evidence for `{machine}` has noncanonical material: {error}")
                })?
                .into_bytes();
                let fixture_configuration = self.fixture_configuration(&machine, &scenario_ids);
                let program_hash = program_hashes
                    .get(&machine)
                    .ok_or_else(|| {
                        format!("Uhura evidence refers to unknown machine PublicId `{machine}`")
                    })
                    .and_then(|hash| super::codec::decode_hex_32(hash))?;
                Ok((
                    machine,
                    super::codec::hex(&super::codec::hash(
                        EVIDENCE_ID_PROTOCOL,
                        &[program_hash.to_vec(), semantic_ir, fixture_configuration],
                    )),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        Ok(ProfileHashes {
            presentations: presentation_hashes,
            evidence: evidence_hashes,
        })
    }

    pub fn machine_ui_interface_hash(&self, machine_id: &str) -> Result<String, String> {
        let machine = self
            .machine_program
            .machines
            .get(machine_id)
            .ok_or_else(|| format!("unknown Uhura machine `{machine_id}`"))?;
        Ok(super::codec::hex(&machine_ui_interface_hash(
            &self.machine_program,
            machine,
        )?))
    }

    fn dependency_material(&self, mut references: SemanticReferences) -> serde_json::Value {
        let mut selected_types = BTreeSet::new();
        let mut selected_constants = BTreeSet::new();
        let mut selected_functions = BTreeSet::new();
        let mut selected_routes = BTreeSet::new();
        loop {
            let mut changed = false;
            for id in references.types.clone() {
                if self.machine_program.types.contains_key(&id) && selected_types.insert(id.clone())
                {
                    collect_type_definition_references(
                        self.machine_program
                            .types
                            .get(&id)
                            .expect("selected type exists"),
                        &mut references,
                    );
                    changed = true;
                }
            }
            for id in references.constants.clone() {
                if self.machine_program.constants.contains_key(&id)
                    && selected_constants.insert(id.clone())
                {
                    if let Some(ty) = self.machine_program.constant_types.get(&id) {
                        collect_type_references(ty, &mut references);
                    }
                    collect_value_references(
                        self.machine_program
                            .constants
                            .get(&id)
                            .expect("selected constant exists"),
                        &mut references,
                    );
                    if self.route_tables.contains_key(&id) {
                        references.routes.insert(id.clone());
                    }
                    changed = true;
                }
            }
            for id in references.functions.clone() {
                if self.machine_program.functions.contains_key(&id)
                    && selected_functions.insert(id.clone())
                {
                    collect_function_references(
                        self.machine_program
                            .functions
                            .get(&id)
                            .expect("selected function exists"),
                        &mut references,
                    );
                    changed = true;
                }
            }
            for id in references.routes.clone() {
                if self.route_tables.contains_key(&id) && selected_routes.insert(id) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let types = selected_types
            .iter()
            .filter_map(|id| {
                self.machine_program
                    .types
                    .get(id)
                    .map(|value| (id.clone(), semantic_json(value)))
            })
            .collect::<BTreeMap<_, _>>();
        let functions = selected_functions
            .iter()
            .filter_map(|id| {
                self.machine_program
                    .functions
                    .get(id)
                    .map(|value| (id.clone(), semantic_json(value)))
            })
            .collect::<BTreeMap<_, _>>();
        let constants = selected_constants
            .iter()
            .filter_map(|id| {
                let value = self.machine_program.constants.get(id)?;
                let ty = self.machine_program.constant_types.get(id);
                let encoded = ty
                    .and_then(|ty| self.machine_program.canonical_value_bytes(ty, value).ok())
                    .map(|bytes| super::codec::hex(&bytes));
                Some((
                    id.clone(),
                    serde_json::json!({
                        "type": ty.map(TypeRef::canonical_name),
                        "canonicalBytes": encoded,
                        "fallbackValue": encoded.is_none().then(|| semantic_json(value)),
                    }),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let route_tables = selected_routes
            .iter()
            .filter_map(|id| {
                self.route_tables
                    .get(id)
                    .map(|value| (id.clone(), semantic_json(value)))
            })
            .collect::<BTreeMap<_, _>>();
        serde_json::json!({
            "types": types,
            "constants": constants,
            "functions": functions,
            "routeTables": route_tables,
        })
    }

    fn fixture_configuration(&self, machine_id: &str, scenario_ids: &BTreeSet<String>) -> Vec<u8> {
        let Some(machine) = self.machine_program.machines.get(machine_id) else {
            return uhura_base::to_canonical_json(&serde_json::json!({
                "missingMachine": machine_id,
            }))
            .into_bytes();
        };
        let empty_state = BTreeMap::new();
        let evaluate = |expression: &Expr| {
            super::runtime::evaluate_with_locals(
                &self.machine_program,
                machine,
                &Value::Unit,
                &empty_state,
                BTreeMap::new(),
                expression,
            )
            .map(|value| value.to_wire_json())
            .map_err(|error| error.to_string())
        };
        let mut fixtures = Vec::new();
        for scenario_id in scenario_ids {
            let Some(scenario) = self.evidence.scenarios.get(scenario_id) else {
                continue;
            };
            for (step_index, step) in scenario.steps.iter().enumerate() {
                let EvidenceStep::Bind { port, fixture, .. } = step else {
                    continue;
                };
                let port_definition = machine
                    .ports
                    .iter()
                    .find(|candidate| candidate.name == *port);
                let (contract, contract_hash, declared_configuration) = match port_definition {
                    Some(port_definition) => (
                        Some(port_definition.contract.clone()),
                        Some(port_definition.contract_hash.clone()),
                        port_definition
                            .configuration
                            .as_ref()
                            .map(&evaluate)
                            .transpose()
                            .map(|value| value.unwrap_or_else(|| Value::Unit.to_wire_json())),
                    ),
                    None => (None, None, Err(format!("unknown fixture port `{port}`"))),
                };
                let fixture_configuration = match fixture {
                    Expr::Call { args, .. } if args.len() <= 1 => args
                        .first()
                        .map(&evaluate)
                        .transpose()
                        .map(|value| value.unwrap_or_else(|| Value::Unit.to_wire_json())),
                    Expr::Call { args, .. } => Err(format!(
                        "fixture binding has {} configuration arguments",
                        args.len()
                    )),
                    _ => Err("fixture binding is not a checked fixture call".into()),
                };
                fixtures.push(serde_json::json!({
                    "scenario": scenario_id,
                    "step": step_index,
                    "port": port,
                    "contract": contract,
                    "contractHash": contract_hash,
                    "declaredConfiguration": result_json(declared_configuration),
                    "fixtureConfiguration": result_json(fixture_configuration),
                }));
            }
        }
        uhura_base::to_canonical_json(&serde_json::json!({ "fixtures": fixtures })).into_bytes()
    }

    fn machine_program_material(&self, machine_id: &str, machine: &Machine) -> serde_json::Value {
        let mut references = SemanticReferences::default();
        collect_machine_references(machine, &mut references);
        let mut selected_types = BTreeSet::new();
        let mut selected_constants = BTreeSet::new();
        let mut selected_functions = BTreeSet::new();
        let mut selected_routes = BTreeSet::new();
        let machine_json = semantic_machine_json_v04(machine);

        loop {
            let mut changed = false;
            for id in references.types.clone() {
                if self.machine_program.types.contains_key(&id) && selected_types.insert(id.clone())
                {
                    collect_type_definition_references(
                        self.machine_program
                            .types
                            .get(&id)
                            .expect("selected type exists"),
                        &mut references,
                    );
                    changed = true;
                }
            }
            for id in references.constants.clone() {
                if self.machine_program.constants.contains_key(&id)
                    && selected_constants.insert(id.clone())
                {
                    if let Some(ty) = self.machine_program.constant_types.get(&id) {
                        collect_type_references(ty, &mut references);
                    }
                    collect_value_references(
                        self.machine_program
                            .constants
                            .get(&id)
                            .expect("selected constant exists"),
                        &mut references,
                    );
                    if self.route_tables.contains_key(&id) {
                        references.routes.insert(id.clone());
                    }
                    changed = true;
                }
            }
            for id in references.functions.clone() {
                if self.machine_program.functions.contains_key(&id)
                    && selected_functions.insert(id.clone())
                {
                    collect_function_references(
                        self.machine_program
                            .functions
                            .get(&id)
                            .expect("selected function exists"),
                        &mut references,
                    );
                    changed = true;
                }
            }
            for id in references.routes.clone() {
                if self.route_tables.contains_key(&id) && selected_routes.insert(id) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let types = selected_types
            .iter()
            .filter_map(|id| {
                self.machine_program
                    .types
                    .get(id)
                    .map(|value| (id.clone(), semantic_json(value)))
            })
            .collect::<BTreeMap<_, _>>();
        let functions = selected_functions
            .iter()
            .filter_map(|id| {
                self.machine_program
                    .functions
                    .get(id)
                    .map(|value| (id.clone(), semantic_function_json_v04(value)))
            })
            .collect::<BTreeMap<_, _>>();
        let constants = selected_constants
            .iter()
            .filter_map(|id| {
                let value = self.machine_program.constants.get(id)?;
                let ty = self.machine_program.constant_types.get(id);
                let encoded = ty
                    .and_then(|ty| self.machine_program.canonical_value_bytes(ty, value).ok())
                    .map(|bytes| super::codec::hex(&bytes));
                Some((
                    id.clone(),
                    serde_json::json!({
                        "type": ty.map(TypeRef::canonical_name),
                        "canonicalBytes": encoded,
                        "fallbackValue": encoded.is_none().then(|| semantic_json(value)),
                    }),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let route_tables = selected_routes
            .iter()
            .filter_map(|id| {
                self.route_tables
                    .get(id)
                    .map(|value| (id.clone(), semantic_json(value)))
            })
            .collect::<BTreeMap<_, _>>();

        let mut material = serde_json::json!({
            "identityProtocol": self.machine_program.identity_protocol,
            "language": self.machine_program.language,
            "machineIdentity": machine_id,
            "machine": machine_json,
            "types": types,
            "constants": constants,
            "functions": functions,
            "routeTables": route_tables,
            "loweringOptions": [],
        });
        material
            .as_object_mut()
            .expect("machine material is an object")
            .insert(
                "composedPartDeclarations".into(),
                serde_json::to_value(
                    self.machine_program
                        .composed_part_declarations
                        .get(machine_id)
                        .cloned()
                        .unwrap_or_default(),
                )
                .expect("composed Part declaration identities serialize"),
            );
        material
    }
}

impl MachineProgram {
    fn assign_v04_site_ids(&mut self) {
        let identities = &mut self.site_identities;
        for (machine_id, machine) in &mut self.machines {
            for (index, (_, source)) in machine.invariants.iter_mut().enumerate() {
                let frame = SiteIdentityFrame::new(
                    machine_id,
                    "root",
                    "invariant",
                    format!("invariant/{index}"),
                );
                assign_site_identity(source, frame, identities);
            }
            for (input, handler) in &mut machine.handlers {
                assign_statement_site_ids(
                    machine_id,
                    &format!("handler/{input}"),
                    &mut handler.body,
                    identities,
                );
            }
            for (name, transition) in &mut machine.transitions {
                assign_statement_site_ids(
                    machine_id,
                    &format!("update/{name}"),
                    &mut transition.body,
                    identities,
                );
            }
            assign_statement_site_ids(
                machine_id,
                "before-commit",
                &mut machine.before_commit,
                identities,
            );
        }
    }

    fn validate_v04_site_ids(&self) -> Result<(), String> {
        let mut used = BTreeMap::<&str, BTreeSet<&str>>::new();
        for (machine_id, machine) in &self.machines {
            for (_, source) in &machine.invariants {
                used.entry(&source.id).or_default().insert(machine_id);
            }
            collect_statement_site_ids(&machine.before_commit, machine_id, &mut used);
            for transition in machine.transitions.values() {
                collect_statement_site_ids(&transition.body, machine_id, &mut used);
            }
            for handler in machine.handlers.values() {
                collect_statement_site_ids(&handler.body, machine_id, &mut used);
            }
        }

        for (site_id, owners) in &used {
            if !is_lower_sha256(site_id) {
                return Err(format!(
                    "Uhura 0.4 fault site `{site_id}` is not a lowercase SHA-256 identity"
                ));
            }
            let frame = self.site_identities.get(*site_id).ok_or_else(|| {
                format!("Uhura 0.4 fault site `{site_id}` has no canonical identity frame")
            })?;
            if frame.site_id() != **site_id {
                return Err(format!(
                    "Uhura 0.4 fault site `{site_id}` does not match its canonical identity frame"
                ));
            }
            if owners.len() != 1 || !owners.contains(frame.public_owner.as_str()) {
                return Err(format!(
                    "Uhura 0.4 fault site `{site_id}` has public owner `{}`, but occurs in {}",
                    frame.public_owner,
                    owners.iter().copied().collect::<Vec<_>>().join(", ")
                ));
            }
            if !matches!(
                frame.kind.as_str(),
                "invariant" | "invariant_condition" | "unreachable"
            ) {
                return Err(format!(
                    "Uhura 0.4 fault site `{site_id}` has unsupported kind `{}`",
                    frame.kind
                ));
            }
        }
        if let Some(unused) = self
            .site_identities
            .keys()
            .find(|site_id| !used.contains_key(site_id.as_str()))
        {
            return Err(format!(
                "Uhura 0.4 site identity frame `{unused}` is not used by executable semantics"
            ));
        }
        Ok(())
    }
}

impl Program {
    fn assign_v04_presentation_node_ids(&mut self) {
        for presentation in self.presentations.values_mut() {
            let name = presentation
                .id
                .rsplit_once("::")
                .map_or(presentation.id.as_str(), |(_, name)| name);
            presentation.source.id = v04_node_id(
                &presentation.id,
                "root",
                "ui",
                &format!("declaration/{name}"),
            );
            assign_ui_node_ids(&presentation.id, "tree", &mut presentation.nodes);
        }
    }
}

fn validate_lowered_contract_sum(
    machine: &str,
    port: &str,
    direction: &str,
    lowered: &[ConstructorDef],
    resolved: &uhura_port::SumDecl,
) -> Result<(), String> {
    let lowered = lowered
        .iter()
        .map(|constructor| {
            (
                constructor.name.as_str(),
                constructor
                    .fields
                    .iter()
                    .map(|(name, ty)| (name.as_deref(), ty.canonical_name()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let resolved = resolved
        .constructors
        .iter()
        .map(|constructor| {
            (
                constructor.name.as_str(),
                constructor
                    .fields
                    .iter()
                    .map(|field| (Some(field.name.as_str()), field.ty.as_str().to_string()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    if lowered == resolved {
        Ok(())
    } else {
        Err(format!(
            "Uhura port `{machine}.{port}` lowered {direction} sum differs from its resolved contract instance"
        ))
    }
}

fn validate_port_contract_instance(
    machine: &Machine,
    port: &PortDef,
    resolved: &uhura_port::ContractInstance,
    expected: &uhura_port::ContractInstance,
) -> Result<(), String> {
    if resolved != expected {
        return Err(format!(
            "Uhura port `{}.{}` contract instance differs from its checked standard contract",
            machine.id, port.name
        ));
    }
    if port.contract != expected.identity.to_string() {
        return Err(format!(
            "Uhura port `{}.{}` contract identity does not match its resolved instance",
            machine.id, port.name
        ));
    }
    if port.contract_hash != expected.content_hash {
        return Err(format!(
            "Uhura port `{}.{}` contract hash does not match its resolved instance",
            machine.id, port.name
        ));
    }
    let expected_arguments = expected
        .type_arguments
        .iter()
        .map(|argument| argument.argument.as_str())
        .collect::<Vec<_>>();
    let actual_arguments = port
        .type_arguments
        .iter()
        .map(TypeRef::canonical_name)
        .collect::<Vec<_>>();
    if actual_arguments
        != expected_arguments
            .iter()
            .map(|argument| (*argument).to_string())
            .collect::<Vec<_>>()
    {
        return Err(format!(
            "Uhura port `{}.{}` type arguments do not match its resolved instance",
            machine.id, port.name
        ));
    }
    validate_lowered_contract_sum(
        &machine.id,
        &port.name,
        "receive",
        &port.receive,
        &expected.receive,
    )?;
    validate_lowered_contract_sum(&machine.id, &port.name, "send", &port.send, &expected.send)
}

fn result_json(result: Result<serde_json::Value, String>) -> serde_json::Value {
    match result {
        Ok(value) => serde_json::json!({ "kind": "value", "value": value }),
        Err(message) => {
            serde_json::json!({ "kind": "invalid-checked-fixture", "message": message })
        }
    }
}

fn machine_ui_interface_hash(
    program: &MachineProgram,
    machine: &Machine,
) -> Result<[u8; 32], String> {
    let observation_type = MachineProgram::machine_observation_type(machine);
    let observation = super::typed::canonical_type_identity_bytes(&observation_type)
        .map_err(|error| error.to_string())?;
    let local_input = type_definition_projection(&machine.local_input)?;

    // The public machine input is the aggregate local sum followed by every
    // non-empty port receive sum in canonical port-name order. Send-only ports
    // contribute the empty sum and therefore do not change the UI input
    // interface.
    let mut aggregate_input_parts = vec![local_input];
    let mut receive_ports = machine
        .ports
        .iter()
        .filter(|port| !port.receive.is_empty())
        .collect::<Vec<_>>();
    receive_ports.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    aggregate_input_parts.extend(
        receive_ports
            .iter()
            .map(|port| {
                type_definition_projection(&TypeDef::Sum {
                    id: format!("{}::port.{}.Receive", machine.id, port.name),
                    constructors: port.receive.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    let input = super::codec::frame("aggregate-input-contract", &aggregate_input_parts);

    let mut input_references = SemanticReferences::default();
    collect_type_definition_references(&machine.local_input, &mut input_references);
    for port in receive_ports {
        collect_constructor_references(&port.receive, &mut input_references);
    }
    let input_types = reachable_type_definitions(program, input_references)
        .iter()
        .filter_map(|id| program.types.get(id))
        .map(type_definition_projection)
        .collect::<Result<Vec<_>, _>>()?;
    let input_contract = super::codec::frame(
        "reachable-input-contract",
        &[
            input,
            super::codec::frame("referenced-type-list", &input_types),
        ],
    );

    let mut observation_references = SemanticReferences::default();
    collect_type_references(&observation_type, &mut observation_references);
    let observation_types = reachable_type_definitions(program, observation_references)
        .iter()
        .filter_map(|id| program.types.get(id))
        .map(type_definition_projection)
        .collect::<Result<Vec<_>, _>>()?;
    let observation_contract = super::codec::frame(
        "reachable-observation-contract",
        &[
            observation,
            super::codec::frame("referenced-type-list", &observation_types),
        ],
    );

    Ok(super::codec::hash(
        MACHINE_UI_INTERFACE_ID_PROTOCOL,
        &[
            machine.id.as_bytes().to_vec(),
            input_contract,
            observation_contract,
        ],
    ))
}

fn reachable_type_definitions(
    program: &MachineProgram,
    mut references: SemanticReferences,
) -> BTreeSet<String> {
    let mut selected = BTreeSet::new();
    loop {
        let mut changed = false;
        for id in references.types.clone() {
            if program.types.contains_key(&id) && selected.insert(id.clone()) {
                collect_type_definition_references(
                    program.types.get(&id).expect("selected type exists"),
                    &mut references,
                );
                changed = true;
            }
        }
        if !changed {
            return selected;
        }
    }
}

fn type_definition_projection(definition: &TypeDef) -> Result<Vec<u8>, String> {
    Ok(match definition {
        TypeDef::Key { id, underlying } => super::codec::frame(
            "key-type",
            &[
                id.as_bytes().to_vec(),
                super::typed::canonical_type_identity_bytes(underlying)
                    .map_err(|error| error.to_string())?,
            ],
        ),
        TypeDef::Record { id, fields } => {
            let mut parts = vec![id.as_bytes().to_vec()];
            parts.extend(
                fields
                    .iter()
                    .map(|(name, ty)| {
                        Ok(super::codec::frame(
                            "field",
                            &[
                                name.as_bytes().to_vec(),
                                super::typed::canonical_type_identity_bytes(ty)
                                    .map_err(|error| error.to_string())?,
                            ],
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            );
            super::codec::frame("record-type", &parts)
        }
        TypeDef::Sum { id, constructors } => {
            let mut parts = vec![id.as_bytes().to_vec()];
            parts.extend(
                constructors
                    .iter()
                    .map(|constructor| {
                        let mut constructor_parts = vec![constructor.name.as_bytes().to_vec()];
                        constructor_parts.extend(
                            constructor
                                .fields
                                .iter()
                                .map(|(name, ty)| {
                                    Ok(super::codec::frame(
                                        "field",
                                        &[
                                            match name {
                                                Some(name) => super::codec::frame(
                                                    "some",
                                                    &[name.as_bytes().to_vec()],
                                                ),
                                                None => super::codec::frame("none", &[]),
                                            },
                                            super::typed::canonical_type_identity_bytes(ty)
                                                .map_err(|error| error.to_string())?,
                                        ],
                                    ))
                                })
                                .collect::<Result<Vec<_>, String>>()?,
                        );
                        Ok(super::codec::frame("constructor", &constructor_parts))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            );
            super::codec::frame("sum-type", &parts)
        }
    })
}

fn scenario_machine(
    suite: &EvidenceSuite,
    scenario_id: &str,
    visiting: &mut BTreeSet<String>,
) -> Option<String> {
    if !visiting.insert(scenario_id.into()) {
        return None;
    }
    let scenario = suite.scenarios.get(scenario_id)?;
    match &scenario.origin {
        ScenarioOrigin::Machine { machine, .. } => Some(machine.clone()),
        ScenarioOrigin::Snapshot { reference } => {
            scenario_machine(suite, &reference.scenario, visiting)
        }
    }
}

fn collect_presentation_references(
    presentation: &Presentation,
    references: &mut SemanticReferences,
) {
    for node in &presentation.nodes {
        collect_ui_node_references(node, references);
    }
}

fn collect_ui_node_references(node: &UiNode, references: &mut SemanticReferences) {
    match node {
        UiNode::Text { .. } => {}
        UiNode::Interpolation { value, .. } => {
            collect_expression_references(value, references);
        }
        UiNode::Element {
            attributes,
            children,
            ..
        } => {
            for attribute in attributes {
                match &attribute.value {
                    UiAttributeValue::Text { .. } => {}
                    UiAttributeValue::Expression { value } => {
                        collect_expression_references(value, references);
                    }
                    UiAttributeValue::Event { input, .. } => {
                        collect_expression_references(input, references);
                    }
                }
            }
            for child in children {
                collect_ui_node_references(child, references);
            }
        }
        UiNode::If {
            condition,
            children,
            ..
        } => {
            collect_expression_references(condition, references);
            for child in children {
                collect_ui_node_references(child, references);
            }
        }
        UiNode::Match { value, cases, .. } => {
            collect_expression_references(value, references);
            for case in cases {
                collect_pattern_references(&case.pattern, references);
                for child in &case.children {
                    collect_ui_node_references(child, references);
                }
            }
        }
        UiNode::Each {
            value,
            pattern,
            key,
            children,
            ..
        } => {
            collect_expression_references(value, references);
            collect_pattern_references(pattern, references);
            collect_expression_references(key, references);
            for child in children {
                collect_ui_node_references(child, references);
            }
        }
    }
}

fn collect_scenario_references(scenario: &Scenario, references: &mut SemanticReferences) {
    for step in &scenario.steps {
        match step {
            EvidenceStep::Bind { fixture, .. }
            | EvidenceStep::Send { input: fixture, .. }
            | EvidenceStep::Deliver { input: fixture, .. }
            | EvidenceStep::ExpectObservationWhere {
                condition: fixture, ..
            } => collect_expression_references(fixture, references),
            EvidenceStep::ExpectReaction {
                outcome, commands, ..
            } => {
                collect_pattern_references(outcome, references);
                for command in commands {
                    collect_expression_references(command, references);
                }
            }
            EvidenceStep::ExpectObservationPattern { pattern, .. }
            | EvidenceStep::ExpectInspectionPattern { pattern, .. } => {
                collect_pattern_references(pattern, references);
            }
            EvidenceStep::ExpectRestore { commands, .. } => {
                for command in commands {
                    collect_expression_references(command, references);
                }
            }
            EvidenceStep::Start { .. }
            | EvidenceStep::ExpectSnapshot { .. }
            | EvidenceStep::Pin { .. } => {}
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPresentationIdentity {
    pub id: String,
    pub presentation_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPortBinding {
    pub port: String,
    pub adapter: String,
    pub required_contract_hash: String,
    pub admitted_contract_instance_hash: String,
}

/// Path-independent content selected by one deployment.
///
/// `configuration` is `null` for a resource without protocol configuration.
/// Physical filenames and module locators deliberately have no representation
/// in this contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeploymentContentIdentity {
    pub protocol: String,
    pub configuration: serde_json::Value,
    pub content_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeploymentIdentityMaterial {
    pub machine: String,
    pub machine_program_id: String,
    pub presentation: Option<DeploymentPresentationIdentity>,
    pub entry: String,
    pub lifetime: String,
    /// Canonical tagged Uhura configuration JSON.
    pub configuration: serde_json::Value,
    pub port_bindings: Vec<DeploymentPortBinding>,
    pub stylesheet: Option<DeploymentContentIdentity>,
    pub provider: Option<DeploymentContentIdentity>,
}

fn deployment_identity_fields(
    material: &DeploymentIdentityMaterial,
) -> Result<Vec<Vec<u8>>, String> {
    validate_deployment_text("machine PublicId", &material.machine)?;
    validate_deployment_text("entry name", &material.entry)?;
    validate_deployment_text("lifetime", &material.lifetime)?;
    let machine_program_id = super::codec::decode_hex_32(&material.machine_program_id)?.to_vec();
    let (presentation, presentation_id) = match &material.presentation {
        Some(presentation) => {
            validate_deployment_text("presentation PublicId", &presentation.id)?;
            (
                super::codec::frame("some", &[presentation.id.as_bytes().to_vec()]),
                super::codec::frame(
                    "some",
                    &[super::codec::decode_hex_32(&presentation.presentation_id)?.to_vec()],
                ),
            )
        }
        None => (
            super::codec::frame("none", &[]),
            super::codec::frame("none", &[]),
        ),
    };
    let configuration = uhura_base::try_to_canonical_json(&material.configuration)
        .map_err(|error| format!("deployment configuration is not canonical Uhura data: {error}"))?
        .into_bytes();

    let mut bindings = material.port_bindings.clone();
    bindings.sort_by(|left, right| left.port.as_bytes().cmp(right.port.as_bytes()));
    for pair in bindings.windows(2) {
        if pair[0].port == pair[1].port {
            return Err(format!(
                "duplicate deployment adapter port `{}`",
                pair[0].port
            ));
        }
    }
    let bindings = bindings
        .iter()
        .map(|binding| {
            validate_deployment_text("port locator", &binding.port)?;
            validate_deployment_text("adapter identity", &binding.adapter)?;
            Ok(super::codec::frame(
                "port-binding",
                &[
                    binding.port.as_bytes().to_vec(),
                    binding.adapter.as_bytes().to_vec(),
                    super::codec::decode_hex_32(&binding.required_contract_hash)?.to_vec(),
                    super::codec::decode_hex_32(&binding.admitted_contract_instance_hash)?.to_vec(),
                ],
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(vec![
        material.machine.as_bytes().to_vec(),
        machine_program_id,
        presentation,
        presentation_id,
        material.entry.as_bytes().to_vec(),
        material.lifetime.as_bytes().to_vec(),
        configuration,
        super::codec::frame("port-binding-list", &bindings),
        deployment_content_field("stylesheet", material.stylesheet.as_ref())?,
        deployment_content_field("provider", material.provider.as_ref())?,
    ])
}

fn deployment_content_field(
    kind: &str,
    content: Option<&DeploymentContentIdentity>,
) -> Result<Vec<u8>, String> {
    let Some(content) = content else {
        return Ok(super::codec::frame("none", &[]));
    };
    validate_deployment_text(&format!("{kind} protocol"), &content.protocol)?;
    let configuration = uhura_base::try_to_canonical_json(&content.configuration)
        .map_err(|error| format!("{kind} configuration is not canonical Uhura data: {error}"))?
        .into_bytes();
    Ok(super::codec::frame(
        "some",
        &[super::codec::frame(
            kind,
            &[
                content.protocol.as_bytes().to_vec(),
                configuration,
                super::codec::decode_hex_32(&content.content_hash)?.to_vec(),
            ],
        )],
    ))
}

fn validate_deployment_text(kind: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{kind} must be nonempty UTF-8 text"));
    }
    Ok(())
}

/// Canonical, path-independent material for the current `DeploymentId`.
pub fn deployment_identity_bytes(material: &DeploymentIdentityMaterial) -> Result<Vec<u8>, String> {
    Ok(super::codec::frame(
        DEPLOYMENT_ID_PROTOCOL,
        &deployment_identity_fields(material)?,
    ))
}

pub fn deployment_hash(material: &DeploymentIdentityMaterial) -> Result<String, String> {
    Ok(super::codec::hex(&super::codec::hash(
        DEPLOYMENT_ID_PROTOCOL,
        &deployment_identity_fields(material)?,
    )))
}

fn semantic_json(value: &impl Serialize) -> serde_json::Value {
    let mut value = serde_json::to_value(value).expect("machine-kernel semantic IR serializes");
    strip_physical_sources(&mut value);
    value
}

fn semantic_function_json_v04(function: &Function) -> serde_json::Value {
    let mut function = function.clone();
    function.source.id.clear();
    semantic_json(&function)
}

fn semantic_machine_json_v04(machine: &Machine) -> serde_json::Value {
    let mut machine = machine.clone();
    machine.source.id.clear();
    for (_, source) in &mut machine.requires {
        source.id.clear();
    }
    for port in &mut machine.ports {
        port.source.id.clear();
    }
    for command in &mut machine.local_commands {
        command.source.id.clear();
    }
    for outcome in &mut machine.outcomes {
        outcome.source.id.clear();
    }
    for field in &mut machine.state {
        field.source.id.clear();
    }
    for function in machine.functions.values_mut() {
        function.source.id.clear();
    }
    for (_, _, _, source) in &mut machine.derives {
        source.id.clear();
    }
    // Invariant sources are runtime-observable SiteIds and remain semantic.
    for field in &mut machine.observation {
        field.source.id.clear();
    }
    for transition in machine.transitions.values_mut() {
        transition.source.id.clear();
        erase_non_site_statement_sources(&mut transition.body);
    }
    for handler in machine.handlers.values_mut() {
        handler.source.id.clear();
        erase_non_site_statement_sources(&mut handler.body);
    }
    erase_non_site_statement_sources(&mut machine.before_commit);
    semantic_json(&machine)
}

fn erase_non_site_statement_sources(statements: &mut [Statement]) {
    for statement in statements {
        match statement {
            Statement::Let { source, .. }
            | Statement::Set { source, .. }
            | Statement::Emit { source, .. }
            | Statement::Finish { source, .. }
            | Statement::Delegate { source, .. } => source.id.clear(),
            Statement::If {
                source,
                then_body,
                else_body,
                ..
            } => {
                source.id.clear();
                erase_non_site_statement_sources(then_body);
                erase_non_site_statement_sources(else_body);
            }
            Statement::Match { source, arms, .. } => {
                source.id.clear();
                for arm in arms {
                    erase_non_site_statement_sources(&mut arm.body);
                }
            }
            Statement::While { source, body, .. } => {
                source.id.clear();
                erase_non_site_statement_sources(body);
            }
            Statement::Unreachable { .. } => {
                // This SiteId is observable through ProgramFault.
            }
        }
    }
}

fn assign_site_identity(
    source: &mut SourceRef,
    fallback: SiteIdentityFrame,
    identities: &mut BTreeMap<String, SiteIdentityFrame>,
) {
    let fallback_id = fallback.site_id();
    if !is_lower_sha256(&source.id) {
        source.id.clone_from(&fallback_id);
        identities.entry(fallback_id).or_insert(fallback);
    } else if source.id == fallback_id {
        identities.entry(fallback_id).or_insert(fallback);
    }
}

fn collect_statement_site_ids<'a>(
    statements: &'a [Statement],
    machine_id: &'a str,
    output: &mut BTreeMap<&'a str, BTreeSet<&'a str>>,
) {
    for statement in statements {
        match statement {
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                collect_statement_site_ids(then_body, machine_id, output);
                collect_statement_site_ids(else_body, machine_id, output);
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_statement_site_ids(&arm.body, machine_id, output);
                }
            }
            Statement::While { body, .. } => {
                collect_statement_site_ids(body, machine_id, output);
            }
            Statement::Unreachable { source } => {
                output.entry(&source.id).or_default().insert(machine_id);
            }
            Statement::Let { .. }
            | Statement::Set { .. }
            | Statement::Emit { .. }
            | Statement::Finish { .. }
            | Statement::Delegate { .. } => {}
        }
    }
}

fn assign_statement_site_ids(
    owner: &str,
    parent: &str,
    statements: &mut [Statement],
    identities: &mut BTreeMap<String, SiteIdentityFrame>,
) {
    for (index, statement) in statements.iter_mut().enumerate() {
        let path = format!("{parent}/statement/{index}");
        match statement {
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                assign_statement_site_ids(owner, &format!("{path}/then"), then_body, identities);
                assign_statement_site_ids(owner, &format!("{path}/else"), else_body, identities);
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    let branch = pattern_semantic_path(&arm.pattern);
                    assign_statement_site_ids(
                        owner,
                        &format!("{path}/branch/{branch}"),
                        &mut arm.body,
                        identities,
                    );
                }
            }
            Statement::While { body, .. } => {
                assign_statement_site_ids(owner, &format!("{path}/body"), body, identities);
            }
            Statement::Unreachable { source } => {
                assign_site_identity(
                    source,
                    SiteIdentityFrame::new(owner, "root", "unreachable", path),
                    identities,
                );
            }
            Statement::Let { .. }
            | Statement::Set { .. }
            | Statement::Emit { .. }
            | Statement::Finish { .. }
            | Statement::Delegate { .. } => {}
        }
    }
}

fn assign_ui_node_ids(public_owner: &str, prefix: &str, nodes: &mut [UiNode]) {
    for (ordinal, node) in nodes.iter_mut().enumerate() {
        let path = format!("{prefix}/{ordinal}");
        match node {
            UiNode::Text { source, .. } => {
                source.id = v04_node_id(public_owner, "root", "ui_text", &format!("{path}/text"));
            }
            UiNode::Interpolation { source, .. } => {
                source.id = v04_node_id(
                    public_owner,
                    "root",
                    "ui_interpolation",
                    &format!("{path}/interpolation"),
                );
            }
            UiNode::Element {
                name,
                attributes,
                children,
                source,
            } => {
                source.id = v04_node_id(
                    public_owner,
                    "root",
                    "ui_element",
                    &format!("{path}/element/{name}"),
                );
                let mut duplicates = BTreeMap::<(&str, &str), usize>::new();
                for attribute in attributes {
                    let (kind, name) = match &attribute.value {
                        UiAttributeValue::Event { event, .. } => {
                            ("ui_event_binding", event.as_str())
                        }
                        UiAttributeValue::Text { .. } | UiAttributeValue::Expression { .. } => {
                            ("ui_attribute", attribute.name.as_str())
                        }
                    };
                    let category = if kind == "ui_event_binding" {
                        "event"
                    } else {
                        "attribute"
                    };
                    let duplicate = duplicates.entry((category, name)).or_default();
                    attribute.source.id = v04_node_id(
                        public_owner,
                        "root",
                        kind,
                        &format!("{path}/{category}/{name}/{duplicate}"),
                    );
                    *duplicate += 1;
                }
                assign_ui_node_ids(public_owner, &format!("{path}/children"), children);
            }
            UiNode::If {
                children, source, ..
            } => {
                source.id = v04_node_id(public_owner, "root", "ui_if", &format!("{path}/if"));
                assign_ui_node_ids(public_owner, &format!("{path}/then"), children);
            }
            UiNode::Match { cases, source, .. } => {
                source.id = v04_node_id(public_owner, "root", "ui_match", &format!("{path}/match"));
                for (case, value) in cases.iter_mut().enumerate() {
                    value.source.id = v04_node_id(
                        public_owner,
                        "root",
                        "ui_case",
                        &format!("{path}/case/{case}"),
                    );
                    assign_ui_node_ids(
                        public_owner,
                        &format!("{path}/case/{case}/children"),
                        &mut value.children,
                    );
                }
            }
            UiNode::Each {
                children, source, ..
            } => {
                source.id = v04_node_id(public_owner, "root", "ui_each", &format!("{path}/each"));
                assign_ui_node_ids(public_owner, &format!("{path}/children"), children);
            }
        }
    }
}

fn pattern_semantic_path(pattern: &Pattern) -> String {
    fn material(pattern: &Pattern) -> serde_json::Value {
        match pattern {
            Pattern::Ignore => serde_json::json!({ "kind": "ignore" }),
            Pattern::Bind { .. } => serde_json::json!({ "kind": "bind" }),
            Pattern::Literal { value } => {
                serde_json::json!({ "kind": "literal", "value": value })
            }
            Pattern::Constructor {
                type_id,
                constructor,
                fields,
            } => serde_json::json!({
                "kind": "constructor",
                "type": type_id,
                "constructor": constructor,
                "fields": fields.iter().map(material).collect::<Vec<_>>(),
            }),
            Pattern::Tuple { values } => serde_json::json!({
                "kind": "tuple",
                "values": values.iter().map(material).collect::<Vec<_>>(),
            }),
            Pattern::Record { fields, rest } => {
                let mut fields = fields
                    .iter()
                    .map(|(name, pattern)| (name, material(pattern)))
                    .collect::<Vec<_>>();
                fields.sort_by(|(left, left_pattern), (right, right_pattern)| {
                    left.as_bytes().cmp(right.as_bytes()).then_with(|| {
                        uhura_base::to_canonical_json(left_pattern)
                            .as_bytes()
                            .cmp(uhura_base::to_canonical_json(right_pattern).as_bytes())
                    })
                });
                serde_json::json!({
                    "kind": "record",
                    "fields": fields,
                    "rest": rest,
                })
            }
            Pattern::Alternative { patterns } => {
                let mut patterns = patterns.iter().map(material).collect::<Vec<_>>();
                patterns.sort_by_cached_key(uhura_base::to_canonical_json);
                serde_json::json!({
                    "kind": "alternative",
                    "patterns": patterns,
                })
            }
        }
    }

    let canonical = uhura_base::to_canonical_json(&material(pattern));
    uhura_base::sha256_hex(canonical.as_bytes())
}

fn v04_node_id(owner: &str, composition: &str, kind: &str, path: &str) -> String {
    crate::semantic_node_id(owner, composition, kind, path)
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn strip_physical_sources(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                strip_physical_sources(value);
            }
        }
        serde_json::Value::Object(object) => {
            let is_source_ref = object.len() == 4
                && object.contains_key("id")
                && object.contains_key("path")
                && object.contains_key("start")
                && object.contains_key("end");
            if is_source_ref {
                object.retain(|field, _| field == "id");
                return;
            }
            for value in object.values_mut() {
                strip_physical_sources(value);
            }
        }
        _ => {}
    }
}

#[derive(Default)]
struct SemanticReferences {
    types: BTreeSet<String>,
    constants: BTreeSet<String>,
    functions: BTreeSet<String>,
    routes: BTreeSet<String>,
}

fn collect_machine_references(machine: &Machine, references: &mut SemanticReferences) {
    collect_type_references(&machine.config, references);
    for (requirement, _) in &machine.requires {
        collect_expression_references(requirement, references);
    }
    for port in &machine.ports {
        for ty in &port.type_arguments {
            collect_type_references(ty, references);
        }
        if let Some(configuration) = &port.configuration {
            collect_expression_references(configuration, references);
        }
        collect_constructor_references(&port.receive, references);
        collect_constructor_references(&port.send, references);
    }
    collect_type_definition_references(&machine.local_input, references);
    for command in &machine.local_commands {
        collect_constructor_references(std::slice::from_ref(&command.constructor), references);
    }
    for outcome in &machine.outcomes {
        collect_constructor_references(std::slice::from_ref(&outcome.constructor), references);
    }
    for field in &machine.state {
        collect_type_references(&field.ty, references);
        collect_expression_references(&field.initial, references);
    }
    for function in machine.functions.values() {
        collect_function_references(function, references);
    }
    for (_, ty, expression, _) in &machine.derives {
        collect_type_references(ty, references);
        collect_expression_references(expression, references);
    }
    for (invariant, _) in &machine.invariants {
        collect_expression_references(invariant, references);
    }
    for field in &machine.observation {
        collect_type_references(&field.ty, references);
        collect_expression_references(&field.expression, references);
    }
    for transition in machine.transitions.values() {
        for (_, ty) in &transition.params {
            collect_type_references(ty, references);
        }
        collect_statement_references(&transition.body, references);
    }
    for handler in machine.handlers.values() {
        collect_pattern_references(&handler.pattern, references);
        collect_statement_references(&handler.body, references);
    }
    collect_statement_references(&machine.before_commit, references);
}

fn collect_type_definition_references(definition: &TypeDef, references: &mut SemanticReferences) {
    match definition {
        TypeDef::Key { underlying, .. } => collect_type_references(underlying, references),
        TypeDef::Record { fields, .. } => {
            for (_, ty) in fields {
                collect_type_references(ty, references);
            }
        }
        TypeDef::Sum { constructors, .. } => {
            collect_constructor_references(constructors, references)
        }
    }
}

fn collect_constructor_references(
    constructors: &[ConstructorDef],
    references: &mut SemanticReferences,
) {
    for constructor in constructors {
        for (_, ty) in &constructor.fields {
            collect_type_references(ty, references);
        }
    }
}

fn collect_function_references(function: &Function, references: &mut SemanticReferences) {
    for (_, ty) in &function.params {
        collect_type_references(ty, references);
    }
    collect_type_references(&function.result, references);
    collect_expression_references(&function.body, references);
}

fn collect_type_references(ty: &TypeRef, references: &mut SemanticReferences) {
    match ty {
        TypeRef::Named { id } => collect_named_type_components(id, &mut references.types),
        TypeRef::Option { value }
        | TypeRef::Seq { value }
        | TypeRef::NonEmpty { value }
        | TypeRef::Set { value }
        | TypeRef::FiniteView { value } => collect_type_references(value, references),
        TypeRef::Map { key, value } | TypeRef::Table { key, value } => {
            collect_type_references(key, references);
            collect_type_references(value, references);
        }
        TypeRef::Tuple { values } => {
            for value in values {
                collect_type_references(value, references);
            }
        }
        TypeRef::Record { fields } => {
            for (_, ty) in fields {
                collect_type_references(ty, references);
            }
        }
        TypeRef::Bool
        | TypeRef::Unit
        | TypeRef::Never
        | TypeRef::Int
        | TypeRef::Nat
        | TypeRef::PositiveInt
        | TypeRef::Decimal
        | TypeRef::BoundaryNumber
        | TypeRef::Ratio
        | TypeRef::Text => {}
    }
}

fn nested_finite_view_path(
    segment: impl Into<String>,
    path: Option<Vec<String>>,
) -> Option<Vec<String>> {
    path.map(|mut path| {
        path.insert(0, segment.into());
        path
    })
}

fn finite_view_path(
    ty: &TypeRef,
    definitions: &BTreeMap<String, TypeDef>,
    visited: &mut BTreeSet<String>,
) -> Option<Vec<String>> {
    match ty {
        TypeRef::FiniteView { .. } => Some(vec!["FiniteView".into()]),
        TypeRef::Option { value } => nested_finite_view_path(
            "Option.value",
            finite_view_path(value, definitions, visited),
        ),
        TypeRef::Seq { value } => {
            nested_finite_view_path("Seq.item", finite_view_path(value, definitions, visited))
        }
        TypeRef::NonEmpty { value } => nested_finite_view_path(
            "NonEmpty.item",
            finite_view_path(value, definitions, visited),
        ),
        TypeRef::Set { value } => {
            nested_finite_view_path("Set.item", finite_view_path(value, definitions, visited))
        }
        TypeRef::Map { key, value } => nested_finite_view_path(
            "Map.key",
            finite_view_path(key, definitions, visited),
        )
        .or_else(|| {
            nested_finite_view_path("Map.value", finite_view_path(value, definitions, visited))
        }),
        TypeRef::Table { key, value } => {
            nested_finite_view_path("Table.key", finite_view_path(key, definitions, visited))
                .or_else(|| {
                    nested_finite_view_path(
                        "Table.value",
                        finite_view_path(value, definitions, visited),
                    )
                })
        }
        TypeRef::Tuple { values } => values.iter().enumerate().find_map(|(index, value)| {
            nested_finite_view_path(
                format!("Tuple[{}]", index + 1),
                finite_view_path(value, definitions, visited),
            )
        }),
        TypeRef::Record { fields } => fields.iter().find_map(|(name, value)| {
            nested_finite_view_path(
                format!("Record.{name}"),
                finite_view_path(value, definitions, visited),
            )
        }),
        TypeRef::Named { id } => {
            if id.contains("FiniteView<") {
                return Some(vec![id.clone()]);
            }
            if !visited.insert(id.clone()) {
                return None;
            }
            let name = id.rsplit("::").next().unwrap_or(id);
            match definitions.get(id) {
                Some(TypeDef::Key { underlying, .. }) => nested_finite_view_path(
                    format!("{name}.over"),
                    finite_view_path(underlying, definitions, visited),
                ),
                Some(TypeDef::Record { fields, .. }) => fields.iter().find_map(|(field, value)| {
                    nested_finite_view_path(
                        format!("{name}.{field}"),
                        finite_view_path(value, definitions, visited),
                    )
                }),
                Some(TypeDef::Sum { constructors, .. }) => {
                    constructors.iter().find_map(|constructor| {
                        constructor
                            .fields
                            .iter()
                            .enumerate()
                            .find_map(|(index, (field, value))| {
                                let field = field
                                    .as_deref()
                                    .map(ToOwned::to_owned)
                                    .unwrap_or_else(|| format!("#{}", index + 1));
                                nested_finite_view_path(
                                    format!("{name}.{}.{field}", constructor.name),
                                    finite_view_path(value, definitions, visited),
                                )
                            })
                    })
                }
                None => None,
            }
        }
        TypeRef::Bool
        | TypeRef::Unit
        | TypeRef::Never
        | TypeRef::Int
        | TypeRef::Nat
        | TypeRef::PositiveInt
        | TypeRef::Decimal
        | TypeRef::BoundaryNumber
        | TypeRef::Ratio
        | TypeRef::Text => None,
    }
}

fn collect_named_type_components(identity: &str, output: &mut BTreeSet<String>) {
    output.insert(identity.to_string());
    for component in identity.split(|character: char| "<>,(){} ".contains(character)) {
        if component.contains("::") {
            output.insert(component.to_string());
        }
    }
}

fn collect_value_references(value: &Value, references: &mut SemanticReferences) {
    match value {
        Value::Key { type_id, value } => {
            collect_named_type_components(type_id, &mut references.types);
            collect_value_references(value, references);
        }
        Value::Tuple(values)
        | Value::Seq(values)
        | Value::NonEmpty(values)
        | Value::Set(values) => {
            for value in values {
                collect_value_references(value, references);
            }
        }
        Value::Record(fields) => {
            for (_, value) in fields {
                collect_value_references(value, references);
            }
        }
        Value::Variant {
            type_id, fields, ..
        } => {
            collect_named_type_components(type_id, &mut references.types);
            for (_, value) in fields {
                collect_value_references(value, references);
            }
        }
        Value::Map(entries) => {
            for (key, value) in entries {
                collect_value_references(key, references);
                collect_value_references(value, references);
            }
        }
        Value::Table { key_type, entries } => {
            collect_named_type_components(key_type, &mut references.types);
            for (_, value) in entries {
                collect_value_references(value, references);
            }
        }
        Value::Unit
        | Value::Bool(_)
        | Value::Integer { .. }
        | Value::Decimal(_)
        | Value::Ratio(_)
        | Value::Boundary(_)
        | Value::Text(_) => {}
    }
}

fn collect_expression_references(expression: &Expr, references: &mut SemanticReferences) {
    match expression {
        Expr::Literal { value } => collect_value_references(value, references),
        Expr::Name { name } => {
            references.constants.insert(name.clone());
        }
        Expr::Constructor {
            type_id, fields, ..
        } => {
            collect_named_type_components(type_id, &mut references.types);
            for (_, value) in fields {
                collect_expression_references(value, references);
            }
        }
        Expr::Key { type_id, value } => {
            collect_named_type_components(type_id, &mut references.types);
            collect_expression_references(value, references);
        }
        Expr::Tuple { values } | Expr::Seq { values } => {
            for value in values {
                collect_expression_references(value, references);
            }
        }
        Expr::Record { fields } => {
            for (_, value) in fields {
                collect_expression_references(value, references);
            }
        }
        Expr::Map {
            entries,
            result_type,
        } => {
            collect_type_references(result_type, references);
            for (key, value) in entries {
                collect_expression_references(key, references);
                collect_expression_references(value, references);
            }
        }
        Expr::Collect { clauses: entries } => {
            for (key, value) in entries {
                collect_expression_references(key, references);
                collect_expression_references(value, references);
            }
        }
        Expr::Table { key_type, entries } => {
            collect_named_type_components(key_type, &mut references.types);
            for (_, value) in entries {
                collect_expression_references(value, references);
            }
        }
        Expr::Unary { value, .. } | Expr::Field { value, .. } => {
            collect_expression_references(value, references)
        }
        Expr::Binary { left, right, .. }
        | Expr::Index {
            value: left,
            key: right,
        } => {
            collect_expression_references(left, references);
            collect_expression_references(right, references);
        }
        Expr::Call {
            function,
            args,
            result_type,
        } => {
            references.functions.insert(function.clone());
            collect_type_references(result_type, references);
            for argument in args {
                collect_expression_references(argument, references);
            }
        }
        Expr::Invoke { function, args } => {
            collect_expression_references(function, references);
            for argument in args {
                collect_expression_references(argument, references);
            }
        }
        Expr::Method {
            value,
            args,
            result_type,
            ..
        } => {
            collect_expression_references(value, references);
            collect_type_references(result_type, references);
            for argument in args {
                collect_expression_references(argument, references);
            }
        }
        Expr::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_expression_references(condition, references);
            collect_expression_references(then_value, references);
            collect_expression_references(else_value, references);
        }
        Expr::Match { value, arms } => {
            collect_expression_references(value, references);
            for arm in arms {
                collect_pattern_references(&arm.pattern, references);
                collect_expression_references(&arm.value, references);
            }
        }
        Expr::Is { value, pattern } => {
            collect_expression_references(value, references);
            collect_pattern_references(pattern, references);
        }
        Expr::Update { value, fields } => {
            collect_expression_references(value, references);
            for (_, value) in fields {
                collect_expression_references(value, references);
            }
        }
        Expr::Let { bindings, value } => {
            for (_, binding) in bindings {
                collect_expression_references(binding, references);
            }
            collect_expression_references(value, references);
        }
        Expr::Lambda { body, .. } => collect_expression_references(body, references),
        Expr::SetComprehension {
            pattern,
            source,
            conditions,
            value,
            result_type,
        } => {
            collect_type_references(result_type, references);
            collect_pattern_references(pattern, references);
            collect_expression_references(source, references);
            for condition in conditions {
                collect_expression_references(condition, references);
            }
            collect_expression_references(value, references);
        }
    }
}

fn collect_statement_references(statements: &[Statement], references: &mut SemanticReferences) {
    for statement in statements {
        match statement {
            Statement::Let { value, .. }
            | Statement::Set { value, .. }
            | Statement::Emit { value, .. }
            | Statement::Finish { outcome: value, .. } => {
                collect_expression_references(value, references)
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                collect_expression_references(condition, references);
                collect_statement_references(then_body, references);
                collect_statement_references(else_body, references);
            }
            Statement::Match { value, arms, .. } => {
                collect_expression_references(value, references);
                for arm in arms {
                    collect_pattern_references(&arm.pattern, references);
                    collect_statement_references(&arm.body, references);
                }
            }
            Statement::While {
                condition,
                decreases,
                body,
                ..
            } => {
                collect_expression_references(condition, references);
                collect_expression_references(decreases, references);
                collect_statement_references(body, references);
            }
            Statement::Delegate { args, .. } => {
                for argument in args {
                    collect_expression_references(argument, references);
                }
            }
            Statement::Unreachable { .. } => {}
        }
    }
}

fn collect_pattern_references(pattern: &Pattern, references: &mut SemanticReferences) {
    match pattern {
        Pattern::Literal { value } => collect_value_references(value, references),
        Pattern::Constructor {
            type_id, fields, ..
        } => {
            collect_named_type_components(type_id, &mut references.types);
            for field in fields {
                collect_pattern_references(field, references);
            }
        }
        Pattern::Tuple { values } | Pattern::Alternative { patterns: values } => {
            for value in values {
                collect_pattern_references(value, references);
            }
        }
        Pattern::Record { fields, .. } => {
            for (_, value) in fields {
                collect_pattern_references(value, references);
            }
        }
        Pattern::Ignore | Pattern::Bind { .. } => {}
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Presentation {
    pub id: String,
    pub machine: String,
    pub binding: String,
    pub nodes: Vec<UiNode>,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UiNode {
    Text {
        value: String,
        source: SourceRef,
    },
    Interpolation {
        value: Expr,
        source: SourceRef,
    },
    Element {
        name: String,
        attributes: Vec<UiAttribute>,
        children: Vec<UiNode>,
        source: SourceRef,
    },
    If {
        condition: Expr,
        children: Vec<UiNode>,
        source: SourceRef,
    },
    Match {
        value: Expr,
        cases: Vec<UiCase>,
        source: SourceRef,
    },
    Each {
        value: Expr,
        pattern: Pattern,
        key: Box<Expr>,
        children: Vec<UiNode>,
        source: SourceRef,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UiAttribute {
    pub name: String,
    pub value: UiAttributeValue,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UiAttributeValue {
    Text { value: String },
    Expression { value: Expr },
    Event { event: String, input: Expr },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UiCase {
    pub pattern: Pattern,
    pub children: Vec<UiNode>,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceSuite {
    pub scenarios: BTreeMap<String, Scenario>,
    pub examples: BTreeMap<String, EvidenceRef>,
    pub checkpoints: BTreeMap<String, EvidenceRef>,
    /// Editor/catalog metadata attached to examples. It does not participate
    /// in machine or evidence behavior hashes.
    #[serde(default)]
    pub example_metadata: BTreeMap<String, EvidenceExampleMetadata>,
    /// Physical declaration provenance for example registrations. This table
    /// is diagnostic/editor metadata and is deliberately excluded from the
    /// semantic evidence-hash projection.
    #[serde(default)]
    pub example_sources: BTreeMap<String, SourceRef>,
    /// Physical declaration provenance for checkpoint registrations. This
    /// remains separate from the semantic reference for stable identities.
    #[serde(default)]
    pub checkpoint_sources: BTreeMap<String, SourceRef>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceExampleMetadata {
    pub presentation: Option<String>,
    pub kind: Option<EvidencePresentationKind>,
    pub is_default: bool,
    pub note: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidencePresentationKind {
    Page,
    Component,
    Surface,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub scenario: String,
    pub pin: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub origin: ScenarioOrigin,
    pub steps: Vec<EvidenceStep>,
    pub source: SourceRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ScenarioOrigin {
    Machine {
        machine: String,
        configuration: Value,
    },
    Snapshot {
        reference: EvidenceRef,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EvidenceStep {
    Bind {
        port: String,
        fixture: Expr,
        source: SourceRef,
    },
    Start {
        source: SourceRef,
    },
    Send {
        input: Expr,
        source: SourceRef,
    },
    Deliver {
        input: Expr,
        source: SourceRef,
    },
    ExpectReaction {
        outcome: Pattern,
        commands: Vec<Expr>,
        source: SourceRef,
    },
    ExpectObservationPattern {
        pattern: Pattern,
        source: SourceRef,
    },
    ExpectInspectionPattern {
        pattern: Pattern,
        source: SourceRef,
    },
    ExpectObservationWhere {
        condition: Expr,
        source: SourceRef,
    },
    ExpectRestore {
        commands: Vec<Expr>,
        source: SourceRef,
    },
    ExpectSnapshot {
        reference: EvidenceRef,
        source: SourceRef,
    },
    Pin {
        name: String,
        source: SourceRef,
    },
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for MachineProgram {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnaryOp {
    Not,
    Negate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Expr {
    Literal {
        value: Value,
    },
    Name {
        name: String,
    },
    Constructor {
        type_id: String,
        constructor: String,
        fields: Vec<(Option<String>, Expr)>,
    },
    Key {
        type_id: String,
        value: Box<Expr>,
    },
    Tuple {
        values: Vec<Expr>,
    },
    Record {
        fields: Vec<(String, Expr)>,
    },
    Seq {
        values: Vec<Expr>,
    },
    Map {
        entries: Vec<(Expr, Expr)>,
        /// Exact checked `Map<K,V>` result identity. Runtime ordering and
        /// duplicate detection must never infer `K` from a member value.
        result_type: TypeRef,
    },
    Table {
        key_type: String,
        entries: Vec<(String, Expr)>,
    },
    Unary {
        op: UnaryOp,
        value: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        function: String,
        args: Vec<Expr>,
        /// The result type proven by the checker.
        ///
        /// Runtime primitives must use this identity instead of inventing
        /// placeholder generic names such as `Option<Seq<_>>`. In particular,
        /// an empty collection carries no values from which a sound nominal
        /// identity could be inferred.
        result_type: TypeRef,
    },
    Invoke {
        function: Box<Expr>,
        args: Vec<Expr>,
    },
    Field {
        value: Box<Expr>,
        field: String,
    },
    Index {
        value: Box<Expr>,
        key: Box<Expr>,
    },
    Method {
        value: Box<Expr>,
        method: String,
        args: Vec<Expr>,
        /// The exact result type proven by the checker.
        result_type: TypeRef,
    },
    If {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Is {
        value: Box<Expr>,
        pattern: Pattern,
    },
    Update {
        value: Box<Expr>,
        fields: Vec<(String, Expr)>,
    },
    Let {
        bindings: Vec<(String, Expr)>,
        value: Box<Expr>,
    },
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },
    Collect {
        clauses: Vec<(Expr, Expr)>,
    },
    SetComprehension {
        pattern: Pattern,
        source: Box<Expr>,
        conditions: Vec<Expr>,
        value: Box<Expr>,
        /// Exact checked `Set<T>` result identity, including when `T` is an
        /// empty or nested structural collection.
        result_type: TypeRef,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub value: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StatementMatchArm {
    pub pattern: Pattern,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Statement {
    Let {
        name: String,
        value: Expr,
        source: SourceRef,
    },
    Set {
        field: String,
        value: Expr,
        source: SourceRef,
    },
    Emit {
        value: Expr,
        source: SourceRef,
    },
    If {
        condition: Expr,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
        source: SourceRef,
    },
    Match {
        value: Expr,
        arms: Vec<StatementMatchArm>,
        source: SourceRef,
    },
    While {
        condition: Expr,
        decreases: Expr,
        body: Vec<Statement>,
        /// Compiler-private total `Option<T>` local selected by a lexical
        /// update return in this loop. Absent for ordinary loops.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        break_local: Option<String>,
        source: SourceRef,
    },
    Finish {
        outcome: Expr,
        source: SourceRef,
    },
    Unreachable {
        source: SourceRef,
    },
    Delegate {
        transition: String,
        args: Vec<Expr>,
        source: SourceRef,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Pattern {
    Ignore,
    Bind {
        name: String,
    },
    Literal {
        value: Value,
    },
    Constructor {
        type_id: String,
        constructor: String,
        fields: Vec<Pattern>,
    },
    Tuple {
        values: Vec<Pattern>,
    },
    Record {
        fields: Vec<(String, Pattern)>,
        rest: bool,
    },
    Alternative {
        patterns: Vec<Pattern>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACHINE: &str = "example.hash@1::Machine";
    const DATA: &str = "example.hash@1::Data";
    const SEED: &str = "example.hash@1::seed";
    const READ: &str = "example.hash@1::read";
    const UI_LABEL: &str = "example.hash@1::ui_label";
    const UI_READ: &str = "example.hash@1::ui_read";
    const EVIDENCE_FLAG: &str = "example.hash@1::evidence_flag";
    const EVIDENCE_CHECK: &str = "example.hash@1::evidence_check";

    fn source(id: &str) -> SourceRef {
        SourceRef {
            id: id.into(),
            path: "/one/worktree/program.uhura".into(),
            start: 10,
            end: 20,
        }
    }

    fn hash_program() -> Program {
        let mut program = Program::new();
        program
            .machine_program
            .modules
            .push("example.hash@1".into());
        program.machine_program.types.insert(
            DATA.into(),
            TypeDef::Record {
                id: DATA.into(),
                fields: vec![("value".into(), TypeRef::Int)],
            },
        );
        program
            .machine_program
            .constants
            .insert(SEED.into(), Value::int(1));
        program
            .machine_program
            .constant_types
            .insert(SEED.into(), TypeRef::Int);
        program.machine_program.functions.insert(
            READ.into(),
            Function {
                id: READ.into(),
                params: Vec::new(),
                result: TypeRef::Int,
                body: Expr::Name { name: SEED.into() },
                source: source("read"),
            },
        );
        let input_id = format!("{MACHINE}.Input");
        let outcome_id = format!("{MACHINE}.Outcome");
        program.machine_program.machines.insert(
            MACHINE.into(),
            Machine {
                id: MACHINE.into(),
                config: TypeRef::Named { id: DATA.into() },
                requires: Vec::new(),
                ports: Vec::new(),
                local_input: TypeDef::Sum {
                    id: input_id.clone(),
                    constructors: vec![ConstructorDef {
                        name: "ping".into(),
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
                state: vec![StateField {
                    name: "count".into(),
                    ty: TypeRef::Int,
                    initial: Expr::Name { name: SEED.into() },
                    source: source("count"),
                }],
                functions: BTreeMap::new(),
                derives: Vec::new(),
                invariants: Vec::new(),
                observation: vec![ObservationField {
                    name: "count".into(),
                    ty: TypeRef::Int,
                    expression: Expr::Call {
                        function: READ.into(),
                        args: Vec::new(),
                        result_type: TypeRef::Int,
                    },
                    source: source("observe-count"),
                }],
                transitions: BTreeMap::new(),
                handlers: BTreeMap::from([(
                    "ping".into(),
                    Handler {
                        input: "ping".into(),
                        pattern: Pattern::Constructor {
                            type_id: input_id,
                            constructor: "ping".into(),
                            fields: Vec::new(),
                        },
                        body: vec![Statement::Finish {
                            outcome: Expr::Constructor {
                                type_id: outcome_id,
                                constructor: "done".into(),
                                fields: Vec::new(),
                            },
                            source: source("finish"),
                        }],
                        source: source("ping"),
                    },
                )]),
                before_commit: Vec::new(),
                source: source("machine"),
            },
        );
        program.freeze_program_hashes();
        program
    }

    fn profile_program() -> Program {
        let mut program = hash_program();
        program
            .machine_program
            .constants
            .insert(UI_LABEL.into(), Value::Text("profile".into()));
        program
            .machine_program
            .constant_types
            .insert(UI_LABEL.into(), TypeRef::Text);
        program.machine_program.functions.insert(
            UI_READ.into(),
            Function {
                id: UI_READ.into(),
                params: Vec::new(),
                result: TypeRef::Text,
                body: Expr::Name {
                    name: UI_LABEL.into(),
                },
                source: source("ui-read"),
            },
        );
        program
            .machine_program
            .constants
            .insert(EVIDENCE_FLAG.into(), Value::Bool(true));
        program
            .machine_program
            .constant_types
            .insert(EVIDENCE_FLAG.into(), TypeRef::Bool);
        program.machine_program.functions.insert(
            EVIDENCE_CHECK.into(),
            Function {
                id: EVIDENCE_CHECK.into(),
                params: Vec::new(),
                result: TypeRef::Bool,
                body: Expr::Name {
                    name: EVIDENCE_FLAG.into(),
                },
                source: source("evidence-check"),
            },
        );
        let TypeDef::Sum { constructors, .. } = &mut program
            .machine_program
            .machines
            .get_mut(MACHINE)
            .expect("profile machine")
            .local_input
        else {
            unreachable!()
        };
        constructors[0]
            .fields
            .push((Some("payload".into()), TypeRef::Named { id: DATA.into() }));
        program.presentations.insert(
            "example.hash@1::Web".into(),
            Presentation {
                id: "example.hash@1::Web".into(),
                machine: MACHINE.into(),
                binding: "model".into(),
                nodes: vec![UiNode::Interpolation {
                    value: Expr::Call {
                        function: UI_READ.into(),
                        args: Vec::new(),
                        result_type: TypeRef::Text,
                    },
                    source: source("ui-text"),
                }],
                source: source("ui"),
            },
        );
        program.evidence.scenarios.insert(
            "example.hash@1::proof".into(),
            Scenario {
                id: "example.hash@1::proof".into(),
                origin: ScenarioOrigin::Machine {
                    machine: MACHINE.into(),
                    configuration: Value::Unit,
                },
                steps: vec![
                    EvidenceStep::ExpectObservationWhere {
                        condition: Expr::Call {
                            function: EVIDENCE_CHECK.into(),
                            args: Vec::new(),
                            result_type: TypeRef::Bool,
                        },
                        source: source("evidence-where"),
                    },
                    EvidenceStep::Pin {
                        name: "ready".into(),
                        source: source("pin"),
                    },
                ],
                source: source("proof"),
            },
        );
        program.evidence.examples.insert(
            "example.hash@1::example".into(),
            EvidenceRef {
                scenario: "example.hash@1::proof".into(),
                pin: "ready".into(),
            },
        );
        program.evidence.checkpoints.insert(
            "example.hash@1::checkpoint".into(),
            EvidenceRef {
                scenario: "example.hash@1::proof".into(),
                pin: "ready".into(),
            },
        );
        program.freeze_program_hashes();
        program
    }

    fn machine_hash(program: &Program) -> &str {
        program.machine_program.program_hashes.get(MACHINE).unwrap()
    }

    fn finite_view() -> TypeRef {
        TypeRef::FiniteView {
            value: Box::new(TypeRef::Int),
        }
    }

    fn assert_v04_finite_view_boundary(program: &Program, boundary: &str, nested_path: &str) {
        let error = program.validate_protocol().unwrap_err();
        assert!(
            error.contains(boundary) && error.contains(nested_path),
            "expected `{boundary}` through `{nested_path}`, got: {error}",
        );
    }

    fn hash_program_v04() -> Program {
        hash_program()
    }

    fn profile_program_v04() -> Program {
        profile_program()
    }

    #[test]
    fn v04_machine_identity_excludes_logical_module_layout() {
        let base = hash_program_v04();
        base.validate_protocol().unwrap();

        let mut moved = base.clone();
        moved.machine_program.modules = vec![
            "renamed::machine".into(),
            "split::types".into(),
            "split::functions".into(),
        ];
        moved.freeze_program_hashes();

        assert_eq!(machine_hash(&moved), machine_hash(&base));
    }

    #[test]
    fn current_identity_protocol_is_mandatory() {
        let mut program = hash_program_v04();
        program.machine_program.identity_protocol = "removed-identity-protocol".into();
        assert!(
            program
                .validate_protocol()
                .unwrap_err()
                .contains("expected identity protocol")
        );
    }

    #[test]
    fn retired_language_artifacts_are_rejected() {
        let mut program = hash_program_v04();
        program.machine_program.language = "uhura 0.3".into();
        assert_eq!(
            program.validate_protocol().unwrap_err(),
            "expected Uhura language `uhura 0.4`, got `uhura 0.3`"
        );
        assert!(
            program.try_freeze_program_hashes().is_err(),
            "retired language artifacts must not enter the current identity path"
        );
    }

    #[test]
    fn v04_protocol_rejects_finite_views_at_every_declared_value_boundary() {
        let mut key = hash_program_v04();
        key.machine_program.types.insert(
            "example.hash@1::InvalidKey".into(),
            TypeDef::Key {
                id: "example.hash@1::InvalidKey".into(),
                underlying: finite_view(),
            },
        );
        assert_v04_finite_view_boundary(&key, "key `example.hash@1::InvalidKey`", "FiniteView");

        let mut constant = hash_program_v04();
        constant
            .machine_program
            .constants
            .insert("example.hash@1::cached".into(), Value::Seq(Vec::new()));
        constant
            .machine_program
            .constant_types
            .insert("example.hash@1::cached".into(), finite_view());
        assert_v04_finite_view_boundary(
            &constant,
            "constant `example.hash@1::cached`",
            "FiniteView",
        );

        let mut configuration = hash_program_v04();
        configuration
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .config = TypeRef::Option {
            value: Box::new(finite_view()),
        };
        assert_v04_finite_view_boundary(
            &configuration,
            "machine `example.hash@1::Machine` configuration",
            "Option.value -> FiniteView",
        );

        let mut input = hash_program_v04();
        let TypeDef::Sum { constructors, .. } = &mut input
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .local_input
        else {
            unreachable!()
        };
        constructors[0]
            .fields
            .push((Some("value".into()), finite_view()));
        assert_v04_finite_view_boundary(
            &input,
            "machine `example.hash@1::Machine` input constructor `ping` field `value`",
            "FiniteView",
        );

        let mut command = hash_program_v04();
        command
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .local_commands
            .push(CommandDef {
                constructor: ConstructorDef {
                    name: "send".into(),
                    fields: vec![(Some("value".into()), finite_view())],
                },
                source: source("send"),
            });
        assert_v04_finite_view_boundary(
            &command,
            "machine `example.hash@1::Machine` command constructor `send` field `value`",
            "FiniteView",
        );

        let mut outcome = hash_program_v04();
        outcome
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .outcomes[0]
            .constructor
            .fields
            .push((Some("value".into()), finite_view()));
        assert_v04_finite_view_boundary(
            &outcome,
            "machine `example.hash@1::Machine` outcome constructor `done` field `value`",
            "FiniteView",
        );

        let wrapper = "example.hash@1::EphemeralBox";
        let mut state = hash_program_v04();
        state.machine_program.types.insert(
            wrapper.into(),
            TypeDef::Record {
                id: wrapper.into(),
                fields: vec![(
                    "values".into(),
                    TypeRef::Option {
                        value: Box::new(finite_view()),
                    },
                )],
            },
        );
        state
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .state[0]
            .ty = TypeRef::Named { id: wrapper.into() };
        assert_v04_finite_view_boundary(
            &state,
            "machine `example.hash@1::Machine` state field `count`",
            "EphemeralBox.values -> Option.value -> FiniteView",
        );

        let mut observation = hash_program_v04();
        observation
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .observation[0]
            .ty = finite_view();
        assert_v04_finite_view_boundary(
            &observation,
            "machine `example.hash@1::Machine` observation field `count`",
            "FiniteView",
        );

        let forged_port = |type_arguments, receive, send| PortDef {
            name: "forged".into(),
            contract: "forged@1::Contract".into(),
            contract_instance: None,
            type_arguments,
            configuration: None,
            receive,
            send,
            contract_hash: String::new(),
            source: source("forged-port"),
        };
        let mut port_argument = hash_program_v04();
        port_argument
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports = vec![forged_port(vec![finite_view()], Vec::new(), Vec::new())];
        assert_v04_finite_view_boundary(
            &port_argument,
            "port `forged` contract type argument #1",
            "FiniteView",
        );

        let mut port_configuration = hash_program_v04();
        let configuration_wrapper = "example.hash@1::PortConfigurationProbe";
        port_configuration.machine_program.types.insert(
            configuration_wrapper.into(),
            TypeDef::Record {
                id: configuration_wrapper.into(),
                fields: vec![
                    ("safe".into(), TypeRef::Int),
                    ("views".into(), finite_view()),
                ],
            },
        );
        let configuration_value = || Expr::Call {
            function: "example.hash@1::forged_configuration".into(),
            args: Vec::new(),
            result_type: TypeRef::Named {
                id: configuration_wrapper.into(),
            },
        };
        let safe_field = Expr::Field {
            value: Box::new(configuration_value()),
            field: "safe".into(),
        };
        assert!(
            port_configuration
                .machine_program
                .v04_expression_finite_view_path(&safe_field)
                .is_none(),
            "selecting a storable field must not reject an ephemeral sibling",
        );
        let mut configured = forged_port(Vec::new(), Vec::new(), Vec::new());
        configured.configuration = Some(Expr::Field {
            value: Box::new(configuration_value()),
            field: "views".into(),
        });
        port_configuration
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports = vec![configured];
        assert_v04_finite_view_boundary(
            &port_configuration,
            "port `forged` configuration",
            "FiniteView",
        );

        let mut port_receive = hash_program_v04();
        port_receive
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports = vec![forged_port(
            Vec::new(),
            vec![ConstructorDef {
                name: "received".into(),
                fields: vec![(Some("value".into()), finite_view())],
            }],
            Vec::new(),
        )];
        assert_v04_finite_view_boundary(
            &port_receive,
            "port `forged` receive constructor `received` field `value`",
            "FiniteView",
        );

        let mut port_send = hash_program_v04();
        port_send
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports = vec![forged_port(
            Vec::new(),
            Vec::new(),
            vec![ConstructorDef {
                name: "sent".into(),
                fields: vec![(Some("value".into()), finite_view())],
            }],
        )];
        assert_v04_finite_view_boundary(
            &port_send,
            "port `forged` send constructor `sent` field `value`",
            "FiniteView",
        );
    }

    #[test]
    fn v04_protocol_keeps_finite_views_inside_ephemeral_evaluator_material() {
        let wrapper = "example.hash@1::EphemeralBox";
        let function = "example.hash@1::ephemeral_identity";
        let mut program = hash_program_v04();
        program.machine_program.types.insert(
            wrapper.into(),
            TypeDef::Record {
                id: wrapper.into(),
                fields: vec![("values".into(), finite_view())],
            },
        );
        program.machine_program.functions.insert(
            function.into(),
            Function {
                id: function.into(),
                params: vec![("values".into(), finite_view())],
                result: finite_view(),
                body: Expr::Name {
                    name: "values".into(),
                },
                source: source("ephemeral-function"),
            },
        );
        let machine = program.machine_program.machines.get_mut(MACHINE).unwrap();
        machine.derives.push((
            "values".into(),
            finite_view(),
            Expr::Method {
                value: Box::new(Expr::Map {
                    entries: Vec::new(),
                    result_type: TypeRef::Map {
                        key: Box::new(TypeRef::Text),
                        value: Box::new(TypeRef::Int),
                    },
                }),
                method: "values".into(),
                args: Vec::new(),
                result_type: finite_view(),
            },
            source("ephemeral-computed"),
        ));
        machine.handlers.get_mut("ping").unwrap().body.insert(
            0,
            Statement::Let {
                name: "ephemeral".into(),
                value: Expr::Call {
                    function: function.into(),
                    args: vec![Expr::Method {
                        value: Box::new(Expr::Map {
                            entries: Vec::new(),
                            result_type: TypeRef::Map {
                                key: Box::new(TypeRef::Text),
                                value: Box::new(TypeRef::Int),
                            },
                        }),
                        method: "values".into(),
                        args: Vec::new(),
                        result_type: finite_view(),
                    }],
                    result_type: finite_view(),
                },
                source: source("ephemeral-local"),
            },
        );

        program.freeze_program_hashes();
        program
            .validate_protocol()
            .expect("ephemeral evaluator material remains legal");
    }

    #[test]
    fn public_ir_rejects_a_forged_recursive_finite_view_boundary_before_admission() {
        let wrapper = "example.hash@1::ForgedStoredView";
        let mut program = hash_program_v04();
        program.machine_program.types.insert(
            wrapper.into(),
            TypeDef::Record {
                id: wrapper.into(),
                fields: vec![(
                    "nested".into(),
                    TypeRef::Seq {
                        value: Box::new(finite_view()),
                    },
                )],
            },
        );
        program
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .state[0]
            .ty = TypeRef::Named { id: wrapper.into() };
        program.freeze_program_hashes();

        let admission_error = program
            .machine_program
            .admit(MACHINE, Value::Unit, "forged-finite-view")
            .unwrap_err();
        assert!(
            admission_error.message.contains("state field `count`")
                && admission_error
                    .message
                    .contains("ForgedStoredView.nested -> Seq.item -> FiniteView"),
            "admission must reject forged checked IR before evaluating values: {admission_error}",
        );

        let error = Program::from_json(&program.to_canonical_string()).unwrap_err();
        assert!(
            error.contains("state field `count`")
                && error.contains("ForgedStoredView.nested -> Seq.item -> FiniteView"),
            "forged public IR must fail at protocol validation: {error}",
        );
    }

    #[test]
    fn v04_composed_part_public_ids_are_runtime_inert_machine_identity_material() {
        let base = hash_program_v04();
        let mut composed = base.clone();
        composed.machine_program.composed_part_declarations.insert(
            MACHINE.into(),
            BTreeSet::from(["vendor.parts@1::Counter".into()]),
        );
        composed.freeze_program_hashes();
        assert_ne!(machine_hash(&composed), machine_hash(&base));

        let mut other_provider = base;
        other_provider
            .machine_program
            .composed_part_declarations
            .insert(
                MACHINE.into(),
                BTreeSet::from(["vendor.other@1::Counter".into()]),
            );
        other_provider.freeze_program_hashes();
        assert_ne!(machine_hash(&other_provider), machine_hash(&composed));
    }

    #[test]
    fn composed_part_identity_rejects_unknown_machines() {
        let mut unknown_machine = hash_program_v04();
        unknown_machine
            .machine_program
            .composed_part_declarations
            .insert(
                "example.hash@1::Missing".into(),
                BTreeSet::from(["vendor.parts@1::Counter".into()]),
            );
        assert!(
            unknown_machine
                .validate_protocol()
                .unwrap_err()
                .contains("unknown machine")
        );
    }

    #[test]
    fn v04_hash_retains_only_stable_runtime_site_ids() {
        let mut base = hash_program_v04();
        let machine = base.machine_program.machines.get_mut(MACHINE).unwrap();
        machine.invariants.push((
            Expr::Literal {
                value: Value::Bool(true),
            },
            source("authored-invariant"),
        ));
        machine.handlers.get_mut("ping").unwrap().body = vec![
            Statement::Let {
                name: "ignored".into(),
                value: Expr::Literal {
                    value: Value::int(1),
                },
                source: source("nonsemantic-let"),
            },
            Statement::Unreachable {
                source: source("authored-unreachable"),
            },
        ];
        base.freeze_program_hashes();

        let invariant_id = &base.machine_program.machines[MACHINE].invariants[0].1.id;
        let Statement::Unreachable {
            source: unreachable,
        } = &base.machine_program.machines[MACHINE].handlers["ping"].body[1]
        else {
            unreachable!()
        };
        assert_eq!(invariant_id.len(), 64);
        assert_eq!(unreachable.id.len(), 64);
        assert!(invariant_id.bytes().all(|value| value.is_ascii_hexdigit()));
        assert!(
            unreachable
                .id
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        );

        let mut moved = base.clone();
        let machine = moved.machine_program.machines.get_mut(MACHINE).unwrap();
        machine.invariants[0].1.path = "moved.uhura".into();
        machine.invariants[0].1.start = 9_000;
        machine.invariants[0].1.end = 9_100;
        let Statement::Let { source, .. } = &mut machine.handlers.get_mut("ping").unwrap().body[0]
        else {
            unreachable!()
        };
        source.id = "different-editor-node".into();
        moved.freeze_program_hashes();

        assert_eq!(machine_hash(&moved), machine_hash(&base));
        assert_eq!(
            moved.machine_program.machines[MACHINE].invariants[0].1.id,
            base.machine_program.machines[MACHINE].invariants[0].1.id
        );
    }

    #[test]
    fn public_v04_ir_rejects_an_unframed_valid_looking_site_id() {
        let mut program = hash_program_v04();
        program
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .handlers
            .get_mut("ping")
            .unwrap()
            .body
            .push(Statement::Unreachable {
                source: source("authored-unreachable"),
            });
        program.freeze_program_hashes();

        let Statement::Unreachable { source } = program
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .handlers
            .get_mut("ping")
            .unwrap()
            .body
            .last_mut()
            .unwrap()
        else {
            unreachable!()
        };
        source.id = "a".repeat(64);
        program.machine_program.site_identities.clear();

        let error = Program::from_json(&program.to_canonical_string())
            .expect_err("an opaque valid-looking digest is not a canonical SiteId");
        assert!(error.contains("has no canonical identity frame"), "{error}");
    }

    #[test]
    fn fallback_site_pattern_material_ignores_binder_spelling_and_set_like_order() {
        let record = Pattern::Record {
            fields: vec![
                (
                    "left".into(),
                    Pattern::Bind {
                        name: "left".into(),
                    },
                ),
                ("right".into(), Pattern::Ignore),
            ],
            rest: false,
        };
        let reordered = Pattern::Record {
            fields: vec![
                ("right".into(), Pattern::Ignore),
                (
                    "left".into(),
                    Pattern::Bind {
                        name: "renamed".into(),
                    },
                ),
            ],
            rest: false,
        };
        assert_eq!(
            pattern_semantic_path(&record),
            pattern_semantic_path(&reordered),
        );

        let alternatives = Pattern::Alternative {
            patterns: vec![
                record,
                Pattern::Literal {
                    value: Value::int(1),
                },
            ],
        };
        let reordered_alternatives = Pattern::Alternative {
            patterns: vec![
                Pattern::Literal {
                    value: Value::int(1),
                },
                reordered,
            ],
        };
        assert_eq!(
            pattern_semantic_path(&alternatives),
            pattern_semantic_path(&reordered_alternatives),
        );
    }

    #[test]
    fn machine_hash_ignores_physical_source_location_presentation_and_evidence() {
        let base = hash_program();
        let mut changed = base.clone();
        let machine = changed.machine_program.machines.get_mut(MACHINE).unwrap();
        machine.source.path = "/another/worktree/formatted.uhura".into();
        machine.source.start = 900;
        machine.source.end = 999;
        changed.presentations.insert(
            "example.hash@1::Web".into(),
            Presentation {
                id: "example.hash@1::Web".into(),
                machine: MACHINE.into(),
                binding: "model".into(),
                nodes: vec![UiNode::Text {
                    value: "presentation-only".into(),
                    source: source("ui"),
                }],
                source: source("presentation"),
            },
        );
        changed.evidence.scenarios.insert(
            "example.hash@1::proof".into(),
            Scenario {
                id: "example.hash@1::proof".into(),
                origin: ScenarioOrigin::Machine {
                    machine: MACHINE.into(),
                    configuration: Value::Unit,
                },
                steps: Vec::new(),
                source: source("proof"),
            },
        );
        changed.freeze_program_hashes();
        assert_eq!(machine_hash(&changed), machine_hash(&base));
    }

    #[test]
    fn machine_hash_uses_canonical_runtime_site_identity() {
        let base = hash_program();

        let mut reformatted = base.clone();
        let Statement::Finish { source, .. } = &mut reformatted
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .handlers
            .get_mut("ping")
            .unwrap()
            .body[0]
        else {
            unreachable!()
        };
        source.path = "/another/worktree/reformatted.uhura".into();
        source.start = 4_000;
        source.end = 4_200;
        reformatted.freeze_program_hashes();
        assert_eq!(
            machine_hash(&reformatted),
            machine_hash(&base),
            "physical file placement and formatting spans are not semantics",
        );

        let mut semantic_id_changed = base.clone();
        let Statement::Finish { source, .. } = &mut semantic_id_changed
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .handlers
            .get_mut("ping")
            .unwrap()
            .body[0]
        else {
            unreachable!()
        };
        source.id = "finish/renamed-semantic-site".into();
        semantic_id_changed.freeze_program_hashes();
        assert_eq!(
            machine_hash(&semantic_id_changed),
            machine_hash(&base),
            "authored source labels are replaced by canonical runtime site identities",
        );
    }

    #[test]
    fn presentation_hash_uses_canonical_render_and_event_identity() {
        let mut base = profile_program();
        let presentation_id = "example.hash@1::Web";
        base.presentations.get_mut(presentation_id).unwrap().nodes = vec![UiNode::Element {
            name: "button".into(),
            attributes: vec![UiAttribute {
                name: "onpress".into(),
                value: UiAttributeValue::Event {
                    event: "press".into(),
                    input: Expr::Constructor {
                        type_id: format!("{MACHINE}.Input"),
                        constructor: "ping".into(),
                        fields: Vec::new(),
                    },
                },
                source: source("ui-onpress"),
            }],
            children: vec![UiNode::Text {
                value: "Ping".into(),
                source: source("ui-button-label"),
            }],
            source: source("ui-button"),
        }];
        base.freeze_program_hashes();

        let mut reformatted = base.clone();
        let UiNode::Element { source, .. } = &mut reformatted
            .presentations
            .get_mut(presentation_id)
            .unwrap()
            .nodes[0]
        else {
            unreachable!()
        };
        source.path = "/another/worktree/reformatted-web.uhura".into();
        source.start = 8_000;
        source.end = 8_500;
        reformatted.freeze_program_hashes();
        assert_eq!(
            reformatted.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );

        let mut semantic_id_changed = base.clone();
        let UiNode::Element { source, .. } = &mut semantic_id_changed
            .presentations
            .get_mut(presentation_id)
            .unwrap()
            .nodes[0]
        else {
            unreachable!()
        };
        source.id = "ui-button/new-render-and-event-key".into();
        semantic_id_changed.freeze_program_hashes();
        assert_eq!(
            semantic_id_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
            "authored source labels are replaced by canonical presentation identities",
        );
        assert_eq!(
            machine_hash(&semantic_id_changed),
            machine_hash(&base),
            "presentation source identity remains outside machine semantics",
        );
    }

    #[test]
    fn v04_ui_identity_uses_exact_public_ids_and_reachable_contracts() {
        let base = profile_program_v04();
        let presentation_id = "example.hash@1::Web";
        let interface = base.machine_ui_interface_hash(MACHINE).unwrap();
        let presentation = base.presentation_hashes[presentation_id].clone();

        assert_eq!(
            interface,
            "dc76f834e009e90ca630a4de1ff0b242a61b8a083341ca92f356be3660b672a8"
        );
        assert_eq!(
            presentation,
            "f0440392e5cb0df5460ae5540c14a1a214502e8d3eb4e67f86beb977fb032e92"
        );

        let mut moved = base.clone();
        moved
            .presentations
            .get_mut(presentation_id)
            .unwrap()
            .source
            .path = "moved/presentation.uhura".into();
        let UiNode::Interpolation { source, .. } =
            &mut moved.presentations.get_mut(presentation_id).unwrap().nodes[0]
        else {
            unreachable!()
        };
        source.path = "moved/presentation.uhura".into();
        source.start = 10_000;
        source.end = 10_100;
        moved.freeze_program_hashes();
        assert_eq!(moved.machine_ui_interface_hash(MACHINE).unwrap(), interface);
        assert_eq!(moved.presentation_hashes[presentation_id], presentation);

        let mut implementation_changed = base.clone();
        implementation_changed
            .machine_program
            .functions
            .get_mut(READ)
            .unwrap()
            .body = Expr::Literal {
            value: Value::int(999),
        };
        implementation_changed.freeze_program_hashes();
        assert_ne!(machine_hash(&implementation_changed), machine_hash(&base));
        assert_eq!(
            implementation_changed
                .machine_ui_interface_hash(MACHINE)
                .unwrap(),
            interface
        );
        assert_eq!(
            implementation_changed.presentation_hashes[presentation_id],
            presentation
        );

        let mut interface_changed = base.clone();
        let TypeDef::Sum { constructors, .. } = &mut interface_changed
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .local_input
        else {
            unreachable!()
        };
        constructors.push(ConstructorDef {
            name: "other".into(),
            fields: Vec::new(),
        });
        interface_changed.freeze_program_hashes();
        assert_ne!(
            interface_changed
                .machine_ui_interface_hash(MACHINE)
                .unwrap(),
            interface
        );
        assert_ne!(
            interface_changed.presentation_hashes[presentation_id],
            presentation
        );

        let mut presentation_changed = base;
        presentation_changed
            .presentations
            .get_mut(presentation_id)
            .unwrap()
            .binding = "renamed_model".into();
        presentation_changed.freeze_program_hashes();
        assert_ne!(
            presentation_changed.presentation_hashes[presentation_id],
            presentation
        );
    }

    #[test]
    fn machine_hash_ignores_unreachable_declarations_and_text_lookalikes() {
        let mut base = hash_program();
        let prefix = format!("{SEED}Extra");
        let text_identity = "example.hash@1::text-lookalike";
        base.machine_program
            .constants
            .insert(prefix.clone(), Value::int(10));
        base.machine_program
            .constant_types
            .insert(prefix.clone(), TypeRef::Int);
        base.machine_program
            .constants
            .insert(text_identity.into(), Value::int(20));
        base.machine_program
            .constant_types
            .insert(text_identity.into(), TypeRef::Int);
        base.machine_program.functions.insert(
            "example.hash@1::unused".into(),
            Function {
                id: "example.hash@1::unused".into(),
                params: Vec::new(),
                result: TypeRef::Text,
                body: Expr::Literal {
                    value: Value::Text(text_identity.into()),
                },
                source: source("unused"),
            },
        );
        base.machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .observation
            .push(ObservationField {
                name: "label".into(),
                ty: TypeRef::Text,
                expression: Expr::Literal {
                    value: Value::Text(text_identity.into()),
                },
                source: source("label"),
            });
        base.freeze_program_hashes();

        let mut changed = base.clone();
        changed
            .machine_program
            .constants
            .insert(prefix, Value::int(11));
        changed
            .machine_program
            .constants
            .insert(text_identity.into(), Value::int(21));
        changed.machine_program.types.insert(
            "example.hash@1::Unused".into(),
            TypeDef::Record {
                id: "example.hash@1::Unused".into(),
                fields: vec![("extra".into(), TypeRef::Bool)],
            },
        );
        changed.freeze_program_hashes();
        assert_eq!(machine_hash(&changed), machine_hash(&base));
    }

    #[test]
    fn machine_hash_changes_for_reachable_constant_function_and_type() {
        let base = hash_program();

        let mut constant_changed = base.clone();
        constant_changed
            .machine_program
            .constants
            .insert(SEED.into(), Value::int(2));
        constant_changed.freeze_program_hashes();
        assert_ne!(machine_hash(&constant_changed), machine_hash(&base));

        let mut function_changed = base.clone();
        function_changed
            .machine_program
            .functions
            .get_mut(READ)
            .unwrap()
            .body = Expr::Literal {
            value: Value::int(99),
        };
        function_changed.freeze_program_hashes();
        assert_ne!(machine_hash(&function_changed), machine_hash(&base));

        let mut type_changed = base.clone();
        let TypeDef::Record { fields, .. } =
            type_changed.machine_program.types.get_mut(DATA).unwrap()
        else {
            unreachable!()
        };
        fields.push(("flag".into(), TypeRef::Bool));
        type_changed.freeze_program_hashes();
        assert_ne!(machine_hash(&type_changed), machine_hash(&base));
    }

    #[test]
    fn application_program_uses_the_flat_current_wire_contract() {
        let program = profile_program_v04();
        let actual = serde_json::to_value(&program).expect("application program serializes");
        let mut expected = serde_json::to_value(program.as_machine_program())
            .expect("machine program serializes")
            .as_object()
            .expect("machine program wire value is an object")
            .clone();
        expected.insert(
            "presentations".into(),
            serde_json::to_value(&program.presentations).unwrap(),
        );
        expected.insert(
            "evidence".into(),
            serde_json::to_value(&program.evidence).unwrap(),
        );
        expected.insert(
            "route_tables".into(),
            serde_json::to_value(&program.route_tables).unwrap(),
        );
        expected.insert(
            "presentation_hashes".into(),
            serde_json::to_value(&program.presentation_hashes).unwrap(),
        );
        expected.insert(
            "evidence_hashes".into(),
            serde_json::to_value(&program.evidence_hashes).unwrap(),
        );

        assert_eq!(actual, serde_json::Value::Object(expected));
        assert!(
            actual.get("machine_program").is_none(),
            "the ownership boundary must not introduce a nested wire field"
        );

        let canonical = program.to_canonical_string();
        let roundtripped = Program::from_json(&canonical).expect("canonical Program round-trip");
        assert_eq!(roundtripped, program);
        assert_eq!(roundtripped.to_canonical_string(), canonical);
    }

    #[test]
    fn supplied_program_hashes_are_recomputed_and_verified() {
        let program = hash_program();
        Program::from_json(&program.to_canonical_string()).unwrap();

        let mut forged = program;
        forged
            .machine_program
            .program_hashes
            .insert(MACHINE.into(), "0".repeat(64));
        assert!(Program::from_json(&forged.to_canonical_string()).is_err());
    }

    #[test]
    fn public_ir_requires_exact_canonical_text_and_protocol() {
        let program = hash_program();
        let canonical = program.to_canonical_string();
        assert_eq!(
            Program::from_json(&format!("{canonical}\n")).unwrap_err(),
            "Uhura machine IR must be exact canonical `uhura-ir/1` JSON",
        );

        let mut wrong_protocol = program;
        wrong_protocol.machine_program.protocol = "uhura-ir/01".into();
        assert_eq!(
            Program::from_json(&wrong_protocol.to_canonical_string()).unwrap_err(),
            "expected `uhura-ir/1`, got `uhura-ir/01`",
        );
    }

    #[test]
    fn public_ir_rejects_unknown_fields_at_every_depth() {
        let program = hash_program();

        let mut top_level = serde_json::to_value(&program).unwrap();
        top_level["future"] = serde_json::json!({ "enabled": true });
        let error = Program::from_json(
            &uhura_base::try_to_canonical_json(&top_level).expect("integer-only test IR"),
        )
        .unwrap_err();
        assert_eq!(
            error,
            "Uhura machine IR does not match the closed `uhura-ir/1` schema",
        );

        let mut nested = serde_json::to_value(&program).unwrap();
        nested["machines"][MACHINE]["source"]["future"] = serde_json::json!(true);
        let error = Program::from_json(
            &uhura_base::try_to_canonical_json(&nested).expect("integer-only test IR"),
        )
        .unwrap_err();
        assert_eq!(
            error,
            "Uhura machine IR does not match the closed `uhura-ir/1` schema",
        );
    }

    #[test]
    fn public_ir_float_and_invalid_type_material_return_errors_without_panicking() {
        let mut float_json = serde_json::to_value(hash_program()).unwrap();
        float_json["machines"][MACHINE]["source"]["start"] = serde_json::json!(1.5);
        let float_source = serde_json::to_string(&float_json).unwrap();
        let float_result = std::panic::catch_unwind(|| Program::from_json(&float_source));
        assert!(float_result.is_ok(), "float admission must not panic");
        assert_eq!(
            float_result.unwrap().unwrap_err(),
            "Uhura machine IR is not canonical: $.machines.example.hash@1::Machine.source.start: floating-point JSON number `1.5` is not canonical Uhura data",
        );

        let mut invalid_type = profile_program();
        let TypeDef::Sum { constructors, .. } = &mut invalid_type
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .local_input
        else {
            unreachable!()
        };
        constructors[0].fields.push((
            Some("invalid".into()),
            TypeRef::Named {
                id: "Token<Map<>>".into(),
            },
        ));
        let invalid_type_source = invalid_type.to_canonical_string();
        let invalid_type_result =
            std::panic::catch_unwind(|| Program::from_json(&invalid_type_source));
        assert!(
            invalid_type_result.is_ok(),
            "malformed type material must not panic during identity recomputation",
        );
        let error = invalid_type_result.unwrap().unwrap_err();
        assert!(
            error.starts_with(
                "Uhura presentation `example.hash@1::Web` has invalid machine interface type material:"
            ),
            "{error}",
        );
        assert!(error.contains("invalid canonical type"), "{error}");
    }

    #[test]
    fn profile_identities_have_separate_dependency_scopes() {
        let base = profile_program();
        let presentation_id = "example.hash@1::Web";

        let mut presentation_changed = base.clone();
        presentation_changed
            .presentations
            .get_mut(presentation_id)
            .unwrap()
            .nodes[0] = UiNode::Text {
            value: "changed presentation".into(),
            source: source("changed-presentation"),
        };
        presentation_changed.freeze_program_hashes();
        assert_eq!(machine_hash(&presentation_changed), machine_hash(&base),);
        assert_ne!(
            presentation_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );
        assert_eq!(
            presentation_changed.evidence_hashes[MACHINE],
            base.evidence_hashes[MACHINE],
        );

        let mut ui_constant_changed = base.clone();
        ui_constant_changed
            .machine_program
            .constants
            .insert(UI_LABEL.into(), Value::Text("changed helper".into()));
        ui_constant_changed.freeze_program_hashes();
        assert_eq!(machine_hash(&ui_constant_changed), machine_hash(&base));
        assert_ne!(
            ui_constant_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );
        assert_eq!(
            ui_constant_changed.evidence_hashes[MACHINE],
            base.evidence_hashes[MACHINE],
        );

        let mut ui_function_changed = base.clone();
        ui_function_changed
            .machine_program
            .functions
            .get_mut(UI_READ)
            .unwrap()
            .body = Expr::Literal {
            value: Value::Text("changed function".into()),
        };
        ui_function_changed.freeze_program_hashes();
        assert_eq!(machine_hash(&ui_function_changed), machine_hash(&base));
        assert_ne!(
            ui_function_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );

        let mut evidence_constant_changed = base.clone();
        evidence_constant_changed
            .machine_program
            .constants
            .insert(EVIDENCE_FLAG.into(), Value::Bool(false));
        evidence_constant_changed.freeze_program_hashes();
        assert_eq!(
            machine_hash(&evidence_constant_changed),
            machine_hash(&base)
        );
        assert_eq!(
            evidence_constant_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );
        assert_ne!(
            evidence_constant_changed.evidence_hashes[MACHINE],
            base.evidence_hashes[MACHINE],
        );

        let mut evidence_function_changed = base.clone();
        evidence_function_changed
            .machine_program
            .functions
            .get_mut(EVIDENCE_CHECK)
            .unwrap()
            .body = Expr::Literal {
            value: Value::Bool(false),
        };
        evidence_function_changed.freeze_program_hashes();
        assert_eq!(
            machine_hash(&evidence_function_changed),
            machine_hash(&base)
        );
        assert_ne!(
            evidence_function_changed.evidence_hashes[MACHINE],
            base.evidence_hashes[MACHINE],
        );

        let mut evidence_registration_changed = base.clone();
        evidence_registration_changed.evidence.examples.insert(
            "example.hash@1::second-example".into(),
            EvidenceRef {
                scenario: "example.hash@1::proof".into(),
                pin: "ready".into(),
            },
        );
        evidence_registration_changed.freeze_program_hashes();
        assert_eq!(
            machine_hash(&evidence_registration_changed),
            machine_hash(&base),
        );
        assert_eq!(
            evidence_registration_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );
        assert_ne!(
            evidence_registration_changed.evidence_hashes[MACHINE],
            base.evidence_hashes[MACHINE],
        );

        let mut evidence_source_changed = base.clone();
        evidence_source_changed.evidence.example_sources.insert(
            "example.hash@1::example".into(),
            SourceRef {
                id: "example-registration".into(),
                path: "moved/conformance.uhura".into(),
                start: 200,
                end: 240,
            },
        );
        evidence_source_changed.evidence.checkpoint_sources.insert(
            "example.hash@1::checkpoint".into(),
            SourceRef {
                id: "checkpoint-registration".into(),
                path: "moved/conformance.uhura".into(),
                start: 250,
                end: 290,
            },
        );
        evidence_source_changed.freeze_program_hashes();
        assert_eq!(
            evidence_source_changed.machine_program.program_hashes,
            base.machine_program.program_hashes,
            "physical evidence registration sources do not affect machine identity",
        );
        assert_eq!(
            evidence_source_changed.presentation_hashes, base.presentation_hashes,
            "physical evidence registration sources do not affect presentation identity",
        );
        assert_eq!(
            evidence_source_changed.evidence_hashes, base.evidence_hashes,
            "physical evidence registration sources do not affect evidence identity",
        );

        let mut implementation_changed = base.clone();
        implementation_changed
            .machine_program
            .functions
            .get_mut(READ)
            .unwrap()
            .body = Expr::Literal {
            value: Value::int(77),
        };
        implementation_changed.freeze_program_hashes();
        assert_ne!(machine_hash(&implementation_changed), machine_hash(&base));
        assert_eq!(
            implementation_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );
        assert_ne!(
            implementation_changed.evidence_hashes[MACHINE],
            base.evidence_hashes[MACHINE],
        );

        let mut interface_changed = base.clone();
        let TypeDef::Sum { constructors, .. } = &mut interface_changed
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .local_input
        else {
            unreachable!()
        };
        constructors.push(ConstructorDef {
            name: "second".into(),
            fields: Vec::new(),
        });
        interface_changed.freeze_program_hashes();
        assert_ne!(
            interface_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );

        let mut referenced_interface_type_changed = base.clone();
        let TypeDef::Record { fields, .. } = referenced_interface_type_changed
            .machine_program
            .types
            .get_mut(DATA)
            .expect("profile payload type")
        else {
            unreachable!()
        };
        fields.push(("label".into(), TypeRef::Text));
        referenced_interface_type_changed.freeze_program_hashes();
        assert_ne!(
            referenced_interface_type_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
        );
    }

    #[test]
    fn v04_machine_ui_interface_identity_covers_the_complete_aggregate_input_contract() {
        let presentation_id = "example.hash@1::Web";
        let port_data = "example.hash@1::PortData";
        let receive_port = PortDef {
            name: "requests".into(),
            contract: "example.request@1".into(),
            contract_instance: None,
            type_arguments: vec![TypeRef::Named {
                id: port_data.into(),
            }],
            configuration: None,
            receive: vec![ConstructorDef {
                name: "settled".into(),
                fields: vec![(
                    Some("payload".into()),
                    TypeRef::Named {
                        id: port_data.into(),
                    },
                )],
            }],
            send: vec![ConstructorDef {
                name: "request".into(),
                fields: vec![(Some("payload".into()), TypeRef::Text)],
            }],
            contract_hash: "11".repeat(32),
            source: source("requests-port"),
        };

        let mut base = profile_program_v04();
        base.machine_program.types.insert(
            port_data.into(),
            TypeDef::Record {
                id: port_data.into(),
                fields: vec![("value".into(), TypeRef::Int)],
            },
        );
        base.machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports
            .push(receive_port);
        base.freeze_program_hashes();
        let interface = base.machine_ui_interface_hash(MACHINE).unwrap();

        let mut receive_changed = base.clone();
        receive_changed
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports[0]
            .receive[0]
            .fields[0]
            .1 = TypeRef::Text;
        receive_changed.freeze_program_hashes();
        assert_ne!(
            receive_changed.machine_ui_interface_hash(MACHINE).unwrap(),
            interface,
            "changing an accepted port input must invalidate MachineUiInterfaceId",
        );
        assert_ne!(
            receive_changed.presentation_hashes[presentation_id],
            base.presentation_hashes[presentation_id],
            "PresentationId must consume MachineUiInterfaceId",
        );

        let mut reachable_receive_type_changed = base.clone();
        let TypeDef::Record { fields, .. } = reachable_receive_type_changed
            .machine_program
            .types
            .get_mut(port_data)
            .expect("port-only receive payload type")
        else {
            unreachable!()
        };
        fields.push(("label".into(), TypeRef::Text));
        reachable_receive_type_changed.freeze_program_hashes();
        assert_ne!(
            reachable_receive_type_changed
                .machine_ui_interface_hash(MACHINE)
                .unwrap(),
            interface,
            "changing a type reachable only through a port input must invalidate MachineUiInterfaceId",
        );

        let mut send_only_changed = base.clone();
        send_only_changed
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports[0]
            .send[0]
            .fields[0]
            .1 = TypeRef::Int;
        send_only_changed.freeze_program_hashes();
        assert_eq!(
            send_only_changed
                .machine_ui_interface_hash(MACHINE)
                .unwrap(),
            interface,
            "machine commands are outside the aggregate input contract",
        );

        let mut empty_receive_added = base.clone();
        empty_receive_added
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports
            .push(PortDef {
                name: "telemetry".into(),
                contract: "example.sink@1".into(),
                contract_instance: None,
                type_arguments: vec![TypeRef::Text],
                configuration: None,
                receive: Vec::new(),
                send: vec![ConstructorDef {
                    name: "send".into(),
                    fields: vec![(Some("value".into()), TypeRef::Text)],
                }],
                contract_hash: "22".repeat(32),
                source: source("telemetry-port"),
            });
        empty_receive_added.freeze_program_hashes();
        assert_eq!(
            empty_receive_added
                .machine_ui_interface_hash(MACHINE)
                .unwrap(),
            interface,
            "a send-only port contributes the empty input sum",
        );
    }

    #[test]
    fn deployment_identity_sorts_bindings_and_covers_exact_resolved_material() {
        let base = profile_program();
        let material = DeploymentIdentityMaterial {
            machine: MACHINE.into(),
            machine_program_id: machine_hash(&base).into(),
            presentation: Some(DeploymentPresentationIdentity {
                id: "example.hash@1::Web".into(),
                presentation_id: base.presentation_hashes["example.hash@1::Web"].clone(),
            }),
            entry: "web".into(),
            lifetime: "application-session".into(),
            configuration: Value::Unit.to_wire_json(),
            port_bindings: vec![
                DeploymentPortBinding {
                    port: "zeta".into(),
                    adapter: "adapter.zeta".into(),
                    required_contract_hash: "22".repeat(32),
                    admitted_contract_instance_hash: "44".repeat(32),
                },
                DeploymentPortBinding {
                    port: "alpha".into(),
                    adapter: "adapter.alpha".into(),
                    required_contract_hash: "11".repeat(32),
                    admitted_contract_instance_hash: "33".repeat(32),
                },
            ],
            stylesheet: Some(DeploymentContentIdentity {
                protocol: "text/css".into(),
                configuration: serde_json::Value::Null,
                content_hash: "55".repeat(32),
            }),
            provider: Some(DeploymentContentIdentity {
                protocol: "uhura-adapter-provider/0".into(),
                configuration: serde_json::json!({"endpoint": "https://example.test"}),
                content_hash: "66".repeat(32),
            }),
        };
        let first = deployment_hash(&material).unwrap();
        assert_eq!(
            first,
            "5f3b3b1c78805a38ab9143f5dea18edc30cc006f0737a9f9743d60f3509206f2"
        );
        assert!(
            !serde_json::to_string(&material)
                .unwrap()
                .contains("\"path\""),
            "physical resource and provider paths have no DeploymentId field"
        );
        let mut reordered_material = material.clone();
        reordered_material.port_bindings.reverse();
        let reordered = deployment_hash(&reordered_material).unwrap();
        assert_eq!(first, reordered);

        let mut content_changed = material.clone();
        content_changed.provider.as_mut().unwrap().content_hash = "77".repeat(32);
        assert_ne!(first, deployment_hash(&content_changed).unwrap());

        let mut configuration_changed = material.clone();
        configuration_changed
            .provider
            .as_mut()
            .unwrap()
            .configuration = serde_json::json!({"endpoint": "https://other.test"});
        assert_ne!(first, deployment_hash(&configuration_changed).unwrap());

        let mut duplicate = material;
        duplicate.port_bindings[1].port = duplicate.port_bindings[0].port.clone();
        assert!(deployment_hash(&duplicate).is_err());
    }

    #[test]
    fn evidence_fixture_identity_uses_evaluated_canonical_configuration() {
        let mut program = profile_program();
        program
            .machine_program
            .machines
            .get_mut(MACHINE)
            .unwrap()
            .ports
            .push(PortDef {
                name: "requests".into(),
                contract: "RequestPort".into(),
                contract_instance: None,
                type_arguments: Vec::new(),
                configuration: None,
                receive: Vec::new(),
                send: Vec::new(),
                contract_hash: "11".repeat(32),
                source: source("requests-port"),
            });
        let scenario_id = "example.hash@1::proof";
        program
            .evidence
            .scenarios
            .get_mut(scenario_id)
            .unwrap()
            .steps
            .insert(
                0,
                EvidenceStep::Bind {
                    port: "requests".into(),
                    fixture: Expr::Call {
                        function: "RequestPort.fixture".into(),
                        args: Vec::new(),
                        result_type: TypeRef::Unit,
                    },
                    source: source("bind-requests"),
                },
            );
        let selected = BTreeSet::from([scenario_id.to_string()]);
        let implicit_unit = program.fixture_configuration(MACHINE, &selected);

        let EvidenceStep::Bind { fixture, .. } = &mut program
            .evidence
            .scenarios
            .get_mut(scenario_id)
            .unwrap()
            .steps[0]
        else {
            unreachable!()
        };
        let Expr::Call { args, .. } = fixture else {
            unreachable!()
        };
        args.push(Expr::Literal { value: Value::Unit });
        assert_eq!(
            program.fixture_configuration(MACHINE, &selected),
            implicit_unit,
        );

        let EvidenceStep::Bind { fixture, .. } = &mut program
            .evidence
            .scenarios
            .get_mut(scenario_id)
            .unwrap()
            .steps[0]
        else {
            unreachable!()
        };
        let Expr::Call { args, .. } = fixture else {
            unreachable!()
        };
        args[0] = Expr::Literal {
            value: Value::Text("different".into()),
        };
        assert_ne!(
            program.fixture_configuration(MACHINE, &selected),
            implicit_unit,
        );
    }
}
