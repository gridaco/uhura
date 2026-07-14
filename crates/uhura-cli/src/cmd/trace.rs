//! `uhura trace [path] --script=<name> [--expanded]` — the headless play
//! harness (design §9.5, §12.4): boot projections → `Init` → tick pump.
//! Each step prints one canonical `StepTrace` JSONL line; `--expanded`
//! embeds the full V per step as presentation.
//!
//! The pump owns exactly the §7.2 ordering contract: driver messages
//! apply to `X` (revision-checked) immediately before their event steps;
//! emitted commands deliver to the driver as they appear. Scripts carry a
//! harness-only `[[ui]]` stimulus section (plan micro-decision #13): each
//! stimulus presses a descriptor that must be PRESENT in the current view
//! — a vanished control fails the script, which is the point.

use std::collections::BTreeMap;
use std::process::ExitCode;

use uhura_base::{Ident, Severity, render_text, to_canonical_json};
use uhura_check::check;
use uhura_check::fixture::{FixtureData, load_fixture};
use uhura_core::event::{ApplyNote, Event, apply_failure, apply_updates, decode_carried_data};
use uhura_core::ir::ProgramIr;
use uhura_core::state::{Projections, UiState};
use uhura_core::step::step_u;
use uhura_core::view::{Descriptor, Node, Snapshot};
use uhura_fixture::FixtureDriver;
use uhura_port::envelope::{ProjectionUpdate, ProviderMsg};

use crate::CommonArgs;

pub fn run(common: &CommonArgs, script: Option<&str>, expanded: bool) -> ExitCode {
    let root = &common.root;
    let input = match super::assemble_input(root) {
        Ok(input) => input,
        Err(code) => return code,
    };
    let manifest = input.manifest.clone();
    let output = check(&input);
    if output
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        print!("{}", render_text(&output.diagnostics, &output.source_map));
        eprintln!("uhura trace: the check must come up clean first");
        return ExitCode::from(1);
    }
    let Some(lowered) = &output.lowered else {
        eprintln!("uhura trace: no checked program");
        return ExitCode::from(1);
    };

    // The play profile names the fixture; `--script` overrides the script.
    let Some(profile) = manifest.play.values().next() else {
        eprintln!("uhura trace: the manifest declares no [play.*] profile (§3)");
        return ExitCode::from(1);
    };
    let Some(fixture_rel) = manifest.fixtures.get(&profile.fixture) else {
        eprintln!(
            "uhura trace: play fixture `{}` is not declared",
            profile.fixture
        );
        return ExitCode::from(1);
    };
    let script_name = script.unwrap_or(profile.script.as_str());
    let script_rel = format!("fixtures/scripts/{script_name}.toml");

    let read = |rel: &str| -> Result<String, String> {
        std::fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}: {e}"))
    };
    let (fixture_text, script_text) = match (read(fixture_rel), read(&script_rel)) {
        (Ok(f), Ok(s)) => (f, s),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("uhura trace: {e}");
            return ExitCode::from(2);
        }
    };

    match run_script(&lowered.program, &fixture_text, &script_text, expanded) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("uhura trace: {script_name}: {e}");
            ExitCode::from(1)
        }
    }
}

/// The whole headless run as a pure-ish function of its inputs (the gate
/// tests golden-pin its output): boot deliveries → `Init` on the entry
/// route → tick pump until the driver idles and every stimulus fired.
pub fn run_script(
    program: &ProgramIr,
    fixture_text: &str,
    script_text: &str,
    expanded: bool,
) -> Result<Vec<String>, String> {
    let fixture =
        load_fixture(fixture_text).map_err(|issues| format!("fixture: {}", issues[0].message))?;
    let script_json = uhura_fixture::toml_to_json(script_text)?;
    let stimuli = parse_stimuli(&script_json)?;
    let fixture_json = fixture_slices_json(&fixture);
    let mut driver = FixtureDriver::new(&fixture_json, &to_canonical_json(&script_json))?;

    let mut x = Projections::default();
    let mut lines = Vec::new();

    // ── boot projections (§9.2): delivered before Init, revision 1 ─────
    let boot_applies = apply_updates(program, &mut x, &boot_updates(program, &fixture)?)?;

    // ── Init on the entry route ─────────────────────────────────────────
    let mut result = step_u(
        program,
        UiState::boot(),
        &x,
        Event::Init {
            route: program.entry.clone(),
            params: BTreeMap::new(),
        },
    )
    .map_err(|e| e.to_string())?;
    result.t.applies = boot_applies.iter().map(ApplyNote::to_json).collect();
    let mut u = result.u;
    let mut v = result.v;
    finish_step(&mut lines, result.t, &v, expanded);
    deliver_commands(&mut driver, &result.c)?;

    // ── the pump ────────────────────────────────────────────────────────
    let mut tick = 0u64;
    let mut next_stimulus = 0usize;
    loop {
        let stimuli_left = next_stimulus < stimuli.len();
        if driver.idle() && !stimuli_left {
            break;
        }
        tick += 1;
        if tick > 10_000 {
            return Err("the script did not quiesce within 10000 ticks".into());
        }

        // Driver arrivals first, then this tick's stimuli.
        for msg_json in driver.tick() {
            let msg_value: serde_json::Value =
                serde_json::from_str(&msg_json).map_err(|e| format!("driver message: {e}"))?;
            let msg = ProviderMsg::from_json(&msg_value)?;
            let (event, applies) = match msg {
                ProviderMsg::Projection(update) => {
                    let applies = apply_updates(program, &mut x, std::slice::from_ref(&update))?;
                    (
                        Event::Projection {
                            updates: vec![update],
                        },
                        applies,
                    )
                }
                ProviderMsg::ProjectionFailed {
                    port,
                    projection,
                    key,
                    reason,
                } => {
                    let note =
                        apply_failure(program, &mut x, &port, &projection, key.as_ref(), &reason)?;
                    (
                        Event::ProjectionFailed {
                            port,
                            projection,
                            key,
                            reason,
                        },
                        vec![note],
                    )
                }
                ProviderMsg::Outcome(envelope) => {
                    // Piggybacked settlement updates apply BEFORE the
                    // outcome dispatches (§9.4) — flicker-free by
                    // construction.
                    let applies = apply_updates(program, &mut x, &envelope.updates)?;
                    (
                        Event::Outcome {
                            correlation: envelope.correlation,
                            result: envelope.outcome,
                            updates: envelope.updates,
                        },
                        applies,
                    )
                }
                ProviderMsg::Command(c) => {
                    return Err(format!("the driver emitted a command `{}`", c.command));
                }
            };
            let mut result = step_u(program, u, &x, event).map_err(|e| e.to_string())?;
            result.t.applies = applies.iter().map(ApplyNote::to_json).collect();
            u = result.u;
            v = result.v;
            finish_step(&mut lines, result.t, &v, expanded);
            deliver_commands(&mut driver, &result.c)?;
        }

        while next_stimulus < stimuli.len() && stimuli[next_stimulus].at_tick == tick {
            let stim = &stimuli[next_stimulus];
            next_stimulus += 1;
            let descriptor = find_stimulus_descriptor(&v, stim)?;
            let data = decode_carried_data(&descriptor, &stim.data)?;
            let event = Event::Ui {
                descriptor,
                data,
                view_rev: v.revision,
            };
            let result = step_u(program, u, &x, event).map_err(|e| e.to_string())?;
            u = result.u;
            v = result.v;
            finish_step(&mut lines, result.t, &v, expanded);
            deliver_commands(&mut driver, &result.c)?;
        }

        if let Some(stim) = stimuli.get(next_stimulus)
            && stim.at_tick < tick
        {
            return Err(format!(
                "`[[ui]]` stimuli must be ordered by at-tick (tick {} after {tick})",
                stim.at_tick
            ));
        }
    }

    Ok(lines)
}

fn finish_step(
    lines: &mut Vec<String>,
    mut t: uhura_core::trace::StepTrace,
    v: &Snapshot,
    expanded: bool,
) {
    if expanded {
        t.v = Some(v.to_json());
    }
    lines.push(t.to_line());
}

fn deliver_commands(
    driver: &mut FixtureDriver,
    commands: &[uhura_port::envelope::CommandEnvelope],
) -> Result<(), String> {
    for c in commands {
        let json = to_canonical_json(&ProviderMsg::Command(c.clone()).to_json());
        driver.deliver(&json)?;
    }
    Ok(())
}

/// The boot deliveries (§9.2): every `boot = true` projection from its
/// `boot.<name>` fixture slice, revision 1 (driver mints start at 2 —
/// micro-decision #43). `uhura play`'s `/api/play/boot.json` and the wasm ABI
/// contract test build the same envelope.
pub fn boot_updates(
    program: &ProgramIr,
    fixture: &FixtureData,
) -> Result<Vec<ProjectionUpdate>, String> {
    let mut updates = Vec::new();
    for (name, decl) in &program.projections {
        if !decl.boot {
            continue;
        }
        let Some(value) = fixture.get("boot", name.as_str()) else {
            return Err(format!(
                "boot projection `{name}` needs a `boot.{name}` fixture slice (§6.1)"
            ));
        };
        updates.push(ProjectionUpdate {
            port: decl.port.clone(),
            projection: name.clone(),
            key: None,
            revision: 1,
            value: value.clone(),
        });
    }
    Ok(updates)
}

/// The resolved fixture slices as one canonical JSON object — the
/// `FixtureDriver::new` input (`uhura play` serves it as
/// `/api/play/fixture.json`).
pub fn fixture_slices_json(fixture: &FixtureData) -> String {
    let mut root = serde_json::Map::new();
    for (ns, slices) in &fixture.slices {
        let mut ns_map = serde_json::Map::new();
        for (name, value) in slices {
            ns_map.insert(name.clone(), value.clone());
        }
        root.insert(ns.clone(), serde_json::Value::Object(ns_map));
    }
    to_canonical_json(&serde_json::Value::Object(root))
}

// ── the harness-only [[ui]] stimulus section ────────────────────────────────

struct Stimulus {
    at_tick: u64,
    emit: Ident,
    /// Payload subset the target descriptor must carry.
    where_: serde_json::Map<String, serde_json::Value>,
    /// Renderer-carried fields (`{ value = "…" }`).
    data: serde_json::Map<String, serde_json::Value>,
}

fn parse_stimuli(script: &serde_json::Value) -> Result<Vec<Stimulus>, String> {
    let Some(entries) = script.get("ui") else {
        return Ok(Vec::new());
    };
    let Some(entries) = entries.as_array() else {
        return Err("`ui` must be an array of stimulus tables".into());
    };
    let mut out = Vec::new();
    for entry in entries {
        let Some(obj) = entry.as_object() else {
            return Err("a `[[ui]]` entry is a table".into());
        };
        for key in obj.keys() {
            if !["at-tick", "emit", "where", "data"].contains(&key.as_str()) {
                return Err(format!("`[[ui]]` has no `{key}` field"));
            }
        }
        let at_tick = obj
            .get("at-tick")
            .and_then(serde_json::Value::as_u64)
            .filter(|t| *t >= 1)
            .ok_or("`[[ui]]` needs `at-tick` ≥ 1")?;
        let emit = obj
            .get("emit")
            .and_then(serde_json::Value::as_str)
            .ok_or("`[[ui]]` needs `emit`")
            .and_then(|s| Ident::new(s).map_err(|_| "`emit` must be a kebab name"))?;
        let table = |field: &str| -> Result<serde_json::Map<String, serde_json::Value>, String> {
            match obj.get(field) {
                None => Ok(serde_json::Map::new()),
                Some(serde_json::Value::Object(map)) => Ok(map.clone()),
                Some(_) => Err(format!("`{field}` must be a table")),
            }
        };
        out.push(Stimulus {
            at_tick,
            emit,
            where_: table("where")?,
            data: table("data")?,
        });
    }
    Ok(out)
}

/// Finds the descriptor a stimulus presses in the CURRENT view: emit name
/// matches and `where` is a subset of the payload. Searching page, then
/// surfaces bottom→top, then surface dismiss controls; distinct matches
/// are ambiguous (identical ones — e.g. a field's submit and its button —
/// collapse).
fn find_stimulus_descriptor(v: &Snapshot, stim: &Stimulus) -> Result<Descriptor, String> {
    let mut matches: Vec<Descriptor> = Vec::new();
    collect_matches(&v.page.root, stim, &mut matches);
    for surface in &v.surfaces {
        collect_matches(&surface.root, stim, &mut matches);
        if surface.dismiss.emit == stim.emit && subset(&stim.where_, &surface.dismiss.payload) {
            matches.push(surface.dismiss.clone());
        }
    }
    let mut distinct: Vec<&Descriptor> = Vec::new();
    for d in &matches {
        if !distinct
            .iter()
            .any(|seen| seen.emit == d.emit && seen.scope == d.scope && seen.payload == d.payload)
        {
            distinct.push(d);
        }
    }
    match distinct.len() {
        0 => Err(format!(
            "no descriptor in the current view emits `{}` with {} — the control is \
             gone or never existed",
            stim.emit,
            serde_json::Value::Object(stim.where_.clone())
        )),
        1 => Ok(distinct[0].clone()),
        n => Err(format!(
            "{n} distinct descriptors emit `{}` with that `where` — narrow it",
            stim.emit
        )),
    }
}

fn collect_matches(node: &Node, stim: &Stimulus, out: &mut Vec<Descriptor>) {
    for d in &node.on {
        if d.emit == stim.emit && subset(&stim.where_, &d.payload) {
            out.push(d.clone());
        }
    }
    for child in &node.children {
        collect_matches(child, stim, out);
    }
}

fn subset(
    where_: &serde_json::Map<String, serde_json::Value>,
    payload: &serde_json::Value,
) -> bool {
    where_
        .iter()
        .all(|(k, v)| payload.get(k).is_some_and(|p| p == v))
}
