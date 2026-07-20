# `<region>`

- **Status:** Implemented semantic activation area; role taxonomy and keyboard equivalence unsettled
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Current spike design; no accepted widget RFC
- **Specification:** Pre-specification; activation semantics implemented
- **Implementation:** Checker, Core semantic view, browser Editor, and Play implemented
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<region>` declares a semantic activation area: a labelled, focusable wrapper
that makes exactly one child element activatable. It is a catalog element, not
a user-authored component, and source cannot invent its properties or events.

The current base catalog is stored with the Instagram project while Uhura is
incubating. Calling the element built-in describes its authoring and checking
role; it does not yet mean that a globally packaged catalog exists.

## Why Uhura needs an explicit region element

The web's ambient answer to "make this area clickable" is `onclick` on a
`div`. That idiom is an accessibility failure by default: the div has no role,
no accessible name, no focusability, and no keyboard activation. The ARIA
Authoring Practices [button pattern](https://www.w3.org/WAI/ARIA/apg/patterns/button/)
exists precisely because retrofitting those four properties onto a generic
container is easy to forget and hard to audit.

Uhura removes the retrofit entirely by making the failure inexpressible:

- **Layout elements can never take handlers.** Binding `on:` to a `<view>` is
  rejected as `UH5002 markup/event-not-declared`, and the diagnostic
  explicitly steers the author: *"`on:` never attaches to layout elements —
  wrap the content in `<region>`"*. The golden rejection
  `reject-event-on-layout` pins that diagnostic. There is no div-onclick to
  misuse.
- **Activation implies a name.** `label` is required, so an activatable area
  without an accessible name fails checking (`UH5004
  markup/missing-required-prop`) before anything renders.
- **Activation implies keyboard access.** The Play renderer gives every
  region `role="button"`, `tabindex="0"`, and Enter/Space activation — the
  author cannot opt out of focusability by omission.
- **Redundant gestures must have a first-class path.** A gesture-only
  affordance (Instagram's double-tap-to-like) is marked `supplementary`, and
  the checker requires the same machine event to also be emitted from a
  focusable element in the same definition (`UH5020
  markup/supplementary-unreachable`). WCAG's
  [pointer gesture guidance](https://www.w3.org/WAI/WCAG22/Understanding/pointer-gestures.html)
  asks for exactly this single-pointer/keyboard alternative; Uhura makes the
  omission a compile error rather than an audit finding.

`<button>` remains the element for a self-contained action control with
content children. `<region>` exists for the wrapper case: an area of existing
markup — an author row, a media block, a metadata line — whose activation is a
navigation or gesture semantic layered over content. CSS keeps owning all of
its appearance.

## Current semantic contract

Real usage from the Instagram post card (rendered by the feed page):

```uhura
<region label="View profile" on:activate={emit author-tapped(user: post.author.id)}>
  <view class="author-row">
    <img class="avatar" src={post.author.avatar.src} alt={post.author.avatar.alt} />
    <text class="username">{post.author.username}</text>
  </view>
</region>
```

And the supplementary double-activation form, from the same component:

```uhura
<region label="Like this post" supplementary
        on:activate-double={emit like-toggled(post: post.id, now-liked: true)}>
  <img class="media" src={m.image.src} alt={m.image.alt} />
</region>
```

| Contract | Current behavior |
|---|---|
| Class | `interactive` |
| Children | Exactly one element (`UH5006 markup/bad-children` otherwise) |
| `label` | Required `text`; the accessible name |
| `supplementary` | Optional `bool` presence marker; declares the region a redundant gesture path |
| `class` | Universal, CSS-owned class list |
| `activate` | Input event with no carried payload |
| `activate-double` | Input event with no carried payload |
| Nesting | Interactive class: a region cannot nest inside another interactive element, and nothing interactive may appear beneath it (`UH5007 markup/nested-interactive`) |

The nested-interactive rule also sees through components: expanding a
component whose markup contains an interactive element inside a region is
rejected using the checker's interactive memo. The golden rejection
`reject-nested-interactive` pins the diagnostic.

The supplementary reachability check is name-level: for every emit bound on a
supplementary region, some non-supplementary use in the same definition must
emit the same machine event. The Instagram corpus satisfies it because
`like-toggled` is also emitted by the Like `<button>` in the action row.

## Ownership

Core owns nothing region-specific: a region contributes an ordinary semantic
node whose bound descriptors flow through the standard event pipeline. The
renderer owns gesture recognition (click, double-click, key presses) and
focus presentation. Whether a double activation should have platform-specific
timing or touch semantics is entirely renderer policy.

## Accessibility and validation

The checker enforces, with golden or corpus coverage:

- catalog closure — unknown properties and events on `<region>` are rejected;
- `label` is required (`UH5004`);
- the children model is exactly one element (`UH5006`);
- no interactive nesting in either direction (`UH5007`, pinned by
  `reject-nested-interactive`); and
- supplementary reachability (`UH5020`) — though no golden rejection
  currently pins this diagnostic; it is enforced in code with corpus
  evidence only.

The browser realization maps `label` to `aria-label`, sets `role="button"`,
and in Play sets `tabindex="0"`; Editor previews omit the tabindex, so a
board region is never focusable. Play wires three gestures: click emits the
`activate` descriptor, double-click emits `activate-double` (suppressing the
platform default), and — for regions specifically — Enter and Space emit the
first bound descriptor in `activate`, `press`, `activate-double` order.

Known accessibility gaps, stated honestly:

- **Everything is a button.** `role="button"` is applied to every region,
  including supplementary ones and pure navigation targets. An author-row
  region that navigates announces as a button, not a link; ARIA link
  semantics do not exist in the contract.
- **`aria-label` flattens content.** A region names itself with `aria-label`
  over composite children; assistive technology reads the label and may not
  expose the wrapped username, caption, or image content as the region's
  name computation.
- **Supplementary regions are still focusable.** A `supplementary` region
  receives `tabindex="0"` like any other, adding a tab stop that duplicates
  the focusable element the checker required elsewhere — the reverse of what
  the marker suggests.
- **Keyboard double activation is a collapse, not an equivalent.** Enter on
  a region whose only binding is `activate-double` fires that descriptor on
  a single key press. That satisfies reachability but blurs the gesture
  semantics.

## Rendering and platform behavior

Browser Editor and Play realize the element as a `div` with class
`uh-region` (plus authored classes):

```html
<div class="uh-region authored-classes" role="button" tabindex="0" aria-label="…">
  <!-- the single semantic child -->
</div>
```

Editor previews are inert: no tabindex, and the read-only renderer facade
never wires gesture listeners, so a board region cannot emit. Play attaches
the click, double-click, and region-specific keydown listeners once per
element and dispatches whatever descriptors the current node carries.

A native renderer may realize the contract with its platform gesture system
(for example a tap and double-tap recognizer over a labelled accessibility
element). It must reproduce the observable semantics — accessible name,
focusability, keyboard activation, and single/double activation dispatch —
or reject the capability honestly.

## Motion

`<region>` defines no semantic motion. Focus rings, press feedback, and
ripple-style effects are renderer and platform presentation.

## Conformance

Existing executable coverage proves:

- the base catalog exposes `region` as an interactive element with one-child
  children, required `label`, `supplementary`, `activate`, and
  `activate-double` (contract corpus);
- `on:` bindings on layout elements are rejected with the wrap-in-region
  note (`reject-event-on-layout` golden);
- interactive nesting is rejected (`reject-nested-interactive` golden); and
- the complete Instagram corpus checks and lowers with region semantic nodes
  in the post card, reel card, connection row, stories tray, and the feed,
  search, profile, and story pages.

A durable support claim additionally requires conformance coverage for:

- a golden rejection pinning `UH5020 markup/supplementary-unreachable`;
- keyboard activation order (Enter/Space descriptor selection) as a pinned
  renderer policy test;
- double-click default suppression and its interaction with text selection;
- accessibility-tree assertions for the label-over-content name computation;
  and
- equivalent semantics or an honest unsupported-capability diagnostic in
  non-browser renderers.

## Decisions and open questions

This page is part of the v0 element documentation effort signalled in
[gridaco/uhura#22](https://github.com/gridaco/uhura/issues/22). The current
implementation and
[Instagram spike design](../../../../studies/instagram-spike-design.md)
establish useful evidence but do not replace an accepted widget RFC. The
`§10` and `§4.8` cited by the checker diagnostics are spike-design sections,
not a versioned language specification.

Known gaps and open questions:

1. Whether regions need a role taxonomy (button, link, none) instead of the
   unconditional `role="button"`, and how a navigation region should
   announce.
2. Whether `supplementary` should remove the region from the focus order,
   given that its purpose is to declare the redundant path.
3. Whether keyboard activation of `activate-double`-only regions should
   exist, be remapped, or be rejected statically.
4. The children model is exactly one element; whether a region should ever
   wrap conditional (`{#if}` / `{#match}`) content directly is undecided —
   today the author must wrap those in a `<view>` first.
5. Long-press, context-menu, and other activation gestures are not part of
   the contract; whether they become new events or renderer policy is open.
6. The checker validates that `label` is bound, not that its text is
   non-empty or useful.

Current implementation references:

- [Base catalog declaration](../../../../../examples/instagram/client/catalog/base.toml)
- [Catalog and markup checking](../../../../../crates/uhura-check/src/markup.rs)
- [Browser property mapping](../../../../../web/src/renderer/appliers.ts)
- [Gesture and keyboard wiring](../../../../../web/src/renderer/reconciler.ts)
- [Contract corpus tests](../../../../../crates/uhura-check/tests/contracts_corpus.rs)
- [Golden rejections](../../../../../crates/uhura-tests/goldens/m2/)
- [Current Instagram post-card usage](../../../../../examples/instagram/client/components/post-card.uhura)
- [Specification router](../../../../spec/README.md)
