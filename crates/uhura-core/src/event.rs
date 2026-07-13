//! The external event alphabet (design §7.2) and the harness-side
//! projection-store transaction (`apply_updates`, §9.3/§9.4).
//!
//! Ordering contract (§7.2): for `Outcome` and `Projection` events the
//! HARNESS applies the carried updates to `X` — revision-checked —
//! immediately before calling `step_u`, so clear-overlay-on-ok is
//! flicker-free by construction. `step_u` itself never mutates `X`.

use std::collections::BTreeMap;

use uhura_base::{Ident, Value};
use uhura_port::envelope::{OutcomeResult, ProjectionUpdate};

use crate::decode::decode_value;
use crate::ir::ProgramIr;
use crate::state::Projections;
use crate::view::Descriptor;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A renderer emission from a present descriptor. `data` carries the
    /// catalog-declared `carries` fields (`{ value: "…" }`); stale
    /// `view_rev` is ACCEPTED — guards are the backstop (§7.2).
    Ui {
        descriptor: Descriptor,
        data: Option<Value>,
        view_rev: u64,
    },
    /// Exactly one per command, eventually. `updates` are the settlement
    /// piggyback — already applied by the harness when `step_u` runs.
    Outcome {
        correlation: String,
        result: OutcomeResult,
        updates: Vec<ProjectionUpdate>,
    },
    /// Standalone projection deliveries — already applied by the harness.
    Projection { updates: Vec<ProjectionUpdate> },
    ProjectionFailed {
        port: Ident,
        projection: Ident,
        key: Option<serde_json::Value>,
        reason: String,
    },
    /// Boots the machine onto a route. Boot projections are delivered
    /// before this (§9.2), so bare reads are legal from the first view.
    Init {
        route: Ident,
        params: BTreeMap<Ident, Value>,
    },
}

impl Event {
    /// The stable wire/trace form (hand-written — micro-decision #14).
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Event::Ui {
                descriptor,
                data,
                view_rev,
            } => {
                let mut obj = serde_json::json!({
                    "kind": "ui",
                    "descriptor": descriptor.to_json(),
                    "view-rev": view_rev,
                });
                if let Some(data) = data {
                    obj["data"] = data.to_json();
                }
                obj
            }
            Event::Outcome {
                correlation,
                result,
                updates,
            } => serde_json::json!({
                "kind": "outcome",
                "correlation": correlation,
                "outcome": result.to_json(),
                "updates": updates.iter().map(ProjectionUpdate::to_json).collect::<Vec<_>>(),
            }),
            Event::Projection { updates } => serde_json::json!({
                "kind": "projection",
                "updates": updates.iter().map(ProjectionUpdate::to_json).collect::<Vec<_>>(),
            }),
            Event::ProjectionFailed {
                port,
                projection,
                key,
                reason,
            } => serde_json::json!({
                "kind": "projection-failed",
                "port": port.to_string(),
                "projection": projection.to_string(),
                "key": key,
                "reason": reason,
            }),
            Event::Init { route, params } => serde_json::json!({
                "kind": "init",
                "route": route.to_string(),
                "params": params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_json()))
                    .collect::<serde_json::Map<_, _>>(),
            }),
        }
    }

    /// Parses the wire form (the §12.3 ABI's `dispatch(event_json)` input).
    /// `ui` carried data is typed by the descriptor's declared carries —
    /// the renderer trust boundary (§4.2). Wire `init` params decode
    /// strings as ids (route params are entity references — §3), bools and
    /// integers as themselves (plan micro-decision #56).
    pub fn from_json(json: &serde_json::Value) -> Result<Event, String> {
        let kind = json
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or("an event needs a `kind`")?;
        let str_field = |field: &str| -> Result<String, String> {
            json.get(field)
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .ok_or_else(|| format!("a `{kind}` event needs a text `{field}`"))
        };
        let updates_field = || -> Result<Vec<ProjectionUpdate>, String> {
            match json.get("updates") {
                None => Ok(Vec::new()),
                Some(serde_json::Value::Array(items)) => items
                    .iter()
                    .map(ProjectionUpdate::from_json)
                    .collect::<Result<Vec<_>, _>>(),
                Some(_) => Err("`updates` must be a list".into()),
            }
        };
        match kind {
            "ui" => {
                let descriptor = Descriptor::from_json(
                    json.get("descriptor")
                        .ok_or("a `ui` event needs a `descriptor`")?,
                )?;
                let view_rev = json
                    .get("view-rev")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or("a `ui` event needs an integer `view-rev`")?;
                let data = match json.get("data") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::Object(map)) => decode_carried_data(&descriptor, map)?,
                    Some(_) => return Err("`data` must be an object".into()),
                };
                Ok(Event::Ui {
                    descriptor,
                    data,
                    view_rev,
                })
            }
            "outcome" => Ok(Event::Outcome {
                correlation: str_field("correlation")?,
                result: OutcomeResult::from_json(
                    json.get("outcome")
                        .ok_or("an `outcome` event needs an `outcome`")?,
                )?,
                updates: updates_field()?,
            }),
            "projection" => Ok(Event::Projection {
                updates: updates_field()?,
            }),
            "projection-failed" => Ok(Event::ProjectionFailed {
                port: Ident::new(&str_field("port")?).map_err(|e| e.to_string())?,
                projection: Ident::new(&str_field("projection")?).map_err(|e| e.to_string())?,
                key: match json.get("key") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(k) => Some(k.clone()),
                },
                reason: str_field("reason")?,
            }),
            "init" => {
                let mut params = BTreeMap::new();
                match json.get("params") {
                    None => {}
                    Some(serde_json::Value::Object(map)) => {
                        for (name, value) in map {
                            let name = Ident::new(name).map_err(|e| e.to_string())?;
                            let value = match value {
                                serde_json::Value::String(s) => Value::Id(s.clone()),
                                serde_json::Value::Bool(b) => Value::Bool(*b),
                                serde_json::Value::Number(n) => Value::Int(
                                    n.as_i64()
                                        .ok_or_else(|| format!("param `{name}` is not an i64"))?,
                                ),
                                other => {
                                    return Err(format!(
                                        "param `{name}` must be a string, bool, or integer, \
                                         got {other}"
                                    ));
                                }
                            };
                            params.insert(name, value);
                        }
                    }
                    Some(_) => return Err("`params` must be an object".into()),
                }
                Ok(Event::Init {
                    route: Ident::new(&str_field("route")?).map_err(|e| e.to_string())?,
                    params,
                })
            }
            other => Err(format!("`{other}` is not an event kind")),
        }
    }
}

/// Types renderer-carried fields by the descriptor's declared carries
/// (`text | bool | int` — §4.2). Both the trace harness's `[[ui]]`
/// stimuli and the wire `ui` event decode through here, so the headless
/// pump and the play shell share one trust boundary.
pub fn decode_carried_data(
    descriptor: &Descriptor,
    data: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<Value>, String> {
    if data.is_empty() {
        return Ok(None);
    }
    let mut record = BTreeMap::new();
    for (field, value) in data {
        let field = Ident::new(field).map_err(|e| e.to_string())?;
        let Some(carry) = descriptor.carries.get(&field) else {
            return Err(format!(
                "`{}` does not carry `{field}` (declared carries: {:?})",
                descriptor.event,
                descriptor.carries.keys().collect::<Vec<_>>()
            ));
        };
        let typed = match (carry.as_str(), value) {
            ("text", serde_json::Value::String(s)) => Value::Text(s.clone()),
            ("bool", serde_json::Value::Bool(b)) => Value::Bool(*b),
            ("int", serde_json::Value::Number(n)) => Value::Int(
                n.as_i64()
                    .ok_or_else(|| format!("`{field}` is not an i64"))?,
            ),
            (expected, got) => {
                return Err(format!("`{field}` carries {expected}, got {got}"));
            }
        };
        record.insert(field, typed);
    }
    Ok(Some(Value::Record(record)))
}

/// What one update did to `X` — recorded in the step's trace line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyNote {
    Applied {
        projection: Ident,
        key: Option<Value>,
        revision: u64,
    },
    /// Revisions strictly increase per (projection, key); stale deliveries
    /// drop with a diagnosis, never an error (§9.3).
    DroppedStale {
        projection: Ident,
        key: Option<Value>,
        revision: u64,
        current: u64,
    },
    Failed {
        projection: Ident,
        key: Option<Value>,
        reason: String,
    },
}

impl ApplyNote {
    pub fn to_json(&self) -> serde_json::Value {
        let (kind, projection, key, extra) = match self {
            ApplyNote::Applied {
                projection,
                key,
                revision,
            } => (
                "applied",
                projection,
                key,
                serde_json::json!({ "revision": revision }),
            ),
            ApplyNote::DroppedStale {
                projection,
                key,
                revision,
                current,
            } => (
                "dropped-stale",
                projection,
                key,
                serde_json::json!({ "revision": revision, "current": current }),
            ),
            ApplyNote::Failed {
                projection,
                key,
                reason,
            } => (
                "failed",
                projection,
                key,
                serde_json::json!({ "reason": reason }),
            ),
        };
        let mut obj = extra;
        obj["apply"] = serde_json::Value::String(kind.into());
        obj["projection"] = serde_json::Value::String(projection.to_string());
        if let Some(key) = key {
            obj["key"] = key.to_json();
        }
        obj
    }
}

/// Applies provider updates to the projection store — the harness calls
/// this immediately before `step_u` for `Outcome`/`Projection` events and
/// for boot deliveries (§9.2). Value and key JSON decode against the IR's
/// baked-in types; a decode failure is a provider-conformance error, not a
/// drop.
pub fn apply_updates(
    p: &ProgramIr,
    x: &mut Projections,
    updates: &[ProjectionUpdate],
) -> Result<Vec<ApplyNote>, String> {
    let mut notes = Vec::new();
    for update in updates {
        let decl = p.projections.get(&update.projection).ok_or_else(|| {
            format!(
                "`{}` is not a projection of this app's ports",
                update.projection
            )
        })?;
        if decl.port != update.port {
            return Err(format!(
                "projection `{}` belongs to port `{}`, not `{}`",
                update.projection, decl.port, update.port
            ));
        }
        let key = decode_key(decl.key.as_ref(), update.key.as_ref(), &update.projection)?;
        let value = decode_value(&update.value, &decl.ty)
            .map_err(|e| format!("`{}` value: {e}", update.projection))?;
        let instance = (update.projection.clone(), key.clone());
        if let Some(current) = x.snapshots.get(&instance)
            && update.revision <= current.revision
        {
            notes.push(ApplyNote::DroppedStale {
                projection: update.projection.clone(),
                key,
                revision: update.revision,
                current: current.revision,
            });
            continue;
        }
        // A fresh snapshot supersedes any recorded failure (session truth).
        x.failed.remove(&instance);
        x.snapshots.insert(
            instance,
            crate::state::ProjectionSnapshot {
                revision: update.revision,
                value,
            },
        );
        notes.push(ApplyNote::Applied {
            projection: update.projection.clone(),
            key,
            revision: update.revision,
        });
    }
    Ok(notes)
}

/// Marks a projection instance failed (§9.2 session truth): the snapshot
/// is removed so availability matches render the `failed` arm. Port
/// conformance is checked exactly as `apply_updates` does — the wire is
/// not trusted to route.
pub fn apply_failure(
    p: &ProgramIr,
    x: &mut Projections,
    port: &Ident,
    projection: &Ident,
    key: Option<&serde_json::Value>,
    reason: &str,
) -> Result<ApplyNote, String> {
    let decl = p
        .projections
        .get(projection)
        .ok_or_else(|| format!("`{projection}` is not a projection of this app's ports"))?;
    if decl.port != *port {
        return Err(format!(
            "projection `{projection}` belongs to port `{}`, not `{port}`",
            decl.port
        ));
    }
    let key = decode_key(decl.key.as_ref(), key, projection)?;
    let instance = (projection.clone(), key.clone());
    x.snapshots.remove(&instance);
    x.failed.insert(instance, reason.to_string());
    Ok(ApplyNote::Failed {
        projection: projection.clone(),
        key,
        reason: reason.to_string(),
    })
}

fn decode_key(
    key_ty: Option<&crate::ir::TyIr>,
    key: Option<&serde_json::Value>,
    projection: &Ident,
) -> Result<Option<Value>, String> {
    match (key_ty, key) {
        (None, None) => Ok(None),
        (Some(ty), Some(json)) => decode_value(json, ty)
            .map(Some)
            .map_err(|e| format!("`{projection}` key: {e}")),
        (Some(_), None) => Err(format!("`{projection}` is keyed; the update names no key")),
        (None, Some(_)) => Err(format!("`{projection}` is unkeyed; the update names a key")),
    }
}
