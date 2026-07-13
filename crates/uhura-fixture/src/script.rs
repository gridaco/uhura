//! Script JSON → validated entries (design §9.5). The grammar is closed:
//! unknown keys are errors at every level, `on-unscripted = "error"` is the
//! only policy, and `after-ticks ≥ 1` keeps optimistic states observable.
//! Slice references resolve against the fixture at parse time, so a dangling
//! `<ns>.<name>` can never survive to a tick.

use std::collections::BTreeMap;

use uhura_base::Ident;
use uhura_port::OutcomeResult;

/// Namespace → slice name → pre-resolved JSON value.
pub(crate) type Fixture = BTreeMap<String, BTreeMap<String, serde_json::Value>>;

type Map = serde_json::Map<String, serde_json::Value>;

pub(crate) struct Script {
    pub(crate) deliver: Vec<DeliverEntry>,
    pub(crate) replies: Vec<ReplyEntry>,
}

pub(crate) struct DeliverEntry {
    /// Absolute due tick — standalone entries are scheduled at `new()`.
    pub(crate) after_ticks: u64,
    pub(crate) port: Ident,
    pub(crate) projection: Ident,
    pub(crate) key: Option<serde_json::Value>,
    pub(crate) body: DeliverBody,
}

pub(crate) enum DeliverBody {
    /// The slice's JSON, cloned at parse (fixture data is fixed). May still
    /// carry `"@fresh-id"` markers — minted at emission.
    Slice(serde_json::Value),
    /// A projection-failed reason string.
    Failed(String),
}

#[derive(Debug)]
pub(crate) struct ReplyEntry {
    pub(crate) command: Ident,
    /// Every field must be JSON-equal to the same-named payload field.
    pub(crate) where_fields: BTreeMap<String, serde_json::Value>,
    pub(crate) after_ticks: u64,
    pub(crate) repeat: bool,
    /// One-shot entries are consumed on match (§9.5 — a duplicate in-flight
    /// command IS the dedupe assertion).
    pub(crate) consumed: bool,
    pub(crate) outcome: OutcomeResult,
    pub(crate) updates: Vec<UpdateSpec>,
}

/// A settlement update riding inside the outcome envelope (§9.4).
#[derive(Debug)]
pub(crate) struct UpdateSpec {
    pub(crate) port: Ident,
    pub(crate) projection: Ident,
    pub(crate) key: KeySpec,
    pub(crate) slice: serde_json::Value,
}

#[derive(Debug)]
pub(crate) enum KeySpec {
    Absent,
    Literal(serde_json::Value),
    /// `{ "from": "payload.<field>" }` — resolved at deliver time.
    FromPayload(String),
}

/// Whole-string `"@payload.<field>"` marker → the field name. One of the
/// only two substitutions (§9.5); anything else passes through verbatim.
pub(crate) fn payload_marker(s: &str) -> Option<&str> {
    s.strip_prefix("@payload.")
        .filter(|field| !field.is_empty())
}

pub(crate) fn parse_script(json: &serde_json::Value, fixture: &Fixture) -> Result<Script, String> {
    let top = as_object(json, "a script")?;
    closed_keys(
        "top-level",
        top,
        &["on-unscripted", "deliver", "reply", "ui"],
    )?;
    if let Some(policy) = top.get("on-unscripted")
        && policy.as_str() != Some("error")
    {
        return Err(format!(
            "`on-unscripted` must be \"error\" — the only policy (§9.5), got {policy}"
        ));
    }
    // "ui" is harness-only: accepted here, ignored completely.

    let mut deliver = Vec::new();
    if let Some(value) = top.get("deliver") {
        for (i, item) in as_array(value, "`deliver`")?.iter().enumerate() {
            deliver.push(parse_deliver(item, fixture).map_err(|e| format!("deliver[{i}]: {e}"))?);
        }
    }
    let mut replies = Vec::new();
    if let Some(value) = top.get("reply") {
        for (i, item) in as_array(value, "`reply`")?.iter().enumerate() {
            replies.push(parse_reply(item, fixture).map_err(|e| format!("reply[{i}]: {e}"))?);
        }
    }
    Ok(Script { deliver, replies })
}

fn parse_deliver(json: &serde_json::Value, fixture: &Fixture) -> Result<DeliverEntry, String> {
    let obj = as_object(json, "a deliver entry")?;
    closed_keys(
        "deliver entry",
        obj,
        &[
            "after-ticks",
            "port",
            "projection",
            "key",
            "slice",
            "failed",
        ],
    )?;
    let after_ticks = after_ticks(obj)?;
    let port = ident_field(obj, "port")?;
    let projection = ident_field(obj, "projection")?;
    let key = match obj.get("key") {
        None | Some(serde_json::Value::Null) => None,
        Some(k) => {
            if k.as_object().is_some_and(|o| o.contains_key("from")) {
                return Err("a deliver `key` is a literal — `{ \"from\": … }` resolves \
                     from a command payload and is reply-only"
                    .into());
            }
            Some(k.clone())
        }
    };
    let body = match (obj.get("slice"), obj.get("failed")) {
        (Some(slice), None) => {
            let slice = lookup_slice(slice, fixture)?;
            // Standalone deliveries have no matched command, so payload
            // markers in their slices are errors NOW, not at emission.
            reject_payload_markers(&slice)?;
            DeliverBody::Slice(slice)
        }
        (None, Some(failed)) => DeliverBody::Failed(
            failed
                .as_str()
                .ok_or("`failed` is a projection-failed reason string")?
                .to_string(),
        ),
        _ => return Err("a deliver entry has exactly one of `slice` or `failed`".into()),
    };
    Ok(DeliverEntry {
        after_ticks,
        port,
        projection,
        key,
        body,
    })
}

fn parse_reply(json: &serde_json::Value, fixture: &Fixture) -> Result<ReplyEntry, String> {
    let obj = as_object(json, "a reply entry")?;
    closed_keys(
        "reply entry",
        obj,
        &[
            "on",
            "after-ticks",
            "repeat",
            "outcome",
            "refusal",
            "reason",
            "updates",
        ],
    )?;

    let on = as_object(obj.get("on").ok_or("missing `on`")?, "`on`")?;
    closed_keys("`on`", on, &["command", "where"])?;
    let command = ident_field(on, "command")?;
    let mut where_fields = BTreeMap::new();
    if let Some(w) = on.get("where") {
        for (field, expected) in as_object(w, "`where`")? {
            where_fields.insert(field.clone(), expected.clone());
        }
    }

    let after_ticks = after_ticks(obj)?;
    let repeat = match obj.get("repeat") {
        None => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(other) => return Err(format!("`repeat` must be a bool, got {other}")),
    };
    let outcome = parse_outcome(obj)?;

    let mut updates = Vec::new();
    if let Some(value) = obj.get("updates") {
        for (i, item) in as_array(value, "`updates`")?.iter().enumerate() {
            updates.push(parse_update(item, fixture).map_err(|e| format!("updates[{i}]: {e}"))?);
        }
    }

    Ok(ReplyEntry {
        command,
        where_fields,
        after_ticks,
        repeat,
        consumed: false,
        outcome,
        updates,
    })
}

/// `refusal` accompanies exactly `refused`; `reason` exactly `unavailable`.
fn parse_outcome(obj: &Map) -> Result<OutcomeResult, String> {
    let name = obj
        .get("outcome")
        .and_then(serde_json::Value::as_str)
        .ok_or("`outcome` must be \"ok\" | \"refused\" | \"unavailable\"")?;
    let refusal = obj.get("refusal");
    let reason = obj.get("reason");
    match name {
        "ok" => {
            if refusal.is_some() || reason.is_some() {
                return Err("`ok` carries neither `refusal` nor `reason`".into());
            }
            Ok(OutcomeResult::Ok)
        }
        "refused" => {
            if reason.is_some() {
                return Err("`reason` belongs to `unavailable`, not `refused`".into());
            }
            let refusal = refusal
                .and_then(serde_json::Value::as_str)
                .ok_or("`refused` requires a `refusal` name")?;
            Ok(OutcomeResult::Refused {
                refusal: Ident::new(refusal).map_err(|e| e.to_string())?,
            })
        }
        "unavailable" => {
            if refusal.is_some() {
                return Err("`refusal` belongs to `refused`, not `unavailable`".into());
            }
            let reason = reason
                .and_then(serde_json::Value::as_str)
                .ok_or("`unavailable` requires a `reason` text")?;
            Ok(OutcomeResult::Unavailable {
                reason: reason.to_string(),
            })
        }
        other => Err(format!(
            "`{other}` is not an outcome (ok | refused | unavailable)"
        )),
    }
}

fn parse_update(json: &serde_json::Value, fixture: &Fixture) -> Result<UpdateSpec, String> {
    let obj = as_object(json, "an update entry")?;
    closed_keys("update entry", obj, &["port", "projection", "key", "slice"])?;
    Ok(UpdateSpec {
        port: ident_field(obj, "port")?,
        projection: ident_field(obj, "projection")?,
        key: parse_key_spec(obj.get("key"))?,
        slice: lookup_slice(obj.get("slice").ok_or("missing `slice`")?, fixture)?,
    })
}

fn parse_key_spec(value: Option<&serde_json::Value>) -> Result<KeySpec, String> {
    use serde_json::Value as J;
    match value {
        None | Some(J::Null) => Ok(KeySpec::Absent),
        Some(J::Object(map)) if map.contains_key("from") => {
            // `{ "from": "payload.<field>" }` — the only computed key (§9.5).
            let target = map.get("from").and_then(J::as_str);
            let field = target.and_then(|t| t.strip_prefix("payload."));
            match (map.len(), field) {
                (1, Some(field)) if !field.is_empty() => Ok(KeySpec::FromPayload(field.into())),
                _ => Err("a computed key is exactly `{ \"from\": \"payload.<field>\" }`".into()),
            }
        }
        Some(literal) => Ok(KeySpec::Literal(literal.clone())),
    }
}

/// `after-ticks ≥ 1`, so optimistic states are always observable (§9.5).
fn after_ticks(obj: &Map) -> Result<u64, String> {
    let value = obj.get("after-ticks").ok_or("missing `after-ticks`")?;
    match value.as_u64() {
        Some(n) if n >= 1 => Ok(n),
        _ => Err(format!("`after-ticks` must be an integer ≥ 1, got {value}")),
    }
}

fn lookup_slice(value: &serde_json::Value, fixture: &Fixture) -> Result<serde_json::Value, String> {
    let reference = value
        .as_str()
        .ok_or("`slice` is a `<ns>.<name>` reference string")?;
    let Some((ns, name)) = reference.split_once('.') else {
        return Err(format!(
            "`{reference}` is not a `<ns>.<name>` slice reference"
        ));
    };
    fixture
        .get(ns)
        .and_then(|slices| slices.get(name))
        .cloned()
        .ok_or_else(|| format!("no fixture slice `{reference}`"))
}

fn reject_payload_markers(value: &serde_json::Value) -> Result<(), String> {
    use serde_json::Value as J;
    match value {
        J::String(s) => match payload_marker(s) {
            Some(_) => Err(format!(
                "`{s}` is reply-only — a standalone delivery has no command payload (§9.5)"
            )),
            None => Ok(()),
        },
        J::Array(items) => items.iter().try_for_each(reject_payload_markers),
        J::Object(map) => map.values().try_for_each(reject_payload_markers),
        _ => Ok(()),
    }
}

fn ident_field(obj: &Map, field: &str) -> Result<Ident, String> {
    let s = obj
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("missing or non-text `{field}`"))?;
    Ident::new(s).map_err(|e| e.to_string())
}

fn as_object<'v>(value: &'v serde_json::Value, what: &str) -> Result<&'v Map, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{what} must be a JSON object"))
}

fn as_array<'v>(
    value: &'v serde_json::Value,
    what: &str,
) -> Result<&'v Vec<serde_json::Value>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{what} must be an array"))
}

/// The §9.5 grammar is closed: unknown keys are errors, never ignored.
fn closed_keys(what: &str, obj: &Map, allowed: &[&str]) -> Result<(), String> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "unknown {what} key `{key}` — the §9.5 grammar is closed"
            ));
        }
    }
    Ok(())
}
