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

The Instagram project in [`examples/instagram-uhura/`](examples/instagram-uhura/)
exercises the complete workflow.

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

Start the Instagram example:

```sh
cargo run --locked -p uhura-cli -- examples/instagram-uhura
```

The Editor opens at <http://127.0.0.1:8787/>. Use its Play action or open
<http://127.0.0.1:8787/play> to run the experience.

Useful commands:

```sh
# Check a project
cargo run --locked -p uhura-cli -- check examples/instagram-uhura

# Run a deterministic interaction trace
cargo run --locked -p uhura-cli -- trace examples/instagram-uhura \
  --script=like-refused --expanded

# Test the Rust workspace
cargo test --workspace

# Check the browser application
(cd web && corepack pnpm check)
```

## Repository layout

- [`crates/`](crates/) — checker, runtime, Wasm bindings, and CLI.
- [`web/`](web/) — Editor and Play browser application.
- [`examples/instagram-uhura/`](examples/instagram-uhura/) — end-to-end example.
- [`docs/spec/`](docs/spec/) — living language specification.
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
- [RFC index](docs/rfcs/README.md)
- [Working group](docs/working-group/README.md)
