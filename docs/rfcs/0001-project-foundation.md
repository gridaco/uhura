# RFC 0001 — Project foundation: UI language and headless experience runtime

- **Status:** Draft
- **Scope:** Project identity, responsibility boundaries, repository posture,
  and the minimum semantic model
- **Supersedes:** None
- **Superseded in part by:**
  [RFC 0004](0004-standalone-machine-core-and-source-composition.md), which
  makes Web UI an explicit profile over a standalone machine core
- **Related work:** Spock language/runtime; prior art in XAML, Svelte, QML,
  and Elm

## 1. Proposal

Establish **Uhura** as an independently buildable project incubating at the
`uhura/` repository root.

Uhura will own both:

1. a declarative language for presentation, non-authoritative UI-session state,
   experience transitions, bindings, and typed external requirements; and
2. a deterministic headless core runtime that checks and evaluates those
   semantics without performing concrete rendering or I/O.

Uhura is a greenfield proposal. It is not a rename, second syntax, or
source-compatible continuation of any earlier workstream.

The canonical source suffix is `.uhura`. Exact syntax is deferred. If a future
RFC adds an XML interchange serialization, it should use an explicit compound
suffix such as `.uhura.xml` rather than hiding XML behind `.uhura`.

## 2. Motivation

A presentation language that delegates all UI state and transitions to an
unspecified host creates the wrong abstraction boundary. Tabs, modal stacks,
form drafts, pending and error states, optimistic overlays, local navigation,
and pagination coordination are portable experience semantics. If Uhura does
not own them, each renderer or application host must recreate them, preventing
consistent checking, playback, tracing, and multi-renderer conformance.

Putting these concerns in Spock is also incorrect. Spock is the authority for
durable backend truth and guarded product behavior. Making a server runtime
the mechanic for open modals, selected tabs, field dirtiness, or scroll-driven
loading would introduce network coupling and mix discardable interface state
with durable product state.

Uhura therefore owns the experience machine while remaining a consumer of
authoritative product projections.

## 3. Responsibility split

The founding rule is:

> Uhura decides what the interface presents and requests next. Spock decides
> whether an authoritative product operation is valid and commits its result.

| Concern | Owner |
|---|---|
| Durable product state, policy, transactions, backend workflows | Spock |
| UI-session state, experience transitions, semantic view evaluation | Uhura |
| Layout, paint, native widget mechanics, device integration | Renderer/host drivers |

No state may be authoritative in both Spock and Uhura. An optimistic Uhura
overlay remains provisional until an authoritative Spock outcome or projection
update settles it.

## 4. Core semantic shape

Uhura Core is modeled as a deterministic, I/O-free step:

```text
step-u(program, ui-state, external-projections, event)
  -> next-ui-state
   + semantic-view
   + service-commands
   + platform-intents
   + diagnostics
   + trace
```

The core evaluates source semantics into a stable-keyed, target-neutral
semantic view. A concrete renderer maps that view to static shapes, web
controls, or native controls and returns semantic events.

Service and platform effects are requested explicitly. Drivers execute them
and return outcomes or environment changes as later events. Time, randomness,
network, storage, URL/history, clipboard, and device state are never ambient
core inputs.

## 5. Contract model

Uhura owns:

- language and checked-IR semantics;
- UI state, transition, event, and trace semantics;
- imported port requirement and linking rules;
- the semantic-view renderer protocol; and
- service-command and platform-intent envelopes.

Spock owns the semantics of the projections, commands, outcomes, and refusals
it exports. Uhura imports those contracts without redefining them. The linker
checks satisfaction using a language-neutral representation so fixtures and
other providers can implement the same ports.

## 6. Separation inside Uhura

One project does not imply one mixed source layer. Uhura must keep these
concerns explicitly separable:

- presentation and component/template structure;
- UI machine state and transitions;
- required external projections and command ports;
- platform capability requirements;
- widget catalogs;
- compiler/checker and checked IR;
- pure core runtime; and
- renderer and host-driver adapters.

Behavior must not be hidden inside renderer callbacks or arbitrary code in
widget markup.

## 7. Prior art and inherited requirements

An earlier internal application-scale stress study established useful
requirements: deterministic parsing, explicit typed dependencies, pure
templates, stable diagnostics, portable widget semantics, static projection,
and lowercase kebab-case source. Uhura should retain those properties where
they survive scrutiny. The public prior art points the same way: XAML and QML
for closed, checkable declarative vocabularies; Svelte for markup-first
components over a compiled template language; Elm for the pure message-driven
state machine.

Uhura deliberately rejects the study's strongest consumer-only restriction. A
Uhura program may define non-authoritative UI state and transitions, and Uhura
Core executes them. Durable application state, business behavior, policy, and
effects remain external.

No earlier format governs Uhura. Any future migration from a legacy format
requires an explicit adapter and diagnostics; this RFC promises no automatic
or lossless conversion.

## 8. Repository posture

During incubation, `uhura/` is co-located but isolated:

- no nested Git repository or submodule;
- no implicit dependency on root workspace packages or configuration;
- independent manifests, lockfiles, toolchains, tests, and CI when
  implementation begins;
- integration only through versioned public contracts and fixtures; and
- a clean path for later history extraction or repository promotion.

“Standalone” means independently buildable and extractable. It does not mean
independent Git history, governance, license, or release process today.

No implementation manifest is added by this RFC. Rust is the preferred initial
core direction because it supports a portable native/Wasm engine and reinforces
the boundary, but implementation language and ABI are separate decisions.

## 9. Consequences

Expected benefits:

- one portable and testable owner for UI-state semantics;
- deterministic playback, replay, tracing, and static scenario projection;
- renderers that implement a semantic protocol rather than reinterpret source;
- a sharp authority boundary between UI optimism and backend truth;
- a language-neutral contract seam between Spock and Uhura; and
- an independently extractable project.

Expected costs:

- two cooperating state machines require explicit command correlation,
  revisioning, cancellation, refusal, and stale-update rules;
- the core/renderer boundary needs stable identity and reconciliation semantics;
- Rust/Wasm integration may slow early iteration compared with a TypeScript-only
  spike;
- UI checkpoints and language evolution require migration rules.

These costs are architectural work, not reasons to move UI state into Spock or
renderer-specific code.

## 10. Deferred decisions

This RFC does not accept:

- a source grammar or XML/non-XML choice;
- exact machine, expression, component, or module semantics;
- a widget taxonomy;
- a runtime ABI or Wasm serialization;
- an event ordering or concurrency model;
- a Spock interface-description format;
- a renderer implementation;
- a package/repository name in public registries;
- a license or release policy.

Each requires a focused RFC and conformance evidence.

## 11. Acceptance criteria

This foundation can be accepted when the project agrees that:

1. Uhura owns both UI language semantics and a headless core runtime.
2. Spock remains authoritative for durable product state and behavior.
3. Renderers receive a semantic view and never reinterpret Uhura source.
4. All I/O occurs through explicit commands or platform intents.
5. Spock and Uhura contracts are linked through a language-neutral boundary.
6. Uhura remains isolated and independently extractable during incubation.
7. Prior art is credited without implying compatibility or prematurely
   performing migration.
