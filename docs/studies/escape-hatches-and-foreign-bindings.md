# Escape hatches and foreign bindings

- **Status:** Non-normative problem study — need established, design open
- **Lifetime:** Disposable study
- **Scope:** User-supplied behavior and integrations outside Uhura's supported
  semantic model
- **Method:** Current repository audit plus primary-source precedent review;
  rolling official documentation retrieved July 18, 2026
- **Doctrine:** [Mission and identity](../doctrine/mission.md),
  [authoring ergonomics](../doctrine/authoring.md), and
  [design principles](../doctrine/principles.md)
- **Related work:** [Language necessity and surface reuse](language-necessity-and-surface-reuse.md),
  [program harnesses](../../examples/programs/README.md), and the
  [widget taxonomy](../widgets/README.md)
- **Authority:** Research brief only; this document accepts no syntax, foreign
  function interface, ABI, trust keyword, module format, or runtime topology

## Outcome

Uhura requires an explicit escape boundary.

A deliberately bounded frontend language cannot anticipate every algorithm,
browser API, native capability, vendor SDK, existing library, hosted control,
or application-specific integration. Refusing every foreign implementation
would make ordinary applications impossible or force authors to hide the same
power in provider monoliths, renderer forks, generated source, untracked DOM
mutation, or engine patches. Those are still escape hatches, only invisible
and harder to review.

The established need does **not** establish that Uhura needs one general
JavaScript FFI. It does not decide:

- whether the contract appears in Uhura source, a manifest, or a separate
  interface document;
- whether current ports and catalog declarations can cover the need;
- whether foreign work is synchronous, asynchronous, event-driven, rendered,
  or split among several mechanisms;
- whether JavaScript, WebAssembly, native code, another Uhura project, or a
  remote process supplies the implementation;
- whether `extern`, `unsafe`, `unchecked`, `host`, `capability`, `binding`, or
  another term describes any part of the system; or
- which guarantees remain language-enforced, checked, runtime-validated,
  tested, trusted, or unavailable.

JavaScript is the immediate browser integration pressure, not the semantic
identity of the boundary. A future design must remain honest about targets
that do not execute JavaScript.

## 1. Problem statement

Uhura needs an explicit, reviewable account of behavior or integration outside
its supported semantic model. Future design must determine what is declared,
where it is declared, and how any target implementation is supplied.

The foreign implementation may be opaque to some Uhura tooling. The boundary
must not be opaque. Authors, reviewers, agents, checkers, hosts, preview tools,
and deployment systems need to discover what foreign behavior exists, what it
may observe or affect, and which guarantees stop at the seam.

The design problem is therefore not merely how to call JavaScript. It is how
to preserve a small, legible, and checkable Uhura program while admitting
behavior whose implementation is outside Uhura's semantic authority.

## 2. Current repository evidence

Uhura already contains several narrow seams. They are evidence about the
problem, not a general escape-hatch design.

### 2.1 Typed service ports

The Instagram client declares closed projection, command, refusal, and data
contracts in files such as
[`feed.port.toml`](../../examples/instagram/client/ports/feed.port.toml).
Uhura source imports selected items with `use port`; Core emits commands and
consumes declared provider updates and outcomes rather than performing network
or database work.

This demonstrates a contract-first external authority boundary. It does not
demonstrate synchronous foreign computation, custom visual integration, or a
general host capability model.

### 2.2 An application-owned JavaScript provider

The current
[`uhura.toml`](../../examples/instagram/client/uhura.toml) selects an
application-owned JavaScript provider module for Play. Its checked-in
[`spock.ts`](../../examples/instagram/client/providers/spock.ts) implementation
uses browser and Spock APIs while speaking the provider envelope expected by
Uhura.

Uhura's envelope parsing and projection application validate portions of the
wire shape and declared contract, but they do not prove the provider's
JavaScript behavior, termination, authority use, or semantic fidelity. The
module is therefore an existing unchecked adapter seam, not evidence that
arbitrary JavaScript belongs inside `.uhura` source.

### 2.3 Narrow host capabilities

The Play provider receives a host-owned file picker and cancellation signal.
The browser `File` stays outside Core; declared identifiers and serializable
metadata cross the port boundary. This is useful evidence that an opaque host
value need not become an Uhura value.

It also exposes an important pressure: some browser capabilities require work
to begin in the originating user-activation stack. A design that assumes every
host operation can be deferred through an ordinary asynchronous queue must
test that assumption.

### 2.4 Platform intents and renderer implementations

Core currently emits a small fixed set of typed platform intents. Browser
rendering realizes the checked catalog, but element implementations are
hard-coded rather than application-bound. A catalog element accepted by the
checker but unknown to the browser renderer falls back to an unsupported
realization; an element absent from the catalog is rejected during checking.
There is no user-supplied JavaScript element binding today.

Service effects and hosted visual controls therefore must not be treated as
the same solved problem.

### 2.5 Fixtures, replay, and static examples

Deterministic fixture scripts can satisfy the port contracts without loading
the Play provider. Pinned examples bind frozen state and data; derived examples
replay declared events into a frozen semantic result. Neither executes
interactive foreign behavior.

That is a valuable precedent for substitutes and recordings. It does not yet
decide whether every foreign binding requires a fixture, fallback, snapshot,
mock, or unsupported-capability diagnostic.

## 3. Distinct integration pressures

The following pressures may converge on shared infrastructure, but they have
different ownership, scheduling, lifecycle, and portability problems.

| Pressure | Representative need | Principal design risk |
| --- | --- | --- |
| External authority or service | Network request, storage, analytics, application provider | Foreign results becoming hidden product truth or bypassing declared command and outcome semantics |
| Host or device capability | Clipboard, camera, file picker, notification, sensor | Permission, user activation, cancellation, platform availability, and ambient authority |
| Foreign visual realization | Map, video SDK, chart canvas, hosted native control, custom element | Subtree ownership, focus, gestures, accessibility, sizing, disposal, and static preview |
| External event source | WebSocket, media clock, observer, device stream | Hidden scheduling, backpressure, duplicate or late events, and lifetime after disposal |
| Synchronous computation | Parser, codec, complex validation, cryptography, application algorithm | Purity, termination, ambient reads, exceptions, and execution inside an otherwise deterministic step |
| Initialization or configuration | Host-provided boot data, environment selection, injected handles | Undeclared startup authority, non-replayable initial state, and target-specific configuration |

A proposal must identify which pressures it covers. One mechanism should not
be called general merely because it can technically invoke code for all of
them.

## 4. Review hypothesis

The smallest useful hypothesis is:

> Foreign code may realize an explicit declared boundary. It must not silently
> acquire authority over Uhura state, event ordering, semantic presentation, or
> external product truth beyond that boundary.

This is a review hypothesis, not accepted mechanics. Future work must determine
what “declared,” “realize,” “authority,” and “beyond” mean for each integration
class.

An escape hatch that allows arbitrary foreign expressions anywhere would
create a second language inside Uhura. A boundary that is too weak may instead
force every useful operation through unnatural asynchronous plumbing or engine
extensions. Both outcomes must be tested rather than assumed.

## 5. Questions every design must answer

### 5.1 Placement and visibility

- Where is the foreign dependency declared?
- Can tools enumerate the complete foreign surface without executing or
  interpreting arbitrary code?
- Is the binding visible at the call site, project boundary, dependency
  boundary, or all three?
- Are application bindings different from reusable package bindings?
- Can a dependency introduce foreign authority transitively without the
  application acknowledging it?

### 5.2 Contract and values

- What may cross: copied values, serialized records, events, streams, binary
  data, or opaque resource handles?
- Which side validates arguments, results, errors, and version compatibility?
- Can closures, DOM nodes, browser `File` objects, proxies, cyclic graphs, or
  other host identities cross?
- If opaque resources exist, who owns, borrows, disposes, and invalidates
  them?
- Does a typed signature describe only data shape, or also ordering,
  idempotence, cancellation, ownership, and behavioral obligations?

### 5.3 Authority, trust, and security

- What clock, randomness, storage, network, DOM, device, origin, secret, or
  process authority may the implementation access?
- Is authority technically constrained, granted by capability, sandboxed,
  audited, or only documented?
- What exact proposition does `unsafe`, `unchecked`, or another trust label
  mean?
- Who assumes the unverified obligation: binding author, package publisher,
  application author, call site, or host?
- How are module identity, content, provenance, dependencies, and versions
  pinned?

### 5.4 Execution and causality

- May foreign code execute inside a transition or observation?
- Can it synchronously re-enter Uhura, emit more than once, schedule hidden
  callbacks, or mutate renderer state?
- What is the ordering relationship between foreign results and ordinary
  events?
- How are exceptions, panics, invalid results, timeout, nontermination,
  duplicate settlement, and refusal represented?
- If a result arrives after cancellation, navigation, hot reload, or disposal,
  how is it correlated and classified?

### 5.5 Determinism, replay, and inspection

- Which deterministic claim still holds: an individual Core step, replay over
  recorded inputs, the complete application, or none?
- Must requests and results appear in a trace?
- Can replay and static inspection proceed when the foreign implementation is
  absent?
- Does a supposedly pure binding read locale, clock, random state, global
  variables, mutable caches, or host configuration?
- How does a tool distinguish a guaranteed property from a convention or
  test?

### 5.6 Rendering and lifecycle

- Who owns mount, update, focus, measurement, gesture arbitration,
  accessibility, animation, and disposal for foreign visual content?
- Is the binding confined to an owned subtree or surface?
- Which semantic events may it emit?
- What happens in Editor, headless evaluation, screenshots, reduced-motion
  mode, and unsupported renderers?
- Is a deterministic placeholder, fixture, recorded state, or alternative
  realization required?

### 5.7 Tooling and distribution

- Can the checker generate or validate binding stubs?
- Where does static checking end and runtime validation begin?
- Can formatting, language servers, dependency graphs, traces, and agents
  explain the boundary?
- How are missing target implementations and incompatible versions reported?
- Can a project state whether it is portable, replayable, statically
  previewable, or dependent on unchecked bindings without a manual audit?

## 6. Candidate design families

These are comparison families, not proposed or accepted designs.

| Family | Boundary shape | Question it isolates |
| --- | --- | --- |
| Narrow built-in seams only | Extend ports, platform intents, and catalog contracts case by case | Can Uhura cover real integrations without a general foreign mechanism? |
| Contract plus project binding | A target-neutral declaration is satisfied by a target-specific module selected outside ordinary behavior source | Is one explicit application seam sufficient, and what tooling can verify it? |
| Declared foreign item | Source names an externally implemented function, command, stream, resource, or element | Does local visibility justify a new language concept? |
| Inline foreign body | Uhura source embeds or quotes JavaScript or another language | Does maximal convenience justify the shadow-language, tooling, and portability cost? |
| Sandboxed component | A capability-limited component implements a typed interface through a portable ABI | Do stronger isolation and composition repay their runtime and packaging cost? |
| Generated adapter | Uhura owns a contract and generates target-language types or glue around an application implementation | How much drift and unchecked surface can generation remove? |

Several families may coexist if the problem taxonomy requires them. The study
does not assume that service effects, synchronous algorithms, and hosted views
should share one authoring form.

## 7. Guarantee ledger

Every candidate must state the source of each claimed guarantee:

```text
language semantics
checker
generated binding
runtime validation
target sandbox or capability system
conformance test
application test
binding-author obligation
unverified convention
not guaranteed
```

At minimum, the ledger must cover:

- argument and result shape;
- state ownership and mutation;
- effect and capability authority;
- termination and resource bounds;
- event ordering and reentrancy;
- correlation, cancellation, and disposal;
- exception and malformed-result behavior;
- deterministic replay;
- static preview and headless inspection;
- renderer and target portability;
- accessibility for foreign visual content; and
- module provenance and version compatibility.

One strong guarantee must not be inferred from another. A typed interface does
not prove purity. A sandbox does not prove semantic correctness. A recorded
trace does not make live execution deterministic. A JavaScript module that
works in Play does not establish a non-browser realization.

## 8. Evidence program

The current
[L2 task supervisor](../../examples/programs/l2-task-supervisor/README.md)
already pressures an asynchronous external-worker boundary: ordered requests,
correlated results, cancellation, duplicate settlement, stale outcomes, and
replay. It can compare provider-style candidates, but it does not establish
the need or shape of synchronous foreign computation or hosted visual content.

Before selecting a design, add independent problem evidence for:

1. a host capability that requires user activation, can be denied, and may
   complete after its owner is disposed;
2. a foreign event stream with subscription, backpressure or coalescing,
   cancellation, and late delivery;
3. a visual integration with declared inputs and semantic events, focus and
   accessibility obligations, lifecycle cleanup, and a static representation;
4. a deterministic local algorithm or library operation whose placement
   reveals whether an asynchronous command boundary is ergonomically or
   semantically sufficient; and
5. an unavailable, incompatible, malformed, throwing, nonterminating, or
   malicious binding.

Candidate implementations should cover the same problems using the smallest
viable alternatives. A proposal may not invent a convenient example that only
its preferred mechanism handles well.

Required adversarial cases should include:

- missing target implementation;
- wrong version or contract hash;
- malformed, cyclic, oversized, or unsupported result data;
- a result or event after cancellation, disposal, navigation, or hot reload;
- duplicate settlement and synchronous reentrancy;
- exception, refusal, timeout, and no completion;
- replay with the foreign module unavailable;
- Editor with no interactive realization;
- a target with no JavaScript runtime;
- a foreign element mutating outside its owned visual region or emitting an
  undeclared event; and
- a supposedly deterministic function reading clock, locale, randomness, or
  mutable ambient state.

## 9. Decision gates

A future proposal is not ready for acceptance until it:

1. names the independent user problems and integration classes it addresses;
2. compares the smallest viable port, host-capability, renderer-extension,
   library, and no-new-language alternatives relevant to those problems;
3. defines the precise semantic boundary and every admitted value or resource;
4. identifies every lost, conditional, or target-specific guarantee;
5. specifies execution ordering, reentrancy, failure, cancellation, late
   delivery, lifetime, and cleanup;
6. defines Editor, fixture, replay, headless, and unsupported-target behavior;
7. makes foreign dependencies, capability grants, trust, provenance, and
   versions auditable;
8. provides diagnostics for missing and incompatible bindings;
9. demonstrates that the common safe path remains short; and
10. passes the relevant program and adversarial comparison without changing
    the problem.

Calling a boundary `unsafe` or `unchecked` is insufficient when the term does
not identify the exact obligation and its owner. Likewise, calling a module
typed is insufficient when only its payload shape is checked.

## 10. Prior work

These precedents establish useful questions, not an Uhura design.

- Rust uses
  [`unsafe`](https://doc.rust-lang.org/stable/reference/unsafe-keyword.html)
  to create or discharge explicit proof obligations the compiler cannot
  verify. Its
  [external blocks](https://doc.rust-lang.org/stable/reference/items/external-blocks.html)
  place responsibility for correct foreign signatures on the declaration
  author. Unsafe code does not suspend Rust's remaining invariants.
- Elm separates JavaScript interoperation into
  [initial flags, message ports, and custom elements](https://guide.elm-lang.org/interop/).
  Its [port guidance](https://guide.elm-lang.org/interop/ports) emphasizes
  strong message boundaries and explicit state ownership rather than mirroring
  every JavaScript function.
- Flutter distinguishes asynchronous
  [platform channels](https://docs.flutter.dev/platform-integration/platform-channels)
  from hosted platform views. It also documents generated type-safe channel
  bindings, threading, lifecycle, and target-specific limitations.
- The WebAssembly Component Model uses
  [WIT](https://component-model.bytecodealliance.org/design/wit.html) to
  describe typed interfaces rather than behavior. Its
  [component model](https://component-model.bytecodealliance.org/design/components.html)
  separates declared imports and exports from the implementation that
  satisfies them.
- Spock's non-accepted
  [`extern fn` study](https://github.com/gridaco/spock/blob/main/docs/rfd/0001-effects-once-extern.md)
  proposes the useful constraint that a foreign body may replace an
  implementation but not its declared contract. Spock's existing `unchecked
  sql` syntax separately demonstrates lexical visibility for a deliberately
  unchecked subsystem; neither precedent determines Uhura's boundary.

Rust's concern is memory and type soundness, Elm's is application interop,
Flutter's is host-platform integration, WebAssembly's is component
composition, and Spock's is backend computation. None proves that its keyword,
transport, or trust model transfers directly to Uhura's determinism,
presentation, preview, and portability requirements.

## 11. Open questions

1. Is one general escape boundary desirable, or should effects, host
   capabilities, event streams, foreign elements, and synchronous computation
   remain distinct?
2. Can current ports and catalog contracts be generalized without confusing
   external product truth with platform or presentation mechanics?
3. Does Uhura need synchronous foreign computation at all?
4. If synchronous calls exist, can purity and termination be enforced,
   sandboxed, bounded, or only trusted?
5. Should foreign bindings be application-only, or may reusable packages
   introduce them?
6. Does the contract live in source, the project manifest, an interface file,
   a generated artifact, or more than one of these?
7. Which target names belong to the semantic program, and which belong only
   to deployment configuration?
8. Can opaque resource handles cross the boundary without undermining
   snapshots, replay, and lifetime analysis?
9. What permission and capability system is technically enforceable for
   browser JavaScript?
10. Which substitutes are required for Editor, testing, replay, and unsupported
    targets?
11. How does a project report transitive unchecked dependencies and downgraded
    guarantees?
12. What evidence would justify inline JavaScript over a narrower declared
    message or component boundary?

## 12. Consequence for current redesign

The redesign must reserve space for explicit foreign integration. It must not
assume that every real frontend need will become an Uhura primitive, nor force
foreign work to masquerade as ordinary deterministic behavior.

That obligation does not authorize syntax or implementation work yet. The next
step is independent problem evidence and candidate comparison under the review
questions above. A later RFC may select one or more boundary forms; this study
selects none.
