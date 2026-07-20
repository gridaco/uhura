# Uhura v0 widget draft

- **Status:** Mixed implementation reference and design draft
- **Scope:** v0 incubation only
- **Lifetime:** Disposable with the v0 language draft
- **Taxonomy:** [Stable widget taxonomy](../../README.md)
- **Language model:** [v0 incubation language model](../../../spec/drafts/v0.md)

This subtree records the exact capability names, current implementation, known
defects, and proposed additions being explored for v0. It is deliberately not
the permanent widget catalogue.

Entries may mix implemented and proposed material only when every claim is
labelled. The entire subtree may be rewritten or deleted when v0 is replaced.
No later language generation inherits these names or contracts by default.

## Elements

| Entry | Facets | Availability | Decision | Implementation |
|---|---|---|---|---|
| [`<button>`](elements/button.md) | None | Built-in base catalogue during incubation | Control taxonomy and some state semantics unsettled | Checked action control and browser realization implemented; known composition and accessibility gaps documented |
| [`<scroll>`](elements/scroll.md) | None | Built-in base catalogue during incubation | No accepted widget RFC | Element and Play behavior implemented; static preview pose proposed |
| [`<textfield>`](elements/textfield.md) | None | Built-in base catalogue during incubation | Renamed from `<text-field>` in the v0 catalogue; no accepted widget RFC | Checked controlled promotion, browser realization, and Play draft/IME mechanics implemented; labelling and validation gaps documented |
| [`<icon>`](elements/icon.md) | [Icon font](integrations/icon-font.md) | Built-in Lucide family and local families implemented | Font-only realization selected before v1; permanent resource model open | Checked token, strict WOFF2 pipeline, host resources, and browser realization implemented |
| [`<img>`](elements/img.md) | None | Built-in base catalogue during incubation | Renamed from `<image>` in the v0 catalogue; no accepted widget RFC | Typed asset and accessibility contract plus browser Editor/Play realization implemented |
| [`<view>`](elements/view.md) | None | Built-in base catalogue during incubation | Neutral container implemented; semantic role taxonomy unsettled | Checker/runtime/browser realization implemented; known ARIA defects documented |
| [`<region>`](elements/region.md) | None | Built-in base catalogue during incubation | Activation semantics implemented; role taxonomy and keyboard equivalence unsettled | Checked one-child activation area, supplementary reachability, and browser realization implemented; focus-order and role gaps documented |
| [`<text>`](elements/text.md) | None | Built-in base catalogue during incubation | Text-only children rule implemented; semantic tag and inline-content model unsettled | Checked text-run children, typed interpolation, run joining, and browser realization implemented; no inline markup documented |

## Integrations

| Entry | Applies to | Decision | Implementation |
|---|---|---|---|
| [Icon font](integrations/icon-font.md) | [`<icon>`](elements/icon.md) | Sole icon-resource mechanism in the v0 incubation line | Built-in and local WOFF2 families implemented across checker, host, Editor, and Play |

Availability, decision status, implementation status, and version support are
separate axes. A prototype may be implemented without being accepted, and an
accepted design may remain unimplemented.
