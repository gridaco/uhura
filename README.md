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
session, TypeScript Play host (`web/src/play/`), and `uhura` CLI, exercised end to end by the
Instagram slice at `examples/instagram-uhura/`. The design doc
(`docs/working-group/instagram-spike-design.md`) is authoritative; there is no
accepted grammar freeze, package, or compatibility promise yet.

Quick tour (run from the repo root): `cargo run -p uhura-cli --
examples/instagram-uhura` opens the default read-only Editor at
http://127.0.0.1:8787/; its Play button enters the live shell on the same
server. `… check examples/instagram-uhura` checks the project, `… trace
examples/instagram-uhura --script=like-refused --expanded` (headless
machine), and `scripts/build-wasm.sh && cargo run -p uhura-cli -- play
examples/instagram-uhura` opens the interactive Play shell directly.
`editor` remains an explicit spelling of the default, `project` remains the
build-only Canvas command, and `dev` remains an alias for `play`.
`cargo test --workspace` runs the
golden suites plus the design's §13 acceptance battery
(`crates/uhura-tests/tests/acceptance_feed.rs`); the battery's native↔wasm
parity criterion runs when `node` and the wasm package are present and is
reported as skipped otherwise (`UHURA_REQUIRE_PARITY=1` makes that a
failure).

The browser host and read-only Editor controller are authored as strict,
framework-free TypeScript under `web/`. Their small compiled assets are
checked in so Cargo, `uhura editor`, and `uhura play` remain usable without a
Node runtime. Contributors changing web source run
`(cd web && corepack pnpm install --frozen-lockfile)` once and then
`(cd web && corepack pnpm check)` under the versions pinned by `.nvmrc` and the
package manager declaration. The umbrella Spock checkout and this repository
share the same Node 24 LTS patch and pnpm 10.11.0; React remains deliberately
deferred.

## Why Uhura exists

Every product starts from the user, and the only honest, complete definition
of "the user's product" is its experience — the pages a person actually sees
and the things they can actually do. Most tools treat that definition as a
picture: click-through screens that cannot be checked, backed by nothing
real. A button that navigates nowhere, a logged-out page rendering a
logged-in list, a tab with no state behind it, an action with no owner —
these are not aesthetic problems. They are **broken contracts**, and a
picture cannot catch them.

Uhura exists to make the experience half of a product a real, checkable,
executable artifact. The design is not documentation of the product; the
design **is** the spec — and a spec deserves a compiler.

## Doctrine

- **Correctness over looks.** Invalid experience should fail to express, the
  way a type system makes whole classes of bugs unrepresentable: every
  element comes from a declared catalog, every event names a declared
  handler, and a command's outcome — ok, typed failure, refusal,
  unavailability — is a typed union the program branches on, not an
  exception. Aesthetics — spacing, motion, theme — are a layer built on top
  of a correct contract, owned by the renderer. Get the contract right
  first.

- **A state machine you can walk.** Because every transition is declared in
  a closed language, a checked program is more than a set of screens — it is
  a graph of journeys. Pick a path and follow it from start to finish.

- **A prototype you can play, honestly.** The machine is deterministic and
  headless: no clocks, no randomness, no floats, no I/O. Identical inputs
  produce byte-identical traces, native or wasm. "Watch the flow run" is
  only evidence when two runs cannot disagree — and a journey that cannot
  complete is a broken contract, not a failed test.

- **No imaginary halves.** Click-through prototypes fake the product's other
  half. Uhura's ports are hash-pinned typed contracts with real outcomes,
  exercised against a scripted fixture in CI and a real provider
  ([Spock](https://github.com/gridaco/spock)) in play.

- **Undefined behavior is owed, not hidden.** A gap in the experience — an
  unhandled outcome, an unreachable state, an event with no effect — is not
  a bug in what's written; it is a decision that's still owed. The checker's
  job is to make every gap a visible, owned diagnostic, never a silent
  guess.

- **Semantic view, not pixels.** The runtime evaluates to a renderer-neutral
  semantic view and emits typed commands and platform intents. What a thing
  *is* — a list, a button, a sheet — is language; where its pixels land is
  not.

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
