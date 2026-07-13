//! `StepTrace` — one canonical JSONL record per step, the conformance
//! artifact goldens pin (design §7.5): the input event, the dispatch
//! record (per-handler guard results, committed writes), structural ops,
//! `C`/`I`, drops, and the `u-hash`/`v-hash` pair.

use uhura_base::{Ident, to_canonical_json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepTrace {
    /// The input event, wire form.
    pub event: serde_json::Value,
    /// Harness-filled: what `apply_updates` did to `X` immediately before
    /// this step (§7.2 ordering made visible).
    pub applies: Vec<serde_json::Value>,
    pub disposition: Disposition,
    /// Structural ops in application order (`init`, `open-surface`,
    /// `already-open`, `dismiss`, `navigate`, `back`, `nav-underflow`).
    pub structural: Vec<serde_json::Value>,
    /// Emitted command envelopes, wire form.
    pub c: Vec<serde_json::Value>,
    /// Emitted intents, wire form.
    pub i: Vec<serde_json::Value>,
    /// Runtime diagnostics (`G` — §7.5 lists them in the record).
    pub g: Vec<serde_json::Value>,
    pub u_hash: String,
    pub v_hash: String,
    /// `--expanded` presentation only: the full snapshot.
    pub v: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Nothing dispatches (projection deliveries, `Init`).
    Delivery,
    /// Dropped before any handler ran (§7.2 acceptance, unknown
    /// correlation, unmounted outcome origin).
    Dropped { reason: DropReason, detail: String },
    /// The reserved structural `dismiss` event (micro-decision #36).
    Reserved { scope: String },
    /// Handlers were consulted (selected or not — `no-handler` and
    /// `projection-not-ready` outcomes live inside the record).
    Dispatched(DispatchRecord),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropReason {
    StaleScope,
    Occluded,
    Ineligible,
    StaleOutcome,
    UnknownCorrelation,
}

impl DropReason {
    pub fn as_str(self) -> &'static str {
        match self {
            DropReason::StaleScope => "stale-scope",
            DropReason::Occluded => "occluded",
            DropReason::Ineligible => "ineligible",
            DropReason::StaleOutcome => "stale-outcome",
            DropReason::UnknownCorrelation => "unknown-correlation",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchRecord {
    pub scope: String,
    pub definition: Ident,
    /// `"like-toggled"` | `"like-post.ok"` | `"like-post.err"`.
    pub on: String,
    /// Per matched handler, source order, up to and including the
    /// selected one.
    pub guards: Vec<GuardNote>,
    /// Absolute handler index within the definition, when one ran.
    pub selected: Option<usize>,
    /// Committed writes in execution order (empty when aborted/dropped).
    pub writes: Vec<serde_json::Value>,
    /// `projection-not-ready`: the body read an undelivered projection —
    /// nothing committed (§4.2 transactional backstop).
    pub aborted: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardNote {
    /// Absolute handler index within the definition.
    pub handler: usize,
    pub result: GuardResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardResult {
    Satisfied,
    Unsatisfied,
    /// A guard-position not-ready read: the guard is false (§4.2).
    NotReady,
}

impl StepTrace {
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "event": self.event,
            "u-hash": self.u_hash,
            "v-hash": self.v_hash,
        });
        if !self.applies.is_empty() {
            obj["applies"] = self.applies.clone().into();
        }
        match &self.disposition {
            Disposition::Delivery => {}
            Disposition::Dropped { reason, detail } => {
                obj["drop"] = serde_json::Value::String(reason.as_str().into());
                if !detail.is_empty() {
                    obj["drop-detail"] = serde_json::Value::String(detail.clone());
                }
            }
            Disposition::Reserved { scope } => {
                obj["reserved"] = serde_json::json!({ "event": "dismiss", "scope": scope });
            }
            Disposition::Dispatched(record) => {
                let mut dispatch = serde_json::json!({
                    "scope": record.scope,
                    "definition": record.definition.to_string(),
                    "on": record.on,
                    "guards": record.guards.iter().map(|g| serde_json::json!({
                        "handler": g.handler,
                        "guard": match g.result {
                            GuardResult::Satisfied => "satisfied",
                            GuardResult::Unsatisfied => "unsatisfied",
                            GuardResult::NotReady => "not-ready",
                        },
                    })).collect::<Vec<_>>(),
                    "selected": record.selected,
                });
                if !record.writes.is_empty() {
                    dispatch["writes"] = record.writes.clone().into();
                }
                if record.selected.is_none() {
                    obj["drop"] = serde_json::Value::String("no-handler".into());
                }
                if let Some(reason) = &record.aborted {
                    dispatch["aborted"] = serde_json::Value::String(reason.clone());
                    obj["drop"] = serde_json::Value::String(reason.clone());
                }
                obj["dispatch"] = dispatch;
            }
        }
        if !self.structural.is_empty() {
            obj["structural"] = self.structural.clone().into();
        }
        if !self.c.is_empty() {
            obj["c"] = self.c.clone().into();
        }
        if !self.i.is_empty() {
            obj["i"] = self.i.clone().into();
        }
        if !self.g.is_empty() {
            obj["g"] = self.g.clone().into();
        }
        if let Some(v) = &self.v {
            obj["v"] = v.clone();
        }
        obj
    }

    /// The JSONL line (canonical: sorted keys, no floats by construction).
    pub fn to_line(&self) -> String {
        to_canonical_json(&self.to_json())
    }
}
