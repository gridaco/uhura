# `<img>`

- **Status:** Implemented asset-backed image; loading, failure, and responsive-source semantics unsettled
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Native element in the current canonical checker
- **Decision:** Renamed from `<image>` during the v0 experiment; no accepted widget RFC
- **Specification:** Pre-specification; single-source asset-backed image implemented
- **Implementation:** Checker, Core semantic view, host asset boundary, browser Editor, and Play implemented
- **Owners:** Checker, Core, Renderer, Host
- **Supported renderers:** Browser Editor and Play

`<img>` declares one non-interactive image resource. It is a system-defined
native element, not raw HTML passthrough, an SVG graphics primitive, a CSS
background, or a Flutter-style provider and lifecycle widget.

The spelling is deliberate. The current semantic concept and browser
realization align with HTML [`img`](https://html.spec.whatwg.org/multipage/embedded-content.html#the-img-element).
Uhura adds typed asset identity, static accessibility validation, host
resolution, and deterministic Editor materialization around that primitive. It
does not currently own enough additional behavior to justify a separate
`<image>` widget contract.

SVG [`image`](https://svgwg.org/svg2-draft/embedded.html#ImageElement) is a
different element positioned in an SVG coordinate system. A future literal SVG
vocabulary may use that spelling inside `<svg>`. A future higher-level image
widget would need a genuinely broader contract before claiming `<image>`.

## Why Uhura still needs a catalog element

Using the HTML name does not make Uhura source HTML. The catalog element gives
the language and every renderer one checked boundary for:

- accepting a typed asset identity instead of arbitrary URL text;
- requiring an explicit informative or decorative choice;
- carrying an opaque identity through Core without fetching it;
- letting the host resolve fixture, local, remote, or signed materialization;
- producing deterministic, provider-free Editor previews; and
- requiring renderers to support the capability or reject it honestly.

Those are language and platform guardrails around the standard image concept,
not a new visual widget taxonomy.

## Current semantic contract

Informative image:

```uhura
<img class="avatar" src={user.avatar.src} alt={user.avatar.alt} />
```

Decorative image:

```uhura
<img class="ornament" src={ornament} decorative />
```

| Contract | Current behavior |
|---|---|
| Class | `content` |
| Children | None |
| `src` | Required `asset` expression |
| `alt` | `text`; selects informative semantics |
| `decorative` | Bare-only presence marker |
| Accessibility choice | Exactly one of `alt` or `decorative` |
| `class` | Universal, opaque, and CSS-owned |
| Events | None |
| Focus and activation | None |
| State and effects | None in Core |
| Browser realization | Native HTML `img` with class `uh-img` |

The accessibility branch is structural. These are invalid:

```uhura
<img src={ornament} />
<img src={ornament} alt="Ornament" decorative />
<img src={ornament} decorative={false} />
<img src={ornament} decorative={true} />
<img src={ornament} decorative={is-decorative} />
```

Boolean members selected by an `exactly-one-of` catalog rule are presence
markers and must be bare. Ordinary boolean properties elsewhere remain
expression-capable.

`<image>` is not a compatibility alias. The 0.2.0 catalog rejects it as an
unknown element and provides migration guidance to use `<img>`.

## Asset identity is not a URL

Authored `src` has type `asset`; a string literal is not a substitute. Core
lowers the evaluated identity to the existing semantic value:

```json
{ "t": "image", "asset": "avatar-mira" }
```

The internal `t: "image"` discriminator names the asset-value wire form, not
the catalog element. It is also used for image-valued video sources and posters
and intentionally remains unchanged by the element rename.

Core never fetches the asset. Materialization happens at the renderer and host
boundary:

- the host validates each declared local asset and optional SHA-256 pin against
  the captured project revision before publishing either surface;
- Editor reads that checked asset table and applies its data URI;
- Editor synthesizes a deterministic SVG stand-in from an unknown asset ID;
- local Play serves the same captured bytes through an encoded asset route; and
- provider-backed Play may exchange the stable ID for a remote or short-lived
  signed URL.

Missing files, unsafe paths, and hash drift in a declared local asset registry
are build errors for both Editor and Play. The deterministic Editor stand-in is
only a rendering fallback for an otherwise-valid program that references an
asset identity absent from the local registry; it does not hide a broken
declaration.

Resolved URLs are platform materialization details. They never become visible
to Uhura expressions, stores, events, snapshots, or application logic.

## Ownership and lifecycle

The checker owns catalog closure, the required asset type, child and event
closure, and the informative/decorative choice. Core evaluates `src` and `alt`
and carries them as ordinary semantic properties. It has no decoder, cache,
network client, image dimensions, or image-specific state machine.

The host or provider owns resolving asset identity to bytes or a URL. The
browser renderer owns native element creation and source application. When a
keyed Play node changes assets, the renderer removes the previous `src` before
starting resolution. Per-element resolution tokens prevent an older async
result from overwriting the newer source.

Resolution failure leaves `src` absent and reports the existing renderer
diagnostic to the console. A later realization may retry. Uhura currently has
no semantic:

- `load`, `error`, progress, retry, decoded, or cache event;
- pending, failed, or fallback child state;
- loading or error builder;
- automatic spinner, skeleton, or retry policy; or
- application-visible native image measurements.

Browser fetching, decoding, caching, and broken-image presentation therefore
remain native platform behavior rather than Core state.

## Accessibility and validation

The browser mapping leaves native image semantics authoritative.

| Authored branch | Browser mapping |
|---|---|
| `alt={text}` | Native `alt="…"` |
| bare `decorative` | Native `alt=""` |

Neither branch synthesizes `role="img"`, `aria-label`, or `aria-hidden`.
HTML uses non-empty alternative text as replacement content and null
alternative text for decorative or redundant images. The WAI guidance likewise
uses [null alternative text for decorative images](https://www.w3.org/WAI/tutorials/images/decorative/).

The renderer defensively emits `alt=""` if malformed semantic input somehow
reaches an informative branch without text, but checked source cannot omit both
branches. The checker does not yet judge alternative-text quality or contextual
correctness. In particular:

- a literal or evaluated empty `alt` passes the informative source branch but
  becomes decorative under native HTML semantics;
- manifest asset metadata does not supply the element's alternative text;
- the same asset may correctly be informative in one composition and
  decorative in another; and
- a functional image must describe its surrounding action or be decorative
  when the enclosing control already has an accessible name.

`<img>` itself is not interactive. Activation belongs to `<button>` or
`<region>`. A native image is valid phrasing content inside a button, but its
accessibility choice must still be explicit.

The current `<view role="list">` realization has a separate known defect: it
forces `role="listitem"` onto every direct child. A direct `<img>` therefore
loses its native image semantics. A neutral item wrapper preserves the nested
image until list composition receives a durable item-boundary design.

## Rendering, intrinsic sizing, and CSS

Browser Editor and Play realize the element as:

```html
<img class="uh-img authored-classes" src="resolved-url" alt="authored text">
```

The shared baseline is intentionally small:

```css
.uh-img { display: block; background-color: #d9d9de; }
```

The native element is replaced content with intrinsic dimensions and an
intrinsic aspect ratio when the decoded resource provides them. Authored CSS
[`object-fit`](https://drafts.csswg.org/css-images-3/#the-object-fit) and
`object-position` now operate on the image itself. The previous browser
realization used a `div` and CSS background, making authored `object-fit`
ineffective; that realization was removed with the rename.

Width, height, aspect ratio, crop, position, border radius, and fit remain
CSS-owned. The base style does not force `cover`, `contain`, or a size. Unsized
images may therefore participate in layout using intrinsic dimensions, and an
asset change may affect layout.

Core carries no MIME type, pixel dimensions, density, orientation, or intrinsic
ratio. Equivalent intrinsic layout across non-browser renderers is not yet a
portable guarantee.

`<img>` currently has no numeric semantic properties. CSS lengths and
percentages retain ordinary CSS grammar. If a future unitless semantic property
represents a bounded proportion, it must follow Uhura's shared `0..1`
normalization rule rather than introducing an ad hoc `0..100` scale.

## Static Editor behavior

Editor materializes the same native element and accessibility branch as Play,
but it never invokes the Play provider or resolves a remote URL. A captured
asset uses its local data URI; an unknown ID receives a deterministic,
ID-derived SVG stand-in. That keeps static boards reproducible and usable before
a live backend exists.

Every Editor realization is fresh inside an inert host. `<img>` owns no effect
channel, but the actual image resource can still affect presentation:

- fallback and real assets may have different intrinsic dimensions;
- CSS sizing is needed when preview geometry must be stable; and
- animated GIF, WebP, APNG, or SVG resources are not currently frozen to a
  deterministic frame.

Static Editor determinism covers asset selection and fallback generation, not a
portable decoded-frame or intrinsic-size contract.

## Motion

`<img>` defines no semantic motion, transition, or completion event. Authored
CSS transitions and animated image resources are presentation. Core does not
observe their frames or completion.

A future animated-resource policy must decide Editor frame selection, pause and
resume ownership, reduced-motion behavior, and whether animation is content or
decoration. Those decisions do not belong to the current narrow primitive.

## Conformance

Existing executable coverage proves:

- the native vocabulary exposes `img` as a childless, eventless content element;
- `src` is required and asset-typed;
- exactly one of `alt` or bare `decorative` is required;
- binding both branches, neither branch, or non-bare `decorative` is rejected;
- legacy `<image>` is rejected rather than retained as an alias;
- the complete Instagram corpus checks and lowers with `img` semantic nodes;
- Editor and Play create native `IMG` elements with class `uh-img`;
- informative and decorative branches map to native `alt` semantics without
  ARIA overrides;
- Editor uses local data URIs and deterministic missing-asset stand-ins; and
- local and provider-backed Play apply native `src`, clear changed sources, and
  ignore stale async resolutions.

A durable support claim additionally requires conformance coverage for:

- literal and runtime-empty alternative text policy;
- children, events, unknown properties, and wrong source-type rejection;
- non-bare `decorative={true}` and dynamic decorative expressions;
- resolver failure, disposal, retry, and keyed source reuse;
- native load failure and replacement-text presentation in a real browser;
- accessibility-tree behavior rather than attribute assertions alone;
- intrinsic sizing and layout shift under source changes;
- animated resources and reduced-motion policy;
- preservation of image semantics inside lists; and
- equivalent semantics or an honest unsupported-capability diagnostic in
  non-browser renderers.

## Decisions and open questions

1. Whether informative `alt` must be non-empty, and how runtime-empty
   expressions are diagnosed.
2. Whether manifest `alt` metadata remains required, becomes an authoring lint
   or suggestion, or is removed from asset identity.
3. Whether portable intrinsic width, height, ratio, or density metadata belongs
   in the asset contract.
4. Whether responsive candidates, density variants, `<picture>`, `srcset`, or
   `sizes` need a language-level representation.
5. Whether lazy loading, decoding, and fetch priority remain renderer policy or
   become checked semantic hints.
6. Whether loading, failure, fallback composition, progress, retry, or cache
   state belongs on `<img>` or a separate behavior/pattern.
7. Which raster, animated, vector, and document formats renderers must support,
   including SVG security and privacy policy.
8. Whether animated resources need a deterministic Editor frame and a
   reduced-motion contract.
9. Whether arbitrary network URLs are ever legal, likely as an opt-in
   integration rather than the built-in `asset` type.
10. How asset negotiation and fallback work across non-browser renderers.
11. Whether a future provider-, fit-, cache-, and lifecycle-owning abstraction
    is sufficiently different to merit a separate `<image>` capability.

No current CSS convention, asset manifest field, or browser-native lifecycle
settles these questions. The narrow `<img>` contract should remain useful
without silently growing into a cross-platform media framework.

Current implementation and research references:

- [Native element and accessibility checking](../../../../../crates/uhura-check/src/checker.rs)
- [Supplemental resource manifest](../../../../../crates/uhura-check/src/resource_manifest.rs)
- [Checked local asset registry](../../../../../crates/uhura-check/src/assets.rs)
- [Semantic view projection](../../../../../crates/uhura-core/src/render.rs)
- [Canonical shared projection renderer](../../../../../web/src/renderer/projection.ts)
- [Browser asset materialization](../../../../../web/src/renderer/assets.ts)
- [Editor snapshot asset table](../../../../../web/src/editor/editor-state.ts)
- [Play provider asset resolution](../../../../../examples/instagram/client/providers/spock.ts)
- [Projection renderer tests](../../../../../web/src/renderer/projection.test.ts)
- [Asset resolution tests](../../../../../web/src/play/tests/assets.test.ts)
- [Current Instagram image composition](../../../../../examples/instagram/client/ui.uhura)
