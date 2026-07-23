# Uhura web application

The read-only Editor and interactive Play shell are routes of one strict,
framework-free TypeScript application. Both render Uhura semantic nodes through
the shared renderer in `src/renderer/`; policy keeps Editor previews inert and
enables runtime delivery and browser effects only in Play.

The native host remains authoritative for coherent source observation,
checking, evidence execution, Editor-state publication, deployment admission,
and Play artifacts. In Play, the Wasm `Session` executes the same canonical
Uhura engine used natively. Browser code owns routing mechanics, presentation,
and the admitted adapter bridge; it never parses Uhura source or reconstructs
language semantics.

## Install and check

Use the Node and pnpm versions pinned by `.nvmrc` and `package.json`:

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm check
```

Run these commands from `web/`. `check` performs the TypeScript build, lint,
production builds, and browser-unit tests.

## Development loop

Build Wasm and the browser application, then start the native host from the
Uhura repository root:

```sh
./scripts/build-wasm.sh
corepack pnpm@10.11.0 -C web build
cargo run --locked -p uhura-cli -- editor examples/instagram/client --port 8787
```

The source checkout needs a built web application before a browser command can
serve itself. `corepack pnpm build` creates it and also builds the Instagram
provider. For frontend iteration, leave the native host running and start Vite
from `web/`:

```sh
corepack pnpm dev
# http://127.0.0.1:5173/
```

Vite serves the same application entry for Editor `/` and Play `/play`, and
proxies `/api` to `http://127.0.0.1:8787`. Set `UHURA_NATIVE_ORIGIN` before
starting Vite to use another native origin. The native process—not Vite—watches
saved Uhura project files and publishes complete replacement Editor states.

Use `corepack pnpm dev:provider` only when iterating on the example's configured
Spock provider. It is independent of the application dev server.

## Build and runtime contract

`corepack pnpm build` creates three generated products:

- `dist/`: the origin-root live build used by native Editor and Play;
- `dist-export/`: the relative-chunk template materialized by `uhura export`;
- `../examples/instagram/client/providers/dist/spock.js`: the configured
  Instagram Play provider.

Both outputs are ignored by Git. `web/src/` and the provider TypeScript are the
authoritative source; CI rebuilds them instead of comparing checked-in bundles.
The Wasm package remains external and is served below `/api/play/wasm/` rather
than bundled by Vite.

The native host serves the compiled application unchanged and provides SPA
fallback for `/` and `/play`. Play admits the complete browser-owned and
application-owned adapter set against the exact port contract and instance
hashes before any command leaves the machine. `web.history` is a built-in
browser adapter; a configured provider module supplies typed application
adapters through `createUhuraAdapters(config, host)`. Deliveries return through
a deferred FIFO bridge and cannot synchronously re-enter a reaction. Node and
Vite are build-time dependencies only.
`../scripts/package.sh` builds both application profiles, the provider, Wasm,
and the release binary. It packages live and export Web distributions
separately beside the executable. `uhura export` configures the export template
for a canonical mount without invoking Node or Vite at command runtime.
