# Uhura widget catalogue

- **Status:** Initial catalogue; implemented capabilities documented
- **Authority:** Reference index; listing does not imply acceptance or support
- **Master specification:** [Uhura specification](../spec/README.md)
- **Decision records:** [Uhura RFCs](../rfcs/README.md)
- **Research:** [Uhura Working Group](../working-group/README.md)

This directory is the canonical documentation home for Uhura UI capabilities.
It gives elements, surfaces, reusable patterns, integrations, and cross-cutting
behaviors one discoverable catalogue without pretending that they share one
implementation mechanism.

In this documentation, **widget** is an umbrella term. A **catalog element** is
the narrower machine-readable primitive loaded by the checker. The distinction
must remain explicit in every entry.

## Taxonomy

Each concrete capability has one primary form and zero or more facets. A shared
facet document may define a reusable integration or behavior contract without
being a capability itself.

| Role | Classification | Directory | Meaning |
|---|---|---|---|
| Primary form | Element | [`elements/`](elements/) | A semantic primitive declared by a versioned element catalog and realized by a renderer. |
| Primary form | Surface | [`surfaces/`](surfaces/) | A Core-managed presentation layer or orchestration primitive outside the element tree. |
| Primary form | Pattern | [`patterns/`](patterns/) | A reusable composition of existing language, elements, components, and CSS that adds no primitive contract. |
| Facet | Integration | [`integrations/`](integrations/) | A renderer or host-backed boundary with an external platform or service. |
| Facet | Behavior | [`behaviors/`](behaviors/) | A cross-cutting contract that affects one or more capabilities or their realization. |

Primary form classifies what a capability is. Facets record additional
contracts that may apply to any primary form. Neither axis decides whether a
capability is built-in, opt-in, experimental, or accepted.

## Catalogue

| Entry | Primary form | Facets | Availability | Decision | Implementation |
|---|---|---|---|---|---|
| [`<button>`](elements/button.md) | Element | None | Built-in base catalog; project-pinned during incubation | Control taxonomy and some state semantics unsettled | Checked action control and browser realization implemented; known composition and accessibility gaps documented |
| [`<scroll>`](elements/scroll.md) | Element | None | Built-in base catalog; project-pinned during incubation | No accepted widget RFC | Element and Play behavior implemented; static preview pose proposed |
| [`<icon>`](elements/icon.md) | Element | [Icon font](integrations/icon-font.md) | Built-in default family and local families planned | Font-only realization selected before v1; permanent v1 resource model open | Checked token implemented; provisional SVG renderer remains until the font pipeline lands |
| [`<img>`](elements/img.md) | Element | None | Built-in base catalog; project-pinned during incubation | Renamed from `<image>` to align the narrow primitive with HTML; no accepted widget RFC | Typed asset and accessibility contract plus native browser Editor/Play realization implemented |
| [`<view>`](elements/view.md) | Element | None | Built-in base catalog; project-pinned during incubation | Neutral container implemented; semantic role taxonomy unsettled | Checker/Core/browser realization implemented; list role overwrite and ARIA-nonconforming tablist documented |

When an entry is added, list it here with its primary form, facets,
availability, decision record, implementation status, and supported renderers.

## Shared facets

| Entry | Classification | Applies to | Decision | Implementation |
|---|---|---|---|---|
| [Icon font](integrations/icon-font.md) | Integration | [`<icon>`](elements/icon.md) | Sole icon-resource mechanism before v1 | Font pipeline pending; current SVG table is provisional and non-conforming |

## Entry requirements

Start from [`TEMPLATE.md`](TEMPLATE.md). Every entry should state:

- whether it documents a capability or shared facet;
- its primary form when it is a capability, applicable facets, and
  implementation owners;
- whether availability is undecided, built-in, or opt-in;
- the relevant RFC and specification status;
- its semantic contract, including props, events, children, slots, or intents;
- state and effect ownership;
- accessibility and static-validation requirements;
- renderer or host requirements and fallback behavior;
- motion and reduced-motion behavior where applicable; and
- executable conformance coverage.

Availability, decision status, and implementation status are separate axes. A
prototype may be implemented without being supported, and an accepted design
may remain unimplemented.

## Authority boundary

This catalogue is an index and capability reference. It does not create
language law:

1. Working-group documents hold non-normative research.
2. RFCs record durable decisions.
3. Once versioned work begins, a versioned specification and executable
   conformance suite define observable behavior that implementations may claim
   to support.
4. Machine-readable catalogs and implementation code remain artifacts to which
   an entry links; prose here must not silently override them.

Proposed entries must say so prominently. Merely adding a document here does
not reserve a name, add a builtin, or require renderer support.
