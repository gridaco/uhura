# A0 — Return Desk

- **Status:** Language-neutral application specification
- **Harness:** A0 — first application-transfer problem
- **Implementation:** None
- **Authority:** This product contract is authoritative for candidate
  comparison; no current Uhura behavior is accepted here
- **Scope:** One return draft, three URL-owned steps, one temporary policy
  surface, one revisioned external order, and one correlated submission

The product behavior, ownership boundaries, fixtures, traces, and semantic
presentation observations in this document are the authority. A current Uhura
project, a redesigned language, or a familiar-language baseline is an answer
sheet. The application must not be weakened or renamed to fit an answer.

## Purpose

Return Desk lets a customer prepare and submit one return for one known order.
The user selects quantities and reasons, chooses a return method, reviews the
draft, and submits it. The draft belongs to the application session. Order
eligibility and acceptance of the return belong to an external service.

The application is intentionally small, but it crosses boundaries absent from
the [L0–L2 program harnesses](../../programs/):

- the machine core must compose with explicitly activated web UI;
- the active step belongs to the URL rather than a copied state field;
- one application-owned draft must outlive several logical route scopes;
- external revisioned truth may invalidate but must not silently rewrite that
  draft;
- navigation is a requested consequence, not an immediate location mutation;
- a temporary surface has an owning instance and rejects stale interactions;
- submission and settlement are asynchronous and explicitly correlated; and
- static design examples must pin complete application context without
  pretending that external work ran.

This is an application-transfer harness, not a claim that application concepts
belong in the pure machine kernel.

## The application

The return flow has three semantic steps:

1. **items** — choose return quantities and one reason for each selected line;
2. **method** — choose how the selected items will be returned; and
3. **review** — inspect the same draft and submit it.

A read-only return-policy surface may be opened for one order line. Successful
submission leads to a receipt location. The user may also leave the flow for
the order location while a request is pending.

The harness does not prescribe page components, forms, controls, element names,
layout, styling, or a visual arrangement.

## Fixed vocabulary and fixture

Identifiers are opaque values with equality. The readable values below are
fixed fixture identities, not a required identifier representation.

The closed reason vocabulary is:

```text
damaged
not-needed
```

The closed return-method vocabulary is:

```text
drop-off
pickup
```

The initial external order observation is:

```text
Order {
  id: "order-100"
  revision: 7
  lines: [
    {
      id: "lamp"
      title: "Desk lamp"
      purchased-quantity: 2
      returnable-quantity: 2
      policy-summary: "Return the lamp in protective packaging."
    },
    {
      id: "mug"
      title: "Stoneware mug"
      purchased-quantity: 1
      returnable-quantity: 1
      policy-summary: "Wrap the mug to prevent breakage in transit."
    }
  ]
  allowed-methods: [drop-off, pickup]
}
```

The canonical changed observation is:

```text
Order {
  id: "order-100"
  revision: 8
  lines: [
    {
      id: "lamp"
      title: "Desk lamp"
      purchased-quantity: 2
      returnable-quantity: 0
      policy-summary: "Return the lamp in protective packaging."
    },
    {
      id: "mug"
      title: "Stoneware mug"
      purchased-quantity: 1
      returnable-quantity: 1
      policy-summary: "Wrap the mug to prevent breakage in transit."
    }
  ]
  allowed-methods: [drop-off, pickup]
}
```

Revision 8 makes the lamp ineligible. The application does not calculate why.
Titles and any future service-authored strings are inert data, never markup or
executable source.

The accepted return identity used by the canonical scenario is:

```text
"return-900"
```

A0 has no separate Return projection. The accepted settlement is authoritative
for the return identity. Receipt presentation is deliberately limited to that
identity and completed status; loading durable receipt details is a later
application problem.

## Ownership

| Fact | Owner |
| --- | --- |
| Order identity, revision, lines, quantities, policy summaries, and allowed methods | External order service |
| Whether a submitted return is accepted and the accepted return identity | External return service |
| Draft selections, reasons, method, request correlation, submission phase, and completion notice | One application-session coordinator |
| Current URL and browser history position | Explicit web/framework boundary, observed by the application |
| Current logical route scope and policy-surface instance | The UI application under checked lifecycle rules |
| Layout, focus mechanics, caret, IME, hit testing, animation, paint, and physical scroll | Renderer |

Discarding the application session must not change the order, create a return,
or affect another client. Ending a route scope must not discard the
application-owned draft or a pending submission.

The coordinator is one explicit owner, but this requirement does not select a
global-state language feature. A candidate may express it through composition,
an actor, a parent machine, an owned module instance, or another checked model.
Views and surfaces may not discover it through a hidden global store.

## Step results

Every declared interaction or delivery is processed atomically and receives
exactly one result:

```text
accepted
blocked(reason)
duplicate
stale
invalid(reason)
```

- **accepted** applies the specified transition and may request consequences;
- **blocked** is a well-formed current interaction whose owning location,
  prerequisites, or coordinator phase does not permit it;
- **duplicate** repeats the current fact or intent and requires no transition;
- **stale** addresses an older revision, ended scope, destroyed surface, or
  non-current request; and
- **invalid** is outside the admitted value or structural domain.

Classification uses this precedence unless a later rule is more specific:

1. malformed values, unknown vocabulary, invalid quantities, invalid order
   structure, and unknown current-domain identities are `invalid`;
2. well-formed inputs addressed to known ended lifetimes or older revisions are
   `stale`;
3. an exact repetition of a current fact or intent is `duplicate`;
4. a current semantic interaction whose eligibility rule fails is `blocked`;
5. otherwise the input is `accepted`.

Blocked, duplicate, stale, and invalid steps do not change state, open or close
a scope, allocate identity, or request consequences. Diagnostics may be
reported separately and do not alter later behavior.

## Locations and navigation

The admitted fixture locations are:

```text
/orders/order-100/return?step=items
/orders/order-100/return?step=method
/orders/order-100/return?step=review
/orders/order-100
/returns/<accepted-return-id>
```

The return path with a missing or unknown `step` value canonicalizes to
`step=items`.

A receipt location is inside the A0 domain only after the coordinator has
completed and only when its identity matches the retained accepted return
identity. A fresh-session receipt deep link and a mismatched receipt identity
are outside A0; this harness defines no Return projection or not-found product.

The active step is derived from the latest host-delivered location. It is not
stored as a second independently mutable field in the draft or route-local
state.

Navigation follows this protocol:

```text
semantic interaction
  -> requested navigation
  -> later host-delivered location
  -> new observed location and logical route scope
```

Requesting navigation never mutates the observed location in the same
application step. Links, in-application actions, browser Back or Forward, and
an externally supplied location all reconcile through the same delivered
location rule.

A changed, admissible flow location ends the previous logical route scope and
begins a new one. This is a semantic lifetime, not a requirement to remount a
framework component; a candidate may use a long-lived view, keyed scope,
actor, or another checked realization. An exact duplicate delivery of the
current location is `duplicate`: it begins no scope and requests no repeated
normalization. Leaving the flow ends its scope. Entering a normalizing state
also ends the previously usable scope until the canonical location is
delivered.

At most one navigation intent is outstanding. Requesting navigation records
that intent until a changed location is delivered. Repeating the identical
navigation-producing interaction while it is outstanding is duplicate; a
different user navigation interaction is blocked as `navigation-pending`.
A changed location delivery clears the intent even when browser history or
another host action supplied a location other than the requested target.

A required normalization or accepted-completion redirect may supersede an
outstanding navigation intent. It emits its required replace request and
becomes the sole outstanding intent. Navigation failure and cancellation are
outside A0; a conforming host eventually supplies a changed location.

### Step admissibility

The items step is always the earliest flow step. It is complete when:

- an order observation is available;
- the draft is based on that current order revision;
- at least one order line has a positive selected quantity;
- every selected quantity is at most the current returnable quantity; and
- every selected line has one admitted reason.

The method step is admissible when the items step is complete. It is complete
when the draft has one method allowed by the current order observation.

The review step is admissible when the method step is complete.

When a delivered flow location is not admissible, the application requests a
history **replace** to the earliest incomplete step:

- invalid or missing `step` -> `items`;
- `method` before items are complete -> `items`;
- `review` before items are complete -> `items`; and
- `review` after items are complete but before method is complete -> `method`.

Only the later location delivery completes normalization. During that interval
the semantic presentation is `normalizing`; it must not present an
inadmissible step as usable.

An application action that asks to advance to an inadmissible step is blocked
locally and requests no navigation. Moving among admissible steps requests a
history **push**. Browser history deliveries are not intercepted merely
because the draft is dirty or a submission is pending.

## External order observation

The application observes one order boundary in one of these semantic states:

```text
waiting
available(Order)
```

The boundary is declared explicitly. This harness does not prescribe loaders,
fetch calls, caches, subscriptions, hooks, or a candidate representation of
external observations.

For available observations:

- the order identity must be `order-100`;
- revisions must be positive integers;
- line identities and allowed methods must each be unique;
- quantities must be non-negative integers and returnable quantity must not
  exceed purchased quantity;
- every method must belong to the closed method vocabulary;
- a higher revision replaces the previous external observation;
- an equal revision with equal contents is duplicate;
- a lower revision is stale and cannot replace newer truth; and
- equal revision with different contents is invalid fixture or provider
  behavior.

An observation that violates the structural rules is invalid and cannot
replace external truth or mutate the coordinator.

When an accepted newer observation omits the line shown by the current policy
surface, that surface is destroyed in the same step. No replacement identity
is allocated and no consequence is requested. When the line remains, its
surface title and policy summary derive from the new observation.

The first available observation initializes a blank draft at that revision
when the coordinator has no draft, has not completed a return, and the current
location is inside that order's return flow. Entering the flow later performs
the same initialization from an already available observation. Merely
observing an order while outside its flow does not create a draft.

A newer revision never silently clamps, deletes, or rewrites authored draft
values. A revision conflict exists when the draft's base revision differs from
the current order revision or a service refusal reports a required revision
newer than the draft. While that conflict exists:

- the latest delivered order remains the external truth, and a reported
  revision fence does not invent a newer projection;
- the old draft remains inspectable;
- a `source-changed` conflict is derived, including the reported revision fence
  when the matching observation has not arrived;
- submission and forward progress are blocked; and
- an inadmissible current method or review location normalizes to `items`.

The user may explicitly **restart from current order** when no request is
pending and the available observation is at least as new as any reported
revision fence. Restart replaces the old draft with a blank draft based on
that observation and clears the fence, refusal, or unavailability result. It
does not attempt field-by-field merging. Restart at the same revision is
duplicate only when the draft is already blank, no fence exists, and the
submission phase is already `idle`.

## Draft semantics

A draft has the observable meaning:

```text
ReturnDraft {
  order: order-id
  base-revision: positive integer
  selections: line-id -> {
    quantity: positive integer
    reason: none | damaged | not-needed
  }
  method: none | drop-off | pickup
}
```

This notation is not required source syntax or a required in-memory shape.

The admitted semantic editing inputs are:

```text
choose-quantity(line, quantity)
choose-reason(line, reason)
choose-method(method)
restart-from-current-order
```

Editing rules are:

- an edit is admitted only while the current available order revision equals
  the draft base revision and no service-reported revision fence exists;
- quantities must be integers from `0` through the current returnable quantity
  for that line;
- choosing quantity `0` removes that line's selection and reason;
- a positive quantity creates or updates the selection and preserves an
  existing reason, if any;
- a reason may be chosen only for a positively selected line;
- a method must belong to the closed method vocabulary;
- edits are blocked while a submission is pending, after completion, or while
  the draft has a revision conflict; and
- an edit after refusal or unavailability returns the submission phase to
  `idle`, while any revision conflict remains derived from the two revisions.

A quantity that was valid when authored may become invalid
against newer external truth. It remains visible as a conflict until explicit
restart; it is not an invariant violation in the coordinator.

Review reads the same draft. It does not own copied selections, reasons,
method, totals, or validation state.

## Policy surface lifecycle

The admitted surface interactions are:

```text
open-policy(line)
dismiss-policy(surface-instance)
```

Opening policy for a line in the current order from the items step creates one
read-only policy surface instance owned by the current logical route scope. It
presents that line's identity, title, and exact external `policy-summary`. It
cannot invent policy, edit the draft, request submission, or change product
truth.

At most one policy surface is open. Opening the already open line is duplicate;
opening a different valid line replaces the previous surface with a new
instance. Each newly created surface receives a fresh deterministic identity
that is never reused during the session. The fixtures bind the first identity
as `surface-1`; a candidate need not use that spelling or a counter. An unknown
line is invalid and opens nothing.

The matching dismiss input destroys the surface. Any delivered location change
that ends the owning route scope also destroys it. A later interaction
addressed to a destroyed, replaced, or unknown surface identity is stale and
cannot close a newer surface or mutate the draft.

An accepted order update refreshes the surface's external title and summary
when its line remains. If the line disappears, the surface closes as specified
by the external-observation rule; the application never retains an old policy
snapshot as current truth.

The candidate may realize this as a modal, overlay, conditional view, nested
machine, or another checked UI construct. Focus trapping, focus restoration,
animation, modality mechanics, and visual styling are outside A0.

## Semantic interaction domain

The complete application interaction set is:

```text
choose-quantity(line, quantity)
choose-reason(line, reason)
choose-method(method)
go-to-step(items | method | review)
open-policy(line)
dismiss-policy(surface-instance)
restart-from-current-order
submit
follow-order-link
follow-receipt-link
```

External inputs are limited to delivered location, order observation, and
correlated settlement. Candidate adapters may not inject a second private
interaction vocabulary that performs application behavior.

| Interaction | Owning location or scope | Accepted when | Fixed non-accepted behavior |
| --- | --- | --- | --- |
| `choose-quantity` | active `items` | line and quantity satisfy the draft rules; editable phase | unknown line or out-of-range/non-integral quantity is invalid; same quantity is duplicate; wrong location, normalizing, pending, completed, or revision conflict is blocked |
| `choose-reason` | active `items` | line is selected; admitted reason; editable phase | unknown/unselected line or unknown reason is invalid; same reason is duplicate; wrong location or non-editable phase is blocked |
| `choose-method` | active `method` | admitted and currently allowed method; editable phase | unknown or disallowed method is invalid; same method is duplicate; wrong location or non-editable phase is blocked |
| `go-to-step` | active return-flow scope | target is admitted and currently admissible | unknown target is invalid; current or identical pending target is duplicate; different navigation while one is pending, normalizing, or incomplete prerequisite is blocked |
| `open-policy` | active `items` | line exists in current order | unknown line is invalid; same currently open line is duplicate; wrong location or normalizing is blocked |
| `dismiss-policy` | owning scope | identity matches the current open surface | unknown, replaced, destroyed, or ended-scope identity is stale |
| `restart-from-current-order` | active `items` | no request pending; current observation satisfies any revision fence | already blank/current/idle is duplicate; missing observation, wrong location, normalizing, or pending is blocked |
| `submit` | active `review` | draft is complete and current; phase is `idle` or `unavailable` | pending Submit is duplicate; wrong location, normalizing, incomplete/conflicted draft, refusal, or completion is blocked |
| `follow-order-link` | active flow or matching receipt | location is not normalizing and no different navigation is pending | an identical pending request is duplicate; otherwise blocked |
| `follow-receipt-link` | an admitted outside-flow location | a completion notice exists and no different navigation is pending | an identical pending request is duplicate; otherwise blocked |

Accepted `go-to-step` requests a push to the target step. Accepted
`follow-order-link` requests a push to `/orders/order-100`. Accepted
`follow-receipt-link` requests a push to the retained receipt. Draft edits,
restart, and policy-surface interactions request no external consequence.
Submit is defined below.

Every accepted UI interaction above must be operable from a checked `ui`
presentation. A headless machine with adapter-injected interactions and a
ceremonial `ui` activation is not a conforming A0 application. This requirement
does not prescribe elements, widgets, components, layout, or event spelling.

## Submission protocol

Submitting is admitted only from `step=review` when:

- the current location is not normalizing;
- the order observation is available;
- the draft is complete and based on that exact revision;
- there is no open conflict;
- the submission phase is `idle` or `unavailable`;
- no request is pending; and
- the coordinator has not completed.

An admitted submit attaches a fresh deterministic request identity and
requests exactly one external command:

```text
submit-return(
  request-id,
  order-id,
  expected-revision,
  selections,
  method
)
```

The selections include each selected line's quantity and reason. Request
identities are never reused within the application session. The fixtures call
the first two identities `request-1` and `request-2`; a candidate need not use
a counter or that spelling. Identity must come from checked coordinator state
or another explicitly declared deterministic source, not an ambient clock or
randomness.

The command payload is an immutable snapshot. While it is pending, repeating
Submit is duplicate and emits no second command. Navigating among or away from
locations does not cancel or forget the pending request.

Selections in a request are keyed by line identity. Their semantic equality is
order-insensitive; map iteration or source declaration order is not an
observable command difference.

The application performs no optimistic mutation of order or return truth. It
presents a pending application state until a declared settlement arrives.

### Settlements

Every settlement carries the request identity and has one of these meanings:

```text
accepted(return-id)
refused(order-changed(current-revision))
unavailable
```

`accepted`, `refused`, and `unavailable` are distinct. A domain refusal means
the service received and rejected the operation. Unavailable is a provider
guarantee that this request was not applied, will not settle later, and is safe
to retry. It is not an ambiguous transport timeout: no domain decision exists
because the operation did not reach commitment.

A matching settlement has these effects:

- **accepted** records completion, clears the draft, and retains the return
  identity;
- **refused** preserves the draft and records the exact refusal;
- **unavailable** preserves the draft and records retryable unavailability.

An order-changed refusal is valid only when its reported revision is greater
than the pending snapshot's expected revision. It records a non-authoritative
revision fence and waits for the external order boundary to deliver an
observation at least that new. The refusal does not invent or mutate an order
observation. A reported revision less than or equal to the submitted revision
is an invalid provider delivery: it does not settle the request or change
state.

The revision fence immediately blocks edits and further submission. Ordinary
edits cannot clear it. Only an admitted restart against a sufficiently new
external observation clears the fence.

A settlement that does not address the one pending request is stale, including
an unknown identity. A duplicate or contradictory settlement for an already
settled request is also stale. It causes no state change, navigation, or
external command.

An order-changed refusal can be repaired only by an admitted restart. Submit
after unavailability is admitted when the ordinary draft, revision, location,
and completeness rules remain satisfied; it requests the same draft contents
under a new request identity.

An outcome that might have committed despite a lost response would require a
stable idempotency identity distinct from transport-attempt correlation. That
larger protocol is outside A0 and must not be simulated by delivering
`unavailable`.

### Late settlement and navigation

The coordinator, not a logical route scope, owns pending correlation.

When acceptance arrives:

- if the matching receipt location is already current, the application
  requests no navigation and creates no notice;
- otherwise, if the current delivered location is inside the return flow for
  `order-100`, the application requests one history **replace** to the receipt
  location;
- otherwise, the application does not hijack the current
  location and instead exposes one completion notice with a semantic link to
  the receipt.

Following the completion notice requests a history **push** to the receipt.
The notice persists across other admitted outside-flow locations and is
cleared when that receipt location is delivered.

After completion, a later delivery of an old return-flow location requests a
replace back to the receipt. The completed coordinator cannot begin a second
return in A0.

## Requested consequences

A0 observes only these ordered consequence families:

```text
navigate(push | replace, location)
submit-return(request snapshot)
```

One semantic input or external delivery produces a finite ordered sequence of
consequences. External work cannot synchronously deliver a settlement into the
middle of that step. Any location or settlement caused by a consequence is a
later declared delivery.

Renderer operations and diagnostics are not product consequences.

## Conformance oracles

A0 keeps three kinds of observation separate.

### Semantic presentation

The checked `ui` presentation must expose the product meaning needed to verify:

- current delivered location and active or normalizing step;
- order boundary status and safely presentable order fields;
- draft selections, reasons, and method;
- source, eligibility, and completeness conflicts;
- whether each semantic edit, step navigation, policy-surface action, link,
  restart, or submit action is enabled;
- idle, pending, refused, unavailable, or completed submission status;
- the current policy surface's line, title, and exact policy summary;
- completion notice and receipt link when acceptance settled outside the flow;
  and
- receipt identity and completed status at the matching receipt location.

Presentation need not expose request, route-scope, or surface-instance
identities. It does not retain the previous step's consequences. The oracle
does not include DOM shape, HTML element names, component
boundaries, CSS, pixels, animation frames, focus mechanics, physical scroll,
or screenshot similarity.

### Step oracle

Every atomic input exposes its one
[step result](#step-results) and finite ordered requested consequences. These
values belong to trace and conformance inspection, not to product presentation
and not necessarily to persistent coordinator state.

### Lifecycle and checkpoint inspection

Headless inspection may additionally expose enough non-product context to
verify correlation and replay: current route-scope identity or generation,
open surface identity and owner, pending and settled request identities,
immutable pending snapshot, outstanding navigation intent, revision fence, and
deterministic identity allocation state. A candidate may encode this
differently, but it cannot hide the facts from its trace/checkpoint tooling or
require the user-facing presentation to display them.

## Canonical trace

`[]` means no requested consequence. Navigation requests do not change the
location shown in the same row; the following host delivery does.

| Step | Declared input or delivery | Required result | Ordered consequences | Required state or observation after the step |
| ---: | --- | --- | --- | --- |
| 0 | initial application session | initialized | `[]` | no delivered location; order `waiting`; no draft |
| 1 | location `/orders/order-100/return?step=review` | accepted | `[navigate(replace, step=items)]` | delivered location remains `review` but is inadmissible; presentation `normalizing` |
| 2 | location `/orders/order-100/return?step=items` | accepted | `[]` | active `items`; order `waiting` |
| 3 | order revision 7 | accepted | `[]` | blank draft at revision 7 |
| 4 | `choose-quantity(lamp, 1)` | accepted | `[]` | lamp selected without a reason; items incomplete |
| 5 | `choose-reason(lamp, damaged)` | accepted | `[]` | items complete |
| 6 | `open-policy(lamp)` | accepted | `[]` | policy surface `surface-1` open for lamp |
| 7 | `dismiss-policy(surface-1)` | accepted | `[]` | no surface open; draft unchanged |
| 8 | `go-to-step(method)` | accepted | `[navigate(push, step=method)]` | delivered location remains `items` |
| 9 | location `/orders/order-100/return?step=method` | accepted | `[]` | active `method`; new logical route scope |
| 10 | `choose-method(drop-off)` | accepted | `[]` | method complete |
| 11 | `go-to-step(review)` | accepted | `[navigate(push, step=review)]` | delivered location remains `method` |
| 12 | location `/orders/order-100/return?step=review` | accepted | `[]` | active `review` |
| 13 | `submit` | accepted | `[submit-return(request-1, order-100, revision 7, lamp × 1 damaged, drop-off)]` | `request-1` pending with immutable snapshot |
| 14 | `submit` | duplicate | `[]` | `request-1` remains the only pending request |
| 15 | browser Back delivers `step=method` | accepted | `[]` | active `method`; pending coordinator survives route-scope replacement |
| 16 | `request-1` refused: `order-changed(8)` | accepted | `[]` | draft preserved; revision fence 8 and exact refusal visible; no request pending |
| 17 | order revision 8 | accepted | `[navigate(replace, step=items)]` | lamp selection remains visible but conflicted; current location remains `method` until delivery |
| 18 | location `/orders/order-100/return?step=items` | accepted | `[]` | active `items`; revision conflict visible |
| 19 | `restart-from-current-order` | accepted | `[]` | blank draft at revision 8; fence and refusal cleared |
| 20 | `choose-quantity(mug, 1)` | accepted | `[]` | mug selected without reason; items incomplete |
| 21 | `choose-reason(mug, not-needed)` | accepted | `[]` | items complete with only mug selected |
| 22 | `go-to-step(method)` | accepted | `[navigate(push, step=method)]` | delivered location remains `items` |
| 23 | location `/orders/order-100/return?step=method` | accepted | `[]` | active `method` |
| 24 | `choose-method(pickup)` | accepted | `[]` | method complete |
| 25 | `go-to-step(review)` | accepted | `[navigate(push, step=review)]` | delivered location remains `method` |
| 26 | location `/orders/order-100/return?step=review` | accepted | `[]` | active `review` |
| 27 | `submit` | accepted | `[submit-return(request-2, order-100, revision 8, mug × 1 not-needed, pickup)]` | `request-2` pending |
| 28 | browser Back delivers `step=method` | accepted | `[]` | active `method`; `request-2` still pending |
| 29 | `request-2` accepted as `return-900` | accepted | `[navigate(replace, /returns/return-900)]` | completed; draft cleared; delivered location still `method` |
| 30 | location `/returns/return-900` | accepted | `[]` | receipt active; completion notice absent |
| 31 | duplicate acceptance for `request-2` | stale | `[]` | no state or location change |

The trace proves at least:

- direct deep links reconcile through requested replacement;
- URL progress is not duplicated as draft state;
- logical route scopes and a temporary surface do not own the draft;
- a pending request survives browser history navigation;
- duplicate submission emits no duplicate command;
- revisioned external truth does not silently repair local state;
- an explicit restart establishes a new revision boundary;
- request identities are unique;
- acceptance is owned outside a destroyed route scope; and
- duplicate settlement cannot repeat navigation.

## Additional required scenarios

Candidate conformance must also cover:

1. a direct `review` location with complete items but no method normalizes to
   `method`;
2. a missing and an unknown `step` value each normalize to `items`;
3. browser Forward redelivers an inadmissible location after restart and is
   normalized by the same rule;
4. an older order revision arrives after revision 8 and cannot replace it;
5. a newer observation removes a selected line identity rather than only
   reducing quantity; the authored selection remains visibly conflicted and a
   policy surface for that line closes without allocating a replacement;
6. a policy surface is destroyed by navigation, then a stale dismiss for it
   cannot close a newer surface;
7. unavailable settlement preserves the draft and a later Submit emits one
   command with a new request identity;
8. an order-changed refusal reporting a revision no newer than the pending
   snapshot is invalid and leaves that request pending;
9. ordinary edits and Submit cannot bypass a valid order-changed revision
   fence before the required observation and explicit restart;
10. acceptance arrives after the user has left for `/orders/order-100`; current
   location is not hijacked, one receipt notice appears, and following it
   requests a push to the receipt;
11. a conflicting or unknown settlement cannot mutate the draft or navigate;
12. a location delivery into the old flow after completion replaces to the
    receipt rather than starting another return; and
13. repeating the same inadmissible or completed-flow location while its
    normalization request is outstanding is duplicate and emits no second
    navigation; and
14. repeating one navigation-producing interaction while its intent is pending
    is duplicate, a different user navigation is blocked, and a changed
    location delivery clears the intent.

## Static examples

Every candidate answer must provide static pins for:

- waiting at items;
- an incomplete items draft;
- a complete items draft;
- method selection;
- review;
- an open policy surface;
- pending submission after browser Back;
- source-changed conflict with the old selection retained;
- domain refusal;
- retryable unavailability;
- completion notice outside the flow; and
- receipt.

Each pin supplies complete location, external observation, coordinator state,
pending or settled correlations, and route/surface lifecycle context. A pending
pin contains the immutable request snapshot that a prior semantic Submit would
have requested; pinning that state does not prove that a host executed the
external command. Pins that are not reachable must be labelled as such.

At least the canonical trace, late-outside-flow scenario, and stale-surface
scenario must also be authored as reachable scenarios rather than static pins.

One checkpoint is taken with `request-2` pending after browser Back. It contains
the delivered location and route-scope generation, order revision, coordinator
draft and phase, immutable pending snapshot, settled-request ledger, fresh
request and surface identity allocation state, outstanding navigation intent
or its absence, open-surface context, and replay provenance.

Restoring the checkpoint emits no navigation or `submit-return` command and
does not allocate another identity. Exactly one outstanding `request-2`
remains eligible for settlement. Replaying the same
acceptance-and-location suffix must produce the same coordinator state,
semantic presentation, step results, requested navigation, and receipt.

## Invariants

These properties hold initially and after every atomic step:

1. External order and return truth can change only through declared external
   deliveries.
2. The current step has exactly one authority: the latest delivered URL.
3. At most one application draft exists, and it belongs to exactly one order
   and one base revision.
4. Every stored selection has positive integral quantity and either no reason
   or one admitted reason; quantity `0` is represented by absence.
5. A newer external revision may make a stored selection conflicted but cannot
   silently mutate it.
6. Review derives from the one draft and owns no copied form values.
7. At most one submission is pending, and its payload is immutable.
8. Every emitted request identity is unique within the session.
9. A service-reported revision fence is newer than its submitted revision and
   blocks editing and submission until explicit restart against a sufficiently
   new external observation.
10. Each settlement can settle at most one matching pending request; stale,
   duplicate, unknown, and contradictory settlements have no consequences.
11. At most one policy surface is open, it belongs to one current route scope,
    its line exists in the current order observation, and a destroyed instance
    cannot receive effective input.
12. Request and surface identities are never reused within the session.
13. At most one navigation intent is outstanding; an identical user request
    cannot emit another consequence, and only a changed location or specified
    semantic supersession can replace or clear it.
14. A route-scope or surface replacement cannot discard or settle
    coordinator-owned work.
15. Navigation changes observed location only through a later host delivery.
16. Completion creates no second return and cannot navigate more than once for
    one delivered acceptance.
17. Replaying the same complete delivery and interaction sequence from the same
    context produces the same classifications, states, presentations, and
    ordered consequences.

## Candidate-language obligations

Every answer sheet must make these dependencies statically visible without
this harness prescribing their grammar or package names:

- the `ui` extension;
- route and location decoding;
- search-parameter observation;
- link and navigation/history semantics;
- authoritative order observation;
- request and settlement lifecycle; and
- temporary-surface presentation and instance addressing.

Imports may contribute checked vocabulary and contracts. Merely importing one
cannot fetch the order, initialize the coordinator, mutate a draft, navigate,
submit, or acquire host authority.

In addition, a candidate answer must:

- keep its L0–L2 machine answers usable without `ui` or browser machinery;
- expose one explicit owner for the cross-route draft and correlation;
- distinguish application-owned values from externally authoritative facts;
- process one declared input as one finite, non-reentrant step;
- publish state before requested external work may later settle;
- preserve ordered consequences;
- address location, route scope, surface, request, and return identities without
  conflation;
- remain replayable headlessly without DOM or pixel comparison;
- use the same declared contract for fixture and live providers;
- disclose candidate-specific host glue; and
- avoid arbitrary JavaScript or another unchecked escape hatch.

The harness does not require a candidate to expose `X`, copy external facts
into owned state, use actors, implement a global store, or lower to a
particular IR. Those choices remain part of the language proposal and its
evaluation.

## Fair comparison

All candidates receive the same trace-level environment:

```text
location delivery
revisioned order observation
semantic user interaction
observed navigation request
observed submit-return request
correlated service settlement
```

Conformance to this document is pass/fail. Only conforming answers are compared
for readability, compactness, learnability, and ergonomics.

Report separately:

1. application-authored source;
2. `ui` and imported application-feature declarations;
3. static examples, scenarios, and checkpoint source;
4. fixture and candidate-specific adapter glue; and
5. behavior supplied by the compiler, framework, and runtime.

Do not compare line or token counts until all required behaviors, negative
cases, and artifact classes are present. Shared helpers count as authored
source when the application answer depends on them.

## Fixed comparison probes

These probes are not additional product features. They make dependency
diagnostics and change cost comparable.

### Missing-dependency probes

Starting from a conforming answer, the comparison removes each owning
declaration in turn:

1. `ui` activation while retaining checked presentation;
2. route/location or search-parameter capability while retaining its first use;
3. link/navigation capability while retaining its first use;
4. request/settlement capability while retaining Submit; and
5. the host binding for one otherwise valid imported application feature.

Cases 1–4 must fail statically at the first construct requiring the missing
contract. An incompatible imported feature version must fail dependency
resolution rather than masquerade as a missing host. Case 5 must produce an
unavailable-host diagnostic before the scenario begins and must not partially
run it.

A candidate may bundle tightly coupled capabilities in one explicit feature
module. The probe removes the module that owns the required contract; it does
not require one module per bullet or freeze import spelling.

### Controlled change C1

After base conformance and initial measurements, every answer receives this
same change:

```text
Reason =
  damaged
  | not-needed
  | other(note)
```

`note` is exact user-authored text. The intermediate empty value is valid
draft state but does not complete the items step. A non-empty value completes
the reason; whitespace is text and no trimming or Unicode normalization is
implicit. The submit snapshot carries the tagged reason and its note. Changing
from `other(note)` to another reason discards the note.

The change adds one static incomplete pin and one reachable scenario that edits
the empty note to `"Does not fit the room"`, reaches review, and verifies the
tagged request payload. It adds no route, external authority, settlement kind,
widget requirement, or runtime capability.

Record edits to application source, feature declarations, examples,
scenarios, checkpoint representation, adapters, and runtime glue separately.
The purpose is to reveal whether the answer modeled the reason domain or only
made the original two literals concise.

## Non-goals

A0 deliberately excludes:

- order search, order history, pagination, feeds, and lists of returns;
- authentication, customer identity, authorization implementation, and order
  lookup;
- multiple simultaneous drafts, persistence across reload, multi-tab,
  cross-device, offline, or collaborative behavior;
- prices, refunds, currency, tax, payment, inventory, exchanges, and gift
  returns;
- policy computation, deadlines, clocks, debounce, automatic retry, and
  timeout;
- ambiguous transport outcomes and multi-attempt idempotency protocols;
- photos, attachments, rich text, shipping labels, carriers, maps, printers,
  cameras, and other device APIs;
- optimistic mutation of product truth;
- automatic fetching, cache invalidation, subscription, or revalidation
  policy;
- browser-Back interception and dirty-draft blocking;
- dynamic form schemas or field-by-field validation-framework design;
- widget, accessibility-widget, focus, caret, IME, animation, modal, CSS,
  layout, responsive, or pixel conformance;
- server rendering, hydration, file-based routing, and named JavaScript
  framework lifecycles; and
- inline JavaScript, foreign-function behavior, and escape-hatch design.

The application asks one bounded question:

> Can a standalone deterministic machine become a practical, route-aware web
> application whose draft and asynchronous settlement outlive logical route
> scopes and temporary surfaces, while every non-core semantic dependency
> remains explicit?
