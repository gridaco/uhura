# `<pager>`

- **Status:** Implemented paged viewport; `page-change` declared but never wired, indicator semantics unsettled
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Current spike design; no accepted widget RFC
- **Specification:** Pre-specification; uncontrolled paging implemented, observation unrealized
- **Implementation:** Checker, Core semantic view, browser Editor, and Play implemented; `page-change` unimplemented in every renderer
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<pager>` declares a paged viewport: one slide visible at a time, slides
supplied by exactly one keyed `{#each}`. It is a catalog element, not a
user-authored component, and source cannot invent its properties or events.

The current base catalog is stored with the Instagram project while Uhura is
incubating. Calling the element built-in describes its authoring and checking
role; it does not yet mean that a globally packaged catalog exists.

## Why Uhura needs an explicit pager element

The web builds carousels out of scroll containers, scroll-snap CSS, and
JavaScript that reverse-engineers the current page from scroll offsets. The
result is a mechanism, not a semantic: nothing identifies "this is a paged
collection of N slides, currently on slide k" to a compiler, a static design
tool, or a non-web renderer. The ARIA Authoring Practices
[carousel pattern](https://www.w3.org/WAI/ARIA/apg/patterns/carousel/) exists
because that identity must otherwise be reconstructed by hand every time.

An explicit `<pager>` gives Uhura one stable owner for:

- **the paged-collection identity** — a viewport whose children are exactly
  one keyed `{#each}`, checked statically (`UH5006 markup/bad-children`
  names the rule: *"children come from exactly one keyed `{#each}`"*);
- **the accessible name** — `label` is required, so an unnamed carousel is a
  compile error rather than an audit finding;
- **the page indicator** — declared as data (`indicator="dots"`), realized
  by the renderer, instead of a hand-built sibling widget that can drift
  from the real page count; and
- **future page observations** — the catalog reserves `page-change` as an
  observation event so the machine can one day react to paging the same way
  `near-end` works on `<scroll>`.

This follows established practice in declarative UI systems: Flutter's
[`PageView`](https://api.flutter.dev/flutter/widgets/PageView-class.html) is
an explicit paged viewport distinct from its general scroll views, with the
page as a first-class concept rather than a derived offset.

CSS keeps owning slide sizing, snap physics presentation, and indicator
appearance. `<pager>` exists because paged-collection identity is a semantic
capability rather than aesthetics.

## Current semantic contract

Real usage from the Instagram post card's photo carousel:

```uhura
<pager class="media" indicator="dots" label="Photo carousel">
  {#each c.slides as s (s.id)}
    <img class="slide" src={s.src} alt={s.alt} />
  {/each}
</pager>
```

| Contract | Current behavior |
|---|---|
| Class | `layout` |
| Viewport | Yes; the catalog meta-schema permits observation events only on viewports |
| Children | Exactly one keyed `{#each}` (`UH5006` otherwise) |
| `indicator` | Optional enum `none` or `dots` |
| `label` | Required `text`; the accessible name |
| `class` | Universal, CSS-owned class list |
| `page-change` | Declared observation event (`kind = "observe"`); see below — no renderer emits it |
| Current page | Renderer-owned physical state; absent from Uhura state and the semantic view |

### `page-change` is declared, never realized

This must be stated honestly. The catalog declares `page-change` — its own
comment says *"Declared for controlled use; the spike never binds it"* — and
the checker would accept an `on:page-change={emit …}` binding. But no
occurrence in the corpus binds it, and no renderer code references the event
at all: the browser pager's scroll listener only repaints the dots. A bound
`page-change` today would check, lower, and then never fire. The event is a
reserved name plus checker eligibility, not a working capability, and this
page labels it **unimplemented** accordingly.

## Ownership

Core owns the slide list (through the keyed `{#each}`) and nothing else. The
renderer owns the current page, snap physics, and indicator state. Like
`<scroll>`'s physical offset, the current page is deliberately not
application state in the uncontrolled model; whether a controlled variant
(machine-owned page index) should exist is an open question below.

## Accessibility and validation

The checker enforces, with golden or corpus coverage:

- catalog closure — unknown properties and events on `<pager>` are rejected;
- `label` is required (`UH5004 markup/missing-required-prop`);
- the children model is exactly one keyed `{#each}` (`UH5006`), asserted
  directly by the contract corpus; and
- `indicator` values are checked against the enum.

The browser realization maps `label` to `aria-label` and sets
`role="group"`. The dots are mechanic DOM marked `aria-hidden="true"`.

Known accessibility gaps, stated honestly:

- **No current-page semantics.** Nothing announces "slide 2 of 5". The dots
  are hidden from assistive technology and there is no live region, no
  `aria-roledescription="carousel"`, and no per-slide group labelling from
  the APG carousel pattern.
- **No keyboard paging.** No previous/next controls exist and none are
  synthesized; paging is native horizontal scrolling of the track only.
- **Reduced-motion behavior is undefined** for snap animation.
- The checker validates that `label` is bound, not that its text is
  non-empty or useful.

## Rendering and platform behavior

Browser Editor and Play realize the element as a `div` with class
`uh-pager` (plus authored classes) owning two pieces of mechanic DOM: a
scroll-snap track that hosts the semantic slides, and, when
`indicator="dots"`, an indicator layer:

```html
<div class="uh-pager authored-classes" role="group" aria-label="…">
  <div class="uh-track" data-uh-mechanic="track"><!-- semantic slides --></div>
  <div class="uh-dots" data-uh-mechanic="dots" aria-hidden="true"><span class="uh-dot">…</span></div>
</div>
```

The track is `display: flex; overflow-x: auto; scroll-snap-type: x
mandatory` with each slide `flex: 0 0 100%`, so paging is native CSS
scroll-snap in both Editor and Play. Semantic children are reconciled into
the track, not the pager element itself; the mechanic track and dots are
never counted as semantic children, and reconciliation leaves them in place
when semantic children are swept — the same mechanic-DOM policy as the
textfield's inner input. A track scroll listener keeps the active dot in
sync with the scroll position; the dot count follows the semantic child
count on every apply.

A native renderer may realize the contract with its platform paged-view
primitive. It must reproduce the observable semantics — accessible name,
one-slide-at-a-time paging over the keyed children, and the declared
indicator — or reject the capability honestly.

## Motion

`<pager>` defines no semantic motion. Snap animation, momentum, and
indicator transitions are renderer and platform presentation; no completion
event exists (and `page-change`, which could anchor one, is unrealized).

## Conformance

Existing executable coverage proves:

- the base catalog exposes `pager` as a layout viewport whose children model
  is keyed-each, with `indicator`, required `label`, and a declared
  `page-change` observation (contract corpus asserts the children model and
  the catalog meta-schema);
- the complete Instagram corpus checks and lowers with pager semantic nodes
  in the post card and reel card; and
- renderer policy tests pin that mechanic pager DOM is excluded from
  semantic realization.

A durable support claim additionally requires conformance coverage for:

- `page-change` end-to-end, once it is designed: cadence, payload (page
  index? key?), and settling semantics — or its removal from the catalog;
- dot-count and active-dot behavior under slide insertion, removal, and
  reorder through the keyed `{#each}`;
- accessibility-tree assertions for the group name and a decided
  current-page announcement model; and
- equivalent semantics or an honest unsupported-capability diagnostic in
  non-browser renderers.

## Decisions and open questions

This page is part of the v0 element documentation effort signalled in
[gridaco/uhura#22](https://github.com/gridaco/uhura/issues/22). The current
implementation and
[Instagram spike design](../../../../studies/instagram-spike-design.md)
establish useful evidence but do not replace an accepted widget RFC. The
`§10` cited by the catalog and diagnostics is the spike design's
semantic-element-catalog section, not a versioned language specification.

Known gaps and open questions:

1. `page-change` has no design: no payload shape, no cadence (per snap
   settle? per crossing?), no renderer wiring, and no corpus usage. Whether
   it ships, changes shape, or is dropped from the catalog is undecided.
2. Whether a controlled pager (machine-owned page index, programmatic
   paging) should exist, mirroring the controlled-promotion mechanism the
   textfield uses.
3. The indicator enum is `none | dots`; numbered, fraction ("2/5"), and
   thumbnail indicators have no owner, and whether indicators stay catalog
   data or become authored markup is open.
4. Current-page announcement and the APG carousel roles are undecided;
   `role="group"` plus a hidden indicator is known to be insufficient.
5. Keyboard and assistive paging controls (previous/next) have no owner —
   authored buttons cannot target the pager because no paging intent exists.
6. Autoplaying or looping pagers are not part of the contract.

Current implementation references:

- [Base catalog declaration](../../../../../examples/instagram/client/catalog/base.toml)
- [Catalog and markup checking](../../../../../crates/uhura-check/src/markup.rs)
- [Browser property mapping and mechanic track/dots](../../../../../web/src/renderer/appliers.ts)
- [Track child-hosting in reconciliation](../../../../../web/src/renderer/reconciler.ts)
- [Mechanic-DOM realization contract](../../../../../web/src/renderer/contracts.ts)
- [Play pager styles](../../../../../web/src/play/shell.css)
- [Contract corpus tests](../../../../../crates/uhura-check/tests/contracts_corpus.rs)
- [Current Instagram post-card usage](../../../../../examples/instagram/client/components/post-card.uhura)
- [Specification router](../../../../spec/README.md)
