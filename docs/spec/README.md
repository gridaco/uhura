# Uhura specification

- **Status:** Pre-specification working draft
- **Target:** Unversioned
- **Owner:** [Uhura Working Group](../working-group/README.md)
- **Foundational RFC:** [RFC 0001](../rfcs/0001-project-foundation.md)
- **Historical workstream:** Frame

This is the living master document for **Uhura**, a declarative UI language,
checker/compiler, and deterministic headless experience runtime. Its canonical
source suffix is `.uhura`; the grammar and serialization are not yet accepted.

Uhura owns the semantics of non-authoritative UI-session state. It consumes
typed external projections, receives semantic events and command outcomes,
advances an experience machine, and produces a renderer-neutral semantic view
plus explicit commands and platform intents.

Uhura does not own authoritative domain state, authorization, transactions,
backend effects, or concrete rendering.

> **Maturity warning:** this document fixes a project boundary and a model for
> research. It does not define usable source syntax or a conforming runtime.
> Terms and examples below are conceptual until accepted by an RFC and backed
> by executable tests.

## 1. Normative language

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**,
and **MAY** are interpreted as described by
[BCP 14](https://www.rfc-editor.org/info/bcp14) only when capitalized.

Before Uhura 1.0, capitalized requirements are proposals. Stable semantics
require an accepted RFC, a versioned specification, and conformance tests.

## 2. Product model

The system has four independent responsibilities:

| Layer | Owns | Does not own |
|---|---|---|
| Spock | Durable product truth, guarded business transitions, authorization, transactions, server workflows, and backend effects | UI-session state or rendering |
| Uhura | Presentation semantics, UI-session state and transitions, external port requirements, semantic view evaluation, and UI traces | Durable domain authority, direct I/O, or pixels |
| Renderer and host drivers | Layout, paint, native controls, input normalization, device mechanics, and execution of explicit platform intents | Product truth or experience transition semantics |
| NCC | Human-facing authoring, contract linking, fixtures/scenarios, provenance, infinite-canvas projection, cross-artifact diagnostics, and playback | Redefining Spock or Uhura semantics |

Spock and Uhura are sibling language/runtime projects. Either can be checked or
tested independently. NCC composes them through versioned contracts.

Uhura originated in the **NCC** repository (itself renamed from Wire under a
separate migration that defined repository, package, CLI, documentation, and
compatibility scope — see the
[Wire → NCC migration note](https://github.com/gridaco/ncc/blob/main/docs/migration-wire-to-ncc.md))
and now lives in the Spock repository as a sibling workspace.

## 3. Uhura project components

Uhura is one semantic project but not one undifferentiated implementation.
Conforming work is expected to separate:

1. **Source language:** presentation, components/templates, explicit UI
   machines, bindings, and imported ports.
2. **Checker/compiler:** parsing, resolution, type checking, static analysis,
   extraction, lowering, formatting, and diagnostics.
3. **Checked IR:** a versioned, target-neutral executable representation.
4. **Uhura Core:** the deterministic and I/O-free experience runtime.
5. **Renderer protocol:** semantic view snapshots or patches and semantic input
   events.
6. **Host-driver protocol:** service commands and platform intents whose
   effects occur outside the core.
7. **Bindings:** adapters for Spock contracts, fixtures, and other compatible
   service providers.
8. **Widget catalogs:** independently versioned semantic capabilities informed
   by proven implementations without copying their runtime APIs.

Keeping these artifacts in one project gives them one semantic authority.
Keeping their contracts separate prevents UI behavior, platform behavior, and
backend behavior from leaking into each other.

Presentation, UI machines, and external ports SHOULD remain separately visible
in source or modules. Co-location MUST NOT make widget event attributes a
hidden general-purpose programming language.

## 4. What Uhura may describe

Subject to future grammar RFCs, Uhura is intended to describe:

- semantic widget trees, properties, slots, accessibility, and interaction
  intent;
- reusable components or finite presentation templates;
- immutable lexical values, collection rendering, and structural selection;
- typed reads from explicit external projections;
- non-authoritative UI-session state and its initial values;
- event-driven UI transitions, guards, and local derived values;
- pages, logical routes, and surface orchestration such as dialogs, sheets,
  popovers, menus, tooltips, prompts, and toasts;
- form drafts, local validation presentation, submission lifecycle, pending and
  error states;
- optimistic overlays that remain subordinate to versioned external truth;
- typed service commands and platform intents, without implementing their
  effects;
- typed handling of command outcomes and external projection updates;
- renderer-independent focus, scroll, navigation, and announcement intents;
- references to external localized messages; and
- bounded fixture/scenario projection for a static infinite canvas.

Runtime interaction may continue indefinitely across events. Every individual
core step, source check, and bounded static projection must terminate within
specified resource limits.

## 5. What Uhura cannot own

Uhura source and Core MUST NOT:

- define authoritative records, database schemas, permissions, or security
  policy;
- commit a transaction or claim a business operation succeeded before its
  authoritative outcome arrives;
- own durable cross-user or cross-device product truth;
- perform hidden network, storage, filesystem, clock, randomness, clipboard,
  URL-history, or device I/O;
- contain secrets or transport credentials;
- lay out, paint, hit-test, or instantiate platform-native controls;
- depend on DOM, React, Flutter, SwiftUI, or another renderer's object model;
- execute arbitrary host-language code, reflection, or ambient callbacks; or
- redefine an imported Spock command, projection, refusal, or outcome.

Uhura checkpoints may preserve a UI session without making that state
authoritative. Persistence alone is not authority: the question is whether the
state can alter product truth or another actor's reality.

## 6. State ownership

No fact may be authoritative in both Uhura and an external system.

| Concern | Semantic owner |
|---|---|
| Selected tab, modal/surface stack, local filter | Uhura |
| Form value, dirty/touched state, local validation display | Uhura |
| Submission pending state and correlation identifiers | Uhura |
| Optimistic overlay and rollback/rebase policy | Uhura |
| Logical page, route, or navigation state | Uhura |
| Durable record, accepted mutation, authorization result | Spock or another authoritative service |
| Shared workflow or resumable cross-device progress | Spock or another authoritative service |
| Hover, pressed animation, caret, IME, pointer capture | Renderer |
| Physical scroll offset, measurement, row realization | Renderer |
| URL/history, clock, network, storage, clipboard | Explicit host driver |

A concern can cross boundaries without gaining two owners:

- An optimistic `liked` flag is a correlated Uhura overlay until Spock accepts
  it; the accepted flag is Spock truth.
- A form draft is Uhura state; authoritative validation and commit are Spock
  behavior. A deliberately server-saved draft becomes Spock state.
- Infinite scrolling uses renderer viewport observations, Uhura request/window
  coordination, and Spock-owned query/cursor semantics and canonical records.
- Logical navigation belongs to Uhura; browser history is driven through an
  explicit host intent and returns location changes as events.

## 7. Formal core

Let:

- `P` be a checked Uhura program;
- `U` be the current non-authoritative UI-session state;
- `X` be the latest typed external projection set;
- `E` be one semantic event, external update, or command outcome;
- `V` be a stable-keyed, renderer-neutral semantic view;
- `C` be a finite ordered list of typed service commands;
- `I` be a finite ordered list of platform intents;
- `G` be diagnostics; and
- `T` be an observable execution trace.

The abstract Uhura Core step is:

```text
step-u(P, U, X, E) -> (U', V, C, I, G, T)
```

For fixed versioned inputs, the result must be deterministic. Core has no
ambient I/O. Drivers execute `C` and `I`; results return as later events or
projection updates.

An authoritative service such as Spock evolves separately:

```text
invoke-s(S, command) -> (S', outcome, projection-update, effects)
```

Uhura may predict a presentation result, but it cannot substitute that
prediction for `S'` or the authoritative outcome.

Concrete event queueing, run-to-completion semantics, conflict/reentrancy
rules, cancellation, command correlation, stale projection handling, and
checkpoint migration remain open RFC topics.

## 8. Headless rendering boundary

Headless does not mean handing raw state and source to each renderer. Uhura Core
must evaluate components, templates, collection rendering, structural
selection, bindings, and UI state into `V`. Otherwise every renderer would
silently become another Uhura interpreter.

`V` describes semantic controls and stable identity. A renderer maps it to
static shapes, web controls, or native controls and emits catalog-defined
semantic events.

The following remain renderer concerns unless a future portable contract
promotes a semantic intent:

- layout algorithms and text measurement;
- paint and compositing;
- native accessibility API mapping;
- hit testing and pointer capture;
- caret, selection, and IME mechanics;
- animation frames and physical scroll position; and
- virtualization and platform resource lifetime.

Canonical runtime behavior should be specified using revisioned full view
snapshots. View patches may be an optional transport optimization with an
explicit base revision and snapshot fallback.

## 9. Contract ownership

“Uhura is the contract holder” means that Uhura owns:

- source and checked-IR semantics;
- UI state, event, transition, and trace semantics;
- the types and linking rules for required external ports;
- semantic view and renderer protocols;
- host command/intent envelopes and settlement rules; and
- checkpoints, diagnostics, revisions, and compatibility rules.

It does not mean that Uhura owns every imported contract. Spock owns the
meaning of the projections, commands, outcomes, and refusals it exports. Uhura
declares requirements against those exports. A language-neutral linker checks
that a selected provider satisfies them.

Fixtures and alternative service implementations may satisfy the same required
ports. Uhura Core must not require Spock runtime objects or Spock source syntax.

NCC may host the linker and surface its diagnostics, but the linker should be
usable independently and must not invent compatibility.

## 10. Vocabulary

These terms are deliberately distinct:

| Term | Meaning |
|---|---|
| **projection** | A typed, externally owned read model supplied to Uhura |
| **event** | A fact delivered to Uhura, such as `save-requested` or `location-changed` |
| **transition** | A deterministic change to Uhura-owned UI state |
| **command** | A typed request to an authoritative service |
| **outcome** | The service-owned result or refusal returned for a command |
| **intent** | A typed request to a platform capability such as history, focus, or clipboard |
| **semantic view** | The evaluated, renderer-neutral widget tree |
| **renderer** | An implementation that realizes a semantic view on a target platform |
| **driver** | An implementation of explicit service or platform capabilities |

The generic term `action` should not collapse events, transitions, commands,
intents, and outcomes into one ambiguous mechanism.

## 11. Checkability and extraction

The toolchain is expected to parse and check, without running application code:

- grammar and module integrity;
- names, types, component/template calls, slots, and widget contracts;
- bindings against imported projection types;
- event and command payloads;
- machine states, reachable transitions, exhaustiveness, and obvious dead ends;
- command/outcome correlation shapes;
- stable keys for collection and component identity;
- required renderer and host capabilities;
- dependency and capability closure; and
- explicit resource limits for static projection.

It should be possible to extract a graph of views, UI states, events,
transitions, external reads, commands, outcomes, messages, and capabilities.
When linked with Spock exports, NCC can infer and validate where flows connect
and how authoritative data is consumed. Inference may propose links; only
explicit or mechanically proven links become part of a checked bundle.

Checkability does not prove usability, aesthetics, backend security, service
availability, or that a business workflow is desirable.

## 12. Authoring principles

Uhura is intended for human and LLM authors. The language should prefer:

- lowercase kebab-case names wherever the host grammar permits;
- explicit imports and lexical scope;
- a closed, regular grammar with no ambient context;
- one canonical formatter and stable source locations;
- structured, machine-readable diagnostics with deterministic repair spans;
- bounded constructs and explicit capabilities;
- semantic names rather than renderer-specific implementation names;
- separable presentation, machine, and port definitions; and
- version-pinned catalogs and contracts instead of implicit latest behavior.

The source format must be selected on these properties, not on novelty. The
`.uhura` suffix is reserved for canonical Uhura source. If a future RFC adds an
XML interchange serialization, it should use an explicit compound suffix such
as `.uhura.xml` rather than make XML the hidden meaning of `.uhura`.

## 13. Open decisions

The foundation deliberately leaves these unsettled:

- exact source syntax and serialization;
- component versus pure-template semantics;
- reducer, statechart, or hybrid UI machine model;
- expression language and totality restrictions;
- async command ordering, concurrency, cancellation, and retries;
- checkpoint and hot-reload compatibility;
- checked IR and stable ABI representation;
- widget taxonomy and catalog versioning;
- message and localization model, including MessageFormat 2;
- renderer and host capability negotiation;
- Spock import/export schema and linker ownership packaging;
- static projection bounds and scenario format;
- Rust/Wasm and TypeScript host packaging; and
- public naming, licensing, release, and repository extraction policy.

Each material decision requires focused research, examples, counterexamples,
and executable conformance cases before it enters a versioned specification.

## 14. Historical research evidence

The [Frame application-scale stress-test handoff](../working-group/frame-stress-test-handoff.md)
records which findings from the closed Frame workstream remain valid, which
responsibilities move into Uhura Core, and which Frame assumptions and syntax
must not be carried forward.
