# Core inspection TODO

This tracker owns deferred debugger work that changes runtime semantics or the
native inspection protocol. Browser-only work remains in the
[Play debugger TODO](../../../web/src/play/TODO.md); source serving and Editor
navigation remain in the
[CLI Play-host TODO](../../uhura-cli/src/cmd/TODO.md).

Labels use the same meanings as the Play tracker: priorities are P0/P1/P2,
difficulty is S/M/L/XL, and every item here necessarily involves engine work.

## Runtime identity and trace fidelity

- [ ] **[P1][L][Engine work: Yes] Decide and encode component runtime-instance
      semantics.**
  - **Owner:** `state.rs`, `view.rs`, `inspect.rs`, the Wasm ABI, and protocol
    mirrors.
  - First decide whether authored components are inspectable runtime instances
    or only reusable static topology in the current language semantics.
  - If they are instances, expose stable instance identity, owning scope,
    mounted order, and inspectable state without coupling identity to DOM
    position.
  - If they are not instances, encode that fact explicitly so the browser does
    not present initial/static component values as live state.
  - Cover repeated components, keyed lists, remounts, nested components, and
    source-definition reuse.

- [ ] **[P2][L][Engine work: Yes] Record richer evaluated trace facts only when
      a concrete debugger view needs them.**
  - **Owner:** `trace.rs`, `eval.rs`, `step.rs`, `inspect.rs`, and the Wasm ABI.
  - Candidate facts include evaluated guard inputs/results, expression-level
    reads, and explicit value provenance.
  - Keep tracing observational: enabling inspection must not change evaluation
    order, canonical values, hashes, or emitted commands.
  - Do not add wall-clock timing to core; host-side instrumentation owns
    nondeterministic duration measurements.
  - Version additions and pin native/Wasm byte parity before the browser relies
    on them.

## Runtime control

- [ ] **[P2][XL][Engine work: Yes] Design real pause, step, restore, and replay
      semantics.**
  - **Owner:** `step.rs`, `state.rs`, the driver/provider boundary, Wasm session
    ownership, and the Play pump.
  - Treat this as a runtime project, not a timeline UI feature. Historical
    inspection records are immutable observations and are not valid mutable
    session objects.
  - Specify pending commands, provider outcomes, ticks, correlations, minted
    serials, structural navigation, and external capabilities across restore.
  - Decide whether replay is pure event replay, snapshot restore, or a new
    forked session; do not blur those contracts.
  - Require native/Wasm determinism and provider-effect tests before exposing
    controls in Play.

## Payload size and exposure

- [ ] **[P2][L][Engine work: Yes] Measure and, only if justified, add a compact
      inspection encoding.**
  - **Owner:** `inspect.rs`, protocol types, Wasm ABI, and the browser store.
  - Start with representative size and update-frequency measurements; the web
    tracker can impose a byte budget without changing this protocol.
  - If full snapshots are too expensive, design versioned deltas with an
    explicit base revision and recovery path. Never make a dropped delta render
    as a coherent snapshot.
  - Preserve deterministic serialization and native/Wasm parity.

- [ ] **[P2][L][Engine work: Yes] Define the inspection exposure and redaction
      boundary before Play is shareable outside trusted development.**
  - **Owner:** inspection protocol design with CLI-host policy enforcement.
  - Decide which state, provider payloads, command arguments, and projection
    values may cross the native/Wasm boundary.
  - Redaction that protects untrusted clients must happen before sensitive
    values reach JavaScript; CSS or DOM omission is not a security boundary.
  - Keep trusted local development as the explicit default until this contract
    exists.

## Non-goals for core inspection

- Graph layout, search, zoom, filtering, or visualization styling.
- Browser timeline state and selection behavior.
- Source-file serving or opening an Editor location.
- Nondeterministic performance timing inside the deterministic core.
