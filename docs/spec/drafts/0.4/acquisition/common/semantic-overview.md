# Shared semantic overview

Treat this document and the arm reference as the complete teaching packet for
the trial. The task descriptions are language-neutral authorities. If an arm
cannot express an obligation, preserve the obligation and report the gap.

## Program model

A program owns immutable configuration, mutable serializable state, a closed
input domain, a pure observation, and—when needed—a closed ordered command
domain. One admitted input produces one finite deterministic step.

Conceptually:

```text
initial : Configuration -> State
step    : Configuration × State × Input
          -> Classification × State × Command*
observe : Configuration × State -> Observation
```

An answer may package these operations differently. It must preserve the
declared behavior rather than mimic this notation.

## Admission and boundaries

Configuration is validated before state exists. Invalid configuration is
rejected; it is never reordered, clamped, or partially initialized.

Inputs and data use closed declared shapes. Unknown constructors and malformed
payloads are outside the admitted domain. Boundary decoding is not hidden
inside a transition.

There is no implicit `null`, truthiness, string conversion, percentage
conversion, or lossy numeric coercion. A bare normalized scalar is in the
inclusive range `0..1`.

## One atomic reaction

A reaction evaluates against committed pre-state and a private draft:

1. read configuration and current draft values;
2. build draft state changes in statement order;
3. append command values to a private ordered buffer;
4. select exactly one declared result;
5. run any admitted finite commit reconciliation;
6. validate invariants; and
7. publish the result, final state, and commands together.

A **commit** result publishes the final draft and commands. It may be a
committed no-op. An **abort** result publishes neither draft changes nor
commands and therefore stutters exactly. A program fault also publishes
nothing, but it is an author defect rather than a domain result.

External work never executes inside a reaction. Commands are inert data made
available only after publication. Any report caused by a command is a later
input; it cannot synchronously re-enter the current step.

## Values, ownership, and order

Data values are immutable outside the private reaction draft. Every mutable
state path has one declared owner. Derived values and observations are pure;
they are not duplicated mutable facts.

Sequences retain order. Ordered commands retain append order. Map and set
traversal is not an observable ordering source. A program that requires FIFO
owns an explicit sequence or queue.

State, classifications, observations, and ordered commands must replay
deterministically from the same admitted configuration and ordered input
history. No task may use ambient time, randomness, browser state, storage,
network access, process state, or a mutable global.

## Closed computation

Matching over a closed sum is exhaustive. Pure helpers terminate and perform
no state change or command emission. General recursion and unbounded loops are
outside the trial.

A bounded loop must expose a natural-valued measure that the language or
reviewer can establish strictly decreases on every back edge. It is not a
runtime timeout.

## Composition and application boundary

Source composition may split and namespace declarations without adding a
runtime scheduler or child actor. A composed part is a checked owner and
dependency boundary, not an allocated runtime object.

A typed port contributes declared later inputs and ordered command values. Its
source declaration requires host authority but does not acquire or initialize
that authority.

UI, navigation, and framework semantics are absent from the standalone core
unless explicitly activated. Presentation reads committed observation and
constructs declared semantic inputs; it does not mutate private state.

## What to submit

Complete every task using only the forms admitted by your arm reference.
Do not call another language, embed foreign code, invent a runtime library,
or move required behavior into prose. Comments may explain a necessary
assumption but do not satisfy a behavior.
