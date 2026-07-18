# Plain TypeScript comparison baseline

- **Status:** Non-authoritative comparison answer
- **Language:** Plain TypeScript executed directly by Bun, with no package
  dependency, project configuration, or state-machine library
- **Problems:** The frozen [L0–L2 program harnesses](../../)
- **Purpose:** A familiar control for semantic, ergonomic, learnability, and
  verbosity comparisons

This baseline asks the same question as an Uhura language candidate:

> How much author-visible machinery is required to express the complete L0,
> L1, and L2 behavior correctly?

[`baseline.ts`](baseline.ts) is ordinary TypeScript. It uses discriminated
unions, pure functions, readonly public state, copy-on-write collections, and
returned command arrays. It does not contain a custom reducer framework,
state-machine DSL, generic transaction runtime, renderer, clock, I/O, or
global store.

[`baseline.test.ts`](baseline.test.ts) derives executable checks from the
Markdown problems. It is evidence that the answer is conforming, not a second
problem definition.

## Why TypeScript

TypeScript is a useful control because it is widely used for frontend
behavior, familiar to human and model authors, and capable of expressing
closed input and outcome unions without a state-machine library. A new Uhura
language should demonstrate what it buys over this ordinary solution:
semantic compression, stronger guarantees, better diagnostics, deterministic
inspection, or lower recurring authoring cost.

This is not a claim that TypeScript is the smallest possible host-language
answer. It is intentionally conventional rather than code-golfed.

## Apples-to-apples boundary

The frozen Markdown behavior is the authority. The baseline is idiomatic
TypeScript rather than a transliteration of a proposed Uhura syntax:

| Program | Required TypeScript result |
| --- | --- |
| L0 | Next counter state; observation remains a separate pure function |
| L1 | Next state plus the exact accepted/refused outcome |
| L2 | Next state, exact classification, and ordered worker commands |

The comparison deliberately does not force L0 or L1 into an effect/result
envelope that their problem does not require. Conversely, it does not omit
L2 classification, correlation, command ordering, or scheduling just because
those make the source longer.

A paper candidate without a parser, checker, or executable semantics is not
marked behaviorally conforming merely because its source appears to cover the
same cases. Its raw authoring surface and proposed guarantees may be compared,
but that evidence remains provisional until the same oracle can execute
against it.

The following rules apply when measuring this baseline against a candidate:

- first require semantic conformance;
- compare [`baseline.ts`](baseline.ts) with the candidate's complete L0–L2
  authoring source;
- include every implementation helper in that source;
- exclude tests, comments, documentation, build configuration, generated
  files, and the candidate compiler/runtime from authoring-source metrics;
- measure boundary decoders separately, or exclude them from both sides;
- do not credit a guarantee that TypeScript or the candidate does not actually
  enforce; and
- record lines, lexical tokens, concepts, duplicated facts, cause-to-effect
  distance, and controlled-change cost rather than treating one size number as
  the verdict.

## Deliberate representation choices

- `bigint` represents mathematical counter values and attempt ledgers rather
  than silently adopting JavaScript's safe-integer ceiling.
- Progress arrives as JavaScript `number` so the program can classify `NaN`
  and infinities. Finiteness and inclusive `0..1` range are validated before
  attempt age.
- `ReadonlyMap` represents keyed task state. Accepted transitions copy the map
  and queue before changing them.
- Native `Map` iteration never determines FIFO scheduling; the explicit queue
  does.
- Refused, duplicate, stale, and invalid results return the original state.
  Only accepted L2 inputs run the scheduler.
- Commands are data. Running cancellation places `cancel` in the array before
  scheduling appends a replacement `start`.
- Runtime exceptions represent invalid initial construction or a violated
  internal invariant, never an L1 outcome or L2 classification.

## Guarantees and conventions

| Property | Plain TypeScript baseline |
| --- | --- |
| Closed typed inputs/outcomes | Checker, for typed callers |
| Exhaustive union handling | Checker through `assertNever` |
| Mathematical integer operations | JavaScript `bigint` runtime |
| Ordered queue and commands | JavaScript array semantics |
| Refusal/non-accepted stuttering | Source structure plus executable tests |
| Public readonly state | Checker only; casts or hostile JavaScript can bypass it |
| No accidental shared mutation | Source discipline plus tests, not the language |
| Function purity and termination | Review/tests, not the language |
| Invariant preservation | Executable oracle, not a publication barrier |
| Canonical codec, receipts, checkpoints, hashes | Not provided |
| Host command isolation and no reentry | Integration convention, not enforced here |
| Exact normalized decimal type | Not provided; progress uses IEEE `number` |

These differences are comparison evidence. They must not be hidden by calling
TypeScript “equivalent” merely because the traces pass.

## Verification

From the Uhura repository root, with Bun available:

```sh
bun run examples/programs/baselines/typescript/baseline.test.ts
bun run examples/programs/baselines/measure-source.mjs \
  --check examples/programs/baselines/metrics.json
```

The baseline is deliberately standalone. It does not add a package,
`tsconfig`, script, reference, or check to Uhura's web workspace.
Bun executes the semantic oracle but does not statically type-check it; this
comparison intentionally installs no TypeScript checker.

The executable checks cover:

- L0 canonical and adversarial traces plus invalid configuration;
- all forty L1 safe-state/input evaluations, refusal counts and ordering,
  reversibility, the canonical crossing, and the two shortest solutions;
- the complete twenty-six-input L2 adversarial trace, state invariants,
  deterministic replay, and every listed boundary case.

## Reproducible raw size

The shared corpus scanner is
[`../measure-source.mjs`](../measure-source.mjs). Run it against the complete
authoring source for every answer:

```sh
bun run examples/programs/baselines/measure-source.mjs \
  examples/programs/baselines/typescript/baseline.ts \
  path/to/candidate-source
```

It reports physical lines and bytes, then discards C-style comments and
whitespace to count code-bearing lines and approximate lexical tokens.
Identifiers, numeric and quoted-string literals, recognized multi-character
operators, and remaining punctuation each count as tokens under one scanner
for every answer in the current corpus. This is intentionally not
TypeScript's compiler tokenizer or a universal lexer. If a future answer uses
other comment, identifier, or literal rules, extend the scanner first and
remeasure every answer.

Current baseline measurement:

| Source | Physical lines | Code lines | Approximate tokens | Bytes |
| --- | ---: | ---: | ---: | ---: |
| `baseline.ts` | 650 | 555 | 2,671 | 14,824 |

The measured authoring source is only [`baseline.ts`](baseline.ts).
[`baseline.test.ts`](baseline.test.ts), this documentation, build
configuration, and the measurement tool are excluded, just as a candidate's
oracle, compiler, runtime, and measurement tooling are excluded. The raw
numbers are evidence, not a score; the guarantees table records work that the
two sources may not perform equally.

[`../metrics.json`](../metrics.json) records the baseline values, and the
repository check fails when the source and table drift. Line and byte counts
remain formatting-sensitive; approximate tokens are the strongest of these
raw size signals, not a substitute for the semantic and ergonomic criteria.
