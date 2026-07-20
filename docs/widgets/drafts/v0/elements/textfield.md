# `<textfield>`

- **Status:** Implemented controlled text input; visible labelling and richer input semantics unsettled
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Element
- **Facets:** None
- **Availability:** Built-in base catalog; currently project-pinned during incubation
- **Decision:** Renamed from `<text-field>` in base catalog 0.2.0; no accepted widget RFC
- **Specification:** Pre-specification; controlled draft semantics implemented
- **Implementation:** Checker, Core semantic view, browser Editor, and Play implemented
- **Owners:** Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

`<textfield>` declares a single-line text input whose draft value is
application state. It is a catalog element, not a user-authored component, and
source cannot invent its properties or events. It takes no children.

The current base catalog is stored with the Instagram project while Uhura is
incubating. Calling the element built-in describes its authoring and checking
role; it does not yet mean that a globally packaged catalog exists.

The spelling is deliberate and unhyphenated. Base catalog 0.1.0 shipped
`<text-field>`; catalog 0.2.0 renamed it to `<textfield>` in the same change
that renamed `<image>` to `<img>`. The old spelling is not a compatibility
alias: the checker rejects `<text-field>` as an unknown element and attaches a
migration note naming the new spelling. A golden rejection pins that exact
diagnostic.

## Why Uhura needs an explicit textfield element

HTML's [`input`](https://html.spec.whatwg.org/multipage/input.html#the-input-element)
is uncontrolled by default: the DOM owns the draft, and application state only
learns about it through events the author chose to wire. Frameworks then layer
conventions on top. React's
[controlled input](https://react.dev/reference/react-dom/components/input#controlling-an-input-with-a-state-variable)
convention — bind `value`, handle `onChange` — is enforced only at runtime: a
`value` without a change handler produces a console warning and a field that
silently ignores typing. Nothing in the type system or build distinguishes a
controlled input from a forgotten handler.

Uhura promotes that convention into a checked catalog contract. The catalog
declares the element controlled — binding `value` obligates handling
`on:change` — and the checker rejects the violation statically as
`UH5008 markup/controlled-promotion`. The obligation is data in the catalog,
not a rule hard-coded for one element, so future controlled elements inherit
the same mechanism. This buys:

- **No controlled/uncontrolled ambiguity.** A `value` binding is a promise
  that the machine owns the draft; the compiler holds the author to it before
  anything renders.
- **Deterministic replay.** Every keystroke the application sees is a `change`
  event carrying `value: text` through the ordinary event pipeline. The draft
  lives in Uhura state, so traces and replays reproduce typing exactly; there
  is no shadow DOM-owned draft that a replay cannot observe.
- **Portable semantics.** A non-web renderer receives a label, placeholder,
  disabled flag, and two events — not a DOM idiom. Flutter's
  [`TextField`](https://api.flutter.dev/flutter/material/TextField-class.html)
  with an explicit controller reflects the same judgment that draft ownership
  must be explicit rather than ambient.

CSS keeps owning size, spacing, borders, and typography. `<textfield>` exists
because draft ownership, change delivery, and submit gestures are semantic
capabilities rather than aesthetics.

## Current semantic contract

Real usage from the Instagram comments composer:

```uhura
<textfield value={draft} label="Add a comment" placeholder="Add a comment…"
           disabled={comment-pending}
           on:change={emit composer-changed()}
           on:submit={emit submit-requested()} />
```

| Contract | Current behavior |
|---|---|
| Class | `interactive` |
| Children | None |
| `value` | Optional `text`; binding it promotes the field to controlled and obligates `on:change` |
| `placeholder` | Optional `text` |
| `label` | Required `text`; the accessible name |
| `disabled` | Optional `bool` |
| `class` | Universal, CSS-owned class list |
| `change` | Input event carrying `value: text`; fires per committed edit, one per IME composition |
| `submit` | Input event with no carried payload; fired by the Enter gesture when bound |
| Nesting | Interactive class forbids interactive descendants; moot here because children are closed |

The carried `value` reaches the machine through signature coverage rather than
authored arguments: `on:change={emit composer-changed()}` is legal when the
machine declares `on composer-changed(value: text)`, because authored emit
arguments and renderer-carried fields must jointly cover the handler
signature. The corpus uses exactly this shape in all four current occurrences
(search query, caption, alternative text, and comment composer).

Uhura's `change` is HTML's `input`, not HTML's blur-time `change`: it fires on
each committed edit. During IME composition the renderer buffers locally and
emits a single `change` at `compositionend`.

`submit` is a gesture, not a form model. Enter emits the bound descriptor and
suppresses the platform default; when `on:submit` is absent, Enter does
nothing semantic. An Enter that commits an IME conversion is never a submit
gesture; the browser Play policy checks both `isComposing` and the WebKit
keyCode 229 tell.

## Draft ownership

Core owns the draft; the renderer owns caret and IME. The bound `value`
expression is the single source of truth: the typical handler stores the
carried text (`set query = value`), and Core echoes it back through the
semantic view. A handler that discards the carried value produces a field that
visibly reverts — that is the contract working, not a renderer bug.

The browser Play policy keeps typing and asynchronous state changes from
corrupting each other. Per field, it counts in-flight `change` emissions that
have not yet been stepped; while the count is nonzero, an externally computed
value never applies directly — it is stashed, and the last stashed value wins
only once typing settles and Core did not echo the draft back meanwhile. A
tick-scheduled outcome landing mid-typing therefore cannot eat keystrokes.
Field state is keyed by the input element in a `WeakMap`, so teardown is
automatic and a remounted field starts fresh.

Editor previews are inert by construction: the read-only renderer facade is
built without a textfield controller, and the realized input is additionally
marked read-only. Nothing typed into a preview can exist, and no `change` or
`submit` can be emitted from a board.

## Accessibility and validation

The checker enforces, with golden or corpus coverage:

- catalog closure — unknown properties and events on `<textfield>` are
  rejected, and `on:` bindings are only legal where the catalog declares them;
- `label` is required (`UH5004 markup/missing-required-prop`);
- the children model is `none`;
- controlled promotion — `value` without `on:change` is
  `UH5008 markup/controlled-promotion`; and
- legacy `<text-field>` is rejected with a rename note, not aliased.

The browser realization maps `label` to `aria-label` on the inner native
input and `placeholder` to the native attribute. Known accessibility gaps,
stated honestly:

- There is no visible label. `aria-label` names the field for assistive
  technology only, while the placeholder disappears as soon as the user
  types. The WAI guidance treats visible
  [labels](https://www.w3.org/WAI/tutorials/forms/labels/) as the default and
  hidden naming as the exception; the current contract inverts that and has
  no `<label>`-style association, heading fallback, or described-by channel.
- There is no error, validity, or description semantic: no invalid state, no
  `aria-describedby` equivalent, no required-field marker.
- `disabled` maps to the native disabled state, which removes the field from
  the focus order; a read-only-but-focusable pose does not exist.
- The checker validates that `label` is bound, not that its text is
  non-empty or useful.

The inner input is mechanic DOM, marked `data-uh-mechanic="input"` — it is
created by the property applier, is not a semantic child, never appears as a
realization path segment, and reconciliation deliberately leaves it in place
when semantic children are swept. Renderer policy tests pin this.

## Rendering and platform behavior

Browser Editor and Play realize the element as a `div` with class
`uh-textfield` (plus authored classes) wrapping one native
`input[type=text]`:

```html
<div class="uh-textfield authored-classes">
  <input type="text" data-uh-mechanic="input" aria-label="…" placeholder="…">
</div>
```

Play wires the input to the textfield controller and applies Core's draft
through it; Editor sets the input read-only and applies the value directly.
The base stylesheet gives the input a minimal bordered treatment; everything
visual beyond that is authored CSS on the wrapper and input.

A native renderer may realize the contract with its platform text-input
primitive. It must reproduce the observable semantics — accessible name,
per-edit `change` with a single emission per composed sequence, the Enter
submit gesture, and external-value application that never destroys
uncommitted typing — or reject the capability honestly. Caret ownership,
selection, autocorrect, keyboard type, and IME presentation remain renderer
and platform concerns unless separately promoted into portable contracts.

## Motion

`<textfield>` defines no semantic motion, transition, or completion event.
Caret blinking, focus rings, and platform keyboard animations are renderer
and platform presentation.

## Conformance

Existing executable coverage proves:

- the base catalog exposes `textfield` as a childless interactive element
  with `value`, `placeholder`, required `label`, `disabled`, `change`
  carrying one `text` field, and `submit`;
- the catalog's controlled declaration is `value` → `change`, loaded and
  asserted by the contract corpus;
- binding `value` without `on:change` produces the pinned `UH5008` golden;
- `<text-field>` produces the pinned `UH5001` golden with the rename note;
- the complete Instagram corpus checks and lowers with `textfield` semantic
  nodes;
- Editor and Play realize the same `uh-textfield` structure, the inner input
  is mechanic DOM excluded from semantic realization, and the Editor
  realization stays inert.

A durable support claim additionally requires conformance coverage for:

- the in-flight counter and stash race: external values landing mid-typing,
  Core echoing versus replacing the draft, and settlement ordering;
- IME composition in a real browser, including the WebKit
  Enter-after-compositionend behavior the code special-cases;
- `submit` with and without a bound descriptor, and Enter during
  composition;
- `disabled` interaction and focus-order behavior;
- an unbound-`value` field's behavior, once its semantics are decided;
- accessibility-tree assertions rather than attribute assertions alone; and
- equivalent semantics or an honest unsupported-capability diagnostic in
  non-browser renderers.

The Play textfield mechanics currently have no dedicated unit test; the
in-flight, stash, and composition invariants are documented and exercised
only indirectly.

## Decisions and open questions

This page is part of the v0 element documentation effort signalled in
[gridaco/uhura#22](https://github.com/gridaco/uhura/issues/22). The current
implementation and
[Instagram spike design](../../../../studies/instagram-spike-design.md)
establish useful evidence but do not replace an accepted widget RFC or
versioned specification. The `§10` cited by the catalog and the checker
diagnostic is the spike design's semantic-element-catalog section, not a
section of a versioned language specification.

Known gaps and open questions:

1. The catalog does not require `value`, but the runtime has no uncontrolled
   draft semantics: an unbound field's typed text survives only until the
   next reconciliation applies the evaluated (empty) value. Whether `value`
   becomes required, or an uncontrolled mode becomes real, is undecided.
2. Whether the contract needs a visible-label mechanism instead of
   `aria-label`-only naming, and how that composes with authored layout.
3. Whether input purposes (password, email, number, search), autocomplete
   semantics, and keyboard hints become checked semantic properties or stay
   renderer policy.
4. Multiline input is not covered; a text area is a distinct children/sizing
   contract, not a `textfield` flag.
5. Validation, error presentation, described-by, and required-field semantics
   have no owner.
6. Whether `submit` should relate to a future form or field-group concept or
   remain a per-field Enter gesture.
7. Selection, caret position, and programmatic focus are renderer-owned
   today; a portable focus or selection intent is a separate capability.
8. The per-edit `change` cadence versus a settled/blur cadence is
   implementation-proven but not adjudicated for non-browser renderers.

Current implementation references:

- [Base catalog declaration](../../../../../examples/instagram/client/catalog/base.toml)
- [Catalog and markup checking](../../../../../crates/uhura-check/src/markup.rs)
- [Catalog controlled-promotion schema](../../../../../crates/uhura-check/src/catalog.rs)
- [Play textfield mechanics](../../../../../web/src/play/textfield.ts)
- [Browser property mapping](../../../../../web/src/renderer/appliers.ts)
- [Shared browser reconciliation](../../../../../web/src/renderer/reconciler.ts)
- [Read-only Editor renderer](../../../../../web/src/renderer/editor.ts)
- [Renderer policy tests](../../../../../web/src/renderer/tests/policies.test.ts)
- [Contract corpus tests](../../../../../crates/uhura-check/tests/contracts_corpus.rs)
- [Golden rejections](../../../../../crates/uhura-tests/goldens/m2/)
- [Current Instagram composer usage](../../../../../examples/instagram/client/surfaces/comments-sheet.uhura)
- [Specification router](../../../../spec/README.md)
