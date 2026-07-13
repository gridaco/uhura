//! The tick-based driver (design §9.5): standalone deliveries are scheduled
//! at `new()` on absolute ticks, reply outcomes at (deliver tick +
//! after-ticks). Reply slices resolve BOTH substitutions in one pass at
//! `deliver()` — payload-echo values are spliced verbatim and never
//! re-walked, so wire data can never be mistaken for an authored
//! `@fresh-id` marker. Standalone slices (validated payload-marker-free at
//! `new()`) mint their fresh ids at emission. Revisions always mint at
//! emission (§9.3 strict increase).

use std::collections::BTreeMap;

use uhura_base::{Ident, to_canonical_json};
use uhura_port::{OutcomeEnvelope, OutcomeResult, ProjectionUpdate, ProviderMsg};

use crate::script::{self, DeliverBody, Fixture, KeySpec, ReplyEntry};

#[derive(Debug)]
pub struct FixtureDriver {
    replies: Vec<ReplyEntry>,
    /// Insertion-ordered: standalone entries land here at `new()`, so they
    /// precede same-tick reply outcomes.
    scheduled: Vec<Scheduled>,
    tick: u64,
    /// Per-(projection, canonical key) revision, starting at 1 — revision 1
    /// is reserved for the harness's boot deliveries, so the first driver
    /// delivery of an instance is revision 2.
    revisions: BTreeMap<(String, String), u64>,
    /// 1-based `@fresh-id` mint counter — advanced in resolution order
    /// (reply slices at `deliver()`, standalone slices at emission).
    fresh: u64,
}

#[derive(Debug)]
struct Scheduled {
    due: u64,
    item: Pending,
}

#[derive(Debug)]
enum Pending {
    Projection {
        port: Ident,
        projection: Ident,
        key: Option<serde_json::Value>,
        slice: serde_json::Value,
    },
    ProjectionFailed {
        port: Ident,
        projection: Ident,
        key: Option<serde_json::Value>,
        reason: String,
    },
    Outcome {
        correlation: String,
        outcome: OutcomeResult,
        /// Fully resolved at deliver time (payload echo AND fresh ids);
        /// only revisions remain for emission.
        updates: Vec<PendingUpdate>,
    },
}

#[derive(Debug)]
struct PendingUpdate {
    port: Ident,
    projection: Ident,
    key: Option<serde_json::Value>,
    slice: serde_json::Value,
}

impl FixtureDriver {
    /// `fixture_json`: `{"<ns>": {"<name>": <value>}}` — pre-resolved slices,
    /// possibly carrying the `"@fresh-id"` / `"@payload.<field>"` markers.
    /// `script_json`: the §9.5 script object, parsed strictly.
    pub fn new(fixture_json: &str, script_json: &str) -> Result<FixtureDriver, String> {
        let fixture = parse_fixture(fixture_json)?;
        let script_value: serde_json::Value =
            serde_json::from_str(script_json).map_err(|e| format!("invalid script JSON: {e}"))?;
        reject_floats(&script_value, "script data")?;
        let parsed = script::parse_script(&script_value, &fixture)?;

        let scheduled = parsed
            .deliver
            .into_iter()
            .map(|entry| Scheduled {
                due: entry.after_ticks,
                item: match entry.body {
                    DeliverBody::Slice(slice) => Pending::Projection {
                        port: entry.port,
                        projection: entry.projection,
                        key: entry.key,
                        slice,
                    },
                    DeliverBody::Failed(reason) => Pending::ProjectionFailed {
                        port: entry.port,
                        projection: entry.projection,
                        key: entry.key,
                        reason,
                    },
                },
            })
            .collect();

        Ok(FixtureDriver {
            replies: parsed.replies,
            scheduled,
            tick: 0,
            revisions: BTreeMap::new(),
            fresh: 0,
        })
    }

    /// A command envelope arrives. Matches the first unconsumed reply entry
    /// in file order whose `on.command` equals the envelope's command and
    /// whose every `where` field is JSON-equal to the same-named payload
    /// field; no match is `on-unscripted = "error"` (§9.5 — a duplicate
    /// in-flight command IS the dedupe assertion).
    pub fn deliver(&mut self, cmd_json: &str) -> Result<(), String> {
        let value: serde_json::Value =
            serde_json::from_str(cmd_json).map_err(|e| format!("invalid command JSON: {e}"))?;
        let ProviderMsg::Command(cmd) = ProviderMsg::from_json(&value)? else {
            return Err("deliver() takes a `kind = \"command\"` envelope".into());
        };
        reject_floats(&cmd.payload, "command payloads")?;

        let index = self
            .replies
            .iter()
            .position(|entry| {
                !entry.consumed
                    && entry.command == cmd.command
                    && entry
                        .where_fields
                        .iter()
                        .all(|(field, expected)| cmd.payload.get(field) == Some(expected))
            })
            .ok_or_else(|| {
                format!(
                    "unscripted command `{}` (on-unscripted = \"error\", §9.5)",
                    cmd.command
                )
            })?;

        // Resolve everything payload-dependent NOW, before consuming — a
        // missing payload field fails this deliver() and consumes nothing.
        // Fresh ids mint in the same single pass so spliced payload data
        // is never re-interpreted as a marker.
        let entry = &self.replies[index];
        let mut fresh = self.fresh;
        let mut updates = Vec::with_capacity(entry.updates.len());
        for spec in &entry.updates {
            let key = match &spec.key {
                KeySpec::Absent => None,
                KeySpec::Literal(literal) => Some(literal.clone()),
                KeySpec::FromPayload(field) => {
                    Some(cmd.payload.get(field).cloned().ok_or_else(|| {
                        format!("the payload has no `{field}` field for the update key")
                    })?)
                }
            };
            updates.push(PendingUpdate {
                port: spec.port.clone(),
                projection: spec.projection.clone(),
                key,
                slice: resolve_markers(&spec.slice, &cmd.payload, &mut fresh)?,
            });
        }
        self.fresh = fresh;
        let outcome = entry.outcome.clone();
        let due = self.tick + entry.after_ticks;
        if !entry.repeat {
            self.replies[index].consumed = true;
        }
        self.scheduled.push(Scheduled {
            due,
            item: Pending::Outcome {
                correlation: cmd.correlation,
                outcome,
                updates,
            },
        });
        Ok(())
    }

    /// Advances one tick and returns the provider messages that come due, in
    /// schedule order, as canonical JSON strings. Messages are built HERE so
    /// revision numbers and fresh-id mints are ordered by emission.
    pub fn tick(&mut self) -> Vec<String> {
        self.tick += 1;
        let scheduled = std::mem::take(&mut self.scheduled);
        let mut out = Vec::new();
        for entry in scheduled {
            if entry.due == self.tick {
                let msg = self.emit(entry.item);
                out.push(to_canonical_json(&msg.to_json()));
            } else {
                self.scheduled.push(entry);
            }
        }
        out
    }

    /// Nothing scheduled. Unconsumed one-shot reply entries do NOT count —
    /// they may never fire.
    pub fn idle(&self) -> bool {
        self.scheduled.is_empty()
    }

    fn emit(&mut self, item: Pending) -> ProviderMsg {
        match item {
            Pending::Projection {
                port,
                projection,
                key,
                slice,
            } => {
                let value = self.mint_fresh(&slice);
                let revision = self.next_revision(&projection, &key);
                ProviderMsg::Projection(ProjectionUpdate {
                    port,
                    projection,
                    key,
                    revision,
                    value,
                })
            }
            Pending::ProjectionFailed {
                port,
                projection,
                key,
                reason,
            } => ProviderMsg::ProjectionFailed {
                port,
                projection,
                key,
                reason,
            },
            Pending::Outcome {
                correlation,
                outcome,
                updates,
            } => {
                let updates = updates
                    .into_iter()
                    .map(|u| {
                        let revision = self.next_revision(&u.projection, &u.key);
                        ProjectionUpdate {
                            port: u.port,
                            projection: u.projection,
                            key: u.key,
                            revision,
                            value: u.slice,
                        }
                    })
                    .collect();
                ProviderMsg::Outcome(OutcomeEnvelope {
                    correlation,
                    outcome,
                    updates,
                })
            }
        }
    }

    /// Increments first, so the first driver mint for an instance is 2 —
    /// strictly increasing per (projection, canonical key), never colliding
    /// with the harness's boot revision 1 (§9.3).
    fn next_revision(&mut self, projection: &Ident, key: &Option<serde_json::Value>) -> u64 {
        let canonical_key = match key {
            None => "null".to_string(),
            Some(k) => to_canonical_json(k),
        };
        let counter = self
            .revisions
            .entry((projection.to_string(), canonical_key))
            .or_insert(1);
        *counter += 1;
        *counter
    }

    /// Deep-walks a value replacing whole-string `"@fresh-id"` markers with
    /// `fresh-<n>`, minted in pre-order walk order (§9.5 — object fields
    /// walk in canonical key order; serde_json maps are BTree-backed).
    fn mint_fresh(&mut self, value: &serde_json::Value) -> serde_json::Value {
        use serde_json::Value as J;
        match value {
            J::String(s) if s == "@fresh-id" => {
                self.fresh += 1;
                J::String(format!("fresh-{}", self.fresh))
            }
            J::Array(items) => J::Array(items.iter().map(|v| self.mint_fresh(v)).collect()),
            J::Object(map) => J::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), self.mint_fresh(v)))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
}

/// Deep-walks an AUTHORED reply slice in one pass, resolving the only two
/// substitutions (§9.5): whole-string `"@payload.<field>"` markers splice
/// the matched payload's field value VERBATIM (spliced data is not
/// re-walked — wire data equal to a marker string stays untouched), and
/// whole-string `"@fresh-id"` markers mint `fresh-<n>` in pre-order walk
/// order. A missing payload field fails the enclosing `deliver()`.
fn resolve_markers(
    value: &serde_json::Value,
    payload: &serde_json::Value,
    fresh: &mut u64,
) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    match value {
        J::String(s) if s == "@fresh-id" => {
            *fresh += 1;
            Ok(J::String(format!("fresh-{fresh}")))
        }
        J::String(s) => match script::payload_marker(s) {
            Some(field) => payload.get(field).cloned().ok_or_else(|| {
                format!("the payload has no `{field}` field for `@payload.{field}`")
            }),
            None => Ok(value.clone()),
        },
        J::Array(items) => items
            .iter()
            .map(|item| resolve_markers(item, payload, fresh))
            .collect::<Result<Vec<_>, _>>()
            .map(J::Array),
        J::Object(map) => map
            .iter()
            .map(|(k, v)| Ok((k.clone(), resolve_markers(v, payload, fresh)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(J::Object),
        _ => Ok(value.clone()),
    }
}

fn parse_fixture(text: &str) -> Result<Fixture, String> {
    use serde_json::Value as J;
    let value: J = serde_json::from_str(text).map_err(|e| format!("invalid fixture JSON: {e}"))?;
    reject_floats(&value, "fixture data")?;
    let J::Object(namespaces) = value else {
        return Err("fixture data is `{\"<ns>\": {\"<name>\": <value>}}`".into());
    };
    let mut out = Fixture::new();
    for (ns, slices) in namespaces {
        let J::Object(slices) = slices else {
            return Err(format!(
                "fixture namespace `{ns}` must be an object of slices"
            ));
        };
        out.insert(ns, slices.into_iter().collect());
    }
    Ok(out)
}

/// Determinism guard: the value model has no floats (§7.5) — refuse them at
/// the boundary rather than reaching canonical JSON with one.
fn reject_floats(value: &serde_json::Value, what: &str) -> Result<(), String> {
    use serde_json::Value as J;
    match value {
        J::Number(n) if !(n.is_i64() || n.is_u64()) => {
            Err(format!("floats do not exist in {what} (§7.5): {n}"))
        }
        J::Array(items) => items.iter().try_for_each(|v| reject_floats(v, what)),
        J::Object(map) => map.values().try_for_each(|v| reject_floats(v, what)),
        _ => Ok(()),
    }
}
