# Uhura

Uhura is a frontend builder system whose core is a deterministic state-machine
language with an explicit, opt-in Web UI profile.

An Uhura machine defines configuration, owned state, typed inputs, atomic
reactions, ordered commands, and a pure public observation. The checker
validates that complete program before it runs. The active 0.4 frontend
evaluates Rust-shaped core source and explicit `use uhura::ui;` Web
presentation without changing that semantic boundary. Evidence uses the same
frontend and adds source-authored scenarios, checkpoints, pins, and static
examples without creating a second execution model.

Uhura owns experience behavior, not pixels or product truth. The browser owns
layout and presentation; admitted adapters own browser capabilities and access
to authoritative data and operations.

## What it provides

- A closed, checkable language for deterministic state machines.
- Optional checked Web presentations, semantic events, and surfaces.
- An opt-in Web application topology with checked file routes and pure typed
  UI components.
- Deterministic reactions, checkpoints, evidence, and interaction traces.
- Typed ports admitted against exact adapter ownership and contract identities.
- A read-only Editor for browsing checked previews.
- A Play mode for running the experience against a provider.
- An immutable Web export of one checked Editor/Play generation.
- One canonical engine used natively and through Wasm, with cross-boundary
  conformance tests.

The full-stack Instagram project in
[`examples/instagram/`](examples/instagram/) exercises the complete workflow;
its Uhura source remains an independently checkable project under `client/`.
Language-neutral [program harnesses](examples/programs/) separately pressure
the experience-machine model without depending on widgets or rendering. The
parallel [application harnesses](examples/applications/) test whether candidate
models remain practical when `ui`, locations, lifecycle, and external
settlement are composed explicitly.

## Uhura and Spock

Uhura is a subsystem of [Spock](https://github.com/gridaco/spock), while the two
languages keep separate responsibilities:

- **Uhura** specifies non-authoritative interface state and experience behavior.
- **Spock** specifies authoritative backend state and guarded product behavior.

Integration crosses versioned port and provider contracts. A fact should never
be authoritative in both systems.

## Run the example

From the Uhura repository root, build the Wasm engine and browser application
once:

```sh
corepack pnpm@10.11.0 -C web install --frozen-lockfile
scripts/build-wasm.sh
corepack pnpm@10.11.0 -C web build
```

Open the independently checkable Instagram client in the Uhura Editor:

```sh
cargo run --locked -p uhura-cli -- editor examples/instagram/client
```

The Editor opens at <http://127.0.0.1:8787/>. To run Play against the complete
seeded Spock authority on one origin, use the framework command documented by
the [Instagram example](examples/instagram/).

Useful commands:

```sh
# Check source, lower one machine program, and execute all authored evidence
cargo run --locked -p uhura-cli -- check \
  examples/instagram/client --deny-warnings

# Start Play as the primary route; Editor remains available at /
cargo run --locked -p uhura-cli -- play examples/instagram/client

# Export ordinary files for any static host; no Uhura process runs at serving time
cargo run --locked -p uhura-cli -- export examples/instagram/client \
  --out dist/instagram-web --mount /instagram/

# Serialize one source-authored evidence scenario as canonical JSONL
cargo run --locked -p uhura-cli -- trace examples/instagram/client \
  --script=feed_like_refused_scenario --expanded

# Test the Rust workspace
cargo test --locked --workspace

# Check the browser application
corepack pnpm@10.11.0 -C web check
```

`check` and `trace` use the same checked program and evidence runner that feed
Editor previews. `--script` selects an authored `scenario`; it is not a
fixture-script language or an alternate runtime.

`export` uses the packaged mount-neutral Web template and browser Wasm runtime.
The resulting directory is host-vendor agnostic but mount-specific. Its
`uhura-static-bundle.json` records the entry document and required
mount-scoped history fallback.

## Repository layout

- [`crates/`](crates/) — checker, runtime, host, Wasm bindings, CLI, and the
  [single-engine acceptance crate](crates/uhura-tests/).
- [`web/`](web/) — Editor and Play browser application.
- [`examples/`](examples/) — language-design program and application harnesses, plus the full-stack Instagram example.
- [`docs/doctrine/`](docs/doctrine/) — durable language doctrine and review principles.
- [`docs/spec/`](docs/spec/) — stable router for disposable drafts and future version specifications.
- [`docs/widgets/`](docs/widgets/) — stable capability taxonomy and version-scoped catalogues.
- [`docs/implementation/`](docs/implementation/) — current non-normative code ownership and contributor change routes.
- [`docs/rfcs/`](docs/rfcs/) — historical proposals and supersedable decisions.
- [`docs/studies/`](docs/studies/) — stable research router with disposable study leaves.

Authored source is canonical. Generated browser, provider, and Wasm artifacts
are build outputs and are not committed.

## Design research

Uhura's behavioral language is being reviewed from first principles. The
[design principles](docs/doctrine/principles.md) define the questions, while
these references provide the current evidence:

- [Uhura 0.4 incubation candidate](docs/spec/drafts/0.4/) consolidates the
  active design: a source-neutral transaction kernel, Rust-shaped
  machine source, Svelte-shaped `ui`, pure presentation composition, and
  modular source that lowers to one global machine IR.
- [Language necessity and surface reuse](docs/studies/language-necessity-and-surface-reuse.md)
  asks whether Uhura needs an independently owned language at all.
- [Program harnesses](examples/programs/README.md) provide language-neutral
  L0–L2 problems for comparing candidate semantics.
- [A0 Return Desk](examples/applications/a0-return-desk/README.md) provides the
  parallel practical application-transfer problem.
- [Uhura 0.4](examples/programs/answers/uhura-0.4/) exercises the current
  candidate against L0–L2; the application harness carries the corresponding
  A0 answer. [Relay B3](docs/spec/drafts/relay-b3/) is the short historical
  pointer for the experiment that preceded this shape. It is not a runtime,
  module, authored language, or product boundary. Retired source remains
  recoverable from Git history rather than executable in the current tree.
- [Transactional state-machine language prior art](docs/studies/transactional-state-machine-language-prior-art.md)
  compares Scilla, FSM-Hume, Lustre/SCADE, Kôika/Bluespec, Elm, and adjacent
  models.
- [Visual state-machine authoring and deterministic simulation prior art](docs/studies/visual-state-machine-authoring-prior-art.md)
  compares Stateflow, IEC controller languages, game-engine visual scripting,
  Photon Quantum, and relevant negative evidence.

These studies are non-authoritative and disposable. They inform later
decisions; they do not define current syntax or runtime behavior.

## Status

Uhura is incubating. Its grammar, ABI, package structure, and compatibility
policy may change while the language and toolchain are being established.

## Documentation

- [Documentation index and authority](docs/README.md)
- [Language doctrine](docs/doctrine/README.md)
- [Specification router and historical design drafts](docs/spec/README.md)
- [Widget taxonomy and version-scoped catalogues](docs/widgets/README.md)
- [Current implementation map](docs/implementation/README.md)
- [RFC index](docs/rfcs/README.md)
- [Studies](docs/studies/README.md)
