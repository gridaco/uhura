# Play debugger TODO

This tracker starts at the current Play debugger spike: the inspection protocol,
bounded step history, focused behavior graph, live highlighting, definition
pinning, a resizable responsive shell, graph zoom/pan, route-scoped page-scale
and history-swipe locking, and keyboard navigation already exist. Items below
describe what is still required to turn that demonstration into a useful
debugger.

Ownership stays local. Engine and CLI prerequisites are tracked beside their
implementations rather than being hidden in this browser backlog:

- [Core inspection TODO](../../../crates/uhura-core/src/TODO.md)
- [CLI Play-host TODO](../../../crates/uhura-cli/src/cmd/TODO.md)

## Labels

- **Priority P0** — next debugger increment; highest user value on the existing
  foundation.
- **Priority P1** — required before calling the debugger product-ready.
- **Priority P2** — later hardening or capability work; require evidence before
  expanding scope.
- **Difficulty S** — localized policy, rendering, or test change.
- **Difficulty M** — coordinated work across several Play modules.
- **Difficulty L** — substantial UI architecture, performance, or cross-host
  integration.
- **Difficulty XL** — runtime-semantics or protocol-lifecycle project.
- **Engine work: No** — the current inspection artifact and step records are
  sufficient.
- **Engine work: Conditional** — the browser-owned version is possible now,
  but a stronger form requires the engine tracker.
- **Engine work: Yes** — blocked on an item in the core inspection tracker.

## P0 — make recorded execution useful

- [ ] **[P0][M][Engine work: No] Add a retained-step timeline and historical
      inspection mode.**
  - **Owner:** `inspection-store.ts`, `debug-controller.ts`, and
    `debug-surface.ts`.
  - Present the already-retained bounded history; do not add another history
    buffer in the visualization.
  - Provide Live/Pause, previous/next step, direct step selection, and a clear
    indication when the graph is showing history rather than the running tip.
  - Keep historical inspection observational. Selecting an old record must
    never mutate, pause, or restore the runtime.
  - Returning to Live must resume from the newest publication without losing
    steps received while the viewer was paused.

- [ ] **[P0][M][Engine work: No] Isolate and narrate the causal path for one
      recorded step.**
  - **Owner:** `debug-model.ts`, `debug-layout.ts`, and `debug-surface.ts`.
  - Derive an explicit sequence from facts already present in the trace:
    event, consulted guards, selected handler, writes, sends, structural
    effects, outcomes, and projection application.
  - Add a “taken path only” view that dims or hides unrelated topology without
    changing the underlying graph identity.
  - Show before/after values from adjacent retained snapshots for state that
    changed in the selected step.
  - Never invent expression values that the trace does not record. Richer
    evaluated facts belong in the core tracker.

## P1 — product-quality browser debugger

- [ ] **[P1][L][Engine work: No] Make large definitions navigable.**
  - **Owner:** `debug-model.ts`, `debug-layout.ts`, `debug-surface.ts`, and
    `shell.css`.
  - Build on the existing zoom/pan camera with search, semantic filters,
    fit-to-selection, and a compact overview or minimap.
  - Allow unrelated branches to collapse while preserving a stable route back
    to the full definition.
  - Keep lane labels and current-step context visible while the canvas scrolls.
  - Preserve the existing roving-tab-stop and arrow-navigation contract.
  - Validate against the current 77-node/115-edge feed definition and at least
    one larger synthetic fixture.

- [ ] **[P1][L][Engine work: No] Replace whole-graph rerenders with incremental
      runtime decoration.**
  - **Owner:** `debug-model.ts`, `debug-layout.ts`, and `debug-surface.ts`.
  - Cache static topology and geometry by `(programHash, focusDefinitionId)`.
  - Rebuild the graph only when the checked program or focused definition
    changes; otherwise patch node classes, status text, edge activity, summary,
    and selection details by stable ID.
  - Avoid layout reads after graph writes on ordinary runtime steps.
  - Add a repeatable performance fixture and define budgets for update time,
    allocations, and dropped frames before optimizing further.
  - Keep the accessible SVG-edge/HTML-node renderer unless profiling isolates
    edge paint as the bottleneck. A Canvas edge layer is a measured fallback,
    not an out-of-the-box renderer switch.

- [ ] **[P1][S][Engine work: No] Split ambiguous Follow-live behavior into an
      explicit policy.**
  - **Owner:** `debug-model.ts` and `debug-surface.ts`.
  - Decide whether the default follows the latest dispatch origin, the topmost
    mounted definition, or exposes both as separate modes.
  - Preserve the transition source long enough to explain navigation without
    leaving the debugger apparently stuck on the prior page.
  - Cover navigation, replace, back, surface open/dismiss, and provider outcome
    delivery in model tests.

- [ ] **[P1][M][Engine work: No] Add checked-in real-browser and accessibility
      regressions.**
  - **Owner:** `tests/` plus the repository's browser-test harness.
  - Exercise the actual Play route: open/close, lazy subscription, live
    transition highlights, definition pinning, Follow live, historical mode,
    keyboard graph navigation, and disposal.
  - Cover wide right-dock, narrow bottom-dock, and compact takeover layouts,
    including bounds and overflow assertions.
  - Add an automated accessibility audit and a short manual assistive-
    technology checklist; unit DOM contracts are not a substitute for either.
  - Add targeted visual snapshots only for layout states whose geometry is part
    of the contract.

- [ ] **[P1][M][Engine work: No] Turn source spans into source navigation.**
  - **Owner:** `debug-surface.ts` for the interaction; the safe source contract
    is owned by the [CLI Play-host TODO](../../../crates/uhura-cli/src/cmd/TODO.md).
  - Show a small source excerpt and provide an Open-in-Editor action.
  - Bind every excerpt to the source revision/hash that produced the inspected
    program; never display current bytes against a stale span.
  - Treat UTF-8 byte offsets as bytes throughout the host boundary.

## P2 — measured hardening and optional capabilities

- [ ] **[P2][M][Engine work: No] Replace count-only browser retention with a
      measured byte budget.**
  - **Owner:** `inspection-store.ts` and the timeline UI.
  - Keep the existing hard step-count ceiling as a safety backstop, but evict
    by measured payload size so large state snapshots cannot dominate memory.
  - Make truncation visible in the timeline and preserve the newest coherent
    step boundary.
  - Measure representative projects before choosing defaults.

- [ ] **[P2][M][Engine work: Conditional] Export and reopen observational
      inspection sessions.**
  - **Owner:** `inspection-store.ts`, protocol types, and a small Play UI seam.
  - Export versioned program metadata plus retained records, with explicit
    truncation and redaction metadata.
  - Reopening an export is an offline viewer, not deterministic runtime replay.
  - Exact replay or runtime restoration is blocked on the core runtime-control
    item; do not imply that an observational export can reproduce provider
    effects.

- [ ] **[P2][M][Engine work: Yes] Represent component runtime instances without
      misleading static values.**
  - **Owner:** browser presentation after the core inspection contract decides
    whether components have inspectable runtime identity.
  - Until that contract exists, label component graphs as static topology when
    instance-specific state is unavailable.
  - See the component-instance item in the
    [Core inspection TODO](../../../crates/uhura-core/src/TODO.md).

## Non-goals for the browser backlog

- Reimplementing the Uhura evaluator in TypeScript.
- Rewinding the running session by assigning old browser snapshots.
- Inferring guard values or provider effects that the engine did not record.
- Making inspection data safe for an untrusted or public Play deployment solely
  by hiding fields in the DOM.
