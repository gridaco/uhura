# Uhura 0.4 source-neutral kernel

- **Status:** Active candidate semantics
- **Source syntax:** [Source and lowering](source.md)
- **Project and identities:** [Project, resolution, and identity](project.md)
- **Application extensions:** [Application profile](application.md)
- **Decision boundary:**
  [RFC 0004](../../../rfcs/0004-standalone-machine-core-and-source-composition.md)

This document describes the machine accepted by the runtime. It deliberately
contains no source keywords, file headers, module grammar, UI forms, or
formatter rules. A source spelling may change without changing this kernel
when it lowers to the same semantic program.

## Machine program

A complete checked machine program defines:

- configuration type `C`;
- mutable state type `S` and an initializer `init : C -> S`;
- closed input type `I`;
- closed, possibly empty outcome type `O` with policy
  `policy : O -> {commit, abort}`;
- closed command type `K`;
- pure observation type `V` and projection `observe : C × S -> V`;
- finite reaction `react`; and
- optional invariants over configuration and state.

The reaction contract is:

```text
react : C × S × I
     -> Commit(O, S, Seq<K>)
      | Abort(O)
      | Fault(ProgramFault)
```

One runtime instance contains one configuration, one complete committed state,
one FIFO input inbox, and one lifecycle. Source modules and parts do not exist
as scheduled runtime objects.

## Admission

Admission:

1. decodes one complete configuration;
2. validates configuration preconditions;
3. initializes every state path;
4. validates initial invariants;
5. validates all required host bindings; and
6. publishes genesis only if all checks succeed.

Admission is atomic. A part cannot be admitted independently from the complete
machine it contributes to.

## Reaction

For one dequeued input, the runtime:

1. copies the committed state to a private draft;
2. creates an empty ordered command buffer;
3. evaluates exactly one resolved reaction;
4. reaches one declared outcome or a closed program fault;
5. on fault, discards the draft and command buffer, publishes only the fault
   classification, and faults the instance;
6. otherwise applies the selected outcome policy;
7. for commit, runs at most one statically composed reconciliation phase and
   validates invariants;
8. atomically publishes the commit or abort result; and
9. starts external command handling only after a commit is published.

The reaction is finite, synchronous in the semantic sense, non-reentrant, and
run to completion. No input can be delivered into the current reaction.

### Commit

Commit publishes the draft and complete ordered command buffer together. A
committed no-op is still a committed outcome and receipt.

### Abort

Abort discards the draft and the entire command buffer. Committed state and
observation stutter. Abort is a declared domain result, not an exception or
program failure.

### Fault

The authored fault family is closed:

```text
ProgramFault =
  invariant_violation(SiteId)
  | unreachable_reached(SiteId)
```

A fault rolls back the reaction, records a deterministic receipt, and faults
the instance. Parser, checker, adapter, process, or resource failures are
implementation failures and cannot be forged as program faults.

Invariant obligations have one total semantic order after composition: the
root owner first, then canonical namespaced owner-path order, then authored
obligation order within each owner. The first false obligation selects the
fault site. Physical file and declaration placement do not participate.

`SiteId` is the fault-capable subset of the stable semantic node identity
defined by
[Project and identity §6](project.md#6-semantic-node-and-source-provenance):

```text
SiteId = NodeId(resolved-public-owner, composition-owner-path,
                site-kind, canonical-semantic-node-path)
```

The node path is assigned after name resolution and source composition. It
includes branch constructors and positions inside semantically ordered
statement or obligation sequences, but excludes logical module path, `use`
layout, comments, formatting, filename, byte offset, and line number. Two
identical sites in different control-flow positions therefore remain
distinct. Reordering source declarations whose order is non-semantic
preserves the address; reordering statements or obligations whose order is
semantic changes the checked IR and may change it. Source locations remain
attached only through the non-semantic provenance sidecar.

## Commands and external authority

A command is inert typed data. Buffering a command performs no I/O. A driver
may begin external work only after the commit containing that command is
published. A result, refusal, observation, or availability change can affect
the machine only as a later declared input.

There is no callback, promise, exception, ambient clock, random source,
network, storage object, DOM object, or synchronous foreign call inside a
reaction.

## Ports

A port is a typed host requirement. Resolution contributes its receive family
to the complete input domain and its send family to the complete command
domain:

```text
Input  = LocalInput  ⊕ Σ qualify(port, Receive)
Command = LocalCommand ⊕ Σ qualify(port, Send)
```

A port binding is required before admission. Importing its contract does not
bind authority. Sending through a port remains command buffering; receiving
from a port remains a later FIFO input.

## Values and totality

This section is the normative 0.4 value and harness-prelude contract. It
retains the 0.3 domains and canonical typed encoding, while deliberately
removing accidental observation of map or set storage order.

### Scalar domains

- `Unit` has one value.
- `Bool` has exactly `false` and `true`.
- `Text` is a finite sequence of Unicode scalar values. Equality is exact;
  there is no implicit normalization, case folding, locale, or collation.
- `Int` is an arbitrary-precision mathematical integer. `Nat` admits integers
  greater than or equal to zero. `PositiveInt` admits integers greater than or
  equal to one.
- `Decimal` is a finite exact base-10 value represented canonically without
  redundant trailing fractional zeros. Addition, subtraction, and
  multiplication are exact. Division is not admitted by 0.4.
- `Ratio` is an exact `Decimal` in the inclusive interval `[0, 1]`.
- `BoundaryNumber` is either a finite `Decimal`, NaN, positive infinity, or
  negative infinity. It exists only for typed boundary decoding; none of its
  non-finite cases enter ordinary arithmetic.

Numeric comparison and `+`, `-`, `*`, `min`, and `max` compute mathematical
results without overflow, saturation, or floating-point rounding. A result
used as `Nat`, `PositiveInt`, or `Ratio` must be proved to satisfy that domain
from type bounds, active path facts, and declared invariants, or produced
through a total conversion. `Ratio::checked_from(BoundaryNumber)` returns
`Option<Ratio>`; it never clamps or faults.

All equality is typed complete-value equality. There is no numeric, text,
key, constructor, or structural coercion.

### Compound domains

- Tuples are finite positional products.
- Records are closed structural products with unique declared field names.
  Their ordered field names and field types determine type identity and
  canonical encoding.
- Closed sums retain nominal declaration identity, constructor identity, and
  typed payload fields.
- A key is a nominal wrapper over one admitted value type; keys with different
  declaration identities never compare equal.
- `Option<T>` is the closed sum `None | Some(T)`.
- `Seq<T>` is a finite ordered sequence. `NonEmpty<T>` is a sequence with at
  least one element.
- `Set<T>` is a finite duplicate-free collection with no semantic traversal
  order.
- `Map<K,V>` is a finite unique-key mapping with no semantic traversal order.
- `Table<K,V>` contains exactly one `V` for every constructor of one closed
  unit-constructor key sum `K`; lookup is total and constructor order defines
  its only traversal order.
- `Entry<K,V>` is the closed record `{ key: K, value: V }`.
- `Uncons<T>` is the closed record `{ head: T, tail: Seq<T> }`.

Sets and maps sort by canonical typed bytes for encoding and comparison. That
internal order is not exposed to source. A `FiniteView<T>` is an ephemeral,
orderless evaluator view used only by admitted traversal-invariant operations;
it cannot be stored, emitted, observed, placed in a command or input, or
converted directly to `Seq`.

### Required total prelude

The L0–L2 and A0 harness contracts fix these operations. The notation follows
the selected source spelling for readability; this section fixes operation
identity and semantics, while `source.md` owns grammar.

```text
Ratio::checked_from(BoundaryNumber)          -> Option<Ratio>
Int::from(BoundaryNumber)                    -> Option<Int>
NonEmpty::checked_from(Seq<T>)               -> Option<NonEmpty<T>>
Seq::from_options(Seq<Option<T>>)            -> Seq<T>
Map::empty()                                 -> Map<K,V>
Map::from(Seq<(K,V)>)                        -> Map<K,V>
Map::from_unique(Seq<(K,V)>)                 -> Option<Map<K,V>>
Set::empty()                                 -> Set<T>
Set::from_unique(Seq<T>)                     -> Option<Set<T>>

Seq<T>.append(T)                            -> Seq<T>
Seq<T>.without(T)                           -> Seq<T>
Seq<T>.uncons()                             -> Option<Uncons<T>>
Seq<T>.len()                                -> Nat
Seq<T>.is_empty()                           -> Bool
Seq<T>.is_unique()                          -> Bool
Seq<T>.contains(T)                          -> Bool
Seq<T>.map(T -> U)                          -> Seq<U>
Seq<T>.filter(T -> Bool)                    -> Seq<T>
Seq<T>.try_map(T -> Option<U>)               -> Option<Seq<U>>
Seq<T>.all(T -> Bool)                       -> Bool
Seq<T>.any(T -> Bool)                       -> Bool
Seq<T>.count(T -> Bool)                     -> Nat

Map<K,V>.get(K)                             -> Option<V>
Map<K,V>.put(K, V)                          -> Map<K,V>
Map<K,V>.remove(K)                          -> Map<K,V>
Map<K,V>.entries()                          -> FiniteView<Entry<K,V>>
Map<K,V>.entries_by_key()                   -> Seq<(K,V)>
Map<K,V>.values()                           -> FiniteView<V>
Map<K,V>.try_map_values(Entry<K,V> -> Option<U>) -> Option<Map<K,U>>

Set<T>.add(T)                               -> Set<T>
Set<T>.remove(T)                            -> Set<T>
Set<T>.contains(T)                          -> Bool
Set::filter_map(FiniteView<T>, T -> Option<U>) -> Set<U>

Table<K,V>[K]                               -> V
Table::from(exhaustive literal entries)     -> Table<K,V>
Table<K,V>.set(K, V)                        -> Table<K,V>
Table<K,V>.values()                         -> Seq<V>

FiniteView<T>.all(T -> Bool)                -> Bool
FiniteView<T>.any(T -> Bool)                -> Bool
FiniteView<T>.count(T -> Bool)              -> Nat
```

`Seq::from_options` and all sequence transformations preserve left-to-right
source order. `without` removes every equal element while preserving the
relative order of the rest. `Map::from` is admitted only over a literal
sequence of compile-time key/value pairs with no duplicate typed key; its
source order is not observable. `put` on `Map` replaces the equal key or
inserts one mapping; set insertion deduplicates by typed equality.

`Int::from` returns `Some(value)` exactly when the boundary number is finite
and mathematically integral; otherwise it returns `None`.
`Map::from_unique(items)` returns `Some(map)` exactly when every item is a
key/value pair and no two keys are equal by typed equality. The result does
not expose the sequence's order. `Set::from_unique(items)` analogously returns
`Some(set)` exactly when the input is duplicate-free by typed equality.
Neither operation silently drops or replaces a duplicate.

`try_map` visits a sequence from left to right. It returns `Some(sequence)` in
the same order exactly when every binder result is `Some`; if any binder
result is `None`, the result is `None`. `try_map_values` applies its pure
binder pointwise to every `Entry { key, value }`, preserves each original key,
and returns `Some(map)` exactly when every result is `Some`; otherwise it
returns `None`. Its result cannot reveal evaluation order.

`entries()` exposes only the closed `Entry<K,V>` fields `key` and `value`
through an orderless `FiniteView`. `entries_by_key()` is the explicit ordered
escape needed by presentation repetition: it returns `(key, value)` pairs in
ascending canonical typed-key-byte order. Moving a map through canonical
encoding therefore preserves this sequence exactly.

`uncons([]) = None`. For every non-empty sequence,
`uncons([head, ...tail]) = Some(Uncons { head, tail })`, with
`tail.len() + 1 = original.len()`. This law preserves FIFO head order and is
available to the static `decreases` proof fragment.

`NonEmpty::checked_from([]) = None`. For every non-empty sequence `items`,
`NonEmpty::checked_from(items) = Some(items)` with the exact values and
left-to-right order preserved; the result differs only by its checked
non-empty refinement.

`Table::from` is admitted only for a source literal containing exactly one
entry for every unit constructor of `K`, with no unknown or duplicate key.
Accepted construction is therefore total and infallible. `values()` returns a
sequence in `K`'s constructor declaration order, independent of literal entry
order.

Every admitted operation is total for checked arguments. A potentially invalid
refinement returns `Option`, or the checker rejects the source before runtime.
There is no collection bounds fault, missing-key fault, overflow fault, or
implicit iteration over an orderless collection. A fold over a map, set, or
finite view is admitted only when its result is invariant under traversal
order. Programs that need observable order construct an explicit `Seq`
through an operation whose ordering rule is declared.

No other scalar or collection operation is part of the 0.4 core candidate.
Additional operations needed by A0 or Instagram must be added to this
normative contract before those ports; an implementation helper or host
method cannot fill the gap implicitly.

Pure functions are total, non-recursive, and free of authority. Standard
collection operations terminate over finite admitted values. An authored loop
is accepted only when the checker proves a `Nat` measure strictly decreases on
every back edge. Failure to establish termination is a source rejection, not a
runtime fault.

## Observation

Observation is a pure total function of configuration and committed state:

```text
observe : C × S -> V
```

It cannot read a draft, inbox, command buffer, host object, physical UI
mechanic, or external fact that has not arrived as declared data. Observation
does not mutate, emit, subscribe, or establish a lifecycle.

## In-transaction reads

Source composition may lower a declared read dependency to a pure selector:

```text
select_j : C × S -> T_j
```

When a reaction calls a selector, `S` is the current draft at that exact call
site. A later call after an assignment therefore sees the later draft. When a
selector is composed into observation, `S` is the committed state being
projected. A selector is not itself an observation: it is not published,
callable by a host, or stored as an independently observable runtime value.

Selectors may compose only through a statically acyclic dependency graph.
They cannot mutate, emit, inspect the inbox or command buffer, or read external
facts. A compiler may inline or memoize them only when doing so preserves
call-site draft semantics.

## Source composition boundary

After source checking, composition produces ordinary aggregate domains:

```text
S = S_root × S_part_1 × ... × S_part_n
I = OwnerInput(root) ⊕ OwnerInput(part_1) ⊕ ... ⊕ OwnerInput(part_n)
K = OwnerCommand(root) ⊕ OwnerCommand(part_1) ⊕ ... ⊕ OwnerCommand(part_n)
V = V_root × V_part_1 × ... × V_part_n

OwnerInput(o) =
  qualify(o, I_o)
  ⊕ qualify(o.port_1, Receive_port_1)
  ⊕ ... ⊕ qualify(o.port_m, Receive_port_m)

OwnerCommand(o) =
  qualify(o, K_o)
  ⊕ qualify(o.port_1, Send_port_1)
  ⊕ ... ⊕ qualify(o.port_m, Send_port_m)
```

Here `qualify(root, x) = x`; part and port owners add their stable semantic
path.

All four domains use one canonical owner order: the root first, then stable
part paths lexicographically by canonical identifier bytes, segment by
segment. Within an owner, product fields and local sum constructors retain
their declaration order. The local input or command sum precedes every port
contribution; ports are then sorted by canonical port name; and each port's
receive and send constructors retain contract declaration order. This same
order governs constructor ordinals, checkpoints, canonical encodings,
receipts, semantic IR, program hashes, and invariant groups. Source-file
layout and authored `part` or `port` declaration placement do not participate.

For each configured part, composition also fixes one pure total binding
`bind_i : C -> C_part_i`. Part requirements and state initialization read that
bound value during admission. It adds no separately decoded configuration or
checkpoint domain. Dependency handles are selector/update edges, not values
inside `C_part_i`.

For each port, composition fixes a pure total configuration binding. A
root-owned binding reads only `C`; a part-owned binding reads only its
`bind_i(C)` value. The binding expression is semantic program IR, is evaluated
and contract-checked during atomic admission, and is reproduced from `C`
during restore. Port configuration adds no checkpoint state, adapter
selection, initialization, or authority.

An in-transaction update dependency is statically inlined or lowered as an
ordinary checked call over the same draft and command buffer. It adds no
inbox, scheduler step, intermediate receipt, or independently observable
runtime boundary.

Namespacing in these equations prevents collision and preserves provenance.
It does not create another execution model.

An omitted configuration, state, or observation contribution is the `Unit`
product; an omitted input or command contribution is the empty sum. These are
composition identity elements, not implicit mutable objects or hidden events.
An omitted root outcome contribution is the empty sum with its unique empty
policy. It is admitted only when the complete input sum is empty and no source
form requires an outcome; otherwise `O` must be declared and non-empty.

## Semantic program identity

The kernel is admitted under one `MachineProgramId`:

```text
MachineProgramId =
  sha256(frame("uhura-machine-program/0", canonical-semantic-machine-IR))
```

The exact projection and framing are defined by
[Project and identity §7](project.md#7-identity-layers-and-hashing). The
identity covers the complete composed machine, its transitive reachable
semantic declarations, resolved contract instances, canonical domain order,
and runtime-observable `SiteId` values.

It does not cover physical source paths or spans, logical module paths,
locator and import spelling, provenance occurrences, unused declarations,
evidence, presentation IR, host bindings, or runtime instance identity.
Consequently:

- two source layouts that resolve and lower to the same semantic machine have
  the same `MachineProgramId`;
- moving or renaming a logical module cannot invalidate a checkpoint by
  itself;
- changing a public package identity, public declaration name, part owner
  path, reachable behavior, contract, or semantic order changes the identity;
  and
- source-revision, presentation, deployment, and runtime-instance identities
  remain separate values.

An implementation may serialize semantic IR and source provenance in one
transport artifact, but its semantic projection must be independently
canonicalizable and byte-identical across provenance-only changes.

## Receipts and checkpoints

Every completed reaction records common receipt fields:

- semantic program identity;
- instance identity and sequence;
- admitted input;
- pre-state identity;
- commit, abort, or fault classification; and
- semantic owner and fault-site attribution when applicable.

A tooling envelope may attach authored module, path, and source-span
provenance through the checked sidecar. Those physical coordinates are not
semantic receipt fields and do not enter receipt identity.

The classification payload is a tagged union:

```text
CommitReceipt { outcome: O, postStateId, commands: Seq<K> }
AbortReceipt  { outcome: O }
FaultReceipt  { fault: ProgramFault }
```

A fault has no outcome. Even when reconciliation or invariant checking faults
after a commit-policy outcome was provisionally selected, that outcome was
never published and is absent from the semantic receipt. The draft and command
buffer are discarded.

A checkpoint contains the complete configuration, committed global state,
inbox and sequencing context required by the profile, semantic program
identity, and canonical integrity data. A part checkpoint is not a complete
restorable runtime checkpoint.

Restoring and replaying the same semantic program, checkpoint, and ordered
inputs must reproduce the same semantic receipts. Moving source files or
changing comments cannot change that result when resolved semantic IR is
unchanged.

## Kernel exclusions

This kernel does not define:

- source files, modules, imports, exports, or parts;
- UI, HTML, CSS, widgets, routes, or components;
- runtime child machines, actors, supervision, discovery, or dynamic
  instantiation;
- host adapter implementation;
- package resolution or distribution; or
- arbitrary foreign execution.

Those subjects may lower into or surround the kernel, but cannot silently
alter its reaction contract.
