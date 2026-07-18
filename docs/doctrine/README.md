# Uhura doctrine

- **Status:** Durable working doctrine
- **Lifetime:** Version-independent; expected to survive language rewrites
- **Authority:** Design rubric, never proof of version behavior
- **Specification router:** [Uhura specifications](../spec/README.md)
- **Decision history:** [Uhura RFCs](../rfcs/README.md)
- **Research router:** [Uhura studies](../studies/README.md)

This directory is the canonical source for how Uhura language changes are
judged. It is deliberately a small language-design corpus, not a committee
charter and not an unversioned specification.

Doctrine should survive changes to grammar, IR, runtime architecture, widget
catalogues, and implementation. A routine version rewrite should not require
editing doctrine. If it does, reviewers must decide whether the version is
violating the design center or whether evidence has genuinely changed the
design center.

## Documents

| Document | Theme | Purpose |
|---|---|---|
| [Mission and identity](mission.md) | Philosophical and technical | Defines what Uhura is for, the product bet, and the tension between a small honest model and a humane frontend language. |
| [Authoring ergonomics](authoring.md) | Human and empirical | Defines readability, semantic compactness, good defaults, concept budgets, and how those claims should be measured. |
| [Design principles](principles.md) | Mixed review rubric | Provides the questions and evidence expected when accepting, rejecting, or revising a language feature. |

Documents not indexed here are not implicit doctrine.

## Eligibility

Doctrine may state:

- the mission and product audience;
- enduring ownership and honesty principles;
- criteria for deciding which language and implementation layers Uhura should
  own;
- the tension between mathematical modeling and authoring ergonomics;
- readability, compactness, accessibility, and evidence standards; and
- questions every language generation should answer.

Doctrine must not define:

- current grammar or accepted spellings;
- exact state, event, queue, output, trace, or ABI shapes;
- widget names, properties, events, or availability;
- current renderer, host, compiler, or crate topology;
- compatibility behavior for a particular version; or
- an implementation fact.

Candidate formal models, syntax sketches, and experiments belong in
[studies](../studies/README.md). Accepted rationale belongs in an
[RFC](../rfcs/README.md). An exact proposal may live in an explicitly
disposable incubation draft under [`spec/`](../spec/README.md); supported
behavior belongs only in a named version specification and conformance suite.

## Authority and lifetime

| Family | Lifetime | Question it answers |
|---|---|---|
| Doctrine | Durable and live | How should Uhura judge a design? |
| Version specification or reference | Frozen per supported version | What observable behavior does this version define? |
| Incubation draft | Disposable and replaceable as a unit | What exact design is currently being explored? |
| RFC | Durable historical record; supersedable | What decision was made, and why? |
| Study router | Stable navigation only | Where is current research classified? |
| Study leaf | Disposable | What evidence, experiment, or unresolved idea was explored? |
| Implementation and examples | Replaceable evidence | What has been tried or shipped? |

Doctrine does not override the observable contract of an existing version.
Conversely, an implementation or version does not become good language design
merely by existing. RFC acceptance records a decision; only a named version
specification and its conformance suite define that version's behavior.

## Doctrine is revisable

These documents are working hypotheses. When a valuable proposal conflicts
with them, the review must distinguish three possibilities:

1. the proposal is a poor fit for Uhura;
2. the doctrine is wrong, incomplete, or stated too broadly; or
3. the proposed ownership or abstraction boundary is wrong.

The correct result may be to reject the feature, reshape it, or revise the
doctrine. A principle must not survive contrary evidence merely because it was
written first. Equally, a local exception must not silently hollow out a
principle.

If doctrine changes, update it explicitly and preserve the reasoning in an
RFC. A study may provide evidence, but durable reasoning must not depend on a
disposable document remaining in the tree.

## Lightweight change path

Uhura does not need a language committee to use disciplined evidence:

```text
problem or observation
  -> disposable study or bounded prototype when useful
  -> RFC when a durable decision is needed
  -> versioned specification and conformance
  -> supported implementation
```

Small corrections can remain small. A change needs more study when its
semantics, ownership, ergonomics, or compatibility cannot yet be explained.
The [design principles](principles.md) are a review aid, not a scorecard or
ceremonial gate.
