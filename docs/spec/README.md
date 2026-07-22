# Uhura specifications

- **Status:** Stable router
- **Authority:** Navigation only; contains no language semantics
- **Doctrine:** [Uhura doctrine](../doctrine/README.md)
- **Decision history:** [Uhura RFCs](../rfcs/README.md)

Uhura intentionally has no unversioned living specification. Exact grammar,
runtime, widget, and compatibility claims must live under a named version or
an explicitly disposable draft.

## Current state

Uhura has one implemented active candidate and no supported compatibility
version.

- [Uhura 0.4 incubation candidate](drafts/0.4/) — the single active exact
  design and implemented frontend. It uses a Rust-shaped machine surface plus
  Svelte-shaped `ui`, pure typed UI composition, and an opt-in Web application
  topology that still lowers to one global semantic machine IR.
- Retired source frontends have no admission path in the current toolchain.
  Earlier experiments remain available through Git history.
- [Relay B3 historical record](drafts/relay-b3/) — a short provenance pointer
  for the experiment that produced the current transaction model. Git history
  retains its former detailed candidate documents.
- [v0 historical pointer](drafts/v0.md) — the retired UI-first experiment.
- [RFC 0003](../rfcs/0003-source-comments-docs-and-annotations.md) — accepted
  source-metadata decision reconciled with the 0.4 grammar; its checked
  sibling-list attachment and Editor authoring projection are implemented,
  while the RFC names the remaining placement and declaration-doc work.
- [RFC 0004](../rfcs/0004-standalone-machine-core-and-source-composition.md) —
  accepted core-first and source-composition boundary incorporated by the 0.4
  candidate.
- [RFC 0005](../rfcs/0005-web-application-topology-and-ui-composition.md) —
  accepted opt-in Web application topology and pure UI-composition boundary.

The active candidate may be replaced wholesale. Durable decisions belong in
RFCs; observable supported behavior must be restated in the specification and
conformance suite of the version that adopts it.

## Path policy

- `docs/spec/README.md` is a stable router and should remain short.
- `docs/spec/drafts/` is disposable. Draft paths carry no compatibility
  promise and may disappear from the current tree; Git history is sufficient
  archival recovery.
- A claimable language release should use an immutable version path such as
  `docs/spec/versions/1.0/` and a matching conformance suite.
- Accepted version documents receive corrections and errata, not silent
  semantic rewrites.
