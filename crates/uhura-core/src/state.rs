//! Machine state — owned, ordered data only (design §7.1). `step_u`
//! arrives with M4; M3 builds these values from resolved examples and
//! evaluates views over them.

use std::collections::BTreeMap;

use uhura_base::{Ident, Value};

use crate::ir;

/// Projections are ABSENT until delivered; keyed instances (§7.1/§9.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Projections {
    pub snapshots: BTreeMap<(Ident, Option<Value>), ProjectionSnapshot>,
    pub failed: BTreeMap<(Ident, Option<Value>), String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionSnapshot {
    /// Strictly increasing per (projection, key); stale deliveries drop.
    pub revision: u64,
    pub value: Value,
}

impl Projections {
    /// Tooling-facing canonical JSON for the complete external projection
    /// store. Composite `(projection, key)` identities are encoded as ordered
    /// entries instead of object keys so keyed instances remain lossless.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "snapshots": self.snapshots.iter().map(|((projection, key), snapshot)| {
                serde_json::json!({
                    "projection": projection.to_string(),
                    "key": key.as_ref().map(Value::to_json),
                    "revision": snapshot.revision,
                    "value": snapshot.value.to_json(),
                })
            }).collect::<Vec<_>>(),
            "failed": self.failed.iter().map(|((projection, key), reason)| {
                serde_json::json!({
                    "projection": projection.to_string(),
                    "key": key.as_ref().map(Value::to_json),
                    "reason": reason,
                })
            }).collect::<Vec<_>>(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiState {
    /// `+1` every step, always.
    pub rev: u64,
    /// Bottom → top; the top entry is the current page.
    pub nav: Vec<NavEntry>,
    /// Bottom → top.
    pub surfaces: Vec<SurfaceState>,
    /// Correlation tag → in-flight command (payload echo for outcome
    /// handlers; origin scope for dispatch).
    pub pending: BTreeMap<u64, PendingCommand>,
    /// All identity — replay mints identical ids (§7.1).
    pub counters: Counters,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavEntry {
    /// Minted page serial — the `"page:<n>"` scope.
    pub serial: u64,
    pub route: Ident,
    pub params: BTreeMap<Ident, Value>,
    pub state: BTreeMap<Ident, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceState {
    /// Minted surface serial — the `"surface:<n>"` scope.
    pub serial: u64,
    pub definition: Ident,
    /// The canonical open context (evaluated `open-surface` args = props).
    pub props: BTreeMap<Ident, Value>,
    pub state: BTreeMap<Ident, Value>,
    /// The scope that opened this instance.
    pub opener: String,
    /// Key-path of the triggering node, for FocusRestore (§4.2).
    pub restore_focus: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCommand {
    pub port: Ident,
    pub command: Ident,
    /// The echoed payload (`cmd` in outcome handlers — §4.2).
    pub payload: Value,
    /// Origin scope (`"page:1"` / `"surface:2"`).
    pub origin: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Next command tag (`t-<n>` / wire `c-<n>`).
    pub tag: u64,
    /// Next page serial.
    pub page_serial: u64,
    /// Next surface serial.
    pub surface_serial: u64,
}

impl Counters {
    // Mints are 1-based (`t-1`, `page:1`, `surface:1` — §8.1's examples).
    pub fn mint_tag(&mut self) -> u64 {
        self.tag += 1;
        self.tag
    }

    pub fn mint_page(&mut self) -> u64 {
        self.page_serial += 1;
        self.page_serial
    }

    pub fn mint_surface(&mut self) -> u64 {
        self.surface_serial += 1;
        self.surface_serial
    }
}

impl UiState {
    /// The machine boots empty: no page until `Init` mounts one (§9.2 —
    /// boot projections are delivered before the first event).
    pub fn boot() -> UiState {
        UiState {
            rev: 0,
            nav: Vec::new(),
            surfaces: Vec::new(),
            pending: BTreeMap::new(),
            counters: Counters::default(),
        }
    }

    /// Canonical JSON of the logical machine configuration, excluding only
    /// the per-step revision. This is tooling identity, not a replacement for
    /// `u-hash`: counters and pending correlations remain semantically visible.
    pub fn configuration_json(&self) -> serde_json::Value {
        let fields = |map: &BTreeMap<Ident, Value>| -> serde_json::Value {
            map.iter()
                .map(|(k, v)| (k.to_string(), v.to_json()))
                .collect::<serde_json::Map<_, _>>()
                .into()
        };
        serde_json::json!({
            "nav": self.nav.iter().map(|entry| serde_json::json!({
                "serial": entry.serial,
                "route": entry.route.to_string(),
                "params": fields(&entry.params),
                "state": fields(&entry.state),
            })).collect::<Vec<_>>(),
            "surfaces": self.surfaces.iter().map(|s| {
                let mut obj = serde_json::json!({
                    "serial": s.serial,
                    "definition": s.definition.to_string(),
                    "props": fields(&s.props),
                    "state": fields(&s.state),
                    "opener": s.opener,
                });
                if let Some(rf) = &s.restore_focus {
                    obj["restore-focus"] = serde_json::Value::String(rf.clone());
                }
                obj
            }).collect::<Vec<_>>(),
            "pending": self.pending.iter().map(|(tag, cmd)| {
                (format!("t-{tag}"), serde_json::json!({
                    "port": cmd.port.to_string(),
                    "command": cmd.command.to_string(),
                    "payload": cmd.payload.to_json(),
                    "origin": cmd.origin,
                }))
            }).collect::<serde_json::Map<_, _>>(),
            "counters": {
                "tag": self.counters.tag,
                "page-serial": self.counters.page_serial,
                "surface-serial": self.counters.surface_serial,
            },
        })
    }

    /// Canonical JSON of the whole machine state — the `u-hash` input
    /// (§7.5). Pending keys render as their minted tag form.
    pub fn to_json(&self) -> serde_json::Value {
        let mut state = self.configuration_json();
        state["rev"] = self.rev.into();
        state
    }

    /// Revision-independent identity for inspection and visualization tools.
    pub fn configuration_hash(&self) -> String {
        uhura_base::hash_json(&self.configuration_json())
    }

    /// SHA-256 of the canonical machine state (§7.5).
    pub fn u_hash(&self) -> String {
        uhura_base::hash_json(&self.to_json())
    }
}

/// The initial state record of a definition (init literals realized).
pub fn initial_state(def: &ir::DefIr) -> BTreeMap<Ident, Value> {
    def.state
        .iter()
        .map(|(name, init)| (name.clone(), init_value(init)))
        .collect()
}

pub fn init_value(init: &ir::InitValue) -> Value {
    match init {
        ir::InitValue::Int(i) => Value::Int(*i),
        ir::InitValue::Text(s) => Value::Text(s.clone()),
        ir::InitValue::Bool(b) => Value::Bool(*b),
        ir::InitValue::None => Value::None,
        ir::InitValue::EmptyMap => Value::Map(BTreeMap::new()),
    }
}

/// The canonical map-key string of a key value: ids keep their text,
/// tags render `"t-<n>"` (`Value::Map` is keyed by this string — IR
/// micro-decision; keys are NOT identifiers, so external ids such as
/// UUIDs are valid; entity-id shapes are linted, §6.2).
pub fn map_key_string(key: &Value) -> Option<String> {
    match key {
        Value::Id(s) | Value::Text(s) => Some(s.clone()),
        Value::Tag(n) => Some(format!("t-{n}")),
        _ => None,
    }
}
