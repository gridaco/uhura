//! The `uhura-provider/0` wire messages (design §9.3) — hand-written stable
//! JSON shapes (plan micro-decision #14), never serde-tagged enums.
//!
//! Payload/value fields cross the boundary as raw canonical JSON: the typed
//! side (core building a command, the harness applying an update) converts
//! at its edge via `Value::to_json` / `PortContract::decode_value`.

use uhura_base::Ident;

/// Asserted at shell boot (§12.3); messages themselves carry no protocol
/// field.
pub const PROVIDER_PROTOCOL: &str = "uhura-provider/0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderMsg {
    Command(CommandEnvelope),
    Projection(ProjectionUpdate),
    ProjectionFailed {
        port: Ident,
        projection: Ident,
        key: Option<serde_json::Value>,
        reason: String,
    },
    Outcome(OutcomeEnvelope),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandEnvelope {
    pub port: Ident,
    pub command: Ident,
    /// Core-minted, opaque, echoed verbatim (`"c-<n>"`).
    pub correlation: String,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionUpdate {
    pub port: Ident,
    pub projection: Ident,
    pub key: Option<serde_json::Value>,
    /// Strictly increasing per (projection, key); stale deliveries are
    /// dropped with a diagnostic (§9.3).
    pub revision: u64,
    pub value: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeEnvelope {
    pub correlation: String,
    pub outcome: OutcomeResult,
    /// Settlement updates, applied atomically before the outcome
    /// dispatches (§9.4).
    pub updates: Vec<ProjectionUpdate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeResult {
    Ok,
    Refused {
        refusal: Ident,
    },
    /// The implicit extension of every outcome union; routes to `.err`.
    Unavailable {
        reason: String,
    },
}

impl ProjectionUpdate {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "port": self.port.to_string(),
            "projection": self.projection.to_string(),
            "key": self.key,
            "revision": self.revision,
            "value": self.value,
        })
    }

    pub fn from_json(json: &serde_json::Value) -> Result<Self, String> {
        Ok(ProjectionUpdate {
            port: ident_field(json, "port")?,
            projection: ident_field(json, "projection")?,
            key: key_field(json)?,
            revision: json
                .get("revision")
                .and_then(serde_json::Value::as_u64)
                .ok_or("`revision` must be a non-negative integer")?,
            value: json.get("value").cloned().ok_or("missing `value`")?,
        })
    }
}

impl OutcomeResult {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            OutcomeResult::Ok => serde_json::json!({ "ok": {} }),
            OutcomeResult::Refused { refusal } => serde_json::json!({
                "refused": { "refusal": refusal.to_string() }
            }),
            OutcomeResult::Unavailable { reason } => serde_json::json!({
                "unavailable": { "reason": reason }
            }),
        }
    }

    pub fn from_json(json: &serde_json::Value) -> Result<Self, String> {
        let obj = json
            .as_object()
            .filter(|o| o.len() == 1)
            .ok_or("`outcome` must be a single-variant object")?;
        let (variant, body) = obj.iter().next().expect("len checked");
        match variant.as_str() {
            "ok" => Ok(OutcomeResult::Ok),
            "refused" => {
                let refusal = body
                    .get("refusal")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("`refused` needs a `refusal` name")?;
                Ok(OutcomeResult::Refused {
                    refusal: Ident::new(refusal).map_err(|e| e.to_string())?,
                })
            }
            "unavailable" => {
                let reason = body
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("`unavailable` needs a `reason` text")?;
                Ok(OutcomeResult::Unavailable {
                    reason: reason.to_string(),
                })
            }
            other => Err(format!("`{other}` is not an outcome variant")),
        }
    }
}

impl ProviderMsg {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ProviderMsg::Command(c) => serde_json::json!({
                "kind": "command",
                "port": c.port.to_string(),
                "command": c.command.to_string(),
                "correlation": c.correlation,
                "payload": c.payload,
            }),
            ProviderMsg::Projection(u) => {
                let mut json = u.to_json();
                json["kind"] = serde_json::Value::String("projection".into());
                json
            }
            ProviderMsg::ProjectionFailed {
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
            ProviderMsg::Outcome(o) => serde_json::json!({
                "kind": "outcome",
                "correlation": o.correlation,
                "outcome": o.outcome.to_json(),
                "updates": o.updates.iter().map(ProjectionUpdate::to_json).collect::<Vec<_>>(),
            }),
        }
    }

    pub fn from_json(json: &serde_json::Value) -> Result<Self, String> {
        let kind = json
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or("a provider message needs a `kind`")?;
        match kind {
            "command" => Ok(ProviderMsg::Command(CommandEnvelope {
                port: ident_field(json, "port")?,
                command: ident_field(json, "command")?,
                correlation: str_field(json, "correlation")?,
                payload: json.get("payload").cloned().ok_or("missing `payload`")?,
            })),
            "projection" => Ok(ProviderMsg::Projection(ProjectionUpdate::from_json(json)?)),
            "projection-failed" => Ok(ProviderMsg::ProjectionFailed {
                port: ident_field(json, "port")?,
                projection: ident_field(json, "projection")?,
                key: key_field(json)?,
                reason: str_field(json, "reason")?,
            }),
            "outcome" => {
                let updates = match json.get("updates") {
                    None => Vec::new(),
                    Some(serde_json::Value::Array(items)) => items
                        .iter()
                        .map(ProjectionUpdate::from_json)
                        .collect::<Result<Vec<_>, _>>()?,
                    Some(_) => return Err("`updates` must be a list".to_string()),
                };
                Ok(ProviderMsg::Outcome(OutcomeEnvelope {
                    correlation: str_field(json, "correlation")?,
                    outcome: OutcomeResult::from_json(
                        json.get("outcome").ok_or("missing `outcome`")?,
                    )?,
                    updates,
                }))
            }
            other => Err(format!("`{other}` is not a provider message kind")),
        }
    }
}

fn str_field(json: &serde_json::Value, field: &str) -> Result<String, String> {
    json.get(field)
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing or non-text `{field}`"))
}

fn ident_field(json: &serde_json::Value, field: &str) -> Result<Ident, String> {
    Ident::new(&str_field(json, field)?).map_err(|e| e.to_string())
}

fn key_field(json: &serde_json::Value) -> Result<Option<serde_json::Value>, String> {
    match json.get("key") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(k) => Ok(Some(k.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(s: &str) -> Ident {
        Ident::new(s).unwrap()
    }

    #[test]
    fn round_trips_every_kind() {
        let msgs = [
            ProviderMsg::Command(CommandEnvelope {
                port: ident("feed"),
                command: ident("like-post"),
                correlation: "c-4".into(),
                payload: serde_json::json!({ "post": "post-lena-glaze" }),
            }),
            ProviderMsg::Projection(ProjectionUpdate {
                port: ident("feed"),
                projection: ident("feed-page"),
                key: None,
                revision: 2,
                value: serde_json::json!({ "has-more": true }),
            }),
            ProviderMsg::ProjectionFailed {
                port: ident("feed"),
                projection: ident("feed-page"),
                key: None,
                reason: "unreachable".into(),
            },
            ProviderMsg::Outcome(OutcomeEnvelope {
                correlation: "c-4".into(),
                outcome: OutcomeResult::Refused {
                    refusal: ident("rate-limited"),
                },
                updates: vec![ProjectionUpdate {
                    port: ident("comments"),
                    projection: ident("for-post"),
                    key: Some(serde_json::json!("post-lena-glaze")),
                    revision: 3,
                    value: serde_json::json!({ "comments": [] }),
                }],
            }),
        ];
        for msg in msgs {
            let json = msg.to_json();
            assert_eq!(ProviderMsg::from_json(&json).unwrap(), msg, "{json}");
        }
    }

    #[test]
    fn outcome_shapes_match_the_design() {
        assert_eq!(OutcomeResult::Ok.to_json().to_string(), r#"{"ok":{}}"#);
        assert_eq!(
            OutcomeResult::Refused {
                refusal: ident("rate-limited")
            }
            .to_json()
            .to_string(),
            r#"{"refused":{"refusal":"rate-limited"}}"#
        );
    }
}
