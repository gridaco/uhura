# Client state architecture in the wild: ecosystem patterns and use cases

- **Status:** Non-normative research input
- **Method:** Desk survey of public frameworks, libraries, official
  architecture guidance, and community practice, as of July 2026.
- **Companion:** [Database-bound state in client applications](db-bound-state-survey.md)
  is the deep dive on one state domain (server-owned data); this note
  surveys everything around it. The
  [application-scale stress test](application-scale-stress-test.md) is the
  companion requirements corpus at feature scale.
- **Follow-up:** [A class-differentiated state IR](state-ir-proposal.md)
  (draft 0) proposes an IR that discharges this survey's findings.
- **Destination:** Uhura Working Group

The companion survey studied how client stores bind to database authority.
This note studies the rest of the iceberg: how working front-end developers
actually structure client state, in the named stacks they recognize, hire
for, and argue about — "React + TanStack Query + Zustand," "Flutter +
Riverpod," "Compose + ViewModel + StateFlow." Three outputs: a shared,
front-end-recognizable vocabulary for the problem (§1–§2), the
per-ecosystem patterns with their known strengths and pathologies (§3),
and the cross-ecosystem convergences (§4) plus a catalog of concrete use
cases (§5) that any state model must serve.

As with the companion, this note proposes **no syntax** and evaluates no
in-house system. It is a map of the battlefield, drawn so that a front-end
developer can locate their own experience on it in one read.

## 1. The problem, in front-end terms

A client application is a coordination problem across five clocks that
tick independently:

1. **user input** (events arrive whenever they arrive, including during 2–5);
2. **rendering** (frames must be produced from *some* consistent snapshot);
3. **asynchronous I/O** (responses arrive out of order, late, or never);
4. **the authority** (someone else's write can invalidate anything, anytime);
5. **the lifecycle** (tabs suspend, apps are killed and restored, routes
   unmount mid-flight).

Every pattern in this survey is a discipline for keeping those clocks from
corrupting each other. The disciplines differ; the clocks never do.

### 1.1 The state domains

Practice — not theory — has factored client state into recognizable
domains. Every mature stack answers each domain with a different tool,
which is itself the finding: *no shipping ecosystem treats state as one
kind of thing.*

| # | Domain | Contents | Typical owner today | Signature bug |
|---|---|---|---|---|
| D1 | **Server-cache state** | remote records rendered locally | query cache / sync engine (companion survey, entire) | staleness; duplication; lying counters |
| D2 | **Shared app state** | session, viewer, theme, cart, feature flags | global store (Zustand/Pinia/ViewModel) | who may write it; init-order races |
| D3 | **Interaction & machine state** | open/closed, hover, drag, edit-mode, playback, wizards | component-local state; statecharts at the hairy end | impossible-state combinations of booleans |
| D4 | **Form state** | values, dirty/touched, validation, submission phase | a dedicated form library, in every ecosystem | validation timing; server-error mapping |
| D5 | **Route & navigation state** | URL, back stack, dialogs, deep links | the router | state that should be in the URL and isn't; back-button breakage |
| D6 | **Device-persistent state** | drafts, preferences, caches that survive restart | localStorage/SQLite/SavedState | schema drift; restoration gaps |
| D7 | **Derived state** | anything computable from D1–D6 | memos/selectors/signals | stored-instead-of-derived; stale derivations |

Two boundaries dominate the bug statistics. The D1↔D2 boundary produced
the defining architectural correction of the last decade ("server state is
not client state" — §2, §3.1). The D3↔D1 boundary — local interaction
state referencing remote records — is the companion survey's
"local remainder" (§3.9 there), where the dangling pointers live.

### 1.2 What "trying to model" means here

For the working-group purpose, a use case is *modeled* when its state
shape, its lifecycle, and its failure modes are expressible such that the
signature bugs of §1.1's table become either inexpressible or visible.
The catalog in §5 lists the concrete use cases; §4 lists the disciplines
the field converged on for them. The gap between "the discipline exists"
and "the discipline is checked" is where a language earns its keep — every
library below documents its discipline; almost none can enforce it.

## 2. The lineage in one page

Front-end developers inherit their patterns from a short, traceable
history. Naming it lets §3's stack cards stay brief.

- **Smalltalk MVC → MVP → MVVM.** The 2005 MVVM formulation (Gossman, for
  WPF/XAML) contributed *bindable view-models*: `INotifyPropertyChanged`
  for change tracking, two-way bindings, and `ICommand` — commands as
  first-class bindable objects whose *availability* (`CanExecute`) is
  derived state. XAML remains the most complete data-binding *language*
  ever mainstreamed (§3.9).
- **QML (2010)** made property bindings auto-recomputing expressions in a
  declarative language with first-class states and transitions — signals
  avant la lettre, still shipping in automotive and embedded.
- **Knockout → AngularJS (2010).** Two-way binding for the web. AngularJS's
  digest-cycle collapse — unconstrained two-way bindings over shared
  mutable models producing untraceable update storms — is the trauma that
  shaped the next decade.
- **React (2013) + Flux → Redux (2015).** The one-way rebellion: view as a
  function of state, mutations as dispatched actions, reducers as pure
  functions. Time-travel devtools made "state as data" tangible.
- **The Elm Architecture** is the formal pole of that school —
  model/message/update/view with *effects as data* (commands and
  subscriptions) — and the acknowledged ancestor of Redux, MVI on Android,
  Bloc in Flutter, and TCA on iOS. Harel's statecharts (1987) are the
  other formal pole, surfacing wherever interaction gets hairy (XState).
- **Observables and proxies** (MobX 2016, Vue's reactivity) kept the
  opposite metaphysics alive: mutable cells with tracked reads.
- **The signals renaissance** (Solid → Preact Signals → Vue refs → Svelte 5
  runes → Angular signals; a TC39 proposal to standardize the primitive)
  is that metaphysics refined: fine-grained reactive cells, auto-tracked
  derivations, effects at the edges. As of 2026 every major web framework
  except React has adopted it, and React's compiler is a response to the
  same pressure.
- **The server-cache split** (React Query/SWR era, 2019–) removed remote
  data from the general store — the correction the companion survey's
  §2.1 school institutionalized.
- **The server swing** (RSC + server actions, 2023–) moved reads and
  writes back into the framework/server, shrinking the client model — the
  re-derivation school (companion §2.3) arriving inside React itself.

## 3. Stack cards

Each card: the recognizable stack, where the §1.1 domains live, what it is
praised for, and what bites. "Bites" entries are the adversarial evidence —
the things a model must make impossible or visible.

### 3.1 React (web)

**Era 1 — Redux-everything (2015–2019).** One global store; server data
fetched in thunks/sagas and normalized in (normalizr); every request
hand-rolls `isLoading`/`error` flags.
*Praised:* one mental model; pure reducers testable; time travel.
*Bites:* the store becomes a hand-written, policy-free database — the
companion survey's query-cache and normalized-graph problems rediscovered
by every team independently; action/selector boilerplate; the flags
multiply (`isSubmitting`, `isRefreshing`, …) because pending-ness has no
type.

**Era 2 — the split stack (2019–, today's default).** The hiring-page
stack: React + TypeScript + **TanStack Query** (D1) + **Zustand** or Redux
Toolkit or Jotai (D2) + component state (D3) + **React Hook Form + Zod**
(D4) + a router (D5). Apollo replaces TanStack Query in GraphQL shops;
RTK Query in Redux shops.
*Praised:* each domain gets a fit-for-purpose tool; the D1 library
dissolves the flag boilerplate with typed query states; Zustand/Jotai cost
almost nothing.
*Bites:* five libraries, five reactivity models, glued in userland; the
seams (query cache ↔ store ↔ form) are untyped — nothing stops a Zustand
slice from caching a server record (D1 leaking into D2) and going stale;
nothing checks that pending/error states are actually handled; invalidation
remains hand-named (companion §2.1).

**Era 3 — the server swing (2023–).** Next.js App Router: server
components fetch (D1 moves server-side), server actions mutate +
`revalidateTag`, `useOptimistic` for the gap.
*Praised:* dramatically less client code for read-mostly pages.
*Bites:* two execution worlds in one file tree ("use client" folklore);
interactive islands still need the whole Era-2 stack; optimism is local
and lost on navigation.

**Corners worth naming.** MobX/Valtio — proxy observables inside React,
loved by those who have them, invisible to everyone else. **XState** —
statecharts for D3-heavy work (wizards, players, drag, connection
lifecycles): the only tool in the ecosystem where impossible states are
*structurally* impossible, and ceremony keeps it a specialist choice.
Recoil — archived; its atom model survives in Jotai.

### 3.2 Vue

**The stack:** Vue 3 Composition API (`ref`/`computed` — signals in
practice) + **Pinia** (D2; typed stores, devtools) + TanStack Query port
or Nuxt's `useAsyncData`/`useFetch` (D1, with server-rendered payload
hydration) + VeeValidate/FormKit (D4) + vue-router (D5).
*Praised:* fine-grained reactivity with the least ideology; single-file
components; `v-model` survives as *disciplined* two-way — sugar over a
prop/event pair, so the data flow stays one-way underneath (the field's
accepted rehabilitation of two-way binding: leaf-level only, desugarable).
*Bites:* reactivity-loss footguns (destructuring a `ref`, `.value` at the
seam); options-vs-composition split ecosystems; D1 outside Nuxt is BYO.

### 3.3 Svelte / SvelteKit

**The stack:** Svelte 5 runes (`$state`, `$derived`, `$effect` — signals
with compile-time sugar) for D2/D3/D7; SvelteKit `load` (D1 on
navigation) + form actions (writes, progressive enhancement) — the
re-derivation school (companion §2.3) as the framework's native posture.
*Praised:* the least ceremony per unit of state in the field; the compiler
does the bookkeeping.
*Bites:* live (non-navigation) server data is BYO; pre-runes store
patterns were folkloric and the migration is ongoing evidence that
cross-component state needs first-class primitives from day one.

### 3.4 Angular

**The stacks:** services + RxJS (`BehaviorSubject`-as-store, `async` pipe)
for the classicists; **NgRx** (actions/reducers/effects/selectors — Redux
with streams) for the enterprises; and since v16 the **signals turn**
(`signal`/`computed`/`effect`, signal inputs, zoneless change detection)
with RxJS interop at the edges. HttpClient + interceptors for D1; Angular
Query port exists; reactive forms are the strongest built-in D4 in any web
framework.
*Praised:* dependency injection that is real and testable; RxJS handles
genuinely hard async (typeahead, retry policies, websocket lifecycles)
declaratively; reactive forms treat validation as composed, typed
functions.
*Bites:* RxJS's learning cliff is the ecosystem's tax — powerful exactly
where it is least readable; NgRx ceremony; three reactivity systems
(zones, streams, signals) coexisting during the long migration.

### 3.5 The signals natives — Solid, Qwik, Preact

Solid: `createSignal`/`createResource` — note that the *async read* is a
framework primitive with built-in pending/error states, not a library.
Qwik: resumability — state and closures serialize into the HTML, so there
is no hydration replay; the store is designed for serialization from the
start. Preact Signals: the primitive extracted and made portable.
*Evidence value:* this school plus §3.2–§3.4's convergence plus a TC39
proposal means fine-grained cells + auto-tracked derivation + effects at
the edge is settled territory — D7 is a solved problem with a known shape.

### 3.6 Flutter

**Camp Riverpod:** a compile-safe provider graph (`Provider`,
`FutureProvider`, `StreamProvider`, `Notifier`) for D1/D2, with
**`AsyncValue<T>`** — a sealed loading/data/error union that the view must
exhaustively unfold — as the field's best mainstream artifact for "pending
is typed, renderable state."
**Camp Bloc:** events in, states out; sealed state classes (with freezed);
explicit transitions; the enterprise/testing choice — MVI-shaped, Elm
lineage visible. Cubit as its lighter half.
**Camp GetX:** state + DI + routing + snackbars in one package around
global mutable service locators; enormously popular, professionally
distrusted — the standing datapoint that *convenience beats discipline in
the mass market*, which any model's defaults must respect.
Shared: repository pattern + Dio for I/O; freezed/json_serializable
codegen for immutable models; drift for typed reactive SQLite (D6 as a
reactive read path); `go_router` for D5 after Navigator 2.0's declarative
nav-as-state API proved too raw to use directly (§4, C9); forms are
imperative islands (`TextEditingController`) — the weakest D4 of the major
ecosystems.
*Bites:* three incompatible camps fragment hiring and review; BuildContext
plumbing; nav state and dialog state chronically outside the state system.

### 3.7 SwiftUI (iOS/macOS)

**The stack:** `@State`/`@Binding` for D3 (value types — mutation is
localized by construction); `@Observable` (2023 Observation framework —
dependency-tracked, render-minimal; the ObservableObject/@Published +
Combine generation before it over-invalidated famously) for D2;
`@Environment` for DI; **NavigationStack with a path value** — the back
stack as an inspectable, codable *value*, giving deep links and state
restoration as serialization (the most complete nav-as-state in the
mainstream; §4 C9); **SwiftData `@Query`** (with `@FetchRequest` as the
Core Data ancestor) — a declarative live database query bound directly in
the view, CloudKit-mirrored: a bound-class read path (companion §2.5/§2.6)
shipping inside an OS vendor's UI framework; `.task(id:)` tying request
lifecycle to view identity with structured-concurrency cancellation — the
cleanest mainstream answer to the stale-response race (§4 C5).
**TCA (The Composable Architecture):** Elm on Swift — reducers, effects as
values, exhaustive tests, a store tree; chosen where correctness matters,
debated for ceremony (the XState/NgRx debate, replayed).
*Bites:* the MV-vs-MVVM holy war is an unresolved D2/D3 boundary dispute;
two-way bindings into nested value types get awkward; pre-@Observable
over-rendering trained a generation in superstition.

### 3.8 Jetpack Compose (Android)

**The official doctrine** — and Android is the only ecosystem whose vendor
publishes one — is UDF: a ViewModel exposes an immutable
`UiState` data class via `StateFlow`; events flow up as function calls;
`remember`/`mutableStateOf` for D3 via the **snapshot system** (Compose's
mutable state is transactional — concurrent snapshot isolation for UI
state, a quietly radical design); **Room** returning `Flow` — the local
database as a reactive read path (D6/D1 hybrid); **Paging 3** — windowing
as a typed library (`PagingSource`, `RemoteMediator`: the companion
survey's §3.7 partiality contract, as an API, and universally grumbled at
for its complexity — evidence that partiality resists library-shaped
solutions); **SavedStateHandle** — the only mainstream framework that
takes process-death restoration seriously (D6; §4 C10); Hilt for DI; MVI
variants (Orbit; Slack's Circuit; Cash App's Molecule) where teams want
Elm shape.
Also canonical: the official guidance to model one-shot events ("show a
snackbar once") *as state to be consumed*, not as fired events — the
platform itself concluding that event streams into UI are a bug factory.
*Bites:* `UiState` god-objects accrete; the state-vs-event debate
generated years of blog schism before the guidance settled it; Paging 3.

### 3.9 The XAML/MVVM and QML lineage (desktop and embedded)

WPF → WinUI/MAUI/Avalonia (+ CommunityToolkit.Mvvm's source-generated
observables, ReactiveUI): two-way bindings, `DataTemplate`s (type-directed
view selection — pattern matching in the view layer), and **`ICommand`
with `CanExecute`** — the command whose availability is bindable derived
state, a concept the web never fully re-adopted and forms reinvent as
"disable submit while invalid."
QML: bindings as auto-re-evaluating expressions in a declarative language
with first-class states/transitions; logic in JS islands.
*Evidence value:* a binding *language* with declared states shipped for
two decades in cars and cockpits; the lessons are (a) bindable
command-availability is a first-class concept worth keeping, (b)
unconstrained two-way binding over a mutable object graph is the known
failure mode (AngularJS re-proved it on the web), (c) view selection by
data type (DataTemplate) is load-bearing at scale.

### 3.10 The minimal-client school

htmx / Hotwire / Phoenix LiveView / Livewire, with Alpine.js for D3
sprinkles: D1 and D2 dissolve to the server (companion §2.3–§2.4); the
client keeps only interaction state. Included here because it is the
*control group for this entire note*: every gram of client-state machinery
must justify itself against "put it on the server and re-render."

Honorable lineage mentions: Ember Octane's tracked properties and Lit's
reactive properties (signals convergence, again); Backbone's models+events
(the ancestor everyone rewrote); jQuery's DOM-as-store (the null school —
state lived in class names and data attributes, and its illegibility is
what every framework since was founded to fix).

## 4. Convergences

Where independent ecosystems, under different vendors and metaphysics,
arrive at the same discipline, the survey treats it as evidence. Twelve
convergences, each with its meaning for a model.

**C1. Unidirectional data flow won.** Elm → Redux → MVI → Bloc → TCA →
Android's official UDF doctrine. Even two-way binding survivors (`v-model`,
SwiftUI `Binding`) are rehabilitated as *desugarable leaf-level* two-way
over a one-way core. → A model can assume one-way flow as ground truth and
treat leaf two-way as sugar, matching §3.2/§3.7 practice.

**C2. Fine-grained reactive derivation won D7.** Signals in five web
frameworks plus a TC39 proposal; tracked properties on desktop for two
decades (§3.9); Compose's snapshot reads. Derived-vs-stored is the
recurring bug (§1.1), and auto-tracked derivation is the cure the field
chose. → Derivation is settled; the open questions are elsewhere.

**C3. Server-cache state separated from app state.** The React Query
correction, Riverpod's async providers, Apollo-vs-store splits — every
ecosystem now isolates D1 behind a policy-bearing layer. The companion
survey is entirely about doing this properly. → D1 deserves a distinct
storage class, not a corner of the general store (companion §5.3 agrees
from the other direction).

**C4. Async states became sealed unions.** Riverpod's `AsyncValue`, the
Elm community's RemoteData pattern (NotAsked/Loading/Success/Failure),
query-state objects in TanStack Query, Suspense/error boundaries as the
structural variant. The anti-pattern it killed: parallel boolean flags.
→ Pending/failure as *typed, renderable, exhaustively-handled* state is
proven practice; a checker can enforce what Riverpod can only encourage.

**C5. The async lifecycle wants structure, not discipline.** The stale
typeahead response, the double-submit, the fetch outliving its view —
every ecosystem's folklore. The strong answers are structural: RxJS
`switchMap` (new intent cancels old), SwiftUI `.task(id:)` (lifecycle tied
to view identity), TanStack Query's keyed dedup, AbortController plumbing
(the weak manual form). → Cancellation and superseding-intent semantics
belong to the model, not the call site.

**C6. Forms are always their own subsystem.** React Hook Form, VeeValidate,
Angular reactive forms, FormKit, Flutter's controllers — no ecosystem
succeeded in treating form state as ordinary state. Registration, dirty
tracking, validation timing, focus management, and server-error mapping
(companion §3.5's refusal vocabulary, landing on fields) recur identically
everywhere. → D4 has a stable, known shape; a model may adopt or omit it,
but cannot pretend general state covers it.

**C7. Statecharts appear exactly where interaction is hairy — and only
there.** XState, Bloc's explicit transitions, TCA's enum state, QML
states. The ceremony repels the mass market (GetX's popularity is the
counter-evidence), but the wins are real where impossible states are
expensive. → Machine-shaped D3 is the highest-value, lowest-adoption
discipline: the cost is syntax-shaped, which is precisely what a language
(unlike a library) can change. (No syntax proposed here; the point is
where the leverage sits.)

**C8. Two metaphysics both shipped; neither won.** Immutable snapshots +
messages (Elm/Redux/TCA/UiState) versus mutable tracked cells
(MobX/Vue/signals/Compose snapshots). The field's verdict is that either
works *if* derivation is tracked (C2), mutation is disciplined (C1), and
effects sit at the edges. → The metaphysics is a free choice; the three
invariants are not.

**C9. Navigation-as-state is wanted, hard, and half-done everywhere.**
Navigator 2.0 shipped nav-as-declarative-state so raw that the community
built go_router to hide it; SwiftUI's NavigationStack path-as-value is the
success story; the web's URL is the oldest shared store (D5) and still
routinely desynchronized from app state; dialogs/back-button/deep-links
remain folklore. → Nav state is part of the state model or it is a bug
source; the field has one good existence proof (path as codable value)
and one cautionary tale (raw declarative nav APIs).

**C10. Restoration is the forgotten lifecycle.** Android's
SavedStateHandle and process death is the only rigorous mainstream story;
the web has bfcache surprises and scroll-restoration folklore; drafts
survive by ad-hoc localStorage. Users experience the gaps as data loss.
→ "What survives suspension/death, and in what schema" is a per-domain
property (D3 no, D4 usually, D6 by definition) worth making declarative —
the companion's lifetime axis (§3.8 there), landing on the client.

**C11. Identity and windows govern lists.** Keys/reconciliation in every
framework; virtualization (RecyclerView, `ListView.builder`, TanStack
Virtual) as the render-layer twin of the companion's partiality problem;
Paging 3 as the typed-window attempt that proves the API is hard.
→ Stable identity (companion §3.3: mint it client-side) and windowed
collections are one problem seen from two layers; model them once.

**C12. One-shot events are modeled as state.** Android's official
guidance; Compose's event-consumption idiom; Elm's everything-is-Msg;
"toast" semantics as the eternal bug. → Ephemeral effects (snackbar,
scroll-to, focus-this) are consumable state with defined read-once
semantics, not fired-and-forgotten events.

## 5. The use-case catalog

The concrete situations a state model must serve — chosen so that every
front-end developer has built each one, and each one exercises a distinct
combination of domains and disciplines. Companion-survey references mark
where the bound class (D1) carries the weight.

| # | Use case | Domains | Today's idiom | The trap it sets |
|---|---|---|---|---|
| U1 | Infinite feed + pull-to-refresh | D1 D7 | query cache w/ infinite query; Paging 3 | window merge on refresh; scroll restoration (C10); companion §3.7 |
| U2 | Like button | D1 | optimistic mutation recipe | counter aggregate lies (companion §3.6); double-tap idempotency |
| U3 | Comment composer | D1 D3 D6 | controlled input + mutation | draft survives navigation? minted id (companion §3.3); optimistic insert ordering |
| U4 | Search-as-you-type | D1 D3 | debounce + switchMap/keyed query | the stale-response race (C5); empty-vs-loading-vs-no-results triage (C4) |
| U5 | Filter panel with shareable URL | D5 D7 D1 | router query params + serializers | URL ↔ state desync; back button as filter-undo (C9) |
| U6 | Multi-step wizard | D3 D4 D6 | stepper component + form lib; XState at the serious end | impossible step/data combinations (C7); resume after kill (C10) |
| U7 | Autosaving editor | D1 D3 D6 | debounced PUT + dirty flag | conflict with a concurrent editor (companion §3.2); "saved" indicator honesty |
| U8 | Inline rename | D1 D3 | edit-mode boolean + mutation | refusal → revert-or-retry UX (companion §3.5); focus/selection across re-render |
| U9 | Drag to reorder | D3 D1 | dnd library + order mutation | ephemeral gesture vs committed order; fractional index minting (companion §3.3) |
| U10 | Presence / typing indicator | D2-ish, its own class | websocket + timeout maps | it is *not* database state — wrong store class if forced into D1 (companion §2.7 note on ephemerality) |
| U11 | Notification badge | D1 | pushed counter or poll | read-state sync across devices; badge vs list consistency |
| U12 | Auth session | D2 D6 | context/store + storage + interceptors | token refresh races (C5); gated routes during the pending gap (C4) |
| U13 | Upload with progress | D3 D1 | task object + progress events | cancel/retry/resume lifecycle; placeholder media until authority mints (companion §3.3) |
| U14 | Undo | D3 or D1 | command stack (local); rarely authoritative | local undo vs shared-truth undo diverge — Figma's multiplayer-undo discussion is the reference point |
| U15 | Offline note | D6→D1 | local-first engine or ad-hoc queue | the device→bound promotion; rebase on reconnect (companion §2.6, §6.3) |
| U16 | Server-validated form | D4 D1 | form lib + error mapping | refusal vocabulary → per-field placement (C6 ∩ companion §3.5) |
| U17 | Modal / dialog flows | D5 D3 | portal + open-state; sometimes route-bound | in the back stack or not? dismissal semantics; state left behind on dismiss |
| U18 | Live dashboard | D1 D7 | polling or standing queries | derived aggregates over live windows (companion §3.1, §3.6) |
| U19 | Skeletons & perceived speed | D7 (C4) | Suspense/AsyncValue unfolds | layout shift; pending state that flickers on fast networks |
| U20 | Same user, two tabs | D2 D6 | BroadcastChannel/storage events; usually nothing | the forgotten topology: the user *is* a distributed system before the server is involved |

Six of these deserve a paragraph, because they are the standard
interview-question shaped traps and each one indicts a specific gap:

- **U4 (typeahead)** is the canonical async-lifecycle bug: the field's
  correct answers are all *supersession* semantics (new intent cancels
  old), which only RxJS and structured concurrency express directly.
  Everyone else documents a recipe.
- **U6 (wizard)** is where boolean-soup D3 collapses: `canGoNext`,
  `isStepTwoValid`, `hasSkippedStepThree`. It is the statechart poster
  child (C7) and the restoration poster child (C10) at once.
- **U9 (reorder)** cleanly splits ephemeral from authoritative state: the
  drag position is D3 at 120 Hz; the committed order is D1 with a minted
  position value. Stacks that conflate the two get janky *and* wrong.
- **U10 (presence)** is the catalog's category test: it is shared and
  live but *not durable* and *not authoritative* — a storage class of its
  own (ephemeral-shared), and forcing it through the D1 machinery is a
  known smell.
- **U14 (undo)** distinguishes state-restoring undo (trivial with
  snapshots, D3) from *intent-inverting* undo against a shared authority
  (hard, needs inverse commands, per Figma's account). A model that
  conflates them promises what it cannot keep.
- **U20 (two tabs)** is the cheapest honesty check available: any model
  claiming to handle distribution should first survive the same user
  opening a second tab.

## 6. What this means for a state-and-binding model

Read §4's convergences against §5's catalog and the shape of the demand
becomes explicit — stated here as concepts, not design:

1. **The domain taxonomy is real.** Every mature ecosystem factored state
   into roughly D1–D7 and tooled them separately; the companion survey's
   storage classes are D1/D6/D3 seen from the authority side. A model
   should name the domains rather than average over them — the field's
   single-store eras (Redux-everything, GetX) are its cautionary tales.
2. **Three invariants, free metaphysics** (C1/C2/C8): one-way flow,
   tracked derivation, effects at the edges. Snapshots vs cells is taste;
   the invariants are not.
3. **Async is state, exhaustively handled** (C4/C5): sealed
   pending/success/refused/failed unions plus supersession and
   cancellation semantics tied to view/intent lifetime. The field enforces
   this by convention at best; it is checkable.
4. **The highest-leverage unclaimed ground is machine-shaped interaction
   state** (C7): proven wins, adoption blocked on ceremony — a
   cost a language can restructure and a library cannot.
5. **Navigation, restoration, and one-shot effects are state model
   citizens** (C9/C10/C12), with one good existence proof each
   (path-as-value; SavedStateHandle; events-as-consumable-state).
6. **Identity and windows unify the render layer with the sync layer**
   (C11 ∩ companion §3.3/§3.7): mint identity client-side, declare
   windows, and both layers' folklore collapses into one contract.
7. **Forms and presence are honest special cases** (C6, U10): one has a
   stable known shape to adopt or exclude explicitly; the other is a
   distinct ephemeral-shared class that must not be forced through the
   authoritative machinery.
8. **The mass market chooses convenience** (§3.6 GetX, §3.1 era-2 glue):
   defaults decide adoption. Disciplines that read as ceremony get
   routed around — the model's *default* path must be its safe path.

## 7. Open questions for the working group

- **How much D4 (forms) belongs in-model?** The shape is known (C6);
  the cost of owning it is large; the cost of excluding it is that every
  prototype contains a form by its second screen.
- **What are dialog/back-stack semantics as state?** (C9, U17) The field
  offers one good value-shaped answer and many bad API-shaped ones.
- **Is restoration declarative and default?** (C10) Per-domain
  survivability as a property, with process death as the test.
- **Where does ephemeral-shared state live?** (U10) Presence-class data
  needs liveness without authority — neither the bound class nor local
  state fits.
- **Can supersession (C5) be inferred from intent structure,** or is it
  declared? The typeahead trap suggests inference is possible for reads;
  writes are less clear.
- **What is the two-tab story?** (U20) Even a prototype-scale model meets
  it the moment a preview link is opened twice.

## Appendix A. Survey corpus

React ecosystem: React, Redux/Redux Toolkit, normalizr, redux-saga,
TanStack Query, SWR, RTK Query, Apollo Client, Zustand, Jotai, Recoil
(archived), MobX, Valtio, XState, React Hook Form, Formik, Zod, Next.js
App Router (RSC, server actions, `useOptimistic`), Remix/React Router,
TanStack Router/Start/Virtual.
Vue: Vue 3 Composition API, Pinia, Nuxt (`useAsyncData`), VeeValidate,
FormKit, VueUse.
Svelte: stores, Svelte 5 runes, SvelteKit load/actions.
Angular: RxJS, NgRx (+ component-store), signals (v16+), reactive forms,
zoneless change detection.
Signals natives & standardization: SolidJS (`createResource`), Qwik
(resumability), Preact Signals, TC39 Signals proposal; Ember Octane
tracked properties; Lit.
Flutter: setState/InheritedWidget, Provider, Riverpod (`AsyncValue`),
Bloc/Cubit (+ freezed), GetX, Dio, drift, go_router / Navigator 2.0,
flutter_form_builder.
Apple: SwiftUI (`@State`, `@Observable`, Observation), Combine (legacy),
NavigationStack path, SwiftData `@Query`, Core Data `@FetchRequest`,
CloudKit mirroring, structured concurrency (`.task(id:)`), TCA
(Point-Free).
Android: Jetpack Compose (snapshot system), ViewModel + StateFlow +
UiState (official UDF and app-architecture guidance, including
events-as-state), Room + Flow, Paging 3, SavedStateHandle, Hilt, Orbit
MVI, Slack Circuit, Cash App Molecule.
Desktop/embedded lineage: WPF/XAML (INotifyPropertyChanged, ICommand /
CanExecute, DataTemplate), MVVM (Gossman, 2005), WinUI/MAUI/Avalonia,
CommunityToolkit.Mvvm, ReactiveUI, Knockout, QML (bindings,
states/transitions).
Minimal-client: htmx, Hotwire Turbo, Phoenix LiveView, Livewire,
Alpine.js; Backbone and jQuery-era DOM-as-store as ancestors.
Formal poles and essays: The Elm Architecture; RemoteData pattern (the
Elm community's "slaying a UI antipattern"); Harel statecharts (1987);
AngularJS digest-cycle retrospectives; React's "You Might Not Need an
Effect"; Figma's multiplayer account (undo under concurrency); Android
process-death and restoration guidance; the companion survey's corpus for
everything database-bound.
