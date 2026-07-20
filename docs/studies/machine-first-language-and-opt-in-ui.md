# Machine-first language, opt-in UI, and explicit framework features

- **Status:** Historical research input incorporated by RFC 0004
- **Lifetime:** Disposable study
- **Scope:** Language topology and candidate constraints, not exact grammar,
  framework catalogue, widgets, or implementation
- **Doctrine:** [Mission and identity](../doctrine/mission.md),
  [authoring ergonomics](../doctrine/authoring.md), and
  [design principles](../doctrine/principles.md)
- **Related study:**
  [Language necessity and surface reuse](language-necessity-and-surface-reuse.md)
- **Problem corpora:** [Program harnesses](../../examples/programs/README.md)
  and [application harnesses](../../examples/applications/README.md)
- **Fixed inputs:** The extension is named `ui`; web is its only supported
  presentation target; meta-framework semantics are imported feature by feature
  rather than activated ambiently
- **Decision:** [RFC 0004](../rfcs/0004-standalone-machine-core-and-source-composition.md)
- **Active candidate:** [Uhura 0.4](../spec/drafts/0.4/)
- **Authority:** Research only; RFC 0004 owns the adopted boundary and the
  active candidate owns its proposed exact source

## Recommendation

Uhura should first be independently checkable and executable as a pure
state-machine language. A valid program should be able to declare state,
admitted inputs, transitions, observations, and requested consequences without
declaring a user interface, importing a widget catalogue, or requiring a
renderer.

Here, **pure** means that presentation semantics and ambient host authority are
not prerequisites of the machine language. It does not assert Turing
completeness, require a finite enumeration of states, or select one reducer,
statechart, actor, or transition topology.

User interfaces should remain a first-class Uhura application domain through a
checked extension named **`ui`**. That extension supports the web platform
only. First class should mean excellent language support, tooling, and
defaults, not an ambient assumption in every program. Source that uses UI
vocabulary should explicitly activate `ui`.

Activating `ui` should not silently activate a meta-framework. Routing, links,
location and search-parameter observation, navigation, data-loading lifecycles,
form actions, session integration, or another framework-level behavior should
enter source through an explicit import of the feature that defines it. A file
convention, project template, renderer selection, or dependency elsewhere in
the project must not make those semantics ambient.

The activation may eventually be an import, pragma, module header, project
capability, or another checked form. This study does not choose that form. It
recommends only the semantic boundary:

```text
machine language
  + explicit ui extension
  + explicit framework-feature imports
  -> web UI program
```

A filename, compilation target, renderer selection, or the accidental
presence of markup should not silently activate another language surface.

This document is not a harness. The language-neutral
[L0–L2 programs](../../examples/programs/README.md) remain the pure behavioral
problems against which candidate languages are tested. The
[A0 Return Desk](../../examples/applications/a0-return-desk/) separately tests
whether a candidate composes into a practical application with explicit UI and
framework dependencies. This recommendation states how those two axes relate.

## Three-layer source topology

The recommendation distinguishes three source layers:

| Layer | Availability | Responsibility |
|---|---|---|
| Machine core | Always | State, admitted inputs, transitions, outcomes, observations, and requested consequences |
| `ui` extension | Explicit activation | Checked web presentation and translation between semantic web interactions and machine inputs |
| Framework features | Explicit named imports | Independently meaningful application semantics such as links, route state, search parameters, navigation, or request lifecycles |

The machine core is a complete language rather than the hidden implementation
language of a framework. The `ui` extension is a presentation capability, not a
framework prelude. A framework feature may depend on `ui`, a host observation,
a command capability, another imported feature, or only the machine core, but
that dependency closure must be statically inspectable.

This is similar in topology to how Next.js code names the specific semantics it
uses:

```js
import Link from "next/link"
import { useSearchParams } from "next/navigation"
```

Next.js documents [`Link`](https://nextjs.org/docs/app/api-reference/components/link)
as an imported component with client-navigation behavior and
[`useSearchParams`](https://nextjs.org/docs/app/api-reference/functions/use-search-params)
as an imported read-only view of URL search parameters. Uhura should borrow the
dependency visibility, not React components, hooks, JavaScript execution,
Next.js package names, or its client/server lifecycle.

An illustrative Uhura-shaped source might make the layers visible as:

```text
use ui
import link from "<framework>/link"
import search-params from "<framework>/navigation"
```

This is not proposed grammar, and the module names are placeholders. The
required property is that a reader, checker, formatter, and agent can determine
from bounded local source which extra semantic contracts are in use.

### Feature granularity

“Feature by feature” means that importing one framework must not grant its whole
semantic universe. An imported module may expose multiple tightly coupled
symbols, but each usable capability must be present in the explicit import
surface. In particular, activating `ui` must not by itself provide:

- route or URL ownership;
- link interception or prefetching;
- location, path, or search-parameter observation;
- navigation history mutation;
- loaders, actions, caching, invalidation, or request settlement;
- authentication or session state;
- clock, network, storage, clipboard, or device authority; or
- file-convention lifecycle behavior.

Some of these may later become first-party Uhura features. First-party
distribution does not make them core or ambient.

### Import contract

Every framework-feature proposal should state:

1. its canonical name, version, provider, and dependency closure;
2. the types, values, elements, observations, inputs, commands, or syntax it
   contributes;
3. which facts it owns and which external facts it only observes;
4. its deterministic lowering into the machine, `ui`, renderer, or host
   boundary;
5. any runtime capability, availability, fallback, and unsupported behavior;
6. how fixtures, examples, traces, checkpoints, and headless checking represent
   it; and
7. its diagnostics, compatibility rules, and removal behavior.

Importing a feature may introduce vocabulary and requirements. It must not run
initialization code, mutate state, perform I/O, or acquire host authority merely
because the import exists. A command still requests external work, and an
external observation or outcome still returns through a declared boundary.

Transitive implementation dependencies may be resolved by the lockfile, but
they should not silently place their authoring vocabulary in scope. A future
prelude or bundle, if one is justified, must expose its exact resolved feature
closure rather than recreate an ambient meta-framework under a shorter name.

## Why the boundary matters

### The core can be judged independently

A machine language should not need HTML, components, styles, pages, or
widgets to explain its transition semantics. Keeping the behavioral core
independent makes termination, determinism, atomicity, replay, correlation,
and effect authority reviewable without renderer concerns obscuring them.

The existing program harnesses deliberately contain no UI oracle. A future
candidate should be able to solve and execute those programs without
inventing a visual wrapper.

### First-class support does not require ambient syntax

`ui` and first-party framework features can still be designed, documented,
checked, formatted, previewed, and shipped by Uhura itself. Explicit activation
and imports do not make them third-party afterthoughts. They tell a reader and
a checker which additional concepts are available and which guarantees they
carry.

This is especially useful for human and agent learning. A bounded declaration
reduces the grammar and semantic vocabulary that must be assumed at any point,
and it prevents an HTML-shaped form from importing undeclared browser or
JavaScript expectations.

### The web runtime remains a boundary

`ui` may derive a checked web presentation and translate semantic interaction
into declared machine inputs. It must not grant the machine ambient access to
the DOM, layout, network, clock, storage, or device APIs. Web-only support
removes non-web renderer obligations; it does not move browser authority into
the machine.

One abstract decomposition is:

```text
step    : Machine × State × External × Input
        -> NextState × Result × OrderedList<Output>

observe : Machine × State × External
        -> Observation

present : Ui × Machine × State × External
        -> WebPresentation
```

`present` is downstream of machine state and declared external observations.
Browser interaction returns through declared inputs. HTML, CSS, DOM, Canvas,
accessibility APIs, layout, and paint remain concerns of the checked `ui`
contract or its web runtime according to a future proposal. The exact types
and functions above are illustrative, not a proposed Uhura kernel.

### Relationship to current doctrine

RFC 0004 adopted this study's distinction and the
[mission](../doctrine/mission.md) now describes Uhura as a frontend-focused
builder system built on a standalone machine language. Presentation remains
downstream whenever a program activates it, but it is no longer a prerequisite
of a complete core program.

The core-language boundary does not require the Uhura product, examples, or
distribution to become domain-neutral. Uhura may remain unusually good at
building interfaces while its core calculus is useful and testable without
one. Product focus and language dependency are different decisions.

The adopted interpretation is that frontend dedication names the product and
standard distribution while the core remains presentation-independent. This
does not turn Uhura into a general-purpose language or require the product to
support non-UI application domains.

## Fixed name and target

The extension is named **`ui`**. Candidate proposals should treat that name as
an input, not compare it against `html`, `dom`, `view`, or `presentation`.
Whatever activation form is selected must refer to the extension as `ui`.

`ui` is intentionally broader than literal HTML. A web interface may use HTML
semantics alongside Uhura-owned widgets, interaction bindings, accessibility
contracts, styles, surfaces, and presentation-level animation. Naming that
whole surface `html` would falsely imply that every admitted primitive is an
HTML element. Naming it `dom` would expose the browser's mutable runtime object
model at the wrong boundary. Application routing, URL observation, navigation,
and other framework semantics remain separate imports even when they contribute
elements or bindings that `ui` can present.

Web is the only supported presentation target. This removes a family of
renderer profiles, non-web fallback policy, and cross-platform capability
negotiation from the language-design problem. HTML, CSS, DOM, Canvas, and web
accessibility APIs may still occupy distinct layers inside the one `ui`
extension and its runtime.

An illustrative spelling such as:

```text
use ui
```

communicates the intended topology, but it is not a proposal. Current Uhura
uses `use` for named dependencies, and a future grammar may find an import,
pragma, profile header, capability declaration, or project-level mechanism
more regular. The activation form is open; the name `ui` is not.

## Minimum obligations for a future proposal

A concrete proposal should demonstrate all of the following:

1. A machine-only program parses, checks, executes, traces, and can be tested
   without loading `ui` or a browser runtime.
2. UI syntax is unavailable until `ui` is explicitly activated under a
   statically discoverable scope.
3. `ui` activation does not imply routing, navigation, loading, session, or
   another meta-framework feature.
4. Every framework feature used by source is present in an explicit named
   import, with a statically inspectable version and dependency closure.
5. Activation and imports alone introduce no state changes, causes, effects,
   initialization, or authority.
6. UI events enter through declared machine inputs; bindings are not hidden
   mutation callbacks.
7. UI presentation derives from declared state and observations and does not
   acquire independent behavior or effect authority.
8. The `ui` extension and each framework feature define their accepted
   vocabulary, lowering, capability requirements, availability, and fallbacks.
9. Diagnostics distinguish an unknown construct, missing `ui` activation,
   missing feature import, unavailable host binding, and incompatible feature
   version.
10. Headless machine conformance remains possible even when the primary Uhura
   distribution includes first-party UI tooling.
11. No non-web presentation target or second presentation-extension taxonomy is
   required by the proposal.

The proposal should test these obligations against the L0–L2 programs and the
language-neutral [A0 Return Desk](../../examples/applications/a0-return-desk/).
The programs establish behavioral independence; A0 tests whether the extension
and explicit application features remain ergonomic rather than ceremonial.

## Open decisions

This recommendation intentionally leaves these questions unresolved:

- whether `ui` activation is an import, pragma, header, capability, or manifest
  declaration;
- whether activation is per file, module, package, or project;
- how locally visible a project-level activation must be;
- the exact framework import grammar, module identity, version constraint, and
  lockfile representation;
- how fine-grained feature modules should be and whether explicit symbol imports
  or qualified module access is canonical;
- whether feature imports are file-, module-, package-, or project-scoped and
  whether re-export or a transparent prelude is ever justified;
- how pure library features, external observations, commands, presentation
  integrations, and host capabilities are distinguished at an import site;
- how semantic elements, HTML vocabulary, styles, components, surfaces,
  animation, and widget catalogues compose inside `ui`;
- where the checked `ui` contract ends and the browser runtime begins;
- what minimal observation model the machine core exposes to non-UI uses;
- how `ui` and framework features are versioned and web capabilities are
  checked;
- which current v0 navigation, routing, projection, and request-lifecycle
  behaviors become separately imported framework features; and
- how the current implicitly UI-oriented v0 source would migrate.

Those are language-design questions for the upcoming candidate review. A
candidate must not assume the current v0 grammar, `use` declarations, element
catalogue, semantic-view IR, or browser-runtime topology. It should not reopen
the `ui` name or add non-web target support as part of this exercise.

## Maturity path

This study records three fixed inputs while the concrete activation, import
grammar, and feature contracts remain open: the core is independently usable,
the web presentation extension is named `ui`, and meta-framework semantics are
imported feature by feature. Candidate comparison should not score alternative
extension names, ambient framework bundles, or non-web presentation targets. If
the boundary survives candidate comparison:

1. an RFC should record the accepted rationale and ownership boundary;
2. durable doctrine should state the resulting core-first identity without
   preserving discarded candidate terminology; and
3. a named version specification should define the exact activation, import
   model, semantics, diagnostics, and conformance behavior.

Until then, this document informs proposals but does not constrain the current
implementation or define the exact syntax of `ui` or framework-feature imports.
