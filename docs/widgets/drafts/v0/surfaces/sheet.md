# Sheet surface

- **Status:** Implemented default surface modality; modality validation, motion, and drag dismissal unsettled
- **Version scope:** v0 incubation draft
- **Lifetime:** Disposable with the v0 widget draft
- **Document type:** Capability
- **Primary form:** Surface
- **Facets:** None
- **Availability:** Built-in surface machinery; `sheet` is the default modality
- **Decision:** Current spike design (§8.1); no accepted widget RFC
- **Specification:** Pre-specification; structural lifecycle implemented
- **Implementation:** Parser, checker, Core surface stack, browser Editor, and Play implemented
- **Owners:** Syntax, Checker, Core, Renderer
- **Supported renderers:** Browser Editor and Play

A sheet is a surface — a machine-managed presentation layer stacked above
the page — whose modality is `sheet`. Surfaces are not elements: the checker
steers markup away explicitly (writing `<sheet>` produces the unknown-element
note *"sheets are surfaces — `surface <name> modality sheet`"*, and mounting
a declared surface by name in markup adds *"surfaces mount via
`open-surface`, not markup"*). A surface is declared as its own definition
kind and entered only through the machine.

This is the first Surface entry in the v0 draft; the stable
[surfaces hub](../../../surfaces/README.md) records that no surface is
documented outside this disposable draft yet.

## Why Uhura needs machine-owned sheets

The web's bottom sheet is a positioned `div` plus hand-written scrim,
Escape handling, focus management, and open/close state scattered through
application code. Every one of those concerns is a correctness or
accessibility bug when forgotten, which is why the ARIA APG
[dialog pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)
is a checklist at all. Uhura moves the whole lifecycle into the machine:

- **Opening is a machine operation.** `open-surface <name>(args)` is a
  statement in a handler, staged and applied structurally — not a markup
  conditional. The surface instance gets a serial, its own state, a
  recorded opener scope, and a captured focus-restore key-path from the
  triggering node.
- **Dismissal is structural, not authored.** Every surface view carries a
  first-class dismiss descriptor whose event is the reserved machine event
  `dismiss`. Escape and the scrim emit that published descriptor; Core
  handles it structurally with no authored handler involved, pops the
  instance, force-closes surfaces it opened, and issues a focus-restore
  intent when the dismissed surface was topmost. A `dismiss` arriving from
  a non-surface scope is an evaluation error.
- **Occlusion is enforced in acceptance.** While any surface is stacked,
  UI events targeting the page are dropped as occluded, and only the top
  surface's scope is accepted — interaction with covered layers is
  impossible by construction, not by pointer-events CSS.
- **Focus is a machine intent.** The dismiss pops emit a `focus-restore`
  intent carrying the stored key-path; the renderer's focus controller
  performs it. Entering a mounted sheet focuses its first focusable
  element.

The result: an author writes one declaration line and one `open-surface`
statement, and scrim, Escape, stacking, inertness, and focus restoration
exist with replayable, trace-visible semantics.

## Current semantic contract

The corpus' only surface, the Instagram comments sheet:

```uhura
surface comments-sheet modality sheet

props {
  post: id
}
```

opened from the feed, post, and reels pages:

```uhura
on comments-requested(post: id) {
  open-surface comments-sheet(post: post)
}
```

| Contract | Current behavior |
|---|---|
| Declaration | `surface <name>` with optional `modality <ident>`; the parser hint names `sheet` |
| Default | Omitted modality lowers to `"sheet"`; the IR models modality as surface-only data |
| Instance identity | `definition:serial` key; serials minted per open |
| Props | Declared props bound by name from the `open-surface` argument list |
| State | Each instance gets fresh initial state; no state survives dismissal |
| Dismiss | Reserved structural `dismiss`; Escape and scrim emit the published descriptor |
| Occlusion | Any stacked surface occludes the page; only the top surface accepts UI events |
| Focus | First focusable focused on mount; focus-restore intent to the opener's key-path on topmost dismissal |
| Sweep | Surfaces opened by a dismissed or swept scope force-close with it |

The modality value flows verbatim from parse through lowering
(`DefKind::Surface` only — pages and components never carry one) into the
IR and out through evaluation into each `SurfaceView`, which also carries
the dismiss descriptor and the focus-restore key-path.

## Ownership

Core owns the surface stack: instances, serials, props, per-instance state,
opener bookkeeping, structural dismiss, occlusion acceptance, the orphan
sweep, and focus-restore intents. The renderer owns presentation: scrim,
panel, stacking order, inertness, Escape listening, and actually moving
focus. The renderer never invents a dismiss event — it presses the
descriptor Core published, IME-guarded so an Escape that cancels a
composition is not a dismiss gesture.

## Accessibility and validation

Implemented behavior:

- The Play panel is `role="dialog"` with `aria-modal="true"` and
  `tabindex="-1"`, receives focus (or its first focusable child) on mount.
- The page host and every below-top overlay are `inert` while surfaces are
  stacked, so focus and assistive-technology traversal cannot reach
  occluded content — containment by inertness rather than a roving trap.
- Escape and scrim dismissal are wired unconditionally.
- Focus restoration targets the recorded key-path, falling back to a
  focusable descendant of the recorded node.

Known gaps, stated honestly:

- **No accessible name for the sheet itself.** Nothing labels the dialog;
  no title property or labelled-by association exists in the declaration.
- **`role="dialog"` regardless of modality.** A sheet announces as a
  dialog; no sheet-specific role or `aria-roledescription` exists.
- **The modality value is unvalidated.** The parser accepts any identifier
  after `modality`; no checker rule constrains it to a known set. A typo
  (`modality shete`) lowers, runs, and silently renders with the default
  sheet presentation.
- There is no non-dismissable sheet: Escape and scrim always dismiss.

## Rendering and platform behavior

The Play shell mounts each stacked surface as an overlay in host order:

```html
<div class="uh-surface-overlay" data-surface-key="comments-sheet:2">
  <div class="uh-scrim"></div>
  <div class="uh-surface uh-modality-sheet" role="dialog" aria-modal="true" tabindex="-1">
    <!-- the surface's semantic root -->
  </div>
</div>
```

The base stylesheet presents the panel as a bottom sheet: the overlay is a
bottom-justified column, the panel is 72% block-size with top-rounded
corners over a 40% black scrim. The `uh-modality-<modality>` class is
stamped from the surface view, though today only the base `.uh-surface`
bottom-sheet treatment exists — sheet presentation is effectively the
universal surface presentation. Editor boards realize the same overlay
structure read-only and label surface nodes with their modality in the
surface hierarchy and workflow views.

A native renderer may realize the contract with its platform sheet
primitive. It must reproduce the observable semantics — stacked modal
presentation, scrim/Escape-equivalent dismissal through the published
descriptor, occlusion, and focus restoration — or reject the capability
honestly.

## Motion

Sheets define no semantic motion: no enter/exit transition, no completion
event, no reduced-motion contract. The current realization mounts and
removes overlays instantly; slide-up presentation and drag-to-dismiss are
future renderer or contract work, listed below.

## Conformance

Existing executable coverage proves:

- surface declarations parse with and without `modality`, and lowering
  defaults the modality to `sheet` for surface definitions only;
- the evaluated snapshot carries per-surface modality, dismiss descriptor,
  and restore-focus key-path with an exact-field protocol check;
- the M4 gate tests drive `comments-sheet` end-to-end over the Instagram
  IR: opening, surface views in snapshots, dismissal, and the FocusRestore
  intent asserted on topmost dismissal.

A durable support claim additionally requires conformance coverage for:

- direct tests pinning the non-surface-scope dismiss error, occlusion drop
  dispositions, and the opened-surface sweep (implemented and documented in
  the step machinery, but not individually pinned);
- checker-level modality validation once the legal set is decided;
- an accessible-name mechanism for the panel;
- Escape-during-IME and scrim-versus-panel-click edge cases as pinned
  renderer tests;
- stacked-sheet (surface-over-surface) presentation and focus behavior; and
- equivalent semantics or an honest unsupported-capability diagnostic in
  non-browser renderers.

## Decisions and open questions

This page is part of the v0 documentation effort signalled in
[gridaco/uhura#22](https://github.com/gridaco/uhura/issues/22). The current
implementation and
[Instagram spike design](../../../../studies/instagram-spike-design.md)
establish useful evidence but do not replace an accepted widget RFC. The
`§8.1` cited by the code is the spike design's surface-stack section, not a
versioned language specification.

Known gaps and open questions:

1. The legal modality set is undefined — see the
   [dialog page](dialog.md) for the second declared value and its
   unrealized presentation. Whether modality becomes a checked enum, and
   where its meaning (presentation? occlusion? dismissal policy?) is
   specified, is open.
2. Sheet naming: a title/label for the dialog role has no owner.
3. Non-dismissable and confirm-before-dismiss sheets are inexpressible;
   whether dismissability becomes declarable is open.
4. Detents, partial-height sheets, and drag-to-dismiss are not part of the
   contract.
5. Enter/exit motion and its completion semantics have no owner.
6. Whether `open-surface` idempotency per (definition, context) — described
   in the spike design — is fully honored by the current staging is not
   pinned by a dedicated test.

Current implementation references:

- [Surface declaration parsing](../../../../../crates/uhura-syntax/src/parser/mod.rs)
- [Modality lowering and default](../../../../../crates/uhura-check/src/lower.rs)
- [Surface IR](../../../../../crates/uhura-core/src/ir.rs)
- [Surface view and dismiss descriptor](../../../../../crates/uhura-core/src/eval.rs)
- [Semantic view types](../../../../../crates/uhura-core/src/view.rs)
- [Structural dismiss, occlusion, and the sweep](../../../../../crates/uhura-core/src/step.rs)
- [Checker steering notes](../../../../../crates/uhura-check/src/markup.rs)
- [Play surface stack](../../../../../web/src/play/surfaces.ts)
- [Play focus mechanics](../../../../../web/src/play/focus.ts)
- [Play sheet styles](../../../../../web/src/play/shell.css)
- [Current Instagram comments sheet](../../../../../examples/instagram/client/surfaces/comments-sheet.uhura)
- [Specification router](../../../../spec/README.md)
