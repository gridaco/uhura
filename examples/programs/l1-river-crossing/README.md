# L1 — River Crossing

- **Status:** Language-neutral program specification
- **Level:** L1 — one closed, exhaustively verifiable machine
- **Class:** Pure standalone program harness
- **Subject:** Deterministic state transition, refusal, invariant preservation,
  and trace replay
- **Implementation:** None inside this language-neutral problem; executable
  answers are indexed by the parent harness
- **Authority:** The problem contract is authoritative for candidate
  comparison; no Uhura behavior is accepted here

This document defines the problem and its observable behavior. It is the
authority against which candidate languages are evaluated. A candidate source
file, intermediate representation, runtime, or test adapter is an answer sheet:
if it conflicts with this document, the answer sheet is wrong.

The problem must not be reduced, reinterpreted, or given extra assumptions to
fit the current capabilities of any language. An answer may choose any
internal representation as long as it preserves the complete behavior below.

## Scope

A farmer must move a wolf, a goat, and a cabbage from the left bank of a river
to the right bank.

The farmer operates the boat. Every crossing carries the farmer and at most
one of the other three entities. The wolf may not be left with the goat
without the farmer. The goat may not be left with the cabbage without the
farmer.

This is a pure transition system. It has no clock, randomness, storage,
network, user interface, renderer, or other external authority.

## State

The entities are:

- farmer;
- wolf;
- goat; and
- cabbage.

Each entity is on exactly one side:

- left; or
- right.

A state is the complete position of all four entities. Position is the only
mutable problem state. Crossing count, history, and whether a state has been
visited are not part of the state.

The two banks and all entities are distinct. There is one farmer, one wolf, one
goat, and one cabbage.

### Initial state

All four entities are on the left bank.

| Entity | Side |
| --- | --- |
| Farmer | Left |
| Wolf | Left |
| Goat | Left |
| Cabbage | Left |

### Goal state

All four entities are on the right bank.

Reaching the goal makes the derived status `solved`. It does not change the
transition rules or make the state absorbing. A later legal crossing may leave
the goal, after which the derived status is again `in-progress`.

## Safety invariant

A state is unsafe if either of these conditions holds:

1. the wolf and goat are on the same side and the farmer is on the other side;
2. the goat and cabbage are on the same side and the farmer is on the other
   side.

Equivalently, with `position(entity)` denoting an entity's side, the forbidden
condition is:

```text
(position(wolf) = position(goat) and
 position(farmer) != position(wolf))
or
(position(goat) = position(cabbage) and
 position(farmer) != position(goat))
```

The initial state is safe. Every accepted transition must produce a safe
state. Unsafe assignments are outside the program's state domain.

## Inputs

The complete input domain contains four crossing attempts:

1. cross alone;
2. cross with the wolf;
3. cross with the goat;
4. cross with the cabbage.

Direction is not an input. A crossing always starts on the farmer's current
side and ends on the opposite side.

No other passenger is valid. An unknown passenger is outside the input domain
and must be rejected by the candidate language, checker, or harness before a
transition is evaluated.

## Transition semantics

Every valid input is evaluated atomically against one complete pre-transition
state.

For one crossing attempt:

1. If a passenger was named and that passenger is not on the farmer's current
   side, refuse the attempt with `passenger-not-with-farmer`. Do not construct
   or safety-check a tentative state.
2. Otherwise, construct one tentative state by moving the farmer to the
   opposite side and, when present, moving the passenger to that same side.
   All other entities remain where they were.
3. Evaluate both safety conditions against the complete tentative state.
4. If either safety condition is violated, refuse the attempt with `unsafe`.
5. Otherwise, accept the attempt and replace the current state with the
   tentative state.

Moving the farmer and passenger is one atomic transition. An implementation
must not expose or validate an intermediate state in which only one of them
has moved.

### Refusal behavior

A refused attempt leaves the state exactly unchanged.

There are two refusal forms:

- `passenger-not-with-farmer`, carrying the named passenger;
- `unsafe`, carrying the non-empty ordered list of violated relationships.

The canonical relationship order is:

1. `wolf-with-goat`;
2. `goat-with-cabbage`.

If both relationships would be unsafe, both appear in that order. The
passenger-location check has precedence: when the passenger is not with the
farmer, the result is only `passenger-not-with-farmer`; tentative-state safety
is not evaluated.

Refusal performs no crossing, changes no position, and emits no hidden retry,
timer, or other consequence.

## Required observations

After initialization and after every evaluated input, the program must expose:

1. the complete positions of farmer, wolf, goat, and cabbage;
2. the derived status, either `in-progress` or `solved`;
3. for an input, exactly one outcome:
   - accepted, including the passenger if any, departure side, and arrival
     side; or
   - refused, including the exact refusal form described above.

The status is `solved` if and only if all four entities are on the right bank.
It is otherwise `in-progress`.

Debug metadata may exist outside the problem observation, but it must not
alter state, transition behavior, replay, or the required observations.

## Canonical seven-crossing trace

`L` means left and `R` means right. Positions are shown after each accepted
crossing.

| Step | Input | Farmer | Wolf | Goat | Cabbage | Outcome | Status |
| ---: | --- | :---: | :---: | :---: | :---: | --- | --- |
| 0 | Initial | L | L | L | L | — | `in-progress` |
| 1 | Cross with goat | R | L | R | L | `accepted(goat,L,R)` | `in-progress` |
| 2 | Cross alone | L | L | R | L | `accepted(none,R,L)` | `in-progress` |
| 3 | Cross with wolf | R | R | R | L | `accepted(wolf,L,R)` | `in-progress` |
| 4 | Cross with goat | L | R | L | L | `accepted(goat,R,L)` | `in-progress` |
| 5 | Cross with cabbage | R | R | L | R | `accepted(cabbage,L,R)` | `in-progress` |
| 6 | Cross alone | L | R | L | R | `accepted(none,R,L)` | `in-progress` |
| 7 | Cross with goat | R | R | R | R | `accepted(goat,L,R)` | `solved` |

This is the canonical trace for comparison, not the only shortest solution.
The other shortest trace exchanges the wolf and cabbage trips.

## Exhaustive properties

These properties are part of the problem contract:

1. For every safe state and every valid input, evaluation terminates with
   exactly one accepted or refused result.
2. The next state and outcome are deterministic functions of the current state
   and input.
3. Refusal is a stuttering transition: the post-state is semantically equal
   to the pre-state.
4. Every accepted transition changes the farmer's side, changes the named
   passenger's side if there is one, and changes no other position.
5. Every state reached by an accepted transition satisfies the safety
   invariant.
6. Every accepted crossing is reversible by attempting the same crossing from
   its post-state.
7. Replaying the same initial state and input sequence produces the same state,
   outcomes, observations, and trace.
8. The left/right mirror of any legal transition path over safe states is
   legal under the same rules; the mirrored path starts from the mirrored
   state, not necessarily from the fixed initial state.
9. The goal is unreachable in fewer than seven accepted crossings.
10. Exactly two seven-crossing input sequences reach the goal from the initial
    state.

There are sixteen possible assignments of four entities to two sides. Exactly
ten satisfy the safety invariant, and all ten are reachable from the initial
state.

Using entity order farmer, wolf, goat, cabbage and encoding left as `0` and
right as `1`, the safe states are:

```text
0000
0001
0010
0100
0101
1010
1011
1101
1110
1111
```

The exhaustive transition oracle therefore contains forty evaluations: four
valid inputs for each of ten safe states. It contains:

- twenty accepted transitions;
- ten `passenger-not-with-farmer` refusals;
- ten `unsafe` refusals, of which:
  - four contain only `wolf-with-goat`;
  - four contain only `goat-with-cabbage`;
  - two contain both relationships.

## Required adversarial cases

At minimum, a candidate answer must demonstrate these cases in addition to the
canonical trace and exhaustive oracle:

1. From the initial state, crossing alone is refused as unsafe with both
   relationships listed in canonical order. State remains the initial state.
2. From the initial state, crossing with the wolf is refused as unsafe with
   only `goat-with-cabbage`. State remains unchanged.
3. From the initial state, crossing with the cabbage is refused as unsafe with
   only `wolf-with-goat`. State remains unchanged.
4. After the first canonical crossing, attempting to cross with the wolf is
   refused because the wolf is not with the farmer. Safety is not evaluated
   and state remains unchanged.
5. After the first canonical crossing, crossing with the goat is accepted and
   returns exactly to the initial state.
6. Repeating any refused input repeats the same outcome without accumulating
   hidden state.
7. An input naming anything other than wolf, goat, cabbage, or no passenger
   is rejected as outside the input domain.

## Candidate-language obligations

A candidate language or system must provide an answer sheet that:

1. implements this problem without changing its state, input domain,
   invariant, refusal precedence, outcomes, or observations;
2. stands alone without a widget catalog, document tree, renderer, service,
   database, framework application, or network provider;
3. has a deterministic, terminating semantic step for every safe-state and
   valid-input pair;
4. makes every state change causally attributable to one input;
5. preserves the atomic crossing semantics;
6. represents or checks the closed entity, side, input, outcome, and status
   domains without accepting invented values;
7. treats unsafe assignments as outside the program's state domain;
8. preserves refusal as an exact no-state-change result;
9. exposes state, input, outcome, and trace in a form that permits deterministic
   replay to be compared mechanically;
10. passes the canonical trace, all forty exhaustive evaluations, and every
    adversarial case;
11. uses no ambient clock, randomness, I/O, process state, or hidden global
    authority; and
12. explains any semantic defaults or ordering rules on which its answer
    depends.

The answer sheet may use a reducer, transition table, statechart, algebraic
data type, generated representation, or another model. This specification
does not prefer one. Compactness and readability are evaluated only after
semantic equivalence is established.

An answer that encodes only the canonical seven inputs, invokes a built-in
puzzle solver, or special-cases the expected final state does not conform.

## Non-goals

This harness does not define or evaluate:

- widgets, elements, markup, styling, layout, accessibility, or rendering;
- pointer, keyboard, gesture, animation, or boat-motion mechanics;
- an automatic planner, graph search, or proof-search algorithm;
- hints, scoring, crossing history, undo, or a user-facing game;
- persistence, restoration, networking, concurrency, timers, or randomness;
- authoritative product data or external effects;
- a particular source syntax, file extension, intermediate representation,
  runtime implementation, or code-generation target;
- whether one language design should use reducers, statecharts, actors, or
  another machine notation; or
- general computational universality.

The harness proves only what its contract and conformance cases establish. Its
purpose is to make a language account precisely for a small complete program,
not to let the program expand or shrink around the language.
