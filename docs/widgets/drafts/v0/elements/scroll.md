# `<scroll>`

- **Status:** Implemented element; static preview pose proposed
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Current spike design; no accepted widget RFC
- **Specification:** Pre-specification; physical position is renderer-owned
- **Implementation:** Checker, semantic view, browser Editor, and Play implemented; preview pose unimplemented
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<scroll>` declares a semantic viewport. It is a catalog element, not a
user-authored component, and source cannot invent its properties or events.

The current base catalog is stored with the Instagram project while Uhura is
incubating. Calling the element built-in describes its authoring and checking
role; it does not yet mean that a globally packaged catalog exists.

## Why Uhura needs an explicit scroll element

HTML does not need a dedicated `<scroll>` element. CSS can make an arbitrary
block, flex, or grid box a scroll container through `overflow`. That is a useful
web rendering mechanism, but it does not identify a portable semantic viewport
to a compiler, static design tool, or non-web renderer. The
[CSS Overflow specification](https://www.w3.org/TR/css-overflow-3/) makes this
box-level model explicit.

Uhura needs to know which node owns scrolling before a renderer exists. An
explicit `<scroll>` gives the checker and every renderer one stable owner for:

- the active scroll axis;
- viewport observations such as `near-end`;
- lifecycle and position restoration;
- deterministic static-preview positioning;
- future renderer capability negotiation and fallback; and
- future viewport-specific accessibility and interaction rules.

A `<view>` styled with `overflow: auto` may happen to scroll in a browser, but
it is not a semantic Uhura viewport. It cannot legally bind `near-end`, does not
participate in the Play scroll-position cache, and gives a native renderer no
portable instruction.

This follows established UI-tool practice. Flutter exposes an explicit family
of [scrolling widgets](https://docs.flutter.dev/ui/widgets/scrolling), and its
[`ScrollController.initialScrollOffset`](https://api.flutter.dev/flutter/widgets/ScrollController/initialScrollOffset.html)
can set an initial physical position. Figma requires authors to apply explicit
[scroll overflow behavior](https://help.figma.com/hc/en-us/articles/360039818734-Prototype-scroll-and-overflow-behavior)
to a frame and distinguishes vertical, horizontal, both-direction, and
non-scrolling prototypes.

Uhura retains CSS for size, layout, clipping, scrollbar appearance, and visual
treatment. `<scroll>` exists because viewport ownership and observations are
semantic capabilities rather than aesthetics.

## Current semantic contract

```uhura
<scroll class="feed-scroll"
        direction="vertical"
        on:near-end={emit feed-near-end()}>
  <!-- children -->
</scroll>
```

| Contract | Current behavior |
|---|---|
| Class | `layout` |
| Viewport | Yes |
| Children | Any valid markup |
| `direction` | Optional `vertical` or `horizontal`; browser default is `vertical` |
| `class` | Universal, CSS-owned class list |
| `near-end` | Optional observation event with no renderer-carried payload fields; authored event arguments may still carry descriptor payload |
| Physical offset | Renderer-owned; absent from Uhura state and semantic view data |

The browser realizes `<scroll>` as a constrained overflow container. CSS must
give it a finite viewport before content can overflow.

The catalog intends `near-end` to fire once when the end enters a zone one
viewport away, re-arm after leaving that zone, and re-evaluate when content
extent grows. Descriptor absence means that no observation is installed.
Current Play behavior proves an approximation for vertical scrolling: it
observes a sentinel using an all-sided `100%` intersection margin and watches
vertical content growth. Horizontal geometry is not yet axis-specific.

Play caches physical pixel positions per navigation instance and stable node
key so Back can restore a page where the user left it. This cache is renderer
state. It is not application state, is not visible to Uhura expressions, and
does not travel through Core.

Editor previews are inert. They currently realize every scroll container at
its start position and never emit `near-end`.

## Static preview pose

A static example often needs to communicate a state that is only visible after
scrolling: content below the fold, a sticky treatment, a pagination affordance,
or the relationship between the viewport and a long list. Always rendering the
top makes those examples incomplete.

The preview control should use an exact normalized fraction instead of pixels
or a naked `0..100` convention. Pixels are brittle across frame sizes, fonts,
content changes, and renderers. A unitless value in `0..1` describes the
intended pose while leaving measurement to the renderer and follows the
proposed language-wide rule for bounded proportional scalars.

### Proposed authoring shape

The following syntax is proposed, not accepted or implemented:

```uhura
<scroll ref="feed" class="feed-scroll">
  <!-- children -->
</scroll>
```

```uhura
example midway {
  projection feed.feed-page = fixture.feed.pages-1-2

  preview {
    scroll feed = 0.5
  }
}
```

`ref` is proposed as a general compiler-owned authoring identity, initially
consumed here for static preview targeting. It is not a CSS class, DOM id,
catalog property, runtime value, or semantic-view property. Source references
must be unique within a subject. For a particular example, the reference must
also resolve to exactly one realized `<scroll>` occurrence; zero or multiple
matches, including ambiguous repeated component or collection instances, are
diagnostics.

The `preview` clause belongs to `.examples.uhura`, whose contents are excluded
from runtime checked IR. This makes the pose reproducible and reviewable without
turning physical scroll position into application state.

### Normalized position semantics

| Input | Meaning |
|---|---|
| No effective pose after inheritance | Start position (`0`) |
| `0` | Logical start of the active axis |
| `1` | Logical end of the active axis |
| A value between `0` and `1` | That fraction of the maximum scrollable range |

For the active axis:

```text
range  = max(0, scroll-extent - viewport-extent)
offset = range * position
```

The authored value is an exact fraction in `0..1`; values outside that range
are checker errors and must never be silently clamped. This requires a future
exact fixed-point or rational Uhura scalar, not binary floating-point runtime
state. Renderer pixel rounding is allowed, but the same inputs and layout must
choose the same effective position.

Omission in a derived example does not reset an inherited pose. The child must
explicitly provide a new value for that target to replace the inherited value.

`direction` selects the axis. Positions use logical start and end, so a
horizontal renderer must map them correctly for writing direction rather than
exposing platform-specific `scrollLeft` signs.

When content does not overflow, the effective offset is `0`. This is not a Core
error because only the renderer knows final layout; the Editor may surface a
preview warning.

### Preview-only invariants

- The Editor applies the pose after the preview tree, stylesheet, and asset
  sizing are realized, and reapplies it after a fresh realization.
- Applying a pose is instant. It has no animation and does not depend on
  reduced-motion preferences.
- It emits no `near-end` event and performs no Core transition.
- It never enters `ProgramIr`, `uhura-view`, checkpoints, traces, or Play.
- A child example inherits poses through `from`; a child value for the same
  target replaces the inherited value.
- Play continues to use fresh, user-driven, and route-restored positions. A
  future runtime initial-position feature needs a separate name and explicit
  precedence rules.

The preview model should carry the resolved pose as Editor-only data keyed by a
renderer-neutral semantic node reference. The browser may then apply it through
the existing post-realization node mapping without adding scroll effects to the
general read-only Editor policy.

## Accessibility and validation

`<scroll>` does not create a landmark role by itself. Child content retains its
own semantics and order. Renderers must preserve ordinary platform scrolling
and must not hide focusable descendants. Applying the proposed static pose must
not synthesize application events.

The checker currently validates the element name, direction enum, child model,
and event eligibility. A future scroll RFC should also settle whether named
scroll regions need a portable label or role contract and how keyboard-only
access works when a viewport contains no naturally focusable descendant.

Preview references must be stable across formatting and must not use generated
node ordinals or CSS selectors. Resolution must diagnose a non-scroll target,
an absent target, and ambiguous repeated instances.

## Rendering and platform behavior

The current browser renderer maps `<scroll>` to a `div` with vertical or
horizontal overflow CSS. A native renderer may use its platform scroll-view
primitive. Under the preview-pose proposal, a static renderer that cannot
realize scrolling should render the selected pose and report that interactive
scrolling is unsupported.

Scrollbar visibility, overscroll effects, momentum, snapping, measurement,
virtualization, and physical offset remain renderer concerns unless separately
promoted into portable contracts.

## Motion

User-driven scrolling follows platform mechanics. The proposed static preview
pose is applied instantaneously and defines no animation contract.

## Conformance

Current and proposed conformance coverage should include:

- omitted and explicit vertical/horizontal direction;
- valid and invalid properties and events;
- descriptor-controlled `near-end` subscription and edge re-arming;
- axis-correct horizontal and vertical end observation;
- position restoration by navigation instance;
- preview fractions at `0`, an interior value, and `1`;
- out-of-range preview diagnostics;
- no-overflow and nested-scroll previews;
- absent, ambiguous, repeated, and non-scroll preview references;
- example inheritance and override;
- zero runtime-IR or semantic-view change when only preview poses change; and
- proof that applying a static pose emits no event.

## Decisions and open questions

The current implementation and
[Instagram spike design](../../../../studies/instagram-spike-design.md) establish
useful evidence but do not replace an accepted widget RFC or versioned
specification.

Known gaps:

- The proposed `ref` and `preview` syntax and exact fraction literal are not
  implemented or accepted; the current semantic value model is integer-only.
- Existing parsers reject unknown example clauses, so adding the closed-set
  `preview` clause is a language compatibility change rather than ignorable
  metadata.
- Carrying resolved poses over the closed Editor wire contract would require a
  later EditorState protocol revision; runtime `ProgramIr` and `uhura-view`
  protocols would remain unchanged.
- The catalog's `near-end` threshold is currently validated and hash-pinned,
  but the browser uses a hard-coded one-viewport threshold rather than receiving
  it through checked IR.
- Horizontal `near-end` observation is not yet axis-specific or covered by an
  end-to-end geometry test.
- Preview target identity for repeated component or collection instances needs
  a focused addressing decision.
- Runtime programmatic scrolling, scroll-to-item, controlled position, snapping,
  and restoration precedence are separate features, not implied by the static
  preview pose.

Current implementation references:

- [Base catalog declaration](../../../../../examples/instagram/client/catalog/base.toml)
- [Catalog and markup checking](../../../../../crates/uhura-check/src/markup.rs)
- [Browser scroll policy](../../../../../web/src/play/scroll.ts)
- [Read-only Editor renderer](../../../../../web/src/renderer/editor.ts)
- [Specification router](../../../../spec/README.md)
