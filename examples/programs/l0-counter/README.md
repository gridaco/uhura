# L0 — Bounded Counter

- **Status:** Language-neutral program specification
- **Level:** L0 — one local transition system
- **Implementation:** None
- **Authority:** The problem contract is authoritative for candidate
  comparison; no Uhura behavior is accepted here

The problem statement, transition rules, and traces in this document are the
authority. A candidate language must fit the program; the program must not be
changed to fit the current language.

## Purpose

This is the smallest program in the language-design harness. It tests whether
a candidate can express one owned value, immutable configuration,
deterministic inputs, atomic transitions, derived observations, and explicit
boundary behavior without involving a view or host-language logic.

Passing L0 is necessary but says little about composition, effects, or open systems.

## Configuration and State

The immutable configuration is the integer triple:

```text
C = (minimum, maximum, initial)
```

It is valid exactly when:

```text
minimum <= initial <= maximum
```

An invalid configuration must be rejected before execution. It must not be
silently reordered or clamped.

The complete mutable state is one integer:

```text
S = count
```

The initial state is `count = initial`.

## Inputs

The program accepts exactly three inputs:

- `increment`
- `decrement`
- `reset`

Any other input is outside the program's closed input domain and must be
rejected before transition evaluation.

Every admitted input produces one terminating, atomic transition. Inputs at a
boundary are accepted no-op transitions, not errors and not missing handlers.

## Transition Semantics

For current state `count`:

```text
step(count, increment) = min(count + 1, maximum)
step(count, decrement) = max(count - 1, minimum)
step(count, reset)     = initial
```

`reset` always targets the configured initial value, not zero and not the
minimum. If the counter is already at that value, reset is an accepted no-op.

The program emits no commands, effects, or other external outputs. The harness
records the post-transition observation after every input, including accepted
no-ops.

## Invariant

The following must hold initially and after every transition:

```text
minimum <= count <= maximum
```

## Observation

The observable result is derived from the current state and configuration:

```text
{
  count,
  at_minimum: count == minimum,
  at_maximum: count == maximum
}
```

The two boundary flags are not independently mutable state.

## Canonical Trace

Given `C = (0, 2, 0)`:

| Step | Input | Observed `count` | `at_minimum` | `at_maximum` |
| ---: | --- | ---: | --- | --- |
| 0 | initial observation | 0 | true | false |
| 1 | increment | 1 | false | false |
| 2 | increment | 2 | false | true |
| 3 | increment | 2 | false | true |
| 4 | decrement | 1 | false | false |
| 5 | reset | 0 | true | false |
| 6 | decrement | 0 | true | false |

## Adversarial Traces

These cases are also required by this problem contract for candidate
comparison:

| Configuration | Inputs | Observed counts after each input |
| --- | --- | --- |
| `(7, 7, 7)` | increment, decrement, reset | `7, 7, 7` |
| `(-2, 1, -1)` | decrement, decrement, increment, reset | `-2, -2, -1, -1` |
| `(0, 2, 1)` | increment, reset, decrement, reset | `2, 1, 0, 1` |

For the degenerate configuration `(7, 7, 7)`, both `at_minimum` and
`at_maximum` are `true` in every observation.

The same valid configuration and input trace must always produce the same sequence of observations.

## Candidate-Language Obligations

A candidate solution must:

- express the complete configuration, state, input set, and transition rules
  without hidden host-language behavior;
- reject invalid configuration before the first transition;
- define all three inputs for every valid state, including boundary no-ops;
- make each input one finite, deterministic, atomic step;
- derive boundary observations rather than duplicating them as mutable facts;
- preserve the invariant for every possible finite input trace;
- permit deterministic replay from configuration plus input history;
- remain independent of buttons, widgets, rendering, and any particular interaction source.

A candidate may choose its own surface syntax. It may not weaken, reinterpret,
or omit a required behavior to produce a cleaner example.

## Non-Goals

L0 does not test:

- views, widgets, styling, or event binding;
- persistence, shared state, navigation, or instance lifetime;
- asynchronous work, commands, clocks, randomness, or external authority;
- collections, concurrency, cancellation, or machine composition;
- general computational completeness.

Those pressures belong to later programs in the harness.
