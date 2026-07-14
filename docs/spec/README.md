# Uhura specification

- **Status:** Pre-specification working draft
- **Target:** Unversioned
- **Owner:** [Uhura Working Group](../working-group/README.md)
- **Foundational RFC:** [RFC 0001](../rfcs/0001-project-foundation.md)
- **Accepted source-language RFC:**
  [RFC 0003](../rfcs/0003-source-comments-docs-and-annotations.md)
- **Prior art:** XAML, Svelte, QML, Elm

This is the living master document for **Uhura**, a declarative UI language,
checker/compiler, and deterministic headless experience runtime. Its canonical
source suffix is `.uhura`. The complete grammar and serialization are not yet
accepted; the comment, declaration-doc, and markup-annotation subsystem in
§13 is accepted independently.

Uhura owns the semantics of non-authoritative UI-session state. It consumes
typed external projections, receives semantic events and command outcomes,
advances an experience machine, and produces a renderer-neutral semantic view
plus explicit commands and platform intents.

Uhura does not own authoritative domain state, authorization, transactions,
backend effects, or concrete rendering.

> **Maturity warning:** this document fixes a project boundary and a model for
> research. It does not yet define a complete usable source syntax or a
> conforming runtime. Section 13 is an accepted source-language decision with
> implementation pending; other terms and examples remain conceptual until
> accepted by an RFC and backed by executable tests.

## 1. Normative language

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**,
and **MAY** are interpreted as described by
[BCP 14](https://www.rfc-editor.org/info/bcp14) only when capitalized.

Before Uhura 1.0, capitalized requirements are proposals unless their section
incorporates an accepted RFC. Acceptance locks the design decision; claiming a
conforming implementation additionally requires a versioned specification and
executable conformance tests.

## 2. Product model

The system has four independent responsibilities:

| Layer | Owns | Does not own |
|---|---|---|
| Spock | Durable product truth, guarded business transitions, authorization, transactions, server workflows, and backend effects | UI-session state or rendering |
| Uhura | Presentation semantics, UI-session state and transitions, external port requirements, semantic view evaluation, and UI traces | Durable domain authority, direct I/O, or pixels |
| Renderer and host drivers | Layout, paint, native controls, input normalization, device mechanics, and execution of explicit platform intents | Product truth or experience transition semantics |
| Composition layer | Human-facing authoring, contract linking, fixtures/scenarios, provenance, infinite-canvas projection, cross-artifact diagnostics, and playback | Redefining Spock or Uhura semantics |

Spock and Uhura are sibling language/runtime projects. Either can be checked or
tested independently. The composition layer combines them through versioned
contracts.

Uhura lives in its own dedicated repository; Spock, its canonical provider,
is developed in its own repository alongside it.

## 3. Uhura project components

Uhura is one semantic project but not one undifferentiated implementation.
Conforming work is expected to separate:

1. **Source language:** presentation, components/templates, explicit UI
   machines, bindings, and imported ports.
2. **Checker/compiler:** parsing, resolution, type checking, static analysis,
   extraction, lowering, formatting, and diagnostics.
3. **Checked IR:** a versioned, target-neutral executable representation.
4. **Uhura Core:** the deterministic and I/O-free experience runtime.
5. **Renderer protocol:** semantic view snapshots or patches and semantic input
   events.
6. **Host-driver protocol:** service commands and platform intents whose
   effects occur outside the core.
7. **Bindings:** adapters for Spock contracts, fixtures, and other compatible
   service providers.
8. **Widget catalogs:** independently versioned semantic capabilities informed
   by proven implementations without copying their runtime APIs.

Keeping these artifacts in one project gives them one semantic authority.
Keeping their contracts separate prevents UI behavior, platform behavior, and
backend behavior from leaking into each other.

Presentation, UI machines, and external ports SHOULD remain separately visible
in source or modules. Co-location MUST NOT make widget event attributes a
hidden general-purpose programming language.

## 4. What Uhura may describe

Subject to future grammar RFCs, Uhura is intended to describe:

- semantic widget trees, properties, slots, accessibility, and interaction
  intent;
- reusable components or finite presentation templates;
- immutable lexical values, collection rendering, and structural selection;
- typed reads from explicit external projections;
- non-authoritative UI-session state and its initial values;
- event-driven UI transitions, guards, and local derived values;
- pages, logical routes, and surface orchestration such as dialogs, sheets,
  popovers, menus, tooltips, prompts, and toasts;
- form drafts, local validation presentation, submission lifecycle, pending and
  error states;
- optimistic overlays that remain subordinate to versioned external truth;
- typed service commands and platform intents, without implementing their
  effects;
- typed handling of command outcomes and external projection updates;
- renderer-independent focus, scroll, navigation, and announcement intents;
- references to external localized messages; and
- bounded fixture/scenario projection for a static infinite canvas.

Runtime interaction may continue indefinitely across events. Every individual
core step, source check, and bounded static projection must terminate within
specified resource limits.

## 5. What Uhura cannot own

Uhura source and Core MUST NOT:

- define authoritative records, database schemas, permissions, or security
  policy;
- commit a transaction or claim a business operation succeeded before its
  authoritative outcome arrives;
- own durable cross-user or cross-device product truth;
- perform hidden network, storage, filesystem, clock, randomness, clipboard,
  URL-history, or device I/O;
- contain secrets or transport credentials;
- lay out, paint, hit-test, or instantiate platform-native controls;
- depend on DOM, React, Flutter, SwiftUI, or another renderer's object model;
- execute arbitrary host-language code, reflection, or ambient callbacks; or
- redefine an imported Spock command, projection, refusal, or outcome.

Uhura checkpoints may preserve a UI session without making that state
authoritative. Persistence alone is not authority: the question is whether the
state can alter product truth or another actor's reality.

## 6. State ownership

No fact may be authoritative in both Uhura and an external system.

| Concern | Semantic owner |
|---|---|
| Selected tab, modal/surface stack, local filter | Uhura |
| Form value, dirty/touched state, local validation display | Uhura |
| Submission pending state and correlation identifiers | Uhura |
| Optimistic overlay and rollback/rebase policy | Uhura |
| Logical page, route, or navigation state | Uhura |
| Durable record, accepted mutation, authorization result | Spock or another authoritative service |
| Shared workflow or resumable cross-device progress | Spock or another authoritative service |
| Hover, pressed animation, caret, IME, pointer capture | Renderer |
| Physical scroll offset, measurement, row realization | Renderer |
| URL/history, clock, network, storage, clipboard | Explicit host driver |

A concern can cross boundaries without gaining two owners:

- An optimistic `liked` flag is a correlated Uhura overlay until Spock accepts
  it; the accepted flag is Spock truth.
- A form draft is Uhura state; authoritative validation and commit are Spock
  behavior. A deliberately server-saved draft becomes Spock state.
- Infinite scrolling uses renderer viewport observations, Uhura request/window
  coordination, and Spock-owned query/cursor semantics and canonical records.
- Logical navigation belongs to Uhura; browser history is driven through an
  explicit host intent and returns location changes as events.

## 7. Formal core

Let:

- `P` be a checked Uhura program;
- `U` be the current non-authoritative UI-session state;
- `X` be the latest typed external projection set;
- `E` be one semantic event, external update, or command outcome;
- `V` be a stable-keyed, renderer-neutral semantic view;
- `C` be a finite ordered list of typed service commands;
- `I` be a finite ordered list of platform intents;
- `G` be diagnostics; and
- `T` be an observable execution trace.

The abstract Uhura Core step is:

```text
step-u(P, U, X, E) -> (U', V, C, I, G, T)
```

For fixed versioned inputs, the result must be deterministic. Core has no
ambient I/O. Drivers execute `C` and `I`; results return as later events or
projection updates.

An authoritative service such as Spock evolves separately:

```text
invoke-s(S, command) -> (S', outcome, projection-update, effects)
```

Uhura may predict a presentation result, but it cannot substitute that
prediction for `S'` or the authoritative outcome.

Concrete event queueing, run-to-completion semantics, conflict/reentrancy
rules, cancellation, command correlation, stale projection handling, and
checkpoint migration remain open RFC topics.

## 8. Headless rendering boundary

Headless does not mean handing raw state and source to each renderer. Uhura Core
must evaluate components, templates, collection rendering, structural
selection, bindings, and UI state into `V`. Otherwise every renderer would
silently become another Uhura interpreter.

`V` describes semantic controls and stable identity. A renderer maps it to
static shapes, web controls, or native controls and emits catalog-defined
semantic events.

The following remain renderer concerns unless a future portable contract
promotes a semantic intent:

- layout algorithms and text measurement;
- paint and compositing;
- native accessibility API mapping;
- hit testing and pointer capture;
- caret, selection, and IME mechanics;
- animation frames and physical scroll position; and
- virtualization and platform resource lifetime.

Canonical runtime behavior should be specified using revisioned full view
snapshots. View patches may be an optional transport optimization with an
explicit base revision and snapshot fallback.

## 9. Contract ownership

“Uhura is the contract holder” means that Uhura owns:

- source and checked-IR semantics;
- UI state, event, transition, and trace semantics;
- the types and linking rules for required external ports;
- semantic view and renderer protocols;
- host command/intent envelopes and settlement rules; and
- checkpoints, diagnostics, revisions, and compatibility rules.

It does not mean that Uhura owns every imported contract. Spock owns the
meaning of the projections, commands, outcomes, and refusals it exports. Uhura
declares requirements against those exports. A language-neutral linker checks
that a selected provider satisfies them.

Fixtures and alternative service implementations may satisfy the same required
ports. Uhura Core must not require Spock runtime objects or Spock source syntax.

The composition layer may host the linker and surface its diagnostics, but the
linker should be usable independently and must not invent compatibility.

## 10. Vocabulary

These terms are deliberately distinct:

| Term | Meaning |
|---|---|
| **projection** | A typed, externally owned read model supplied to Uhura |
| **event** | A fact delivered to Uhura, such as `save-requested` or `location-changed` |
| **transition** | A deterministic change to Uhura-owned UI state |
| **command** | A typed request to an authoritative service |
| **outcome** | The service-owned result or refusal returned for a command |
| **intent** | A typed request to a platform capability such as history, focus, or clipboard |
| **semantic view** | The evaluated, renderer-neutral widget tree |
| **renderer** | An implementation that realizes a semantic view on a target platform |
| **driver** | An implementation of explicit service or platform capabilities |

The generic term `action` should not collapse events, transitions, commands,
intents, and outcomes into one ambiguous mechanism.

## 11. Checkability and extraction

The toolchain is expected to parse and check, without running application code:

- grammar and module integrity;
- names, types, component/template calls, slots, and widget contracts;
- bindings against imported projection types;
- event and command payloads;
- machine states, reachable transitions, exhaustiveness, and obvious dead ends;
- command/outcome correlation shapes;
- stable keys for collection and component identity;
- required renderer and host capabilities;
- dependency and capability closure; and
- explicit resource limits for static projection.

It should be possible to extract a graph of views, UI states, events,
transitions, external reads, commands, outcomes, messages, and capabilities.
When linked with Spock exports, the composition layer can infer and validate
where flows connect and how authoritative data is consumed. Inference may
propose links; only explicit or mechanically proven links become part of a
checked bundle.

Checkability does not prove usability, aesthetics, backend security, service
availability, or that a business workflow is desirable.

## 12. Authoring principles

Uhura is intended for human and LLM authors. The language should prefer:

- lowercase kebab-case names wherever the host grammar permits;
- explicit imports and lexical scope;
- a closed, regular grammar with no ambient context;
- one canonical formatter and stable source locations;
- structured, machine-readable diagnostics with deterministic repair spans;
- bounded constructs and explicit capabilities;
- semantic names rather than renderer-specific implementation names;
- separable presentation, machine, and port definitions; and
- version-pinned catalogs and contracts instead of implicit latest behavior.

The source format must be selected on these properties, not on novelty. The
`.uhura` suffix is reserved for canonical Uhura source. If a future RFC adds an
XML interchange serialization, it should use an explicit compound suffix such
as `.uhura.xml` rather than make XML the hidden meaning of `.uhura`.

## 13. Source comments, documentation, and markup annotations

This section incorporates accepted
[RFC 0003](../rfcs/0003-source-comments-docs-and-annotations.md). It applies
to `.uhura` and `.examples.uhura` source. Its implementation is pending.

### 13.1 Three source tiers

Uhura distinguishes:

| Tier | Purpose | Checked authoring metadata |
|---|---|---|
| Ordinary comment | Formatter-preserved source trivia | No |
| Doc comment | Singular documentation of a declaration or declared member | Yes |
| Markup annotation | Ordered, kinded note on one precise markup occurrence | Yes |

The lexical forms are:

| Source region | Ordinary | Doc | Annotation |
|---|---|---|---|
| DSL regions | `// …` and `////…` at comment-bearing boundaries | `/// …`; `//! …` in the file preamble | None |
| Markup | `<!-- … -->` | None | `<!-- @kind … -->` |
| `<style>` body | CSS syntax, outside this subsystem | None | None |

Ordinary comments and metadata are structural trivia: they do not count as
markup nodes, children, roots, statements, handlers, expressions, or
toward any bounded construct count.

Exactly `///` is an outer doc only when a fourth slash does not follow. These
lexical classes apply in every DSL stream, including braced interpolation,
braced attribute values, event bindings, arguments, and block heads. A
non-empty `///` run in such an expression remains doc metadata and is
incompatible because the expression is not documentable. `//!` documents the
source module and every non-empty run MUST occur before the first non-comment
syntactic item. Entirely empty doc runs emit no metadata or placement
diagnostic. `// @kind …` remains ordinary in DSL mode.

The ordinary-comment boundaries are closed:

| Context | Legal immediately before |
|---|---|
| Module | header; complete top-level `use`; `props`/`emits` head; route `param`; `store`; first markup node or `<style>`; EOF |
| `props`/`emits`/`state` body | respectively a prop, emitted event, or state field; `}` |
| Event/handler parameter list | first parameter; later parameter after the preceding comma; `)` |
| `store` body | `state` head; handler; `}` |
| Handler body | complete statement; `}` |
| Examples module | complete top-level `use`; named example; EOF |
| Example body | complete example clause; `}` |

Inner port-import items, arguments, example-clause sub-lists, types,
expressions, guards, event bindings, and tokens within one complete item are not
comment-bearing. A comment MUST NOT occur between a parameter and its comma.
Invalid placement is `UH0001 syntax/unexpected-token`. A comment before the
first markup node or `<style>` is trailing module DSL trivia and remains before
that transition after formatting.

### 13.2 Annotation kinds

An annotation kind contains 1–64 ASCII bytes and MUST match:

```text
lower (lower | digit)* ("-" (lower | digit)+)*
```

Kinds are case-sensitive lowercase ASCII. Every valid kind is accepted and
preserved exactly. `annotation` is the conventional general-purpose kind, but
`doc`, `rationale`, `review-note`, and other kinds have the same language
behavior and gain no Uhura Core or runtime semantics.

Therefore a localized markup note is written as:

```uhura
<!-- @annotation The primary action. -->
<button />
```

This is also valid:

```uhura
<!-- @doc The primary action. -->
<button />
```

A markup `@doc` remains annotation-class metadata whose literal kind is `doc`;
it does not turn the element occurrence into a documented declaration. A
comment whose first non-whitespace content does not begin with `@` is ordinary.
If it begins with `@` but has an invalid kind or an empty payload, it MUST be
diagnosed rather than silently treated as ordinary.

XML-shaped comments are legal only in markup sibling positions, including a
trailing ordinary comment before a parent or arm close. They are not legal
inside a start/close tag, attribute list, braced expression, or block head. A
well-formed comment in such a position receives `UH0001`; malformed bodies or
annotation markers receive `UH0016`.

Annotation payloads are opaque UTF-8 text. They contain no interpolation,
attributes, Markdown, or directive mini-language. XML-shaped comments MUST
terminate with `-->`, MUST NOT contain `--` in their body, and MUST NOT end
their body with `-`.

### 13.3 Normalization

Horizontal whitespace means ASCII space or tab, a blank line contains only
horizontal whitespace, and CRLF or bare CR normalizes to LF before body
classification. Marker whitespace means horizontal whitespace or LF; other
Unicode whitespace remains payload.

For each `//!` or `///` line, remove the sigil, remove at most one immediately
following ASCII space, remove trailing horizontal whitespace, normalize the
line ending to LF, and join the run in source order. Trailing empty lines are
removed; interior empty lines remain. An empty normalized doc run emits no
metadata but still separates runs of the opposite doc form. The empty run
itself receives no dangling, misplaced-inner-doc, or incompatible-target
diagnostic; surviving non-empty runs are checked independently. Canonical
formatting omits the empty run's doc-sigil lines while retaining any
interleaved ordinary comments at their legal boundary.

An ordinary one-line markup comment removes leading and trailing horizontal
whitespace from its body. An ordinary multiline markup comment removes all
blank boundary lines, trailing horizontal whitespace on each line, and the
common ASCII-space indentation of non-empty lines. Interior line breaks and
blank lines remain.

A markup annotation first removes whitespace before its marker. Its kind MUST
be followed by at least one ASCII space, tab, or LF; the marker and first such
separator are removed. Its remaining body uses ordinary markup-comment
normalization and MUST be non-empty.

A doc run contains one doc form and may span only whitespace and ordinary
comments. The opposite doc form splits the run. Independent doc runs resolving
to one target are incompatible rather than merged. A doc metadata span is the
half-open envelope from its first sigil through the final doc token; it excludes
the final line ending and may contain transparent whitespace or ordinary
comments.

### 13.4 Attachment and targets

Docs and annotations attach forward to the next compatible target in the same
syntactic item or markup sibling list. Whitespace and ordinary comments are
transparent. Metadata MUST NOT skip an incompatible construct and MUST NOT
cross a `}`, parameter-list open or close, DSL-to-markup transition, markup
parent, block arm, markup-to-style transition, or end-of-file boundary.
Reaching a close, arm, transition, or EOF with no construct to target is
dangling; encountering an incompatible construct is incompatible. An opening
delimiter after an ineligible construct has begun belongs to that construct,
not the dangling case. Closing delimiters, arm labels, and region-transition
markers such as `<style>` are boundaries, not incompatible constructs. A
parameter doc MUST be inside its parameter list immediately before the
parameter. Thus a doc between an event/handler name and `(` is incompatible,
while one after `(` and immediately before `)` is dangling.

Documentation targets are closed:

- the source module via preamble `//!`;
- the `component`, `page`, or `surface` declaration;
- a prop, emitted-event, emitted-event payload parameter, or route-parameter
  declaration;
- a `store` scope or state field;
- an event or outcome handler or its parameter; and
- a named example declaration.

Imports, grouping sections, statements, expressions, example clauses, markup
occurrences, style blocks, and CSS are not documentable. A signature with any
documented parameter or ordinary parameter-list comment MUST use the RFC 0003
multiline parameter form.

```uhura
emits {
  submitted(
    /// The submitted record.
    record: id
  )
}
```

Markup annotation targets are closed: catalog elements, component invocations,
and complete `if`, `each`, and `match` blocks. All kinds, including `@doc`, use
this same table.

Attributes, event bindings, arguments, expressions, text/interpolation runs,
match arms, `<style>`, CSS constructs, and parser recovery nodes are not
annotatable.

An annotation after an opening element attaches to the next child, not the
containing element. Annotations are repeatable and retain target-local source
order; a documentable target has at most one normalized doc.

### 13.5 Formatting and checked metadata

The canonical formatter emits an ordinary DSL comment on its own line at the
following item's indentation. It emits trailing list trivia at member
indentation inside the closing delimiter and trailing file trivia at top-level
indentation; module transition trivia remains immediately before the first
markup node or `<style>`. A trailing source comment therefore moves to a
boundary line before the next item, close, or transition. The formatter emits
docs and annotations immediately before their target, preserves metadata
order, and is idempotent.

It preserves the body after an ordinary DSL comment's first `//`, except for
trailing horizontal whitespace and line-ending normalization; leading body
spacing and `////` slash dividers remain unchanged.

Markup layout is chosen from normalized text. Text without LF uses
`<!-- text -->` or `<!-- @kind text -->`; empty ordinary text uses `<!-- -->`.
Text containing LF uses separate opening/marker, normalized-body, and closing
lines at the target's indentation.

Checking MUST produce a separate authoring-metadata projection. Each entry
contains its class (`doc` or `annotation`), kind, normalized text, metadata
span, canonical project-relative file, target class and span, and target-local
order. The metadata span of a markup annotation is its full
`<!-- … -->` span. Ordinary comments do not enter this projection.

For `//!`/`///`, class and kind are both `doc`. For every tagged markup
comment, class is `annotation` and kind is the exact marker—even when that kind
is `doc`. Consumers MUST distinguish class from kind. `order` is the zero-based
ordinal among entries on one target; a doc has ordinal `0`. The source-module
target span is the full file span. Target class uses this closed vocabulary:

```text
source-module
component-declaration | page-declaration | surface-declaration
prop-declaration | emitted-event-declaration | emitted-event-parameter
route-parameter | store-scope | state-field
event-handler | outcome-handler | handler-parameter
example-declaration
catalog-element | component-invocation
if-block | each-block | match-block
```

Target spans are half-open byte spans and, except for the source module,
exclude leading metadata/trivia, trailing trivia, and line endings:

| Class | Span |
|---|---|
| Source module | byte `0` through file length, including preamble metadata |
| Header declaration | kind keyword through final header token |
| Prop | name through final type token |
| Emitted event | event name through `)` |
| Emitted-event/handler parameter | name through final type token when written, otherwise name; excludes comma |
| Route parameter | `param` through final type token |
| Store/handler/example | its keyword through body `}` |
| State field | name through final initializer token |
| Element/component invocation | opening `<` through self-close or matching closing tag, including children |
| `if`/`each`/`match` block | opening block `{` through matching close `}`, including arms |

The authoring projection is not canonical runtime `ProgramIr` or semantic
`V`. Editing valid comments, docs, or annotations MUST NOT change runtime IR,
view hashes, `step-u`, commands, intents, traces, or runtime diagnostic codes,
messages, and semantic outcomes. Source locations may shift with surrounding
text. Wire encoding, durable target identity, visibility, rendering, and
collaborative lifecycle are separate decisions.

### 13.6 Diagnostics

The subsystem reserves:

| Code | Rule |
|---|---|
| `UH0016` | `syntax/malformed-markup-comment` |
| `UH0017` | `syntax/dangling-metadata` |
| `UH0018` | `syntax/misplaced-inner-doc` |
| `UH0019` | `syntax/incompatible-metadata-target` |

Malformed markup comments or annotations MUST NOT silently degrade to an
ordinary comment or text node. Non-boundary ordinary DSL comments use the
existing `UH0001` diagnostic, as do well-formed XML-shaped comments outside a
markup sibling position.

Precedence is malformed markup (`UH0016`), non-empty inner doc after the
preamble (`UH0018`), incompatible construct (`UH0019`), then close/end boundary
without a target (`UH0017`). Empty doc runs receive none of these metadata
diagnostics.

## 14. Open decisions

The foundation deliberately leaves these unsettled:

- complete source syntax and serialization beyond the accepted comment,
  declaration-doc, and markup-annotation subsystem in §13;
- component versus pure-template semantics;
- reducer, statechart, or hybrid UI machine model;
- expression language and totality restrictions;
- async command ordering, concurrency, cancellation, and retries;
- checkpoint and state-preserving Play hot-reload compatibility;
- live rebuilding of static Editor previews, proposed separately by
  [RFC 0002](../rfcs/0002-live-static-editor-preview-rebuilds.md);
- checked IR and stable ABI representation;
- widget taxonomy and catalog versioning;
- message and localization model, including MessageFormat 2;
- renderer and host capability negotiation;
- Spock import/export schema and linker ownership packaging;
- static projection bounds and scenario format;
- long-term Rust/Wasm and TypeScript host release packaging beyond the
  checked-in v0 browser artifacts; and
- public naming, licensing, release, and repository extraction policy.

Each material decision requires focused research, examples, counterexamples,
and executable conformance cases before it enters a versioned specification.

## 15. Historical research evidence

The [application-scale stress-test requirements](../working-group/application-scale-stress-test.md)
record which findings from an earlier application-scale stress study remain
valid, which responsibilities move into Uhura Core, and which assumptions and
syntax must not be carried forward.
