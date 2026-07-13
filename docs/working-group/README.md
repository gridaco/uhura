# Uhura Working Group

- **Status:** Forming
- **Scope:** Uhura language, checker/compiler, headless core runtime, semantic
  view protocol, host ports, widget catalogs, and conformance
- **Master document:** [Uhura specification](../spec/README.md)
- **Decision process:** [Uhura RFCs](../rfcs/README.md)

The Uhura Working Group is the stewardship and research space for the project.
It may prepare RFCs, examples, counterexamples, formal models, prototypes, and
conformance fixtures. Working-group material is non-normative until promoted
through an accepted RFC and incorporated into a versioned specification and
test suite.

## Responsibilities

The group is responsible for:

- preserving the authority boundary between Uhura, Spock, renderers, drivers,
  and NCC;
- researching source grammar and deterministic LLM/human authoring;
- specifying UI-machine and core runtime semantics;
- specifying semantic view identity and renderer interoperability;
- defining typed external-port and host-capability contracts;
- curating widget taxonomies with evidence from proven implementations;
- producing realistic application stress cases before freezing syntax;
- maintaining machine-readable diagnostics and conformance fixtures; and
- documenting tradeoffs, rejected alternatives, and unresolved questions.

## Working method

Substantial proposals should include:

1. the problem and explicit non-goals;
2. ownership and authority analysis;
3. a small formal or operational model where behavior is involved;
4. realistic examples and adversarial counterexamples;
5. static checking and runtime consequences;
6. renderer, host, Spock, and NCC boundary effects;
7. migration and compatibility consequences; and
8. executable conformance cases before acceptance.

The group should prefer lowercase kebab-case names wherever the selected host
grammar allows it. Examples must be marked proposed until their syntax has been
accepted.

## Research inputs

- [Frame application-scale stress-test handoff](frame-stress-test-handoff.md)
  preserves reusable requirements and explicitly reassigns state ownership for
  Uhura without treating historical Frame XML as Uhura syntax.

## Immediate research queue

- UI machine model: statecharts, reducers, or a constrained hybrid
- command ordering, cancellation, concurrency, correlation, and refusal
- checked IR and Rust/Wasm host ABI
- source syntax, module system, and canonical formatting for `.uhura`
- component/template identity and collection reconciliation
- semantic widget taxonomy and surface primitives
- forms, navigation, optimistic UI, offline behavior, and infinite scrolling
- Spock export/import contract representation
- deterministic static scenario projection for NCC
- message and localization model, including MessageFormat 2
- compatibility strategy for Frame and Wire v4 inputs

Implementation should follow accepted semantics closely enough to produce
evidence, but prototypes do not become specification by being first.
