# Uhura v0 widget draft

- **Status:** Historical mixed implementation reference and design draft
- **Scope:** Retired v0 incubation only; not Uhura 0.4 behavior
- **Lifetime:** Disposable with the v0 language draft
- **Taxonomy:** [Stable widget taxonomy](../../README.md)
- **Language model:** [v0 incubation language model](../../../spec/drafts/v0.md)

This subtree records the exact capability names, implementation state, known
defects, and proposed additions explored before the Uhura 0.4 replacement. It
is deliberately neither the current 0.4 catalogue nor a permanent widget
catalogue. Source links are historical provenance and may now point at
replacement implementations.

Entries may mix implemented and proposed material only when every claim is
labelled. The entire subtree may be rewritten or deleted when v0 is replaced.
No later language generation inherits these names or contracts by default.

## Elements

| Entry | Facets | Availability | Decision | Implementation |
|---|---|---|---|---|
| [`<button>`](elements/button.md) | None | Native element in the pre-0.4 checker | Control taxonomy and some state semantics unsettled | Historical checked action control and browser realization; known composition and accessibility gaps documented |
| [`<scroll>`](elements/scroll.md) | None | Native element in the pre-0.4 checker | No accepted widget RFC | Historical element and Play behavior; static preview pose proposed |
| [`<icon>`](elements/icon.md) | [Icon font](integrations/icon-font.md) | Built-in Lucide family and local families implemented | Font-only realization selected before v1; permanent resource model open | Checked token, strict WOFF2 pipeline, host resources, and browser realization implemented |
| [`<img>`](elements/img.md) | None | Native element in the pre-0.4 checker | Renamed from `<image>` during the v0 experiment; no accepted widget RFC | Historical typed asset, accessibility, Editor, and Play realization |
| [`<view>`](elements/view.md) | None | Native element in the pre-0.4 checker | Neutral container implemented; semantic role taxonomy unsettled | Historical checker/runtime/browser realization; known ARIA defects documented |

## Integrations

| Entry | Applies to | Decision | Implementation |
|---|---|---|---|
| [Icon font](integrations/icon-font.md) | [`<icon>`](elements/icon.md) | Sole icon-resource mechanism in the retired v0 incubation line | Historical built-in and local WOFF2 implementation across checker, host, Editor, and Play |

Availability, decision status, implementation status, and version support are
separate axes. A prototype may be implemented without being accepted, and an
accepted design may remain unimplemented.
