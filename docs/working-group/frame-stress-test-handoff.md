# Frame application-scale stress-test handoff

- **Status:** Non-normative research handoff
- **Source workstream:** Historical Frame proposal
- **Destination:** Uhura Working Group
- **Evidence:** [Frame examples](../../../docs/frame/examples/README.md),
  [Instagram-scale corpus](../../../docs/frame/examples/instagram/README.md),
  [feedback](../../../docs/frame/examples/instagram/FEEDBACK.md), and
  [research questions](../../../docs/frame/working-group/application-scale-stress-test-questions.md)

This note carries reusable evidence from the closed Frame workstream into
Uhura without carrying forward Frame syntax or its consumer-only state boundary.

The source corpus is intentionally preserved under `docs/frame/`. Its
`.frame.xml` files are historical, non-runnable stress fixtures. They are not
`.uhura` examples, migration promises, or accepted widget catalogs.

## Why the corpus remains useful

The Instagram-scale source exercised 16 view roots, 10 surface roots, 15 pure
templates, 253 distinct external invocation names, 215 distinct external model
paths, 36 collection expansions, 44 structural matches, and 294 bindings.

Those counts do not validate Frame. They expose specification pressure across:

- feeds, pagination, refresh, and windowed collections;
- Stories, Reels, media playback, calls, and live presentation;
- forms, composers, creation flows, drafts, and publishing status;
- navigation, tabs, routes, modal surfaces, and transient feedback;
- search, maps, charts, analytics, and privileged platform capabilities;
- user-authored structured text, localization, accessibility, and IME; and
- optimistic interactions, realtime updates, cancellation, and failures.

The feedback is therefore useful as a requirements corpus even though Uhura
assigns several concerns to different owners.

## Findings retained directly

The following mechanism-neutral findings remain valid for Uhura:

1. **Explicit inputs:** time, locale, visibility, viewport facts, capability
   availability, randomness, and external outcomes must not be ambient core
   state.
2. **Deterministic composition:** modules and imports must be finite, pinned,
   acyclic, resource-bounded, source-located, and provenance-preserving.
3. **Visible dependencies:** components/templates cannot discover behavior or
   data through an active page, surface, ancestor, renderer object, or implicit
   binding context.
4. **Stable identity:** repeated content requires stable keys, duplicate-key
   diagnostics, and an explicit reconciliation contract.
5. **Catalog authority:** source cannot invent widget properties, slots,
   children, events, accessibility behavior, or renderer capabilities merely by
   naming them.
6. **Event eligibility:** generic layout nodes do not silently become
   interactive controls. Semantic events require declared keyboard, focus,
   pointer, and assistive-technology behavior.
7. **Safe content:** user- and service-authored content is inert typed data, not
   HTML, executable markup, or MessageFormat source.
8. **Surface identity:** a reusable surface definition is distinct from a
   mounted instance, its anchor, result correlation, modality, and focus
   restoration.
9. **Capability honesty:** media, maps, visualization, editors, drag/drop, and
   privileged device functions need explicit versioned capabilities and honest
   unsupported behavior.
10. **Projection honesty:** a static infinite canvas presents finite pinned
    scenarios. It cannot imply that time passed, I/O completed, or an effect
    occurred.
11. **Renderer separation:** layout, paint, measurement, hit testing,
    virtualization mechanics, caret, IME, and platform resource lifetimes stay
    outside the language core.
12. **Conformance before convenience:** shorthand may be accepted only after a
    canonical semantic model, stable diagnostics, and round-trip behavior exist.

## Findings reassigned for Uhura

Frame externalized all application state to a Host Contract. Uhura deliberately
uses a three-owner model instead:

| Stress-test concern | Uhura disposition |
|---|---|
| Selected tabs, logical navigation, modal stack | Uhura UI-session state |
| Form drafts, dirty/touched state, local validation display | Uhura UI-session state |
| Pending/error state, command correlation, optimistic overlays | Uhura, subordinate to authoritative outcomes |
| Pagination request/window coordination and local query state | Uhura |
| Durable records, permissions, transactions, accepted operations | Spock or another authoritative service |
| Query execution, canonical cursor meaning, shared workflow state | Spock or another authoritative service |
| Viewport observation, row realization, physical scroll anchor | Renderer/host protocol |
| Layout, paint, gesture recognition, caret, IME, device APIs | Renderer/host protocol |
| Contract selection, version pinning, fixtures, cross-link diagnostics | The orchestration layer |

The ownership test from the Uhura foundation remains decisive:

> If discarding the UI session could change product truth, authorization, a
> transaction, or another client's reality, the state is external authority.
> If it coordinates one experience and can be reconstructed without corrupting
> that truth, it belongs to Uhura.

## Findings not carried forward

Uhura does not inherit:

- Frame XML vocabulary, namespaces, widgets, properties, or event names;
- the claim that every page-, form-, tab-, route-, or surface-local transition
  must be declared outside the UI language;
- the omnibus Frame Host Contract as one owner for UI state, product behavior,
  routes, surfaces, effects, and outcomes;
- the assumption that every one of the corpus's 253 invocation names is an
  external action—many should become local events and Uhura transitions;
- `ui:list` as an accepted combination of semantics, scrolling, windowing, and
  continuation observation; or
- any implication that well-formed XML is checked, renderable, or playable.

## Required re-test

A future Uhura application-scale corpus should be authored only after the
source grammar, UI-machine model, checked IR, widget catalogs, and core/renderer
protocol reach a testable draft. It should then revisit at least:

- append, prepend, refresh, cancellation, stale outcomes, and anchor
  preservation;
- multiple instances of one surface and disappearing anchors;
- controlled input during IME composition and external replacement;
- optimistic commit, refusal, rollback, and projection rebase;
- logical navigation versus browser/native history;
- media visibility, playback, interruption, and accessibility;
- supported and unsupported privileged capabilities;
- closed and extensible value evolution;
- deterministic module resolution and hot-reload identity; and
- static scenarios versus live runtime traces.

Until then, the historical corpus is requirements evidence only. Uhura syntax
must emerge from its own RFCs and conformance work.
