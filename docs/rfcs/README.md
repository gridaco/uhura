# Uhura RFCs

- **Status:** Durable decision-history index
- **Lifetime:** RFC identities are permanent; decisions may be superseded
- **Doctrine:** [Uhura doctrine](../doctrine/README.md)
- **Specification router:** [Uhura specifications](../spec/README.md)

RFCs record durable Uhura project, language, runtime, and contract decisions.
They are historical records, not current language law or substitutes for a
versioned specification and executable conformance tests.

An accepted RFC may be superseded by a later RFC. Acceptance does not imply
implementation, permanent syntax, or adoption by every later version. When a
decision changes, preserve the earlier RFC and add explicit `Supersedes` and
`Superseded by` metadata; do not rewrite history to resemble the new design.

| RFC | Title | Status |
|---|---|---|
| [0001](0001-project-foundation.md) | Project foundation: UI language and headless experience runtime | Draft |
| [0002](0002-model-driven-editor-live-updates.md) | Model-driven Editor and saved-source live updates | Accepted |
| [0003](0003-source-comments-docs-and-annotations.md) | Source comments, declaration docs, and markup annotations | Accepted |

RFC numbers are local to the Uhura project, zero-padded, and never reused after
a proposal has been shared. The status inside an RFC is authoritative.
Supported statuses are Draft, Accepted, Rejected, Deferred, Withdrawn, and
Superseded.

RFCs must summarize any evidence necessary to understand their decision.
Links to disposable studies or incubation drafts are optional provenance, not
dependencies that require those documents to remain in the current tree.
