# Uhura specifications

- **Status:** Stable router
- **Authority:** Navigation only; contains no language semantics
- **Doctrine:** [Uhura doctrine](../doctrine/README.md)
- **Decision history:** [Uhura RFCs](../rfcs/README.md)

Uhura intentionally has no unversioned living specification. Exact grammar,
runtime, widget, and compatibility claims must live under a named version or
an explicitly disposable draft.

## Current state

Uhura has one implemented active candidate, one retained executable
differential baseline, and no supported compatibility version.

- [Uhura 0.4 incubation candidate](drafts/0.4/) — the single active exact
  design and implemented frontend. It retains the 0.3 transactional kernel,
  adopts a Rust-shaped machine surface plus Svelte-shaped `ui`, and defines
  modular source composition that lowers to one global semantic machine IR.
- Uhura 0.3 — the retained executable differential and compatibility baseline.
  Its implemented behavior is comparison evidence, not the current authoring
  surface or a stable compatibility contract.
- [Relay B3 historical record](drafts/relay-b3/) — a short provenance pointer
  for the experiment that produced the 0.3 kernel. Git history retains its
  former detailed candidate documents.
- [v0 historical pointer](drafts/v0.md) — the retired UI-first experiment.
- [RFC 0003](../rfcs/0003-source-comments-docs-and-annotations.md) — accepted
  source-metadata decision reconciled with the 0.4 grammar; its checked
  attachment and authoring projection remain a separate pending
  implementation.
- [RFC 0004](../rfcs/0004-standalone-machine-core-and-source-composition.md) —
  accepted core-first and source-composition boundary incorporated by the 0.4
  candidate.

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
