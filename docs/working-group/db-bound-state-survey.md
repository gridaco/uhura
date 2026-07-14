# Database-bound state in client applications: a field survey

- **Status:** Non-normative research input
- **Method:** Desk survey of public systems, protocols, and engineering
  writing, as of July 2026. No benchmarks were run; claims about internal
  systems (Linear, Figma, Notion, Asana) rest on their public talks and
  engineering blogs.
- **Companion:** [Client state architecture in the wild](client-state-survey.md)
  surveys the general per-ecosystem client patterns (React + …, Flutter +
  …) surrounding the database-bound domain studied here, and carries the
  use-case catalog.
- **Follow-up:** [A class-differentiated state IR](state-ir-proposal.md)
  (draft 0) proposes an IR that discharges this survey's findings.
- **Destination:** Uhura Working Group

A data-binding language for prototypes owns a mutable store. Nothing about
that store says where its data lives: a counter, a draft, a feed page, and a
bank balance all read the same way. The question this note studies is what
happens when some of that data is *meant to be bound to a database* — truth
owned elsewhere, other writers present, invariants enforced by an authority
the client cannot see inside.

This note deliberately ignores the current shape of Uhura and its companion
systems, and it proposes **no syntax**. It surveys what shipping systems
actually do, sorts what is good from what is bad, and asks of each pattern
the working-group question: *has anyone lifted this into a declarative,
checkable form — and if not, why not?* The output is a taxonomy (§5) and a
set of modelability verdicts (§6) intended as raw material for a future
model of database-bound stores, not as a design.

## 1. The problem, stated without a framework

Bind a client store to a database and five gaps open between the store the
view reads and the store that owns truth. Every pattern in this survey is a
stance on these five gaps; no pattern escapes them.

1. **Latency.** Reads and writes take wall-clock time. Something must fill
   the gap: a spinner, stale data, or a guess.
2. **Partiality.** The client can never hold the whole database. Every
   client store is a *window*, and windows have edges: pagination frontiers,
   filter boundaries, permission horizons.
3. **Contention.** Other writers exist. Local state can be invalidated by
   someone else's write at any moment, whether or not the client is looking.
4. **Authority.** Invariants — uniqueness, balances, quotas, permissions —
   can only be enforced where the data is total. The client may *propose*;
   only the authority *disposes*. Any client-side certainty about the
   outcome of a write is a simulation.
5. **Lifetime.** The client store and the schema evolve on different clocks.
   A cached or replicated store can outlive the deploy that wrote it.

It is also useful to name the three paths any binding model must provide,
because systems differ mainly in how they wire these three together:

- the **read path** — how authority state reaches the client store;
- the **write path** — how client intent reaches the authority;
- the **repair path** — how the store is made right after the two disagree
  (a write refused, a race lost, a window shifted).

A final orientation point. The premise "data does not inherently mean
external" is itself a finding of this survey: the systems people praise make
*reading* bound data feel identical to reading local data, and the systems
people curse are the ones where remoteness leaks into every read site. Where
the schools genuinely differ — and must differ — is on the write and repair
paths. That asymmetry recurs throughout and is picked up as the central
claim of §5.3.

## 2. The schools of practice

Seven recognizable schools ship today. Each is described by mechanism,
strengths, and characteristic pathologies — the pathologies are the valuable
part, since they are what a model would have to prevent.

### 2.1 The query-cache school

**Representatives:** SWR, TanStack Query (React Query), RTK Query, Apollo
Client's document cache, tRPC paired with a query cache.

**Mechanism.** A client cache keyed by *request identity* (a query key, a
URL, a procedure + arguments). Reads render whatever is cached, then
revalidate — the school is literally named for HTTP's
`stale-while-revalidate` (RFC 5861). Writes are RPCs; after a write, the
application *invalidates* named keys, which refetch. Freshness is a policy
knob (`staleTime`, focus/reconnect revalidation), not a fact.

**What is good.**

- The server remains the only truth; the cache never pretends otherwise.
  Repair is trivial: throw the entry away and refetch.
- The mental model is small and composes with any backend. This is the
  field's default for a reason.
- Freshness-as-policy is honest about the latency gap: every read site
  tolerates staleness by construction.
- RTK Query's `providesTags` / `invalidatesTags` demonstrates that the
  write→read dependency map can be *declared as data* — a tag algebra
  connecting mutations to the queries they dirty — rather than living in
  imperative callbacks.

**What is bad.**

- The dependency map is hand-maintained. A forgotten invalidation is the
  signature bug of this school: the write succeeded, the screen lies. Even
  RTK Query's tags are coarse, and the documented list/item idiom (a
  synthetic `LIST` tag beside per-id tags) shows the algebra straining.
- No entity identity: the same record fetched by two queries exists twice.
  Update one copy and the other lies until its key is invalidated. Feed +
  detail-view divergence is the canonical form.
- Optimistic updates are a per-mutation, hand-written recipe: snapshot the
  cache, patch it, roll back on error, invalidate on settle. The inverse
  logic is written by hand, duplicated per mutation, and wrong exactly when
  it matters (partial failure, concurrent writes).
- Liveness is polling and focus heuristics. Nothing pushes.

### 2.2 The normalized-graph school

**Representatives:** Apollo Client's normalized cache, Relay, urql with
Graphcache.

**Mechanism.** Query responses are decomposed into an entity graph keyed by
`(type, id)`; queries *read through* the graph. One entity update reflects
in every view that references it. Relay is the strong form: a compiler knows
every fragment each view needs, and mutations carry declared cache-repair
instructions (`@appendEdge`, `@deleteRecord`, connection handlers) in the
query language itself.

**What is good.**

- Entity identity dissolves the duplication pathology of §2.1: values are
  stored once.
- Data dependencies are colocated with the view and statically knowable —
  Relay's compiler is the existence proof that "what this view reads" can be
  a checked artifact.
- Relay's mutation directives prove that *repair itself can be declared*:
  "this mutation appends to that connection" is data, not an imperative
  cache poke.

**What is bad.**

- The client cache becomes a shadow database with no schema authority, no
  invariants, and no owner. Partial entities (fields present only if some
  past query happened to select them) are a permanent hazard.
- Identity solves *value* staleness, not *membership* staleness. A freshly
  created entity does not know which lists it belongs to; every list is a
  query result, and mutations must name the connections they affect. "Why
  didn't my list update" is this school's signature bug — the invalidation
  problem of §2.1 reappears one level up, at collection membership.
- Cache eviction and garbage collection are manual and folkloric.
- Local-only state must be bolted on (client-side resolvers, reactive
  variables) and never feels native — an early sighting of the
  local-remainder problem (§3.9).
- The machinery cost is famously high; teams adopt Relay for the compiler
  and pay for it in ceremony.

### 2.3 The re-derivation school

**Representatives:** Remix / React Router loaders and actions, Next.js
server actions with `revalidateTag`/`revalidatePath`, SvelteKit load
functions and form actions; at the limit, HTML-over-the-wire systems — htmx,
Hotwire Turbo, Unpoly — where the "store" is the DOM itself.

**Mechanism.** After any write, throw client state away and *re-derive* the
visible page from the server. The page (or a tagged fragment) is the unit of
consistency. The repair path **is** the read path.

**What is good.**

- Correctness by re-derivation. There is no cache to corrupt, so there is no
  invalidation bug class — the school's whole bet is that the cheapest
  correct repair is "recompute everything in scope."
- Pending state is standardized and renderable (navigation states, fetcher
  states) rather than exceptional.
- Progressive enhancement falls out; the htmx position — the most
  sophisticated client cache is no client cache — is a real discipline, not
  a joke.

**What is bad.**

- Refetch amplification: one like-button costs the whole page's loaders.
  Tag-scoped revalidation reintroduces §2.1's naming problem at page scale.
- Every interaction pays a round trip; optimism must be reintroduced by hand
  (per-fetcher optimistic UI, `useOptimistic`) and is local, uncoordinated,
  and lost on navigation.
- The server cannot push; someone else's write is invisible until you
  navigate. Realtime is out of scope by construction.

The school matters to this survey mostly as a *control group*: it sets the
simplicity bar any richer binding model must beat (see §2.4 and §7, finding
12).

### 2.4 The server-resident-session school

**Representatives:** Phoenix LiveView, Laravel Livewire, Blazor Server;
Rails Turbo Streams as a hybrid.

**Mechanism.** Dissolve the problem: keep the authoritative *session* store
on the server (a process per socket), send events up and minimal rendered
diffs down. The client holds a DOM and a patch applier. There is no client
store, therefore no binding problem.

**What is good.**

- The five gaps collapse to one (latency), and it is honest: every
  interaction visibly costs a round trip.
- Reads on the server are local; realtime push is native (any server-side
  event can re-render any client); there is one language and one store.
- Per-interaction server rendering keeps authority, invariants, and
  permissions in exactly one place.

**What is bad.**

- Interaction latency is bounded below by RTT, always. Optimism is
  structurally unavailable (escape hatches are JS bolt-ons).
- Per-client server state costs memory, pinning, and failover complexity.
- Offline is impossible. Transient view state (focus, scroll, half-typed
  input) must survive server re-renders, which is a permanent source of
  leaks in the abstraction.

Like §2.3, this is a control group: it demonstrates that *no client store*
is a coherent, shippable answer, and therefore that a client-store model
justifies itself only by beating this school on latency-feel and offline.

### 2.5 The reactive-database school

**Representatives:** Meteor (the ancestor — minimongo, DDP pub/sub, methods
with latency compensation, 2012), Firebase Realtime Database and Firestore,
RethinkDB changefeeds (dead since 2016, widely mourned and widely copied),
Supabase Realtime (Postgres CDC over websockets), Convex, InstantDB,
Triplit.

**Mechanism.** The database is the API. Clients declare *standing queries*
(subscriptions); the platform maintains them incrementally and pushes
changes. Writes are either direct store writes guarded by rules (Firebase)
or named server functions (Meteor methods, Convex mutations, InstantDB
transactions).

**What is good.**

- The invalidation problem *disappears*. A subscription is a
  continuously-repaired read path; there are no keys to forget.
- Optimism is systemic, not per-mutation: Meteor simulated methods client
  side and reconciled against server results in 2012 ("latency
  compensation"), and nearly every later system rediscovered exactly this.
- Convex's structural bet is instructive: queries and mutations are
  *deterministic* server functions, which is what makes automatic
  subscription, caching, and transactional retry tractable. The constraint
  is the feature.
- Liveness is native, including from other writers.

**What is bad.**

- The query language must be constrained enough to maintain incrementally.
  Firestore's famous limits — indexed predicates only, no joins, restricted
  disjunctions — are not stinginess; they are the price of turning every
  query into an incrementally-maintainable stream. Every school-5 system
  pays it somewhere.
- Authority migrates into rules DSLs (Firebase security rules, Firestore
  rules, InstantDB's permission language). These grow into weak, awkward
  languages: no aggregates, limited reads of other records, so schemas
  contort (duplicating an owner id onto every document is the classic).
- Invariants that rules cannot express need server functions anyway
  (Firestore's transactional counters plus Cloud Functions), so authority
  ends up fragmented across three places: rules, functions, and hopeful
  client code.
- Vendor coupling is existential. Parse (announced 2016, gone 2017) and
  Realm / MongoDB Atlas Device Sync (deprecated 2024) both took entire app
  architectures down with them. When the store is the API, the store's
  vendor is a dependency of every screen.

### 2.6 The replica-and-log school (sync engines)

**Representatives:** Replicache → Zero (Rocicorp; Zero reached 1.0 in June
2026), ElectricSQL (rebuilt 2024 as a read-path "shape" sync layer over
Postgres, stable since 2025), PowerSync (declared sync buckets into client
SQLite plus a write-back queue), LiveStore (event-sourced client SQLite),
TanStack DB (client collections with differential-dataflow live queries and
an explicit optimistic overlay; in beta), WatermelonDB (a deliberately
minimal pull/push protocol with per-column merges). CouchDB/PouchDB is the
grandparent (MVCC revision trees, conflicts surfaced as data). Product
proofs: Linear's sync engine (workspace object graph in IndexedDB, delta
packets, a transaction queue with rollback), Figma multiplayer
(server-authoritative per-property last-writer-wins) and Figma's LiveGraph
(query invalidation fed by CDC), Notion's op-queue-plus-cache architecture.

**Mechanism.** The client holds a *partial replica* (rows, shapes, buckets,
collections) plus a *log of pending intents*. Reads are local against the
replica — fast and offline-capable. Writes append a named mutation to the
log, apply speculatively to the local view, ship to the authority, and the
client *rebases* still-pending mutations onto each authoritative snapshot
that arrives. The lineage is git (`pull --rebase`) by way of game netcode:
client-side prediction with server reconciliation, which Replicache cites
explicitly.

**What is good.**

- The latency gap is closed at the *store* level, once, rather than by
  per-mutation recipes. Optimism, rollback, and merge become engine
  semantics with defined behavior instead of app folklore.
- Replicache's mutator design is the cleanest statement in the field of
  what a write should be: a *named, serializable, replayable function*, run
  speculatively on the client and authoritatively on the server. Mutations
  as data. (Notably, this is the command/event-sourcing tradition arriving
  from the other direction.)
- Reads and writes decouple cleanly. Electric's stance — sync the read path
  over plain HTTP, let writes be your ordinary API — demonstrates that the
  two paths are separable problems, and TanStack DB consuming Electric
  shapes as one collection type among several shows the read path can be a
  commodity.
- Partiality gets *named artifacts*: PowerSync's sync-rule buckets and
  Electric's shapes are declared, inspectable definitions of "which subset
  of truth this client holds." The window stops being an accident of query
  parameters.
- Offline falls out of the architecture instead of being a feature.

**What is bad.**

- Partial replication is the hard problem, and everyone says so. Linear's
  public war story arc runs from "bootstrap the whole workspace" to years of
  partial-sync engineering as workspaces outgrew clients. What subset, who
  decides, how it composes with permissions and joins — every system has
  folklore here; none has clean semantics.
- Permissions become data-shaped and must be enforced at *sync* time, not
  endpoint time. Zero expresses read permissions as queries; Electric and
  PowerSync lean on Postgres row-level security or rule definitions. This is
  §2.5's rules-DSL problem again, now with replication consistency attached.
- Rebase requires mutations to be *pure and replayable* against a store
  snapshot — a language-shaped constraint that JavaScript cannot check, so
  discipline substitutes for types. The double-implementation problem
  (optimistic logic re-implements server logic) is solved only by literally
  sharing mutator code across client and server, or by sharing semantics.
- Aggregates over a partial replica silently lie: a count over a window is
  not a count over truth (§3.6).
- Schema evolution meets replicas in the wild: version fences and forced
  resets (Replicache's `schemaVersion`) are the state of the art, which is
  to say the state of the art is "start over" (§3.8).
- CouchDB's lesson still stands as the cautionary limit: surface divergence
  as data (`_conflicts`, deterministic-but-arbitrary winners) and every
  reader inherits distributed-systems homework.

### 2.7 The convergent school (CRDTs, local-first)

**Representatives:** Automerge (and Automerge Repo), Yjs, Jazz, Evolu;
Ditto in industrial edge sync. The ideology anchor is Ink & Switch's
"Local-first software" essay (2019, Kleppmann et al.); Google Docs' OT
lineage is the centralized ancestor.

**Mechanism.** The data type itself guarantees convergence: operations
commute (or merge deterministically), so any set of replicas that has seen
the same operations agrees, with no authority required. Sync is gossip.

**What is good.**

- Offline and collaboration are the *native* case, not features. There is
  no rollback concept because there is nothing to roll back — everything
  merges.
- Merge granularity is per-field or finer (sequence CRDTs for text), which
  is exactly right for concurrent editing of different parts of a document.
- Jazz shows the school productizing: permissions (groups), sync, and
  storage packaged around convergent values rather than left as an
  exercise.

**What is bad.**

- Convergence is not correctness. Global invariants — uniqueness, budgets,
  seat-booking, "balance never negative" — are inexpressible without
  reintroducing an authority, at which point the school's premise is gone.
- Merge semantics are per-type and subtle (list moves, counters,
  undo); history management and compaction are real costs.
- The school's honest domain is *documents* — text, canvases, whiteboards —
  not records with invariants. Figma's verdict is the one to remember: a
  server exists, so per-property last-writer-wins with server authority
  ("CRDT-inspired, not CRDTs") is simpler and sufficient.

For a survey premised on a *database as authority*, the field's quiet
consensus is that convergent types apply to leaf **values** (one
collaboratively edited rich-text field) rather than to the record graph.

### 2.8 Locating the three familiar moves

The three moves a TSX developer reaches for map onto this landscape
exactly:

1. *"SWR plus a dedicated action that refreshes the key"* is school §2.1,
   and inherits its signature bug: the hand-maintained dependency map
   between writes and reads.
2. *"An optimistic store wrapper"* is not a school but the cross-cutting
   problem of §3.2 — and the field's evidence is that per-mutation optimism
   does not compose. It wants to be systemic (an overlay plus rebase, as in
   §2.6) or absent (pending UI, as in §2.3/§2.4).
3. *"A reusable store auto-synced with the server at data/column level,
   Firebase-style"* is schools §2.5/§2.6, and the real content of the move
   is everything it leaves unspecified: which subset (§3.7), whose
   authority (§3.5), what granularity (§3.4), and what happens on refusal
   (§3.2).

That the three intuitive moves land in three different schools with three
different failure modes is the strongest argument that the *concepts* need
naming before any surface design: the moves feel interchangeable and are
not.

## 3. Cross-cutting problems

Every school answers the same nine problems somewhere — in its engine, its
conventions, or its bug tracker. These, not the schools, are the stable
structure of the field: schools come and go, the problems recur verbatim.
Each subsection ends with a modelability note, collected in §6.

### 3.1 Invalidation and liveness

The field forms a ladder, each rung trading hand-maintenance for machinery:

1. **Manual keys** (SWR, TanStack Query): the app names what a write dirties.
2. **Declared tag algebra** (RTK Query): the naming becomes data, checkable
   in principle, still coarse.
3. **Scope re-derivation** (loaders; `revalidateTag`): dirty the page, not
   the entry.
4. **CDC row streams** (Supabase Realtime, Electric shapes, Figma
   LiveGraph): the database's replication log drives invalidation; nothing
   is forgotten, but granularity is the row, and query-shaped views must be
   re-derived from row changes.
5. **True incremental view maintenance** (Materialize and Feldera
   server-side; TanStack DB's differential dataflow client-side; Zero's and
   Convex's constrained query engines): standing queries updated
   incrementally, joins and aggregates included — the CS floor of the whole
   problem.

Two orthogonal observations. First, *what travels* differs: snapshots,
diffs, row events, or semantic operations — and systems that confuse events
with state (GraphQL subscriptions used as a cache-update mechanism) push
reconstruction onto every consumer, which is why "live queries" keep being
reinvented above subscriptions. Second, the *liveness channel* can be
content-free: Replicache's "poke" (a push that says only "something
changed, pull when convenient") decouples the notification transport from
the data protocol and composes with anything.

**Modelability.** Rungs 2 and 4 are proven declarable (tags; shapes/buckets
plus CDC). Rung 5 is proven *buyable* but is a research-grade component —
the systems that offer it all constrain the query language to keep it
tractable (§3.7, §6.2). Nobody ships arbitrary-SQL live queries.

### 3.2 Optimism and repair

Five repair mechanisms exist in the wild:

| Mechanism | Home school | Cost carried by |
|---|---|---|
| Refetch (discard and re-read) | §2.1, §2.3 | latency, amplification |
| Targeted patch (declared or manual cache edit) | §2.1, §2.2 | correctness of the inverse map |
| Rebase (replay pending intents on new truth) | §2.6 | purity/replayability of mutations |
| Merge (convergent types) | §2.7 | invariant expressiveness |
| Re-render (server owns the view) | §2.4 | RTT per interaction |

Optimism proper — showing an outcome before authority confirms it — then
divides by *who implements the guess*: nobody (pending UI), the app
(per-mutation snapshot/patch/rollback recipes), the engine (speculative
application of the same named mutation that will run at the authority), or
the type (CRDT merge, where "guess" and "truth" are the same thing).

Three field lessons stand out:

- **The optimism horizon.** Some outcomes are safe to fake locally: the echo
  of your own field edit, an increment, an append. Some are not: uniqueness
  claims, permission-dependent results, anything read-back-dependent.
  Shipping systems encode this horizon in folklore ("don't optimistically
  render the payment"); no system in this survey types it.
- **Pending and failed are renderable states, not exceptions.** Everyone
  converges here: React's `useOptimistic` and transitions, Remix fetcher
  states, Replicache's pending-mutation list, TanStack DB's overlay states.
  A binding model that hides pending-ness behind a boolean has already
  failed the field test.
- **The double-implementation problem.** Optimistic logic re-implements
  authority logic. The field's answers: share the code (Meteor isomorphic
  methods; Replicache/Zero mutators run verbatim on both sides), share the
  semantics (Convex's determinism), or abstain (re-derivation). Writing the
  guess and the truth in two languages and hoping is the known-bad answer —
  and it is also the default one.

**Modelability.** High — this is the best-mapped territory in the survey.
Rebase semantics demand *checkable* properties (mutations pure and
replayable over a store snapshot), which is exactly what a language can
enforce and a JavaScript library can only document.

### 3.3 Identity and minting

Two regimes exist:

- **Server-minted ids** force the temp-id dance: the client invents a
  placeholder, renders it, then remaps every reference when the real id
  arrives. Apollo's optimistic responses institutionalize this; the remap
  plumbing is a classic bug farm (dangling refs, keys changing under
  focused inputs, analytics double-counting).
- **Client-minted ids** (UUID/ULID/KSUID-class): the id the client renders
  is the id, forever. Replicache-lineage engines, CRDTs, and most
  local-first systems simply require this. The costs are id-format
  governance and authority-side validation (format, collision, quota) —
  small, boring, and paid once.

The field's direction of travel is unambiguous: client minting dissolves an
entire complexity class and is a precondition for offline creation at all.

Ordering is identity's sibling problem: concurrent inserts into ordered
collections. Fractional indexing (Figma's published approach, widely
copied, including by Notion-style editors) treats order as a mintable value
with the same dissolving effect — position becomes data, not a
renumbering transaction.

**Modelability.** Trivial and high-leverage: "identifiers are client-
mintable by construction" is a *decision*, not a mechanism, and every
downstream system simplifies when it is made early.

### 3.4 Granularity

What is the unit of sync, of conflict, of atomicity? The field uses, at
least: the endpoint/document (Firestore documents, query-cache entries),
the query result, the row (CDC, shapes), and the field/column (Figma's
per-property LWW, InstantDB's triples, Triplit's per-attribute merges,
WatermelonDB's per-column conflict resolution).

The trade is clean: **finer granularity buys merge freedom and fewer
conflicts; coarser granularity buys atomicity and invariants.** Two users
editing different fields of the same row both win under field-level LWW —
which is exactly right for a form and exactly wrong for `(amount,
currency)`. Mature systems therefore mix levels: row-scoped sync,
field-scoped merge, transaction-scoped writes.

**Modelability.** Granularity is provably declarable — the systems above
each fixed one globally, and their pathologies are precisely the places
where one global answer is wrong. Per-type (even per-field-group)
declaration is the obvious lift, with atomicity groups as the checkable
artifact. No surveyed system offers that; the gap is real.

### 3.5 Authority, invariants, permissions

When the store is the API, authorization must be expressible over *data*,
not endpoints. This is a one-way door: endpoint-shaped auth ("who may call
this route") does not survive the move to standing queries and replicas,
because reads no longer happen at routes. Hence the parade of rules DSLs
(Firebase/Firestore rules, InstantDB permissions, Meteor publish
functions) and the substrate bet on Postgres row-level security (Supabase,
Electric, PowerSync), with Zero's permissions-as-queries as the most
language-integrated form.

Field lessons:

- Rules DSLs start too weak (no joins, no aggregates), and schemas contort
  to compensate. Authorization is a *query-shaped* problem; under-powered
  rule languages just relocate the queries into denormalized columns.
- Invariants live at the authority, full stop. The client-facing question
  is only how refusal is *communicated*: as a typed, expected vocabulary
  ("insufficient-funds", "duplicate-handle") versus transport-shaped
  failure (HTTP status folklore, stringly errors). GraphQL's errors-as-data
  movement (the `userErrors` convention popularized by Shopify's schema) is
  the field converging on refusals as first-class, enumerable data.
- Permission changes are writes too: revoking read access must *retract*
  already-synced rows from a replica — the interaction of §3.5 with §3.7
  that every sync engine handles ad hoc.

**Modelability.** Refusal vocabularies: proven (declared error unions are
ordinary type-system work). Data-shaped permission predicates: proven in
several DSLs, with the caveat that the predicate language must be at least
join-capable or the schema pays. Sync-time enforcement and retraction:
engine work, mostly unmodeled — semantics here would be novel.

### 3.6 Derived data and aggregates

Counts, sums, rollups, denormalized "latest" pointers. Placement options
seen shipping: transactional maintenance at the authority (Postgres
triggers, Firestore transactional counters plus scheduled reconciliation),
authority-side computed projections (Convex query functions, materialized
views, IVM engines), and client-side aggregation over the replica.

The last one is a trap with a precise shape: **an aggregate over a window
is not an aggregate over truth.** A client that counts its replicated
subset lies whenever partiality (§3.7) or permissions (§3.5) have trimmed
the window — which is always. The field's working consensus: aggregates are
authority data that clients *render*, unless the replica is provably total
for the aggregated scope.

**Modelability.** The *placement decision* is declarable (this value is
computed-at-authority vs derived-locally), and "provably total for scope"
is a checkable side condition in systems with declared windows. Automatic
incremental aggregates are §3.1 rung 5 again — buy, don't invent.

### 3.7 Partiality and windowing

Every client store is a window onto truth. The field's forms: pagination
cursors (query-cache), tag/path scopes (re-derivation), declared shapes
(Electric), declared buckets (PowerSync's sync rules), queries-as-windows
(Zero, where the synced set is the union of active queries), and
bootstrap-the-workspace-then-deltas (Linear — until workspaces outgrew
clients and partial sync had to be engineered after the fact, the most
instructive war story in the survey).

Windows have three edge behaviors that systems must define and mostly
don't: membership churn (a row edits its way into or out of the window
while on screen), references across the edge (a synced row points at an
unsynced one), and growth (is "load more" a query or a window mutation?).

The convergent good idea: **partiality as a compiled, named artifact** —
shapes and buckets are declarations of "which subset of truth this client
holds," inspectable and versionable, where ad-hoc query parameters are
not. The convergent bad news: window ∩ permissions ∩ joins is where every
surveyed system keeps folklore instead of semantics.

**Modelability.** Declared windows: proven twice independently (shapes,
buckets). Edge semantics: unowned territory, and a place where a model
could genuinely lead rather than follow the field.

### 3.8 Schema evolution and replica lifetime

Replicas outlive deploys. The field's answers, in descending order of
honesty: version fences with forced reset (Replicache's `schemaVersion`;
every hosted vendor's cache-bust), expand/contract migration discipline
borrowed from service schemas, tolerant-reader conventions, and — as
research — bidirectional lens migration of live data (Ink & Switch's
Project Cambria). Nobody has this clean; the honest state of the art for
client-held data is "detect the fence, throw the replica away, re-sync."

**Modelability.** Fences and reset protocols: trivially modelable and
worth modeling early (they are also the escape hatch that makes everything
else shippable). Automatic migration of client-held data: not today. Note
the prototype-specific corollary: for a *prototyping* language, "reset on
schema change" may be the correct default class behavior rather than a
concession — prototypes change schema hourly and own no precious replicas.

### 3.9 The local remainder

Composer drafts, selection, toggles, half-completed wizards, in-flight
filters: every school ends up with a second, local store beside the bound
one — Apollo's local resolvers and reactive variables, Redux living beside
React Query, Meteor's Session, LiveView's client hooks. The pattern is so
universal it should be treated as a law: **there is always a local
remainder.**

The bugs live at the seam: a draft referencing a record that a sync just
deleted; optimistic view state keyed by a temp id that got remapped; local
selection surviving a permission retraction of its target. No surveyed
system types the references that cross from local state into bound state;
the seam is where the field keeps its dangling pointers.

This is the inverse face of the premise that opened this note. "Data does
not inherently mean external" cuts both ways: the field's recurring failure
is treating bound and local as *different frameworks* rather than as
different *classes of the same store* — which is what makes the seam
untypeable in the first place.

**Modelability.** The remainder itself is ordinary state. The *seam* —
typed references from local data to bound data, with defined behavior on
deletion, remap, and retraction — is unclaimed and looks eminently
modelable.

## 4. What travels, who decides, when

Before the taxonomy, three tiny vocabularies that §5 uses, distilled from
the schools:

- **Wire content:** snapshot | diff | row event | named operation. (Systems
  that send events but need state make every consumer a reducer; systems
  that send snapshots pay bandwidth for simplicity.)
- **Decision seat:** endpoint code | rules-over-data | shared deterministic
  mutators | the database itself (constraints, RLS).
- **Freshness:** on-demand | on-navigation | policy heuristics
  (focus/poll/TTL) | pushed.

## 5. A taxonomy

### 5.1 The axes

Ten axes suffice to place every surveyed system; disagreements between
schools are disagreements on these.

| # | Axis | Values observed in the field |
|---|---|---|
| A1 | Locus of truth | server-authoritative · convergent-shared · client-authoritative · server-resident session |
| A2 | Read topology | pull-per-view · standing query (pushed) · replica window · none (server renders) |
| A3 | Write topology | RPC command (named intent) · state patch (CRUD overwrite) · replayable op log · convergent ops |
| A4 | Repair mechanism | refetch · targeted patch · rebase · merge · re-render |
| A5 | Optimism | none (pending UI) · manual inverse · systemic overlay + rebase · convergent |
| A6 | Granularity | endpoint/document · query · row · field |
| A7 | Identity minting | server-minted (+ temp-id remap) · client-minted |
| A8 | Liveness | on-demand · on-navigation · policy heuristics · pushed |
| A9 | Authority seat | endpoint code · rules DSL over data · shared/deterministic mutators · database constraints/RLS |
| A10 | Partiality contract | ad-hoc parameters · declared windows · full replica · none |

### 5.2 The schools, placed

| School | A1 truth | A2 reads | A3 writes | A4 repair | A5 optimism | A6 grain | A7 ids | A10 window |
|---|---|---|---|---|---|---|---|---|
| §2.1 query-cache | server | pull-per-view | RPC command | refetch (+manual patch) | manual inverse | query | server | ad-hoc |
| §2.2 normalized graph | server | pull through graph | RPC command | targeted patch (declared at best) | manual inverse | entity/field | server (+temp-id) | ad-hoc |
| §2.3 re-derivation | server | pull-per-scope | RPC command / form | re-derive scope | none→manual | page/fragment | server | none |
| §2.4 server-resident | server session | none (server renders) | events up | re-render | none | n/a | server | none |
| §2.5 reactive database | platform | standing query | guarded patch or named fn | engine patch | systemic (platform) | document/row/field | mixed | query-as-window |
| §2.6 replica-and-log | server | replica window | replayable op log | rebase | systemic overlay | row + field merge | client | declared windows |
| §2.7 convergent | shared | replica (full doc) | convergent ops | merge | convergent | field/sequence | client | document |

Reading the table columnwise is more useful than rowwise: A2 has been
converging for a decade (everything drifts toward "reads look local and
update themselves"), while A3–A5 remain genuinely plural — the schools are,
to first order, *write-path and repair-path ideologies*.

### 5.3 Storage classes: binding as a property, not a paradigm

The synthesis this survey supports — the concept topology the working group
asked for — is that "database-bound" is best understood as a **storage
class of a declared store**, not as a different programming model bolted
onto the local one. The field keeps two frameworks side by side (§3.9) and
suffers at the seam; the systems people admire (Meteor then, the sync
engines and reactive databases now) are precisely the ones that made class
membership *invisible on the read side* and *explicit on the write side*.

Four classes cover the surveyed field. (A fifth, the server-resident
session of §2.4, is the degenerate class with no client store at all —
included for completeness as the control.)

| Class | Truth | Updates arrive | Writes are | Failure surface | Identity | Field exemplars |
|---|---|---|---|---|---|---|
| **ephemeral** | this client, this view | never (local writes only) | direct assignment | none | local | component state everywhere |
| **device** | this client, durable | never (local writes only) | direct assignment | storage quota only | local | drafts, preferences; PouchDB-unsynced, localStorage |
| **bound** (authoritative) | a database elsewhere | unbidden, continuously | named intents, speculative until settled | refusal (typed, expected) · unavailability | client-minted, authority-validated | §2.5, §2.6 entire |
| **convergent** | the document's replicas jointly | unbidden, continuously | convergent ops | none (merge) — invariants inexpressible | client-minted | §2.7; leaf values inside bound records |

What *must* differ by class — and what the field shows goes wrong when it
is left implicit:

1. **Write semantics.** Ephemeral/device writes are assignments; bound
   writes are *proposals* that can be refused; convergent writes are
   merges. Every school-1/2 pathology traces to writing bound data with
   assignment semantics.
2. **Failure surface.** Only bound stores can refuse; a model where
   refusal handling is optional reproduces the stringly-error status quo
   (§3.5).
3. **Arrival.** Bound and convergent stores update *unbidden*; every read
   site must tolerate change it did not cause. (The query-cache school
   retrofits this with revalidation heuristics; the sync schools make it
   the ground truth.)
4. **Identity.** Bound and convergent classes need client-mintable ids
   (§3.3); ephemeral state does not care.
5. **Lifetime.** Bound replicas need version fences and a reset story
   (§3.8); device state needs migrations only on the app's own clock;
   ephemeral state needs nothing.
6. **Aggregate placement.** Bound aggregates default to
   authority-computed (§3.6).

What must *not* differ by class: the reading surface. The field's clearest
positive result is that reads over local state and reads over bound state
can and should be indistinguishable at the point of use — this is the one
thing school after school independently achieved (minimongo, entity graphs,
client SQLite, live queries), and it is the property that makes the class
system a *taxonomy of stores* rather than a taxonomy of APIs.

One sentence, then, for the paradigm the evidence supports: **uniform
reads; class-differentiated writes, repair, and lifetime — with the class,
its window, its intents, and its refusals declared rather than implied.**

The seams the classes create are themselves part of the topology, and the
field says exactly where they are: references from ephemeral/device data
into bound data (§3.9 — the dangling-pointer seam), convergent values
embedded as fields of bound records (§2.7 — the merge-inside-authority
seam), and bound intents whose optimistic echo touches ephemeral view
state (§3.2 — the rollback seam). A model that names the classes gets to
type the seams; the surveyed systems, having no classes, cannot.

## 6. What can be modeled

The working-group question applied to §2–§3: which patterns has the field
already lifted into declarative, checkable form (proof by existence), which
submit under a constraint someone has already paid, and which resist.

### 6.1 Proven modelable (exists in the field as a declared artifact)

- **Named intents with declared refusal vocabularies** — Meteor methods,
  Replicache/Zero mutators, Convex mutations; refusals-as-data per the
  `userErrors` convention. The single most convergent result in the survey.
- **Declared invalidation algebra** — RTK Query tags; subsumed entirely by
  subscription models (the stronger form of the same declaration).
- **Declared cache repair** — Relay's mutation directives: patch semantics
  expressed in the query language, compiler-checked.
- **Declared partial-replication windows** — PowerSync buckets, Electric
  shapes: independent double invention of the same artifact.
- **Data-shaped permission predicates** — rules DSLs, RLS,
  permissions-as-queries; modelable, with the join-capability caveat of
  §3.5.
- **Determinism as the liveness enabler** — Convex end to end; differential
  dataflow client-side in TanStack DB.
- **Client-minted identity and mintable order** — sync-engine standard
  practice; fractional indexing.
- **Freshness as policy** — `staleTime`-class knobs; trivial but worth
  keeping explicit.
- **Pending/failed as renderable state** — fetcher states, overlay states,
  pending-mutation lists.
- **Version fences + reset** — `schemaVersion`-class protocols.

### 6.2 Modelable under a constraint the field has already priced

- **Standing queries / live reads** — modelable iff the query language is
  constrained (indexed predicates, bounded joins, no arbitrary compute).
  Firestore, Zero, and Convex all paid this price independently; the
  constraint is well understood and buys the disappearance of the entire
  invalidation problem (§3.1).
- **Systemic optimism via rebase** — modelable iff mutations are pure,
  serializable, and replayable against a store snapshot (§3.2). This is a
  *type-system-shaped* constraint: the one part of the field where a
  checked language has a structural advantage over every JavaScript
  incumbent, all of which enforce purity by documentation.
- **Client-side aggregates** — sound iff the window is provably total for
  the aggregated scope (§3.6); otherwise authority-computed.
- **Field-level merge** — sound iff conflict-free fields are declared as
  such and atomicity groups are respected (§3.4).
- **Convergent leaf values inside authoritative records** — sound iff
  confined to fields with no cross-field invariants (§2.7).

### 6.3 Resists modeling today

- **General IVM over arbitrary queries** — research-grade engineering
  (Materialize/Feldera-class); a language should *interface to* it, not
  attempt to own it.
- **Cross-record invariants under optimism** — the optimism horizon (§3.2)
  can be respected (demote such writes to pending-or-refused) but not
  dissolved; no system fakes uniqueness safely.
- **Automatic migration of client-held replicas** — Cambria remains
  research; fences-and-reset is the shippable truth (§3.8).
- **Window ∩ permission ∩ join composition** — the semantics of partial
  replication under authorization with references across the edge; every
  surveyed system holds folklore here. High-risk, high-novelty territory.
- **Long-offline reconciliation UX** — rebase after hours offline against
  moved truth produces conflicts that are *application decisions*; the
  field punts to the app, and honestly so.

## 7. Findings

1. **Reads converge, writes diverge.** Every school makes reads look local;
   no two schools agree on writes. Standardize the read surface; make the
   write and repair paths class-explicit (§5.3).
2. **Named intents are the field's fixed point.** From Meteor methods
   through server actions to sync-engine mutators, durable writes converge
   on named, argument-carrying commands with declared refusals — CQRS
   vocabulary without the ceremony. CRUD patches survive only where
   granularity is fine and invariants are absent.
3. **Per-mutation optimism does not compose; store-level optimism does.**
   The overlay-plus-rebase architecture turns optimism from per-feature
   heroics into engine semantics, at the price of replayable mutations — a
   price a checked language is uniquely positioned to enforce (§6.2).
4. **Invalidation should be declared or dissolved.** Hand-maintained
   key/tag maps are the field's largest self-inflicted bug class;
   subscriptions dissolve the problem, declared dependency algebras tame
   it. Nothing defends the imperative middle.
5. **Constraint buys liveness.** Every live-query system constrains its
   query language; the constraint is the feature. An expressive-but-dead
   query surface is the wrong trade for prototype-scale work.
6. **Mint identity at the client.** Dissolves the temp-id remap class,
   enables offline creation, and costs only validation at the authority.
   Mintable order (fractional indexing) extends the same move to
   sequences.
7. **Aggregates belong to the authority.** A client computing over a
   window lies; placement is a declarable property with a checkable
   totality side condition.
8. **Partiality wants a named artifact.** Shapes and buckets — invented
   twice, independently — are the field telling us the window must be a
   declared, versionable contract, because ad-hoc parameters cannot
   compose with permissions.
9. **Permissions must be data-shaped once the store is the API**, and the
   predicate language must be at least join-capable or schemas contort.
   This is language-design territory with several cautionary precedents.
10. **The local remainder is a law, so type the seam.** Local-referencing-
    bound is where the field's dangling pointers live and where no
    incumbent offers anything (§3.9) — cheap novelty available.
11. **Bind to contracts, not vendors.** Parse and Device Sync took whole
    architectures with them; the survivors (Electric's HTTP shapes,
    WatermelonDB's minimal protocol, poke-then-pull) are the ones whose
    seams are boring, documented wire contracts.
12. **Respect the control groups.** Re-derivation and server-resident
    sessions are coherent, shippable answers with near-zero client model.
    A bound-store class earns its complexity only where it clearly beats
    them: perceived latency, offline, and multiplayer liveness.

## 8. Open questions for the working group

- **Window-edge semantics** (§3.7): what does a reference across the
  replica edge *mean*, and what happens on screen when a row exits its
  window while displayed? The field has no answer to steal.
- **The optimism horizon as a checked property** (§3.2): how much of
  "safe to speculate" is inferable from intent/read-back structure versus
  requiring declaration?
- **Retraction** (§3.5): permission revocation as a first-class update to
  a replica — semantics, not just engine behavior.
- **The prototype default for lifetime** (§3.8): is fence-and-reset the
  honest *default* class behavior for a prototyping language, with durable
  replicas as the opt-in?
- **Class placement of the demo/fixture case:** a prototype's "database"
  may itself be disposable and seeded. Whether that is the bound class
  with a toy authority, or a distinct class, affects nothing in the field
  survey but everything in ergonomics — flagged, not answered, here.

## Appendix A. Survey corpus

Query-cache: SWR; TanStack Query; RTK Query (`providesTags`/
`invalidatesTags`); tRPC; RFC 5861 (stale-while-revalidate).
Normalized graph: Apollo Client; Relay (compiler, mutation directives);
urql Graphcache.
Re-derivation: Remix / React Router; Next.js server actions,
`revalidateTag`; SvelteKit; htmx; Hotwire Turbo; Unpoly; React
`useOptimistic`.
Server-resident: Phoenix LiveView; Laravel Livewire; Blazor Server; Rails
Turbo Streams.
Reactive database: Meteor (DDP, minimongo, latency compensation);
Firebase Realtime Database; Cloud Firestore (rules, offline persistence,
query constraints); RethinkDB changefeeds; Supabase Realtime (CDC, RLS);
Convex (deterministic functions, open-source backend); InstantDB (triple
store, permission language); Triplit.
Replica-and-log: Replicache (mutators, poke, `schemaVersion`); Zero
(ZQL, permissions-as-queries; 1.0 June 2026 — see
[InfoQ](https://www.infoq.com/news/2026/06/zero-version-1/) and
[zero.rocicorp.dev](https://zero.rocicorp.dev/)); ElectricSQL (shapes;
[alternatives survey](https://electric-sql.com/docs/reference/alternatives));
PowerSync (sync-rule buckets); LiveStore (event-sourced client SQLite);
TanStack DB (differential-dataflow live queries, optimistic overlay;
[docs](https://tanstack.com/db/latest/docs/overview),
[InfoQ beta coverage](https://www.infoq.com/news/2025/08/tanstack-db-beta/));
WatermelonDB (pull/push protocol, per-column merges); CouchDB / PouchDB
(revision trees, `_conflicts`); Datomic / DataScript (db-as-value,
transaction log).
Product engineering accounts: Linear sync engine (talks by Tuomas Artman;
partial-sync war stories); Figma — "How Figma's multiplayer technology
works" (per-property LWW, fractional indexing) and LiveGraph (CDC-driven
query invalidation); Notion engineering blog (op queue, client SQLite
caching); Asana's Luna/LunaDb (reactive full-stack framework,
retired).
Convergent / local-first: Automerge, Automerge Repo; Yjs; Jazz; Evolu;
Ditto; Ink & Switch — "Local-first software" (2019) and Project Cambria
(schema-evolution lenses); OT lineage (Google Docs / Jupiter).
Theory and adjacent: differential dataflow / Materialize; Feldera (DBSP);
event sourcing / CQRS; game-netcode client prediction and server
reconciliation (Source engine networking, Gaffer On Games); Shopify
GraphQL `userErrors` convention; Apple Core Data + CloudKit mirroring.
Cautionary shutdowns: Parse (announced 2016, closed 2017); Realm /
MongoDB Atlas Device Sync (deprecated 2024).
