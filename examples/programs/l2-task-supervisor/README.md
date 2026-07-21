# L2 — Keyed Task Supervisor

- **Status:** Language-neutral program specification
- **Level:** L2 — one open, keyed machine system
- **Implementation:** None inside this language-neutral problem; executable
  answers are indexed by the parent harness
- **Authority:** The problem contract is authoritative for candidate
  comparison; no Uhura behavior is accepted here

The problem statement, operational rules, invariants, and traces in this
document are the authority. A candidate language must fit the program; the
program must not be weakened or rearranged to fit the current language. Any
Uhura source written for this harness is an answer sheet to this specification.

## Purpose

This program specifies a deterministic supervisor for asynchronous keyed
tasks. It admits work, starts at most two attempts concurrently, preserves
first-in-first-out queue order, correlates asynchronous reports with the exact
attempt that produced them, and handles cancellation and retry without letting
late reports corrupt newer state.

The task work itself occurs outside the program. The supervisor owns task
lifecycle and scheduling state; it produces ordered requests for an external
worker and accepts explicitly correlated reports from that worker.

This is a standalone behavioral problem. It does not depend on a view, widget,
network protocol, thread API, actor model, or host-language collection type.

## Fixed Configuration

The concurrency limit is exactly:

```text
limit = 2
```

It is part of this program, not an input and not a candidate-language option.

Task identifiers are opaque values with equality. A task identifier remains
reserved for the lifetime of the supervisor after its first accepted
submission, including after success, failure, or cancellation.

Attempts are positive integers. The first attempt actually started for a task
is attempt `1`. Each later start of that task increments the attempt number by
one. An attempt number is allocated only when the task starts, not when it is
submitted or placed back in the queue.

The pair:

```text
(task identifier, attempt)
```

is the complete correlation identity for external reports and cancellation
requests. A pair is never reused.

## State

Each known task has exactly one task state:

```text
queued
running(attempt, progress)
succeeded
failed
cancelled
```

The supervisor additionally retains, for every known task, the number of
attempts that have started. This correlation ledger remains available when the
task is not running so that late and duplicate reports can be classified.

The supervisor also owns one FIFO queue containing task identifiers. Queue
position is determined by the order in which a task enters `queued`:

- an accepted `submit` appends a new task at the tail;
- an accepted `retry` appends the existing task at the tail; and
- cancelling a queued task removes it without changing the relative order of
  the remaining tasks.

The initial state contains no tasks, no running attempts, and an empty queue.

## Inputs

The program accepts these input shapes:

```text
submit(task)
cancel(task)
retry(task)
progress(task, attempt, value)
succeed(task, attempt)
fail(task, attempt)
```

`value` in a `progress` input is a finite normalized scalar in the inclusive
range `0..1`. It is not a percentage. Values below `0`, above `1`, NaN, and
infinity are invalid and must not be clamped or normalized implicitly.

Inputs are processed one at a time. Every input and all scheduling caused by
that input form one finite, atomic supervisor step.

## Outputs

The supervisor produces only these external worker requests:

```text
start(task, attempt)
cancel(task, attempt)
```

Outputs are an ordered sequence. Their order is required by this problem
contract for candidate comparison.

A `start` output grants one concurrency slot to exactly one correlated
attempt. A `cancel` output requests that the worker stop a previously started
attempt. Cancellation is best-effort at the worker boundary: the supervisor
frees the slot immediately and treats later reports for that attempt as stale.

The worker cannot synchronously feed a new input into the middle of the
current step. Any report caused by an output is a later input.

## Input Classification

Every input is classified deterministically as one of:

- **accepted** — it performs its defined transition, which may be an accepted
  no-op;
- **duplicate** — the same already-observed fact or idempotent request is
  ignored;
- **stale** — the input refers to a real task or attempt that is no longer
  current and is ignored; or
- **invalid** — the input could not have been produced by a valid current
  interaction with this supervisor and is rejected.

Duplicate, stale, and invalid inputs do not change state, run the scheduler, or
produce worker outputs. An implementation may expose diagnostics separately,
but diagnostics are not worker outputs and must not alter subsequent
behavior.

### `submit`

`submit(task)` is accepted only when `task` has never been submitted before.
It creates the task in `queued` with zero started attempts and appends it to
the FIFO queue.

A submission using any known task identifier is invalid, regardless of the
task's current state. Terminal identifiers are not reusable.

### `cancel`

For a queued task, `cancel(task)` is accepted. The task is removed from the
queue and becomes `cancelled`. It produces no `cancel` output because no
attempt was started.

For a running task, `cancel(task)` is accepted. The task becomes `cancelled`
and first produces:

```text
cancel(task, current attempt)
```

The running slot is freed in the same step.

For an already cancelled task, `cancel(task)` is a duplicate and is ignored.
For a succeeded or failed task, it is invalid. For an unknown task, it is
invalid.

### `retry`

`retry(task)` is accepted only when the task is `failed` or `cancelled`. The
task becomes `queued` and is appended at the FIFO tail.

If the task was cancelled before ever starting, its next start is still
attempt `1`. Otherwise its next start uses one more than the greatest attempt
previously started.

Retrying a queued, running, or succeeded task is invalid. Retrying an unknown
task is invalid.

### `progress`

`progress(task, attempt, value)` is accepted only when the task is currently
`running(attempt, current)` and `value` is greater than `current`. It changes
the running state to:

```text
running(attempt, value)
```

Equal progress for the current attempt is duplicate and ignored. Lower
progress for the current attempt is stale and ignored. This makes reordered
progress reports deterministic without permitting progress to move backward.

Progress `1` does not imply success. The task remains running until a matching
`succeed`, `fail`, or user `cancel` input is accepted.

Classification follows this order:

1. An unknown task, non-positive attempt, or non-finite or out-of-range
   progress value is invalid.
2. For a known task and valid value, an attempt greater than the greatest
   attempt started is invalid.
3. An attempt lower than the greatest attempt started is stale.
4. The greatest attempt started is stale when it is no longer running.
5. For the current running attempt, a lower value is stale, an equal value is
   duplicate, and a greater value is accepted.

This precedence means, for example, that an out-of-range value is invalid even
when it is attached to an otherwise stale attempt.

### `succeed` and `fail`

`succeed(task, attempt)` is accepted only when that exact attempt is currently
running. The task becomes `succeeded`, and its slot is freed.

`fail(task, attempt)` is accepted only when that exact attempt is currently
running. The task becomes `failed`, and its slot is freed.

The first accepted terminal report wins. While the task remains in that
terminal state, repeating the same terminal report for the settled attempt is
duplicate and a different terminal report for that attempt is stale. Once an
accepted retry moves a failed task back to `queued`, every later report for
the settled attempt is stale, including a report identical to the one that
originally settled it. Any report for an attempt that was previously started
and then cancelled by the user is stale.

Reports for an older attempt are stale. Reports for an attempt that has not
started are invalid. Reports for an unknown task or a non-positive attempt are
invalid.

## Scheduling Semantics

After applying an accepted input's direct state change and direct `cancel`
output, the supervisor fills all available slots before the step ends.

Scheduling is exactly:

```text
while running-count < 2 and the FIFO queue is not empty:
    remove the task at the queue head
    attempt = that task's started-attempt count + 1
    record the incremented started-attempt count
    set the task to running(attempt, 0)
    append start(task, attempt) to this step's outputs
```

Consequently:

- a submission or retry starts in the same step when a slot is available;
- a success, failure, or running cancellation starts the oldest queued work
  in the same step;
- queued tasks start strictly in FIFO entry order;
- a retried task enters behind every task already queued;
- scheduling never preempts a running task; and
- an accepted queued cancellation cannot itself create a free running slot.

When cancelling a running task also starts queued work, the output order is:

```text
cancel(cancelled task, cancelled attempt)
start(oldest queued task, new attempt)
```

## Canonical Adversarial Trace

The notation `A:1@0.75` means task `A` is running attempt `1` at progress
`0.75`. `Q=[D,C]` is the FIFO queue from head to tail. Settled states are shown
only when they change or matter to the input classification.

| Step | Input | Classification | Ordered worker outputs | State after the step |
| ---: | --- | --- | --- | --- |
| 0 | initial | — | `[]` | running `[]`; `Q=[]` |
| 1 | `submit(A)` | accepted | `[start(A,1)]` | running `[A:1@0]`; `Q=[]` |
| 2 | `submit(B)` | accepted | `[start(B,1)]` | running `[A:1@0, B:1@0]`; `Q=[]` |
| 3 | `submit(C)` | accepted | `[]` | running `[A:1@0, B:1@0]`; `Q=[C]` |
| 4 | `submit(D)` | accepted | `[]` | running `[A:1@0, B:1@0]`; `Q=[C,D]` |
| 5 | `submit(A)` | invalid | `[]` | unchanged |
| 6 | `progress(A,2,0.5)` | invalid future attempt | `[]` | unchanged |
| 7 | `cancel(C)` | accepted | `[]` | `C=cancelled`; running `[A:1@0, B:1@0]`; `Q=[D]` |
| 8 | `retry(C)` | accepted | `[]` | running `[A:1@0, B:1@0]`; `Q=[D,C]` |
| 9 | `cancel(B)` | accepted | `[cancel(B,1), start(D,1)]` | `B=cancelled`; running `[A:1@0, D:1@0]`; `Q=[C]` |
| 10 | `succeed(B,1)` | stale after cancellation | `[]` | unchanged |
| 11 | `fail(A,1)` | accepted | `[start(C,1)]` | `A=failed`; running `[D:1@0, C:1@0]`; `Q=[]` |
| 12 | `retry(A)` | accepted | `[]` | running `[D:1@0, C:1@0]`; `Q=[A]` |
| 13 | `progress(D,1,0.75)` | accepted | `[]` | running `[D:1@0.75, C:1@0]`; `Q=[A]` |
| 14 | `progress(D,1,0.5)` | stale regression | `[]` | unchanged |
| 15 | `succeed(D,1)` | accepted | `[start(A,2)]` | `D=succeeded`; running `[C:1@0, A:2@0]`; `Q=[]` |
| 16 | `progress(A,1,0.9)` | stale older attempt | `[]` | unchanged |
| 17 | `progress(A,2,0.6)` | accepted | `[]` | running `[C:1@0, A:2@0.6]`; `Q=[]` |
| 18 | `progress(A,2,0.6)` | duplicate | `[]` | unchanged |
| 19 | `fail(A,2)` | accepted | `[]` | `A=failed`; running `[C:1@0]`; `Q=[]` |
| 20 | `retry(A)` | accepted | `[start(A,3)]` | running `[C:1@0, A:3@0]`; `Q=[]` |
| 21 | `cancel(A)` | accepted | `[cancel(A,3)]` | `A=cancelled`; running `[C:1@0]`; `Q=[]` |
| 22 | `succeed(A,3)` | stale after cancellation | `[]` | unchanged |
| 23 | `progress(C,1,1)` | accepted | `[]` | running `[C:1@1]`; `Q=[]` |
| 24 | `succeed(C,1)` | accepted | `[]` | `C=succeeded`; running `[]`; `Q=[]` |
| 25 | `succeed(C,1)` | duplicate | `[]` | unchanged |
| 26 | `retry(C)` | invalid after success | `[]` | unchanged |

At the end:

```text
A = cancelled, attempts started = 3
B = cancelled, attempts started = 1
C = succeeded, attempts started = 1
D = succeeded, attempts started = 1
running = []
queue = []
```

The trace deliberately proves that:

- the concurrency limit is enforced;
- FIFO order survives queued cancellation and retry;
- cancelling queued work emits no worker cancellation;
- cancelling running work emits cancellation before replacement work starts;
- a task cancelled before its first start still begins with attempt `1`;
- retry after a started attempt increments correlation;
- reordered progress never moves backward;
- late, duplicate, and future-attempt reports have distinct classifications;
  and
- stale reports cannot settle or mutate a newer attempt.

## Additional Required Cases

The canonical trace is not exhaustive. Conformance must also cover:

- cancelling an already cancelled task is duplicate and emits nothing;
- cancelling a succeeded or failed task is invalid;
- retrying a queued or running task is invalid;
- `progress` at exactly `0` for a newly started task is duplicate;
- progress below `0` or above `1` is invalid rather than clamped;
- success at progress below `1` is valid;
- failure at progress `1` is valid;
- a task may remain running at progress `1`;
- conflicting terminal reports for one attempt leave the first accepted result
  unchanged;
- retry of a task cancelled before starting enters at the FIFO tail;
- invalid, duplicate, and stale inputs do not opportunistically run the
  scheduler; and
- replaying the same complete input trace produces the same classifications,
  task states, queue order, and ordered outputs.

## Invariants

The following must hold initially and after every step:

1. At most two tasks are running.
2. Every known task has exactly one task state.
3. The FIFO queue contains each queued task exactly once and contains no task
   in any other state.
4. If the queue is non-empty after a step, exactly two tasks are running.
5. Running progress is always in `0..1` and never decreases within an attempt.
6. A running task's attempt equals its greatest started attempt.
7. Started-attempt counts never decrease, and every emitted
   `(task, attempt)` start correlation is globally unique.
8. Every accepted progress, success, or failure report matches exactly one
   current running correlation.
9. At most one `start` and at most one `cancel` output are emitted for a given
   correlation.
10. A `cancel` output is emitted only for a correlation previously emitted by
    `start`.
11. No stale, duplicate, or invalid input changes task state, queue order,
    attempt history, or worker outputs.
12. Among tasks that remain queued, start order is their FIFO entry order.
13. State, classifications, and ordered outputs are a deterministic function
    of the initial state and the complete ordered input history.

## Liveness

The supervisor guarantees scheduling order, not completion. A worker may never
report progress, success, or failure. In that case its task remains running
and continues to occupy a slot indefinitely.

FIFO guarantees apply when a slot becomes available; they do not guarantee
that a slot eventually becomes available. The supervisor has no ambient clock,
timeout, automatic cancellation, preemption, or fairness assumption beyond
the transition and queue rules stated here.

## Observation

A conforming harness must be able to inspect, without executing worker work:

- every task's current task state;
- the greatest attempt started for each task;
- queue order;
- the set of current running correlations;
- available capacity, derived as `2 - running-count`; and
- the ordered worker outputs produced by the most recent step or complete
  trace.

Available capacity and running count are derived observations, not separately
mutable facts. Observation is pure and emits no `start` or `cancel` worker
request. A reachability claim requires a valid input trace.

## Candidate-Language Obligations

A candidate solution must:

- express the complete state, input, output, correlation, scheduling, retry,
  cancellation, and progress semantics without hidden host-language behavior;
- preserve the fixed limit of two and strict FIFO queue order;
- make each input plus resulting scheduling one terminating, deterministic,
  atomic step;
- preserve ordered outputs, including cancellation before replacement start;
- distinguish accepted, duplicate, stale, and invalid inputs as specified;
- retain enough attempt history to reject future correlations and ignore stale
  correlations after settlement, cancellation, and retry;
- admit normalized progress only in `0..1`, without percentage conversion or
  clamping;
- ensure late reports cannot mutate a newer attempt with the same task
  identifier;
- make state and derived observations available to headless inspection;
- permit exact replay from the initial state plus ordered inputs;
- support conformance tests over state, input classification, queue order, and
  ordered outputs rather than relying on rendered pixels; and
- remain independent of any particular worker implementation, renderer,
  widget catalogue, actor model, map type, queue type, or concurrency library.

A candidate may choose its own surface syntax, decomposition, and internal
representation. It may not change the program's states, fixed limit, FIFO
policy, attempt allocation, correlation rules, or invalid-input behavior to
make the answer shorter.

## Non-Goals

L2 does not specify:

- how workers execute tasks or whether they use threads, processes, network
  calls, actors, or another mechanism;
- task payloads, return values, failure reasons, priorities, dependencies,
  deadlines, or resource weights;
- clocks, timeouts, automatic retry, retry limits, backoff, or jitter;
- worker acknowledgment that cancellation physically completed;
- preemption, pausing, resuming, checkpointing, or partial-result recovery;
- persistence, crash recovery, distributed coordination, or multi-supervisor
  authority;
- deletion or reuse of task identifiers;
- fairness beyond the stated FIFO queue and fixed concurrency slots;
- navigation, page or surface lifetime, views, widgets, styling, animation, or
  accessibility; or
- general computational completeness.

Those concerns require separate programs. Adding them here would make this
specification less precise as evidence about asynchronous state, correlation,
FIFO scheduling, cancellation, and retry.
