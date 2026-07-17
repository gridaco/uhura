# Uhura specifications

- **Status:** Stable router
- **Authority:** Navigation only; contains no language semantics
- **Doctrine:** [Uhura doctrine](../doctrine/README.md)
- **Decision history:** [Uhura RFCs](../rfcs/README.md)

Uhura intentionally has no unversioned living specification. Exact grammar,
runtime, widget, and compatibility claims must live under a named version or
an explicitly disposable draft.

## Current state

Uhura has no complete accepted specification or supported compatibility
version.

- [v0 incubation language model](drafts/v0.md) — disposable
  pre-specification draft; it mixes target semantics, open questions, and
  independently accepted RFC material.
- [RFC 0003](../rfcs/0003-source-comments-docs-and-annotations.md) — accepted
  decision for source comments, declaration documentation, and markup
  annotations. Its implementation remains tracked separately.

The v0 draft is not a base document that v1 must edit in place. It may be
replaced wholesale. Durable conclusions must be restated in an RFC; observable
behavior must be restated in the specification and conformance suite of the
version that adopts it.

## Path policy

- `docs/spec/README.md` is a stable router and should remain short.
- `docs/spec/drafts/` is disposable. Draft paths carry no compatibility
  promise and may disappear from the current tree; Git history is sufficient
  archival recovery.
- A claimable language release should use an immutable version path such as
  `docs/spec/versions/1.0/` and a matching conformance suite.
- Accepted version documents receive corrections and errata, not silent
  semantic rewrites.
