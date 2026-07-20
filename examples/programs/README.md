# Program harnesses

- **Status:** Non-normative language-design corpus
- **Current form:** Language-neutral problem specifications plus
  non-authoritative comparison answers
- **Implementation status:** Executable Uhura 0.4 incubation candidate with a
  retained 0.3 differential baseline; no accepted stable language version
- **Scope:** The experience-machine language, not widgets or presentation

Program harnesses are small, pure, standalone problems used to design and
compare Uhura's behavioral core. Each program has meaning without Uhura, HTML,
a renderer, or a visual demo. The problem is the authority; every proposed
surface language, semantic kernel, and IR is an answer sheet.

If a candidate cannot express a program, preserve the program and record the
gap. Do not weaken an invariant, omit an adversarial trace, or introduce hidden
host behavior to make the candidate pass.

## Current portfolio

| Level | Program | Primary pressure |
| --- | --- | --- |
| L0 | [Bounded counter](l0-counter/) | One owned value, elementary events, guards, derived observation, and deterministic boundary behavior |
| L1 | [River crossing](l1-river-crossing/) | Compound state, legal moves, safety invariants, goal observation, and exhaustive reachability |
| L2 | [Asynchronous task supervisor](l2-task-supervisor/) | Keyed identity, shared coordination, ordered outputs, correlation, cancellation, and external event ordering |

The levels describe semantic pressure, not language maturity or user skill:

```text
L0  one local transition system
L1  one closed, exhaustively verifiable machine
L2  one open, keyed machine system
```

None is a widget exercise. A later visual or textual projection may make a
program pleasant to operate, but pixels and interaction controls are never its
oracle.

## Comparison baselines

| Baseline | Form | Purpose |
| --- | --- | --- |
| [Plain TypeScript](baselines/typescript/) | Zero-package pure functions executed directly by Bun | A familiar frontend-language control for semantic, ergonomic, learnability, and verbosity comparisons |

A baseline is an answer sheet, not an authority and not a proposed Uhura
implementation. It must preserve the same frozen behavior before its
readability or size is compared.

## Uhura answer sheets

| Language | Answer | Status |
| --- | --- | --- |
| Uhura 0.3 | [L0–L2 source](answers/uhura-0.3/) | Executable differential baseline |
| Uhura 0.4 | [L0–L2 source](answers/uhura-0.4/) | Executable incubation candidate; the 0.4 frontend passes the same frozen semantic traces |

The 0.4 source is specified in the
[active incubation candidate](../../docs/spec/drafts/0.4/) and checks,
executes, checkpoints, and replays against the same frozen problems through
the retained canonical engine.

Answer sheets remain subordinate to the three problem statements. Their
presence records an executable language claim; the versioned specification
remains the authority for supported syntax and semantics.

For apples-to-apples source measurements, count the complete authoring source
and every helper it relies on. Exclude tests, comments, documentation, build
configuration, generated files, boundary decoders, and compiler/runtime
implementation from both sides. Record those surfaces separately when they
materially affect guarantees or stewardship cost.
Use the shared [corpus source scanner](baselines/measure-source.mjs) for raw
line, token, and byte evidence.

Do not force a baseline into a candidate's preferred abstraction. Two answers
are comparable when they preserve the same state, inputs, outcomes,
observations, ordering, and boundary consequences—not when their declarations
have matching names.

## Optional comparison notation

The programs may be described using the abstract shape:

```text
P = (C, S, E, X, K, O, R, initial, step, observe)

initial : C             -> S
step    : C × S × X × E -> S × K × O*
observe : C × S × X     -> R
```

Where:

- `C` is immutable program configuration and may be unit;
- `S` is serializable program-owned state;
- `E` is one declared input event;
- `X` is declared external observation and may be unit;
- `K` is the step result, such as an accepted/refused outcome or an input
  classification, and may be unit when the program defines no distinct result;
- `O*` is a finite ordered sequence of requested consequences;
- `R` is the derived observation;
- `initial` derives the initial state from valid configuration;
- `step` performs finite work and has one result for fixed inputs; and
- `observe` is a pure description of the current meaning of the program.

This frame is an evaluation aid, not proposed Uhura syntax or an accepted IR.
A candidate may use reducers, statecharts, facts, composed machines, or another
model. It must preserve the specified observable behavior without ambient
authority.

Candidates are not required to expose, name, or lower to these parts. A
missing one-to-one mapping is not a failure when the complete problem behavior
is preserved.

The corpus does not claim that three examples prove universal computation.
It asks how candidate designs account for deterministic transitions, external
inputs, ownership, and keyed behavior. It does not establish which of those
concepts Uhura must expose directly.

## Required contents

Every program specification states:

1. the independent problem and its non-goals;
2. configuration and initial state;
3. admitted inputs, per-step results, and ordered outputs;
4. exact transition behavior;
5. derived observations;
6. safety invariants and any liveness assumptions;
7. canonical and adversarial traces;
8. finite exhaustive properties where applicable; and
9. obligations shared by every candidate language.

The Markdown specification is authoritative for the challenge. Future machine-
readable cases must be derived from it and checked for agreement rather than
quietly becoming a second problem definition.

## Candidate comparison

Before implementing competing language designs:

1. freeze all three problem specifications;
2. pre-register expected traces, exhaustive checks, and invalid cases;
3. give each candidate the same starting information and boundary adapters;
4. require every candidate eligible to win to satisfy every semantic case;
5. compare readability and compactness only among semantically conforming
   candidates; and
6. preserve failures and workarounds as evidence.

Conformance to each program's stated transitions, results, outputs, invariants,
and traces is pass/fail. Other qualities remain comparative evidence until
accepted through the language-change process. Source size alone cannot
compensate for incorrect or unspecified behavior.

Among conforming candidates, record:

- concepts and declarations an author must name;
- valid and invalid states the source can represent;
- cause-to-effect reading distance;
- duplicated facts and lifecycle bookkeeping;
- hidden defaults or authority;
- source, token, and checked-model size;
- diagnostic location and repair attempts; and
- edits required by the same controlled change.

## Future comparison protocol

The artifacts in this section are not part of the current Markdown-only
corpus. They should be fixed before candidate implementations are used to
select a language design.

Positive traces are insufficient. Future comparison should include planted
probes for:

- ambiguous transitions;
- non-terminating internal event cycles;
- mutation by a non-owner;
- ambient clock, randomness, storage, or network access;
- duplicate identities and correlation tokens;
- stale outcomes addressed to dead attempts or instances; and
- invalid normalized values.

For each probe, the protocol must state in advance whether a retained
requirement demands static rejection, bounded behavior, a particular
diagnostic or classification, runtime validation, tests, or only an explicit
record of reliance on author discipline. A probe is pass/fail only when its
guarantee was fixed before candidate work began; otherwise it is comparative
evidence.

After the initial comparison, each conforming solution receives the same
previously fixed change request. This checks whether the language modeled the
problem's concepts or merely made the original snapshot concise.

A separate held-out program should validate the selected design before
acceptance. Practical application transfer is then tested against the
language-neutral [A0 Return Desk](../applications/a0-return-desk/). The
[Instagram product example](../instagram/) remains broader dogfood and
integration evidence rather than a controlled comparison harness. None of
these steps permits retroactively changing the L0–L2 problems.

## Relationship to specifications and conformance

This corpus may motivate a language design, falsify one, or supply cases to a
future specification. It has no language authority by itself.

When a named Uhura version accepts relevant behavior:

- the specification defines that behavior;
- executable tests become the conformance oracle;
- these programs remain readable evidence; and
- candidate-specific source may be retained, replaced, or removed according to
  its continuing value.
