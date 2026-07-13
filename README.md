# Uhura

**Uhura is an incubating declarative UI language and deterministic headless
experience runtime.**

Uhura defines what an interface presents, the non-authoritative UI-session
state that drives it, and how semantic events advance that state. Its core
runtime evaluates a checked program into a renderer-neutral semantic view and
emits typed commands or platform intents. It does not lay out or paint pixels,
perform I/O, or own authoritative product truth.

The project is an incubating spike: a Rust workspace under `crates/`
implements the checker, core machine, fixture driver, static canvas, wasm
session, play shell (`shell/`), and `uhura` CLI, exercised end to end by the
Instagram slice at `examples/instagram-uhura/`. The design doc
(`docs/working-group/instagram-spike-design.md`) is authoritative; there is no
accepted grammar freeze, package, or compatibility promise yet.

Quick tour (run from the repo root): `cargo run -p uhura-cli --
check examples/instagram-uhura`, `… project examples/instagram-uhura`
(static canvas), `… trace examples/instagram-uhura --script=like-refused
--expanded` (headless machine), and `scripts/build-wasm.sh && cargo run -p
uhura-cli -- dev examples/instagram-uhura` (live play shell at
http://127.0.0.1:8787/). `cargo test --workspace` runs the
golden suites plus the design's §13 acceptance battery
(`crates/uhura-tests/tests/acceptance_feed.rs`); the battery's native↔wasm
parity criterion runs when `node` and the wasm package are present and is
reported as skipped otherwise (`UHURA_REQUIRE_PARITY=1` makes that a
failure).

## Project position

Uhura is a greenfield design: no earlier format constrains its grammar or
ABI. It stands on well-known prior art — Svelte, XAML, QML, Elm (see
[References and prior work](#references-and-prior-work)) — without being a
rename, source-compatible version, or extension of any of it.

Uhura is a subsystem of the [Spock](https://github.com/gridaco/spock)
project: Spock is the ecosystem and toolchain root, and this repository is
included there as a git submodule. The two remain distinct languages with a
hard semantic boundary:

- **Spock** specifies and executes authoritative backend state and guarded
  product behavior.
- **Uhura** specifies and executes non-authoritative interface-session state
  and experience behavior.

## Founding boundary

> Uhura owns UI semantics and the headless UI-state mechanic. Spock owns
> authoritative product semantics. A concrete renderer owns platform rendering.

No fact may be authoritative in both Uhura and Spock.

The practical ownership test is:

> If discarding the UI session could change product truth, authorization, a
> transaction, or another client's reality, the state belongs to Spock. If it
> coordinates one experience and can be reconstructed without corrupting that
> truth, it belongs to Uhura.

## Repository status

This repository is the canonical source of the Uhura language — its grammar,
checker, runtime, and conformance suite. It is consumed by the
[Spock](https://github.com/gridaco/spock) repository as a git submodule
(`uhura/` at Spock's root): Spock is the ecosystem and toolchain root, and
once Uhura is minimally stable its tooling will ship through the unified
`spock` toolchain rather than as a standalone distribution. Spock is also
Uhura's canonical provider; the port seam stays provider-neutral.

Until an implementation RFC says otherwise:

- it has no dependency on Spock internals;
- its example corpus lives at `examples/instagram-uhura/`;
- it publishes no package or executable;
- Rust is the preferred initial core implementation direction, not yet a
  normative language requirement; and
- it is licensed under the repository's MIT license; package names,
  compatibility, and release policy remain open.

Cross-project integration must go through versioned contracts and conformance
fixtures.

## References and prior work

Uhura draws on well-known prior art in declarative UI and deterministic state
management:

- Svelte, for markup-first components over a closed template language —
  `{#if}`/`{#each}`-style control blocks compiled ahead of time rather than
  interpreted framework calls.
- XAML, for declarative UI held apart from imperative code, with a closed,
  checkable element vocabulary.
- QML, for a typed declarative UI language backed by its own runtime rather
  than a general-purpose scripting host.
- Elm, for the pure state machine: typed messages advance the model, the view
  is a pure function of state, and effects leave the language as data.
- Redux, for replayable state transitions and the action log as a debugging
  surface — time travel as a consequence of purity.
- Storybook, for component examples as first-class, checkable artifacts
  (`.examples.uhura`).

## Documentation

- [Documentation index and authority](docs/README.md)
- [Living master specification](docs/spec/README.md)
- [RFC 0001: project foundation](docs/rfcs/0001-project-foundation.md)
- [Working group](docs/working-group/README.md)
