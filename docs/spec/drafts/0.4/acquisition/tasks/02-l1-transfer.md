# Task 02 — complete the crossing transition

Copy the complete arm-specific `02-l1` scaffold into `02-l1` with your arm's
source extension and replace every `TRIAL-TODO`. Preserve the declared public
domains and result payloads.

The scaffold models four entities on one of two sides. One input crosses the
operator alone or with one optional passenger.

Implement:

1. `opposite`;
2. ordered safety-violation computation; and
3. the complete crossing handler or reducer.

Rules:

- If a named passenger is not on the operator's current side, return only
  `passenger-not-with-operator`. Do not construct or safety-check a candidate.
- Otherwise atomically move the operator and optional passenger to the
  opposite side.
- A candidate violates `predator-with-dependent` when predator and dependent
  share a side without the operator.
- It violates `dependent-with-cargo` when dependent and cargo share a side
  without the operator.
- When both are violated, retain exactly that declared order.
- Any violation refuses and leaves state exactly unchanged.
- Otherwise publish the complete candidate and an accepted payload containing
  passenger, departure, and arrival.
- The safety invariant applies to committed state.
- `solved` is derived exactly when all four entities are on the right. It is
  not an absorbing state.

The source must be total for all ten safe assignments and all four valid
inputs. Do not special-case a solution trace.
