# Instagram Uhura client

This directory is the independently checkable Uhura 0.4 application inside
the [Instagram framework example](../README.md). It deliberately exercises the
opt-in `web-app@1` topology rather than maintaining routes and one aggregate
presentation by hand.

## Authored topology

- `machine.uhura` is the headless deterministic `Instagram` machine. It owns
  domain state, transitions, commands, deterministic demo data, and the
  committed `location` observation used by application routing.
- `routing.uhura` owns the typed `Location` enum and route-key types. The
  framework checks page paths against this enum.
- `parts.uhura` owns the composed notice state plus its checked dismissal
  consumer. Its `Reads` and `Updates` handles lower into the same aggregate
  machine transaction.
- `presentation.uhura` contains pure shared presentation functions. It owns no
  UI, state, or host authority.
- `example-values.uhura` contains named, typed constants used only as concise
  props by colocated component/surface examples; evidence modules themselves
  retain their restricted declaration role.
- `app/**/page.uhura` contains nine machine-bound pages. Its directory paths
  generate the checked route table.
- `components/*.uhura` contains eight stateless, typed UI components.
- `surfaces/comments-sheet.uhura` contains one stateless surface component.
- Sibling `page.examples.uhura` and `*.examples.uhura` files register 61 page,
  23 component, and 7 surface examples: 91 Editor previews in total.
- `evidence/scenarios.uhura` owns the shared reachable machine scenarios,
  checkpoints, and pins referenced explicitly by those colocated example
  registrations.
- `host.toml` deploys one application-session `Instagram` instance, selects
  the generated `crate::Application`, binds browser/provider adapters, and
  loads the app-local Spock adapter.
- `uhura.toml` declares the strict 0.4 package, explicit machine/helper
  modules, `web-app@1` configuration, shared evidence, assets, and icon
  resources.
- `providers/spock.ts` translates the admitted Uhura authority and mutation
  port contracts to the example's Spock GraphQL, RPC, and storage endpoints.

The framework generates `APPLICATION_ROUTES: Routes<Location>` and the public
machine-bound `Application` presentation as ordinary checked source. Neither
is an authored file or Editor subject. `Application` chooses among the nine
pages only from committed `view.location`; browser history remains an
explicitly bound Router port rather than a second navigation state.

The 18 authored UI subjects preserve the Editor catalogue while proving real
reuse: pages call pure components with exact props and map every emitted event
to a checked machine input; `CommentsSheet` calls `CommentRow`. Calls add no
wrapper element, runtime component instance, state, lifecycle, or scheduler.
Direct component and surface previews supply constant typed props and render
that same checked component graph without forging wrapper machines or live
dispatch bindings.

There is no parallel flat UI module, compatibility frontend, TOML port
catalogue, scripted runtime, fixture snapshot, or app-local contract lockfile.
The deterministic data needed by evidence is ordinary typed Uhura data;
`fixtures/assets/` remains the local media plane.

At Play admission the host publishes the adapter identity plus exact contract
and contract-instance hashes for all three ports. Uhura supplies `router`
because that port is explicitly assigned to its built-in `web.history`
adapter. The configured provider supplies only the `authority` and `mutations`
ports assigned to `app.provider` through
`createUhuraAdapters(config, host)`. The disjoint sets must cover the
deployment exactly. Adapter deliveries return through a deferred queue, so
foreign code cannot synchronously re-enter an Uhura reaction.

## Editor workflow provenance

Derived evidence is connected to its direct parent in a dedicated rail above
each row. Edge labels summarize the directly authored transition/delivery
steps, and selection highlights immediate parent/child relationships. These
are checked evidence edges, not a second runtime state graph. Mounted surfaces
come directly from canonical `uhura-view/1` nodes marked `surface: true`.
Their opaque node keys provide identity; authored accessible labels provide
display names. A derived projection marks surfaces as introduced or retained
relative to its direct evidence parent, and the Inspector preserves nested
render-tree containment instead of inventing a separate opener graph.

Use the parent README for full-stack commands. Uhura-only checks can target
this directory directly; no running authority is required to validate its
machine, generated application, authored pages/components, or evidence.
