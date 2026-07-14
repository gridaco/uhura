# Uhura Projects, Examples, and Providers

## Project layout

```text
uhura.toml
uhura.lock
app/**/page.uhura
components/*.uhura
surfaces/*.uhura
ports/*.port.toml
fixtures/standard.toml
fixtures/scripts/*.toml
fixtures/assets/manifest.toml
providers/
catalog/base.toml
styles/theme.css
```

`uhura.toml` declares the app entry route, catalog, named ports, fixtures, assets, play profile, and optional live provider module/configuration. `uhura.lock` pins canonical catalog and port hashes. Contract drift is a link error; delete and regenerate the lock only as an intentional re-pin after reviewing the contract change.

## Port contracts

Ports are the typed boundary to authoritative providers. Keep UI source provider-neutral.

```toml
[port]
name = "feed"
version = "0.1.0"

[types.user-ref]
kind = "record"

[types.user-ref.fields]
id = "id"
username = "text"

[types.media]
kind = "union"

[types.media.variants.image]
src = "asset"
alt = "text"

[projections.viewer]
type = "user-ref"
boot = true

[projections.post-by-id]
type = "post-summary"
key = "id"

[refusals.not-authorized]

[commands.like-post]
payload = { post = "id" }
refusals = ["not-authorized"]
```

Current type grammar includes `bool`, `int`, `text`, `option<T>`, `list<T>`, and declared types of kind `record`, `union`, `enum`, `id`, `opaque`, or `asset`.

Use opaque values for provider-owned cursors that Uhura may echo but not inspect. Use asset values for provider-resolved media references. Command success payloads are empty in the current spike; authority settlement travels as projection updates, not a second payload carrier.

Projections are absent until delivered. A `boot = true` projection arrives before `Init`; other projections must be handled through availability. Keyed projections use a typed key.

## Examples and preview provenance

Place examples beside their definition:

```text
page.uhura
page.examples.uhura
```

Pinned example:

```uhura
use fixture standard

example first-page default {
  projection feed.viewer = fixture.users.mira
  projection feed.feed-page = fixture.feed.page-1
}
```

Derived example:

```uhura
example like-pending {
  from first-page
  events [ like-toggled(post: "post-1", now-liked: true) ]
  note "optimistic state while the command is pending"
}
```

Use examples to expose meaningful loading, ready, empty, failure, pending, refusal, surface, and navigation states. A derived example must remain reachable through checked events from its source example. Preserve `from`, projections, events, and notes so Canvas can show honest provenance.

Examples do not enter runtime IR. Static projection must not execute I/O or emit commands; replay-derived previews are checked build artifacts over deterministic core steps.

## Fixtures and scripts

Fixture data in `fixtures/*.toml` supplies named, typed projection slices. Keep slices aligned with port contracts and use meaningful product data rather than lorem ipsum or decorative counters.

Scripts in `fixtures/scripts/*.toml` provide deterministic provider delivery and UI events. A reply matches one pending command by command name and optional payload predicate. Use `after-ticks` to make pending states observable. Keep `on-unscripted = "error"` for strict scenarios unless the design explicitly requires another policy.

Use focused scripts for one behavior and a canonical demo script for the full walkthrough. Trace both success and failure/refusal paths.

## Provider seam

Core exchanges versioned JSON envelopes with a provider:

```text
command
projection
projection-failed
outcome with optional atomic projection updates
```

Rules:

- Echo core-minted correlation ids unchanged.
- Produce exactly one eventual outcome per command.
- Deliver one ordered, non-reentrant stream.
- Increase projection revisions strictly per `(projection, key)`.
- Apply an accepted command's projection consequence before or atomically with its outcome.
- Convert transport failure to `unavailable`; do not throw ambient exceptions into Core.
- Keep files and browser-native values outside the serializable Core envelope.

The fixture driver is the deterministic CI provider. A live provider may use Spock, but no Spock object or source syntax belongs in Uhura Core or `.uhura` source.

## Spock boundary

Spock owns users, posts, relationships, permissions, transactions, files, accepted commands, and durable timestamps. Uhura owns local drafts, optimistic overlays, pending markers, notices, selected UI sections, surfaces, and logical navigation.

Derive displayed counts from Spock source rows in the provider. Do not store decorative counts in both systems. Return authority echoes and projection updates after mutations. Treat the current `X-Spock-Actor` integration as prototype impersonation, not production authentication.
