# Task 01 — author the complete L0 program

Write one complete standalone answer in `01-l0` with your arm's source
extension. Do not use a supplied scaffold.

## Configuration and admission

The immutable configuration is:

```text
(minimum: integer, maximum: integer, initial: integer)
```

It is valid exactly when:

```text
minimum <= initial <= maximum
```

Reject an invalid configuration before state exists. Do not reorder or clamp
configuration.

## State and inputs

The only mutable state is integer `count`, initially `initial`.

The closed input domain is:

```text
increment | decrement | reset
```

Every declared input is accepted, including a boundary no-op. There are no
commands.

Transitions:

```text
increment -> min(count + 1, maximum)
decrement -> max(count - 1, minimum)
reset     -> initial
```

## Invariant and observation

Initially and after every step:

```text
minimum <= count <= maximum
```

Pure observation:

```text
{
  count,
  at_minimum: count == minimum,
  at_maximum: count == maximum
}
```

The flags are derived, not stored.

For configuration `(0, 2, 0)`, the inputs
`increment, increment, increment, decrement, reset, decrement` observe counts
`1, 2, 2, 1, 0, 0`.

The answer must expose exact configuration admission, initial state, closed
inputs, one finite atomic step, and pure observation without a renderer,
global state, or host-language escape.
