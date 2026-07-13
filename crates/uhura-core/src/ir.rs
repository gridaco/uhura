//! The checked IR (`"uhura-ir/0"`) — the serialized artifact the checker
//! lowers to and the machine executes (design §12.2). Everything here is
//! *resolved*: name references are classified, port/catalog authority is
//! reduced to pins plus the slices the runtime needs, and markup is a
//! closed template-op tree with stable node ordinals (§8.1 keys).
//!
//! Serialization is canonical JSON via `uhura_base::to_canonical_json`
//! (sorted keys fall out of the `BTreeMap` backing; no float can occur by
//! type shape). Spans live in a side table owned by the checker — IR bytes
//! are location-independent, so `*.examples.uhura` files and whitespace
//! churn cannot move them (§6.1 invariance).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uhura_base::{Ident, hash_json, to_canonical_json};

pub const IR_PROTOCOL: &str = "uhura-ir/0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProgramIr {
    /// Always `"uhura-ir/0"`; `load_program` hard-gates on it.
    pub protocol: String,
    pub app: Ident,
    /// Entry route name (a key of `routes`).
    pub entry: Ident,
    pub catalog: CatalogPin,
    /// Port name → contract pin. Contracts themselves stay outside the IR;
    /// the runtime slices it needs are baked in below.
    pub ports: BTreeMap<Ident, PortPin>,
    /// Projection name → binding. Projection (and command) names are
    /// globally unique across the app's ports — a link rule, which is what
    /// lets `X` key snapshots by bare name (§7.1).
    pub projections: BTreeMap<Ident, ProjectionIr>,
    /// Element → event → signature: the catalog slice descriptor building
    /// needs (`kind`, `carries` — §8.1).
    pub element_events: BTreeMap<Ident, BTreeMap<Ident, ElementEventIr>>,
    /// Element → prop → kind: how `eval_view` wraps prop values in V —
    /// human text becomes `{t:"plain"}`, tokens stay bare text, assets
    /// become `{t:"image"}` (§8.1).
    pub element_props: BTreeMap<Ident, BTreeMap<Ident, PropKindIr>>,
    /// Route name → shape. The page definition lives in `pages` under the
    /// same name.
    pub routes: BTreeMap<Ident, RouteIr>,
    pub pages: BTreeMap<Ident, DefIr>,
    pub components: BTreeMap<Ident, DefIr>,
    pub surfaces: BTreeMap<Ident, DefIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CatalogPin {
    pub name: Ident,
    pub version: String,
    /// SHA-256 of the catalog's canonical form.
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PortPin {
    pub version: String,
    /// SHA-256 of the contract's canonical form (§9.1).
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectionIr {
    pub port: Ident,
    /// Delivered before `Init`; bare reads legal (§9.2).
    pub boot: bool,
    /// The snapshot's structural type — how `apply_updates` decodes wire
    /// JSON into typed values (the runtime half of L8).
    pub ty: TyIr,
    /// The key type of a keyed projection (`for-post(post: id)`).
    pub key: Option<TyIr>,
}

/// The closed structural type grammar, baked into the IR wherever the
/// runtime must decode wire JSON into typed `Value`s (event payloads,
/// projection updates). Declared nominal id/cursor types collapse to `Id`
/// here — the wire form is a string either way; nominal identity is a
/// check-time concern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TyIr {
    Bool,
    Int,
    Text,
    Id,
    /// Core-minted command tag (`"t-<n>"` on the wire).
    Tag,
    Asset,
    /// A closed token set; values are the member names.
    Enum(Vec<Ident>),
    Option(Box<TyIr>),
    List(Box<TyIr>),
    Map {
        key: MapKeyIr,
        value: Box<TyIr>,
    },
    Record(BTreeMap<Ident, TyIr>),
    /// Variant → payload record fields.
    Union(BTreeMap<Ident, BTreeMap<Ident, TyIr>>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MapKeyIr {
    Id,
    Tag,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ElementEventIr {
    pub kind: EventKindIr,
    /// Renderer-carried payload fields and their shapes (§4.2).
    pub carries: BTreeMap<Ident, CarryTypeIr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKindIr {
    Input,
    Observe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CarryTypeIr {
    Text,
    Bool,
    Int,
}

/// V-facing prop wrapping (§8.1): `Plain` is inert human text, `Token` is
/// a bare semantic string (enum/icon values), `Asset` is an opaque asset
/// reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PropKindIr {
    Plain,
    Token,
    Bool,
    Int,
    Asset,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RouteIr {
    /// `/profile/:user` as segments; params in path order.
    pub segments: Vec<RouteSegIr>,
    pub params: Vec<Ident>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteSegIr {
    Static(String),
    Param(Ident),
}

// ── definitions ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DefIr {
    /// Surfaces only (`"sheet"`).
    pub modality: Option<String>,
    /// Declared prop names, source order (payload shapes are compile-
    /// checked; open-surface context and component expansion bind by name).
    pub props: Vec<Ident>,
    /// Declared emit names, source order.
    pub emits: Vec<Ident>,
    /// Route params, path order (pages).
    pub params: Vec<Ident>,
    pub state: BTreeMap<Ident, InitValue>,
    /// Machine-event signatures (pages/surfaces): event → params in
    /// signature order. This is both the `Ui` eligibility check (§7.2 —
    /// payload fields must match exactly) and how payload JSON decodes
    /// into typed values.
    pub events: BTreeMap<Ident, Vec<EventParamIr>>,
    pub handlers: Vec<HandlerIr>,
    /// Exactly one root (§4.4/§8.1 — pages and surfaces included: the
    /// snapshot has a single `root` Node).
    pub root: NodeIr,
}

/// State initializers are literals only (§4.3), so the IR never needs a
/// full `Value` encoding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InitValue {
    Int(i64),
    Text(String),
    Bool(bool),
    None,
    /// `{}` — the empty map. Maps are represented as records keyed by the
    /// canonical string of the key value (`"t-4"`, kebab-safe ids —
    /// entity-id shapes are linted, §6.2).
    EmptyMap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EventParamIr {
    pub name: Ident,
    pub ty: TyIr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct HandlerIr {
    pub on: EventKeyIr,
    /// Param names in signature order; payload fields bind by name.
    pub params: Vec<Ident>,
    pub guard: Option<ExprIr>,
    pub body: Vec<StmtIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKeyIr {
    Semantic {
        event: Ident,
    },
    Outcome {
        command: Ident,
        which: OutcomeKindIr,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeKindIr {
    Ok,
    Err,
}

/// The five statements, resolved (§4.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StmtIr {
    Set {
        field: Ident,
        key: Option<ExprIr>,
        value: ExprIr,
    },
    Send {
        port: Ident,
        command: Ident,
        args: Vec<ArgIr>,
        bind: Option<Ident>,
    },
    OpenSurface {
        surface: Ident,
        args: Vec<ArgIr>,
    },
    Dismiss,
    Navigate {
        route: Ident,
        args: Vec<ArgIr>,
    },
    NavigateReplace {
        route: Ident,
        args: Vec<ArgIr>,
    },
    NavigateBack,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ArgIr {
    pub name: Ident,
    pub value: ExprIr,
}

// ── expressions ─────────────────────────────────────────────────────────────

/// Resolved expressions: every name is classified, so the runtime never
/// consults a scope table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExprIr {
    Int(i64),
    Text(String),
    Bool(bool),
    None,
    StateRef(Ident),
    PropRef(Ident),
    ParamRef(Ident),
    /// `{#each}` item, `{:when v x}` binding, or handler param.
    BindingRef(Ident),
    /// Singleton (or boot) projection read.
    ProjectionRef(Ident),
    /// Keyed projection read: `for-post(post)`.
    ProjectionKeyed {
        projection: Ident,
        key: Box<ExprIr>,
    },
    Field {
        base: Box<ExprIr>,
        name: Ident,
    },
    /// Option-returning `base[key]` (§4.3).
    Index {
        base: Box<ExprIr>,
        key: Box<ExprIr>,
    },
    Unary {
        op: UnaryOpIr,
        expr: Box<ExprIr>,
    },
    Binary {
        op: BinaryOpIr,
        lhs: Box<ExprIr>,
        rhs: Box<ExprIr>,
    },
    If {
        cond: Box<ExprIr>,
        then: Box<ExprIr>,
        els: Box<ExprIr>,
    },
    ToText(Box<ExprIr>),
    Count(Box<ExprIr>),
    RecordLit(Vec<ArgIr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnaryOpIr {
    Not,
    Neg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BinaryOpIr {
    Add,
    Sub,
    Concat,
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Coalesce,
}

// ── markup template ops ─────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeIr {
    Element(ElementIr),
    Component(ComponentCallIr),
    If {
        cond: ExprIr,
        then: Vec<NodeIr>,
        els: Vec<NodeIr>,
    },
    Each(EachIr),
    Match(MatchIr),
}

/// Node ordinals are assigned depth-first pre-order per definition, so
/// they are sibling-unique whatever branches render (§8.1: static keys are
/// source ordinals; `{#each}` items take `"<ordinal>.<key-value>"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ElementIr {
    pub element: Ident,
    pub ord: u32,
    /// Authored CSS classes — an opaque expression, carried to V verbatim.
    pub class: Option<ExprIr>,
    /// Semantic props, source order.
    pub props: Vec<ArgIr>,
    pub events: Vec<ElementEventBindingIr>,
    /// `<text>` content runs; empty for every other element.
    pub text: Vec<TextRunIr>,
    pub children: Vec<NodeIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ElementEventBindingIr {
    /// The catalog event (`press`, `near-end`, `change`).
    pub event: Ident,
    /// The machine event the descriptor emits.
    pub emit: Ident,
    /// Author args; carried fields are appended by the renderer (§4.2).
    pub args: Vec<ArgIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ComponentCallIr {
    pub component: Ident,
    pub ord: u32,
    pub props: Vec<ArgIr>,
    /// Call-site consumption of the component's declared emits (§4.4).
    pub emits: Vec<EmitBindingIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EmitBindingIr {
    /// The component's declared emit name.
    pub emit: Ident,
    pub target: EmitTargetIr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmitTargetIr {
    /// Bare `on:like-toggled` — same name, same payload, enclosing scope.
    Forward,
    /// `on:dismissed={emit notice-dismissed()}` — new event; args evaluate
    /// in the *caller's* scope; the component payload is discarded.
    Rebind { event: Ident, args: Vec<ArgIr> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EachIr {
    pub ord: u32,
    pub item: Ident,
    pub over: OverIr,
    pub seq: ExprIr,
    /// Evaluated with `item` bound; its canonical string joins the slot
    /// ordinal to form the child key.
    pub key: ExprIr,
    pub body: Vec<NodeIr>,
}

/// What the sequence expression yields (typecheck bakes it in so iteration
/// can rebuild typed map keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverIr {
    List,
    MapIdKeys,
    MapTagKeys,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MatchIr {
    pub source: MatchSourceIr,
    pub arms: Vec<MatchArmIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchSourceIr {
    /// Availability arms (`loading | failed reason | ready v`) over
    /// projection presence — session truth, not contract types (§9.2).
    Availability {
        projection: Ident,
        key: Option<ExprIr>,
    },
    /// A closed port union value; arms are variant names.
    Union { value: ExprIr },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MatchArmIr {
    /// `None` is the `{:else}` arm.
    pub variant: Option<Ident>,
    pub binding: Option<Ident>,
    pub body: Vec<NodeIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextRunIr {
    Literal(String),
    Interp(ExprIr),
}

// ── serialize / load / hash ─────────────────────────────────────────────────

impl ProgramIr {
    /// The canonical byte form — what `--emit-ir` writes and goldens pin.
    pub fn to_canonical_string(&self) -> String {
        let value = serde_json::to_value(self).expect("IR types are always serializable");
        to_canonical_json(&value)
    }

    /// SHA-256 of the canonical form.
    pub fn hash(&self) -> String {
        let value = serde_json::to_value(self).expect("IR types are always serializable");
        hash_json(&value)
    }
}

/// Loads serialized IR with the hard version gate (§12.2): anything but
/// `"uhura-ir/0"` is refused before deserialization is attempted.
pub fn load_program(text: &str) -> Result<ProgramIr, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("IR is not valid JSON: {e}"))?;
    match value.get("protocol").and_then(serde_json::Value::as_str) {
        Some(IR_PROTOCOL) => {}
        Some(other) => {
            return Err(format!(
                "IR protocol `{other}` is not supported (this build reads `{IR_PROTOCOL}`)"
            ));
        }
        None => return Err("IR has no `protocol` field".to_string()),
    }
    serde_json::from_value(value).map_err(|e| format!("malformed `{IR_PROTOCOL}`: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(s: &str) -> Ident {
        Ident::new(s).unwrap()
    }

    fn tiny_program() -> ProgramIr {
        let root = NodeIr::Element(ElementIr {
            element: ident("view"),
            ord: 0,
            class: Some(ExprIr::Text("screen".into())),
            props: vec![],
            events: vec![],
            text: vec![],
            children: vec![NodeIr::Element(ElementIr {
                element: ident("text"),
                ord: 1,
                class: None,
                props: vec![],
                events: vec![],
                text: vec![
                    TextRunIr::Literal("Hello ".into()),
                    TextRunIr::Interp(ExprIr::StateRef(ident("who"))),
                ],
                children: vec![],
            })],
        });
        let mut state = BTreeMap::new();
        state.insert(ident("who"), InitValue::Text("world".into()));
        let mut events = BTreeMap::new();
        events.insert(ident("poked"), vec![]);
        let page = DefIr {
            modality: None,
            props: vec![],
            emits: vec![],
            params: vec![],
            state,
            events,
            handlers: vec![HandlerIr {
                on: EventKeyIr::Semantic {
                    event: ident("poked"),
                },
                params: vec![],
                guard: None,
                body: vec![StmtIr::Set {
                    field: ident("who"),
                    key: None,
                    value: ExprIr::Text("you".into()),
                }],
            }],
            root,
        };
        let mut routes = BTreeMap::new();
        routes.insert(
            ident("home"),
            RouteIr {
                segments: vec![RouteSegIr::Static("home".into())],
                params: vec![],
            },
        );
        let mut pages = BTreeMap::new();
        pages.insert(ident("home"), page);
        ProgramIr {
            protocol: IR_PROTOCOL.to_string(),
            app: ident("tiny"),
            entry: ident("home"),
            catalog: CatalogPin {
                name: ident("base"),
                version: "0.1.0".into(),
                hash: "0".repeat(64),
            },
            ports: BTreeMap::new(),
            projections: BTreeMap::new(),
            element_events: BTreeMap::new(),
            element_props: BTreeMap::new(),
            routes,
            pages,
            components: BTreeMap::new(),
            surfaces: BTreeMap::new(),
        }
    }

    #[test]
    fn round_trips_canonically() {
        let program = tiny_program();
        let bytes = program.to_canonical_string();
        let reloaded = load_program(&bytes).unwrap();
        assert_eq!(reloaded, program);
        assert_eq!(reloaded.to_canonical_string(), bytes, "byte-stable");
    }

    #[test]
    fn version_gate_is_hard() {
        let program = tiny_program();
        let bytes = program
            .to_canonical_string()
            .replace("uhura-ir/0", "uhura-ir/1");
        let err = load_program(&bytes).unwrap_err();
        assert!(err.contains("uhura-ir/1"), "{err}");
    }
}
