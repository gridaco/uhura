# Instagram framework example

This is the canonical full-stack Spock framework example. One `spock` command
discovers [`spock.toml`](spock.toml), materializes and seeds the Spock authority,
loads the Uhura client, and serves Editor, Play, GraphQL, RPC, and storage from
one process and one port.

From an Uhura checkout, build the app-owned Play provider once, then start the
project with the published npm CLI:

```sh
corepack pnpm@10.11.0 -C web install --frozen-lockfile
corepack pnpm@10.11.0 -C web build:provider
npx --yes spock@0.5.0 start examples/instagram
```

When Uhura is checked out as the Spock submodule, the equivalent parent-repo
commands are:

```sh
corepack pnpm@10.11.0 -C uhura/web install --frozen-lockfile
corepack pnpm@10.11.0 -C uhura/web build:provider
npx --yes spock@0.5.0 start uhura/examples/instagram

# Or exercise the current Spock workspace binary.
cargo run --locked -p spock-cli -- start uhura/examples/instagram
```

Open <http://127.0.0.1:4000/> for Uhura Editor and use **Play** to enter the
live Instagram app at `/play`. Spock Studio is available at `/~studio`.
`spock dev` uses the same topology and keeps the backend generation pinned
while publishing valid client saves live. Invalid client saves retain the last
good Play generation. Backend, seed, and backend-affecting `spock.toml` changes
are observed as `restart_required`; they are not migrated or swapped live.

## Layout

- `spock.toml` composes the two roots and owns framework lifecycle only.
- `backend/app.spock` declares the authority schema, seed, GraphQL projection,
  RPC commands, and storage model.
- `backend/seed/` contains the media imported into Spock storage when the
  disposable authority database starts.
- `client/` is a complete Uhura project. Its `uhura.toml` remains the owner of
  routes, catalog, ports, fixtures, and Play provider configuration.

The example deliberately has two asset planes. `backend/seed/` is authority
input and is captured relative to `app.spock`; it cannot escape the backend
root. `client/fixtures/assets/` is deterministic Uhura fixture data used by
Editor, trace, and CI. Similar files in those directories serve different
lifecycles and must not be deduplicated.

The checked-in provider source is `client/providers/spock.ts`. Its generated
`client/providers/dist/spock.js` output is ignored, so a source checkout must
run `pnpm -C uhura/web build:provider` after cloning or changing the provider.
The published Spock CLI hosts that artifact but does not currently compile
app-specific TypeScript providers.

## What the example proves

The client has a paged feed filtered by the active actor's follow graph, real
aggregate counts, profiles, post/Reels/Saved/tagged grids, multi-frame stories,
people search, comments, optimistic likes and private saves, and single-image
post creation. The provider reads authority truth through same-origin Spock
GraphQL, signs media downloads and uploads through Spock storage, and maps port
commands to Spock RPC without putting Spock knowledge in Uhura Core.

When served by the framework host, the provider discovers the versioned
`spock-host-environment/1` document at `/~project/environment`. Those
same-origin capabilities take precedence over the absolute development URLs in
`client/uhura.toml`, which remain a fallback for standalone Uhura development.

The demo identity seam uses `X-Spock-Actor` and is intentionally forgeable. It
is a local integration proof, not a production authentication boundary.

## Uhura-only checks

The client remains independently checkable and traceable:

```sh
cargo run --locked --manifest-path uhura/Cargo.toml -p uhura-cli -- \
  check uhura/examples/instagram/client --deny-warnings
cargo run --locked --manifest-path uhura/Cargo.toml -p uhura-cli -- \
  trace uhura/examples/instagram/client --script=demo
```

Those commands use the deterministic fixture plane and do not require a
running Spock backend.
