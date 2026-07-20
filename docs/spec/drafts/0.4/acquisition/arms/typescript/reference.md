# Plain TypeScript control reference

This arm uses ordinary TypeScript as an existing-language control. It is not a
TypeScript-looking Uhura grammar and does not receive Uhura's language
guarantees by resemblance.

The repository's executable
`examples/programs/baselines/typescript/baseline.ts` is the comparison
baseline. It is referenced as existing evidence but is deliberately not copied
into this packet because it contains task answers.

## Admitted answer style

Use zero-package TypeScript with:

- `type` aliases and `interface` declarations;
- `readonly` public records;
- literal and discriminated unions for closed domains;
- pure synchronous top-level functions;
- `const` and local `let`;
- exhaustive `switch` plus an `assertNever` helper;
- copy-on-write object, array, `Map`, and `Set` values;
- returned ordered command arrays; and
- explicit observation functions.

The task source must not use:

- a state-machine or reducer package;
- classes, prototypes, decorators, or dependency injection;
- `any`, `unknown` plus unchecked assertion, `as`, or non-null `!`;
- mutation of configuration or input state;
- callbacks that register work or hide control flow;
- promises, `async`, generators, threads, or timers;
- exceptions as an ordinary input classification;
- DOM, browser, network, storage, process, clock, or randomness globals;
- global mutable state; or
- command execution inside a reducer.

An invalid constructor argument may throw before state exists, and a violated
internal invariant may throw as a program fault. Neither exception is a
declared transition result.

TypeScript permits many excluded forms. Avoiding them is source discipline,
not a new TypeScript guarantee. The rubric records that difference.

## Closed data

Use literal or discriminated unions:

```ts
export type Mode =
  | { readonly type: "idle" }
  | {
      readonly type: "active";
      readonly request: string;
      readonly ratio: number;
    };
```

Use exact record types:

```ts
export interface Request {
  readonly id: string;
  readonly label: string;
}
```

`null` may represent an explicit optional value only when the type says so.
There is no truthiness-based domain decision. Check exact cases.

For exhaustive selection:

```ts
function assertNever(value: never): never {
  throw new Error("unexpected closed-union member: " + String(value));
}

function label(mode: Mode): string {
  switch (mode.type) {
    case "idle":
      return "Idle";
    case "active":
      return mode.request;
    default:
      return assertNever(mode);
  }
}
```

Do not use `default: return state` to hide a missing case.

## Pure reducer boundary

Model initialization, one step, and observation explicitly:

```ts
export interface State {
  readonly active: string | null;
}

export type Input =
  | { readonly type: "begin"; readonly id: string }
  | { readonly type: "cancel"; readonly id: string };

export type Classification = "accepted" | "duplicate" | "invalid";

export interface Command {
  readonly type: "start";
  readonly id: string;
}

export interface Step {
  readonly state: State;
  readonly classification: Classification;
  readonly commands: readonly Command[];
}
```

One step is a pure synchronous function. A non-accepted result returns the
original state and an empty command sequence. Accepted state is created by
copying:

```ts
return {
  state: { ...state, active: input.id },
  classification: "accepted",
  commands: [{ type: "start", id: input.id }],
};
```

Commands are data. Do not call a worker, callback, promise, or host API.

## Numbers and normalized input

Use `bigint` when the task requires mathematical integer counters or attempt
ledgers. Use JavaScript `number` only for the task's raw boundary number. A
normalized value is valid only after:

```ts
Number.isFinite(value) && value >= 0 && value <= 1
```

Do not clamp, divide by 100, or accept `NaN` or infinities. TypeScript does not
provide a nominal exact `Ratio`; the source check and tests carry this
obligation.

## Persistent collections and order

Readonly types do not make a runtime collection immutable. Copy before an
accepted change:

```ts
const next = new Map(state.items);
next.set(id, item);
return { ...state, items: next };
```

Never mutate `state.items` directly. For a readonly array:

```ts
const nextQueue = [...state.queue, id];
const without = state.queue.filter((item) => item !== id);
```

Array order is explicit. Native `Map` or `Set` traversal must not determine a
task-visible order. Use an owned readonly array when FIFO matters.

## Atomic result convention

The control has no language-enforced draft or publication barrier. Preserve it
by construction:

- compute local candidate values first;
- return the original state and `[]` for duplicate, stale, or invalid input;
- construct copied next state only on the accepted path;
- return command data in exact order; and
- never expose a partially built value.

Pure observation is a separate function over configuration and committed
state. Derived fields are not stored merely for presentation.

This convention is part of the answer sheet. It is enforced by review and
executable tests, not by TypeScript's effect system.
