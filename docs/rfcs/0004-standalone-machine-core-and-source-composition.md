# RFC 0004: Standalone machine core and source composition

- **Status:** Accepted
- **Decision date:** 2026-07-19
- **Scope:** Core-language dependency, Web UI activation, framework features,
  and the source/runtime composition boundary
- **Does not select:** Concrete machine syntax, package syntax, widget
  vocabulary, or a supported compatibility version
- **Supersedes in part:** The UI-mandatory premise of
  [RFC 0001](0001-project-foundation.md)

## Context

Uhura began as a frontend language whose behavior and presentation were
designed together. The language review found that this framing made the
transactional state-machine core harder to judge, test, and reuse
independently. It also encouraged source-file and UI topology to leak into
runtime reasoning.

The review established two separations:

1. presentation is a first-class application of the machine language, not a
   prerequisite of the machine language; and
2. authored modularity is a source and checker concern, not evidence that the
   runtime needs child machines, actors, nested inboxes, or another scheduler.

## Decision

### Standalone core

A conforming core Uhura program can declare and execute a deterministic state
machine without a renderer, widget catalogue, route framework, or UI source.
The core remains sufficient for the language-neutral L0–L2 program harnesses.

Uhura remains a frontend-focused product. Its standard application profile is
Web UI, and its authoring, editor, preview, accessibility, and widget work may
remain optimized for that domain. Product focus does not make presentation a
dependency of the semantic kernel.

### Explicit Web UI profile

Web presentation is enabled explicitly by the profile named `ui`. Web is the
only selected presentation target. The profile may add checked markup,
components, semantic interaction bindings, styles, surfaces, and Web-specific
capability contracts. It may not grant the machine ambient DOM, network,
storage, clock, history, or JavaScript authority.

Routing, links, location and search-parameter observation, data-loading
lifecycle, sessions, and other meta-framework semantics enter through explicit
feature imports. Activating `ui` does not activate them transitively.

### One semantic machine, modular source

A deployment entry lowers to one complete deterministic machine program:

```text
Configuration × State × Input
  -> Commit(Outcome, NextState, Commands)
   | Abort(Outcome)
   | Fault
```

Source may be split into modules and checked ownership parts. Those constructs
provide namespacing, visibility, dependency declarations, diagnostics, and
editor provenance. They do not imply runtime instances or communication.

The compiler resolves and checks the source graph, composes one complete
machine, and flattens it into the same global semantic IR. An in-transaction
update across source parts is a statically checked call lowered into the
current reaction. It does not enqueue an internal event, create a receipt, or
cross a host boundary.

The default policy is:

```text
global semantic IR
  + namespaced source ownership
  + explicit checked dependencies
```

Global storage is an implementation fact. Ambient global lookup and global
source visibility are not language features.

## Required static properties

A concrete source design incorporating this RFC must enforce:

1. exactly one source owner for every mutable state path;
2. no cross-part private-state read or write;
3. cross-part reaction reads through exported pure draft selectors, kept
   distinct from committed public observation;
4. cross-part writes through declared in-transaction update interfaces;
5. no cycle in update or derived-value dependencies;
6. inert imports that cannot initialize code or acquire host authority;
7. one complete resolved input, outcome, command, state, and observation
   domain after composition;
8. atomic admission of the complete composed program;
9. complete checkpoints and globally ordered receipts;
10. authored module, owner, name, and source-span provenance in diagnostics
    and inspection artifacts; and
11. semantic identity when the same declarations and logical composition
    names are moved, split across files, or recombined without changing the
    checked program.

## Consequences

- Pure machine harnesses no longer need a decorative UI wrapper.
- UI and framework evolution cannot silently change reaction semantics.
- Source can be organized for people, agents, and editor tooling without
  extending the runtime calculus.
- Dynamic runtime actors, independently scheduled child machines, discovery,
  supervision, and cross-instance protocols remain separate future features.
- A source design must distinguish physical file location, logical source
  names, semantic IR identity, and runtime instance identity.

## Non-decisions

This RFC does not select:

- `part`, partial declarations, modules, or another exact composition syntax;
- a package manager or registry;
- whether a project may host several independent machine instances;
- dynamic machine creation or lifecycle;
- a concrete `ui` activation token;
- an escape-hatch syntax; or
- a compatibility version.

Those choices belong to a named incubation candidate and must preserve this
boundary if adopted.
