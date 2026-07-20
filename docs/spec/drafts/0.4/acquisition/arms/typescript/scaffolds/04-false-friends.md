# TypeScript false-friend worksheet

Each snippet is intended to preserve the behavior described in its caption.
Diagnose and replace the transferred assumption.

## F-T01 — input-state mutation

```ts
function accept(state: State): State {
  state.count += 1n;
  return state;
}
```

Intent: return accepted next state without mutating committed input state.

## F-T02 — `any` closes nothing

```ts
function step(state: State, input: any): Step {
  return reduce(state, input);
}
```

Intent: accept exactly the declared closed input union.

## F-T03 — unchecked assertion

```ts
const report = payload as ProgressInput;
return stepProgress(state, report);
```

Intent: admit external payload only after exact boundary validation.

## F-T04 — swallowed union case

```ts
switch (input.type) {
  case "begin":
    return begin(state, input);
  default:
    return { state, classification: "duplicate", commands: [] };
}
```

Intent: handle every declared input explicitly and fail checking when a new
case is omitted.

## F-T05 — command execution

```ts
worker.start(input.id);
return { state: next, classification: "accepted", commands: [] };
```

Intent: request later work without executing authority inside the reducer.

## F-T06 — mutable `Map`

```ts
state.tasks.set(input.id, nextTask);
return { ...state, tasks: state.tasks };
```

Intent: publish a copied accepted state and leave input state unchanged.

## F-T07 — asynchronous semantic step

```ts
async function step(state: State, input: Input): Promise<Step> {
  const result = await worker.run(input);
  return settle(state, result);
}
```

Intent: keep one semantic step finite and synchronous; external completion is
a later correlated input.

## F-T08 — exception as classification

```ts
if (input.attempt <= 0n) {
  throw new RangeError("invalid attempt");
}
```

Intent: classify a decoded but contextually invalid input as ordinary
`invalid` without a fault.

## F-T09 — ambient authority

```ts
const id = `${Date.now()}-${Math.random()}`;
```

Intent: preserve replayable identity without ambient time or randomness.

## F-T10 — accidental map ordering

```ts
const next = state.tasks.keys().next().value;
```

Intent: choose the oldest queued identity by the program's explicit FIFO
state, not native map traversal.
