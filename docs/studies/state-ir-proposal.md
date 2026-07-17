# A class-differentiated state IR (draft 0)

- **Status:** Study proposal — non-normative, unaccepted
- **Lifetime:** Disposable study
- **Inputs:** [Database-bound state in client applications](db-bound-state-survey.md)
  (cited below as **DB**), [Client state architecture in the wild](client-state-survey.md)
  (cited as **CS**), and the
  [application-scale stress test](application-scale-stress-test.md).
- **Destination:** Uhura studies

The two surveys ended in findings; this note proposes the intermediate
representation that models them. It is deliberately pitched at IR altitude:
a typed, versioned, machine-checkable document that a surface language
compiles *to* and a runtime executes *from*. Proposing the IR first keeps
the surface syntax — which needs more iteration — free to change without
renegotiating semantics.

Every fragment of IR text in this note is an **illustrative encoding**, not
a surface syntax and not a wire format. Within this proposal, the IR is a set
of node kinds, well-formedness rules, and an operational model; how it is
spelled (JSON, binary, anything) is a serialization decision out of scope
here.

## 1. Problem and non-goals

**Problem.** Model client application state such that the signature bugs
cataloged in the surveys become inexpressible or visible (CS §1.2):
uniform reads over state whose truth may live elsewhere; writes and repair
differentiated by storage class; optimism as engine semantics rather than
per-feature recipes; pending, refusal, and retraction as ordinary
renderable state; interaction state machine-shaped; navigation and
restoration inside the model.

**Non-goals.**

- No surface syntax. (Illustrative encodings only.)
- No widget or view semantics; the IR ends at a binding surface that a
  view layer reads.
- No wire encodings or transport; the authority seam is specified as a
  contract, not a protocol.
- No authority implementation: permissions, invariants, and aggregate
  maintenance are authority-side (DB §3.5, §3.6); the IR models only
  their client-visible consequences (refusals, retraction, pushed values).
- No collaborative text editing; convergent values are a reserved
  extension point (DB §2.7 verdict: leaf values, not the record graph).
- No replica migration beyond version fences (DB §3.8: fences-and-reset is
  the shippable truth, and the honest *default* for prototyping).

## 2. What the IR must discharge

The traceability table — each load-bearing survey finding, and the IR
construct that answers it. This table is the proposal's contract with the
research; §4–§11 are its elaboration.

| Survey finding | IR answer | Where |
|---|---|---|
| Uniform reads; class-differentiated writes (DB §5.3) | `class` on every cell; one read surface; write legality checked per class | §4, CHK-1 |
| Named intents with declared refusals (DB F2) | `intent` nodes bound to contract `command`s with refusal unions | §7, §8 |
| Optimism is store-level overlay + rebase (DB F3) | pending log + pure `echo` patches + rebase semantics | §7.2, §10 |
| Invalidation declared or dissolved (DB F4) | no query keys exist; bound state arrives as pushed projections into a replica | §8 |
| Constraint buys liveness (DB F5) | total, pure expression language; declared windows | §5, §8.3 |
| Client-minted identity & order (DB F6, CS C11) | `mint` effects at the submit edge; order values mintable | §4.3, §7.1 |
| Aggregates belong to authority (DB F7) | `aggregate` legal only over windows declared total; else a bound field | §5.2, CHK-5 |
| Partiality as a named artifact (DB F8) | `window` declarations in the contract | §8.3 |
| Permissions are data-shaped, authority-side (DB F9) | out of client scope; surfaces as `retracted` events + option-valued dereference | §8.4 |
| The local remainder's seam must be typed (DB F10) | dereference of a bound id is always option-valued | §4.4, CHK-6 |
| Bind to contracts, not vendors (DB F11) | the `contract` section is the only coupling to any authority | §8 |
| One-way flow (CS C1) | machines are the only writers; derivations pure; bindings read-only | §6 |
| Tracked derivation (CS C2) | `derive` nodes over tracked reads | §5 |
| Server-cache split (CS C3) | `bound` is a class, not a corner of the store | §4.1 |
| Sealed async states (CS C4) | window status and intent instances are readable state | §7.4, §8.3 |
| Supersession & cancellation (CS C5) | per-intent `supersede` policy with key | §7.3 |
| Forms are a subsystem (CS C6) | not primitive; shown lowered onto cells/derives/intents | §12 U16 |
| Machine-shaped interaction state (CS C7) | `machine`/`mode`/`transition` nodes | §6 |
| Metaphysics free, invariants fixed (CS C8) | snapshot-per-step semantics; cells-shaped representation | §10 |
| Navigation as a value (CS C9) | nav stack is a typed cell; nav ops are machine effects | §9.1 |
| Restoration is declarative (CS C10) | class/scope determine survival; store-level fence | §9.2 |
| One-shot events as consumable state (CS C12) | `consumable` cells with drain semantics | §4.5 |
| Presence is its own class (CS U10) | `shared-ephemeral` class: liveness without authority or durability | §4.1 |

## 3. Shape of the IR

An IR instance is one document with six sections:

```
store      cells and collections, each with a storage class
derive     pure, tracked derivations (including guarded aggregates)
machines   modes, transitions, and the only effects in the system
intents    named writes against the contract, with echo and supersession
contract   the authority seam: projections, commands, refusals, windows
nav        routes, the stack type, restoration fences
```

### 3.1 Ownership and authority

- **Machines own all writes.** Assignments, mints, intent submissions,
  consumable pushes, and nav operations exist only as transition effects
  (CS C1). Derivations and bindings cannot write.
- **The authority owns bound truth.** Invariants, permissions, aggregate
  maintenance, and the outcome of every intent are authority decisions;
  the client holds a replica and a pending log, never authority (DB §1
  gap 4).
- **The renderer owns nothing.** It reads one consistent snapshot per
  step and emits named events into machines.
- **The host owns transports and persistence primitives** (storage,
  clocks, the network); the IR names what it needs from them and nothing
  more.

### 3.2 Nondeterminism is minted at the edge

One principle recurs through the design and is stated once here: every
source of nondeterminism — fresh ids, order values, timestamps, anything
random — is *minted at the submit edge* and carried as data (in payloads
and intent instances). Everything downstream (echoes, derivations, rebase
replay) is pure over its inputs. This is what makes optimistic replay
deterministic (DB §6.2), restoration faithful, and traces replayable.

## 4. The `store` section

Every piece of state is a declared **cell** (single value) or
**collection** (keyed set of records). Illustrative encoding:

```jsonc
{ "kind": "collection", "name": "posts", "class": "bound",
  "of": "post-summary", "key": "id<post>", "window": "feed-window" }

{ "kind": "cell", "name": "compose-draft", "class": "device",
  "type": "text" }

{ "kind": "cell", "name": "notice", "class": "ephemeral",
  "scope": "session", "consumable": true, "type": "text" }
```

### 4.1 Storage classes

| Class | Truth | Survives | Writes | Updates arrive unbidden | Failure surface |
|---|---|---|---|---|---|
| `ephemeral` | this client | `scope: view` or `session` | assignment | no | none |
| `device` | this client | restart (behind the fence, §9.2) | assignment | no (§15 two-tab question) | storage only |
| `bound` | an authority | as replica (resettable) | **intents only** | yes | refusal · unavailable · retraction |
| `shared-ephemeral` | nobody (peers, live) | not at all | assignment (published) | yes | loss is normal |

`convergent` is reserved as a per-field merge annotation on bound records
(DB §2.7), not a top-level class; draft 0 defines no merge kinds beyond
last-writer-wins.

The class table is the heart of the proposal: reads are identical across
all four rows (a binding cannot tell the classes apart), and every other
column differs. Assignment to a `bound` cell is *ill-formed* (CHK-1) —
the single rule that makes the query-cache school's signature bugs
(CS §3.1 era 1) inexpressible.

### 4.2 Types

The IR type grammar is closed and total: `bool`, `int`, `text`,
`id<entity>`, `option<T>`, `list<T>`, records with named fields, and
tagged unions (enums with payloads). Every type is codable — required for
device persistence, nav params, and window params (CHK-9, CHK-10). No
maps-as-types at the contract boundary; keyed data is a collection.

### 4.3 Identity and order

`id<entity>` values are client-mintable (`mint` effect, §6.2), opaque, and
validated by the authority on settlement (DB §3.3). Collections declare
their order: either an authority-provided field, or a client-mintable
dense order value (`mint-order between a b`) for reorderable collections
(DB fractional-indexing lineage; exercised in U9).

### 4.4 References and the seam

The only way to hold a relationship is an `id<entity>` value; local cells
holding ids into bound collections are the *local remainder seam*
(DB §3.9). Dereference is therefore **always option-valued**: the row may
have settled away, been retracted, or left the window. There is no
non-optional dereference to escape-hatch around (CHK-6). This one typing
rule is the proposal's answer to the field's dangling pointers.

### 4.5 Consumable cells

A cell marked `consumable` is a drain-on-read queue: pushes append,
a consuming binding or transition drains exactly once (CS C12 — the
Android events-as-state doctrine as a primitive). Checker: every
consumable must have at least one consumer (CHK-8).

## 5. The `derive` section

### 5.1 Derivations

`derive` nodes are pure expressions over tracked reads of cells,
collections, and other derivations. The expression language is total: no
general recursion, no effects, no clock, no randomness. Wall-clock time is
an ordinary host-ticked cell (`now`, declared granularity) so time-reads
are tracked like any dependency — but `now` is banned inside echoes
(§7.2), per §3.2.

### 5.2 Aggregates

`aggregate` nodes (`count`, `sum`, `min`, `max`, `exists`) over a bound
collection are well-formed **only** if the collection's window is declared
`total: true` for the aggregated scope (§8.3). Over a paginated prefix,
the aggregate is ill-formed (CHK-5) — the value must instead be an
authority-provided field (DB §3.6: a count over a window lies). This turns
the survey's most common silent lie into a compile error.

## 6. The `machines` section

### 6.1 Modes and transitions

A `machine` is instantiated with typed params and holds a current `mode`
(a tagged union, possibly with payload — impossible states are
unrepresentable, CS C7). `transition` nodes pattern-match on an input and
a mode, guard with a pure expression, and produce a new mode plus a list
of effects. Draft 0 keeps machines flat (no hierarchy; §15).

Inputs a machine can match on:

- named view events (opaque to this note; the binding surface delivers
  them with typed payloads);
- `settled(intent)`, `refused(intent, refusal)`, `unavailable(intent)` —
  the outcome events of §7.4;
- `retracted(collection)` and `window-status(window)` changes (§8.3–§8.4);
- drains of consumable cells it subscribes to.

### 6.2 Effects — the complete list

Transitions are the only effect sites in the IR. The full vocabulary:

| Effect | Legal target |
|---|---|
| `assign cell value` | ephemeral, device, shared-ephemeral cells |
| `mint id<entity>` / `mint-order between a b` | binds a fresh value into the transition's scope |
| `submit intent payload` | any declared intent |
| `push consumable value` | consumable cells |
| `nav push/pop/replace entry` | the nav stack (§9.1) |
| `consume` | drain a consumable the machine subscribes to |

There is no `assign` into bound state, no fetch, no timer, no imperative
escape. Timers, if wanted, are `now`-derived guards; fetching does not
exist as a concept — bound state is simply *there*, at some window status
(the survey's "uniform reads" made literal).

## 7. The `intents` section

### 7.1 Declaration

```jsonc
{ "kind": "intent", "name": "like", "command": "like-post",
  "payload": { "post": "id<post>" },
  "supersede": { "policy": "exhaust", "key": "payload.post" },
  "echo": [
    { "patch": "update", "in": "posts", "at": "payload.post",
      "set": { "viewer-has-liked": true,
               "like-count": "row.like-count + 1" } } ] }
```

An intent names a contract command (§8.2), fixes a payload type, declares
its **echo** — the optimistic patch-set — and its supersession policy.
Fresh ids and order values are minted at submit (§3.2) and travel in the
payload; the echo may reference them but never mint.

### 7.2 Echo semantics

An echo is a pure function from `(payload, current-base)` to patches on
bound state (`set`, `update`, `insert`, `remove` — patches are data,
DB §2.6). Purity is checked (CHK-3): no `now`, no mints, no reads outside
the snapshot. Echoes are *replayed* whenever the base changes under them
(§10) — this is what "systemic optimism" means operationally, and purity
is what makes replay sound. An intent may declare an empty echo:
pending-UI-only, the honest choice outside the optimism horizon (DB §3.2 —
uniqueness claims, permission-dependent outcomes).

### 7.3 Supersession

Every intent declares a policy over its in-flight instances sharing a
`key` (a pure expression over the payload): `parallel` (default),
`switch` (new supersedes old — the typeahead shape, though reads use
window re-parameterization for this), `exhaust` (drop while one is in
flight — double-tap protection, U2), `enqueue` (strict order). Lineage:
Rx flattening strategies, structured-concurrency cancellation (CS C5) —
promoted from folklore to a declared, checkable property (CHK-7).

### 7.4 Intent instances are state

Submitting mints an *instance*, readable in a system collection
`pending(<intent>)` with fields: payload, minted values, phase
(`in-flight | settling`), and key. Pending UI binds to this collection —
pending-ness has a type and a place (CS C4), never a hand-rolled boolean.
Outcomes arrive as machine inputs (§6.1); a refusal names a member of the
command's declared refusal union, and the machine must handle every member
of every union it can trigger, or route to a declared fallback (CHK-2 —
exhaustiveness that libraries encourage and this IR enforces).

## 8. The `contract` section

The only coupling to any authority (DB F11). Everything the client knows
about "elsewhere" is declared here; a fixture, a dev server, or a
production backend are interchangeable implementations of it.

### 8.1 Projections

Named, typed, optionally keyed read surfaces
(`projection feed-page: feed-value`,
`projection comment-thread(id<post>): thread-value`). The authority pushes
`(projection, key, revision, value)` updates; revisions are per-(projection,
key) monotone. There is no client-initiated fetch in the model; demand is
expressed by opening windows.

### 8.2 Commands and refusals

`command like-post(post: id<post>) refuses not-authorized | rate-limited |
not-found`. Refusal unions are closed and kebab-named; anything outside
the union arriving from the wire is `unavailable`, not a refusal —
transport failure and domain refusal never share a type (DB §3.5).

### 8.3 Windows

The partiality contract (DB F8): a window names a projection family, typed
parameters, and a totality claim.

```jsonc
{ "kind": "window", "name": "feed-window", "over": "feed-page",
  "params": {}, "total": false }
{ "kind": "window", "name": "thread-window", "over": "comment-thread",
  "params": { "post": "id<post>" }, "total": true }
```

Window status (`opening | live | error | detached`) is readable state —
the sealed async union (CS C4) lives here and on intent instances, *not*
on every read. Re-parameterizing a window is supersession with `switch`
semantics by construction (the typeahead answer, U4). Window params must
be codable so they can round-trip through the URL (CHK-9, U5).

### 8.4 Retraction

Rows may leave the replica because the authority revoked, deleted, or the
window moved (DB §3.5, §3.7). The IR makes retraction survivable rather
than solvable: dereference is option-valued always (§4.4), and machines
may subscribe to `retracted(collection)` for active responses (close the
sheet whose subject vanished). Edge semantics beyond this — references
*across* the window edge — are flagged open (§15), matching the survey's
finding that the field has no answer to steal.

## 9. Navigation, restoration, fences

### 9.1 The nav stack is a value

A typed stack of `(route, params)` entries in a session-scoped cell;
`nav` effects are the only writers; deep links deserialize into it and
back (CS C9's one success story — path-as-value — adopted wholesale).
Params obey the codable rule. Whether dialogs are stack entries or machine
modes is deliberately both-supported and unresolved (U17, §15).

### 9.2 Fences

The store declares one integer `fence`. Device cells and any persisted
machine modes are stored under it; on mismatch, they reset to initial
values (DB §3.8 — reset-on-schema-change as the honest prototype default).
Bound replicas are always reconstructible from the authority and carry no
fence of their own.

## 10. Operational model

Configuration and step rules — small, but load-bearing: the properties
below are the conformance surface.

```
C = ⟨B, P⟩            B  base: authoritative replica ⊎ local cells
                       P  pending: ordered log of intent instances
V(C) = B ⊕ echo(p₁) ⊕ … ⊕ echo(pₙ)     the view — computed, never stored
                       (echoes applied in submission order; derivations
                        and bindings read V only)

ASSIGN c v      c local            B' = B[c ↦ v]
SUBMIT i a      (ids pre-minted)   P' = P ++ [instance(i, a)]
UPDATE π k r v  r > rev(π,k)       B' = B[π,k ↦ v]          (else drop)
SETTLE p ok U                      B' = B ⊕ U atomically;  P' = P − p
SETTLE p refused/unavailable       P' = P − p;  outcome event enqueued
RETRACT R                          B' = B − R;  retraction event enqueued
```

Every rule ends by recomputing V and re-evaluating tracked derivations;
the renderer reads one V per step (snapshot isolation — CS C8's fixed
invariants with the metaphysics left free; Compose's snapshot system is
the mainstream precedent).

**Properties** (each becomes an executable conformance case, §14):

- **P1 Settlement convergence.** `P = ∅ ⇒ V = B`. No residual optimism:
  when nothing is pending, the screen is the authority's truth.
- **P2 Refusal soundness.** A refused or unavailable intent leaves `V`
  exactly as if it had never been submitted (guaranteed by echo purity +
  recomputation — rollback is not an operation, it is the *absence* of an
  echo).
- **P3 Rebase determinism.** `V` is a pure function of `(B, P)`. Any
  interleaving of UPDATE/SETTLE arriving between frames yields the same
  view for the same final `(B, P)`.
- **P4 Mint stability.** Replay never re-mints: ids and order values in
  `V` are constant across rebases for a given instance (§3.2).

## 11. Static checks

What the checker enforces — the "language earns its keep" list (CS §1.2),
each traceable to a survey finding.

| # | Rule | Discharges |
|---|---|---|
| CHK-1 | no `assign` into bound state; intents only | DB §5.3 write discipline |
| CHK-2 | every reachable refusal handled or routed to declared fallback | DB F2, CS C4 |
| CHK-3 | echoes pure: no `now`, no mint, snapshot reads only | DB §6.2 replayability |
| CHK-4 | inserts into bound collections use minted ids/order | DB F6 |
| CHK-5 | aggregates only over `total: true` windows | DB F7 |
| CHK-6 | dereference of `id<entity>` is option-typed, no escape | DB F10 seam |
| CHK-7 | every intent declares supersession (explicit `parallel` allowed) | CS C5 |
| CHK-8 | every consumable has ≥ 1 consumer | CS C12 |
| CHK-9 | window and nav params codable | CS C9, U5 |
| CHK-10 | device cells and persisted modes codable, under the fence | CS C10, DB §3.8 |
| CHK-11 | machine transition coverage: unhandled (mode, input) pairs reported | CS C7 |
| CHK-12 | refusal unions closed: wire refusals outside the union are `unavailable` | DB §3.5 |

## 12. Use-case walkthroughs

The adversarial examples, drawn from the catalog (CS §5). Each names the
trap and where the IR catches or dissolves it.

**U2 — like button.** `like` intent (§7.1): echo flips the flag and
increments the count *against the current base*; `exhaust` on
`payload.post` absorbs double-taps; `rate-limited` refusal must be handled
(CHK-2) — e.g. a transition pushing the `notice` consumable. Settlement
carries the authority's count; P1/P2 guarantee the counter cannot drift
and a refusal leaves no trace. The trap (lying counters, hand rollback) is
structurally gone.

**U4 — typeahead.** The query is a window parameter; typing
re-parameterizes; `switch` semantics drop the stale response by
construction (no `switchMap` recipe to know). Empty-vs-loading-vs-no-rows
is the window status union plus a derivation over the collection — three
distinct, bindable facts.

**U6 — wizard.** One machine; modes are a tagged union carrying each
step's accumulated data — `hasSkippedStepThree` booleans cannot exist
(CHK-11 reports unhandled combinations instead). Mark the machine's mode
persisted-to-device and resume-after-kill follows from §9.2's fence rules.

**U9 — drag to reorder.** The 120 Hz gesture lives in an ephemeral cell;
drop mints an order value `between` neighbors (§4.3) and submits a
reorder intent whose echo moves the row. Gesture state and authoritative
order never share a cell, which was the whole trap.

**U10 — presence.** A `shared-ephemeral` collection keyed by peer;
assigning your entry publishes it; peers' entries appear and expire.
No intents, no refusals, no durability — forcing presence through the
bound machinery is now a type error rather than a smell.

**U14 — undo.** Local undo: a machine over ephemeral snapshots — ordinary.
Authoritative undo: there is deliberately no primitive; it is an explicit
inverse intent with its own refusals (per Figma's account of multiplayer
undo). The IR refuses to promise what the authority may refuse.

**U16 — server-validated form.** No form primitives: fields are ephemeral
cells, dirty/validity are derivations, submit is an intent whose refusal
union includes field-addressed variants; the machine maps refusal payloads
back onto field-error cells. The lowering is mechanical — evidence for
§15's question of whether it deserves sugar, while proving the core needs
nothing new.

**U20 — two tabs.** Bound state converges through the authority — each tab
is just another client, and P1–P3 apply per tab. Device cells are the
honest gap: draft 0 gives them per-instance isolation and flags
cross-instance semantics open (§15) rather than pretending.

## 13. Boundary effects

- **Renderer:** unchanged responsibilities — read one snapshot per step,
  emit named events. Nothing in this proposal is renderer-visible except
  that pending instances, window statuses, and consumables are ordinary
  bindable state.
- **Host:** must supply durable storage (device class, fence-keyed), a
  ticking `now`, and a transport that delivers contract traffic; nothing
  else.
- **Authority:** implements the contract — serves projections, applies
  commands, answers with declared refusals, pushes updates and
  retractions. A scripted fixture and a live server (Spock being the
  reference case) are interchangeable behind it; conformance cases run
  against the fixture (§14).

## 14. Conformance case seeds

Candidate executable evidence before acceptance:

1. P1: submit → settle-ok → view equals base exactly.
2. P2: submit → refuse → view byte-equal to never-submitted.
3. P3: same `(B, P)` reached via different UPDATE/SETTLE interleavings →
   identical views.
4. P4: rebase across three base changes → minted ids/order stable.
5. CHK-5: aggregate over non-total window rejected; over total window,
   value matches authority after settlement.
6. Supersession: `exhaust` drops the second submit; `switch` supersedes;
   instance collections reflect it.
7. Retraction while referenced: dereference yields none; subscribed
   machine receives the event; no crash path exists.
8. Fence bump: device cells reset; bound replica rebuilds; ephemeral
   unaffected.

## 15. Open questions

- **Dialogs:** stack entries or modes? Both are expressible; restoration
  argues for entries, locality for modes (U17).
- **Machine hierarchy:** flat modes will bloat at stress-test scale;
  statechart-style nesting is the known remedy and a known ceremony risk
  (CS C7).
- **Device state across instances** (U20): last-writer-wins via storage
  events, or explicit merge? Deferred with per-instance isolation as the
  draft default.
- **Convergent leaf fields:** which merge kinds beyond LWW earn a place
  (counter, set-union, text)? Reserved, not designed.
- **Forms sugar:** the U16 lowering is mechanical but verbose; whether the
  surface language (not this IR) owns a form idiom remains open (CS §7).
- **Window-edge references:** option-valued dereference makes edges
  survivable; whether an edge *policy* (pin referenced rows? widen the
  window?) belongs in the contract is unresolved — the one place this
  proposal knowingly leads the field with no precedent to lean on
  (DB §3.7).

## Appendix A. Node-kind index

| Kind | Section | One-line semantics |
|---|---|---|
| `cell` | store | single typed value; class-governed writes; optional `consumable`, `scope` |
| `collection` | store | keyed records; class-governed; `key`, order, optional `window` |
| `derive` | derive | pure tracked expression over cells/collections/derives |
| `aggregate` | derive | count/sum/min/max/exists; requires total window |
| `machine` | machines | typed params + mode union + transitions |
| `transition` | machines | (mode, input, guard) → (mode', effects) |
| `intent` | intents | command binding + payload + echo + supersession |
| `projection` | contract | typed, optionally keyed, revisioned read surface |
| `command` | contract | named write with closed refusal union |
| `window` | contract | projection family + typed params + totality claim |
| `route` | nav | named route with codable params; stack entries reference it |

## Appendix B. Illustrative instance — the like-button slice

Non-normative encoding of the U2 walkthrough, end to end:

```jsonc
{
  "fence": 3,
  "store": [
    { "kind": "collection", "name": "posts", "class": "bound",
      "of": "post-summary", "key": "id<post>", "window": "feed-window" },
    { "kind": "cell", "name": "notice", "class": "ephemeral",
      "scope": "session", "consumable": true, "type": "text" }
  ],
  "contract": {
    "projections": [
      { "name": "feed-page", "value": "feed-value" } ],
    "commands": [
      { "name": "like-post", "payload": { "post": "id<post>" },
        "refuses": ["not-authorized", "rate-limited", "not-found"] } ],
    "windows": [
      { "name": "feed-window", "over": "feed-page", "params": {},
        "total": false } ]
  },
  "intents": [
    { "name": "like", "command": "like-post",
      "payload": { "post": "id<post>" },
      "supersede": { "policy": "exhaust", "key": "payload.post" },
      "echo": [
        { "patch": "update", "in": "posts", "at": "payload.post",
          "set": { "viewer-has-liked": true,
                   "like-count": "row.like-count + 1" } } ] }
  ],
  "machines": [
    { "name": "post-card", "params": { "post": "id<post>" },
      "modes": ["idle"],
      "transitions": [
        { "in": "idle", "on": { "event": "tap-like" },
          "do": [ { "submit": "like",
                    "payload": { "post": "param.post" } } ] },
        { "in": "idle",
          "on": { "refused": "like", "refusal": "rate-limited" },
          "do": [ { "push": "notice",
                    "value": "Couldn't like this post. Try again." } ] },
        { "in": "idle",
          "on": { "refused": "like", "refusal": "*" },
          "do": [ { "push": "notice", "value": "Something went wrong." } ] }
      ] }
  ]
}
```

Expressions appear as strings here for readability; within this proposal they
are structured expression nodes in the total language of §5.1. The wildcard
refusal arm is the declared fallback CHK-2 accepts.
