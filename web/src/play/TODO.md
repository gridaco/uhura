# Play debugger TODO

This tracker starts at the current canonical Play debugger: an admitted host
inspection artifact, bounded correlated receipt/inspection history, a focused
machine graph, conservative live highlighting, machine pinning, responsive
debug chrome, graph zoom/pan, route-scoped page-scale and history-swipe
locking, and keyboard navigation already exist. Items below describe what is
still required to turn that foundation into a useful debugger.

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
- **Engine work: No** — admitted host inspection plus correlated machine
  receipts and inspections are sufficient.
- **Engine work: Conditional** — a conservative browser-owned version is
  possible now, but a stronger claim requires the core tracker.
- **Engine work: Yes** — blocked on an item in the core inspection tracker.

## P0 — make recorded execution useful

- [ ] **[P0][M][Engine work: No] Add a retained-step timeline and historical
      inspection mode.**
  - **Owner:** `inspection-store.ts`, `debug-controller.ts`, and
    `debug-surface.ts`.
  - Present the store's already-retained receipt/inspection pairs; do not add a
    second history buffer in the visualization.
  - Provide Live/Pause, previous/next step, direct step selection, and a clear
    indication when the graph is showing history rather than the running tip.
  - Keep historical inspection observational. Selecting an old publication
    must never mutate, pause, or restore the Wasm `Session`.
  - Returning to Live must resume from the newest publication without losing
    receipts received while the viewer was paused.

- [ ] **[P0][M][Engine work: Conditional] Narrate the defensible causal path
      for one receipt.**
  - **Owner:** `session.ts`, `adapter-host.ts`, `debug-model.ts`,
    `debug-layout.ts`, and `debug-surface.ts`.
  - Start from facts the canonical boundary actually publishes: resolved local
    or port input, reaction disposition or fault, ordered commands, state
    differences between adjacent inspections, and the post-observation.
  - Correlate those facts only with nodes and edges in the admitted interaction
    graph. A “taken path” view may dim unrelated topology, but must label
    conservative context separately from proven activity.
  - Preserve exact port identity for adapter-delivered inputs and port-bound
    commands; the browser must not reconstruct provider or router semantics.
  - Evaluated guard values, internal transition paths, and expression
    provenance require an explicit core inspection addition. Never infer them
    from the rendered UI.

## P1 — product-quality browser debugger

- [ ] **[P1][L][Engine work: No] Make large machine graphs navigable.**
  - **Owner:** `debug-model.ts`, `debug-layout.ts`, `debug-surface.ts`, and
    `shell.css`.
  - Build on the existing zoom/pan camera with search, semantic filters,
    fit-to-selection, and a compact overview or minimap.
  - Allow unrelated branches to collapse while preserving a stable route back
    to the full admitted machine graph.
  - Keep lane labels and current-receipt context visible while the canvas
    scrolls.
  - Preserve the existing roving-tab-stop and arrow-navigation contract.
  - Validate against the Instagram machine graph and at least one larger
    generated graph fixture; record node/edge counts in the test rather than in
    this backlog.

- [ ] **[P1][L][Engine work: No] Replace whole-graph rerenders with incremental
      receipt decoration.**
  - **Owner:** `debug-model.ts`, `debug-layout.ts`, and `debug-surface.ts`.
  - Cache static topology and geometry by
    `(machineProgramHash, focusDefinitionId)`.
  - Rebuild only when the admitted deployment or focused machine changes;
    otherwise patch node classes, status text, edge activity, summary, and
    selection details by stable ID.
  - Avoid layout reads after graph writes on ordinary receipt publications.
  - Add a repeatable performance fixture and define budgets for update time,
    allocations, and dropped frames before optimizing further.
  - Keep the accessible SVG-edge/HTML-node renderer unless profiling isolates
    edge paint as the bottleneck. A Canvas edge layer is a measured fallback,
    not an automatic renderer switch.

- [ ] **[P1][S][Engine work: No] Make live-machine following explicit.**
  - **Owner:** `debug-model.ts` and `debug-surface.ts`.
  - The runtime machine from the admitted deployment is the sole live target.
    Imported machines may be pinned for static inspection, but must never be
    decorated as if they were the running instance.
  - Rename UI copy if “Follow live” can be mistaken for route, page, component,
    or DOM focus.
  - Cover switching between the running machine and a pinned imported machine,
    then returning to the newest receipt without changing the session.

- [ ] **[P1][M][Engine work: No] Add checked-in real-browser and accessibility
      regressions.**
  - **Owner:** `tests/` plus the repository's browser-test harness.
  - Exercise the actual Play route: open/close, lazy subscription, genesis and
    reaction highlights, machine pinning, Follow live, historical mode,
    keyboard graph navigation, and disposal.
  - Include one adapter-delivered port input and one emitted port command so the
    test crosses the real `adapter-host.ts` boundary.
  - Cover wide right-dock, narrow bottom-dock, and compact takeover layouts,
    including bounds and overflow assertions.
  - Add an automated accessibility audit and a short manual
    assistive-technology checklist; unit DOM contracts are not a substitute for
    either.
  - Add targeted visual snapshots only for layout states whose geometry is part
    of the contract.

- [ ] **[P1][M][Engine work: No] Turn admitted source spans into source
      navigation.**
  - **Owner:** `debug-surface.ts` for the interaction; the safe source contract
    is owned by the [CLI Play-host TODO](../../../crates/uhura-cli/src/cmd/TODO.md).
  - Show a small source excerpt and provide an Open-in-Editor action.
  - Bind every excerpt to the machine program/deployment identity that produced
    the inspected graph; never display current bytes against a stale span.
  - Treat UTF-8 byte offsets as bytes throughout the host boundary.

## P2 — measured hardening and optional capabilities

- [ ] **[P2][M][Engine work: No] Replace count-only browser retention with a
      measured byte budget.**
  - **Owner:** `inspection-store.ts` and the timeline UI.
  - Keep the existing hard publication-count ceiling as a safety backstop, but
    evict by measured payload size so large inspections cannot dominate memory.
  - Make truncation visible in the timeline and preserve complete correlated
    receipt/inspection pairs.
  - Measure representative projects before choosing defaults.

- [ ] **[P2][M][Engine work: Conditional] Export and reopen observational
      inspection sessions.**
  - **Owner:** `inspection-store.ts`, protocol types, and a small Play UI seam.
  - Export versioned host deployment identity plus retained correlated
    publications, with explicit truncation and redaction metadata.
  - Reopening an export is an offline viewer, not deterministic session replay.
  - Exact replay or runtime restoration needs an explicit session/checkpoint
    design; do not imply that observational records reproduce foreign effects.

- [ ] **[P2][L][Engine work: Yes] Visualize richer transition internals only
      after the machine protocol can prove them.**
  - **Owner:** browser presentation after the core inspection contract defines
    any evaluated guard, transition, or expression-provenance facts.
  - Until then, keep receipt decoration conservative and explain its limits in
    the details pane.
  - Do not revive a browser-side evaluator or a second trace schema.

## Non-goals for the browser backlog

- Reimplementing the Uhura evaluator in TypeScript.
- Rewinding the running `Session` by assigning old browser inspections.
- Inferring guards, internal transition paths, or foreign effects that receipts
  do not record.
- Letting an adapter provider bypass `adapter-host.ts` contract admission.
- Making inspection data safe for an untrusted or public Play deployment solely
  by hiding fields in the DOM.
