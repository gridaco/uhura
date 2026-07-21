# Uhura 0.4 conformance and implementation gates

- **Status:** Active candidate validation plan
- **Candidate:** [Uhura 0.4](README.md)
- **Source:** [Source and lowering](source.md)
- **Project and identities:** [Project, resolution, and identity](project.md)
- **Application:** [Application profile](application.md)

This document contains tests and gates, not additional semantics. A failing
test identifies a defect or an underspecified owning document; it cannot
choose behavior by itself.

## 1. Baseline rule

The language-neutral L0–L2 and A0 problem statements are the behavioral
baselines. Uhura 0.4 has one source frontend, one project admission path, and
one canonical runtime. Retired source spellings, migration modes, and
differential compatibility loaders are outside the conformance surface.

## 2. Required harnesses

### L0 — bounded counter

Must prove:

- exact configuration rejection;
- complete three-event domain;
- boundary no-op commits;
- exact observation labels and values;
- invariant preservation; and
- exact agreement with the frozen canonical and adversarial traces.

### L1 — river crossing

Must prove:

- algebraic data, option matching, and exhaustive match;
- exhaustive total-table construction and constructor-ordered values;
- refusal precedence;
- true abort stuttering;
- atomic candidate movement;
- ordered violation reporting;
- reversible safe states; and
- non-absorbing solved state.

### L2 — keyed task supervisor

Must prove:

- permanent identity and attempt correlation;
- strict FIFO and concurrency two;
- normalized progress;
- first-terminal-wins classification;
- ordered cancellation and start commands;
- finite commit reconciliation; and
- canonical replay after checkpoint restore.

### A0 — Return Desk

Must prove:

- practical application ownership;
- typed ports and later settlement;
- routing as explicit framework vocabulary plus host capability;
- commit, duplicate, stale, invalid, and blocked behavior;
- refusal and pending-state presentation through committed modeled state,
  never an aborted draft or receipt lookup;
- static examples and checkpoints; and
- editor-readable ownership and interaction graphs.

### Instagram

Instagram is broad dogfood, not a language authority. Its 0.4 role is to
exercise:

- multiple source modules and parts;
- separately inspectable page, component, and surface presentations; reusable
  component invocation remains outside this gate until the UI grammar selects
  its exact props, event-interface, and children contract;
- shared session, navigation, notice, request, and feature state;
- cross-part draft-read and update dependencies plus committed observation;
- UI input construction;
- provider and browser adapters; and
- preservation of the existing Editor and Play workflows.

## 3. Source-composition equivalence probes

These are two different claims and must be tested separately.

### File-layout invariance

At least one non-trivial harness has the same logical machine, part
declarations, public names, composition names, and dependencies in two
layouts:

1. all declarations co-located in one source file; and
2. the same declarations split across source modules reached through `use`.

The two layouts must lower to identical semantic machine IR and program
identity. With the same configuration, fixtures, runtime instance identity,
sequence origin, and ordered inputs, they produce identical:

- admission result;
- observation sequence;
- commit/abort/fault classifications;
- outcomes;
- state values;
- ordered commands;
- checkpoint values; and
- semantic receipt identities.

Only non-semantic source provenance may differ. Moving one physical file and
updating the logical-module map is a smaller instance of this test. Renaming a
logical module and updating `use` paths is another.

### Flat-versus-part behavioral equivalence

At least one harness also has:

1. one flat root-owned machine; and
2. one machine decomposed into named parts.

The fixture declares an explicit mapping between their state, input, command,
and observation paths. Under that mapping they must produce the same admission
classification and observable reaction traces. Program identity, raw
checkpoint shape, source attribution, and receipt identity may differ because
the part names are semantic paths. This test establishes that authored
modularity does not add a scheduler or change behavior; it does not pretend
that two different logical topologies are the same program.

## 4. Project, identity, static, and non-local checks

### Project and resolution rejection fixtures

The closed project boundary has isolated rejection fixtures for:

- missing `uhura.toml`, `[project]`, or `[modules]`;
- an unknown project-manifest key;
- invalid package name, non-positive compatibility version, or language other
  than exact `"0.4"`;
- an empty, invalid, reserved, duplicate, or case-ambiguous logical module;
- duplicate physical mapping, wrong extension, missing file, invalid UTF-8,
  unsafe path, symlink escape, or unmapped project-owned `.uhura` source;
- an unknown, empty, duplicated, unsafe, missing, or core-module-overlapping
  `[evidence.modules]` logical module mapping;
- dependency alias collision or use of reserved `crate` or `uhura`;
- a dependency without a lock, a lock without dependencies, or an unknown
  lock key;
- root package mismatch, missing, duplicate, unused, or wrong-version package
  records;
- package-integrity mismatch before dependency parsing;
- a vendored dependency manifest with `[assets]`, project-local `[icons]`, or
  any non-empty reserved package-resource integrity input;
- cyclic package resolution;
- unresolved, ambiguous, private, wildcard, dynamic, conditional, or
  side-effect-only source locators;
- package-global public-name collision, including collision through
  re-export; and
- an invalid source declaration locator or module-qualified host selector.

One positive lock fixture pins at least two path packages with a transitive
dependency. Permuting every manifest and lock table must preserve the resolved
graph and all semantic identities.

### Identity and provenance probes

The suite distinguishes `PackageId`, `PublicId`, `NodeId`, `SiteId`,
`MachineProgramId`, `PresentationId`, `DeploymentId`, `SourceRevisionId`, and
runtime instance identity. No assertion may call two of these merely
“the hash.”

Paired fixtures prove:

- moving a physical file and updating `[modules]` preserves public, node,
  site, and machine identities while changing physical provenance and
  `SourceRevisionId`;
- renaming a logical module and updating every `use` path has the same
  invariance;
- splitting and recombining declarations across modules preserves semantic
  machine IR and identity;
- renaming a dependency alias or vendored path while resolving the same
  package declarations preserves machine identity;
- comments and formatting change source revision and byte spans but preserve
  semantic identities;
- changing package name or compatibility version changes affected public and
  machine identities;
- renaming a public declaration or part composition path changes its semantic
  identity;
- changing a semantically ordered statement, invariant, field, or constructor
  position changes the applicable node or machine identity;
- changing an unused declaration or lock record does not change an unrelated
  machine identity; and
- every semantic editor node joins to at least one valid provenance occurrence
  whose UTF-8 byte range lies inside its captured source.

The semantic-IR comparison uses the canonical semantic projection, not a
transport artifact that may contain a provenance sidecar.

Presentation fixtures separately prove that an internal machine change with
an identical UI interface preserves `PresentationId` while changing
`MachineProgramId` and the combined deployment, whereas an input or
observation interface change invalidates `PresentationId`.

### Static rejection fixtures

Every rejection required by
[Source and lowering §7](source.md#7-required-checks-and-non-local-obligations)
has its own isolated fixture. In particular, coverage must distinguish:

- committed observation used as a draft-read dependency;
- a public constant inside a machine or part;
- undeclared, cyclic, or wrong-owner `Reads` and `Updates` dependencies;
- part-argument arity/type mismatch or part/port configuration binding that
  reads non-config data;
- omitted root outcomes with a non-empty composed input sum or an
  outcome-requiring source form;
- conflicting outcome signature or policy;
- unqualified or wrong-direction port constructors and same-name constructors
  in distinct qualified ports;
- discarded or nested effectful update, handler fallthrough, and a public part
  update that exposes the enclosing `Outcome`;
- `before commit` in a part or outcome selection from reconciliation;
- order-revealing map/set transformation;
- non-literal, missing, duplicate, or unknown-key exhaustive table
  construction;
- a pattern binding escaping through `||`, `!`, or a false branch;
- non-`Nat`, non-strict, or path-incomplete loop decrease evidence;
- unresolved constructor qualification;
- Rust-style braced construction of a compact machine-domain variant, or a
  protocol payload arity/order mismatch;
- structural substitution between distinct source `struct` declarations, or
  invented per-field `pub`;
- missing, duplicate, ambiguous, or source-layout-dependent logical-module
  resolution.

Diagnostics must identify the authored concept, not only the flattened field or
generated constructor.

### Runtime and artifact probes

Separate probes must establish:

- state and commands authored before an abort remain unpublished;
- a fault receipt never carries a provisionally selected outcome;
- admission of a composed program is atomic;
- every diagnostic and editor node has authored owner and source provenance;
- a semantic fault site remains stable after physical and logical module
  moves;
- file moves, logical-module renames, and equivalent source splits preserve
  canonical semantic IR and `MachineProgramId`;
- source-revision identity changes when physical captured bytes or paths
  change, without entering receipts or checkpoint compatibility;
- the machine hash projection contains no logical module set or dependency
  alias;
- lock integrity and table order do not enter the hash wholesale;
- reordering non-semantic part declarations or struct literal fields preserves
  invariant selection and IR;
- reordering non-semantic port declarations preserves aggregate constructor
  ordinals, checkpoints, canonical encodings, and program hash; and
- a mixed root/part/port golden fixes the exact input and command constructor
  ordinals, including local-before-port order within every owner.

Application-phase probes additionally establish:

- `crate::PublicName` and a dependency-alias host selector hash by their
  resolved `PublicId`, not their authored spelling;
- a module-qualified host selector is rejected;
- `feed.api` is admitted only as the quoted exact composed port locator;
- missing, extra, duplicate, or wrong-contract adapter bindings reject the
  whole deployment;
- port-table and host-table reordering preserves `DeploymentId`;
- moving a stylesheet or provider module while preserving selected contents
  preserves `DeploymentId`; and
- changing entry name, typed configuration, presentation, adapter identity,
  contract-instance hash, provider configuration or contents, or stylesheet
  contents changes `DeploymentId`.

## 5. Familiarity and false-friend trial

The Rust-shaped candidate must be compared against a coherent
TypeScript-shaped challenger under equal budgets twice: first as a closed
paper/grammar acquisition gate before parser commitment, then against
executable frontends after implementation. Each trial receives:

- the same language-neutral task;
- one bounded semantic overview;
- one closed syntax/reference packet;
- representative but not task-identical examples;
- the same diagnostics and repair opportunity; and
- the same time or agent-context budget.

Measure:

- first parse/check success;
- semantic conformance;
- source size and concept count;
- invented Rust, TypeScript, JavaScript, Svelte, or Uhura constructs;
- false assumptions about mutation, borrowing, moves, lifetimes, traits,
  method lookup, macros, effects, panic, overflow, async, exceptions, objects,
  modules, ownership, literal types, collection mutation or iteration,
  part/port allocation, and tail-expression outcome selection;
- diagnostic repairs;
- edit locality on a controlled change; and
- transfer to an unseen task.

The repair prompts must deliberately probe likely false transfer: `mut` or
borrow syntax, per-field `pub`, braced machine-domain payloads,
`session.signed_in()`, nested update calls, dynamic `Table::from`,
`unreachable!()`, aliased or transitive `ui` activation, and Rust-style module
identity. A candidate that accepts or repeatedly elicits these forms has not
earned familiarity merely by resembling Rust.

The selected Rust shape is a hypothesis, not a systems-language argument. The
TypeScript-shaped challenger should replace it if the trial shows materially
better acquisition and repair without importing callbacks, promises,
exceptions, mutable objects, ambient browser authority, or unacceptable core
ambiguity.

## 6. Editor-consuming features

Before selection, one implementation must demonstrate that the lowered and
provenance IR supports:

- source ranges and stable diagnostics;
- module/part hierarchy;
- state ownership inspection;
- input/handler interaction graph;
- update, draft-read, and committed-observation dependency graphs;
- UI structure and input edges;
- exact observations and snapshots;
- global receipts with part attribution;
- checkpoint restore and replay;
- static example placement and preview;
- incremental source update; and
- rename/move refactoring without semantic drift.

The Editor must use the same checked program as Play. A docs-only graph or a
second UI parser does not pass.

## 7. Implementation sequence

1. Generate the draft grammar and run the equal-budget paper/grammar
   acquisition gate.
2. Freeze grammar and the project, locator, identity, provenance, and hash
   contracts.
3. Add the closed 0.4 `uhura.toml` reader, one mapped source module, parser,
   and source AST without changing reaction behavior.
4. Lower monolithic L0–L2 with package-global public identities, stable
   semantic node/site identities, a separate provenance sidecar, and the
   final `MachineProgramId` projection.
5. Execute the frozen L0–L2 traces and the monolithic identity gates.
6. Add the full logical-module map, `use`/`pub use`, vendored dependency
   resolution, and `uhura.lock`.
7. Pass file-layout, module-rename, dependency-alias, integrity, and
   provenance-equivalence probes.
8. Add parts, dependency interfaces, canonical owner composition, ports,
   dotted port locators, and host admission.
9. Pass flat-versus-part behavior, canonical aggregate-order, and deployment
   identity probes.
10. Add the `ui` profile, framework `use` declarations, presentation identity,
    UI provenance, and native evidence modules under their manifest role.
11. Port A0 and preserve its oracle comparison.
12. Port Instagram and validate Editor and Play.
13. Repeat acquisition, repair, and controlled-change trials against the
    executable frontend.

No phase may add a second reaction engine or use JavaScript execution to fill
a missing source semantic.

### Phase M0: monolithic L0–L2

Before a multi-file resolver exists, the implementation must already:

- require `uhura.toml` with one `[modules]` entry;
- remove source language and module headers;
- assign the root package `PackageId` and every public declaration its final
  package-global `PublicId`;
- keep logical and physical source paths out of semantic identifiers;
- compute final-form `NodeId`, `SiteId`, and `MachineProgramId` values;
- emit the `uhura-provenance/0` sidecar;
- reject non-standard dependency locators while `[dependencies]` is empty;
- run L0–L2 without `host.toml`; and
- pass formatting and physical-file-move invariance.

These identity decisions cannot be deferred to the module phase: doing so
would make the monolithic frontend another temporary language stack.

### Phase M1: modules and packages

Before parts or UI, the implementation must:

- consume the full explicit one-to-one logical-module map;
- enforce package-global public-name uniqueness and visibility;
- resolve inert `use`, aliases, and same-name `pub use`;
- load and verify the exact vendored package graph from `uhura.lock`;
- preserve semantic identity across source splits, logical-module renames,
  dependency-alias renames, and equivalent re-exports;
- reject every project and resolution fixture in §4; and
- retain complete module and reference provenance.

### Phase M2: parts, ports, and host

Before A0, the implementation must:

- compose canonical root and part owner paths;
- lower part reads and updates without another scheduler;
- assign canonical aggregate ordinals independent of source placement;
- resolve port contracts and immutable port configuration into machine IR;
- admit quoted dotted port locators through the exact `host.toml` schema;
- validate the complete adapter table atomically;
- compute `DeploymentId` from resolved identities and selected contents; and
- pass both composition equivalence suites.

### Phase M3: UI and application

Before Instagram, the implementation must:

- resolve direct unaliased `use uhura::ui;` activation;
- compute `PresentationId` separately from `MachineProgramId`;
- retain UI nodes and input edges in the same provenance sidecar;
- keep renderer preview pose outside semantic checkpoints;
- prove Editor and Play consume the same semantic and provenance artifacts;
  and
- pass deployment identity probes with selected UI resources.

## 8. Selection bar

The candidate remains worth carrying forward only if it:

- preserves every declared kernel guarantee;
- makes L0–L2 at least as clear and no less exact;
- makes A0 and Instagram materially easier to navigate and change;
- reduces hybrid or one-off syntax;
- supports modular source without runtime-topology inflation;
- preserves or improves editor-consuming artifacts;
- has bounded learnability for people and agents; and
- keeps false semantic transfer diagnosable.

Passing execution alone is insufficient. The rewrite exists to improve the
language product, not to rename tokens over the same authoring cost.
