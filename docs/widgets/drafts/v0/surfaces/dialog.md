# Dialog surface

- **Status:** Named but unrealized modality: the machinery is modality-generic, no dialog presentation exists, and no corpus usage exists
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Surface
- **Facets:** None
- **Availability:** Proposed; the surface machinery would accept `modality dialog` today, but nothing distinguishes it from a sheet
- **Decision:** Named by a checker steering note only; no accepted widget RFC
- **Specification:** Pre-specification; no dialog-specific semantics exist anywhere
- **Implementation:** Generic surface machinery implemented; dialog presentation unimplemented; zero corpus usage
- **Owners:** Syntax, Checker, Core, Renderer
- **Supported renderers:** None with dialog-specific behavior

A dialog is the second surface modality the v0 line names: a machine-managed
modal layer that should present as a centered interruption rather than a
bottom sheet. This page documents an honest peculiarity of the current
state: **dialog exists as a name, not as a capability.** Every claim below
is labelled accordingly.

The one place the codebase commits to dialogs at all is the checker's
steering note: writing `<dialog>` as markup is rejected as an unknown
element with the note *"dialogs are surfaces (core surface stack)"* —
matching the sibling note that steers `<sheet>` to
`surface <name> modality sheet`. The steering is implemented; the
destination is not.

## What exists today (implemented)

The surface machinery is modality-generic, so `modality dialog` would flow
through it end to end:

- **Parse.** The parser accepts any identifier after `modality` (its
  error hint names only `sheet`), so `surface confirm-delete modality
  dialog` parses.
- **Lower and IR.** The modality string lowers verbatim for surface
  definitions and rides the IR; nothing inspects the value.
- **Core.** Evaluation copies it into the surface view. Opening, stacking,
  occlusion, structural dismiss via the reserved `dismiss` event, the
  orphan sweep, and focus-restore intents are all shared with sheets and
  are documented on the [sheet page](sheet.md) — none of them branch on
  modality. The step machinery's acceptance comment mentions "top-surface
  modality", but the implemented rule is modality-independent: any stacked
  surface occludes the page and only the top surface is interactive.
- **Play.** The panel would be stamped `uh-modality-dialog` and, as for
  every surface, `role="dialog"` with `aria-modal="true"`, scrim and
  Escape dismissal, inert layers below, and first-focusable entry.

## What does not exist (the gap)

- **No presentation.** No `.uh-modality-dialog` rule exists in any
  stylesheet. The base `.uh-surface` treatment is a bottom sheet — 72%
  block-size, top-rounded corners, bottom-justified overlay — so a
  `modality dialog` surface would render as a sheet with a different class
  name. A centered, width-constrained dialog pose exists nowhere.
- **No validation.** No checker rule constrains the modality value; the
  legal set is undefined. `dialog` is exactly as (un)checked as `shete`.
- **No semantics.** Nothing dialog-specific is specified: no
  alert/confirmation taxonomy, no required-choice (non-dismissable) mode,
  no default-action or destructive-action concept, no dialog title.
- **No usage.** The corpus contains exactly one surface, `comments-sheet
  modality sheet`; `modality dialog` appears nowhere in any example, test,
  or golden. There is no real usage example to cite, and this page
  deliberately does not invent one as if it were supported.

## Why Uhura needs dialogs as surfaces (proposed)

The argument is the sheet argument with a different pose. A confirmation
dialog hand-built from positioned divs re-implements scrim, Escape, focus
capture, and restoration — the ARIA APG
[modal dialog pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)
checklist — per occurrence. Uhura's surface stack already owns that
lifecycle structurally: machine-opened instances, published dismiss
descriptors, occlusion in event acceptance, and focus-restore intents.
Making dialogs a modality of the same stack, rather than a new primitive,
means:

- one lifecycle, one trace shape, and one replay story for every modal
  layer;
- the modality axis carries only what actually differs — presentation
  geometry and, plausibly, dismissal policy (dialogs conventionally resist
  scrim-dismissal for destructive confirmations); and
- the checker's element-steering keeps authors from rebuilding modality
  out of `<view>`s, exactly as it does for sheets.

The native web is converging on the same split: HTML's
[`dialog` element](https://html.spec.whatwg.org/multipage/interactive-elements.html#the-dialog-element)
distinguishes modal presentation from content, and Uhura's machine-owned
variant adds the statically checkable lifecycle HTML leaves to script.

## Proposed contract sketch (not implemented)

A minimal dialog declaration would be spelled exactly like a sheet:

```uhura
surface confirm-delete modality dialog

props {
  post: id
}
```

opened with the same machine statement (`open-surface
confirm-delete(post: post)`) and dismissed through the same reserved
`dismiss`. Everything on the sheet page's contract table would apply
unchanged except presentation. This sketch is authorable today and would
run — but it would look like a sheet, which is why this page labels the
capability unrealized rather than partially supported.

## Accessibility and validation

Nothing dialog-specific is implemented. The generic surface behavior
(`role="dialog"`, `aria-modal`, inert lower layers, focus entry and
restoration) and its gaps — no accessible name for the panel, no
non-dismissable mode — are documented on the [sheet page](sheet.md) and
apply verbatim. A real dialog capability would additionally need:

- a title/accessible-name mechanism (dialogs are conventionally named by
  a visible heading);
- a decided dismissal policy (Escape yes, scrim configurable, or
  required-choice); and
- initial-focus rules for confirmation patterns (focus the safe action,
  not the first focusable, per APG guidance).

## Rendering and platform behavior

Unimplemented. The required renderer work is a `.uh-modality-dialog`
presentation (centered overlay, constrained inline-size, full scrim) in
Play and Editor, and a native mapping to platform dialog/alert primitives.
Until a renderer distinguishes the modality, claiming dialog support would
be false; renderers currently realize every surface as a sheet.

## Motion

Not part of any contract; sheets define none either. Dialog conventions
(fade/scale-in) would need the same motion-contract decision the sheet
page defers.

## Conformance

Nothing dialog-specific is provable today, and no conformance case exists.
Before support can be claimed, at minimum:

- the legal modality set is decided and checker-validated;
- a dialog presentation exists in the browser renderers, pinned by policy
  tests;
- occlusion, dismissal, and focus behavior are adjudicated per modality
  (identical to sheets, or deliberately different) and pinned; and
- at least one corpus surface uses `modality dialog` end to end.

## Decisions and open questions

This page is part of the v0 documentation effort signalled in
[gridaco/uhura#22](https://github.com/gridaco/uhura/issues/22). It exists
to keep the modality axis honest: the name is reserved by a steering note,
the machinery is ready, and the capability is not designed.

Open questions:

1. Is `dialog` actually the second modality v0 wants, or should the axis
   wait for a real product need? (The corpus has never needed one.)
2. Whether modality means presentation only, or also dismissal policy and
   occlusion behavior — the step machinery's comment gestures at
   modality-aware acceptance that does not exist.
3. Alert versus confirmation versus free-form dialog taxonomy, default and
   destructive actions, and required-choice semantics.
4. Dialog naming (visible title versus label prop) and initial focus.
5. Whether the unvalidated-modality hole is fixed by an enum in the
   parser, the checker, or a future surface catalog analogous to the
   element catalog.

Current implementation references (all modality-generic):

- [Checker steering note for `<dialog>`](../../../../../crates/uhura-check/src/markup.rs)
- [Surface declaration parsing](../../../../../crates/uhura-syntax/src/parser/mod.rs)
- [Modality lowering and default](../../../../../crates/uhura-check/src/lower.rs)
- [Surface view and dismiss descriptor](../../../../../crates/uhura-core/src/eval.rs)
- [Structural dismiss and occlusion](../../../../../crates/uhura-core/src/step.rs)
- [Play surface stack and modality class](../../../../../web/src/play/surfaces.ts)
- [Sheet-only surface styles](../../../../../web/src/play/shell.css)
- [Sheet surface page](sheet.md)
- [Specification router](../../../../spec/README.md)
