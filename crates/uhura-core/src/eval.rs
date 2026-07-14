//! `eval_view` — V as a pure function of (program, U, X) (design §7.1,
//! §8.1). Component expansion is transparent (call-site keys, resolved
//! emit chains, prebuilt descriptor payloads); availability arms come from
//! projection presence; descriptor presence is the subscription.

use std::collections::BTreeMap;

use uhura_base::{Ident, Value};

use crate::ir::{self, ProgramIr};
use crate::state::{Projections, UiState, map_key_string};
use crate::view::{Descriptor, DescriptorKind, Node, PageView, Snapshot, SurfaceView, VValue};

/// An internal invariant break — checked programs cannot produce these; if
/// one appears it is a UH9xxx bug, not an author error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalError(pub String);

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "view evaluation broke an invariant: {}", self.0)
    }
}

/// The full snapshot (§8.1): the top nav entry's page plus the surface
/// stack, bottom → top.
pub fn eval_view(p: &ProgramIr, u: &UiState, x: &Projections) -> Result<Snapshot, EvalError> {
    let entry = u
        .nav
        .last()
        .ok_or_else(|| EvalError("empty nav stack".into()))?;
    let def = p
        .pages
        .get(&entry.route)
        .ok_or_else(|| EvalError(format!("no page for route `{}`", entry.route)))?;
    let page_frame = Frame {
        program: p,
        x,
        scope: format!("page:{}", entry.serial),
        state: &entry.state,
        props: BTreeMap::new(),
        params: entry.params.clone(),
        bindings: Vec::new(),
        emits: EmitEnv::Machine,
    };
    let root = single_root(page_frame.eval_node_ir(&def.root, None)?)?;

    let mut surfaces = Vec::new();
    for s in &u.surfaces {
        let def = p
            .surfaces
            .get(&s.definition)
            .ok_or_else(|| EvalError(format!("no surface `{}`", s.definition)))?;
        let scope = format!("surface:{}", s.serial);
        let frame = Frame {
            program: p,
            x,
            scope: scope.clone(),
            state: &s.state,
            props: s.props.clone(),
            params: BTreeMap::new(),
            bindings: Vec::new(),
            emits: EmitEnv::Machine,
        };
        let root = single_root(frame.eval_node_ir(&def.root, None)?)?;
        surfaces.push(SurfaceView {
            key: format!("{}:{}", s.definition, s.serial),
            definition: s.definition.to_string(),
            modality: def.modality.clone().unwrap_or_else(|| "sheet".into()),
            restore_focus: s.restore_focus.clone(),
            dismiss: dismiss_descriptor(&scope),
            root,
        });
    }

    Ok(Snapshot {
        revision: u.rev,
        page: PageView {
            route: entry.route.to_string(),
            root,
        },
        surfaces,
    })
}

/// A standalone definition preview (§6.2 `PreviewPayload::Fragment`) —
/// component and surface examples render outside any nav stack.
pub fn eval_fragment(
    p: &ProgramIr,
    def: &ir::DefIr,
    props: &BTreeMap<Ident, Value>,
    state: &BTreeMap<Ident, Value>,
    x: &Projections,
) -> Result<Node, EvalError> {
    let frame = Frame {
        program: p,
        x,
        scope: "fragment:0".to_string(),
        state,
        props: props.clone(),
        params: BTreeMap::new(),
        bindings: Vec::new(),
        emits: EmitEnv::Machine,
    };
    single_root(frame.eval_node_ir(&def.root, None)?)
}

/// The first-class surface dismiss descriptor (§8.1): Escape/scrim emit the
/// reserved machine event `dismiss`, which core handles structurally (M4) —
/// no authored handler is involved.
fn dismiss_descriptor(scope: &str) -> Descriptor {
    Descriptor {
        kind: DescriptorKind::Input,
        event: Ident::new("dismiss").expect("kebab"),
        emit: Ident::new("dismiss").expect("kebab"),
        scope: scope.to_string(),
        payload: serde_json::json!({}),
        carries: BTreeMap::new(),
    }
}

fn single_root(nodes: Vec<Node>) -> Result<Node, EvalError> {
    let mut nodes = nodes;
    match nodes.len() {
        1 => Ok(nodes.remove(0)),
        n => Err(EvalError(format!(
            "a definition renders exactly one root, got {n}"
        ))),
    }
}

/// Where an emit resolves once it reaches machine scope (built at component
/// expansion; §4.4's one explicit model).
#[derive(Clone, Debug)]
pub(crate) enum EmitSink {
    /// Emit under this (possibly renamed) name; use-site payload passes
    /// through (bare forward chains collapse to one rename).
    Rename(Ident),
    /// Call-site rebind: fixed name, payload prebuilt in the caller's
    /// scope; the component's payload is discarded (§4.4).
    Fixed(Ident, serde_json::Value),
    /// Unbound at some call site — the control is dead (warned at check).
    Dead,
}

#[derive(Clone, Debug)]
pub(crate) enum EmitEnv {
    /// Page/surface markup: emits hit the machine as-is.
    Machine,
    /// Component markup: the component's declared emits, resolved through
    /// every call site above.
    Component(BTreeMap<Ident, EmitSink>),
}

impl EmitEnv {
    fn resolve(&self, emit: &Ident) -> EmitSink {
        match self {
            EmitEnv::Machine => EmitSink::Rename(emit.clone()),
            EmitEnv::Component(table) => table.get(emit).cloned().unwrap_or(EmitSink::Dead),
        }
    }
}

/// Why an expression could not produce a value.
#[derive(Clone, Debug)]
pub(crate) enum Stop {
    /// A projection read hit an undelivered instance. Legal in guards and
    /// bodies (transactional backstop, §4.2); an invariant break in view
    /// position (the checker forces availability matches).
    NotReady(Ident),
    Internal(String),
}

pub(crate) struct Frame<'a> {
    pub(crate) program: &'a ProgramIr,
    pub(crate) x: &'a Projections,
    pub(crate) scope: String,
    pub(crate) state: &'a BTreeMap<Ident, Value>,
    pub(crate) props: BTreeMap<Ident, Value>,
    pub(crate) params: BTreeMap<Ident, Value>,
    pub(crate) bindings: Vec<(Ident, Value)>,
    pub(crate) emits: EmitEnv,
}

impl Frame<'_> {
    // ── expressions ────────────────────────────────────────────────────
    //
    // Optionals hold their value BARE (`Value::None` or the value itself;
    // `Value::Some` is never constructed — options do not nest, §7.1), so
    // equality, `??`, and JSON encoding stay uniform.

    pub(crate) fn eval_expr(&self, e: &ir::ExprIr) -> Result<Value, Stop> {
        use ir::ExprIr as E;
        match e {
            E::Int(i) => Ok(Value::Int(*i)),
            E::Text(s) => Ok(Value::Text(s.clone())),
            E::Bool(b) => Ok(Value::Bool(*b)),
            E::None => Ok(Value::None),
            E::StateRef(name) => self
                .state
                .get(name)
                .cloned()
                .ok_or_else(|| Stop::Internal(format!("no state field `{name}`"))),
            E::PropRef(name) => self
                .props
                .get(name)
                .cloned()
                .ok_or_else(|| Stop::Internal(format!("no prop `{name}`"))),
            E::ParamRef(name) => self
                .params
                .get(name)
                .cloned()
                .ok_or_else(|| Stop::Internal(format!("no param `{name}`"))),
            E::BindingRef(name) => self
                .bindings
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| Stop::Internal(format!("no binding `{name}`"))),
            E::ProjectionRef(name) => self.read_projection(name, None),
            E::ProjectionKeyed { projection, key } => {
                let key = self.eval_expr(key)?;
                self.read_projection(projection, Some(key))
            }
            E::Field { base, name } => match self.eval_expr(base)? {
                Value::Record(fields) => fields
                    .get(name)
                    .cloned()
                    .ok_or_else(|| Stop::Internal(format!("no field `{name}`"))),
                other => Stop::internal(format!("`.{name}` on non-record {other:?}")),
            },
            E::Index { base, key } => {
                let base = self.eval_expr(base)?;
                let key = self.eval_expr(key)?;
                match base {
                    Value::Map(map) => {
                        let key = map_key_string(&key)
                            .ok_or_else(|| Stop::Internal("non-identity map key".into()))?;
                        Ok(map.get(&key).cloned().unwrap_or(Value::None))
                    }
                    Value::List(items) => match key {
                        Value::Int(i) => Ok(usize::try_from(i)
                            .ok()
                            .and_then(|i| items.get(i).cloned())
                            .unwrap_or(Value::None)),
                        _ => Stop::internal("list index must be int".into()),
                    },
                    other => Stop::internal(format!("indexing non-container {other:?}")),
                }
            }
            E::Unary { op, expr } => {
                let v = self.eval_expr(expr)?;
                match (op, v) {
                    (ir::UnaryOpIr::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (ir::UnaryOpIr::Neg, Value::Int(i)) => Ok(Value::Int(i.saturating_neg())),
                    (op, v) => Stop::internal(format!("unary {op:?} on {v:?}")),
                }
            }
            E::Binary { op, lhs, rhs } => self.eval_binary(*op, lhs, rhs),
            E::If { cond, then, els } => match self.eval_expr(cond)? {
                Value::Bool(true) => self.eval_expr(then),
                Value::Bool(false) => self.eval_expr(els),
                other => Stop::internal(format!("if condition was {other:?}")),
            },
            E::ToText(inner) => {
                let v = self.eval_expr(inner)?;
                match v {
                    Value::Int(i) => Ok(Value::Text(i.to_string())),
                    Value::Text(s) | Value::Id(s) => Ok(Value::Text(s)),
                    Value::Bool(b) => Ok(Value::Text(b.to_string())),
                    other => Stop::internal(format!("to-text on {other:?}")),
                }
            }
            E::Count(inner) => match self.eval_expr(inner)? {
                Value::List(items) => Ok(Value::Int(items.len() as i64)),
                Value::Map(map) => Ok(Value::Int(map.len() as i64)),
                other => Stop::internal(format!("count on {other:?}")),
            },
            E::RecordLit(entries) => {
                let mut record = BTreeMap::new();
                for arg in entries {
                    record.insert(arg.name.clone(), self.eval_expr(&arg.value)?);
                }
                Ok(Value::Record(record))
            }
        }
    }

    fn eval_binary(
        &self,
        op: ir::BinaryOpIr,
        lhs: &ir::ExprIr,
        rhs: &ir::ExprIr,
    ) -> Result<Value, Stop> {
        use ir::BinaryOpIr as B;
        // Short-circuit forms first — they gate projection reads in guards
        // (§4.2's transactional rule depends on this).
        match op {
            B::And => {
                return match self.eval_expr(lhs)? {
                    Value::Bool(false) => Ok(Value::Bool(false)),
                    Value::Bool(true) => self.eval_expr(rhs),
                    other => Stop::internal(format!("&& on {other:?}")),
                };
            }
            B::Or => {
                return match self.eval_expr(lhs)? {
                    Value::Bool(true) => Ok(Value::Bool(true)),
                    Value::Bool(false) => self.eval_expr(rhs),
                    other => Stop::internal(format!("|| on {other:?}")),
                };
            }
            B::Coalesce => {
                return match self.eval_expr(lhs)? {
                    Value::None => self.eval_expr(rhs),
                    present => Ok(present),
                };
            }
            _ => {}
        }
        let l = self.eval_expr(lhs)?;
        let r = self.eval_expr(rhs)?;
        match (op, l, r) {
            (B::Add, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.saturating_add(b))),
            (B::Sub, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.saturating_sub(b))),
            (B::Concat, Value::Text(a), Value::Text(b)) => Ok(Value::Text(a + &b)),
            (B::Eq, a, b) => Ok(Value::Bool(a == b)),
            (B::NotEq, a, b) => Ok(Value::Bool(a != b)),
            (B::Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            (B::Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            (B::Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            (B::Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
            (op, l, r) => Stop::internal(format!("{op:?} on {l:?} / {r:?}")),
        }
    }

    fn read_projection(&self, name: &Ident, key: Option<Value>) -> Result<Value, Stop> {
        match self.x.snapshots.get(&(name.clone(), key)) {
            Some(snapshot) => Ok(snapshot.value.clone()),
            None => Err(Stop::NotReady(name.clone())),
        }
    }

    // ── markup ─────────────────────────────────────────────────────────

    /// Evaluates one template op into zero or more V nodes. `key_override`
    /// is the call-site/each-item key (§8.1 transparent expansion).
    fn eval_node_ir(
        &self,
        node: &ir::NodeIr,
        key_override: Option<&str>,
    ) -> Result<Vec<Node>, EvalError> {
        match node {
            ir::NodeIr::If { cond, then, els } => {
                let cond = self.value(cond)?;
                let branch = match cond {
                    Value::Bool(true) => then,
                    Value::Bool(false) => els,
                    other => {
                        return Err(EvalError(format!("if condition was {other:?}")));
                    }
                };
                self.eval_nodes(branch)
            }
            ir::NodeIr::Each(each) => self.eval_each(each),
            ir::NodeIr::Match(m) => self.eval_match(m, key_override),
            ir::NodeIr::Element(el) => Ok(vec![self.eval_element(el, key_override)?]),
            ir::NodeIr::Component(call) => Ok(vec![self.eval_component(call, key_override)?]),
        }
    }

    fn eval_nodes(&self, nodes: &[ir::NodeIr]) -> Result<Vec<Node>, EvalError> {
        let mut out = Vec::new();
        for node in nodes {
            out.extend(self.eval_node_ir(node, None)?);
        }
        Ok(out)
    }

    fn eval_each(&self, each: &ir::EachIr) -> Result<Vec<Node>, EvalError> {
        let seq = self.value(&each.seq)?;
        let items: Vec<Value> = match (&each.over, seq) {
            (ir::OverIr::List, Value::List(items)) => items,
            (ir::OverIr::MapIdKeys, Value::Map(map)) => {
                map.keys().map(|k| Value::Id(k.clone())).collect()
            }
            (ir::OverIr::MapTagKeys, Value::Map(map)) => map
                .keys()
                .map(|k| {
                    k.strip_prefix("t-")
                        .and_then(|n| n.parse::<u64>().ok())
                        .map(Value::Tag)
                        .ok_or_else(|| EvalError(format!("non-tag map key `{k}`")))
                })
                .collect::<Result<_, _>>()?,
            (over, other) => {
                return Err(EvalError(format!("each over {over:?} got {other:?}")));
            }
        };

        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for item in items {
            let mut frame = self.child_frame();
            frame.bindings.push((each.item.clone(), item));
            let key_value = frame.value(&each.key)?;
            let key_str = key_string(&key_value)
                .ok_or_else(|| EvalError(format!("non-identity each key {key_value:?}")))?;
            // Keys are sibling identity (§8.1) — a data collision is §4.8's
            // "duplicate keys" rejection, raised here because only
            // evaluation sees the data; check catches it through example
            // replay (§6.2), the harnesses at dispatch time.
            if !seen.insert(key_str.clone()) {
                return Err(EvalError(format!(
                    "duplicate key `{key_str}` in {{#each}} — keys are sibling \
                     identity and must be unique (§8.1)"
                )));
            }
            let item_key = format!("{}.{}", each.ord, key_str);
            let rendered = {
                let mut nodes = Vec::new();
                for (j, node) in each.body.iter().enumerate() {
                    let node_key = if each.body.len() == 1 {
                        item_key.clone()
                    } else {
                        format!("{item_key}.{j}")
                    };
                    nodes.extend(frame.eval_node_ir(node, Some(&node_key))?);
                }
                nodes
            };
            out.extend(rendered);
        }
        Ok(out)
    }

    fn eval_match(
        &self,
        m: &ir::MatchIr,
        key_override: Option<&str>,
    ) -> Result<Vec<Node>, EvalError> {
        let (variant, binding_value): (String, Option<Value>) = match &m.source {
            ir::MatchSourceIr::Availability { projection, key } => {
                let key = match key {
                    Some(k) => Some(self.value(k)?),
                    None => None,
                };
                let instance = (projection.clone(), key);
                if let Some(snapshot) = self.x.snapshots.get(&instance) {
                    ("ready".into(), Some(snapshot.value.clone()))
                } else if let Some(reason) = self.x.failed.get(&instance) {
                    ("failed".into(), Some(Value::Text(reason.clone())))
                } else {
                    ("loading".into(), None)
                }
            }
            ir::MatchSourceIr::Union { value } => {
                let v = self.value(value)?;
                let Value::Record(entries) = &v else {
                    return Err(EvalError(format!("match on non-union {v:?}")));
                };
                let Some((variant, payload)) = entries.iter().next() else {
                    return Err(EvalError("empty union value".into()));
                };
                if entries.len() != 1 {
                    return Err(EvalError("union value with multiple variants".into()));
                }
                (variant.as_str().to_string(), Some(payload.clone()))
            }
        };

        let arm = m
            .arms
            .iter()
            .find(|arm| arm.variant.as_ref().is_some_and(|v| v.as_str() == variant))
            .or_else(|| m.arms.iter().find(|arm| arm.variant.is_none()));
        let Some(arm) = arm else {
            return Err(EvalError(format!("no arm for `{variant}`")));
        };

        let mut frame = self.child_frame();
        if let (Some(binding), Some(value)) = (&arm.binding, binding_value) {
            frame.bindings.push((binding.clone(), value));
        }
        // A match root keeps the call-site key across arms (§4.4).
        let mut out = Vec::new();
        for node in &arm.body {
            out.extend(frame.eval_node_ir(node, key_override)?);
        }
        Ok(out)
    }

    fn eval_element(
        &self,
        el: &ir::ElementIr,
        key_override: Option<&str>,
    ) -> Result<Node, EvalError> {
        let key = key_override
            .map(ToString::to_string)
            .unwrap_or_else(|| el.ord.to_string());

        let class = match &el.class {
            None => None,
            Some(expr) => match self.value(expr)? {
                Value::Text(s) => Some(s),
                other => return Err(EvalError(format!("class was {other:?}"))),
            },
        };

        let prop_kinds = self.program.element_props.get(&el.element);
        let mut props = BTreeMap::new();
        for arg in &el.props {
            let value = self.value(&arg.value)?;
            if value == Value::None {
                continue; // an unset optional prop is absent in V
            }
            let kind = prop_kinds
                .and_then(|kinds| kinds.get(&arg.name))
                .copied()
                .unwrap_or(ir::PropKindIr::Token);
            let v = wrap_prop(kind, value)
                .map_err(|m| EvalError(format!("prop `{}`: {m}", arg.name)))?;
            props.insert(arg.name.clone(), v);
        }

        // <text> content: runs join into one inert plain value.
        if !el.text.is_empty() {
            let mut content = String::new();
            for run in &el.text {
                match run {
                    ir::TextRunIr::Literal(s) => content.push_str(s),
                    ir::TextRunIr::Interp(expr) => match self.value(expr)? {
                        Value::Text(s) => content.push_str(&s),
                        other => {
                            return Err(EvalError(format!("interpolation was {other:?}")));
                        }
                    },
                }
            }
            props.insert(
                Ident::new("content").expect("kebab"),
                VValue::Plain(content),
            );
        }

        let mut on = Vec::new();
        for binding in &el.events {
            let mut payload = serde_json::Map::new();
            for arg in &binding.args {
                payload.insert(arg.name.to_string(), self.value(&arg.value)?.to_json());
            }
            let payload = serde_json::Value::Object(payload);
            let (emit, payload) = match self.emits.resolve(&binding.emit) {
                EmitSink::Dead => continue, // unbound at some call site — dead control
                EmitSink::Rename(name) => (name, payload),
                EmitSink::Fixed(name, prebuilt) => (name, prebuilt),
            };
            let sig = self
                .program
                .element_events
                .get(&el.element)
                .and_then(|events| events.get(&binding.event));
            let Some(sig) = sig else {
                return Err(EvalError(format!(
                    "`{}` has no `{}` event",
                    el.element, binding.event
                )));
            };
            on.push(Descriptor {
                kind: match sig.kind {
                    ir::EventKindIr::Input => DescriptorKind::Input,
                    ir::EventKindIr::Observe => DescriptorKind::Observe,
                },
                event: binding.event.clone(),
                emit,
                scope: self.scope.clone(),
                payload,
                carries: sig
                    .carries
                    .iter()
                    .map(|(f, ty)| {
                        (
                            f.clone(),
                            match ty {
                                ir::CarryTypeIr::Text => "text".to_string(),
                                ir::CarryTypeIr::Bool => "bool".to_string(),
                                ir::CarryTypeIr::Int => "int".to_string(),
                            },
                        )
                    })
                    .collect(),
            });
        }

        let children = self.eval_nodes(&el.children)?;

        Ok(Node {
            key,
            element: el.element.clone(),
            class,
            props,
            children,
            on,
        })
    }

    fn eval_component(
        &self,
        call: &ir::ComponentCallIr,
        key_override: Option<&str>,
    ) -> Result<Node, EvalError> {
        let def = self
            .program
            .components
            .get(&call.component)
            .ok_or_else(|| EvalError(format!("no component `{}`", call.component)))?;

        let mut props = BTreeMap::new();
        for arg in &call.props {
            props.insert(arg.name.clone(), self.value(&arg.value)?);
        }

        // Resolve the component's emits through this call site (§4.4).
        let mut table = BTreeMap::new();
        for binding in &call.emits {
            let sink = match &binding.target {
                ir::EmitTargetIr::Forward => self.emits.resolve(&binding.emit),
                ir::EmitTargetIr::Rebind { event, args } => {
                    let mut payload = serde_json::Map::new();
                    for arg in args {
                        payload.insert(arg.name.to_string(), self.value(&arg.value)?.to_json());
                    }
                    match self.emits.resolve(event) {
                        EmitSink::Dead => EmitSink::Dead,
                        EmitSink::Rename(name) => {
                            EmitSink::Fixed(name, serde_json::Value::Object(payload))
                        }
                        // An outer rebind discards the inner payload too.
                        fixed @ EmitSink::Fixed(..) => fixed,
                    }
                }
            };
            table.insert(binding.emit.clone(), sink);
        }

        let empty_state = BTreeMap::new();
        let frame = Frame {
            program: self.program,
            x: self.x,
            scope: self.scope.clone(),
            state: &empty_state,
            props,
            params: BTreeMap::new(),
            bindings: Vec::new(),
            emits: EmitEnv::Component(table),
        };
        // Expansion is transparent: the root takes the call-site key.
        let call_key = key_override
            .map(ToString::to_string)
            .unwrap_or_else(|| call.ord.to_string());
        let nodes = frame.eval_node_ir(&def.root, Some(&call_key))?;
        single_root(nodes)
    }

    // ── helpers ────────────────────────────────────────────────────────

    fn child_frame(&self) -> Frame<'_> {
        Frame {
            program: self.program,
            x: self.x,
            scope: self.scope.clone(),
            state: self.state,
            props: self.props.clone(),
            params: self.params.clone(),
            bindings: self.bindings.clone(),
            emits: self.emits.clone(),
        }
    }

    /// Expression evaluation in view position: `NotReady` is an invariant
    /// break here (the checker forces availability matches, §9.2).
    fn value(&self, e: &ir::ExprIr) -> Result<Value, EvalError> {
        self.eval_expr(e).map_err(|stop| match stop {
            Stop::NotReady(name) => EvalError(format!(
                "view-position read of undelivered projection `{name}`"
            )),
            Stop::Internal(message) => EvalError(message),
        })
    }
}

impl Stop {
    fn internal<T>(message: String) -> Result<T, Stop> {
        Err(Stop::Internal(message))
    }
}

fn wrap_prop(kind: ir::PropKindIr, value: Value) -> Result<VValue, String> {
    match (kind, value) {
        (ir::PropKindIr::Plain, Value::Text(s)) => Ok(VValue::Plain(s)),
        (ir::PropKindIr::Token, Value::Text(s) | Value::Id(s)) => Ok(VValue::Text(s)),
        (ir::PropKindIr::Bool, Value::Bool(b)) => Ok(VValue::Bool(b)),
        (ir::PropKindIr::Int, Value::Int(i)) => Ok(VValue::Int(i)),
        (ir::PropKindIr::Asset, Value::Id(s) | Value::Text(s)) => Ok(VValue::Image(s)),
        (kind, value) => Err(format!("{kind:?} prop got {value:?}")),
    }
}

/// The canonical key string of an each-key value (§8.1 item keys).
fn key_string(v: &Value) -> Option<String> {
    match v {
        Value::Id(s) | Value::Text(s) => Some(s.clone()),
        Value::Tag(n) => Some(format!("t-{n}")),
        Value::Int(i) => Some(i.to_string()),
        _ => None,
    }
}
