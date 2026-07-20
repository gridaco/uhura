# `<text>`

- **Status:** Implemented text container; semantic tag and inline-content model unsettled
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Current spike design; no accepted widget RFC
- **Specification:** Pre-specification; text-only children rule implemented
- **Implementation:** Checker, Core lowering, browser Editor, and Play implemented
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<text>` is the only place text can appear. It declares no semantic
properties of its own and no events; its entire contract is its children
model: literal text runs and `{expr}` interpolation, nothing else. It is the
smallest element in the base catalog.

The current base catalog is stored with the Instagram project while Uhura is
incubating. Calling the element built-in describes its authoring and checking
role; it does not yet mean that a globally packaged catalog exists.

## Why Uhura needs an explicit text element

HTML permits bare text nodes anywhere flow content is allowed, so any
container may quietly become a text owner. That ambient permission is what
Uhura removes. Text outside `<text>` is a compile error
(`UH5012 markup/interpolation-outside-text`, with the message *"text content
(and `{expr}` interpolation) lives inside `<text>` only"*), which buys:

- **One typed interpolation site.** `{expr}` is only legal inside `<text>`,
  and every interpolated expression is checked against `text`. The corpus
  converts numbers explicitly (`to-text(post.comment-count)`); there is no
  implicit stringification a renderer could disagree about.
- **Deterministic realization.** Because text owns its runs, lowering keeps
  them as an ordered run list, Core evaluation joins literals and evaluated
  interpolations into a single inert content value, and the renderer applies
  it as one `textContent` write. There is no mixed children reconciliation
  for text nodes at all — the reconciler skips child handling for `text`
  entirely.
- **A portable text primitive.** Non-web renderers receive "a labelled-run
  text node" rather than the DOM's anything-may-contain-text model. This is
  the same judgment as Flutter's explicit
  [`Text`](https://api.flutter.dev/flutter/widgets/Text-class.html) widget:
  text is a widget, not an ambient capability of containers.

CSS keeps owning typography, color, wrapping, and truncation via the
universal `class` attribute.

## Current semantic contract

Real usage from the Instagram post card:

```uhura
<text class="username">{post.author.username}</text>
<text class="meta">{to-text(post.comment-count) ++ (if post.comment-count == 1 then " comment · " else " comments · ") ++ post.posted-label}</text>
```

| Contract | Current behavior |
|---|---|
| Class | `content` |
| Children | Text runs only: literals and `{expr}` interpolation (`UH5006 markup/bad-children` for anything else) |
| Interpolation | Each `{expr}` checks against `text` |
| Props | None declared; `class` is universal |
| Events | None; content-class elements cannot take `on:` bindings |

Core evaluation joins the runs: literals concatenate with evaluated
interpolation results into one plain string, delivered to the renderer as a
single synthesized `content` value. The `content` name is pipeline
machinery, not an authorable property — the catalog declares no props for `text`, and
writing `content=…` in source is rejected as an unknown prop.

## Ownership

Core owns evaluation of interpolated expressions and produces the joined
string. The renderer owns nothing but painting it; there is no state, no
lifecycle, and no failure mode beyond expression evaluation itself.

## Accessibility and validation

The checker enforces, with golden or corpus coverage:

- text and interpolation outside `<text>` are rejected (`UH5012`);
- non-text children inside `<text>` are rejected (`UH5006`); and
- interpolated expressions must type as `text`.

The realization adds no ARIA: a text node is plain content, named by its own
text. Accessibility concerns live where text is consumed — for example a
`<region>` label naming over wrapped text content, documented on the
[region page](region.md).

## Rendering and platform behavior

Browser Editor and Play realize the element as a `p` with class `uh-text`
(plus authored classes), and apply the joined content with a single
`textContent` assignment:

```html
<p class="uh-text authored-classes">joined literal and interpolated runs</p>
```

The `p` tag is renderer policy, not contract. It applies equally to
paragraph-like captions and to inline-like fragments such as usernames; CSS
in the corpus restyles it per class. A native renderer may realize the
contract with its platform text primitive; it must reproduce the joined-runs
content exactly.

## Motion

`<text>` defines no semantic motion. Text change is an ordinary content
update with no transition contract.

## Conformance

Existing executable coverage proves:

- the base catalog exposes `text` as a content element with the text
  children model (contract corpus asserts the catalog's element set and
  meta-schema);
- the complete Instagram corpus checks, lowers, and renders with text nodes
  in every page and component; and
- renderer policy tests exercise text realization alongside the other
  elements.

A durable support claim additionally requires conformance coverage for:

- a golden rejection pinning `UH5012` (text outside `<text>`);
- a golden rejection pinning element children inside `<text>`; and
- bidi, whitespace, and empty-content behavior across renderers.

## Decisions and open questions

This page is part of the v0 element documentation effort signalled in
[gridaco/uhura#22](https://github.com/gridaco/uhura/issues/22). The current
implementation and
[Instagram spike design](../../../../studies/instagram-spike-design.md)
establish useful evidence but do not replace an accepted widget RFC. The
`§4.4` cited by the checker diagnostic is the spike design's markup section,
not a versioned language specification.

Known gaps and open questions:

1. There is no inline markup: emphasis, links, or mixed-style runs inside
   one text node are inexpressible. Instagram-style "username then caption"
   lines are built from sibling `<text>` nodes styled by CSS.
2. The `p` realization is unconditional. Whether the contract needs a
   semantic role or level (paragraph, span-like, heading) or the tag stays
   renderer policy is undecided.
3. Whitespace normalization between runs is unspecified beyond the current
   implementation's literal joining.
4. Truncation, line clamping, and "see more" expansion are CSS or future
   capabilities; nothing semantic exists.
5. Localization concerns — plural rules, directionality isolation around
   interpolated values — have no owner. The corpus hand-rolls English
   pluralization with `if` expressions.

Current implementation references:

- [Base catalog declaration](../../../../../examples/instagram/client/catalog/base.toml)
- [Catalog and markup checking](../../../../../crates/uhura-check/src/markup.rs)
- [Text-run lowering](../../../../../crates/uhura-check/src/lower.rs)
- [Run joining at evaluation](../../../../../crates/uhura-core/src/eval.rs)
- [Browser property mapping](../../../../../web/src/renderer/appliers.ts)
- [Shared browser reconciliation](../../../../../web/src/renderer/reconciler.ts)
- [Contract corpus tests](../../../../../crates/uhura-check/tests/contracts_corpus.rs)
- [Current Instagram post-card usage](../../../../../examples/instagram/client/components/post-card.uhura)
- [Specification router](../../../../spec/README.md)
