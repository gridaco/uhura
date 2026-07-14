# Uhura web host

The Editor controller and Play host are strict, framework-free TypeScript.
They render and host Uhura's versioned semantic view; they do not own product
truth or reinterpret the Rust/Wasm state machine. React is intentionally not a
dependency of this package.

## Develop

Use the pinned Node and pnpm versions from `.nvmrc` and
`web/package.json`. From the Uhura repository root, enter the web package and
install its package graph once:

```sh
(cd web && corepack pnpm install --frozen-lockfile)
```

Run the complete frontend gate:

```sh
(cd web && corepack pnpm check)
```

For isolated Play-host work, build Wasm once and run the Rust host on port 8787
in one terminal:

```sh
./scripts/build-wasm.sh
cargo run --locked -p uhura-cli -- play examples/instagram-uhura --port 8787
```

Then start Vite from a second terminal:

```sh
(cd web && corepack pnpm dev)
# http://127.0.0.1:5173/shell/
```

Vite proxies the IR, provider, assets, Wasm, and SSE endpoints to the Rust
host. From `web/`, use `corepack pnpm dev:editor` or
`corepack pnpm dev:provider` for generated-asset watch builds; restart
`uhura editor` after an Editor build because its Canvas is intentionally
generated once at process start.

## Build contracts

`corepack pnpm build` (from `web/`) produces three deliberately different
artifacts:

- `dist/play/`: the Play HTML plus hashed ESM and CSS. The Rust host serves
  this tree under `/shell/` while returning its `index.html` at `/` or `/play`.
- `dist/editor/canvas-chrome.js`: exactly one classic IIFE, inlined into the
  standalone Canvas HTML by `uhura-project`.
- `../examples/instagram-uhura/providers/dist/spock.js`: exactly one browser
  ESM module, captured and content-addressed by the Play server.

The generated artifacts are committed. This is intentional: a clean checkout
can build the Cargo workspace and run Editor/Play without installing Node.
CI runs the pinned frontend build and rejects stale generated output. Cargo
must never invoke pnpm from `build.rs`.

The Wasm module stays external at `/wasm/uhura_wasm.js`; Vite must not bundle
it. The authored app stylesheet is injected after the host stylesheet so app
styles retain the established cascade precedence.
