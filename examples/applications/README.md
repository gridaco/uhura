# Application harnesses

- **Status:** Non-normative language-design corpus
- **Current form:** Language-neutral Markdown application specifications
- **Implementation status:** No accepted Uhura solutions or runtime fixtures
- **Scope:** Practical composition of the machine language, `ui`, and explicit
  application features

Application harnesses are bounded, practical product problems used to test
whether a candidate machine language remains honest and ergonomic when it
becomes a web application. The problem is the authority; a current Uhura
project, a redesigned language, or a familiar-language baseline is an answer
sheet.

This is a separate comparison axis from the
[program harnesses](../programs/):

| Axis | Question |
| --- | --- |
| L0–L2 programs | Can the candidate express and check the required computation? |
| A0 applications | Can that computation compose into a practical application without hidden state, authority, or framework behavior? |

`A0` is therefore not `L3`. The `L` series increases standalone machine
pressure. The `A` series crosses an orthogonal boundary into location,
lifecycles, external authority, presentation, and imported application
semantics. Calling the first application `L3` would wrongly suggest that those
concerns belong to a more advanced machine kernel.

## Current portfolio

| Harness | Application | Primary pressure |
| --- | --- | --- |
| A0 | [Return Desk](a0-return-desk/) | One session-owned transaction draft across URL-owned steps, logical route scopes, revisioned external truth, a temporary surface, and a correlated settlement |

The A-series identifiers organize application-transfer problems. They do not
describe user skill, language maturity, product importance, or a required
implementation order.

## What belongs here

An application harness is more concrete than a pure program and more controlled
than a product demo. It includes a recognizable job, fixed fixtures, semantic
locations and interactions, external deliveries, application-owned state,
requested consequences, lifecycle rules, and presentation observations. It
deliberately excludes visual design and production infrastructure that do not
help distinguish language models.

Each harness must:

1. have independent product meaning rather than be a language feature list;
2. define what the application, external authorities, host, and renderer own;
3. freeze semantic locations, inputs, deliveries, consequences, and lifecycle
   behavior;
4. distinguish static examples, reachable scenarios, and resumable
   checkpoints;
5. provide canonical and adversarial traces with a semantic, non-pixel oracle;
6. name its non-goals aggressively enough to remain implementable by every
   serious candidate;
7. require every non-core semantic dependency to be visible without
   prescribing candidate import grammar; and
8. remain meaningful if Uhura is replaced or redesigned.

A harness must not select a widget catalogue, CSS system, component hierarchy,
framework package name, state topology, effect syntax, external-observation
representation, actor model, or IR merely to make its answer easier to write.

## Authority and boundary discipline

Every A-series problem separates at least four kinds of fact:

| Plane | Typical owner |
| --- | --- |
| Experience coordination | The application machine |
| Product truth and operation acceptance | An external service |
| Location and host deliveries | An explicitly declared web/framework boundary |
| Layout, focus mechanics, paint, and physical input | The renderer |

Discarding the application session must not change product truth. Conversely,
an external service must not become the hidden owner of drafts, open
application surfaces, logical progress, pending correlations, or other
experience coordination merely because a familiar framework has a cache or
global store.

The harness fixes observable ownership and behavior, not one internal
representation. A candidate may use immutable external observations, owned
versioned copies, actors, reducers, statecharts, or another checked model if it
preserves authority, replay, ordering, and lifecycle.

## Explicit application dependencies

Machine-only L0–L2 answers must remain valid without loading UI or browser
machinery. An application answer must explicitly activate `ui`, and every
framework-level semantic feature it uses must be present in a statically
inspectable dependency surface.

The harness names required capabilities, such as location observation,
navigation, links, external facts, or request settlement. It does not freeze
whether they are imported separately or from a tightly coupled module, nor
does it freeze spelling, package names, or scope. Imports alone may not
initialize state, perform I/O, navigate, submit work, or acquire ambient host
authority.

Candidate comparisons should plant negative probes for missing `ui`
activation, missing application features, unavailable host bindings, and
incompatible feature versions. The expected diagnostic class must be fixed
before answers are scored.

## Shared evaluation boundary

Candidates receive the same abstract environment:

- declared location and external-fact deliveries;
- semantic user interactions;
- observations of requested navigation and external work;
- explicitly correlated service settlements; and
- the same fixtures, traces, lifecycle events, and static examples.

Candidate-specific adapters may translate that environment into a candidate's
checked boundary, but they cannot add product behavior, infer missing
correlation, repair stale state, or hide ambient framework work. Adapter code
and runtime-provided semantics must be disclosed separately from application
source.

Arbitrary JavaScript or another unchecked escape hatch is not a conforming
answer. If the application can be completed only by moving required behavior
outside the candidate language, record that as a candidate failure.

## Conformance oracles

Application conformance is headless and semantic, but it does not collapse all
observable facts into the UI.

A **presentation oracle** may include:

- the current logical location;
- externally supplied information that is safe to present;
- draft values and derived conflicts;
- enabled semantic actions;
- pending, refused, unavailable, or completed status;
- the product content of an open temporary surface; and
- product notices and links.

DOM shape, element names, CSS, pixels, animation, focus-ring appearance, and
component decomposition are not part of the oracle unless a future harness
explicitly makes one of them its subject.

A separate **step oracle** records the classification and finite ordered
consequences of each input. A **lifecycle/checkpoint oracle** records internal
identity, correlation, ownership, and allocation facts needed for replay and
stale-input checks. Neither kind of inspection data must be displayed to the
user or copied into persistent state merely to make it observable to tests.

## Examples, scenarios, and checkpoints

These artifacts have different claims:

- An **example** pins a complete presentation context for static design
  inspection. It need not be reachable and must not claim that I/O ran.
- A **scenario** starts from valid initial context and proves reachability by
  applying declared inputs and deliveries.
- A **checkpoint** is a resumable machine and boundary snapshot. Replaying the
  same suffix from it must reproduce the same semantic state, observations,
  requested consequences, and classifications.

An application example must identify its location, external facts,
application-owned state, pending correlations, and route/surface lifecycle
context. A partial bag of convenient values is not a valid pin.

## Candidate comparison

Conformance is pass/fail. Compare ergonomics only among answers that preserve
the complete problem.

Report at least these surfaces separately:

1. application-authored source;
2. explicit `ui` and feature dependency declarations;
3. static examples, scenarios, and checkpoints;
4. candidate-specific fixtures or adapter glue; and
5. compiler, framework, and runtime semantics supplied without application
   source.

Among conforming answers, compare:

- concepts and declarations the author must name;
- invalid states representable in authored source;
- duplicated route, draft, pending, correlation, and instance bookkeeping;
- cause-to-effect reading distance;
- hidden defaults or authority;
- source and token size under one shared counting rule;
- static-preview authoring cost;
- diagnostic locality and repair quality;
- the bounded guide needed for a human or agent to acquire the design; and
- edits required by one pre-registered controlled change.

Compactness cannot compensate for missing behavior, hidden host work, or an
unfairly powerful adapter.

## Why Return Desk is A0

A support queue was considered but deferred. Its filtered collection, detail
page, reply flow, optimistic item mutation, and retraction pressures overlap
substantially with the existing Instagram Feed evidence.

Return Desk instead isolates a different application seam: one transaction
draft must survive several URL-owned steps and route-local instances while an
external order revision changes underneath it. It exercises practical UI and
framework composition without requiring a feed, ranking, pagination, media,
realtime collaboration, or a widget-specific oracle.

## Evolution

The A0 problem statement is frozen while candidate answers are compared.
Changing it requires a product-level or fairness reason independent of whether
one candidate passes.

Accepted semantics eventually belong in a named specification and executable
conformance suite. The application harness remains readable evidence; it does
not acquire language authority.
