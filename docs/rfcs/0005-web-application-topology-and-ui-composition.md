# RFC 0005: Web application topology and pure UI composition

- **Status:** Accepted
- **Decision date:** 2026-07-22
- **Scope:** Opt-in Web application source topology, generated route and entry
  composition, and reusable UI presentation functions
- **Depends on:** [RFC 0004](0004-standalone-machine-core-and-source-composition.md)
- **Does not select:** Layouts, slots, loaders, server rendering, component
  state, or another runtime scheduler

## Context

Uhura 0.4 established a standalone deterministic machine language and an
explicit Web `ui` profile. Its first implementation deliberately admitted
only individually named, machine-bound presentations. That was sufficient to
prove the machine kernel and Editor pipeline, but it forced application
authors to maintain route tables, one aggregate presentation, and a flat
module inventory by hand. It also made reusable visual structure either
duplicated source or an observation-coupled presentation.

The earlier Instagram experiment demonstrated that a conventional
`app/**/page.uhura`, `components/`, and `surfaces/` tree is useful authoring
topology. Its page-local stores were incompatible with the later machine-first
decision and are not restored. The useful question is narrower: can a project
layer derive route and presentation composition while the checked program
continues to contain one explicit machine and one deterministic runtime?

## Decision

### The framework is an opt-in project resolver

A root project may select the versioned `web-app` framework profile in
`uhura.toml`. Without that selection, every core and evidence source remains
explicitly mapped exactly as before.

The framework recognizes a closed source tree:

- `app/**/page.uhura` for routed, machine-bound pages;
- `components/**/*.uhura` for pure reusable UI functions;
- `surfaces/**/*.uhura` for pure reusable surface functions; and
- colocated `*.examples.uhura` files for tooling-only evidence registration.

The resolver validates the complete tree, assigns deterministic logical
modules, and produces the same closed source inventory consumed by the
ordinary checker. Unrecognized `.uhura` sources, orphan example files, invalid
dynamic route segments, and collisions are errors. Non-source assets do not
become modules. This is not an ambient recursive source loader.

For a framework project, a path under `app/` is semantic route input. Moving a
page can therefore change application behavior even though ordinary module
file placement remains nonsemantic provenance. The project specification must
keep those two path classes distinct.

### Routes are generated source and remain checked language values

The project names its machine and an ordinary, user-authored `Location` enum
through exact root-package declaration locators. Directory names define path
patterns, a page declaration such as `PostPage` identifies the corresponding
`Location::Post` constructor, and `[parameter]` segments identify constructor
fields. The framework generates a `Routes<Location>` constant. The ordinary
checker still proves constructor totality, field use, and route ambiguity.

The machine must still import the Router vocabulary, configure its Router
port with the generated route value, and expose the committed application
location in its observation. The live host must still bind that port to
browser history. Selecting the framework grants no history, URL, DOM, or host
authority.

Framework admission proves that the configured machine owns a
`Router<Location>` port configured by that exact generated route value. The
ordinary checker already requires a handler for the port's `changed` input.
What that handler commits remains authored machine behavior: the framework
does not inject a state write or claim that handler presence proves the
intended navigation policy. Scenarios remain the executable proof of that
policy.

### One generated application presentation selects routed pages

The framework generates one public machine-bound `Application` presentation.
It selects a page only from the machine's committed `Option<Location>`
observation. `None` selects the root page while the first browser-location
delivery is pending. It does not read browser location independently or
create a second routing state.

The host explicitly selects `Application` like any other presentation. The
generated declaration and route table pass through the ordinary checker and
are part of the checked application artifact; they are not handwritten source
and are not presented as editable files in the Editor.

### UI reuse is a pure presentation-function call graph

Uhura admits two `ui` declaration forms:

1. a machine-bound presentation receives one immutable observation; and
2. a reusable component receives exact typed immutable props and declares a
   finite emitted-event protocol.

A component call supplies every prop and maps every emitted event. A
machine-bound presentation may also call another presentation bound to the
same machine. Calls expand without a wrapper node, runtime instance, state,
lifecycle, context, scheduler, or hidden authority. Pure components cannot
call machine-bound presentations, cross-machine calls are rejected, and the
complete call graph must be acyclic.

Component events compose outward until a machine-bound call site maps them to
one checked machine input. This is a typed expression transformation, not a
JavaScript callback or synchronous machine re-entry.

An explicitly imported public pure component from a locked package follows the
same call rules as a project-local component. Its package identity and source
provenance remain intact; the framework does not copy it into the root source
tree or create a package-specific runtime.

Presentation identity includes the reachable call closure. An unused
component does not change an unrelated presentation hash; changing a called
component does.

### Editor roles remain authoring metadata

Framework paths provide authoritative page, component, and surface roles.
Evidence examples must agree with those roles. Pure component and surface
examples supply checked constant props and render directly; tooling does not
forge wrapper machines to make them look machine-bound.

The Editor continues to derive previews, replay ancestry, annotations, and
provenance from checked artifacts. The generated `Application` entry is not a
separate authoring subject.

## Consequences

- The machine kernel, transaction model, instance model, and Wasm runtime do
  not change.
- Application authors stop duplicating route patterns and aggregate page
  selection while retaining typed, explicit `Location` and Router semantics.
- File placement has a narrowly defined semantic role only inside an opted-in
  framework root.
- UI reuse becomes compact without importing JavaScript closures, component
  state, or a second effect system.
- CLI, standalone host, Editor, Play, and Spock must share one project
  resolver so the same tree cannot mean different programs to different
  tools.

## Deliberate V1 limits

The first framework version has no layouts, slots or child parameters,
loaders, actions, server components, server rendering, route-level state,
component state, lifecycle hooks, context, default or spread props, optional
event mappings, query-field convention, optional route fields, or runtime
component instances. Every `Location` constructor field in V1 is therefore a
named required path field represented by one matching `[parameter]` segment;
the ordinary route-table checker rejects any shape outside that closed
profile. These are design gates, not features implied by the directory names.

There is no compatibility parser or lowering path for the retired UI-first
page/store language. Historical source remains in Git history.
