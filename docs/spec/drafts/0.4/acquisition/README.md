# Uhura 0.4 bounded acquisition trial

- **Status:** Pre-parser evaluation packet
- **Candidate:** [Uhura 0.4](../)
- **Authority:** Evaluation protocol only; this directory defines no language
  semantics
- **Problem authorities:** The language-neutral
  [L0–L2 program harnesses](../../../../../examples/programs/) and
  [A0 Return Desk](../../../../../examples/applications/a0-return-desk/)

This directory tests whether a fresh author can acquire the Rust-shaped Uhura
0.4 source model from a bounded packet. It compares that candidate with the
existing zero-package
[plain TypeScript baseline](../../../../../examples/programs/baselines/typescript/).
The TypeScript arm is an existing-language control, not a second
TypeScript-looking Uhura grammar.

The trial is deliberately narrower than language conformance. Its source tasks
exercise representative, frozen slices of L0, L1, L2, and A0. They do not make
a participant's paper source an accepted answer sheet for the complete
harnesses.

## What this phase can establish

The pre-parser phase can record:

- comprehension of the shared deterministic-machine semantics;
- whether first source follows the supplied closed reference;
- semantic omissions visible against pre-registered checklists;
- false transfer from Rust, TypeScript, JavaScript, Svelte, or existing Uhura;
- the effect of one normalized repair opportunity; and
- source burden and edit locality.

It cannot measure real parser, checker, formatter, or diagnostic quality.
Before an executable frontend exists, validity and semantics are adjudicated
against the packet and rubrics by a reviewer who did not author the
submission. Those results must be labeled **paper evidence**.

The executable trial described by the 0.4
[conformance plan](../conformance.md#5-familiarity-and-false-friend-trial)
must later repeat the tasks through real frontends and full oracles.

## Packet integrity

The common overview and each arm reference are task-neutral. They intentionally
do not include the complete 0.4 `source.md`, because that document contains the
L0 answer and L1/L2 specimens used by this trial.

`protocol.json` fixes packet order, budgets, required submission files, frozen
problem references, and leakage terms. `run.mjs check` verifies every referenced
file, parses every oracle, scans the teaching files for task-answer leakage,
and reports the exact packet digest and approximate token count for each arm.

Examples teach equivalent non-task behavior. They are not normative and are
not copied into a participant's answer.

## Running the protocol

The runner uses only Node.js standard-library modules and never invokes an LLM:

```sh
node docs/spec/drafts/0.4/acquisition/run.mjs check
node docs/spec/drafts/0.4/acquisition/run.mjs prepare \
  --arm rust \
  --run pilot-rust-01
node docs/spec/drafts/0.4/acquisition/run.mjs validate PATH_TO_SUBMISSION
node docs/spec/drafts/0.4/acquisition/run.mjs score \
  PATH_TO_SUBMISSION \
  PATH_TO_ADJUDICATION.json
node docs/spec/drafts/0.4/acquisition/run.mjs summarize PATH_TO_RESULTS
```

`prepare` writes the prompt to standard output unless `--out PATH` is supplied.
The controller, not this runner, starts a clean human or agent session and
materializes its response using [the submission contract](common/response-format.md).

`score` reports a vector rather than an arbitrary weighted total:

- `C/10`: semantic comprehension;
- `V/4`: source artifacts that satisfy the paper syntax/static reference;
- `S/33`: frozen semantic obligations;
- `N-recognized/10` and `N-repaired/10`: false-friend performance;
- repair defects resolved, remaining, and introduced; and
- raw source burden and changed-file measurements.

Semantic eligibility requires all four validity checks, all 33 semantic
obligations, no hidden authority, and no unresolved hard defect after repair.
Compactness cannot compensate for a semantic failure.

## Running a study

Every run uses one fresh context and one arm. Record the exact model, settings,
date, prompt digest, first response, normalized diagnostics, repair response,
and adjudication. Do not expose `oracles/` or repository tools to the
participant.

The recommended sequence is:

1. run at least three sessions per arm to debug the packet;
2. revise and re-freeze the packet if the packet itself caused ambiguity;
3. discard pilot scores after any such revision;
4. for decision evidence, use randomized arm assignment and at least eight
   runs per arm across at least two model families; and
5. report human and agent trials separately.

A single plausible completion is not learnability evidence.

## Directory roles

| Path | Role |
| --- | --- |
| `common/` | Shared semantic overview and submission envelope |
| `arms/` | Arm-specific closed references, equivalent examples, and task scaffolds |
| `tasks/` | Language-neutral instructions derived from frozen harnesses |
| `oracles/` | Reviewer-only answer keys and adjudication rubrics |
| `results/` | Result-storage convention; no result is bundled into the language spec |
| `protocol.json` | Machine-readable packet and budget definition |
| `run.mjs` | Dependency-free integrity, preparation, validation, scoring, and summary tool |
