# Instagram framework example

This is the canonical full-stack Spock framework example. One `spock` command
discovers [`spock.toml`](spock.toml), materializes and seeds the Spock authority,
loads the Uhura client, and serves Editor, Play, GraphQL, RPC, and storage from
one process and one port.

> **RFD 0024 implementation preview — experimental, unstable, and
> non-normative.** The backend uses proposed Spock `error` declarations that
> the `0.5.2` and `0.5.3` backend toolchains accept as implementation evidence.
> RFD 0024 remains draft; this inclusion is not language acceptance or a
> compatibility promise.

The client is strict Uhura 0.4. Published `spock@0.5.3` and earlier embed the
retired client frontend and cannot run this checkout. Until a compatible npm
release exists, initialize this repository as the `uhura/` submodule of the
companion Spock source checkout, then run from the Spock repository root:

```sh
git submodule update --init --recursive
corepack pnpm@10.11.0 -C crates/spock-runtime/studio install --frozen-lockfile
corepack pnpm@10.11.0 -C crates/spock-runtime/studio build
corepack pnpm@10.11.0 -C uhura/web install --frozen-lockfile
corepack pnpm@10.11.0 -C uhura/web build
bash uhura/scripts/build-wasm.sh

SPOCK_UHURA_WEB_DIST="$PWD/uhura/web/dist" \
SPOCK_UHURA_WASM_DIST="$PWD/uhura/crates/uhura-wasm/pkg/web" \
cargo run --locked -p spock-cli -- start uhura/examples/instagram
```

Open <http://127.0.0.1:4000/> for Uhura Editor and use **Play** to enter the
live Instagram app at `/play`. Spock Studio is available at `/~studio`.
`spock dev` uses the same topology and keeps the backend generation pinned
while publishing valid client saves live. Invalid client saves retain the last
good Play generation. Backend, seed, and backend-affecting `spock.toml` changes
are observed as `restart_required`; they are not migrated or swapped live.
If `build-wasm.sh` reports a missing or mismatched `wasm-bindgen-cli`, run the
lockfile-exact project-local install command it prints and repeat the build.

## Layout

- `spock.toml` composes the two roots and owns framework lifecycle only.
- `backend/app.spock` declares the authority schema, seed, GraphQL projection,
  RPC commands, and storage model.
- `backend/seed/` contains the media imported into Spock storage when the
  disposable authority database starts.
- `client/machine.uhura` is the 0.4 headless application machine and its
  deterministic demo data.
- `client/parts.uhura` proves checked source composition, ownership, and
  cross-part `Reads`/`Updates` over that same machine transaction.
- `client/ui.uhura` is the explicit 0.4 Web UI projection.
- `client/evidence.uhura` is the native 0.4 checked Editor/preview corpus.
- `client/host.toml` deploys the machine and binds browser/provider adapters.
- `client/uhura.toml` declares the 0.4 package, modules, evidence, assets, and
  icon resources.

The example deliberately has two asset planes. `backend/seed/` is authority
input and is captured relative to `app.spock`; it cannot escape the backend
root. `client/fixtures/assets/` is deterministic local media used by checked
evidence, Editor, Play, and CI. Similar files in those directories serve
different lifecycles and must not be deduplicated.

The checked-in provider source is `client/providers/spock.ts`. Its generated
`client/providers/dist/spock.js` output is ignored, so a source checkout must
run `corepack pnpm@10.11.0 -C uhura/web build:provider` from the companion
Spock repository root after cloning or changing the provider.
The module exports typed adapters for the exact admitted `authority` and
`mutations` port instances; browser history is supplied separately by Uhura's
built-in `web.history` adapter. The Spock framework host serves the generated
provider artifact but does not currently compile app-specific TypeScript. A
future compatible npm distribution will carry that same boundary.

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
`client/host.toml`, which remain a fallback for standalone Uhura development.

The demo identity seam uses `X-Spock-Actor` and is intentionally forgeable. It
is a local integration proof, not a production authentication boundary.

## Uhura-only checks

The client remains independently checkable:

```sh
cargo run --locked --manifest-path uhura/Cargo.toml -p uhura-cli -- \
  check uhura/examples/instagram/client --deny-warnings
```

That command checks the machine, Web presentation, and evidence corpus and
does not require a running Spock backend.

To inspect one authored scenario through the same evidence runner:

```sh
cargo run --locked --manifest-path uhura/Cargo.toml -p uhura-cli -- \
  trace uhura/examples/instagram/client \
  --script=feed_like_refused_scenario --expanded
```
