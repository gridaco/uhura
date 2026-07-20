# Uhura documentation

- **Status:** Stable documentation router
- **Doctrine:** [Uhura doctrine](doctrine/README.md)
- **Specification router:** [Uhura specifications](spec/README.md)
- **Widget taxonomy:** [Uhura widget taxonomy](widgets/README.md)
- **Decision history:** [Uhura RFCs](rfcs/README.md)
- **Research:** [Uhura studies](studies/README.md)

This page classifies Uhura documents by authority and lifetime. The
classification exists so that a useful v0 experiment cannot become permanent
language shape merely because many pages describe it.

## Document families

| Family | Canonical path | Lifetime | Authority |
|---|---|---|---|
| Doctrine | [`doctrine/`](doctrine/) | Durable, live, version-independent | Judges designs; never defines version behavior |
| Specification router | [`spec/README.md`](spec/README.md) | Stable | Points to drafts and supported version specifications; contains no semantics |
| Incubation drafts | [`spec/drafts/`](spec/drafts/) | Disposable | Proposed exact language models; may be replaced or deleted wholesale |
| Widget taxonomy | [`widgets/README.md`](widgets/README.md) | Stable router; live taxonomy | Provides revisable capability vocabulary, not builtin names or contracts |
| Widget drafts | [`widgets/drafts/`](widgets/drafts/) | Disposable | Version-scoped capability names, contracts, implementation notes, and proposals |
| RFCs | [`rfcs/`](rfcs/) | Durable historical record; supersedable | Record decisions and rationale; do not define current behavior by themselves |
| Study router | [`studies/README.md`](studies/README.md) | Stable | Classifies current research; contains no language semantics |
| Study leaves | Other documents under [`studies/`](studies/) | Disposable | Preserve evidence, experiments, and unresolved ideas; no language authority |
| Implementation and examples | Outside `docs/` | Replaceable evidence | Demonstrate behavior; do not become specification by accident |

There is intentionally no unversioned living specification.

## Current incubation snapshot

Uhura has one active, implemented incubation candidate, one retained
differential baseline, and no supported compatibility version.

- [Uhura 0.4](spec/drafts/0.4/) is the only active exact design. Its five
  documents separate the source-neutral kernel, concrete source and lowering,
  application profile, and conformance plan.
- Uhura 0.4 is the implemented incubation candidate exercised by the checked-in
  [program](../examples/programs/answers/uhura-0.4/) and
  [application](../examples/applications/a0-return-desk/answers/uhura-0.4/)
  answers, plus the canonical [Instagram project](../examples/instagram/client/).
  Implementation evidence does not establish a supported compatibility
  version.
- Uhura 0.3 is the retained differential baseline exercised by the checked-in
  [program](../examples/programs/answers/uhura-0.3/) and
  [application](../examples/applications/a0-return-desk/answers/uhura-0.3/)
  answers.
- [Relay B3](spec/drafts/relay-b3/) and
  [v0](spec/drafts/v0.md) are short historical pointers. Their former detailed
  drafts remain in Git history rather than normal documentation navigation.
- [v0 widget draft](widgets/drafts/v0/README.md) preserves the historical
  capability study that preceded the 0.4 replacement. It is not current 0.4
  behavior and remains disposable.
- [RFC 0002](rfcs/0002-model-driven-editor-live-updates.md) and
  [RFC 0003](rfcs/0003-source-comments-docs-and-annotations.md) are accepted
  historical decisions with independent implementation status.
- [RFC 0004](rfcs/0004-standalone-machine-core-and-source-composition.md)
  fixes the standalone-core, explicit-`ui`, and modular-source/global-IR
  boundary incorporated by the active candidate.
- [RFC 0001](rfcs/0001-project-foundation.md) remains a draft proposal; it is
  not a foundational authority merely because other work was inspired by it.

Replacing v0 does not require migrating its section structure, grammar,
runtime tuple, widget vocabulary, or implementation topology. A later version
must restate every behavior it adopts.

## Authority flow

```text
doctrine ── judges designs
studies  ── provide disposable evidence
RFCs     ── preserve durable decisions and rationale
specs    ── define observable behavior for one named version
tests    ── check conformance to that version
```

Authority does not flow backward:

- an implementation or study cannot silently amend a specification;
- an RFC does not define behavior until a version incorporates it;
- a version does not become doctrine merely because it shipped; and
- doctrine cannot retroactively change the behavior of an existing version.

## Removal rule

Stable routers may link to the currently relevant disposable subtree. Durable
doctrine and RFC reasoning must not require a disposable leaf to remain
available. When a draft is abandoned:

1. update its stable router;
2. copy any durable conclusion into an RFC;
3. delete the draft body rather than keeping obsolete prose in normal
   navigation; and
4. use Git history when historical recovery is needed.

Compatibility pointers may remain at paths already embedded in tools or
external documents, but they must contain no copied language semantics.
