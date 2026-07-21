# Uhura 0.4 checked UI catalogue

- **Status:** Executable incubation catalogue
- **Version scope:** Uhura 0.4 candidate only
- **Lifetime:** Disposable with the 0.4 candidate
- **Authority:** Exact checked element contract within the active candidate
- **Compatibility:** None; this is not a supported or stable widget API
- **Application profile:** [Uhura 0.4 `ui`](../../../spec/drafts/0.4/application.md)
- **Executable owner:** [`uhura-check` 0.4 catalogue](../../../../crates/uhura-check/src/ui_catalog/elements.rs)
- **Browser parity contract:** [`uhura-ui-catalog/0`](../../../../resources/ui-catalog/0.4.json)
- **Taxonomy:** [Stable widget taxonomy](../../README.md)

This page records the finite element vocabulary admitted by the Uhura 0.4
checker. It is authoritative for the disposable 0.4 candidate, not for another
version and not as a permanent classification of UI capabilities. The
executable catalogue and its conformance tests must change in the same patch
as this reference.

The `ui` profile is activated lexically with `use uhura::ui;`. Lowercase
elements below are native to that profile. `Link` and `Surface` are checked
standard-library elements and become available only through their explicit
imports:

```uhura
use uhura::ui;
use uhura::web_router::Link;
use uhura::ui_surface::Surface;
```

`class: Text` is admitted on every element and is omitted from the table.
“Children” means checked nested UI nodes are admitted; “void” means they are
rejected.

## Attribute kinds

| Kind | Checked source contract |
|---|---|
| `Text` | Quoted text or a checked expression of type `Text` |
| `Bool` | Checked expression of type `Bool` |
| `ExactNumeric` | Checked expression of type `Int`, `Nat`, `PositiveInt`, `Decimal`, or `Ratio` |
| `Ratio` | Checked expression of type `Ratio`, the normalized inclusive `0..1` value type |
| `Token(a \| b)` | One quoted token from the listed closed set; expressions are rejected |
| `CheckedExpression` | Framework-owned expression checked by that feature's projection contract |
| `Key` | Checked scalar, nominal scalar-key, or fieldless-sum value |

For `<input>`, `value` is `Text` by default. When the same element has the
literal attribute `type="number"`, `value` is `ExactNumeric`.

## Elements and attributes

| Elements | Profile status | Content | Attributes | Required and additional constraints |
|---|---|---|---|---|
| `<main>`, `<section>`, `<header>`, `<h1>`, `<h2>`, `<p>`, `<output>`, `<label>`, `<legend>`, `<dl>`, `<dt>`, `<dd>` | Native | Children | `aria-label: Text` | None |
| `<fieldset>` | Native | Children | `aria-label: Text`, `disabled: Bool` | None |
| `<progress>` | Native | Children | `aria-label: Text`, `value: ExactNumeric`, `max: ExactNumeric` | None |
| `<button>` | Native | Children | `aria-label: Text`, `label: Text`, `aria-pressed: Bool`, `disabled: Bool`, `busy: Bool`, `pressed: Bool`, `current: Bool` | Syntactically text-bearing content, `label`, or `aria-label`; `on press`; no nested interactive element |
| `<input>` | Native | Void | `aria-label: Text`, `type: Text`, `disabled: Bool`, `min: ExactNumeric`, `max: ExactNumeric`, `value: Text` or `ExactNumeric` | None |
| `<view>` | Native | Children | `role: Token(none \| list \| navigation)` | A literal `role="list"` admits only unroled `<view>` item boundaries as rendered direct children |
| `<scroll>` | Native | Children | `direction: Token(vertical \| horizontal)`, `position: Ratio` | None |
| `<pager>` | Native | Children | `indicator: Token(none \| dots)`, `label: Text` | `label` |
| `<text>` | Native | Children | None | None |
| `<img>` | Native | Void | `src: Text`, `alt: Text`, `decorative: Bool` | `src`; exactly one of `alt` or `decorative` |
| `<video>` | Native | Void | `src: Text`, `poster: Text`, `label: Text`, `autoplay: Bool`, `muted: Bool`, `loop: Bool`, `controls: Bool`, `playsinline: Bool` | `src`, `label` |
| `<icon>` | Native | Void | `name: Text`, `family: Text` | `name`; family is a literal configured registry name, and `name` must resolve to a finite set of registered glyphs before rendering |
| `<textfield>` | Native | Void | `value: Text`, `placeholder: Text`, `label: Text`, `disabled: Bool` | `label`; supplying `value` also requires `on change` |
| `<region>` | Native | Children | `label: Text`, `supplementary: Bool` | `label`; one of `on activate` or `on activate-double`; no nested interactive element |
| `<Link>` | Explicit `use uhura::web_router::Link;` | Children | `routes: CheckedExpression`, `to: CheckedExpression`, `disabled: Bool` | `routes`, `to`; syntactically text-bearing content; no nested interactive element |
| `<Surface>` | Explicit `use uhura::ui_surface::Surface;` | Children | `key: Key` | `key` |

For accessible-name checking, text-bearing content means a non-whitespace text
node or interpolation reachable through the checked child tree. The check is
structural: it does not claim that every runtime branch produces a non-empty
name. The executable catalogue's `interactive` classification currently
includes `<button>`, `<input>`, `<scroll>`, `<pager>`, `<video>`,
`<textfield>`, `<region>`, and `<Link>`. Nested-interactive constraints query
that classification rather than carrying a second list of element names.

For `role="list"`, whitespace is ignored and every possible rendered direct
child through `if`, `each`, or `match` must be an unroled `<view>`. That
neutral boundary receives `role="listitem"` in the browser; semantic and
interactive content remains nested inside it rather than having its role
overwritten. Direct interpolation or a semantic direct child is rejected.

## Browser realization

The checker classifies each admitted element by realization boundary:

| Boundary | Elements |
|---|---|
| Native HTML | `<main>`, `<section>`, `<header>`, `<h1>`, `<h2>`, `<p>`, `<output>`, `<progress>`, `<label>`, `<fieldset>`, `<legend>`, `<input>`, `<dl>`, `<dt>`, `<dd>` |
| Uhura browser primitive adapter | `<button>`, `<view>`, `<scroll>`, `<pager>`, `<text>`, `<img>`, `<video>`, `<icon>`, `<textfield>`, `<region>` |
| Core-lowered standard extension | `<Link>`, `<Surface>` |

The small versioned browser parity contract lists only the adapter IDs crossing
the Rust/TypeScript boundary. Rust and browser tests verify both sides against
that file. It is a generated-or-verified implementation contract, not a second
semantic catalogue; attributes, constraints, and events remain owned by the
checker catalogue above.

Play realizes `<Surface>` as a keyed dialog in a frame-owned surface layer,
never in the browser's document-wide modal top layer. The page and lower
surfaces are inert, focus enters the top surface, keyboard focus remains
contained within the application layers, and focus returns to the prior
application owner when that surface disappears. Host-owned Play chrome remains
operable and retains focus across surface updates or closure. Because content
outside the application frame is deliberately available, the contained dialog
does not claim document-wide `aria-modal` semantics.
Static Editor previews use the same contained visual stack but remain wholly
inert. The 0.4 `<Surface>` contract declares no dismissal event, so Escape and
the scrim cannot invent a machine input; authors provide an explicit checked
control when dismissal is part of the machine.

The `tablist` token is deliberately not admitted on `<view>`. A parent
`role="tablist"` without checked tab children, selection, focus movement, and
keyboard behavior is not an accessible tabs contract. A future tabs
capability must add that complete contract rather than reintroduce a
container-only role token.

## Events and payloads

An event binding has the form `on event -> MachineInput(...)`. `event` in the
right-hand expression is an immutable payload supplied by the checked element
contract.

| Element | Event | Payload | Condition |
|---|---|---|---|
| `<button>` | `press` | `Unit` | Always |
| `<input>` | `input` | `{ value: Text }` | Admitted unless the element has literal `type="number"` |
| `<input>` | `change` | `{ number: BoundaryNumber }` | Admitted only when the element has literal `type="number"` |
| `<scroll>` | `near-end` | `Unit` | Always |
| `<pager>` | `page-change` | `Unit` | Always |
| `<textfield>` | `change` | `{ text: Text }` | Always |
| `<textfield>` | `submit` | `Unit` | Always |
| `<region>` | `activate` | `Unit` | Always |
| `<region>` | `activate-double` | `Unit` | Always |
| `<Link>` | `follow` | `Unit` | Always |

All other element/event pairs are rejected. This table states source checking,
not browser realization details; every renderer must separately prove that it
implements each admitted contract or reject the required capability honestly.

## Resource-backed validation

`<icon>` is checked in two stages before any Editor or Play render is
published. The finite UI catalogue checks its shape. After project resources
are admitted, the checker then validates `family` against the exact configured
registry and every possible `name` against that family's glyph map.

`family` must be a quoted literal. `name` may be a quoted literal or a finite
expression composed from checked literals, constants, `if`, and `match`.
Unknown families, unknown glyphs, and names whose possible values cannot be
bounded are source errors. The browser receives only checked logical tokens
and has no fallback contract for repairing them.

## Deliberate limits

This catalogue does not establish a general widget system, component
invocation, raw HTML passthrough, arbitrary custom elements, or stable renderer
behavior. An imported `ui` presentation is not callable with element-shaped
syntax in 0.4. New elements and contracts require a checker change, matching
renderer work, conformance coverage, and an update to this page.
