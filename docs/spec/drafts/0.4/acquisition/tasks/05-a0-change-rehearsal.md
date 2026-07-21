# Task 05 — apply the A0 reason change

Copy the complete arm-specific `05-a0` scaffold into `05-a0` with your arm's
source extension and apply this controlled change:

```text
Reason =
  damaged
  | not-needed
  | other(note)
```

`note` is exact user-authored text.

Required behavior:

- `other("")` is valid draft state but does not make the reason complete;
- every non-empty note is complete, including text made only of whitespace;
- no trimming, Unicode normalization, or implicit conversion occurs;
- submission carries the tagged reason and exact note;
- choosing `damaged`, `not-needed`, or a different `other(note)` replaces the
  complete previous reason, so an abandoned note is not retained elsewhere;
- the change adds no route, external authority, state owner, host capability,
  or hidden side table.

The scaffold is only the reason-domain slice of A0. Passing this task is a
surface-transfer rehearsal, not conformance of a complete A0 application.
