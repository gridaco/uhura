# Carousel and the existing pager

- **Status:** Non-normative candidate study for the future `ui` sub-specification
- **Lifetime:** Disposable study
- **Method:** Primary-source reading of the referenced specifications retrieved
  July 20, 2026, plus direct verification against the code on `main` and
  `feature/uhura-0.4-language-rewrite`
- **Origin:** The "Carousel and existing pager" candidate in the widget
  candidate landscape ([#19](https://github.com/gridaco/uhura/issues/19))
- **Doctrine:** [Mission and identity](../doctrine/mission.md),
  [authoring ergonomics](../doctrine/authoring.md), and
  [design principles](../doctrine/principles.md)
- **Related study:** [Machine-first language, opt-in UI, and explicit framework
  features](machine-first-language-and-opt-in-ui.md)
- **Authority:** Research only; this document reserves no syntax, accepts no
  contract, and creates no obligation for the `ui` sub-specification

## 1. Problem and non-goals

Uhura ships a `pager` element whose observable contract has never been
settled. The candidate landscape asks five questions about it: the smallest
renderer-neutral contract; who owns the current page; how keys, inactive
slides, direction, page change, and static preview work; whether carousel is a
Pattern over pager, an opt-in compound widget, or a separate Element; and how
real controls, interactive indicators, autoplay, focus/hover stopping, reduced
motion, and non-drag alternatives are guaranteed.

This study collects the external prior art, records what the implementations
actually do today, applies the machine/UI state-ownership boundary to the
contested state, and proposes candidate answers.

Non-goals:

- deciding `ui` grammar or attribute spelling;
- specifying autoplay scheduling or animation implementation;
- settling the Pattern/compound/Element taxonomy itself — this study only
  places carousel within whichever tiers the `ui` sub-specification defines;
- touching the deterministic core language.

## 2. What exists today

### 2.1 The v0 spike contract

The spike catalog (`examples/instagram/client/catalog/base.toml` on `main`)
declares `pager` as a layout viewport with keyed children, an
`indicator` token (`none | dots`), a required `label`, and a `page-change`
event of kind `observe`. The catalog is self-describing about its own gaps:
the current page is "uncontrolled in the spike", and `page-change` is
"Declared for controlled use; the spike never binds it".

### 2.2 The 0.4 branch

On `feature/uhura-0.4-language-rewrite`:

- The checker keeps `pager` in the native element vocabulary, keeps
  `("pager", "page-change")` as a valid event, restricts attributes to
  `indicator` (`none | dots`) and `label` (required text), and defines **no
  current-page, page-size, or direction attribute**
  (`crates/uhura-check/src/checker.rs`).
- The web renderer projects `pager` to `role="group"` plus an `aria-label`
  taken from `label`, builds a scroll-snap track
  (`scroll-snap-type: x mandatory`; each child `flex: 0 0 100%;
  scroll-snap-align: center`), and derives the active dot purely inside the
  renderer as `round(track.scrollLeft / track.clientWidth)`
  (`web/src/renderer/projection.ts`). Dots are non-interactive. The
  projection test names this division of labor directly: it "owns pager
  children through a stable track and synchronizes dot mechanics"
  (`web/src/renderer/projection.test.ts`).
- No code under `web/` dispatches `page-change`. The event has therefore been
  declared but bound by zero renderers in both v0 and 0.4.

So today's pager is a purely renderer-owned scroller whose current page is
invisible to the machine, while the checked vocabulary carries an event that
promises the opposite. That contradiction is the concrete cost of leaving the
contract unsettled.

### 2.3 ARIA APG carousel pattern

The [APG Carousel pattern](https://www.w3.org/WAI/ARIA/apg/patterns/carousel/)
supplies the accessibility half of the prior art:

- The container takes `role="region"` or `role="group"` with
  `aria-roledescription="carousel"` and a required accessible name. Each slide
  takes `role="group"` with `aria-roledescription="slide"` (the tabbed variant
  uses `tabpanel`), named individually — a "3 of 10" name is explicitly
  admitted because `group` supports neither `aria-setsize` nor
  `aria-posinset`.
- Previous- and next-slide **buttons are listed unconditionally as needed
  features** (APG's wording is "needed", not "required"). A slide picker is
  explicitly optional and comes in two shapes: a tabs-pattern picker (single tab stop) or
  a grouped button picker where the current slide's button is
  `aria-disabled="true"` (kept in the tab sequence deliberately).
- A rotation stop/start button is required **only if** the carousel
  auto-rotates, must be first in the carousel's tab sequence, and carries no
  toggle state — its label changes with the action instead.
- Auto-rotation must stop when keyboard focus enters the carousel (and must
  not resume without an explicit request) and must stop while the pointer
  hovers the carousel. A live-region wrapper is optional: `aria-live="off"`
  while auto-rotating, `"polite"` otherwise.
- Activating rotation/previous/next does not move focus.
- The pattern says **nothing** about touch or swipe gestures and nothing about
  reduced motion. The gesture half of a carousel is simply outside APG's
  scope.

### 2.4 CSS Scroll Snap

[CSS Scroll Snap Level 1](https://www.w3.org/TR/css-scroll-snap-1/) supplies
the paging-mechanics half, and its own boundary is instructive:

- The normative surface is geometric: snap positions are alignments of a
  child's snap area within the container's snapport; `mandatory` obliges the
  container to rest at a snap position when scrolling terminates;
  `proximity` leaves even that to UA discretion; `scroll-snap-stop: always`
  forbids passing over a snap position mid-gesture.
- Everything kinetic is deliberately unspecified: "The CSS Scroll Snap Module
  intentionally does not specify nor mandate any precise animations or physics
  used to enforce snap positions; this is left up to the user agent."
- After content changes, the UA must re-snap, and "if the scroll container
  was snapped before the content change and that same snap position still
  exists … the scroll container must be re-snapped to that same snap
  position". The UA therefore already tracks *which element* is current — it
  just never exposes it: Level 1 defines no event and no script- or
  accessibility-visible notion of the snapped element. (Snap events are being
  drafted in Scroll Snap Level 2; that attribution is from general knowledge,
  not this document's references.)
- Axis and alignment resolve logically against the snap container's writing
  mode, so an inline-axis `start` alignment is the right edge under RTL.

The design lesson both sources agree on: specify the *resting states* and the
*semantic controls*, and leave the *motion between them* to the realization.

## 3. Ownership analysis

Apply the state-ownership boundary articulated for 0.4 (PR #24 discussion):
durable, program-observable state belongs to the machine; the UI may own
ephemeral interaction mechanics; observations flow down; semantic intent
returns through explicit, typed machine inputs; the UI must not silently
retain or mutate state on which program behavior depends.

For a pager this splits cleanly:

| Concern | Owner | Reasoning |
| --- | --- | --- |
| Current page, when anything observes it | Machine | Conditional content, analytics, guards, or persistence make it program-observable by definition |
| Current page, when nothing observes it | Renderer | Equivalent to the caret/IME allowance for `textfield`: pure interaction mechanics |
| Scroll physics, momentum, in-flight gesture offset | Renderer | Exactly what CSS Scroll Snap reserves for the UA |
| Page set, order, and identity (keys) | Machine (via markup projection) | The keyed children are projected data; re-snap identity depends on stable keys |
| Indicator rendering (dots) | Renderer | Mechanics today, and correct as mechanics — until indicators become interactive, at which point activation is semantic intent and must dispatch a typed input |
| Autoplay *policy* (whether/interval) | Declaration | An accessibility-governed behavior, not free renderer choice |
| Autoplay *scheduling* (timers, pause on hover/focus) | Renderer | Kinetics, like snapping physics |

The one genuinely contested cell is the current page, and the textfield
precedent already resolves it: renderer-owned while unobserved, machine-owned
(controlled) the moment the program depends on it. The failure mode of leaving
it uncontrolled while pretending otherwise is precisely the declared but
never-bound `page-change` we have now.

## 4. A small operational model

A pager, renderer-neutrally, is:

```text
pages   : ordered list of keyed slides (keys unique, order meaningful)
current : key into pages (not an index; see re-snap rule)
axis    : inline | block, resolved logically (RTL flips inline)
events  : PageChanged(key) — semantic intent, emitted at rest, not per frame
invariant : current ∈ keys(pages); pages nonempty ⇒ current defined
```

Consequences worth making explicit:

- **Key, not index.** CSS Scroll Snap's re-snap rule is element-identity
  based: if the snapped element survives a mutation, the container re-snaps to
  *it*, not to its old offset. An index-based contract cannot express that; a
  key-based one gets it for free, and deleting the current slide becomes the
  same well-defined question as deleting any keyed row.
- **Rest, not motion.** `PageChanged` fires when a scroll terminates on a new
  snap position — mirroring `mandatory`'s end-state obligation — never during
  the gesture. In-flight offset is renderer mechanics and is not observable.
- **Deterministic replay.** Because the event carries the resting key only,
  a trace replays identically regardless of gesture physics, which keeps the
  self-verifying preview property intact.

## 5. Adversarial cases

- **Current slide deleted.** Machine updates `pages`; the invariant forces a
  new `current`. Candidate rule mirroring the CSS ambiguity clause: nearest
  following key, else nearest preceding. A controlled pager makes this a
  checkable machine decision instead of UA-defined drift.
- **Stale page intent.** User swipes to a slide the machine has meanwhile
  removed: `PageChanged(key)` arrives with a dead key. This is exactly the
  0.4 result taxonomy's `stale` outcome; no new machinery is needed.
- **Single page.** Previous/next controls have nothing to do; the APG
  expectation that they exist collides with a degenerate page set. Candidate:
  controls render disabled rather than vanish, keeping the tab order stable.
- **Autoplay under reduced motion.** APG is silent on reduced motion; CSS
  leaves motion to the UA. If the platform signals reduced motion, candidate
  rule: declared autoplay is suppressed and the optional live region behaves
  as "not automatically rotating" (`polite`). This must be a conformance
  rule, not renderer goodwill, precisely because neither upstream source
  covers it.
- **RTL.** If `axis` resolves logically, "next" under RTL moves left. The
  contract must say whether previous/next controls are logical (recommended,
  matching Scroll Snap's writing-mode resolution) or physical.
- **Mid-gesture machine write.** The machine sets `current` while a drag is in
  flight. Renderer owns the gesture; candidate rule: the write wins at gesture
  end unless the gesture itself terminated on a different snap position after
  the write — the same last-writer question every controlled input has, and
  it should be answered once, in the `ui` sub-specification, for all
  controlled mechanics.

## 6. Static checking and runtime consequences

Checkable today, with no new machinery:

- keyed slides with a checked key type (the 0.4 checker's ui `each` key
  rules already exist);
- required `label` (already enforced in the 0.4 checker);
- `page-change` bound ⇒ payload type matches the declared machine input;
- **the reverse obligation**: an element must not declare an event in the
  checked vocabulary that no conforming renderer realizes — the current
  pager would fail this, which is the point.

Checkable once controlled current-page exists:

- controlled/uncontrolled consistency: a pager whose current page is read
  anywhere must bind `page-change` (the analogue of the controlled
  `textfield` spelling);
- autoplay declared ⇒ rotation control present in the realization contract.

Runtime: stale keys map to `stale`, malformed payloads to `invalid`, guarded
refusals to `blocked` — the existing result vocabulary covers the pager
without extension.

## 7. Renderer and boundary effects

- **Web realization** keeps exactly what it has: scroll-snap track, physics,
  in-flight offset, dot synchronization. It gains one duty (dispatch
  `PageChanged` on rest) and one option (drive position from a controlled
  `current`).
- **Static preview pose.** The candidate landscape's framing lists "a
  scrolled viewport" as a pose problem. A key-based `current` doubles as the
  declarative pose: a static preview of a pager at slide `k` is well-defined
  without any gesture machinery. The same shape should generalize to
  `scroll`'s preview pose.
- **Accessibility projection.** The APG mapping is mechanical from the model:
  container name from `label`; per-slide names ("n of m" admitted); controls
  as real buttons. Today's realization stops at `role="group"` +
  `aria-label` with no roledescription, no slide names, and no controls —
  the gap between the two is a candidate conformance checklist, not a design
  question.
- **Editor.** The editor canvas needs the pose, the key list, and the
  indicator declaration — all present in the model; nothing editor-specific
  leaks into the contract.

## 8. Migration

- v0/0.4 markup (`label`, `indicator`, keyed children) is forward-compatible
  unchanged; every addition (controlled `current`, bound `page-change`,
  `axis`) is opt-in.
- The observe-kind `page-change` of the spike maps onto a typed machine input
  under the 0.4 event model, following whatever general observe→input mapping
  the `ui` sub-specification adopts for the other declared events.
- No existing example binds `page-change`, so nothing breaks by defining it
  properly; the instagram spike gains the option of making story/media paging
  observable.

## 9. Candidate answers to the issue's questions

1. **Smallest renderer-neutral contract:** keyed slides + resting current
   page (key) + logical axis + required label + indicator token. Everything
   kinetic excluded, mirroring the Scroll Snap normative split.
2. **Current page ownership:** renderer-owned while unobserved;
   machine-owned (controlled, with `PageChanged` at rest) the moment any
   program behavior depends on it — the textfield rule applied to paging.
3. **Keys, direction, preview:** keys mandatory and identity-bearing
   (re-snap follows the element, so must the contract); direction logical
   with RTL resolved as in CSS writing modes; static preview = declared
   current key, which also answers the pose problem.
4. **Carousel's tier:** carousel adds only *semantic controls and rotation
   policy* on top of pager mechanics — prev/next buttons, optional picker,
   rotation control — all of which are ordinary buttons plus one declaration.
   The evidence therefore favors **composition over a new Element**: a
   Pattern (or compound) over `pager` + `button`, in whichever tier the `ui`
   sub-specification provides for named compositions. A separate Element is
   justified only if autoplay policy cannot be declared compositionally.
5. **Guarantees:** APG's baseline features (real prev/next buttons; rotation
   control iff autoplay; focus/hover stopping; no focus theft) become
   conformance obligations of the carousel composition. Reduced motion and
   touch alternatives are **not covered by either upstream source** and are
   exactly where Uhura's contract must go beyond prior art: candidate rules —
   reduced motion suppresses declared autoplay; every gesture-reachable page
   is control-reachable (the required buttons already guarantee this).

## 10. Open questions

- The general observe→typed-input mapping for `ui` events (this study assumes
  it; the sub-specification owns it).
- The last-writer rule for machine writes during in-flight gestures, shared
  with all controlled mechanics.
- Whether the composition tier ("Pattern") the answer to question 4 relies on
  is itself part of the `ui` sub-specification's initial scope.
- Whether `indicator` grows an interactive form (picker), which would move
  indicator activation from mechanics to semantic intent.
