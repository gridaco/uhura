# Uhura Project Workflows

Use these workflows to create, change, diagnose, and prove a current Uhura project without confusing deterministic design fixtures with live provider truth.

## Contents

- [Define the experience contract](#define-the-experience-contract)
- [Create a new project](#create-a-new-project)
- [Modify an existing project](#modify-an-existing-project)
- [Diagnose and repair a failure](#diagnose-and-repair-a-failure)
- [Prove Canvas projection](#prove-canvas-projection)
- [Prove Play and provider behavior](#prove-play-and-provider-behavior)
- [Choose verification scenarios](#choose-verification-scenarios)
- [Report completion](#report-completion)

## Define the experience contract

Extract observable requirements before editing source:

1. Pages and logical routes, including dynamic parameters.
2. Reusable components and their typed props and emits.
3. Sheets, dialogs, popovers, or other surfaces and their ownership.
4. External projections needed to render each state.
5. Commands and declared refusals needed to change authoritative truth.
6. UI-session state, guards, optimistic overlays, and rollback behavior.
7. Loading, ready, empty, failed, pending, accepted, refused, and retry states.
8. Navigation push, replace, back, and focus-restoration behavior.
9. Pinned and derived examples needed for Canvas coverage.
10. Deterministic trace scripts and live-provider scenarios needed for proof.

Classify every fact before implementation. Spock or another provider owns durable product truth; Uhura owns reconstructible UI-session state; the renderer owns pixels and device mechanics.

## Create a new project

Uhura currently has no `init` command. Start from the smallest checked project shape that meets the request; do not copy the entire Instagram corpus unless the product genuinely needs it.

1. Confirm the target directory and do not overwrite existing files.
2. Inspect `examples/instagram-uhura` for accepted syntax and copy only relevant structural patterns.
3. Create `uhura.toml` with an app entry, catalog, named ports, fixtures, assets when needed, and a play profile.
4. Add or select a semantic catalog. Do not use arbitrary HTML elements or DOM events in `.uhura` source.
5. Define typed port contracts before source reads or sends provider data.
6. Create one definition per file:
   - `app/**/page.uhura` for routes;
   - `components/*.uhura` for reusable presentation;
   - `surfaces/*.uhura` for mounted sheets, dialogs, and similar experiences.
7. Add typed props, emits, params, state, handlers, and markup using only accepted source forms.
8. Add fixture slices for projections and deterministic scripts for changed event paths.
9. Add pinned examples for meaningful static states and derived examples for reachable event-driven states.
10. Run formatter and checker. Generate `uhura.lock` only through the repository-supported workflow; never hand-edit hashes or silently delete an existing lock to bypass drift.
11. Trace each important success and refusal/failure path.
12. Build Canvas and inspect every affected preview.
13. Run Play with the intended live provider when the project declares one.

Do not claim a new project is complete merely because one happy-state page renders. The requested failure, pending, navigation, and surface behaviors require explicit examples or runtime scenarios.

## Modify an existing project

Preserve the existing model and prove both the baseline and the requested delta.

1. Find the project root and the command used by its scripts or documentation.
2. Read the complete affected source graph: manifest, lock, definition, examples, imported components and surfaces, ports, fixture slices, scripts, styles, provider adapter, and focused tests.
3. Run the existing `fmt --check` and `check --deny-warnings` baseline when the project is expected to be healthy. Record pre-existing failures separately.
4. Identify the smallest ownership-correct change. Do not move provider truth into a store merely to avoid a port or provider change.
5. Modify semantic events and handlers before renderer appearance when behavior changes.
6. Update every affected example and trace script. A new reachable state that has no proof artifact is unfinished.
7. Run the formatter and checker after the smallest coherent edit. Fix the earliest new diagnostic first.
8. Run focused traces for the changed paths and inspect commands, intents, state writes, drops, and final hashes.
9. Build Canvas and inspect the preview provenance and visual delta.
10. Run Play against the intended provider and actor for changes that cross the provider seam.
11. Run focused crate tests, then broader workspace tests proportional to implementation changes.
12. Preserve unrelated working-tree changes and generated browser assets unless the task requires rebuilding them.

When a port contract changes, review the lock, fixtures, scripts, provider implementation, Spock contract, browser provider tests, and every source import. A port edit is not isolated to its TOML file.

## Diagnose and repair a failure

1. Reproduce the exact failing command from the same checkout and project root.
2. Separate source/checker failures from environment failures such as occupied ports, stale Wasm, missing provider servers, CORS, or browser/provider protocol mismatch.
3. For source failures, fix the earliest parse, resolution, type, catalog, contract, or example diagnostic first.
4. For trace failures, identify the first divergent input and inspect handler selection, guards, state writes, commands, outcomes, intents, and hashes.
5. For Canvas failures, determine whether checking, example resolution, replay, static projection, asset resolution, or browser rendering failed.
6. For Play failures, inspect the final server output and browser console before changing `.uhura` source. A successful readiness message followed by bind failure is not a running Editor.
7. For provider failures, capture the provider module, configured URLs, actor, request envelope, response or refusal, projection revision, and browser error.
8. Make the narrowest repair, rerun the original reproduction, and then run one neighboring success or regression scenario.
9. Rebuild Wasm only when core, ABI, or browser bundle compatibility requires it.

Do not hide unsupported semantics in CSS, fixtures, untyped provider state, or renderer callbacks. Record the missing language/runtime capability instead.

## Prove Canvas projection

Canvas is a deterministic, read-only projection of checked examples. It is not proof of live provider behavior.

1. Run `fmt --check` and `check --deny-warnings` first.
2. Run `project` into a temporary output directory.
3. Confirm `canvas.html` is produced without a provider running.
4. Inspect the affected preview groups, names, sizes, notes, origin, `from` provenance, and declared interactions.
5. Confirm derived examples replay from their declared ancestor without blocked descendants or hidden drops.
6. Compare preview counts and replay-derived counts when the change is expected to alter them.
7. For renderer changes, run focused project tests and inspect the generated DOM/CSS or a browser screenshot in addition to string/golden assertions.

Canvas projection must not emit commands, mutate provider truth, or require network I/O.

## Prove Play and provider behavior

Play runs the actual Uhura machine and may use a live provider. Test it separately from Canvas.

1. Start the required authority first, then start Uhura Editor or Play.
2. Confirm the final process output shows successful binds; stop stale processes or select explicit free ports.
3. Open `/play`, choose the intended actor when supported, and restart the Uhura session before the focused scenario.
4. Exercise the exact semantic event path.
5. Verify the pending or optimistic state before settlement when applicable.
6. Verify the accepted result or declared refusal and the provider-owned projection consequence.
7. Restart Play and distinguish session reset from provider-data persistence.
8. For Spock integration, inspect Studio or the affected endpoint to prove durable truth changed only on the provider side.

The current `X-Spock-Actor` seam is development impersonation, not production authentication or authorization proof.

## Choose verification scenarios

Use every row that applies.

| Changed surface | Required proof |
| --- | --- |
| Page, component, surface, or CSS | Format, check, affected examples, Canvas inspection |
| Handler, guard, or local state | Focused success trace, failure/refusal trace, final state/view hashes |
| Optimistic command flow | Pending state, exactly one command, accepted settlement, refusal/unavailable rollback |
| Projection availability | Loading, ready, failed, and empty states as applicable |
| Navigation | Push/replace/back trace and retained/initialized state behavior |
| Surface | Open, stack ownership, dismissal, cascade closure, and focus restoration |
| Port contract | Check, lock review, fixtures, scripts, provider compatibility, consumer imports |
| Fixture or script | Check plus focused trace showing deterministic matching and settlement |
| Canvas renderer | Project generation, focused renderer tests, preview metadata, visual inspection |
| Browser Play or provider | Web checks, live provider and actor scenario, browser console/network evidence |
| Core, IR, ABI, or Wasm | Focused Rust tests, Wasm rebuild, native/Wasm parity, workspace tests |
| Spock-backed experience | Spock check, Editor, Play, Studio, affected read, affected command |

## Report completion

Provide a compact handoff with:

1. Files created or modified.
2. Experience behavior implemented or repaired.
3. Exact format, check, trace, project, Play, web, and test commands that apply.
4. Preview counts, replay-derived counts, trace paths, and provider/actor used when available.
5. Evidence for both successful and refused/failed paths.
6. Provider or Spock changes and generated artifacts changed or intentionally unchanged.
7. Current language, Canvas, runtime, security, or tooling limitations exposed by the task.

Do not report completion while required checks fail, a started server is unmanaged, Canvas and Play are being treated as the same proof, or the result depends on unimplemented syntax.
