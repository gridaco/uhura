# Instagram Uhura client

This directory is the independently checkable Uhura 0.4 application inside
the [Instagram framework example](../README.md):

- `machine.uhura` is the headless deterministic state machine. It owns the
  domain model, routes, transitions, commands, and deterministic demo data.
- `parts.uhura` owns the composed notice state plus its checked dismissal
  consumer. Their `Reads` and `Updates` handles lower into the same aggregate
  machine transaction.
- `ui.uhura` opts into the Web UI capability and projects that machine through
  18 public UI declarations. `FeedPage` is the deployed live presentation.
- `evidence.uhura` is a native 0.4 tooling module attached through
  `[evidence.modules]`. It defines the 91 checked page, component, and surface previews
  consumed by Editor, including 34 replay-derived previews that retain their
  direct provenance.
- `host.toml` deploys one application-session `Instagram` instance, binds its
  `FeedPage` presentation, maps browser history and provider ports, and loads
  the app-local Spock adapter.
- `uhura.toml` declares the strict 0.4 package, its machine/part/UI modules,
  attached evidence, and supplemental asset/icon resources.
- `providers/spock.ts` translates the admitted Uhura authority and mutation
  port contracts to the example's Spock GraphQL, RPC, and storage endpoints.

There is no parallel page/component source tree, TOML port catalog, scripted
runtime, fixture snapshot, or app-local contract lockfile. The deterministic
data needed by evidence is ordinary typed Uhura data in `machine.uhura`;
`fixtures/assets/` remains the local media plane.

The 18 UI declarations preserve the Editor's separately inspectable page,
component, and surface catalogue. Reusable component invocation is not claimed
yet: the 0.4 UI profile deliberately leaves its exact props, event-interface,
and children syntax gated while the machine/composition language is being
proved.

At Play admission the host publishes the adapter identity plus exact contract
and contract-instance hashes for all three ports. Uhura supplies `router`
because that port is explicitly assigned to its built-in `web.history`
adapter. The configured provider supplies only the `authority` and `mutations`
ports assigned to `app.provider` through `createUhuraAdapters(config, host)`.
The disjoint sets must cover the deployment exactly. Adapter deliveries return
through a deferred queue, so foreign code cannot synchronously re-enter an
Uhura reaction.

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
machine, presentation, or evidence.
