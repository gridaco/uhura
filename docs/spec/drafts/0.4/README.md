# Uhura 0.4 incubation candidate

- **Status:** Executable, disposable language-design candidate
- **Compatibility:** None; `0.4` is a candidate identity, not a released
  compatibility version
- **Differential baseline:** Uhura 0.3
- **Doctrine:** [Uhura doctrine](../../../doctrine/README.md)
- **Core/topology decision:**
  [RFC 0004](../../../rfcs/0004-standalone-machine-core-and-source-composition.md)
- **Problem authorities:** [L0–L2 programs](../../../../examples/programs/) and
  [A0 Return Desk](../../../../examples/applications/a0-return-desk/)

This directory is the single active exact design for the next Uhura language
experiment. It preserves the tested 0.3 reaction kernel while replacing the
hybrid 0.3 authoring grammar with one coherent surface and adding checked
source composition.

The candidate starts from one observation:

> Runtime topology and authored topology do not need to have the same shape.

The runtime receives one global machine program. Source may still be split,
namespaced, resolved through `use`, privately owned, and checked through
explicit dependencies. The compiler is responsible for resolving that
authored graph and lowering it to the one machine IR.

## Design hypothesis

Use only two familiar parent patterns:

1. **Rust shape** for modules and visibility, declarations, functions,
   immutable locals, expressions, algebraic data, matching, and closed struct
   updates.
2. **Svelte shape** for `ui` markup, interpolation, structural selection,
   keyed repetition, and styling.

Uhura owns syntax only where the borrowed languages cannot state the required
semantics honestly:

- `machine` and compile-time `part`;
- declared events, commands, ports, state, observation, and outcomes;
- atomic `on` reactions;
- commit/abort outcome policy;
- deferred ordered `emit`;
- invariants, total matching, termination evidence, and `before commit`;
- checked in-transaction `update`; and
- semantic UI event-to-input construction.

Familiar spelling does not import Rust execution semantics or Svelte
lifecycle.
The candidate language remains closed, deterministic, total within its stated
bounds, and free of ambient authority.

### Why Rust shape for the core

Rust's algebraic sums, exhaustive matching, immutable-by-default bindings,
block expressions, explicit state change, and exact-data orientation resemble
the kernel more closely than TypeScript's object, callback, exception, and
promise model. `use`, `pub`, `fn`, `let`, `struct`, `enum`, and `match` let
people and agents transfer familiar reading rules without making Uhura look
like executable JavaScript.

The borrowing is deliberately shallow. Uhura has no references, lifetimes,
ownership moves, borrow checking, `mut` locals, traits, `impl`, macros,
panics, machine-word overflow, destructors, threads, `async`, or `unsafe`.
Struct updates are persistent value copies. Methods are a closed total
prelude, not trait dispatch. Uhura keeps machines, events, outcomes, state,
atomic reactions, commands, ports, parts, invariants, and commit policy as
visible language concepts rather than encoding them as Rust libraries or
attributes.

Svelte remains the better parent for presentation because HTML-shaped source
and its structural blocks already match the Web authoring problem. Expressions
inside `ui` still use Uhura's checked core expression language; they are not
JavaScript.

This remains a falsifiable ergonomics decision. The
[conformance plan](conformance.md#5-familiarity-and-false-friend-trial)
keeps an equal-budget TypeScript-shaped challenger as a control.

## Fixed candidate decisions

- The [kernel](kernel.md) is source-neutral and retains one finite,
  non-reentrant, atomic reaction.
- The [source language](source.md) uses explicit `use`/`pub` boundaries and
  removes per-file `language` and `module` headers. Project and dependency
  identity belong to the manifest and lock.
- Local machine input is declared as `events`; an omitted `commands` block
  means an empty local command domain.
- Named pure code uses `fn`; `|value|` exists only as a non-escaping
  collection binder. Closed constants use `const`, immutable locals use `let`,
  owned draft state uses assignment, block tails return values, and explicit
  `return` is lexical.
- Pure public state projection remains named `observe`; `view` is the binding
  name normally used by UI source. Cross-part current-draft reads use the
  separate generated `Reads` interface.
- `part name = Declaration(...)` is a source ownership and composition
  declaration. It never creates a runtime actor or independently scheduled
  child.
- Parts state their required commit/abort outcome policies explicitly with
  `requires outcomes`.
- Use declarations are inert. They resolve checked vocabulary and cannot
  initialize, subscribe, perform I/O, or acquire host authority.
- Semantic program identity is computed from checked resolved IR. Physical
  source layout and comments are provenance, not behavior identity.
- Web presentation is the explicit `ui` profile, activated by inert
  `use uhura::ui;`. Framework behavior remains feature-by-feature `use`.

## Documents

| Document | Owns |
| --- | --- |
| [Kernel](kernel.md) | Source-neutral values, reactions, outcomes, publication, faults, ports, receipts, and checkpoints |
| [Source and lowering](source.md) | Core source semantics, modules, visibility, parts, dependencies, static checks, and source-to-IR lowering |
| [Core grammar](grammar.ebnf) | Exact lexical rules, precedence, and accepted core phrase structure |
| [Project and identity](project.md) | Manifest, module map, dependency lock, resolution, semantic identity, and provenance |
| [Application profile](application.md) | `ui`, framework features, host authority, evidence, static examples, and editor-facing projection |
| [Conformance](conformance.md) | L0–L2, A0, source-composition equivalence, negative checks, acquisition tests, migration, and implementation gates |
| [Acquisition packet](acquisition/) | Controlled Rust-shaped versus TypeScript-shaped familiarity and transfer trial |

No parallel primer, partial grammar, implementation diary, or second combined
contract is authoritative. The grammar is a syntactic appendix to
`source.md`; it cannot silently choose semantics. The examples are the
problem authorities; the acquisition packet samples whether readers can
transfer those semantics into either candidate surface.

If these documents disagree:

1. `kernel.md` owns semantic execution;
2. `source.md` owns authored spelling and lowering;
3. `grammar.ebnf` owns exact core phrase recognition;
4. `project.md` owns non-source resolution, identity, and provenance;
5. `application.md` owns explicitly activated application features; and
6. `conformance.md` identifies a defect but cannot silently choose semantics.

The candidate is blocked until the owning document is corrected.

## Terms

| Term | Meaning |
| --- | --- |
| **machine** | The complete transaction, replay, receipt, and checkpoint boundary |
| **module** | An inert logical compile-time source namespace and visibility boundary |
| **part** | A statically composed source owner of a namespaced machine contribution |
| **dependency** | A declared read or in-transaction update interface between parts |
| **component** | Pure UI reuse; never an implicit state owner |
| **instance** | One runtime instantiation of the complete lowered machine |
| **view** | The conventional UI binding for a machine's pure observation |

## End-to-end shape

```text
manifest + source modules + lock
  -> resolve use paths, public declarations, profiles, and framework features
  -> check types, ownership, visibility, totality, and dependencies
  -> compose one complete machine
  -> flatten namespaced contributions into one semantic MachineProgram
  -> execute with the deterministic kernel
  -> retain source and ownership provenance for diagnostics and tools
```

The compiled program is global because a complete program is necessarily one
value. That fact does not make every authored name globally visible or every
state path globally writable.

## Baseline and history

Uhura 0.3 remains the executable differential baseline while 0.4 is incubated;
it is no longer the current authoring frontend. The historical
[Relay B3 record](../relay-b3/) explains how the retained transaction model
was reached; it is not a second runtime or current syntax authority.

No 0.3 source is promised automatic migration. A migration tool is useful only
after this candidate's surface and semantic equivalence tests are accepted.
