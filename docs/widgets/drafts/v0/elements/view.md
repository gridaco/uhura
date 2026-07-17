# `<view>`

- **Status:** Implemented neutral container; list realization defective and semantic role refinements incomplete
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Current spike design; no accepted widget RFC
- **Specification:** Pre-specification; the roleless container is implemented, but the role taxonomy is unsettled
- **Implementation:** Checker, semantic view, browser Editor, and Play implemented
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<view>` declares a non-interactive structural container and CSS layout hook.
It is a system-defined catalog element, not a user-authored component, raw
HTML `<div>`, transparent fragment, interactive region, or semantic viewport.

The neutral container is fundamental: components and control flow eventually
need a renderer-known node that can group arbitrary checked children without
inventing action or viewport behavior. The current `role` enum is much less
mature. It combines neutral grouping, list structure, a navigation landmark,
and a `tablist` container role under one property even though those refinements
have different validation and rendering obligations.

## Why Uhura needs a first-class view

HTML already has a generic [`div`](https://html.spec.whatwg.org/multipage/grouping-content.html#the-div-element).
Uhura still needs a renderer-neutral structural primitive because source is
not passed through as HTML. A catalog `<view>` gives the checker and every
renderer one stable owner for:

- grouping an arbitrary sequence into one semantic node;
- carrying a CSS class hook through the semantic view;
- preserving a structural and reconciliation boundary across updates;
- hosting elements, components, conditions, matches, and keyed repetition;
- remaining explicitly ineligible for input and viewport events; and
- providing deterministic structure in static Editor previews.

A component can package a composition, but its expansion still needs a catalog
element at its root. A raw `<div onclick>` escape hatch would bypass catalog
closure, event eligibility, renderer support, and accessibility validation.

The current v0 design deliberately removed language-level
`column`, `row`, `stack`, `grid`, and `spacer` elements. `<view>` is the one
generic container and CSS owns layout. That keeps the language surface small,
but it also means the current element is not a Flutter-like portable layout
widget: Core does not understand geometry, and a non-browser renderer would
need a compatible styling layer of its own to reproduce equivalent visual
layout.

## The language-design boundary

A roleless grouping box is a sound primitive. A generic replacement for every
structural or interactive widget is not.

Use the more specific capability when its contract matters:

- `<scroll>` owns a semantic viewport, observations, and position restoration;
- `<button>` and `<region>` own activation and accessible input behavior;
- `<text>` owns human-readable text runs;
- a surface owns modal presentation and focus orchestration; and
- future list, navigation, tabs, landmarks, or decorative-geometry
  capabilities may need contracts stronger than a `role` token.

Like HTML `div`, a neutral view should be the fallback when no more meaningful
element applies. Unlike a transparent fragment, the current browser
realization creates a real box. Empty neutral views are legal and are currently
used as scrims, progress segments, and layout placeholders; a literal
`role="list"` has an additional keyed-child requirement. Whether every renderer
must preserve that box or whether Uhura eventually gains a separate fragment
primitive remains open.

## Current semantic contract

```uhura
<view class="profile-card stack-sm">
  <img src={user.avatar.src} alt={user.avatar.alt} />
  <text>{user.display-name}</text>
  <button label="Follow" on:press={emit follow-tapped()}>
    <text>Follow</text>
  </button>
</view>
```

### Current browser workaround for lists

Current list compositions use a neutral item wrapper:

```uhura
<view role="list" class="people-list">
  {#each people as person (person.id)}
    <view class="person-row">
      <img src={person.avatar.src} alt={person.avatar.alt} />
      <text>{person.display-name}</text>
    </view>
  {/each}
</view>
```

The wrapper receives the current browser renderer's synthesized `listitem` role
so the nested image and text semantics remain intact. It is a temporary
workaround, not a recommended language contract or a checker-enforced item
boundary.

| Contract | Current behavior |
|---|---|
| Class | `layout` |
| Viewport | No |
| Children | Any valid Uhura markup; empty is legal except for the literal-list structural rule |
| `role` | Optional enum: `none`, `list`, `navigation`, or `tablist` |
| `class` | Universal `text`, opaque to Core, and CSS-owned |
| Events | None |
| Focus and activation | None on the view itself |
| State and effects | None |
| Browser realization | Native `div` with class `uh-view` |

“Any valid markup” includes catalog elements, component calls, conditions,
keyed repetition, and matches, all recursively checked. It does not permit raw
text or interpolation: human text still lives inside `<text>`. Interactive
descendants are legal when the view is not already inside an interactive
ancestor. `<view>` does not create an interactive boundary, but it also does
not clear an ambient one; a button nested through a view inside `<region>` is
still rejected.

Every definition needs exactly one root element or one root-producing match,
but the root does not have to be `<view>`.

### Current role mapping

| Authored value | Browser Editor and Play behavior |
|---|---|
| Omitted | No `role` attribute |
| `none` | No `role` attribute; currently equivalent to omission on this non-focusable `div` |
| `list` | `role="list"`; renderer defectively forces `role="listitem"` onto every direct realized child, overwriting any existing role |
| `navigation` | `role="navigation"` only |
| `tablist` | `role="tablist"` only |

This table records implementation behavior. It does not mean that all four
values satisfy a complete portable accessibility contract.

## Ownership

The checker owns catalog closure, role typing, child checking, event rejection,
and the one special structural rule for a literal list. Core evaluates the
authored class, role token, and children into an ordinary semantic node. It has
no `<view>`-specific type, layout engine, ARIA interpretation, list model, or
state machine.

The browser renderer owns the platform element, CSS application, role mapping,
and keyed DOM reconciliation. Play reuses a keyed view when its element kind is
stable and attempts to preserve focus in descendants across ordinary moves.
Editor freshly realizes the same semantic structure inside an inert preview
host. Because `<view>` has no effects, the policy distinction primarily affects
interactive or viewport descendants. Play also preserves the view's platform
node identity and presentation transients across updates, while Editor starts
from a fresh realization.

All geometry is styling state. Flex, grid, stacking, gaps, padding, clipping,
positioning, pointer-event behavior, and visual order do not enter Core or the
semantic properties. A browser view styled with `overflow: auto` may scroll
physically, but it does not become `<scroll>`: it receives no `near-end`
observation, Play position cache, or route restoration.

## Accessibility and validation

A roleless view contributes no named landmark or interactive role. Its
children retain their semantic order and behavior. The view itself is not
focusable and has no keyboard handling.

The checker currently guarantees:

- only the declared `role` and universal `class` properties are accepted;
- literal role values belong to the closed enum and role expressions are
  type-correct;
- `class` is text-valued and discoverable literal class names are checked
  against the compiled stylesheets;
- raw text outside `<text>` is rejected;
- all descendants are recursively checked; and
- every `on:` binding on `<view>` is rejected with guidance to use
  `<region>` for activation.

The checker does not inspect CSS declarations. CSS visual reordering can
therefore diverge from semantic, reading, and focus order without a diagnostic.
Authors must keep meaningful visual order aligned with source order.

### `none` is not hidden

When honored, ARIA `none` removes an element's implicit native semantics; it
does not hide the element's descendants. Under the
[presentational-role conflict rules](https://www.w3.org/TR/wai-aria-1.2/#conflict_resolution_presentation_none),
user agents ignore it on focusable or interactive elements and on elements
with global ARIA properties. The current browser renderer omits the attribute
entirely because its non-focusable neutral `div` already has no useful native
role to remove. Thus `role="none"` and omission are currently indistinguishable
in browser output.

This token must not be used as a substitute for decorative or hidden-content
semantics. Empty CSS-only views add no meaningful descendants, but a view with
content continues to expose that content. The cross-renderer purpose of an
explicit `none` value remains unsettled.

### List structure and its current defects

The [WAI-ARIA list role](https://www.w3.org/TR/wai-aria-1.2/#list) requires the
list to own at least one list item. Separately, Uhura's current synthesis and
identity policy requires exactly one direct keyed `{#each}` for a literal
`role="list"` and rejects static siblings before or after it. Each keys are
identity-typed, and duplicate evaluated keys are errors.

That rule is incomplete:

- `role={expression}` can evaluate to `list` while bypassing the rule because
  only a literal value activates the special check;
- one iteration may produce multiple sibling nodes, so one data item need not
  correspond to one semantic list item;
- an empty collection produces no list items and therefore violates the
  current required-owned-elements model unless an appropriate busy/loading
  state is represented; `<view>` cannot express that state;
- two sources cannot form one list, which has led the comments surface to
  render persisted and optimistic comments as adjacent lists; and
- the dependent rule is hardcoded in the checker rather than represented by
  catalog data.

The browser renderer supplies list items by forcing `role="listitem"` onto every
direct child, overwriting any existing role. That works only when the child is
a neutral wrapper. A direct button loses its native button role; a direct
`<img>` loses its native image semantics; a pager loses `group`; and a nested
list, navigation landmark, or tablist loses its own role. An existing renderer
test currently records this behavior.
One DOM node cannot represent both the list-item boundary and the child's
independent semantics.

The Instagram implementation happens to use view-root components or explicit
view wrappers for every current list item, but the checker does not enforce
that convention. A durable design needs an explicit item boundary, a mechanic
wrapper owned by the renderer, or stricter composition validation.

Dynamic role expressions introduce a second standards boundary. The
[WAI-ARIA role model](https://www.w3.org/TR/wai-aria-1.2/#roles) defines roles
as element types that do not change with time or user actions. Today
`role={expression}` can both bypass the literal-list structural check and,
under keyed Play reuse, mutate the same DOM node from one role kind to another.
A durable contract should either require browser-mapped roles to be statically
fixed or replace the semantic and platform node when its role kind changes.

### Navigation is only a landmark token

`navigation` currently adds only the ARIA landmark role. It does not require
navigational children, provide a label, or distinguish multiple navigation
landmarks. The Instagram bottom bar is the sole current use.

The [navigation landmark guidance](https://www.w3.org/WAI/ARIA/apg/patterns/landmarks/examples/navigation.html)
allows one landmark to remain unnamed. Multiple landmarks should be labeled so
their purposes are distinguishable; landmarks with identical link sets should
use the same label. The current `<view>` contract cannot express those labels.
It also does not decide whether navigation destinations should be links,
buttons, route intents, or a dedicated navigation capability.

### `tablist` is not a tabs capability

`tablist` currently adds only the parent role. It does not create `tab` or
`tabpanel` roles, `aria-selected`, accessible relationships, roving focus,
orientation, or arrow-key behavior.

The Instagram profile uses ordinary buttons with `aria-current` under this
container. Because the [WAI-ARIA `tablist` role](https://www.w3.org/TR/wai-aria-1.2/#tablist)
requires owned `tab` elements, the current output is structurally
ARIA-nonconforming; `aria-current` on ordinary buttons does not substitute for
`aria-selected` on tabs. It also does not implement the complete
[WAI-ARIA Tabs Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/), including
panel relationships, labeling, roving focus, and keyboard interaction. The
current token is spike evidence for a future tabs capability, not a supported
accessible tabs default.

## Rendering and platform behavior

Browser Editor and Play realize `<view>` as:

```html
<div class="uh-view authored-classes">…</div>
```

The shared baseline is deliberately small:

```css
.uh-view { display: block; min-inline-size: 0; }
```

There is no semantic axis, distribution, alignment, gap, padding, size,
position, clipping, color, shape, elevation, or layout variant. Project CSS
supplies all of those. `row`, `stack`, `grid`, `screen`, `card`, `scrim`, and
similar names are conventions or reusable patterns, not `<view>` properties.

`<view>` currently has no numeric semantic properties. CSS lengths and
percentages retain normal CSS grammar and are not reinterpreted as Uhura
normalized scalars. If a future unitless semantic property represents a
bounded proportion, it must follow the shared `0..1` normalization rule rather
than introducing an ad hoc `0..100` scale.

A non-browser renderer can map the neutral node and accessibility intent
without supporting CSS. Visual and layout fidelity requires CSS support, an
explicit translation layer, or renderer-specific styling. The current
ARIA-named role refinements are not yet a settled portable contract, and no
renderer may infer semantic actions or viewport behavior from CSS appearance.

The current Instagram corpus provides useful, non-normative evidence:

- 118 views appear across 18 implementation definitions;
- 113 carry a class, while only 15 carry a role;
- the roles are 13 lists, one navigation landmark, and one tablist; `none` is
  unused;
- 17 of the 18 definitions use `<view>` as their root; and
- three empty views serve as a scrim, repeated progress geometry, and a layout
  placeholder.

That breadth validates the need for neutral grouping, but it also demonstrates
“view soup”: screen shells, stacks, grids, overlays, state geometry, landmarks,
and composite widgets are visually similar in source despite having different
semantic needs. Reusable patterns and future specialized capabilities should
remain discoverable without turning every CSS convention into a built-in.

## Motion

`<view>` defines no semantic motion. CSS transitions, transforms, and keyframe
animations are presentation and produce no Core event. The current browser
baseline adds no transition.

Any future built-in layout or visibility transition must define interruption,
completion, reduced-motion behavior, focus order, and whether the semantic
child remains present. It must not infer application completion from a CSS
animation event.

## Conformance

Existing executable coverage currently proves:

- the base catalog loads `<view>` as a layout-class, eventless element;
- the complete Instagram corpus checks and lowers deterministically;
- `on:press` on a view is rejected with the layout-element diagnostic;
- role omission plus current `list`, `navigation`, and `tablist` uses survive
  through semantic-view goldens; `none` has no corpus coverage; and
- Editor and Play share browser structure for a list view and its synthesized
  direct-child list-item roles.

The last renderer test records the current role-overwrite defect; it is not a
durable accessibility guarantee. Its hand-built semantic fixture also bypasses
the source check that requires one keyed each.

A durable support claim additionally requires conformance coverage for:

- exact `children="any"`, non-viewport, eventless, and role-enum declarations;
- empty, nested, component-produced, conditional, matched, and repeated
  children;
- raw-text and unknown property, role, and event rejection;
- current browser omission/`none` mapping without hiding descendants, plus an
  explicit cross-renderer decision for `none`;
- literal and dynamic list-role structural validation;
- exactly one semantic item boundary per collection item;
- empty lists and lists composed from more than one data source;
- preservation of button, img, pager, landmark, and nested-list semantics
  inside list items;
- labels and distinguishability for repeated navigation landmarks;
- either a complete tabs contract or rejection/removal of `tablist`;
- semantic order under authored layout CSS;
- keyed reuse, class changes, and focused descendants across Play updates;
- statically fixed role validation, or semantic and platform-node replacement
  if a future contract permits role-kind changes; and
- equivalent structural semantics or an honest capability diagnostic in
  non-browser renderers.

## Decisions and open questions

1. Whether `<view>` is permanently a real layout box or Uhura also needs a
   transparent fragment/group primitive.
2. Whether `role` remains one enum or list, navigation, tabs, landmarks, and
   other structures become dedicated elements or checked patterns.
3. Whether portable language roles should be intent-level concepts rather
   than browser ARIA token names.
4. Whether semantic roles must be statically fixed, or a role-kind change must
   replace the semantic and platform node rather than mutate it in place.
5. How one logical list item is identified across components, conditions, and
   multi-node iteration bodies without overwriting the child's own role.
6. Whether the one-keyed-each list restriction remains, expands to multiple
   sources, or moves into a dedicated list capability.
7. How named landmarks, descriptions, labels, and cross-node relationships are
   expressed without exposing arbitrary DOM IDs or ARIA attributes.
8. Whether the host owns route-level `main` semantics or pages need a portable
   main-content capability.
9. Whether empty decorative views remain ordinary CSS boxes or need explicit
   decorative/progress semantics in cases where their visual state carries
   information.
10. How non-browser renderers handle CSS-owned layout honestly: implement CSS,
    translate an accepted subset, use renderer-specific styles, or reject the
    capability.
11. Whether common screen, stack, row, grid, card, spacer, and overlay forms
    remain documented patterns or justify any typed built-ins after further
    dogfooding.
12. Whether semantic visibility, measurement, hit testing, or layout
    observation ever belongs on `<view>` rather than separate capabilities.

No current class convention or browser ARIA mapping settles these questions.
An accepted view/widget RFC should preserve the neutral, non-interactive
container while moving stronger semantic promises out of an under-validated
role token.

Current implementation and research references:

- [Base catalog declaration](../../../../../examples/instagram/client/catalog/base.toml)
- [Catalog markup checking](../../../../../crates/uhura-check/src/markup.rs)
- [Core semantic-node evaluation](../../../../../crates/uhura-core/src/eval.rs)
- [Semantic view protocol](../../../../../crates/uhura-core/src/view.rs)
- [Shared browser property mapping](../../../../../web/src/renderer/appliers.ts)
- [Shared browser reconciliation](../../../../../web/src/renderer/reconciler.ts)
- [Shared renderer policy tests](../../../../../web/src/renderer/tests/policies.test.ts)
- [Instagram spike element catalog](../../../../studies/instagram-spike-design.md)
- [Instagram dogfood gaps](../../../../studies/instagram-demo-dogfood.md)
- [Current list composition](../../../../../examples/instagram/client/components/stories-tray.uhura)
- [Current navigation composition](../../../../../examples/instagram/client/components/bottom-nav.uhura)
- [Current tablist composition](../../../../../examples/instagram/client/app/profile/[user]/page.uhura)
