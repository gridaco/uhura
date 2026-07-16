# Uhura

Uhura is a declarative UI language and deterministic experience runtime.

An Uhura program defines what an interface presents, the local UI state that
drives it, and how semantic events advance that state. The checker validates
elements, handlers, commands, and outcomes before the program runs. The runtime
then evaluates the program into a renderer-neutral semantic view.

Uhura owns experience behavior, not pixels or product truth. Renderers own
layout and presentation; providers own authoritative data and operations.

## What it provides

- A closed, checkable language for pages, components, surfaces, and events.
- Deterministic state transitions and replayable interaction traces.
- Typed ports for fixture-backed tests and live providers.
- A read-only Editor for browsing checked previews.
- A Play mode for running the experience against a provider.
- Native and Wasm runtimes with conformance tests.

The full-stack Instagram project in
[`examples/instagram/`](examples/instagram/) exercises the complete workflow;
its Uhura source remains an independently checkable project under `client/`.

## Uhura and Spock

Uhura is a subsystem of [Spock](https://github.com/gridaco/spock), while the two
languages keep separate responsibilities:

- **Uhura** specifies non-authoritative interface state and experience behavior.
- **Spock** specifies authoritative backend state and guarded product behavior.

Integration crosses versioned port and provider contracts. A fact should never
be authoritative in both systems.

## Run the example

From the repository root, install and build the browser application once:

```sh
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm build
cd ..
scripts/build-wasm.sh
```

Start the complete framework example with the npm-distributed Spock CLI:

```sh
npx --yes spock@0.5.0 start examples/instagram
```

The Editor opens at <http://127.0.0.1:4000/>. Use its Play action or open
<http://127.0.0.1:4000/play> to run the experience against the seeded Spock
authority on the same origin.

Useful commands:

```sh
# Check a project
cargo run --locked -p uhura-cli -- check examples/instagram/client

# Run Uhura Editor without the Spock authority
cargo run --locked -p uhura-cli -- editor examples/instagram/client

# Run a deterministic interaction trace
cargo run --locked -p uhura-cli -- trace examples/instagram/client \
  --script=like-refused --expanded

# Test the Rust workspace
cargo test --workspace

# Check the browser application
(cd web && corepack pnpm check)
```

## Repository layout

- [`crates/`](crates/) — checker, runtime, Wasm bindings, and CLI.
- [`web/`](web/) — Editor and Play browser application.
- [`examples/instagram/`](examples/instagram/) — full-stack Spock framework example.
- [`docs/spec/`](docs/spec/) — living language specification.
- [`docs/widgets/`](docs/widgets/) — widget catalogue and capability taxonomy.
- [`docs/rfcs/`](docs/rfcs/) — accepted design decisions.
- [`docs/working-group/`](docs/working-group/) — active design notes.

Authored source is canonical. Generated browser, provider, and Wasm artifacts
are build outputs and are not committed.

## Status

Uhura is incubating. Its grammar, ABI, package structure, and compatibility
policy may change while the language and toolchain are being established.

## Documentation

- [Documentation index and authority](docs/README.md)
- [Living specification](docs/spec/README.md)
- [Widget catalogue and taxonomy](docs/widgets/README.md)
- [RFC index](docs/rfcs/README.md)
- [Working group](docs/working-group/README.md)
