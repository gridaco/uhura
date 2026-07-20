# A0 Return Desk reference oracle

- **Status:** Executable, language-independent application evidence over a
  bounded transport domain
- **Problem authority:** [A0 Return Desk](..)
- **Uhura answer:** [Uhura 0.4](../answers/uhura-0.4/)
- **Not:** An Uhura parser, checker, interpreter, backend, or conformance result

This oracle makes one exact interpretation of A0 executable before a language
implementation exists. Its base-domain traces provide a differential target
for future candidates without making JavaScript the authoring language or
semantic authority. The same model also contains the C1
`Reason.other(note)` extension; that constructor is outside the base Uhura
answer and must be excluded when comparing that answer.

The JavaScript boundary accepts external integers only through the safe-integer
transport subset. It is therefore not a differential oracle for Uhura values
outside that subset; arbitrary-precision integer cases remain a separate Uhura
conformance gate. Internal identity counters promote to `BigInt` before
precision loss and receive tagged canonical checkpoint encoding.

Run either:

```sh
bun examples/applications/a0-return-desk/reference-oracle/validate.mjs
node examples/applications/a0-return-desk/reference-oracle/validate.mjs
```

Both currently report:

```text
PASS 25 validation groups: canonical 0..31, 15 adversarial scenarios, 12 static pins, checkpoint replay, and C1.
```

Coverage includes:

- every canonical row's result and ordered consequences;
- all 14 A0 scenarios plus superseded-receipt navigation recovery;
- 12 required static pins;
- atomic rollback for non-accepted results;
- request, route-scope, and surface identity behavior;
- closed boundary records and malformed-delivery precedence;
- safe external integer admission and exact internal counters;
- receipt encoding and redirecting/offered/acknowledged lifecycle;
- checkpoint, restore, rerun, and semantic tamper rejection; and
- the controlled `Reason.other(note)` extension as a separate, post-base study.

The C1 pass is edit-locality evidence for the application model only. It is
not Uhura controlled-change evidence: the checked-in Uhura answer intentionally
remains the base two-reason program, and the current machine implementation
must first pass that base before a separate C1 source change is authored and
measured.

The model deliberately does not:

- parse or execute Uhura source;
- prove compiler-enforced totality, termination, ownership, or port authority;
- validate checked `ui` syntax or browser mechanics;
- implement module resolution or missing-binding diagnostics;
- restore physical host effects; or
- replace an independent implementation or formal proof.
