# `<button>`

- **Status:** Implemented generic action element; control taxonomy and state semantics unsettled
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Current spike design; no accepted widget RFC
- **Specification:** Pre-specification
- **Implementation:** Checker, semantic view, browser Editor, and Play implemented
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<button>` declares one user-invoked action. It is a system-defined catalog
element, not a user-authored component, an implicit form submit/reset control,
or a visual variant such as primary, destructive, or icon-only.

The narrow primitive is implemented and useful. Its current `pressed`,
`current`, child-content, and pending-state contracts are not mature enough to
standardize the broader control taxonomy. This entry records both the working
contract and the problems that a future widget RFC must resolve.

## Why Uhura needs a first-class button

HTML already has the correct browser primitive. Uhura still needs to identify
an action control before a browser renderer exists. A semantic `<button>` gives
the checker and every renderer one stable owner for:

- a required accessible-name input;
- input-event eligibility;
- focus and platform activation behavior;
- unavailable, pending, toggle, and current-state presentation;
- rejection of nested interactive content;
- deterministic static Editor representation; and
- renderer-neutral `press` delivery to a checked machine event.

Making a `<view>` clickable through CSS or an undeclared handler would lose
those guarantees. The catalog deliberately forbids input events on layout
elements. `<region>` exists for making a larger content region activatable;
`<button>` is the compact action-control primitive.

The browser uses a native [HTML `button`](https://html.spec.whatwg.org/multipage/form-elements.html#the-button-element),
but the language contract is not “all HTML button features.” Uhura currently
has no implicit form submission, reset, value, URL, popover command, or host
callback behavior.

## The language-design boundary

A generic command button is a sound primitive. A catch-all interactive control
is not.

The following concepts may use button-shaped presentation on a platform but
carry additional semantics and composition rules:

- a toggle, checkbox, or switch;
- a navigation link or current-location item;
- a tab and its associated panel;
- a menu or disclosure trigger;
- a form submit or reset control;
- a split button or long-press action; and
- a destructive action with confirmation policy.

The current element must not silently standardize those concepts merely because
the spike can express some of their visual states with booleans. Future
capabilities may refine `<button>`, compose it as a pattern, or require separate
elements. Those choices remain open.

## Current semantic contract

```uhura
<button class="primary-button"
        label="Share post"
        disabled={caption-invalid}
        busy={publish-pending}
        on:press={emit publish-tapped()}>
  <text>Share post</text>
</button>
```

An icon-only action uses the same explicit accessible label:

```uhura
<button label="Open profile" on:press={emit profile-tapped()}>
  <icon name="user-round" />
</button>
```

These examples show accepted current syntax. They do not settle the final
child-composition or pending-state design.

| Contract | Current behavior |
|---|---|
| Class | `interactive` |
| Children | Zero or more direct catalog elements whose class is `content` |
| `label` | Required `text`; accessible-name input |
| `disabled` | Optional `bool`; browser maps it to native disabled state |
| `busy` | Optional `bool`; marks pending presentation but does not disable |
| `pressed` | Optional `bool`; presence marks a two-state toggle in the browser |
| `current` | Optional `bool`; true marks this item current in the browser |
| `class` | Universal, opaque, and CSS-owned |
| `press` | Optional input event with no renderer-carried payload fields |
| Form behavior | None; browser realization is always `type="button"` |

The current base catalog's content-class elements are `<text>`, `<img>`,
`<video>`, and `<icon>`. The checker accepts only direct catalog-element
children in this position. Raw text, control-flow blocks, and component calls
are rejected even when they would expand to content-only output. Empty child
content is legal.

`press` carries no platform data. An authored binding may still prebuild a
typed descriptor payload:

```uhura
<button label="Follow Mira"
        on:press={emit follow-toggled(target: user.id, now-following: true)} />
```

The handler is optional. Without `on:press`, Play renders a focusable native
button but emits no Uhura event when it is activated. The checker does not
currently warn about an enabled handlerless button.

### State meanings

The four boolean properties are authored semantic state, not hidden widget
state:

- `disabled=true` means ordinary user activation is unavailable. The browser
  uses its native disabled property.
- `busy=true` means work associated with the control is pending. It currently
  adds `aria-busy="true"` and a visual opacity rule. It deliberately does not
  disable the control, remove its descriptor, show a spinner, or suppress a
  repeated press.
- Binding `pressed`, including `pressed={false}`, identifies a toggle button.
  Omission identifies an ordinary command button. The renderer never flips the
  value automatically.
- `current=true` marks the button as the current item. `false` and omission
  both remove the browser `aria-current` attribute. The renderer never manages
  group exclusivity.

Semantic `pressed` is not the transient visual state while a pointer is held
down. Hover, pointer-down feedback, ripples, and pressed animation are
renderer-owned presentation.

## Ownership and activation

Uhura state, expressions, or external projections determine `label`,
`disabled`, `busy`, `pressed`, and `current`. Core evaluates those values into
the semantic view and carries a checked `press` descriptor when one is bound.
Core has no button-specific state machine and does not toggle or clear any of
the properties.

In Play, the browser renderer:

1. realizes a native `<button type="button">`;
2. applies the current semantic properties and content;
3. relies on native pointer, Enter, Space, focus, and disabled behavior; and
4. translates native `click` activation into the current `press` descriptor.

For renderer wiring, descriptor presence is the subscription. The renderer
does not infer an action from the label, icon, class, or location in the tree.
Core also accepts a self-contained descriptor without rechecking whether the
current semantic view still exposes that descriptor. A stale descriptor may
therefore still dispatch, so machine guards remain necessary.

`disabled` is an interaction boundary, not an authorization or transactional
boundary. It remains a property in the semantic view, and Core does not reject
a descriptor because its originating button was disabled. Pending-sensitive
handlers still need machine guards against duplicate, stale, or otherwise
ineligible work.

The Editor creates the same static element and state attributes inside an
`inert` preview host. It installs no event channel. Static examples can therefore
show enabled, disabled, busy, pressed, and current states without causing Core
transitions.

## Accessibility and validation

Outside the known list-composition defect below, the browser mapping follows
the native button pattern for role, focus, and keyboard activation. The
explicit `label` is assigned to `aria-label`; when non-empty, it overrides a
name that might otherwise be computed from child content.

That explicit name is essential for icon-only buttons, but it creates an author
obligation. When a button contains visible text, its accessible name should
contain that visible wording. The W3C's
[Label in Name guidance](https://www.w3.org/WAI/WCAG22/Understanding/label-in-name)
explains why this matters for speech input. The current checker verifies only
that `label` is present and has type `text`; it does not reject an empty label,
guarantee that an empty value supplies an accessible name, or compare the label
with visible child text.

The checker currently guarantees:

- only declared properties and the universal `class` attribute are accepted;
- `label` is present and type-correct;
- optional state properties are boolean;
- only the declared `press` event may be bound;
- element events use an explicit `emit` binding with a checked target payload;
- direct children have catalog class `content`; and
- an interactive element cannot occur inside another interactive element,
  including through component expansion.

### Toggle naming

The [WAI-ARIA Button Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/button/)
describes `aria-pressed` as toggle state and recommends that a toggle retain a
stable label. For example, “Mute” remains “Mute” while pressed state says
whether it is active. An alternative command design may change “Like” to
“Unlike” without exposing toggle state.

The current Instagram example combines both models: every `pressed` button
also changes labels such as Like/Unlike, Save/Remove, and Follow/Unfollow. The
checker accepts this and the browser produces combinations such as “Unlike,
pressed.” That implementation evidence must not be presented as an accepted
accessibility default. The `pressed` contract and example authoring need a
separate correction after the design is decided.

### Current is not a tabs contract

`current=true` currently maps only to `aria-current="true"`. It does not add a
`tab` role, `aria-selected`, panel relationships, roving focus, or arrow-key
navigation.

The Instagram profile places current-marked buttons inside a `tablist`, but
that does not implement the complete
[WAI-ARIA Tabs Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/).
This is useful spike evidence for a future tab capability, not proof that the
generic button's `current` boolean is sufficient.

## Rendering and platform behavior

The browser renderer gives every instance class `uh-button` plus the authored
class string. Its baseline CSS resets native appearance, uses inline-flex
layout, preserves a visible Play focus outline, and applies opacity to disabled
and busy states. Instagram compositions—not the element contract—add local
target-size minima, including 44 pixels, and primary/secondary styles.

There is no semantic `kind`, `variant`, `size`, `color`, `icon-position`,
`destructive`, or loading-indicator property. Those are currently CSS or
composition concerns. Adding a class changes presentation, not action
semantics.

A non-browser renderer should use its native action-control primitive or an
equivalent accessible realization. It must preserve the semantic name, state,
activation, and lack of implicit form behavior. A static renderer may present
the authored state without an effect channel, as the Editor does.

Stable keyed Play reconciliation reuses the platform element across view
updates and preserves focus across ordinary moves. Changing `pressed`,
`current`, `busy`, or the label must update the existing control rather than
creating hidden widget state.

### Current child-model defect

The language's generic `children="content"` category is broader than a portable
button content model. HTML permits phrasing content with no interactive or
explicitly tabbable descendants. Uhura currently admits every content-class
element, including video. Native `<img>` is valid phrasing content and corrects
the former `div` realization mismatch, but the browser still maps `<text>` to
`p`, and `<video controls>` is an interactive media control. Those remaining
realizations can produce non-conforming or nested-interactive DOM inside a
native button even though the Uhura checker accepts the source.

The current demo uses only text and icon button content, but even text uses a
context-insensitive `p` mapping. A durable design must refine the semantic child
model, introduce safe slots, make browser realization context-aware, or combine
those approaches. The current broad rule is implementation evidence, not a
portable guarantee.

### Interactive list-item composition

When a direct child of `<view role="list">` is a button, the browser renderer
currently overwrites the native button role with `role="listitem"`. One DOM
role cannot represent both the list item and its nested action. The Instagram
demo avoids this by using structural wrappers, but the catalog does not enforce
that composition. This is a known renderer accessibility defect, not intended
button semantics.

## Motion

`<button>` defines no semantic motion. Native or CSS hover, focus, press, and
state transitions are presentation. The current browser baseline has no
built-in ripple, pressed animation, or busy spinner.

Any future built-in feedback must define interruption, cancellation, focus
visibility, and reduced-motion behavior without turning a visual completion
into the semantic `press` event or command outcome.

## Conformance

Existing executable coverage currently proves:

- shared Editor/Play structure and explicit label application;
- Editor inertness and Play click emission;
- nested-interactive rejection, including component expansion; and
- one optimistic pressed-state flow.

A durable support claim additionally requires conformance coverage for:

- required, correctly typed, and non-empty accessible labels;
- matching accessible and visible text where visible text exists;
- accepted content and rejection of interactive or platform-invalid children;
- nested-interactive rejection through conditional branches;
- absent and present `press` descriptors;
- exactly one press for ordinary pointer, Enter, Space, and assistive
  activation;
- no ordinary user press while disabled;
- the current rule that `busy` alone does not disable native activation,
  suppress renderer emission, or remove a descriptor;
- the distinction between an ordinary button (`pressed` absent), a toggle that
  is off (`false`), and a toggle that is on (`true`);
- `current` false and true without pretending to implement tabs;
- no automatic mutation of busy, pressed, or current state;
- browser `type="button"` and no implicit form submission;
- Editor state representation;
- focus preservation across keyed view updates;
- native button-role preservation when composed with list semantics; and
- equivalent semantics or an honest capability diagnostic in non-browser
  renderers.

Current tests do not yet prove real-browser keyboard activation, disabled
suppression, the state-to-ARIA mappings, label/content agreement,
platform-valid child content, or list composition.

## Decisions and open questions

1. Whether `<button>` remains only a command button or also has typed
   refinements for toggle, current-location, tab, menu, and disclosure roles.
2. Whether `pressed` stays, how it interacts with changing labels, whether it
   admits a mixed state, and whether `pressed` and `current` may coexist.
3. Whether `current` is removed, becomes a typed page/step/location value, or
   moves to navigation-specific capabilities.
4. Whether `label` always remains explicit or may be inferred from safe visible
   text, and how descriptions, localization, and label drift are checked.
5. Whether a handlerless enabled button remains legal or `on:press` becomes
   required outside static examples.
6. Whether busy state needs a default indicator or announcement, and where
   repeat suppression belongs without conflating busy and disabled.
7. How button content, slots, conditional content, and component-produced
   content become composable while remaining valid on every renderer.
8. Whether disabled controls remain outside focus navigation on every platform
   or need a discoverable-but-unavailable variant.
9. Whether form submission, validation, menu/disclosure relationships, links,
   and navigation require separate elements or explicit refinements.
10. Which visual defaults belong to the renderer or theme: focus treatment,
    touch-target minimum, variants, destructive emphasis, loading indication,
    and press feedback.
11. What one `press` means across down/up/cancel, keyboard repeat, double-click,
    long press, assistive activation, and focus-changing effects.

No current usage or browser mapping settles these questions. An accepted
button/widget RFC must preserve the narrow action semantics while separating
specialized control patterns from visual similarity.

Current implementation and research references:

- [Base catalog declaration](../../../examples/instagram/client/catalog/base.toml)
- [Catalog markup checking](../../../crates/uhura-check/src/markup.rs)
- [Shared browser property mapping](../../../web/src/renderer/appliers.ts)
- [Shared browser activation and reconciliation](../../../web/src/renderer/reconciler.ts)
- [Instagram button usage](../../../examples/instagram/client/components/post-card.uhura)
- [Instagram spike element catalog](../../working-group/instagram-spike-design.md)
- [Instagram dogfood gaps](../../working-group/instagram-demo-dogfood.md)
