//! uhura-wasm: thin wasm-bindgen wrappers — `Session` over uhura-core and
//! `FixtureDriver` over uhura-fixture. JSON strings across the boundary;
//! no timers, fetch, or DOM inside wasm (design §12.3). The envelope
//! shapes are frozen by `tests/abi_contract.rs` (plan micro-decision #14):
//! the same bytes the native trace harness prints. Errors cross the
//! boundary as thrown STRINGS (`Result<_, String>` — constructing a
//! `JsError` calls a JS import, which would poison the native rlib the
//! contract test runs against).

use wasm_bindgen::prelude::*;

use uhura_base::to_canonical_json;
use uhura_core::event::{ApplyNote, Event, apply_failure, apply_updates};
use uhura_core::ir::{IR_PROTOCOL, ProgramIr, load_program};
use uhura_core::state::{Projections, UiState};
use uhura_core::step::step_u;
use uhura_core::view::{Snapshot, VIEW_PROTOCOL};
use uhura_port::envelope::{PROVIDER_PROTOCOL, ProjectionUpdate};

/// The three protocol versions this build speaks, as one JSON object —
/// the shell hard-asserts all of them at boot (§12.3).
#[wasm_bindgen]
pub fn protocols() -> String {
    to_canonical_json(&serde_json::json!({
        "ir": IR_PROTOCOL,
        "view": VIEW_PROTOCOL,
        "provider": PROVIDER_PROTOCOL,
    }))
}

/// One machine: owns `U` and `X`, steps on dispatched events. The §7.2
/// ordering contract lives INSIDE `dispatch`: provider updates carried by
/// an event apply to `X` (revision-checked) before `step_u` runs — the
/// shell never touches the projection store.
#[wasm_bindgen]
pub struct Session {
    program: ProgramIr,
    u: UiState,
    x: Projections,
    v: Option<Snapshot>,
    /// Boot apply notes attach to the next step's trace record, mirroring
    /// the native harness (boot deliveries precede `Init` — §9.2).
    pending_applies: Vec<ApplyNote>,
}

#[wasm_bindgen]
impl Session {
    /// `ir_json` is the canonical `uhura-ir/0` artifact
    /// (`/api/play/ir.json` from `uhura play`); anything else is refused before
    /// deserialization.
    #[wasm_bindgen(constructor)]
    pub fn new(ir_json: &str) -> Result<Session, String> {
        let program = load_program(ir_json)?;
        Ok(Session {
            program,
            u: UiState::boot(),
            x: Projections::default(),
            v: None,
            pending_applies: Vec::new(),
        })
    }

    /// Applies the boot deliveries (`{"updates": […]}` — revision-1
    /// projection updates, §9.2) before the first `Init` dispatch.
    pub fn boot(&mut self, boot_json: &str) -> Result<(), String> {
        let json: serde_json::Value =
            serde_json::from_str(boot_json).map_err(|e| format!("boot: {e}"))?;
        let updates = match json.get("updates") {
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .map(ProjectionUpdate::from_json)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("boot: {e}"))?,
            _ => return Err("boot needs an `updates` list".to_string()),
        };
        let notes = apply_updates(&self.program, &mut self.x, &updates)
            .map_err(|e| format!("boot: {e}"))?;
        self.pending_applies.extend(notes);
        Ok(())
    }

    /// One step: parse the event, apply carried provider updates to `X`
    /// (§7.2/§9.4 ordering), run `step_u`, return the frozen step-result
    /// envelope (`{"c": …, "g": …, "i": …, "t": …, "v": …}`) as canonical
    /// JSON.
    ///
    /// TRANSACTIONAL against the session (§4.2's discipline extended to
    /// the wrapper): everything runs against staged copies and commits
    /// only on success, so a thrown string means the input was refused
    /// and the Session still holds exactly what it held before — the
    /// shell keeps pumping into a consistent machine (decision #59).
    pub fn dispatch(&mut self, event_json: &str) -> Result<String, String> {
        let json: serde_json::Value =
            serde_json::from_str(event_json).map_err(|e| format!("event: {e}"))?;
        let event = Event::from_json(&json)?;

        let mut x = self.x.clone();
        let mut applies = self.pending_applies.clone();
        match &event {
            Event::Outcome { updates, .. } | Event::Projection { updates } => {
                // Piggybacked settlement updates land BEFORE the outcome
                // dispatches — flicker-free by construction (§9.4). A
                // mid-batch failure aborts whole: X never half-applies.
                let notes = apply_updates(&self.program, &mut x, updates)?;
                applies.extend(notes);
            }
            Event::ProjectionFailed {
                port,
                projection,
                key,
                reason,
            } => {
                let note = apply_failure(
                    &self.program,
                    &mut x,
                    port,
                    projection,
                    key.as_ref(),
                    reason,
                )?;
                applies.push(note);
            }
            Event::Ui { .. } | Event::Init { .. } => {}
        }

        let mut result =
            step_u(&self.program, self.u.clone(), &x, event).map_err(|e| e.to_string())?;
        result.t.applies = applies.iter().map(ApplyNote::to_json).collect();
        let out = to_canonical_json(&result.to_json());
        // ── commit ──────────────────────────────────────────────────────
        self.pending_applies.clear();
        self.x = x;
        self.u = result.u;
        self.v = Some(result.v);
        Ok(out)
    }

    /// The current `uhura-view/0` snapshot, canonical JSON. There is no
    /// view before the first dispatch (`Init` mounts the entry page).
    pub fn view(&self) -> Result<String, String> {
        match &self.v {
            Some(v) => Ok(v.to_canonical_string()),
            None => Err("no view before the first dispatch (§9.2)".to_string()),
        }
    }

    /// The machine revision (`U.rev` — `+1` every step). Lossless as a JS
    /// number for any session a browser can hold.
    pub fn revision(&self) -> f64 {
        self.u.rev as f64
    }

    /// The IR protocol this session loaded (`"uhura-ir/0"`).
    pub fn ir_version(&self) -> String {
        self.program.protocol.clone()
    }
}

/// The scripted provider (§9.5), wrapped 1:1 — the shell wires `Session`
/// and `FixtureDriver` together by passing envelope JSON, so the Spock
/// seam stays visible in the browser too (§9.6).
#[wasm_bindgen(js_name = FixtureDriver)]
pub struct FixtureDriverJs {
    inner: uhura_fixture::FixtureDriver,
}

#[wasm_bindgen(js_class = FixtureDriver)]
impl FixtureDriverJs {
    /// `fixture_json`: the resolved slice tree (`/api/play/fixture.json`);
    /// `script_json`: the closed script grammar as JSON
    /// (`/api/play/script.json`).
    #[wasm_bindgen(constructor)]
    pub fn new(fixture_json: &str, script_json: &str) -> Result<FixtureDriverJs, String> {
        uhura_fixture::FixtureDriver::new(fixture_json, script_json)
            .map(|inner| FixtureDriverJs { inner })
    }

    /// Accepts one command envelope (wire form, `kind: "command"`).
    pub fn deliver(&mut self, cmd_json: &str) -> Result<(), String> {
        self.inner.deliver(cmd_json)
    }

    /// Advances one tick and returns the provider messages due — the
    /// shell maps wall time to tick ordinals (§8.4).
    pub fn tick(&mut self) -> Vec<String> {
        self.inner.tick()
    }

    /// True when nothing is scheduled or in flight.
    pub fn idle(&self) -> bool {
        self.inner.idle()
    }
}
