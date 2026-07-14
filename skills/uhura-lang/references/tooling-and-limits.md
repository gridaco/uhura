# Uhura Tooling and Current Limits

## CLI from the Uhura repository

```sh
cargo run --locked -p uhura-cli -- check examples/instagram-uhura
cargo run --locked -p uhura-cli -- check examples/instagram-uhura --deny-warnings
cargo run --locked -p uhura-cli -- check examples/instagram-uhura --emit-ir
cargo run --locked -p uhura-cli -- fmt examples/instagram-uhura
cargo run --locked -p uhura-cli -- fmt examples/instagram-uhura --check
cargo run --locked -p uhura-cli -- trace examples/instagram-uhura --script=like-refused
cargo run --locked -p uhura-cli -- trace examples/instagram-uhura --script=like-refused --expanded
cargo run --locked -p uhura-cli -- project examples/instagram-uhura --out=renders
cargo run --locked -p uhura-cli -- editor examples/instagram-uhura --port 8787 --out=renders
cargo run --locked -p uhura-cli -- play examples/instagram-uhura --port 8787
```

No command selects the Editor. `project` is the build-only Canvas command. `dev` is a compatibility alias for `play`.

From the umbrella Spock repository, add `--manifest-path uhura/Cargo.toml` and use `uhura/examples/...` paths.

## Verification order

1. Run `fmt --check`.
2. Run `check --deny-warnings`.
3. Run a focused trace for every changed event path.
4. Build the Canvas with `project` and inspect affected previews.
5. Run Editor for Canvas plus `/play`, or run Play directly.
6. Test the intended provider and actor, not only fixtures.
7. Run focused crate tests, then `cargo test --workspace --locked` for broad changes.

Use `--format=json` when a machine-readable diagnostics envelope is useful. Exit codes are 0 for success, 1 for checked program failure, and 2 for CLI or environment misuse.

## Editor, Canvas, and Play

The Editor serves a static, deterministic, fixture-backed Canvas at `/` and live Play at `/play`. Canvas supports preview search, navigation, selection, pan, zoom, and metadata inspection. It is read-only; restart the command to rebuild after source changes.

`project` writes a self-contained `canvas.html` without starting a server. Canvas must not require a running provider or emit runtime commands while projecting pinned states.

Play runs the real state machine. Restart creates a fresh Uhura session but does not reset provider truth. Mobile/Desktop selection is renderer chrome, not a language viewport primitive.

## Browser and Wasm assets

The checked-in browser assets allow Cargo and the CLI to run without Node. When changing web source:

```sh
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm check
```

Rebuild Wasm after core/ABI changes and once during initial source setup:

```sh
scripts/build-wasm.sh
```

Protocol mismatch errors mean the browser shell and Wasm bundle disagree; rebuild Wasm before changing application source.

## Spock-backed Instagram integration

From the umbrella Spock repository:

```sh
./scripts/spock-uhura.sh \
  examples/instagram-poc/app.spock \
  uhura/examples/instagram-uhura
```

Verify:

```text
http://127.0.0.1:8787/          read-only Uhura Editor
http://127.0.0.1:8787/play      live Spock-backed Play
http://127.0.0.1:4000/~studio   Spock Studio
```

Stop old servers before using default ports. The current runner checks the Spock port early but an existing Uhura server can still create a false readiness signal before bind failure; inspect the final process output.

## Determinism and traces

One external input produces one core step. Trace records expose the input event, handler/guard selection, state writes, structural operations, commands, intents, drops, diagnostics, and canonical state/view hashes.

Use traces to prove behavior such as:

- exactly one command per eligible press;
- optimistic view before outcome;
- rollback after refusal or unavailability;
- atomic provider update before `.ok` handling;
- duplicate pagination suppression;
- surface dismissal and focus restoration;
- push/replace/back navigation semantics;
- native and Wasm parity for fixed inputs.

## Current limits

The language and runtime are incubating and not compatibility-frozen. Current gaps include:

- no source editing in Editor;
- no Canvas workflow connectors or automatic edge routing;
- no first-class parent/child Canvas visualization for surfaces;
- no visibility-aware media lifecycle;
- no multiline controlled text element;
- no per-tab retained navigation stacks;
- no browser URL/deep-link reconciliation;
- no story clock/host observation contract;
- no complete interactive list-item wrapper policy;
- no demand-driven keyed provider projection protocol;
- no command cancellation, timeout, or checkpoint/state-preserving hot reload;
- no slots/children, shared layouts, import aliases, surface results, or general match expressions;
- no production authorization guarantee from the Spock v0 provider.

Do not fake these with CSS, timers inside Core, broad fixture behavior, untyped provider state, or hidden browser callbacks. Name the gap and keep the current contract honest.
