# Uhura widget taxonomy

- **Status:** Stable taxonomy and router
- **Lifetime:** Stable navigation; taxonomy is live and revisable
- **Authority:** Working classification only, never capability behavior
- **Doctrine:** [Uhura doctrine](../doctrine/README.md)
- **Specification router:** [Uhura specifications](../spec/README.md)
- **Decision history:** [Uhura RFCs](../rfcs/README.md)
- **Studies:** [Uhura studies](../studies/README.md)

This is the canonical documentation home for classifying Uhura UI
capabilities. It gives elements, surfaces, reusable patterns, integrations, and
cross-cutting behaviors one discoverable topology without making any current
element name or contract permanent.

In this documentation, **widget** is an umbrella term. A **catalog element** is
the narrower machine-readable primitive recognized by a particular language
version. The distinction must remain explicit.

The classifications below are working vocabulary, not a compatibility
contract or permanent language ontology. They may be revised when another
topology explains the capability space better. Stable paths exist for
navigation and tooling, not to make their current labels semantic law.

## Taxonomy

Each concrete capability has one primary form and zero or more facets. A shared
facet may define a reusable integration or behavior without being a capability
itself.

| Role | Classification | Stable directory | Meaning |
|---|---|---|---|
| Primary form | Element | [`elements/`](elements/) | A semantic primitive declared by a versioned catalog and realized by a renderer. |
| Primary form | Surface | [`surfaces/`](surfaces/) | A language-managed presentation layer or orchestration primitive outside the ordinary element tree. |
| Primary form | Pattern | [`patterns/`](patterns/) | A reusable composition of existing language and capabilities that adds no primitive contract. |
| Facet | Integration | [`integrations/`](integrations/) | A renderer- or host-backed boundary with an external platform or service. |
| Facet | Behavior | [`behaviors/`](behaviors/) | A cross-cutting contract that affects one or more capabilities or realizations. |

Primary form classifies what a capability is. Facets record additional
contracts that may apply. Neither axis decides whether a capability is built
in, opt-in, experimental, accepted, or implemented.

## Version-scoped catalogues

- [v0 widget draft](drafts/v0/README.md) — current implemented and proposed
  capability notes; disposable with the v0 incubation model.

Exact names, properties, events, accessibility requirements, availability,
renderer mappings, and implementation gaps belong under a named version or
draft. A later catalogue must not edit v0 pages into a new language generation.
It should create its own version path and may reuse only the taxonomy that
still helps.

## Stable paths

The taxonomy hubs and [`patterns/`](patterns/) remain stable navigation paths.
The checker currently directs some diagnostics to `docs/widgets/patterns`, so
that path must not become a version-specific contract.

Concrete draft leaves are intentionally not stable. They may be rewritten,
merged, or deleted with their containing draft. A durable RFC must summarize
any rationale that needs to survive their removal.

## Entry requirements

Start from [`TEMPLATE.md`](TEMPLATE.md). Every concrete entry should state:

- its version or draft scope and lifetime;
- primary form, facets, and implementation owners;
- availability and decision status separately;
- semantic contract, including properties, events, children, or slots;
- state and external-effect ownership;
- accessibility and static-validation requirements;
- renderer or host requirements and fallback behavior;
- motion and reduced-motion behavior where applicable; and
- conformance and implementation evidence.

Adding a draft document does not reserve a name, add a builtin, or require
renderer support.
