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

Uhura is a greenfield proposed successor to the Frame workstream. It is not a
rename, source-compatible version, or incremental extension of Frame XML.
Existing Frame and Wire v4 documents remain historical and migration inputs;
they do not constrain Uhura's eventual grammar or ABI.

Uhura is a sibling of [Spock](https://github.com/gridaco/spock), not a child of
it:

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

This is Uhura's dedicated repository. [Spock](https://github.com/gridaco/spock)
is Uhura's canonical provider, developed in its own repository; the port seam
stays provider-neutral.

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

## Documentation

- [Documentation index and authority](docs/README.md)
- [Living master specification](docs/spec/README.md)
- [RFC 0001: project foundation](docs/rfcs/0001-project-foundation.md)
- [Working group](docs/working-group/README.md)
